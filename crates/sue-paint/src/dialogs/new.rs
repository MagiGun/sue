//! The New dialog: a canvas from presets, paper sizes and units, the
//! canvas presets someone registers, and each kind of work's own
//! settings (a comic's pages and borders, an animation's output frame
//! and timeline, a moving illustration's frames).

use egui::Ui;
use sue_core::txn::SetProperty;
use sue_core::{Color, EntityId, Preset, PresetLibrary, Properties, Value};
use sue_wm::icons::Icon;
use sue_wm::ui::{FormGrid, Space, dropdown, form_grid, icon_button, segmented, text_field};
use sue_wm::{AppletContext, Theme};

use super::canvas::anchor_ui;
use super::fill::{Reads, check, number};
use super::{Outcome, bare, label, note, tip};
use crate::anim::setup::{OutputFrame, Overflow, Settings};
use crate::doc::{self, Edit, NewPicture};
use crate::studio::SuePaint;

/// A length unit of the New dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Px,
    Cm,
    Mm,
    In,
    /// Points, 72 to the inch, as Clip Studio Paint's New offers them.
    Pt,
}

impl Unit {
    const ALL: [Self; 5] = [Self::Px, Self::Cm, Self::Mm, Self::In, Self::Pt];

    fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::In => "in",
            Self::Pt => "pt",
        }
    }

    /// Units per inch (pixels per inch for `Px` come from the resolution).
    fn per_inch(self, dpi: f64) -> f64 {
        match self {
            Self::Px => dpi,
            Self::Cm => 2.54,
            Self::Mm => 25.4,
            Self::In => 1.0,
            Self::Pt => 72.0,
        }
    }

    /// The unit named `label`.
    fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.label() == label)
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

/// One of the animation presets of New: an output frame, its resolution,
/// the blank space round it and the frame rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnimationPreset {
    pub size: [u32; 2],
    pub dpi: u32,
    /// Blank space round the frame: a tenth of each side, or none.
    pub blank: bool,
    pub fps: u32,
}

impl AnimationPreset {
    /// What the list shows.
    pub(crate) fn name(&self) -> String {
        let [width, height] = self.size;
        if self.blank {
            tr!(
                "animation-preset-blank",
                width = width.to_string(),
                height = height.to_string(),
                dpi = self.dpi.to_string()
            )
        } else {
            tr!(
                "animation-preset-plain",
                width = width.to_string(),
                height = height.to_string(),
                dpi = self.dpi.to_string()
            )
        }
    }

    /// The blank space round the frame: top, bottom, left, right.
    pub(crate) fn blank_space(&self) -> [u32; 4] {
        if self.blank {
            let [width, height] = self.size;
            [height / 10, height / 10, width / 10, width / 10]
        } else {
            [0; 4]
        }
    }
}

/// The animation presets of New, as Clip Studio Paint lists them: three
/// screen sizes with blank space round the frame for camera work, and
/// two for phones and posts without.
pub(crate) fn animation_presets() -> [AnimationPreset; 5] {
    let preset = |width, height, dpi, blank, fps| AnimationPreset {
        size: [width, height],
        dpi,
        blank,
        fps,
    };
    [
        preset(720, 540, 144, true, 24),
        preset(1280, 720, 144, true, 24),
        preset(1920, 1080, 192, true, 24),
        preset(1080, 1920, 72, false, 30),
        preset(1080, 1080, 72, false, 30),
    ]
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
            Self::Illustration => Icon::DocIllustration,
            Self::Webtoon => Icon::DocWebtoon,
            Self::Comic => Icon::DocComic,
            Self::Fanzine => Icon::DocPrintBook,
            Self::ComicAll => Icon::DocComicSettings,
            Self::Animation => Icon::DocAnimation,
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

    /// The name a preset keeps it under.
    fn key(self) -> &'static str {
        match self {
            Self::Illustration => "illustration",
            Self::Webtoon => "webtoon",
            Self::Comic => "comic",
            Self::Fanzine => "fanzine",
            Self::ComicAll => "comic_all",
            Self::Animation => "animation",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|project| project.key() == key)
    }
}

/// The basic expression color of a new work: what its layers show as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Expression {
    #[default]
    Color,
    Gray,
    Monochrome,
}

impl Expression {
    pub(crate) const ALL: [Self; 3] = [Self::Color, Self::Gray, Self::Monochrome];

    /// Its index in `crate::effects::EXPRESSIONS`, as layers store it.
    pub(crate) fn index(self) -> u8 {
        match self {
            Self::Color => 0,
            Self::Gray => 1,
            Self::Monochrome => 2,
        }
    }

    fn from_index(index: u8) -> Self {
        Self::ALL
            .into_iter()
            .find(|expression| expression.index() == index)
            .unwrap_or_default()
    }

    fn label(self) -> String {
        match self {
            Self::Color => tr!("color-2"),
            Self::Gray => tr!("gray"),
            Self::Monochrome => tr!("monochrome"),
        }
    }
}

/// How the length of an animation is shown and typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TimeUnit {
    /// A count of frames, the first numbered 1.
    #[default]
    Frames,
    /// Seconds and the frames past the last whole second.
    SecondsFrames,
}

/// The picture's property that holds its basic expression color
/// (`Int`, an index of `crate::effects::EXPRESSIONS`): layers made in it
/// later start with it.
pub(crate) const BASIC_EXPRESSION: &str = "paint.expression";
/// Whether a monochrome work draws black (`Bool`).
pub(crate) const MONO_BLACK: &str = "paint.mono.black";
/// Whether a monochrome work draws white (`Bool`).
pub(crate) const MONO_WHITE: &str = "paint.mono.white";
/// The tone frequency a monochrome work's tones start with, in lines an
/// inch (`Float`).
pub(crate) const TONE_FREQUENCY: &str = "paint.tone.frequency";

