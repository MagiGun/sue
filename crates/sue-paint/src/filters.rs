//! Tonal corrections and filters that rewrite a layer's pixels.
//!
//! Each [`Filter`] works on straight RGBA8 pixels. With a selection, the
//! result is mixed in by the selection's coverage, so only the selected
//! area changes. The studio previews a filter in place and records the
//! changed rectangle as one undo step when it is applied.

use crate::select::Selection;

/// A pixel operation with its settings.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "one filter is set up at a time")]
pub enum Filter {
    /// Turns every color into its opposite.
    Invert,
    /// Brightness and contrast, each -1 to 1.
    BrightnessContrast {
        /// Lighter above 0.
        brightness: f32,
        /// Stronger above 0.
        contrast: f32,
    },
    /// Turns hue, saturation and luminosity.
    HueSaturation {
        /// Degrees, -180 to 180.
        hue: f32,
        /// -1 (gray) to 1.
        saturation: f32,
        /// -1 (black) to 1 (white).
        luminosity: f32,
    },
    /// Fewer tones per channel.
    Posterize {
        /// Tones per channel, 2 to 32.
        levels: u32,
    },
    /// Black or white by brightness.
    Binarize {
        /// The brightness that turns white, 0 to 1.
        threshold: f32,
    },
    /// A soft blur.
    GaussianBlur {
        /// Radius in pixels.
        radius: f32,
    },
    /// Crisper edges.
    Sharpen {
        /// 0 to 3.
        amount: f32,
    },
    /// Square blocks of one color.
    Mosaic {
        /// Block side in pixels.
        size: u32,
    },
    /// Motion Blur: smeared along a direction.
    MotionBlur {
        /// The direction, degrees.
        angle: f32,
        /// How far, pixels.
        distance: f32,
    },
    /// Radial Blur: spun round the middle.
    RadialBlur {
        /// How far round, degrees.
        angle: f32,
    },
    /// Zoom Blur: streaked toward the middle.
    ZoomBlur {
        /// How far, 0 to 1 of the way.
        amount: f32,
    },
    /// Smooth: jagged edges softened.
    Smooth,
    /// Remove Dust: specks no larger than `size` cleared.
    Dust {
        /// The largest speck, pixels.
        size: u32,
    },
    /// Line Width: lines thickened (or, below 0, thinned) by pixels.
    LineWidth {
        /// Pixels; negative thins.
        by: f32,
    },
    /// Wave: rows and columns shifted along a wave.
    Wave {
        /// How far, pixels.
        amplitude: f32,
        /// How long a wave is, pixels.
        wavelength: f32,
    },
    /// Pinch: drawn into the middle, or (below 0) swelled out.
    Pinch {
        /// -1 to 1.
        amount: f32,
    },
    /// Polar Coordinates: rows wrapped round the middle (or back again).
    Polar {
        /// Back from round to straight.
        inverse: bool,
    },
    /// Ripple: rings spreading from the middle.
    Ripple {
        /// How far, pixels.
        amplitude: f32,
        /// How far apart, pixels.
        wavelength: f32,
    },
    /// Fisheye: bulged like a wide lens.
    Fisheye {
        /// 0 to 1.
        amount: f32,
    },
    /// Chromatic Aberration: red and blue pulled apart toward the edges.
    Aberration {
        /// Pixels at the edge.
        shift: f32,
    },
    /// Retro: faded, warm, darker toward the edges, with grain.
    Retro {
        /// 0 to 1.
        amount: f32,
    },
    /// Perlin Noise: smooth noise in gray.
    PerlinNoise {
        /// How large, pixels.
        scale: f32,
        /// How strong the contrast, 0 to 2.
        contrast: f32,
    },
    /// Clouds: smooth noise between two colors.
    Clouds {
        /// The dark color.
        from: [u8; 3],
        /// The light color.
        to: [u8; 3],
    },
    /// Unsharp Mask: edges sharpened by a blurred copy.
    Unsharp {
        /// Blur radius, pixels.
        radius: f32,
        /// How much, 0 to 5.
        amount: f32,
        /// Differences smaller than this stay, 0 to 1.
        threshold: f32,
    },
    /// Line Extraction: the lines of a photo or scan as black line art on
    /// clear, from a difference of gaussians, with dark areas filled.
    LineExtraction {
        /// How faint an edge still becomes a line, 0 to 1.
        sensitivity: f32,
        /// Areas darker than this fill black, 0 (none) to 1.
        black: f32,
        /// How much the picture is smoothed first, in pixels.
        smooth: f32,
    },
    /// Grain.
    Noise {
        /// 0 to 1.
        amount: f32,
    },
    /// Every pixel in one color, keeping its alpha.
    Recolor {
        /// The new color.
        rgb: [u8; 3],
    },
    /// Light areas become transparent, dark ones stay, in one color.
    BrightnessToOpacity {
        /// The color left behind.
        rgb: [u8; 3],
    },
    /// Tone Curve: a curve for all channels, then one for each.
    ToneCurve {
        /// All of red, green and blue.
        rgb: sue_model::Response,
        /// Red alone.
        red: sue_model::Response,
        /// Green alone.
        green: sue_model::Response,
        /// Blue alone.
        blue: sue_model::Response,
        /// The channel being edited (0 all, then red, green, blue).
        channel: u8,
    },
    /// Level Correction: the input's black, middle and white points, and
    /// the output's black and white.
    Levels {
        /// Input black, 0 to 1.
        black: f32,
        /// Input white, 0 to 1.
        white: f32,
        /// Middle gray's gamma, 0.1 to 10 (1 leaves it).
        gamma: f32,
        /// Output black, 0 to 1.
        out_black: f32,
        /// Output white, 0 to 1.
        out_white: f32,
    },
    /// Color Balance: shifts toward red, green and blue (or away, to cyan,
    /// magenta and yellow) in the shadows, midtones and highlights.
    ColorBalance {
        /// For shadows, midtones and highlights: red, green and blue shifts,
        /// -1 to 1.
        shifts: [[f32; 3]; 3],
        /// The range being edited (0 shadows, 1 midtones, 2 highlights).
        range: u8,
    },
    /// Gradient Map: brightness picks a color from a gradient.
    GradientMap {
        /// The color of black.
        from: [u8; 3],
        /// The color of white.
        to: [u8; 3],
    },
    /// Manga focus lines toward the middle (Clip Studio Paint's saturated
    /// lines).
    FocusLines {
        /// How many lines.
        count: u32,
        /// The clear middle, 0 to 1 of the way out.
        gap: f32,
        /// Line width at the base, in pixels.
        width: f32,
        /// How much the lengths vary, 0 to 1.
        jitter: f32,
        /// A different scatter for each number.
        seed: u32,
        /// The ink.
        rgb: [u8; 3],
    },
    /// Manga speed lines, parallel (Clip Studio Paint's streamlines).
    SpeedLines {
        /// How many lines.
        count: u32,
        /// Their direction in degrees.
        angle: f32,
        /// Line width in the middle, in pixels.
        width: f32,
        /// How much the lengths vary, 0 to 1.
        jitter: f32,
        /// A different scatter for each number.
        seed: u32,
        /// The ink.
        rgb: [u8; 3],
    },
    /// Screentone dots.
    Tone {
        /// How much of the area the dots cover, 0 to 1.
        density: f32,
        /// Dot spacing in pixels.
        cell: f32,
        /// The screen's angle in degrees.
        angle: f32,
        /// The ink.
        rgb: [u8; 3],
    },
}

