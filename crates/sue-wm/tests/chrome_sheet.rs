//! A sheet of the chrome every surface shares, for a person to look at:
//! the title bar, pills, the segmented choice, the response curve, the
//! color wheel in its shapes with its popup, the command palette and the
//! font list, in every built-in theme and at two scales.
//!
//! It is not part of the test run. To write the pictures:
//!
//! ```text
//! SUE_CHROME_SHEET=<folder> cargo test -p sue-wm --test chrome_sheet -- --ignored
//! ```

use std::path::{Path, PathBuf};

use egui::{Event, Id, Pos2, Rect, Ui, pos2, vec2};
use sue_platform::{EguiPainter, GpuContext, wgpu};
use sue_rice::{RiceSchema, ThemePreset, builtin_presets};
use sue_wm::ui::Density;
use sue_wm::widgets::{self, CurveLabels};
use sue_wm::{
    Palette, PaletteEntry, SpringMode, SpringParams, Theme, WheelShape, color_button,
    set_wheel_shape,
};
use zune_core::bit_depth::BitDepth;
use zune_core::colorspace::ColorSpace;
use zune_core::options::EncoderOptions;

/// The size of the sheet, in points.
const SIZE: [f32; 2] = [1180.0, 900.0];

/// What the sheet keeps between frames.
struct Sheet {
    theme: Theme,
    curve: sue_model::Response,
    bent: sue_model::Response,
    colors: [[u8; 4]; 3],
    palette: Palette,
    entries: Vec<PaletteEntry>,
    /// Where the pointer rests, once the first frame has laid things out.
    hover: Option<Pos2>,
    /// The swatch button to press, once known.
    swatch: Option<Pos2>,
}

impl Sheet {
    fn new(theme: Theme) -> Self {
        let spring = SpringParams {
            stiffness: 260.0,
            damping: 32.0,
            mass: 1.0,
        };
        let mut palette = Palette::open(spring, SpringMode::InstantSnap);
        palette.step(1.0);
        palette.set_query("new");
        let entry = |label: &str, hint: &str| PaletteEntry {
            label: label.to_owned(),
            command: label.to_owned(),
            hint: hint.to_owned(),
        };
        Self {
            theme,
            curve: sue_model::Response::LINEAR,
            bent: sue_model::Response::new(&[[0.0, 0.0], [0.3, 0.1], [0.7, 0.85], [1.0, 1.0]]),
            colors: [[214, 72, 48, 255], [48, 120, 214, 255], [64, 160, 96, 128]],
            palette,
            entries: vec![
                entry("New Document", "Ctrl+N"),
                entry("New Raster Layer", "Ctrl+Shift+N"),
                entry("New Window", ""),
                entry("Open", "Ctrl+O"),
                entry("Renew the Thumbnails", ""),
            ],
            hover: None,
            swatch: None,
        }
    }

