//! Just enough SQLite to read and write the database inside a Clip Studio
//! `.clip` file, in plain Rust (no C library, so it builds for every target
//! Sue ships, the browser included).
//!
//! [`read`] walks every table's b-tree (interior and leaf pages, overflow
//! chains, UTF-8 and UTF-16 text) and answers each table with its column
//! names and rows. [`write`] lays tables out as fresh b-trees in a new
//! database: rowid tables only, no indexes, nothing free, which is what a
//! `.clip` needs.
//!
//! [`Database`] opens the same bytes without reading any table: it lists
//! the schema, counts rows, reads one table by name, and with
//! [`Database::lazy_table`] answers rows whose blobs are only places in
//! the file ([`BlobRef`]) until [`Database::blob`] is asked for one, so a
//! large database never needs every blob in memory at once.

use std::collections::HashSet;

/// One value in a row.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// SQL NULL.
    Null,
    /// A whole number.
    Integer(i64),
    /// A floating point number.
    Real(f64),
    /// Text.
    Text(String),
    /// Bytes.
    Blob(Vec<u8>),
}

impl Value {
    /// The number, if this is one (reals are cut to whole numbers).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn int(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Real(value) => Some(*value as i64),
            _ => None,
        }
    }

    /// The number as a float, if this is one.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn real(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Real(value) => Some(*value),
            _ => None,
        }
    }

    /// The text, if this is text.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The bytes of a blob, or of text.
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(bytes) => Some(bytes),
            Self::Text(text) => Some(text.as_bytes()),
            _ => None,
        }
    }
}

/// A table: its name, its columns and its rows (rowid, then one value per
/// column).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    /// Its name.
    pub name: String,
    /// Its columns, in order.
    pub columns: Vec<String>,
    /// Its rows, by rowid.
    pub rows: Vec<(i64, Vec<Value>)>,
}

impl Table {
    /// Where `column` is (any case).
    #[must_use]
    pub fn column(&self, column: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|name| name.eq_ignore_ascii_case(column))
    }
}

/// A table to write: its `CREATE TABLE` statement, its rows (rowid, then a
/// value per column; the `INTEGER PRIMARY KEY` column's value is the
/// rowid, so it is written as NULL), and whether it is `AUTOINCREMENT`
/// (then `sqlite_sequence` remembers its largest rowid).
#[derive(Clone, Debug, PartialEq)]
pub struct NewTable {
    /// Its name, as in `sql`.
    pub name: String,
    /// `CREATE TABLE name(...)`.
    pub sql: String,
    /// Its rows.
    pub rows: Vec<(i64, Vec<Value>)>,
}

// ----- Reading ---------------------------------------------------------------

struct Reader<'a> {
    bytes: &'a [u8],
    page_size: usize,
    usable: usize,
    utf16: Option<bool>,
}

