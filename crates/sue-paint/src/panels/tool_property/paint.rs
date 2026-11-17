//! Tool Property for the tools that paint: the pens, pencils and
//! brushes, the airbrush, decoration, the eraser and the figure tools,
//! and the Blend tool beside them.

use egui::Ui;

use super::brush::{color_jitter, dynamic_row, starting_and_ending};
use super::{Category, Rows};
use crate::brush::{AntiAlias, Tip};
use crate::tools::{SubTool, Tool};

/// A painting tool's settings: its size, opacity, hardness and
/// anti-aliasing, its smoothing, then its tip.
pub(super) fn paint_properties(ui: &mut Ui, rows: &mut Rows<'_>, tool: Tool, sub: &mut SubTool) {
    if tool == Tool::Eraser {
        // How it erases lines on vector layers.
        let names = super::names(&crate::vector::Erase::NAMES);
        rows.choice_u8(
            ui,
            "vector-erase",
            Category::Operation,
            &tr!("vector"),
            &mut sub.kind,
            &names,
        );
    }
    let tip = &mut sub.tip;
    let (mut screen_size, mut at_least_one) = (tip.screen_size, tip.at_least_one);
    dynamic_row(
        ui,
        rows,
        "brush-size",
        Category::BrushSize,
        &tr!("brush-size"),
        &mut tip.size,
        0.5..=1000.0,
        &tr!("px"),
        true,
        &mut tip.size_by,
        |ui, theme| {
            ui.separator();
            for (id, label, hint, value) in [
                (
                    "screen-size",
                    tr!("specify-by-size-on-screen"),
                    tr!("brush-looks-same-size-any-zoom"),
                    &mut screen_size,
                ),
                (
                    "at-least-one",
                    tr!("at-least-1-pixel"),
                    tr!("light-pressure-never-thins-line-below"),
                    &mut at_least_one,
                ),
            ] {
                if sue_wm::ui::checkbox(("sue-paint-size-option", id), &label, *value)
                    .show(ui, theme)
                    .on_hover_text(hint)
                    .clicked()
                {
                    *value = !*value;
                }
            }
        },
    );
    tip.screen_size = screen_size;
    tip.at_least_one = at_least_one;
    let mut opacity = tip.opacity * 100.0;
    if dynamic_row(
        ui,
        rows,
        "opacity",
        Category::Ink,
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        " %",
        false,
        &mut tip.opacity_by,
        |_, _| {},
    ) {
        tip.opacity = opacity / 100.0;
    }
    if tool == Tool::Figure && sub.mode == crate::tools::LASSO_FILL {
        // How the fill mixes with what is under it.
        super::blend_mode_row(ui, rows, "lasso-fill-blend", &mut sub.blend);
    }
    let tip = &mut sub.tip;
    rows.percent(
        ui,
        "hardness",
        Category::Ink,
        &tr!("hardness"),
        &mut tip.hardness,
        0.0,
        100.0,
    );
    let levels: Vec<String> = AntiAlias::ALL.iter().map(|level| level.label()).collect();
    let mut level = AntiAlias::ALL
        .iter()
        .position(|found| *found == tip.aa)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "anti-aliasing",
        Category::AntiAliasing,
        &tr!("anti-aliasing"),
        &mut level,
        &levels,
    ) {
        tip.aa = AntiAlias::ALL[level];
    }
    rows.slider_u32(
        ui,
        "stabilization",
        Category::Correction,
        &tr!("stabilization"),
        &mut sub.stabilization,
        0..=100,
    );
    rows.check(
        ui,
        "adjust-by-speed",
        Category::Correction,
        &tr!("adjust-by-speed"),
        Some(&tr!("less-smoothing-as-pen-moves-faster")),
        &mut sub.adjust_by_speed,
    );
    rows.slider_u32(
        ui,
        "post-correction",
        Category::Correction,
        &tr!("correction"),
        &mut sub.correction,
        0..=10,
    );
    rows.check(
        ui,
        "flick",
        Category::Correction,
        &tr!("flick-into-a-point"),
        Some(&tr!("quick-stroke-carries-along-pens-momentum")),
        &mut sub.flick,
    );
    // Clip Studio Paint 5.0's Smart Shape.
    rows.check(
        ui,
        "smart-shape",
        Category::Correction,
        &tr!("smart-shape"),
        Some(&tr!("smart-shape-hint")),
        &mut sub.smart_shape,
    );
    tip_properties(ui, rows, &mut sub.tip);
    if tool == Tool::Figure {
        rows.hint(ui, &tr!("drag-canvas-draw-shape"));
    }
}

