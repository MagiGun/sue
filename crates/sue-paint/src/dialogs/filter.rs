//! The filter dialogs: their settings with a live preview, the quick
//! looks' gallery, and keeping a filter's result.

use egui::{Align, Layout, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_core::{EntityId, RasterTile2D};
use sue_wm::{AppletContext, ArenaEdit, Theme, widgets};

use super::{Outcome, ok_cancel};
use crate::doc;
use crate::filters::Filter;
use crate::select::Selection;
use crate::studio::SuePaint;

/// Small pictures for a dialog's gallery, uploaded once.
#[derive(Clone, Default)]
pub(crate) struct Thumbs(pub Vec<egui::TextureHandle>);

impl std::fmt::Debug for Thumbs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Thumbs({})", self.0.len())
    }
}

/// A filter's dialog: the settings, the layer it previews on and the
/// layer's pixels from before.
#[derive(Clone, Debug)]
pub(crate) struct FilterDialog {
    pub filter: Filter,
    pub layer: EntityId,
    pub before: Vec<u8>,
    pub size: [u32; 2],
    pub selection: Option<Selection>,
    pub offset: [i64; 2],
    pub shown: Option<Filter>,
    pub preview: bool,
    /// The quick looks' small previews of this layer, made once.
    pub thumbs: Thumbs,
    /// It works on a merged layer made for it (Apply to all): Cancel
    /// takes that layer away again.
    pub merged: bool,
}

impl FilterDialog {
    pub(crate) fn new(
        filter: Filter,
        layer: EntityId,
        raster: &RasterTile2D,
        selection: Option<Selection>,
        offset: [i64; 2],
    ) -> Self {
        Self {
            filter,
            layer,
            before: raster.bytes().map(<[u8]>::to_vec).unwrap_or_default(),
            size: [raster.width(), raster.height()],
            selection,
            offset,
            shown: None,
            preview: true,
            thumbs: Thumbs::default(),
            merged: false,
        }
    }

    /// The layer's pixels with `filter` applied to the originals.
    fn result(&self, filter: Filter) -> Vec<u8> {
        let mut pixels = self.before.clone();
        filter.apply(
            &mut pixels,
            self.size[0],
            self.size[1],
            self.selection.as_ref(),
            self.offset,
        );
        pixels
    }

    /// Writes `pixels` into the layer in place (a preview, not history).
    fn show(&self, cx: &mut AppletContext<'_>, pixels: &[u8]) {
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, self.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == pixels.len())
        {
            bytes.copy_from_slice(pixels);
        }
    }

    /// Shows the setting live on the layer when it changed, unless the
    /// pointer is down on a slow filter; answers the dialog that now
    /// shows it, if it changed.
    pub(super) fn show_preview(
        &self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
    ) -> Option<Self> {
        let pointer_down = ctx.input(|input| input.pointer.any_down());
        let wanted = if self.preview {
            Some(self.filter)
        } else {
            None
        };
        let cheap = !matches!(
            self.filter,
            Filter::GaussianBlur { .. }
                | Filter::Sharpen { .. }
                | Filter::Mosaic { .. }
                | Filter::LineExtraction { .. }
                | Filter::MotionBlur { .. }
                | Filter::RadialBlur { .. }
                | Filter::ZoomBlur { .. }
                | Filter::LineWidth { .. }
                | Filter::Wave { .. }
                | Filter::Pinch { .. }
                | Filter::Polar { .. }
                | Filter::Ripple { .. }
                | Filter::Twirl { .. }
                | Filter::ZigZag { .. }
                | Filter::Crystallize { .. }
                | Filter::JpegNoise { .. }
                | Filter::Quick { .. }
                | Filter::SmartSmooth { .. }
                | Filter::Fisheye { .. }
                | Filter::Aberration { .. }
                | Filter::Glow { .. }
                | Filter::DropShadow { .. }
                | Filter::Inflate { .. }
                | Filter::Retro { .. }
                | Filter::PerlinNoise { .. }
                | Filter::Unsharp { .. }
                | Filter::Dust { .. }
        );
        if wanted != self.shown && (cheap || !pointer_down) {
            let pixels = match wanted {
                Some(filter_now) => self.result(filter_now),
                None => self.before.clone(),
            };
            self.show(cx, &pixels);
            let mut updated = self.clone();
            updated.shown = wanted;
            return Some(updated);
        }
        None
    }
}