fn be16(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_be_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A varint at `at`: its value and length.
fn varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for i in 0..9 {
        let byte = *bytes.get(at + i)?;
        if i == 8 {
            return Some(((value << 8) | u64::from(byte), 9));
        }
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

impl Reader<'_> {
    fn page(&self, number: u32) -> Result<&[u8], String> {
        let start = (number as usize)
            .checked_sub(1)
            .ok_or("page 0 does not exist")?
            * self.page_size;
        self.bytes
            .get(start..start + self.page_size)
            .ok_or_else(|| format!("page {number} is past the end of the database"))
    }

    /// The payload of a cell whose payload is `size` bytes starting at
    /// `at` in `page`, following its overflow pages. `max_local` and
    /// `min_local` are the page kind's limits.
    fn payload(&self, page: &[u8], at: usize, size: usize, index: bool) -> Result<Vec<u8>, String> {
        let usable = self.usable;
        let max_local = if index {
            (usable - 12) * 64 / 255 - 23
        } else {
            usable - 35
        };
        if size <= max_local {
            return page
                .get(at..at + size)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "a cell runs past its page".to_owned());
        }
        let min_local = (usable - 12) * 32 / 255 - 23;
        let k = min_local + (size - min_local) % (usable - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = page
            .get(at..at + local)
            .ok_or("a cell runs past its page")?
            .to_vec();
        let mut next = be32(page, at + local).ok_or("a cell runs past its page")?;
        let mut seen = HashSet::new();
        while out.len() < size {
            if next == 0 || !seen.insert(next) {
                return Err("an overflow chain is broken".to_owned());
            }
            let overflow = self.page(next)?;
            next = be32(overflow, 0).ok_or("an overflow page is short")?;
            let take = (size - out.len()).min(usable - 4);
            out.extend_from_slice(
                overflow
                    .get(4..4 + take)
                    .ok_or("an overflow page is short")?,
            );
        }
        Ok(out)
    }

    /// How many bytes of a table cell's `size` byte payload sit in the
    /// leaf page itself; the rest is in its overflow pages.
    fn local(&self, size: usize) -> usize {
        let usable = self.usable;
        let max_local = usable - 35;
        if size <= max_local {
            return size;
        }
        let min_local = (usable - 12) * 32 / 255 - 23;
        let k = min_local + (size - min_local) % (usable - 4);
        if k <= max_local { k } else { min_local }
    }

    /// `len` bytes from byte `start` of a table cell's payload, copying
    /// nothing else: overflow pages before `start` are only stepped over.
    fn range(&self, cell: Cell, start: usize, len: usize) -> Result<Vec<u8>, String> {
        let end = start
            .checked_add(len)
            .filter(|end| *end <= cell.size)
            .ok_or("a value runs past its record")?;
        let page = self.page(cell.page)?;
        let local = self.local(cell.size);
        let mut out = Vec::with_capacity(len);
        if start < local {
            let from = cell.at + start;
            let to = cell.at + end.min(local);
            out.extend_from_slice(page.get(from..to).ok_or("a cell runs past its page")?);
        }
        if end <= local {
            return Ok(out);
        }
        let step = self.usable - 4;
        let mut next = be32(page, cell.at + local).ok_or("a cell runs past its page")?;
        let mut at = local;
        let mut seen = HashSet::new();
        while at < end {
            if next == 0 || !seen.insert(next) {
                return Err("an overflow chain is broken".to_owned());
            }
            let overflow = self.page(next)?;
            next = be32(overflow, 0).ok_or("an overflow page is short")?;
            let from = start.max(at);
            let to = end.min(at + step);
            if from < to {
                out.extend_from_slice(
                    overflow
                        .get(4 + from - at..4 + to - at)
                        .ok_or("an overflow page is short")?,
                );
            }
            at += step;
        }
        Ok(out)
    }

    /// Every (rowid, record) of the table b-tree at `root`.
    fn table_rows(&self, root: u32) -> Result<Vec<(i64, Vec<u8>)>, String> {
        let mut rows = Vec::new();
        self.table_cells(root, &mut |rowid, cell, page| {
            rows.push((rowid, self.payload(page, cell.at, cell.size, false)?));
            Ok(())
        })?;
        Ok(rows)
    }

    /// Calls `visit` with every cell of the table b-tree at `root`, in
    /// rowid order: its rowid, where its payload is, and its leaf page.
    fn table_cells(&self, root: u32, visit: &mut Visit<'_>) -> Result<(), String> {
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        while let Some(number) = pending.pop() {
            if !seen.insert(number) {
                return Err("a b-tree page is used twice".to_owned());
            }
            let page = self.page(number)?;
            let header = if number == 1 { 100 } else { 0 };
            let kind = *page.get(header).ok_or("a page is empty")?;
            let cells = be16(page, header + 3).ok_or("a page header is short")?;
            match kind {
                0x0d => {
                    for i in 0..cells {
                        let at = be16(page, header + 8 + 2 * i).ok_or("a cell pointer is short")?;
                        let (size, a) = varint(page, at).ok_or("a cell is short")?;
                        let (rowid, b) = varint(page, at + a).ok_or("a cell is short")?;
                        let size = usize::try_from(size).map_err(|_| "a cell is too big")?;
                        let cell = Cell {
                            page: number,
                            at: at + a + b,
                            size,
                        };
                        #[allow(clippy::cast_possible_wrap)]
                        visit(rowid as i64, cell, page)?;
                    }
                }
                0x05 => {
                    // Children are pushed right to left so they pop in order.
                    let right = be32(page, header + 8).ok_or("a page header is short")?;
                    pending.push(right);
                    for i in (0..cells).rev() {
                        let at =
                            be16(page, header + 12 + 2 * i).ok_or("a cell pointer is short")?;
                        pending.push(be32(page, at).ok_or("a cell is short")?);
                    }
                }
                other => return Err(format!("page {number} is not a table page ({other:#x})")),
            }
        }
        Ok(())
    }

    /// The values of the record in `cell`, its blobs left where they are.
    fn lazy_record(&self, cell: Cell) -> Result<Vec<LazyValue>, String> {
        let start = self.range(cell, 0, cell.size.min(9))?;
        let (header_size, first) = varint(&start, 0).ok_or("a record is short")?;
        let header_size = usize::try_from(header_size).map_err(|_| "a record is broken")?;
        let header = self.range(cell, 0, header_size)?;
        let mut at = first;
        let mut body = header_size;
        let mut values = Vec::new();
        while at < header_size {
            let (kind, used) = varint(&header, at).ok_or("a record header is short")?;
            at += used;
            let len = serial_len(kind)?;
            if kind >= 12 && kind % 2 == 0 {
                body.checked_add(len)
                    .filter(|end| *end <= cell.size)
                    .ok_or("a record is short")?;
                values.push(LazyValue::Blob(BlobRef {
                    cell,
                    start: body,
                    len,
                }));
            } else {
                let raw = self
                    .range(cell, body, len)
                    .map_err(|_| "a record is short")?;
                values.push(match self.value(kind, &raw) {
                    Value::Null => LazyValue::Null,
                    Value::Integer(value) => LazyValue::Integer(value),
                    Value::Real(value) => LazyValue::Real(value),
                    Value::Text(text) => LazyValue::Text(text),
                    Value::Blob(_) => return Err("a record is broken".to_owned()),
                });
            }
            body = body.saturating_add(len);
        }
        Ok(values)
    }

    /// The value of serial type `kind` whose bytes are `raw` (as long as
    /// [`serial_len`] says).
    fn value(&self, kind: u64, raw: &[u8]) -> Value {
        match kind {
            0 => Value::Null,
            1..=6 => {
                let mut value = i64::from(raw.first().copied().unwrap_or(0) as i8);
                for byte in raw.iter().skip(1) {
                    value = (value << 8) | i64::from(*byte);
                }
                Value::Integer(value)
            }
            7 => {
                let mut bits = [0_u8; 8];
                bits.copy_from_slice(&raw[..8]);
                Value::Real(f64::from_bits(u64::from_be_bytes(bits)))
            }
            8 => Value::Integer(0),
            9 => Value::Integer(1),
            n if n % 2 == 0 => Value::Blob(raw.to_vec()),
            _ => Value::Text(self.text(raw)),
        }
    }

    fn text(&self, bytes: &[u8]) -> String {
        match self.utf16 {
            None => String::from_utf8_lossy(bytes).into_owned(),
            Some(big) => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| {
                        if big {
                            u16::from_be_bytes([pair[0], pair[1]])
                        } else {
                            u16::from_le_bytes([pair[0], pair[1]])
                        }
                    })
                    .collect();
                String::from_utf16_lossy(&units)
            }
        }
    }

    /// The values of a record.
    fn record(&self, bytes: &[u8]) -> Result<Vec<Value>, String> {
        let (header_size, mut at) = varint(bytes, 0).ok_or("a record is short")?;
        let header_size = usize::try_from(header_size).map_err(|_| "a record is broken")?;
        let mut types = Vec::new();
        while at < header_size {
            let (kind, len) = varint(bytes, at).ok_or("a record header is short")?;
            types.push(kind);
            at += len;
        }
        let mut body = header_size;
        let mut values = Vec::with_capacity(types.len());
        for kind in types {
            let len = serial_len(kind)?;
            let raw = body
                .checked_add(len)
                .and_then(|end| bytes.get(body..end))
                .ok_or("a record is short")?;
            values.push(self.value(kind, raw));
            body += len;
        }
        Ok(values)
    }
}

