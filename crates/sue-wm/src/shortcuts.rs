//! Shortcuts for any applet, as settings. An applet lists its commands with
//! their default chords; each becomes a key setting in the applet's own rice
//! section (`<section>.<command>`, dots as underscores), so Settings shows
//! and rebinds them exactly like Sue's own keys, with as many chords per
//! command as people want, and every studio assigns keys the same way.

use egui::InputState;
use sue_rice::{Keybind, Keybinds, RiceDeclaration, RiceParameter, RiceSchema};

use crate::{Menu, MenuItem, Theme, keys};

/// One command and the chords that run it by default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// The command, e.g. `file.save_as`.
    pub command: &'static str,
    /// What people read in Settings, e.g. `Save As...`.
    pub label: String,
    /// Its chords unless someone changed them.
    pub chords: Keybinds,
}

impl Shortcut {
    /// `command`, labeled `label`, bound to `chords` (`Ctrl+S or F12`).
    ///
    /// # Panics
    /// If `chords` does not parse (a built-in table is wrong).
    #[must_use]
    pub fn new(command: &'static str, label: impl Into<String>, chords: &str) -> Self {
        Self {
            command,
            label: label.into(),
            chords: chords
                .parse()
                .unwrap_or_else(|error| panic!("built-in shortcut {command} {chords}: {error}")),
        }
    }
}

/// `key` with `modifiers` as settings write a chord (`Ctrl+Shift+S`,
/// `Ctrl+-`), plain ASCII where the key has a character.
#[must_use]
pub fn chord_text(modifiers: egui::Modifiers, key: egui::Key) -> String {
    let mut chord = String::new();
    for (on, name) in [
        (modifiers.command || modifiers.ctrl, "Ctrl+"),
        (modifiers.alt, "Alt+"),
        (modifiers.shift, "Shift+"),
    ] {
        if on {
            chord.push_str(name);
        }
    }
    chord.push_str(match key {
        egui::Key::Minus => "-",
        key => key.symbol_or_name(),
    });
    chord
}

/// Groups default chords into one shortcut per command holding every
/// chord it has, in first-seen order, labeled by `label`.
#[must_use]
pub fn table(
    defaults: impl IntoIterator<Item = (egui::Modifiers, egui::Key, &'static str)>,
    label: impl Fn(&str) -> String,
) -> Vec<Shortcut> {
    let mut table: Vec<Shortcut> = Vec::new();
    for (modifiers, key, command) in defaults {
        let chord = chord_text(modifiers, key);
        match table
            .iter_mut()
            .find(|shortcut| shortcut.command == command)
        {
            Some(shortcut) => {
                if let Ok(parsed) = chord.parse() {
                    shortcut.chords.push(parsed);
                }
            }
            None => table.push(Shortcut::new(command, label(command), &chord)),
        }
    }
    table
}

/// The setting a command's chords live in, under `section`.
#[must_use]
pub fn setting(section: &str, command: &str) -> String {
    let key: String = command
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{section}.{key}")
}

/// The key settings for `table`, for an applet's `rice_declarations`.
#[must_use]
pub fn declarations(section: &str, table: &[Shortcut]) -> Vec<RiceDeclaration> {
    table
        .iter()
        .map(|shortcut| {
            RiceDeclaration::new(
                setting(section, shortcut.command),
                RiceParameter::Keybind {
                    value: shortcut.chords.clone(),
                },
                &shortcut.label,
            )
        })
        .collect()
}

/// The chords that run `shortcut` now (its setting, else its defaults).
#[must_use]
pub fn chords(rice: &RiceSchema, section: &str, shortcut: &Shortcut) -> Keybinds {
    rice.get_keybind(&setting(section, shortcut.command))
        .unwrap_or_else(|| shortcut.chords.clone())
}

/// Consumes this frame's presses of `table`'s chords, as they are set now,
/// and returns the commands they run, in the table's order.
pub fn take(
    input: &mut InputState,
    rice: &RiceSchema,
    section: &str,
    table: &[Shortcut],
) -> Vec<&'static str> {
    table
        .iter()
        .filter(|shortcut| keys::consume(input, &chords(rice, section, shortcut)))
        .map(|shortcut| shortcut.command)
        .collect()
}

/// Each shortcut's label and its chords as set now, for a Keyboard
/// Shortcuts list.
#[must_use]
pub fn rows(rice: &RiceSchema, section: &str, table: &[Shortcut]) -> Vec<(String, String)> {
    table
        .iter()
        .map(|shortcut| {
            let chords = chords(rice, section, shortcut);
            let text = if chords.is_empty() {
                tr!("none")
            } else {
                chords
                    .chords()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            (shortcut.label.clone(), text)
        })
        .collect()
}

/// Lays out `rows` in two columns, what on the left and its keys on the
/// right.
pub fn grid_ui(ui: &mut egui::Ui, id: &str, rows: &[(String, String)], theme: &Theme) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(egui::vec2(24.0, 6.0))
        .striped(true)
        .show(ui, |ui| {
            for (what, keys) in rows {
                ui.label(egui::RichText::new(what).color(theme.text));
                ui.label(egui::RichText::new(keys).color(theme.muted));
                ui.end_row();
            }
        });
}

