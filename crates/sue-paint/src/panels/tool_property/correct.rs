//! Tool Property for the Correct Line tool.

use egui::Ui;
use sue_wm::Theme;

use super::small_text;
use crate::panels::slider_row;
use crate::tools::SubTool;

/// Correct Line's sub tools: control points, pinch, simplify, connect,
/// line width, redraw and dust.
pub(super) fn correct_line_properties(ui: &mut Ui, sub: &mut SubTool, theme: &Theme) {
    let small = small_text(theme);
    let size = |ui: &mut Ui, sub: &mut crate::tools::SubTool| {
        slider_row(
            ui,
            &tr!("brush-size"),
            &mut sub.tip.size,
            1.0..=500.0,
            &tr!("px"),
            true,
            theme,
        );
    };
    let levels = |ui: &mut Ui, name: &str, level: &mut u32| {
        ui.horizontal(|ui| {
            ui.label(small(name));
            for value in 1..=5 {
                ui.selectable_value(level, value, value.to_string());
            }
        });
    };
    match sub.mode {
        0 => {
            for (kind, name) in [
                tr!("move-control-points"),
                tr!("add-control-point"),
                tr!("delete-control-point"),
                tr!("switch-corner"),
                tr!("adjust-line-width"),
                tr!("adjust-opacity"),
                tr!("split-line"),
            ]
            .iter()
            .enumerate()
            {
                ui.radio_value(&mut sub.kind, kind as u8, name.as_str());
            }
            ui.label(small(&match sub.kind {
                1 => tr!("click-line-add-control-point"),
                2 => tr!("click-control-point-delete"),
                3 => tr!("click-control-point-make-corner-smooth"),
                4 => tr!("drag-control-point-up-widen-line"),
                5 => tr!("drag-control-point-up-more-opacity"),
                6 => tr!("click-line-split-two-there"),
                _ => tr!("drag-control-point-move"),
            }));
        }
        1 => {
            ui.horizontal(|ui| {
                ui.label(small(&tr!("fix-end")));
                ui.selectable_value(&mut sub.kind, 0, tr!("both-ends"));
                ui.selectable_value(&mut sub.kind, 1, tr!("free"));
            });
            levels(ui, &tr!("pinch-level"), &mut sub.level);
            slider_row(
                ui,
                &tr!("effect-range"),
                &mut sub.amount,
                1.0..=500.0,
                &tr!("px"),
                true,
                theme,
            );
            ui.label(small(&tr!("drag-line-pull-into-shape")));
        }
        2 => {
            levels(ui, &tr!("simplify"), &mut sub.level);
            ui.checkbox(&mut sub.whole, small(&tr!("delete-short-lines")));
            size(ui, sub);
        }
        3 => {
            ui.label(small(&tr!("brush-over-ends-two-lines-join")));
            size(ui, sub);
        }
        4 => {
            for (kind, name) in crate::correct::Widen::NAMES.iter().enumerate() {
                ui.radio_value(&mut sub.kind, kind as u8, crate::localize(name));
            }
            if matches!(sub.kind, 0 | 1 | 4) {
                slider_row(
                    ui,
                    &tr!("width-2"),
                    &mut sub.amount,
                    0.1..=100.0,
                    &tr!("px"),
                    true,
                    theme,
                );
            }
            ui.checkbox(&mut sub.whole, small(&tr!("process-whole-line")));
            size(ui, sub);
        }
        5 => {
            let mut level = sub.stabilization as f32;
            if slider_row(
                ui,
                &tr!("stabilization"),
                &mut level,
                0.0..=100.0,
                "",
                false,
                theme,
            ) {
                sub.stabilization = level.round() as u32;
            }
            ui.label(small(&tr!("trace-over-part-line-draw-again")));
        }
        6 => {
            size(ui, sub);
            ui.label(small(&tr!("trace-over-line-width-follows-your")));
        }
        _ => {
            slider_row(
                ui,
                &tr!("dust-size"),
                &mut sub.amount,
                1.0..=100.0,
                &tr!("px"),
                false,
                theme,
            );
            ui.label(small(&tr!("drag-box-remove-specks-click-whole")));
        }
    }
    ui.label(small(&tr!("correct-line-works-vector-layers-remove")));
}
