//! Sue Paint's dialogs: New (a canvas from presets, paper sizes and
//! units), the filter dialogs with a live preview, and the shortcut list.

use egui::{Align, Id, Layout, Order, RichText, Sense, Ui, vec2};
use sue_core::{Color, EntityId, Preset, PresetLibrary, Properties, RasterTile2D};
use sue_wm::{AppletContext, ArenaEdit, Theme, widgets};

use crate::doc::{self, NewPicture};
use crate::filters::Filter;
use crate::icons::{self, Icon};
use crate::select::Selection;
use crate::studio::SuePaint;
use sue_model::Response;
use sue_rice::{RiceValue, SetRiceValue};

/// A length unit of the New dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Px,
    Cm,
    Mm,
    In,
}

impl Unit {
    const ALL: [Self; 4] = [Self::Px, Self::Cm, Self::Mm, Self::In];

    fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::In => "in",
        }
    }

    /// Units per inch (pixels per inch for `Px` come from the resolution).
    fn per_inch(self, dpi: f64) -> f64 {
        match self {
            Self::Px => dpi,
            Self::Cm => 2.54,
            Self::Mm => 25.4,
            Self::In => 1.0,
        }
    }
}

/// Paper sizes of the size list: name, pixels and resolution.
const SIZES: [(&str, u32, u32, f64); 11] = [
    ("Custom", 0, 0, 0.0),
    ("A4", 2894, 4093, 350.0),
    ("A5", 2039, 2894, 350.0),
    ("B5", 2508, 3541, 350.0),
    ("US Letter", 2550, 3300, 300.0),
    ("Postcard", 1378, 2039, 350.0),
    ("HD 1280 x 720", 1280, 720, 72.0),
    ("Full HD 1920 x 1080", 1920, 1080, 72.0),
    ("4K 3840 x 2160", 3840, 2160, 72.0),
    ("Square 2048", 2048, 2048, 72.0),
    ("Icon 512", 512, 512, 72.0),
];

/// The operator new-canvas presets belong to.
pub(crate) const NEW_PRESET_TARGET: &str = "paint.new";

/// The built-in new-canvas presets, as backbone presets any feature can
/// read.
pub(crate) fn new_presets() -> PresetLibrary {
    let mut library = PresetLibrary::new();
    for (name, width, height, dpi, paper) in [
        ("Illustration", 1600_i64, 1200_i64, 72.0, true),
        ("Print (A4, 350 dpi)", 2894, 4093, 350.0, true),
        ("Screen (Full HD)", 1920, 1080, 72.0, true),
        ("Square Post", 2048, 2048, 72.0, true),
        ("Transparent Sprite", 512, 512, 72.0, false),
    ] {
        library.add(Preset::new(
            name,
            NEW_PRESET_TARGET,
            Properties::new()
                .with("width", width)
                .with("height", height)
                .with("dpi", dpi)
                .with("paper", paper),
        ));
    }
    library
}

/// What a new work is (the New dialog's Project row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Project {
    /// One picture.
    #[default]
    Illustration,
    /// A story of pages.
    Comic,
    /// A picture with a timeline.
    Animation,
}

/// The New dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewDialog {
    pub project: Project,
    /// Pages of a comic.
    pub pages: u32,
    /// A comic read right to left (manga).
    pub right_bound: bool,
    /// Frames of an animation.
    pub frames: u32,
    /// Frames a second of an animation.
    pub fps: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub unit: Unit,
    pub paper: bool,
    pub paper_color: [u8; 3],
    pub size: usize,
    pub preset: Option<String>,
}

impl NewDialog {
    pub(crate) fn new(spec: &NewPicture) -> Self {
        Self {
            project: Project::Illustration,
            pages: 8,
            right_bound: true,
            frames: 24,
            fps: 24,
            name: spec.name.clone(),
            width: spec.width,
            height: spec.height,
            dpi: spec.dpi,
            unit: Unit::Px,
            paper: spec.paper.is_some(),
            paper_color: spec
                .paper
                .map_or([255, 255, 255], |color| crate::color::bytes(color.rgb)),
            size: 0,
            preset: None,
        }
    }

    pub(crate) fn spec(&self) -> NewPicture {
        NewPicture {
            name: if self.name.trim().is_empty() {
                "Illustration".to_owned()
            } else {
                self.name.trim().to_owned()
            },
            width: self.width,
            height: self.height,
            dpi: self.dpi,
            paper: self.paper.then(|| {
                let [r, g, b] = self.paper_color;
                Color::srgb8(r, g, b)
            }),
        }
    }

    pub(crate) fn apply_preset(&mut self, preset: &Preset) {
        let int = |key| {
            preset
                .values
                .get(key)
                .and_then(sue_core::Value::as_i64)
                .and_then(|value| u32::try_from(value).ok())
        };
        if let Some(width) = int("width") {
            self.width = width;
        }
        if let Some(height) = int("height") {
            self.height = height;
        }
        if let Some(dpi) = preset.values.f64("dpi") {
            self.dpi = dpi;
        }
        if let Some(paper) = preset
            .values
            .get("paper")
            .and_then(sue_core::Value::as_bool)
        {
            self.paper = paper;
        }
        self.preset = Some(preset.name.clone());
        self.size = 0;
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
}

/// The setting that holds the pen pressure curve every tool uses.
pub(crate) const PEN_PRESSURE: &str = "sue_paint.pen_pressure";

/// Pen Pressure Settings (Clip Studio Paint's Adjust pen pressure): draw
/// on the pad as you normally would and the curve is fitted to how hard
/// you press, then made stronger or lighter by hand or on the graph.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PressureDialog {
    /// The curve being adjusted.
    pub curve: Response,
    /// The pen's own pressures drawn on the pad, as they came.
    pub raw: Vec<f32>,
    /// The pad's lines: points (from the pad's corner) and raw pressure.
    pub lines: Vec<Vec<(egui::Vec2, f32)>>,
    /// The fitted curve is being tried out.
    pub checked: bool,
    /// The graph shows.
    pub show_graph: bool,
    /// The pen's latest pressure, while it touches.
    pub force: Option<f32>,
}

