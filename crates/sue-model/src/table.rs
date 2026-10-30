//! Tables. Spreadsheets, databases (Notion-style), CSV files, BIM schedules,
//! bills of materials, cue lists and subtitle lists are all a grid of
//! [`Value`]s with optional typed columns. [`Table`] stores cells sparsely,
//! so a sheet with a cell at row one million costs one cell. Formulas are
//! kept as text next to their last computed value, so files open instantly
//! and any feature can read the numbers without a formula engine.

use std::collections::BTreeMap;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{Arena, ComponentType, CoreError, EntityId, ReversibleTransaction, TxnError, Value};

/// A column's name and the kind of values it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    /// Header text.
    pub name: String,
    /// `any`, `text`, `number`, `bool`, `date`, `choice`, `entity` or a
    /// feature's own kind.
    pub kind: String,
}

/// One cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// The value (for a formula, its last result).
    pub value: Value,
    /// The formula source, e.g. `=SUM(A1:A9)`.
    pub formula: Option<String>,
}

impl Cell {
    /// A plain value.
    pub fn value(value: impl Into<Value>) -> Self {
        Self {
            value: value.into(),
            formula: None,
        }
    }
}

/// A sparse grid of cells with optional column definitions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    columns: Vec<Column>,
    cells: BTreeMap<(u32, u32), Cell>,
}

/// `A`, `B`, ..., `Z`, `AA`, ... for a zero-based column.
#[must_use]
pub fn column_name(mut column: u32) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (column % 26) as u8);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}

/// Parses an `A1` reference into a zero-based `(row, column)`.
#[must_use]
pub fn parse_a1(reference: &str) -> Option<(u32, u32)> {
    let reference = reference.trim().trim_start_matches('$');
    let split = reference.find(|ch: char| !ch.is_ascii_alphabetic())?;
    let (letters, digits) = reference.split_at(split);
    if letters.is_empty() {
        return None;
    }
    let mut column: u64 = 0;
    for letter in letters.bytes() {
        column = column * 26 + u64::from(letter.to_ascii_uppercase() - b'A' + 1);
        if column > u64::from(u32::MAX) {
            return None;
        }
    }
    let row: u32 = digits.trim_start_matches('$').parse().ok()?;
    Some((row.checked_sub(1)?, u32::try_from(column - 1).ok()?))
}

impl Table {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A table from rows of values (row 0 first).
    #[must_use]
    pub fn from_rows(rows: Vec<Vec<Value>>) -> Self {
        let mut table = Self::new();
        for (row, values) in rows.into_iter().enumerate() {
            for (column, value) in values.into_iter().enumerate() {
                table.set(row as u32, column as u32, Some(Cell::value(value)));
            }
        }
        table
    }

    /// The column definitions.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Sets the column definitions.
    pub fn set_columns(&mut self, columns: Vec<Column>) -> Vec<Column> {
        std::mem::replace(&mut self.columns, columns)
    }

    /// The cell at `row`, `column`.
    #[must_use]
    pub fn get(&self, row: u32, column: u32) -> Option<&Cell> {
        self.cells.get(&(row, column))
    }

    /// The value at `row`, `column`.
    #[must_use]
    pub fn value(&self, row: u32, column: u32) -> Option<&Value> {
        self.get(row, column).map(|cell| &cell.value)
    }

    /// Sets (or with `None` clears) a cell, returning the previous one.
    pub fn set(&mut self, row: u32, column: u32, cell: Option<Cell>) -> Option<Cell> {
        match cell {
            Some(cell) => self.cells.insert((row, column), cell),
            None => self.cells.remove(&(row, column)),
        }
    }

