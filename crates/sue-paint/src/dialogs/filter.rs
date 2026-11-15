//! The filter dialogs: their settings with a live preview, the quick
//! looks' gallery, the gradient map's stops and sets, and keeping a
//! filter's result.
//!
//! Every control is one of Sue's own (`sue_wm::ui`): it shows a focus
//! ring when the keys reach it, and a tooltip with its name.

use std::path::PathBuf;

use egui::{Rect, Sense, Ui, pos2, vec2};
use sue_core::{EntityId, RasterTile2D};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, ArenaEdit, Theme, widgets};

use super::Outcome;
use crate::doc;
use crate::filters::{Filter, LEVELS_SAME, SMART_LEVELS};
use crate::gradient_map::{GradientMap, GradientSet, NamedGradient, SET_EXTENSION, StopColor};
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
    /// The main and sub drawing colors when it opened, for gradient stops
    /// that take them.
    pub colors: [[u8; 3]; 2],
    /// The layer, small, for the pad that places a blur's centre.
    pub pad: Thumbs,
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
            filter: filter.upgraded(),
            layer,
            before: raster.bytes().map(<[u8]>::to_vec).unwrap_or_default(),
            size: [raster.width(), raster.height()],
            selection,
            offset,
            shown: None,
            preview: true,
            thumbs: Thumbs::default(),
            merged: false,
            colors: [[0, 0, 0], [255, 255, 255]],
            pad: Thumbs::default(),
        }
    }

    /// The same dialog with the drawing colors `main` and `sub`: gradient
    /// stops that take them show them.
    #[must_use]
    pub(crate) fn with_colors(mut self, main: [u8; 3], sub: [u8; 3]) -> Self {
        self.colors = [main, sub];
        if let Filter::Gradient { map } = &mut self.filter {
            *map = map.with_colors(main, sub);
        }
        self
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
                | Filter::CurvedSurface { .. }
                | Filter::Artistic { .. }
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

/// How large a filter's dialog is before the screen clamps it, so every
/// control and OK and Cancel fit.
pub(super) fn dialog_size(filter: &Filter) -> egui::Vec2 {
    vec2(width(filter), rows(filter).mul_add(ROW, BASE))
}

/// How large a correction layer's dialog is.
pub(super) fn correction_size(filter: &Filter) -> egui::Vec2 {
    vec2(width(filter), rows(filter).mul_add(ROW, BASE - ROW))
}

/// A dialog's width: the gradient map's two columns need more.
fn width(filter: &Filter) -> f32 {
    match filter.upgraded() {
        Filter::Gradient { .. } => WIDE,
        _ => NARROW,
    }
}

/// The width of most filter dialogs, and of the gradient map's.
const NARROW: f32 = 400.0;
const WIDE: f32 = 680.0;

/// A row of controls in a dialog, with its gap.
const ROW: f32 = 38.0;
/// The dialog's frame, its title and the row of OK and Cancel.
const BASE: f32 = 190.0;

/// How many rows of controls a filter's settings take.
fn rows(filter: &Filter) -> f32 {
    match filter.upgraded() {
        Filter::Gradient { .. } => 12.0,
        Filter::ChannelLevels { .. } => 8.0,
        Filter::ToneCurve { .. } => 8.0,
        Filter::Artistic { .. } => 11.5,
        Filter::RadialBlur { .. } | Filter::ZoomBlur { .. } => 8.0,
        Filter::PerlinNoise { .. } => 6.0,
        Filter::DropShadow { .. } => 6.0,
        Filter::FocusLines { .. } | Filter::SpeedLines { .. } => 5.0,
        Filter::ColorBalance { .. } | Filter::SmartSmooth { .. } | Filter::Glow { .. } => 4.0,
        _ => 3.0,
    }
}

// ----- The gradient sets ----------------------------------------------------------

/// The gradient sets: Sue's own, then each file in the data folder's
/// `gradient_sets` folder; which one is shown, the gradient picked in it
/// and the stop being edited.
#[derive(Clone, Debug, Default)]
pub(crate) struct GradientLibrary {
    loaded: bool,
    folder: Option<PathBuf>,
    /// Each set and the file it is kept in (none for Sue's own).
    pub sets: Vec<(Option<PathBuf>, GradientSet)>,
    pub set: usize,
    pub picked: Option<usize>,
    pub stop: usize,
    /// What went wrong writing a set, shown in the dialog.
    pub note: Option<String>,
}

/// The folder in Sue Paint's data folder that holds the gradient sets.
const SETS_FOLDER: &str = "gradient_sets";

impl GradientLibrary {
    /// Reads the sets once, from `data` (Sue Paint's data folder).
    pub(crate) fn load(&mut self, data: Option<PathBuf>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.folder = data.map(|folder| folder.join(SETS_FOLDER));
        self.sets = vec![(None, GradientSet::builtin())];
        if let Some(folder) = &self.folder
            && let Ok(entries) = sue_platform::list_dir(folder, false)
        {
            let mut files: Vec<PathBuf> = entries
                .into_iter()
                .map(|entry| entry.path)
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == SET_EXTENSION)
                })
                .collect();
            files.sort();
            for path in files {
                if let Some(set) = sue_platform::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| GradientSet::from_text(&text))
                {
                    self.sets.push((Some(path), set));
                }
            }
        }
    }

    /// The set shown now.
    fn current(&self) -> Option<&(Option<PathBuf>, GradientSet)> {
        self.sets.get(self.set)
    }

    /// A set of people's own to add gradients to: the one shown, or (when
    /// Sue's own is shown) "My Gradients", made when it is missing.
    fn own_set(&mut self) -> Option<usize> {
        if self.current().is_some_and(|(file, _)| file.is_some()) {
            return Some(self.set);
        }
        if let Some(index) = self.sets.iter().position(|(file, _)| file.is_some()) {
            return Some(index);
        }
        let folder = self.folder.clone()?;
        let set = GradientSet {
            name: tr!("gradient-set-mine"),
            gradients: Vec::new(),
        };
        let path = folder.join(format!("my_gradients.{SET_EXTENSION}"));
        self.sets.push((Some(path), set));
        Some(self.sets.len() - 1)
    }

    /// Writes set `index` to its file.
    fn write(&mut self, index: usize) {
        let Some((Some(path), set)) = self.sets.get(index) else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), sue_platform::fs::create_dir_all)
            .and_then(|()| sue_platform::fs::write(path, set.to_text()));
        self.note = written
            .err()
            .map(|error| tr!("gradient-set-not-written", error = error.to_string()));
    }

    /// Adds `map` as a new gradient named `name` to a set of people's own,
    /// picks it and writes the set.
    fn add(&mut self, name: String, map: GradientMap) {
        let Some(index) = self.own_set() else {
            self.note = Some(tr!("sue-paint-has-no-data-folder"));
            return;
        };
        self.sets[index]
            .1
            .gradients
            .push(NamedGradient { name, map });
        self.set = index;
        self.picked = Some(self.sets[index].1.gradients.len() - 1);
        self.write(index);
    }

    /// Takes the picked gradient out of its set (Sue's own set stays as it
    /// ships).
    fn delete(&mut self) {
        let (Some(picked), Some((Some(_), set))) = (self.picked, self.sets.get_mut(self.set))
        else {
            return;
        };
        if picked < set.gradients.len() {
            set.gradients.remove(picked);
            self.picked = None;
            self.write(self.set);
        }
    }

    /// Keeps `map` as the picked gradient of a set of people's own (on
    /// OK), writing the set when it changed.
    pub(crate) fn keep(&mut self, map: &GradientMap) {
        let (Some(picked), Some((Some(_), set))) = (self.picked, self.sets.get_mut(self.set))
        else {
            return;
        };
        if let Some(gradient) = set.gradients.get_mut(picked)
            && gradient.map != *map
        {
            gradient.map = *map;
            self.write(self.set);
        }
    }
}