impl SuePaint {
    /// Keeps `filter`'s result on its layer and, for a tonal correction,
    /// on every layer picked with it (Clip Studio Paint 5.0), as one step.
    pub(crate) fn keep_filter(&mut self, cx: &mut AppletContext<'_>, filter: &FilterDialog) {
        let after = filter.result(filter.filter);
        filter.show(cx, &after);
        let label = filter.filter.label();
        let mut edit = crate::doc::Edit::new(label.clone());
        let mut changed = false;
        if let Some(patch) = doc::diff_patch(&filter.before, &after, filter.size[0], filter.size[1])
        {
            edit = edit.then(sue_core::txn::RasterEdit::new(
                filter.layer,
                label.clone(),
                vec![patch],
            ));
            changed = true;
        }
        if filter.filter.to_text().is_some() && !filter.merged {
            for (layer, patch) in self.correct_picked(cx, filter) {
                edit = edit.then(sue_core::txn::RasterEdit::new(
                    layer,
                    label.clone(),
                    vec![patch],
                ));
                changed = true;
            }
        }
        if changed {
            cx.record(ArenaEdit(edit));
        }
    }

    /// Runs `dialog`'s filter on the other unlocked raster layers picked
    /// with the current one, in place; answers each changed layer with
    /// its patch, to record.
    fn correct_picked(
        &mut self,
        cx: &mut AppletContext<'_>,
        dialog: &FilterDialog,
    ) -> Vec<(sue_core::EntityId, sue_core::txn::RasterPatch)> {
        let (Some(picture), Some(picked)) =
            (self.picture(), self.doc().map(|doc| doc.picked.clone()))
        else {
            return Vec::new();
        };
        let infos = doc::layers(&cx.world.arena, picture);
        let mut out = Vec::new();
        for layer in picked.into_iter().filter(|layer| *layer != dialog.layer) {
            let Some(info) = infos.iter().find(|info| {
                info.id == layer
                    && info.kind == sue_model::picture::LayerKind::Raster
                    && !info.locked
                    && !doc::is_folder(&cx.world.arena, info.id)
            }) else {
                continue;
            };
            let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) else {
                continue;
            };
            let (width, height) = (raster.width(), raster.height());
            let Some(bytes) = raster.bytes_mut() else {
                continue;
            };
            let before = bytes.to_vec();
            dialog.filter.apply(
                bytes,
                width,
                height,
                dialog.selection.as_ref(),
                info.style.offset,
            );
            if let Some(patch) = doc::diff_patch(&before, bytes, width, height) {
                out.push((layer, patch));
            }
        }
        if !out.is_empty()
            && let Some(doc) = self.doc_mut()
        {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
        out
    }
}

