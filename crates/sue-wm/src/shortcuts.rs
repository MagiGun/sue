//! Shortcuts for any applet, as settings. An applet lists its commands with
//! their default chords; each becomes a key setting in the applet's own rice
//! section (`<section>.<command>`, dots as underscores), so Settings shows
//! and rebinds them exactly like Sue's own keys, with as many chords per
//! command as people want, and every studio assigns keys the same way.
//!
//! Every command can be given a key, whether or not it ships with one. A
//! command without a default chord gets no setting of its own (a studio
//! has hundreds of them): the keys people give such commands live together
//! in one text setting, `<section>.custom`, read and written as
//! [`SparseKeys`] (`layer.merge_down=Ctrl+E view.grid=F7 or F8`). [`bind`]
//! says which of the two a new key goes to.
//!
//! One chord held by two commands of a studio runs only the first of them;
//! [`conflicts`] finds those, so Settings can mark them.

use std::collections::HashMap;
use std::fmt;

use egui::InputState;
use sue_rice::{Keybind, Keybinds, RiceDeclaration, RiceParameter, RiceSchema, RiceValue};

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

/// The key, in a studio's section, of the one setting that holds the keys
/// of every command without a default chord. A command's own setting has
/// an underscore where its name has a dot, so the two never meet.
pub const SPARSE_KEY: &str = "custom";

/// The setting that holds the keys of commands without a default chord,
/// under `section`.
#[must_use]
pub fn sparse_setting(section: &str) -> String {
    format!("{section}.{SPARSE_KEY}")
}

/// The keys people gave to commands that ship without one, as one text:
/// `layer.merge_down=Ctrl+E view.grid=F7 or F8`. Entries are parted by
/// spaces; an entry is a command's name, `=`, and its chords joined by
/// ` or `. Neither a name nor a chord has a space in it, so nothing needs
/// quoting. A name this build does not know is kept as it is (a command of
/// a newer build, a sub tool, an Auto Action).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SparseKeys(Vec<(String, Keybinds)>);

impl SparseKeys {
    /// Reads the text. What does not read (a word that is no entry, a
    /// chord that is no chord) is left out, never an error: the text is a
    /// setting people may edit by hand.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut keys = Self::default();
        let mut entry: Option<(String, Keybinds)> = None;
        let mut after_or = false;
        for word in text.split_whitespace() {
            if after_or {
                after_or = false;
                if let (Some((_, chords)), Ok(chord)) = (entry.as_mut(), word.parse()) {
                    chords.push(chord);
                }
                continue;
            }
            if word == "or" && entry.is_some() {
                after_or = true;
                continue;
            }
            if let Some((name, chords)) = entry.take() {
                keys.set(&name, chords);
            }
            entry = match word.split_once('=') {
                Some((name, chord)) if !name.is_empty() => {
                    let mut chords = Keybinds::default();
                    if let Ok(chord) = chord.parse() {
                        chords.push(chord);
                    }
                    Some((name.to_owned(), chords))
                }
                _ => None,
            };
        }
        if let Some((name, chords)) = entry {
            keys.set(&name, chords);
        }
        keys
    }

    /// The keys kept in `section`'s setting (none when the studio has not
    /// declared it).
    #[must_use]
    pub fn read(rice: &RiceSchema, section: &str) -> Self {
        rice.get_str(&sparse_setting(section))
            .map(Self::parse)
            .unwrap_or_default()
    }

    /// The chords of `command`, if it was given any.
    #[must_use]
    pub fn get(&self, command: &str) -> Option<&Keybinds> {
        self.0
            .iter()
            .find(|(name, _)| name == command)
            .map(|(_, chords)| chords)
    }

    /// Gives `command` these chords, in its old place if it had some; no
    /// chords takes it out.
    pub fn set(&mut self, command: &str, chords: Keybinds) {
        // A chord is one word here, as every chord a key press makes is.
        let chords = Keybinds::new(
            chords
                .chords()
                .iter()
                .copied()
                .filter(|chord| !chord.to_string().contains(char::is_whitespace)),
        );
        let place = self.0.iter().position(|(name, _)| name == command);
        match (place, chords.is_empty()) {
            (Some(index), true) => {
                self.0.remove(index);
            }
            (Some(index), false) => self.0[index].1 = chords,
            (None, true) => {}
            (None, false) => self.0.push((command.to_owned(), chords)),
        }
    }

    /// Every command with its chords, in the order they were given.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Keybinds)> {
        self.0.iter().map(|(name, chords)| (name.as_str(), chords))
    }

    /// How many commands have keys here.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// `true` with no key at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for SparseKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (name, chords)) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{name}={chords}")?;
        }
        Ok(())
    }
}

