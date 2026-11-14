//! A tip picture and its mip pyramid.

/// One level of the pyramid: coverage, 0 to 1, row by row.
#[derive(Clone, Debug, PartialEq)]
struct Level {
    width: usize,
    height: usize,
    texels: Vec<f32>,
}

impl Level {
    /// The texel at `(x, y)`; nothing outside the picture.
    fn at(&self, x: i64, y: i64, wrap: bool) -> f32 {
        let x = if wrap {
            x.rem_euclid(self.width as i64)
        } else if x < 0 || x >= self.width as i64 {
            return 0.0;
        } else {
            x
        };
        if y < 0 || y >= self.height as i64 {
            return 0.0;
        }
        self.texels[y as usize * self.width + x as usize]
    }

    /// Bilinear coverage at `(x, y)` in this level's texels (texel
    /// centers at `i + 0.5`).
    fn sample(&self, x: f32, y: f32, wrap: bool) -> f32 {
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (wx, wy) = (fx - x0, fy - y0);
        let (ix, iy) = (x0 as i64, y0 as i64);
        let top = self.at(ix, iy, wrap) * (1.0 - wx) + self.at(ix + 1, iy, wrap) * wx;
        let bottom = self.at(ix, iy + 1, wrap) * (1.0 - wx) + self.at(ix + 1, iy + 1, wrap) * wx;
        top * (1.0 - wy) + bottom * wy
    }

    /// The next level: half the size (rounded up), spanning exactly the
    /// same picture. Each texel is the mean of the area of this level it
    /// covers, so an odd side is shared out over the texels rather than
    /// padded with empty ones. That keeps every level's extent, and so a
    /// ribbon's period, the picture's own: a padded level would read the
    /// empty padding at every repeat and band the ribbon.
    fn halved(&self) -> Self {
        let (width, height) = (self.width.div_ceil(2), self.height.div_ceil(2));
        // Across, row by row; then down, column by column.
        let mut across = Vec::with_capacity(width * self.height);
        for row in self.texels.chunks(self.width) {
            across.extend(shrink(row.iter().copied(), self.width, width));
        }
        let mut texels = vec![0.0; width * height];
        for x in 0..width {
            let column = (0..self.height).map(|y| across[y * width + x]);
            for (y, value) in shrink(column, self.height, height).into_iter().enumerate() {
                texels[y * width + x] = value;
            }
        }
        Self {
            width,
            height,
            texels,
        }
    }
}

/// `values` (`from` of them) box-filtered down to `to` (at most `from`),
/// each output the mean of the stretch of the input it spans: output `j`
/// covers inputs `j * from / to .. (j + 1) * from / to`, parts of an
/// input counted by how much of it lies inside.
fn shrink(values: impl Iterator<Item = f32>, from: usize, to: usize) -> Vec<f32> {
    let input: Vec<f32> = values.collect();
    let ratio = from as f32 / to as f32;
    (0..to)
        .map(|j| {
            let (start, end) = (j as f32 * ratio, (j + 1) as f32 * ratio);
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).min(from);
            let sum: f32 = (first..last)
                .map(|i| {
                    let lo = start.max(i as f32);
                    let hi = end.min((i + 1) as f32);
                    input[i] * (hi - lo).max(0.0)
                })
                .sum();
            sum / ratio
        })
        .collect()
}

/// A tip picture as coverage, with its mip pyramid: each level half the
/// size of the one before, down to one texel. A dab or ribbon reads the
/// level whose texels are about one screen pixel, blending the two
/// nearest, so a large picture stamped small is filtered rather than
/// point-sampled and does not alias or shimmer as the size changes.
///
/// It holds coverage only. The picture's colors (a tip that keeps its
/// colors) and its file stay with the brush (P3-7).
#[derive(Clone, Debug, PartialEq)]
pub struct TipImage {
    levels: Vec<Level>,
}

impl TipImage {
    /// The picture of `width` by `height` texels whose coverage is
    /// `coverage`, row by row, 0 for none and 255 for full. `None` for an
    /// empty size or when the bytes do not fill it.
    #[must_use]
    pub fn from_coverage(width: usize, height: usize, coverage: &[u8]) -> Option<Self> {
        let texels: Vec<f32> = coverage.iter().map(|&c| f32::from(c) / 255.0).collect();
        Self::from_texels(width, height, texels)
    }

