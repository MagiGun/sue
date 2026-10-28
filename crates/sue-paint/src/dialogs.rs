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

/// The New dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewDialog {
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

/// An open dialog.
#[derive(Clone, Debug)]
pub(crate) enum Dialog {
    New(NewDialog),
    Filter(FilterDialog),
    Shortcuts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Open,
    Cancel,
    Ok,
}

/// Every shortcut, for the Keyboard Shortcuts dialog.
const SHORTCUTS: [(&str, &str); 30] = [
    ("New", "Ctrl+N"),
    ("Open", "Ctrl+O"),
    ("Save project", "Ctrl+S"),
    ("Close canvas", "Ctrl+W"),
    ("Undo / Redo", "Ctrl+Z / Ctrl+Y"),
    ("Cut / Copy / Paste", "Ctrl+X / C / V"),
    ("Clear / Fill", "Del / Alt+Del"),
    ("Reverse gradient", "Ctrl+I"),
    ("New raster layer", "Ctrl+Shift+N"),
    ("Merge with layer below", "Ctrl+E"),
    ("Move layer up / down", "Ctrl+] / Ctrl+["),
    ("Select layer above / below", "Alt+] / Alt+["),
    ("Select all / Deselect", "Ctrl+A / Ctrl+D"),
    ("Reselect / Invert", "Ctrl+Shift+D / Ctrl+Shift+I"),
    ("Zoom in / out", "Ctrl++ / Ctrl+-, wheel"),
    ("Fit / 100%", "Ctrl+0 / Ctrl+Alt+0"),
    ("Rotate view", "- and =, or Hand: Rotate"),
    ("Scroll", "Space+drag, middle drag"),
    ("Pick a color while painting", "Alt+click"),
    ("Brush size", "[ and ]"),
    ("Swap main and sub color", "X"),
    ("Pen / Pencil", "P"),
    ("Brush / Airbrush", "B"),
    ("Eraser", "E"),
    ("Fill / Gradient", "G"),
    ("Selection / Auto select", "M / W"),
    ("Eyedropper", "I"),
    ("Hand / Move layer", "H / K"),
    ("Figure", "U"),
    ("Command palette", "Tab"),
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
            Dialog::Filter(filter) => (filter.filter.label(), vec2(400.0, 250.0)),
            Dialog::Shortcuts => ("Keyboard Shortcuts", vec2(470.0, 560.0)),
        };
        let size = size.min(screen.size() - vec2(16.0, 16.0));
        let at = screen.center() - size / 2.0;
        let mut outcome = Outcome::Open;
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
                            Dialog::Shortcuts => {
                                egui::ScrollArea::vertical().max_height(size.y - 90.0).show(
                                    ui,
                                    |ui| {
                                        egui::Grid::new("sue-paint-shortcuts")
                                            .num_columns(2)
                                            .spacing(vec2(24.0, 6.0))
                                            .show(ui, |ui| {
                                                for (what, keys) in SHORTCUTS {
                                                    ui.label(RichText::new(what).color(theme.text));
                                                    ui.label(
                                                        RichText::new(keys).color(theme.muted),
                                                    );
                                                    ui.end_row();
                                                }
                                            });
                                    },
                                );
                                ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                                    if ui.button("Close").clicked() {
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
        if escape {
            outcome = Outcome::Cancel;
        } else if enter && !ctx.text_edit_focused() && !matches!(dialog, Dialog::Shortcuts) {
            outcome = Outcome::Ok;
        }
        match (dialog, outcome) {
            (dialog, Outcome::Open) => {
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
            (Dialog::New(new), Outcome::Ok) => {
                let spec = new.spec();
                self.make_picture(cx, &spec);
            }
            (Dialog::New(_) | Dialog::Shortcuts, Outcome::Cancel | Outcome::Ok) => {}
        }
    }
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
            (Icon::Paper, "Illustration", true),
            (Icon::Frame, "Webtoon", false),
            (Icon::Duplicate, "Comic", false),
            (Icon::Open, "Fanzine print", false),
            (Icon::NewFolder, "Show all comic settings", false),
            (Icon::Play, "Animation", false),
        ];
        for (icon, tip, enabled) in kinds {
            let (rect, response) = ui.allocate_exact_size(vec2(38.0, 34.0), Sense::click());
            let fill = if enabled { theme.active } else { theme.pill };
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
            egui::color_picker::color_edit_button_srgb(ui, &mut new.paper_color);
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
fn filter_ui(ui: &mut Ui, dialog: &mut FilterDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.spacing_mut().slider_width = 220.0;
    match &mut dialog.filter {
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
        Filter::Invert | Filter::Recolor { .. } | Filter::BrightnessToOpacity { .. } => {}
    }
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