/// Where a table cell's payload is: its leaf page, the payload's first
/// byte in that page, and the payload's whole size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    page: u32,
    at: usize,
    size: usize,
}

/// What [`Reader::table_cells`] calls with each cell: its rowid, where its
/// payload is, and its leaf page.
type Visit<'v> = dyn FnMut(i64, Cell, &[u8]) -> Result<(), String> + 'v;

/// How many bytes a value of serial type `kind` takes in a record.
fn serial_len(kind: u64) -> Result<usize, String> {
    Ok(match kind {
        0 | 8 | 9 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        6 | 7 => 8,
        10 | 11 => return Err("a record uses a reserved type".to_owned()),
        n => usize::try_from((n - 12) / 2).map_err(|_| "a record is broken")?,
    })
}

/// One row of the database's schema table: a table, an index, a view or
/// a trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaEntry {
    /// `table`, `index`, `view` or `trigger`.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// The table it belongs to (its own name for a table).
    pub table: String,
    /// The page its b-tree starts on (0 when it has none).
    pub root: i64,
    /// The statement that made it (empty for an automatic index).
    pub sql: String,
}

/// A blob still in the database: where it is and how long. Read it with
/// [`Database::blob`] or a part of it with [`Database::blob_range`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobRef {
    cell: Cell,
    start: usize,
    len: usize,
}

impl BlobRef {
    /// Its length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` when it holds no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// One value of a row read lazily: as [`Value`], but a blob is only a
/// [`BlobRef`].
#[derive(Clone, Debug, PartialEq)]
pub enum LazyValue {
    /// SQL NULL.
    Null,
    /// A whole number.
    Integer(i64),
    /// A floating point number.
    Real(f64),
    /// Text.
    Text(String),
    /// Bytes, not read yet.
    Blob(BlobRef),
}

impl LazyValue {
    /// The number, if this is one (reals are cut to whole numbers).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn int(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Real(value) => Some(*value as i64),
            _ => None,
        }
    }

    /// The text, if this is text.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The blob's place, if this is a blob.
    #[must_use]
    pub fn blob(&self) -> Option<BlobRef> {
        match self {
            Self::Blob(blob) => Some(*blob),
            _ => None,
        }
    }
}

/// A table read lazily: as [`Table`], with its blobs left in the database.
#[derive(Clone, Debug, PartialEq)]
pub struct LazyTable {
    /// Its name.
    pub name: String,
    /// Its columns, in order.
    pub columns: Vec<String>,
    /// Its rows, by rowid.
    pub rows: Vec<(i64, Vec<LazyValue>)>,
}

impl LazyTable {
    /// Where `column` is (any case).
    #[must_use]
    pub fn column(&self, column: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|name| name.eq_ignore_ascii_case(column))
    }
}

/// An SQLite database opened but not read: its schema is known, and each
/// table is read only when asked for.
pub struct Database<'a> {
    reader: Reader<'a>,
    schema: Vec<SchemaEntry>,
}

impl<'a> Database<'a> {
    /// Opens the database in `bytes` and reads its schema.
    ///
    /// # Errors
    /// When `bytes` is not an SQLite database or its schema is broken.
    pub fn open(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 100 || &bytes[..16] != b"SQLite format 3\0" {
            return Err("not an SQLite database".to_owned());
        }
        let page_size = match be16(bytes, 16).unwrap_or(0) {
            1 => 65_536,
            size if size >= 512 && size.is_power_of_two() => size,
            size => return Err(format!("an SQLite page size of {size} is not valid")),
        };
        let reserved = usize::from(bytes[20]);
        let usable = page_size
            .checked_sub(reserved)
            .filter(|usable| *usable >= 480)
            .ok_or("the SQLite header is broken")?;
        let utf16 = match be32(bytes, 56).unwrap_or(1) {
            2 => Some(false),
            3 => Some(true),
            _ => None,
        };
        let reader = Reader {
            bytes,
            page_size,
            usable,
            utf16,
        };
        let mut schema = Vec::new();
        for (_, record) in reader.table_rows(1)? {
            let row = reader.record(&record)?;
            let text = |index: usize| {
                row.get(index)
                    .and_then(Value::text)
                    .unwrap_or_default()
                    .to_owned()
            };
            schema.push(SchemaEntry {
                kind: text(0),
                name: text(1),
                table: text(2),
                root: row.get(3).and_then(Value::int).unwrap_or(0),
                sql: text(4),
            });
        }
        Ok(Self { reader, schema })
    }

    /// The size of a page in bytes.
    #[must_use]
    pub fn page_size(&self) -> usize {
        self.reader.page_size
    }

