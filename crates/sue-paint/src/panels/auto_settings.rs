//! The settings a step of an Auto Action keeps from the dialog it showed
//! while it was recorded: played without its dialog, the step uses them
//! (as OK in that dialog would have), and played with it, the dialog
//! opens at them.
//!
//! A step's settings are one line of text (`settings <text>` after the
//! step in `auto_actions.txt`, see `docs/paint-auto-action-files.md`):
//! a filter's (`filter:<name>:<values>`, or `filter:<correction text>`
//! for the tonal corrections, as correction layers keep them), a
//! selection's width (`select:<kind>,<pixels>`) or Select Color Gamut's
//! (`gamut:<tolerance>,<every layer>`). A Sue Paint that cannot read a
//! line plays the step with the dialog's own settings.

use crate::dialogs::Dialog;
use crate::filters::Filter;

/// The settings of `dialog` worth keeping in a step, if it has any.
pub(crate) fn of_dialog(dialog: &Dialog) -> Option<String> {
    match dialog {
        Dialog::Filter(filter) => Some(format!("filter:{}", filter_text(&filter.filter)?)),
        Dialog::Correction(_, filter, ..) => Some(format!("filter:{}", filter_text(filter)?)),
        Dialog::SelectBy(kind, amount) => Some(format!("select:{kind},{amount}")),
        Dialog::Gamut(tolerance, all) => Some(format!("gamut:{tolerance},{}", u8::from(*all))),
        _ => None,
    }
}

