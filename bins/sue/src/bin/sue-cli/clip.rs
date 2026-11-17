//! `sue-cli clip dump` and `sue-cli clip diff`: the survey tool for Clip
//! Studio files.
//!
//! `dump` prints what a `.clip` is made of without interpreting it: the
//! header, every chunk, what each stored bitmap holds block by block, and
//! every table of the database with its `CREATE` text, its row count and
//! its rows, blobs decoded where their layout is known and shown as bytes
//! where it is not. `diff` prints what differs between two files, so two
//! files made the same way but for one feature show what that feature
//! writes. Neither changes a file.
//!
//! This is a tool for people working on Sue's converter. Its output is a
//! plain English listing of names and numbers from the file, like `lang
//! check`, and is not in the language database.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Write as _};
use std::process::ExitCode;

use sue_io::clip::{Layout, attribute_parts, layout, survey_bitmap};
use sue_io::sqlite::{BlobRef, Database, LazyTable, LazyValue};

/// How to use the `clip` subcommands.
pub const USAGE: &str = "\
Usage: sue-cli clip dump <file> [--bytes <n>] [--nulls] [--no-inflate]
       sue-cli clip diff <before> <after> [--bytes <n>] [--nulls] [--no-inflate]

  clip dump         Print what a Clip Studio file holds: its chunks, what
                    each stored bitmap is made of, and every table of its
                    database with its CREATE text, row count and rows
  clip diff         Print what differs between two Clip Studio files:
                    chunks, tables, columns, rows and values
  --bytes <n>       Show up to <n> bytes of a blob Sue cannot decode
                    (256 unless given; 0 for none)
  --nulls           Also list the columns of a row that are NULL
  --no-inflate      Do not decompress bitmap blocks to learn their size";

/// What the person asked for beside the files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The most bytes of an undecoded blob to show.
    pub bytes: usize,
    /// Whether NULL columns are listed.
    pub nulls: bool,
    /// Whether bitmap blocks are decompressed to learn their size.
    pub inflate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bytes: 256,
            nulls: false,
            inflate: true,
        }
    }
}

/// A blob up to this size is read whole to see whether its layout is known.
const DECODE_LIMIT: usize = 1 << 20;
/// Two blobs are compared this many bytes at a time.
const COMPARE_STEP: usize = 4 << 20;
/// The most runs of numbers printed for one list.
const MAX_RUNS: usize = 48;
/// The most characters of a text value printed.
const MAX_TEXT: usize = 200;

/// What `sue-cli clip <args>` was asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command<'a> {
    /// Print how the subcommands are used.
    Help,
    /// Print what one file holds.
    Dump(&'a str),
    /// Print what differs between two files.
    Diff(&'a str, &'a str),
}

/// Reads the words after `clip`. `None` when they are not a command.
fn parse(args: &[String]) -> Option<(Command<'_>, Options)> {
    let mut options = Options::default();
    let mut words = Vec::new();
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        match arg {
            "--help" | "-h" => return Some((Command::Help, options)),
            "--nulls" => options.nulls = true,
            "--no-inflate" => options.inflate = false,
            "--bytes" => options.bytes = rest.next()?.parse().ok()?,
            flag if flag.starts_with("--") => return None,
            word => words.push(word),
        }
    }
    let command = match words.as_slice() {
        ["dump", file] => Command::Dump(file),
        ["diff", before, after] => Command::Diff(before, after),
        _ => return None,
    };
    Some((command, options))
}

/// Runs `sue-cli clip <args>`.
pub fn run(args: &[String]) -> ExitCode {
    let Some((command, options)) = parse(args) else {
        return usage();
    };
    let result = match command {
        Command::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Command::Dump(file) => read(file).and_then(|bytes| dump(file, &bytes, options)),
        Command::Diff(before, after) => read(before).and_then(|a| {
            let b = read(after)?;
            diff((before, &a), (after, &b), options)
        }),
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sue-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("{path}: {error}"))
}

/// A `.clip` taken apart as far as it goes.
struct Opened<'a> {
    name: &'a str,
    bytes: &'a [u8],
    layout: Layout<'a>,
    /// The stored bitmaps' payloads by id.
    external: HashMap<String, &'a [u8]>,
    /// The size of the `SQLi` chunk, when there is one.
    database_size: Option<usize>,
    database: Option<Database<'a>>,
    /// Why the database could not be opened, when it could not.
    database_problem: Option<String>,
}

impl<'a> Opened<'a> {
    fn new(name: &'a str, bytes: &'a [u8]) -> Result<Self, String> {
        let layout = layout(bytes).map_err(|error| format!("{name}: {error}"))?;
        let mut external = HashMap::new();
        let mut database_bytes = None;
        for chunk in &layout.chunks {
            match &chunk.kind {
                b"Exta" => {
                    if let Ok((id, payload)) = chunk.external() {
                        external.insert(id, payload);
                    }
                }
                b"SQLi" => database_bytes = Some(chunk.data),
                _ => {}
            }
        }
        let (database, database_problem) = match database_bytes.map(Database::open) {
            Some(Ok(database)) => (Some(database), None),
            Some(Err(error)) => (None, Some(error)),
            None => (None, Some("the file has no SQLi chunk".to_owned())),
        };
        Ok(Self {
            name,
            bytes,
            layout,
            external,
            database_size: database_bytes.map(<[u8]>::len),
            database,
            database_problem,
        })
    }

    /// The names of the database's tables, in schema order.
    fn tables(&self) -> Vec<&str> {
        self.database.as_ref().map_or_else(Vec::new, |database| {
            database
                .schema()
                .iter()
                .filter(|entry| entry.kind == "table")
                .map(|entry| entry.name.as_str())
                .collect()
        })
    }

    fn create(&self, table: &str) -> &str {
        self.database
            .as_ref()
            .and_then(|database| {
                database
                    .schema()
                    .iter()
                    .find(|entry| entry.kind == "table" && entry.name == table)
            })
            .map_or("", |entry| entry.sql.as_str())
    }

    fn table(&self, table: &str) -> Result<LazyTable, String> {
        self.database
            .as_ref()
            .ok_or("there is no database")?
            .lazy_table(table)?
            .ok_or_else(|| "the table has no rows to walk".to_owned())
    }

