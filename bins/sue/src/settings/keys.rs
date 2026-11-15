//! Keyboard Shortcuts: a studio's commands grouped as its menus are, each
//! with its keys as chips, a search by the command's name or its key, and
//! a mark on every row whose key another command holds too, with an offer
//! to take the key from the other one.
//!
//! Every command can be given a key, whether or not it ships with one:
//! the key goes where `sue_wm::shortcuts::bind` says (the command's own
//! setting, or the studio's setting for commands without a default).

/// Text from the language database's `settings` namespace.
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use egui::{Align, Layout, RichText, Ui};
use sue_rice::{Keybind, Keybinds, RiceParameter, RiceSchema, RiceValue};
use sue_wm::shortcuts::{self, Conflict, SparseKeys};
use sue_wm::ui::Tip;
use sue_wm::{AppletContext, KeyCommand, KeyGroup, KeyMap};

use super::{Edit, Settings};

/// Sue's own keys (the `[keys]` section): run before any studio's.
pub(super) const SUE_KEYS: &str = "keys";

/// The id of the key map of Sue's own keys.
pub(crate) const SUE_KEYMAP: &str = "sue";

/// Sue's own keys as a key map of one group, in the language in use: one
/// row per setting of `[keys]`, each a command named by its setting's key
/// (`command_palette` is `keys.command_palette`), so its rows look, search
/// and warn like a studio's.
pub(crate) fn sue_keymap(rice: &RiceSchema) -> KeyMap {
    let commands = rice
        .entries()
        .iter()
        .filter(|entry| entry.section() == SUE_KEYS)
        .filter_map(|entry| {
            let RiceParameter::Keybind { value } = entry.parameter() else {
                return None;
            };
            let name = entry.path().strip_prefix("keys.")?;
            let defaults = match super::default_of(entry.path()) {
                Some(RiceValue::Keybind(shipped)) => shipped,
                _ => value.clone(),
            };
            Some(KeyCommand {
                command: name.to_owned(),
                label: super::label_of(entry),
                defaults,
            })
        })
        .collect();
    KeyMap {
        id: SUE_KEYMAP.to_owned(),
        title: tr!("sue"),
        section: SUE_KEYS.to_owned(),
        groups: vec![KeyGroup {
            title: tr!("sue"),
            commands,
        }],
    }
}

/// Who holds a key: a command of the key map shown, or a command of
/// another (one of Sue's own keys under a studio's, a studio's command
/// under Sue's own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Holder {
    /// A command of the key map.
    Command(String),
    /// A command of the key map `map`.
    Other {
        /// That key map's id.
        map: String,
        /// The command.
        command: String,
    },
}

/// The keys of a key map as they are set now.
pub(super) struct Resolved {
    /// Each command once, with its chords, in the map's order.
    pub rows: Vec<(String, Keybinds)>,
    /// The chords held by more than one command (a command of the map
    /// and another of it, or of a key map it meets).
    pub conflicts: Vec<Conflict>,
}

/// Parts the map's id from the command's in the name of a command of
/// another key map.
const OTHER: char = '\u{1f}';

/// The name a holder of a key goes by.
fn holder_of(name: &str) -> Holder {
    match name.split_once(OTHER) {
        Some((map, command)) => Holder::Other {
            map: map.to_owned(),
            command: command.to_owned(),
        },
        None => Holder::Command(name.to_owned()),
    }
}

/// Each command of `map` once, with its chords as set now.
fn rows_of(rice: &RiceSchema, map: &KeyMap) -> Vec<(String, Keybinds)> {
    let sparse = SparseKeys::read(rice, &map.section);
    let mut rows: Vec<(String, Keybinds)> = Vec::new();
    for command in map.commands() {
        if rows.iter().any(|(known, _)| known == &command.command) {
            continue;
        }
        let chords = shortcuts::chords_among(
            rice,
            &map.section,
            &command.command,
            &command.defaults,
            &sparse,
        );
        rows.push((command.command.clone(), chords));
    }
    rows
}

/// Resolves `map` against `rice` and the key maps of `maps` it meets: a
/// studio's keys meet Sue's own (which run first, so a press of one never
/// reaches the studio), and Sue's own meet every studio's. Two studios
/// never meet: only one of them is in front.
pub(super) fn resolve(rice: &RiceSchema, map: &KeyMap, maps: &[KeyMap]) -> Resolved {
    let rows = rows_of(rice, map);
    let mine = map.section == SUE_KEYS;
    let mut all: Vec<(String, Keybinds)> = rows.clone();
    for other in maps {
        if other.id == map.id || !(mine || other.section == SUE_KEYS) {
            continue;
        }
        all.extend(
            rows_of(rice, other)
                .into_iter()
                .map(|(command, chords)| (format!("{}{OTHER}{command}", other.id), chords)),
        );
    }
    let borrowed: Vec<(&str, Keybinds)> = all
        .iter()
        .map(|(name, chords)| (name.as_str(), chords.clone()))
        .collect();
    let mut conflicts = shortcuts::conflicts_among(&borrowed);
    // Only a chord a command of this map holds is this map's business.
    conflicts.retain(|conflict| conflict.commands.iter().any(|name| !name.contains(OTHER)));
    Resolved { rows, conflicts }
}

