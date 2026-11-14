//! Edit, Advanced Fill: its settings and its dialog.

use egui::{Rect, RichText, Sense, Ui, vec2};
use sue_wm::Theme;

use super::{Outcome, right_label};

/// Edit, Advanced Fill's settings, kept from one use to the next as Clip
/// Studio Paint keeps them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AdvancedFill {
    /// 0 main color, 1 sub color, 2 transparent, 3 `rgb`.
    pub color: u8,
    pub rgb: [u8; 3],
    /// 0 to 100.
    pub opacity: f32,
    pub blend: sue_model::BlendMode,
    pub area: crate::fill::AdvancedArea,
    /// Refer to every shown layer (else the current one).
    pub refer_all: bool,
}

impl Default for AdvancedFill {
    fn default() -> Self {
        Self {
            color: 0,
            rgb: [0, 0, 0],
            opacity: 100.0,
            blend: sue_model::BlendMode::Normal,
            area: crate::fill::AdvancedArea {
                anti_alias: false,
                scaling: 10,
                gap: 2,
                ..crate::fill::AdvancedArea::default()
            },
            refer_all: false,
        }
    }
}

/// What people read for an Advanced Fill target.
pub(crate) fn target_label(target: crate::fill::Target) -> String {
    use crate::fill::Target;
    match target {
        Target::All => tr!("target-all-colors"),
        Target::Transparent => tr!("only-transparent"),
        Target::NotTransparent => tr!("other-than-transparent"),
        Target::SurroundedByClear => tr!("area-surrounded-by-transparent"),
        Target::Black => tr!("only-black"),
        Target::NotBlack => tr!("other-than-black"),
        Target::SurroundedByBlack => tr!("area-surrounded-by-black"),
        Target::WhiteOrClear => tr!("only-white-and-transparent"),
        Target::NotWhiteOrClear => tr!("other-than-white-and-transparent"),
        Target::SurroundedByWhite => tr!("area-surrounded-by-white-and-transparent"),
        Target::SemiClear => tr!("treat-semi-transparent-as-transparent"),
    }
}

/// Advanced Fill's dialog, laid out as Clip Studio Paint's: the color and
/// how it lays down, then what it fills, then what it refers to; OK and
/// Cancel at the right.
pub(super) fn advanced_fill_ui(
    ui: &mut Ui,
    fill: &mut AdvancedFill,
    main: [u8; 3],
    sub: [u8; 3],
    theme: &Theme,
) -> Outcome {
    use crate::fill::Target;
    let mut outcome = Outcome::Open;
    let column = 118.0;
    ui.spacing_mut().item_spacing = vec2(8.0, 7.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 96.0);
            ui.horizontal(|ui| {
                right_label(ui, &tr!("drawing-color"), column, theme);
                for (index, (color, tip)) in [
                    (Some(main), tr!("main-color")),
                    (Some(sub), tr!("sub-color")),
                    (None, tr!("transparent-color")),
                ]
                .into_iter()
                .enumerate()
                {
                    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
                    let chosen = usize::from(fill.color) == index;
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        if chosen { theme.selection } else { theme.pill },
                    );
                    let chip = rect.shrink(5.0);
                    match color {
                        Some([r, g, b]) => {
                            ui.painter()
                                .rect_filled(chip, 1.0, egui::Color32::from_rgb(r, g, b));
                        }
                        None => {
                            // A checker for see-through.
                            let half = chip.size() / 2.0;
                            for (dx, dy, light) in [
                                (0.0, 0.0, true),
                                (1.0, 0.0, false),
                                (0.0, 1.0, false),
                                (1.0, 1.0, true),
                            ] {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        chip.min + vec2(dx * half.x, dy * half.y),
                                        half,
                                    ),
                                    0.0,
                                    if light {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::GRAY
                                    },
                                );
                            }
                        }
                    }
                    if response.on_hover_text(tip).clicked() {
                        fill.color = index as u8;
                    }
                }
                let mut rgba = [fill.rgb[0], fill.rgb[1], fill.rgb[2], 255];
                let pick = sue_wm::color_button(
                    ui,
                    "sue-paint-advanced-fill-color",
                    &mut rgba,
                    false,
                    theme,
                );
                if pick.changed {
                    fill.rgb = [rgba[0], rgba[1], rgba[2]];
                    fill.color = 3;
                }
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("opacity"), column, theme);
                ui.add(
                    egui::DragValue::new(&mut fill.opacity)
                        .range(0.0..=100.0)
                        .max_decimals(0),
                );
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("blending-mode"), column, theme);
                egui::ComboBox::from_id_salt("sue-paint-advanced-fill-blend")
                    .selected_text(sue_lang::translate("model", fill.blend.label()))
                    .width(180.0)
                    .height(360.0)
                    .show_ui(ui, |ui| {
                        for mode in sue_model::BlendMode::ALL {
                            ui.selectable_value(
                                &mut fill.blend,
                                mode,
                                sue_lang::translate("model", mode.label()),
                            );
                        }
                    });
            });
            ui.checkbox(&mut fill.area.anti_alias, tr!("anti-aliasing"));
            ui.separator();
            ui.horizontal(|ui| {
                right_label(ui, &tr!("target-color"), column, theme);
                egui::ComboBox::from_id_salt("sue-paint-advanced-fill-target")
                    .selected_text(target_label(fill.area.target))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for target in Target::ALL
                            .into_iter()
                            .filter(|target| target.offered(fill.area.closed))
                        {
                            ui.selectable_value(
                                &mut fill.area.target,
                                target,
                                target_label(target),
                            );
                        }
                    });
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("tolerance"), column, theme);
                let mut percent = fill.area.tolerance * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(0.0..=100.0)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    fill.area.tolerance = percent / 100.0;
                }
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("area-scaling"), column, theme);
                ui.add(
                    egui::DragValue::new(&mut fill.area.scaling)
                        .range(-50..=50)
                        .suffix(tr!("px")),
                );
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("scaling-mode"), column, theme);
                egui::ComboBox::from_id_salt("sue-paint-advanced-fill-scaling")
                    .selected_text(if fill.area.round {
                        tr!("round")
                    } else {
                        tr!("rectangle")
                    })
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut fill.area.round, false, tr!("rectangle"));
                        ui.selectable_value(&mut fill.area.round, true, tr!("round"));
                    });
            });
            if ui
                .checkbox(&mut fill.area.closed, tr!("fill-closed-area"))
                .changed()
                && !fill.area.target.offered(fill.area.closed)
            {
                fill.area.target = Target::All;
            }
            ui.add_enabled_ui(fill.area.closed, |ui| {
                ui.horizontal(|ui| {
                    right_label(ui, &tr!("close-gap"), column, theme);
                    ui.add(
                        egui::DragValue::new(&mut fill.area.gap)
                            .range(0..=50)
                            .suffix(tr!("px")),
                    );
                });
            });
            ui.separator();
            ui.checkbox(&mut fill.refer_all, tr!("refer-multiple"))
                .on_hover_text(tr!("refer-multiple-hint"));
            ui.checkbox(&mut fill.area.border, tr!("refer-to-image-border"));
        });
        ui.vertical(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(tr!("ok"))
                            .strong()
                            .color(theme.selection_text),
                    )
                    .fill(theme.selection)
                    .min_size(vec2(80.0, 24.0)),
                )
                .clicked()
            {
                outcome = Outcome::Ok;
            }
            if ui
                .add(egui::Button::new(tr!("cancel")).min_size(vec2(80.0, 24.0)))
                .clicked()
            {
                outcome = Outcome::Cancel;
            }
        });
    });
    outcome
}
