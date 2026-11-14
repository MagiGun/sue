//! The Animation menu's commands, and those of the timeline, the cels
//! and the light table palettes.

use sue_core::{Arena, UndoStack};
use sue_wm::{Call, Command, Facts, ItemState, World};

use super::{Basis, flag, when};
use crate::studio::SuePaint;

/// How an item for a Animation command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_anim(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Animation command looks right now.
    fn state_anim(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "anim.timeline.enable" => flag(self.timeline_on),
            "anim.onion" => flag(self.onion),
            "anim.loop" => flag(self.looping),
            "anim.light_table" => flag(self.light_table),
            "anim.light_table.color" => flag(self.light_color),
            "anim.light_table.cel" => flag(self.light_cel),
            "anim.light_table.general" => flag(self.light_general),
            "anim.light_table.opacity_target" => flag(self.opacity_general),
            "anim.light_table.tool" => flag(self.light_tool),
            "anim.cel.lock" => flag(self.cel_lock.is_some()),
            _ => when(has_doc),
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "anim", super::nothing).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("anim.timeline.new", "new-timeline-ellipsis"),
    c("anim.timeline.edit", "edit-timeline-ellipsis"),
    c("anim.exposure_sheet", "exposure-sheet-ellipsis"),
    c("anim.timeline.enable", "enable-timeline"),
    c("anim.layer.folder", "animation-folder-2"),
    c("anim.layer.camera", "camera-folder"),
    c("anim.cel.new", "new-animation-cel"),
    c("anim.cel.assign", "assign-animation-cel"),
    c("anim.key.add", "add-keyframe"),
    c("anim.key.delete", "delete-keyframe"),
    c("anim.frame.insert", "insert-frame"),
    c("anim.frame.delete", "delete-frame"),
    c("anim.onion", "enable-onion-skin"),
    c("anim.onion_settings", "onion-skin-settings-ellipsis"),
    c("anim.light_table", "enable-light-table"),
    c("anim.light_table.register", "register-selected-layer"),
    c(
        "anim.light_table.clear",
        "deregister-all-images-light-table",
    ),
    c("anim.play", "play-stop"),
    c("anim.first", "go-to-start"),
    c("anim.last", "go-to-end"),
    c("anim.prev", "go-to-previous-frame"),
    c("anim.next", "go-to-next-frame"),
    c("anim.loop", "loop"),
    c("anim.camera.key", "camera-keyframe"),
    c("anim.camera.unkey", "delete-camera-keyframe"),
    c(
        "anim.light_table.reverse",
        "reverse-layers-vertically-light-table",
    ),
    c("anim.light_table.color", "color-mode"),
    c("anim.light_table.cel", "show-cel-light-table"),
    c("anim.light_table.general", "show-general-light-table"),
    c("anim.light_table.opacity_target", "switch-opacity-target"),
    c("anim.light_table.tool", "light-table-tool"),
    c(
        "anim.light_table.reset",
        "reset-position-layers-light-table",
    ),
    c("anim.light_table.file", "select-and-register-file"),
    c(
        "anim.light_table.register_general",
        "register-layer-general-light-table",
    ),
    c(
        "anim.light_table.deregister",
        "deregister-selected-light-table",
    ),
    c("anim.cel.lock", "lock-cel-editing-target"),
    c("anim.cel.prev", "select-previous-cel"),
    c("anim.cel.next", "select-next-cel"),
];