// ----- Keeping the result ----------------------------------------------------------

impl SuePaint {
    /// Keeps `filter`'s result on its layer and, for a tonal correction,
    /// on every layer picked with it (Clip Studio Paint 5.0), as one step.
    pub(crate) fn keep_filter(&mut self, cx: &mut AppletContext<'_>, filter: &FilterDialog) {
        if let Filter::Gradient { map } = &filter.filter {
            self.gradients.keep(map);
        }
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

    /// A filter's dialog, with the gradient sets read on first use.
    pub(super) fn filter_dialog_ui(
        &mut self,
        ui: &mut Ui,
        dialog: &mut FilterDialog,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        self.gradients.load(crate::colorsets::folder(cx));
        filter_ui(ui, dialog, &mut self.gradients, theme)
    }

    /// A correction layer's dialog, with the gradient sets.
    pub(super) fn correction_dialog_ui(
        &mut self,
        ui: &mut Ui,
        filter: &mut Filter,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        self.gradients.load(crate::colorsets::folder(cx));
        let colors = [self.main, self.sub];
        let mut outcome = Outcome::Open;
        let mut pad = Thumbs::default();
        filter_params(
            ui,
            filter,
            &mut Params {
                colors,
                library: Some(&mut self.gradients),
                layer: None,
                pad: &mut pad,
            },
            theme,
        );
        note(ui, &tr!("changes-every-layer-under-one"), theme);
        ok_cancel(ui, &mut outcome, theme);
        if outcome == Outcome::Ok
            && let Filter::Gradient { map } = filter
        {
            self.gradients.keep(map);
        }
        outcome
    }
}

// ----- Controls -----------------------------------------------------------------

/// What a filter's settings can reach besides the filter.
struct Params<'a> {
    /// The main and sub drawing colors.
    colors: [[u8; 3]; 2],
    /// The gradient sets.
    library: Option<&'a mut GradientLibrary>,
    /// The layer's pixels and size, for the centre pad.
    layer: Option<(&'a [u8], [u32; 2])>,
    /// The centre pad's picture.
    pad: &'a mut Thumbs,
}

/// The room a slider's name takes, so the longest of them fit and the
/// tracks of a dialog line up.
fn label_room(theme: &Theme) -> f32 {
    theme.space(Space::S9) * 3.0
}

/// A slider for `value` with its name before it and in its tooltip.
fn number(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let response = ui::slider(
        ("sue-paint-filter", salt),
        &mut wide,
        f64::from(*range.start())..=f64::from(*range.end()),
    )
    .label(label)
    .label_width(label_room(theme))
    .decimals(decimals)
    .show(ui, theme);
    let changed = response.changed();
    ui::tooltip(response, &Tip::new(label), theme);
    if changed {
        *value = wide as f32;
    }
    changed
}

/// [`number`], its values mapped by their logarithm (sizes).
fn size(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let response = ui::slider(
        ("sue-paint-filter", salt),
        &mut wide,
        f64::from(*range.start())..=f64::from(*range.end()),
    )
    .label(label)
    .label_width(label_room(theme))
    .decimals(1)
    .logarithmic()
    .show(ui, theme);
    let changed = response.changed();
    ui::tooltip(response, &Tip::new(label), theme);
    if changed {
        *value = wide as f32;
    }
    changed
}

/// A slider for a whole number.
fn whole(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let response = ui::slider(
        ("sue-paint-filter", salt),
        &mut wide,
        f64::from(*range.start())..=f64::from(*range.end()),
    )
    .label(label)
    .label_width(label_room(theme))
    .step(1.0)
    .show(ui, theme);
    let changed = response.changed();
    ui::tooltip(response, &Tip::new(label), theme);
    if changed {
        *value = wide.round() as u32;
    }
    changed
}

/// A strip of `options`, `chosen` the one that holds.
fn choice(ui: &mut Ui, salt: &str, options: &[String], chosen: &mut u8, theme: &Theme) -> bool {
    let answer = ui::segmented(
        ("sue-paint-filter", salt),
        options,
        Some(usize::from(*chosen)),
    )
    .show(ui, theme);
    let name = options
        .get(usize::from(*chosen))
        .cloned()
        .unwrap_or_default();
    ui::tooltip(answer.response, &Tip::new(&name), theme);
    match answer.picked {
        Some(index) if index != usize::from(*chosen) => {
            *chosen = index as u8;
            true
        }
        _ => false,
    }
}

/// A checkbox for `value`.
fn check(ui: &mut Ui, salt: &str, label: &str, value: &mut bool, theme: &Theme) -> bool {
    let response = ui::checkbox(("sue-paint-filter", salt), label, *value).show(ui, theme);
    let clicked = response.clicked();
    ui::tooltip(response, &Tip::new(label), theme);
    if clicked {
        *value = !*value;
    }
    clicked
}

/// A color swatch that opens the color wheel, with its name beside it.
fn color(ui: &mut Ui, salt: &str, label: &str, rgb: &mut [u8; 3], theme: &Theme) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut rgba = [rgb[0], rgb[1], rgb[2], 255];
        let pick = sue_wm::color_button(ui, ("sue-paint-filter", salt), &mut rgba, false, theme);
        if pick.changed {
            *rgb = [rgba[0], rgba[1], rgba[2]];
            changed = true;
        }
        ui.label(
            egui::RichText::new(label)
                .font(theme.text_font(TextRole::Body))
                .color(theme.text),
        );
    });
    changed
}