    /// Number of non-empty cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// `true` if no cell is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Rows and columns up to the last used cell (or column definition).
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        let rows = self.cells.keys().map(|(row, _)| row + 1).max().unwrap_or(0);
        let columns = self
            .cells
            .keys()
            .map(|(_, column)| column + 1)
            .max()
            .unwrap_or(0)
            .max(self.columns.len() as u32);
        (rows, columns)
    }

    /// The cells of one row, by column.
    pub fn row(&self, row: u32) -> impl Iterator<Item = (u32, &Cell)> {
        self.cells
            .range((row, 0)..=(row, u32::MAX))
            .map(|((_, column), cell)| (*column, cell))
    }

    /// Every cell with its position, row by row.
    pub fn iter(&self) -> impl Iterator<Item = ((u32, u32), &Cell)> {
        self.cells.iter().map(|(position, cell)| (*position, cell))
    }

    /// Inserts `count` empty rows before `at`, moving later rows down.
    pub fn insert_rows(&mut self, at: u32, count: u32) {
        let moved: Vec<_> = self
            .cells
            .keys()
            .filter(|(row, _)| *row >= at)
            .copied()
            .collect();
        let mut taken: Vec<_> = moved
            .into_iter()
            .filter_map(|key| self.cells.remove(&key).map(|cell| (key, cell)))
            .collect();
        for ((row, column), cell) in taken.drain(..) {
            self.cells.insert((row + count, column), cell);
        }
    }

    /// Removes `count` rows from `at`, moving later rows up. Returns the
    /// removed cells (relative to `at`), for undo.
    pub fn remove_rows(&mut self, at: u32, count: u32) -> Vec<((u32, u32), Cell)> {
        let keys: Vec<_> = self
            .cells
            .keys()
            .filter(|(row, _)| *row >= at)
            .copied()
            .collect();
        let mut removed = Vec::new();
        let mut shifted = Vec::new();
        for (row, column) in keys {
            let Some(cell) = self.cells.remove(&(row, column)) else {
                continue;
            };
            if row < at + count {
                removed.push(((row - at, column), cell));
            } else {
                shifted.push(((row - count, column), cell));
            }
        }
        self.cells.extend(shifted);
        removed
    }

    /// Inserts `count` empty columns before `at`, moving later columns
    /// right (and their definitions with them, if the table has any).
    pub fn insert_columns(&mut self, at: u32, count: u32) {
        self.shift_columns(at, count);
        let at = (at as usize).min(self.columns.len());
        if !self.columns.is_empty() {
            for _ in 0..count {
                self.columns.insert(at, Column::default());
            }
        }
    }

    /// Moves every cell from column `at` on `count` columns right.
    fn shift_columns(&mut self, at: u32, count: u32) {
        let moved: Vec<_> = self
            .cells
            .keys()
            .filter(|(_, column)| *column >= at)
            .copied()
            .collect();
        let taken: Vec<_> = moved
            .into_iter()
            .filter_map(|key| self.cells.remove(&key).map(|cell| (key, cell)))
            .collect();
        for ((row, column), cell) in taken {
            self.cells.insert((row, column + count), cell);
        }
    }

    /// Removes `count` columns from `at`, moving later columns left.
    /// Returns the removed cells (relative to `at`) and definitions, for
    /// undo.
    pub fn remove_columns(&mut self, at: u32, count: u32) -> (Vec<Placed>, Vec<Column>) {
        let keys: Vec<_> = self
            .cells
            .keys()
            .filter(|(_, column)| *column >= at)
            .copied()
            .collect();
        let mut removed = Vec::new();
        let mut shifted = Vec::new();
        for (row, column) in keys {
            let Some(cell) = self.cells.remove(&(row, column)) else {
                continue;
            };
            if column < at + count {
                removed.push(((row, column - at), cell));
            } else {
                shifted.push(((row, column - count), cell));
            }
        }
        self.cells.extend(shifted);
        let start = (at as usize).min(self.columns.len());
        let end = ((at + count) as usize).min(self.columns.len());
        let columns = self.columns.drain(start..end).collect();
        (removed, columns)
    }

    /// A table of `rows` by `columns` empty text cells, as a word
    /// processor inserts one: every cell exists, so its size is its shape.
    #[must_use]
    pub fn grid(rows: u32, columns: u32) -> Self {
        let mut table = Self::new();
        table.columns = (0..columns)
            .map(|_| Column {
                name: String::new(),
                kind: "text".to_owned(),
            })
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                table.set(row, column, Some(Cell::value("")));
            }
        }
        table
    }

    /// The text a cell shows: its text, or its value written out.
    #[must_use]
    pub fn text(&self, row: u32, column: u32) -> String {
        match self.value(row, column) {
            None => String::new(),
            Some(Value::Text(text)) => text.clone(),
            Some(Value::Int(number)) => number.to_string(),
            Some(Value::Float(number)) => number.to_string(),
            Some(Value::Bool(yes)) => yes.to_string(),
            Some(other) => format!("{other:?}"),
        }
    }

    /// Every cell's text, row by row, `rows` by `columns` of [`Self::size`].
    #[must_use]
    pub fn texts(&self) -> Vec<Vec<String>> {
        let (rows, columns) = self.size();
        (0..rows)
            .map(|row| (0..columns).map(|column| self.text(row, column)).collect())
            .collect()
    }
}

