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
}