/// A line of muted text under the settings.
fn note(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// A small heading over a group of settings.
fn heading(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// OK and Cancel, OK the primary one, at the right.
fn ok_cancel(ui: &mut Ui, outcome: &mut Outcome, theme: &Theme) {
    ui.add_space(theme.space(Space::S4));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
        let cancel = tr!("cancel");
        let response = ui::button("sue-paint-filter-cancel", &cancel).show(ui, theme);
        if response.clicked() {
            *outcome = Outcome::Cancel;
        }
        ui::tooltip(response, &Tip::new(&cancel).shortcut("Esc"), theme);
        let ok = tr!("ok");
        let response = ui::button("sue-paint-filter-ok", &ok)
            .primary()
            .show(ui, theme);
        if response.clicked() {
            *outcome = Outcome::Ok;
        }
        ui::tooltip(response, &Tip::new(&ok).shortcut("Enter"), theme);
    });
}

// ----- Each filter's settings ----------------------------------------------------------

/// A filter's or correction's settings.
fn filter_params(ui: &mut Ui, filter: &mut Filter, params: &mut Params<'_>, theme: &Theme) {
    // The forms a dialog edits: Level Correction with its channels and
    // Gradient Map with stops.
    *filter = filter.upgraded();
    match filter {
        Filter::BrightnessContrast {
            brightness,
            contrast,
        } => {
            number(
                ui,
                "brightness",
                &tr!("brightness"),
                brightness,
                -1.0..=1.0,
                2,
                theme,
            );
            number(
                ui,
                "contrast",
                &tr!("contrast"),
                contrast,
                -1.0..=1.0,
                2,
                theme,
            );
        }
        Filter::HueSaturation {
            hue,
            saturation,
            luminosity,
        } => {
            number(ui, "hue", &tr!("hue"), hue, -180.0..=180.0, 0, theme);
            number(
                ui,
                "saturation",
                &tr!("saturation"),
                saturation,
                -1.0..=1.0,
                2,
                theme,
            );
            number(
                ui,
                "luminosity",
                &tr!("luminosity"),
                luminosity,
                -1.0..=1.0,
                2,
                theme,
            );
        }
        Filter::Posterize { levels } => {
            whole(ui, "levels", &tr!("filter-levels"), levels, 2..=32, theme);
        }
        Filter::Binarize { threshold } => {
            number(
                ui,
                "threshold",
                &tr!("threshold"),
                threshold,
                0.0..=1.0,
                2,
                theme,
            );
        }
        Filter::GaussianBlur { radius } => {
            size(ui, "radius", &tr!("radius"), radius, 0.5..=100.0, theme);
        }
        Filter::Mosaic { size: block } => {
            whole(ui, "block", &tr!("block-size"), block, 2..=200, theme);
        }
        Filter::MotionBlur { angle, distance } => {
            number(ui, "angle", &tr!("angle"), angle, 0.0..=360.0, 0, theme);
            number(
                ui,
                "distance",
                &tr!("distance"),
                distance,
                1.0..=200.0,
                0,
                theme,
            );
        }
        Filter::RadialBlur { angle, center } => {
            number(ui, "angle", &tr!("angle"), angle, 1.0..=90.0, 0, theme);
            center_pad(ui, center, params, theme);
        }
        Filter::ZoomBlur { amount, center } => {
            number(ui, "amount", &tr!("amount"), amount, 0.0..=1.0, 2, theme);
            center_pad(ui, center, params, theme);
        }
        Filter::Fisheye { amount } | Filter::Retro { amount } => {
            number(ui, "amount", &tr!("amount"), amount, 0.0..=1.0, 2, theme);
        }
        Filter::Pinch { amount } => {
            number(ui, "amount", &tr!("amount"), amount, -1.0..=1.0, 2, theme);
        }
        Filter::Dust { size: dust } => {
            whole(ui, "dust", &tr!("dust-size"), dust, 1..=100, theme);
        }
        Filter::LineWidth { by } => {
            number(ui, "width", &tr!("width"), by, -10.0..=10.0, 1, theme);
        }
        Filter::Wave {
            amplitude,
            wavelength,
        }
        | Filter::Ripple {
            amplitude,
            wavelength,
        } => {
            number(
                ui,
                "amplitude",
                &tr!("amplitude"),
                amplitude,
                0.0..=50.0,
                1,
                theme,
            );
            number(
                ui,
                "wavelength",
                &tr!("wavelength"),
                wavelength,
                2.0..=400.0,
                0,
                theme,
            );
        }
        Filter::Twirl { angle } => {
            number(ui, "angle", &tr!("angle"), angle, -720.0..=720.0, 0, theme);
        }
        Filter::ZigZag { amplitude, folds } => {
            number(
                ui,
                "amplitude",
                &tr!("amplitude"),
                amplitude,
                0.0..=100.0,
                1,
                theme,
            );
            number(ui, "folds", &tr!("folds"), folds, 1.0..=20.0, 1, theme);
        }
        Filter::SmartSmooth {
            strength,
            remove_noise,
        } => smart_smoothing(ui, strength, remove_noise, theme),
        Filter::Quick { strength, .. } | Filter::JpegNoise { strength } => {
            number(
                ui,
                "strength",
                &tr!("strength"),
                strength,
                0.0..=1.0,
                2,
                theme,
            );
        }
        Filter::LensBlur { radius } => {
            whole(ui, "radius", &tr!("radius"), radius, 1..=64, theme);
        }
        Filter::Crystallize { size: cell } => {
            whole(ui, "cell", &tr!("cell-size"), cell, 2..=200, theme);
        }
        Filter::Polar { inverse } => {
            let mut way = u8::from(*inverse);
            let options = [tr!("rectangular-to-polar"), tr!("polar-to-rectangular")];
            if choice(ui, "polar", &options, &mut way, theme) {
                *inverse = way == 1;
            }
        }
        Filter::Aberration { shift } => {
            number(ui, "shift", &tr!("shift"), shift, 0.0..=40.0, 1, theme);
        }
        Filter::Glow {
            radius,
            strength,
            rgb,
        } => {
            number(
                ui,
                "glow-size",
                &tr!("glow-size"),
                radius,
                1.0..=200.0,
                0,
                theme,
            );
            number(
                ui,
                "glow-strength",
                &tr!("glow-strength"),
                strength,
                0.0..=2.0,
                2,
                theme,
            );
            color(ui, "glow-color", &tr!("filter-color"), rgb, theme);
        }
        Filter::DropShadow {
            dx,
            dy,
            blur,
            opacity,
            rgb,
        } => {
            number(
                ui,
                "across",
                &tr!("shadow-across"),
                dx,
                -200.0..=200.0,
                0,
                theme,
            );
            number(
                ui,
                "down",
                &tr!("shadow-down"),
                dy,
                -200.0..=200.0,
                0,
                theme,
            );
            number(
                ui,
                "soft",
                &tr!("shadow-softness"),
                blur,
                0.0..=100.0,
                0,
                theme,
            );
            number(ui, "opacity", &tr!("opacity"), opacity, 0.0..=1.0, 2, theme);
            color(ui, "shadow-color", &tr!("filter-color"), rgb, theme);
        }
        Filter::Inflate { size: round, depth } => {
            number(
                ui,
                "inflate-size",
                &tr!("inflate-size"),
                round,
                1.0..=100.0,
                0,
                theme,
            );
            number(
                ui,
                "inflate-depth",
                &tr!("inflate-depth"),
                depth,
                0.0..=1.0,
                2,
                theme,
            );
        }
        Filter::PerlinNoise {
            scale,
            amplitude,
            attenuation,
            repetition,
            offset,
        } => {
            size(ui, "scale", &tr!("scale"), scale, 2.0..=500.0, theme);
            number(
                ui,
                "amplitude",
                &tr!("amplitude"),
                amplitude,
                0.0..=2.0,
                2,
                theme,
            );
            number(
                ui,
                "attenuation",
                &tr!("attenuation"),
                attenuation,
                0.0..=1.0,
                2,
                theme,
            );
            whole(
                ui,
                "repetition",
                &tr!("repetition"),
                repetition,
                1..=8,
                theme,
            );
            number(
                ui,
                "offset-x",
                &tr!("offset-x"),
                &mut offset[0],
                -2000.0..=2000.0,
                0,
                theme,
            );
            number(
                ui,
                "offset-y",
                &tr!("offset-y"),
                &mut offset[1],
                -2000.0..=2000.0,
                0,
                theme,
            );
        }
        Filter::Unsharp {
            radius,
            amount,
            threshold,
        } => {
            number(ui, "radius", &tr!("radius"), radius, 0.5..=50.0, 1, theme);
            number(ui, "amount", &tr!("amount"), amount, 0.0..=5.0, 2, theme);
            number(
                ui,
                "threshold",
                &tr!("threshold"),
                threshold,
                0.0..=1.0,
                2,
                theme,
            );
        }
        Filter::LineExtraction {
            sensitivity,
            black,
            smooth,
        } => {
            number(
                ui,
                "sensitivity",
                &tr!("edge-sensitivity"),
                sensitivity,
                0.0..=1.0,
                2,
                theme,
            );
            number(ui, "black", &tr!("black-area"), black, 0.0..=1.0, 2, theme);
            number(
                ui,
                "smoothing",
                &tr!("smoothing"),
                smooth,
                0.5..=8.0,
                1,
                theme,
            );
        }
        Filter::Noise { amount } => {
            number(ui, "amount", &tr!("amount"), amount, 0.0..=1.0, 2, theme);
        }
        Filter::Sharpen { amount } => {
            number(ui, "amount", &tr!("amount"), amount, 0.0..=3.0, 2, theme);
        }
        Filter::ToneCurve {
            rgb,
            red,
            green,
            blue,
            channel,
        } => {
            let channels = channel_names();
            choice(ui, "curve-channel", &channels, channel, theme);
            let curve = match channel {
                1 => red,
                2 => green,
                3 => blue,
                _ => rgb,
            };
            let side = Density::of(ui).control_height() * 6.0;
            curve_ui(ui, ("tone-curve", *channel), curve, side, theme);
        }
        Filter::ChannelLevels { sets, channel } => {
            let channels = channel_names();
            choice(ui, "levels-channel", &channels, channel, theme);
            let set = &mut sets[usize::from((*channel).min(3))];
            heading(ui, &tr!("input"), theme);
            number(
                ui,
                "in-shadow",
                &tr!("shadow"),
                &mut set[0],
                0.0..=0.99,
                2,
                theme,
            );
            number(
                ui,
                "in-gamma",
                &tr!("gamma"),
                &mut set[2],
                0.1..=10.0,
                2,
                theme,
            );
            number(
                ui,
                "in-highlight",
                &tr!("highlight"),
                &mut set[1],
                0.01..=1.0,
                2,
                theme,
            );
            heading(ui, &tr!("output"), theme);
            number(
                ui,
                "out-shadow",
                &tr!("shadow"),
                &mut set[3],
                0.0..=1.0,
                2,
                theme,
            );
            number(
                ui,
                "out-highlight",
                &tr!("highlight"),
                &mut set[4],
                0.0..=1.0,
                2,
                theme,
            );
            let reset = tr!("reset-channel");
            let response = ui::button("sue-paint-levels-reset", &reset)
                .ghost()
                .enabled(*set != LEVELS_SAME)
                .show(ui, theme);
            if response.clicked() {
                *set = LEVELS_SAME;
            }
            ui::tooltip(response, &Tip::new(&reset), theme);
        }
        Filter::ColorBalance { shifts, range } => {
            let ranges = [tr!("shadow"), tr!("midtone"), tr!("highlight")];
            choice(ui, "balance-range", &ranges, range, theme);
            let shift = &mut shifts[usize::from((*range).min(2))];
            number(
                ui,
                "cyan-red",
                &tr!("cyan-to-red"),
                &mut shift[0],
                -1.0..=1.0,
                2,
                theme,
            );
            number(
                ui,
                "magenta-green",
                &tr!("magenta-to-green"),
                &mut shift[1],
                -1.0..=1.0,
                2,
                theme,
            );
            number(
                ui,
                "yellow-blue",
                &tr!("yellow-to-blue"),
                &mut shift[2],
                -1.0..=1.0,
                2,
                theme,
            );
        }
        Filter::Gradient { map } => gradient_ui(ui, map, params, theme),
        Filter::FocusLines {
            count,
            gap,
            width,
            jitter,
            seed,
            ..
        } => {
            whole(ui, "lines", &tr!("filter-lines"), count, 8..=600, theme);
            number(ui, "gap", &tr!("clear-middle"), gap, 0.0..=0.95, 2, theme);
            size(ui, "width", &tr!("width-px"), width, 0.5..=40.0, theme);
            number(
                ui,
                "jitter",
                &tr!("length-variation"),
                jitter,
                0.0..=1.0,
                2,
                theme,
            );
            whole(ui, "seed", &tr!("scatter"), seed, 1..=999, theme);
        }
        Filter::SpeedLines {
            count,
            angle,
            width,
            jitter,
            seed,
            ..
        } => {
            whole(ui, "lines", &tr!("filter-lines"), count, 4..=600, theme);
            number(ui, "angle", &tr!("angle"), angle, -180.0..=180.0, 0, theme);
            size(ui, "width", &tr!("width-px"), width, 0.5..=40.0, theme);
            number(
                ui,
                "jitter",
                &tr!("length-variation"),
                jitter,
                0.0..=1.0,
                2,
                theme,
            );
            whole(ui, "seed", &tr!("scatter"), seed, 1..=999, theme);
        }
        Filter::Tone {
            density,
            cell,
            angle,
            ..
        } => {
            let mut percent = *density * 100.0;
            if number(
                ui,
                "density",
                &tr!("density-percent"),
                &mut percent,
                1.0..=90.0,
                0,
                theme,
            ) {
                *density = percent / 100.0;
            }
            number(
                ui,
                "cell",
                &tr!("dot-spacing-px"),
                cell,
                3.0..=60.0,
                1,
                theme,
            );
            number(ui, "angle", &tr!("angle"), angle, 0.0..=90.0, 0, theme);
        }
        Filter::CurvedSurface { amount, axis } => {
            number(ui, "amount", &tr!("amount"), amount, -1.0..=1.0, 2, theme);
            let ways = [
                tr!("curved-ball"),
                tr!("curved-across"),
                tr!("curved-up-down"),
            ];
            choice(ui, "curved-axis", &ways, axis, theme);
        }
        Filter::Artistic {
            process,
            line_width,
            simplicity,
            density,
            line_opacity,
            antialias,
            blending,
            color_blur,
            colors,
        } => {
            let ways = [
                tr!("artistic-colors-and-lines"),
                tr!("artistic-lines"),
                tr!("artistic-colors"),
            ];
            choice(ui, "artistic-process", &ways, process, theme);
            let lines = *process != 2;
            let colored = *process != 1;
            ui.add_enabled_ui(lines, |ui| {
                heading(ui, &tr!("artistic-line"), theme);
                number(
                    ui,
                    "line-width",
                    &tr!("line-thickness"),
                    line_width,
                    1.0..=10.0,
                    1,
                    theme,
                );
                number(
                    ui,
                    "simplicity",
                    &tr!("line-simplicity"),
                    simplicity,
                    0.0..=1.0,
                    2,
                    theme,
                );
                number(
                    ui,
                    "density",
                    &tr!("line-density"),
                    density,
                    0.0..=1.0,
                    2,
                    theme,
                );
                number(
                    ui,
                    "line-opacity",
                    &tr!("artistic-line-opacity"),
                    line_opacity,
                    0.0..=1.0,
                    2,
                    theme,
                );
                check(
                    ui,
                    "antialias",
                    &tr!("line-anti-aliasing"),
                    antialias,
                    theme,
                );
            });
            ui.add_enabled_ui(colored, |ui| {
                heading(ui, &tr!("artistic-color"), theme);
                number(
                    ui,
                    "blending",
                    &tr!("color-blending"),
                    blending,
                    0.0..=1.0,
                    2,
                    theme,
                );
                number(
                    ui,
                    "color-blur",
                    &tr!("color-blur"),
                    color_blur,
                    0.0..=20.0,
                    1,
                    theme,
                );
                whole(
                    ui,
                    "colors",
                    &tr!("number-of-colors"),
                    colors,
                    2..=32,
                    theme,
                );
            });
        }
        Filter::Levels { .. }
        | Filter::GradientMap { .. }
        | Filter::Invert
        | Filter::Smooth
        | Filter::Clouds { .. }
        | Filter::Recolor { .. }
        | Filter::BrightnessToOpacity { .. } => {}
    }
}

