//! The animation dialogs: Onion Skin Settings, Edit Timeline and Export
//! Timelapse.

use egui::{RichText, Ui, vec2};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, label, ok_cancel};
use crate::doc;
use crate::studio::SuePaint;

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

/// Edit Timeline: frames a second and length in frames.
pub(super) fn timeline_ui(ui: &mut Ui, fps: &mut u32, length: &mut u32, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-timeline-edit")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("frame-rate-2"), theme);
            ui.add(egui::DragValue::new(fps).range(1..=120).suffix(tr!("fps")));
            ui.end_row();
            label(ui, &tr!("playback-time"), theme);
            ui.add(
                egui::DragValue::new(length)
                    .range(1..=100_000)
                    .suffix(tr!("frames")),
            );
            ui.end_row();
        });
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Export Timelapse: the video's size and length.
pub(super) fn timelapse_ui(
    ui: &mut Ui,
    export: &mut crate::timelapse::Export,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-timelapse-export")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("long-side"), theme);
            egui::ComboBox::from_id_salt("sue-paint-timelapse-side")
                .selected_text(tr!("px-2", side = export.side))
                .show_ui(ui, |ui| {
                    for side in [480, 720, 1080, 1280, 1920] {
                        ui.selectable_value(&mut export.side, side, tr!("px-2", side = side));
                    }
                });
            ui.end_row();
            label(ui, &tr!("length"), theme);
            let mut fit = export.seconds.is_some();
            ui.vertical(|ui| {
                if ui
                    .radio_value(&mut fit, true, tr!("fit-to-specified-length"))
                    .changed()
                    || ui
                        .radio_value(&mut fit, false, tr!("export-all-frames"))
                        .changed()
                {
                    export.seconds = fit.then_some(30.0);
                }
                if let Some(seconds) = export.seconds.as_mut() {
                    ui.add(
                        egui::DragValue::new(seconds)
                            .range(1.0..=600.0)
                            .suffix(" s"),
                    );
                }
            });
            ui.end_row();
        });
    ui.label(
        RichText::new(tr!("motion-jpeg-avi-video-animated-gif"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

impl SuePaint {
    /// Changes the timeline's frame rate and length.
    pub(super) fn timeline_ok(&mut self, cx: &mut AppletContext<'_>, fps: u32, length: u32) {
        if let Some(picture) = self.picture()
            && let Some(edit) = crate::anim::edit_timeline(&cx.world.arena, picture, fps, length)
        {
            Self::apply(cx, edit);
        }
    }

    /// Keeps the onion skin settings on the picture and shows it.
    pub(super) fn onion_ok(&mut self, cx: &mut AppletContext<'_>, onion: &crate::anim::Onion) {
        if let Some(picture) = self.picture() {
            let edit = doc::Edit::new(tr!("onion-skin-settings")).then(
                sue_core::txn::SetProperty::new(picture, crate::anim::ONION, onion.to_text()),
            );
            if Self::apply(cx, edit) {
                // The settings are for seeing it.
                self.onion = true;
            }
        }
    }
}