/// The key settings for `table`, for an applet's `rice_declarations`: one
/// setting for each command with a default chord and, when the table has
/// commands without one, the one setting that holds the keys people give
/// those ([`SparseKeys`]).
#[must_use]
pub fn declarations(section: &str, table: &[Shortcut]) -> Vec<RiceDeclaration> {
    let mut declarations: Vec<RiceDeclaration> = table
        .iter()
        .filter(|shortcut| !shortcut.chords.is_empty())
        .map(|shortcut| {
            RiceDeclaration::new(
                setting(section, shortcut.command),
                RiceParameter::Keybind {
                    value: shortcut.chords.clone(),
                },
                &shortcut.label,
            )
        })
        .collect();
    if table.iter().any(|shortcut| shortcut.chords.is_empty()) {
        // Settings are declared in English, like the labels of the table.
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        declarations.push(RiceDeclaration::new(
            sparse_setting(section),
            RiceParameter::Text {
                value: String::new(),
            },
            tr!("shortcuts-custom"),
        ));
    }
    declarations
}

/// The chords that run `command` now: its own setting, else what
/// `sparse` has for it, else `defaults`.
#[must_use]
pub fn chords_among(
    rice: &RiceSchema,
    section: &str,
    command: &str,
    defaults: &Keybinds,
    sparse: &SparseKeys,
) -> Keybinds {
    rice.get_keybind(&setting(section, command))
        .or_else(|| sparse.get(command).cloned())
        .unwrap_or_else(|| defaults.clone())
}

/// The chords that run `shortcut` now (its setting, else the key it was
/// given among the commands without a default, else its defaults).
#[must_use]
pub fn chords(rice: &RiceSchema, section: &str, shortcut: &Shortcut) -> Keybinds {
    chords_among(
        rice,
        section,
        shortcut.command,
        &shortcut.chords,
        &SparseKeys::read(rice, section),
    )
}

/// The chords of every row of `table` as set now, in the table's order.
/// Use this for a whole table: it reads the keys of the commands without
/// a default once, not once per row.
#[must_use]
pub fn resolve(rice: &RiceSchema, section: &str, table: &[Shortcut]) -> Vec<Keybinds> {
    let sparse = SparseKeys::read(rice, section);
    table
        .iter()
        .map(|shortcut| chords_among(rice, section, shortcut.command, &shortcut.chords, &sparse))
        .collect()
}

/// Where a new set of `chords` for `command` goes: the setting to change
/// and its new value, for a `SetRiceValue` (so the change can be undone).
/// A command with a setting of its own gets it there; any other command
/// (one without a default chord, a sub tool, an Auto Action) gets it in
/// the studio's [`SparseKeys`] setting, where no chords takes it out.
#[must_use]
pub fn bind(
    rice: &RiceSchema,
    section: &str,
    command: &str,
    chords: &Keybinds,
) -> (String, RiceValue) {
    let own = setting(section, command);
    if rice.get_keybind(&own).is_some() {
        return (own, RiceValue::Keybind(chords.clone()));
    }
    let mut sparse = SparseKeys::read(rice, section);
    sparse.set(command, chords.clone());
    (sparse_setting(section), RiceValue::Text(sparse.to_string()))
}

/// The changes that move keys out of the [`SparseKeys`] setting for
/// commands that have a setting of their own by now (a command that gained
/// a default chord in a newer build): the key someone gave it is kept, in
/// its own setting. Empty when there is nothing to move. Each pair is a
/// setting and its new value, to be set in this order.
#[must_use]
pub fn adopt(rice: &RiceSchema, section: &str) -> Vec<(String, RiceValue)> {
    let mut sparse = SparseKeys::read(rice, section);
    let moved: Vec<(String, Keybinds)> = sparse
        .iter()
        .filter(|(command, _)| rice.get_keybind(&setting(section, command)).is_some())
        .map(|(command, chords)| (command.to_owned(), chords.clone()))
        .collect();
    if moved.is_empty() {
        return Vec::new();
    }
    let mut changes = Vec::new();
    for (command, chords) in moved {
        sparse.set(&command, Keybinds::default());
        changes.push((setting(section, &command), RiceValue::Keybind(chords)));
    }
    changes.push((sparse_setting(section), RiceValue::Text(sparse.to_string())));
    changes
}

/// One chord that more than one command of a studio holds. A press of it
/// runs only the first of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The chord, as the first command that holds it writes it.
    pub chord: Keybind,
    /// The commands that hold it, in the table's order.
    pub commands: Vec<String>,
}

