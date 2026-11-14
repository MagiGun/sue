//! The brush rows of Tool Property: sliders with dynamics, starting and
//! ending, color jitter and the dynamics settings.

use egui::{Pos2, RichText, Sense, Stroke, Ui, pos2, vec2};
use sue_wm::Theme;

use crate::brush::{Dynamics, Jitter, TaperMode};
use crate::panels::slider_row;

/// A slider row with a dynamics button at its end (Brush Size and Opacity,
/// as in Clip Studio Paint): the button shows whether the pen changes the
/// setting and opens its dynamics. `extra` adds the setting's own options
/// under them. Returns `true` when the value changed.
#[allow(clippy::too_many_arguments, reason = "a slider row and its dynamics")]
pub(super) fn dynamic_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    log: bool,
    dynamics: &mut Dynamics,
    extra: impl FnOnce(&mut Ui),
    theme: &Theme,
) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(78.0, 18.0),
            egui::Label::new(
                RichText::new(label)
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            ),
        );
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(36.0);
        let changed = ui
            .add(
                egui::Slider::new(value, range)
                    .logarithmic(log)
                    .max_decimals(if log { 1 } else { 0 })
                    .suffix(suffix),
            )
            .changed();
        let (rect, button) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
        let on = dynamics.any();
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            3.0,
            if on {
                theme.active
            } else if button.hovered() {
                theme.hover
            } else {
                theme.pill
            },
        );
        // A little pressure graph.
        let inner = rect.shrink2(vec2(5.0, 4.0));
        let curve: Vec<Pos2> = (0..=8)
            .map(|step| {
                let t = step as f32 / 8.0;
                pos2(
                    inner.left() + t * inner.width(),
                    inner.bottom() - t * t * inner.height(),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            curve,
            Stroke::new(1.2, if on { theme.text } else { theme.muted }),
        ));
        let button = button.on_hover_text(tr!("dynamics-what-the-pen-changes", label = label));
        egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(300.0);
                dynamics_ui(ui, label, dynamics, theme);
                extra(ui);
            });
        changed
    })
    .inner
}

/// Starting and ending (Clip Studio Paint's): how they are measured, how
/// long they are, what they change, and whether speed stretches them.
pub(super) fn starting_and_ending_ui(ui: &mut Ui, tip: &mut crate::brush::Tip, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    egui::CollapsingHeader::new(small(&tr!("starting-and-ending")))
        .id_salt("starting-and-ending")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    vec2(78.0, 18.0),
                    egui::Label::new(small(&tr!("how-to-specify"))),
                );
                egui::ComboBox::from_id_salt("taper-mode")
                    .selected_text(tip.taper_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in TaperMode::ALL {
                            ui.selectable_value(&mut tip.taper_mode, mode, mode.label());
                        }
                    });
            });
            let (unit, top) = match tip.taper_mode {
                TaperMode::Percent => (" %".to_owned(), 100.0),
                TaperMode::Length | TaperMode::Fade => (tr!("px"), 400.0),
            };
            slider_row(
                ui,
                &tr!("starting-2"),
                &mut tip.taper_start,
                0.0..=top,
                &unit,
                false,
                theme,
            );
            let ending = if tip.taper_mode == TaperMode::Fade {
                tr!("fade-by")
            } else {
                tr!("ending")
            };
            slider_row(
                ui,
                &ending,
                &mut tip.taper_end,
                0.0..=top,
                &unit,
                false,
                theme,
            );
            ui.horizontal(|ui| {
                ui.checkbox(&mut tip.taper_size, small(&tr!("brush-size-2")));
                ui.checkbox(&mut tip.taper_opacity, small(&tr!("opacity")));
            });
            ui.checkbox(
                &mut tip.taper_by_speed,
                small(&tr!("starting-and-ending-by-speed")),
            )
            .on_hover_text(tr!("quicker-strokes-get-longer-starts-ends"));
        });
}

