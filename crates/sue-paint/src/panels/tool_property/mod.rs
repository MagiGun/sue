//! The Tool Property palette.

mod brush;
mod effect;
mod three;

use egui::{RichText, Sense, Stroke, Ui, vec2};
use sue_wm::Theme;

use crate::brush::AntiAlias;
use crate::panels::slider_row;
use crate::select::Op;
use crate::studio::SuePaint;
use crate::tools::Tool;

use brush::{color_jitter_ui, dynamic_row, starting_and_ending_ui};

/// Blend's liquify sub tools (Push, Expand, Pinch and Twirl), which the
/// Liquify button shows.
pub(super) const fn is_liquify_mode(mode: u8) -> bool {
    matches!(mode, 2..=5)
}

impl SuePaint {
    /// The settings of the current sub tool, and a command they ask for.
    #[allow(clippy::too_many_lines, reason = "one block per tool")]
    pub(super) fn tool_properties(
        &mut self,
        ui: &mut Ui,
        tool: Tool,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        if tool == Tool::Operation
            && let Some(folder) = self.camera_folder(arena)
        {
            return self.camera_properties(ui, folder, arena, theme);
        }
        if tool == Tool::Operation
            && let Some(layer) = self
                .layer()
                .filter(|layer| crate::three::is_3d(arena, *layer))
        {
            self.view3d_properties(ui, layer, arena, theme);
            return None;
        }
        if tool == Tool::Operation
            && let Some(layer) = self.layer()
            && let Some(lines) = Self::effect_of(arena, layer)
        {
            self.effect_properties(ui, layer, lines, theme);
            return None;
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        let mut command = None;
        let sub = self.toolbox.current_mut();
        match tool {
            tool if tool.paints() => {
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
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        kind as u8,
                                        crate::localize(name),
                                    );
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
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
                        let chosen = tip.aa == level;
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            if chosen { theme.active } else { theme.pill },
                        );
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
                let tip = &mut sub.tip;
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
                if tool == Tool::Figure {
                    ui.label(small(&tr!("drag-canvas-draw-shape")));
                }
            }
            Tool::Fill | Tool::AutoSelect => {
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
                    ui.selectable_value(&mut sub.mode, 0, tr!("editing-layer"));
                    ui.selectable_value(&mut sub.mode, 1, tr!("all-layers"));
                    ui.selectable_value(&mut sub.mode, 2, tr!("reference-layer"));
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
                        sub.spread = spread.round() as u32;
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
                    self.select_ops(ui, theme);
                }
            }
            Tool::Gradient => {
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
            Tool::Select if matches!(sub.mode, 4 | 5) => {
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
            Tool::Select if sub.mode == 3 => {
                ui.label(small(&tr!("click-corners-click-first-again-press")));
            }
            Tool::Select => {
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("shape-2")));
                    ui.selectable_value(&mut sub.mode, 0, tr!("rectangle"));
                    ui.selectable_value(&mut sub.mode, 1, tr!("ellipse"));
                    ui.selectable_value(&mut sub.mode, 2, tr!("lasso"));
                });
                self.select_ops(ui, theme);
            }
            Tool::Eyedropper => {
                ui.selectable_value(&mut sub.mode, 0, tr!("obtain-display-color"));
                ui.selectable_value(&mut sub.mode, 1, tr!("obtain-layer-color"));
            }
            Tool::Zoom => {
                ui.selectable_value(&mut sub.mode, 0, tr!("zoom-in-on-click"));
                ui.selectable_value(&mut sub.mode, 1, tr!("zoom-out-on-click"));
                ui.label(small(&tr!("alt-click-does-the-opposite")));
            }
            Tool::Hand => {
                ui.selectable_value(&mut sub.mode, 0, tr!("scroll"));
                ui.selectable_value(&mut sub.mode, 1, tr!("rotate"));
            }
            Tool::MoveLayer => {
                ui.label(small(&tr!("drag-move-current-layer")));
            }
            Tool::Ruler => {
                match sub.mode {
                    6 => {
                        ui.label(small(&tr!("click-place-points-click-last-one")));
                        ui.label(small(&tr!("ctrl-1-snap-ruler-turns-rulers")));
                    }
                    7 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("figure")));
                            ui.selectable_value(&mut sub.kind, 0, tr!("rectangle"));
                            ui.selectable_value(&mut sub.kind, 1, tr!("ellipse"));
                        });
                        ui.label(small(&tr!("drag-lay-ruler-strokes-start-follow")));
                    }
                    8 => {
                        ui.label(small(&tr!("draw-ruler-freehand-strokes-start-follow")));
                    }
                    5 => {
                        ui.label(small(&tr!("drag-lay-straight-ruler-strokes-start")));
                        ui.label(small(&tr!("ctrl-1-snap-ruler-turns-off")));
                    }
                    2 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("special-ruler")));
                            egui::ComboBox::from_id_salt("sue-paint-special-ruler")
                                .selected_text(crate::localize(
                                    crate::rulers::Special::NAMES[usize::from(sub.kind.min(2))],
                                ))
                                .show_ui(ui, |ui| {
                                    for (kind, name) in
                                        crate::rulers::Special::NAMES.iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut sub.kind,
                                            kind as u8,
                                            crate::localize(name),
                                        );
                                    }
                                });
                        });
                        ui.label(small(&match sub.kind {
                            0 => tr!("drag-set-lines-direction-every-stroke"),
                            1 => tr!("click-middle-every-stroke-runs-toward"),
                            _ => tr!("click-middle-every-stroke-goes-round"),
                        }));
                        ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
                    }
                    1 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("process")));
                            egui::ComboBox::from_id_salt("sue-paint-perspective-process")
                                .selected_text(if sub.kind == 1 {
                                    tr!("delete-vanishing-point-2")
                                } else {
                                    tr!("add-vanishing-point-2")
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        0,
                                        tr!("add-vanishing-point-2"),
                                    );
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        1,
                                        tr!("delete-vanishing-point-2"),
                                    );
                                });
                        });
                        ui.checkbox(&mut sub.fix_eye_level, small(&tr!("fix-eye-level")))
                            .on_hover_text(tr!("vanishing-points-slide-along-eye-level"));
                        ui.label(small(&if sub.kind == 1 {
                            tr!("click-vanishing-point-take-away")
                        } else {
                            tr!("drag-along-two-edges-meet-distance")
                        }));
                        if ui
                            .button(tr!("create-perspective-ruler-ellipsis"))
                            .clicked()
                        {
                            command = Some("layer.ruler.perspective");
                        }
                    }
                    _ => {
                        ui.label(small(&tr!("drag-canvas-lay-ruler-strokes-draw")));
                        let mut lines = sub.lines as f32;
                        if slider_row(
                            ui,
                            &tr!("number-of-lines"),
                            &mut lines,
                            2.0..=32.0,
                            "",
                            false,
                            theme,
                        ) {
                            sub.lines = lines.round() as u32;
                        }
                        ui.checkbox(&mut sub.mirror, small(&tr!("line-symmetry")))
                            .on_hover_text(tr!("every-other-line-mirrored-as-clip"));
                        ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
                    }
                }
                if ui.button(tr!("delete-ruler")).clicked() {
                    command = Some("layer.ruler.delete");
                }
            }
            Tool::CorrectLine => {
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
            Tool::Blend => {
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
                ui.label(small(&match sub.mode {
                    0 => tr!("softens-what-under-brush"),
                    1 => tr!("drags-color-along-the-stroke"),
                    2 => tr!("moves-picture-along-stroke"),
                    3 => tr!("swells-picture-brushs-middle"),
                    4 => tr!("draws-picture-into-brushs-middle"),
                    _ => tr!("turns-picture-around-brushs-middle"),
                }));
            }
            Tool::Frame => {
                ui.label(small(&if sub.mode == 1 {
                    tr!("drag-line-across-panel-divide")
                } else {
                    tr!("drag-draw-rectangle-panel")
                }));
            }
            Tool::Balloon => {
                let mut line = sub.tip.size;
                if slider_row(
                    ui,
                    &tr!("line-width-2"),
                    &mut line,
                    1.0..=20.0,
                    &tr!("px"),
                    false,
                    theme,
                ) {
                    sub.tip.size = line;
                }
                ui.label(small(&if sub.mode == 1 {
                    tr!("drag-inside-balloon-where-tail-points")
                } else {
                    tr!("drag-to-draw-a-balloon")
                }));
            }
            // Clip Studio Paint's Text tool property: the font, its size,
            // the direction and the justification, also for the words
            // being typed.
            Tool::Text => {
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("font")));
                    let shown = if sub.font.is_empty() {
                        tr!("default-font-ubuntu-light")
                    } else {
                        sub.font.clone()
                    };
                    egui::ComboBox::from_id_salt("sue-paint-text-tool-font")
                        .selected_text(shown)
                        .width(ui.available_width().max(80.0))
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut sub.font,
                                String::new(),
                                tr!("default-font-ubuntu-light"),
                            );
                            let selected = sub.font.clone();
                            if let Some(family) = sue_wm::fonts::list_ui(ui, &selected, "") {
                                sub.font = family;
                            }
                        });
                });
                slider_row(
                    ui,
                    &tr!("size-2"),
                    &mut sub.tip.size,
                    6.0..=400.0,
                    &tr!("px"),
                    true,
                    theme,
                );
                ui.horizontal(|ui| {
                    ui.radio_value(&mut sub.kind, 0, small(&tr!("pref-horizontal")));
                    ui.radio_value(&mut sub.kind, 1, small(&tr!("pref-vertical")));
                });
                ui.checkbox(&mut sub.whole, small(&tr!("centered")));
                // Clip Studio Paint's letter spacing, line spacing and
                // justification.
                slider_row(
                    ui,
                    &tr!("letter-spacing"),
                    &mut sub.tracking,
                    -0.3..=1.5,
                    &tr!("em"),
                    false,
                    theme,
                );
                let mut leading = if sub.leading > 0.0 { sub.leading } else { 1.0 };
                if slider_row(
                    ui,
                    &tr!("line-spacing"),
                    &mut leading,
                    0.5..=3.0,
                    "x",
                    false,
                    theme,
                ) {
                    sub.leading = leading;
                }
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("justify")));
                    ui.radio_value(&mut sub.justify, 0, small(&tr!("justify-start")));
                    ui.radio_value(&mut sub.justify, 1, small(&tr!("justify-middle")));
                    ui.radio_value(&mut sub.justify, 2, small(&tr!("justify-end")));
                });
                // Ruby (furigana): a reading over the selected words.
                if let Some(typing) = self.typing.as_mut() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(small(&tr!("ruby")));
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut typing.reading)
                                .hint_text(tr!("ruby-hint"))
                                .desired_width(ui.available_width()),
                        );
                        if field.gained_focus() || field.clicked() {
                            typing.reading_focus = true;
                        }
                        if field.lost_focus() {
                            if ui.input(|input| input.key_pressed(egui::Key::Enter))
                                && !typing.selected.is_empty()
                            {
                                typing.add_ruby();
                            }
                            typing.reading_focus = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        let chosen = !typing.selected.is_empty();
                        if ui
                            .add_enabled(
                                chosen && !typing.reading.trim().is_empty(),
                                egui::Button::new(small(&tr!("set-ruby"))),
                            )
                            .on_disabled_hover_text(tr!("ruby-needs-selection"))
                            .clicked()
                        {
                            typing.add_ruby();
                        }
                        if ui
                            .add_enabled(
                                chosen && typing.has_ruby(),
                                egui::Button::new(small(&tr!("remove-ruby"))),
                            )
                            .clicked()
                        {
                            typing.remove_ruby();
                        }
                    });
                }
            }
            // The Object tool: what it does on the layer in hand.
            Tool::Operation => {
                ui.label(small(&tr!("object-tool-hint")));
            }
            _ => {
                ui.label(small(&tr!("tool-arrives-later-sue-paint")));
            }
        }
        command
    }

    fn select_ops(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr!("mode"))
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            );
            for op in Op::ALL {
                ui.selectable_value(&mut self.select_op, op, op.label());
            }
        });
        ui.label(
            RichText::new(tr!("shift-adds-alt-subtracts-both-intersect"))
                .size(theme.small_size)
                .color(theme.muted),
        );
    }
}