/// A cell and its place, row and column.
pub type Placed = ((u32, u32), Cell);

/// The class of a table set in running text: a word processor's table,
/// shown where an `embed` mark ([`crate::mark_kind::EMBED`]) names it.
pub const TEXT_TABLE: &str = "sue.text_table";
/// How many rows at the top of a table are its headings, repeated at the
/// top of each page it runs onto (`Int`, a property of its entity).
pub const HEADER_ROWS: &str = "sue.table.header_rows";
/// A table's column widths as shares of its width (`List` of `Float`, a
/// property of its entity); equal columns without it.
pub const WIDTHS: &str = "sue.table.widths";

/// A table entity for running text holding `cells` (row by row, short
/// rows padded with empty cells), its first `header_rows` rows headings,
/// with column widths `widths` (any unit, kept as shares) when known.
#[must_use]
pub fn text_table(cells: &[Vec<String>], header_rows: u32, widths: &[f32]) -> sue_core::Entity {
    let columns = cells.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
    let mut table = Table::grid(cells.len().max(1) as u32, columns);
    for (row, texts) in cells.iter().enumerate() {
        for (column, text) in texts.iter().enumerate() {
            table.set(row as u32, column as u32, Some(Cell::value(text.as_str())));
        }
    }
    let mut entity = sue_core::Entity::empty("Table", TEXT_TABLE).with_ext(table);
    if header_rows > 0 {
        entity = entity.with_property(HEADER_ROWS, Value::Int(i64::from(header_rows)));
    }
    if widths.len() == columns as usize && widths.iter().all(|width| *width > 0.0) {
        let list = widths
            .iter()
            .map(|width| Value::Float(f64::from(*width)))
            .collect();
        entity = entity.with_property(WIDTHS, Value::List(list));
    }
    entity
}

/// The shares of a table's width its `columns` take, from its [`WIDTHS`]
/// property (the others' average where it says nothing), adding up to one.
#[must_use]
pub fn column_shares(widths: Option<&Value>, columns: usize) -> Vec<f32> {
    let mut shares: Vec<f32> = match widths {
        Some(Value::List(list)) => list
            .iter()
            .map(|value| value.as_f64().unwrap_or(0.0) as f32)
            .collect(),
        _ => Vec::new(),
    };
    shares.truncate(columns);
    let known: Vec<f32> = shares
        .iter()
        .copied()
        .filter(|share| share.is_finite() && *share > 0.0)
        .collect();
    // A column it says nothing of gets the others' average.
    let fallback = if known.is_empty() {
        1.0
    } else {
        known.iter().sum::<f32>() / known.len() as f32
    };
    shares.resize(columns, 0.0);
    for share in &mut shares {
        if !share.is_finite() || *share <= 0.0 {
            *share = fallback;
        }
    }
    let total: f32 = shares.iter().sum();
    shares.iter().map(|share| share / total.max(1e-6)).collect()
}