    /// The first `len` bytes of `blob` (all of it when it is shorter).
    fn start(&self, blob: &BlobRef, len: usize) -> Vec<u8> {
        self.database
            .as_ref()
            .and_then(|database| database.blob_range(blob, 0, len).ok())
            .unwrap_or_default()
    }

    /// The stored bitmap `blob` names, when it is the id of one.
    fn named_bitmap(&self, blob: &BlobRef) -> Option<&'a [u8]> {
        self.external.get(&self.bitmap_id(blob)?).copied()
    }

    /// The text of a blob that names a stored bitmap (`extrnlid` and
    /// more), whether or not the file has a chunk by that name.
    fn bitmap_id(&self, blob: &BlobRef) -> Option<String> {
        if blob.is_empty() || blob.len() > 256 {
            return None;
        }
        String::from_utf8(self.start(blob, blob.len()))
            .ok()
            .filter(|id| id.starts_with("extrnlid"))
    }
}

// ----- Words for values --------------------------------------------------------

/// `values` with repeats folded: `1 (x6) 0 7 7`.
fn runs<T: Display + PartialEq>(values: &[T]) -> String {
    let mut out = String::new();
    let mut printed = 0;
    let mut at = 0;
    while at < values.len() {
        let same = values[at..]
            .iter()
            .take_while(|value| **value == values[at])
            .count();
        if printed == MAX_RUNS {
            let _ = write!(out, " ... and {} more", values.len() - at);
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        if same > 2 {
            let _ = write!(out, "{} (x{same})", values[at]);
        } else {
            let _ = write!(out, "{}", values[at]);
            if same == 2 {
                let _ = write!(out, " {}", values[at]);
            }
        }
        printed += 1;
        at += same;
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// `bytes` (which start at `offset` of their blob) sixteen to a line with
/// the letters beside them, each line after `indent`.
fn hex_lines(out: &mut String, indent: &str, offset: usize, bytes: &[u8]) {
    for (i, line) in bytes.chunks(16).enumerate() {
        let letters: String = line
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    char::from(*byte)
                } else {
                    '.'
                }
            })
            .collect();
        let _ = writeln!(
            out,
            "{indent}{:06x}  {:<47}  |{letters}|",
            offset + i * 16,
            hex(line)
        );
    }
}

/// The first `limit` bytes of `bytes` as hex lines, and how many follow.
fn preview(out: &mut String, indent: &str, bytes: &[u8], whole: usize, limit: usize) {
    let shown = &bytes[..bytes.len().min(limit)];
    hex_lines(out, indent, 0, shown);
    if whole > shown.len() && limit > 0 {
        let _ = writeln!(out, "{indent}... {} more bytes", whole - shown.len());
    }
}

fn printable(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
}

/// The first byte at which `x` and `y` differ, when they do: the end of
/// the shorter one when it is the start of the other.
fn first_mismatch(x: &[u8], y: &[u8]) -> Option<usize> {
    x.iter()
        .zip(y)
        .position(|(p, q)| p != q)
        .or_else(|| (x.len() != y.len()).then_some(x.len().min(y.len())))
}

/// Where a window of bytes around byte `at` starts: a line of sixteen
/// before the line `at` is on.
fn window_start(at: usize) -> usize {
    at.saturating_sub(16) / 16 * 16
}

/// The chunks of each kind in `opened`, in file order.
fn chunks_by_kind<'a>(opened: &Opened<'a>) -> BTreeMap<String, Vec<&'a [u8]>> {
    let mut kinds: BTreeMap<String, Vec<&'a [u8]>> = BTreeMap::new();
    for chunk in &opened.layout.chunks {
        kinds
            .entry(kind_text(chunk.kind))
            .or_default()
            .push(chunk.data);
    }
    kinds
}

/// One value on one line.
fn value_text(opened: &Opened<'_>, value: &LazyValue) -> String {
    match value {
        LazyValue::Null => "null".to_owned(),
        LazyValue::Integer(value) => value.to_string(),
        LazyValue::Real(value) => format!("{value:?}"),
        LazyValue::Text(text) => {
            let count = text.chars().count();
            if count > MAX_TEXT {
                let start: String = text.chars().take(MAX_TEXT).collect();
                format!("{start:?} ... ({count} characters)")
            } else {
                format!("{text:?}")
            }
        }
        LazyValue::Blob(blob) => {
            let mut out = format!("blob, {} bytes", blob.len());
            if blob.len() <= 128 {
                let bytes = opened.start(blob, blob.len());
                if printable(&bytes) {
                    let _ = write!(out, ", the text {:?}", String::from_utf8_lossy(&bytes));
                }
            }
            out
        }
    }
}

/// What a stored bitmap holds, a line per fact.
fn bitmap_lines(payload: &[u8], options: Options) -> Vec<String> {
    let survey = survey_bitmap(payload, options.inflate);
    let mut lines = Vec::new();
    if survey.blocks.is_empty() && survey.lists.is_empty() {
        lines.push(format!("{} bytes that are no blocks", payload.len()));
        return lines;
    }
    let with_data = survey
        .blocks
        .iter()
        .filter(|block| block.compressed.is_some())
        .count();
    lines.push(format!(
        "blocks: {}, {with_data} with data",
        survey.blocks.len()
    ));
    let headers: Vec<String> = survey
        .blocks
        .iter()
        .map(|block| {
            format!(
                "[raw size {}, {} by {}]",
                block.raw_size, block.size[0], block.size[1]
            )
        })
        .collect();
    lines.push(format!("block headers: {}", runs(&headers)));
    let numbered = survey
        .blocks
        .iter()
        .zip(0_u32..)
        .all(|(block, i)| block.index == i);
    if !numbered {
        let indexes: Vec<u32> = survey.blocks.iter().map(|block| block.index).collect();
        lines.push(format!("block numbers: {}", runs(&indexes)));
    }
    if options.inflate && with_data > 0 {
        let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
        let mut failed = 0;
        for block in &survey.blocks {
            match (block.compressed, block.inflated) {
                (Some(_), Some(size)) => *sizes.entry(size).or_default() += 1,
                (Some(_), None) => failed += 1,
                _ => {}
            }
        }
        let sizes: Vec<String> = sizes
            .iter()
            .map(|(size, count)| format!("{size} bytes (x{count})"))
            .collect();
        if !sizes.is_empty() {
            lines.push(format!("a block inflates to: {}", sizes.join(", ")));
        }
        if failed > 0 {
            lines.push(format!("blocks that do not inflate: {failed}"));
        }
    }
    for list in &survey.lists {
        lines.push(format!(
            "{}: header {}; values {}",
            list.name,
            runs(&list.head),
            runs(&list.values)
        ));
    }
    if survey.leftover > 0 {
        lines.push(format!("then {} bytes that are neither", survey.leftover));
    }
    lines
}

