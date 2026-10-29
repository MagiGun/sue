//! Tables in running text. A table is an entity of its own holding the
//! shared [`sue_model::Table`] (class [`sue_model::TEXT_TABLE`]), set in
//! the text where an `embed` mark names it, on an object character of its
//! own paragraph, as pictures are. Its cells hold plain text.
//!
//! Here it is laid out (columns by their shares of the line, rows as tall
//! as their tallest cell, breaking across pages between rows with the
//! heading rows repeated), painted, and edited: a caret in one cell types
//! there in place, recorded as one undo step per burst of typing.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, CharIndex, LayoutJob};
use egui::{Color32, Galley, Painter, Pos2, Rect, Stroke, Vec2, pos2};
use sue_core::{Arena, EntityId, Value};
use sue_model::{Cell, SetCell, Table};
use sue_wm::{AppletContext, ArenaEdit};

use crate::layout::{Setting, format};
use crate::style::{CharFormat, ParagraphLook};

/// The most line spacing a cell's text gets (a double-spaced manuscript
/// keeps its tables compact).
pub const CELL_SPACING: f32 = 1.15;

/// The room a table keeps above and below it, in points.
pub const TABLE_SPACE: f32 = 6.0;

/// What a table shows: every cell's text, its heading rows and its
/// column shares.
#[derive(Clone, Debug, PartialEq)]
pub struct TableSource {
    /// Cell texts, row by row.
    pub cells: Vec<Vec<String>>,
    /// Rows at the top that are headings.
    pub header_rows: usize,
    /// Each column's share of the width, adding up to one.
    pub shares: Vec<f32>,
}

impl TableSource {
    /// Reads table entity `id` (`None` if it holds no table).
    #[must_use]
    pub fn read(arena: &Arena, id: EntityId) -> Option<Self> {
        let entity = arena.get_by_id(id).ok()?;
        let table = entity.ext::<Table>()?;
        let cells = table.texts();
        let columns = cells.first().map_or(0, Vec::len);
        Some(Self {
            cells,
            header_rows: header_rows(entity.property(sue_model::HEADER_ROWS)),
            shares: sue_model::column_shares(entity.property(sue_model::WIDTHS), columns),
        })
    }
}

fn header_rows(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_i64)
        .map_or(0, |rows| rows.clamp(0, 64) as usize)
}

/// A table, laid out. Lengths are in pixels from its top left.
#[derive(Clone, Debug)]
pub struct TableBlock {
    /// The table's entity.
    pub entity: EntityId,
    /// Column edges across, one more than there are columns.
    pub edges: Vec<f32>,
    /// Each row's top and bottom.
    pub rows: Vec<(f32, f32)>,
    /// Each cell's text, shaped, row by row.
    pub cells: Vec<Vec<Arc<Galley>>>,
    /// Each cell's text, row by row.
    pub texts: Vec<Vec<String>>,
    /// Rows at the top that are headings.
    pub header_rows: usize,
    /// The room between a cell's edge and its text.
    pub pad: f32,
    /// A table of contents' entries, when it is one (no rules, no cells to
    /// type in).
    pub listing: Option<crate::contents::Listing>,
}

impl TableBlock {
    /// The number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.edges.len().saturating_sub(1)
    }

    /// The height of the heading rows repeated on later pages (0 when
    /// there are none, or when the table is all headings).
    #[must_use]
    pub fn repeat_height(&self) -> f32 {
        let rows = self.header_rows;
        if rows == 0 || rows >= self.rows.len() {
            0.0
        } else {
            self.rows[rows - 1].1 - self.rows[0].0
        }
    }

    /// Cell `(row, column)`'s rectangle.
    #[must_use]
    pub fn cell_rect(&self, row: usize, column: usize) -> Rect {
        let (top, bottom) = self.rows[row];
        Rect::from_min_max(
            pos2(self.edges[column], top),
            pos2(self.edges[column + 1], bottom),
        )
    }

    /// The cell under `local` and the byte offset in its text nearest to
    /// it, if the point is inside the table across.
    #[must_use]
    pub fn hit(&self, local: Vec2) -> Option<(usize, usize, usize)> {
        if self.rows.is_empty() || self.columns() == 0 {
            return None;
        }
        if local.x < self.edges[0] || local.x > *self.edges.last()? {
            return None;
        }
        let row = self
            .rows
            .iter()
            .position(|(_, bottom)| local.y < *bottom)
            .unwrap_or(self.rows.len() - 1);
        let column = (0..self.columns())
            .find(|column| local.x < self.edges[column + 1])
            .unwrap_or(self.columns() - 1);
        let inside = local - self.text_at(row, column).to_vec2();
        let cursor = self.cells[row][column].cursor_from_pos(inside);
        let text = &self.texts[row][column];
        Some((row, column, byte_of(text, cursor.index.0)))
    }

    /// Where cell `(row, column)`'s text starts.
    #[must_use]
    pub fn text_at(&self, row: usize, column: usize) -> Pos2 {
        pos2(self.edges[column] + self.pad, self.rows[row].0 + self.pad)
    }

    /// The caret's rectangle before byte `offset` of a cell's text.
    #[must_use]
    pub fn caret(&self, row: usize, column: usize, offset: usize) -> Option<Rect> {
        let galley = self.cells.get(row)?.get(column)?;
        let text = &self.texts[row][column];
        let index = text[..offset.min(text.len())].chars().count();
        let rect = galley.pos_from_cursor(CCursor {
            index: CharIndex(index),
            prefer_next_row: true,
        });
        let rect = rect.translate(self.text_at(row, column).to_vec2());
        Some(Rect::from_min_max(
            rect.min,
            pos2(rect.min.x + 1.0, rect.max.y.max(rect.min.y + 4.0)),
        ))
    }
}

