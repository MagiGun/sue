//! Tool Property for the tools that paint: the pens, pencils and
//! brushes, the airbrush, decoration, the eraser and the figure tools,
//! and the Blend tool beside them.

use egui::{Sense, Stroke, Ui, vec2};
use sue_wm::Theme;

use super::brush::{color_jitter_ui, dynamic_row, starting_and_ending_ui};
use super::small_text;
use crate::brush::{AntiAlias, Tip};
use crate::panels::slider_row;
use crate::tools::{SubTool, Tool};

/// A painting tool's settings: its size, opacity, hardness and
/// smoothing, then its tip.
pub(super) fn paint_properties(ui: &mut Ui, tool: Tool, sub: &mut SubTool, theme: &Theme) {
    let small = small_text(theme);
    // Clip Studio Paint 5.0's Smart Shape.
    ui.checkbox(&mut sub.smart_shape, small(&tr!("smart-shape")))
        .on_hover_text(tr!("smart-shape-hint"));
    if tool == Tool::Eraser {
        // How it erases lines on vector layers.
        ui.horizontal(|ui| {
            ui.label(small(&tr!("vector")));
            egui::ComboBox::from_id_salt("sue-paint-vector-eraser")
                .selected_text(crate::localize(
                    crate::vector::Erase::NAMES[usize::from(sub.kind.min(2))],
                ))
                .show_ui(ui, |ui| {
                    for (kind, name) in crate::vector::Erase::NAMES.iter().enumerate() {
                        ui.selectable_value(&mut sub.kind, kind as u8, crate::localize(name));
                    }
                });
        });
    }
    let tip = &mut sub.tip;
    let (mut screen_size, mut at_least_one) = (tip.screen_size, tip.at_least_one);
    dynamic_row(
        ui,
        &tr!("brush-size"),
        &mut tip.size,
        0.5..=1000.0,
        &tr!("px"),
        true,
        &mut tip.size_by,
        |ui| {
            ui.separator();
            ui.checkbox(&mut screen_size, small(&tr!("specify-by-size-on-screen")))
                .on_hover_text(tr!("brush-looks-same-size-any-zoom"));
            ui.checkbox(&mut at_least_one, small(&tr!("at-least-1-pixel")))
                .on_hover_text(tr!("light-pressure-never-thins-line-below"));
        },
        theme,
    );
    tip.screen_size = screen_size;
    tip.at_least_one = at_least_one;
    let mut opacity = tip.opacity * 100.0;
    if dynamic_row(
        ui,
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        " %",
        false,
        &mut tip.opacity_by,
        |_| {},
        theme,
    ) {
        tip.opacity = opacity / 100.0;
    }
    let mut hardness = tip.hardness * 100.0;
    if slider_row(
        ui,
        &tr!("hardness"),
        &mut hardness,
        0.0..=100.0,
        " %",
        false,
        theme,
    ) {
        tip.hardness = hardness / 100.0;
    }
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(78.0, 18.0),
            egui::Label::new(small(&tr!("anti-aliasing"))),
        );
        for level in AntiAlias::ALL {
            let (rect, response) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
            let chosen = tip.aa == level;
            ui.painter()
                .rect_filled(rect, 3.0, if chosen { theme.active } else { theme.pill });
            let soft = level.edge();
            let center = rect.center();
            ui.painter().circle_filled(center, 5.0, theme.text);
            if soft > 0.0 {
                ui.painter().circle_stroke(
                    center,
                    5.0 + soft,
                    Stroke::new(soft, theme.text.gamma_multiply(0.35)),
                );
            }
            if response.on_hover_text(level.label()).clicked() {
                tip.aa = level;
            }
        }
    });
    let mut stabilization = sub.stabilization as f32;
    if slider_row(
        ui,
        &tr!("stabilization"),
        &mut stabilization,
        0.0..=100.0,
        "",
        false,
        theme,
    ) {
        sub.stabilization = stabilization.round() as u32;
    }
    ui.checkbox(&mut sub.adjust_by_speed, small(&tr!("adjust-by-speed")))
        .on_hover_text(tr!("less-smoothing-as-pen-moves-faster"));
    let mut correction = sub.correction as f32;
    if slider_row(
        ui,
        &tr!("correction"),
        &mut correction,
        0.0..=10.0,
        "",
        false,
        theme,
    ) {
        sub.correction = correction.round() as u32;
    }
    ui.checkbox(&mut sub.flick, small(&tr!("flick-into-a-point")))
        .on_hover_text(tr!("quick-stroke-carries-along-pens-momentum"));
    tip_properties(ui, &mut sub.tip, theme);
    if tool == Tool::Figure {
        ui.label(small(&tr!("drag-canvas-draw-shape")));
    }
}

