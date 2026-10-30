//! Sue Paint's dialogs: New (a canvas from presets, paper sizes and
//! units), the filter dialogs with a live preview, and the shortcut list.

use egui::{Align, Id, Layout, Order, Rect, RichText, Sense, Ui, pos2, vec2};
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
pub(crate) const SIZES: [(&str, u32, u32, f64); 11] = [
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
        (tr!("illustration"), 1600_i64, 1200_i64, 72.0, true),
        (tr!("print-a4-350-dpi"), 2894, 4093, 350.0, true),
        (tr!("screen-full-hd"), 1920, 1080, 72.0, true),
        (tr!("square-post"), 2048, 2048, 72.0, true),
        (tr!("transparent-sprite"), 512, 512, 72.0, false),
        (tr!("webtoon-800-x-12800"), 800, 12800, 72.0, true),
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
    /// One tall picture to scroll down (Webtoon).
    Webtoon,
    /// A story of pages.
    Comic,
    /// A story of pages sized for a printed fanzine.
    Fanzine,
    /// A story of pages with every comic setting shown.
    ComicAll,
    /// A picture with a timeline.
    Animation,
}

impl Project {
    /// Every project, in the order of Clip Studio Paint's Project row.
    pub(crate) const ALL: [Self; 6] = [
        Self::Illustration,
        Self::Webtoon,
        Self::Comic,
        Self::Fanzine,
        Self::ComicAll,
        Self::Animation,
    ];

    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Illustration => Icon::ProjectIllustration,
            Self::Webtoon => Icon::ProjectWebtoon,
            Self::Comic => Icon::ProjectComic,
            Self::Fanzine => Icon::ProjectFanzine,
            Self::ComicAll => Icon::ProjectComicAll,
            Self::Animation => Icon::ProjectAnimation,
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Illustration => tr!("illustration"),
            Self::Webtoon => tr!("webtoon"),
            Self::Comic => tr!("comic"),
            Self::Fanzine => tr!("printing-of-fanzine"),
            Self::ComicAll => tr!("show-all-comic-settings"),
            Self::Animation => tr!("animation"),
        }
    }

    /// A story of pages.
    pub(crate) fn is_comic(self) -> bool {
        matches!(self, Self::Comic | Self::Fanzine | Self::ComicAll)
    }
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
    /// Record timelapse from the start.
    pub timelapse: bool,
}

impl NewDialog {
    /// Chooses `project`, with the canvas Clip Studio Paint starts it at.
    pub(crate) fn choose(&mut self, project: Project) {
        if project == self.project {
            return;
        }
        self.project = project;
        self.preset = None;
        self.size = 0;
        let (width, height, dpi, name) = match project {
            Project::Illustration => (1600, 1200, 72.0, tr!("illustration")),
            Project::Webtoon => (690, 20_000, 350.0, tr!("webtoon")),
            // B5 and A5 at 600 and 350 dots an inch.
            Project::Comic | Project::ComicAll => (4299, 6070, 600.0, tr!("comic")),
            Project::Fanzine => (2039, 2894, 350.0, tr!("comic")),
            Project::Animation => (1920, 1080, 72.0, tr!("animation")),
        };
        self.width = width;
        self.height = height;
        self.dpi = dpi;
        self.name = name;
        if project == Project::Fanzine {
            self.pages = 16;
        }
    }

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
            timelapse: false,
        }
    }

    pub(crate) fn spec(&self) -> NewPicture {
        NewPicture {
            name: if self.name.trim().is_empty() {
                tr!("illustration")
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
            timelapse: self.timelapse,
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
}

/// The setting that holds the pen pressure curve every tool uses.
pub(crate) const PEN_PRESSURE: &str = "sue_paint.pen_pressure";

/// The pen pressure curve Sue Paint starts with: a little softer than
/// straight, as most hands press drawing tablets, so a usual press is
/// already more than half a line. File, Pen Pressure Settings fits it to a
/// hand.
pub(crate) const DEFAULT_PEN_PRESSURE: &str = "0,0 0.4,0.5 1,1";

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
    /// Onion Skin Settings.
    Onion(crate::anim::Onion),
    /// Export Timelapse: the video's size and length.
    Timelapse(crate::timelapse::Export),
    /// Register Material, Image: its name.
    Material(String),
    /// Expand (0), Shrink (1) or Blur the border of (2) the selection, by
    /// pixels.
    SelectBy(u8, f32),
    /// Select Color Gamut: the tolerance and whether it looks at every
    /// layer.
    Gamut(f32, bool),
    /// Export Webtoon: the height each piece is cut at and the width the
    /// pieces are scaled to, in pixels.
    Webtoon(u32, u32),
    /// Edit Timeline: frames a second and length in frames.
    Timeline(u32, u32),
    /// New 3D Layer: which of Sue Paint's shapes.
    Shape3D(usize),
    /// Register Workspace: its name.
    Workspace(String),
    /// Manage Workspaces.
    Workspaces,
    /// A page to read: its title and paragraphs.
    Message(String, Vec<String>),
    /// Print Settings, or Print when set.
    Print(crate::print::PrintSettings, bool),
    /// Modifier Key Settings: what Alt, Space and Ctrl+Shift do on the
    /// canvas (indexes into their choices).
    Modifiers([usize; 3]),
    /// Command Bar Settings.
    CommandBar(String),
    /// File, Preferences: Sue Paint's own, page by page.
    Preferences(crate::prefs::Preferences),
    /// Canvas Properties.
    CanvasProperties(CanvasProperties),
    /// Color Profile Preview Settings: which profile.
    Proof(usize),
    /// Change Page Settings.
    PageSettings(PageSettings),
    /// Change Story Information.
    StoryInfo(StoryInfo),
    /// The Story Editor: each text layer's page number, page, layer and
    /// words.
    StoryEditor(Vec<(usize, sue_core::EntityId, sue_core::EntityId, String)>),
    /// Convert Layer: the layer, to raster (0) or vector (1), and its name.
    Convert(sue_core::EntityId, u8, String),
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

/// Change Page Settings: every page's size, resolution and borders.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageSettings {
    pub story: sue_core::EntityId,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub bleed: f64,
    pub inner: f64,
}

/// Change Story Information.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoryInfo {
    pub story: sue_core::EntityId,
    pub title: String,
    pub author: String,
    pub first: i64,
    pub right_bound: bool,
}