impl PressureDialog {
    /// Adjusting from `curve`, the one in use.
    pub(crate) fn new(curve: Response) -> Self {
        Self {
            curve,
            raw: Vec::new(),
            lines: Vec::new(),
            checked: false,
            show_graph: true,
            force: None,
        }
    }

    /// The fewest pressures [`Self::fit`] works from.
    pub(crate) const ENOUGH: usize = 24;

    /// A curve for someone who drew `raw` (pressures as they normally
    /// press): the firm end of their pressing reaches a full line and
    /// their usual pressing half of one. `None` until there are enough.
    pub(crate) fn fit(raw: &[f32]) -> Option<Response> {
        let mut sorted: Vec<f32> = raw
            .iter()
            .copied()
            .filter(|pressure| pressure.is_finite() && *pressure > 0.0)
            .collect();
        if sorted.len() < Self::ENOUGH {
            return None;
        }
        sorted.sort_by(f32::total_cmp);
        let at = |share: f32| sorted[((sorted.len() - 1) as f32 * share).round() as usize];
        let firm = at(0.95).clamp(0.15, 1.0);
        let usual = at(0.5).clamp(0.02, firm * 0.9);
        let gamma = (0.5_f32.ln() / (usual / firm).ln()).clamp(0.35, 2.8);
        let out = |x: f32| (x / firm).min(1.0).powf(gamma);
        let mut points: Vec<[f32; 2]> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|share| [share * firm, out(share * firm)])
            .collect();
        points.push([1.0, 1.0]);
        Some(Response::new(&points))
    }
}

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

/// The Page Manager: the story's pages as spreads, one chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PagesDialog {
    pub chosen: usize,
}

