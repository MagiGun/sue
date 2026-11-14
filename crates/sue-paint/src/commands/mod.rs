//! Sue Paint's commands: one registry that the menus, the shortcuts, the
//! command palette and the tests read.
//!
//! Each group's commands are a `const` table in a file of their own. A
//! command's name is saved data (shortcut settings, Auto Actions and the
//! custom touch keys hold it), so it is never renamed without a
//! migration.

use std::sync::OnceLock;

use sue_core::Arena;
use sue_model::picture::LayerKind;
use sue_wm::{AppletContext, Call, CommandRegistry, Facts, ItemState};

use crate::doc;
use crate::studio::{KEYS_SECTION, SuePaint};

mod anim;
mod canvas;
mod edit;
mod figure;
mod file;
mod filter;
mod help;
mod layer;
mod select;
mod story;
mod tool;
mod view;
mod window;

/// Commands the shell's own keys run (`keys.undo`, `keys.redo`,
/// `keys.open_file`, `keys.save`, `keys.quit`): they get no row in the
/// studio's shortcuts, so the menus show the shell's keys beside them.
pub(crate) const SHELL_OWNED: [&str; 5] = [
    "edit.undo",
    "edit.redo",
    "file.open",
    "file.save",
    "app.quit",
];

/// Every command of Sue Paint.
pub fn registry() -> &'static CommandRegistry<SuePaint> {
    static REGISTRY: OnceLock<CommandRegistry<SuePaint>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::new("paint", KEYS_SECTION);
        registry.add(file::COMMANDS);
        registry.add(edit::COMMANDS);
        registry.add(story::COMMANDS);
        registry.add(anim::COMMANDS);
        registry.add(layer::COMMANDS);
        registry.add(select::COMMANDS);
        registry.add(view::COMMANDS);
        registry.add(filter::COMMANDS);
        registry.add(window::COMMANDS);
        registry.add(help::COMMANDS);
        registry.add(canvas::COMMANDS);
        registry.add(figure::COMMANDS);
        registry.add(tool::COMMANDS);
        registry
    })
}

/// Sue Paint's shortcuts: a row for every command that can be given a
/// key, with its default chords (Clip Studio Paint's), but for the
/// commands the shell's keys run. The labels are the settings' English
/// descriptions; Settings and the Keyboard Shortcuts list read them in
/// other languages from the language database.
pub(crate) fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: OnceLock<Vec<sue_wm::shortcuts::Shortcut>> = OnceLock::new();
    TABLE.get_or_init(|| {
        registry()
            .shortcuts()
            .into_iter()
            .filter(|shortcut| !SHELL_OWNED.contains(&shortcut.command))
            .collect()
    })
}

/// What most commands' state looks at, worked out once per item.
pub(super) struct Basis {
    /// A canvas is open.
    pub has_doc: bool,
    /// The current layer.
    pub info: Option<doc::LayerInfo>,
    /// The current layer is a raster layer.
    pub raster: bool,
    /// The canvas has a selection.
    pub selection: bool,
}

impl SuePaint {
    /// The [`Basis`] of every command's state now.
    pub(super) fn basis(&self, arena: &Arena) -> Basis {
        let has_doc = self.doc().is_some();
        let info = self.layer_info(arena);
        let raster = info
            .as_ref()
            .is_some_and(|info| info.kind == LayerKind::Raster);
        let selection = self.doc().is_some_and(|doc| doc.selection.is_some());
        Basis {
            has_doc,
            info,
            raster,
            selection,
        }
    }
}

/// An item checked when `on`.
pub(super) fn flag(on: bool) -> ItemState {
    ItemState::check(on)
}

/// An item that works when `on` and is grayed otherwise.
pub(super) fn when(on: bool) -> ItemState {
    if on {
        ItemState::on()
    } else {
        ItemState::off()
    }
}

/// How an item looks for a command that works while a canvas is open.
pub(super) fn doc_open(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some())
}

/// A command the studio runs before the registry: Story and Animation
/// commands by their own match, the tool keys by `tool_key`. Through
/// the registry they do nothing, as they did through the one match.
pub(super) fn nothing(_: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {}

#[cfg(test)]
mod tests;
