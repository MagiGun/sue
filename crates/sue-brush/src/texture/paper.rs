//! A paper: a gray height field that repeats in both directions, with a
//! mip pyramid so a paper shown small does not shimmer.

/// The longest side a paper is kept at. A larger picture is scaled down
/// to it when it is made into a paper.
pub const LARGEST: usize = 2048;

/// One level of the pyramid: `width` by `height` heights, row by row.
#[derive(Clone, Debug, PartialEq)]
struct Level {
    width: usize,
    height: usize,
    texels: Vec<f32>,
}

impl Level {
    /// The height at texel `(x, y)`, the field repeating both ways.
    fn at(&self, x: i64, y: i64) -> f32 {
        let x = x.rem_euclid(self.width as i64) as usize;
        let y = y.rem_euclid(self.height as i64) as usize;
        self.texels[y * self.width + x]
    }

    /// Bilinear at `(x, y)` in this level's texels, texel centers at
    /// half steps, repeating.
    fn sample(&self, x: f32, y: f32) -> f32 {
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = self.at(x0, y0) + (self.at(x0 + 1, y0) - self.at(x0, y0)) * tx;
        let bottom = self.at(x0, y0 + 1) + (self.at(x0 + 1, y0 + 1) - self.at(x0, y0 + 1)) * tx;
        top + (bottom - top) * ty
    }

    /// The level half as large each way (a side of 1 stays 1), each
    /// texel the mean of the ones it covers.
    fn halved(&self) -> Self {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let (sx, sy) = (self.width / w, self.height / h);
        let mut texels = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for dy in 0..sy {
                    for dx in 0..sx {
                        sum += self.texels[(y * sy + dy) * self.width + x * sx + dx];
                    }
                }
                texels.push(sum / (sx * sy) as f32);
            }
        }
        Self {
            width: w,
            height: h,
            texels,
        }
    }
}

/// A paper texture: heights from 0 (a valley, where little paint lands)
/// to 1 (a high spot, which catches paint first). It repeats in both
/// directions and is kept with sides that are powers of two, so every
/// level of its pyramid tiles exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper {
    levels: Vec<Level>,
}