/// RGB, then red, green and blue.
fn channel_names() -> [String; 4] {
    [tr!("rgb"), tr!("red"), tr!("green"), tr!("blue")]
}

/// Smart Smoothing: Weak, Medium or Strong, and Remove noise.
fn smart_smoothing(ui: &mut Ui, strength: &mut f32, remove_noise: &mut bool, theme: &Theme) {
    let mut level = SMART_LEVELS
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - *strength).abs().total_cmp(&(b.1 - *strength).abs()))
        .map_or(1, |(index, _)| index as u8);
    heading(ui, &tr!("intensity"), theme);
    let levels = [tr!("weak"), tr!("medium"), tr!("strong")];
    if choice(ui, "smart-level", &levels, &mut level, theme) {
        *strength = SMART_LEVELS[usize::from(level.min(2))];
    }
    check(
        ui,
        "remove-noise",
        &tr!("remove-noise"),
        remove_noise,
        theme,
    );
}

/// A response curve, drawn by Sue's shared curve editor.
fn curve_ui(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    curve: &mut sue_model::Response,
    side: f32,
    theme: &Theme,
) -> bool {
    widgets::response_curve(
        ui,
        id,
        curve,
        side,
        widgets::CurveLabels {
            input: tr!("input"),
            low: "0".to_owned(),
            high: "255".to_owned(),
            output: tr!("output"),
        },
        theme,
    )
}

