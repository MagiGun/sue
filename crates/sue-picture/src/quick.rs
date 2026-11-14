//! Clip Studio Paint's Quick Filters and Quick Effects: one-click looks
//! picked from a gallery, each with a strength. Quick Filters change the
//! colors (Vivid, Warm, Sepia and the like); Quick Effects restyle the
//! picture (Glow, Halftone, Outline and the like), built on Sue Paint's
//! own filters.

use crate::filters::Filter;

/// How many looks each gallery has.
pub const LOOKS: u8 = 10;

/// What people read for look `index`.
#[must_use]
pub fn name(effect: bool, index: u8) -> String {
    match (effect, index) {
        (false, 0) => tr!("quick-vivid"),
        (false, 1) => tr!("quick-soft"),
        (false, 2) => tr!("quick-warm"),
        (false, 3) => tr!("quick-cool"),
        (false, 4) => tr!("quick-sepia"),
        (false, 5) => tr!("quick-monochrome"),
        (false, 6) => tr!("quick-vintage"),
        (false, 7) => tr!("quick-fade"),
        (false, 8) => tr!("quick-high-contrast"),
        (false, _) => tr!("quick-night"),
        (true, 0) => tr!("quick-glow"),
        (true, 1) => tr!("quick-halftone"),
        (true, 2) => tr!("quick-outline"),
        (true, 3) => tr!("quick-pencil"),
        (true, 4) => tr!("quick-pixel-art"),
        (true, 5) => tr!("quick-film-grain"),
        (true, 6) => tr!("quick-vignette"),
        (true, 7) => tr!("quick-poster"),
        (true, 8) => tr!("quick-watercolor"),
        (true, _) => tr!("quick-emboss"),
    }
}

/// Applies look `index` to `pixels` (RGBA, `width` by `height`) at
/// `strength` (0 keeps the picture, 1 is the whole look). Transparent
/// pixels stay transparent.
pub fn apply(pixels: &mut [u8], width: u32, height: u32, effect: bool, index: u8, strength: f32) {
    let original = pixels.to_vec();
    if effect {
        restyle(pixels, width, height, index);
    } else {
        recolor(pixels, width, height, index);
    }
    let strength = strength.clamp(0.0, 1.0);
    for (out, was) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(original.as_chunks::<4>().0)
    {
        for channel in 0..3 {
            let (a, b) = (f32::from(was[channel]), f32::from(out[channel]));
            out[channel] = (a + (b - a) * strength).round().clamp(0.0, 255.0) as u8;
        }
        // Effects may draw where there was nothing (a glow); the colors
        // keep the layer's own coverage.
        if !effect {
            out[3] = was[3];
        } else {
            let (a, b) = (f32::from(was[3]), f32::from(out[3]));
            out[3] = (a + (b - a) * strength).round() as u8;
        }
    }
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]
}

