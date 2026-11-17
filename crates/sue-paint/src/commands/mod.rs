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
mod assets;
mod canvas;
mod color;
mod edit;
mod extras;
mod figure;
mod file;
mod filter;
mod help;
mod layer;
mod select;
mod story;
pub(crate) mod subtool;
mod tool;
mod tool_palette;
mod touch;
mod view;
mod window;

pub(crate) use assets::{
    FOLDER_PREFIX, MOVE_BY_PREFIX, PALETTE_IN_FOLDER, PALETTE_ON_CANVAS, STROKE_PREFIX,
};
pub(crate) use story::{WEBTOON_READING, webtoon_right_to_left};
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
        registry.add(extras::COMMANDS);
        registry.add(subtool::COMMANDS);
        registry.add(tool_palette::COMMANDS);
        registry.add(color::COMMANDS);
        registry.add(touch::COMMANDS);
        registry.add(assets::COMMANDS);
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
        // A lifted stroke its worker is still laying lands first, so the
        // command finds the picture and the history in order.
        if self.engine_ending() {
            self.engine_settle(cx);
        }
        // Words being typed are kept before anything else happens.
        self.finish_typing(cx);
        // A tool's command (a key, a script or Auto Actions) picks the
        // tool; while its key is held down, only until the key comes up.
        let before = self.tool_state();
        if command.starts_with("tool.") && self.tool_key(command, cx) {
            self.hold_tool_key(command, before);
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
        if command.starts_with("anim.") && self.anim_command(command, cx) {
            return;
        }
        if !self.state(command, &Facts::of(cx)).enabled {
            return;
        }
        let recorded = cx.history.len();
        let had_dialog = self.dialog.is_some();
        crate::commands::registry().run(self, cx, command);
        // A step being recorded that opened a dialog: what the dialog
        // ends with goes with it.
        if !had_dialog
            && self.dialog.is_some()
            && let Some(index) = self.auto.recording
            && let Some(action) = self.auto.sets.get(index)
            && action.steps.last().map(String::as_str) == Some(command)
        {
            self.auto.dialog_step = Some(action.steps.len() - 1);
        }
        self.track_new_layers(cx, recorded);
    }

    /// A layer made while the picture has a timeline gets a track of its
    /// own in the same undo step, as Clip Studio Paint's timeline lists
    /// every layer at once (A25V-5). `recorded` is how many steps the
    /// history held before: nothing is done unless a step was added.
    pub(crate) fn track_new_layers(&self, cx: &mut AppletContext<'_>, recorded: usize) {
        if cx.history.len() == recorded {
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(tracks) = crate::anim::setup::give_every_layer_a_track(&cx.world.arena, picture)
        else {
            return;
        };
        let last = cx.history.cursor();
        let Some(before) = cx.history.parent(last) else {
            return;
        };
        let label = cx.history.label(last).unwrap_or_default().to_owned();
        if Self::apply(cx, tracks) {
            fold_since(cx, before, label);
        }
    }

    /// How a menu item for `command` looks right now: grayed when it is
    /// not a command of the studio.
    pub(crate) fn state(&self, command: &str, facts: &Facts<'_>) -> ItemState {
        let registry = crate::commands::registry();
        if !registry.has(command) {
            return ItemState::off();
        }
        registry.state(self, facts, command)
    }

    /// Any palette with a switch of its own shows.
    #[cfg(test)]
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
        // A tool picked by holding its key goes back once the key is up.
        self.follow_held_tool(ctx, cx);
        // Puppet warp keeps Enter and Esc for itself.
        if ctx.text_edit_focused()
            || self.dialog.is_some()
            || self.files.dialog_open()
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
        // The keys people gave Auto Actions.
        self.auto_keys(ctx, cx);
        let registry = crate::commands::registry();
        for name in taken {
            let before = self.tool_state();
            if self.tool_key(name, cx) {
                self.hold_tool_key(name, before);
                continue;
            }
            if registry.has(name) {
                self.run(name, cx);
            } else {
                let label = registry.label(name).unwrap_or_else(|| name.to_owned());
                cx.notify(tr!("not-sue-paint-yet", label = label));
            }
        }
        // Keys people gave sub tools, kept with the other keys that ship
        // without a default.
        let picked = ctx.input_mut(|input| subtool::take(input, cx.rice()));
        for name in picked {
            self.run(&name, cx);
        }
    }
}

impl SuePaint {
    /// Keeps Window, Canvas listing the open canvases by the names their
    /// tabs show; the list's keys are written again when it changed.
    pub(crate) fn follow_canvases(&mut self, arena: &Arena) {
        let names: Vec<String> = self
            .docs
            .iter()
            .map(|doc| {
                arena
                    .get_by_id(doc.picture)
                    .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned())
            })
            .collect();
        if crate::menus::set_canvases(&mut self.menus, &names) {
            self.bound.invalidate();
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
/// Folds the undo steps made after `mark` into one called `label`, so
/// they undo together.
pub(crate) fn fold_since(cx: &mut AppletContext<'_>, mark: sue_core::NodeId, label: String) {
    let mut steps = cx.history.take_since(mark);
    if steps.len() == 1 {
        cx.history.record_boxed(steps.remove(0));
    } else if !steps.is_empty() {
        let mut batch = sue_core::txn::Batch::new(label);
        for step in steps {
            batch.push(step);
        }
        cx.history.record(batch);
    }
}

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

#[cfg(test)]
mod tests;