/// A blur's centre: the layer small, the centre marked on it; a click or
/// a drag puts it there. The numbers beside it type it.
fn center_pad(ui: &mut Ui, center: &mut [f32; 2], params: &mut Params<'_>, theme: &Theme) {
    heading(ui, &tr!("blur-center"), theme);
    let side = Density::of(ui).control_height() * 4.0;
    ui.horizontal(|ui| {
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(side), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let radius = theme.radius(Radius::Sm);
        painter.rect_filled(rect, radius, theme.surface(1));
        // The layer, fitted in the pad.
        let mut picture = rect;
        if let Some((pixels, [width, height])) = params.layer
            && width > 0
            && height > 0
        {
            let fit = side / width.max(height) as f32;
            picture = Rect::from_center_size(
                rect.center(),
                egui::Vec2::from([width, height].map(|side| side as f32 * fit)),
            );
            if params.pad.0.is_empty() {
                params
                    .pad
                    .0
                    .push(small_texture(ui.ctx(), pixels, [width, height], side));
            }
            if let Some(texture) = params.pad.0.first() {
                pad_picture(&painter, texture, picture);
            }
        }
        let name = tr!("blur-center");
        if (response.clicked() || response.dragged())
            && let Some(at) = response.interact_pointer_pos()
        {
            center[0] = ((at.x - picture.left()) / picture.width().max(1.0)).clamp(0.0, 1.0);
            center[1] = ((at.y - picture.top()) / picture.height().max(1.0)).clamp(0.0, 1.0);
        }
        // The mark: a ring and a cross in the text color, over a halo of
        // the panel so it reads on any picture.
        let mark = pos2(
            picture.left() + picture.width() * center[0],
            picture.top() + picture.height() * center[1],
        );
        let (reach, span) = (theme.space(Space::S3), theme.space(Space::S5));
        let ink = theme.focus_stroke();
        let halo = egui::Stroke::new(ink.width + theme.selected_stroke().width, theme.panel);
        let ink = egui::Stroke::new(ink.width, theme.text);
        for stroke in [halo, ink] {
            painter.circle_stroke(mark, reach, stroke);
            let (across, down) = (egui::Vec2::X * span, egui::Vec2::Y * span);
            painter.line_segment([mark - across, mark + across], stroke);
            painter.line_segment([mark - down, mark + down], stroke);
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        ui::focus_ring(ui, &response, radius, theme);
        ui::tooltip(
            response,
            &Tip::new(&name).help(&tr!("blur-center-help")),
            theme,
        );
    });
    let mut across = center[0] * 100.0;
    if number(
        ui,
        "center-x",
        &tr!("center-x"),
        &mut across,
        0.0..=100.0,
        0,
        theme,
    ) {
        center[0] = across / 100.0;
    }
    let mut down = center[1] * 100.0;
    if number(
        ui,
        "center-y",
        &tr!("center-y"),
        &mut down,
        0.0..=100.0,
        0,
        theme,
    ) {
        center[1] = down / 100.0;
    }
}

/// The pad's small copy of the layer, drawn in `rect`.
fn pad_picture(painter: &egui::Painter, texture: &egui::TextureHandle, rect: Rect) {
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// `pixels` small, over white, as a texture at most `side` points wide.
fn small_texture(
    ctx: &egui::Context,
    pixels: &[u8],
    [width, height]: [u32; 2],
    side: f32,
) -> egui::TextureHandle {
    let scale = (side * 2.0 / width.max(height).max(1) as f32).min(1.0);
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
            if let Some(pixel) = pixels.get(from..from + 4) {
                let a = u16::from(pixel[3]);
                for channel in 0..3 {
                    small[to + channel] =
                        ((u16::from(pixel[channel]) * a + 255 * (255 - a)) / 255) as u8;
                }
                small[to + 3] = 255;
            }
        }
    }
    ctx.load_texture(
        "sue-paint-filter-pad",
        egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &small),
        egui::TextureOptions::LINEAR,
    )
}

