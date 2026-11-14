//! The Edit menu's commands: undo, the clipboard, fills, transforms,
//! alignment, the canvas's size and materials.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "edit", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("edit.undo", "undo"),
    c("edit.redo", "redo"),
    c("edit.cut", "cut").keys("Ctrl+X or F2"),
    c("edit.copy", "copy-2").keys("Ctrl+C or F3"),
    c("edit.paste", "paste").keys("Ctrl+V or F4"),
    c("edit.paste_here", "paste-to-shown-position"),
    c("edit.clear", "delete").keys("Ctrl+Delete or Ctrl+Backspace or Delete or Backspace"),
    c("edit.clear_outside", "delete-outside-selection"),
    c("edit.smart_smoothing", "smart-smoothing-ellipsis"),
    c("edit.fill", "fill").keys("Shift+Delete or Shift+Backspace or Alt+Delete or Alt+Backspace"),
    c("edit.advanced_fill", "advanced-fill-ellipsis"),
    c("edit.recolor", "convert-to-drawing-color"),
    c("edit.outline_selection", "outline-selection-ellipsis"),
    c("edit.luma_to_alpha", "convert-brightness-to-opacity"),
    c("edit.transform.scale", "scale-rotate-ellipsis").keys("Ctrl+T"),
    c("edit.transform.scale_only", "scale-ellipsis"),
    c("edit.transform.rotate", "rotate-ellipsis"),
    c("edit.transform.free", "free-transform").keys("Ctrl+Shift+T"),
    c("edit.transform.distort", "distort"),
    c("edit.transform.skew", "skew"),
    c("edit.transform.perspective", "perspective"),
    c("edit.flip_h", "flip-horizontal"),
    c("edit.flip_v", "flip-vertical"),
    c("edit.rotate180", "rotate-180-degrees"),
    c("edit.transform.mesh", "mesh-transformation"),
    c("edit.transform.puppet", "puppet-warp-ellipsis"),
    c("edit.align.left", "align-left"),
    c("edit.align.hcenter", "align-horizontal-center"),
    c("edit.align.right", "align-right"),
    c("edit.align.top", "align-top"),
    c("edit.align.vcenter", "align-vertical-center"),
    c("edit.align.bottom", "align-bottom"),
    c("edit.resolution", "change-image-resolution-ellipsis"),
    c("edit.canvas_size", "change-canvas-size-ellipsis"),
    c("edit.canvas_height", "change-canvas-height-ellipsis"),
    c("edit.crop", "crop"),
    c("edit.canvas_properties", "canvas-properties-ellipsis"),
    c("edit.clear_history", "this-canvas"),
    c("edit.brush_tip", "create-brush-tip-from-image-ellipsis"),
    c("edit.material.image", "image-ellipsis"),
    c("edit.material.tip", "brush-tip-ellipsis"),
    c("edit.transform.cancel", "cancel-transform").with(Flags::HIDDEN),
    c("edit.material.paste", "paste-material").with(Flags::HIDDEN),
];