/// A filter's or correction's settings.
pub(super) fn filter_params(ui: &mut Ui, filter: &mut Filter, theme: &Theme) {
    ui.spacing_mut().slider_width = 220.0;
    match filter {
        Filter::BrightnessContrast {
            brightness,
            contrast,
        } => {
            ui.add(egui::Slider::new(brightness, -1.0..=1.0).text("Brightness"));
            ui.add(egui::Slider::new(contrast, -1.0..=1.0).text("Contrast"));
        }
        Filter::HueSaturation {
            hue,
            saturation,
            luminosity,
        } => {
            ui.add(egui::Slider::new(hue, -180.0..=180.0).text("Hue"));
            ui.add(egui::Slider::new(saturation, -1.0..=1.0).text("Saturation"));
            ui.add(egui::Slider::new(luminosity, -1.0..=1.0).text("Luminosity"));
        }
        Filter::Posterize { levels } => {
            ui.add(egui::Slider::new(levels, 2..=32).text("Levels"));
        }
        Filter::Binarize { threshold } => {
            ui.add(egui::Slider::new(threshold, 0.0..=1.0).text("Threshold"));
        }
        Filter::GaussianBlur { radius } => {
            ui.add(
                egui::Slider::new(radius, 0.5..=100.0)
                    .logarithmic(true)
                    .text("Radius"),
            );
        }
        Filter::Mosaic { size } => {
            ui.add(
                egui::Slider::new(size, 2..=200)
                    .logarithmic(true)
                    .text("Block size"),
            );
        }
        Filter::MotionBlur { angle, distance } => {
            ui.add(egui::Slider::new(angle, 0.0..=360.0).text("Angle"));
            ui.add(egui::Slider::new(distance, 1.0..=200.0).text("Distance"));
        }
        Filter::RadialBlur { angle } => {
            ui.add(egui::Slider::new(angle, 1.0..=90.0).text("Angle"));
        }
        Filter::ZoomBlur { amount } | Filter::Fisheye { amount } | Filter::Retro { amount } => {
            ui.add(egui::Slider::new(amount, 0.0..=1.0).text("Amount"));
        }
        Filter::Pinch { amount } => {
            ui.add(egui::Slider::new(amount, -1.0..=1.0).text("Amount"));
        }
        Filter::Dust { size } => {
            ui.add(egui::Slider::new(size, 1..=100).text("Dust size"));
        }
        Filter::LineWidth { by } => {
            ui.add(egui::Slider::new(by, -10.0..=10.0).text("Width"));
        }
        Filter::Wave {
            amplitude,
            wavelength,
        }
        | Filter::Ripple {
            amplitude,
            wavelength,
        } => {
            ui.add(egui::Slider::new(amplitude, 0.0..=50.0).text("Amplitude"));
            ui.add(egui::Slider::new(wavelength, 2.0..=400.0).text("Wavelength"));
        }
        Filter::Twirl { angle } => {
            ui.add(egui::Slider::new(angle, -720.0..=720.0).text(tr!("angle")));
        }
        Filter::ZigZag { amplitude, folds } => {
            ui.add(egui::Slider::new(amplitude, 0.0..=100.0).text(tr!("amplitude")));
            ui.add(egui::Slider::new(folds, 1.0..=20.0).text(tr!("folds")));
        }
        Filter::SmartSmooth { strength } => {
            ui.add(egui::Slider::new(strength, 0.0..=1.0).text(tr!("smoothness")));
        }
        Filter::Quick { strength, .. } => {
            ui.add(egui::Slider::new(strength, 0.0..=1.0).text(tr!("strength")));
        }
        Filter::LensBlur { radius } => {
            ui.add(egui::Slider::new(radius, 1..=64).text(tr!("radius")));
        }
        Filter::JpegNoise { strength } => {
            ui.add(egui::Slider::new(strength, 0.0..=1.0).text(tr!("strength")));
        }
        Filter::Crystallize { size } => {
            ui.add(egui::Slider::new(size, 2..=200).text(tr!("cell-size")));
        }
        Filter::Polar { inverse } => {
            ui.radio_value(inverse, false, tr!("rectangular-to-polar"));
            ui.radio_value(inverse, true, tr!("polar-to-rectangular"));
        }
        Filter::Aberration { shift } => {
            ui.add(egui::Slider::new(shift, 0.0..=40.0).text("Shift"));
        }
        Filter::Glow {
            radius,
            strength,
            rgb,
        } => {
            ui.add(egui::Slider::new(radius, 1.0..=200.0).text(tr!("glow-size")));
            ui.add(egui::Slider::new(strength, 0.0..=2.0).text(tr!("glow-strength")));
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(rgb);
                ui.label(tr!("color"));
            });
        }
        Filter::DropShadow {
            dx,
            dy,
            blur,
            opacity,
            rgb,
        } => {
            ui.add(egui::Slider::new(dx, -200.0..=200.0).text(tr!("shadow-across")));
            ui.add(egui::Slider::new(dy, -200.0..=200.0).text(tr!("shadow-down")));
            ui.add(egui::Slider::new(blur, 0.0..=100.0).text(tr!("shadow-softness")));
            ui.add(egui::Slider::new(opacity, 0.0..=1.0).text(tr!("opacity")));
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(rgb);
                ui.label(tr!("color"));
            });
        }
        Filter::Inflate { size, depth } => {
            ui.add(egui::Slider::new(size, 1.0..=100.0).text(tr!("inflate-size")));
            ui.add(egui::Slider::new(depth, 0.0..=1.0).text(tr!("inflate-depth")));
        }
        Filter::PerlinNoise { scale, contrast } => {
            ui.add(egui::Slider::new(scale, 2.0..=500.0).text("Scale"));
            ui.add(egui::Slider::new(contrast, 0.0..=2.0).text("Contrast"));
        }
        Filter::Unsharp {
            radius,
            amount,
            threshold,
        } => {
            ui.add(egui::Slider::new(radius, 0.5..=50.0).text("Radius"));
            ui.add(egui::Slider::new(amount, 0.0..=5.0).text("Amount"));
            ui.add(egui::Slider::new(threshold, 0.0..=1.0).text("Threshold"));
        }
        Filter::LineExtraction {
            sensitivity,
            black,
            smooth,
        } => {
            ui.add(egui::Slider::new(sensitivity, 0.0..=1.0).text("Edge sensitivity"));
            ui.add(egui::Slider::new(black, 0.0..=1.0).text("Black area"));
            ui.add(egui::Slider::new(smooth, 0.5..=8.0).text("Smoothing"));
        }
        Filter::Noise { amount } => {
            ui.add(egui::Slider::new(amount, 0.0..=1.0).text("Amount"));
        }
        Filter::Sharpen { amount } => {
            ui.add(egui::Slider::new(amount, 0.0..=3.0).text("Amount"));
        }
        Filter::ToneCurve {
            rgb,
            red,
            green,
            blue,
            channel,
        } => {
            ui.horizontal(|ui| {
                for (index, name) in ["RGB", &tr!("red"), &tr!("green"), &tr!("blue")]
                    .iter()
                    .enumerate()
                {
                    ui.selectable_value(channel, index as u8, *name);
                }
            });
            let curve = match channel {
                1 => red,
                2 => green,
                3 => blue,
                _ => rgb,
            };
            widgets::response_curve(
                ui,
                ("tone-curve", *channel),
                curve,
                180.0,
                widgets::CurveLabels {
                    input: tr!("input"),
                    low: "0".to_owned(),
                    high: "255".to_owned(),
                    output: tr!("output"),
                },
                theme,
            );
        }
        Filter::Levels {
            black,
            white,
            gamma,
            out_black,
            out_white,
        } => {
            ui.label(RichText::new(tr!("input")).color(theme.muted));
            ui.add(egui::Slider::new(black, 0.0..=0.99).text("Shadow"));
            ui.add(
                egui::Slider::new(gamma, 0.1..=10.0)
                    .logarithmic(true)
                    .text("Gamma"),
            );
            ui.add(egui::Slider::new(white, 0.01..=1.0).text("Highlight"));
            ui.label(RichText::new(tr!("output")).color(theme.muted));
            ui.add(egui::Slider::new(out_black, 0.0..=1.0).text("Shadow"));
            ui.add(egui::Slider::new(out_white, 0.0..=1.0).text("Highlight"));
        }
        Filter::ColorBalance { shifts, range } => {
            ui.horizontal(|ui| {
                for (index, name) in [tr!("shadow"), tr!("midtone"), tr!("highlight")]
                    .iter()
                    .enumerate()
                {
                    ui.selectable_value(range, index as u8, name.as_str());
                }
            });
            let shift = &mut shifts[usize::from((*range).min(2))];
            ui.add(egui::Slider::new(&mut shift[0], -1.0..=1.0).text("Cyan to red"));
            ui.add(egui::Slider::new(&mut shift[1], -1.0..=1.0).text("Magenta to green"));
            ui.add(egui::Slider::new(&mut shift[2], -1.0..=1.0).text("Yellow to blue"));
        }
        Filter::GradientMap { from, to } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("black-becomes")).color(theme.muted));
                ui.color_edit_button_srgb(from);
                ui.label(RichText::new(tr!("white-becomes")).color(theme.muted));
                ui.color_edit_button_srgb(to);
            });
        }
        Filter::FocusLines {
            count,
            gap,
            width,
            jitter,
            seed,
            ..
        } => {
            ui.add(
                egui::Slider::new(count, 8..=600)
                    .logarithmic(true)
                    .text("Lines"),
            );
            ui.add(egui::Slider::new(gap, 0.0..=0.95).text("Clear middle"));
            ui.add(
                egui::Slider::new(width, 0.5..=40.0)
                    .logarithmic(true)
                    .text("Width (px)"),
            );
            ui.add(egui::Slider::new(jitter, 0.0..=1.0).text("Length variation"));
            ui.add(egui::Slider::new(seed, 1..=999).text("Scatter"));
        }
        Filter::SpeedLines {
            count,
            angle,
            width,
            jitter,
            seed,
            ..
        } => {
            ui.add(
                egui::Slider::new(count, 4..=600)
                    .logarithmic(true)
                    .text("Lines"),
            );
            ui.add(egui::Slider::new(angle, -180.0..=180.0).text("Angle"));
            ui.add(
                egui::Slider::new(width, 0.5..=40.0)
                    .logarithmic(true)
                    .text("Width (px)"),
            );
            ui.add(egui::Slider::new(jitter, 0.0..=1.0).text("Length variation"));
            ui.add(egui::Slider::new(seed, 1..=999).text("Scatter"));
        }
        Filter::Tone {
            density,
            cell,
            angle,
            ..
        } => {
            let mut percent = *density * 100.0;
            if ui
                .add(egui::Slider::new(&mut percent, 1.0..=90.0).text("Density (%)"))
                .changed()
            {
                *density = percent / 100.0;
            }
            ui.add(egui::Slider::new(cell, 3.0..=60.0).text("Dot spacing (px)"));
            ui.add(egui::Slider::new(angle, 0.0..=90.0).text("Angle"));
        }
        Filter::Invert
        | Filter::Smooth
        | Filter::Clouds { .. }
        | Filter::Recolor { .. }
        | Filter::BrightnessToOpacity { .. } => {}
    }
}

