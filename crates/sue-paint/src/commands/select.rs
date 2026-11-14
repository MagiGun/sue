//! The Select menu's commands.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "select", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("select.from_layer", "create-selection-from-layer"),
    c("select.all", "select-all").keys("Ctrl+A"),
    c("select.none", "deselect").keys("Ctrl+D"),
    c("select.again", "reselect").keys("Ctrl+Shift+D"),
    c("select.invert", "invert-selected-area-2").keys("Ctrl+Shift+I or Shift+F7"),
    c("select.expand", "expand-selected-area-ellipsis"),
    c("select.shrink", "shrink-selected-area-ellipsis"),
    c("select.feather", "blur-border-ellipsis"),
    c("select.gamut", "select-color-gamut-ellipsis"),
    c("select.add_layer", "add-selection"),
    c("select.subtract_layer", "remove-selection"),
    c("select.to_layer", "convert-to-selection-layer"),
    c(
        "select.from_selection_layer",
        "convert-selection-layer-to-selection",
    ),
    c("select.quick_mask", "quick-mask"),
    c("select.launcher", "selection-launcher"),
];
