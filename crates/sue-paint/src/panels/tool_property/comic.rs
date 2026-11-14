//! Tool Property for the comic tools: Frame Border and Balloon.

use egui::Ui;
use sue_wm::Theme;

use super::small_text;
use crate::panels::slider_row;
use crate::tools::SubTool;

/// The Frame Border tool: how a panel is drawn or divided.
pub(super) fn frame_properties(ui: &mut Ui, sub: &SubTool, theme: &Theme) {
    let small = small_text(theme);
    ui.label(small(&if sub.mode == 1 {
        tr!("drag-line-across-panel-divide")
    } else {
        tr!("drag-draw-rectangle-panel")
    }));
}

/// The Balloon tool: its line width, and how a balloon or its tail is
/// drawn.
pub(super) fn balloon_properties(ui: &mut Ui, sub: &mut SubTool, theme: &Theme) {
    let small = small_text(theme);
    let mut line = sub.tip.size;
    if slider_row(
        ui,
        &tr!("line-width-2"),
        &mut line,
        1.0..=20.0,
        &tr!("px"),
        false,
        theme,
    ) {
        sub.tip.size = line;
    }
    ui.label(small(&if sub.mode == 1 {
        tr!("drag-inside-balloon-where-tail-points")
    } else {
        tr!("drag-to-draw-a-balloon")
    }));
}
