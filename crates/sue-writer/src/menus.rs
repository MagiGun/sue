//! Sue Writer's menus, as data for the shared menu bar.

use sue_wm::{Menu, MenuItem};

use crate::compile::FORMATS;
use crate::project::{LABELS, STATUSES};

/// Every menu of the bar.
#[must_use]
pub fn menus() -> Vec<Menu> {
    fn item(label: impl Into<String>, command: impl Into<String>) -> MenuItem {
        MenuItem::action(label, command)
    }
    let mut export: Vec<MenuItem> = FORMATS
        .iter()
        .map(|(key, name, _)| item(*name, format!("compile.{key}")))
        .collect();
    export[0] = export[0].clone().key("Ctrl+P");
    vec![
        Menu::new(
            "File",
            vec![
                item("New Project...", "file.new"),
                item("New Document", "binder.new_doc").key("Ctrl+N"),
                item("New Folder", "binder.new_folder").key("Ctrl+Shift+N"),
                MenuItem::Separator,
                item("Import a Document...", "file.import").key("Ctrl+O"),
                MenuItem::Separator,
                item("Compile...", "compile.open").key("Ctrl+Shift+E"),
                MenuItem::sub("Export As", export),
                MenuItem::Separator,
                item("Project Settings...", "project.settings"),
                item("Back to the Hub", "studio.leave"),
            ],
        ),
        Menu::new(
            "Edit",
            vec![
                item("Undo", "edit.undo").key("Ctrl+Z"),
                item("Redo", "edit.redo").key("Ctrl+Shift+Z"),
                MenuItem::Separator,
                item("Find...", "edit.find").key("Ctrl+F"),
                item("Replace...", "edit.replace").key("Ctrl+H"),
                item("Find Next", "edit.find_next").key("F3"),
                item("Find Previous", "edit.find_previous").key("Shift+F3"),
            ],
        ),
        Menu::new(
            "View",
            vec![
                item("Text", "view.document").key("Ctrl+1"),
                item("Corkboard", "view.corkboard").key("Ctrl+2"),
                item("Outliner", "view.outliner").key("Ctrl+3"),
                MenuItem::Separator,
                item("Page View", "view.page"),
                item("Draft View", "view.draft"),
                item("Composition Mode", "view.composition").key("F11"),
                item("Typewriter Scrolling", "view.typewriter"),
                MenuItem::Separator,
                item("Binder", "view.binder"),
                item("Inspector", "view.inspector"),
                MenuItem::Separator,
                item("Zoom In", "view.zoom_in"),
                item("Zoom Out", "view.zoom_out"),
                item("Actual Size", "view.zoom_reset"),
                item("Fit to Window", "view.zoom_fit"),
            ],
        ),
        Menu::new(
            "Insert",
            vec![
                item("Page Break", "insert.page_break").key("Ctrl+Enter"),
                item("Scene Break", "insert.scene_break"),
                item("Footnote...", "insert.footnote").key("Ctrl+Alt+F"),
                item("Link...", "insert.link").key("Ctrl+K"),
                item("Picture...", "insert.picture"),
                item("Today's Date", "insert.date"),
            ],
        ),
        Menu::new(
            "Format",
            vec![
                item("Bold", "format.bold").key("Ctrl+B"),
                item("Italic", "format.italic").key("Ctrl+I"),
                item("Underline", "format.underline").key("Ctrl+U"),
                item("Strikethrough", "format.strike").key("Ctrl+Shift+X"),
                item("Superscript", "format.superscript").key("Ctrl+Shift+="),
                item("Subscript", "format.subscript").key("Ctrl+="),
                item("Small Capitals", "format.small_caps"),
                item("Code", "format.code"),
                item("Clear Formatting", "format.clear").key("Ctrl+Space"),
                MenuItem::Separator,
                MenuItem::sub(
                    "Highlight",
                    vec![
                        item("Yellow", "format.highlight.yellow"),
                        item("Green", "format.highlight.green"),
                        item("Blue", "format.highlight.blue"),
                        item("Pink", "format.highlight.pink"),
                        item("None", "format.highlight.none"),
                    ],
                ),
                MenuItem::sub(
                    "Text Color",
                    vec![
                        item("Automatic", "format.color.none"),
                        item("Red", "format.color.c0392b"),
                        item("Blue", "format.color.1f5fbf"),
                        item("Green", "format.color.2e7d32"),
                        item("Gray", "format.color.6b6b6b"),
                    ],
                ),
                MenuItem::Separator,
                MenuItem::sub(
                    "Alignment",
                    vec![
                        item("Left", "align.left").key("Ctrl+L"),
                        item("Center", "align.center").key("Ctrl+E"),
                        item("Right", "align.right").key("Ctrl+R"),
                        item("Justify", "align.justify").key("Ctrl+J"),
                    ],
                ),
                MenuItem::sub(
                    "Line Spacing",
                    vec![
                        item("Single", "spacing.1"),
                        item("1.15", "spacing.1.15"),
                        item("1.5", "spacing.1.5"),
                        item("Double", "spacing.2"),
                    ],
                ),
                item("Bulleted List", "list.bullet").key("Ctrl+Shift+L"),
                item("Numbered List", "list.number"),
                item("Increase Indent", "indent.more"),
                item("Decrease Indent", "indent.less"),
            ],
        ),
        Menu::new(
            "Project",
            vec![
                item("Split at Selection", "project.split").key("Ctrl+Shift+K"),
                item("Merge With Next", "project.merge"),
                item("Duplicate", "project.duplicate"),
                item("Move to Trash", "project.trash"),
                item("Empty Trash", "project.empty_trash"),
                MenuItem::Separator,
                item("Take Snapshot", "project.snapshot").key("Ctrl+5"),
                item("Include in Compile", "project.include"),
                MenuItem::sub(
                    "Status",
                    STATUSES
                        .iter()
                        .map(|status| item(*status, format!("status.{status}")))
                        .chain([item("No Status", "status.none")])
                        .collect(),
                ),
                MenuItem::sub(
                    "Label",
                    LABELS
                        .iter()
                        .map(|(label, _)| item(*label, format!("label.{label}")))
                        .chain([item("No Label", "label.none")])
                        .collect(),
                ),
                MenuItem::Separator,
                item("Writing Statistics...", "project.statistics"),
                item("Project Settings...", "project.settings"),
            ],
        ),
        Menu::new("Help", vec![item("Keyboard Shortcuts", "help.shortcuts")]),
    ]
}