/// Sets `dialog` to the kept `settings`, when they are of its kind (a
/// filter only takes the settings of the same filter). Whether it took
/// them.
pub(crate) fn apply(dialog: &mut Dialog, settings: &str) -> bool {
    let Some((kind, text)) = settings.split_once(':') else {
        return false;
    };
    match (kind, dialog) {
        ("filter", Dialog::Filter(open)) => match filter_of(text) {
            Some(filter) if same_kind(&filter, &open.filter) => {
                open.filter = filter;
                true
            }
            _ => false,
        },
        ("filter", Dialog::Correction(_, open, ..)) => match filter_of(text) {
            Some(filter) if same_kind(&filter, open) => {
                *open = filter;
                true
            }
            _ => false,
        },
        ("select", Dialog::SelectBy(kind, amount)) => match numbers(text).as_slice() {
            [of, value] if f64::from(*kind) == *of => {
                *amount = *value as f32;
                true
            }
            _ => false,
        },
        ("gamut", Dialog::Gamut(tolerance, all)) => match numbers(text).as_slice() {
            [value, every] => {
                *tolerance = *value as f32;
                *all = *every != 0.0;
                true
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether two filters are the same filter (their settings aside); the
/// tonal corrections that share a dialog count as one.
fn same_kind(a: &Filter, b: &Filter) -> bool {
    let family = |filter: &Filter| match filter {
        Filter::Levels { .. } | Filter::ChannelLevels { .. } => 1,
        Filter::GradientMap { .. } | Filter::Gradient { .. } => 2,
        _ => 0,
    };
    std::mem::discriminant(a) == std::mem::discriminant(b)
        || (family(a) != 0 && family(a) == family(b))
}

fn numbers(text: &str) -> Vec<f64> {
    text.split(',')
        .map(|value| value.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
}

/// `filter` as text: a tonal correction as correction layers keep it,
/// any other filter as its name and its values.
fn filter_text(filter: &Filter) -> Option<String> {
    if let Some(text) = filter.to_text() {
        return Some(text);
    }
    let rgb = |[r, g, b]: [u8; 3]| [f64::from(r), f64::from(g), f64::from(b)];
    let f = f64::from;
    let n = |value: u32| f64::from(value);
    let b = |value: bool| f64::from(u8::from(value));
    let (name, values): (&str, Vec<f64>) = match *filter {
        Filter::GaussianBlur { radius } => ("gaussian", vec![f(radius)]),
        Filter::Sharpen { amount } => ("sharpen", vec![f(amount)]),
        Filter::Mosaic { size } => ("mosaic", vec![n(size)]),
        Filter::MotionBlur { angle, distance } => ("motion", vec![f(angle), f(distance)]),
        Filter::RadialBlur { angle, center } => {
            ("radial", vec![f(angle), f(center[0]), f(center[1])])
        }
        Filter::ZoomBlur { amount, center } => {
            ("zoom", vec![f(amount), f(center[0]), f(center[1])])
        }
        Filter::Smooth => ("smooth", Vec::new()),
        Filter::Dust { size } => ("dust", vec![n(size)]),
        Filter::LineWidth { by } => ("line_width", vec![f(by)]),
        Filter::Wave {
            amplitude,
            wavelength,
        } => ("wave", vec![f(amplitude), f(wavelength)]),
        Filter::Pinch { amount } => ("pinch", vec![f(amount)]),
        Filter::Polar { inverse } => ("polar", vec![b(inverse)]),
        Filter::Ripple {
            amplitude,
            wavelength,
        } => ("ripple", vec![f(amplitude), f(wavelength)]),
        Filter::Fisheye { amount } => ("fisheye", vec![f(amount)]),
        Filter::Twirl { angle } => ("twirl", vec![f(angle)]),
        Filter::ZigZag { amplitude, folds } => ("zigzag", vec![f(amplitude), f(folds)]),
        Filter::SmartSmooth {
            strength,
            remove_noise,
        } => ("smart_smooth", vec![f(strength), b(remove_noise)]),
        Filter::Quick {
            effect,
            look,
            strength,
        } => ("quick", vec![b(effect), f64::from(look), f(strength)]),
        Filter::LensBlur { radius } => ("lens", vec![n(radius)]),
        Filter::JpegNoise { strength } => ("jpeg", vec![f(strength)]),
        Filter::Crystallize { size } => ("crystallize", vec![n(size)]),
        Filter::Aberration { shift } => ("aberration", vec![f(shift)]),
        Filter::Glow {
            radius,
            strength,
            rgb: color,
        } => (
            "glow",
            [vec![f(radius), f(strength)], rgb(color).to_vec()].concat(),
        ),
        Filter::DropShadow {
            dx,
            dy,
            blur,
            opacity,
            rgb: color,
        } => (
            "shadow",
            [vec![f(dx), f(dy), f(blur), f(opacity)], rgb(color).to_vec()].concat(),
        ),
        Filter::Inflate { size, depth } => ("inflate", vec![f(size), f(depth)]),
        Filter::Retro { amount } => ("retro", vec![f(amount)]),
        Filter::PerlinNoise {
            scale,
            amplitude,
            attenuation,
            repetition,
            offset,
        } => (
            "perlin",
            vec![
                f(scale),
                f(amplitude),
                f(attenuation),
                n(repetition),
                f(offset[0]),
                f(offset[1]),
            ],
        ),
        Filter::Clouds { from, to } => ("clouds", [rgb(from), rgb(to)].concat()),
        Filter::Unsharp {
            radius,
            amount,
            threshold,
        } => ("unsharp", vec![f(radius), f(amount), f(threshold)]),
        Filter::LineExtraction {
            sensitivity,
            black,
            smooth,
        } => ("line_extraction", vec![f(sensitivity), f(black), f(smooth)]),
        Filter::Noise { amount } => ("noise", vec![f(amount)]),
        Filter::Recolor { rgb: color } => ("recolor", rgb(color).to_vec()),
        Filter::BrightnessToOpacity { rgb: color } => ("to_opacity", rgb(color).to_vec()),
        Filter::FocusLines {
            count,
            gap,
            width,
            jitter,
            seed,
            rgb: color,
        } => (
            "focus_lines",
            [
                vec![n(count), f(gap), f(width), f(jitter), n(seed)],
                rgb(color).to_vec(),
            ]
            .concat(),
        ),
        Filter::SpeedLines {
            count,
            angle,
            width,
            jitter,
            seed,
            rgb: color,
        } => (
            "speed_lines",
            [
                vec![n(count), f(angle), f(width), f(jitter), n(seed)],
                rgb(color).to_vec(),
            ]
            .concat(),
        ),
        Filter::Tone {
            density,
            cell,
            angle,
            rgb: color,
        } => (
            "tone",
            [vec![f(density), f(cell), f(angle)], rgb(color).to_vec()].concat(),
        ),
        Filter::CurvedSurface { amount, axis } => {
            ("curved_surface", vec![f(amount), f64::from(axis)])
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
        } => (
            "artistic",
            vec![
                f64::from(process),
                f(line_width),
                f(simplicity),
                f(density),
                f(line_opacity),
                b(antialias),
                f(blending),
                f(color_blur),
                n(colors),
            ],
        ),
        // Every tonal correction has its text above.
        _ => return None,
    };
    let values: Vec<String> = values.iter().map(ToString::to_string).collect();
    Some(format!("{name}={}", values.join(",")))
}

/// A filter read back from [`filter_text`].
fn filter_of(text: &str) -> Option<Filter> {
    let Some((name, values)) = text.split_once('=') else {
        return Filter::from_text(text);
    };
    let values = if values.is_empty() {
        Vec::new()
    } else {
        let read = numbers(values);
        if read.is_empty() {
            return None;
        }
        read
    };
    let f = |value: f64| value as f32;
    let n = |value: f64| value.max(0.0).round() as u32;
    let byte = |value: f64| value.clamp(0.0, 255.0).round() as u8;
    let b = |value: f64| value != 0.0;
    let rgb = |values: &[f64]| [byte(values[0]), byte(values[1]), byte(values[2])];
    Some(match (name, values.as_slice()) {
        ("gaussian", &[radius]) => Filter::GaussianBlur { radius: f(radius) },
        ("sharpen", &[amount]) => Filter::Sharpen { amount: f(amount) },
        ("mosaic", &[size]) => Filter::Mosaic { size: n(size) },
        ("motion", &[angle, distance]) => Filter::MotionBlur {
            angle: f(angle),
            distance: f(distance),
        },
        ("radial", &[angle, x, y]) => Filter::RadialBlur {
            angle: f(angle),
            center: [f(x), f(y)],
        },
        ("zoom", &[amount, x, y]) => Filter::ZoomBlur {
            amount: f(amount),
            center: [f(x), f(y)],
        },
        ("smooth", &[]) => Filter::Smooth,
        ("dust", &[size]) => Filter::Dust { size: n(size) },
        ("line_width", &[by]) => Filter::LineWidth { by: f(by) },
        ("wave", &[amplitude, wavelength]) => Filter::Wave {
            amplitude: f(amplitude),
            wavelength: f(wavelength),
        },
        ("pinch", &[amount]) => Filter::Pinch { amount: f(amount) },
        ("polar", &[inverse]) => Filter::Polar {
            inverse: b(inverse),
        },
        ("ripple", &[amplitude, wavelength]) => Filter::Ripple {
            amplitude: f(amplitude),
            wavelength: f(wavelength),
        },
        ("fisheye", &[amount]) => Filter::Fisheye { amount: f(amount) },
        ("twirl", &[angle]) => Filter::Twirl { angle: f(angle) },
        ("zigzag", &[amplitude, folds]) => Filter::ZigZag {
            amplitude: f(amplitude),
            folds: f(folds),
        },
        ("smart_smooth", &[strength, remove_noise]) => Filter::SmartSmooth {
            strength: f(strength),
            remove_noise: b(remove_noise),
        },
        ("quick", &[effect, look, strength]) => Filter::Quick {
            effect: b(effect),
            look: byte(look),
            strength: f(strength),
        },
        ("lens", &[radius]) => Filter::LensBlur { radius: n(radius) },
        ("jpeg", &[strength]) => Filter::JpegNoise {
            strength: f(strength),
        },
        ("crystallize", &[size]) => Filter::Crystallize { size: n(size) },
        ("aberration", &[shift]) => Filter::Aberration { shift: f(shift) },
        ("glow", &[radius, strength, ref color @ ..]) if color.len() == 3 => Filter::Glow {
            radius: f(radius),
            strength: f(strength),
            rgb: rgb(color),
        },
        ("shadow", &[dx, dy, blur, opacity, ref color @ ..]) if color.len() == 3 => {
            Filter::DropShadow {
                dx: f(dx),
                dy: f(dy),
                blur: f(blur),
                opacity: f(opacity),
                rgb: rgb(color),
            }
        }
        ("inflate", &[size, depth]) => Filter::Inflate {
            size: f(size),
            depth: f(depth),
        },
        ("retro", &[amount]) => Filter::Retro { amount: f(amount) },
        ("perlin", &[scale, amplitude, attenuation, repetition, x, y]) => Filter::PerlinNoise {
            scale: f(scale),
            amplitude: f(amplitude),
            attenuation: f(attenuation),
            repetition: n(repetition),
            offset: [f(x), f(y)],
        },
        ("clouds", values) if values.len() == 6 => Filter::Clouds {
            from: rgb(&values[..3]),
            to: rgb(&values[3..]),
        },
        ("unsharp", &[radius, amount, threshold]) => Filter::Unsharp {
            radius: f(radius),
            amount: f(amount),
            threshold: f(threshold),
        },
        ("line_extraction", &[sensitivity, black, smooth]) => Filter::LineExtraction {
            sensitivity: f(sensitivity),
            black: f(black),
            smooth: f(smooth),
        },
        ("noise", &[amount]) => Filter::Noise { amount: f(amount) },
        ("recolor", values) if values.len() == 3 => Filter::Recolor { rgb: rgb(values) },
        ("to_opacity", values) if values.len() == 3 => {
            Filter::BrightnessToOpacity { rgb: rgb(values) }
        }
        ("focus_lines", &[count, gap, width, jitter, seed, ref color @ ..]) if color.len() == 3 => {
            Filter::FocusLines {
                count: n(count),
                gap: f(gap),
                width: f(width),
                jitter: f(jitter),
                seed: n(seed),
                rgb: rgb(color),
            }
        }
        ("speed_lines", &[count, angle, width, jitter, seed, ref color @ ..])
            if color.len() == 3 =>
        {
            Filter::SpeedLines {
                count: n(count),
                angle: f(angle),
                width: f(width),
                jitter: f(jitter),
                seed: n(seed),
                rgb: rgb(color),
            }
        }
        ("tone", &[density, cell, angle, ref color @ ..]) if color.len() == 3 => Filter::Tone {
            density: f(density),
            cell: f(cell),
            angle: f(angle),
            rgb: rgb(color),
        },
        ("curved_surface", &[amount, axis]) => Filter::CurvedSurface {
            amount: f(amount),
            axis: byte(axis),
        },
        (
            "artistic",
            &[
                process,
                line_width,
                simplicity,
                density,
                line_opacity,
                antialias,
                blending,
                color_blur,
                colors,
            ],
        ) => Filter::Artistic {
            process: byte(process),
            line_width: f(line_width),
            simplicity: f(simplicity),
            density: f(density),
            line_opacity: f(line_opacity),
            antialias: b(antialias),
            blending: f(blending),
            color_blur: f(color_blur),
            colors: n(colors),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_keeps_its_settings_through_text() {
        let filters = [
            Filter::GaussianBlur { radius: 12.5 },
            Filter::Mosaic { size: 9 },
            Filter::MotionBlur {
                angle: 33.0,
                distance: 7.25,
            },
            Filter::RadialBlur {
                angle: 4.0,
                center: [0.25, 0.75],
            },
            Filter::Smooth,
            Filter::Polar { inverse: true },
            Filter::Quick {
                effect: true,
                look: 3,
                strength: 0.4,
            },
            Filter::DropShadow {
                dx: -3.0,
                dy: 4.5,
                blur: 2.0,
                opacity: 0.6,
                rgb: [10, 20, 30],
            },
            Filter::PerlinNoise {
                scale: 0.1,
                amplitude: 0.7,
                attenuation: 0.5,
                repetition: 4,
                offset: [1.0, 2.0],
            },
            Filter::Clouds {
                from: [1, 2, 3],
                to: [250, 251, 252],
            },
            Filter::SpeedLines {
                count: 80,
                angle: 30.0,
                width: 2.0,
                jitter: 0.3,
                seed: 7,
                rgb: [0, 0, 0],
            },
            Filter::Artistic {
                process: 2,
                line_width: 1.5,
                simplicity: 0.3,
                density: 0.6,
                line_opacity: 0.9,
                antialias: true,
                blending: 0.2,
                color_blur: 1.0,
                colors: 12,
            },
            Filter::HueSaturation {
                hue: 20.0,
                saturation: -10.0,
                luminosity: 5.0,
            },
            Filter::Posterize { levels: 5 },
        ];
        for filter in filters {
            let text = filter_text(&filter).unwrap_or_else(|| panic!("{filter:?} as text"));
            assert_eq!(filter_of(&text), Some(filter), "{text}");
        }
        assert_eq!(filter_of("gaussian=oops"), None);
        assert_eq!(filter_of("unknown=1"), None);
    }

    #[test]
    fn settings_go_only_to_a_dialog_of_their_kind() {
        let mut select = Dialog::SelectBy(0, 4.0);
        let kept = of_dialog(&Dialog::SelectBy(0, 12.0)).unwrap();
        assert!(apply(&mut select, &kept));
        assert!(matches!(select, Dialog::SelectBy(0, amount) if amount == 12.0));
        // Shrink's settings do not go to Expand's dialog.
        let shrink = of_dialog(&Dialog::SelectBy(1, 3.0)).unwrap();
        assert!(!apply(&mut select, &shrink));
        assert!(!apply(&mut select, "gamut:10,1"));
        let mut gamut = Dialog::Gamut(5.0, false);
        assert!(apply(&mut gamut, "gamut:30,1"));
        assert!(matches!(gamut, Dialog::Gamut(tolerance, true) if tolerance == 30.0));
        assert!(!apply(&mut gamut, "nonsense"));
    }
}
