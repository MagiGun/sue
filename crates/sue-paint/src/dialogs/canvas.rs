//! The canvas's and layers' dialogs: Change Canvas Size, Change Image
//! Resolution, Canvas Properties, Color Profile Preview Settings, Print,
//! Convert Layer, New 3D Layer, Register Material and Create Perspective
//! Ruler.

use egui::{RichText, Ui, vec2};
use sue_core::EntityId;
use sue_wm::{AppletContext, Theme};

use super::{Outcome, label, ok_cancel};
use crate::doc;
use crate::studio::SuePaint;

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

/// Print Settings, or Print when `printing`.
pub(super) fn print_ui(
    ui: &mut Ui,
    settings: &mut crate::print::PrintSettings,
    printing: bool,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("paper"), theme);
        egui::ComboBox::from_id_salt("sue-paint-paper")
            .selected_text(crate::localize(crate::print::PAPERS[settings.paper].0))
            .show_ui(ui, |ui| {
                for (index, (name, ..)) in crate::print::PAPERS.iter().enumerate() {
                    ui.selectable_value(&mut settings.paper, index, crate::localize(name));
                }
            });
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("orientation"), theme);
        ui.radio_value(&mut settings.orientation, 0, tr!("auto"));
        ui.radio_value(&mut settings.orientation, 1, tr!("portrait"));
        ui.radio_value(&mut settings.orientation, 2, tr!("landscape"));
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("size"), theme);
        ui.radio_value(&mut settings.size, 0, tr!("fit-to-page"));
        ui.radio_value(&mut settings.size, 1, tr!("print-size"));
        ui.radio_value(&mut settings.size, 2, tr!("scale"));
    });
    if settings.size == 2 {
        ui.add(egui::Slider::new(&mut settings.percent, 1.0..=400.0).suffix(" %"));
    }
    ui.horizontal(|ui| {
        label(ui, &tr!("margin"), theme);
        ui.add(egui::Slider::new(&mut settings.margin, 0.0..=50.0).suffix(tr!("mm")));
    });
    ui.checkbox(&mut settings.all_pages, tr!("every-page-of-the-story"));
    if printing {
        ui.label(
            RichText::new(tr!("pages-open-as-pdf-your-viewer"))
                .size(theme.small_size)
                .color(theme.muted),
        );
    }
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Color Profile Preview Settings: which profile.
pub(super) fn proof_ui(ui: &mut Ui, profile: &mut usize, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.label(
        RichText::new(tr!("how-picture-would-print-shown-while"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    for (index, (name, ..)) in crate::proof::PROFILES.iter().enumerate() {
        ui.radio_value(profile, index, crate::localize(name));
    }
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Canvas Properties: the name, resolution and paper.
pub(super) fn canvas_properties_ui(
    ui: &mut Ui,
    props: &mut CanvasProperties,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("name"), theme);
        ui.text_edit_singleline(&mut props.name);
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("resolution"), theme);
        ui.add(
            egui::DragValue::new(&mut props.dpi)
                .range(1.0..=2400.0)
                .suffix(tr!("dpi")),
        );
    });
    if props.paper.is_some() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut props.paper_shown, tr!("paper-2"));
            ui.color_edit_button_srgb(&mut props.paper_color);
        });
    }
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Convert Layer: to raster or vector, and its name.
pub(super) fn convert_ui(ui: &mut Ui, kind: &mut u8, name: &mut String, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("name"), theme);
        ui.text_edit_singleline(name);
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("type"), theme);
        ui.radio_value(kind, 0, tr!("raster-layer"));
        ui.radio_value(kind, 1, tr!("vector-layer"));
    });
    ui.label(
        RichText::new(tr!("vector-pixels-are-traced-lines-down"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// New 3D Layer: which shape.
pub(super) fn shape_3d_ui(ui: &mut Ui, kind: &mut usize, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    label(ui, &tr!("shape"), theme);
    for (index, name) in crate::three::SHAPES.iter().enumerate() {
        ui.radio_value(kind, index, crate::localize(name));
    }
    ui.label(
        RichText::new(tr!("file-import-3d-data-brings-models"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Register Material, Image: its name.
pub(super) fn material_ui(ui: &mut Ui, name: &mut String, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("material-name"), theme);
        ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
    });
    ui.label(
        RichText::new(tr!("current-layer-goes-into-material-palette"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Create Perspective Ruler: how many vanishing points.
pub(super) fn perspective_ui(ui: &mut Ui, count: &mut u32, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("type"), theme);
        ui.vertical(|ui| {
            for (points, name) in [
                (1, tr!("n-1-point-perspective")),
                (2, tr!("n-2-point-perspective")),
                (3, tr!("n-3-point-perspective")),
            ] {
                ui.radio_value(count, points, name);
            }
        });
    });
    ui.label(
        RichText::new(tr!("strokes-run-toward-vanishing-points-while"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

impl SuePaint {
    /// Keeps the print settings, and prints when `printing`.
    pub(super) fn print_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        settings: crate::print::PrintSettings,
        printing: bool,
    ) {
        self.print = settings;
        if printing {
            self.print_now(cx);
        }
    }

    /// Keeps the canvas's name, resolution and paper as one undo step.
    pub(super) fn canvas_properties_ok(cx: &mut AppletContext<'_>, props: &CanvasProperties) {
        let mut edit = crate::doc::Edit::new(tr!("canvas-properties")).then(
            sue_core::txn::SetProperty::new(props.picture, sue_model::picture::DPI, props.dpi),
        );
        let name = props.name.trim();
        if !name.is_empty() {
            edit = edit.then(sue_core::txn::Rename::new(props.picture, name));
        }
        if let Some(paper) = props.paper {
            let [r, g, b] = props.paper_color;
            edit = edit
                .then(sue_core::txn::SetProperty::new(
                    paper,
                    sue_model::picture::COLOR,
                    sue_core::Color::srgb8(r, g, b),
                ))
                .then(sue_core::txn::SetProperty::new(
                    paper,
                    sue_model::picture::VISIBLE,
                    props.paper_shown,
                ));
        }
        Self::apply(cx, edit);
    }

    /// Converts `layer` to raster (0) or vector (1) and names it.
    pub(super) fn convert_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        kind: u8,
        name: &str,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let vector = crate::vector::is_vector(arena, layer);
        let made = match (kind, vector) {
            (1, false) => crate::doc::vectorize(arena, picture, layer).map(Some),
            (0, true) => crate::doc::rasterize(arena, picture, layer).map(Some),
            _ => Ok(None),
        };
        match made {
            Ok(Some((mut edit, id))) => {
                if !name.trim().is_empty() {
                    edit = edit.then(sue_core::txn::Rename::new(id, name.trim()));
                }
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            Ok(None) => {
                if !name.trim().is_empty() {
                    Self::apply(
                        cx,
                        crate::doc::Edit::new(tr!("rename-layer"))
                            .then(sue_core::txn::Rename::new(layer, name.trim())),
                    );
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Adds a 3D layer with shape `kind`.
    pub(super) fn shape_3d_ok(&mut self, cx: &mut AppletContext<'_>, kind: usize) {
        let name = crate::three::SHAPES[kind.min(crate::three::SHAPES.len() - 1)];
        let model = sue_core::Entity::empty(crate::localize(name), sue_model::class::MODEL)
            .with_ext(crate::three::shape(kind));
        self.add_3d_layer(cx, name, model);
    }

    /// Changes the canvas size.
    pub(super) fn canvas_ok(&mut self, cx: &mut AppletContext<'_>, canvas: &CanvasDialog) {
        if let Some(picture) = self.picture() {
            match doc::resize_canvas(&cx.world.arena, picture, canvas.size, canvas.shift()) {
                Ok(edit) => {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(None);
                    }
                }
                Err(error) => cx.fail(error),
            }
        }
    }

    /// Changes the image resolution.
    pub(super) fn resolution_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        resolution: &ResolutionDialog,
    ) {
        if let Some(picture) = self.picture() {
            match doc::resample(&cx.world.arena, picture, resolution.size, resolution.dpi) {
                Ok(edit) => {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(None);
                    }
                }
                Err(error) => cx.fail(error),
            }
        }
    }

    /// Shows the picture as `profile` would print it.
    pub(super) fn proof_ok(&mut self, profile: usize) {
        self.proof_profile = profile;
        self.proof_on = true;
    }
}
