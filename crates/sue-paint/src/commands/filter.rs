//! The Filter menu's commands, and the tonal corrections of Edit.
//!
//! Every entry is one command with its own `run`: a filter that needs
//! no setting runs at once, the others open their dialog with Clip
//! Studio Paint's starting values. All of them work on the current
//! raster layer and are grayed without one.
//!
//! Auto Actions record every one of them ([`Flags::RECORDABLE`], as the
//! old `recordable()` kept every `filter.` name), and none leaves the
//! canvas, so none carries [`Flags::LEAVES_CANVAS`].

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::when;
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::studio::SuePaint;

/// What a command of this group does.
type Run = fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>);

/// A filter works while the current layer is a raster layer.
fn on_raster(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.basis(facts.arena).raster)
}

/// A command of this group, drawn with the icon `icon`.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: Run,
) -> Command<SuePaint> {
    Command::new(name, label, "filter", run)
        .state(on_raster)
        .icon(icon)
        .with(Flags::RECORDABLE)
}

/// Quick Filter or Quick Effect (`effect`), on the current layer or on a
/// new layer of everything shown merged (`all`).
fn quick(paint: &mut SuePaint, cx: &mut AppletContext<'_>, effect: bool, all: bool) {
    if all {
        // Apply to all: a merged layer of everything shown.
        paint.run("layer.merge_visible", cx);
    }
    paint.filter_dialog(
        cx,
        Filter::Quick {
            effect,
            look: 0,
            strength: 1.0,
        },
    );
    if all && let Some(Dialog::Filter(dialog)) = paint.dialog.as_mut() {
        dialog.merged = true;
    }
}

/// Edit, Tonal Correction, Grayscale: the current layer, and every
/// other unlocked raster layer picked with it, in grays of the same
/// brightness, alpha kept, as one step. With a selection only the
/// selected area turns gray.
fn grayscale(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    // Brightness picks a color from black to white: the gray of the
    // same brightness.
    const GRAY: Filter = Filter::GradientMap {
        from: [0, 0, 0],
        to: [255, 255, 255],
    };
    let label = tr!("grayscale");
    let current = match paint.paintable(&cx.world.arena) {
        Ok(info) => info,
        Err(error) => {
            cx.fail(error);
            return;
        }
    };
    let arena = &cx.world.arena;
    let selection = paint.doc().and_then(|doc| doc.selection.clone());
    let picked = paint
        .doc()
        .map(|doc| doc.picked.clone())
        .unwrap_or_default();
    let others: Vec<doc::LayerInfo> = paint
        .picture()
        .map(|picture| doc::layers(arena, picture))
        .unwrap_or_default()
        .into_iter()
        .filter(|info| {
            info.id != current.id
                && picked.contains(&info.id)
                && info.kind == sue_model::picture::LayerKind::Raster
                && !info.locked
                && !doc::is_folder(arena, info.id)
        })
        .collect();
    let mut edit = doc::Edit::new(label.clone());
    let mut changed = false;
    for info in std::iter::once(current).chain(others) {
        let offset = info.style.offset;
        let made = doc::rewrite(arena, info.id, &label, |pixels, width, height| {
            GRAY.apply(pixels, width, height, selection.as_ref(), offset);
        });
        match made {
            Ok(Some(step)) => {
                edit = edit.then(step);
                changed = true;
            }
            Ok(None) => {}
            Err(error) => {
                cx.fail(error);
                return;
            }
        }
    }
    if changed {
        SuePaint::apply(cx, edit);
    } else {
        cx.notify(tr!("nothing-changed", label = label));
    }
}