/// Which way a [`Reshape`] works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Rows.
    Rows,
    /// Columns.
    Columns,
}

/// Inserts or removes rows or columns of a table entity, as one undo step.
/// Inserted cells get `fill` across the table (an empty text, so that a
/// word processor's new row exists); removed cells come back on undo.
#[derive(Debug)]
pub struct Reshape {
    id: EntityId,
    axis: Axis,
    at: u32,
    count: u32,
    insert: bool,
    fill: Option<Value>,
    removed: Vec<Placed>,
    definitions: Vec<Column>,
}

impl Reshape {
    /// Inserts `count` rows before `at`, their cells filled with `fill`.
    #[must_use]
    pub fn insert_rows(id: EntityId, at: u32, count: u32, fill: Option<Value>) -> Self {
        Self::new(id, Axis::Rows, at, count, true, fill)
    }

    /// Removes `count` rows from `at`.
    #[must_use]
    pub fn remove_rows(id: EntityId, at: u32, count: u32) -> Self {
        Self::new(id, Axis::Rows, at, count, false, None)
    }

    /// Inserts `count` columns before `at`, their cells filled with `fill`.
    #[must_use]
    pub fn insert_columns(id: EntityId, at: u32, count: u32, fill: Option<Value>) -> Self {
        Self::new(id, Axis::Columns, at, count, true, fill)
    }

    /// Removes `count` columns from `at`.
    #[must_use]
    pub fn remove_columns(id: EntityId, at: u32, count: u32) -> Self {
        Self::new(id, Axis::Columns, at, count, false, None)
    }

    fn new(
        id: EntityId,
        axis: Axis,
        at: u32,
        count: u32,
        insert: bool,
        fill: Option<Value>,
    ) -> Self {
        Self {
            id,
            axis,
            at,
            count,
            insert,
            fill,
            removed: Vec::new(),
            definitions: Vec::new(),
        }
    }