/// An open dialog.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant, reason = "one dialog is open at a time")]
pub(crate) enum Dialog {
    New(NewDialog),
    Filter(FilterDialog),
    Shortcuts,
    Pressure(PressureDialog),
    Pages(PagesDialog),
    Frame(crate::comic::FrameSpec),
    Canvas(CanvasDialog),
    Resolution(ResolutionDialog),
    /// The Text tool's words: the text layer they go to (new if `None`).
    Text(Option<EntityId>, crate::lettering::Lettering),
    /// A correction layer's settings, shown live: the layer, the
    /// correction, its setting when the dialog opened, and whether the
    /// layer was just made (Cancel takes it away again).
    Correction(EntityId, Filter, String, bool),
    /// Create Perspective Ruler: how many vanishing points, 1 to 3.
    Perspective(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Open,
    Cancel,
    Ok,
}

/// What the pen and mouse do, for the Keyboard Shortcuts dialog under
/// the keys Settings has: Clip Studio Paint's gestures.
const GESTURES: [(&str, &str); 11] = [
    ("Scroll", "Space+drag, middle drag"),
    ("Rotate the view", "Shift+Space+drag, Shift+wheel"),
    (
        "Zoom in / out by clicking",
        "Ctrl+Space+click / Alt+Space+click",
    ),
    ("Zoom", "Ctrl+wheel"),
    ("Pick a color", "Alt+click, right-click"),
    ("Pick the layer under the pointer", "Ctrl+Shift+click"),
    ("Change the brush size", "Ctrl+Alt+drag"),
    ("Straight line", "Shift+drag"),
    (
        "Selection: add / subtract / intersect",
        "Shift / Alt / Shift+Alt",
    ),
    ("Zoom tool: the other way", "Alt+click"),
    ("Command palette", "Command in the status bar"),
];

impl SuePaint {
    /// Draws the open dialog, if any, over the studio.
    pub(crate) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("sue-paint-scrim"))
            .fade_in(false)
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.45));
            });
        let (title, size) = match &dialog {
            Dialog::New(_) => ("New", vec2(560.0, 560.0)),
            Dialog::Filter(filter) => (
                filter.filter.label(),
                vec2(
                    400.0,
                    match filter.filter {
                        Filter::ToneCurve { .. } => 360.0,
                        Filter::Levels { .. } => 330.0,
                        _ => 250.0,
                    },
                ),
            ),
            Dialog::Shortcuts => ("Keyboard Shortcuts", vec2(470.0, 560.0)),
            Dialog::Pages(_) => ("Page Manager", vec2(820.0, 620.0)),
            Dialog::Frame(_) => ("New Frame Border Folder", vec2(420.0, 330.0)),
            Dialog::Canvas(_) => ("Change Canvas Size", vec2(380.0, 300.0)),
            Dialog::Resolution(_) => ("Change Image Resolution", vec2(380.0, 250.0)),
            Dialog::Text(..) => ("Text", vec2(440.0, 380.0)),
            Dialog::Perspective(_) => ("Create Perspective Ruler", vec2(320.0, 190.0)),
            Dialog::Correction(_, filter, ..) => (
                filter.label(),
                vec2(
                    400.0,
                    match filter {
                        Filter::ToneCurve { .. } => 340.0,
                        Filter::Levels { .. } => 310.0,
                        _ => 230.0,
                    },
                ),
            ),
            Dialog::Pressure(pressure) => (
                "Adjust pen pressure",
                vec2(560.0, if pressure.show_graph { 520.0 } else { 330.0 }),
            ),
        };
        let size = size.min(screen.size() - vec2(16.0, 16.0));
        let at = screen.center() - size / 2.0;
        let mut outcome = Outcome::Open;
        let mut change_keys = false;
        let keys = matches!(dialog, Dialog::Shortcuts).then(|| {
            sue_wm::shortcuts::rows(
                cx.rice(),
                crate::studio::KEYS_SECTION,
                crate::studio::shortcut_table(),
            )
        });
        egui::Area::new(Id::unique("sue-paint-dialog"))
            .fade_in(false)
            .order(Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        let inner = size - vec2(32.0, 32.0);
                        ui.set_min_size(inner);
                        ui.set_max_size(inner);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(title)
                                    .size(theme.ui_size + 1.0)
                                    .color(theme.text),
                            );
                        });
                        ui.separator();
                        match &mut dialog {
                            Dialog::New(new) => outcome = new_ui(ui, new, theme),
                            Dialog::Filter(filter) => outcome = filter_ui(ui, filter, theme),
                            Dialog::Pressure(pressure) => {
                                outcome = pressure_ui(ui, pressure, theme)
                            }
                            Dialog::Pages(pages) => outcome = self.pages_ui(ui, pages, cx, theme),
                            Dialog::Frame(frame) => outcome = frame_ui(ui, frame, theme),
                            Dialog::Text(_, lettering) => outcome = text_ui(ui, lettering, theme),
                            Dialog::Perspective(count) => {
                                ui.horizontal(|ui| {
                                    label(ui, "Type:", theme);
                                    ui.vertical(|ui| {
                                        for (points, name) in [
                                            (1, "1-point perspective"),
                                            (2, "2-point perspective"),
                                            (3, "3-point perspective"),
                                        ] {
                                            ui.radio_value(count, points, name);
                                        }
                                    });
                                });
                                ui.label(
                                    RichText::new("Strokes run toward the vanishing points while Snap to Special Ruler is on")
                                        .size(theme.small_size)
                                        .color(theme.muted),
                                );
                                ok_cancel(ui, &mut outcome);
                            }
                            Dialog::Correction(_, filter, ..) => {
                                filter_params(ui, filter, theme);
                                ui.label(
                                    RichText::new("Changes every layer under this one")
                                        .size(theme.small_size)
                                        .color(theme.muted),
                                );
                                ok_cancel(ui, &mut outcome);
                            }
                            Dialog::Canvas(canvas) => outcome = canvas_ui(ui, canvas, theme),
                            Dialog::Resolution(resolution) => {
                                outcome = resolution_ui(ui, resolution, theme)
                            }
                            Dialog::Shortcuts => {
                                egui::ScrollArea::vertical().max_height(size.y - 90.0).show(
                                    ui,
                                    |ui| {
                                        let keys = keys.as_deref().unwrap_or_default();
                                        sue_wm::shortcuts::grid_ui(
                                            ui,
                                            "sue-paint-keys",
                                            keys,
                                            theme,
                                        );
                                        ui.add_space(12.0);
                                        ui.label(
                                            RichText::new("With the pen and mouse")
                                                .color(theme.text)
                                                .strong(),
                                        );
                                        let gestures: Vec<(String, String)> = GESTURES
                                            .iter()
                                            .map(|(what, how)| {
                                                ((*what).to_owned(), (*how).to_owned())
                                            })
                                            .collect();
                                        sue_wm::shortcuts::grid_ui(
                                            ui,
                                            "sue-paint-gestures",
                                            &gestures,
                                            theme,
                                        );
                                    },
                                );
                                ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                                    if ui.button("Close").clicked() {
                                        outcome = Outcome::Cancel;
                                    }
                                    if ui
                                        .button("Change Shortcuts...")
                                        .on_hover_text(
                                            "Settings, Keymaps: any number of keys for each",
                                        )
                                        .clicked()
                                    {
                                        change_keys = true;
                                        outcome = Outcome::Cancel;
                                    }
                                });
                            }
                        }
                    });
            });
        let (escape, enter) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Enter),
            )
        });
        if change_keys {
            cx.request(sue_wm::ShellRequest::OpenSettings(Some(
                "keymaps".to_owned(),
            )));
        }
        if escape {
            outcome = Outcome::Cancel;
        } else if enter
            && !ctx.text_edit_focused()
            && !matches!(
                dialog,
                Dialog::Shortcuts | Dialog::Pressure(_) | Dialog::Pages(_) | Dialog::Text(..)
            )
        {
            outcome = Outcome::Ok;
        }
        match (dialog, outcome) {
            (dialog, Outcome::Open) => {
                if let Dialog::Correction(layer, filter, ..) = &dialog
                    && let Some(text) = filter.to_text()
                    && let Ok(entity) = cx.world.arena.get_by_id(*layer)
                    && entity
                        .property(doc::CORRECTION_FILTER)
                        .and_then(sue_core::Value::as_str)
                        != Some(text.as_str())
                    && let Ok(entity) = cx.world.arena.get_by_id_mut(*layer)
                {
                    // The setting shows live while the dialog is open.
                    entity.properties_mut().set(doc::CORRECTION_FILTER, text);
                }
                if let Dialog::Filter(filter) = &dialog {
                    let pointer_down = ctx.input(|input| input.pointer.any_down());
                    let wanted = if filter.preview {
                        Some(filter.filter)
                    } else {
                        None
                    };
                    let cheap = !matches!(
                        filter.filter,
                        Filter::GaussianBlur { .. }
                            | Filter::Sharpen { .. }
                            | Filter::Mosaic { .. }
                    );
                    if wanted != filter.shown && (cheap || !pointer_down) {
                        let pixels = match wanted {
                            Some(filter_now) => filter.result(filter_now),
                            None => filter.before.clone(),
                        };
                        filter.show(cx, &pixels);
                        let mut updated = filter.clone();
                        updated.shown = wanted;
                        self.dialog = Some(Dialog::Filter(updated));
                        ctx.request_repaint();
                        return;
                    }
                }
                self.dialog = Some(dialog);
            }
            (Dialog::Filter(filter), Outcome::Cancel) => {
                if filter.shown.is_some() {
                    filter.show(cx, &filter.before);
                }
            }
            (Dialog::Filter(filter), Outcome::Ok) => {
                let after = filter.result(filter.filter);
                filter.show(cx, &after);
                if let Some(patch) =
                    doc::diff_patch(&filter.before, &after, filter.size[0], filter.size[1])
                {
                    cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                        filter.layer,
                        filter.filter.label(),
                        vec![patch],
                    )));
                }
            }
            (Dialog::Frame(frame), Outcome::Ok) => self.add_frame_layer(cx, frame),
            (Dialog::Text(layer, lettering), Outcome::Ok) => self.put_text(cx, layer, &lettering),
            (Dialog::Perspective(count), Outcome::Ok) => self.make_perspective(cx, count),
            (Dialog::Correction(layer, filter, before, made), outcome) => {
                let text = filter.to_text().unwrap_or_default();
                // The setting shows live while the dialog is open.
                let set_now = |cx: &mut AppletContext<'_>, text: &str| {
                    if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
                        entity
                            .properties_mut()
                            .set(doc::CORRECTION_FILTER, text.to_owned());
                    }
                };
                match outcome {
                    Outcome::Open => {}
                    Outcome::Ok => {
                        // Back as it was, then the change as one undo step.
                        set_now(cx, &before);
                        if text != before {
                            Self::apply(cx, doc::set(layer, doc::CORRECTION_FILTER, text));
                        }
                    }
                    Outcome::Cancel => {
                        set_now(cx, &before);
                        if made {
                            let _ = cx.undo(false);
                        }
                    }
                }
            }
            (Dialog::Canvas(canvas), Outcome::Ok) => {
                if let Some(picture) = self.picture() {
                    match doc::resize_canvas(&cx.world.arena, picture, canvas.size, canvas.shift())
                    {
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
            (Dialog::Resolution(resolution), Outcome::Ok) => {
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
            (Dialog::New(new), Outcome::Ok) => {
                let spec = new.spec();
                match new.project {
                    Project::Illustration => self.make_picture(cx, &spec),
                    Project::Comic => {
                        self.make_story(cx, &spec, new.pages as usize, new.right_bound);
                    }
                    Project::Animation => self.make_animation(cx, &spec, new.frames, new.fps),
                }
            }
            (Dialog::Pressure(pressure), Outcome::Ok) => {
                let text = pressure.curve.to_string();
                match cx.edit_rice(SetRiceValue::new(PEN_PRESSURE, RiceValue::Text(text))) {
                    Ok(_) => cx.notify("Pen pressure adjusted for every tool"),
                    Err(error) => cx.fail(format!("Cannot keep the pen pressure: {error}")),
                }
            }
            (
                Dialog::New(_)
                | Dialog::Shortcuts
                | Dialog::Pressure(_)
                | Dialog::Pages(_)
                | Dialog::Frame(_)
                | Dialog::Canvas(_)
                | Dialog::Resolution(_)
                | Dialog::Text(..),
                Outcome::Cancel | Outcome::Ok,
            )
            | (Dialog::Perspective(_), Outcome::Cancel) => {}
        }
    }
}

/// Change Canvas Size: width, height and the anchor.
fn canvas_ui(ui: &mut Ui, canvas: &mut CanvasDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-canvas-size")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, "Width:", theme);
            ui.add(
                egui::DragValue::new(&mut canvas.size[0])
                    .range(1..=doc::MAX_SIDE)
                    .suffix(" px"),
            );
            ui.end_row();
            label(ui, "Height:", theme);
            ui.add(
                egui::DragValue::new(&mut canvas.size[1])
                    .range(1..=doc::MAX_SIDE)
                    .suffix(" px"),
            );
            ui.end_row();
            label(ui, "Reference point:", theme);
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
        RichText::new(format!("Now {} by {} px", canvas.old[0], canvas.old[1]))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Change Image Resolution: the size in pixels (keeping its shape) and the
/// resolution.
fn resolution_ui(ui: &mut Ui, resolution: &mut ResolutionDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let ratio = resolution.old[1] as f64 / resolution.old[0].max(1) as f64;
    egui::Grid::new("sue-paint-resolution")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, "Width:", theme);
            if ui
                .add(
                    egui::DragValue::new(&mut resolution.size[0])
                        .range(1..=doc::MAX_SIDE)
                        .suffix(" px"),
                )
                .changed()
                && resolution.keep
            {
                resolution.size[1] =
                    ((f64::from(resolution.size[0]) * ratio).round() as u32).max(1);
            }
            ui.end_row();
            label(ui, "Height:", theme);
            if ui
                .add(
                    egui::DragValue::new(&mut resolution.size[1])
                        .range(1..=doc::MAX_SIDE)
                        .suffix(" px"),
                )
                .changed()
                && resolution.keep
            {
                resolution.size[0] =
                    ((f64::from(resolution.size[1]) / ratio.max(1e-6)).round() as u32).max(1);
            }
            ui.end_row();
            label(ui, "Resolution:", theme);
            ui.add(
                egui::DragValue::new(&mut resolution.dpi)
                    .range(1.0..=2400.0)
                    .suffix(" dpi"),
            );
            ui.end_row();
        });
    ui.checkbox(&mut resolution.keep, "Keep the shape");
    ok_cancel(ui, &mut outcome);
    outcome
}

