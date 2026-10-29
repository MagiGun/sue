//! Sue Writer's menus, as data for the shared menu bar.
//!
//! They follow LibreOffice Writer's grouping (26.8): the character
//! formats under Format, Text, and alignment, spacing and lists in their
//! own submenus, so that each menu stays short. The toolbar holds what
//! Word's mini toolbar and LibreOffice's single toolbar keep at hand.

use sue_wm::{Menu, MenuItem};

use crate::compile::{self, FORMATS};
use crate::project::{LABELS, STATUSES};

/// Every menu of the bar.
#[must_use]
pub fn menus() -> Vec<Menu> {
    fn item(label: impl Into<String>, command: impl Into<String>) -> MenuItem {
        MenuItem::action(label, command)
    }
    let mut export: Vec<MenuItem> = FORMATS
        .iter()
        .map(|(key, _, _)| item(compile::format_name(key), format!("compile.{key}")))
        .collect();
    export[0] = export[0].clone().key("Ctrl+P");
    vec![
        Menu::new(
            tr!("file"),
            vec![
                item(tr!("new-project-ellipsis"), "file.new"),
                item(tr!("new-document"), "binder.new_doc").key("Ctrl+N"),
                item(tr!("new-folder"), "binder.new_folder").key("Ctrl+Shift+N"),
                MenuItem::Separator,
                item(tr!("import-a-document-ellipsis"), "file.import").key("Ctrl+O"),
                MenuItem::Separator,
                item(tr!("compile-ellipsis"), "compile.open").key("Ctrl+Shift+E"),
                MenuItem::sub(tr!("export-as"), export),
                MenuItem::Separator,
                item(tr!("project-settings-ellipsis"), "project.settings"),
                item(tr!("back-to-the-hub"), "studio.leave"),
            ],
        ),
        Menu::new(
            tr!("edit"),
            vec![
                item(tr!("undo"), "edit.undo").key("Ctrl+Z"),
                item(tr!("redo"), "edit.redo").key("Ctrl+Shift+Z"),
                MenuItem::Separator,
                item(tr!("find-ellipsis"), "edit.find").key("Ctrl+F"),
                item(tr!("replace-ellipsis"), "edit.replace").key("Ctrl+H"),
            ],
        ),
        Menu::new(
            tr!("view"),
            vec![
                item(tr!("text"), "view.document").key("Ctrl+1"),
                item(tr!("corkboard"), "view.corkboard").key("Ctrl+2"),
                item(tr!("outliner"), "view.outliner").key("Ctrl+3"),
                MenuItem::Separator,
                item(tr!("page-view"), "view.page"),
                item(tr!("draft-view"), "view.draft"),
                MenuItem::Separator,
                item(tr!("binder"), "view.binder"),
                item(tr!("inspector"), "view.inspector"),
                MenuItem::Separator,
                item(tr!("composition-mode"), "view.composition").key("F11"),
                item(tr!("typewriter-scrolling"), "view.typewriter"),
                MenuItem::sub(
                    tr!("zoom"),
                    vec![
                        item(tr!("zoom-in"), "view.zoom_in"),
                        item(tr!("zoom-out"), "view.zoom_out"),
                        item(tr!("actual-size"), "view.zoom_reset"),
                        item(tr!("fit-to-window"), "view.zoom_fit"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("insert"),
            vec![
                item(tr!("page-break"), "insert.page_break").key("Ctrl+Enter"),
                item(tr!("scene-break"), "insert.scene_break"),
                item(tr!("footnote-ellipsis"), "insert.footnote").key("Ctrl+Alt+F"),
                item(tr!("header-and-footer-ellipsis"), "insert.header_footer"),
                item(tr!("table-of-contents-ellipsis"), "insert.contents"),
                item(tr!("link-ellipsis"), "insert.link").key("Ctrl+K"),
                item(tr!("picture-ellipsis"), "insert.picture"),
                item(tr!("todays-date"), "insert.date"),
            ],
        ),
        Menu::new(
            tr!("format"),
            vec![
                MenuItem::sub(
                    tr!("text"),
                    vec![
                        item(tr!("bold"), "format.bold").key("Ctrl+B"),
                        item(tr!("italic"), "format.italic").key("Ctrl+I"),
                        item(tr!("underline"), "format.underline").key("Ctrl+U"),
                        item(tr!("strikethrough"), "format.strike").key("Ctrl+Shift+X"),
                        item(tr!("superscript"), "format.superscript").key("Ctrl+Shift+="),
                        item(tr!("subscript"), "format.subscript").key("Ctrl+="),
                        item(tr!("small-capitals"), "format.small_caps"),
                        item(tr!("code"), "format.code"),
                        MenuItem::Separator,
                        MenuItem::sub(
                            tr!("change-case"),
                            sue_type::case::Case::ALL
                                .iter()
                                .map(|(_, key, name)| {
                                    item(
                                        sue_lang::translate("type", name),
                                        format!("format.case.{key}"),
                                    )
                                })
                                .collect(),
                        ),
                    ],
                ),
                MenuItem::sub(
                    tr!("text-color"),
                    vec![
                        item(tr!("automatic"), "format.color.none"),
                        item(tr!("red"), "format.color.c0392b"),
                        item(tr!("blue"), "format.color.1f5fbf"),
                        item(tr!("green"), "format.color.2e7d32"),
                        item(tr!("gray"), "format.color.6b6b6b"),
                    ],
                ),
                MenuItem::sub(
                    tr!("highlight"),
                    vec![
                        item(tr!("yellow"), "format.highlight.yellow"),
                        item(tr!("green"), "format.highlight.green"),
                        item(tr!("blue"), "format.highlight.blue"),
                        item(tr!("pink"), "format.highlight.pink"),
                        item(tr!("none"), "format.highlight.none"),
                    ],
                ),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("align-text"),
                    vec![
                        item(tr!("left"), "align.left").key("Ctrl+L"),
                        item(tr!("center"), "align.center").key("Ctrl+E"),
                        item(tr!("right"), "align.right").key("Ctrl+R"),
                        item(tr!("justify"), "align.justify").key("Ctrl+J"),
                    ],
                ),
                MenuItem::sub(
                    tr!("line-spacing"),
                    vec![
                        item(tr!("single"), "spacing.1"),
                        item("1.15", "spacing.1.15"),
                        item("1.5", "spacing.1.5"),
                        item(tr!("double"), "spacing.2"),
                    ],
                ),
                MenuItem::sub(
                    tr!("lists"),
                    vec![
                        item(tr!("bulleted-list"), "list.bullet").key("Ctrl+Shift+L"),
                        item(tr!("numbered-list"), "list.number"),
                        item(tr!("increase-indent"), "indent.more"),
                        item(tr!("decrease-indent"), "indent.less"),
                    ],
                ),
                MenuItem::Separator,
                item(tr!("clear-direct-formatting"), "format.clear").key("Ctrl+Space"),
            ],
        ),
        Menu::new(
            tr!("table"),
            vec![
                item(tr!("insert-table-ellipsis"), "table.insert").key("Ctrl+F12"),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("insert"),
                    vec![
                        item(tr!("rows-above"), "table.row_above"),
                        item(tr!("rows-below"), "table.row_below"),
                        item(tr!("columns-before"), "table.column_left"),
                        item(tr!("columns-after"), "table.column_right"),
                    ],
                ),
                MenuItem::sub(
                    tr!("delete"),
                    vec![
                        item(tr!("rows"), "table.delete_row"),
                        item(tr!("columns"), "table.delete_column"),
                        item(tr!("table"), "table.delete"),
                    ],
                ),
                item(tr!("heading-row"), "table.header"),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("convert"),
                    vec![
                        item(tr!("text-to-table"), "table.from_text"),
                        item(tr!("table-to-text"), "table.to_text"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("review"),
            vec![
                item(tr!("new-comment-ellipsis"), "review.comment").key("Ctrl+Alt+C"),
                item(tr!("previous-comment"), "review.previous_comment"),
                item(tr!("next-comment"), "review.next_comment"),
                item(tr!("resolve-comment"), "review.resolve"),
                item(tr!("delete-comment"), "review.delete_comment"),
                item(tr!("delete-all-comments"), "review.delete_all_comments"),
                MenuItem::Separator,
                item(tr!("comments"), "review.comments"),
                MenuItem::sub(
                    tr!("track-changes"),
                    vec![
                        item(tr!("record-changes"), "review.track").key("Ctrl+Shift+C"),
                        MenuItem::Separator,
                        item(tr!("previous-change"), "review.previous_change"),
                        item(tr!("next-change"), "review.next_change"),
                        item(tr!("accept-change"), "review.accept"),
                        item(tr!("reject-change"), "review.reject"),
                        MenuItem::Separator,
                        item(tr!("accept-all"), "review.accept_all"),
                        item(tr!("reject-all"), "review.reject_all"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("project"),
            vec![
                item(tr!("split-at-selection"), "project.split").key("Ctrl+Shift+K"),
                item(tr!("merge-with-next"), "project.merge"),
                item(tr!("duplicate"), "project.duplicate"),
                item(tr!("move-to-trash"), "project.trash"),
                item(tr!("empty-trash"), "project.empty_trash"),
                MenuItem::Separator,
                item(tr!("take-snapshot"), "project.snapshot").key("Ctrl+5"),
                item(tr!("include-in-compile"), "project.include"),
                MenuItem::sub(
                    tr!("status"),
                    STATUSES
                        .iter()
                        .map(|status| {
                            item(
                                crate::project::status_text(status),
                                format!("status.{status}"),
                            )
                        })
                        .chain([item(tr!("no-status"), "status.none")])
                        .collect(),
                ),
                MenuItem::sub(
                    tr!("label"),
                    LABELS
                        .iter()
                        .map(|(label, _)| {
                            item(crate::project::label_text(label), format!("label.{label}"))
                        })
                        .chain([item(tr!("no-label"), "label.none")])
                        .collect(),
                ),
                MenuItem::Separator,
                item(tr!("writing-statistics-ellipsis"), "project.statistics"),
            ],
        ),
        Menu::new(
            tr!("help"),
            vec![item(tr!("keyboard-shortcuts"), "help.shortcuts")],
        ),
    ]
}

/// The menus for a phone: one list under a single button, each menu a
/// submenu, without Help (its shortcuts need a keyboard).
#[must_use]
pub fn phone_menu(menus: &[Menu]) -> Vec<MenuItem> {
    menus
        .iter()
        .filter(|menu| {
            !menu.items.iter().any(
                |item| matches!(item, MenuItem::Action { command, .. } if command.starts_with("help.")),
            )
        })
        .map(|menu| MenuItem::sub(menu.title.clone(), menu.items.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(items: &[MenuItem]) -> usize {
        items
            .iter()
            .filter(|item| !matches!(item, MenuItem::Separator))
            .count()
    }

    #[test]
    fn menus_stay_short() {
        // LibreOffice keeps a menu's first level to about a dozen entries;
        // Sue Writer's are shorter still, the rest in submenus.
        for menu in menus() {
            assert!(
                count(&menu.items) <= 12,
                "{} has {} entries",
                menu.title,
                count(&menu.items)
            );
        }
        let format = menus()
            .into_iter()
            .find(|menu| menu.title == "Format")
            .unwrap();
        assert_eq!(count(&format.items), 7);
        let phone = phone_menu(&menus());
        assert_eq!(phone.len(), menus().len() - 1);
        assert!(
            phone
                .iter()
                .all(|item| matches!(item, MenuItem::Sub { .. }))
        );
    }
}
