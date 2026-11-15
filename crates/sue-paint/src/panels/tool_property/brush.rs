//! The brush rows of Tool Property: sliders with dynamics, starting and
//! ending, color jitter and the dynamics settings.

use egui::{RichText, Ui};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};

use super::{Category, Rows, small_text};
use crate::brush::{Dynamics, Jitter, TaperMode};

/// A slider row with a dynamics button at its end (Brush Size and Opacity,
/// as in Clip Studio Paint): the button is lit while the pen changes the
/// setting and opens its dynamics. `extra` adds the setting's own options
/// under them. Returns `true` when the value changed.
#[allow(clippy::too_many_arguments, reason = "a slider row and its dynamics")]
pub(super) fn dynamic_row(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    id: &str,
    category: Category,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    log: bool,
    dynamics: &mut Dynamics,
    extra: impl FnOnce(&mut Ui, &Theme),
) -> bool {
    let label_width = rows.label_width();
    rows.row(ui, id, category, |ui, theme, enabled| {
        ui.horizontal(|ui| {
            let side = Density::of(ui).icon_button();
            let room = (ui.available_width() - side - theme.space(Space::S2)).max(side);
            let mut shown = f64::from(*value);
            let mut slider = sue_wm::ui::slider(
                ("sue-paint-tool-slider", id),
                &mut shown,
                f64::from(*range.start())..=f64::from(*range.end()),
            )
            .label(label)
            .label_width(label_width)
            .suffix(suffix)
            .decimals(usize::from(log))
            .enabled(enabled)
            .width(room);
            if log {
                slider = slider.logarithmic();
            }
            let changed = slider.show(ui, theme).on_hover_text(label).changed();
            if changed {
                *value = shown as f32;
            }
            let name = tr!("dynamics-what-the-pen-changes", label = label);
            let button =
                sue_wm::ui::icon_button(("sue-paint-dynamics", id), Icon::PressureCurve, &name)
                    .selected(dynamics.any())
                    .enabled(enabled)
                    .show(ui, theme);
            egui::Popup::from_toggle_button_response(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_width(theme.space(Space::S9) * 7.5);
                    dynamics_ui(ui, label, dynamics, theme);
                    extra(ui, theme);
                });
            changed
        })
        .inner
    })
    .unwrap_or(false)
}

/// Starting and ending (Clip Studio Paint's): how they are measured, how
/// long they are, what they change, and whether speed stretches them.
pub(super) fn starting_and_ending(ui: &mut Ui, rows: &mut Rows<'_>, tip: &mut crate::brush::Tip) {
    let modes: Vec<String> = TaperMode::ALL.iter().map(|mode| mode.label()).collect();
    let mut mode = TaperMode::ALL
        .iter()
        .position(|found| *found == tip.taper_mode)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "taper-mode",
        Category::StartEnd,
        &tr!("how-to-specify"),
        &mut mode,
        &modes,
    ) {
        tip.taper_mode = TaperMode::ALL[mode];
    }
    let (unit, top) = match tip.taper_mode {
        TaperMode::Percent => (" %".to_owned(), 100.0),
        TaperMode::Length | TaperMode::Fade => (tr!("px"), 400.0),
    };
    rows.slider(
        ui,
        "taper-start",
        Category::StartEnd,
        &tr!("starting-2"),
        &mut tip.taper_start,
        0.0..=top,
        &unit,
        false,
    );
    let ending = if tip.taper_mode == TaperMode::Fade {
        tr!("fade-by")
    } else {
        tr!("ending")
    };
    rows.slider(
        ui,
        "taper-end",
        Category::StartEnd,
        &ending,
        &mut tip.taper_end,
        0.0..=top,
        &unit,
        false,
    );
    rows.check(
        ui,
        "taper-size",
        Category::StartEnd,
        &tr!("taper-brush-size"),
        None,
        &mut tip.taper_size,
    );
    rows.check(
        ui,
        "taper-opacity",
        Category::StartEnd,
        &tr!("taper-opacity"),
        None,
        &mut tip.taper_opacity,
    );
    rows.check(
        ui,
        "taper-by-speed",
        Category::StartEnd,
        &tr!("starting-and-ending-by-speed"),
        Some(&tr!("quicker-strokes-get-longer-starts-ends")),
        &mut tip.taper_by_speed,
    );
}