    /// As [`Self::from_coverage`], from coverage 0 to 1 (held to that
    /// range).
    #[must_use]
    pub fn from_texels(width: usize, height: usize, texels: Vec<f32>) -> Option<Self> {
        if width == 0 || height == 0 || width.checked_mul(height)? != texels.len() {
            return None;
        }
        let texels = texels
            .into_iter()
            .map(|t| {
                if t.is_finite() {
                    t.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect();
        let mut levels = vec![Level {
            width,
            height,
            texels,
        }];
        while let Some(last) = levels.last() {
            if last.width == 1 && last.height == 1 {
                break;
            }
            let next = last.halved();
            levels.push(next);
        }
        Some(Self { levels })
    }

    /// The picture's width, in texels.
    #[must_use]
    pub fn width(&self) -> usize {
        self.levels[0].width
    }

    /// The picture's height, in texels.
    #[must_use]
    pub fn height(&self) -> usize {
        self.levels[0].height
    }

    /// How many levels the pyramid has, the picture itself included.
    #[must_use]
    pub fn levels(&self) -> usize {
        self.levels.len()
    }

    /// The coverage at `(x, y)` in the picture's own texels, read where
    /// one screen pixel spans `footprint` texels: the picture itself,
    /// bilinear, for a footprint up to 1, else a blend of the two levels
    /// whose texels are nearest that size. With `wrap` the picture
    /// repeats along `x` (a ribbon).
    ///
    /// The level is chosen by comparing the footprint with powers of two,
    /// not by a logarithm, so it is the same on every platform.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32, footprint: f32, wrap: bool) -> f32 {
        if footprint.is_nan() || footprint <= 1.0 {
            return self.levels[0].sample(x, y, wrap);
        }
        let mut level = 0;
        let mut scale = 1.0_f32;
        while level + 1 < self.levels.len() && scale * 2.0 <= footprint {
            level += 1;
            scale *= 2.0;
        }
        let here = self.read(level, x, y, wrap);
        if level + 1 == self.levels.len() {
            return here;
        }
        // Between this level (texels of about `scale`) and the next
        // (about twice).
        let blend = ((footprint - scale) / scale).clamp(0.0, 1.0);
        if blend <= 0.0 {
            return here;
        }
        let next = self.read(level + 1, x, y, wrap);
        here + (next - here) * blend
    }

    /// Level `level` read at `(x, y)` in the picture's own texels: every
    /// level spans the whole picture, so the point is scaled by how many
    /// of the level's texels fit in the picture each way.
    fn read(&self, level: usize, x: f32, y: f32, wrap: bool) -> f32 {
        let (base, this) = (&self.levels[0], &self.levels[level]);
        let sx = this.width as f32 / base.width as f32;
        let sy = this.height as f32 / base.height as f32;
        this.sample(x * sx, y * sy, wrap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pyramid_halves_down_to_one_texel_and_keeps_the_mean() {
        let image = TipImage::from_coverage(5, 3, &[255; 15]).expect("a picture");
        let sizes: Vec<(usize, usize)> = image.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(5, 3), (3, 2), (2, 1), (1, 1)]);
        let even = TipImage::from_coverage(
            4,
            4,
            &[
                255, 0, 255, 0, 0, 255, 0, 255, 255, 0, 255, 0, 0, 255, 0, 255,
            ],
        )
        .expect("a checkerboard");
        assert!(
            even.levels[1]
                .texels
                .iter()
                .all(|&t| (t - 0.5).abs() < 1.0e-6)
        );
        assert!((even.levels[2].texels[0] - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn every_level_spans_the_picture_so_a_repeat_has_no_seam() {
        // Full pictures of awkward widths, read wrapped along their
        // middle row at every size: full everywhere, at every repeat.
        for width in [3_usize, 5, 6, 7, 12, 20] {
            let image = TipImage::from_coverage(width, 9, &vec![255; width * 9]).expect("full");
            for footprint in [1.0_f32, 1.3, 2.0, 2.9, 4.0, 7.5, 16.0] {
                for step in 0..200 {
                    let x = step as f32 * 0.173;
                    let v = image.sample(x, 4.5, footprint, true);
                    assert!(
                        (v - 1.0).abs() < 1.0e-5,
                        "width {width}, footprint {footprint}, at {x}: {v}"
                    );
                }
            }
            // And an odd side is shared out, not padded: still full.
            assert!(
                image
                    .levels
                    .iter()
                    .all(|l| l.texels.iter().all(|&t| (t - 1.0).abs() < 1.0e-5))
            );
        }
        // A stripe pattern keeps its mean at every level.
        let stripes: Vec<u8> = (0..7 * 3)
            .map(|i| if i % 7 < 3 { 255 } else { 0 })
            .collect();
        let image = TipImage::from_coverage(7, 3, &stripes).expect("stripes");
        for level in &image.levels {
            let mean = level.texels.iter().sum::<f32>() / level.texels.len() as f32;
            assert!((mean - 3.0 / 7.0).abs() < 1.0e-5, "{mean}");
        }
    }

    #[test]
    fn a_bad_size_is_no_picture() {
        assert!(TipImage::from_coverage(0, 4, &[]).is_none());
        assert!(TipImage::from_coverage(2, 2, &[0; 3]).is_none());
    }

    #[test]
    fn sampling_is_bilinear_and_empty_outside() {
        let image = TipImage::from_coverage(2, 1, &[0, 255]).expect("a picture");
        assert_eq!(image.sample(0.5, 0.5, 1.0, false), 0.0);
        assert_eq!(image.sample(1.5, 0.5, 1.0, false), 1.0);
        assert!((image.sample(1.0, 0.5, 1.0, false) - 0.5).abs() < 1.0e-6);
        assert_eq!(image.sample(-3.0, 0.5, 1.0, false), 0.0);
        // Wrapped, the left neighbour of texel 0 is texel 1.
        assert!((image.sample(0.0, 0.5, 1.0, true) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn a_fine_pattern_read_small_is_its_mean() {
        // One-texel stripes read with a footprint of 8 texels.
        let stripes: Vec<u8> = (0..64 * 64)
            .map(|i| if i % 2 == 0 { 255 } else { 0 })
            .collect();
        let image = TipImage::from_coverage(64, 64, &stripes).expect("stripes");
        for x in 0..32 {
            let v = image.sample(20.0 + x as f32 * 0.37, 30.0, 8.0, false);
            assert!((v - 0.5).abs() < 0.02, "{v} at step {x}");
        }
    }
}