/// The Text tool's words, font, size, ink and placement.
fn text_ui(ui: &mut Ui, lettering: &mut crate::lettering::Lettering, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let edit = ui.add(
        egui::TextEdit::multiline(&mut lettering.content)
            .desired_rows(5)
            .desired_width(f32::INFINITY)
            .hint_text("Type here"),
    );
    // The words take the keys while nothing else does.
    if ui.ctx().memory(|memory| memory.focused().is_none()) {
        edit.request_focus();
    }
    egui::Grid::new("sue-paint-text")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, "Font:", theme);
            let shown = if lettering.font.is_empty() {
                "Sue (Ubuntu Light)".to_owned()
            } else {
                lettering.font.clone()
            };
            egui::ComboBox::from_id_salt("sue-paint-text-font")
                .selected_text(shown)
                .width(240.0)
                .height(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut lettering.font, String::new(), "Sue (Ubuntu Light)");
                    for family in sue_wm::fonts::families() {
                        ui.selectable_value(&mut lettering.font, family.clone(), family);
                    }
                });
            ui.end_row();
            label(ui, "Size:", theme);
            ui.add(
                egui::Slider::new(&mut lettering.size, 6.0..=400.0)
                    .logarithmic(true)
                    .suffix(" px"),
            );
            ui.end_row();
            label(ui, "Color:", theme);
            ui.color_edit_button_srgb(&mut lettering.rgb);
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.checkbox(&mut lettering.center, "Centered");
        ui.checkbox(&mut lettering.vertical, "Vertical (columns, right to left)");
    });
    ui.label(
        RichText::new("Enter starts a new line; Ctrl+Enter is OK")
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    let commit = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command
            )
        })
    });
    if commit {
        outcome = Outcome::Ok;
    }
    outcome
}