/// The New dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewDialog {
    pub project: Project,
    /// Pages of a comic.
    pub pages: u32,
    /// A comic read right to left (manga).
    pub right_bound: bool,
    /// Frames of an animation (its playback time).
    pub frames: u32,
    /// Frames a second of an animation.
    pub fps: u32,
    pub name: String,
    /// The canvas, or for an animation the output frame.
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
    /// Presets made with Register preset.
    pub own: Vec<CanvasPreset>,
    /// Register (true) or Delete (false) the preset shown, once the
    /// dialog can keep it.
    pub preset_action: Option<bool>,
    /// The basic expression color.
    pub expression: Expression,
    /// A monochrome work draws black, white, or both.
    pub mono_black: bool,
    pub mono_white: bool,
    /// The tone frequency of a monochrome work, in lines an inch.
    pub frequency: f64,
    /// An illustration that moves: it gets a timeline.
    pub moving: bool,
    /// A comic's bleed and inner border, in millimeters.
    pub bleed: f64,
    pub inner: f64,
    /// A comic's author and first page number (Show all comic settings).
    pub author: String,
    pub first_page: u32,
    /// An animation's blank space round the output frame: top, bottom,
    /// left, right, in pixels.
    pub blank: [u32; 4],
    /// Show the title-safe area.
    pub title_safe: bool,
    /// The overflow frame, when on: its size and the point of it the
    /// output frame keeps to.
    pub overflow: Option<([u32; 2], u8)>,
    /// How far the output frame is moved from that point, in pixels:
    /// right and down.
    pub overflow_offset: [i32; 2],
    /// The timeline's name, scene, shot and division line.
    pub timeline: String,
    pub scene: u32,
    pub shot: u32,
    pub division: u32,
    pub time_unit: TimeUnit,
}

/// A canvas preset someone registered in the New dialog: its size,
/// resolution and paper, and what else the dialog held when it was
/// registered (the kind of work, the unit, the expression color, an
/// animation's frame and timeline, a comic's pages).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasPreset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub paper: bool,
    /// The rest, as `key=value` pairs in the order they were kept.
    pub more: Vec<(String, String)>,
}

impl CanvasPreset {
    /// One line: name, width, height, dpi and paper, tab separated, then
    /// a `key=value` field for each of the rest. A line from an older
    /// Sue has the first five only and reads the same.
    pub(crate) fn to_line(&self) -> String {
        let mut line = format!(
            "{}\t{}\t{}\t{}\t{}",
            self.name.replace(['\t', '\n'], " "),
            self.width,
            self.height,
            self.dpi,
            u8::from(self.paper)
        );
        for (key, value) in &self.more {
            line.push('\t');
            line.push_str(&format!(
                "{}={}",
                key.replace(['\t', '\n', '='], " "),
                value.replace(['\t', '\n'], " ")
            ));
        }
        line.push('\n');
        line
    }

