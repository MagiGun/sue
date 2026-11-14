//! The New dialog: a canvas from presets, paper sizes and units, and
//! the canvas presets someone registers.

use egui::{Align, Id, Layout, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_core::{Color, Preset, PresetLibrary, Properties};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, right_label, section};
use crate::doc::{self, NewPicture};
use crate::icons::{self, Icon};
use crate::studio::SuePaint;

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
    /// Presets made with Register preset: name, width, height, dpi,
    /// paper.
    pub own: Vec<CanvasPreset>,
    /// Register (true) or Delete (false) the preset shown, once the
    /// dialog can keep it.
    pub preset_action: Option<bool>,
}

/// A canvas preset someone registered in the New dialog.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasPreset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub paper: bool,
}

impl CanvasPreset {
    /// One line: name, width, height, dpi and paper, tab separated.
    pub(crate) fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\n",
            self.name.replace(['\t', '\n'], " "),
            self.width,
            self.height,
            self.dpi,
            u8::from(self.paper)
        )
    }

    /// A preset read back from [`Self::to_line`].
    pub(crate) fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.split('\t');
        let name = parts.next()?.trim().to_owned();
        let width = parts.next()?.trim().parse().ok()?;
        let height = parts.next()?.trim().parse().ok()?;
        let dpi = parts.next()?.trim().parse().ok()?;
        let paper = parts.next()?.trim() == "1";
        (!name.is_empty()).then_some(Self {
            name,
            width,
            height,
            dpi,
            paper,
        })
    }

    /// As a backbone preset for the New dialog.
    pub(crate) fn preset(&self) -> Preset {
        Preset::new(
            self.name.clone(),
            NEW_PRESET_TARGET,
            Properties::new()
                .with("width", i64::from(self.width))
                .with("height", i64::from(self.height))
                .with("dpi", self.dpi)
                .with("paper", self.paper),
        )
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

impl SuePaint {
    /// Register preset (or, with `register` false, Delete preset) in the
    /// New dialog: the canvas size, resolution and paper kept by name for
    /// next time.
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
                        width = new.width,
                        height = new.height,
                        number = number
                    )
                })
                .find(|name| !taken(name))
                .unwrap_or_default();
            self.canvas_presets.push(CanvasPreset {
                name: name.clone(),
                width: new.width,
                height: new.height,
                dpi: new.dpi,
                paper: new.paper,
            });
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

/// The New dialog's body, laid out as Clip Studio Paint's: the Project
/// row with OK and Cancel beside it, the file name and preset, the Canvas,
/// then the project's own settings and Record timelapse at the bottom.
#[allow(clippy::too_many_lines, reason = "the whole New dialog")]
pub(super) fn new_ui(ui: &mut Ui, new: &mut NewDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let mut presets = new_presets();
    for own in &new.own {
        presets.add(own.preset());
    }
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
        // Register keeps the size, resolution and paper as a preset;
        // Delete takes away one registered here.
        let own = new
            .preset
            .as_ref()
            .is_some_and(|name| new.own.iter().any(|preset| preset.name == *name));
        for (icon, tip, enabled, register) in [
            (Icon::Import, tr!("register-preset"), true, true),
            (
                Icon::Delete,
                if own {
                    tr!("delete-preset")
                } else {
                    tr!("delete-preset-own-only")
                },
                own,
                false,
            ),
        ] {
            let (rect, _) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::hover());
            if crate::panels::icon_button(ui, rect, icon, enabled, false, &tip, theme) {
                new.preset_action = Some(register);
            }
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

impl SuePaint {
    /// Makes the new work.
    pub(super) fn new_ok(&mut self, cx: &mut AppletContext<'_>, new: &NewDialog) {
        let spec = new.spec();
        match new.project {
            Project::Illustration | Project::Webtoon => self.make_picture(cx, &spec),
            Project::Comic | Project::Fanzine | Project::ComicAll => {
                self.make_story(cx, &spec, new.pages as usize, new.right_bound);
            }
            Project::Animation => self.make_animation(cx, &spec, new.frames, new.fps),
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
    fn registered_canvas_presets_keep_as_lines() {
        let preset = CanvasPreset {
            name: "Poster	wide".to_owned(),
            width: 3000,
            height: 2000,
            dpi: 300.0,
            paper: false,
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
    }
}