    /// How many whole pages the bytes hold.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.reader.bytes.len() / self.reader.page_size
    }

    /// How text is stored: `UTF-8`, `UTF-16LE` or `UTF-16BE`.
    #[must_use]
    pub fn text_encoding(&self) -> &'static str {
        match self.reader.utf16 {
            None => "UTF-8",
            Some(false) => "UTF-16LE",
            Some(true) => "UTF-16BE",
        }
    }

    /// Every row of the schema, in the order the database lists them.
    #[must_use]
    pub fn schema(&self) -> &[SchemaEntry] {
        &self.schema
    }

    /// The root page of the table `name`, when the schema has it.
    fn root(&self, name: &str) -> Option<(&SchemaEntry, u32)> {
        self.schema
            .iter()
            .filter(|entry| entry.kind == "table" && entry.name == name)
            .find_map(|entry| {
                let root = u32::try_from(entry.root).ok().filter(|root| *root != 0)?;
                Some((entry, root))
            })
    }

    /// The table `name` with all its values, or `None` when there is none.
    ///
    /// # Errors
    /// When a page of the table is broken.
    pub fn table(&self, name: &str) -> Result<Option<Table>, String> {
        let Some((entry, root)) = self.root(name) else {
            return Ok(None);
        };
        self.table_at(entry, root).map(Some)
    }

    fn table_at(&self, entry: &SchemaEntry, root: u32) -> Result<Table, String> {
        let (columns, key) = columns_of(&entry.sql);
        let mut rows = Vec::new();
        for (rowid, record) in self.reader.table_rows(root)? {
            let mut values = self.reader.record(&record)?;
            values.resize(columns.len().max(values.len()), Value::Null);
            if let Some(key) = key
                && values.get(key) == Some(&Value::Null)
            {
                values[key] = Value::Integer(rowid);
            }
            rows.push((rowid, values));
        }
        Ok(Table {
            name: entry.name.clone(),
            columns,
            rows,
        })
    }

    /// How many rows the table `name` has, counted without reading one,
    /// or `None` when there is no such table.
    ///
    /// # Errors
    /// When a page of the table is broken.
    pub fn row_count(&self, name: &str) -> Result<Option<usize>, String> {
        let Some((_, root)) = self.root(name) else {
            return Ok(None);
        };
        let mut count = 0;
        self.reader.table_cells(root, &mut |_, _, _| {
            count += 1;
            Ok(())
        })?;
        Ok(Some(count))
    }

    /// The table `name` with its blobs left in the database, or `None`
    /// when there is none. Numbers and text are read; of a blob only the
    /// place and the length.
    ///
    /// # Errors
    /// When a page of the table is broken.
    pub fn lazy_table(&self, name: &str) -> Result<Option<LazyTable>, String> {
        let Some((entry, root)) = self.root(name) else {
            return Ok(None);
        };
        let (columns, key) = columns_of(&entry.sql);
        let mut rows = Vec::new();
        self.reader.table_cells(root, &mut |rowid, cell, _| {
            let mut values = self.reader.lazy_record(cell)?;
            values.resize(columns.len().max(values.len()), LazyValue::Null);
            if let Some(key) = key
                && values.get(key) == Some(&LazyValue::Null)
            {
                values[key] = LazyValue::Integer(rowid);
            }
            rows.push((rowid, values));
            Ok(())
        })?;
        Ok(Some(LazyTable {
            name: entry.name.clone(),
            columns,
            rows,
        }))
    }

    /// The bytes of `blob`.
    ///
    /// # Errors
    /// When the pages the blob lies on are broken.
    pub fn blob(&self, blob: &BlobRef) -> Result<Vec<u8>, String> {
        self.reader.range(blob.cell, blob.start, blob.len)
    }

    /// `len` bytes of `blob` from its byte `start` (fewer when the blob
    /// ends first), reading only the pages they lie on and stepping over
    /// the ones before.
    ///
    /// # Errors
    /// When the pages the blob lies on are broken.
    pub fn blob_range(&self, blob: &BlobRef, start: usize, len: usize) -> Result<Vec<u8>, String> {
        let start = start.min(blob.len);
        let len = len.min(blob.len - start);
        self.reader.range(blob.cell, blob.start + start, len)
    }
}