/// What is inside `blob`, in lines after `indent`: its named parts when
/// it is laid out as an offscreen attribute, the bitmap it names when it
/// is the id of one, or else its first bytes.
fn blob_lines(opened: &Opened<'_>, blob: &BlobRef, options: Options) -> Vec<String> {
    let mut lines = Vec::new();
    if blob.is_empty() {
        return lines;
    }
    if let Some(payload) = opened.named_bitmap(blob) {
        lines.push(format!(
            "names a stored bitmap (an Exta chunk) of {} bytes:",
            payload.len()
        ));
        lines.extend(
            bitmap_lines(payload, options)
                .into_iter()
                .map(|line| format!("  {line}")),
        );
        return lines;
    }
    if let Some(lines) = attribute_lines(opened, blob) {
        return lines;
    }
    let read = if blob.len() <= DECODE_LIMIT {
        blob.len()
    } else {
        options.bytes
    };
    let bytes = opened.start(blob, read);
    if printable(&bytes) && bytes.len() <= 128 {
        return lines;
    }
    let mut text = String::new();
    preview(&mut text, "", &bytes, blob.len(), options.bytes);
    lines.extend(text.lines().map(str::to_owned));
    lines
}

/// The named parts of `blob`, a line each, when it is laid out as an
/// offscreen attribute.
fn attribute_lines(opened: &Opened<'_>, blob: &BlobRef) -> Option<Vec<String>> {
    if blob.is_empty() || blob.len() > DECODE_LIMIT {
        return None;
    }
    let bytes = opened.start(blob, blob.len());
    if bytes.len() != blob.len() {
        return None;
    }
    let lines = attribute_parts(&bytes)?
        .into_iter()
        .map(|part| {
            let mut line = format!("{}: {}", part.name, runs(&part.values));
            if !part.rest.is_empty() {
                let _ = write!(line, "; then the bytes {}", hex(&part.rest));
            }
            line
        })
        .collect();
    Some(lines)
}

// ----- dump ----------------------------------------------------------------------

fn kind_text(kind: [u8; 4]) -> String {
    if printable(&kind) {
        String::from_utf8_lossy(&kind).into_owned()
    } else {
        format!("[{}]", hex(&kind))
    }
}

fn dump_chunks(out: &mut String, opened: &Opened<'_>, options: Options) {
    let layout = &opened.layout;
    let size = opened.bytes.len();
    let _ = writeln!(out, "file: {}, {size} bytes", opened.name);
    let agrees = if layout.declared_size == size as u64 {
        "as the file is".to_owned()
    } else {
        format!("but the file has {size}")
    };
    let _ = writeln!(
        out,
        "header: CSFCHUNK, says {} bytes ({agrees}), first chunk at byte {}",
        layout.declared_size, layout.first_chunk
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "chunks: {}", layout.chunks.len());
    for (number, chunk) in (1..).zip(&layout.chunks) {
        let _ = writeln!(
            out,
            "  {number}. {} at byte {}, {} bytes of data",
            kind_text(chunk.kind),
            chunk.offset,
            chunk.data.len()
        );
        let indent = "       ";
        match &chunk.kind {
            b"Exta" => match chunk.external() {
                Ok((id, payload)) => {
                    let _ = writeln!(out, "{indent}id: {id}");
                    let _ = writeln!(out, "{indent}payload: {} bytes", payload.len());
                    for line in bitmap_lines(payload, options) {
                        let _ = writeln!(out, "{indent}{line}");
                    }
                }
                Err(error) => {
                    let _ = writeln!(out, "{indent}broken: {error}");
                    preview(out, indent, chunk.data, chunk.data.len(), options.bytes);
                }
            },
            b"SQLi" => {
                let _ = writeln!(out, "{indent}the database, listed below");
            }
            _ => {
                let (numbers, rest) = chunk.data.as_chunks::<8>();
                if !numbers.is_empty() && rest.is_empty() && numbers.len() <= 16 {
                    let numbers: Vec<u64> =
                        numbers.iter().map(|raw| u64::from_be_bytes(*raw)).collect();
                    let _ = writeln!(out, "{indent}as 64-bit numbers: {}", runs(&numbers));
                }
                preview(out, indent, chunk.data, chunk.data.len(), options.bytes);
            }
        }
    }
    if layout.trailing > 0 {
        let _ = writeln!(
            out,
            "  after the last chunk: {} bytes that are no chunk",
            layout.trailing
        );
    }
    if let Some(problem) = &layout.problem {
        let _ = writeln!(out, "  the chunks stop there: {problem}");
    }
}

/// The name a row goes by: its `MainId` (or rowid) and its name, if any.
fn row_label(table: &LazyTable, row: &(i64, Vec<LazyValue>)) -> String {
    let mut label = match table.column("MainId").and_then(|i| row.1.get(i)?.int()) {
        Some(id) => format!("row {} (MainId {id})", row.0),
        None => format!("row {}", row.0),
    };
    let name = table
        .columns
        .iter()
        .zip(&row.1)
        .find_map(|(column, value)| column.ends_with("Name").then(|| value.text()).flatten());
    if let Some(name) = name {
        let _ = write!(label, " {name:?}");
    }
    label
}

/// The values of one row, a column per line, after `indent`.
fn row_lines(
    out: &mut String,
    indent: &str,
    opened: &Opened<'_>,
    table: &LazyTable,
    row: &(i64, Vec<LazyValue>),
    options: Options,
) {
    let mut nulls = Vec::new();
    for (i, value) in row.1.iter().enumerate() {
        let column = table
            .columns
            .get(i)
            .map_or_else(|| format!("(value {})", i + 1), Clone::clone);
        if *value == LazyValue::Null {
            nulls.push(column);
            continue;
        }
        let _ = writeln!(out, "{indent}{column} = {}", value_text(opened, value));
        if let LazyValue::Blob(blob) = value {
            for line in blob_lines(opened, blob, options) {
                let _ = writeln!(out, "{indent}    {line}");
            }
        }
    }
    if options.nulls && !nulls.is_empty() {
        let _ = writeln!(out, "{indent}null: {}", nulls.join(", "));
    }
}

