//! Tool Property for the Figure tool's own settings, shown before its
//! brush: a box shape's Roundness of corner, Line/Fill and Aspect, a
//! polygon's corners, and how saturated and speed lines are laid.

use egui::Ui;

use super::{Category, Rows};
use crate::tools::{POLYGON, SATURATED_LINES, SPEED_LINES, SubTool};

/// The Figure sub tool's own rows.
pub(super) fn figure_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    match sub.mode {
        SATURATED_LINES | SPEED_LINES => effect_rows(ui, rows, sub),
        1 | 2 | 3 | 4 | POLYGON => box_rows(ui, rows, sub),
        _ => {}
    }
}

/// A rectangle's, ellipse's or polygon's rows.
fn box_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    if matches!(sub.mode, 1 | 3 | POLYGON) {
        rows.slider(
            ui,
            "figure-roundness",
            Category::Operation,
            &tr!("roundness-of-corner"),
            &mut sub.roundness,
            0.0..=500.0,
            &tr!("px"),
            false,
        );
    }
    if sub.mode == POLYGON {
        rows.slider_u32(
            ui,
            "figure-corners",
            Category::Operation,
            &tr!("number-of-corners"),
            &mut sub.corners,
            3..=32,
        );
    }
    // The filled rectangle and ellipse only fill; the others choose.
    if matches!(sub.mode, 1 | 2 | POLYGON) {
        rows.choice_u8(
            ui,
            "figure-line-fill",
            Category::Operation,
            &tr!("line-fill"),
            &mut sub.line_fill,
            &[
                tr!("create-line"),
                tr!("create-fill"),
                tr!("create-line-and-fill"),
            ],
        );
    }
    super::select::aspect_rows(ui, rows, &mut sub.aspect);
}

/// Saturated and speed lines: how many, how long, curved, bunched, and
/// how far past where they aim they end.
fn effect_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let effect = &mut sub.effect;
    rows.slider_u32(
        ui,
        "effect-count",
        Category::Operation,
        &tr!("number-of-lines"),
        &mut effect.count,
        4..=800,
    );
    let mut length = effect.length * 100.0;
    if rows.slider(
        ui,
        "effect-length",
        Category::Operation,
        &tr!("effect-length"),
        &mut length,
        5.0..=100.0,
        " %",
        false,
    ) {
        effect.length = length / 100.0;
    }
    rows.percent(
        ui,
        "effect-jitter",
        Category::Operation,
        &tr!("effect-length-jitter"),
        &mut effect.jitter,
        0.0,
        100.0,
    );
    let mut curved = effect.curve != 0.0;
    if rows.check(
        ui,
        "effect-make-curve",
        Category::Operation,
        &tr!("make-curve"),
        Some(&tr!("make-curve-hint")),
        &mut curved,
    ) {
        effect.curve = if curved { 0.03 } else { 0.0 };
    }
    if curved {
        let mut bend = effect.curve * 100.0;
        if rows.slider(
            ui,
            "effect-curve",
            Category::Operation,
            &tr!("curve"),
            &mut bend,
            -30.0..=30.0,
            " %",
            false,
        ) {
            // Never exactly straight while Make curve is on.
            effect.curve = if bend.abs() < 0.5 {
                0.005
            } else {
                bend / 100.0
            };
        }
    }
    rows.slider_u32(
        ui,
        "effect-grouping",
        Category::Operation,
        &tr!("grouping"),
        &mut effect.group,
        1..=20,
    );
    rows.percent(
        ui,
        "effect-spread",
        Category::Operation,
        &if sub.mode == SPEED_LINES {
            tr!("effect-spread-across")
        } else {
            tr!("gap-from-reference-position")
        },
        &mut effect.spread,
        0.0,
        100.0,
    );
    rows.hint(
        ui,
        &if sub.mode == SPEED_LINES {
            tr!("effect-speed-hint")
        } else {
            tr!("effect-saturated-hint")
        },
    );
}