/// The column names of a `CREATE TABLE` statement, and which one (if any)
/// is the `INTEGER PRIMARY KEY` that stands for the rowid.
fn columns_of(sql: &str) -> (Vec<String>, Option<usize>) {
    let Some(open) = sql.find('(') else {
        return (Vec::new(), None);
    };
    let close = sql.rfind(')').unwrap_or(sql.len());
    let inner = &sql[open + 1..close.max(open + 1)];
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    let mut columns = Vec::new();
    let mut key = None;
    for part in parts {
        let part = part.trim();
        let upper = part.to_ascii_uppercase();
        if ["PRIMARY ", "UNIQUE", "CHECK", "FOREIGN ", "CONSTRAINT "]
            .iter()
            .any(|word| upper.starts_with(word))
        {
            continue;
        }
        let name: String = if let Some(quote @ ('"' | '`' | '[' | '\'')) = part.chars().next() {
            let end = if quote == '[' { ']' } else { quote };
            part[1..].split(end).next().unwrap_or_default().to_owned()
        } else {
            part.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        if name.is_empty() {
            continue;
        }
        let words: Vec<&str> = upper.split_whitespace().collect();
        if words.get(1) == Some(&"INTEGER") && upper.contains("PRIMARY KEY") {
            key = Some(columns.len());
        }
        columns.push(name);
    }
    (columns, key)
}

/// Every table of the SQLite database in `bytes`, `sqlite_sequence`
/// included, in the order the schema lists them.
///
/// # Errors
/// When `bytes` is not an SQLite database or a page of it is broken.
pub fn read(bytes: &[u8]) -> Result<Vec<Table>, String> {
    let database = Database::open(bytes)?;
    let mut tables = Vec::new();
    for entry in database.schema() {
        if entry.kind != "table" {
            continue;
        }
        let Ok(root) = u32::try_from(entry.root) else {
            continue;
        };
        if root == 0 {
            continue;
        }
        tables.push(database.table_at(entry, root)?);
    }
    Ok(tables)
}

// ----- Writing ---------------------------------------------------------------

/// The page size the writer uses.
const PAGE: usize = 4096;

fn put_varint(out: &mut Vec<u8>, value: u64) {
    if value > 0x00ff_ffff_ffff_ffff {
        let mut bytes = [0_u8; 9];
        bytes[8] = (value & 0xff) as u8;
        let mut rest = value >> 8;
        for i in (0..8).rev() {
            bytes[i] = (rest & 0x7f) as u8 | 0x80;
            rest >>= 7;
        }
        out.extend_from_slice(&bytes);
        return;
    }
    let mut groups = Vec::new();
    let mut rest = value;
    loop {
        groups.push((rest & 0x7f) as u8);
        rest >>= 7;
        if rest == 0 {
            break;
        }
    }
    for (i, group) in groups.iter().enumerate().rev() {
        out.push(if i == 0 { *group } else { group | 0x80 });
    }
}

fn varint_len(value: u64) -> usize {
    let mut out = Vec::new();
    put_varint(&mut out, value);
    out.len()
}

/// A record of `values`.
fn encode_record(values: &[Value]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut body = Vec::new();
    for value in values {
        let kind: u64 = match value {
            Value::Null => 0,
            Value::Integer(0) => 8,
            Value::Integer(1) => 9,
            Value::Integer(n) => {
                let n = *n;
                let (kind, len) = if (-128..128).contains(&n) {
                    (1, 1)
                } else if (-32_768..32_768).contains(&n) {
                    (2, 2)
                } else if (-8_388_608..8_388_608).contains(&n) {
                    (3, 3)
                } else if (-2_147_483_648..2_147_483_648).contains(&n) {
                    (4, 4)
                } else if (-140_737_488_355_328..140_737_488_355_328).contains(&n) {
                    (5, 6)
                } else {
                    (6, 8)
                };
                body.extend_from_slice(&n.to_be_bytes()[8 - len..]);
                kind
            }
            Value::Real(x) => {
                body.extend_from_slice(&x.to_bits().to_be_bytes());
                7
            }
            Value::Text(text) => {
                body.extend_from_slice(text.as_bytes());
                text.len() as u64 * 2 + 13
            }
            Value::Blob(bytes) => {
                body.extend_from_slice(bytes);
                bytes.len() as u64 * 2 + 12
            }
        };
        types.push(kind);
    }
    let types_len: usize = types.iter().map(|kind| varint_len(*kind)).sum();
    let mut header_len = types_len + 1;
    while varint_len(header_len as u64) + types_len != header_len {
        header_len = varint_len(header_len as u64) + types_len;
    }
    let mut out = Vec::with_capacity(header_len + body.len());
    put_varint(&mut out, header_len as u64);
    for kind in types {
        put_varint(&mut out, kind);
    }
    out.extend_from_slice(&body);
    out
}

/// The database being laid out: its pages, page 1 first.
struct Pages {
    pages: Vec<Vec<u8>>,
}

impl Pages {
    fn alloc(&mut self) -> u32 {
        self.pages.push(vec![0; PAGE]);
        u32::try_from(self.pages.len()).unwrap_or(u32::MAX)
    }

    /// A table leaf cell for `rowid` and `payload`, its overflow written to
    /// new pages.
    fn leaf_cell(&mut self, rowid: i64, payload: &[u8]) -> Vec<u8> {
        let usable = PAGE;
        let size = payload.len();
        let mut cell = Vec::new();
        put_varint(&mut cell, size as u64);
        #[allow(clippy::cast_sign_loss)]
        put_varint(&mut cell, rowid as u64);
        let max_local = usable - 35;
        if size <= max_local {
            cell.extend_from_slice(payload);
            return cell;
        }
        let min_local = (usable - 12) * 32 / 255 - 23;
        let k = min_local + (size - min_local) % (usable - 4);
        let local = if k <= max_local { k } else { min_local };
        cell.extend_from_slice(&payload[..local]);
        let chunks: Vec<&[u8]> = payload[local..].chunks(usable - 4).collect();
        let numbers: Vec<u32> = chunks.iter().map(|_| self.alloc()).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let next = numbers.get(i + 1).copied().unwrap_or(0);
            let page = &mut self.pages[numbers[i] as usize - 1];
            page[..4].copy_from_slice(&next.to_be_bytes());
            page[4..4 + chunk.len()].copy_from_slice(chunk);
        }
        cell.extend_from_slice(&numbers[0].to_be_bytes());
        cell
    }

    /// Writes a b-tree page: `kind` 0x0d (leaf) or 0x05 (interior, with
    /// `right`), its `cells` in order.
    fn write_page(&mut self, number: u32, kind: u8, cells: &[Vec<u8>], right: Option<u32>) {
        let header = if number == 1 { 100 } else { 0 };
        let page = &mut self.pages[number as usize - 1];
        let mut end = PAGE;
        let pointers = header + if right.is_some() { 12 } else { 8 };
        for (i, cell) in cells.iter().enumerate() {
            end -= cell.len();
            page[end..end + cell.len()].copy_from_slice(cell);
            let at = pointers + 2 * i;
            page[at..at + 2].copy_from_slice(&u16::try_from(end).unwrap_or(0).to_be_bytes());
        }
        page[header] = kind;
        page[header + 1..header + 3].copy_from_slice(&[0, 0]);
        let count = u16::try_from(cells.len()).unwrap_or(u16::MAX);
        page[header + 3..header + 5].copy_from_slice(&count.to_be_bytes());
        let start = if end == 65_536 { 0 } else { end };
        page[header + 5..header + 7]
            .copy_from_slice(&u16::try_from(start).unwrap_or(0).to_be_bytes());
        page[header + 7] = 0;
        if let Some(right) = right {
            page[header + 8..header + 12].copy_from_slice(&right.to_be_bytes());
        }
    }

    /// Lays out a table b-tree of `cells` (each with its rowid, in rowid
    /// order). `root` is the page the root must be (1 for the schema).
    /// Answers the root page.
    fn table_tree(&mut self, cells: Vec<(i64, Vec<u8>)>, root: Option<u32>) -> u32 {
        // Leaves: as many cells as fit in each.
        let mut level: Vec<(i64, u32)> = Vec::new();
        let groups = group(&cells, 8, root == Some(1));
        if groups.len() == 1 {
            let number = root.unwrap_or_else(|| self.alloc());
            let cells: Vec<Vec<u8>> = groups[0].iter().map(|i| cells[*i].1.clone()).collect();
            self.write_page(number, 0x0d, &cells, None);
            return number;
        }
        for indices in &groups {
            let number = self.alloc();
            let page_cells: Vec<Vec<u8>> = indices.iter().map(|i| cells[*i].1.clone()).collect();
            self.write_page(number, 0x0d, &page_cells, None);
            let last = *indices.last().unwrap_or(&0);
            level.push((cells[last].0, number));
        }
        // Interior levels until one page holds them all.
        loop {
            let entries: Vec<(i64, Vec<u8>)> = level
                .iter()
                .map(|(key, child)| {
                    let mut cell = child.to_be_bytes().to_vec();
                    #[allow(clippy::cast_sign_loss)]
                    put_varint(&mut cell, *key as u64);
                    (*key, cell)
                })
                .collect();
            // Each page's last entry is its right pointer, not a cell.
            let groups = group_interior(&entries, root == Some(1));
            let mut next = Vec::new();
            let single = groups.len() == 1;
            for indices in &groups {
                let number = if single {
                    root.unwrap_or_else(|| self.alloc())
                } else {
                    self.alloc()
                };
                let (last, rest) = indices.split_last().unwrap_or((&0, &[]));
                let page_cells: Vec<Vec<u8>> = rest.iter().map(|i| entries[*i].1.clone()).collect();
                self.write_page(number, 0x05, &page_cells, Some(level[*last].1));
                next.push((level[*last].0, number));
            }
            if single {
                return next[0].1;
            }
            level = next;
        }
    }
}