    /// A preset read back from [`Self::to_line`].
    pub(crate) fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.split('\t');
        let name = parts.next()?.trim().to_owned();
        let width = parts.next()?.trim().parse().ok()?;
        let height = parts.next()?.trim().parse().ok()?;
        let dpi = parts.next()?.trim().parse().ok()?;
        let paper = parts.next()?.trim() == "1";
        let more = parts
            .filter_map(|part| {
                let (key, value) = part.split_once('=')?;
                Some((key.trim().to_owned(), value.trim().to_owned()))
            })
            .collect();
        (!name.is_empty()).then_some(Self {
            name,
            width,
            height,
            dpi,
            paper,
            more,
        })
    }

    /// As a backbone preset for the New dialog: every field a value.
    pub(crate) fn preset(&self) -> Preset {
        let mut values = Properties::new()
            .with("width", i64::from(self.width))
            .with("height", i64::from(self.height))
            .with("dpi", self.dpi)
            .with("paper", self.paper);
        for (key, value) in &self.more {
            values = values.with(key.as_str(), value.clone());
        }
        Preset::new(self.name.clone(), NEW_PRESET_TARGET, values)
    }
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
        if project == Project::Animation {
            // The 1920 x 1080 preset with blank space, as an animation
            // starts in Clip Studio Paint, five seconds long.
            self.apply_animation(&animation_presets()[2]);
            self.frames = self.fps * 5;
        } else {
            self.blank = [0; 4];
            self.overflow = None;
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
            own: Vec::new(),
            preset_action: None,
            expression: Expression::Color,
            mono_black: true,
            mono_white: true,
            frequency: 60.0,
            moving: false,
            bleed: 3.0,
            inner: 10.0,
            author: String::new(),
            first_page: 1,
            blank: [0; 4],
            title_safe: false,
            overflow: None,
            overflow_offset: [0, 0],
            timeline: tr!("timeline-1"),
            scene: 1,
            shot: 1,
            division: 6,
            time_unit: TimeUnit::Frames,
        }
    }

    /// The size of the canvas: an animation's output frame with its blank
    /// space (or its overflow frame), else the size typed.
    pub(crate) fn canvas_size(&self) -> [u32; 2] {
        match self.output_frame() {
            Some(frame) => frame.canvas_size(),
            None => [self.width, self.height],
        }
    }

    /// An animation's output frame, as its timeline keeps it.
    pub(crate) fn output_frame(&self) -> Option<OutputFrame> {
        (self.project == Project::Animation).then(|| OutputFrame {
            size: [self.width, self.height],
            blank: self.blank,
            title_safe: self.title_safe,
            overflow: self.overflow.map(|(size, anchor)| Overflow {
                size,
                anchor,
                offset: self.overflow_offset,
            }),
        })
    }

    pub(crate) fn spec(&self) -> NewPicture {
        let [width, height] = self.canvas_size();
        NewPicture {
            name: if self.name.trim().is_empty() {
                tr!("illustration")
            } else {
                self.name.trim().to_owned()
            },
            width: width.clamp(1, doc::MAX_SIDE),
            height: height.clamp(1, doc::MAX_SIDE),
            dpi: self.dpi,
            paper: self.paper.then(|| {
                let [r, g, b] = self.paper_color;
                Color::srgb8(r, g, b)
            }),
            timelapse: self.timelapse,
        }
    }

    /// Picks an animation preset: its frame, resolution, blank space and
    /// frame rate.
    pub(crate) fn apply_animation(&mut self, preset: &AnimationPreset) {
        [self.width, self.height] = preset.size;
        self.dpi = f64::from(preset.dpi);
        self.blank = preset.blank_space();
        self.fps = preset.fps;
        self.overflow = None;
        self.preset = Some(preset.name());
        self.size = 0;
    }

    pub(crate) fn apply_preset(&mut self, preset: &Preset) {
        let int = |key| {
            preset
                .values
                .get(key)
                .and_then(Value::as_i64)
                .and_then(|value| u32::try_from(value).ok())
        };
        let text = |key| preset.values.get(key).and_then(Value::as_str);
        // A preset kept with its kind of work brings it back first, so
        // the rest lands on the right fields.
        if let Some(project) = text("project").and_then(Project::from_key) {
            self.choose(project);
        }
        if let Some(width) = int("width") {
            self.width = width;
        }
        if let Some(height) = int("height") {
            self.height = height;
        }
        if let Some(dpi) = preset.values.f64("dpi") {
            self.dpi = dpi;
        }
        if let Some(paper) = preset.values.get("paper").and_then(Value::as_bool) {
            self.paper = paper;
        }
        let number = |key| text(key).and_then(|value| value.parse::<u32>().ok());
        if let Some(unit) = text("unit").and_then(Unit::from_label) {
            self.unit = unit;
        }
        if let Some(expression) = number("expression") {
            self.expression = Expression::from_index(expression.min(2) as u8);
        }
        if let Some(fps) = number("fps") {
            self.fps = fps.clamp(1, 120);
        }
        if let Some(frames) = number("frames") {
            self.frames = frames.max(1);
        }
        if let Some(pages) = number("pages") {
            self.pages = pages.max(1);
        }
        if let Some(blank) = text("blank") {
            let values: Vec<u32> = blank.split(',').filter_map(|v| v.parse().ok()).collect();
            if let [top, bottom, left, right] = values.as_slice() {
                self.blank = [*top, *bottom, *left, *right];
            }
        }
        if let Some(moving) = text("moving") {
            self.moving = moving == "1";
        }
        if let Some(color) = text("paper_color")
            .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
        {
            let [_, r, g, b] = color.to_be_bytes();
            self.paper_color = [r, g, b];
        }
        if let Some(timelapse) = text("timelapse") {
            self.timelapse = timelapse == "1";
        }
        self.preset = Some(preset.name.clone());
        self.size = 0;
    }

    /// The preset Register preset keeps: the size, resolution and paper,
    /// and the rest of what the dialog holds now.
    pub(crate) fn to_preset(&self, name: String) -> CanvasPreset {
        let mut more = vec![
            ("project".to_owned(), self.project.key().to_owned()),
            ("unit".to_owned(), self.unit.label().to_owned()),
            ("expression".to_owned(), self.expression.index().to_string()),
        ];
        if self.project == Project::Animation || self.moving {
            more.push(("fps".to_owned(), self.fps.to_string()));
            more.push(("frames".to_owned(), self.frames.to_string()));
        }
        if self.project == Project::Animation {
            let [top, bottom, left, right] = self.blank;
            more.push(("blank".to_owned(), format!("{top},{bottom},{left},{right}")));
        }
        if self.moving {
            more.push(("moving".to_owned(), "1".to_owned()));
        }
        if self.project.is_comic() {
            more.push(("pages".to_owned(), self.pages.to_string()));
        }
        // Tip 45: the paper color and Record timelapse are kept too.
        let [r, g, b] = self.paper_color;
        more.push(("paper_color".to_owned(), format!("#{r:02x}{g:02x}{b:02x}")));
        more.push(("timelapse".to_owned(), u8::from(self.timelapse).to_string()));
        CanvasPreset {
            name,
            width: self.width,
            height: self.height,
            dpi: self.dpi,
            paper: self.paper,
            more,
        }
    }
}

impl SuePaint {
    /// Register preset (or, with `register` false, Delete preset) in the
    /// New dialog: what the dialog holds kept by name for next time.
    pub(super) fn canvas_preset(
        &mut self,
        cx: &mut AppletContext<'_>,
        new: &mut NewDialog,
        register: bool,
    ) {
        if register {
            let taken = |name: &str| {
                self.canvas_presets.iter().any(|preset| preset.name == name)
                    || new_presets().get(NEW_PRESET_TARGET, name).is_some()
            };
            let name = (1..)
                .map(|number| {
                    tr!(
                        "canvas-preset-name",
                        width = new.width.to_string(),
                        height = new.height.to_string(),
                        number = number
                    )
                })
                .find(|name| !taken(name))
                .unwrap_or_default();
            self.canvas_presets.push(new.to_preset(name.clone()));
            new.preset = Some(name.clone());
            cx.notify(tr!("canvas-preset-registered", name = name));
        } else if let Some(name) = new.preset.take() {
            self.canvas_presets.retain(|preset| preset.name != name);
        }
        new.own.clone_from(&self.canvas_presets);
        self.save_extras(cx);
    }
}

/// A length in pixels, edited in `unit`. Returns `true` if it changed.
fn length_field(
    ui: &mut Ui,
    salt: &str,
    pixels: &mut u32,
    unit: Unit,
    dpi: f64,
    name: &str,
    theme: &Theme,
) -> bool {
    let per_inch = unit.per_inch(dpi);
    let suffix = format!(" {}", unit.label());
    let mut value = if unit == Unit::Px {
        f64::from(*pixels)
    } else {
        f64::from(*pixels) / dpi.max(1.0) * per_inch
    };
    let reads = Reads {
        range: 0.0..=f64::from(doc::MAX_SIDE),
        decimals: if unit == Unit::Px { 0 } else { 2 },
        suffix: &suffix,
    };
    if !number(ui, salt, &mut value, &reads, name, theme) {
        return false;
    }
    let typed = if unit == Unit::Px {
        value
    } else {
        value / per_inch * dpi
    };
    *pixels = (typed.round() as u32).clamp(1, doc::MAX_SIDE);
    true
}

