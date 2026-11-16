//! The Timeline palette's menu (the button at the top left of its
//! header), in Clip Studio Paint's order: New animation layer, New
//! animation cel, Edit track, Play, Playback settings, Move frame, Label,
//! Timeline, Show animation cels, Light table, Thumbnail size, then Sue's
//! own Time unit and Audio. The dock adds where the palette goes and
//! "Hide Timeline Palette" under them.
//!
//! Left out, with the reason: Show split view and Graph Editor (Sue has
//! neither, and Clip Studio grays the Graph Editor too), Manage timeline
//! (the picker left of the ruler lists, switches and names timelines),
//! Show 2D camera frames, Show camera path and Show all field guides
//! (the camera's frame and path always show while it is edited), and
//! Thumbnail display settings (Thumbnail size is the one setting). The
//! three Toei exposure sheet entries are left out (Q20).

use sue_wm::MenuItem;

/// An entry of the menu.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Entry {
    /// A command, by its name.
    Item(&'static str),
    /// A submenu: its title, and its entries.
    Sub(fn() -> String, &'static [Entry]),
    /// A line between groups.
    Line,
}

use Entry::{Item, Line, Sub};

/// The whole menu, in order.
pub(crate) const TREE: &[Entry] = &[
    Sub(
        || tr!("new-animation-layer"),
        &[Item("anim.layer.folder"), Item("anim.layer.camera")],
    ),
    Item("anim.cel.new"),
    Sub(
        || tr!("edit-track"),
        &[
            Item("anim.cel.assign"),
            Item("anim.cel.rename_timeline"),
            Item("anim.cel.rename_layer"),
            Line,
            Item("anim.cel.prev"),
            Item("anim.cel.next"),
            Line,
            Item("anim.key.add"),
            Item("anim.key.delete"),
            Item("anim.key.layer"),
            Item("anim.key.layer_delete"),
            Item("anim.keyframes"),
            Line,
            Item("anim.frame.insert"),
            Item("anim.frame.delete"),
        ],
    ),
    Line,
    Item("anim.play"),
    Sub(
        || tr!("playback-settings"),
        &[
            Item("anim.loop"),
            Item("anim.audio.scrub"),
            Item("anim.playback.camera"),
            Line,
            Item("anim.play_range.start"),
            Item("anim.play_range.end"),
            Item("anim.play_range.clear"),
        ],
    ),
    Sub(
        || tr!("move-frame"),
        &[
            Item("anim.first"),
            Item("anim.prev"),
            Item("anim.next"),
            Item("anim.last"),
        ],
    ),
    Sub(
        || tr!("timeline-menu-label"),
        &[
            Item("anim.label"),
            Line,
            Item("anim.track_label.inbetween"),
            Item("anim.track_label.none"),
        ],
    ),
    Line,
    Sub(
        || tr!("timeline"),
        &[
            Item("anim.timeline.enable"),
            Item("anim.timeline.new"),
            Item("anim.timeline.prev"),
            Item("anim.timeline.next"),
            Item("anim.timeline.settings"),
            Item("anim.timeline.edit"),
            Line,
            Item("anim.frame.insert"),
            Item("anim.frame.delete"),
            Line,
            Item("anim.exposure_sheet"),
        ],
    ),
    Sub(
        || tr!("show-animation-cels"),
        &[Item("anim.onion"), Item("anim.onion_settings")],
    ),
    Sub(
        || tr!("light-table"),
        &[
            Item("anim.light_table"),
            Line,
            Item("anim.light_table.register"),
            Item("anim.light_table.register_general"),
            Item("anim.light_table.clear"),
        ],
    ),
    Line,
    Sub(
        || tr!("thumbnail-size"),
        &[
            Item("anim.thumbnails.none"),
            Item("anim.thumbnails.smallest"),
            Item("anim.thumbnails.small"),
            Item("anim.thumbnails.medium"),
            Item("anim.thumbnails.large"),
            Item("anim.thumbnails.largest"),
        ],
    ),
    Sub(
        || tr!("time-unit"),
        &[
            Item("anim.time_unit.frames"),
            Item("anim.time_unit.seconds"),
            Item("anim.time_unit.timecode"),
        ],
    ),
    Sub(
        || tr!("audio"),
        &[Item("anim.audio.settings"), Item("anim.export.audio")],
    ),
];

/// Every command of `entries`, in the order the menu reads them, each
/// once.
#[cfg(test)]
pub(crate) fn commands(entries: &[Entry]) -> Vec<&'static str> {
    fn walk(entries: &[Entry], out: &mut Vec<&'static str>) {
        for entry in entries {
            match entry {
                Item(command) => {
                    if !out.contains(command) {
                        out.push(command);
                    }
                }
                Sub(_, inner) => walk(inner, out),
                Line => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(entries, &mut out);
    out
}

/// The menu's items, each labeled as the registry labels its command and
/// showing the keys `keys` gives it.
pub(crate) fn items(entries: &[Entry], keys: &dyn Fn(&str) -> String) -> Vec<MenuItem> {
    let registry = crate::commands::registry();
    entries
        .iter()
        .map(|entry| match entry {
            Item(command) => {
                let label = registry
                    .label(command)
                    .unwrap_or_else(|| (*command).to_owned());
                MenuItem::action(label, *command).key(keys(command))
            }
            Sub(title, inner) => MenuItem::sub(title(), items(inner, keys)),
            Line => MenuItem::Separator,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The titles of the top level, in order, with the lines as `-`.
    fn top() -> Vec<String> {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        TREE.iter()
            .map(|entry| match entry {
                Item(command) => (*command).to_owned(),
                Sub(title, _) => format!("[{}]", title()),
                Line => "-".to_owned(),
            })
            .collect()
    }

    #[test]
    fn the_menu_reads_in_clip_studio_paints_order() {
        assert_eq!(
            top(),
            [
                "[New Animation Layer]",
                "anim.cel.new",
                "[Edit Track]",
                "-",
                "anim.play",
                "[Playback Settings]",
                "[Move Frame]",
                "[Label]",
                "-",
                "[Timeline]",
                "[Show Animation Cels]",
                "[Light Table]",
                "-",
                "[Thumbnail Size]",
                "[Time Unit]",
                "[Audio]",
            ]
        );
        // The Timeline submenu as Clip Studio Paint's: Enable, New,
        // previous and next, Change settings, Change frame rate (Edit
        // Timeline), Insert and Delete frame.
        let Some(Sub(_, timeline)) = TREE.get(9) else {
            panic!("a Timeline submenu");
        };
        assert_eq!(
            commands(timeline),
            [
                "anim.timeline.enable",
                "anim.timeline.new",
                "anim.timeline.prev",
                "anim.timeline.next",
                "anim.timeline.settings",
                "anim.timeline.edit",
                "anim.frame.insert",
                "anim.frame.delete",
                "anim.exposure_sheet",
            ]
        );
    }

    #[test]
    fn every_entry_is_a_registered_command_with_a_label() {
        let registry = crate::commands::registry();
        for command in commands(TREE) {
            assert!(registry.find(command).is_some(), "{command} is registered");
            assert!(registry.label(command).is_some(), "{command} has a label");
        }
        for entry in TREE {
            if let Sub(title, inner) = entry {
                let title = title();
                assert!(!inner.is_empty(), "{title} holds something");
                assert!(
                    !matches!(inner.first(), Some(Line)),
                    "{title} starts with an item"
                );
                assert!(
                    !matches!(inner.last(), Some(Line)),
                    "{title} ends with an item"
                );
            }
        }
    }

    #[test]
    fn the_flat_list_follows_the_tree() {
        assert_eq!(super::super::MENU, commands(TREE).as_slice());
    }
}
