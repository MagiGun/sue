//! Convert to Lines and Tones: the settings of the conversion (CTV-1)
//! beside a live preview of what the layer becomes. The conversion is
//! `doc::lines_and_tones`; OK runs it as one undo step.

use egui::{Sense, TextureOptions, Ui};
use sue_core::EntityId;
use sue_model::picture;
use sue_wm::ui::{Radius, Space, dropdown, form_grid, slider};
use sue_wm::{AppletContext, Theme};

use super::fill::check;
use super::{Outcome, bare, note, tip};
use crate::doc::{self, LinesAndTones};
use crate::effects::{Effects, TONE_SHAPES};
use crate::filters::Filter;
use crate::studio::SuePaint;

/// The longest side of the preview, in pixels of the layer's copy.
const PREVIEW_SIDE: u32 = 256;

/// The smallest tone cell the preview draws, in its pixels: finer dots
/// would only read as gray there.
const MIN_PREVIEW_CELL: f32 = 2.5;

/// The preview's picture, uploaded once per change of the settings.
#[derive(Clone, Default)]
pub(crate) struct Shown(Option<(LinesAndTones, egui::TextureHandle)>);

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shown({})", self.0.is_some())
    }
}

/// The dialog: the layer, the settings, and a small copy of the layer
/// the preview is made from.
#[derive(Clone, Debug)]
pub(crate) struct LinesTonesDialog {
    pub layer: EntityId,
    pub settings: LinesAndTones,
    /// The layer's pixels made small, straight RGBA.
    small: Vec<u8>,
    small_size: [u32; 2],
    /// The small copy's pixels per pixel of the layer.
    scale: f32,
    /// The picture's resolution: the tone's dot size follows it.
    dpi: f64,
    shown: Shown,
}

impl LinesTonesDialog {
    /// The dialog for `layer`, whose pixels are `bytes` at `size`, on a
    /// picture of `dpi`.
    pub(crate) fn new(layer: EntityId, bytes: &[u8], size: [u32; 2], dpi: f64) -> Self {
        let longest = size[0].max(size[1]).max(1);
        let scale = (PREVIEW_SIDE as f32 / longest as f32).min(1.0);
        let small_size = [
            ((size[0] as f32 * scale).round() as u32).max(1),
            ((size[1] as f32 * scale).round() as u32).max(1),
        ];
        Self {
            layer,
            settings: LinesAndTones::default(),
            small: doc::resize_rgba(bytes, size, small_size),
            small_size,
            scale: small_size[0] as f32 / size[0].max(1) as f32,
            dpi,
            shown: Shown::default(),
        }
    }

    /// What the layer becomes, made small: the tones under the lines on
    /// white paper, opaque RGBA at the copy's size.
    pub(crate) fn preview(&self) -> Vec<u8> {
        let settings = &self.settings;
        let [width, _] = self.small_size;
        let lines = settings.lines.then(|| {
            let mut lines = self.small.clone();
            Filter::LineExtraction {
                sensitivity: settings.sensitivity.clamp(0.0, 1.0),
                black: settings.black.clamp(0.0, 1.0),
                // Smoothing is in the layer's pixels: as far on the copy.
                smooth: (settings.smooth * self.scale).max(0.0),
            }
            .apply(&mut lines, width, self.small_size[1], None, [0, 0]);
            lines
        });
        // The tone's dots as large on the copy as on the layer, but never
        // finer than the preview can show.
        let cell = (self.dpi as f32 / settings.frequency.max(1.0)).max(3.0) * self.scale;
        let tone = Effects {
            tone: Some((cell.max(MIN_PREVIEW_CELL), settings.angle.to_radians())),
            tone_levels: settings.levels,
            tone_shape: settings.shape.min(TONE_SHAPES.len() as u8 - 1),
            ..Effects::default()
        };
        let mut out = Vec::with_capacity(self.small.len());
        for (at, pixel) in self.small.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (at as u32 % width, at as u32 / width);
            let mut ground = [1.0_f32; 3];
            if settings.tones {
                // As the conversion makes it: gray, as opaque as it is dark.
                let alpha = f32::from(pixel[3]) / 255.0;
                let luma = (0.299 * f32::from(pixel[0])
                    + 0.587 * f32::from(pixel[1])
                    + 0.114 * f32::from(pixel[2]))
                    / 255.0;
                let [r, g, b, a] = tone.apply([luma, luma, luma, (1.0 - luma) * alpha], x, y);
                for (channel, value) in ground.iter_mut().zip([r, g, b]) {
                    *channel = *channel * (1.0 - a) + value * a;
                }
            }
            if let Some(lines) = &lines {
                let line = &lines[at * 4..at * 4 + 4];
                let a = f32::from(line[3]) / 255.0;
                for (channel, value) in ground.iter_mut().zip(&line[..3]) {
                    *channel = *channel * (1.0 - a) + f32::from(*value) / 255.0 * a;
                }
            }
            out.extend(ground.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8));
            out.push(255);
        }
        out
    }
}