// ----- Gradient Map ------------------------------------------------------------

/// Gradient Map: the set and its gradients, the bar of stops, the stop
/// being edited and the mixing curve.
fn gradient_ui(ui: &mut Ui, map: &mut GradientMap, params: &mut Params<'_>, theme: &Theme) {
    let colors = params.colors;
    let mut stop = params.library.as_deref().map_or(0, |library| library.stop);
    // The sets and their gradients at the left, the gradient being
    // edited at the right.
    ui.columns(2, |columns| {
        if let Some(library) = params.library.as_deref_mut() {
            gradient_sets_ui(&mut columns[0], map, library, colors, theme);
            stop = library.stop;
        }
        let ui = &mut columns[1];
        stop = stop.min(map.stops().len().saturating_sub(1));
        gradient_bar(ui, map, &mut stop, theme);
        stop_ui(ui, map, &mut stop, colors, theme);
        heading(ui, &tr!("mixing-rate"), theme);
        let side = Density::of(ui).control_height() * 4.0;
        curve_ui(ui, "gradient-mix", &mut map.mix, side, theme);
    });
    if let Some(library) = params.library.as_deref_mut() {
        library.stop = stop;
    }
}

/// The set list and its gradients, with Add, Duplicate and Delete.
fn gradient_sets_ui(
    ui: &mut Ui,
    map: &mut GradientMap,
    library: &mut GradientLibrary,
    colors: [[u8; 3]; 2],
    theme: &Theme,
) {
    let names: Vec<String> = library
        .sets
        .iter()
        .map(|(_, set)| set.name.clone())
        .collect();
    let mut set = library.set.min(names.len().saturating_sub(1));
    ui.horizontal(|ui| {
        let label = tr!("gradient-set");
        ui.label(
            egui::RichText::new(&label)
                .font(theme.text_font(TextRole::Body))
                .color(theme.text),
        );
        let response = ui::dropdown("sue-paint-gradient-set", &mut set, &names)
            .label(&label)
            .show(ui, theme);
        ui::tooltip(response, &Tip::new(&label), theme);
    });
    if set != library.set {
        library.set = set;
        library.picked = None;
    }
    // The gradients of the set, each as a strip of its own colors.
    let strip = vec2(ui.available_width(), Density::of(ui).control_height());
    let mut picked = None;
    let list = Density::of(ui).control_height() * 6.0;
    egui::ScrollArea::vertical()
        .id_salt("sue-paint-gradient-list")
        .max_height(list)
        .show(ui, |ui| {
            if let Some((_, current)) = library.current() {
                for (index, gradient) in current.gradients.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(strip, Sense::click());
                    let shown = gradient.map.with_colors(colors[0], colors[1]);
                    let swatch = rect
                        .split_left_right_at_fraction(0.45)
                        .0
                        .shrink(theme.space(Space::S1));
                    paint_gradient(ui.painter(), swatch, &shown, theme);
                    let on = library.picked == Some(index);
                    if on {
                        ui.painter().rect_stroke(
                            rect,
                            theme.radius(Radius::Xs),
                            theme.selected_stroke(),
                            egui::StrokeKind::Inside,
                        );
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            theme.radius(Radius::Xs),
                            theme.fill_hover(),
                        );
                    }
                    ui.painter().text(
                        pos2(swatch.right() + theme.space(Space::S4), rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &gradient.name,
                        theme.text_font(TextRole::Body),
                        theme.text,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, &gradient.name)
                    });
                    ui::focus_ring(ui, &response, theme.radius(Radius::Xs), theme);
                    if response.clicked() {
                        picked = Some((index, shown));
                    }
                    ui::tooltip(response, &Tip::new(&gradient.name), theme);
                }
            }
        });
    if let Some((index, shown)) = picked {
        library.picked = Some(index);
        library.stop = 0;
        *map = shown;
    }
    ui.horizontal(|ui| {
        let own = library.current().is_some_and(|(file, _)| file.is_some());
        let add = tr!("gradient-add");
        if ui::icon_button("sue-paint-gradient-add", Icon::Plus, &add)
            .show(ui, theme)
            .clicked()
        {
            library.add(tr!("gradient-new-name"), *map);
        }
        let duplicate = tr!("gradient-duplicate");
        let can_copy = library.picked.is_some();
        if ui::icon_button("sue-paint-gradient-duplicate", Icon::Duplicate, &duplicate)
            .enabled(can_copy)
            .show(ui, theme)
            .clicked()
            && let Some((_, current)) = library.current()
            && let Some(gradient) = library.picked.and_then(|at| current.gradients.get(at))
        {
            let name = tr!("gradient-copy-name", name = gradient.name.clone());
            let copy = gradient.map;
            library.add(name, copy);
        }
        let delete = tr!("gradient-delete");
        if ui::icon_button("sue-paint-gradient-delete", Icon::Delete, &delete)
            .enabled(own && library.picked.is_some())
            .show(ui, theme)
            .clicked()
        {
            library.delete();
        }
    });
    if let Some(text) = &library.note {
        note(ui, text, theme);
    }
}