impl Filter {
    /// What people read.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Invert => "Reverse Gradient",
            Self::BrightnessContrast { .. } => "Brightness/Contrast",
            Self::HueSaturation { .. } => "Hue/Saturation/Luminosity",
            Self::Posterize { .. } => "Posterization",
            Self::Binarize { .. } => "Binarization",
            Self::GaussianBlur { .. } => "Gaussian Blur",
            Self::Sharpen { .. } => "Sharpen",
            Self::Mosaic { .. } => "Mosaic",
            Self::LineExtraction { .. } => "Line Extraction",
            Self::MotionBlur { .. } => "Motion Blur",
            Self::RadialBlur { .. } => "Radial Blur",
            Self::ZoomBlur { .. } => "Zoom Blur",
            Self::Smooth => "Smooth",
            Self::Dust { .. } => "Remove Dust",
            Self::LineWidth { .. } => "Line Width",
            Self::Wave { .. } => "Wave",
            Self::Pinch { .. } => "Pinch",
            Self::Polar { .. } => "Polar Coordinates",
            Self::Ripple { .. } => "Ripple",
            Self::Fisheye { .. } => "Fisheye",
            Self::Aberration { .. } => "Chromatic Aberration",
            Self::Retro { .. } => "Retro",
            Self::PerlinNoise { .. } => "Perlin Noise",
            Self::Clouds { .. } => "Clouds",
            Self::Unsharp { .. } => "Unsharp Mask",
            Self::Noise { .. } => "Add Noise",
            Self::Recolor { .. } => "Change Color to Drawing Color",
            Self::BrightnessToOpacity { .. } => "Convert Brightness to Opacity",
            Self::ToneCurve { .. } => "Tone Curve",
            Self::Levels { .. } => "Level Correction",
            Self::ColorBalance { .. } => "Color Balance",
            Self::GradientMap { .. } => "Gradient Map",
            Self::FocusLines { .. } => "Focus Lines",
            Self::SpeedLines { .. } => "Speed Lines",
            Self::Tone { .. } => "Screentone",
        }
    }

    /// Runs the filter over a layer's pixels. `offset` is where the layer
    /// sits in the picture, for reading the selection.
    pub fn apply(
        &self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        selection: Option<&Selection>,
        offset: [i64; 2],
    ) {
        let original = selection.map(|_| pixels.to_vec());
        match *self {
            Self::GaussianBlur { radius } => blur(pixels, width, height, radius),
            Self::Sharpen { amount } => {
                let mut soft = pixels.to_vec();
                blur(&mut soft, width, height, 1.0);
                for (pixel, soft) in pixels.iter_mut().zip(&soft) {
                    let value = f32::from(*pixel) + (f32::from(*pixel) - f32::from(*soft)) * amount;
                    *pixel = value.round().clamp(0.0, 255.0) as u8;
                }
            }
            Self::Mosaic { size } => mosaic(pixels, width, height, size.max(1)),
            Self::LineExtraction {
                sensitivity,
                black,
                smooth,
            } => extract_lines(pixels, width, height, sensitivity, black, smooth),
            Self::MotionBlur { angle, distance } => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let steps = distance.round().max(1.0) as i32;
                along(pixels, width, height, steps, |x, y, t| {
                    (x + cos * t * distance / 2.0, y + sin * t * distance / 2.0)
                });
            }
            Self::RadialBlur { angle } => {
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                let steps = (angle * 2.0).round().clamp(2.0, 64.0) as i32;
                along(pixels, width, height, steps, |x, y, t| {
                    let (sin, cos) = (angle.to_radians() / 2.0 * t).sin_cos();
                    let (dx, dy) = (x - cx, y - cy);
                    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
                });
            }
            Self::ZoomBlur { amount } => {
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                along(pixels, width, height, 24, |x, y, t| {
                    let scale = 1.0 - amount.clamp(0.0, 1.0) * 0.5 * (t + 1.0) / 2.0;
                    (cx + (x - cx) * scale, cy + (y - cy) * scale)
                });
            }
            Self::Smooth => blur(pixels, width, height, 0.7),
            Self::Dust { size } => {
                crate::correct::remove_dust(
                    pixels,
                    width,
                    height,
                    [0, 0, width, height],
                    size.max(1),
                );
            }
            Self::LineWidth { by } => line_width(pixels, width, height, by),
            Self::Wave {
                amplitude,
                wavelength,
            } => {
                let k = std::f32::consts::TAU / wavelength.max(1.0);
                remap(pixels, width, height, |x, y| {
                    (x + (y * k).sin() * amplitude, y + (x * k).sin() * amplitude)
                });
            }
            Self::Pinch { amount } => {
                let (cx, cy, r) = (
                    width as f32 / 2.0,
                    height as f32 / 2.0,
                    width.min(height) as f32 / 2.0,
                );
                remap(pixels, width, height, |x, y| {
                    let (dx, dy) = (x - cx, y - cy);
                    let d = dx.hypot(dy) / r.max(1.0);
                    if d >= 1.0 {
                        return (x, y);
                    }
                    let scale = (d.powf(1.0 - amount.clamp(-1.0, 1.0) * 0.6)) / d.max(1e-4);
                    (cx + dx * scale, cy + dy * scale)
                });
            }
            Self::Polar { inverse } => {
                let (w, h) = (width as f32, height as f32);
                let (cx, cy) = (w / 2.0, h / 2.0);
                let reach = cx.hypot(cy);
                remap(pixels, width, height, |x, y| {
                    if inverse {
                        // A straight picture from a round one.
                        let theta = x / w * std::f32::consts::TAU;
                        let r = (1.0 - y / h) * reach;
                        (cx + theta.sin() * r, cy - theta.cos() * r)
                    } else {
                        let (dx, dy) = (x - cx, y - cy);
                        let theta = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
                        (
                            theta / std::f32::consts::TAU * w,
                            (1.0 - dx.hypot(dy) / reach) * h,
                        )
                    }
                });
            }
            Self::Ripple {
                amplitude,
                wavelength,
            } => {
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                let k = std::f32::consts::TAU / wavelength.max(1.0);
                remap(pixels, width, height, |x, y| {
                    let (dx, dy) = (x - cx, y - cy);
                    let d = dx.hypot(dy).max(1e-4);
                    let push = (d * k).sin() * amplitude;
                    (x + dx / d * push, y + dy / d * push)
                });
            }
            Self::Fisheye { amount } => {
                let (cx, cy, r) = (
                    width as f32 / 2.0,
                    height as f32 / 2.0,
                    (width.min(height) as f32 / 2.0).max(1.0),
                );
                remap(pixels, width, height, |x, y| {
                    let (dx, dy) = ((x - cx) / r, (y - cy) / r);
                    let d = dx.hypot(dy);
                    let scale = 1.0 - amount.clamp(0.0, 1.0) * 0.5 * (1.0 - d * d).max(0.0);
                    (cx + dx * scale * r, cy + dy * scale * r)
                });
            }
            Self::Aberration { shift } => {
                let source = pixels.to_vec();
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                let reach = cx.hypot(cy).max(1.0);
                for y in 0..height {
                    for x in 0..width {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let (dx, dy) = ((px - cx) / reach, (py - cy) / reach);
                        let at = ((y * width + x) * 4) as usize;
                        let red = crate::warp::sample(
                            &source,
                            width,
                            height,
                            px + dx * shift,
                            py + dy * shift,
                        );
                        let blue = crate::warp::sample(
                            &source,
                            width,
                            height,
                            px - dx * shift,
                            py - dy * shift,
                        );
                        pixels[at] = red[0];
                        pixels[at + 2] = blue[2];
                    }
                }
            }
            Self::Retro { amount } => {
                let a = amount.clamp(0.0, 1.0);
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                let reach = cx.hypot(cy).max(1.0);
                for y in 0..height {
                    for x in 0..width {
                        let at = ((y * width + x) * 4) as usize;
                        let [r, g, b] = [pixels[at], pixels[at + 1], pixels[at + 2]]
                            .map(|v| f32::from(v) / 255.0);
                        let l = 0.299 * r + 0.587 * g + 0.114 * b;
                        // Toward sepia, faded, darker at the edges, grainy.
                        let sepia = [l * 1.07 + 0.08, l * 0.95 + 0.03, l * 0.75];
                        let fade = 0.12 * a;
                        let edge = ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) / reach)
                            .powi(2)
                            * 0.45
                            * a;
                        let grain = (noise2(x as f32 * 0.9, y as f32 * 0.9, 7) - 0.5) * 0.12 * a;
                        let out = [r, g, b].into_iter().zip(sepia).map(|(c, s)| {
                            ((c + (s - c) * a) * (1.0 - fade) + fade - edge + grain).clamp(0.0, 1.0)
                        });
                        for (channel, value) in out.enumerate() {
                            pixels[at + channel] = (value * 255.0).round() as u8;
                        }
                    }
                }
            }
            Self::PerlinNoise { scale, contrast } => {
                for y in 0..height {
                    for x in 0..width {
                        let n = fbm(x as f32 / scale.max(1.0), y as f32 / scale.max(1.0), 4);
                        let value = (((n - 0.5) * contrast.max(0.0)) + 0.5).clamp(0.0, 1.0);
                        let byte = (value * 255.0).round() as u8;
                        pixels[((y * width + x) * 4) as usize..][..4]
                            .copy_from_slice(&[byte, byte, byte, 255]);
                    }
                }
            }
            Self::Clouds { from, to } => {
                let scale = (width.max(height) as f32 / 4.0).max(8.0);
                for y in 0..height {
                    for x in 0..width {
                        let t = fbm(x as f32 / scale, y as f32 / scale, 6).clamp(0.0, 1.0);
                        let mix = |a: u8, b: u8| {
                            (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
                        };
                        pixels[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[
                            mix(from[0], to[0]),
                            mix(from[1], to[1]),
                            mix(from[2], to[2]),
                            255,
                        ]);
                    }
                }
            }
            Self::Unsharp {
                radius,
                amount,
                threshold,
            } => {
                let mut soft = pixels.to_vec();
                blur(&mut soft, width, height, radius.max(0.5));
                let limit = threshold.clamp(0.0, 1.0) * 255.0;
                for (index, (pixel, soft)) in pixels.iter_mut().zip(&soft).enumerate() {
                    if index % 4 == 3 {
                        continue;
                    }
                    let difference = f32::from(*pixel) - f32::from(*soft);
                    if difference.abs() >= limit {
                        *pixel = (f32::from(*pixel) + difference * amount)
                            .round()
                            .clamp(0.0, 255.0) as u8;
                    }
                }
            }
            Self::ToneCurve {
                rgb,
                red,
                green,
                blue,
                ..
            } => {
                // A table per channel: the curves run once per value, not
                // once per pixel.
                let table = |curve: &sue_model::Response| -> [u8; 256] {
                    std::array::from_fn(|value| {
                        let out = curve.eval(rgb.eval(value as f32 / 255.0));
                        (out.clamp(0.0, 1.0) * 255.0).round() as u8
                    })
                };
                let tables = [table(&red), table(&green), table(&blue)];
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    for (channel, table) in pixel.iter_mut().zip(&tables) {
                        *channel = table[usize::from(*channel)];
                    }
                }
            }
            Self::FocusLines {
                count,
                gap,
                width: line,
                jitter,
                seed,
                rgb,
            } => crate::manga::focus_lines(
                pixels,
                width,
                height,
                [width as f32 / 2.0, height as f32 / 2.0],
                count,
                gap,
                line,
                jitter,
                seed,
                rgb,
            ),
            Self::SpeedLines {
                count,
                angle,
                width: line,
                jitter,
                seed,
                rgb,
            } => crate::manga::speed_lines(
                pixels, width, height, count, angle, line, jitter, seed, rgb,
            ),
            Self::Tone {
                density,
                cell,
                angle,
                rgb,
            } => crate::manga::tone(pixels, width, height, density, cell, angle, rgb),
            _ => {
                for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    *pixel = self.point(*pixel, index as u32 % width, index as u32 / width);
                }
            }
        }
        if let (Some(selection), Some(original)) = (selection, original) {
            for y in 0..height {
                for x in 0..width {
                    let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                    let cover = if px < 0 || py < 0 {
                        0.0
                    } else {
                        selection.at(px as u32, py as u32)
                    };
                    if cover >= 1.0 {
                        continue;
                    }
                    let at = ((y * width + x) * 4) as usize;
                    for channel in at..at + 4 {
                        let (old, new) = (f32::from(original[channel]), f32::from(pixels[channel]));
                        pixels[channel] = (old + (new - old) * cover).round() as u8;
                    }
                }
            }
        }
    }

    /// The operation on one pixel (for the per-pixel filters).
    /// A correction as text, for a correction layer's setting; `None` for
    /// filters that are not corrections.
    #[must_use]
    pub fn to_text(&self) -> Option<String> {
        let hex = |[r, g, b]: [u8; 3]| format!("{r:02x}{g:02x}{b:02x}");
        Some(match *self {
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => format!("brightness:{brightness},{contrast}"),
            Self::HueSaturation {
                hue,
                saturation,
                luminosity,
            } => format!("hue:{hue},{saturation},{luminosity}"),
            Self::Levels {
                black,
                white,
                gamma,
                out_black,
                out_white,
            } => {
                format!("levels:{black},{white},{gamma},{out_black},{out_white}")
            }
            Self::ToneCurve {
                rgb,
                red,
                green,
                blue,
                ..
            } => format!("curve:{rgb}|{red}|{green}|{blue}"),
            Self::ColorBalance { shifts, .. } => {
                let values: Vec<String> =
                    shifts.iter().flatten().map(ToString::to_string).collect();
                format!("balance:{}", values.join(","))
            }
            Self::Invert => "invert".to_owned(),
            Self::Posterize { levels } => format!("posterize:{levels}"),
            Self::Binarize { threshold } => format!("binarize:{threshold}"),
            Self::GradientMap { from, to } => format!("gradient:{},{}", hex(from), hex(to)),
            _ => return None,
        })
    }

    /// A correction read back from [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let (kind, values) = text.split_once(':').unwrap_or((text, ""));
        let numbers: Vec<f32> = values
            .split(',')
            .filter_map(|value| value.trim().parse().ok())
            .collect();
        let hex = |value: &str| -> Option<[u8; 3]> {
            let value = value.trim();
            let byte = |at: usize| u8::from_str_radix(value.get(at..at + 2)?, 16).ok();
            Some([byte(0)?, byte(2)?, byte(4)?])
        };
        Some(match (kind, numbers.as_slice()) {
            ("brightness", &[brightness, contrast]) => Self::BrightnessContrast {
                brightness,
                contrast,
            },
            ("hue", &[hue, saturation, luminosity]) => Self::HueSaturation {
                hue,
                saturation,
                luminosity,
            },
            ("levels", &[black, white, gamma, out_black, out_white]) => Self::Levels {
                black,
                white,
                gamma,
                out_black,
                out_white,
            },
            ("curve", _) => {
                let curves: Vec<sue_model::Response> = values
                    .split('|')
                    .filter_map(|curve| curve.parse().ok())
                    .collect();
                let [rgb, red, green, blue] = curves.as_slice() else {
                    return None;
                };
                Self::ToneCurve {
                    rgb: *rgb,
                    red: *red,
                    green: *green,
                    blue: *blue,
                    channel: 0,
                }
            }
            ("balance", values) if values.len() == 9 => {
                let mut shifts = [[0.0; 3]; 3];
                for (index, value) in values.iter().enumerate() {
                    shifts[index / 3][index % 3] = *value;
                }
                Self::ColorBalance { shifts, range: 1 }
            }
            ("invert", _) => Self::Invert,
            ("posterize", &[levels]) => Self::Posterize {
                levels: levels as u32,
            },
            ("binarize", &[threshold]) => Self::Binarize { threshold },
            ("gradient", _) => {
                let (from, to) = values.split_once(',')?;
                Self::GradientMap {
                    from: hex(from)?,
                    to: hex(to)?,
                }
            }
            _ => return None,
        })
    }

    /// `true` for corrections that change each channel on its own, so a
    /// table of 256 values a channel stands for them.
    #[must_use]
    pub const fn per_channel(&self) -> bool {
        matches!(
            self,
            Self::ToneCurve { .. }
                | Self::Levels { .. }
                | Self::Invert
                | Self::Posterize { .. }
                | Self::BrightnessContrast { .. }
        )
    }

    pub(crate) fn point(&self, [r, g, b, a]: [u8; 4], x: u32, y: u32) -> [u8; 4] {
        let unit = |value: u8| f32::from(value) / 255.0;
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgb = [unit(r), unit(g), unit(b)];
        let out = match *self {
            Self::Invert => rgb.map(|value| 1.0 - value),
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => {
                let gain = if contrast >= 0.0 {
                    1.0 / (1.0 - contrast.min(0.99))
                } else {
                    1.0 + contrast
                };
                rgb.map(|value| (value + brightness * 0.5 - 0.5) * gain + 0.5)
            }
            Self::HueSaturation {
                hue,
                saturation,
                luminosity,
            } => {
                let mut hsv = crate::color::Hsv::from_rgb(rgb);
                hsv.h += hue;
                hsv.s = (hsv.s * (1.0 + saturation)).clamp(0.0, 1.0);
                let turned = hsv.to_rgb();
                if luminosity >= 0.0 {
                    turned.map(|value| value + (1.0 - value) * luminosity)
                } else {
                    turned.map(|value| value * (1.0 + luminosity))
                }
            }
            Self::Posterize { levels } => {
                let steps = levels.clamp(2, 32) as f32 - 1.0;
                rgb.map(|value| (value * steps).round() / steps)
            }
            Self::Binarize { threshold } => {
                let luma = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                [if luma >= threshold { 1.0 } else { 0.0 }; 3]
            }
            Self::Noise { amount } => {
                let mut h = x.wrapping_mul(0x27D4_EB2D) ^ y.wrapping_mul(0x1656_67B1);
                h ^= h >> 15;
                h = h.wrapping_mul(0x85EB_CA77);
                h ^= h >> 13;
                let grain = ((h & 0xFFFF) as f32 / 65_535.0 - 0.5) * amount;
                rgb.map(|value| value + grain)
            }
            Self::Recolor { rgb } => {
                return [rgb[0], rgb[1], rgb[2], a];
            }
            Self::ToneCurve {
                rgb: all,
                red,
                green,
                blue,
                ..
            } => [
                red.eval(all.eval(rgb[0])),
                green.eval(all.eval(rgb[1])),
                blue.eval(all.eval(rgb[2])),
            ],
            Self::Levels {
                black,
                white,
                gamma,
                out_black,
                out_white,
            } => rgb.map(|value| {
                let t = ((value - black) / (white - black).max(1e-4)).clamp(0.0, 1.0);
                out_black + (out_white - out_black) * t.powf(1.0 / gamma.max(0.01))
            }),
            Self::ColorBalance { shifts, .. } => {
                let light = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                let shadow = (1.0 - light).powi(2);
                let highlight = light.powi(2);
                let middle = (1.0 - shadow - highlight).max(0.0);
                let weights = [shadow, middle, highlight];
                let mut out = rgb;
                for (range, weight) in weights.iter().enumerate() {
                    for channel in 0..3 {
                        out[channel] += shifts[range][channel] * weight * 0.5;
                    }
                }
                out
            }
            Self::GradientMap { from, to } => {
                let light = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                [0, 1, 2].map(|channel| {
                    let (low, high) = (unit(from[channel]), unit(to[channel]));
                    low + (high - low) * light
                })
            }
            Self::BrightnessToOpacity { rgb: color } => {
                let luma = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                return [color[0], color[1], color[2], byte(unit(a) * (1.0 - luma))];
            }
            Self::GaussianBlur { .. }
            | Self::Sharpen { .. }
            | Self::Mosaic { .. }
            | Self::LineExtraction { .. }
            | Self::MotionBlur { .. }
            | Self::RadialBlur { .. }
            | Self::ZoomBlur { .. }
            | Self::Smooth
            | Self::Dust { .. }
            | Self::LineWidth { .. }
            | Self::Wave { .. }
            | Self::Pinch { .. }
            | Self::Polar { .. }
            | Self::Ripple { .. }
            | Self::Fisheye { .. }
            | Self::Aberration { .. }
            | Self::Retro { .. }
            | Self::PerlinNoise { .. }
            | Self::Clouds { .. }
            | Self::Unsharp { .. }
            | Self::FocusLines { .. }
            | Self::SpeedLines { .. }
            | Self::Tone { .. } => rgb,
        };
        [byte(out[0]), byte(out[1]), byte(out[2]), a]
    }
}