fn dump_database(out: &mut String, opened: &Opened<'_>, options: Options) {
    let _ = writeln!(out);
    let Some(database) = &opened.database else {
        let _ = writeln!(
            out,
            "database: none that can be read: {}",
            opened.database_problem.as_deref().unwrap_or_default()
        );
        return;
    };
    let _ = writeln!(
        out,
        "database: {} bytes, {} pages of {} bytes, text in {}",
        opened.database_size.unwrap_or(0),
        database.page_count(),
        database.page_size(),
        database.text_encoding()
    );
    let tables = opened.tables();
    let others: Vec<_> = database
        .schema()
        .iter()
        .filter(|entry| entry.kind != "table")
        .collect();
    let _ = writeln!(
        out,
        "schema: {} tables, {} other entries",
        tables.len(),
        others.len()
    );
    for entry in others {
        let _ = writeln!(
            out,
            "  {} {} on {}: {}",
            entry.kind, entry.name, entry.table, entry.sql
        );
    }
    for name in tables {
        let _ = writeln!(out);
        let table = match opened.table(name) {
            Ok(table) => table,
            Err(error) => {
                let _ = writeln!(out, "table {name}: cannot be read: {error}");
                let _ = writeln!(out, "  {}", opened.create(name));
                continue;
            }
        };
        let _ = writeln!(out, "table {name}: {} rows", table.rows.len());
        let _ = writeln!(out, "  {}", opened.create(name));
        for row in &table.rows {
            let _ = writeln!(out, "  {}", row_label(&table, row));
            row_lines(out, "    ", opened, &table, row, options);
        }
    }
}

/// What `clip dump` prints for the file `name` holding `bytes`.
///
/// # Errors
/// When `bytes` does not start as a Clip Studio file.
pub fn dump(name: &str, bytes: &[u8], options: Options) -> Result<String, String> {
    let opened = Opened::new(name, bytes)?;
    let mut out = String::new();
    dump_chunks(&mut out, &opened, options);
    dump_database(&mut out, &opened, options);
    Ok(out)
}

// ----- diff ----------------------------------------------------------------------

/// The two files of a comparison, and what was found so far.
struct Diff<'a> {
    a: Opened<'a>,
    b: Opened<'a>,
    options: Options,
    out: String,
    count: usize,
}

