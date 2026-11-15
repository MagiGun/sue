//! Sue Writer's menus, as data for the shared menu bar.
//!
//! They follow LibreOffice Writer's grouping (26.8): the character
//! formats under Format, Text, and alignment, spacing and lists in their
//! own submenus, so that each menu stays short. The toolbar holds what
//! Word's mini toolbar and LibreOffice's single toolbar keep at hand.
//!
//! The menus hold the tree of command names. A command's label comes from
//! the registry ([`crate::commands`]), its key from the settings (the
//! studio writes them in each frame), its state from the registry too.
//! Items of a submenu that names what they do ("Delete", "Export As")
//! keep a shorter label of their own.

use sue_wm::{Menu, MenuItem};

use crate::commands::registry;
use crate::compile::{self, FORMATS};
use crate::project::{LABELS, STATUSES};

/// An item for `command`, labeled as the registry labels it.
fn item(command: &str) -> MenuItem {
    let label = registry()
        .label(command)
        .unwrap_or_else(|| command.to_owned());
    MenuItem::action(label, command)
}

/// An item for `command` with a label of its own, for a submenu whose
/// title says the rest.
fn named(label: impl Into<String>, command: impl Into<String>) -> MenuItem {
    MenuItem::action(label, command)
}

/// Every menu of the bar.
#[must_use]
pub fn menus() -> Vec<Menu> {
    let export: Vec<MenuItem> = FORMATS
        .iter()
        .map(|(key, _, _)| named(compile::format_name(key), format!("compile.{key}")))
        .collect();
    vec![
        // One skeleton for every studio (plan D11): File (new, open,
        // save, export, settings, back, quit), Edit, View, the studio's
        // own menus, Help.
        Menu::new(
            tr!("file"),
            vec![
                item("file.new"),
                item("binder.new_doc"),
                item("binder.new_folder"),
                MenuItem::Separator,
                // The shell's Open key runs `file.open`, so the item shows it.
                item("file.open"),
                MenuItem::Separator,
                item("file.save"),
                item("file.save_as"),
                MenuItem::sub(
                    tr!("templates"),
                    vec![item("file.save_template"), item("file.templates_folder")],
                ),
                MenuItem::Separator,
                item("compile.open"),
                MenuItem::sub(tr!("export-as"), export),
                MenuItem::Separator,
                item("file.settings"),
                MenuItem::Separator,
                item("file.leave"),
                item("app.quit"),
            ],
        ),
        Menu::new(
            tr!("edit"),
            vec![
                item("edit.undo"),
                item("edit.redo"),
                MenuItem::Separator,
                item("edit.cut"),
                item("edit.copy"),
                item("edit.paste"),
                item("edit.select_all"),
                MenuItem::Separator,
                item("edit.find"),
                item("edit.replace"),
            ],
        ),
        Menu::new(
            tr!("view"),
            vec![
                item("view.document"),
                item("view.corkboard"),
                item("view.outliner"),
                MenuItem::Separator,
                item("view.page"),
                item("view.draft"),
                MenuItem::Separator,
                item("view.binder"),
                item("view.inspector"),
                MenuItem::Separator,
                item("view.composition"),
                item("view.typewriter"),
                MenuItem::sub(
                    tr!("zoom"),
                    vec![
                        item("view.zoom_in"),
                        item("view.zoom_out"),
                        item("view.zoom_reset"),
                        item("view.zoom_fit"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("insert"),
            vec![
                item("insert.page_break"),
                item("insert.scene_break"),
                item("insert.footnote"),
                item("insert.endnote"),
                item("insert.header_footer"),
                item("insert.contents"),
                item("insert.link"),
                item("insert.picture"),
                item("insert.date"),
                MenuItem::sub(
                    tr!("formatting-mark"),
                    vec![
                        item("insert.mark.nbsp"),
                        item("insert.mark.nbhyphen"),
                        item("insert.mark.soft_hyphen"),
                        item("insert.mark.narrow"),
                        item("insert.mark.zwsp"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("format"),
            vec![
                MenuItem::sub(
                    tr!("text"),
                    vec![
                        item("format.bold"),
                        item("format.italic"),
                        item("format.underline"),
                        item("format.strike"),
                        item("format.superscript"),
                        item("format.subscript"),
                        item("format.small_caps"),
                        item("format.caps"),
                        item("format.grow"),
                        item("format.shrink"),
                        item("format.code"),
                        MenuItem::Separator,
                        MenuItem::sub(
                            tr!("change-case"),
                            sue_type::case::Case::ALL
                                .iter()
                                .map(|(_, key, name)| {
                                    named(
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
                        named(tr!("automatic"), "format.color.none"),
                        named(tr!("red"), "format.color.c0392b"),
                        named(tr!("blue"), "format.color.1f5fbf"),
                        named(tr!("green"), "format.color.2e7d32"),
                        named(tr!("gray"), "format.color.6b6b6b"),
                    ],
                ),
                MenuItem::sub(
                    tr!("highlight"),
                    vec![
                        named(tr!("yellow"), "format.highlight.yellow"),
                        named(tr!("green"), "format.highlight.green"),
                        named(tr!("blue"), "format.highlight.blue"),
                        named(tr!("pink"), "format.highlight.pink"),
                        named(tr!("none"), "format.highlight.none"),
                    ],
                ),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("align-text"),
                    vec![
                        item("align.left"),
                        item("align.center"),
                        item("align.right"),
                        item("align.justify"),
                    ],
                ),
                MenuItem::sub(
                    tr!("line-spacing"),
                    vec![
                        named(tr!("single"), "spacing.1"),
                        named("1.15", "spacing.1.15"),
                        named("1.5", "spacing.1.5"),
                        named(tr!("double"), "spacing.2"),
                    ],
                ),
                MenuItem::sub(
                    tr!("lists"),
                    vec![
                        item("list.bullet"),
                        item("list.number"),
                        item("indent.more"),
                        item("indent.less"),
                    ],
                ),
                MenuItem::Separator,
                item("format.clear"),
            ],
        ),
        Menu::new(
            tr!("table"),
            vec![
                item("table.insert"),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("insert"),
                    vec![
                        named(tr!("rows-above"), "table.row_above"),
                        named(tr!("rows-below"), "table.row_below"),
                        named(tr!("columns-before"), "table.column_left"),
                        named(tr!("columns-after"), "table.column_right"),
                    ],
                ),
                MenuItem::sub(
                    tr!("delete"),
                    vec![
                        named(tr!("rows"), "table.delete_row"),
                        named(tr!("columns"), "table.delete_column"),
                        named(tr!("table"), "table.delete"),
                    ],
                ),
                item("table.header"),
                MenuItem::Separator,
                MenuItem::sub(
                    tr!("convert"),
                    vec![
                        item("table.from_text"),
                        item("table.to_text"),
                        MenuItem::Separator,
                        item("table.from_file"),
                        item("table.export"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("review"),
            vec![
                item("review.spelling"),
                item("review.next_misspelling"),
                MenuItem::Separator,
                item("review.comment"),
                item("review.previous_comment"),
                item("review.next_comment"),
                item("review.resolve"),
                item("review.delete_comment"),
                item("review.delete_all_comments"),
                MenuItem::Separator,
                item("review.comments"),
                MenuItem::sub(
                    tr!("track-changes"),
                    vec![
                        item("review.track"),
                        MenuItem::Separator,
                        item("review.previous_change"),
                        item("review.next_change"),
                        item("review.accept"),
                        item("review.reject"),
                        MenuItem::Separator,
                        item("review.accept_all"),
                        item("review.reject_all"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("project"),
            vec![
                item("project.split"),
                item("project.merge"),
                item("project.duplicate"),
                item("project.trash"),
                item("project.empty_trash"),
                MenuItem::Separator,
                item("project.snapshot"),
                item("project.include"),
                MenuItem::sub(
                    tr!("status"),
                    STATUSES
                        .iter()
                        .map(|status| {
                            named(
                                crate::project::status_text(status),
                                format!("status.{status}"),
                            )
                        })
                        .chain([named(tr!("no-status"), "status.none")])
                        .collect(),
                ),
                MenuItem::sub(
                    tr!("label"),
                    LABELS
                        .iter()
                        .map(|(label, _)| {
                            named(crate::project::label_text(label), format!("label.{label}"))
                        })
                        .chain([named(tr!("no-label"), "label.none")])
                        .collect(),
                ),
                MenuItem::Separator,
                item("project.statistics"),
                item("project.settings"),
            ],
        ),
        Menu::new(
            tr!("help"),
            vec![
                item("help.shortcuts"),
                MenuItem::Separator,
                item("help.updates"),
                item("help.about"),
            ],
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

    /// Every command a menu sends, submenus included.
    fn sent(items: &[MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                MenuItem::Action { command, .. } => out.push(command.clone()),
                MenuItem::Sub { items, .. } => sent(items, out),
                MenuItem::Separator => {}
            }
        }
    }

    #[test]
    fn the_menus_follow_the_skeleton_every_studio_shares() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(titles.first(), Some(&"File"));
        assert_eq!(titles.get(1), Some(&"Edit"));
        assert_eq!(titles.get(2), Some(&"View"));
        assert_eq!(titles.last(), Some(&"Help"));
        let of = |title: &str| {
            let mut out = Vec::new();
            sent(
                &menus.iter().find(|menu| menu.title == title).unwrap().items,
                &mut out,
            );
            out
        };
        let has = |title: &str, commands: &[&str]| {
            let found = of(title);
            for command in commands {
                assert!(found.iter().any(|one| one == command), "{title}: {command}");
            }
        };
        has(
            "File",
            &[
                "file.new",
                "file.open",
                "file.save",
                "file.save_as",
                "compile.open",
                "compile.txt",
                "file.settings",
                "file.leave",
                "app.quit",
            ],
        );
        has(
            "Edit",
            &[
                "edit.undo",
                "edit.redo",
                "edit.cut",
                "edit.copy",
                "edit.paste",
                "edit.select_all",
                "edit.find",
            ],
        );
        has("Table", &["table.from_file", "table.export"]);
        has("Project", &["project.settings"]);
        has("Help", &["help.shortcuts", "help.updates", "help.about"]);
        // The old name of Back to the Hub is in no menu, and every command
        // a menu sends is one the registry runs.
        for menu in &menus {
            let mut all = Vec::new();
            sent(&menu.items, &mut all);
            assert!(!all.iter().any(|one| one == "studio.leave"));
            for command in all {
                assert!(registry().find(&command).is_some(), "{command}");
            }
        }
        // "Compile" reads "Export".
        let file = &menus[0];
        let labels: Vec<&str> = file
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"Export..."), "{labels:?}");
        assert!(!labels.iter().any(|label| label.contains("Compile")));
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
