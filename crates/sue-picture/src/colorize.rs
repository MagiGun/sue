//! Edit, Colorize with a hint image: the areas the line art closes are
//! filled from the colors of a hint layer's strokes, here on the device
//! with no model and nothing sent anywhere.
//!
//! Each pixel takes the color of the hint stroke it can reach most
//! cheaply, where crossing a line costs much more than a step through an
//! open area: a stroke inside an area fills that area, and an area with
//! no stroke of its own takes the nearest one through the gaps of its
//! lines. The result is softened by the blur strength, and lies under
//! the line art.
//!
//! Colorize everything without hints (Clip Studio Paint's "Colorize
//! all", which needs a trained model) is not built (Q5 of the v0.0.8
//! plan).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// What a step into a pixel costs: 1 in the open, and this much more on
/// a line of full ink.
const LINE_COST: f32 = 400.0;

/// How much of a line a pixel of `line` (straight RGBA) is, 0 to 1: dark
/// and opaque is a line, light or clear is not.
fn ink(line: [u8; 4]) -> f32 {
    let alpha = f32::from(line[3]) / 255.0;
    let light =
        (0.299 * f32::from(line[0]) + 0.587 * f32::from(line[1]) + 0.114 * f32::from(line[2]))
            / 255.0;
    (alpha * (1.0 - light) * 1.6).clamp(0.0, 1.0)
}

/// Colors `lines` (the line art, straight RGBA, `width` by `height`)
/// from the strokes of `hints` (the same size): an opaque picture of
/// flat colors, softened by `blur` pixels. `None` when the hint layer
/// has no stroke.
#[must_use]
pub fn colorize(lines: &[u8], hints: &[u8], width: u32, height: u32, blur: f32) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    let count = w * h;
    if lines.len() < count * 4 || hints.len() < count * 4 || count == 0 {
        return None;
    }
    let step: Vec<u32> = lines
        .as_chunks::<4>()
        .0
        .iter()
        .take(count)
        .map(|pixel| (1.0 + ink(*pixel) * LINE_COST).round() as u32)
        .collect();
    let mut cost = vec![u32::MAX; count];
    let mut color = vec![[0_u8; 3]; count];
    let mut queue = BinaryHeap::new();
    for (index, pixel) in hints.as_chunks::<4>().0.iter().take(count).enumerate() {
        if pixel[3] >= 128 {
            cost[index] = 0;
            color[index] = [pixel[0], pixel[1], pixel[2]];
            queue.push(Reverse((0_u32, index)));
        }
    }
    if queue.is_empty() {
        return None;
    }
    while let Some(Reverse((spent, index))) = queue.pop() {
        if spent > cost[index] {
            continue;
        }
        let (x, y) = (index % w, index / w);
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < w).then(|| index + 1),
            (y > 0).then(|| index - w),
            (y + 1 < h).then(|| index + w),
        ];
        for next in neighbours.into_iter().flatten() {
            let reach = spent.saturating_add(step[next]);
            if reach < cost[next] {
                cost[next] = reach;
                color[next] = color[index];
                queue.push(Reverse((reach, next)));
            }
        }
    }
    let mut out: Vec<u8> = color
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect();
    if blur > 0.05 {
        crate::filters::blur_pixels(&mut out, width, height, blur);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 30 by 20 picture: a black line down the middle, a hint stroke
    /// of red on the left and of blue on the right.
    fn two_areas() -> (Vec<u8>, Vec<u8>, u32, u32) {
        let (w, h) = (30_u32, 20_u32);
        let mut lines = vec![0_u8; (w * h * 4) as usize];
        let mut hints = vec![0_u8; (w * h * 4) as usize];
        for y in 0..h {
            let at = ((y * w + 15) * 4) as usize;
            lines[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
        for y in 8..12 {
            for (x, rgb) in [(4, [255, 0, 0]), (25, [0, 0, 255])] {
                let at = ((y * w + x) * 4) as usize;
                hints[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        (lines, hints, w, h)
    }

    #[test]
    fn each_area_takes_the_color_of_its_own_hint() {
        let (lines, hints, w, h) = two_areas();
        let out = colorize(&lines, &hints, w, h, 0.0).unwrap();
        let at = |x: u32, y: u32| {
            let index = ((y * w + x) * 4) as usize;
            [out[index], out[index + 1], out[index + 2], out[index + 3]]
        };
        // Far corners, past the hints, keep to their side of the line.
        assert_eq!(at(0, 0), [255, 0, 0, 255]);
        assert_eq!(at(14, 19), [255, 0, 0, 255]);
        assert_eq!(at(16, 0), [0, 0, 255, 255]);
        assert_eq!(at(29, 19), [0, 0, 255, 255]);
    }

    #[test]
    fn an_area_without_a_hint_takes_the_nearest_through_a_gap() {
        let (mut lines, mut hints, w, h) = two_areas();
        // No blue hint; a gap in the line at the top.
        for pixel in hints.as_chunks_mut::<4>().0 {
            if pixel[2] == 255 {
                *pixel = [0, 0, 0, 0];
            }
        }
        let gap = (15 * 4) as usize;
        lines[gap..gap + 4].fill(0);
        let out = colorize(&lines, &hints, w, h, 0.0).unwrap();
        let right = ((10 * w + 28) * 4) as usize;
        assert_eq!(&out[right..right + 3], &[255, 0, 0]);
        // No hint at all: nothing to color from.
        let empty = vec![0_u8; hints.len()];
        assert_eq!(colorize(&lines, &empty, w, h, 0.0), None);
        // A picture of the wrong size.
        assert_eq!(colorize(&lines[..8], &hints, w, h, 0.0), None);
    }

    #[test]
    fn blur_softens_where_the_areas_meet() {
        let (lines, hints, w, h) = two_areas();
        let sharp = colorize(&lines, &hints, w, h, 0.0).unwrap();
        let soft = colorize(&lines, &hints, w, h, 3.0).unwrap();
        let at = ((10 * w + 14) * 4) as usize;
        assert_eq!(sharp[at + 2], 0);
        assert!(soft[at + 2] > 0, "blue reaches across the line");
    }
}
