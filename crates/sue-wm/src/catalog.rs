//! What the engine tells applets about the running system: the Settings
//! pages, live choices for text parameters (audio devices, applets), read-only
//! tables (models, packages, codecs) and theme presets.
//!
//! Pages are data registered at runtime, like applets: Sue's own come from
//! the engine, and each studio or tool hands its own page
//! ([`crate::SueApplet::settings_page`]), with its keyboard shortcuts as a
//! [`KeyMap`] grouped by its menus. The Settings applet renders whatever is
//! registered, and lists any rice section no page claims under Advanced, so
//! every parameter stays reachable.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sue_rice::{Keybinds, ThemePreset};

use crate::shortcuts::Shortcut;
use crate::{Menu, MenuItem};

/// One selectable value of a text parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The value stored in rice.
    pub value: String,
    /// What the user sees.
    pub label: String,
    /// `false` greys the choice out (e.g. a backend this machine lacks).
    pub enabled: bool,
}

impl Choice {
    /// An enabled choice.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            enabled: true,
        }
    }

    /// The same choice, enabled or not.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A read-only table, e.g. the installed applets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InfoTable {
    /// Column titles.
    pub columns: Vec<String>,
    /// Rows of cells, one per column.
    pub rows: Vec<Vec<String>>,
    /// Shown when there are no rows.
    pub empty: String,
}

/// A command button on a Settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandButton {
    /// Button text.
    pub label: String,
    /// The command sent to the engine, or to the studio or tool
    /// [`applet`](Self::applet) names.
    pub command: String,
    /// The studio or tool whose command it is (its id): Settings closes,
    /// the studio comes to the front and runs the command, as when a
    /// button opens one of its dialogs. `None` for an engine command.
    pub applet: Option<String>,
}

impl CommandButton {
    /// A button that sends `command` to the engine.
    pub fn new(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
            applet: None,
        }
    }

    /// A button that runs `command` of the studio or tool `applet`.
    pub fn of(
        applet: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            applet: Some(applet.into()),
            ..Self::new(label, command)
        }
    }
}

/// One command of a [`KeyMap`]: what it is called and the keys it ships
/// with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCommand {
    /// The command's stable name, e.g. `layer.merge_down`.
    pub command: String,
    /// What people read, in the language in use.
    pub label: String,
    /// Its chords as Sue ships them (may be none).
    pub defaults: Keybinds,
}

/// A titled group of a [`KeyMap`]: one menu (File, Edit) or the tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyGroup {
    /// The menu's title.
    pub title: String,
    /// Its commands, in the menu's order.
    pub commands: Vec<KeyCommand>,
}

/// Every command of a studio or tool that can be given a key, grouped as
/// its menus are, for the Keyboard Shortcuts page. The keys live in the
/// rice section [`section`](Self::section), as `sue_wm::shortcuts` keeps
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyMap {
    /// The studio or tool (its id).
    pub id: String,
    /// What people read: the studio's name.
    pub title: String,
    /// The rice section of its keys, e.g. `sue_paint_keys`.
    pub section: String,
    /// Its groups, in menu order.
    pub groups: Vec<KeyGroup>,
}

impl KeyMap {
    /// Every command of every group, in order; a command in two menus
    /// comes once per menu.
    pub fn commands(&self) -> impl Iterator<Item = &KeyCommand> {
        self.groups.iter().flat_map(|group| &group.commands)
    }

    /// The command named `command`.
    #[must_use]
    pub fn find(&self, command: &str) -> Option<&KeyCommand> {
        self.commands().find(|found| found.command == command)
    }