/// Writes each command's first chord, as set now, beside its menu item,
/// so menus show what Settings has.
pub fn show_in_menus(menus: &mut [Menu], rice: &RiceSchema, section: &str, table: &[Shortcut]) {
    fn walk(items: &mut [MenuItem], first: &dyn Fn(&str) -> Option<String>) {
        for item in items {
            match item {
                MenuItem::Action {
                    command, shortcut, ..
                } => {
                    if let Some(text) = first(command) {
                        *shortcut = text;
                    }
                }
                MenuItem::Sub { items, .. } => walk(items, first),
                _ => {}
            }
        }
    }
    let first = |command: &str| {
        table
            .iter()
            .find(|shortcut| shortcut.command == command)
            .map(|shortcut| {
                chords(rice, section, shortcut)
                    .first()
                    .map(|chord| chord.to_string())
                    .unwrap_or_default()
            })
    };
    for menu in menus {
        walk(&mut menu.items, &first);
    }
}

/// A studio's shortcuts as set now: the chords it keeps from the shell
/// while open, and the keys its menus show. Refresh it every frame; it
/// only does work when the settings changed.
#[derive(Clone, Debug, Default)]
pub struct Bound {
    revision: Option<u64>,
    chords: Vec<Keybind>,
}

impl Bound {
    /// Catches up with the settings: the menus' keys and the chords.
    pub fn refresh(
        &mut self,
        menus: &mut [Menu],
        rice: &RiceSchema,
        section: &str,
        table: &[Shortcut],
    ) {
        if self.revision == Some(rice.revision()) {
            return;
        }
        self.revision = Some(rice.revision());
        show_in_menus(menus, rice, section, table);
        self.chords = table
            .iter()
            .flat_map(|shortcut| chords(rice, section, shortcut).chords().to_vec())
            .collect();
    }

    /// Makes the next [`Self::refresh`] write the keys into the menus
    /// again (after the menus were built anew, in another language).
    pub fn invalidate(&mut self) {
        self.revision = None;
    }

    /// Every chord the shortcuts use (their defaults until the first
    /// refresh).
    #[must_use]
    pub fn chords(&self, table: &[Shortcut]) -> Vec<Keybind> {
        if self.revision.is_none() {
            table
                .iter()
                .flat_map(|shortcut| shortcut.chords.chords().to_vec())
                .collect()
        } else {
            self.chords.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_shortcuts_are_settings_with_any_number_of_chords() {
        let table = vec![
            Shortcut::new("file.save_as", "Save As...", "Ctrl+Shift+S or Ctrl+Alt+S"),
            Shortcut::new("view.zoom_in", "Zoom In", "Ctrl+="),
        ];
        assert_eq!(
            setting("sue_paint_keys", "file.save_as"),
            "sue_paint_keys.file_save_as"
        );
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        assert_eq!(chords(&rice, "demo_keys", &table[0]).chords().len(), 2);
        // Rebinding changes what runs it.
        rice.set(
            "demo_keys.view_zoom_in",
            sue_rice::RiceValue::Keybind("Ctrl+Plus or F9".parse().unwrap()),
        )
        .unwrap();
        let mut input = InputState::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::F9,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        assert_eq!(
            take(&mut input, &rice, "demo_keys", &table),
            vec!["view.zoom_in"]
        );
    }

    #[test]
    fn defaults_group_by_command_and_read_as_plain_text() {
        use egui::{Key, Modifiers};
        let grouped = super::table(
            [
                (
                    Modifiers::COMMAND | Modifiers::SHIFT,
                    Key::S,
                    "file.save_as",
                ),
                (Modifiers::COMMAND, Key::Minus, "view.zoom_out"),
                (Modifiers::ALT | Modifiers::SHIFT, Key::S, "file.save_as"),
            ],
            str::to_uppercase,
        );
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].chords.to_string(), "Ctrl+Shift+S or Alt+Shift+S");
        assert_eq!(grouped[0].label, "FILE.SAVE_AS");
        assert_eq!(grouped[1].chords.to_string(), "Ctrl+-");
    }

    #[test]
    fn menus_and_kept_chords_follow_the_settings() {
        let table = vec![Shortcut::new(
            "file.save_as",
            "Save As",
            "Ctrl+Shift+S or F12",
        )];
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        let mut menus = vec![Menu::new(
            "File",
            vec![MenuItem::action("Save As...", "file.save_as").key("Ctrl+Alt+S")],
        )];
        let mut bound = Bound::default();
        assert_eq!(
            bound.chords(&table).len(),
            2,
            "the defaults before a refresh"
        );
        bound.refresh(&mut menus, &rice, "demo_keys", &table);
        let shown = |menus: &[Menu]| match &menus[0].items[0] {
            MenuItem::Action { shortcut, .. } => shortcut.clone(),
            _ => String::new(),
        };
        assert_eq!(shown(&menus), "Ctrl+Shift+S");
        rice.set(
            "demo_keys.file_save_as",
            sue_rice::RiceValue::Keybind("F9".parse().unwrap()),
        )
        .unwrap();
        bound.refresh(&mut menus, &rice, "demo_keys", &table);
        assert_eq!(shown(&menus), "F9");
        assert_eq!(bound.chords(&table), vec!["F9".parse::<Keybind>().unwrap()]);
        assert_eq!(rows(&rice, "demo_keys", &table)[0].1, "F9");
    }
}
