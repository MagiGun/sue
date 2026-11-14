//! Tool Property for the Selection tool, and the selection modes that
//! Auto Select shares.

use egui::{RichText, Ui};
use sue_wm::Theme;

use super::small_text;
use crate::panels::slider_row;
use crate::select::Op;
use crate::tools::SubTool;

/// The Selection tool: the brush of the selection pens, the polyline's
/// hint, or the shape and the mode.
pub(super) fn select_properties(ui: &mut Ui, sub: &mut SubTool, select_op: &mut Op, theme: &Theme) {
    let small = small_text(theme);
    match sub.mode {
        4 | 5 => {
            slider_row(
                ui,
                &tr!("brush-size"),
                &mut sub.tip.size,
                1.0..=500.0,
                &tr!("px"),
                true,
                theme,
            );
            ui.label(small(&if sub.mode == 4 {
                tr!("paint-what-to-select")
            } else {
                tr!("paint-what-to-leave-out")
            }));
        }
        3 => {
            ui.label(small(&tr!("click-corners-click-first-again-press")));
        }
        _ => {
            ui.horizontal(|ui| {
                ui.label(small(&tr!("shape-2")));
                ui.selectable_value(&mut sub.mode, 0, tr!("rectangle"));
                ui.selectable_value(&mut sub.mode, 1, tr!("ellipse"));
                ui.selectable_value(&mut sub.mode, 2, tr!("lasso"));
            });
            select_ops(ui, select_op, theme);
        }
    }
}

/// How a new selection meets the one there: new, add, subtract or
/// intersect.
pub(super) fn select_ops(ui: &mut Ui, select_op: &mut Op, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(tr!("mode"))
                .size(theme.small_size + 0.5)
                .color(theme.muted),
        );
        for op in Op::ALL {
            ui.selectable_value(select_op, op, op.label());
        }
    });
    ui.label(
        RichText::new(tr!("shift-adds-alt-subtracts-both-intersect"))
            .size(theme.small_size)
            .color(theme.muted),
    );
}
