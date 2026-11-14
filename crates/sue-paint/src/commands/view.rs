//! The View menu's commands.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "view", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("view.reset_turn", "reset-rotation-inversion"),
    c("view.rotate_reset", "reset-rotation-2").keys("Ctrl+Shift+2"),
    c("view.rotate_90", "rotate-90"),
    c("view.rotate_180", "rotate-180"),
    c("view.rotate_270", "rotate-270"),
    c("view.flip_h", "flip-horizontal"),
    c("view.flip_v", "flip-vertical"),
    c("view.rotate_left", "rotate-left-2").keys("-"),
    c("view.rotate_right", "rotate-right-2").keys("="),
    c("view.zoom_in", "zoom-in-2").keys("Ctrl++ or Ctrl+= or Ctrl+;"),
    c("view.zoom_out", "zoom-out-2").keys("Ctrl+-"),
    c("view.actual", "zoom-100-percent").keys("Ctrl+Alt+0"),
    c("view.double", "zoom-200-percent"),
    c("view.fit", "fit-to-screen").keys("Ctrl+0"),
    c("view.reset_display", "reset-display"),
    c("view.fit_navigator", "fit-to-navigator"),
    c("view.print_size", "print-size-2"),
    c("view.scroll_bar", "scroll-bar"),
    c("view.selection", "selection-border"),
    c("view.transform_launcher", "transform-launcher"),
    c("view.text_launcher", "text-launcher"),
    c("view.object_launcher", "object-launcher"),
    c("view.grid", "grid"),
    c("view.ruler_bar", "ruler-bar").keys("Ctrl+R"),
    c("view.pixel_grid", "pixel-grid"),
    c("view.crop_frame", "crop-marks-inner-border"),
    c("view.bleed", "bleed-2"),
    c("view.page_image", "show-page-image"),
    c("view.paper", "paper-2"),
    c("view.tone_area", "show-tone-area"),
    c("view.snap_ruler", "snap-to-ruler").keys("Ctrl+1"),
    c("view.snap_special", "snap-to-special-ruler").keys("Ctrl+2"),
    c("view.snap_grid", "snap-to-grid").keys("Ctrl+3"),
    c("view.profile_preview", "preview"),
    c("view.profile_settings", "preview-settings-ellipsis"),
];