/// `pixels` drawn again from where `from(x, y)` (picture pixel centers)
/// says each one looks, bilinear.
fn remap(pixels: &mut [u8], width: u32, height: u32, from: impl Fn(f32, f32) -> (f32, f32)) {
    let source = pixels.to_vec();
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = from(x as f32 + 0.5, y as f32 + 0.5);
            let at = ((y * width + x) * 4) as usize;
            let inside = sx >= 0.0 && sy >= 0.0 && sx <= width as f32 && sy <= height as f32;
            let value = if inside {
                crate::warp::sample(&source, width, height, sx, sy)
            } else {
                [0; 4]
            };
            pixels[at..at + 4].copy_from_slice(&value);
        }
    }
}

/// Each pixel the average (premultiplied) of `steps` samples along
/// `at(x, y, t)` for `t` from -1 to 1 (the directional blurs).
fn along(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    steps: i32,
    at: impl Fn(f32, f32, f32) -> (f32, f32),
) {
    let source = pixels.to_vec();
    let steps = steps.max(1);
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut sum = [0.0_f32; 4];
            for step in -steps..=steps {
                let (sx, sy) = at(px, py, step as f32 / steps as f32);
                let sample = crate::warp::sample(&source, width, height, sx, sy);
                let alpha = f32::from(sample[3]) / 255.0;
                for channel in 0..3 {
                    sum[channel] += f32::from(sample[channel]) * alpha;
                }
                sum[3] += alpha;
            }
            let count = (steps * 2 + 1) as f32;
            let index = ((y * width + x) * 4) as usize;
            if sum[3] > 0.0 {
                for channel in 0..3 {
                    pixels[index + channel] =
                        (sum[channel] / sum[3]).round().clamp(0.0, 255.0) as u8;
                }
            }
            pixels[index + 3] = (sum[3] / count * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Lines `by` pixels thicker (or, below 0, thinner): the layer's alpha
/// grown or drawn in round, new pixels taking the color of the lines.
fn line_width(pixels: &mut [u8], width: u32, height: u32, by: f32) {
    let alpha = |inverted: bool| {
        let values: Vec<f32> = pixels
            .chunks(4)
            .map(|p| {
                let a = f32::from(p[3]) / 255.0;
                if inverted { 1.0 - a } else { a }
            })
            .collect();
        move |x: i64, y: i64| {
            if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                if inverted { 1.0 } else { 0.0 }
            } else {
                values[(y as u32 * width + x as u32) as usize]
            }
        }
    };
    if by >= 0.0 {
        // The color of the lines: the average of the opaque pixels.
        let (mut sum, mut count) = ([0.0_f32; 3], 0.0_f32);
        for p in pixels.chunks(4).filter(|p| p[3] > 128) {
            for channel in 0..3 {
                sum[channel] += f32::from(p[channel]);
            }
            count += 1.0;
        }
        let ink = sum.map(|value| (value / count.max(1.0)).round() as u8);
        let grown = crate::effects::Edge::build(alpha(false), [0, 0, width, height], by);
        for (index, pixel) in pixels.chunks_mut(4).enumerate() {
            let cover =
                (grown.at(index as u32 % width, index as u32 / width) * 255.0).round() as u8;
            if cover > pixel[3] {
                if pixel[3] == 0 {
                    pixel[..3].copy_from_slice(&ink);
                }
                pixel[3] = cover;
            }
        }
    } else {
        let gaps = crate::effects::Edge::build(alpha(true), [0, 0, width, height], -by);
        for (index, pixel) in pixels.chunks_mut(4).enumerate() {
            let keep = 1.0 - gaps.at(index as u32 % width, index as u32 / width);
            pixel[3] = (f32::from(pixel[3]) * keep).round() as u8;
        }
    }
}

/// A steady value from 0 to 1 for a lattice point.
fn noise2(x: f32, y: f32, salt: u32) -> f32 {
    let mut h = (x.floor() as i32 as u32).wrapping_mul(0x8DA6_B343)
        ^ (y.floor() as i32 as u32).wrapping_mul(0xD816_3841)
        ^ salt.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// Smooth value noise at `(x, y)`, 0 to 1.
fn smooth_noise(x: f32, y: f32, salt: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let ease = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v) = (ease(fx), ease(fy));
    let corner = |dx: f32, dy: f32| noise2(x0 + dx, y0 + dy, salt);
    let top = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * u;
    let bottom = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * u;
    top + (bottom - top) * v
}

/// Layers of smooth noise, each half the size and strength of the last.
fn fbm(x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut weight, mut total, mut scale) = (0.0, 1.0, 0.0, 1.0);
    for octave in 0..octaves {
        sum += smooth_noise(x * scale, y * scale, octave) * weight;
        total += weight;
        weight *= 0.5;
        scale *= 2.0;
    }
    sum / total
}

