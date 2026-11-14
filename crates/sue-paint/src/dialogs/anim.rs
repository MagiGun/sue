//! Onion Skin Settings.

use egui::{RichText, Ui, vec2};
use sue_wm::Theme;

use super::{Outcome, label, ok_cancel};

/// Onion Skin Settings, as Clip Studio Paint lays them out: how many cels
/// show before and after, their colors, and how faint they are.
pub(super) fn onion_ui(ui: &mut Ui, onion: &mut crate::anim::Onion, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.label(
        RichText::new(tr!("number-of-skins"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    egui::Grid::new("sue-paint-onion-count")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("previous-frame-2"), theme);
            ui.add(
                egui::DragValue::new(&mut onion.before)
                    .range(0..=10)
                    .suffix(tr!("cels")),
            );
            ui.end_row();
            label(ui, &tr!("next-frame-2"), theme);
            ui.add(
                egui::DragValue::new(&mut onion.after)
                    .range(0..=10)
                    .suffix(tr!("cels")),
            );
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr!("view-settings"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    egui::Grid::new("sue-paint-onion-view")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("color-mode"), theme);
            egui::ComboBox::from_id_salt("sue-paint-onion-mode")
                .selected_text(crate::localize(
                    crate::anim::Onion::MODES[usize::from(onion.mode.min(2))],
                ))
                .show_ui(ui, |ui| {
                    for (mode, name) in crate::anim::Onion::MODES.iter().enumerate() {
                        ui.selectable_value(&mut onion.mode, mode as u8, crate::localize(name));
                    }
                });
            ui.end_row();
            for (name, color) in [
                (tr!("previous-frame-2"), &mut onion.before_color),
                (tr!("next-frame-2"), &mut onion.after_color),
            ] {
                label(ui, &name, theme);
                let mut rgba = [color[0], color[1], color[2], 255];
                if sue_wm::color_button(ui, name, &mut rgba, false, theme).changed {
                    *color = [rgba[0], rgba[1], rgba[2]];
                }
                ui.end_row();
            }
        });
    let mut specify = onion.opacity.is_some();
    if ui
        .checkbox(&mut specify, tr!("specify-opacity-level"))
        .changed()
    {
        onion.opacity = specify.then_some((0.8, 0.1));
    }
    if let Some((start, step)) = onion.opacity.as_mut() {
        egui::Grid::new("sue-paint-onion-opacity")
            .num_columns(2)
            .spacing(vec2(12.0, 8.0))
            .show(ui, |ui| {
                let percent = |ui: &mut Ui, name: &str, value: &mut f32| {
                    label(ui, name, theme);
                    let mut shown = (*value * 100.0).round();
                    if ui
                        .add(
                            egui::DragValue::new(&mut shown)
                                .range(0.0..=100.0)
                                .suffix(" %"),
                        )
                        .changed()
                    {
                        *value = shown / 100.0;
                    }
                    ui.end_row();
                };
                percent(ui, &tr!("start"), start);
                percent(ui, &tr!("step-opacity"), step);
            });
    }
    ok_cancel(ui, &mut outcome);
    outcome
}