/// A filter dialog's body: its settings, the preview switch, OK and Cancel.
pub(super) fn filter_ui(ui: &mut Ui, dialog: &mut FilterDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    if let Filter::Quick { .. } = dialog.filter {
        quick_gallery(ui, dialog, theme);
    }
    filter_params(ui, &mut dialog.filter, theme);
    ui.checkbox(&mut dialog.preview, tr!("preview"));
    ui.label(
        RichText::new(if dialog.selection.is_some() {
            tr!("changes-selected-area-layer")
        } else {
            tr!("changes-the-whole-layer")
        })
        .size(theme.small_size)
        .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button(tr!("cancel")).clicked() {
            outcome = Outcome::Cancel;
        }
        if ui
            .add(egui::Button::new(RichText::new("OK").strong()).min_size(vec2(72.0, 0.0)))
            .clicked()
        {
            outcome = Outcome::Ok;
        }
    });
    outcome
}

/// The Quick Filters' or Quick Effects' gallery: every look as a small
/// picture of this layer with its name; a click picks it.
fn quick_gallery(ui: &mut Ui, dialog: &mut FilterDialog, theme: &Theme) {
    let Filter::Quick {
        effect,
        look,
        strength,
    } = dialog.filter
    else {
        return;
    };
    let count = usize::from(crate::quick::LOOKS);
    if dialog.thumbs.0.len() != count {
        // The layer, small, once; each look on it.
        let [width, height] = dialog.size;
        let scale = (72.0 / width.max(height).max(1) as f32).min(1.0);
        let (tw, th) = (
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
        );
        let mut small = vec![0_u8; (tw * th * 4) as usize];
        for y in 0..th {
            for x in 0..tw {
                let (sx, sy) = (
                    ((x as f32 + 0.5) / scale) as u32,
                    ((y as f32 + 0.5) / scale) as u32,
                );
                let from = ((sy.min(height - 1) * width + sx.min(width - 1)) * 4) as usize;
                let to = ((y * tw + x) * 4) as usize;
                if let Some(pixel) = dialog.before.get(from..from + 4) {
                    small[to..to + 4].copy_from_slice(pixel);
                }
            }
        }
        dialog.thumbs.0 = (0..count)
            .map(|index| {
                let mut pixels = small.clone();
                crate::quick::apply(&mut pixels, tw, th, effect, index as u8, 1.0);
                // Over white, as the layer shows on paper.
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    let a = f32::from(pixel[3]) / 255.0;
                    for value in &mut pixel[..3] {
                        *value = (f32::from(*value) * a + 255.0 * (1.0 - a)).round() as u8;
                    }
                    pixel[3] = 255;
                }
                ui.ctx().load_texture(
                    format!("sue-paint-quick-{effect}-{index}"),
                    egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &pixels),
                    egui::TextureOptions::LINEAR,
                )
            })
            .collect();
    }
    let tile = vec2(84.0, 92.0);
    let columns = ((ui.available_width() / (tile.x + 6.0)) as usize).max(1);
    egui::Grid::new(("sue-paint-quick", effect))
        .spacing(vec2(6.0, 6.0))
        .show(ui, |ui| {
            for (index, texture) in dialog.thumbs.0.iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
                let chosen = usize::from(look) == index;
                if chosen {
                    ui.painter()
                        .rect_filled(rect, 4.0, theme.selection.gamma_multiply(0.5));
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, 4.0, theme.hover);
                }
                let picture = Rect::from_center_size(
                    pos2(rect.center().x, rect.top() + 38.0),
                    texture.size_vec2(),
                );
                ui.painter().image(
                    texture.id(),
                    picture,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - 10.0),
                    egui::Align2::CENTER_CENTER,
                    crate::quick::name(effect, index as u8),
                    theme.font(theme.small_size),
                    theme.text,
                );
                if response.clicked() {
                    dialog.filter = Filter::Quick {
                        effect,
                        look: index as u8,
                        strength,
                    };
                }
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}