/// The others that hold `chord` besides `me`.
pub(super) fn others(resolved: &Resolved, me: &str, chord: &Keybind) -> Vec<Holder> {
    resolved
        .conflicts
        .iter()
        .filter(|conflict| {
            conflict.commands.iter().any(|name| name == me)
                && conflict_press(&conflict.chord) == conflict_press(chord)
        })
        .flat_map(|conflict| conflict.commands.iter())
        .filter(|name| name.as_str() != me)
        .map(|name| holder_of(name))
        .collect()
}

/// A chord as presses tell them apart (Ctrl and Super are one).
fn conflict_press(chord: &Keybind) -> (bool, bool, bool, String) {
    (
        chord.modifiers.ctrl || chord.modifiers.super_key,
        chord.modifiers.alt,
        chord.modifiers.shift,
        format!("{:?}", chord.key),
    )
}

/// What `holder` is called, in the language in use: a command of `map` by
/// its name, one of another key map with whose it is.
pub(super) fn holder_label(holder: &Holder, map: &KeyMap, maps: &[KeyMap]) -> String {
    match holder {
        Holder::Command(command) => map
            .find(command)
            .map_or_else(|| command.clone(), |found| found.label.clone()),
        Holder::Other { map: id, command } => {
            let Some(other) = maps.iter().find(|other| &other.id == id) else {
                return command.clone();
            };
            let name = other
                .find(command)
                .map_or_else(|| command.clone(), |found| found.label.clone());
            if other.section == SUE_KEYS {
                tr!("keys-sue-key", name = name)
            } else {
                tr!(
                    "keys-studio-key",
                    name = name,
                    studio = other.title.as_str()
                )
            }
        }
    }
}

/// The edit that takes `chord` from `holder` (a command of `map`, or of
/// another key map of `maps`).
pub(super) fn take_from(
    rice: &RiceSchema,
    map: &KeyMap,
    maps: &[KeyMap],
    holder: &Holder,
    chord: &Keybind,
) -> Option<Edit> {
    let (map, command) = match holder {
        Holder::Command(command) => (map, command),
        Holder::Other { map: id, command } => (maps.iter().find(|other| &other.id == id)?, command),
    };
    let (_, chords) = rows_of(rice, map)
        .into_iter()
        .find(|(name, _)| name == command)?;
    let mut left = Keybinds::default();
    for kept in chords.chords() {
        if conflict_press(kept) != conflict_press(chord) {
            left.push(*kept);
        }
    }
    let (path, value) = shortcuts::bind(rice, &map.section, command, &left);
    Some(Edit::Set(path, value))
}

/// `true` when `needle` (lower case) finds the command `label` (`name`)
/// with `chords` in the group `group`.
pub(super) fn finds(needle: &str, group: &str, label: &str, name: &str, chords: &Keybinds) -> bool {
    needle.is_empty()
        || label.to_lowercase().contains(needle)
        || name.to_lowercase().contains(needle)
        || group.to_lowercase().contains(needle)
        || chords
            .chords()
            .iter()
            .any(|chord| chord.to_string().to_lowercase() == needle)
}

impl Settings {
    /// The rows of key map `id`: a group per menu, each closed until
    /// opened (open while searching), with a mark on its title while a
    /// key in it is held twice.
    pub(super) fn keys_ui(
        &mut self,
        ui: &mut Ui,
        id: &str,
        needle: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let Some(map) = cx.catalog.keymap(id).cloned() else {
            return;
        };
        let theme = cx.theme.clone();
        let maps = cx.catalog.keymaps.clone();
        let resolved = resolve(cx.rice(), &map, &maps);
        // A map of one group (Sue's own keys) is its rows under the
        // page's heading, with no title of its own to open.
        let folds = map.groups.len() > 1;
        let mut edit = None;
        for group in &map.groups {
            let shown: Vec<_> = group
                .commands
                .iter()
                .filter(|command| {
                    let chords = resolved
                        .rows
                        .iter()
                        .find(|(name, _)| name == &command.command)
                        .map(|(_, chords)| chords.clone())
                        .unwrap_or_default();
                    finds(
                        needle,
                        &group.title,
                        &command.label,
                        &command.command,
                        &chords,
                    )
                })
                .collect();
            if shown.is_empty() {
                continue;
            }
            let held_twice = shown
                .iter()
                .filter(|command| {
                    resolved
                        .conflicts
                        .iter()
                        .any(|conflict| conflict.commands.contains(&command.command))
                })
                .count();
            let key = format!("keys/{id}/{}", group.title);
            let title = if held_twice == 0 {
                group.title.clone()
            } else {
                tr!(
                    "keys-group-held-twice",
                    group = group.title.as_str(),
                    count = held_twice
                )
            };
            if folds && !self.fold(ui, &key, &title, needle.is_empty(), &theme) {
                continue;
            }
            for command in shown {
                if let Some(next) = self.key_row(ui, &map, &maps, &resolved, command, cx) {
                    edit = Some(next);
                }
            }
        }
        if let Some(edit) = edit {
            self.apply(cx, edit);
        }
    }

