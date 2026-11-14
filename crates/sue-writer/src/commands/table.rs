//! The Table menu: tables inserted, grown, shrunk and converted.

use sue_type::editor::TableEdit;
use sue_wm::{AppletContext, Call, Command, Facts, FilePick, Flags, ItemState, ShellRequest};

use crate::studio::{Dialog, SueWriter, me};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "table", run)
}

/// A command that works only now and then (see [`state`]).
const fn s(name: &'static str, label: &'static str) -> Command<SueWriter> {
    c(name, label).state(state)
}

/// The extensions of the table files Sue Writer reads and writes.
pub(crate) const TABLE_FILES: [&str; 3] = ["csv", "tsv", "tab"];

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
    // A CSV or TSV file as a table at the caret, and a table out as one.
    s("table.from_file", "table-from-file-ellipsis").with(Flags::LEAVES_CANVAS),
    s("table.export", "export-table-as-ellipsis").with(Flags::LEAVES_CANVAS),
];

/// Whether a Table menu command works now: a table file goes into text
/// that is open, and a table goes out when the caret is in one.
fn state(writer: &SueWriter, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    let on = match call.name {
        "table.from_file" => writer.editor.current().is_some(),
        "table.export" => writer.editor.cell().is_some(),
        _ => true,
    };
    if on {
        ItemState::on()
    } else {
        ItemState::off()
    }
}

/// Does what a Table menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    match command {
        "table.insert" => {
            writer.dialog = Some(Dialog::Table {
                rows: 3,
                columns: 3,
                heading: true,
            });
        }
        size if size.starts_with("table.insert.") => {
            // `table.insert.3x4`: three rows of four columns at once,
            // as a size picker asks.
            let size = &size["table.insert.".len()..];
            let Some((rows, columns)) = size
                .split_once('x')
                .and_then(|(rows, columns)| Some((rows.parse().ok()?, columns.parse().ok()?)))
            else {
                return;
            };
            writer.editor.insert_table(cx, rows, columns, 0);
            writer.editor.focus();
        }
        "table.row_above"
        | "table.row_below"
        | "table.column_left"
        | "table.column_right"
        | "table.delete_row"
        | "table.delete_column"
        | "table.delete"
        | "table.header" => {
            let edit = match command {
                "table.row_above" => TableEdit::RowAbove,
                "table.row_below" => TableEdit::RowBelow,
                "table.column_left" => TableEdit::ColumnLeft,
                "table.column_right" => TableEdit::ColumnRight,
                "table.delete_row" => TableEdit::DeleteRow,
                "table.delete_column" => TableEdit::DeleteColumn,
                "table.delete" => TableEdit::DeleteTable,
                _ => TableEdit::HeaderRow,
            };
            if writer.editor.cell().is_none() {
                cx.notify(tr!("click-in-a-table-first"));
            } else {
                writer.editor.edit_table(cx, edit);
            }
        }
        "table.to_text" => {
            if !writer.editor.table_to_text(cx) {
                cx.notify(tr!("click-in-a-table-first"));
            }
        }
        "table.from_text" => {
            writer.editor.text_to_table(cx);
        }
        "table.from_file" => {
            if writer.editor.current().is_none() {
                cx.notify(tr!("open-a-document-first"));
                return;
            }
            let pick = FilePick::open(Some(me()), "table-file", &tr!("table-from-file"))
                .extensions(&TABLE_FILES)
                .starting_in(SueWriter::export_dir(cx));
            cx.request(ShellRequest::PickFile(pick));
        }
        "table.export" => {
            let Some((table, ..)) = writer.editor.cell() else {
                cx.notify(tr!("click-in-a-table-first"));
                return;
            };
            writer.editor.commit(cx);
            writer.table_out = Some(table);
            let name = format!("{}.csv", crate::studio::sanitize(&writer.title(cx)));
            let pick = FilePick::save(Some(me()), "table-export", &tr!("export-table-as"), &name)
                .extensions(&["csv", "tsv"])
                .starting_in(SueWriter::export_dir(cx));
            cx.request(ShellRequest::PickFile(pick));
        }
        _ => {}
    }
}
