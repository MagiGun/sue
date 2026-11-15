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

pub(crate) use view::{ViewMarks, special_snaps};

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

impl SuePaint {
    /// Runs a command of the menus, the command bar or a shortcut.
    pub(crate) fn run(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        // Words being typed are kept before anything else happens.
        self.finish_typing(cx);
        // A tool's command (from a script or Auto Actions) picks the tool.
        if command.starts_with("tool.") && self.tool_key(command) {
            return;
        }
        // The Window menu hides a tab torn off into a palette of its own.
        if let Some(palette) = crate::dock::Palette::torn_by(command)
            && self.dock.is_open(palette)
        {
            self.close_palette(palette);
            return;
        }
        // Studio Mode's keys T1 to T7 run the commands the rice names.
        if let Some(index) = command
            .strip_prefix("touch.custom.")
            .and_then(|index| index.parse::<usize>().ok())
        {
            let custom = self.custom_command(index, cx);
            if !custom.starts_with("touch.") && !custom.is_empty() {
                self.run(&custom, cx);
            }
            return;
        }
        self.record_auto(command);
        if command.starts_with("story.") && self.story_command(command, cx) {
            return;
        }
        if (command.starts_with("anim.")
            || command.starts_with("file.export.gif")
            || command == "file.export.apng"
            || command == "file.export.webp_animation"
            || command == "file.export.exposure_sheet"
            || command == "file.import.exposure_sheet"
            || command == "file.export.sequence"
            || command == "file.export.movie"
            || command == "file.export.cels")
            && self.anim_command(command, cx)
        {
            return;
        }
        if !self.state(command, &Facts::of(cx)).enabled {
            return;
        }
        crate::commands::registry().run(self, cx, command);
    }

    /// How a menu item for `command` looks right now: grayed when it is
    /// not a command of the studio.
    pub(crate) fn state(&self, command: &str, facts: &Facts<'_>) -> ItemState {
        let registry = crate::commands::registry();
        if !registry.has(command) {
            return ItemState::off();
        }
        // A tab torn off into a palette of its own is shown while it is.
        if crate::dock::Palette::torn_by(command).is_some_and(|palette| self.dock.is_open(palette))
        {
            return ItemState::check(true);
        }
        registry.state(self, facts, command)
    }

    pub(crate) fn any_panel(&self) -> bool {
        let p = self.panels;
        p.tool
            || p.sub_tool
            || p.tool_property
            || p.brush_size
            || p.color
            || p.color_set
            || p.navigator
            || p.layer
            || p.timeline
    }

    /// The studio's shortcuts (the shell already took undo, redo, save,
    /// open, Settings and the command palette).
    pub(crate) fn keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        // Puppet warp keeps Enter and Esc for itself.
        if ctx.text_edit_focused()
            || self.dialog.is_some()
            || self.menubar.is_open()
            || self.warp.is_some()
            || self.transform.is_some()
        {
            return;
        }
        // The shortcuts as Settings has them (Clip Studio Paint's unless
        // changed), any number of chords each, tools and brush keys too.
        let bound = &self.bound;
        let taken =
            ctx.input_mut(|input| bound.take(input, cx.rice(), KEYS_SECTION, shortcut_table()));
        let registry = crate::commands::registry();
        for name in taken {
            if self.tool_key(name) {
                continue;
            }
            if registry.has(name) {
                self.run(name, cx);
            } else {
                let label = registry.label(name).unwrap_or_else(|| name.to_owned());
                cx.notify(tr!("not-sue-paint-yet", label = label));
            }
        }
    }
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
/// commands by their own match. Through the registry they do nothing,
/// as they did through the one match.
pub(super) fn nothing(_: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {}

#[cfg(test)]
mod tests;