/// OK and Cancel at the bottom right.
fn ok_cancel(ui: &mut Ui, outcome: &mut Outcome) {
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("OK").clicked() {
            *outcome = Outcome::Ok;
        }
        if ui.button("Cancel").clicked() {
            *outcome = Outcome::Cancel;
        }
    });
}

/// New Frame Border Folder: a layout, the border and the gaps.
fn frame_ui(ui: &mut Ui, frame: &mut crate::comic::FrameSpec, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-frame")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, "Layout:", theme);
            egui::ComboBox::from_id_salt("sue-paint-frame-layout")
                .selected_text(crate::manga::LAYOUTS[frame.layout].0)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (index, (name, _)) in crate::manga::LAYOUTS.iter().enumerate() {
                        ui.selectable_value(&mut frame.layout, index, *name);
                    }
                });
            ui.end_row();
            label(ui, "Border width:", theme);
            ui.add(egui::Slider::new(&mut frame.border, 0.0..=40.0).suffix(" px"));
            ui.end_row();
            label(ui, "Margin:", theme);
            ui.add(egui::Slider::new(&mut frame.margin, 0.0..=400.0).suffix(" px"));
            ui.end_row();
            label(ui, "Gap across:", theme);
            ui.add(egui::Slider::new(&mut frame.gap, 0.0..=200.0).suffix(" px"));
            ui.end_row();
            label(ui, "Gap down:", theme);
            ui.add(egui::Slider::new(&mut frame.gap_y, 0.0..=200.0).suffix(" px"));
            ui.end_row();
        });
    ui.label(
        RichText::new("Layers under the frame show only inside the panels. The Frame Border tool draws more panels and divides them.")
            .size(theme.small_size)
            .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("OK").clicked() {
            outcome = Outcome::Ok;
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// Pen Pressure Settings: the words, the buttons, the pad and the graph.
fn pressure_ui(ui: &mut Ui, pressure: &mut PressureDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let words = if pressure.checked {
        "Draw again to try the adjusted pressure.\nStronger reaches a full line with a lighter touch, Lighter needs a firmer one.\nDone keeps it for every tool."
    } else {
        "Draw freely on the pad with the pen.\nWhile drawing, vary your pressure as you normally would.\nWhen you have finished, choose Check adjusted settings."
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(330.0);
            ui.label(RichText::new(words).color(theme.muted));
        });
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            let wide = vec2(ui.available_width(), 22.0);
            if pressure.checked {
                ui.horizontal(|ui| {
                    let half = vec2(wide.x / 2.0 - 4.0, wide.y);
                    if ui.add_sized(half, egui::Button::new("Stronger")).clicked() {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(0.8));
                    }
                    if ui.add_sized(half, egui::Button::new("Lighter")).clicked() {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(1.25));
                    }
                });
                if ui.add_sized(wide, egui::Button::new("Done")).clicked() {
                    outcome = Outcome::Ok;
                }
            } else {
                let enough = pressure.raw.len() >= PressureDialog::ENOUGH;
                let check = ui
                    .add_enabled_ui(enough, |ui| {
                        ui.add_sized(wide, egui::Button::new("Check adjusted settings"))
                    })
                    .inner
                    .on_disabled_hover_text("Draw a few lines with the pen first");
                if check.clicked()
                    && let Some(curve) = PressureDialog::fit(&pressure.raw)
                {
                    pressure.curve = curve;
                    pressure.checked = true;
                    pressure.lines.clear();
                }
            }
            if ui.add_sized(wide, egui::Button::new("Cancel")).clicked() {
                outcome = Outcome::Cancel;
            }
            if ui
                .add_sized(wide, egui::Button::new("Reset to default"))
                .clicked()
            {
                pressure.curve = Response::LINEAR;
                pressure.raw.clear();
                pressure.lines.clear();
                pressure.checked = false;
            }
        });
    });
    ui.add_space(6.0);

    // The pad: lines as wide as the adjusted pressure makes them.
    let (pad, response) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::drag());
    let touch = ui.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            egui::Event::Touch { phase, force, .. } => Some(match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => *force,
                egui::TouchPhase::End | egui::TouchPhase::Cancel => None,
            }),
            _ => None,
        })
    });
    if let Some(force) = touch {
        pressure.force = force.map(|force| force.clamp(0.0, 1.0));
    }
    if response.drag_started() {
        pressure.lines.push(Vec::new());
        if pressure.lines.len() > 40 {
            pressure.lines.remove(0);
        }
    }
    if response.dragged()
        && let (Some(pos), Some(line)) =
            (response.interact_pointer_pos(), pressure.lines.last_mut())
    {
        let raw = pressure.force.unwrap_or(1.0);
        line.push((pos - pad.min, raw));
        if let Some(force) = pressure.force
            && pressure.raw.len() < 4000
        {
            pressure.raw.push(force);
        }
    }
    let painter = ui.painter_at(pad);
    painter.rect_filled(pad, 4.0, egui::Color32::from_gray(245));
    for line in &pressure.lines {
        for pair in line.windows(2) {
            let width = 0.6 + 9.0 * pressure.curve.eval(pair[1].1);
            painter.line_segment(
                [pad.min + pair[0].0, pad.min + pair[1].0],
                egui::Stroke::new(width, egui::Color32::from_gray(20)),
            );
        }
    }
    if pressure.lines.iter().any(|line| !line.is_empty()) && pressure.raw.is_empty() {
        painter.text(
            pad.center_bottom() - vec2(0.0, 8.0),
            egui::Align2::CENTER_BOTTOM,
            "This pointer reports no pen pressure",
            egui::FontId::proportional(theme.small_size),
            egui::Color32::from_gray(110),
        );
    }
    ui.add_space(6.0);
    ui.checkbox(
        &mut pressure.show_graph,
        RichText::new("Show pen pressure graph").color(theme.text),
    );
    if pressure.show_graph {
        widgets::response_curve(
            ui,
            "pen-pressure",
            &mut pressure.curve,
            150.0,
            widgets::CurveLabels::PRESSURE,
            theme,
        );
    }
    outcome
}