/// Color Jitter (Clip Studio Paint's): every dab, or every stroke, in a
/// slightly different color.
pub(super) fn color_jitter_ui(ui: &mut Ui, jitter: &mut Jitter, stamps: bool, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    let amount = |ui: &mut Ui, label: &str, value: &mut f32, top: f32, unit: &str, on: bool| {
        let mut shown = *value * top;
        ui.add_enabled_ui(on, |ui| {
            if slider_row(ui, label, &mut shown, 0.0..=top, unit, false, theme) {
                *value = shown / top;
            }
        });
    };
    egui::CollapsingHeader::new(small(&tr!("color-jitter")))
        .id_salt("color-jitter")
        .show(ui, |ui| {
            ui.add_enabled_ui(stamps, |ui| {
                ui.checkbox(&mut jitter.tip, small(&tr!("change-brush-tip-color")))
                    .on_disabled_hover_text(tr!("brushes-stamp-build-up-color-mixing"));
            });
            let on = jitter.tip && stamps;
            amount(ui, &tr!("hue"), &mut jitter.hue, 180.0, &tr!("deg"), on);
            amount(
                ui,
                &tr!("saturation"),
                &mut jitter.saturation,
                100.0,
                " %",
                on,
            );
            amount(ui, &tr!("luminosity"), &mut jitter.value, 100.0, " %", on);
            amount(ui, &tr!("blend-sub"), &mut jitter.sub, 100.0, " %", on);
            ui.checkbox(&mut jitter.stroke, small(&tr!("randomize-per-stroke")));
            let on = jitter.stroke;
            amount(
                ui,
                &tr!("hue"),
                &mut jitter.stroke_hue,
                180.0,
                &tr!("deg"),
                on,
            );
            amount(
                ui,
                &tr!("saturation"),
                &mut jitter.stroke_saturation,
                100.0,
                " %",
                on,
            );
            amount(
                ui,
                &tr!("luminosity"),
                &mut jitter.stroke_value,
                100.0,
                " %",
                on,
            );
        });
}

/// The dynamics of one setting: which inputs change it, down to what, and
/// their curves (Clip Studio Paint's Brush Size Dynamics).
fn dynamics_ui(ui: &mut Ui, label: &str, dynamics: &mut Dynamics, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    ui.label(
        RichText::new(tr!("dynamics", label = label))
            .size(theme.ui_size)
            .color(theme.text),
    );
    ui.label(small(&tr!("input-affecting", label = label)));
    let input = |ui: &mut Ui, name: &str, on: &mut bool, min: &mut f32| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(96.0, 18.0), egui::Checkbox::new(on, small(name)));
            let mut percent = *min * 100.0;
            ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(36.0);
            let slider = egui::Slider::new(&mut percent, 0.0..=100.0)
                .max_decimals(0)
                .suffix(" %");
            if ui
                .add_enabled(*on, slider)
                .on_hover_text(tr!("minimum-value-how-much-left-least"))
                .changed()
            {
                *min = percent / 100.0;
            }
        });
    };
    input(
        ui,
        &tr!("pen-pressure"),
        &mut dynamics.pressure,
        &mut dynamics.pressure_min,
    );
    input(
        ui,
        &tr!("velocity"),
        &mut dynamics.velocity,
        &mut dynamics.velocity_min,
    );
    input(
        ui,
        &tr!("pen-tilt"),
        &mut dynamics.tilt,
        &mut dynamics.tilt_min,
    );
    input(
        ui,
        &tr!("random"),
        &mut dynamics.random,
        &mut dynamics.random_min,
    );
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(small(&tr!("pen-pressure-settings")));
            ui.add_enabled_ui(dynamics.pressure, |ui| {
                sue_wm::widgets::response_curve(
                    ui,
                    (label, "pressure"),
                    &mut dynamics.pressure_curve,
                    92.0,
                    sue_wm::widgets::CurveLabels::pressure(),
                    theme,
                );
            });
        });
        ui.vertical(|ui| {
            ui.label(small(&tr!("velocity-settings")));
            ui.add_enabled_ui(dynamics.velocity, |ui| {
                sue_wm::widgets::response_curve(
                    ui,
                    (label, "velocity"),
                    &mut dynamics.velocity_curve,
                    92.0,
                    sue_wm::widgets::CurveLabels {
                        input: tr!("velocity"),
                        low: tr!("still"),
                        high: tr!("fast"),
                        output: tr!("output"),
                    },
                    theme,
                );
            });
        });
    });
    ui.vertical(|ui| {
        ui.label(small(&tr!("tilt-settings")));
        ui.add_enabled_ui(dynamics.tilt, |ui| {
            sue_wm::widgets::response_curve(
                ui,
                (label, "tilt"),
                &mut dynamics.tilt_curve,
                92.0,
                sue_wm::widgets::CurveLabels {
                    input: tr!("pen-tilt"),
                    low: tr!("pen-upright"),
                    high: tr!("pen-flat"),
                    output: tr!("output"),
                },
                theme,
            );
        });
    });
}