/// `map` painted across `rect`, over a checkerboard where it is see
/// through.
fn paint_gradient(painter: &egui::Painter, rect: Rect, map: &GradientMap, theme: &Theme) {
    let steps = (rect.width() / 2.0).ceil().max(1.0) as usize;
    let cell = rect.height() / 2.0;
    // The checkerboard under it.
    let (light, dark) = (theme.surface(0), theme.border_strong());
    let mut x = rect.left();
    let mut column = 0;
    while x < rect.right() {
        for row in 0..2 {
            let shade = if (column + row) % 2 == 0 { light } else { dark };
            let square = Rect::from_min_size(
                pos2(x, rect.top() + cell * row as f32),
                vec2(cell.min(rect.right() - x), cell),
            );
            painter.rect_filled(square, 0.0, shade);
        }
        x += cell;
        column += 1;
    }
    let width = rect.width() / steps as f32;
    for step in 0..steps {
        let t = (step as f32 + 0.5) / steps as f32;
        let [r, g, b, a] = map.raw_at(t);
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        let fill = egui::Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(a));
        painter.rect_filled(
            Rect::from_min_size(
                pos2(rect.left() + width * step as f32, rect.top()),
                vec2(width + 0.5, rect.height()),
            ),
            0.0,
            fill,
        );
    }
    painter.rect_stroke(rect, 0.0, theme.hairline(), egui::StrokeKind::Inside);
}

/// The bar of stops: a click on the bar adds a stop there, a stop is
/// picked by a click and moved by a drag.
fn gradient_bar(ui: &mut Ui, map: &mut GradientMap, stop: &mut usize, theme: &Theme) {
    let density = Density::of(ui);
    let height = density.control_height();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), height + theme.space(Space::S6)),
        Sense::click_and_drag(),
    );
    let bar = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + height))
        .shrink2(egui::Vec2::X * theme.space(Space::S3));
    paint_gradient(ui.painter(), bar, map, theme);
    let handle = theme.space(Space::S3);
    let place = |at: f32| pos2(bar.left() + bar.width() * at, bar.bottom() + handle);
    // The stop under the pointer when it went down, if any.
    let near = |pointer: egui::Pos2, map: &GradientMap| {
        map.stops()
            .iter()
            .enumerate()
            .map(|(index, found)| (index, place(found.at).distance(pointer)))
            .filter(|(_, distance)| *distance <= handle * 2.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    };
    let at_of =
        |pointer: egui::Pos2| ((pointer.x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(index) = near(pointer, map)
    {
        *stop = index;
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(moving) = map.stop_mut(*stop)
    {
        moving.at = at_of(pointer);
        *stop = map.sort_keeping(*stop);
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        match near(pointer, map) {
            Some(index) => *stop = index,
            None => {
                if let Some(index) = map.add_at(at_of(pointer)) {
                    *stop = index;
                }
            }
        }
    }
    // A stop picked, moved or added with the pointer takes the keys
    // next, so Delete right after a click takes that stop away.
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    // Keys: Left and Right move the stop being edited, Delete (or
    // Backspace) takes it away while more than two are left.
    if response.has_focus() {
        let (left, right, delete) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
            )
        });
        if delete && map.remove(*stop) {
            *stop = stop.saturating_sub(1);
        }
        let shift = if right {
            0.01
        } else if left {
            -0.01
        } else {
            0.0
        };
        if shift != 0.0
            && let Some(moving) = map.stop_mut(*stop)
        {
            moving.at = (moving.at + shift).clamp(0.0, 1.0);
            *stop = map.sort_keeping(*stop);
        }
    }
    // The stops: small swatches under the bar, the edited one ringed.
    for (index, found) in map.stops().iter().enumerate() {
        let center = place(found.at);
        let swatch = Rect::from_center_size(center, egui::Vec2::splat(theme.space(Space::S5)));
        sue_wm::swatch(
            ui,
            swatch,
            [found.rgb[0], found.rgb[1], found.rgb[2], found.alpha],
            theme,
        );
        let edge = if index == *stop {
            theme.selected_stroke()
        } else {
            theme.hairline()
        };
        ui.painter().rect_stroke(
            swatch,
            theme.radius(Radius::Xs),
            edge,
            egui::StrokeKind::Outside,
        );
        ui.painter().line_segment(
            [pos2(center.x, bar.bottom()), pos2(center.x, swatch.top())],
            theme.hairline(),
        );
    }
    let name = tr!("gradient-stops");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, &name));
    ui::focus_ring(ui, &response, theme.radius(Radius::Xs), theme);
    ui::tooltip(
        response,
        &Tip::new(&name).help(&tr!("gradient-stops-help")),
        theme,
    );
}

