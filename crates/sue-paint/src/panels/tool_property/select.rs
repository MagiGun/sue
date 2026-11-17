//! Tool Property for the Selection tool, and the selection modes that
//! Auto Select shares.

use egui::Ui;

use super::{Category, Rows};
use crate::brush::AntiAlias;
use crate::select::{Aspect, Op, SHRINK_SELECTION};
use crate::tools::SubTool;

/// The Selection tool: the brush of the selection pens, or the mode and
/// the shape: a rectangle or ellipse keeps its Aspect and can turn after
/// the drag (Adjust angle after fixed), and every drawn shape softens its
/// edge by its Anti-aliasing.
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
            // Full guide 00:24:07: how soft its edge is, how much it
            // selects and how steady it lays.
            rows.percent(
                ui,
                "selection-pen-hardness",
                Category::Ink,
                &tr!("hardness"),
                &mut sub.tip.hardness,
                0.0,
                100.0,
            );
            rows.percent(
                ui,
                "selection-pen-density",
                Category::Ink,
                &tr!("brush-density"),
                &mut sub.tip.opacity,
                1.0,
                100.0,
            );
            rows.slider_u32(
                ui,
                "selection-pen-stabilization",
                Category::Correction,
                &tr!("stabilization"),
                &mut sub.stabilization,
                0..=100,
            );
            rows.hint(
                ui,
                &if sub.mode == 4 {
                    tr!("paint-what-to-select")
                } else {
                    tr!("paint-what-to-leave-out")
                },
            );
            return;
        }
        3 => {
            select_ops(ui, rows, select_op);
            rows.hint(ui, &tr!("click-corners-click-first-again-press"));
        }
        SHRINK_SELECTION => {
            // A lasso of its own kind: no Shape row to turn it into
            // another sub tool.
            select_ops(ui, rows, select_op);
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
    if matches!(sub.mode, 0 | 1) {
        aspect_rows(ui, rows, &mut sub.aspect);
        rows.check(
            ui,
            "adjust-angle",
            Category::Operation,
            &tr!("adjust-angle-after-fixed"),
            Some(&tr!("adjust-angle-after-fixed-hint")),
            &mut sub.adjust_angle,
        );
    }
    let levels: Vec<String> = AntiAlias::ALL.iter().map(|level| level.label()).collect();
    let mut level = AntiAlias::ALL
        .iter()
        .position(|found| *found == sub.area_aa)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "selection-anti-aliasing",
        Category::AntiAliasing,
        &tr!("anti-aliasing"),
        &mut level,
        &levels,
    ) {
        sub.area_aa = AntiAlias::ALL[level];
    }
}

/// Aspect: any shape, a square (or circle), or a ratio of its own.
fn aspect_rows(ui: &mut Ui, rows: &mut Rows<'_>, aspect: &mut Aspect) {
    let mut chosen = match aspect {
        Aspect::Free => 0,
        Aspect::Square => 1,
        Aspect::Ratio { .. } => 2,
    };
    if rows.choice(
        ui,
        "selection-aspect",
        Category::Operation,
        &tr!("aspect"),
        &mut chosen,
        &[tr!("free"), tr!("aspect-square"), tr!("aspect-ratio")],
    ) {
        *aspect = match chosen {
            0 => Aspect::Free,
            1 => Aspect::Square,
            _ => Aspect::Ratio {
                width: 16.0,
                height: 9.0,
            },
        };
    }
    if let Aspect::Ratio { width, height } = aspect {
        rows.slider(
            ui,
            "selection-aspect-width",
            Category::Operation,
            &tr!("aspect-width"),
            width,
            1.0..=100.0,
            "",
            false,
        );
        rows.slider(
            ui,
            "selection-aspect-height",
            Category::Operation,
            &tr!("aspect-height"),
            height,
            1.0..=100.0,
            "",
            false,
        );
        *width = width.max(1.0);
        *height = height.max(1.0);
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