fn byte_of(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(at, _)| at)
}

/// Lays out table `entity` across `width` pixels with a paragraph's look.
#[must_use]
pub fn lay_table(
    ctx: &egui::Context,
    entity: EntityId,
    source: &TableSource,
    look: &ParagraphLook,
    setting: &Setting<'_>,
    width: f32,
) -> TableBlock {
    let pad = 4.0 * setting.scale;
    let columns = source.shares.len();
    let mut edges = Vec::with_capacity(columns + 1);
    let mut x = 0.0;
    edges.push(x);
    for share in &source.shares {
        x += share * width;
        edges.push(x);
    }
    let plain = format(ctx, look, &CharFormat::default(), setting, None);
    let natural = ctx.fonts_mut(|fonts| fonts.row_height(&plain.font_id));
    let mut rows = Vec::with_capacity(source.cells.len());
    let mut cells = Vec::with_capacity(source.cells.len());
    let mut y = 0.0;
    for (index, texts) in source.cells.iter().enumerate() {
        let run = CharFormat {
            bold: index < source.header_rows,
            ..CharFormat::default()
        };
        let line = Some(natural * look.line_spacing.min(CELL_SPACING));
        let mut row = Vec::with_capacity(columns);
        let mut tallest = natural;
        for (column, text) in texts.iter().enumerate().take(columns) {
            let mut job =
                LayoutJob::single_section(text.clone(), format(ctx, look, &run, setting, line));
            job.wrap.max_width = (edges[column + 1] - edges[column] - pad * 2.0).max(8.0);
            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            tallest = tallest.max(galley.size().y);
            row.push(galley);
        }
        let height = tallest + pad * 2.0;
        rows.push((y, y + height));
        y += height;
        cells.push(row);
    }
    TableBlock {
        entity,
        edges,
        rows,
        cells,
        texts: source.cells.clone(),
        header_rows: source.header_rows,
        pad,
        listing: None,
    }
}

/// Paints `rows` of `table` with its top left at `origin`, and, above
/// `top` (the first painted row's top on screen), the heading rows when
/// they repeat on this page.
pub fn paint_table(
    painter: &Painter,
    table: &TableBlock,
    rows: Range<usize>,
    origin: Pos2,
    text: Color32,
    shade: Color32,
) {
    let line = Stroke::new(1.0, text.gamma_multiply(0.55));
    let paint_row = |row: usize, at: Pos2| {
        let (top, bottom) = table.rows[row];
        let height = bottom - top;
        let left = at.x + table.edges[0];
        let right = at.x + table.edges.last().copied().unwrap_or(0.0);
        if row < table.header_rows {
            painter.rect_filled(
                Rect::from_min_max(pos2(left, at.y), pos2(right, at.y + height)),
                0.0,
                shade,
            );
        }
        for (column, galley) in table.cells[row].iter().enumerate() {
            let cell = pos2(at.x + table.edges[column] + table.pad, at.y + table.pad);
            painter.galley(cell, galley.clone(), text);
        }
        painter.hline(left..=right, at.y, line);
        painter.hline(left..=right, at.y + height, line);
        for edge in &table.edges {
            painter.vline(at.x + edge, at.y..=at.y + height, line);
        }
    };
    let repeat = table.repeat_height();
    if repeat > 0.0 && rows.start >= table.header_rows {
        let first = table.rows[rows.start].0;
        let mut y = origin.y + first - repeat;
        for row in 0..table.header_rows {
            paint_row(row, pos2(origin.x, y));
            y += table.rows[row].1 - table.rows[row].0;
        }
    }
    for row in rows {
        paint_row(row, pos2(origin.x, origin.y + table.rows[row].0));
    }
}

/// A change to the table the caret is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEdit {
    /// A new row above the caret's.
    RowAbove,
    /// A new row below the caret's.
    RowBelow,
    /// A new column left of the caret's.
    ColumnLeft,
    /// A new column right of the caret's.
    ColumnRight,
    /// Delete the caret's row (the table, if it is the only one).
    DeleteRow,
    /// Delete the caret's column (the table, if it is the only one).
    DeleteColumn,
    /// Delete the whole table.
    DeleteTable,
    /// Turn the heading row on or off.
    HeaderRow,
}