/// The brightness of each pixel over white, 0 to 1.
fn luma_over_white(pixels: &[u8]) -> Vec<f32> {
    pixels
        .chunks(4)
        .map(|p| {
            let a = f32::from(p[3]) / 255.0;
            let l = (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
                / 255.0;
            l * a + (1.0 - a)
        })
        .collect()
}

/// `values` (one a pixel) blurred by `radius`, through [`blur`].
fn blurred_values(values: &[f32], width: u32, height: u32, radius: f32) -> Vec<f32> {
    let mut gray: Vec<u8> = values
        .iter()
        .flat_map(|value| {
            let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            [byte, byte, byte, 255]
        })
        .collect();
    blur(&mut gray, width, height, radius);
    gray.chunks(4).map(|p| f32::from(p[0]) / 255.0).collect()
}

/// Line Extraction: where the picture is darker than its surroundings (a
/// difference of a fine and a coarse gaussian), black line art on clear;
/// areas darker than `black` fill in.
fn extract_lines(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    sensitivity: f32,
    black: f32,
    smooth: f32,
) {
    let luma = luma_over_white(pixels);
    let fine = blurred_values(&luma, width, height, smooth.max(0.5));
    let coarse = blurred_values(&luma, width, height, smooth.max(0.5) * 2.5 + 1.0);
    let gain = 4.0 + sensitivity.clamp(0.0, 1.0) * 40.0;
    for (index, pixel) in pixels.chunks_mut(4).enumerate() {
        let edge = ((coarse[index] - fine[index]) * gain - 0.15).clamp(0.0, 1.0);
        let dark = if luma[index] < black { 1.0 } else { 0.0 };
        let ink = edge.max(dark);
        let ink = ink * ink * (3.0 - 2.0 * ink);
        pixel.copy_from_slice(&[0, 0, 0, (ink * 255.0).round() as u8]);
    }
}

/// Three box blurs approximate a gaussian; colors blur premultiplied so
/// transparent pixels do not darken the edges.
fn blur(pixels: &mut [u8], width: u32, height: u32, radius: f32) {
    let (w, h) = (width as usize, height as usize);
    let mut data: Vec<[f32; 4]> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let alpha = f32::from(p[3]) / 255.0;
            [
                f32::from(p[0]) * alpha,
                f32::from(p[1]) * alpha,
                f32::from(p[2]) * alpha,
                f32::from(p[3]),
            ]
        })
        .collect();
    let box_radius = ((radius * radius * 4.0 / 3.0 + 1.0).sqrt() / 2.0)
        .round()
        .max(1.0) as usize;
    let mut scratch = vec![[0.0_f32; 4]; data.len()];
    for _ in 0..3 {
        box_pass(&data, &mut scratch, w, h, box_radius, true);
        box_pass(&scratch, &mut data, w, h, box_radius, false);
    }
    for (pixel, value) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(&data) {
        let alpha = value[3] / 255.0;
        let color = |channel: f32| {
            if alpha > 0.0 {
                (channel / alpha).round().clamp(0.0, 255.0) as u8
            } else {
                0
            }
        };
        *pixel = [
            color(value[0]),
            color(value[1]),
            color(value[2]),
            value[3].round().clamp(0.0, 255.0) as u8,
        ];
    }
}

