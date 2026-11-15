//! Tool Property for Fill, Auto Select and Gradient.

use egui::Ui;

use super::select::select_ops;
use super::{Category, Rows};
use crate::brush::AntiAlias;
use crate::fill::{Exclude, Refer};
use crate::select::Op;
use crate::tools::{SubTool, Tool};

/// The choices of Refer multiple: every way of looking at more than the
/// editing layer, in Tool Property's order.
const MULTIPLE: [Refer; 4] = [Refer::All, Refer::Selected, Refer::Folder, Refer::Reference];

/// Fill and Auto Select, in Clip Studio Paint's order: Auto Select's
/// selection mode, then Apply to connected pixels only, Close gap,
/// Tolerance, Area scaling, Refer multiple with what it leaves out,
/// Anti-aliasing, and Fill's opacity.
pub(super) fn fill_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    tool: Tool,
    sub: &mut SubTool,
    select_op: &mut Op,
) {
    if tool == Tool::AutoSelect {
        select_ops(ui, rows, select_op);
    }
    rows.check(
        ui,
        "connected-only",
        Category::Area,
        &tr!("apply-to-connected-pixels-only"),
        Some(&tr!("apply-to-connected-pixels-only-hint")),
        &mut sub.connected_only,
    );
    let gaps = [
        tr!("none"),
        tr!("weak"),
        tr!("medium"),
        tr!("strong"),
        tr!("max"),
    ];
    let mut gap = (sub.gap as usize).min(gaps.len() - 1);
    if rows.choice(
        ui,
        "close-gap",
        Category::Area,
        &tr!("close-gap"),
        &mut gap,
        &gaps,
    ) {
        sub.gap = gap as u32;
    }
    rows.percent(
        ui,
        "tolerance",
        Category::Area,
        &tr!("tolerance"),
        &mut sub.tolerance,
        0.0,
        100.0,
    );
    let mut spread = sub.spread as f32;
    if rows.slider(
        ui,
        "area-scaling",
        Category::Area,
        &tr!("area-scaling"),
        &mut spread,
        -20.0..=20.0,
        &tr!("px"),
        false,
    ) {
        sub.spread = spread.round() as i32;
    }
    refer_rows(ui, rows, sub);
    let levels: Vec<String> = AntiAlias::ALL.iter().map(|level| level.label()).collect();
    let mut level = AntiAlias::ALL
        .iter()
        .position(|found| *found == sub.area_aa)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "area-anti-aliasing",
        Category::AntiAliasing,
        &tr!("anti-aliasing"),
        &mut level,
        &levels,
    ) {
        sub.area_aa = AntiAlias::ALL[level];
    }
    if tool == Tool::Fill {
        rows.percent(
            ui,
            "fill-opacity",
            Category::Ink,
            &tr!("opacity"),
            &mut sub.tip.opacity,
            0.0,
            100.0,
        );
    }
}

/// Refer multiple: whether more than the editing layer is looked at,
/// which layers, and the kinds of layer left out.
fn refer_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.check(
        ui,
        "refer-multiple",
        Category::Reference,
        &tr!("refer-multiple"),
        Some(&tr!("refer-multiple-hint")),
        &mut sub.refer_multiple,
    );
    if !sub.refer_multiple {
        return;
    }
    let names: Vec<String> = MULTIPLE.iter().map(|refer| refer.label()).collect();
    let mut chosen = MULTIPLE
        .iter()
        .position(|refer| *refer == sub.refer)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "refer-layers",
        Category::Reference,
        &tr!("refer"),
        &mut chosen,
        &names,
    ) {
        sub.set_referred(MULTIPLE[chosen]);
    }
    let Exclude {
        mut drafts,
        mut text,
        mut paper,
        mut locked,
    } = sub.exclude;
    rows.check(
        ui,
        "exclude-drafts",
        Category::Reference,
        &tr!("exclude-draft-layers"),
        None,
        &mut drafts,
    );
    rows.check(
        ui,
        "exclude-text",
        Category::Reference,
        &tr!("exclude-text-layers"),
        None,
        &mut text,
    );
    rows.check(
        ui,
        "exclude-paper",
        Category::Reference,
        &tr!("exclude-paper-layers"),
        None,
        &mut paper,
    );
    rows.check(
        ui,
        "exclude-locked",
        Category::Reference,
        &tr!("exclude-locked-layers"),
        None,
        &mut locked,
    );
    sub.exclude = Exclude {
        drafts,
        text,
        paper,
        locked,
    };
}

/// Gradient: its shape, its colors and its opacity.
pub(super) fn gradient_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let mut shape = usize::from(sub.mode == 2);
    if rows.choice(
        ui,
        "gradient-shape",
        Category::Operation,
        &tr!("shape-2"),
        &mut shape,
        &[tr!("linear"), tr!("radial")],
    ) {
        sub.mode = if shape == 1 { 2 } else { 0 };
    }
    if sub.mode != 2 {
        let mut colors = usize::from(sub.mode == 1);
        if rows.choice(
            ui,
            "gradient-colors",
            Category::Ink,
            &tr!("colors-2"),
            &mut colors,
            &[tr!("to-transparent"), tr!("main-to-sub")],
        ) {
            sub.mode = colors as u8;
        }
    }
    rows.percent(
        ui,
        "gradient-opacity",
        Category::Ink,
        &tr!("opacity"),
        &mut sub.tip.opacity,
        0.0,
        100.0,
    );
}
