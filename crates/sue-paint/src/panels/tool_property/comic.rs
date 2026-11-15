//! Tool Property for the comic tools: Frame Border and Balloon.

use egui::Ui;

use super::{Category, Rows};
use crate::tools::SubTool;

/// The Frame Border tool: how a panel is drawn or divided.
pub(super) fn frame_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &SubTool) {
    rows.hint(
        ui,
        &if sub.mode == 1 {
            tr!("drag-line-across-panel-divide")
        } else {
            tr!("drag-draw-rectangle-panel")
        },
    );
}

/// The Balloon tool: its line width, and how a balloon or its tail is
/// drawn.
pub(super) fn balloon_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.slider(
        ui,
        "balloon-line-width",
        Category::BrushSize,
        &tr!("line-width-2"),
        &mut sub.tip.size,
        1.0..=20.0,
        &tr!("px"),
        false,
    );
    rows.hint(
        ui,
        &if sub.mode == 1 {
            tr!("drag-inside-balloon-where-tail-points")
        } else {
            tr!("drag-to-draw-a-balloon")
        },
    );
}