/// The brush tip: its shape, starting and ending, color jitter, direction
/// and spray, spacing, build up, color mixing and paper grain.
fn tip_properties(ui: &mut Ui, tip: &mut Tip, theme: &Theme) {
    let small = small_text(theme);
    let shapes = crate::brush::shapes();
    if !shapes.is_empty() || tip.shape != 0 {
        ui.horizontal(|ui| {
            ui.label(small(&tr!("brush-tip")));
            let now = shapes
                .iter()
                .find(|(id, _)| *id == tip.shape)
                .map_or_else(|| tr!("round"), |(_, name)| name.clone());
            egui::ComboBox::from_id_salt("sue-paint-tip-shape")
                .selected_text(now)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tip.shape, 0, tr!("round"));
                    for (id, name) in &shapes {
                        ui.selectable_value(&mut tip.shape, *id, name);
                    }
                });
        });
    }
    starting_and_ending_ui(ui, tip, theme);
    let stamps = tip.dabbed();
    color_jitter_ui(ui, &mut tip.jitter, stamps, theme);
    if tip.shape != 0 {
        // Clip Studio Paint's Direction and Spray: how each
        // shape turns and how far it falls from the line.
        ui.checkbox(&mut tip.ribbon, small(&tr!("lay-as-a-ribbon")))
            .on_hover_text(tr!("the-shape-bends-along-the-whole-stroke"));
        ui.checkbox(&mut tip.follow, small(&tr!("turn-along-the-line")));
        ui.checkbox(&mut tip.follow_tilt, small(&tr!("turn-with-pen-tilt")))
            .on_hover_text(tr!("a-flat-tip-turns-the-way-the-pen-leans"));
        slider_row(
            ui,
            &tr!("angle-2"),
            &mut tip.angle,
            -180.0..=180.0,
            "\u{b0}",
            false,
            theme,
        );
        let mut scatter = tip.scatter * 100.0;
        if slider_row(
            ui,
            &tr!("scatter"),
            &mut scatter,
            0.0..=300.0,
            " %",
            false,
            theme,
        ) {
            tip.scatter = scatter / 100.0;
        }
        let mut size = tip.size_jitter * 100.0;
        if slider_row(
            ui,
            &tr!("size-jitter"),
            &mut size,
            0.0..=100.0,
            " %",
            false,
            theme,
        ) {
            tip.size_jitter = size / 100.0;
        }
        let mut turn = tip.turn_jitter * 100.0;
        if slider_row(
            ui,
            &tr!("turn-jitter"),
            &mut turn,
            0.0..=100.0,
            " %",
            false,
            theme,
        ) {
            tip.turn_jitter = turn / 100.0;
        }
    }
    if tip.dabbed() {
        let mut spacing = tip.spacing * 100.0;
        if slider_row(
            ui,
            &tr!("spacing"),
            &mut spacing,
            1.0..=200.0,
            " %",
            true,
            theme,
        ) {
            tip.spacing = spacing / 100.0;
        }
    }
    ui.checkbox(&mut tip.build_up, small(&tr!("build-up-airbrush")));
    if tip.build_up {
        let mut flow = tip.flow * 100.0;
        if slider_row(ui, &tr!("flow"), &mut flow, 1.0..=100.0, " %", false, theme) {
            tip.flow = flow / 100.0;
        }
    }
    if !tip.eraser {
        let mut mix = tip.mix * 100.0;
        if slider_row(
            ui,
            &tr!("color-mixing"),
            &mut mix,
            0.0..=100.0,
            " %",
            false,
            theme,
        ) {
            tip.mix = mix / 100.0;
        }
    }
    let mut grain = tip.grain * 100.0;
    if slider_row(
        ui,
        &tr!("paper-grain"),
        &mut grain,
        0.0..=100.0,
        " %",
        false,
        theme,
    ) {
        tip.grain = grain / 100.0;
    }
    ui.add_enabled(
        false,
        egui::Checkbox::new(&mut false, small(&tr!("vector-magnet"))),
    );
}

/// The Blend tool: its size and strength, and what the sub tool does.
pub(super) fn blend_properties(ui: &mut Ui, sub: &mut SubTool, theme: &Theme) {
    let small = small_text(theme);
    slider_row(
        ui,
        &tr!("brush-size"),
        &mut sub.tip.size,
        2.0..=600.0,
        &tr!("px"),
        true,
        theme,
    );
    let mut strength = sub.tip.opacity * 100.0;
    if slider_row(
        ui,
        &tr!("strength"),
        &mut strength,
        1.0..=100.0,
        " %",
        false,
        theme,
    ) {
        sub.tip.opacity = strength / 100.0;
    }
    // Liquify's mode, one of its seven (`blend::LiquifyMode`), as choices
    // that wrap to the palette's width.
    if let Some(mode) = crate::blend::LiquifyMode::of_mode(sub.mode) {
        ui.horizontal_wrapped(|ui| {
            for found in crate::blend::LiquifyMode::ALL {
                let salt = ("sue-paint-liquify-mode", found.id());
                if sue_wm::ui::choice::radio(salt, &found.label(), found == mode)
                    .show(ui, theme)
                    .clicked()
                {
                    sub.mode = found.mode();
                }
            }
        });
    }
    ui.label(small(&match sub.mode {
        0 => tr!("softens-what-under-brush"),
        1 => tr!("drags-color-along-the-stroke"),
        2 => tr!("moves-picture-along-stroke"),
        3 => tr!("swells-picture-brushs-middle"),
        4 => tr!("draws-picture-into-brushs-middle"),
        8 | 9 => tr!("moves-picture-across-stroke"),
        _ => tr!("turns-picture-around-brushs-middle"),
    }));
}
