//! Tool Property for the Correct Line tool.

use egui::Ui;

use super::{Category, Rows};
use crate::tools::SubTool;

/// Correct Line's sub tools: control points, pinch, simplify, connect,
/// line width, redraw and dust.
pub(super) fn correct_line_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let size = |ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool| {
        rows.slider(
            ui,
            "correct-size",
            Category::BrushSize,
            &tr!("brush-size"),
            &mut sub.tip.size,
            1.0..=500.0,
            &tr!("px"),
            true,
        );
    };
    let levels: Vec<String> = (1..=5).map(|level: u32| level.to_string()).collect();
    let level = |ui: &mut Ui, rows: &mut Rows<'_>, id: &str, name: &str, value: &mut u32| {
        let mut chosen = (*value).clamp(1, 5) as usize - 1;
        if rows.choice(ui, id, Category::Operation, name, &mut chosen, &levels) {
            *value = chosen as u32 + 1;
        }
    };
    match sub.mode {
        0 => {
            let names = [
                tr!("move-control-points"),
                tr!("add-control-point"),
                tr!("delete-control-point"),
                tr!("switch-corner"),
                tr!("adjust-line-width"),
                tr!("adjust-opacity"),
                tr!("split-line"),
            ];
            rows.choice_u8(
                ui,
                "control-point-process",
                Category::Operation,
                &tr!("process"),
                &mut sub.kind,
                &names,
            );
            rows.hint(
                ui,
                &match sub.kind {
                    1 => tr!("click-line-add-control-point"),
                    2 => tr!("click-control-point-delete"),
                    3 => tr!("click-control-point-make-corner-smooth"),
                    4 => tr!("drag-control-point-up-widen-line"),
                    5 => tr!("drag-control-point-up-more-opacity"),
                    6 => tr!("click-line-split-two-there"),
                    _ => tr!("drag-control-point-move"),
                },
            );
        }
        1 => {
            rows.choice_u8(
                ui,
                "pinch-fix-end",
                Category::Operation,
                &tr!("fix-end"),
                &mut sub.kind,
                &[tr!("both-ends"), tr!("free")],
            );
            level(ui, rows, "pinch-level", &tr!("pinch-level"), &mut sub.level);
            rows.slider(
                ui,
                "pinch-range",
                Category::Operation,
                &tr!("effect-range"),
                &mut sub.amount,
                1.0..=500.0,
                &tr!("px"),
                true,
            );
            rows.hint(ui, &tr!("drag-line-pull-into-shape"));
        }
        2 => {
            level(ui, rows, "simplify-level", &tr!("simplify"), &mut sub.level);
            rows.check(
                ui,
                "delete-short-lines",
                Category::Operation,
                &tr!("delete-short-lines"),
                None,
                &mut sub.whole,
            );
            size(ui, rows, sub);
        }
        3 => {
            rows.hint(ui, &tr!("brush-over-ends-two-lines-join"));
            size(ui, rows, sub);
        }
        4 => {
            let names = super::names(&crate::correct::Widen::NAMES);
            rows.choice_u8(
                ui,
                "line-width-process",
                Category::Operation,
                &tr!("process"),
                &mut sub.kind,
                &names,
            );
            if matches!(sub.kind, 0 | 1 | 4) {
                rows.slider(
                    ui,
                    "line-width-amount",
                    Category::Operation,
                    &tr!("width-2"),
                    &mut sub.amount,
                    0.1..=100.0,
                    &tr!("px"),
                    true,
                );
            }
            rows.check(
                ui,
                "process-whole-line",
                Category::Operation,
                &tr!("process-whole-line"),
                None,
                &mut sub.whole,
            );
            size(ui, rows, sub);
        }
        5 => {
            rows.slider_u32(
                ui,
                "redraw-stabilization",
                Category::Correction,
                &tr!("stabilization"),
                &mut sub.stabilization,
                0..=100,
            );
            rows.hint(ui, &tr!("trace-over-part-line-draw-again"));
        }
        6 => {
            size(ui, rows, sub);
            rows.hint(ui, &tr!("trace-over-line-width-follows-your"));
        }
        _ => {
            rows.slider(
                ui,
                "dust-size",
                Category::Operation,
                &tr!("dust-size"),
                &mut sub.amount,
                1.0..=100.0,
                &tr!("px"),
                false,
            );
            rows.hint(ui, &tr!("drag-box-remove-specks-click-whole"));
        }
    }
    rows.hint(ui, &tr!("correct-line-works-vector-layers-remove"));
}