/// What each modifier can do: its rice key, label and choices.
pub(crate) const MODIFIERS: [(&str, &str, &[&str]); 3] = [
    (
        "sue_paint.alt_tool",
        "Alt while drawing",
        &["eyedropper", "eraser", "none"],
    ),
    ("sue_paint.space_tool", "Space", &["hand", "none"]),
    (
        "sue_paint.ctrl_shift",
        "Ctrl+Shift click",
        &["pick_layer", "none"],
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Open,
    Cancel,
    Ok,
}

/// What the pen and mouse do, for the Keyboard Shortcuts dialog under
/// the keys Settings has: Clip Studio Paint's gestures.
pub(crate) const GESTURES: [(&str, &str); 11] = [
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
            Dialog::New(_) => (tr!("new-2"), vec2(500.0, 690.0)),
            // The gallery keeps one title (and so one window) whatever look
            // is picked.
            Dialog::Filter(FilterDialog {
                filter: Filter::Quick { effect, .. },
                ..
            }) => (
                if *effect {
                    tr!("quick-effects")
                } else {
                    tr!("quick-filters")
                },
                vec2(500.0, 400.0),
            ),
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
            Dialog::Shortcuts => (tr!("keyboard-shortcuts"), vec2(470.0, 560.0)),
            Dialog::Pages(_) => (tr!("page-manager"), vec2(820.0, 620.0)),
            Dialog::Frame(_) => (tr!("new-frame-border-folder"), vec2(420.0, 330.0)),
            Dialog::Canvas(_) => (tr!("change-canvas-size"), vec2(380.0, 300.0)),
            Dialog::Resolution(_) => (tr!("change-image-resolution"), vec2(380.0, 250.0)),
            Dialog::Text(..) => (tr!("text"), vec2(440.0, 380.0)),
            Dialog::Perspective(_) => (tr!("create-perspective-ruler"), vec2(320.0, 190.0)),
            Dialog::Onion(_) => (tr!("onion-skin-settings"), vec2(360.0, 330.0)),
            Dialog::Timelapse(_) => (tr!("export-timelapse"), vec2(360.0, 230.0)),
            Dialog::Material(_) => (tr!("register-material"), vec2(360.0, 160.0)),
            Dialog::SelectBy(kind, _) => (
                match kind {
                    0 => tr!("expand-selected-area-2"),
                    1 => tr!("shrink-selected-area-2"),
                    3 => tr!("outline-selection"),
                    _ => tr!("blur-border"),
                },
                vec2(320.0, 140.0),
            ),
            Dialog::Gamut(..) => (tr!("select-color-gamut"), vec2(340.0, 170.0)),
            Dialog::Webtoon(..) => (tr!("export-webtoon"), vec2(340.0, 170.0)),
            Dialog::Timeline(..) => (tr!("edit-timeline"), vec2(320.0, 150.0)),
            Dialog::Shape3D(_) => (tr!("new-3d-layer"), vec2(300.0, 230.0)),
            Dialog::Workspace(_) => (tr!("register-workspace"), vec2(340.0, 140.0)),
            Dialog::Workspaces => (tr!("manage-workspaces"), vec2(380.0, 320.0)),
            Dialog::Message(title, _) => (title.clone(), vec2(520.0, 460.0)),
            Dialog::Print(_, true) => (tr!("print"), vec2(380.0, 330.0)),
            Dialog::Print(_, false) => (tr!("print-settings"), vec2(380.0, 330.0)),
            Dialog::Modifiers(_) => (tr!("modifier-key-settings"), vec2(400.0, 230.0)),
            Dialog::Preferences(_) => (tr!("preferences"), vec2(780.0, 540.0)),
            Dialog::CommandBar(_) => (tr!("command-bar-settings"), vec2(420.0, 460.0)),
            Dialog::CanvasProperties(_) => (tr!("canvas-properties"), vec2(380.0, 230.0)),
            Dialog::Proof(_) => (tr!("color-profile-preview-settings"), vec2(400.0, 230.0)),
            Dialog::PageSettings(_) => (tr!("change-page-settings"), vec2(380.0, 270.0)),
            Dialog::StoryInfo(_) => (tr!("change-story-information"), vec2(380.0, 240.0)),
            Dialog::StoryEditor(_) => (tr!("story-editor"), vec2(520.0, 520.0)),
            Dialog::Convert(..) => (tr!("convert-layer"), vec2(360.0, 190.0)),
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
                tr!("adjust-pen-pressure"),
                vec2(560.0, if pressure.show_graph { 520.0 } else { 330.0 }),
            ),
        };
        let title = title.to_owned();
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
        // The dialog's body, in its own window or (in the browser and on
        // phones) over the studio.
        let own_window = !ctx.embed_viewports();
        let mut body = |ui: &mut Ui, inner: egui::Vec2| {
            ui.set_min_size(inner);
            ui.set_max_size(inner);
            if !own_window {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title.as_str())
                            .size(theme.ui_size + 1.0)
                            .color(theme.text),
                    );
                });
                ui.separator();
            }
            // A dialog larger than the screen (a phone) scrolls inside
            // its frame, so its buttons stay in reach.
            egui::ScrollArea::both()
                .id_salt("sue-paint-dialog-body")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    match &mut dialog {
                        Dialog::New(new) => outcome = new_ui(ui, new, theme),
                        Dialog::Filter(filter) => outcome = filter_ui(ui, filter, theme),
                        Dialog::Pressure(pressure) => outcome = pressure_ui(ui, pressure, theme),
                        Dialog::Pages(pages) => outcome = self.pages_ui(ui, pages, cx, theme),
                        Dialog::Frame(frame) => outcome = frame_ui(ui, frame, theme),
                        Dialog::Text(_, lettering) => outcome = text_ui(ui, lettering, theme),
                        Dialog::Onion(onion) => outcome = onion_ui(ui, onion, theme),
                        Dialog::SelectBy(kind, amount) => {
                            ui.horizontal(|ui| {
                                label(
                                    ui,
                                    &if *kind == 2 {
                                        tr!("blur-width")
                                    } else {
                                        tr!("width")
                                    },
                                    theme,
                                );
                                ui.add(egui::Slider::new(amount, 1.0..=100.0).suffix(tr!("px")));
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Workspace(name) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("name"), theme);
                                ui.text_edit_singleline(name);
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Workspaces => {
                            let mut chosen = None;
                            let mut gone = None;
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for (index, space) in self.workspaces.iter_mut().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.text_edit_singleline(&mut space.name);
                                            if ui.button(tr!("use")).clicked() {
                                                chosen = Some(index);
                                            }
                                            if ui.button(tr!("delete")).clicked() {
                                                gone = Some(index);
                                            }
                                        });
                                    }
                                    if self.workspaces.is_empty() {
                                        ui.label(
                                            RichText::new(tr!(
                                                "no-workspaces-yet-window-workspace-register"
                                            ))
                                            .size(theme.small_size)
                                            .color(theme.muted),
                                        );
                                    }
                                });
                            if let Some(index) = chosen {
                                self.use_workspace(index);
                            }
                            if let Some(index) = gone {
                                self.workspaces.remove(index);
                            }
                            ui.add_space(8.0);
                            if ui.button(tr!("close")).clicked() {
                                outcome = Outcome::Ok;
                            }
                        }
                        Dialog::Print(settings, printing) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("paper"), theme);
                                egui::ComboBox::from_id_salt("sue-paint-paper")
                                    .selected_text(crate::localize(
                                        crate::print::PAPERS[settings.paper].0,
                                    ))
                                    .show_ui(ui, |ui| {
                                        for (index, (name, ..)) in
                                            crate::print::PAPERS.iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut settings.paper,
                                                index,
                                                crate::localize(name),
                                            );
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
                                ui.add(
                                    egui::Slider::new(&mut settings.percent, 1.0..=400.0)
                                        .suffix(" %"),
                                );
                            }
                            ui.horizontal(|ui| {
                                label(ui, &tr!("margin"), theme);
                                ui.add(
                                    egui::Slider::new(&mut settings.margin, 0.0..=50.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.checkbox(&mut settings.all_pages, tr!("every-page-of-the-story"));
                            if *printing {
                                ui.label(
                                    RichText::new(tr!("pages-open-as-pdf-your-viewer"))
                                        .size(theme.small_size)
                                        .color(theme.muted),
                                );
                            }
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Preferences(prefs) => match prefs.ui(ui, theme) {
                            Some(true) => outcome = Outcome::Ok,
                            Some(false) => outcome = Outcome::Cancel,
                            None => {}
                        },
                        Dialog::Modifiers(chosen) => {
                            for ((key, name, choices), pick) in
                                MODIFIERS.iter().zip(chosen.iter_mut())
                            {
                                // Each choice reads as Settings names it.
                                let choice_text = |choice: &str| {
                                    sue_lang::text_or(
                                        "rice",
                                        &format!("{}.{choice}", key.replace('.', "-")),
                                        &choice.replace('_', " "),
                                    )
                                };
                                ui.horizontal(|ui| {
                                    label(ui, &crate::localize(name), theme);
                                    egui::ComboBox::from_id_salt(("sue-paint-modifier", *name))
                                        .selected_text(choice_text(
                                            choices[(*pick).min(choices.len() - 1)],
                                        ))
                                        .show_ui(ui, |ui| {
                                            for (index, choice) in choices.iter().enumerate() {
                                                ui.selectable_value(
                                                    pick,
                                                    index,
                                                    choice_text(choice),
                                                );
                                            }
                                        });
                                });
                            }
                            ui.label(
                                RichText::new(tr!("space-shift-turns-view-ctrl-alt"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::CommandBar(adding) => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 150.0)
                                .show(ui, |ui| {
                                    for group in crate::panels::BAR_GROUPS {
                                        for (_, command, tip) in group.iter() {
                                            let mut shown = !self.bar_hidden.contains(*command);
                                            if ui
                                                .checkbox(&mut shown, crate::localize(tip))
                                                .changed()
                                            {
                                                if shown {
                                                    self.bar_hidden.remove(*command);
                                                } else {
                                                    self.bar_hidden.insert((*command).to_owned());
                                                }
                                            }
                                        }
                                        ui.separator();
                                    }
                                    let mut gone = None;
                                    for (index, command) in self.bar_extra.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                crate::menus::label_of(&self.menus, command)
                                                    .unwrap_or(command),
                                            );
                                            if ui.small_button(tr!("remove")).clicked() {
                                                gone = Some(index);
                                            }
                                        });
                                    }
                                    if let Some(index) = gone {
                                        self.bar_extra.remove(index);
                                    }
                                });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("add"), theme);
                                let picked = crate::menus::label_of(&self.menus, adding)
                                    .unwrap_or(&tr!("a-menu-command"))
                                    .to_owned();
                                egui::ComboBox::from_id_salt("sue-paint-bar-add")
                                    .selected_text(picked)
                                    .height(320.0)
                                    .show_ui(ui, |ui| {
                                        for (label, command) in crate::menus::commands(&self.menus)
                                        {
                                            ui.selectable_value(adding, command, label);
                                        }
                                    });
                                if ui.button(tr!("add-2")).clicked()
                                    && !adding.is_empty()
                                    && !self.bar_extra.contains(adding)
                                {
                                    self.bar_extra.push(adding.clone());
                                }
                            });
                            ui.add_space(6.0);
                            if ui.button(tr!("close")).clicked() {
                                outcome = Outcome::Ok;
                            }
                        }
                        Dialog::PageSettings(settings) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("size"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.width)
                                        .range(1..=10_000)
                                        .suffix(tr!("px")),
                                );
                                ui.label("by");
                                ui.add(
                                    egui::DragValue::new(&mut settings.height)
                                        .range(1..=10_000)
                                        .suffix(tr!("px")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("resolution"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.dpi)
                                        .range(1.0..=2400.0)
                                        .suffix(tr!("dpi")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("bleed"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.bleed)
                                        .range(0.0..=50.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("inner-border"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.inner)
                                        .range(0.0..=100.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.label(
                                RichText::new(tr!("every-page-story-changes-each-keeps"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::StoryInfo(info) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("title"), theme);
                                ui.text_edit_singleline(&mut info.title);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("author"), theme);
                                ui.text_edit_singleline(&mut info.author);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("first-page-number"), theme);
                                ui.add(egui::DragValue::new(&mut info.first).range(0..=9_999));
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("binding"), theme);
                                ui.radio_value(
                                    &mut info.right_bound,
                                    false,
                                    tr!("left-left-to-right"),
                                );
                                ui.radio_value(&mut info.right_bound, true, tr!("right-manga"));
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::StoryEditor(texts) => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 120.0)
                                .show(ui, |ui| {
                                    let mut last_page = 0;
                                    for (number, _, _, words) in texts.iter_mut() {
                                        if *number != last_page {
                                            last_page = *number;
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(tr!("page", number = number))
                                                    .color(theme.text)
                                                    .strong(),
                                            );
                                        }
                                        ui.add(
                                            egui::TextEdit::multiline(words)
                                                .desired_rows(2)
                                                .desired_width(f32::INFINITY),
                                        );
                                    }
                                });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Proof(profile) => {
                            ui.label(
                                RichText::new(tr!("how-picture-would-print-shown-while"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            for (index, (name, ..)) in crate::proof::PROFILES.iter().enumerate() {
                                ui.radio_value(profile, index, crate::localize(name));
                            }
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::CanvasProperties(props) => {
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
                        }
                        Dialog::Convert(_, kind, name) => {
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
                        }
                        Dialog::Message(_, paragraphs) => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 110.0)
                                .show(ui, |ui| {
                                    for paragraph in paragraphs.iter() {
                                        if paragraph.starts_with("https://") {
                                            ui.hyperlink(paragraph.as_str());
                                        } else if let Some(heading) = paragraph.strip_prefix("# ") {
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(heading)
                                                    .size(theme.ui_size)
                                                    .color(theme.text)
                                                    .strong(),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new(paragraph.as_str())
                                                    .size(theme.small_size + 0.5)
                                                    .color(theme.text),
                                            );
                                        }
                                    }
                                });
                            ui.add_space(8.0);
                            if ui.button(tr!("close")).clicked() {
                                outcome = Outcome::Ok;
                            }
                        }
                        Dialog::Gamut(tolerance, all) => {
                            ui.label(
                                RichText::new(tr!("selects-colors-near-drawing-color"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ui.horizontal(|ui| {
                                label(ui, &tr!("color-margin"), theme);
                                let mut percent = *tolerance * 100.0;
                                if ui
                                    .add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"))
                                    .changed()
                                {
                                    *tolerance = percent / 100.0;
                                }
                            });
                            ui.checkbox(all, tr!("refer-to-all-layers"));
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Webtoon(height, width) => {
                            egui::Grid::new("sue-paint-webtoon")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("split-every"), theme);
                                    ui.add(
                                        egui::DragValue::new(height)
                                            .range(200..=20000)
                                            .suffix(tr!("px")),
                                    );
                                    ui.end_row();
                                    label(ui, &tr!("output-width"), theme);
                                    ui.add(
                                        egui::DragValue::new(width)
                                            .range(100..=4000)
                                            .suffix(tr!("px")),
                                    );
                                    ui.end_row();
                                });
                            ui.label(
                                RichText::new(tr!("tall-canvas-cut-into-numbered-pictures"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Timeline(fps, length) => {
                            egui::Grid::new("sue-paint-timeline-edit")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("frame-rate-2"), theme);
                                    ui.add(
                                        egui::DragValue::new(fps).range(1..=120).suffix(tr!("fps")),
                                    );
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
                        }
                        Dialog::Shape3D(kind) => {
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
                        }
                        Dialog::Material(name) => {
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
                        }
                        Dialog::Timelapse(export) => {
                            egui::Grid::new("sue-paint-timelapse-export")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("long-side"), theme);
                                    egui::ComboBox::from_id_salt("sue-paint-timelapse-side")
                                        .selected_text(tr!("px-2", side = export.side))
                                        .show_ui(ui, |ui| {
                                            for side in [480, 720, 1080, 1280, 1920] {
                                                ui.selectable_value(
                                                    &mut export.side,
                                                    side,
                                                    tr!("px-2", side = side),
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    label(ui, &tr!("length"), theme);
                                    let mut fit = export.seconds.is_some();
                                    ui.vertical(|ui| {
                                        if ui
                                            .radio_value(
                                                &mut fit,
                                                true,
                                                tr!("fit-to-specified-length"),
                                            )
                                            .changed()
                                            || ui
                                                .radio_value(
                                                    &mut fit,
                                                    false,
                                                    tr!("export-all-frames"),
                                                )
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
                        }
                        Dialog::Perspective(count) => {
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
                        }
                        Dialog::Correction(_, filter, ..) => {
                            filter_params(ui, filter, theme);
                            ui.label(
                                RichText::new(tr!("changes-every-layer-under-one"))
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
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 90.0)
                                .show(ui, |ui| {
                                    let keys = keys.as_deref().unwrap_or_default();
                                    sue_wm::shortcuts::grid_ui(ui, "sue-paint-keys", keys, theme);
                                    ui.add_space(12.0);
                                    ui.label(
                                        RichText::new(tr!("with-the-pen-and-mouse"))
                                            .color(theme.text)
                                            .strong(),
                                    );
                                    let gestures: Vec<(String, String)> = GESTURES
                                        .iter()
                                        .map(|(what, how)| {
                                            (crate::localize(what), crate::localize(how))
                                        })
                                        .collect();
                                    sue_wm::shortcuts::grid_ui(
                                        ui,
                                        "sue-paint-gestures",
                                        &gestures,
                                        theme,
                                    );
                                });
                            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                                if ui.button(tr!("close")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                if ui
                                    .button(tr!("change-shortcuts-ellipsis"))
                                    .on_hover_text(tr!("settings-keymaps-any-number-keys-each"))
                                    .clicked()
                                {
                                    change_keys = true;
                                    outcome = Outcome::Cancel;
                                }
                            });
                        }
                    }
                });
        };
        let mut closed = false;
        let mut keys_here = (false, false);
        if own_window {
            // A window of its own, as Clip Studio Paint's dialogs: it can
            // be moved anywhere, outside Sue's window too, and resized.
            let main = ctx
                .input(|input| input.viewport().outer_rect)
                .unwrap_or(screen);
            let builder = egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_inner_size(size)
                .with_min_inner_size(vec2(240.0, 120.0))
                .with_position(main.center() - size / 2.0)
                .with_resizable(true);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("sue-paint-dialog", title.as_str())),
                builder,
                |ui, _| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, theme.panel);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            let inner = ui.available_size();
                            body(ui, inner);
                        });
                    closed = ui.input(|input| input.viewport().close_requested());
                    keys_here = ui.input(|input| {
                        (
                            input.key_pressed(egui::Key::Escape),
                            input.key_pressed(egui::Key::Enter),
                        )
                    });
                },
            );
        } else {
            egui::Area::new(Id::unique("sue-paint-dialog"))
                .fade_in(false)
                .order(Order::Foreground)
                .fixed_pos(at)
                .show(ctx, |ui| {
                    widgets::window_frame(theme)
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| body(ui, size - vec2(32.0, 32.0)));
                });
        }
        if closed {
            outcome = Outcome::Cancel;
        }
        let (escape, enter) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape) || keys_here.0,
                input.key_pressed(egui::Key::Enter) || keys_here.1,
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
                            | Filter::Fisheye { .. }
                            | Filter::Aberration { .. }
                            | Filter::Retro { .. }
                            | Filter::PerlinNoise { .. }
                            | Filter::Unsharp { .. }
                            | Filter::Dust { .. }
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
                if filter.merged {
                    // Take back the merged layer made for it.
                    self.run("edit.undo", cx);
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
            (Dialog::Timelapse(export), Outcome::Ok) => self.ask_export_timelapse(cx, export),
            (Dialog::Material(name), Outcome::Ok) => self.register_material(cx, &name),
            (Dialog::SelectBy(3, width), Outcome::Ok) => self.outline_selection(cx, width),
            (Dialog::SelectBy(kind, amount), Outcome::Ok) => {
                if let Some(doc) = self.doc_mut()
                    && let Some(selection) = doc.selection.clone()
                {
                    let changed = match kind {
                        0 => selection.expanded(amount),
                        1 => selection.shrunk(amount),
                        _ => selection.blurred(amount),
                    };
                    doc.set_selection(Some(changed));
                }
            }
            (Dialog::Gamut(tolerance, all), Outcome::Ok) => self.select_gamut(cx, tolerance, all),
            (Dialog::Workspace(name), Outcome::Ok) => {
                let name = if name.trim().is_empty() {
                    tr!("workspace")
                } else {
                    name.trim().to_owned()
                };
                let space = self.workspace_now(&name);
                match self.workspaces.iter().position(|known| known.name == name) {
                    Some(index) => self.workspaces[index] = space,
                    None => self.workspaces.push(space),
                }
                self.save_extras(cx);
            }
            (Dialog::Workspaces, _) => self.save_extras(cx),
            (Dialog::Message(..) | Dialog::Print(..) | Dialog::Modifiers(_), Outcome::Cancel) => {}
            (Dialog::Message(..), _) => {}
            (Dialog::Print(settings, printing), _) => {
                self.print = settings;
                if printing {
                    self.print_now(cx);
                }
            }
            (Dialog::Modifiers(chosen), _) => {
                for ((key, _, choices), pick) in MODIFIERS.iter().zip(chosen) {
                    let value = choices[pick.min(choices.len() - 1)].to_owned();
                    if let Err(error) =
                        cx.edit_rice(SetRiceValue::new(*key, RiceValue::Text(value)))
                    {
                        cx.fail(tr!("cannot-keep-the-modifier-keys", error = error));
                    }
                }
            }
            (Dialog::CommandBar(_), _) => self.save_extras(cx),
            (Dialog::Preferences(_), Outcome::Cancel) => {}
            (Dialog::Preferences(prefs), _) => {
                for (key, value) in prefs.changes() {
                    if cx.rice().get(key).as_ref() == Some(&value) {
                        continue;
                    }
                    if let Err(error) = cx.edit_rice(SetRiceValue::new(key, value)) {
                        cx.fail(tr!("cannot-keep-the-preferences", error = error));
                    }
                }
            }
            (
                Dialog::CanvasProperties(_)
                | Dialog::Convert(..)
                | Dialog::Proof(_)
                | Dialog::PageSettings(_)
                | Dialog::StoryInfo(_)
                | Dialog::StoryEditor(_),
                Outcome::Cancel,
            ) => {}
            (Dialog::PageSettings(settings), _) => {
                match crate::story::page_settings(
                    &cx.world.arena,
                    settings.story,
                    [settings.width.max(1), settings.height.max(1)],
                    settings.dpi,
                    settings.bleed,
                    settings.inner,
                ) {
                    Ok(edit) => {
                        Self::apply(cx, edit);
                        for doc in &mut self.docs {
                            doc.view.fitted = false;
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            (Dialog::StoryInfo(info), _) => {
                let mut edit = crate::doc::Edit::new(tr!("change-story-information"))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::AUTHOR,
                        info.author.clone(),
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::FIRST_NUMBER,
                        info.first,
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::BINDING,
                        if info.right_bound { "right" } else { "left" },
                    ));
                if !info.title.trim().is_empty() {
                    edit = edit.then(sue_core::txn::Rename::new(info.story, info.title.trim()));
                }
                Self::apply(cx, edit);
            }
            (Dialog::StoryEditor(texts), _) => {
                let mut edit = crate::doc::Edit::new(tr!("story-editor"));
                let mut changed = false;
                for (_, page, layer, words) in texts {
                    let arena = &cx.world.arena;
                    let same = arena
                        .get_by_id(layer)
                        .ok()
                        .and_then(crate::lettering::Lettering::of)
                        .is_some_and(|lettering| lettering.content == words);
                    if same {
                        continue;
                    }
                    let Some([width, height]) = arena
                        .get_by_id(page)
                        .ok()
                        .and_then(sue_model::picture::size)
                    else {
                        continue;
                    };
                    if let Some(step) =
                        crate::lettering::retext(arena, layer, &words, width, height)
                    {
                        edit = edit.then(step);
                        changed = true;
                    }
                }
                if changed {
                    Self::apply(cx, edit);
                }
            }
            (Dialog::Proof(profile), _) => {
                self.proof_profile = profile;
                self.proof_on = true;
            }
            (Dialog::CanvasProperties(props), _) => {
                let mut edit = crate::doc::Edit::new(tr!("canvas-properties")).then(
                    sue_core::txn::SetProperty::new(
                        props.picture,
                        sue_model::picture::DPI,
                        props.dpi,
                    ),
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
            (Dialog::Convert(layer, kind, name), _) => {
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
            (Dialog::Webtoon(height, width), Outcome::Ok) => {
                self.ask_export_webtoon(cx, height, width)
            }
            (Dialog::Shape3D(kind), Outcome::Ok) => {
                let name = crate::three::SHAPES[kind.min(crate::three::SHAPES.len() - 1)];
                let model = sue_core::Entity::empty(crate::localize(name), sue_model::class::MODEL)
                    .with_ext(crate::three::shape(kind));
                self.add_3d_layer(cx, name, model);
            }
            (Dialog::Timeline(fps, length), Outcome::Ok) => {
                if let Some(picture) = self.picture()
                    && let Some(edit) =
                        crate::anim::edit_timeline(&cx.world.arena, picture, fps, length)
                {
                    Self::apply(cx, edit);
                }
            }
            (Dialog::Onion(onion), Outcome::Ok) => {
                if let Some(picture) = self.picture() {
                    let edit = doc::Edit::new(tr!("onion-skin-settings")).then(
                        sue_core::txn::SetProperty::new(
                            picture,
                            crate::anim::ONION,
                            onion.to_text(),
                        ),
                    );
                    if Self::apply(cx, edit) {
                        // The settings are for seeing it.
                        self.onion = true;
                    }
                }
            }
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
                    Project::Illustration | Project::Webtoon => self.make_picture(cx, &spec),
                    Project::Comic | Project::Fanzine | Project::ComicAll => {
                        self.make_story(cx, &spec, new.pages as usize, new.right_bound);
                    }
                    Project::Animation => self.make_animation(cx, &spec, new.frames, new.fps),
                }
            }
            (Dialog::Pressure(pressure), Outcome::Ok) => {
                let text = pressure.curve.to_string();
                match cx.edit_rice(SetRiceValue::new(PEN_PRESSURE, RiceValue::Text(text))) {
                    Ok(_) => cx.notify(tr!("pen-pressure-adjusted-every-tool")),
                    Err(error) => cx.fail(tr!("cannot-keep-the-pen-pressure", error = error)),
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
            | (
                Dialog::Perspective(_)
                | Dialog::Onion(_)
                | Dialog::Timelapse(_)
                | Dialog::Material(_)
                | Dialog::SelectBy(..)
                | Dialog::Gamut(..)
                | Dialog::Webtoon(..)
                | Dialog::Timeline(..)
                | Dialog::Shape3D(_)
                | Dialog::Workspace(_),
                Outcome::Cancel,
            ) => {}
        }
    }
}

/// Onion Skin Settings, as Clip Studio Paint lays them out: how many cels
/// show before and after, their colors, and how faint they are.
fn onion_ui(ui: &mut Ui, onion: &mut crate::anim::Onion, theme: &Theme) -> Outcome {
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

/// Change Canvas Size: width, height and the anchor.
fn canvas_ui(ui: &mut Ui, canvas: &mut CanvasDialog, theme: &Theme) -> Outcome {
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
fn resolution_ui(ui: &mut Ui, resolution: &mut ResolutionDialog, theme: &Theme) -> Outcome {
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

/// The Text tool's words, font, size, ink and placement.
fn text_ui(ui: &mut Ui, lettering: &mut crate::lettering::Lettering, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let edit = ui.add(
        egui::TextEdit::multiline(&mut lettering.content)
            .desired_rows(5)
            .desired_width(f32::INFINITY)
            .hint_text(tr!("type-here")),
    );
    // The words take the keys while nothing else does.
    if ui.ctx().memory(|memory| memory.focused().is_none()) {
        edit.request_focus();
    }
    egui::Grid::new("sue-paint-text")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("font"), theme);
            let shown = if lettering.font.is_empty() {
                tr!("default-font-ubuntu-light")
            } else {
                lettering.font.clone()
            };
            egui::ComboBox::from_id_salt("sue-paint-text-font")
                .selected_text(shown)
                .width(240.0)
                .height(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut lettering.font,
                        String::new(),
                        tr!("default-font-ubuntu-light"),
                    );
                    let selected = lettering.font.clone();
                    if let Some(family) = sue_wm::fonts::list_ui(ui, &selected, "") {
                        lettering.font = family;
                    }
                });
            ui.end_row();
            label(ui, &tr!("size"), theme);
            ui.add(
                egui::Slider::new(&mut lettering.size, 6.0..=400.0)
                    .logarithmic(true)
                    .suffix(tr!("px")),
            );
            ui.end_row();
            label(ui, &tr!("color"), theme);
            ui.color_edit_button_srgb(&mut lettering.rgb);
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.checkbox(&mut lettering.center, tr!("centered"));
        ui.checkbox(
            &mut lettering.vertical,
            tr!("vertical-columns-right-to-left"),
        );
    });
    ui.label(
        RichText::new(tr!("enter-starts-new-line-ctrl-enter"))
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
        if ui.button(tr!("cancel")).clicked() {
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
            label(ui, &tr!("layout"), theme);
            egui::ComboBox::from_id_salt("sue-paint-frame-layout")
                .selected_text(crate::localize(crate::manga::LAYOUTS[frame.layout].0))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (index, (name, _)) in crate::manga::LAYOUTS.iter().enumerate() {
                        ui.selectable_value(&mut frame.layout, index, crate::localize(name));
                    }
                });
            ui.end_row();
            label(ui, &tr!("border-width"), theme);
            ui.add(egui::Slider::new(&mut frame.border, 0.0..=40.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("margin"), theme);
            ui.add(egui::Slider::new(&mut frame.margin, 0.0..=400.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("gap-across"), theme);
            ui.add(egui::Slider::new(&mut frame.gap, 0.0..=200.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("gap-down"), theme);
            ui.add(egui::Slider::new(&mut frame.gap_y, 0.0..=200.0).suffix(tr!("px")));
            ui.end_row();
        });
    ui.label(
        RichText::new(tr!("layers-under-frame-show-only-inside"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("OK").clicked() {
            outcome = Outcome::Ok;
        }
        if ui.button(tr!("cancel")).clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// Pen Pressure Settings: the words, the buttons, the pad and the graph.
fn pressure_ui(ui: &mut Ui, pressure: &mut PressureDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let words = if pressure.checked {
        tr!("draw-again-try-adjusted-pressure-stronger")
    } else {
        tr!("draw-freely-pad-pen-while-drawing")
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
                    if ui
                        .add_sized(half, egui::Button::new(tr!("stronger")))
                        .clicked()
                    {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(0.8));
                    }
                    if ui
                        .add_sized(half, egui::Button::new(tr!("lighter")))
                        .clicked()
                    {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(1.25));
                    }
                });
                if ui.add_sized(wide, egui::Button::new(tr!("done"))).clicked() {
                    outcome = Outcome::Ok;
                }
            } else {
                let enough = pressure.raw.len() >= PressureDialog::ENOUGH;
                let check = ui
                    .add_enabled_ui(enough, |ui| {
                        ui.add_sized(wide, egui::Button::new(tr!("check-adjusted-settings")))
                    })
                    .inner
                    .on_disabled_hover_text(tr!("draw-few-lines-pen-first"));
                if check.clicked()
                    && let Some(curve) = PressureDialog::fit(&pressure.raw)
                {
                    pressure.curve = curve;
                    pressure.checked = true;
                    pressure.lines.clear();
                }
            }
            if ui
                .add_sized(wide, egui::Button::new(tr!("cancel")))
                .clicked()
            {
                outcome = Outcome::Cancel;
            }
            if ui
                .add_sized(wide, egui::Button::new(tr!("reset-to-default")))
                .clicked()
            {
                pressure.curve = DEFAULT_PEN_PRESSURE.parse().unwrap_or_default();
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
    } else if sue_platform::stylus::source().is_some() {
        // A WinTab tablet or a pen in a browser.
        pressure.force = sue_platform::stylus::pressure();
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
        RichText::new(tr!("show-pen-pressure-graph")).color(theme.text),
    );
    if pressure.show_graph {
        widgets::response_curve(
            ui,
            "pen-pressure",
            &mut pressure.curve,
            150.0,
            widgets::CurveLabels::pressure(),
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
                    .suffix(tr!("px")),
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

/// A label right-aligned in a cell `width` wide, as Clip Studio Paint
/// sets its dialogs' labels.
fn right_label(ui: &mut Ui, text: &str, width: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        format!("{}:", text.trim_end_matches(':')),
        theme.font(theme.ui_size),
        theme.text,
    );
}

/// A section's name with a line running to the right of it.
fn section(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).color(theme.text));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
        ui.painter()
            .hline(rect.x_range(), rect.center().y, theme.hairline());
    });
}

