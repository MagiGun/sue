//! The Table menu: tables inserted, grown, shrunk and converted.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "table")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("table.insert", "insert-table-ellipsis").keys("Ctrl+F12"),
    // `table.insert.3x4`: three rows of four columns at once, as a size
    // picker or a script asks.
    c("table.insert.", "insert-table-ellipsis")
        .with(Flags::FAMILY)
        .with(Flags::HIDDEN),
    c("table.row_above", "insert-rows-above"),
    c("table.row_below", "insert-rows-below"),
    c("table.column_left", "insert-columns-before"),
    c("table.column_right", "insert-columns-after"),
    c("table.delete_row", "delete-rows"),
    c("table.delete_column", "delete-columns"),
    c("table.delete", "delete-table"),
    c("table.header", "heading-row"),
    c("table.from_text", "text-to-table"),
    c("table.to_text", "table-to-text"),
];