    /// One command's row: its name, its keys as chips (a click changes
    /// one, the cross takes it away, + adds one), and a line for each
    /// key another command holds too.
    fn key_row(
        &mut self,
        ui: &mut Ui,
        map: &KeyMap,
        maps: &[KeyMap],
        resolved: &Resolved,
        command: &KeyCommand,
        cx: &AppletContext<'_>,
    ) -> Option<Edit> {
        let theme = cx.theme.clone();
        let rice = cx.rice();
        let name = command.command.as_str();
        let chords = resolved
            .rows
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, chords)| chords.clone())
            .unwrap_or_default();
        let recording_id = format!("{}#{name}", map.section);
        let recording = self
            .recording
            .as_ref()
            .filter(|(known, _)| known == &recording_id)
            .map(|(_, index)| *index);
        let mut edit = None;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            ui.set_width(width);
            ui.label(
                RichText::new(&command.label)
                    .size(theme.ui_size)
                    .color(theme.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let adding = recording == Some(None);
                if sue_wm::ui::icon_button(
                    ("sue-key-add", name),
                    sue_wm::icons::Icon::Plus,
                    &tr!("add-another-shortcut-as-many-as"),
                )
                .selected(adding)
                .show(ui, &theme)
                .clicked()
                {
                    self.recording = Some((recording_id.clone(), None));
                }
                if adding {
                    sue_wm::ui::chip(("sue-key-new", name), &tr!("press-keys-ellipsis"), true)
                        .show(ui, &theme);
                }
                for (index, chord) in chords.chords().iter().enumerate().rev() {
                    let on = recording == Some(Some(index));
                    let text = if on {
                        tr!("press-keys-ellipsis")
                    } else {
                        chord.to_string()
                    };
                    let take = tr!("take-this-shortcut-away");
                    let chip = sue_wm::ui::chip(("sue-key", name, index), &text, on)
                        .removable(&take)
                        .show(ui, &theme);
                    if chip.removed {
                        let mut next = chords.clone();
                        next.remove(index);
                        let (path, value) = shortcuts::bind(rice, &map.section, name, &next);
                        edit = Some(Edit::Set(path, value));
                    } else {
                        let help = tr!("click-then-press-shortcut-put-here");
                        let shown = chord.to_string();
                        let tip = Tip::new(&command.label).shortcut(&shown).help(&help);
                        if sue_wm::ui::tooltip(chip.response, &tip, &theme).clicked() {
                            self.recording = Some((recording_id.clone(), Some(index)));
                        }
                    }
                }
                if chords.is_empty() && recording.is_none() {
                    ui.label(RichText::new(tr!("none")).color(theme.muted));
                }
            });
        });
        // A key held by another command too: say which, and offer to take
        // it from the other one.
        for chord in chords.chords() {
            for other in others(resolved, name, chord) {
                let who = holder_label(&other, map, maps);
                ui.horizontal_wrapped(|ui| {
                    let (_, mark) = ui.allocate_space(egui::Vec2::splat(
                        theme.text_size(sue_wm::ui::TextRole::Caption)
                            + theme.space(sue_wm::ui::Space::S1),
                    ));
                    sue_wm::icons::paint(
                        ui.painter(),
                        mark,
                        sue_wm::icons::Icon::Warning,
                        theme.text,
                    );
                    ui.label(
                        RichText::new(tr!(
                            "keys-held-twice",
                            chord = chord.to_string(),
                            other = who.as_str()
                        ))
                        .size(theme.small_size)
                        .color(theme.text),
                    );
                    let offer = tr!(
                        "keys-take-from",
                        chord = chord.to_string(),
                        other = who.as_str()
                    );
                    let help = tr!("keys-take-from-hint");
                    if sue_wm::ui::button(("sue-key-take", name, who.as_str()), &offer)
                        .ghost()
                        .hint(&help)
                        .show(ui, &theme)
                        .clicked()
                    {
                        edit = take_from(rice, map, maps, &other, chord);
                    }
                });
            }
        }
        if edit.is_some() {
            return edit;
        }
        if let Some(index) = recording
            && let Some(next) = self.record_chord(ui, &chords, index)
        {
            let (path, value) = shortcuts::bind(rice, &map.section, name, &next);
            // Taken already: the information line says by what, at once.
            if let Some(chord) = next
                .chords()
                .iter()
                .find(|chord| index.is_none_or(|index| chords.chords().get(index) != Some(*chord)))
            {
                let mut after = rice.clone();
                if after.set(&path, value.clone()).is_ok() {
                    let now = resolve(&after, map, maps);
                    let held: Vec<String> = others(&now, name, chord)
                        .iter()
                        .map(|other| holder_label(other, map, maps))
                        .collect();
                    self.notice = (!held.is_empty()).then(|| {
                        tr!(
                            "keys-taken-notice",
                            chord = chord.to_string(),
                            command = command.label.as_str(),
                            others = held.join(", ")
                        )
                    });
                }
            }
            return Some(Edit::Set(path, value));
        }
        None
    }
}
