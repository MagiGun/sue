//! Change Canvas Size, Change Image Resolution and Canvas Properties.

use egui::{RichText, Ui, vec2};
use sue_wm::Theme;

use super::{Outcome, label, ok_cancel};
use crate::doc;

/// Change Canvas Size: the new size and where the old picture sits in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanvasDialog {
    pub size: [u32; 2],
    pub old: [u32; 2],
    /// 0 top left to 8 bottom right, row by row.
    pub anchor: u8,
}

impl CanvasDialog {
    /// Where the old top left corner lands.
    pub(crate) fn shift(&self) -> [i64; 2] {
        let (column, row) = (i64::from(self.anchor % 3), i64::from(self.anchor / 3));
        [
            (i64::from(self.size[0]) - i64::from(self.old[0])) * column / 2,
            (i64::from(self.size[1]) - i64::from(self.old[1])) * row / 2,
        ]
    }
}

/// Change Image Resolution: the new size in pixels and the resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolutionDialog {
    pub size: [u32; 2],
    pub old: [u32; 2],
    pub dpi: f64,
    pub keep: bool,
}

/// Canvas Properties: the canvas's name, resolution and paper.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasProperties {
    pub picture: sue_core::EntityId,
    pub name: String,
    pub dpi: f64,
    pub paper: Option<sue_core::EntityId>,
    pub paper_color: [u8; 3],
    pub paper_shown: bool,
}

/// Change Canvas Size: width, height and the anchor.
pub(super) fn canvas_ui(ui: &mut Ui, canvas: &mut CanvasDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-canvas-size")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("width"), theme);
            ui.add(
                egui::DragValue::new(&mut canvas.size[0])
                    .range(1..=doc::MAX_SIDE)
                    .suffix(tr!("px")),
            );
            ui.end_row();
            label(ui, &tr!("height"), theme);
            ui.add(
                egui::DragValue::new(&mut canvas.size[1])
                    .range(1..=doc::MAX_SIDE)
                    .suffix(tr!("px")),
            );
            ui.end_row();
            label(ui, &tr!("reference-point"), theme);
            egui::Grid::new("sue-paint-anchor")
                .spacing(vec2(2.0, 2.0))
                .show(ui, |ui| {
                    for index in 0..9_u8 {
                        let chosen = canvas.anchor == index;
                        let button =
                            egui::Button::new(if chosen { "\u{25CF}" } else { "\u{25CB}" })
                                .min_size(vec2(24.0, 22.0))
                                .selected(chosen);
                        if ui.add(button).clicked() {
                            canvas.anchor = index;
                        }
                        if index % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            ui.end_row();
        });
    ui.label(
        RichText::new(tr!(
            "now-by-px",
            value = canvas.old[0],
            value2 = canvas.old[1]
        ))
        .size(theme.small_size)
        .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Change Image Resolution: the size in pixels (keeping its shape) and the
/// resolution.
pub(super) fn resolution_ui(
    ui: &mut Ui,
    resolution: &mut ResolutionDialog,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    let ratio = resolution.old[1] as f64 / resolution.old[0].max(1) as f64;
    egui::Grid::new("sue-paint-resolution")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("width"), theme);
            if ui
                .add(
                    egui::DragValue::new(&mut resolution.size[0])
                        .range(1..=doc::MAX_SIDE)
                        .suffix(tr!("px")),
                )
                .changed()
                && resolution.keep
            {
                resolution.size[1] =
                    ((f64::from(resolution.size[0]) * ratio).round() as u32).max(1);
            }
            ui.end_row();
            label(ui, &tr!("height"), theme);
            if ui
                .add(
                    egui::DragValue::new(&mut resolution.size[1])
                        .range(1..=doc::MAX_SIDE)
                        .suffix(tr!("px")),
                )
                .changed()
                && resolution.keep
            {
                resolution.size[0] =
                    ((f64::from(resolution.size[1]) / ratio.max(1e-6)).round() as u32).max(1);
            }
            ui.end_row();
            label(ui, &tr!("resolution"), theme);
            ui.add(
                egui::DragValue::new(&mut resolution.dpi)
                    .range(1.0..=2400.0)
                    .suffix(tr!("dpi")),
            );
            ui.end_row();
        });
    ui.checkbox(&mut resolution.keep, tr!("keep-the-shape"));
    ok_cancel(ui, &mut outcome);
    outcome
}
