//! Area fills: the paint bucket's flood fill, masked fills and gradients.
//! They work on a copy of a layer's pixels (straight RGBA8); the studio
//! turns the change into one undo step with [`crate::doc::rewrite`].

use crate::select::Selection;

/// The area a flood fill from `seed` reaches in `reference` (straight RGBA8
/// rows of `size`): one coverage byte per pixel. Pixels count as the same
/// when no premultiplied channel differs by more than `tolerance` (0 to 1).
/// The area grows by one pixel so it tucks under anti-aliased edges.
#[must_use]
pub fn flood(reference: &[u8], size: [u32; 2], seed: [u32; 2], tolerance: f32) -> Vec<u8> {
    let [width, height] = size;
    let count = width as usize * height as usize;
    let mut mask = vec![0_u8; count];
    if seed[0] >= width || seed[1] >= height || reference.len() < count * 4 {
        return mask;
    }
    let premultiplied = |index: usize| {
        let p = &reference[index * 4..index * 4 + 4];
        let alpha = u32::from(p[3]);
        [
            u32::from(p[0]) * alpha / 255,
            u32::from(p[1]) * alpha / 255,
            u32::from(p[2]) * alpha / 255,
            alpha,
        ]
    };
    let target = premultiplied((seed[1] * width + seed[0]) as usize);
    let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as u32;
    let same = |index: usize| {
        premultiplied(index)
            .iter()
            .zip(target)
            .all(|(a, b)| a.abs_diff(b) <= limit)
    };
    let mut stack = vec![(seed[0], seed[1])];
    while let Some((x, y)) = stack.pop() {
        let row = (y * width) as usize;
        if mask[row + x as usize] != 0 || !same(row + x as usize) {
            continue;
        }
        let mut left = x;
        while left > 0 && mask[row + left as usize - 1] == 0 && same(row + left as usize - 1) {
            left -= 1;
        }
        let mut right = x;
        while right + 1 < width
            && mask[row + right as usize + 1] == 0
            && same(row + right as usize + 1)
        {
            right += 1;
        }
        for fill in left..=right {
            mask[row + fill as usize] = 255;
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny < height {
                    let below = (ny * width + fill) as usize;
                    if mask[below] == 0 && same(below) {
                        stack.push((fill, ny));
                    }
                }
            }
        }
    }
    grow(&mask, width, height)
}

/// The mask grown by one pixel in every direction.
fn grow(mask: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = mask.to_vec();
    for y in 0..height {
        for x in 0..width {
            if mask[(y * width + x) as usize] != 0 {
                continue;
            }
            let touches = (y.saturating_sub(1)..=(y + 1).min(height - 1)).any(|ny| {
                (x.saturating_sub(1)..=(x + 1).min(width - 1))
                    .any(|nx| mask[(ny * width + nx) as usize] != 0)
            });
            if touches {
                out[(y * width + x) as usize] = 255;
            }
        }
    }
    out
}