/// One box blur along rows (`horizontal`) or columns, edges clamped.
fn box_pass(
    source: &[[f32; 4]],
    target: &mut [[f32; 4]],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
) {
    let (lines, length) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let index = |line: usize, at: usize| {
        if horizontal {
            line * width + at
        } else {
            at * width + line
        }
    };
    let span = (radius * 2 + 1) as f32;
    for line in 0..lines {
        let mut sum = [0.0_f32; 4];
        for offset in 0..=radius * 2 {
            let at = offset.saturating_sub(radius).min(length - 1);
            for channel in 0..4 {
                sum[channel] += source[index(line, at)][channel];
            }
        }
        for at in 0..length {
            target[index(line, at)] = sum.map(|value| value / span);
            let leaving = at.saturating_sub(radius);
            let entering = (at + radius + 1).min(length - 1);
            for channel in 0..4 {
                sum[channel] +=
                    source[index(line, entering)][channel] - source[index(line, leaving)][channel];
            }
        }
    }
}

/// Averages square blocks.
fn mosaic(pixels: &mut [u8], width: u32, height: u32, size: u32) {
    for by in (0..height).step_by(size as usize) {
        for bx in (0..width).step_by(size as usize) {
            let (x1, y1) = ((bx + size).min(width), (by + size).min(height));
            let mut sum = [0_u64; 4];
            let count = u64::from((x1 - bx) * (y1 - by));
            for y in by..y1 {
                for x in bx..x1 {
                    let at = ((y * width + x) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += u64::from(pixels[at + channel]);
                    }
                }
            }
            let mean = sum.map(|value| (value / count.max(1)) as u8);
            for y in by..y1 {
                for x in bx..x1 {
                    let at = ((y * width + x) * 4) as usize;
                    pixels[at..at + 4].copy_from_slice(&mean);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blurs_distortions_and_generators_change_the_picture_as_meant() {
        let (w, h) = (40_u32, 40_u32);
        // A black dot in the middle of white.
        let dot = || {
            let mut pixels = vec![255_u8; (w * h * 4) as usize];
            for y in 18..22 {
                for x in 18..22 {
                    pixels[((y * w + x) * 4) as usize..][..3].copy_from_slice(&[0, 0, 0]);
                }
            }
            pixels
        };
        let gray = |pixels: &[u8], x: u32, y: u32| pixels[((y * w + x) * 4) as usize];
        let run = |filter: Filter| {
            let mut pixels = dot();
            filter.apply(&mut pixels, w, h, None, [0, 0]);
            pixels
        };
        // Motion blur smears the dot sideways, not up.
        let smeared = run(Filter::MotionBlur {
            angle: 0.0,
            distance: 12.0,
        });
        assert!(gray(&smeared, 24, 20) < 230 && gray(&smeared, 20, 26) > 250);
        assert!(gray(&run(Filter::RadialBlur { angle: 30.0 }), 20, 20) < 255);
        assert!(gray(&run(Filter::ZoomBlur { amount: 0.5 }), 20, 20) < 255);
        assert_ne!(run(Filter::Smooth), dot());
        // Distortions move the dot; Polar there and back is close.
        assert_ne!(
            run(Filter::Wave {
                amplitude: 3.0,
                wavelength: 10.0
            }),
            dot()
        );
        assert_ne!(
            run(Filter::Ripple {
                amplitude: 2.0,
                wavelength: 6.0
            }),
            dot()
        );
        assert!(
            gray(&run(Filter::Pinch { amount: -0.8 }), 23, 20) < 128,
            "swelled out"
        );
        assert!(
            gray(&run(Filter::Fisheye { amount: 1.0 }), 23, 20) < 128,
            "bulged"
        );
        assert_ne!(run(Filter::Polar { inverse: false }), dot());
        let aberrated = run(Filter::Aberration { shift: 3.0 });
        assert_ne!(aberrated, dot());
        let retro = run(Filter::Retro { amount: 1.0 });
        assert!(retro[0] != retro[2], "warm, not gray");
        // Generators fill the layer.
        let noise = run(Filter::PerlinNoise {
            scale: 8.0,
            contrast: 1.0,
        });
        let values: std::collections::HashSet<u8> = noise.chunks(4).map(|p| p[0]).collect();
        assert!(values.len() > 20, "varied");
        let clouds = run(Filter::Clouds {
            from: [0, 0, 255],
            to: [255, 255, 255],
        });
        assert!(
            clouds.chunks(4).all(|p| p[2] == 255 && p[3] == 255),
            "between the two colors"
        );
        // Unsharp mask brightens the gray beside a darker dot.
        let mut sharp: Vec<u8> = dot()
            .iter()
            .map(|v| if *v == 255 { 160 } else { 80 })
            .collect();
        for p in sharp.chunks_mut(4) {
            p[3] = 255;
        }
        Filter::Unsharp {
            radius: 2.0,
            amount: 1.5,
            threshold: 0.0,
        }
        .apply(&mut sharp, w, h, None, [0, 0]);
        assert!(gray(&sharp, 23, 20) > 160 && gray(&sharp, 19, 20) < 80);
        // Line width: an ink line on clear grows and thins.
        let mut line = vec![0_u8; (w * h * 4) as usize];
        for x in 5..35 {
            line[((20 * w + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 0, 255]);
        }
        let mut thick = line.clone();
        Filter::LineWidth { by: 2.0 }.apply(&mut thick, w, h, None, [0, 0]);
        let cover = |pixels: &[u8], y: u32| pixels[((y * w + 20) * 4 + 3) as usize];
        assert_eq!(cover(&thick, 21), 255, "thicker");
        assert!(
            cover(&thick, 22) > 0 && cover(&thick, 24) == 0,
            "by two pixels"
        );
        let mut thin = thick.clone();
        Filter::LineWidth { by: -2.0 }.apply(&mut thin, w, h, None, [0, 0]);
        assert_eq!(thin[((22 * w + 20) * 4 + 3) as usize], 0, "thin again");
        // Remove Dust: a lone speck goes.
        let mut specks = vec![0_u8; (w * h * 4) as usize];
        specks[((5 * w + 5) * 4 + 3) as usize] = 255;
        Filter::Dust { size: 3 }.apply(&mut specks, w, h, None, [0, 0]);
        assert!(specks.chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn line_extraction_finds_the_lines_of_a_picture() {
        // A gray photo-like ground with a dark stripe and a black blot.
        let (w, h) = (60_u32, 40_u32);
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let value = if (28..31).contains(&x) {
                    40
                } else if x < 8 && y < 8 {
                    0
                } else {
                    200
                };
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        Filter::LineExtraction {
            sensitivity: 0.6,
            black: 0.1,
            smooth: 1.0,
        }
        .apply(&mut pixels, w, h, None, [0, 0]);
        let alpha = |x: u32, y: u32| pixels[((y * w + x) * 4 + 3) as usize];
        assert!(alpha(29, 20) > 200, "the stripe is a line");
        assert!(alpha(45, 20) < 20, "the plain ground is clear");
        assert!(alpha(2, 2) > 200, "dark areas fill in");
        assert_eq!(&pixels[..3], &[0, 0, 0], "black line art");
    }
    use crate::select::Op;

    fn image() -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..8_u8 {
            for x in 0..8_u8 {
                pixels.extend([x * 30, y * 30, 128, 255]);
            }
        }
        pixels
    }

    #[test]
    fn corrections_read_back_from_their_text() {
        let curve = sue_model::Response::new(&[[0.0, 0.1], [0.5, 0.3], [1.0, 1.0]]);
        let mut shifts = [[0.0; 3]; 3];
        shifts[2][1] = -0.5;
        for filter in [
            Filter::BrightnessContrast {
                brightness: 0.2,
                contrast: -0.1,
            },
            Filter::HueSaturation {
                hue: 30.0,
                saturation: 0.5,
                luminosity: 0.0,
            },
            Filter::Levels {
                black: 0.1,
                white: 0.9,
                gamma: 1.5,
                out_black: 0.0,
                out_white: 1.0,
            },
            Filter::ToneCurve {
                rgb: curve,
                red: sue_model::Response::LINEAR,
                green: curve,
                blue: sue_model::Response::LINEAR,
                channel: 0,
            },
            Filter::ColorBalance { shifts, range: 1 },
            Filter::Invert,
            Filter::Posterize { levels: 4 },
            Filter::Binarize { threshold: 0.4 },
            Filter::GradientMap {
                from: [1, 2, 3],
                to: [250, 128, 0],
            },
        ] {
            let text = filter.to_text().unwrap();
            assert_eq!(Filter::from_text(&text), Some(filter), "{text}");
        }
        assert_eq!(
            Filter::GaussianBlur { radius: 2.0 }.to_text(),
            None,
            "not a correction"
        );
        assert_eq!(Filter::from_text("nonsense:1"), None);
    }

    #[test]
    fn tonal_corrections_follow_their_settings() {
        let run = |filter: Filter, pixel: [u8; 4]| {
            let mut pixels = pixel.to_vec();
            filter.apply(&mut pixels, 1, 1, None, [0, 0]);
            [pixels[0], pixels[1], pixels[2], pixels[3]]
        };
        let linear = sue_model::Response::LINEAR;
        // A tone curve that darkens red only.
        let dark = sue_model::Response::new(&[[0.0, 0.0], [1.0, 0.5]]);
        let curve = Filter::ToneCurve {
            rgb: linear,
            red: dark,
            green: linear,
            blue: linear,
            channel: 1,
        };
        assert_eq!(run(curve, [200, 100, 50, 255]), [100, 100, 50, 255]);
        // Levels: the input white at 50% makes the middle white.
        let levels = Filter::Levels {
            black: 0.0,
            white: 0.5,
            gamma: 1.0,
            out_black: 0.0,
            out_white: 1.0,
        };
        assert_eq!(run(levels, [128, 64, 0, 200]), [255, 128, 0, 200]);
        // Color balance toward red in the midtones warms a gray.
        let mut shifts = [[0.0; 3]; 3];
        shifts[1][0] = 1.0;
        let balanced = run(
            Filter::ColorBalance { shifts, range: 1 },
            [128, 128, 128, 255],
        );
        assert!(balanced[0] > 128 && balanced[1] == 128, "{balanced:?}");
        // A gradient map from navy to yellow.
        let map = Filter::GradientMap {
            from: [0, 0, 80],
            to: [255, 255, 0],
        };
        assert_eq!(run(map, [0, 0, 0, 255]), [0, 0, 80, 255]);
        assert_eq!(run(map, [255, 255, 255, 255]), [255, 255, 0, 255]);
    }

    #[test]
    fn point_filters_follow_their_definitions() {
        let mut pixels = vec![10, 20, 30, 200];
        Filter::Invert.apply(&mut pixels, 1, 1, None, [0, 0]);
        assert_eq!(pixels, [245, 235, 225, 200]);
        let mut gray = vec![100, 100, 100, 255];
        Filter::Binarize { threshold: 0.5 }.apply(&mut gray, 1, 1, None, [0, 0]);
        assert_eq!(gray, [0, 0, 0, 255]);
        let mut poster = vec![100, 200, 0, 255];
        Filter::Posterize { levels: 2 }.apply(&mut poster, 1, 1, None, [0, 0]);
        assert_eq!(poster, [0, 255, 0, 255]);
        let mut red = vec![255, 0, 0, 255];
        Filter::HueSaturation {
            hue: 120.0,
            saturation: 0.0,
            luminosity: 0.0,
        }
        .apply(&mut red, 1, 1, None, [0, 0]);
        assert_eq!(red, [0, 255, 0, 255]);
        let mut white = vec![255, 255, 255, 255, 0, 0, 0, 255];
        Filter::BrightnessToOpacity { rgb: [9, 9, 9] }.apply(&mut white, 2, 1, None, [0, 0]);
        assert_eq!(white, [9, 9, 9, 0, 9, 9, 9, 255]);
        let mut same = vec![1, 2, 3, 4];
        Filter::BrightnessContrast {
            brightness: 0.0,
            contrast: 0.0,
        }
        .apply(&mut same, 1, 1, None, [0, 0]);
        assert_eq!(same, [1, 2, 3, 4]);
    }

    #[test]
    fn blurs_keep_flat_areas_and_soften_edges() {
        let mut flat = [50, 60, 70, 255].repeat(16 * 16);
        Filter::GaussianBlur { radius: 3.0 }.apply(&mut flat, 16, 16, None, [0, 0]);
        assert!(
            flat.as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [50, 60, 70, 255])
        );
        let mut edge = Vec::new();
        for x in 0..16 {
            edge.extend(if x < 8 {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            });
        }
        Filter::GaussianBlur { radius: 2.0 }.apply(&mut edge, 16, 1, None, [0, 0]);
        assert!(edge[7 * 4] > 0 && edge[8 * 4] < 255);
        // A clear neighbor does not darken a white pixel.
        let mut halo = vec![255, 255, 255, 255, 0, 0, 0, 0];
        Filter::GaussianBlur { radius: 1.0 }.apply(&mut halo, 2, 1, None, [0, 0]);
        assert_eq!(&halo[..3], &[255, 255, 255]);
        let mut blocks = image();
        Filter::Mosaic { size: 4 }.apply(&mut blocks, 8, 8, None, [0, 0]);
        assert_eq!(&blocks[..4], &blocks[3 * 4..4 * 4]);
    }

    #[test]
    fn selections_keep_the_rest_untouched() {
        let mut pixels = image();
        let before = pixels.clone();
        let mut selection = Selection::new(8, 8);
        selection.rect([0, 0, 4, 8], Op::Replace);
        Filter::Invert.apply(&mut pixels, 8, 8, Some(&selection), [0, 0]);
        assert_ne!(&pixels[..4], &before[..4]);
        assert_eq!(&pixels[6 * 4..7 * 4], &before[6 * 4..7 * 4]);
    }
}