/// A chord as a key press tells them apart: Ctrl and Super are one (the
/// command key), as [`keys::consume_chord`] matches them. `None` for a
/// key no press can be.
fn press_of(chord: &Keybind) -> Option<(bool, bool, bool, egui::Key)> {
    keys::egui_key(chord.key).map(|key| {
        (
            chord.modifiers.ctrl || chord.modifiers.super_key,
            chord.modifiers.alt,
            chord.modifiers.shift,
            key,
        )
    })
}

/// The chords more than one of `rows` (a command and its chords) hold, in
/// the order the chords first appear.
#[must_use]
pub fn conflicts_among(rows: &[(&str, Keybinds)]) -> Vec<Conflict> {
    let mut found: Vec<Conflict> = Vec::new();
    let mut places: HashMap<(bool, bool, bool, egui::Key), usize> = HashMap::new();
    for (command, chords) in rows {
        for chord in chords.chords() {
            let Some(press) = press_of(chord) else {
                continue;
            };
            let place = *places.entry(press).or_insert_with(|| {
                found.push(Conflict {
                    chord: *chord,
                    commands: Vec::new(),
                });
                found.len() - 1
            });
            let holders = &mut found[place].commands;
            if !holders.iter().any(|holder| holder == command) {
                holders.push((*command).to_owned());
            }
        }
    }
    found.retain(|conflict| conflict.commands.len() > 1);
    found
}

/// The chords that more than one command of `table` holds as the keys are
/// set now, for marks in Settings.
#[must_use]
pub fn conflicts(rice: &RiceSchema, section: &str, table: &[Shortcut]) -> Vec<Conflict> {
    let rows: Vec<(&str, Keybinds)> = table
        .iter()
        .map(|shortcut| shortcut.command)
        .zip(resolve(rice, section, table))
        .collect();
    conflicts_among(&rows)
}

/// Consumes this frame's presses of `table`'s chords, as they are set now,
/// and returns the commands they run, in the table's order.
pub fn take(
    input: &mut InputState,
    rice: &RiceSchema,
    section: &str,
    table: &[Shortcut],
) -> Vec<&'static str> {
    take_resolved(input, table, &resolve(rice, section, table))
}

/// [`take`] with the chords of each row already at hand.
fn take_resolved(
    input: &mut InputState,
    table: &[Shortcut],
    chords: &[Keybinds],
) -> Vec<&'static str> {
    table
        .iter()
        .zip(chords)
        .filter(|(_, chords)| !chords.is_empty() && keys::consume(input, chords))
        .map(|(shortcut, _)| shortcut.command)
        .collect()
}

/// Each shortcut's label and its chords as set now, for a Keyboard
/// Shortcuts list.
#[must_use]
pub fn rows(rice: &RiceSchema, section: &str, table: &[Shortcut]) -> Vec<(String, String)> {
    table
        .iter()
        .zip(resolve(rice, section, table))
        .map(|(shortcut, chords)| {
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
            (label(section, shortcut), text)
        })
        .collect()
}