/// A caret in a table cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellCaret {
    /// The shown document the table is in.
    pub doc: usize,
    /// The table's entity.
    pub table: EntityId,
    /// The row.
    pub row: usize,
    /// The column.
    pub column: usize,
    /// The byte offset in the cell's text.
    pub offset: usize,
}

impl CellCaret {
    /// A caret at byte `offset` of cell `(row, column)`.
    #[must_use]
    pub fn new(doc: usize, table: EntityId, row: usize, column: usize, offset: usize) -> Self {
        Self {
            doc,
            table,
            row,
            column,
            offset,
        }
    }

    /// The cell's text now.
    #[must_use]
    pub fn text(&self, arena: &Arena) -> String {
        arena
            .get_by_id(self.table)
            .ok()
            .and_then(|entity| entity.ext::<Table>())
            .map(|table| table.text(self.row as u32, self.column as u32))
            .unwrap_or_default()
    }

    /// Replaces `range` of the cell's text with `text`, in place, recorded
    /// with the typing (a burst in one cell is one undo step). Returns
    /// `false` if nothing changed.
    pub fn replace(&mut self, cx: &mut AppletContext<'_>, range: Range<usize>, text: &str) -> bool {
        let (row, column) = (self.row as u32, self.column as u32);
        let Ok(entity) = cx.world.arena.get_by_id_mut(self.table) else {
            return false;
        };
        let Some(table) = entity.ext_mut::<Table>() else {
            return false;
        };
        let before = table.get(row, column).cloned();
        let mut now = table.text(row, column);
        let start = range.start.min(now.len());
        let end = range.end.clamp(start, now.len());
        if (start == end && text.is_empty())
            || !now.is_char_boundary(start)
            || !now.is_char_boundary(end)
        {
            return false;
        }
        now.replace_range(start..end, text);
        let formula = before.as_ref().and_then(|cell| cell.formula.clone());
        table.set(
            row,
            column,
            Some(Cell {
                value: Value::Text(now),
                formula,
            }),
        );
        self.offset = start + text.len();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (self.table, row, column).hash(&mut hash);
            hash.finish()
        };
        cx.record_held_typing(
            key,
            ArenaEdit(SetCell::new(self.table, row, column, before)),
        );
        true
    }

    /// Ends the burst of typing, as one undo step.
    pub fn commit(&mut self, cx: &mut AppletContext<'_>) {
        cx.typing.flush(cx.history);
    }

    /// Moves to another cell (committing this one's typing first), with
    /// the caret at its start or end.
    pub fn move_to(&mut self, cx: &mut AppletContext<'_>, row: usize, column: usize, end: bool) {
        self.commit(cx);
        self.row = row;
        self.column = column;
        self.offset = if end { self.text(cx.arena()).len() } else { 0 };
    }
}

/// The size of table `id` in rows and columns.
#[must_use]
pub fn size_of(arena: &Arena, id: EntityId) -> (usize, usize) {
    arena
        .get_by_id(id)
        .ok()
        .and_then(|entity| entity.ext::<Table>())
        .map_or((0, 0), |table| {
            let (rows, columns) = table.size();
            (rows as usize, columns as usize)
        })
}

/// A new table entity of `rows` by `columns` empty cells, with
/// `header_rows` heading rows.
#[must_use]
pub fn new_table(rows: u32, columns: u32, header_rows: u32) -> sue_core::Entity {
    let entity = sue_core::Entity::empty("Table", sue_model::TEXT_TABLE)
        .with_ext(Table::grid(rows, columns));
    if header_rows > 0 {
        entity.with_property(sue_model::HEADER_ROWS, Value::Int(i64::from(header_rows)))
    } else {
        entity
    }
}

/// A table entity holding `cells`, row by row (short rows padded).
#[must_use]
pub fn table_of(cells: &[Vec<String>], header_rows: u32) -> sue_core::Entity {
    let columns = cells.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
    let mut entity = new_table(cells.len().max(1) as u32, columns, header_rows);
    if let Some(table) = entity.ext_mut::<Table>() {
        for (row, texts) in cells.iter().enumerate() {
            for (column, text) in texts.iter().enumerate() {
                table.set(row as u32, column as u32, Some(Cell::value(text.as_str())));
            }
        }
    }
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_read_their_cells_and_shares() {
        let mut arena = Arena::new();
        let cells = vec![
            vec!["Name".to_owned(), "Qty".to_owned()],
            vec!["Bolt".to_owned()],
        ];
        let handle = arena.insert(table_of(&cells, 1));
        let id = arena.get(handle).unwrap().id();
        let source = TableSource::read(&arena, id).unwrap();
        assert_eq!(source.cells, vec![vec!["Name", "Qty"], vec!["Bolt", ""]]);
        assert_eq!(source.header_rows, 1);
        assert_eq!(source.shares, vec![0.5, 0.5]);
        assert_eq!(size_of(&arena, id), (2, 2));
    }
}