/// The color looks, pixel by pixel.
fn recolor(pixels: &mut [u8], width: u32, height: u32, index: u8) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    for (at, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let rgb = [0, 1, 2].map(|channel| f32::from(pixel[channel]) / 255.0);
        let gray = luma(rgb);
        let saturate = |rgb: [f32; 3], by: f32| rgb.map(|value| gray + (value - gray) * by);
        let contrast = |rgb: [f32; 3], by: f32| rgb.map(|value| 0.5 + (value - 0.5) * by);
        let out = match index {
            // Vivid: richer and a little punchier.
            0 => contrast(saturate(rgb, 1.45), 1.08),
            // Soft: gentler contrast, lifted shadows.
            1 => contrast(rgb, 0.8).map(|value| value * 0.94 + 0.05),
            // Warm and Cool.
            2 => [rgb[0] * 1.08 + 0.02, rgb[1] * 1.02, rgb[2] * 0.88],
            3 => [rgb[0] * 0.9, rgb[1] * 1.0, rgb[2] * 1.1 + 0.02],
            // Sepia.
            4 => [
                0.393 * rgb[0] + 0.769 * rgb[1] + 0.189 * rgb[2],
                0.349 * rgb[0] + 0.686 * rgb[1] + 0.168 * rgb[2],
                0.272 * rgb[0] + 0.534 * rgb[1] + 0.131 * rgb[2],
            ],
            5 => [gray; 3],
            // Vintage: a faded sepia that darkens toward the corners.
            6 => {
                let (x, y) = (
                    (at as u32 % width) as f32 / w,
                    (at as u32 / width) as f32 / h,
                );
                let edge = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 1.4;
                let shade = 1.0 - 0.35 * edge.powi(2);
                let toned = [gray * 1.07 + 0.04, gray * 0.98 + 0.03, gray * 0.82 + 0.05];
                [0, 1, 2].map(|channel| {
                    (rgb[channel] * 0.4 + toned[channel] * 0.6) * 0.92 * shade + 0.06
                })
            }
            // Fade: lifted blacks, a little less color.
            7 => saturate(rgb, 0.75).map(|value| value * 0.82 + 0.13),
            // High contrast: an S curve.
            8 => rgb.map(|value| {
                let t = value.clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t) * 0.6 + contrast([t; 3], 1.3)[0] * 0.4
            }),
            // Night: dark and blue.
            _ => {
                let dim = saturate(rgb, 0.55);
                [dim[0] * 0.55, dim[1] * 0.65, dim[2] * 0.85 + 0.06]
            }
        };
        for channel in 0..3 {
            pixel[channel] = (out[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// The restyling looks, on the whole picture.
fn restyle(pixels: &mut [u8], width: u32, height: u32, index: u8) {
    let side = width.max(height) as f32;
    let run = |filter: Filter, pixels: &mut [u8]| filter.apply(pixels, width, height, None, [0, 0]);
    match index {
        // Glow: a soft copy laid over in Screen.
        0 => {
            let mut soft = pixels.to_vec();
            run(
                Filter::GaussianBlur {
                    radius: (side / 60.0).max(2.0),
                },
                &mut soft,
            );
            for (pixel, glow) in pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(soft.as_chunks::<4>().0)
            {
                for channel in 0..3 {
                    let (a, b) = (
                        f32::from(pixel[channel]) / 255.0,
                        f32::from(glow[channel]) / 255.0,
                    );
                    pixel[channel] = ((1.0 - (1.0 - a) * (1.0 - b * 0.8)) * 255.0).round() as u8;
                }
                pixel[3] = pixel[3].max(glow[3]);
            }
        }
        1 => halftone(pixels, width, height, (side / 120.0).max(4.0)),
        // Outline: the picture's lines alone.
        2 => run(
            Filter::LineExtraction {
                sensitivity: 0.5,
                black: 0.0,
                smooth: 1.0,
            },
            pixels,
        ),
        // Pencil: soft gray lines.
        3 => {
            run(
                Filter::LineExtraction {
                    sensitivity: 0.65,
                    black: 0.0,
                    smooth: 0.5,
                },
                pixels,
            );
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel[0] = pixel[0].max(70);
                pixel[1] = pixel[1].max(70);
                pixel[2] = pixel[2].max(76);
                pixel[3] = (f32::from(pixel[3]) * 0.8) as u8;
            }
        }
        // Pixel art: big pixels in few colors.
        4 => {
            run(
                Filter::Mosaic {
                    size: (side / 96.0).max(2.0) as u32,
                },
                pixels,
            );
            run(Filter::Posterize { levels: 6 }, pixels);
        }
        // Film grain: grain and a slight fade.
        5 => {
            run(Filter::Noise { amount: 0.22 }, pixels);
            recolor(pixels, width, height, 7);
        }
        // Vignette: darker toward the corners.
        6 => {
            let (w, h) = (width.max(1) as f32, height.max(1) as f32);
            for (at, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (
                    (at as u32 % width) as f32 / w,
                    (at as u32 / width) as f32 / h,
                );
                let edge = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 1.41;
                let shade = (1.0 - 0.7 * edge.powf(2.2)).clamp(0.0, 1.0);
                for value in &mut pixel[..3] {
                    *value = (f32::from(*value) * shade).round() as u8;
                }
            }
        }
        // Poster: few, bold colors.
        7 => {
            recolor(pixels, width, height, 0);
            run(Filter::Posterize { levels: 4 }, pixels);
        }
        // Watercolor: smoothed into soft patches.
        8 => {
            run(
                Filter::GaussianBlur {
                    radius: (side / 300.0).max(1.0),
                },
                pixels,
            );
            run(Filter::Posterize { levels: 10 }, pixels);
            recolor(pixels, width, height, 1);
        }
        // Emboss: light from the top left.
        _ => emboss(pixels, width, height),
    }
}

/// Halftone: dots on a grid `cell` pixels apart, each as large as its
/// cell is dark.
fn halftone(pixels: &mut [u8], width: u32, height: u32, cell: f32) {
    let source = pixels.to_vec();
    let at = |x: u32, y: u32| ((y.min(height - 1) * width + x.min(width - 1)) * 4) as usize;
    for y in 0..height {
        for x in 0..width {
            let (cx, cy) = (
                ((x as f32 / cell).floor() + 0.5) * cell,
                ((y as f32 / cell).floor() + 0.5) * cell,
            );
            let sample = at(cx as u32, cy as u32);
            let rgb = [0, 1, 2].map(|channel| f32::from(source[sample + channel]) / 255.0);
            let dark = 1.0 - luma(rgb);
            let radius = cell * 0.7 * dark.sqrt();
            let inside =
                ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() <= radius;
            let here = at(x, y);
            let value = if inside { 20 } else { 250 };
            pixels[here..here + 3].copy_from_slice(&[value; 3]);
            pixels[here + 3] = source[here + 3];
        }
    }
}

/// Emboss: each pixel lit by the difference from its upper left
/// neighbour, on middle gray.
fn emboss(pixels: &mut [u8], width: u32, height: u32) {
    let source = pixels.to_vec();
    for y in 0..height {
        for x in 0..width {
            let here = ((y * width + x) * 4) as usize;
            let near = ((y.saturating_sub(1) * width + x.saturating_sub(1)) * 4) as usize;
            let lum =
                |at: usize| luma([0, 1, 2].map(|channel| f32::from(source[at + channel]) / 255.0));
            let value = (0.5 + (lum(here) - lum(near)) * 2.0).clamp(0.0, 1.0);
            let byte = (value * 255.0).round() as u8;
            pixels[here..here + 3].copy_from_slice(&[byte; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Vec<u8> {
        // A left to right ramp, red to blue, fully covered.
        (0..32 * 24)
            .flat_map(|index| {
                let x = (index % 32) as u8 * 8;
                [255 - x, 80, x, 255]
            })
            .collect()
    }

    #[test]
    fn every_look_changes_the_picture_and_strength_zero_keeps_it() {
        for effect in [false, true] {
            for index in 0..LOOKS {
                let mut pixels = picture();
                apply(&mut pixels, 32, 24, effect, index, 1.0);
                assert_ne!(pixels, picture(), "{} changes it", name(effect, index));
                let mut kept = picture();
                apply(&mut kept, 32, 24, effect, index, 0.0);
                assert_eq!(kept, picture(), "{} at 0 keeps it", name(effect, index));
            }
        }
        // Monochrome leaves no color.
        let mut gray = picture();
        apply(&mut gray, 32, 24, false, 5, 1.0);
        assert!(
            gray.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2])
        );
        // Colors keep the layer's coverage.
        let mut clear = vec![0_u8; 8 * 8 * 4];
        apply(&mut clear, 8, 8, false, 2, 1.0);
        assert!(clear.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    }
}