    /// A studio's key map from its menus: a group per menu (submenus
    /// folded in, in order) holding each command of `table` the menu
    /// shows, then the commands of `table` no menu shows, grouped by the
    /// title `rest` gives the start of their name (`tool`, `brush`).
    /// `label` names a command in the language in use (its menu label
    /// when it gives none).
    #[must_use]
    pub fn from_menus(
        id: impl Into<String>,
        title: impl Into<String>,
        section: impl Into<String>,
        menus: &[Menu],
        table: &[Shortcut],
        label: impl Fn(&str) -> Option<String>,
        rest: impl Fn(&str) -> String,
    ) -> Self {
        fn walk(items: &[MenuItem], out: &mut Vec<(String, String)>) {
            for item in items {
                match item {
                    MenuItem::Action { label, command, .. } => {
                        out.push((command.clone(), label.clone()));
                    }
                    MenuItem::Sub { items, .. } => walk(items, out),
                    MenuItem::Separator => {}
                }
            }
        }
        let row = |shortcut: &Shortcut, shown: &str| KeyCommand {
            command: shortcut.command.to_owned(),
            label: label(shortcut.command)
                .unwrap_or_else(|| shown.to_owned())
                .trim_end_matches("...")
                .to_owned(),
            defaults: shortcut.chords.clone(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut groups: Vec<KeyGroup> = Vec::new();
        for menu in menus {
            let mut actions = Vec::new();
            walk(&menu.items, &mut actions);
            let mut commands: Vec<KeyCommand> = Vec::new();
            for (command, shown) in actions {
                if commands.iter().any(|known| known.command == command) {
                    continue;
                }
                if let Some(shortcut) = table.iter().find(|row| row.command == command) {
                    seen.insert(command);
                    commands.push(row(shortcut, &shown));
                }
            }
            if !commands.is_empty() {
                groups.push(KeyGroup {
                    title: menu.title.clone(),
                    commands,
                });
            }
        }
        for shortcut in table {
            if seen.contains(shortcut.command) {
                continue;
            }
            let start = shortcut.command.split('.').next().unwrap_or_default();
            let title = rest(start);
            let command = row(shortcut, &shortcut.label);
            match groups.iter_mut().find(|group| group.title == title) {
                Some(group) => group.commands.push(command),
                None => groups.push(KeyGroup {
                    title,
                    commands: vec![command],
                }),
            }
        }
        Self {
            id: id.into(),
            title: title.into(),
            section: section.into(),
            groups,
        }
    }
}

/// One row of a Settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsItem {
    /// One rice parameter.
    Param(String),
    /// Every parameter of a rice section.
    Section(String),
    /// A table from [`Catalog::tables`].
    Table(String),
    /// The theme preset switcher.
    Presets,
    /// A row of command buttons.
    Actions(Vec<CommandButton>),
    /// A fact from [`Catalog::facts`], as `label: value`.
    Fact(String),
    /// A line of plain text.
    Note(String),
    /// The keyboard shortcuts of the [`KeyMap`] with this id
    /// ([`Catalog::keymaps`]), grouped by menu, with their conflicts.
    Keys(String),
    /// The icon packs: the one in use with a preview, install, remove, the
    /// icons replaced one by one, and Sue's own set as a template.
    Icons,
    /// The optional packages (`sue_platform::packages`): each with what
    /// it is for, its size, licence and source, Install with progress
    /// and Cancel, Uninstall, and Install from a File.
    Packages,
}

/// A titled group of rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsGroup {
    /// Group title.
    pub title: String,
    /// Rows.
    pub items: Vec<SettingsItem>,
    /// Closed until someone opens it: rows most people never need.
    pub advanced: bool,
    /// Shown only while the setting at this path holds this value (the
    /// custom colors, only when the theme is Custom).
    pub when: Option<(String, String)>,
}

impl SettingsGroup {
    /// A group.
    pub fn new(title: impl Into<String>, items: Vec<SettingsItem>) -> Self {
        Self {
            title: title.into(),
            items,
            advanced: false,
            when: None,
        }
    }

    /// The same group, closed until someone opens it.
    #[must_use]
    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    /// The same group, shown only while `path` holds `value`.
    #[must_use]
    pub fn when(mut self, path: impl Into<String>, value: impl Into<String>) -> Self {
        self.when = Some((path.into(), value.into()));
        self
    }
}

/// One category of the Settings sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsPage {
    /// Stable id, used for deep links (`settings:audio`).
    pub id: String,
    /// Sidebar title.
    pub title: String,
    /// Groups, top to bottom.
    pub groups: Vec<SettingsGroup>,
    /// Other ids that open this page (ids it had before).
    pub aliases: Vec<String>,
    /// Words the Settings filter also finds a setting by, as (rice path,
    /// words): the names other programs give the same option.
    pub search: Vec<(String, String)>,
    /// The keyboard shortcuts of the page's studio or tool, shown on the
    /// Keyboard Shortcuts page rather than on this one.
    pub keys: Option<KeyMap>,
}

impl SettingsPage {
    /// A page.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        groups: Vec<SettingsGroup>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            groups,
            aliases: Vec::new(),
            search: Vec::new(),
            keys: None,
        }
    }

    /// `true` if `id` opens this page.
    #[must_use]
    pub fn answers(&self, id: &str) -> bool {
        self.id == id || self.aliases.iter().any(|alias| alias == id)
    }

    /// Every rice path or section this page shows.
    pub fn claims(&self) -> impl Iterator<Item = &SettingsItem> {
        self.groups.iter().flat_map(|group| &group.items)
    }
}

/// A labelled value the Inspector shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
}

/// A titled group of properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyGroup {
    /// Group title.
    pub title: String,
    /// Properties, top to bottom.
    pub properties: Vec<Property>,
}

impl PropertyGroup {
    /// A group from `(name, value)` pairs.
    pub fn new<N: Into<String>, V: Into<String>>(
        title: impl Into<String>,
        properties: impl IntoIterator<Item = (N, V)>,
    ) -> Self {
        Self {
            title: title.into(),
            properties: properties
                .into_iter()
                .map(|(name, value)| Property {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
        }
    }
}

/// Everything the engine publishes for applets.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    /// Settings categories, in sidebar order.
    pub pages: Vec<SettingsPage>,
    /// Live choices for text parameters, by rice path.
    pub choices: BTreeMap<String, Vec<Choice>>,
    /// Read-only tables, by id.
    pub tables: BTreeMap<String, InfoTable>,
    /// Facts, by id: `(label, value)`.
    pub facts: BTreeMap<String, (String, String)>,
    /// Theme presets.
    pub presets: Vec<ThemePreset>,
    /// Well-known folders by name: `home`, `models`, `rice`, `notes`.
    pub folders: BTreeMap<String, PathBuf>,
    /// The keyboard shortcuts of each studio and tool.
    pub keymaps: Vec<KeyMap>,
}

impl Catalog {
    /// The page `id` opens (its id or an alias).
    #[must_use]
    pub fn page(&self, id: &str) -> Option<&SettingsPage> {
        self.pages.iter().find(|page| page.answers(id))
    }

    /// The key map with `id`.
    #[must_use]
    pub fn keymap(&self, id: &str) -> Option<&KeyMap> {
        self.keymaps.iter().find(|map| map.id == id)
    }
}