    fn label(ui: &mut Ui, text: &str, theme: &Theme) {
        ui.add_space(10.0);
        widgets::heading(ui, text, theme);
        ui.add_space(2.0);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let whole = ui.max_rect();
        ui.painter().rect_filled(whole, 0.0, theme.panel);
        let column = |index: usize| {
            let width = 372.0;
            Rect::from_min_size(
                pos2(16.0 + index as f32 * (width + 16.0), 12.0),
                vec2(width, whole.height() - 24.0),
            )
        };

        // Column 1: the window chrome and the pills.
        let mut first = ui.new_child(egui::UiBuilder::new().max_rect(column(0)));
        let ui1 = &mut first;
        Self::label(ui1, "Title bar, with and without maximize", &theme);
        for (index, maximize) in [true, false].into_iter().enumerate() {
            let (rect, _) = ui1
                .allocate_exact_size(vec2(372.0, widgets::TITLE_BAR + 40.0), egui::Sense::hover());
            ui1.painter().rect_filled(rect, 0.0, theme.panel);
            widgets::outline(ui1, rect, theme.hairline(), 0.0);
            let bar = Rect::from_min_size(rect.min, vec2(rect.width(), widgets::TITLE_BAR));
            ui1.push_id(("bar", index), |ui| {
                widgets::title_bar(ui, bar, "Illustration 12.sue - Sue Paint", maximize, &theme);
            });
            if index == 0 {
                self.hover
                    .get_or_insert(pos2(bar.right() - 20.0, bar.center().y));
            }
            ui1.add_space(6.0);
        }
        Self::label(ui1, "Pill buttons: plain, chosen", &theme);
        ui1.horizontal(|ui| {
            let _ = widgets::pill_button(ui, "Find", false, &theme);
            let _ = widgets::pill_button(ui, "Replace", true, &theme);
            let _ = widgets::pill_button(ui, "Import a Document...", false, &theme);
        });
        Self::label(ui1, "A pill with a divider", &theme);
        widgets::pill_frame(&theme).show(ui1, |ui| {
            ui.horizontal(|ui| {
                let _ = widgets::pill_button(ui, "Fit", false, &theme);
                let _ = widgets::pill_button(ui, "100%", true, &theme);
                widgets::pill_divider(ui, &theme);
                let _ = widgets::pill_button(ui, "Flip", false, &theme);
            });
        });
        Self::label(ui1, "Segmented choice", &theme);
        let options: Vec<String> = ["Scrivenings", "Corkboard", "Outliner"]
            .map(str::to_owned)
            .to_vec();
        let _ = widgets::segmented(ui1, &options, Some(1), &theme);
        Self::label(ui1, "A window frame", &theme);
        ui1.add_space(8.0);
        ui1.horizontal(|ui| {
            ui.add_space(12.0);
            widgets::window_frame(&theme)
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.label("Quit Sue?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let _ = widgets::pill_button(ui, "Quit", true, &theme);
                        let _ = widgets::pill_button(ui, "Cancel", false, &theme);
                    });
                });
        });
        // Below the rest, so the open popup covers no wheel.
        Self::label(ui1, "Swatch buttons; the last one open (HLS)", &theme);
        ui1.horizontal(|ui| {
            let _ = color_button(ui, "sheet-a", &mut self.colors[0], false, &theme);
            let _ = color_button(ui, "sheet-b", &mut self.colors[1], false, &theme);
            let rect = ui.available_rect_before_wrap();
            let _ = color_button(ui, "sheet-c", &mut self.colors[2], true, &theme);
            self.swatch
                .get_or_insert(pos2(rect.left() + 22.0, rect.top() + 11.0));
        });
        // Column 2: curves and colors.
        let mut second = ui.new_child(egui::UiBuilder::new().max_rect(column(1)));
        let ui2 = &mut second;
        Self::label(ui2, "Response curve: large, small", &theme);
        ui2.horizontal_top(|ui| {
            widgets::response_curve(
                ui,
                "large",
                &mut self.bent,
                170.0,
                CurveLabels::pressure(),
                &theme,
            );
            widgets::response_curve(
                ui,
                "small",
                &mut self.curve,
                110.0,
                CurveLabels::pressure(),
                &theme,
            );
        });
        Self::label(ui2, "Color wheel: HSV square, HLS triangle", &theme);
        ui2.horizontal_top(|ui| {
            set_wheel_shape(ui.ctx(), WheelShape::Square);
            let _ = sue_wm::color_wheel(ui, Id::unique("sheet-square"), &mut self.colors[0], 176.0);
            ui.add_space(8.0);
            set_wheel_shape(ui.ctx(), WheelShape::Triangle);
            let _ =
                sue_wm::color_wheel(ui, Id::unique("sheet-triangle"), &mut self.colors[1], 176.0);
        });
        Self::label(
            ui2,
            "Small wheels: gray, and too small for the button",
            &theme,
        );
        ui2.horizontal_top(|ui| {
            let mut gray = [128, 128, 128, 255];
            let _ = sue_wm::color_wheel(ui, Id::unique("sheet-shared"), &mut gray, 120.0);
            ui.add_space(8.0);
            let mut green = [40, 170, 90, 255];
            let _ = sue_wm::color_wheel(ui, Id::unique("sheet-tiny"), &mut green, 80.0);
        });

        // Column 3: the command palette.
        let third = column(2);
        let mut third_ui = ui.new_child(egui::UiBuilder::new().max_rect(third));
        Self::label(&mut third_ui, "Command palette", &theme);
        let area = Rect::from_min_size(third.min + vec2(0.0, 0.0), vec2(third.width(), 300.0));
        let _ = self.palette.ui(ui.ctx(), area, &self.entries, &theme);
        third_ui.add_space(300.0);
        Self::label(&mut third_ui, "Font list: compact, then regular", &theme);
        third_ui.horizontal_top(|ui| {
            for (index, density) in [Density::Compact, Density::Regular].into_iter().enumerate() {
                ui.push_id(("fonts", index), |ui| {
                    ui.allocate_ui_with_layout(
                        vec2(178.0, 200.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_height(200.0);
                            density.scope(ui, |ui| {
                                ui.set_width(178.0);
                                let _ = sue_wm::fonts::list_ui(ui, "Hack", "");
                            });
                        },
                    );
                });
            }
        });
    }
}