/// The brush tip: its shape and how it turns and scatters, spacing, build
/// up, color mixing, paper grain, starting and ending, and color jitter.
fn tip_properties(ui: &mut Ui, rows: &mut Rows<'_>, tip: &mut Tip) {
    let shapes = crate::brush::shapes();
    if !shapes.is_empty() || tip.shape != 0 {
        let mut ids = vec![0];
        let mut names = vec![tr!("round")];
        for (id, name) in &shapes {
            ids.push(*id);
            names.push(name.clone());
        }
        let mut chosen = ids.iter().position(|id| *id == tip.shape).unwrap_or(0);
        if rows.choice(
            ui,
            "tip-shape",
            Category::BrushTip,
            &tr!("brush-tip"),
            &mut chosen,
            &names,
        ) {
            tip.shape = ids[chosen];
        }
    }
    if tip.shape != 0 {
        // Clip Studio Paint's Direction and Spray: how each shape turns
        // and how far it falls from the line.
        rows.check(
            ui,
            "ribbon",
            Category::BrushTip,
            &tr!("lay-as-a-ribbon"),
            Some(&tr!("the-shape-bends-along-the-whole-stroke")),
            &mut tip.ribbon,
        );
        rows.check(
            ui,
            "follow-line",
            Category::BrushTip,
            &tr!("turn-along-the-line"),
            None,
            &mut tip.follow,
        );
        rows.check(
            ui,
            "follow-tilt",
            Category::BrushTip,
            &tr!("turn-with-pen-tilt"),
            Some(&tr!("a-flat-tip-turns-the-way-the-pen-leans")),
            &mut tip.follow_tilt,
        );
        rows.slider(
            ui,
            "tip-angle",
            Category::BrushTip,
            &tr!("angle-2"),
            &mut tip.angle,
            -180.0..=180.0,
            "\u{b0}",
            false,
        );
        rows.percent(
            ui,
            "scatter",
            Category::BrushTip,
            &tr!("scatter"),
            &mut tip.scatter,
            0.0,
            300.0,
        );
        rows.percent(
            ui,
            "size-jitter",
            Category::BrushTip,
            &tr!("size-jitter"),
            &mut tip.size_jitter,
            0.0,
            100.0,
        );
        rows.percent(
            ui,
            "turn-jitter",
            Category::BrushTip,
            &tr!("turn-jitter"),
            &mut tip.turn_jitter,
            0.0,
            100.0,
        );
    }
    if tip.dabbed() {
        rows.percent(
            ui,
            "spacing",
            Category::Stroke,
            &tr!("spacing"),
            &mut tip.spacing,
            1.0,
            200.0,
        );
    }
    rows.check(
        ui,
        "build-up",
        Category::Stroke,
        &tr!("build-up-airbrush"),
        None,
        &mut tip.build_up,
    );
    if tip.build_up {
        rows.percent(
            ui,
            "flow",
            Category::Stroke,
            &tr!("flow"),
            &mut tip.flow,
            1.0,
            100.0,
        );
    }
    if !tip.eraser {
        rows.percent(
            ui,
            "color-mixing",
            Category::Ink,
            &tr!("color-mixing"),
            &mut tip.mix,
            0.0,
            100.0,
        );
    }
    rows.percent(
        ui,
        "paper-grain",
        Category::Texture,
        &tr!("paper-grain"),
        &mut tip.grain,
        0.0,
        100.0,
    );
    starting_and_ending(ui, rows, tip);
    let stamps = tip.dabbed();
    color_jitter(ui, rows, &mut tip.jitter, stamps);
}

/// The Blend tool: its size and strength, and what the sub tool does.
pub(super) fn blend_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.slider(
        ui,
        "blend-size",
        Category::BrushSize,
        &tr!("brush-size"),
        &mut sub.tip.size,
        2.0..=600.0,
        &tr!("px"),
        true,
    );
    rows.percent(
        ui,
        "strength",
        Category::Ink,
        &tr!("strength"),
        &mut sub.tip.opacity,
        1.0,
        100.0,
    );
    // Liquify's mode, one of its seven (`blend::LiquifyMode`).
    if let Some(mode) = crate::blend::LiquifyMode::of_mode(sub.mode) {
        let modes = crate::blend::LiquifyMode::ALL;
        let names: Vec<String> = modes.iter().map(|found| found.label()).collect();
        let mut chosen = modes.iter().position(|found| *found == mode).unwrap_or(0);
        if rows.choice(
            ui,
            "liquify-mode",
            Category::Operation,
            &tr!("mode"),
            &mut chosen,
            &names,
        ) {
            sub.mode = modes[chosen].mode();
        }
    }
    rows.hint(
        ui,
        &match sub.mode {
            0 => tr!("softens-what-under-brush"),
            1 => tr!("drags-color-along-the-stroke"),
            2 => tr!("moves-picture-along-stroke"),
            3 => tr!("swells-picture-brushs-middle"),
            4 => tr!("draws-picture-into-brushs-middle"),
            8 | 9 => tr!("moves-picture-across-stroke"),
            crate::blend::COPY_STAMP => tr!("copies-picture-from-source"),
            _ => tr!("turns-picture-around-brushs-middle"),
        },
    );
}