/// The stop being edited: its place, where its color comes from, its
/// color and opacity, and Delete.
fn stop_ui(
    ui: &mut Ui,
    map: &mut GradientMap,
    stop: &mut usize,
    colors: [[u8; 3]; 2],
    theme: &Theme,
) {
    let count = map.stops().len();
    let Some(edited) = map.stop_mut(*stop) else {
        return;
    };
    let mut place = edited.at * 100.0;
    let moved = number(
        ui,
        "stop-place",
        &tr!("stop-position"),
        &mut place,
        0.0..=100.0,
        0,
        theme,
    );
    if moved {
        edited.at = place / 100.0;
    }
    let sources = [
        tr!("stop-own-color"),
        tr!("stop-main-color"),
        tr!("stop-sub-color"),
    ];
    let mut source = StopColor::ALL
        .iter()
        .position(|kind| *kind == edited.color)
        .unwrap_or(0) as u8;
    if choice(ui, "stop-source", &sources, &mut source, theme) {
        edited.color = StopColor::ALL[usize::from(source.min(2))];
        match edited.color {
            StopColor::Own => {}
            StopColor::Main => edited.rgb = colors[0],
            StopColor::Sub => edited.rgb = colors[1],
        }
    }
    ui.add_enabled_ui(edited.color == StopColor::Own, |ui| {
        color(
            ui,
            "stop-color",
            &tr!("filter-color"),
            &mut edited.rgb,
            theme,
        );
    });
    let mut opacity = f32::from(edited.alpha) / 255.0 * 100.0;
    if number(
        ui,
        "stop-opacity",
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        0,
        theme,
    ) {
        edited.alpha = (opacity / 100.0 * 255.0).round() as u8;
    }
    if moved {
        *stop = map.sort_keeping(*stop);
    }
    let delete = tr!("stop-delete");
    let response = ui::button("sue-paint-stop-delete", &delete)
        .ghost()
        .icon(Icon::Delete)
        .enabled(count > 2)
        .show(ui, theme);
    if response.clicked() && map.remove(*stop) {
        *stop = stop.saturating_sub(1);
    }
    ui::tooltip(response, &Tip::new(&delete).shortcut("Delete"), theme);
}

// ----- The dialogs ------------------------------------------------------------------

/// A filter dialog's body: its settings, the preview switch, OK and Cancel.
fn filter_ui(
    ui: &mut Ui,
    dialog: &mut FilterDialog,
    library: &mut GradientLibrary,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    if let Filter::Quick { .. } = dialog.filter {
        quick_gallery(ui, dialog, theme);
    }
    let mut filter = dialog.filter;
    filter_params(
        ui,
        &mut filter,
        &mut Params {
            colors: dialog.colors,
            library: Some(library),
            layer: Some((&dialog.before, dialog.size)),
            pad: &mut dialog.pad,
        },
        theme,
    );
    dialog.filter = filter;
    check(ui, "preview", &tr!("preview"), &mut dialog.preview, theme);
    note(
        ui,
        &if dialog.selection.is_some() {
            tr!("changes-selected-area-layer")
        } else {
            tr!("changes-the-whole-layer")
        },
        theme,
    );
    ok_cancel(ui, &mut outcome, theme);
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
    let picture = Density::of(ui).control_height() * 2.4;
    if dialog.thumbs.0.len() != count {
        // The layer, small, once; each look on it.
        let [width, height] = dialog.size;
        let scale = (picture / width.max(height).max(1) as f32).min(1.0);
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
    let gap = theme.space(Space::S3);
    let caption = theme.text_size(TextRole::Caption);
    let room = theme.space(Space::S5);
    let tile = vec2(picture + room, picture + caption + caption + gap);
    let columns = ((ui.available_width() / (tile.x + gap)) as usize).max(1);
    egui::Grid::new(("sue-paint-quick", effect))
        .spacing(egui::Vec2::splat(gap))
        .show(ui, |ui| {
            for (index, texture) in dialog.thumbs.0.iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
                let chosen = usize::from(look) == index;
                let corner = theme.radius(Radius::Sm);
                if chosen {
                    ui.painter()
                        .rect_filled(rect, corner, theme.fill_selected());
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, corner, theme.fill_hover());
                }
                let shown = Rect::from_center_size(
                    pos2(rect.center().x, rect.top() + gap + picture / 2.0),
                    texture.size_vec2(),
                );
                pad_picture(ui.painter(), texture, shown);
                let name = crate::quick::name(effect, index as u8);
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - caption),
                    egui::Align2::CENTER_CENTER,
                    &name,
                    theme.text_font(TextRole::Caption),
                    theme.text,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, chosen, &name)
                });
                ui::focus_ring(ui, &response, corner, theme);
                if response.clicked() {
                    dialog.filter = Filter::Quick {
                        effect,
                        look: index as u8,
                        strength,
                    };
                }
                ui::tooltip(response, &Tip::new(&name), theme);
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gradient_map::Stop;

    #[test]
    fn gradient_sets_are_files_people_add_to_and_take_from() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let data = std::env::temp_dir().join(format!("sue-gradient-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut library = GradientLibrary::default();
        library.load(Some(data.clone()));
        assert_eq!(library.sets.len(), 1, "Sue's own set only");
        // Adding on Sue's own set makes My Gradients and writes it.
        let map = GradientMap::of(&[
            Stop::new(0.0, [1, 2, 3]),
            Stop::new(0.3, [200, 10, 10]),
            Stop::new(1.0, [250, 250, 250]),
        ])
        .unwrap();
        library.add("Mine".to_owned(), map);
        assert_eq!(library.note, None);
        assert_eq!(library.sets.len(), 2);
        assert_eq!((library.set, library.picked), (1, Some(0)));
        let file = data
            .join(SETS_FOLDER)
            .join(format!("my_gradients.{SET_EXTENSION}"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("gradient = Mine = "), "{text}");
        // Read again by a fresh library: the same set.
        let mut again = GradientLibrary::default();
        again.load(Some(data.clone()));
        assert_eq!(again.sets.len(), 2);
        assert_eq!(again.sets[1].1.gradients[0].map, map);
        // OK on an edited gradient keeps it; Delete takes it out.
        let mut edited = map;
        edited.stop_mut(1).unwrap().rgb = [0, 255, 0];
        library.keep(&edited);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("00ff00ff"), "{text}");
        library.delete();
        assert!(library.sets[1].1.gradients.is_empty());
        assert_eq!(library.picked, None);
        // Sue's own set is never changed.
        library.set = 0;
        library.picked = Some(0);
        let shipped = library.sets[0].1.clone();
        library.delete();
        library.keep(&map);
        assert_eq!(library.sets[0].1, shipped);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn every_filter_dialog_is_tall_enough_for_its_settings() {
        // The gradient map needs its two columns; the rest one.
        let gradient = Filter::GradientMap {
            from: [0; 3],
            to: [255; 3],
        };
        assert!(dialog_size(&gradient).x > dialog_size(&Filter::Invert).x);
        assert!(dialog_size(&gradient).y > dialog_size(&Filter::Invert).y);
        assert!(correction_size(&gradient).y < dialog_size(&gradient).y);
    }
}