fn png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let options = EncoderOptions::new(
        width as usize,
        height as usize,
        ColorSpace::RGBA,
        BitDepth::Eight,
    );
    let mut out = Vec::new();
    zune_png::PngEncoder::new(pixels, options)
        .encode(&mut out)
        .expect("PNG encoder");
    out
}

fn shoot(gpu: &GpuContext, preset: &ThemePreset, scale: f32, path: &Path) {
    let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
    sue_wm::register_rice(&mut rice).unwrap();
    let theme = Theme::from_rice(&rice).with_preset(preset);
    let ctx = egui::Context::default();
    theme.apply(&ctx, None, None);
    let mut painter = EguiPainter::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut sheet = Sheet::new(theme.clone());
    let size = [
        (SIZE[0] * scale).round() as u32,
        (SIZE[1] * scale).round() as u32,
    ];
    let button = |pos: Pos2, pressed: bool| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let frames = 40;
    for frame in 0..=frames {
        // Press the swatch button to open its popup, then rest on the
        // title bar's close button.
        let events = match (frame, sheet.swatch, sheet.hover) {
            (3, Some(at), _) => vec![Event::PointerMoved(at)],
            (4, Some(at), _) => vec![button(at, true)],
            (5, Some(at), _) => vec![button(at, false)],
            (8, _, Some(at)) => vec![Event::PointerMoved(at)],
            _ => Vec::new(),
        };
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(SIZE[0], SIZE[1]))),
            time: Some(f64::from(frame) * 0.016),
            max_texture_side: Some(painter.max_texture_side()),
            events,
            ..egui::RawInput::default()
        };
        raw.viewports
            .entry(raw.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(scale);
        let output = ctx.run_ui(raw, |ui| sheet.ui(ui));
        if frame < frames {
            painter.apply_textures(output.textures_delta);
            // Give the font scan a moment in the first frames.
            if !sue_wm::fonts::ready() {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            continue;
        }
        let pixels = painter
            .paint_offscreen(
                &ctx,
                size,
                theme.void,
                output.shapes,
                output.textures_delta,
                output.pixels_per_point,
            )
            .expect("offscreen paint");
        std::fs::write(path, png(size[0], size[1], &pixels)).expect("write the sheet");
    }
}

/// Writes the sheets into the folder `SUE_CHROME_SHEET` names (or the
/// temporary folder), one per built-in theme, and the first also at twice
/// the scale.
#[test]
#[ignore = "writes pictures for a person to look at"]
fn chrome_sheet() {
    let dir = std::env::var_os("SUE_CHROME_SHEET").map_or_else(std::env::temp_dir, PathBuf::from);
    std::fs::create_dir_all(&dir).expect("make the folder");
    let gpu = GpuContext::headless().expect("a GPU or a software one");
    for (index, preset) in builtin_presets().iter().enumerate() {
        shoot(
            &gpu,
            preset,
            1.0,
            &dir.join(format!("chrome-{}.png", preset.id)),
        );
        if index == 0 {
            shoot(
                &gpu,
                preset,
                2.0,
                &dir.join(format!("chrome-{}-2x.png", preset.id)),
            );
        }
    }
}