    fn table<'a>(&self, arena: &'a mut Arena) -> Result<&'a mut Table, TxnError> {
        arena
            .get_by_id_mut(self.id)?
            .ext_mut::<Table>()
            .ok_or_else(|| TxnError::from(CoreError::UnknownId(self.id)))
    }

    fn grow(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let (axis, at, count) = (self.axis, self.at, self.count);
        let fill = self.fill.clone();
        let table = self.table(arena)?;
        let (rows, columns) = table.size();
        match axis {
            Axis::Rows => table.insert_rows(at, count),
            Axis::Columns => table.insert_columns(at, count),
        }
        if let Some(fill) = fill {
            let across = match axis {
                Axis::Rows => columns,
                Axis::Columns => rows,
            };
            for index in at..at + count {
                for other in 0..across {
                    let (row, column) = match axis {
                        Axis::Rows => (index, other),
                        Axis::Columns => (other, index),
                    };
                    table.set(row, column, Some(Cell::value(fill.clone())));
                }
            }
        }
        Ok(())
    }

    fn shrink(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let (axis, at, count) = (self.axis, self.at, self.count);
        let table = self.table(arena)?;
        let (removed, definitions) = match axis {
            Axis::Rows => (table.remove_rows(at, count), Vec::new()),
            Axis::Columns => table.remove_columns(at, count),
        };
        self.removed = removed;
        self.definitions = definitions;
        Ok(())
    }

    fn put_back(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let (axis, at, count) = (self.axis, self.at, self.count);
        let removed = std::mem::take(&mut self.removed);
        let definitions = std::mem::take(&mut self.definitions);
        let table = self.table(arena)?;
        match axis {
            Axis::Rows => table.insert_rows(at, count),
            Axis::Columns => {
                table.shift_columns(at, count);
                let start = (at as usize).min(table.columns.len());
                for (offset, column) in definitions.into_iter().enumerate() {
                    table.columns.insert(start + offset, column);
                }
            }
        }
        for ((row, column), cell) in removed {
            let key = match axis {
                Axis::Rows => (row + at, column),
                Axis::Columns => (row, column + at),
            };
            table.cells.insert(key, cell);
        }
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for Reshape {
    fn label(&self) -> String {
        let what = match (self.axis, self.count == 1) {
            (Axis::Rows, true) => "Row",
            (Axis::Rows, false) => "Rows",
            (Axis::Columns, true) => "Column",
            (Axis::Columns, false) => "Columns",
        };
        if self.insert {
            format!("Insert {what}")
        } else {
            format!("Delete {what}")
        }
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        if self.insert {
            self.grow(arena)
        } else {
            self.shrink(arena)
        }
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        if self.insert {
            // Later edits were undone first, so what is there is what was
            // inserted: it goes again.
            self.shrink(arena)?;
            self.removed.clear();
            self.definitions.clear();
            Ok(())
        } else {
            self.put_back(arena)
        }
    }
}

impl ComponentType for Table {
    const NAME: &'static str = "sue.table";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.len(self.columns.len());
        for column in &self.columns {
            out.str(&column.name);
            out.str(&column.kind);
        }
        out.len(self.cells.len());
        for ((row, column), cell) in &self.cells {
            out.varint(u64::from(*row));
            out.varint(u64::from(*column));
            cell.value.encode(out);
            out.str(cell.formula.as_deref().unwrap_or(""));
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut table = Self::new();
        for _ in 0..input.len(2)? {
            table.columns.push(Column {
                name: input.str()?,
                kind: input.str()?,
            });
        }
        for _ in 0..input.len(4)? {
            let row = u32::try_from(input.varint()?).map_err(|_| input.error("row"))?;
            let column = u32::try_from(input.varint()?).map_err(|_| input.error("column"))?;
            let value = Value::decode(input)?;
            let formula = Some(input.str()?).filter(|formula| !formula.is_empty());
            table.cells.insert((row, column), Cell { value, formula });
        }
        Ok(table)
    }

    fn heap_bytes(&self) -> usize {
        self.cells.len() * size_of::<Cell>()
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        for cell in self.cells.values_mut() {
            cell.value.remap_entities(map);
        }
    }
}

/// Sets one cell of an entity's [`Table`], undoably.
#[derive(Debug)]
pub struct SetCell {
    id: EntityId,
    row: u32,
    column: u32,
    cell: Option<Cell>,
}

impl SetCell {
    /// Sets (or with `None` clears) the cell at `row`, `column` of `id`.
    #[must_use]
    pub fn new(id: EntityId, row: u32, column: u32, cell: Option<Cell>) -> Self {
        Self {
            id,
            row,
            column,
            cell,
        }
    }

    fn swap(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let table = arena
            .get_by_id_mut(self.id)?
            .ext_mut::<Table>()
            .ok_or_else(|| TxnError::from(CoreError::UnknownId(self.id)))?;
        self.cell = table.set(self.row, self.column, self.cell.take());
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for SetCell {
    fn label(&self) -> String {
        format!("Edit {}{}", column_name(self.column), self.row + 1)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Entity, UndoStack};

    #[test]
    fn a1_references_convert_both_ways() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        for column in [0, 1, 25, 26, 27, 700, 701, 702, 16_383] {
            assert_eq!(
                parse_a1(&format!("{}7", column_name(column))),
                Some((6, column))
            );
        }
        assert_eq!(parse_a1("$B$12"), Some((11, 1)));
        assert_eq!(parse_a1("A0"), None);
        assert_eq!(parse_a1("12"), None);
    }

    #[test]
    fn sparse_cells_rows_and_undo() {
        let mut table = Table::from_rows(vec![
            vec!["Item".into(), "Qty".into()],
            vec!["Bolt M8".into(), 40.into()],
            vec!["Duct 200mm".into(), 3.into()],
        ]);
        table.set(999_999, 0, Some(Cell::value("far away")));
        assert_eq!(table.size(), (1_000_000, 2));
        assert_eq!(table.len(), 7);
        table.insert_rows(1, 2);
        assert_eq!(table.value(3, 0).and_then(Value::as_str), Some("Bolt M8"));
        let removed = table.remove_rows(1, 2);
        assert!(removed.is_empty());
        assert_eq!(table.value(1, 1), Some(&Value::Int(40)));
        assert_eq!(table.row(2).count(), 2);

        let mut arena = Arena::new();
        let handle = arena.insert(Entity::empty("Parts", "sue.sheet").with_ext(table));
        let id = arena.get(handle).unwrap().id();
        let mut history = UndoStack::new();
        let formula = Cell {
            value: Value::Int(43),
            formula: Some("=SUM(B2:B3)".into()),
        };
        history
            .apply(&mut arena, SetCell::new(id, 3, 1, Some(formula.clone())))
            .unwrap();
        let read = |arena: &Arena| {
            arena
                .get(handle)
                .unwrap()
                .ext::<Table>()
                .unwrap()
                .get(3, 1)
                .cloned()
        };
        assert_eq!(read(&arena), Some(formula));
        history.undo(&mut arena).unwrap();
        assert_eq!(read(&arena), None);

        let table = arena.get(handle).unwrap().ext::<Table>().unwrap().clone();
        let mut out = Writer::new();
        ComponentType::encode(&table, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(Table::decode(1, &mut Reader::new(&bytes)).unwrap(), table);
    }
    #[test]
    fn text_tables_reshape_and_undo_exactly() {
        let mut arena = Arena::new();
        let mut grid = Table::grid(2, 2);
        grid.set(0, 0, Some(Cell::value("a")));
        grid.set(0, 1, Some(Cell::value("b")));
        grid.set(1, 0, Some(Cell::value("c")));
        grid.set(1, 1, Some(Cell::value("d")));
        let handle = arena.insert(Entity::empty("Table", TEXT_TABLE).with_ext(grid.clone()));
        let id = arena.get(handle).unwrap().id();
        let read = |arena: &Arena| arena.get(handle).unwrap().ext::<Table>().unwrap().clone();
        let mut history = UndoStack::new();
        history
            .apply(&mut arena, Reshape::insert_rows(id, 1, 1, Some("".into())))
            .unwrap();
        assert_eq!(read(&arena).size(), (3, 2));
        assert_eq!(read(&arena).text(2, 0), "c");
        assert_eq!(read(&arena).value(1, 1), Some(&Value::Text(String::new())));
        history
            .apply(
                &mut arena,
                Reshape::insert_columns(id, 0, 2, Some("".into())),
            )
            .unwrap();
        assert_eq!(read(&arena).size(), (3, 4));
        assert_eq!(read(&arena).columns().len(), 4);
        assert_eq!(read(&arena).text(0, 2), "a");
        history
            .apply(&mut arena, Reshape::remove_columns(id, 3, 1))
            .unwrap();
        history
            .apply(&mut arena, Reshape::remove_rows(id, 0, 1))
            .unwrap();
        assert_eq!(
            read(&arena).texts(),
            vec![vec!["", "", ""], vec!["", "", "c"]]
        );
        for _ in 0..4 {
            history.undo(&mut arena).unwrap();
        }
        assert_eq!(read(&arena), grid);
        for _ in 0..4 {
            history.redo(&mut arena).unwrap();
        }
        assert_eq!(read(&arena).size(), (2, 3));
        assert_eq!(column_shares(None, 4), vec![0.25; 4]);
        let widths = Value::List(vec![3.0.into(), 1.0.into()]);
        let shares = column_shares(Some(&widths), 3);
        for (share, want) in shares.iter().zip([0.5, 1.0 / 6.0, 1.0 / 3.0]) {
            assert!((share - want).abs() < 1e-5, "{shares:?}");
        }
    }
}