impl Diff<'_> {
    /// Notes one difference.
    fn note(&mut self, line: &str) {
        self.count += 1;
        let _ = writeln!(self.out, "{line}");
    }

    fn detail(&mut self, line: &str) {
        let _ = writeln!(self.out, "    {line}");
    }

    /// Details the bytes of both sides from byte `from` of what they are
    /// part of, as many as `--bytes` allows and 64 at most.
    fn around(&mut self, from: usize, before: &[u8], after: &[u8]) {
        let window = self.options.bytes.min(64);
        for (label, bytes) in [("before:", before), ("after: ", after)] {
            let mut text = String::new();
            hex_lines(&mut text, "", from, &bytes[..bytes.len().min(window)]);
            for line in text.lines() {
                self.detail(&format!("{label} {line}"));
            }
        }
    }

    fn chunks(&mut self) {
        let (a, b) = (chunks_by_kind(&self.a), chunks_by_kind(&self.b));
        let names: Vec<&String> = a
            .keys()
            .chain(b.keys().filter(|k| !a.contains_key(*k)))
            .collect();
        for name in names {
            let (before, after) = (
                a.get(name).map_or(&[][..], Vec::as_slice),
                b.get(name).map_or(&[][..], Vec::as_slice),
            );
            if before.len() != after.len() {
                self.note(&format!(
                    "chunks: {name}: {} before, {} after",
                    before.len(),
                    after.len()
                ));
                continue;
            }
            // The database is compared table by table, and a stored
            // bitmap where a row names it.
            if name == "SQLi" || name == "Exta" {
                continue;
            }
            for (number, (x, y)) in (1..).zip(before.iter().zip(after)) {
                let Some(at) = first_mismatch(x, y) else {
                    continue;
                };
                let which = if before.len() > 1 {
                    format!(" (number {number} of its kind)")
                } else {
                    String::new()
                };
                self.note(&format!(
                    "chunks: {name}{which}: {} bytes before, {} after, first difference at byte {at}",
                    x.len(),
                    y.len()
                ));
                let from = window_start(at);
                self.around(
                    from,
                    x.get(from..).unwrap_or_default(),
                    y.get(from..).unwrap_or_default(),
                );
            }
        }
        for (label, x, y) in [
            (
                "bytes after the last chunk",
                self.a.layout.trailing,
                self.b.layout.trailing,
            ),
            (
                "the first chunk's place",
                usize::try_from(self.a.layout.first_chunk).unwrap_or(usize::MAX),
                usize::try_from(self.b.layout.first_chunk).unwrap_or(usize::MAX),
            ),
        ] {
            if x != y {
                self.note(&format!("chunks: {label}: {x} before, {y} after"));
            }
        }
        let problems = (self.a.layout.problem.clone(), self.b.layout.problem.clone());
        if problems.0 != problems.1 {
            self.note(&format!(
                "chunks: broken before: {}; after: {}",
                problems.0.as_deref().unwrap_or("no"),
                problems.1.as_deref().unwrap_or("no")
            ));
        }
    }

    /// The first byte at which two blobs differ, when they do.
    fn first_difference(&self, x: &BlobRef, y: &BlobRef) -> Result<Option<usize>, String> {
        let (Some(a), Some(b)) = (&self.a.database, &self.b.database) else {
            return Ok(None);
        };
        let shared = x.len().min(y.len());
        let mut at = 0;
        while at < shared {
            let len = COMPARE_STEP.min(shared - at);
            let (before, after) = (a.blob_range(x, at, len)?, b.blob_range(y, at, len)?);
            if let Some(i) = before.iter().zip(&after).position(|(p, q)| p != q) {
                return Ok(Some(at + i));
            }
            at += len;
        }
        Ok((x.len() != y.len()).then_some(shared))
    }

    /// Compares two blobs of one column; notes and details any difference.
    fn blobs(&mut self, place: &str, x: &BlobRef, y: &BlobRef) {
        // Two ids of stored bitmaps are only names: compare the bitmaps.
        if let (Some(before), Some(after)) = (self.a.named_bitmap(x), self.b.named_bitmap(y)) {
            let Some(at) = first_mismatch(before, after) else {
                return;
            };
            self.note(&format!("{place}: the stored bitmap it names differs"));
            let (p, q) = (
                bitmap_lines(before, self.options),
                bitmap_lines(after, self.options),
            );
            if p == q {
                // The same blocks and lists: only what the blocks hold.
                self.detail(&format!(
                    "the same blocks and lists, other data: {} bytes before, {} after, first difference at byte {at}",
                    before.len(),
                    after.len()
                ));
                return;
            }
            for line in p {
                self.detail(&format!("before: {line}"));
            }
            for line in q {
                self.detail(&format!("after:  {line}"));
            }
            return;
        }
        // Ids that name no stored bitmap on either side (a folder's)
        // are only names too.
        if self
            .a
            .bitmap_id(x)
            .is_some_and(|id| !self.a.external.contains_key(&id))
            && self
                .b
                .bitmap_id(y)
                .is_some_and(|id| !self.b.external.contains_key(&id))
        {
            return;
        }
        let at = match self.first_difference(x, y) {
            Ok(None) => return,
            Ok(Some(at)) => at,
            Err(error) => {
                self.note(&format!("{place}: the blobs cannot be compared: {error}"));
                return;
            }
        };
        self.note(&format!(
            "{place}: blob, {} bytes before, {} after, first difference at byte {at}",
            x.len(),
            y.len()
        ));
        let known = (attribute_lines(&self.a, x), attribute_lines(&self.b, y));
        if known.0.is_some() || known.1.is_some() {
            // Known layouts: the lines that differ, side by side.
            let (before, after) = (known.0.unwrap_or_default(), known.1.unwrap_or_default());
            for i in 0..before.len().max(after.len()) {
                let (p, q) = (before.get(i), after.get(i));
                if p != q {
                    self.detail(&format!(
                        "before: {}",
                        p.map_or("(nothing)", String::as_str)
                    ));
                    self.detail(&format!(
                        "after:  {}",
                        q.map_or("(nothing)", String::as_str)
                    ));
                }
            }
            return;
        }
        // Unknown layouts: the bytes around the first difference.
        let window = self.options.bytes.min(64);
        let from = window_start(at);
        let part = |opened: &Opened<'_>, blob: &BlobRef| {
            opened
                .database
                .as_ref()
                .and_then(|database| database.blob_range(blob, from, window).ok())
                .unwrap_or_default()
        };
        let (before, after) = (part(&self.a, x), part(&self.b, y));
        self.around(from, &before, &after);
    }

    fn rows(&mut self, name: &str, before: &LazyTable, after: &LazyTable) {
        if before.rows.len() != after.rows.len() {
            self.note(&format!(
                "table {name}: {} rows before, {} after",
                before.rows.len(),
                after.rows.len()
            ));
        }
        // Rows pair by MainId when both tables have one for every row.
        let keys = |table: &LazyTable| -> Option<Vec<i64>> {
            let column = table.column("MainId")?;
            let keys: Vec<i64> = table
                .rows
                .iter()
                .map(|row| row.1.get(column)?.int())
                .collect::<Option<_>>()?;
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            sorted.dedup();
            (sorted.len() == keys.len()).then_some(keys)
        };
        let rowids = |table: &LazyTable| table.rows.iter().map(|row| row.0).collect::<Vec<_>>();
        let (keys_a, keys_b, by) = match (keys(before), keys(after)) {
            (Some(a), Some(b)) => (a, b, "MainId"),
            _ => (rowids(before), rowids(after), "rowid"),
        };
        let index_b: HashMap<i64, usize> = keys_b.iter().copied().zip(0..).collect();
        let index_a: HashMap<i64, usize> = keys_a.iter().copied().zip(0..).collect();
        for (i, key) in keys_a.iter().enumerate() {
            let row = &before.rows[i];
            let Some(j) = index_b.get(key) else {
                self.note(&format!(
                    "table {name}: only before: {} (paired by {by})",
                    row_label(before, row)
                ));
                continue;
            };
            let other = &after.rows[*j];
            let label = row_label(after, other);
            for (c, column) in before.columns.iter().enumerate() {
                let Some(d) = after.column(column) else {
                    continue;
                };
                let (x, y) = (
                    row.1.get(c).unwrap_or(&LazyValue::Null),
                    other.1.get(d).unwrap_or(&LazyValue::Null),
                );
                let place = format!("table {name}, {label}: {column}");
                if let (LazyValue::Blob(x), LazyValue::Blob(y)) = (x, y) {
                    self.blobs(&place, x, y);
                } else if x != y {
                    let (x, y) = (value_text(&self.a, x), value_text(&self.b, y));
                    self.note(&format!("{place}: {x} before, {y} after"));
                }
            }
        }
        for (j, key) in keys_b.iter().enumerate() {
            if index_a.contains_key(key) {
                continue;
            }
            let row = &after.rows[j];
            self.note(&format!(
                "table {name}: only after: {} (paired by {by})",
                row_label(after, row)
            ));
            let mut text = String::new();
            row_lines(&mut text, "", &self.b, after, row, self.options);
            let lines: Vec<String> = text.lines().map(str::to_owned).collect();
            for line in lines {
                self.detail(&line);
            }
        }
    }

    /// Compares what the schemas hold beside tables: indexes, views and
    /// triggers, by kind and name.
    fn other_entries(&mut self) {
        let others = |opened: &Opened<'_>| -> BTreeMap<(String, String), String> {
            opened
                .database
                .iter()
                .flat_map(|database| database.schema())
                .filter(|entry| entry.kind != "table")
                .map(|entry| {
                    let about = format!("on {}: {}", entry.table, entry.sql);
                    ((entry.kind.clone(), entry.name.clone()), about)
                })
                .collect()
        };
        let (a, b) = (others(&self.a), others(&self.b));
        for (key, before) in &a {
            let (kind, name) = key;
            match b.get(key) {
                None => self.note(&format!("schema: {kind} {name}: only before, {before}")),
                Some(after) if after != before => {
                    self.note(&format!("schema: {kind} {name} differs"));
                    self.detail(&format!("before: {before}"));
                    self.detail(&format!("after:  {after}"));
                }
                Some(_) => {}
            }
        }
        for (key, after) in &b {
            if !a.contains_key(key) {
                let (kind, name) = key;
                self.note(&format!("schema: {kind} {name}: only after, {after}"));
            }
        }
    }

    fn tables(&mut self) {
        if self.a.database.is_none() || self.b.database.is_none() {
            let problems = (
                self.a.database_problem.clone().unwrap_or_default(),
                self.b.database_problem.clone().unwrap_or_default(),
            );
            self.note(&format!(
                "database: cannot be compared; before: {}; after: {}",
                if problems.0.is_empty() {
                    "readable"
                } else {
                    &problems.0
                },
                if problems.1.is_empty() {
                    "readable"
                } else {
                    &problems.1
                }
            ));
            return;
        }
        self.other_entries();
        let names_a: Vec<String> = self.a.tables().into_iter().map(str::to_owned).collect();
        let names_b: Vec<String> = self.b.tables().into_iter().map(str::to_owned).collect();
        for name in &names_a {
            if !names_b.contains(name) {
                let rows = self.a.table(name).map_or(0, |table| table.rows.len());
                self.note(&format!("table {name}: only before, {rows} rows"));
                let create = self.a.create(name).to_owned();
                self.detail(&create);
            }
        }
        for name in &names_b {
            if !names_a.contains(name) {
                let rows = self.b.table(name).map_or(0, |table| table.rows.len());
                self.note(&format!("table {name}: only after, {rows} rows"));
                let create = self.b.create(name).to_owned();
                self.detail(&create);
                if let Ok(table) = self.b.table(name) {
                    let mut text = String::new();
                    for row in &table.rows {
                        let _ = writeln!(text, "{}", row_label(&table, row));
                        row_lines(&mut text, "  ", &self.b, &table, row, self.options);
                    }
                    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                    for line in lines {
                        self.detail(&line);
                    }
                }
            }
        }
        for name in names_a.iter().filter(|name| names_b.contains(name)) {
            let (before, after) = match (self.a.table(name), self.b.table(name)) {
                (Ok(before), Ok(after)) => (before, after),
                (before, after) => {
                    self.note(&format!(
                        "table {name}: cannot be compared; before: {}; after: {}",
                        before.err().unwrap_or_else(|| "readable".to_owned()),
                        after.err().unwrap_or_else(|| "readable".to_owned())
                    ));
                    continue;
                }
            };
            for column in &before.columns {
                if after.column(column).is_none() {
                    self.note(&format!("table {name}: column {column} only before"));
                }
            }
            for column in &after.columns {
                if before.column(column).is_none() {
                    self.note(&format!("table {name}: column {column} only after"));
                }
            }
            if before.columns == after.columns && self.a.create(name) != self.b.create(name) {
                self.note(&format!("table {name}: the CREATE text differs"));
                let (x, y) = (
                    self.a.create(name).to_owned(),
                    self.b.create(name).to_owned(),
                );
                self.detail(&format!("before: {x}"));
                self.detail(&format!("after:  {y}"));
            }
            self.rows(name, &before, &after);
        }
    }
}