/// Splits `cells` into pages of leaf cells, each holding as many as fit.
/// When `first` (the root is page 1) and they all fit page 1, one group.
fn group(cells: &[(i64, Vec<u8>)], header: usize, first: bool) -> Vec<Vec<usize>> {
    let total: usize = cells.iter().map(|(_, cell)| cell.len() + 2).sum();
    let room_first = PAGE - 100 - header;
    if total <= if first { room_first } else { PAGE - header } {
        return vec![(0..cells.len()).collect()];
    }
    let mut groups = vec![Vec::new()];
    let mut used = 0;
    for (i, (_, cell)) in cells.iter().enumerate() {
        let need = cell.len() + 2;
        if used + need > PAGE - header && !groups.last().is_some_and(Vec::is_empty) {
            groups.push(Vec::new());
            used = 0;
        }
        if let Some(group) = groups.last_mut() {
            group.push(i);
        }
        used += need;
    }
    groups
}

/// Splits interior entries into pages; each page's last entry becomes its
/// right pointer, so it takes no room.
fn group_interior(entries: &[(i64, Vec<u8>)], first: bool) -> Vec<Vec<usize>> {
    let total: usize = entries.iter().map(|(_, cell)| cell.len() + 2).sum();
    let room = if first { PAGE - 100 - 12 } else { PAGE - 12 };
    if total <= room {
        return vec![(0..entries.len()).collect()];
    }
    let mut groups = vec![Vec::new()];
    let mut used = 0;
    for (i, (_, cell)) in entries.iter().enumerate() {
        let need = cell.len() + 2;
        if used + need > PAGE - 12 && groups.last().is_some_and(|group| group.len() >= 2) {
            groups.push(Vec::new());
            used = 0;
        }
        if let Some(group) = groups.last_mut() {
            group.push(i);
        }
        used += need;
    }
    // A last page with one entry has no cells; give it one from before.
    if groups.len() > 1 && groups.last().is_some_and(|group| group.len() < 2) {
        let before = groups.len() - 2;
        let moved = groups[before].pop();
        if let Some(moved) = moved
            && let Some(group) = groups.last_mut()
        {
            group.insert(0, moved);
        }
    }
    groups
}