/// Color Jitter (Clip Studio Paint's): every dab, or every stroke, in a
/// slightly different color. Its amounts show while their switch is on.
pub(super) fn color_jitter(ui: &mut Ui, rows: &mut Rows<'_>, jitter: &mut Jitter, stamps: bool) {
    if stamps {
        rows.check(
            ui,
            "jitter-tip",
            Category::ColorJitter,
            &tr!("change-brush-tip-color"),
            None,
            &mut jitter.tip,
        );
    } else {
        rows.hint(ui, &tr!("brushes-stamp-build-up-color-mixing"));
    }
    let amount = |ui: &mut Ui,
                  rows: &mut Rows<'_>,
                  id: &str,
                  label: &str,
                  value: &mut f32,
                  top: f32,
                  unit: &str| {
        let mut shown = *value * top;
        if rows.slider(
            ui,
            id,
            Category::ColorJitter,
            label,
            &mut shown,
            0.0..=top,
            unit,
            false,
        ) {
            *value = shown / top;
        }
    };
    if jitter.tip && stamps {
        amount(
            ui,
            rows,
            "jitter-hue",
            &tr!("hue"),
            &mut jitter.hue,
            180.0,
            &tr!("deg"),
        );
        amount(
            ui,
            rows,
            "jitter-saturation",
            &tr!("saturation"),
            &mut jitter.saturation,
            100.0,
            " %",
        );
        amount(
            ui,
            rows,
            "jitter-luminosity",
            &tr!("luminosity"),
            &mut jitter.value,
            100.0,
            " %",
        );
        amount(
            ui,
            rows,
            "jitter-sub",
            &tr!("blend-sub"),
            &mut jitter.sub,
            100.0,
            " %",
        );
    }
    rows.check(
        ui,
        "jitter-stroke",
        Category::ColorJitter,
        &tr!("randomize-per-stroke"),
        None,
        &mut jitter.stroke,
    );
    if jitter.stroke {
        amount(
            ui,
            rows,
            "jitter-stroke-hue",
            &tr!("hue"),
            &mut jitter.stroke_hue,
            180.0,
            &tr!("deg"),
        );
        amount(
            ui,
            rows,
            "jitter-stroke-saturation",
            &tr!("saturation"),
            &mut jitter.stroke_saturation,
            100.0,
            " %",
        );
        amount(
            ui,
            rows,
            "jitter-stroke-luminosity",
            &tr!("luminosity"),
            &mut jitter.stroke_value,
            100.0,
            " %",
        );
    }
}

/// The dynamics of one setting: which inputs change it, down to what, and
/// their curves (Clip Studio Paint's Brush Size Dynamics).
fn dynamics_ui(ui: &mut Ui, label: &str, dynamics: &mut Dynamics, theme: &Theme) {
    let small = small_text(theme);
    ui.label(
        RichText::new(tr!("dynamics", label = label))
            .size(theme.text_size(TextRole::Body))
            .color(theme.text),
    );
    ui.label(small(&tr!("input-affecting", label = label)));
    let input = |ui: &mut Ui, id: &str, name: &str, on: &mut bool, min: &mut f32| {
        ui.horizontal(|ui| {
            if sue_wm::ui::checkbox(("sue-paint-dynamics-input", label, id), name, *on)
                .show(ui, theme)
                .on_hover_text(name)
                .clicked()
            {
                *on = !*on;
            }
            let mut percent = f64::from(*min * 100.0);
            if sue_wm::ui::slider(
                ("sue-paint-dynamics-min", label, id),
                &mut percent,
                0.0..=100.0,
            )
            .name(&tr!("minimum-value-how-much-left-least"))
            .suffix(" %")
            .enabled(*on)
            .show(ui, theme)
            .on_hover_text(tr!("minimum-value-how-much-left-least"))
            .changed()
            {
                *min = percent as f32 / 100.0;
            }
        });
    };
    input(
        ui,
        "pressure",
        &tr!("pen-pressure"),
        &mut dynamics.pressure,
        &mut dynamics.pressure_min,
    );
    input(
        ui,
        "velocity",
        &tr!("velocity"),
        &mut dynamics.velocity,
        &mut dynamics.velocity_min,
    );
    input(
        ui,
        "tilt",
        &tr!("pen-tilt"),
        &mut dynamics.tilt,
        &mut dynamics.tilt_min,
    );
    input(
        ui,
        "random",
        &tr!("random"),
        &mut dynamics.random,
        &mut dynamics.random_min,
    );
    ui.add_space(theme.space(Space::S2));
    let graph = theme.space(Space::S9) * 2.3;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(small(&tr!("pen-pressure-settings")));
            ui.add_enabled_ui(dynamics.pressure, |ui| {
                sue_wm::widgets::response_curve(
                    ui,
                    (label, "pressure"),
                    &mut dynamics.pressure_curve,
                    graph,
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
                    graph,
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
                graph,
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
