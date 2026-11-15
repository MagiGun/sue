//! Anti-overflow: the paint stays inside the shape the stroke starts in,
//! as a reference layer's lines enclose it. The region is found once, at
//! the stroke's first point, from a snapshot of the reference layer
//! (D21), and each pixel of the stroke is laid by how much of it lies in
//! the region.

use std::collections::VecDeque;

use sue_core::{PixelFormat, RasterTile2D};

use crate::raster::PixelRect;

/// Where anti-overflow lets paint land: 1 inside the region, 0 outside,
/// over a rectangle of layer pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    rect: PixelRect,
    mask: Vec<f32>,
}

impl Region {
    /// The region around layer pixel `start` that the lines of
    /// `reference` (whose pixel `(0, 0)` is layer pixel `origin`)
    /// enclose: every pixel reachable from `start`, side by side, without
    /// crossing a pixel whose alpha reaches `threshold` (0 to 1), and
    /// then one pixel more into the lines, so the paint meets each line
    /// without a gap. A start on a line or outside the reference gives an
    /// empty region (nothing is laid); a reference that is not a picture
    /// with alpha gives `None`.
    #[must_use]
    pub fn enclosing(
        reference: &RasterTile2D,
        origin: [i32; 2],
        start: [i32; 2],
        threshold: f32,
    ) -> Option<Self> {
        let (w, h) = (reference.width() as i32, reference.height() as i32);
        let alpha: Box<dyn Fn(usize) -> f32> = match reference.format() {
            PixelFormat::Rgba8 => {
                let bytes = reference.bytes()?;
                Box::new(move |i| f32::from(bytes[i * 4 + 3]) / 255.0)
            }
            PixelFormat::RgbaF32 => {
                let samples = reference.f32_samples()?;
                Box::new(move |i| samples[i * 4 + 3])
            }
            _ => return None,
        };
        let rect = PixelRect {
            x0: origin[0],
            y0: origin[1],
            x1: origin[0] + w,
            y1: origin[1] + h,
        };
        let mut mask = vec![0.0_f32; (w.max(0) * h.max(0)) as usize];
        let threshold = if threshold.is_finite() {
            threshold.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let (sx, sy) = (start[0] - origin[0], start[1] - origin[1]);
        let open = |x: i32, y: i32| alpha((y * w + x) as usize) < threshold;
        if sx < 0 || sy < 0 || sx >= w || sy >= h || !open(sx, sy) {
            return Some(Self { rect, mask });
        }
        // Spans of open pixels, filled row by row.
        let mut queue = VecDeque::from([(sx, sy)]);
        while let Some((x, y)) = queue.pop_front() {
            let row = (y * w) as usize;
            if mask[row + x as usize] > 0.0 {
                continue;
            }
            let mut lo = x;
            while lo > 0 && open(lo - 1, y) && mask[row + (lo - 1) as usize] == 0.0 {
                lo -= 1;
            }
            let mut hi = x;
            while hi + 1 < w && open(hi + 1, y) && mask[row + (hi + 1) as usize] == 0.0 {
                hi += 1;
            }
            for px in lo..=hi {
                mask[row + px as usize] = 1.0;
            }
            for ny in [y - 1, y + 1] {
                if ny < 0 || ny >= h {
                    continue;
                }
                let nrow = (ny * w) as usize;
                let mut px = lo;
                while px <= hi {
                    if open(px, ny) && mask[nrow + px as usize] == 0.0 {
                        queue.push_back((px, ny));
                        // One seed for each run of open pixels.
                        while px <= hi && open(px, ny) {
                            px += 1;
                        }
                    } else {
                        px += 1;
                    }
                }
            }
        }
        // One pixel into the lines.
        let inside = mask.clone();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if inside[i] > 0.0 {
                    continue;
                }
                let near = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && inside[(ny * w + nx) as usize] > 0.0
                });
                if near {
                    mask[i] = 1.0;
                }
            }
        }
        Some(Self { rect, mask })
    }

    /// How much of layer pixel `(x, y)` lies in the region, 0 or 1.
    #[must_use]
    pub fn at(&self, x: i32, y: i32) -> f32 {
        let r = self.rect;
        if x < r.x0 || y < r.y0 || x >= r.x1 || y >= r.y1 {
            return 0.0;
        }
        self.mask[((y - r.y0) * (r.x1 - r.x0) + (x - r.x0)) as usize]
    }

    /// The pixels in the region.
    #[must_use]
    pub fn count(&self) -> usize {
        self.mask.iter().filter(|&&m| m > 0.0).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 40 by 30 clear reference with a ring of lines from 10 to 29 in
    /// x and 5 to 24 in y, two pixels thick.
    fn ring() -> RasterTile2D {
        let mut raster = RasterTile2D::new(40, 30, PixelFormat::Rgba8).expect("a raster");
        let bytes = raster.bytes_mut().expect("bytes");
        for y in 5..25 {
            for x in 10..30 {
                let edge = !(12..=27).contains(&x) || !(7..=22).contains(&y);
                if edge {
                    bytes[((y * 40 + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        raster
    }

    #[test]
    fn the_region_is_what_the_lines_enclose_and_reaches_into_them() {
        let region = Region::enclosing(&ring(), [100, 50], [120, 65], 0.5).expect("a region");
        assert_eq!(region.at(120, 65), 1.0);
        // Into the line by one pixel, not across it.
        assert_eq!(region.at(111, 65), 1.0);
        assert_eq!(region.at(110, 65), 0.0);
        assert_eq!(region.at(105, 65), 0.0);
        // The inside is 16 by 16, plus a pixel of line all round.
        assert_eq!(region.count(), 16 * 16 + 4 * 16);
        // Starting outside the ring gives the outside.
        let outside = Region::enclosing(&ring(), [100, 50], [101, 51], 0.5).expect("a region");
        assert_eq!(outside.at(120, 65), 0.0);
        assert_eq!(outside.at(139, 79), 1.0);
        // On a line: nothing.
        let none = Region::enclosing(&ring(), [100, 50], [110, 65], 0.5).expect("a region");
        assert_eq!(none.count(), 0);
        let gray = RasterTile2D::new(4, 4, PixelFormat::Gray8).expect("a mask");
        assert!(Region::enclosing(&gray, [0, 0], [1, 1], 0.5).is_none());
    }
}