impl Paper {
    /// A paper from `width` by `height` heights (0 to 1), row by row. A
    /// side that is not a power of two is scaled, repeating, to the next
    /// one (at most [`LARGEST`]), so the field still tiles without a
    /// seam. `None` for an empty field or a length that does not match.
    #[must_use]
    pub fn from_texels(width: usize, height: usize, texels: &[f32]) -> Option<Self> {
        if width == 0 || height == 0 || texels.len() != width * height {
            return None;
        }
        let clean: Vec<f32> = texels
            .iter()
            .map(|&v| {
                if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.5
                }
            })
            .collect();
        let source = Level {
            width,
            height,
            texels: clean,
        };
        let side = |n: usize| n.next_power_of_two().min(LARGEST);
        let (w, h) = (side(width), side(height));
        let base = if (w, h) == (width, height) {
            source
        } else {
            // Read the source at each new texel's center, scaled; when it
            // shrinks, average its pyramid first would be better, but a
            // paper is at most a factor of two off here, except for very
            // large pictures, which are averaged in blocks below.
            let mut level = source;
            while level.width > 2 * w || level.height > 2 * h {
                level = level.halved();
            }
            let (kx, ky) = (
                level.width as f32 / w as f32,
                level.height as f32 / h as f32,
            );
            let mut texels = Vec::with_capacity(w * h);
            for y in 0..h {
                for x in 0..w {
                    texels.push(level.sample((x as f32 + 0.5) * kx, (y as f32 + 0.5) * ky));
                }
            }
            Level {
                width: w,
                height: h,
                texels,
            }
        };
        let mut levels = vec![base];
        loop {
            let last = &levels[levels.len() - 1];
            if last.width == 1 && last.height == 1 {
                break;
            }
            let next = last.halved();
            levels.push(next);
        }
        Some(Self { levels })
    }

    /// A paper from a gray picture of `width` by `height` bytes (0 is a
    /// valley, 255 a high spot): how a person's own paper picture comes
    /// in, once it is decoded.
    #[must_use]
    pub fn from_gray8(width: usize, height: usize, bytes: &[u8]) -> Option<Self> {
        let texels: Vec<f32> = bytes.iter().map(|&b| f32::from(b) / 255.0).collect();
        Self::from_texels(width, height, &texels)
    }

    /// The width of the paper's own field, in texels.
    #[must_use]
    pub fn width(&self) -> usize {
        self.levels[0].width
    }

    /// The height of the paper's own field, in texels.
    #[must_use]
    pub fn height(&self) -> usize {
        self.levels[0].height
    }

    /// The height at `(x, y)` in the paper's own texels, read where one
    /// pixel spans `footprint` texels: bilinear on the paper itself up to
    /// a footprint of 1, else between the two levels nearest that size.
    /// The level is chosen by comparing with powers of two, not by a
    /// logarithm, so it is the same on every platform.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32, footprint: f32) -> f32 {
        if !x.is_finite() || !y.is_finite() {
            return 0.5;
        }
        if footprint.is_nan() || footprint <= 1.0 {
            return self.levels[0].sample(x, y);
        }
        let mut level = 0;
        let mut scale = 1.0_f32;
        while level + 1 < self.levels.len() && scale * 2.0 <= footprint {
            level += 1;
            scale *= 2.0;
        }
        let here = self.read(level, x, y);
        if level + 1 == self.levels.len() {
            return here;
        }
        let blend = ((footprint - scale) / scale).clamp(0.0, 1.0);
        if blend <= 0.0 {
            return here;
        }
        here + (self.read(level + 1, x, y) - here) * blend
    }

    /// Level `level` read at `(x, y)` in the paper's own texels.
    fn read(&self, level: usize, x: f32, y: f32) -> f32 {
        let (base, this) = (&self.levels[0], &self.levels[level]);
        let sx = this.width as f32 / base.width as f32;
        let sy = this.height as f32 / base.height as f32;
        this.sample(x * sx, y * sy)
    }

    /// The mean height of the whole paper.
    #[must_use]
    pub fn mean(&self) -> f32 {
        self.levels[self.levels.len() - 1].texels[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paper_repeats_without_a_seam_in_both_directions() {
        let texels: Vec<f32> = (0..16 * 8).map(|i| (i % 7) as f32 / 6.0).collect();
        let paper = Paper::from_texels(16, 8, &texels).expect("a paper");
        for &(x, y) in &[(0.3, 0.7), (5.5, 2.25), (15.9, 7.9)] {
            let here = paper.sample(x, y, 1.0);
            assert!((paper.sample(x + 16.0, y, 1.0) - here).abs() < 1.0e-5);
            assert!((paper.sample(x, y - 8.0, 1.0) - here).abs() < 1.0e-5);
        }
    }

    #[test]
    fn an_odd_picture_becomes_a_power_of_two_and_keeps_its_mean() {
        let texels: Vec<f32> = (0..30 * 20)
            .map(|i| ((i * 37) % 11) as f32 / 10.0)
            .collect();
        let mean = texels.iter().sum::<f32>() / texels.len() as f32;
        let paper = Paper::from_texels(30, 20, &texels).expect("a paper");
        assert_eq!((paper.width(), paper.height()), (32, 32));
        assert!(
            (paper.mean() - mean).abs() < 0.05,
            "{} against {mean}",
            paper.mean()
        );
        assert!(Paper::from_texels(0, 4, &[]).is_none());
        assert!(Paper::from_texels(2, 2, &[0.0; 3]).is_none());
    }

    #[test]
    fn a_paper_seen_from_far_is_its_mean() {
        let texels: Vec<f32> = (0..64 * 64)
            .map(|i| if (i % 64 + i / 64) % 2 == 0 { 1.0 } else { 0.0 })
            .collect();
        let paper = Paper::from_texels(64, 64, &texels).expect("a paper");
        assert!((paper.sample(10.3, 20.7, 64.0) - 0.5).abs() < 1.0e-4);
        // Close up it is the checker itself.
        assert!(paper.sample(0.5, 0.5, 1.0) > 0.99);
        assert!(paper.sample(1.5, 0.5, 1.0) < 0.01);
        assert_eq!(paper.sample(f32::NAN, 0.0, 1.0), 0.5);
    }
}