/// What `clip diff` prints for the files `before` and `after` (each a name
/// and its bytes).
///
/// # Errors
/// When either does not start as a Clip Studio file.
pub fn diff(
    before: (&str, &[u8]),
    after: (&str, &[u8]),
    options: Options,
) -> Result<String, String> {
    let mut diff = Diff {
        a: Opened::new(before.0, before.1)?,
        b: Opened::new(after.0, after.1)?,
        options,
        out: String::new(),
        count: 0,
    };
    let _ = writeln!(diff.out, "before: {}, {} bytes", before.0, before.1.len());
    let _ = writeln!(diff.out, "after:  {}, {} bytes", after.0, after.1.len());
    let _ = writeln!(diff.out);
    diff.chunks();
    diff.tables();
    if diff.count == 0 {
        let _ = writeln!(diff.out, "no differences in chunks, tables, rows or values");
    } else {
        let _ = writeln!(diff.out);
        let _ = writeln!(diff.out, "{} differences", diff.count);
    }
    Ok(diff.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_io::clip::encode_clip;
    use sue_io::psd::{Psd, PsdKind, PsdLayer};

    fn square(name: &str, left: i32, size: u32, color: [u8; 3]) -> PsdLayer {
        let pixel = [color[0], color[1], color[2], 255];
        PsdLayer::pixels(
            name,
            left,
            10,
            size,
            size,
            pixel.repeat((size * size) as usize),
        )
    }

    fn picture() -> Psd {
        let mut folder = PsdLayer::folder("Inks", PsdKind::FolderOpen { open: true });
        folder.opacity = 128;
        Psd {
            width: 600,
            height: 300,
            layers: vec![
                square("Back", 0, 280, [200, 10, 10]),
                PsdLayer::folder("</Layer group>", PsdKind::FolderEnd),
                square("Line \u{65e5}\u{672c}", 300, 40, [0, 0, 0]),
                folder,
            ],
            merged: Vec::new(),
        }
    }

    fn file(psd: &Psd, sue: Option<&[u8]>) -> Vec<u8> {
        encode_clip(psd, 350.0, sue).unwrap()
    }

    #[test]
    fn a_dump_lists_chunks_tables_create_text_rows_and_decoded_blobs() {
        let bytes = file(&picture(), Some(&[7; 5000]));
        let text = dump("picture.clip", &bytes, Options::default()).unwrap();
        // The header and every chunk.
        assert!(text.contains(&format!("file: picture.clip, {} bytes", bytes.len())));
        assert!(text.contains("(as the file is), first chunk at byte 24"));
        // Two layers have pixels; folders and the root have none.
        assert!(text.contains("chunks: 5"), "{text}");
        assert!(text.contains("  1. Head at byte 24, 40 bytes of data"));
        assert!(text.contains("as 64-bit numbers: 256 "));
        assert_eq!(text.matches(". Exta at byte ").count(), 2);
        assert!(text.contains("id: extrnlid"));
        assert!(text.contains("blocks: 6, 4 with data"), "{text}");
        assert!(text.contains("block headers: [raw size 327680, 256 by 256] (x6)"));
        assert!(text.contains("a block inflates to: 327680 bytes (x4)"));
        assert!(text.contains("BlockStatus: header 12 6 4; values 1 (x6)"));
        // Each block with data has its checksum, an empty one 0.
        assert!(text.contains("BlockCheckSum: header 12 6 4; values "));
        assert!(!text.contains("BlockCheckSum: header 12 6 4; values 0 (x6)"));
        assert!(text.contains(". Foot at byte "));
        // The database, each table with its CREATE text and row count.
        assert!(text.contains("pages of 4096 bytes, text in UTF-8"));
        assert!(
            text.contains("schema: 14 tables, 0 other entries"),
            "{text}"
        );
        assert!(text.contains("table Layer: 4 rows\n  CREATE TABLE Layer(_PW_ID INTEGER"));
        assert!(text.contains("table Canvas: 1 rows\n  CREATE TABLE Canvas("));
        assert!(text.contains("table Offscreen: 4 rows"));
        assert!(text.contains("table sqlite_sequence: "));
        assert!(text.contains("    CanvasWidth = 600.0\n"));
        assert!(text.contains("    CanvasResolution = 350.0\n"));
        assert!(text.contains("\"Line \u{65e5}\u{672c}\""));
        // Clip Studio counts opacity to 256: 128 of 255 is 129 there.
        assert!(text.contains("    LayerOpacity = 129"), "{text}");
        // Blobs: an attribute by its parts, a bitmap id by its bitmap, and
        // an unknown blob by its first bytes.
        assert!(text.contains("    Attribute = blob, 218 bytes"), "{text}");
        assert!(text.contains("        Parameter: 600 300 3 2 33 1 4 5 65536"));
        assert!(text.contains("        InitColor: 20 0 (x3) 4\n"));
        assert!(text.contains("        BlockSize: 12 6 4 "));
        assert!(text.contains("    BlockData = blob, 40 bytes, the text \"extrnlid"));
        assert!(text.contains("        names a stored bitmap (an Exta chunk) of "));
        assert!(text.contains("          blocks: 6, 4 with data"));
        assert!(text.contains("    Data = blob, 5000 bytes\n"));
        assert!(text.contains("        000000  07 07 07 07"));
        assert!(text.contains("        ... 4744 more bytes"));
        assert!(!text.contains("null: "));

        // The options: NULL columns, fewer bytes, no inflating.
        let options = Options {
            bytes: 0,
            nulls: true,
            inflate: false,
        };
        let text = dump("picture.clip", &bytes, options).unwrap();
        // Sue's own files have no NULL column; a row that has some lists
        // them only when asked.
        let opened = Opened::new("picture.clip", &bytes).unwrap();
        let table = LazyTable {
            name: "Made".to_owned(),
            columns: vec!["A".to_owned(), "B".to_owned(), "C".to_owned()],
            rows: vec![(
                1,
                vec![LazyValue::Null, LazyValue::Integer(4), LazyValue::Null],
            )],
        };
        let mut asked = String::new();
        row_lines(&mut asked, "  ", &opened, &table, &table.rows[0], options);
        assert_eq!(asked, "  B = 4\n  null: A, C\n");
        let mut plain = String::new();
        let quiet = Options::default();
        row_lines(&mut plain, "  ", &opened, &table, &table.rows[0], quiet);
        assert_eq!(plain, "  B = 4\n");
        assert!(!text.contains("000000  07"));
        assert!(!text.contains("inflates to"));
    }

    #[test]
    fn a_diff_of_a_file_with_itself_finds_nothing() {
        let bytes = file(&picture(), None);
        let text = diff(("a.clip", &bytes), ("b.clip", &bytes), Options::default()).unwrap();
        assert!(
            text.ends_with("no differences in chunks, tables, rows or values\n"),
            "{text}"
        );
    }

    #[test]
    fn a_diff_names_the_values_rows_and_tables_that_changed() {
        let before = picture();
        let mut after = picture();
        after.layers[0].blend = *b"mul ";
        after.layers[0].visible = false;
        after.layers.push(square("New", 500, 300, [1, 2, 3]));
        let (a, b) = (file(&before, None), file(&after, Some(b"SUE project")));
        let text = diff(("a.clip", &a), ("b.clip", &b), Options::default()).unwrap();
        assert!(text.contains("chunks: Exta: 2 before, 3 after"), "{text}");
        assert!(
            text.contains("table SueProject: only after, 1 rows"),
            "{text}"
        );
        assert!(text.contains("    CREATE TABLE SueProject(Version INTEGER, Data BLOB)"));
        assert!(text.contains("table Layer: 4 rows before, 5 after"));
        assert!(
            text.contains("\"Back\": LayerComposite: 0 before, 2 after"),
            "{text}"
        );
        assert!(text.contains("\"Back\": LayerVisibility: 1 before, 0 after"));
        assert!(text.contains("table Layer: only after: row "), "{text}");
        assert!(text.contains("    LayerName = \"New\""));
        assert!(text.trim_end().ends_with(" differences"));
        // The ids of stored bitmaps are fresh in every file: they are not
        // called a difference while the bitmaps hold the same.
        let moved = {
            let mut psd = picture();
            psd.layers[0].name = "Back again".to_owned();
            file(&psd, None)
        };
        let text = diff(("a.clip", &a), ("c.clip", &moved), Options::default()).unwrap();
        assert!(!text.contains("BlockData"), "{text}");
        assert!(text.contains("LayerName: \"Back\" before, \"Back again\" after"));

        // A bitmap that holds something else is one, with both sides shown.
        let mut bigger = picture();
        bigger.layers[2] = square("Line \u{65e5}\u{672c}", 300, 290, [0, 0, 0]);
        let c = file(&bigger, None);
        let text = diff(("a.clip", &a), ("c.clip", &c), Options::default()).unwrap();
        assert!(
            text.contains("BlockData: the stored bitmap it names differs"),
            "{text}"
        );
        assert!(
            text.contains("    before: blocks: 6, 1 with data"),
            "{text}"
        );
        assert!(
            text.contains("    after:  blocks: 6, 4 with data"),
            "{text}"
        );
        // And an attribute by the part that changed.
        assert!(text.contains("Attribute: blob, "), "{text}");
        assert!(text.contains("    before: BlockSize: "));

        // A blob Sue cannot decode: the bytes around the first difference.
        let mut first = vec![0_u8; 3000];
        let mut second = first.clone();
        (first[1000], second[1000]) = (1, 2);
        second.push(9);
        let (x, y) = (file(&before, Some(&first)), file(&before, Some(&second)));
        let text = diff(("x.clip", &x), ("y.clip", &y), Options::default()).unwrap();
        assert!(
            text.contains(
                "Data: blob, 3000 bytes before, 3001 after, first difference at byte 1000"
            ),
            "{text}"
        );
        assert!(
            text.contains("    before: 0003e0  00 00 00 00 00 00 00 00 01 00"),
            "{text}"
        );
        assert!(
            text.contains("    after:  0003e0  00 00 00 00 00 00 00 00 02 00"),
            "{text}"
        );
    }

    #[test]
    fn a_diff_sees_bytes_that_change_where_the_shape_does_not() {
        // The same square in another color: the same blocks, other pixels,
        // so other block checksums.
        let before = picture();
        let mut after = picture();
        after.layers[0] = square("Back", 0, 280, [10, 200, 10]);
        let (a, b) = (file(&before, None), file(&after, None));
        let text = diff(("a.clip", &a), ("b.clip", &b), Options::default()).unwrap();
        assert!(
            text.contains("BlockData: the stored bitmap it names differs\n"),
            "{text}"
        );
        assert!(
            text.contains("\n    before: BlockCheckSum: header 12 6 4; values "),
            "{text}"
        );
        assert!(
            text.contains("\n    after:  BlockCheckSum: header 12 6 4; values "),
            "{text}"
        );
        assert!(!text.contains("no differences"), "{text}");

        // A chunk that is neither the database nor a bitmap, changed in
        // place: named, with the bytes around the change.
        let mut c = a.clone();
        c[24 + 16 + 33] ^= 0x40;
        let text = diff(("a.clip", &a), ("c.clip", &c), Options::default()).unwrap();
        assert!(
            text.contains(
                "chunks: Head: 40 bytes before, 40 after, first difference at byte 33\n    \
                 before: 000010  "
            ),
            "{text}"
        );
        assert!(text.contains("\n    after:  000010  "), "{text}");
        assert!(text.trim_end().ends_with("\n1 differences"), "{text}");
        // With no bytes asked for, the line alone.
        let none = Options {
            bytes: 0,
            ..Options::default()
        };
        let text = diff(("a.clip", &a), ("c.clip", &c), none).unwrap();
        assert!(text.contains("chunks: Head: 40 bytes"), "{text}");
        assert!(!text.contains("000010"), "{text}");
    }

    #[test]
    fn the_words_after_clip_are_read_as_a_command() {
        let words =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
        let plain = Options::default();
        assert_eq!(
            parse(&words("dump a.clip")),
            Some((Command::Dump("a.clip"), plain))
        );
        let asked = Options {
            bytes: 0,
            nulls: true,
            inflate: false,
        };
        assert_eq!(
            parse(&words("--no-inflate diff a.clip --bytes 0 b.clip --nulls")),
            Some((Command::Diff("a.clip", "b.clip"), asked))
        );
        for help in ["--help", "-h", "dump --help", "diff a b -h"] {
            assert_eq!(parse(&words(help)).map(|(c, _)| c), Some(Command::Help));
        }
        for wrong in [
            "",
            "dump",
            "dump a b",
            "diff a",
            "diff a b c",
            "list a",
            "dump a --bytes",
            "dump a --bytes many",
            "dump a --bytes -1",
            "dump a --loud",
        ] {
            assert_eq!(parse(&words(wrong)), None, "{wrong:?}");
        }
    }

    #[test]
    fn broken_and_foreign_files_are_listed_or_refused_never_a_crash() {
        assert!(dump("x", b"8BPS not a clip", Options::default()).is_err());
        assert!(dump("x", b"CSFCHUNK", Options::default()).is_err());
        assert!(diff(("a", b"x"), ("b", b"y"), Options::default()).is_err());
        let bytes = file(&picture(), Some(&[1; 9000]));
        // Cut at many places: whatever is whole is still listed.
        let options = Options {
            bytes: 16,
            ..Options::default()
        };
        for cut in (24..bytes.len()).step_by(bytes.len() / 150) {
            let text = dump("cut.clip", &bytes[..cut], options).unwrap();
            assert!(text.contains("but the file has"), "cut at {cut}");
            let both = diff(("whole", &bytes), ("cut", &bytes[..cut]), options).unwrap();
            assert!(both.trim_end().ends_with(" differences"), "cut at {cut}");
        }
        // Bytes changed all over the database: still a listing.
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        for _ in 0..40 {
            let mut noisy = bytes.clone();
            for _ in 0..12 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let at = bytes.len() - 1 - (seed % 40_000) as usize;
                noisy[at] ^= (seed >> 32) as u8 | 1;
            }
            let _ = dump("noisy.clip", &noisy, options).unwrap();
            let _ = diff(("whole", &bytes), ("noisy", &noisy), options).unwrap();
        }
        // An unknown chunk kind and bytes after the last chunk.
        let mut odd = bytes.clone();
        let foot = odd.len() - 16;
        odd[foot + 4..foot + 8].copy_from_slice(b"Wxyz");
        odd.extend_from_slice(b"tail");
        let text = dump("odd.clip", &odd, options).unwrap();
        assert!(text.contains(". Wxyz at byte "));
        assert!(text.contains("after the last chunk: 4 bytes that are no chunk"));
        let text = diff(("whole", &bytes), ("odd", &odd), options).unwrap();
        assert!(text.contains("chunks: Foot: 1 before, 0 after"));
        assert!(text.contains("chunks: Wxyz: 0 before, 1 after"));
        assert!(text.contains("chunks: bytes after the last chunk: 0 before, 4 after"));
    }

    #[test]
    fn number_lists_fold_their_repeats() {
        assert_eq!(runs::<u32>(&[]), "");
        assert_eq!(runs(&[1, 1, 1, 0, 7, 7, 2]), "1 (x3) 0 7 7 2");
        let many: Vec<u32> = (0..100).collect();
        assert!(runs(&many).ends_with("47 ... and 52 more"));
        let mut text = String::new();
        hex_lines(&mut text, "> ", 16, b"Sue\x00\xff");
        assert_eq!(
            text,
            format!("> 000010  {:<47}  |Sue..|\n", "53 75 65 00 ff")
        );
    }
}