/// A whole number in a form row. Returns whether it changed.
fn count_field(
    ui: &mut Ui,
    salt: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
    name: &str,
    theme: &Theme,
) -> bool {
    let mut typed = f64::from(*value);
    let reads = Reads {
        range: f64::from(*range.start())..=f64::from(*range.end()),
        decimals: 0,
        suffix,
    };
    if number(ui, salt, &mut typed, &reads, name, theme) {
        *value = typed.round() as u32;
        return true;
    }
    false
}

/// The Project row: one button per kind of work, the chosen one held.
fn project_row(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space(Space::S1);
        for project in Project::ALL {
            let name = project.label();
            let response = icon_button(
                ("sue-paint-new-project", project.key()),
                project.icon(),
                &name,
            )
            .selected(new.project == project)
            .show(ui, theme);
            if response.clicked() {
                new.choose(project);
            }
        }
        // The kind of work chosen, in words: the icons alone do not say.
        ui.add_space(theme.space(Space::S2));
        label(ui, &new.project.label(), theme);
    });
}

/// The New dialog's body, laid out as Clip Studio Paint's: the Project
/// row, the file name and preset, the Canvas, then the project's own
/// settings and Record timelapse at the bottom. OK, Cancel and Register
/// preset are in the footer.
#[allow(clippy::too_many_lines, reason = "the whole New dialog")]
pub(super) fn new_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) -> Outcome {
    let animation = new.project == Project::Animation;
    form_grid(ui, "sue-paint-new-top", theme, |form| {
        form.row(&bare(tr!("project")), |ui| project_row(ui, new, theme));
        let name = bare(tr!("file-name"));
        form.row(&name, |ui| {
            let response = text_field("sue-paint-new-name", &mut new.name)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        form.row(&bare(tr!("preset")), |ui| preset_row(ui, new, theme));
    });
    if animation {
        ui.columns(2, |columns| {
            output_frame_ui(&mut columns[0], new, theme);
            timeline_ui(&mut columns[1], new, theme);
        });
    } else {
        // One form for the canvas and the project's own settings, so
        // their controls stand in one column.
        form_grid(ui, "sue-paint-new-canvas", theme, |form| {
            canvas_ui(form, new, theme);
            if new.project.is_comic() {
                comic_ui(form, new, theme);
            } else {
                moving_ui(form, new, theme);
            }
        });
    }
    let [width, height] = new.canvas_size();
    note(
        ui,
        &tr!(
            "x-px-mb-per-layer",
            width = width,
            height = height,
            value = sue_lang::number(f64::from(width) * f64::from(height) * 4.0 / 1_048_576.0, 1)
        ),
        theme,
    );
    form_grid(ui, "sue-paint-new-timelapse", theme, |form| {
        form.wide(|ui| {
            let name = bare(tr!("record-timelapse"));
            check(
                ui,
                "sue-paint-new-timelapse",
                &mut new.timelapse,
                &name,
                "",
                theme,
            );
        });
    });
    Outcome::Open
}

/// The Preset list (with the animation presets when making an
/// animation), Delete preset, and the Unit.
fn preset_row(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) {
    let animation = new.project == Project::Animation;
    let mut presets = new_presets();
    for own in &new.own {
        presets.add(own.preset());
    }
    let mut names = vec![tr!("custom")];
    let anims = if animation {
        animation_presets().to_vec()
    } else {
        Vec::new()
    };
    names.extend(anims.iter().map(AnimationPreset::name));
    let library: Vec<Preset> = presets.for_target(NEW_PRESET_TARGET).cloned().collect();
    names.extend(library.iter().map(|preset| preset.name.clone()));
    let mut chosen = new
        .preset
        .as_ref()
        .and_then(|shown| names.iter().position(|name| name == shown))
        .unwrap_or(0);
    let name = bare(tr!("preset"));
    let response = dropdown("sue-paint-preset", &mut chosen, &names)
        .label(&name)
        .show(ui, theme);
    if tip(response, &name, "", theme).changed() {
        if chosen == 0 {
            new.preset = None;
        } else if let Some(preset) = anims.get(chosen - 1) {
            new.apply_animation(preset);
        } else if let Some(preset) = library.get(chosen - 1 - anims.len()) {
            new.apply_preset(preset);
        }
    }
    // Delete takes away a preset registered here (Register preset is in
    // the footer).
    let own = new
        .preset
        .as_ref()
        .is_some_and(|name| new.own.iter().any(|preset| preset.name == *name));
    let delete = if own {
        tr!("delete-preset")
    } else {
        tr!("delete-preset-own-only")
    };
    let response = icon_button("sue-paint-new-delete-preset", Icon::Delete, &delete)
        .enabled(own)
        .show(ui, theme);
    if response.clicked() {
        new.preset_action = Some(false);
    }
    let units: Vec<&str> = Unit::ALL.iter().map(|unit| unit.label()).collect();
    let mut unit = Unit::ALL
        .iter()
        .position(|unit| *unit == new.unit)
        .unwrap_or(0);
    let name = bare(tr!("unit"));
    let response = dropdown("sue-paint-unit", &mut unit, &units)
        .label(&name)
        .show(ui, theme);
    if tip(response, &name, "", theme).changed() {
        new.unit = Unit::ALL[unit];
    }
}

/// The Canvas group of a picture or a comic: width and height with the
/// paper sizes, resolution, expression color, paper color.
fn canvas_ui(form: &mut FormGrid<'_, '_>, new: &mut NewDialog, theme: &Theme) {
    form.section(&tr!("canvas"));
    size_rows(form, new, theme);
    let name = bare(tr!("basic-expression-color"));
    form.row(&name, |ui| {
        let colors: Vec<String> = Expression::ALL.iter().map(|e| e.label()).collect();
        let chosen = Expression::ALL
            .iter()
            .position(|expression| *expression == new.expression);
        let strip = segmented("sue-paint-color-mode", &colors, chosen).show(ui, theme);
        tip(strip.response, &name, "", theme);
        if let Some(picked) = strip.picked {
            new.expression = Expression::ALL[picked];
        }
    });
    if new.expression == Expression::Monochrome {
        // Which inks a monochrome work draws, and its tones' lines an
        // inch.
        form.row(&tr!("mono-drawing-colors"), |ui| {
            let black = tr!("mono-black");
            check(
                ui,
                "sue-paint-new-mono-black",
                &mut new.mono_black,
                &black,
                "",
                theme,
            );
            let white = tr!("mono-white");
            check(
                ui,
                "sue-paint-new-mono-white",
                &mut new.mono_white,
                &white,
                "",
                theme,
            );
            if !new.mono_black && !new.mono_white {
                new.mono_black = true;
            }
        });
        let name = tr!("default-tone-frequency");
        form.row(&name, |ui| {
            let lpi = tr!("lpi");
            let reads = Reads {
                range: 5.0..=300.0,
                decimals: 1,
                suffix: &lpi,
            };
            number(
                ui,
                "sue-paint-new-frequency",
                &mut new.frequency,
                &reads,
                &name,
                theme,
            );
        });
    }
    form.wide(|ui| {
        let name = tr!("paper-color");
        check(ui, "sue-paint-new-paper", &mut new.paper, &name, "", theme);
        ui.add_enabled_ui(new.paper, |ui| {
            let [r, g, b] = new.paper_color;
            let mut rgba = [r, g, b, 255];
            if sue_wm::color_button(ui, "sue-paint-new-paper-color", &mut rgba, false, theme)
                .changed
            {
                new.paper_color = [rgba[0], rgba[1], rgba[2]];
            }
        });
    });
}

/// Width, height (with the paper sizes and the swap button) and
/// resolution, as rows of `form`.
fn size_rows(form: &mut sue_wm::ui::FormGrid<'_, '_>, new: &mut NewDialog, theme: &Theme) {
    let name = bare(tr!("width"));
    form.row(&name, |ui| {
        if length_field(
            ui,
            "sue-paint-new-width",
            &mut new.width,
            new.unit,
            new.dpi,
            &name,
            theme,
        ) {
            new.size = 0;
            new.preset = None;
        }
        let swap = tr!("swap-width-and-height");
        if icon_button("sue-paint-new-swap", Icon::Swap, &swap)
            .show(ui, theme)
            .clicked()
        {
            std::mem::swap(&mut new.width, &mut new.height);
            new.size = 0;
        }
        if new.project != Project::Animation {
            let sizes: Vec<String> = SIZES
                .iter()
                .map(|(name, ..)| crate::localize(name))
                .collect();
            let mut size = new.size;
            let label = tr!("paper-size");
            let response = dropdown("sue-paint-size", &mut size, &sizes)
                .label(&label)
                .show(ui, theme);
            if tip(response, &label, "", theme).changed() && size > 0 {
                let (_, width, height, dpi) = SIZES[size];
                new.width = width;
                new.height = height;
                new.dpi = dpi;
                new.size = size;
                new.preset = None;
            }
        }
    });
    let name = bare(tr!("height"));
    form.row(&name, |ui| {
        if length_field(
            ui,
            "sue-paint-new-height",
            &mut new.height,
            new.unit,
            new.dpi,
            &name,
            theme,
        ) {
            new.size = 0;
            new.preset = None;
        }
    });
    let name = bare(tr!("resolution"));
    form.row(&name, |ui| {
        let dpi = tr!("dpi");
        let reads = Reads {
            range: 1.0..=2400.0,
            decimals: 0,
            suffix: &dpi,
        };
        number(ui, "sue-paint-new-dpi", &mut new.dpi, &reads, &name, theme);
        // The common resolutions, with the one set now in its place among
        // them when it is not one of them (a preset's 192), so the list
        // always shows what the canvas gets.
        let mut common: Vec<f64> = vec![72.0, 150.0, 300.0, 350.0, 600.0, 1200.0];
        if !common.contains(&new.dpi) {
            common.push(new.dpi);
            common.sort_by(f64::total_cmp);
        }
        let names: Vec<String> = common.iter().map(|dpi| format!("{dpi}")).collect();
        let mut chosen = common
            .iter()
            .position(|value| *value == new.dpi)
            .unwrap_or_default();
        let label = tr!("common-resolutions");
        let response = dropdown("sue-paint-dpi", &mut chosen, &names)
            .label(&label)
            .show(ui, theme);
        if tip(response, &label, "", theme).changed()
            && let Some(value) = common.get(chosen)
        {
            new.dpi = *value;
        }
    });
}

/// A comic's settings: pages, binding, bleed and inner border, and with
/// Show all comic settings its author and first page number.
fn comic_ui(form: &mut FormGrid<'_, '_>, new: &mut NewDialog, theme: &Theme) {
    form.section(&tr!("comic-settings"));
    let name = bare(tr!("pages"));
    form.row(&name, |ui| {
        count_field(
            ui,
            "sue-paint-new-pages",
            &mut new.pages,
            1..=500,
            "",
            &name,
            theme,
        );
    });
    let name = bare(tr!("binding"));
    form.row(&name, |ui| {
        let sides = [tr!("right-manga-right-to-left"), tr!("left-left-to-right")];
        let chosen = usize::from(!new.right_bound);
        let strip = segmented("sue-paint-new-binding", &sides, Some(chosen)).show(ui, theme);
        tip(strip.response, &name, "", theme);
        if let Some(picked) = strip.picked {
            new.right_bound = picked == 0;
        }
    });
    let mm = tr!("mm");
    let reads = Reads {
        range: 0.0..=50.0,
        decimals: 1,
        suffix: &mm,
    };
    let name = tr!("bleed-width");
    form.row(&name, |ui| {
        number(
            ui,
            "sue-paint-new-bleed",
            &mut new.bleed,
            &reads,
            &name,
            theme,
        );
    });
    let name = bare(tr!("inner-border"));
    form.row(&name, |ui| {
        number(
            ui,
            "sue-paint-new-inner",
            &mut new.inner,
            &reads,
            &name,
            theme,
        );
    });
    if new.project == Project::ComicAll {
        form.section(&tr!("story-information"));
        let name = bare(tr!("author"));
        form.row(&name, |ui| {
            let response = text_field("sue-paint-new-author", &mut new.author)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        let name = bare(tr!("first-page-number"));
        form.row(&name, |ui| {
            count_field(
                ui,
                "sue-paint-new-first",
                &mut new.first_page,
                0..=9999,
                "",
                &name,
                theme,
            );
        });
    }
}

/// Create moving illustration: an illustration with a timeline, its
/// frame rate and playback time.
fn moving_ui(form: &mut FormGrid<'_, '_>, new: &mut NewDialog, theme: &Theme) {
    form.wide(|ui| {
        let name = tr!("create-animated-illustration");
        let help = tr!("animated-illustration-hint");
        check(
            ui,
            "sue-paint-new-moving",
            &mut new.moving,
            &name,
            &help,
            theme,
        );
    });
    if new.moving {
        rate_and_time(form, new, theme);
    } else {
        form.wide(|ui| note(ui, &tr!("animated-illustration-hint"), theme));
    }
}

/// Frame rate and playback time, as rows of `form`.
fn rate_and_time(form: &mut sue_wm::ui::FormGrid<'_, '_>, new: &mut NewDialog, theme: &Theme) {
    let fps = tr!("fps");
    let name = bare(tr!("frame-rate-2"));
    form.row(&name, |ui| {
        count_field(
            ui,
            "sue-paint-new-fps",
            &mut new.fps,
            1..=120,
            &fps,
            &name,
            theme,
        );
    });
    let name = bare(tr!("playback-time"));
    form.row(&name, |ui| {
        match new.time_unit {
            TimeUnit::Frames => {
                count_field(
                    ui,
                    "sue-paint-new-frames",
                    &mut new.frames,
                    1..=100_000,
                    "",
                    &name,
                    theme,
                );
            }
            TimeUnit::SecondsFrames => {
                let rate = new.fps.max(1);
                let (mut seconds, mut rest) = (new.frames / rate, new.frames % rate);
                let s = tr!("seconds-short");
                let f = tr!("frames-short");
                let changed = count_field(
                    ui,
                    "sue-paint-new-seconds",
                    &mut seconds,
                    0..=3600,
                    &s,
                    &name,
                    theme,
                ) | count_field(
                    ui,
                    "sue-paint-new-rest",
                    &mut rest,
                    0..=rate - 1,
                    &f,
                    &name,
                    theme,
                );
                if changed {
                    new.frames = (seconds * rate + rest).max(1);
                }
            }
        }
        let units = [tr!("time-unit-frames"), tr!("time-unit-seconds-frames")];
        let mut unit = usize::from(new.time_unit == TimeUnit::SecondsFrames);
        let label = tr!("playback-time-unit");
        let response = dropdown("sue-paint-new-time-unit", &mut unit, &units)
            .label(&label)
            .show(ui, theme);
        if tip(response, &label, "", theme).changed() {
            new.time_unit = if unit == 1 {
                TimeUnit::SecondsFrames
            } else {
                TimeUnit::Frames
            };
        }
    });
    form.wide(|ui| {
        note(ui, &playback_line(new.frames, new.fps), theme);
    });
}

/// The playback time as people read it: seconds and frames, and the
/// count of frames.
pub(crate) fn playback_line(frames: u32, fps: u32) -> String {
    let rate = fps.max(1);
    tr!(
        "playback-time-line",
        seconds = frames / rate,
        frames = frames % rate,
        total = frames
    )
}

/// An animation's left column: the output frame with its preview, the
/// title-safe area, the overflow frame and the blank space.
fn output_frame_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) {
    form_grid(ui, "sue-paint-new-output", theme, |form| {
        form.section(&tr!("size-of-output-frame"));
        size_rows(form, new, theme);
        form.wide(|ui| preview(ui, new, theme));
        form.wide(|ui| {
            let name = tr!("title-safe-area");
            let help = tr!("title-safe-area-hint");
            check(
                ui,
                "sue-paint-new-title-safe",
                &mut new.title_safe,
                &name,
                &help,
                theme,
            );
        });
        form.wide(|ui| {
            let name = tr!("overflow-frame");
            let help = tr!("overflow-frame-hint");
            let mut on = new.overflow.is_some();
            if check(ui, "sue-paint-new-overflow", &mut on, &name, &help, theme) {
                new.overflow = on.then(|| {
                    let [width, height] = new.canvas_size();
                    ([width, height], 4)
                });
            }
        });
        if let Some((size, anchor)) = &mut new.overflow {
            let px = tr!("px");
            for (index, name) in [tr!("overflow-width"), tr!("overflow-height")]
                .into_iter()
                .enumerate()
            {
                form.row(&name, |ui| {
                    count_field(
                        ui,
                        &format!("sue-paint-new-overflow-{index}"),
                        &mut size[index],
                        1..=doc::MAX_SIDE,
                        &px,
                        &name,
                        theme,
                    );
                });
            }
            form.row(&bare(tr!("reference-point")), |ui| {
                anchor_ui(ui, "sue-paint-new-overflow-anchor", anchor, theme);
            });
            // The output frame moved from the reference point, as Clip
            // Studio Paint's Offset X and Offset Y.
            let side = i64::from(doc::MAX_SIDE);
            for (index, name) in [tr!("offset-x"), tr!("offset-y")].into_iter().enumerate() {
                form.row(&name, |ui| {
                    let mut typed = f64::from(new.overflow_offset[index]);
                    let reads = Reads {
                        range: -(side as f64)..=side as f64,
                        decimals: 0,
                        suffix: &px,
                    };
                    let salt = format!("sue-paint-new-overflow-offset-{index}");
                    if number(ui, &salt, &mut typed, &reads, &name, theme) {
                        new.overflow_offset[index] = typed.round() as i32;
                    }
                });
            }
            // The output frame fits inside the overflow frame.
            size[0] = size[0].max(new.width);
            size[1] = size[1].max(new.height);
        } else {
            form.section(&tr!("blank-space"));
            let px = tr!("px");
            for (index, name) in [
                tr!("blank-top"),
                tr!("blank-bottom"),
                tr!("blank-left"),
                tr!("blank-right"),
            ]
            .into_iter()
            .enumerate()
            {
                form.row(&name, |ui| {
                    if count_field(
                        ui,
                        &format!("sue-paint-new-blank-{index}"),
                        &mut new.blank[index],
                        0..=doc::MAX_SIDE,
                        &px,
                        &name,
                        theme,
                    ) {
                        new.preset = None;
                    }
                });
            }
        }
    });
}

/// A small picture of the canvas: the paper, and in an animation the
/// output frame inside the blank space or the overflow frame.
fn preview(ui: &mut Ui, new: &NewDialog, theme: &Theme) {
    let [width, height] = new.canvas_size();
    let room = theme.space(Space::S9) * 3.0;
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let shape = if aspect >= 1.0 {
        egui::Vec2::new(room, room / aspect)
    } else {
        egui::Vec2::new(room * aspect, room)
    };
    let (rect, _) = ui.allocate_exact_size(shape, egui::Sense::hover());
    let [r, g, b] = new.paper_color;
    let fill = if new.paper {
        // The paper's own color: what the canvas will look like.
        egui::Color32::from_rgb(r, g, b)
    } else {
        theme.surface(2)
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        theme.hairline(),
        egui::StrokeKind::Outside,
    );
    if let Some(frame) = new.output_frame() {
        let [x, y, w, h] = frame.rect().map(|value| value as f32);
        let scale = rect.width() / width.max(1) as f32;
        let inner = egui::Rect::from_min_size(
            rect.min + egui::Vec2::new(x, y) * scale,
            egui::Vec2::new(w, h) * scale,
        );
        // What lies outside the output frame (the blank space or the
        // overflow frame) is dimmed, so the frame reads at a glance on
        // any paper.
        let painter = ui.painter();
        let outside = [
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, inner.min.y)),
            egui::Rect::from_min_max(egui::pos2(rect.min.x, inner.max.y), rect.max),
            egui::Rect::from_min_max(
                egui::pos2(rect.min.x, inner.min.y),
                egui::pos2(inner.min.x, inner.max.y),
            ),
            egui::Rect::from_min_max(
                egui::pos2(inner.max.x, inner.min.y),
                egui::pos2(rect.max.x, inner.max.y),
            ),
        ];
        for part in outside {
            if part.is_positive() {
                painter.rect_filled(part, egui::CornerRadius::ZERO, theme.scrim());
            }
        }
        let width = theme.selected_stroke().width;
        painter.rect_stroke(
            inner,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(width, theme.border_strong_on(fill)),
            egui::StrokeKind::Inside,
        );
        if new.title_safe {
            // The title-safe area: the middle 80 percent of the frame,
            // where words stay clear of any screen's edge.
            let safe = egui::Rect::from_center_size(inner.center(), inner.size() * TITLE_SAFE);
            painter.rect_stroke(
                safe,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(width, theme.text_muted_on(fill)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// How much of the output frame the title-safe area takes, per side.
const TITLE_SAFE: f32 = 0.8;

/// An animation's right column: the timeline's name, frame rate,
/// playback time, scene, shot and division line.
fn timeline_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) {
    form_grid(ui, "sue-paint-new-timeline", theme, |form| {
        form.section(&tr!("timeline"));
        let name = tr!("timeline-name");
        form.row(&name, |ui| {
            let response = text_field("sue-paint-new-timeline-name", &mut new.timeline)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        rate_and_time(form, new, theme);
        for (salt, name, value) in [
            ("sue-paint-new-scene", tr!("scene-number"), &mut new.scene),
            ("sue-paint-new-shot", tr!("shot-number"), &mut new.shot),
            (
                "sue-paint-new-division",
                tr!("division-line"),
                &mut new.division,
            ),
        ] {
            form.row(&name, |ui| {
                count_field(ui, salt, value, 1..=9999, "", &name, theme);
            });
        }
    });
}

impl SuePaint {
    /// Makes the new work, then gives it what the dialog set beyond its
    /// canvas (its expression color, a comic's borders, an animation's
    /// output frame and timeline) as one more undo step.
    pub(super) fn new_ok(&mut self, cx: &mut AppletContext<'_>, new: &NewDialog) {
        let spec = new.spec();
        let before = self.picture();
        match new.project {
            Project::Illustration | Project::Webtoon if new.moving => {
                self.make_animation(cx, &spec, new.frames, new.fps);
            }
            Project::Illustration | Project::Webtoon => self.make_picture(cx, &spec),
            Project::Comic | Project::Fanzine | Project::ComicAll => {
                self.make_story(cx, &spec, new.pages as usize, new.right_bound);
            }
            Project::Animation => self.make_animation(cx, &spec, new.frames, new.fps),
        }
        // Nothing more when the work could not be made.
        let Some(picture) = self.picture().filter(|now| Some(*now) != before) else {
            return;
        };
        if let Some(edit) = settings_edit(&cx.world.arena, picture, new)
            && !edit.is_empty()
        {
            Self::apply(cx, edit);
        }
    }

    /// The New dialog, and Register preset or Delete preset when it asks.
    pub(super) fn new_dialog_ui(
        &mut self,
        ui: &mut Ui,
        new: &mut NewDialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        new.own.clone_from(&self.canvas_presets);
        let outcome = new_ui(ui, new, theme);
        if let Some(register) = new.preset_action.take() {
            self.canvas_preset(cx, new, register);
        }
        outcome
    }
}

/// The edit that gives the new work made from `new` (`picture` is its
/// first page) what the dialog set beyond its canvas.
pub(crate) fn settings_edit(
    arena: &sue_core::Arena,
    picture: EntityId,
    new: &NewDialog,
) -> Option<Edit> {
    let mut edit = Edit::new(tr!("new-work-settings"));
    let pages: Vec<EntityId> = match crate::story::story_of(arena, picture) {
        Some(story) => crate::story::pages(arena, story),
        None => vec![picture],
    };
    if new.expression != Expression::Color {
        for page in &pages {
            edit = edit.then(SetProperty::new(
                *page,
                BASIC_EXPRESSION,
                i64::from(new.expression.index()),
            ));
            // The layer a new work starts with shows in the expression
            // color at once.
            for layer in doc::layers(arena, *page) {
                if layer.kind == sue_model::picture::LayerKind::Raster {
                    edit = edit.then(SetProperty::new(
                        layer.id,
                        crate::effects::EXPRESSION,
                        i64::from(new.expression.index()),
                    ));
                }
            }
            if new.expression == Expression::Monochrome {
                edit = edit
                    .then(SetProperty::new(*page, MONO_BLACK, new.mono_black))
                    .then(SetProperty::new(*page, MONO_WHITE, new.mono_white))
                    .then(SetProperty::new(*page, TONE_FREQUENCY, new.frequency));
            }
        }
    }
    if let Some(story) = crate::story::story_of(arena, picture) {
        for page in &pages {
            edit = edit
                .then(SetProperty::new(*page, crate::story::PAGE_BLEED, new.bleed))
                .then(SetProperty::new(*page, crate::story::PAGE_INNER, new.inner));
        }
        if new.project == Project::ComicAll {
            edit = edit
                .then(SetProperty::new(
                    story,
                    crate::story::AUTHOR,
                    new.author.trim().to_owned(),
                ))
                .then(SetProperty::new(
                    story,
                    crate::story::FIRST_NUMBER,
                    i64::from(new.first_page),
                ));
        }
    }
    if let Some(frame) = new.output_frame() {
        let settings = Settings {
            name: if new.timeline.trim().is_empty() {
                tr!("timeline-1")
            } else {
                new.timeline.trim().to_owned()
            },
            scene: new.scene,
            shot: new.shot,
            division: new.division.max(1),
            start: 0,
            end: None,
        };
        edit = edit
            .then(frame.apply(picture))
            .then(settings.apply(picture));
    }
    Some(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_preset_keeps_the_paper_color_and_timelapse() {
        let mut new = NewDialog::new(&NewPicture::default());
        new.paper_color = [250, 240, 200];
        new.timelapse = true;
        let kept = CanvasPreset::from_line(&new.to_preset("Warm".to_owned()).to_line()).unwrap();
        let mut back = NewDialog::new(&NewPicture::default());
        back.apply_preset(&kept.preset());
        assert_eq!(back.paper_color, [250, 240, 200]);
        assert!(back.timelapse);
    }

    #[test]
    fn points_are_a_unit_of_new_at_72_to_the_inch() {
        assert_eq!(Unit::from_label("pt"), Some(Unit::Pt));
        // An inch at 350 dpi is 350 pixels and 72 points.
        let pixels = 350.0;
        let points = pixels / 350.0 * Unit::Pt.per_inch(350.0);
        assert!((points - 72.0).abs() < 1e-9);
    }

    #[test]
    fn registered_canvas_presets_keep_as_lines() {
        let preset = CanvasPreset {
            name: "Poster	wide".to_owned(),
            width: 3000,
            height: 2000,
            dpi: 300.0,
            paper: false,
            more: Vec::new(),
        };
        let back = CanvasPreset::from_line(preset.to_line().trim_end()).unwrap();
        assert_eq!(back.name, "Poster wide");
        assert_eq!(
            (back.width, back.height, back.dpi, back.paper),
            (3000, 2000, 300.0, false)
        );
        let mut new = NewDialog::new(&NewPicture::default());
        new.apply_preset(&back.preset());
        assert_eq!((new.width, new.height, new.paper), (3000, 2000, false));
        assert_eq!(CanvasPreset::from_line("no tabs here"), None);
        // A line an older Sue wrote reads the same.
        let old = CanvasPreset::from_line("Old\t640\t480\t72\t1").unwrap();
        assert_eq!((old.width, old.more.len()), (640, 0));
    }

    #[test]
    fn a_preset_keeps_the_kind_of_work_and_its_settings() {
        let mut new = NewDialog::new(&NewPicture::default());
        new.choose(Project::Animation);
        new.unit = Unit::Mm;
        new.expression = Expression::Gray;
        new.fps = 12;
        new.frames = 48;
        let line = new.to_preset("Cut".to_owned()).to_line();
        let preset = CanvasPreset::from_line(line.trim_end()).unwrap().preset();
        let mut back = NewDialog::new(&NewPicture::default());
        back.apply_preset(&preset);
        assert_eq!(back.project, Project::Animation);
        assert_eq!(back.unit, Unit::Mm);
        assert_eq!(back.expression, Expression::Gray);
        assert_eq!((back.fps, back.frames), (12, 48));
        assert_eq!(back.blank, [108, 108, 192, 192]);
        assert_eq!(back.canvas_size(), [2304, 1296]);
    }

    #[test]
    fn the_animation_presets_carry_dpi_blank_space_and_frame_rate() {
        let presets = animation_presets();
        assert_eq!(presets.len(), 5);
        let mut new = NewDialog::new(&NewPicture::default());
        new.choose(Project::Animation);
        // The 1920 x 1080 one at 192 dpi gives a 2304 x 1296 canvas.
        new.apply_animation(&presets[2]);
        assert_eq!(new.dpi, 192.0);
        assert_eq!(new.blank, [108, 108, 192, 192]);
        assert_eq!(new.spec().width, 2304);
        assert_eq!(new.spec().height, 1296);
        assert_eq!(new.fps, 24);
        assert_eq!(new.preset.as_deref(), Some(presets[2].name().as_str()));
        // The square one has no blank space and plays at 30.
        new.apply_animation(&presets[4]);
        assert_eq!(new.canvas_size(), [1080, 1080]);
        assert_eq!(new.fps, 30);
        // An overflow frame sets the canvas instead.
        new.overflow = Some(([1500, 1200], 4));
        assert_eq!(new.canvas_size(), [1500, 1200]);
        assert_eq!(new.output_frame().unwrap().rect(), [210, 60, 1080, 1080]);
        // Offset Y moves the output frame down from its reference point.
        new.overflow_offset = [0, 8];
        assert_eq!(new.output_frame().unwrap().rect(), [210, 68, 1080, 1080]);
    }

    #[test]
    fn the_playback_time_reads_in_seconds_and_frames() {
        assert_eq!(playback_line(120, 24), "5 s 0 f (120 frames)");
        assert_eq!(playback_line(30, 24), "1 s 6 f (30 frames)");
    }
}