/// How a slider row reads: its range, unit and name.
struct Reads<'a> {
    salt: &'a str,
    range: std::ops::RangeInclusive<f64>,
    suffix: &'a str,
    name: &'a str,
}

/// A slider row of the form; returns whether it moved.
fn slide(ui: &mut Ui, value: &mut f32, reads: &Reads<'_>, theme: &Theme) -> bool {
    let mut now = f64::from(*value);
    let response = slider(reads.salt, &mut now, reads.range.clone())
        .name(reads.name)
        .suffix(reads.suffix)
        .show(ui, theme);
    let changed = response.changed();
    tip(response, reads.name, "", theme);
    if changed {
        *value = now as f32;
    }
    changed
}

/// A slider row for a share from 0 to 1, shown in percent.
fn slide_percent(ui: &mut Ui, value: &mut f32, reads: &Reads<'_>, theme: &Theme) {
    let mut percent = *value * 100.0;
    if slide(ui, &mut percent, reads, theme) {
        *value = percent / 100.0;
    }
}

/// The dialog's body: Extract line and Tone work, then the preview.
pub(super) fn lines_tones_ui(ui: &mut Ui, dialog: &mut LinesTonesDialog, theme: &Theme) -> Outcome {
    let settings = &mut dialog.settings;
    let (percent, px, lpi, degrees) = (tr!("lt-percent"), tr!("px"), tr!("lpi"), tr!("deg"));
    form_grid(ui, "sue-paint-lines-tones", theme, |form| {
        form.section(&tr!("lt-extract-line"));
        form.wide(|ui| {
            let name = tr!("lt-make-lines");
            check(
                ui,
                "sue-paint-lt-lines",
                &mut settings.lines,
                &name,
                "",
                theme,
            );
        });
        if settings.lines {
            let name = tr!("lt-edge-sensitivity");
            form.row(&name, |ui| {
                let reads = Reads {
                    salt: "sue-paint-lt-sensitivity",
                    range: 0.0..=100.0,
                    suffix: &percent,
                    name: &name,
                };
                slide_percent(ui, &mut settings.sensitivity, &reads, theme);
            });
            let name = tr!("lt-black-fill");
            form.row(&name, |ui| {
                let reads = Reads {
                    salt: "sue-paint-lt-black",
                    range: 0.0..=100.0,
                    suffix: &percent,
                    name: &name,
                };
                slide_percent(ui, &mut settings.black, &reads, theme);
            });
            let name = tr!("lt-smoothing");
            form.row(&name, |ui| {
                let reads = Reads {
                    salt: "sue-paint-lt-smooth",
                    range: 0.0..=8.0,
                    suffix: &px,
                    name: &name,
                };
                slide(ui, &mut settings.smooth, &reads, theme);
            });
        }
        form.section(&tr!("lt-tone-work"));
        form.wide(|ui| {
            let name = tr!("lt-make-tones");
            check(
                ui,
                "sue-paint-lt-tones",
                &mut settings.tones,
                &name,
                "",
                theme,
            );
        });
        if settings.tones {
            let name = bare(tr!("tone-posterization"));
            form.row(&name, |ui| {
                let mut levels = f32::from(settings.levels);
                let reads = Reads {
                    salt: "sue-paint-lt-levels",
                    range: 0.0..=16.0,
                    suffix: "",
                    name: &name,
                };
                if slide(ui, &mut levels, &reads, theme) {
                    settings.levels = levels.round() as u8;
                }
            });
            let name = bare(tr!("tone-shape"));
            form.row(&name, |ui| {
                let shapes = [
                    tr!("tone-shape-circle"),
                    tr!("tone-shape-square"),
                    tr!("tone-shape-diamond"),
                    tr!("tone-shape-line"),
                    tr!("tone-shape-cross"),
                    tr!("tone-shape-noise"),
                ];
                let mut chosen = usize::from(settings.shape).min(shapes.len() - 1);
                let response = dropdown("sue-paint-lt-shape", &mut chosen, &shapes)
                    .label(&name)
                    .show(ui, theme);
                if tip(response, &name, "", theme).changed() {
                    settings.shape = chosen as u8;
                }
            });
            let name = bare(tr!("frequency"));
            form.row(&name, |ui| {
                let reads = Reads {
                    salt: "sue-paint-lt-frequency",
                    range: 5.0..=300.0,
                    suffix: &lpi,
                    name: &name,
                };
                slide(ui, &mut settings.frequency, &reads, theme);
            });
            let name = bare(tr!("angle"));
            form.row(&name, |ui| {
                let reads = Reads {
                    salt: "sue-paint-lt-angle",
                    range: -90.0..=90.0,
                    suffix: &degrees,
                    name: &name,
                };
                slide(ui, &mut settings.angle, &reads, theme);
            });
        }
    });
    preview_ui(ui, dialog, theme);
    note(ui, &tr!("lt-hint"), theme);
    Outcome::Open
}