/// A correction layer's settings.
pub(super) fn correction_ui(ui: &mut Ui, filter: &mut Filter, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    filter_params(ui, filter, theme);
    ui.label(
        RichText::new(tr!("changes-every-layer-under-one"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

impl SuePaint {
    /// Cancel on a filter: the layer as it was, and a layer merged for it
    /// taken away.
    pub(super) fn filter_cancel(&mut self, cx: &mut AppletContext<'_>, filter: &FilterDialog) {
        if filter.shown.is_some() {
            filter.show(cx, &filter.before);
        }
        if filter.merged {
            // Take back the merged layer made for it.
            self.run("edit.undo", cx);
        }
    }

    /// Shows a correction layer's setting live while its dialog is open.
    pub(super) fn show_correction(cx: &mut AppletContext<'_>, layer: EntityId, filter: &Filter) {
        if let Some(text) = filter.to_text()
            && let Ok(entity) = cx.world.arena.get_by_id(layer)
            && entity
                .property(doc::CORRECTION_FILTER)
                .and_then(sue_core::Value::as_str)
                != Some(text.as_str())
            && let Ok(entity) = cx.world.arena.get_by_id_mut(layer)
        {
            // The setting shows live while the dialog is open.
            entity.properties_mut().set(doc::CORRECTION_FILTER, text);
        }
    }

    /// OK on a correction layer's settings.
    pub(super) fn correction_ok(
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        filter: &Filter,
        before: &str,
    ) {
        let text = filter.to_text().unwrap_or_default();
        // Back as it was, then the change as one undo step.
        Self::set_correction(cx, layer, before);
        if text != before {
            Self::apply(cx, doc::set(layer, doc::CORRECTION_FILTER, text));
        }
    }

    /// Cancel on a correction layer's settings: the setting as it was,
    /// and the layer taken away again when the dialog made it.
    pub(super) fn correction_cancel(
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        before: &str,
        made: bool,
    ) {
        Self::set_correction(cx, layer, before);
        if made {
            let _ = cx.undo(false);
        }
    }

    /// The setting shows live while the dialog is open: writes `text`
    /// on the layer in place (a preview, not history).
    fn set_correction(cx: &mut AppletContext<'_>, layer: EntityId, text: &str) {
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            entity
                .properties_mut()
                .set(doc::CORRECTION_FILTER, text.to_owned());
        }
    }
}