/// A new SQLite database holding `tables` (and `sqlite_sequence` for those
/// whose SQL says `AUTOINCREMENT`), as SQLite 3 writes it: 4096 byte pages,
/// UTF-8, schema format 4.
#[must_use]
pub fn write(tables: &[NewTable]) -> Vec<u8> {
    let mut pages = Pages { pages: Vec::new() };
    pages.alloc();
    let mut schema: Vec<Vec<Value>> = Vec::new();
    let mut sequence = Vec::new();
    for table in tables {
        let mut rows: Vec<&(i64, Vec<Value>)> = table.rows.iter().collect();
        rows.sort_by_key(|(rowid, _)| *rowid);
        let cells: Vec<(i64, Vec<u8>)> = rows
            .iter()
            .map(|(rowid, values)| (*rowid, pages.leaf_cell(*rowid, &encode_record(values))))
            .collect();
        let root = pages.table_tree(cells, None);
        schema.push(vec![
            Value::Text("table".to_owned()),
            Value::Text(table.name.clone()),
            Value::Text(table.name.clone()),
            Value::Integer(i64::from(root)),
            Value::Text(table.sql.clone()),
        ]);
        if table.sql.to_ascii_uppercase().contains("AUTOINCREMENT") {
            let max = rows.last().map_or(0, |(rowid, _)| *rowid);
            sequence.push(vec![Value::Text(table.name.clone()), Value::Integer(max)]);
        }
    }
    if !sequence.is_empty() {
        let cells: Vec<(i64, Vec<u8>)> = sequence
            .iter()
            .zip(1_i64..)
            .map(|(values, rowid)| (rowid, pages.leaf_cell(rowid, &encode_record(values))))
            .collect();
        let root = pages.table_tree(cells, None);
        schema.push(vec![
            Value::Text("table".to_owned()),
            Value::Text("sqlite_sequence".to_owned()),
            Value::Text("sqlite_sequence".to_owned()),
            Value::Integer(i64::from(root)),
            Value::Text("CREATE TABLE sqlite_sequence(name,seq)".to_owned()),
        ]);
    }
    let cells: Vec<(i64, Vec<u8>)> = schema
        .iter()
        .zip(1_i64..)
        .map(|(values, rowid)| (rowid, pages.leaf_cell(rowid, &encode_record(values))))
        .collect();
    pages.table_tree(cells, Some(1));
    let count = u32::try_from(pages.pages.len()).unwrap_or(u32::MAX);
    let header = &mut pages.pages[0];
    header[..16].copy_from_slice(b"SQLite format 3\0");
    header[16..18].copy_from_slice(&u16::try_from(PAGE).unwrap_or(0).to_be_bytes());
    header[18] = 1;
    header[19] = 1;
    header[20] = 0;
    header[21] = 64;
    header[22] = 32;
    header[23] = 32;
    header[24..28].copy_from_slice(&1_u32.to_be_bytes());
    header[28..32].copy_from_slice(&count.to_be_bytes());
    header[40..44].copy_from_slice(&1_u32.to_be_bytes());
    header[44..48].copy_from_slice(&4_u32.to_be_bytes());
    header[56..60].copy_from_slice(&1_u32.to_be_bytes());
    header[92..96].copy_from_slice(&1_u32.to_be_bytes());
    header[96..100].copy_from_slice(&3_046_000_u32.to_be_bytes());
    pages.pages.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str, rows: usize, blob: usize) -> NewTable {
        NewTable {
            name: name.to_owned(),
            sql: format!(
                "CREATE TABLE {name}(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, Name TEXT DEFAULT NULL, Data BLOB DEFAULT NULL, Scale REAL DEFAULT NULL)"
            ),
            rows: (1..=rows as i64)
                .map(|rowid| {
                    (
                        rowid,
                        vec![
                            Value::Null,
                            Value::Integer(rowid * 1_000_003 - 70_000),
                            Value::Text(format!("row {rowid} \u{65e5}\u{672c}")),
                            Value::Blob(
                                (0..blob).map(|i| (i * 7 + rowid as usize) as u8).collect(),
                            ),
                            Value::Real(rowid as f64 / 3.0),
                        ],
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn records_round_trip_every_kind_of_value() {
        let values = vec![
            Value::Null,
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(-1),
            Value::Integer(300),
            Value::Integer(-70_000),
            Value::Integer(1 << 40),
            Value::Integer(i64::MIN),
            Value::Integer(4_294_967_295),
            Value::Real(-2.5),
            Value::Text("Ebene \u{1f58c}".to_owned()),
            Value::Blob(vec![0, 255, 7]),
        ];
        let reader = Reader {
            bytes: &[],
            page_size: PAGE,
            usable: PAGE,
            utf16: None,
        };
        assert_eq!(reader.record(&encode_record(&values)).unwrap(), values);
        for value in [0_u64, 127, 128, 16_383, 16_384, 1 << 40, u64::MAX] {
            let mut out = Vec::new();
            put_varint(&mut out, value);
            assert_eq!(varint(&out, 0), Some((value, out.len())), "{value}");
        }
    }

    #[test]
    fn small_and_large_databases_read_back_as_written() {
        // Many rows (several leaves and interior levels), big blobs
        // (overflow chains), and a schema too long for page 1 alone.
        let mut tables = vec![
            table("Small", 3, 10),
            table("Big", 40, 20_000),
            table("Many", 5_000, 3),
        ];
        for i in 0..60 {
            tables.push(table(&format!("Extra{i}"), 1, 1));
        }
        tables.push(NewTable {
            name: "Plain".to_owned(),
            sql: "CREATE TABLE Plain(ExternalID BLOB, Offset INTEGER)".to_owned(),
            rows: vec![(
                1,
                vec![Value::Text("extrnlid0".to_owned()), Value::Integer(80)],
            )],
        });
        let bytes = write(&tables);
        assert_eq!(bytes.len() % PAGE, 0);
        let read = read(&bytes).unwrap();
        for table in &tables {
            let back = read.iter().find(|t| t.name == table.name).unwrap();
            assert_eq!(back.rows.len(), table.rows.len(), "{}", table.name);
            for ((rowid, values), (back_id, back_values)) in table.rows.iter().zip(&back.rows) {
                assert_eq!(rowid, back_id);
                // The rowid column reads back as the rowid.
                let mut expect = values.clone();
                if back.column("_PW_ID") == Some(0) {
                    expect[0] = Value::Integer(*rowid);
                }
                assert_eq!(&expect, back_values, "{} row {rowid}", table.name);
            }
        }
        let sequence = read.iter().find(|t| t.name == "sqlite_sequence").unwrap();
        assert!(sequence.rows.iter().any(|(_, row)| row
            == &vec![Value::Text("Many".to_owned()), Value::Integer(5_000)]));
        assert!(
            !sequence
                .rows
                .iter()
                .any(|(_, row)| row[0] == Value::Text("Plain".to_owned()))
        );
    }

    #[test]
    fn columns_come_from_the_create_statement() {
        let (columns, key) = columns_of(
            "CREATE TABLE \"Odd\"(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, [A b] TEXT, `c` NULL DEFAULT NULL, d, PRIMARY KEY(d))",
        );
        assert_eq!(columns, ["_PW_ID", "A b", "c", "d"]);
        assert_eq!(key, Some(0));
        assert!(read(b"not a database").is_err());
        let mut broken = write(&[table("T", 1, 1)]);
        broken[16] = 3;
        assert!(read(&broken).is_err());
    }

    /// The eager value a lazy one stands for.
    fn full(database: &Database<'_>, value: &LazyValue) -> Value {
        match value {
            LazyValue::Null => Value::Null,
            LazyValue::Integer(value) => Value::Integer(*value),
            LazyValue::Real(value) => Value::Real(*value),
            LazyValue::Text(text) => Value::Text(text.clone()),
            LazyValue::Blob(blob) => Value::Blob(database.blob(blob).unwrap()),
        }
    }

    #[test]
    fn a_lazy_table_holds_what_the_whole_read_holds() {
        let tables = vec![
            table("Small", 3, 10),
            table("Big", 40, 20_000),
            table("Many", 5_000, 3),
            table("None", 0, 0),
        ];
        let bytes = write(&tables);
        let whole = read(&bytes).unwrap();
        let database = Database::open(&bytes).unwrap();
        assert_eq!(database.page_size(), PAGE);
        assert_eq!(database.page_count() * PAGE, bytes.len());
        assert_eq!(database.text_encoding(), "UTF-8");
        let names: Vec<&str> = database
            .schema()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["Small", "Big", "Many", "None", "sqlite_sequence"]);
        for (entry, table) in database.schema().iter().zip(&tables) {
            assert_eq!((entry.kind.as_str(), &entry.sql), ("table", &table.sql));
            assert_eq!(entry.table, table.name);
        }
        for table in &whole {
            assert_eq!(
                database.row_count(&table.name).unwrap(),
                Some(table.rows.len()),
                "{}",
                table.name
            );
            assert_eq!(
                database.table(&table.name).unwrap().as_ref(),
                Some(table),
                "{}",
                table.name
            );
            let lazy = database.lazy_table(&table.name).unwrap().unwrap();
            assert_eq!(lazy.columns, table.columns);
            assert_eq!(lazy.column("mainid"), table.column("MainId"));
            assert_eq!(lazy.rows.len(), table.rows.len());
            for ((rowid, values), (lazy_id, lazy_values)) in table.rows.iter().zip(&lazy.rows) {
                assert_eq!(rowid, lazy_id);
                let read: Vec<Value> = lazy_values
                    .iter()
                    .map(|value| full(&database, value))
                    .collect();
                assert_eq!(&read, values, "{} row {rowid}", table.name);
            }
        }
        assert_eq!(database.table("Missing").unwrap(), None);
        assert_eq!(database.row_count("Missing").unwrap(), None);
        assert!(database.lazy_table("Missing").unwrap().is_none());
    }

    #[test]
    fn a_part_of_a_blob_is_read_without_the_rest() {
        let bytes = write(&[table("Big", 3, 50_000)]);
        let database = Database::open(&bytes).unwrap();
        let lazy = database.lazy_table("Big").unwrap().unwrap();
        let data = lazy.column("Data").unwrap();
        let blob = lazy.rows[1].1[data].blob().unwrap();
        assert_eq!((blob.len(), blob.is_empty()), (50_000, false));
        assert_eq!(lazy.rows[1].1[1].int(), Some(2 * 1_000_003 - 70_000));
        assert!(lazy.rows[1].1[2].text().unwrap().starts_with("row 2"));
        let whole = database.blob(&blob).unwrap();
        let expect: Vec<u8> = (0..50_000_usize).map(|i| (i * 7 + 2) as u8).collect();
        assert_eq!(whole, expect);
        // Parts inside the leaf page, across page edges, and at the end.
        for (start, len) in [
            (0, 16),
            (100, 5_000),
            (4_000, 200),
            (12_345, 20_000),
            (49_990, 10),
            (49_990, 500),
            (50_000, 4),
            (70_000, 4),
            (7, 0),
        ] {
            let from = start.min(expect.len());
            let to = (start + len).min(expect.len());
            assert_eq!(
                database.blob_range(&blob, start, len).unwrap(),
                &expect[from..to],
                "{len} bytes from {start}"
            );
        }
    }

    #[test]
    fn a_lazy_read_does_not_look_inside_a_blob() {
        let bytes = write(&[table("Big", 2, 30_000)]);
        let database = Database::open(&bytes).unwrap();
        let before = database.lazy_table("Big").unwrap().unwrap();
        // Scribble over the data of every overflow page (past its "next
        // page" number): the numbers and text around the blobs still read.
        let mut scribbled = bytes.clone();
        let pages = scribbled.len() / PAGE;
        let mut touched = 0;
        for page in 1..pages {
            let start = page * PAGE;
            // Leaf and interior pages start with 0x0d or 0x05.
            // The last page of a chain also holds the values after the blob.
            let last = scribbled[start..start + 4] == [0; 4];
            if last || matches!(scribbled[start], 0x0d | 0x05) {
                continue;
            }
            for byte in &mut scribbled[start + 4..start + PAGE] {
                *byte = 0xee;
            }
            touched += 1;
        }
        assert!(touched >= 12, "{touched} overflow pages");
        let database = Database::open(&scribbled).unwrap();
        let after = database.lazy_table("Big").unwrap().unwrap();
        let scale = after.column("Scale").unwrap();
        let data = after.column("Data").unwrap();
        for (old, new) in before.rows.iter().zip(&after.rows) {
            assert_eq!(old.1[scale], new.1[scale]);
            assert_eq!(new.1[data].blob().unwrap().len(), 30_000);
        }
        assert_eq!(after.rows[1].1[scale], LazyValue::Real(2.0 / 3.0));
    }

    #[test]
    fn a_broken_overflow_chain_is_an_error_not_a_panic() {
        let bytes = write(&[table("Big", 1, 30_000)]);
        let database = Database::open(&bytes).unwrap();
        let lazy = database.lazy_table("Big").unwrap().unwrap();
        let blob = lazy.rows[0].1[lazy.column("Data").unwrap()].blob().unwrap();
        // The same place in a database whose overflow pages end early, loop,
        // or are cut off.
        let overflow: Vec<usize> = (1..bytes.len() / PAGE)
            .filter(|page| !matches!(bytes[page * PAGE], 0x0d | 0x05))
            .collect();
        let third = overflow[2] * PAGE;
        let mut ended = bytes.clone();
        ended[third..third + 4].copy_from_slice(&0_u32.to_be_bytes());
        let mut looped = bytes.clone();
        let first = u32::try_from(overflow[0] + 1).unwrap();
        looped[third..third + 4].copy_from_slice(&first.to_be_bytes());
        let cut = bytes[..third + PAGE].to_vec();
        for broken in [&ended, &looped, &cut] {
            let database = Database::open(broken).unwrap();
            assert!(database.blob(&blob).is_err());
            assert!(database.blob_range(&blob, 29_000, 10).is_err());
            if broken.len() == bytes.len() {
                assert_eq!(database.blob_range(&blob, 0, 10).unwrap().len(), 10);
            }
            assert!(read(broken).is_err());
        }
        assert!(Database::open(b"not a database").is_err());
    }
}