/// The group, in the order of Edit, Tonal Correction and of the Filter
/// menu.
pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    // Edit, Tonal Correction.
    c(
        "filter.brightness",
        "brightness-contrast-ellipsis",
        "sun",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::BrightnessContrast {
                    brightness: 0.0,
                    contrast: 0.0,
                },
            );
        },
    ),
    c(
        "filter.levels",
        "level-correction-ellipsis",
        "palette-color-sliders",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::Levels {
                    black: 0.0,
                    white: 1.0,
                    gamma: 1.0,
                    out_black: 0.0,
                    out_white: 1.0,
                },
            );
        },
    ),
    c(
        "filter.curve",
        "tone-curve-ellipsis",
        "pressure-curve",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::ToneCurve {
                    rgb: sue_model::Response::LINEAR,
                    red: sue_model::Response::LINEAR,
                    green: sue_model::Response::LINEAR,
                    blue: sue_model::Response::LINEAR,
                    channel: 0,
                },
            );
        },
    ),
    c(
        "filter.hue",
        "hue-saturation-luminosity-ellipsis",
        "palette-color-wheel",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::HueSaturation {
                    hue: 0.0,
                    saturation: 0.0,
                    luminosity: 0.0,
                },
            );
        },
    )
    .keys("Ctrl+U"),
    c(
        "filter.balance",
        "color-balance-ellipsis",
        "palette-color-sliders",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::ColorBalance {
                    shifts: [[0.0; 3]; 3],
                    range: 1,
                },
            );
        },
    ),
    c(
        "filter.invert",
        "reverse-gradient",
        "reverse",
        |p, cx, _| {
            p.filter_now(cx, Filter::Invert);
        },
    )
    .keys("Ctrl+I"),
    c(
        "filter.posterize",
        "posterization-ellipsis",
        "palette-color-set",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Posterize { levels: 4 });
        },
    ),
    c(
        "filter.binarize",
        "binarization-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Binarize { threshold: 0.5 });
        },
    ),
    c(
        "filter.gradient_map",
        "gradient-map-ellipsis",
        "tool-gradient",
        |p, cx, _| {
            let (from, to) = (p.main, p.sub);
            p.filter_dialog(cx, Filter::GradientMap { from, to });
        },
    ),
    c("filter.grayscale", "grayscale", "filter", grayscale),
    // Convert to Lines and Tones (Layer and Filter).
    c(
        "filter.lines_tones",
        "convert-to-lines-and-tones-ellipsis",
        "extract-line",
        |p, cx, _| p.open_lines_and_tones(cx),
    ),
    // Quick Filter and Quick Effect.
    c(
        "filter.quick.layer",
        "apply-to-current-layer-ellipsis",
        "filter",
        |p, cx, _| {
            quick(p, cx, false, false);
        },
    ),
    c(
        "filter.quick.all",
        "apply-to-all-merged-ellipsis",
        "filter",
        |p, cx, _| {
            quick(p, cx, false, true);
        },
    ),
    c(
        "filter.effects.layer",
        "apply-to-current-layer-ellipsis",
        "filter",
        |p, cx, _| {
            quick(p, cx, true, false);
        },
    ),
    c(
        "filter.effects.all",
        "apply-to-all-merged-ellipsis",
        "filter",
        |p, cx, _| {
            quick(p, cx, true, true);
        },
    ),
    // Blur.
    c("filter.blur", "blur", "tool-blend", |p, cx, _| {
        p.filter_now(cx, Filter::GaussianBlur { radius: 1.0 });
    }),
    c(
        "filter.blur_strong",
        "blur-strong",
        "tool-blend",
        |p, cx, _| {
            p.filter_now(cx, Filter::GaussianBlur { radius: 3.0 });
        },
    ),
    c(
        "filter.gaussian",
        "gaussian-blur-ellipsis",
        "tool-blend",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::GaussianBlur { radius: 4.0 });
        },
    ),
    c(
        "filter.motion_blur",
        "motion-blur-ellipsis",
        "tool-blend",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::MotionBlur {
                    angle: 0.0,
                    distance: 20.0,
                },
            );
        },
    ),
    c(
        "filter.radial_blur",
        "radial-blur-ellipsis",
        "tool-blend",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::RadialBlur { angle: 10.0 });
        },
    ),
    c(
        "filter.zoom_blur",
        "zoom-blur-ellipsis",
        "tool-blend",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::ZoomBlur { amount: 0.3 });
        },
    ),
    c(
        "filter.lens_blur",
        "lens-blur-ellipsis",
        "tool-blend",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::LensBlur { radius: 8 });
        },
    ),
    c("filter.smooth", "smooth", "tool-blend", |p, cx, _| {
        p.filter_now(cx, Filter::Smooth);
    }),
    // Correction.
    c(
        "filter.dust",
        "remove-dust-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Dust { size: 10 });
        },
    ),
    c(
        "filter.jpeg_noise",
        "remove-jpeg-noise-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::JpegNoise { strength: 0.6 });
        },
    ),
    c(
        "filter.line_width",
        "line-width-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::LineWidth { by: 1.0 });
        },
    ),
    // Distort.
    c(
        "filter.wave",
        "wave-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::Wave {
                    amplitude: 8.0,
                    wavelength: 80.0,
                },
            );
        },
    ),
    c(
        "filter.pinch",
        "pinch-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Pinch { amount: 0.5 });
        },
    ),
    c(
        "filter.polar",
        "polar-coordinates-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Polar { inverse: false });
        },
    ),
    c(
        "filter.ripple",
        "ripple-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::Ripple {
                    amplitude: 6.0,
                    wavelength: 40.0,
                },
            );
        },
    ),
    c(
        "filter.fisheye",
        "fisheye-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Fisheye { amount: 0.6 });
        },
    ),
    c(
        "filter.twirl",
        "twirl-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Twirl { angle: 180.0 });
        },
    ),
    c(
        "filter.zigzag",
        "zigzag-ellipsis",
        "tool-liquify",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::ZigZag {
                    amplitude: 10.0,
                    folds: 5.0,
                },
            );
        },
    ),
    // Effect.
    c("filter.mosaic", "mosaic-ellipsis", "filter", |p, cx, _| {
        p.filter_dialog(cx, Filter::Mosaic { size: 12 });
    }),
    c(
        "filter.crystallize",
        "crystallize-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Crystallize { size: 20 });
        },
    ),
    c(
        "filter.noise",
        "add-noise-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Noise { amount: 0.2 });
        },
    ),
    c(
        "filter.line_extraction",
        "line-extraction-ellipsis",
        "extract-line",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::LineExtraction {
                    sensitivity: 0.5,
                    black: 0.12,
                    smooth: 1.0,
                },
            );
        },
    ),
    c(
        "filter.aberration",
        "chromatic-aberration-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(cx, Filter::Aberration { shift: 6.0 });
        },
    ),
    c("filter.glow", "glow-ellipsis", "sun", |p, cx, _| {
        let rgb = p.drawing_color();
        p.filter_dialog(
            cx,
            Filter::Glow {
                radius: 16.0,
                strength: 1.0,
                rgb,
            },
        );
    }),
    c(
        "filter.inflate",
        "inflate-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::Inflate {
                    size: 12.0,
                    depth: 0.6,
                },
            );
        },
    ),
    c(
        "filter.drop_shadow",
        "drop-shadow-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::DropShadow {
                    dx: 10.0,
                    dy: 10.0,
                    blur: 8.0,
                    opacity: 0.6,
                    rgb: [0, 0, 0],
                },
            );
        },
    ),
    c("filter.retro", "retro-ellipsis", "filter", |p, cx, _| {
        p.filter_dialog(cx, Filter::Retro { amount: 0.6 });
    }),
    c(
        "filter.focus_lines",
        "focus-lines-ellipsis",
        "filter",
        |p, cx, _| {
            let rgb = p.drawing_color();
            p.filter_dialog(
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
        },
    ),
    c(
        "filter.speed_lines",
        "speed-lines-ellipsis",
        "filter",
        |p, cx, _| {
            let rgb = p.drawing_color();
            p.filter_dialog(
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
        },
    ),
    c("filter.tone", "screentone-ellipsis", "tone", |p, cx, _| {
        let rgb = p.drawing_color();
        p.filter_dialog(
            cx,
            Filter::Tone {
                density: 0.2,
                cell: 8.0,
                angle: 45.0,
                rgb,
            },
        );
    }),
    // Render.
    c(
        "filter.perlin",
        "perlin-noise-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::PerlinNoise {
                    scale: 60.0,
                    contrast: 1.0,
                },
            );
        },
    ),
    c("filter.clouds", "clouds", "filter", |p, cx, _| {
        let (from, to) = (p.main, p.sub);
        p.filter_now(cx, Filter::Clouds { from, to });
    }),
    // Sharpen.
    c("filter.sharpen", "sharpen", "filter", |p, cx, _| {
        p.filter_now(cx, Filter::Sharpen { amount: 0.8 });
    }),
    c(
        "filter.sharpen_strong",
        "sharpen-strong",
        "filter",
        |p, cx, _| {
            p.filter_now(cx, Filter::Sharpen { amount: 2.0 });
        },
    ),
    c(
        "filter.unsharp",
        "unsharp-mask-ellipsis",
        "filter",
        |p, cx, _| {
            p.filter_dialog(
                cx,
                Filter::Unsharp {
                    radius: 3.0,
                    amount: 1.0,
                    threshold: 0.0,
                },
            );
        },
    ),
];