/// Lays `color` (sRGB 0 to 1) at `opacity` over the layer pixels covered by
/// `mask` (one byte per picture pixel). The layer is `layer_size` and sits
/// at `offset` in a picture of `picture_size`. `alpha_lock` keeps alpha.
#[allow(clippy::too_many_arguments, reason = "a fill names its whole setting")]
pub fn paint_mask(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    mask: &[u8],
    picture_size: [u32; 2],
    color: [f32; 3],
    opacity: f32,
    alpha_lock: bool,
) {
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
            if px < 0
                || py < 0
                || px >= i64::from(picture_size[0])
                || py >= i64::from(picture_size[1])
            {
                continue;
            }
            let cover = f32::from(mask[(py as u32 * picture_size[0] + px as u32) as usize]) / 255.0;
            if cover <= 0.0 {
                continue;
            }
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let old = crate::doc::unit(&pixels[at..at + 4]);
            let amount = cover * opacity;
            let out = if alpha_lock {
                [
                    old[0] + (color[0] - old[0]) * amount,
                    old[1] + (color[1] - old[1]) * amount,
                    old[2] + (color[2] - old[2]) * amount,
                    old[3],
                ]
            } else {
                sue_model::BlendMode::Normal.composite(old, [color[0], color[1], color[2], amount])
            };
            pixels[at..at + 4].copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

/// A linear (or radial) gradient from `from` to `to` (picture pixels),
/// `start` to `end` (straight RGBA 0 to 1), laid over the layer's pixels
/// inside `selection` (or everywhere).
#[allow(
    clippy::too_many_arguments,
    reason = "a gradient names its whole setting"
)]
pub fn gradient(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    [from, to]: [[f32; 2]; 2],
    [start, end]: [[f32; 4]; 2],
    radial: bool,
    selection: Option<&Selection>,
    alpha_lock: bool,
) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length_sq = (dx * dx + dy * dy).max(1e-6);
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (
                (i64::from(x) + offset[0]) as f32 + 0.5,
                (i64::from(y) + offset[1]) as f32 + 0.5,
            );
            let cover = match selection {
                Some(selection) if px >= 0.0 && py >= 0.0 => selection.at(px as u32, py as u32),
                Some(_) => 0.0,
                None => 1.0,
            };
            if cover <= 0.0 {
                continue;
            }
            let t = if radial {
                (((px - from[0]).powi(2) + (py - from[1]).powi(2)) / length_sq).sqrt()
            } else {
                ((px - from[0]) * dx + (py - from[1]) * dy) / length_sq
            }
            .clamp(0.0, 1.0);
            let mut color = [0.0; 4];
            for channel in 0..4 {
                color[channel] = start[channel] + (end[channel] - start[channel]) * t;
            }
            color[3] *= cover;
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let old = crate::doc::unit(&pixels[at..at + 4]);
            let out = if alpha_lock {
                let amount = color[3];
                [
                    old[0] + (color[0] - old[0]) * amount,
                    old[1] + (color[1] - old[1]) * amount,
                    old[2] + (color[2] - old[2]) * amount,
                    old[3],
                ]
            } else {
                sue_model::BlendMode::Normal.composite(old, color)
            };
            pixels[at..at + 4].copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 6 by 4 picture split by a black vertical line at x = 2.
    fn walled() -> Vec<u8> {
        let mut pixels = Vec::new();
        for _y in 0..4 {
            for x in 0..6 {
                pixels.extend(if x == 2 { [0, 0, 0, 255] } else { [0, 0, 0, 0] });
            }
        }
        pixels
    }

    #[test]
    fn floods_stop_at_lines_and_tuck_under_them() {
        let pixels = walled();
        let mask = flood(&pixels, [6, 4], [0, 0], 0.0);
        let row: Vec<u8> = mask[..6].to_vec();
        assert_eq!(row, [255, 255, 255, 0, 0, 0], "left side plus one pixel");
        let right = flood(&pixels, [6, 4], [5, 3], 0.0);
        assert_eq!(&right[18..24], &[0, 0, 255, 255, 255, 255]);
        assert!(flood(&pixels, [6, 4], [9, 9], 0.0).iter().all(|v| *v == 0));
        let all = flood(&pixels, [6, 4], [0, 0], 1.0);
        assert!(
            all.iter().all(|v| *v == 255),
            "full tolerance takes everything"
        );
    }

    #[test]
    fn masked_fills_and_gradients_paint_inside() {
        let mut pixels = vec![0_u8; 4 * 4 * 4];
        let mut mask = vec![0_u8; 16];
        mask[5] = 255;
        paint_mask(
            &mut pixels,
            [4, 4],
            [0, 0],
            &mask,
            [4, 4],
            [1.0, 0.0, 0.0],
            1.0,
            false,
        );
        assert_eq!(&pixels[5 * 4..6 * 4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        let mut ramp = vec![0_u8; 10 * 4];
        gradient(
            &mut ramp,
            [10, 1],
            [0, 0],
            [[0.0, 0.5], [10.0, 0.5]],
            [[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]],
            false,
            None,
            false,
        );
        assert!(ramp[3] > ramp[9 * 4 + 3], "fades out to the right");
        assert!(ramp[3] > 200 && ramp[9 * 4 + 3] < 30);
    }
}
