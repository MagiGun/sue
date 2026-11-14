//! Tool Property for Fill, Auto Select and Gradient.

use egui::Ui;
use sue_wm::Theme;

use super::select::select_ops;
use super::small_text;
use crate::fill::Refer;
use crate::panels::slider_row;
use crate::select::Op;
use crate::tools::{SubTool, Tool};

/// Fill and Auto Select: the tolerance, the layers they refer to, and
/// Fill's gap closing, area scaling and opacity.
pub(super) fn fill_properties(
    ui: &mut Ui,
    tool: Tool,
    sub: &mut SubTool,
    select_op: &mut Op,
    theme: &Theme,
) {
    let small = small_text(theme);
    let mut tolerance = sub.tolerance * 100.0;
    if slider_row(
        ui,
        &tr!("tolerance"),
        &mut tolerance,
        0.0..=100.0,
        " %",
        false,
        theme,
    ) {
        sub.tolerance = tolerance / 100.0;
    }
    ui.horizontal(|ui| {
        ui.label(small(&tr!("refer")));
        // The layers looked at are the sub tool's refer setting; its mode
        // says what it does (fill, close and fill, paint unpainted area).
        let mut refer = sub.referred();
        for choice in [Refer::Editing, Refer::All, Refer::Reference] {
            ui.selectable_value(&mut refer, choice, choice.label());
        }
        if refer != sub.referred() {
            sub.set_referred(refer);
        }
    });
    if tool == Tool::Fill {
        ui.horizontal(|ui| {
            ui.label(small(&tr!("close-gap")));
            for (level, name) in [
                tr!("none"),
                tr!("weak"),
                tr!("medium"),
                tr!("strong"),
                tr!("max"),
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut sub.gap, level as u32, name.as_str());
            }
        })
        .response
        .on_hover_text(tr!("fills-hold-inside-lines-small-breaks"));
        let mut spread = sub.spread as f32;
        if slider_row(
            ui,
            &tr!("area-scaling"),
            &mut spread,
            0.0..=20.0,
            &tr!("px"),
            false,
            theme,
        ) {
            sub.spread = spread.round() as i32;
        }
    }
    if tool == Tool::Fill {
        let mut opacity = sub.tip.opacity * 100.0;
        if slider_row(
            ui,
            &tr!("opacity"),
            &mut opacity,
            0.0..=100.0,
            " %",
            false,
            theme,
        ) {
            sub.tip.opacity = opacity / 100.0;
        }
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, small(&tr!("close-gap"))),
        );
    } else {
        select_ops(ui, select_op, theme);
    }
}

/// Gradient: its shape, its colors and its opacity.
pub(super) fn gradient_properties(ui: &mut Ui, sub: &mut SubTool, theme: &Theme) {
    let small = small_text(theme);
    ui.horizontal(|ui| {
        ui.label(small(&tr!("shape-2")));
        let mut radial = sub.mode == 2;
        ui.selectable_value(&mut radial, false, tr!("linear"));
        ui.selectable_value(&mut radial, true, tr!("radial"));
        if radial {
            sub.mode = 2;
        } else if sub.mode == 2 {
            sub.mode = 0;
        }
    });
    if sub.mode != 2 {
        ui.horizontal(|ui| {
            ui.label(small(&tr!("colors-2")));
            ui.selectable_value(&mut sub.mode, 0, tr!("to-transparent"));
            ui.selectable_value(&mut sub.mode, 1, tr!("main-to-sub"));
        });
    }
    let mut opacity = sub.tip.opacity * 100.0;
    if slider_row(
        ui,
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        " %",
        false,
        theme,
    ) {
        sub.tip.opacity = opacity / 100.0;
    }
}
