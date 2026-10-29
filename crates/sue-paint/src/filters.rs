//! Tonal corrections and filters that rewrite a layer's pixels.
//!
//! Each [`Filter`] works on straight RGBA8 pixels. With a selection, the
//! result is mixed in by the selection's coverage, so only the selected
//! area changes. The studio previews a filter in place and records the
//! changed rectangle as one undo step when it is applied.

use crate::select::Selection;

/// A pixel operation with its settings.
#[derive(Clone, Copy, Debug, PartialEq)]
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
            Self::Noise { .. } => "Add Noise",
            Self::Recolor { .. } => "Change Color to Drawing Color",
            Self::BrightnessToOpacity { .. } => "Convert Brightness to Opacity",
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
    fn point(&self, [r, g, b, a]: [u8; 4], x: u32, y: u32) -> [u8; 4] {
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
            Self::BrightnessToOpacity { rgb: color } => {
                let luma = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                return [color[0], color[1], color[2], byte(unit(a) * (1.0 - luma))];
            }
            Self::GaussianBlur { .. }
            | Self::Sharpen { .. }
            | Self::Mosaic { .. }
            | Self::FocusLines { .. }
            | Self::SpeedLines { .. }
            | Self::Tone { .. } => rgb,
        };
        [byte(out[0]), byte(out[1]), byte(out[2]), a]
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