/// A length in pixels, edited in `unit`. Returns `true` if it changed.
fn length_field(ui: &mut Ui, pixels: &mut u32, unit: Unit, dpi: f64) -> bool {
    if unit == Unit::Px {
        return ui
            .add(
                egui::DragValue::new(pixels)
                    .range(1..=doc::MAX_SIDE)
                    .suffix(" px"),
            )
            .changed();
    }
    let per_inch = unit.per_inch(dpi);
    let mut value = f64::from(*pixels) / dpi.max(1.0) * per_inch;
    let response = ui.add(
        egui::DragValue::new(&mut value)
            .speed(0.1)
            .max_decimals(2)
            .suffix(format!(" {}", unit.label())),
    );
    if response.changed() {
        *pixels = ((value / per_inch * dpi).round() as u32).clamp(1, doc::MAX_SIDE);
    }
    response.changed()
}

fn label(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(RichText::new(text).color(theme.muted));
}

/// The New dialog's body.
fn new_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let presets = new_presets();
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
    ui.horizontal(|ui| {
        label(ui, "Project:", theme);
        let kinds = [
            (Icon::Paper, "Illustration", Some(Project::Illustration)),
            (Icon::Frame, "Webtoon", None),
            (Icon::Duplicate, "Comic", Some(Project::Comic)),
            (Icon::Open, "Fanzine print", None),
            (Icon::NewFolder, "Show all comic settings", None),
            (Icon::Play, "Animation", Some(Project::Animation)),
        ];
        for (icon, tip, project) in kinds {
            let enabled = project.is_some();
            let (rect, response) = ui.allocate_exact_size(vec2(38.0, 34.0), Sense::click());
            if let Some(project) = project
                && response.clicked()
            {
                new.project = project;
            }
            let chosen = project == Some(new.project);
            let fill = if chosen {
                theme.active
            } else if enabled && response.hovered() {
                theme.hover
            } else {
                theme.pill
            };
            ui.painter().rect_filled(rect, theme.control_radius, fill);
            icons::paint(
                ui.painter(),
                rect.shrink(8.0),
                icon,
                if enabled {
                    theme.text
                } else {
                    theme.muted.gamma_multiply(0.6)
                },
            );
            response.on_hover_text(if enabled {
                tip.to_owned()
            } else {
                format!("{tip} (not available yet)")
            });
        }
    });
    egui::Grid::new("sue-paint-new")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, "File name:", theme);
            ui.add(egui::TextEdit::singleline(&mut new.name).desired_width(260.0));
            ui.end_row();

            if new.project == Project::Animation {
                label(ui, "Frames:", theme);
                ui.add(egui::DragValue::new(&mut new.frames).range(1..=10_000));
                ui.end_row();
                label(ui, "Frame rate:", theme);
                ui.add(
                    egui::DragValue::new(&mut new.fps)
                        .range(1..=120)
                        .suffix(" fps"),
                );
                ui.end_row();
            }
            if new.project == Project::Comic {
                label(ui, "Pages:", theme);
                ui.add(egui::DragValue::new(&mut new.pages).range(1..=500));
                ui.end_row();
                label(ui, "Binding:", theme);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut new.right_bound, true, "Right (manga, right to left)");
                    ui.selectable_value(&mut new.right_bound, false, "Left (left to right)");
                });
                ui.end_row();
            }

            label(ui, "Preset:", theme);
            ui.horizontal(|ui| {
                let shown = new.preset.clone().unwrap_or_else(|| "Custom".to_owned());
                let mut picked = None;
                egui::ComboBox::from_id_salt("sue-paint-preset")
                    .selected_text(shown)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for preset in presets.for_target(NEW_PRESET_TARGET) {
                            let chosen = new.preset.as_deref() == Some(preset.name.as_str());
                            if ui.selectable_label(chosen, &preset.name).clicked() {
                                picked = Some(preset.clone());
                            }
                        }
                    });
                if let Some(preset) = picked {
                    new.apply_preset(&preset);
                }
                ui.add_enabled(false, egui::Button::new("Register"))
                    .on_disabled_hover_text("Saving your own presets arrives later");
                label(ui, "Unit:", theme);
                egui::ComboBox::from_id_salt("sue-paint-unit")
                    .selected_text(new.unit.label())
                    .width(56.0)
                    .show_ui(ui, |ui| {
                        for unit in Unit::ALL {
                            ui.selectable_value(&mut new.unit, unit, unit.label());
                        }
                    });
            });
            ui.end_row();
        });
    ui.separator();
    label(ui, "Canvas", theme);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            egui::Grid::new("sue-paint-canvas")
                .num_columns(3)
                .spacing(vec2(10.0, 8.0))
                .show(ui, |ui| {
                    label(ui, "Width:", theme);
                    if length_field(ui, &mut new.width, new.unit, new.dpi) {
                        new.size = 0;
                    }
                    let size_name = SIZES[new.size].0;
                    let mut picked = None;
                    egui::ComboBox::from_id_salt("sue-paint-size")
                        .selected_text(size_name)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (index, (name, ..)) in SIZES.iter().enumerate().skip(1) {
                                if ui.selectable_label(new.size == index, *name).clicked() {
                                    picked = Some(index);
                                }
                            }
                        });
                    ui.end_row();
                    label(ui, "Height:", theme);
                    if length_field(ui, &mut new.height, new.unit, new.dpi) {
                        new.size = 0;
                    }
                    if ui
                        .button("Swap")
                        .on_hover_text("Swap width and height")
                        .clicked()
                    {
                        std::mem::swap(&mut new.width, &mut new.height);
                    }
                    ui.end_row();
                    label(ui, "Resolution:", theme);
                    ui.add(
                        egui::DragValue::new(&mut new.dpi)
                            .range(1.0..=2400.0)
                            .max_decimals(0)
                            .suffix(" dpi"),
                    );
                    egui::ComboBox::from_id_salt("sue-paint-dpi")
                        .selected_text("Common")
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for dpi in [72.0, 150.0, 300.0, 350.0, 600.0] {
                                if ui
                                    .selectable_label(new.dpi == dpi, format!("{dpi:.0}"))
                                    .clicked()
                                {
                                    new.dpi = dpi;
                                }
                            }
                        });
                    ui.end_row();
                    label(ui, "Basic expression color:", theme);
                    egui::ComboBox::from_id_salt("sue-paint-color-mode")
                        .selected_text("Color")
                        .width(110.0)
                        .show_ui(ui, |ui| {
                            let _ = ui.selectable_label(true, "Color");
                            ui.add_enabled(false, egui::Button::selectable(false, "Gray"));
                            ui.add_enabled(false, egui::Button::selectable(false, "Monochrome"));
                        });
                    ui.end_row();
                    if let Some(index) = picked {
                        let (_, width, height, dpi) = SIZES[index];
                        new.width = width;
                        new.height = height;
                        new.dpi = dpi;
                        new.size = index;
                        new.preset = None;
                    }
                });
        });
        // A preview of the canvas's shape and paper.
        let (rect, _) = ui.allocate_exact_size(vec2(110.0, 110.0), Sense::hover());
        let aspect = new.width.max(1) as f32 / new.height.max(1) as f32;
        let shape = if aspect >= 1.0 {
            vec2(96.0, 96.0 / aspect)
        } else {
            vec2(96.0 * aspect, 96.0)
        };
        let page = egui::Rect::from_center_size(rect.center(), shape);
        let [r, g, b] = new.paper_color;
        let fill = if new.paper {
            egui::Color32::from_rgb(r, g, b)
        } else {
            theme.pill
        };
        ui.painter().rect_filled(page, 0.0, fill);
        ui.painter()
            .rect_stroke(page, 0.0, theme.hairline(), egui::StrokeKind::Outside);
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut new.paper, "Paper color");
        ui.add_enabled_ui(new.paper, |ui| {
            let [r, g, b] = new.paper_color;
            let mut rgba = [r, g, b, 255];
            if sue_wm::color_button(ui, "sue-paint-new-paper", &mut rgba, false, theme).changed {
                new.paper_color = [rgba[0], rgba[1], rgba[2]];
            }
        });
    });
    ui.add_enabled_ui(false, |ui| {
        ui.checkbox(&mut false, "Template");
        ui.horizontal(|ui| {
            ui.checkbox(&mut false, "Color profile");
            label(ui, "Working profile: sRGB", theme);
        });
        ui.checkbox(&mut false, "Create animated illustration");
        ui.horizontal(|ui| {
            label(ui, "Record timelapse:", theme);
            egui::ComboBox::from_id_salt("sue-paint-timelapse")
                .selected_text("Off")
                .show_ui(ui, |_| {});
        });
    });
    ui.label(
        RichText::new(format!(
            "{} x {} px, {:.1} MB per layer",
            new.width,
            new.height,
            f64::from(new.width) * f64::from(new.height) * 4.0 / 1_048_576.0
        ))
        .size(theme.small_size)
        .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("Cancel").clicked() {
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

/// A filter dialog's body: its settings, the preview switch, OK and Cancel.
/// A filter's or correction's settings.
fn filter_params(ui: &mut Ui, filter: &mut Filter, theme: &Theme) {
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
                for (index, name) in ["RGB", "Red", "Green", "Blue"].iter().enumerate() {
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
                    input: "Input",
                    low: "0",
                    high: "255",
                    output: "Output",
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
            ui.label(RichText::new("Input").color(theme.muted));
            ui.add(egui::Slider::new(black, 0.0..=0.99).text("Shadow"));
            ui.add(
                egui::Slider::new(gamma, 0.1..=10.0)
                    .logarithmic(true)
                    .text("Gamma"),
            );
            ui.add(egui::Slider::new(white, 0.01..=1.0).text("Highlight"));
            ui.label(RichText::new("Output").color(theme.muted));
            ui.add(egui::Slider::new(out_black, 0.0..=1.0).text("Shadow"));
            ui.add(egui::Slider::new(out_white, 0.0..=1.0).text("Highlight"));
        }
        Filter::ColorBalance { shifts, range } => {
            ui.horizontal(|ui| {
                for (index, name) in ["Shadow", "Midtone", "Highlight"].iter().enumerate() {
                    ui.selectable_value(range, index as u8, *name);
                }
            });
            let shift = &mut shifts[usize::from((*range).min(2))];
            ui.add(egui::Slider::new(&mut shift[0], -1.0..=1.0).text("Cyan to red"));
            ui.add(egui::Slider::new(&mut shift[1], -1.0..=1.0).text("Magenta to green"));
            ui.add(egui::Slider::new(&mut shift[2], -1.0..=1.0).text("Yellow to blue"));
        }
        Filter::GradientMap { from, to } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Black becomes").color(theme.muted));
                ui.color_edit_button_srgb(from);
                ui.label(RichText::new("white becomes").color(theme.muted));
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
        Filter::Invert | Filter::Recolor { .. } | Filter::BrightnessToOpacity { .. } => {}
    }
}

fn filter_ui(ui: &mut Ui, dialog: &mut FilterDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    filter_params(ui, &mut dialog.filter, theme);
    ui.checkbox(&mut dialog.preview, "Preview");
    ui.label(
        RichText::new(if dialog.selection.is_some() {
            "Changes the selected area of the layer"
        } else {
            "Changes the whole layer"
        })
        .size(theme.small_size)
        .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("Cancel").clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_pressure_fits_to_how_hard_someone_presses() {
        assert_eq!(PressureDialog::fit(&[0.5; 3]), None, "not enough drawn");
        // A light hand: presses between 5% and 60%.
        let light: Vec<f32> = (0..200)
            .map(|step| 0.05 + 0.55 * step as f32 / 199.0)
            .collect();
        let curve = PressureDialog::fit(&light).unwrap();
        assert!(
            curve.eval(0.6) > 0.85,
            "a firm press is a full line: {}",
            curve.eval(0.6)
        );
        let usual = curve.eval(0.33);
        assert!((usual - 0.5).abs() < 0.15, "a usual press is half: {usual}");
        assert!((curve.eval(1.0) - 1.0).abs() < 1e-3);
        // Stronger and Lighter move it up and down.
        let stronger = curve.map_outputs(|y| y.powf(0.8));
        assert!(stronger.eval(0.33) > usual);
        // It reads back from its setting.
        assert_eq!(
            curve
                .to_string()
                .parse::<Response>()
                .unwrap()
                .points()
                .len(),
            curve.points().len()
        );
    }

    #[test]
    fn presets_and_paper_sizes_fill_the_dialog() {
        let library = new_presets();
        let mut dialog = NewDialog::new(&NewPicture::default());
        assert_eq!(dialog.spec(), NewPicture::default());
        let sprite = library
            .get(NEW_PRESET_TARGET, "Transparent Sprite")
            .unwrap();
        dialog.apply_preset(sprite);
        let spec = dialog.spec();
        assert_eq!((spec.width, spec.height, spec.paper), (512, 512, None));
        assert!(library.for_target(NEW_PRESET_TARGET).count() >= 5);
        for (name, width, height, dpi) in SIZES.iter().skip(1) {
            assert!(*width > 0 && *height > 0 && *dpi > 0.0, "{name}");
        }
        // A4 at 350 dpi is 210 mm wide.
        let a4 = SIZES[1];
        let mm = f64::from(a4.1) / a4.3 * Unit::Mm.per_inch(a4.3);
        assert!((mm - 210.0).abs() < 0.5, "{mm}");
        dialog.name = "  ".to_owned();
        assert_eq!(dialog.spec().name, "Illustration");
    }
}