/// The preview: what the layer becomes, made small, framed by a hairline.
fn preview_ui(ui: &mut Ui, dialog: &mut LinesTonesDialog, theme: &Theme) {
    let fresh = match &dialog.shown.0 {
        Some((settings, _)) => *settings != dialog.settings,
        None => true,
    };
    if fresh {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [dialog.small_size[0] as usize, dialog.small_size[1] as usize],
            &dialog.preview(),
        );
        let texture = ui.ctx().load_texture(
            "sue-paint-lines-tones-preview",
            image,
            TextureOptions::LINEAR,
        );
        dialog.shown = Shown(Some((dialog.settings, texture)));
    }
    let Some((_, texture)) = &dialog.shown.0 else {
        return;
    };
    let room = theme.space(Space::S9) * 6.0;
    let [width, height] = dialog.small_size.map(|side| side as f32);
    let fit = (room / width.max(height)).min(ui.available_width() / width);
    let size = egui::Vec2::new(width * fit, height * fit);
    // In the middle of the dialog, under the settings it shows.
    let response = ui
        .vertical_centered(|ui| {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(size)
                    .sense(Sense::hover()),
            )
        })
        .inner;
    ui.painter().rect_stroke(
        response.rect,
        theme.radius(Radius::Xs),
        theme.hairline(),
        egui::StrokeKind::Outside,
    );
    let name = tr!("preview");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
    tip(response, &name, "", theme);
}

impl SuePaint {
    /// Layer, Convert to Lines and Tones: a 3D layer converts at once
    /// (its own lines and shading); a raster layer opens the dialog.
    pub(crate) fn open_lines_and_tones(&mut self, cx: &mut AppletContext<'_>) {
        if self
            .layer()
            .is_some_and(|layer| crate::three::is_3d(&cx.world.arena, layer))
        {
            self.lines_and_tones(cx);
            return;
        }
        let arena = &cx.world.arena;
        let (Some(picture), Ok(info)) = (self.picture(), self.paintable(arena)) else {
            cx.fail(tr!("pick-raster-layer-convert-lines-tones"));
            return;
        };
        let Ok(entity) = arena.get_by_id(picture) else {
            return;
        };
        let (Some(size), Some(raster)) = (picture::size(entity), doc::raster(arena, info.id))
        else {
            cx.fail(tr!("pick-raster-layer-convert-lines-tones"));
            return;
        };
        if [raster.width(), raster.height()] != size || info.style.offset != [0, 0] {
            cx.fail(tr!("move-layer-back-into-place-merge"));
            return;
        }
        let Some(bytes) = raster.bytes() else {
            cx.fail(tr!("pick-raster-layer-convert-lines-tones"));
            return;
        };
        let dpi = entity
            .property(picture::DPI)
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(72.0);
        let dialog = LinesTonesDialog::new(info.id, bytes, size, dpi);
        self.dialog = Some(super::Dialog::LinesTones(dialog));
    }

    /// OK: the conversion with the dialog's settings, as one undo step;
    /// the lines (or else the tones) become the current layer.
    pub(super) fn lines_tones_ok(&mut self, cx: &mut AppletContext<'_>, dialog: &LinesTonesDialog) {
        let Some(picture) = self.picture() else {
            return;
        };
        match doc::lines_and_tones(&cx.world.arena, picture, dialog.layer, &dialog.settings) {
            Ok((edit, tones, lines)) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = lines.or(tones);
                }
            }
            Err(error) => cx.fail(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer of 64 by 32: black on the left half, mid gray on the right.
    fn dialog() -> LinesTonesDialog {
        let (width, height) = (64_u32, 32_u32);
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|at| {
                let value = if at % width < width / 2 { 0 } else { 128 };
                [value, value, value, 255]
            })
            .collect();
        LinesTonesDialog::new(EntityId::NIL, &pixels, [width, height], 72.0)
    }

    #[test]
    fn the_preview_follows_the_settings() {
        let mut dialog = dialog();
        let both = dialog.preview();
        assert_eq!(both.len(), 64 * 32 * 4, "a small layer keeps its size");
        dialog.settings.tones = false;
        dialog.settings.lines = false;
        let neither = dialog.preview();
        assert!(
            neither.iter().all(|value| *value == 255),
            "white paper alone"
        );
        dialog.settings.tones = true;
        let tones = dialog.preview();
        assert_ne!(tones, neither, "the gray half takes dots");
        dialog.settings.frequency = 20.0;
        assert_ne!(dialog.preview(), tones, "coarser dots");
    }

    #[test]
    fn a_large_layer_is_previewed_small() {
        let pixels = vec![255_u8; 1024 * 512 * 4];
        let dialog = LinesTonesDialog::new(EntityId::NIL, &pixels, [1024, 512], 350.0);
        assert_eq!(dialog.small_size, [256, 128]);
        assert_eq!(dialog.preview().len(), 256 * 128 * 4);
    }
}