/// The New dialog's body, laid out as Clip Studio Paint's: the Project
/// row with OK and Cancel beside it, the file name and preset, the Canvas,
/// then the project's own settings and Record timelapse at the bottom.
#[allow(clippy::too_many_lines, reason = "the whole New dialog")]
fn new_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let presets = new_presets();
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
    let column = 82.0;
    // Project, and OK and Cancel at the right.
    ui.horizontal(|ui| {
        right_label(ui, &tr!("project"), 72.0, theme);
        let group = ui.available_width() - 100.0;
        let side = ((group - 22.0) / 6.0).clamp(30.0, 44.0);
        let (strip, _) =
            ui.allocate_exact_size(vec2(side * 6.0 + 20.0, side + 4.0), Sense::hover());
        ui.painter().rect_filled(strip, 2.0, theme.pill);
        for (index, project) in Project::ALL.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                strip.min + vec2(2.0 + index as f32 * side, 2.0),
                vec2(side, side),
            );
            let response = ui
                .interact(
                    rect,
                    Id::unique(("sue-paint-new-project", index)),
                    Sense::click(),
                )
                .on_hover_text(project.label());
            if new.project == project {
                ui.painter().rect_filled(rect, 2.0, theme.selection);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 2.0, theme.hover);
            }
            icons::paint(
                ui.painter(),
                rect.shrink(side * 0.12),
                project.icon(),
                theme.text,
            );
            if response.clicked() {
                new.choose(project);
            }
        }
        // The list of projects by name, as the arrow at the row's end.
        let arrow = Rect::from_min_max(
            pos2(strip.right() - 18.0, strip.top()),
            strip.right_bottom(),
        );
        let response = ui.interact(arrow, Id::unique("sue-paint-new-projects"), Sense::click());
        icons::paint(
            ui.painter(),
            Rect::from_center_size(arrow.center(), vec2(10.0, 10.0)),
            Icon::Down,
            theme.muted,
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.set_min_width(240.0);
            for project in Project::ALL {
                let row = ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                    icons::paint(ui.painter(), rect.shrink(3.0), project.icon(), theme.text);
                    ui.label(RichText::new(project.label()).color(theme.text));
                    ui.allocate_space(vec2(ui.available_width(), 0.0));
                });
                let pick = ui.interact(
                    row.response.rect,
                    Id::unique(("sue-paint-new-project-row", project.label())),
                    Sense::click(),
                );
                if new.project == project || pick.hovered() {
                    ui.painter().rect_filled(
                        row.response.rect,
                        2.0,
                        theme.selection.gamma_multiply(0.5),
                    );
                }
                if pick.clicked() {
                    new.choose(project);
                    ui.close();
                }
            }
        });
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
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
    ui.horizontal(|ui| {
        right_label(ui, &tr!("file-name"), 72.0, theme);
        ui.add(egui::TextEdit::singleline(&mut new.name).desired_width(260.0));
    });
    ui.add_space(6.0);
    // Preset, with Register and Delete, and Unit at the right.
    ui.horizontal(|ui| {
        right_label(ui, &tr!("preset"), 72.0, theme);
        let shown = new.preset.clone().unwrap_or_else(|| tr!("custom"));
        let mut picked = None;
        egui::ComboBox::from_id_salt("sue-paint-preset")
            .selected_text(shown)
            .width(190.0)
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
        for (icon, tip) in [
            (Icon::Import, tr!("register-preset")),
            (Icon::Delete, tr!("delete-preset")),
        ] {
            let (rect, _) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::hover());
            crate::panels::icon_button(
                ui,
                rect,
                icon,
                false,
                false,
                &tr!("not-available-yet", tip = tip),
                theme,
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            egui::ComboBox::from_id_salt("sue-paint-unit")
                .selected_text(new.unit.label())
                .width(50.0)
                .show_ui(ui, |ui| {
                    for unit in Unit::ALL {
                        ui.selectable_value(&mut new.unit, unit, unit.label());
                    }
                });
            ui.label(
                RichText::new(format!("{}:", tr!("unit").trim_end_matches(':'))).color(theme.text),
            );
        });
    });
    // Canvas: turn it on its side, its size and resolution, and a preview.
    section(ui, &tr!("canvas"), theme);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(26.0, 40.0), Sense::hover());
            if crate::panels::icon_button(
                ui,
                rect,
                Icon::Swap,
                true,
                false,
                &tr!("swap-width-and-height"),
                theme,
            ) {
                std::mem::swap(&mut new.width, &mut new.height);
                new.size = 0;
            }
        });
        ui.vertical(|ui| {
            let mut picked = None;
            ui.horizontal(|ui| {
                right_label(ui, &tr!("width"), column, theme);
                if length_field(ui, &mut new.width, new.unit, new.dpi) {
                    new.size = 0;
                }
                egui::ComboBox::from_id_salt("sue-paint-size")
                    .selected_text(crate::localize(SIZES[new.size].0))
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for (index, (name, ..)) in SIZES.iter().enumerate().skip(1) {
                            if ui
                                .selectable_label(new.size == index, crate::localize(name))
                                .clicked()
                            {
                                picked = Some(index);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("height"), column, theme);
                if length_field(ui, &mut new.height, new.unit, new.dpi) {
                    new.size = 0;
                }
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("resolution"), column, theme);
                ui.add(
                    egui::DragValue::new(&mut new.dpi)
                        .range(1.0..=2400.0)
                        .max_decimals(0),
                );
                egui::ComboBox::from_id_salt("sue-paint-dpi")
                    .selected_text("")
                    .width(24.0)
                    .show_ui(ui, |ui| {
                        for dpi in [72.0, 150.0, 300.0, 350.0, 600.0, 1200.0] {
                            if ui
                                .selectable_label(new.dpi == dpi, format!("{dpi:.0}"))
                                .clicked()
                            {
                                new.dpi = dpi;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                right_label(ui, &tr!("basic-expression-color"), column + 48.0, theme);
                egui::ComboBox::from_id_salt("sue-paint-color-mode")
                    .selected_text(tr!("color-2"))
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, tr!("color-2"));
                        ui.add_enabled(false, egui::Button::selectable(false, tr!("gray")));
                        ui.add_enabled(false, egui::Button::selectable(false, tr!("monochrome")));
                    });
            });
            if let Some(index) = picked {
                let (_, width, height, dpi) = SIZES[index];
                new.width = width;
                new.height = height;
                new.dpi = dpi;
                new.size = index;
                new.preset = None;
            }
        });
        // A preview of the canvas's shape and paper.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(80.0, 100.0), Sense::hover());
            let aspect = new.width.max(1) as f32 / new.height.max(1) as f32;
            let shape = if aspect >= 0.8 {
                vec2(66.0, 66.0 / aspect)
            } else {
                vec2(88.0 * aspect, 88.0)
            };
            let page = Rect::from_center_size(rect.center(), shape);
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
    });
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.checkbox(&mut new.paper, tr!("paper-color"));
        ui.add_space(24.0);
        ui.add_enabled_ui(new.paper, |ui| {
            let [r, g, b] = new.paper_color;
            let mut rgba = [r, g, b, 255];
            if sue_wm::color_button(ui, "sue-paint-new-paper", &mut rgba, false, theme).changed {
                new.paper_color = [rgba[0], rgba[1], rgba[2]];
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        ui.add_enabled(false, egui::Checkbox::new(&mut false, tr!("template")));
    });
    ui.add_enabled_ui(false, |ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut false, "");
            section(ui, &tr!("color-profile"), theme);
        });
        ui.horizontal(|ui| {
            right_label(ui, &tr!("working-profile"), 150.0, theme);
            ui.label("sRGB");
        });
    });
    // The project's own settings.
    if new.project.is_comic() {
        section(ui, &tr!("comic-settings"), theme);
        ui.horizontal(|ui| {
            right_label(ui, &tr!("pages"), column, theme);
            ui.add(egui::DragValue::new(&mut new.pages).range(1..=500));
        });
        ui.horizontal(|ui| {
            right_label(ui, &tr!("binding"), column, theme);
            ui.selectable_value(&mut new.right_bound, true, tr!("right-manga-right-to-left"));
            ui.selectable_value(&mut new.right_bound, false, tr!("left-left-to-right"));
        });
    } else if new.project == Project::Animation {
        section(ui, &tr!("animation-settings"), theme);
        ui.horizontal(|ui| {
            right_label(ui, &tr!("frames-2"), column, theme);
            ui.add(egui::DragValue::new(&mut new.frames).range(1..=10_000));
            right_label(ui, &tr!("frame-rate-2"), column, theme);
            ui.add(
                egui::DragValue::new(&mut new.fps)
                    .range(1..=120)
                    .suffix(tr!("fps")),
            );
        });
    } else {
        ui.add_enabled_ui(false, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut false, "");
                section(ui, &tr!("create-animated-illustration"), theme);
            });
        });
        egui::Frame::new()
            .fill(theme.pill)
            .corner_radius(2.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    icons::paint(ui.painter(), rect, Icon::Info, theme.text);
                    ui.label(
                        RichText::new(tr!("animated-illustration-hint"))
                            .size(theme.small_size)
                            .color(theme.text),
                    );
                });
            });
    }
    ui.label(
        RichText::new(tr!(
            "x-px-mb-per-layer",
            width = new.width,
            height = new.height,
            value = sue_lang::number(
                f64::from(new.width) * f64::from(new.height) * 4.0 / 1_048_576.0,
                1
            )
        ))
        .size(theme.small_size)
        .color(theme.muted),
    );
    ui.horizontal(|ui| {
        right_label(ui, &tr!("record-timelapse"), 110.0, theme);
        egui::ComboBox::from_id_salt("sue-paint-timelapse")
            .selected_text(if new.timelapse { tr!("on") } else { tr!("off") })
            .width(180.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut new.timelapse, false, tr!("off"));
                ui.selectable_value(&mut new.timelapse, true, tr!("on"));
            });
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

fn filter_ui(ui: &mut Ui, dialog: &mut FilterDialog, theme: &Theme) -> Outcome {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pen_curve_is_a_little_soft() {
        let curve: Response = DEFAULT_PEN_PRESSURE.parse().unwrap();
        assert!((curve.eval(0.0)).abs() < 1e-3);
        assert!((curve.eval(1.0) - 1.0).abs() < 1e-3);
        let usual = curve.eval(0.4);
        assert!(usual > 0.42 && usual < 0.6, "softer than straight: {usual}");
        // Rising all the way.
        let mut last = 0.0;
        for step in 1..=20 {
            let value = curve.eval(step as f32 / 20.0);
            assert!(value >= last);
            last = value;
        }
    }

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