#[cfg(test)]
mod tests {
    use super::COMMANDS;
    use sue_wm::Flags;

    #[test]
    fn every_filter_command_is_whole() {
        for command in COMMANDS {
            let name = command.name;
            assert!(name.starts_with("filter."), "{name}");
            assert_eq!(command.group, "filter", "{name}");
            assert!(!command.is_family(), "{name}");
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{name}: no icon is called {:?}",
                command.icon
            );
            assert!(
                sue_lang::has("paint", command.label),
                "{name}: {:?} has no English message",
                command.label
            );
        }
    }

    #[test]
    fn auto_actions_record_every_filter_and_none_leaves_the_canvas() {
        // The group's RECORDABLE list is all of it (the old recordable()
        // kept every `filter.` name), its LEAVES_CANVAS list is empty.
        let recordable: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.flags.has(Flags::RECORDABLE))
            .map(|command| command.name)
            .collect();
        let all: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        assert_eq!(recordable, all);
        assert!(
            COMMANDS
                .iter()
                .all(|command| !command.flags.has(Flags::LEAVES_CANVAS)
                    && !command.flags.has(Flags::HIDDEN))
        );
    }

    #[test]
    fn the_group_keeps_its_names_and_keys() {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        assert_eq!(names.len(), 50);
        let keyed: Vec<(&str, &str)> = COMMANDS
            .iter()
            .filter(|command| !command.chords.is_empty())
            .map(|command| (command.name, command.chords))
            .collect();
        assert_eq!(
            keyed,
            [("filter.hue", "Ctrl+U"), ("filter.invert", "Ctrl+I")]
        );
    }
}