/// What `shortcut` is called in the language in use: its setting's
/// description in the language database (`rice` namespace), else its
/// English label.
#[must_use]
pub fn label(section: &str, shortcut: &Shortcut) -> String {
    let key = format!(
        "{}.description",
        setting(section, shortcut.command).replace('.', "-")
    );
    sue_lang::text_or("rice", &key, &shortcut.label)
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

/// The commands every studio hands to the shell, and the shell's own key
/// settings that run them (`[keys]` of the rice).
const SHELL_KEYS: [(&str, &[&str]); 5] = [
    ("edit.undo", &["keys.undo"]),
    ("edit.redo", &["keys.redo", "keys.redo_alt"]),
    ("file.open", &["keys.open_file"]),
    ("file.save", &["keys.save"]),
    ("app.quit", &["keys.quit"]),
];

/// The chords the shell runs `command` with now, or `None` when it is not
/// one of the shell's commands (or the shell's keys are not in `rice`).
fn shell_chords(rice: &RiceSchema, command: &str) -> Option<Vec<Keybind>> {
    let (_, settings) = SHELL_KEYS.iter().find(|(name, _)| *name == command)?;
    let found: Vec<Keybinds> = settings
        .iter()
        .filter_map(|setting| rice.get_keybind(setting))
        .collect();
    if found.is_empty() {
        return None;
    }
    Some(
        found
            .iter()
            .flat_map(|chords| chords.chords().iter().copied())
            .collect(),
    )
}

/// Writes each command's first chord, as set now, beside its menu item,
/// so menus show what Settings has. Undo, Redo, Open, Save and Quit, when
/// the studio's table does not have them, show the shell's own keys
/// (`keys.undo` and so on): the key the item already shows if it still
/// runs the command, else the first that does.
pub fn show_in_menus(menus: &mut [Menu], rice: &RiceSchema, section: &str, table: &[Shortcut]) {
    show_resolved(menus, rice, table, &resolve(rice, section, table));
}

/// [`show_in_menus`] with the chords of each row already at hand.
fn show_resolved(menus: &mut [Menu], rice: &RiceSchema, table: &[Shortcut], chords: &[Keybinds]) {
    fn walk(items: &mut [MenuItem], shown: &dyn Fn(&str, &str) -> Option<String>) {
        for item in items {
            match item {
                MenuItem::Action {
                    command, shortcut, ..
                } => {
                    if let Some(text) = shown(command, shortcut) {
                        *shortcut = text;
                    }
                }
                MenuItem::Sub { items, .. } => walk(items, shown),
                _ => {}
            }
        }
    }
    let rows: HashMap<&str, &Keybinds> = table
        .iter()
        .zip(chords)
        .rev()
        .map(|(shortcut, chords)| (shortcut.command, chords))
        .collect();
    // A chord of the studio's own wins over the shell's while it is open.
    let own: std::collections::HashSet<_> = chords
        .iter()
        .flat_map(|chords| chords.chords().iter().filter_map(press_of))
        .collect();
    let shown = |command: &str, now: &str| {
        if let Some(chords) = rows.get(command) {
            return Some(
                chords
                    .first()
                    .map(|chord| chord.to_string())
                    .unwrap_or_default(),
            );
        }
        let real: Vec<Keybind> = shell_chords(rice, command)?
            .into_iter()
            .filter(|chord| press_of(chord).is_some_and(|press| !own.contains(&press)))
            .collect();
        let still_true = now
            .parse::<Keybind>()
            .is_ok_and(|shown| real.iter().any(|chord| press_of(chord) == press_of(&shown)));
        if still_true {
            return None;
        }
        Some(
            real.first()
                .map(|chord| chord.to_string())
                .unwrap_or_default(),
        )
    };
    for menu in menus {
        walk(&mut menu.items, &shown);
    }
}

/// A studio's shortcuts as set now: the chords it keeps from the shell
/// while open, and the keys its menus show. Refresh it every frame; it
/// only does work when the settings changed.
#[derive(Clone, Debug, Default)]
pub struct Bound {
    revision: Option<u64>,
    chords: Vec<Keybind>,
    /// The chords of each row of the table, as of `revision`.
    rows: Vec<Keybinds>,
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
        self.rows = resolve(rice, section, table);
        show_resolved(menus, rice, table, &self.rows);
        self.chords = self
            .rows
            .iter()
            .flat_map(|chords| chords.chords().to_vec())
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

    /// As [`take`], reading the settings only when they changed since the
    /// last [`Self::refresh`]: what a studio with a row for every command
    /// calls each frame.
    pub fn take(
        &self,
        input: &mut InputState,
        rice: &RiceSchema,
        section: &str,
        table: &[Shortcut],
    ) -> Vec<&'static str> {
        if self.revision == Some(rice.revision()) && self.rows.len() == table.len() {
            take_resolved(input, table, &self.rows)
        } else {
            take(input, rice, section, table)
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

    fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn binds(text: &str) -> Keybinds {
        text.parse().unwrap()
    }

    /// A studio's table as a registry makes it: a row for every command,
    /// most of them without a default chord.
    fn every_command() -> Vec<Shortcut> {
        vec![
            Shortcut::new("file.save_as", "Save As", "Ctrl+Shift+S"),
            Shortcut::new("layer.merge_down", "Merge Down", ""),
            Shortcut::new("view.grid", "Grid", ""),
            Shortcut::new("view.zoom_in", "Zoom In", "Ctrl+="),
            Shortcut::new("anim.next_frame", "Next Frame", ""),
        ]
    }

    #[test]
    fn tables_whose_commands_all_have_defaults_declare_what_they_did() {
        let table = vec![
            Shortcut::new("file.save_as", "Save As", "Ctrl+Shift+S"),
            Shortcut::new("view.zoom_in", "Zoom In", "Ctrl+="),
        ];
        let declared: Vec<String> = declarations("demo_keys", &table)
            .into_iter()
            .map(|declaration| declaration.path)
            .collect();
        assert_eq!(
            declared,
            ["demo_keys.file_save_as", "demo_keys.view_zoom_in"]
        );
    }

    #[test]
    fn commands_without_a_default_share_one_setting() {
        let table = every_command();
        let declared = declarations("demo_keys", &table);
        let paths: Vec<&str> = declared
            .iter()
            .map(|declaration| declaration.path.as_str())
            .collect();
        // Two settings for the two defaults and one for all the rest, not
        // one empty line per command.
        assert_eq!(
            paths,
            [
                "demo_keys.file_save_as",
                "demo_keys.view_zoom_in",
                "demo_keys.custom"
            ]
        );
        assert_eq!(sparse_setting("demo_keys"), "demo_keys.custom");
        assert_eq!(
            declared[2].default,
            RiceParameter::Text {
                value: String::new()
            }
        );
        assert!(
            sue_lang::has("wm", "shortcuts-custom"),
            "the setting has English words"
        );
        assert_eq!(
            declared[2].description,
            "Keys of commands that have no default key"
        );
        let mut rice = RiceSchema::new();
        rice.register_all(&declared).unwrap();
        // Every row has chords to read: none yet for the three.
        let now = resolve(&rice, "demo_keys", &table);
        assert_eq!(now.len(), 5);
        assert_eq!(now[0], binds("Ctrl+Shift+S"));
        assert!(now[1].is_empty() && now[2].is_empty() && now[4].is_empty());
        assert_eq!(rows(&rice, "demo_keys", &table).len(), 5);
    }

    #[test]
    fn sparse_keys_read_and_write_as_one_text() {
        let text =
            "layer.merge_down=Ctrl+E view.grid=F7 or Ctrl+= or Shift+, anim.next=. tool.x=Ctrl++";
        let keys = SparseKeys::parse(text);
        assert_eq!(keys.len(), 4);
        assert_eq!(keys.get("layer.merge_down"), Some(&binds("Ctrl+E")));
        assert_eq!(
            keys.get("view.grid"),
            Some(&binds("F7 or Ctrl+= or Shift+,"))
        );
        assert_eq!(keys.get("anim.next"), Some(&binds(".")));
        assert_eq!(keys.get("tool.x"), Some(&binds("Ctrl++")));
        assert_eq!(keys.get("view.ruler"), None);
        assert_eq!(keys.to_string(), text);
        assert_eq!(SparseKeys::parse(&keys.to_string()), keys);
        // Keys that are a character people use to part things.
        for chord in [
            "=",
            "Ctrl+;",
            "Alt+#",
            "Shift+Space",
            "Ctrl+-",
            "+",
            "Ctrl+\\",
            "'",
        ] {
            let mut keys = SparseKeys::default();
            keys.set("a.b", binds(chord));
            keys.set("c.d", binds("F2"));
            let back = SparseKeys::parse(&keys.to_string());
            assert_eq!(back, keys, "{chord}");
            assert_eq!(back.get("a.b"), Some(&binds(chord)), "{chord}");
        }
        // Hand-edited text: stray words, a bad chord, extra spaces, a
        // name given twice (the last one counts, in the first one's place).
        let odd = SparseKeys::parse(
            "  junk a.b=Ctrl+Nonsense   c.d=F2 or or e.f=F3 or Bad or F4 =F9 c.d=F5\tor",
        );
        assert_eq!(odd.to_string(), "c.d=F5 e.f=F3 or F4");
        assert!(SparseKeys::parse("").is_empty());
        assert!(SparseKeys::parse("or or or").is_empty());
        // No chords takes a command out; the others keep their order.
        let mut keys = SparseKeys::parse("a.b=F1 c.d=F2 e.f=F3");
        keys.set("c.d", Keybinds::default());
        keys.set("x.y", Keybinds::default());
        keys.set("a.b", binds("F8 or F9"));
        assert_eq!(keys.to_string(), "a.b=F8 or F9 e.f=F3");
        assert_eq!(
            keys.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["a.b", "e.f"]
        );
    }

    #[test]
    fn keys_of_commands_without_a_default_round_trip_through_a_settings_file() {
        let table = every_command();
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        // A command without a default goes to the shared setting.
        let (path, value) = bind(&rice, "demo_keys", "view.grid", &binds("Ctrl+G or F7"));
        assert_eq!(path, "demo_keys.custom");
        assert_eq!(value, RiceValue::Text("view.grid=Ctrl+G or F7".to_owned()));
        rice.set(&path, value).unwrap();
        let (path, value) = bind(&rice, "demo_keys", "anim.next_frame", &binds("."));
        rice.set(&path, value).unwrap();
        // So does a name the table does not have (a sub tool).
        let (path, value) = bind(&rice, "demo_keys", "subtool.g_pen", &binds("Ctrl+="));
        rice.set(&path, value).unwrap();
        // A command with a default keeps its own setting.
        let (path, value) = bind(&rice, "demo_keys", "file.save_as", &binds("F12"));
        assert_eq!(path, "demo_keys.file_save_as");
        assert_eq!(value, RiceValue::Keybind(binds("F12")));
        rice.set(&path, value).unwrap();

        let written = rice.to_suerice();
        assert!(
            written.contains(
                "custom = \"view.grid=Ctrl+G or F7 anim.next_frame=. subtool.g_pen=Ctrl+=\""
            ),
            "{written}"
        );
        assert!(!written.contains("view_grid"), "{written}");
        assert!(!written.contains("layer_merge_down"), "{written}");
        assert_eq!(
            written.lines().filter(|line| line.contains(" = ")).count(),
            3,
            "three settings for five commands: {written}"
        );

        // Read back as a new run does: the file, then the declarations.
        let mut back = RiceSchema::parse(&written).unwrap();
        back.register_all(&declarations("demo_keys", &table))
            .unwrap();
        assert_eq!(
            resolve(&back, "demo_keys", &table),
            resolve(&rice, "demo_keys", &table)
        );
        assert_eq!(
            chords(&back, "demo_keys", &table[2]),
            binds("Ctrl+G or F7"),
            "view.grid"
        );
        assert_eq!(chords(&back, "demo_keys", &table[4]), binds("."));
        assert!(chords(&back, "demo_keys", &table[1]).is_empty());
        assert_eq!(chords(&back, "demo_keys", &table[0]), binds("F12"));
        assert_eq!(
            SparseKeys::read(&back, "demo_keys").get("subtool.g_pen"),
            Some(&binds("Ctrl+="))
        );
        assert_eq!(
            back.to_suerice(),
            written,
            "nothing changes on a second save"
        );
        assert_eq!(
            rows(&back, "demo_keys", &table)[2],
            ("Grid".to_owned(), "Ctrl+G, F7".to_owned())
        );

        // The key runs the command.
        let mut input = InputState::default();
        input
            .events
            .push(press(egui::Key::G, egui::Modifiers::COMMAND));
        input
            .events
            .push(press(egui::Key::Period, egui::Modifiers::NONE));
        assert_eq!(
            take(&mut input, &back, "demo_keys", &table),
            ["view.grid", "anim.next_frame"]
        );

        // Taking the key away again leaves no trace of the command.
        let (path, value) = bind(&back, "demo_keys", "view.grid", &Keybinds::default());
        back.set(&path, value).unwrap();
        assert_eq!(
            back.get_str("demo_keys.custom"),
            Some("anim.next_frame=. subtool.g_pen=Ctrl+=")
        );
        assert!(chords(&back, "demo_keys", &table[2]).is_empty());
    }

    #[test]
    fn a_command_that_gains_a_default_keeps_the_key_someone_gave_it() {
        let old = vec![
            Shortcut::new("view.grid", "Grid", ""),
            Shortcut::new("view.ruler", "Ruler", ""),
        ];
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &old)).unwrap();
        assert!(adopt(&rice, "demo_keys").is_empty());
        for (command, chord) in [("view.grid", "F7"), ("view.ruler", "F8")] {
            let (path, value) = bind(&rice, "demo_keys", command, &binds(chord));
            rice.set(&path, value).unwrap();
        }
        assert!(adopt(&rice, "demo_keys").is_empty(), "nothing to move yet");
        // A newer build ships view.grid with Ctrl+'.
        let new = vec![
            Shortcut::new("view.grid", "Grid", "Ctrl+'"),
            Shortcut::new("view.ruler", "Ruler", ""),
        ];
        let mut next = RiceSchema::parse(&rice.to_suerice()).unwrap();
        next.register_all(&declarations("demo_keys", &new)).unwrap();
        let changes = adopt(&next, "demo_keys");
        assert_eq!(
            changes,
            [
                (
                    "demo_keys.view_grid".to_owned(),
                    RiceValue::Keybind(binds("F7"))
                ),
                (
                    "demo_keys.custom".to_owned(),
                    RiceValue::Text("view.ruler=F8".to_owned())
                ),
            ]
        );
        for (path, value) in changes {
            next.set(&path, value).unwrap();
        }
        assert_eq!(
            resolve(&next, "demo_keys", &new),
            [binds("F7"), binds("F8")]
        );
        assert!(adopt(&next, "demo_keys").is_empty());
    }

    #[test]
    fn one_chord_held_by_two_commands_is_reported() {
        let table = vec![
            Shortcut::new("tool.rotate", "Rotate Tool", "R"),
            Shortcut::new("brush.bigger", "Bigger Brush", "]"),
            Shortcut::new("view.grid", "Grid", ""),
            Shortcut::new("file.save", "Save", "Ctrl+S or Super+S"),
        ];
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        assert!(
            conflicts(&rice, "demo_keys", &table).is_empty(),
            "Ctrl+S and Super+S of one command are no conflict"
        );
        // Bigger Brush on R, and the Grid on Ctrl+S, with Super for Ctrl.
        rice.set(
            "demo_keys.brush_bigger",
            RiceValue::Keybind(binds("] or R")),
        )
        .unwrap();
        let (path, value) = bind(&rice, "demo_keys", "view.grid", &binds("Super+S"));
        rice.set(&path, value).unwrap();
        assert_eq!(
            conflicts(&rice, "demo_keys", &table),
            [
                Conflict {
                    chord: "R".parse().unwrap(),
                    commands: vec!["tool.rotate".to_owned(), "brush.bigger".to_owned()],
                },
                Conflict {
                    chord: "Super+S".parse().unwrap(),
                    commands: vec!["view.grid".to_owned(), "file.save".to_owned()],
                },
            ]
        );
        // What the report is for: only the first row gets the key today.
        let mut input = InputState::default();
        input
            .events
            .push(press(egui::Key::R, egui::Modifiers::NONE));
        assert_eq!(
            take(&mut input, &rice, "demo_keys", &table),
            ["tool.rotate"]
        );
        // Once R is cleared from Rotate it is no conflict, and it works.
        rice.set("demo_keys.tool_rotate", RiceValue::Keybind(binds("")))
            .unwrap();
        assert_eq!(conflicts(&rice, "demo_keys", &table).len(), 1);
        input
            .events
            .push(press(egui::Key::R, egui::Modifiers::NONE));
        assert_eq!(
            take(&mut input, &rice, "demo_keys", &table),
            ["brush.bigger"]
        );
        // Different modifiers are different chords.
        let apart = [("a", binds("Ctrl+R")), ("b", binds("Ctrl+Shift+R"))];
        assert!(conflicts_among(&apart).is_empty());
    }

    fn shown(menus: &[Menu], wanted: &str) -> String {
        fn find(items: &[MenuItem], wanted: &str) -> Option<String> {
            items.iter().find_map(|item| match item {
                MenuItem::Action {
                    command, shortcut, ..
                } if command == wanted => Some(shortcut.clone()),
                MenuItem::Sub { items, .. } => find(items, wanted),
                _ => None,
            })
        }
        menus
            .iter()
            .find_map(|menu| find(&menu.items, wanted))
            .unwrap_or_else(|| panic!("no menu item runs {wanted}"))
    }

    fn shell_menus() -> Vec<Menu> {
        vec![
            Menu::new(
                "File",
                vec![
                    MenuItem::action("Open...", "file.open").key("Ctrl+O"),
                    MenuItem::action("Save", "file.save").key("Ctrl+S"),
                    MenuItem::action("Save As...", "file.save_as"),
                    MenuItem::action("Import...", "file.import").key("Ctrl+I"),
                    MenuItem::action("Quit", "app.quit").key("Ctrl+Q"),
                ],
            ),
            Menu::new(
                "Edit",
                vec![
                    MenuItem::action("Undo", "edit.undo").key("Ctrl+Z"),
                    MenuItem::action("Redo", "edit.redo").key("Ctrl+Y"),
                ],
            ),
        ]
    }

    #[test]
    fn menus_show_the_shells_keys_for_the_shells_commands() {
        let table = vec![Shortcut::new("file.save_as", "Save As", "Ctrl+Shift+S")];
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        let mut menus = shell_menus();
        show_in_menus(&mut menus, &rice, "demo_keys", &table);
        // With the keys as shipped the menus read as they were written:
        // Ctrl+Y is the shell's second key for Redo and still true.
        assert_eq!(shown(&menus, "file.open"), "Ctrl+O");
        assert_eq!(shown(&menus, "file.save"), "Ctrl+S");
        assert_eq!(shown(&menus, "app.quit"), "Ctrl+Q");
        assert_eq!(shown(&menus, "edit.undo"), "Ctrl+Z");
        assert_eq!(shown(&menus, "edit.redo"), "Ctrl+Y");
        assert_eq!(shown(&menus, "file.save_as"), "Ctrl+Shift+S");
        assert_eq!(
            shown(&menus, "file.import"),
            "Ctrl+I",
            "a command of neither keeps its text"
        );

        // Someone changes the shell's keys in Settings.
        for (path, chords) in [
            ("keys.undo", "Alt+Backspace or Ctrl+Z"),
            ("keys.redo_alt", "F4"),
            ("keys.open_file", "Ctrl+Shift+O"),
            ("keys.save", ""),
            ("keys.quit", "Alt+F4 or Ctrl+Q"),
        ] {
            rice.set(path, RiceValue::Keybind(binds(chords))).unwrap();
        }
        show_in_menus(&mut menus, &rice, "demo_keys", &table);
        assert_eq!(shown(&menus, "edit.undo"), "Ctrl+Z", "still one of them");
        assert_eq!(shown(&menus, "app.quit"), "Ctrl+Q", "still one of them");
        assert_eq!(
            shown(&menus, "edit.redo"),
            "Ctrl+Shift+Z",
            "Ctrl+Y is gone, so the first key that works"
        );
        assert_eq!(shown(&menus, "file.open"), "Ctrl+Shift+O");
        assert_eq!(shown(&menus, "file.save"), "", "no key runs it now");
    }

    #[test]
    fn a_studios_own_key_wins_over_the_shells_in_its_menus() {
        // The studio has Undo in its own table, and gives Ctrl+Q to a
        // command of its own (the shell then never sees that chord).
        let table = vec![
            Shortcut::new("edit.undo", "Undo", "Ctrl+Alt+Z"),
            Shortcut::new("view.quick_mask", "Quick Mask", "Ctrl+Q"),
        ];
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        let mut menus = shell_menus();
        let mut bound = Bound::default();
        bound.refresh(&mut menus, &rice, "demo_keys", &table);
        assert_eq!(shown(&menus, "edit.undo"), "Ctrl+Alt+Z");
        assert_eq!(shown(&menus, "app.quit"), "", "the studio holds Ctrl+Q");
        assert_eq!(shown(&menus, "edit.redo"), "Ctrl+Y");
        // A rice without the shell's keys (a test's) changes nothing.
        let mut bare = RiceSchema::new();
        bare.register_all(&declarations("demo_keys", &table))
            .unwrap();
        let mut menus = shell_menus();
        show_in_menus(&mut menus, &bare, "demo_keys", &table);
        assert_eq!(shown(&menus, "app.quit"), "Ctrl+Q");
        assert_eq!(shown(&menus, "file.save"), "Ctrl+S");
    }

    #[test]
    fn a_bound_table_takes_keys_as_the_settings_have_them() {
        let table = every_command();
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations("demo_keys", &table))
            .unwrap();
        let mut menus = vec![Menu::new(
            "View",
            vec![MenuItem::action("Grid", "view.grid").key("Ctrl+'")],
        )];
        let mut bound = Bound::default();
        let mut input = InputState::default();
        let zoom = || press(egui::Key::Equals, egui::Modifiers::COMMAND);
        // Before the first refresh it reads the settings itself.
        input.events.push(zoom());
        assert_eq!(
            bound.take(&mut input, &rice, "demo_keys", &table),
            ["view.zoom_in"]
        );
        bound.refresh(&mut menus, &rice, "demo_keys", &table);
        assert_eq!(shown(&menus, "view.grid"), "", "it has a row and no key");
        assert_eq!(bound.chords(&table).len(), 2);
        input.events.push(zoom());
        assert_eq!(
            bound.take(&mut input, &rice, "demo_keys", &table),
            ["view.zoom_in"]
        );
        assert!(input.events.is_empty(), "the press was consumed");
        // A key given since the refresh works at once, and after it.
        let (path, value) = bind(&rice, "demo_keys", "view.grid", &binds("F7"));
        rice.set(&path, value).unwrap();
        input
            .events
            .push(press(egui::Key::F7, egui::Modifiers::NONE));
        assert_eq!(
            bound.take(&mut input, &rice, "demo_keys", &table),
            ["view.grid"]
        );
        bound.refresh(&mut menus, &rice, "demo_keys", &table);
        assert_eq!(shown(&menus, "view.grid"), "F7");
        assert_eq!(bound.chords(&table).len(), 3);
        input
            .events
            .push(press(egui::Key::F7, egui::Modifiers::NONE));
        assert_eq!(
            bound.take(&mut input, &rice, "demo_keys", &table),
            ["view.grid"]
        );
    }
}
