//! The Filter menu's commands, and the tonal corrections of Edit.

use sue_core::{Arena, UndoStack};
use sue_wm::{AppletContext, Call, Command, Facts, ItemState, World};

use super::{Basis, when};
use crate::dialogs::Dialog;
use crate::filters::Filter;
use crate::studio::SuePaint;

/// Does what a Filter command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_filter(call.name, cx);
}

/// How an item for a Filter command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_filter(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Filter command looks right now.
    fn state_filter(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis {
            has_doc, raster, ..
        } = self.basis(arena);
        match command {
            command if command.starts_with("filter.") => when(raster),
            _ => when(has_doc),
        }
    }

    /// Does what a Filter command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_filter(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "filter.invert" => self.filter_now(cx, Filter::Invert),
            "filter.blur" => self.filter_now(cx, Filter::GaussianBlur { radius: 1.0 }),
            "filter.blur_strong" => self.filter_now(cx, Filter::GaussianBlur { radius: 3.0 }),
            "filter.sharpen" => self.filter_now(cx, Filter::Sharpen { amount: 0.8 }),
            "filter.sharpen_strong" => self.filter_now(cx, Filter::Sharpen { amount: 2.0 }),
            "filter.brightness" => self.filter_dialog(
                cx,
                Filter::BrightnessContrast {
                    brightness: 0.0,
                    contrast: 0.0,
                },
            ),
            "filter.hue" => self.filter_dialog(
                cx,
                Filter::HueSaturation {
                    hue: 0.0,
                    saturation: 0.0,
                    luminosity: 0.0,
                },
            ),
            "filter.posterize" => self.filter_dialog(cx, Filter::Posterize { levels: 4 }),
            "filter.binarize" => self.filter_dialog(cx, Filter::Binarize { threshold: 0.5 }),
            "filter.gaussian" => self.filter_dialog(cx, Filter::GaussianBlur { radius: 4.0 }),
            "filter.mosaic" => self.filter_dialog(cx, Filter::Mosaic { size: 12 }),
            "filter.line_extraction" => self.filter_dialog(
                cx,
                Filter::LineExtraction {
                    sensitivity: 0.5,
                    black: 0.12,
                    smooth: 1.0,
                },
            ),
            "filter.lines_tones" => self.lines_and_tones(cx),
            "filter.motion_blur" => self.filter_dialog(
                cx,
                Filter::MotionBlur {
                    angle: 0.0,
                    distance: 20.0,
                },
            ),
            "filter.radial_blur" => self.filter_dialog(cx, Filter::RadialBlur { angle: 10.0 }),
            "filter.zoom_blur" => self.filter_dialog(cx, Filter::ZoomBlur { amount: 0.3 }),
            "filter.smooth" => self.filter_now(cx, Filter::Smooth),
            "filter.dust" => self.filter_dialog(cx, Filter::Dust { size: 10 }),
            "filter.line_width" => self.filter_dialog(cx, Filter::LineWidth { by: 1.0 }),
            "filter.wave" => self.filter_dialog(
                cx,
                Filter::Wave {
                    amplitude: 8.0,
                    wavelength: 80.0,
                },
            ),
            "filter.pinch" => self.filter_dialog(cx, Filter::Pinch { amount: 0.5 }),
            "filter.polar" => self.filter_dialog(cx, Filter::Polar { inverse: false }),
            "filter.ripple" => self.filter_dialog(
                cx,
                Filter::Ripple {
                    amplitude: 6.0,
                    wavelength: 40.0,
                },
            ),
            "filter.fisheye" => self.filter_dialog(cx, Filter::Fisheye { amount: 0.6 }),
            "filter.twirl" => self.filter_dialog(cx, Filter::Twirl { angle: 180.0 }),
            "filter.zigzag" => self.filter_dialog(
                cx,
                Filter::ZigZag {
                    amplitude: 10.0,
                    folds: 5.0,
                },
            ),
            "filter.crystallize" => self.filter_dialog(cx, Filter::Crystallize { size: 20 }),
            "filter.lens_blur" => self.filter_dialog(cx, Filter::LensBlur { radius: 8 }),
            "filter.quick.layer"
            | "filter.quick.all"
            | "filter.effects.layer"
            | "filter.effects.all" => {
                let all = command.ends_with(".all");
                if all {
                    // Apply to all: a merged layer of everything shown.
                    self.run("layer.merge_visible", cx);
                }
                self.filter_dialog(
                    cx,
                    Filter::Quick {
                        effect: command.starts_with("filter.effects"),
                        look: 0,
                        strength: 1.0,
                    },
                );
                if all && let Some(Dialog::Filter(dialog)) = self.dialog.as_mut() {
                    dialog.merged = true;
                }
            }
            "filter.jpeg_noise" => self.filter_dialog(cx, Filter::JpegNoise { strength: 0.6 }),
            "filter.aberration" => self.filter_dialog(cx, Filter::Aberration { shift: 6.0 }),
            "filter.glow" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::Glow {
                        radius: 16.0,
                        strength: 1.0,
                        rgb,
                    },
                );
            }
            "filter.drop_shadow" => self.filter_dialog(
                cx,
                Filter::DropShadow {
                    dx: 10.0,
                    dy: 10.0,
                    blur: 8.0,
                    opacity: 0.6,
                    rgb: [0, 0, 0],
                },
            ),
            "filter.inflate" => self.filter_dialog(
                cx,
                Filter::Inflate {
                    size: 12.0,
                    depth: 0.6,
                },
            ),
            "filter.retro" => self.filter_dialog(cx, Filter::Retro { amount: 0.6 }),
            "filter.perlin" => self.filter_dialog(
                cx,
                Filter::PerlinNoise {
                    scale: 60.0,
                    contrast: 1.0,
                },
            ),
            "filter.clouds" => self.filter_now(
                cx,
                Filter::Clouds {
                    from: self.main,
                    to: self.sub,
                },
            ),
            "filter.unsharp" => self.filter_dialog(
                cx,
                Filter::Unsharp {
                    radius: 3.0,
                    amount: 1.0,
                    threshold: 0.0,
                },
            ),
            "filter.noise" => self.filter_dialog(cx, Filter::Noise { amount: 0.2 }),
            "filter.curve" => self.filter_dialog(
                cx,
                Filter::ToneCurve {
                    rgb: sue_model::Response::LINEAR,
                    red: sue_model::Response::LINEAR,
                    green: sue_model::Response::LINEAR,
                    blue: sue_model::Response::LINEAR,
                    channel: 0,
                },
            ),
            "filter.levels" => self.filter_dialog(
                cx,
                Filter::Levels {
                    black: 0.0,
                    white: 1.0,
                    gamma: 1.0,
                    out_black: 0.0,
                    out_white: 1.0,
                },
            ),
            "filter.balance" => self.filter_dialog(
                cx,
                Filter::ColorBalance {
                    shifts: [[0.0; 3]; 3],
                    range: 1,
                },
            ),
            "filter.gradient_map" => {
                let (from, to) = (self.main, self.sub);
                self.filter_dialog(cx, Filter::GradientMap { from, to });
            }
            "filter.focus_lines" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::FocusLines {
                        count: 160,
                        gap: 0.45,
                        width: 5.0,
                        jitter: 0.4,
                        seed: 1,
                        rgb,
                    },
                );
            }
            "filter.speed_lines" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::SpeedLines {
                        count: 90,
                        angle: 0.0,
                        width: 4.0,
                        jitter: 0.5,
                        seed: 1,
                        rgb,
                    },
                );
            }
            "filter.tone" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::Tone {
                        density: 0.2,
                        cell: 8.0,
                        angle: 45.0,
                        rgb,
                    },
                );
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "filter", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("filter.brightness", "brightness-contrast-ellipsis"),
    c("filter.levels", "level-correction-ellipsis"),
    c("filter.curve", "tone-curve-ellipsis"),
    c("filter.hue", "hue-saturation-luminosity-ellipsis").keys("Ctrl+U"),
    c("filter.balance", "color-balance-ellipsis"),
    c("filter.invert", "reverse-gradient").keys("Ctrl+I"),
    c("filter.posterize", "posterization-ellipsis"),
    c("filter.binarize", "binarization-ellipsis"),
    c("filter.gradient_map", "gradient-map-ellipsis"),
    c("filter.lines_tones", "convert-to-lines-and-tones-ellipsis"),
    c("filter.quick.layer", "apply-to-current-layer-ellipsis"),
    c("filter.quick.all", "apply-to-all-merged-ellipsis"),
    c("filter.effects.layer", "apply-to-current-layer-ellipsis"),
    c("filter.effects.all", "apply-to-all-merged-ellipsis"),
    c("filter.blur", "blur"),
    c("filter.blur_strong", "blur-strong"),
    c("filter.gaussian", "gaussian-blur-ellipsis"),
    c("filter.motion_blur", "motion-blur-ellipsis"),
    c("filter.radial_blur", "radial-blur-ellipsis"),
    c("filter.zoom_blur", "zoom-blur-ellipsis"),
    c("filter.lens_blur", "lens-blur-ellipsis"),
    c("filter.smooth", "smooth"),
    c("filter.dust", "remove-dust-ellipsis"),
    c("filter.jpeg_noise", "remove-jpeg-noise-ellipsis"),
    c("filter.line_width", "line-width-ellipsis"),
    c("filter.wave", "wave-ellipsis"),
    c("filter.pinch", "pinch-ellipsis"),
    c("filter.polar", "polar-coordinates-ellipsis"),
    c("filter.ripple", "ripple-ellipsis"),
    c("filter.fisheye", "fisheye-ellipsis"),
    c("filter.twirl", "twirl-ellipsis"),
    c("filter.zigzag", "zigzag-ellipsis"),
    c("filter.mosaic", "mosaic-ellipsis"),
    c("filter.crystallize", "crystallize-ellipsis"),
    c("filter.noise", "add-noise-ellipsis"),
    c("filter.line_extraction", "line-extraction-ellipsis"),
    c("filter.aberration", "chromatic-aberration-ellipsis"),
    c("filter.glow", "glow-ellipsis"),
    c("filter.inflate", "inflate-ellipsis"),
    c("filter.drop_shadow", "drop-shadow-ellipsis"),
    c("filter.retro", "retro-ellipsis"),
    c("filter.focus_lines", "focus-lines-ellipsis"),
    c("filter.speed_lines", "speed-lines-ellipsis"),
    c("filter.tone", "screentone-ellipsis"),
    c("filter.perlin", "perlin-noise-ellipsis"),
    c("filter.clouds", "clouds"),
    c("filter.sharpen", "sharpen"),
    c("filter.sharpen_strong", "sharpen-strong"),
    c("filter.unsharp", "unsharp-mask-ellipsis"),
];
