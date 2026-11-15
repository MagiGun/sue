//! Tool Property for the Selection tool, and the selection modes that
//! Auto Select shares.

use egui::Ui;

use super::{Category, Rows};
use crate::select::Op;
use crate::tools::SubTool;

/// The Selection tool: the brush of the selection pens, the polyline's
/// hint, or the shape and the mode.
pub(super) fn select_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
    select_op: &mut Op,
) {
    match sub.mode {
        4 | 5 => {
            rows.slider(
                ui,
                "selection-pen-size",
                Category::BrushSize,
                &tr!("brush-size"),
                &mut sub.tip.size,
                1.0..=500.0,
                &tr!("px"),
                true,
            );
            rows.hint(
                ui,
                &if sub.mode == 4 {
                    tr!("paint-what-to-select")
                } else {
                    tr!("paint-what-to-leave-out")
                },
            );
        }
        3 => {
            rows.hint(ui, &tr!("click-corners-click-first-again-press"));
        }
        _ => {
            select_ops(ui, rows, select_op);
            rows.choice_u8(
                ui,
                "selection-shape",
                Category::Operation,
                &tr!("shape-2"),
                &mut sub.mode,
                &[tr!("rectangle"), tr!("ellipse"), tr!("lasso")],
            );
        }
    }
}

/// How a new selection meets the one there: new, add, subtract or
/// intersect (Clip Studio Paint's Selection mode).
pub(super) fn select_ops(ui: &mut Ui, rows: &mut Rows<'_>, select_op: &mut Op) {
    let names: Vec<String> = Op::ALL.iter().map(|op| op.label()).collect();
    let mut chosen = Op::ALL.iter().position(|op| op == select_op).unwrap_or(0);
    if rows.choice(
        ui,
        "selection-mode",
        Category::Selection,
        &tr!("mode"),
        &mut chosen,
        &names,
    ) {
        *select_op = Op::ALL[chosen];
    }
    rows.hint(ui, &tr!("shift-adds-alt-subtracts-both-intersect"));
}
