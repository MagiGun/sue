//! What the engine tells applets about the running system: the Settings
//! pages, live choices for text parameters (audio devices, applets), read-only
//! tables (models, packages, codecs) and theme presets.
//!
//! Pages are data registered at runtime, like applets (LAW 1). The Settings
//! applet renders whatever is registered, and lists any rice section no page
//! claims on a final page, so every parameter stays reachable (LAW 3).

use std::collections::BTreeMap;
use std::path::PathBuf;

use sue_rice::ThemePreset;

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
    /// The command sent to the engine.
    pub command: String,
}

impl CommandButton {
    /// A button that sends `command`.
    pub fn new(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
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
}

/// A titled group of rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsGroup {
    /// Group title.
    pub title: String,
    /// Rows.
    pub items: Vec<SettingsItem>,
}

impl SettingsGroup {
    /// A group.
    pub fn new(title: impl Into<String>, items: Vec<SettingsItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
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
        }
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
}

impl Catalog {
    /// The page with `id`.
    #[must_use]
    pub fn page(&self, id: &str) -> Option<&SettingsPage> {
        self.pages.iter().find(|page| page.id == id)
    }
}
