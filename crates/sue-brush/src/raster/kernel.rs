//! Row kernels: a row of stroke coverage laid on a row of layer pixels.
//!
//! A layer stores straight (not premultiplied) RGBA. The kernels load a
//! chunk of [`LANES`] pixels into premultiplied `f32` lanes, compose
//! there, where "over" and "erase" are plain multiply-adds on all four
//! channels alike, and store back only the pixels the stroke covers, so
//! a pixel the stroke does not reach keeps its exact bytes, even its
//! color under zero alpha. The chunks have a fixed width and no branch
//! in the composing loops, so they autovectorise in safe Rust (D3).
//!
//! The kernels are generic over the layer's sample type ([`Channel`]):
//! 8-bit today, with 16-bit and float targets behind the same code (D3
//! keeps painting 8-bit sRGB in 0.0.8). Colors are in the layer's own
//! encoding (sRGB values from 0 to 1), as the legacy engine mixed them.

/// Coverage is stored in 16 bits: this is full coverage.
pub const FULL: u16 = u16::MAX;

/// Pixels per chunk.
pub const LANES: usize = 8;

/// One sample of a layer pixel.
pub trait Channel: Copy + Send + Sync + 'static {
    /// The sample as a share of full, 0 to 1 (more for float HDR).
    fn to_unit(self) -> f32;
    /// The sample nearest `value`.
    fn from_unit(value: f32) -> Self;
}

impl Channel for u8 {
    fn to_unit(self) -> f32 {
        f32::from(self) / 255.0
    }

    fn from_unit(value: f32) -> Self {
        // Rounded to nearest; a value that is not a number is 0.
        let v = if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        };
        (v * 255.0 + 0.5) as u8
    }
}

impl Channel for u16 {
    fn to_unit(self) -> f32 {
        f32::from(self) / 65_535.0
    }

    fn from_unit(value: f32) -> Self {
        let v = if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        };
        (v * 65_535.0 + 0.5) as u16
    }
}

impl Channel for f32 {
    fn to_unit(self) -> f32 {
        self
    }

    fn from_unit(value: f32) -> Self {
        value
    }
}

/// What a stroke lays on the layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    /// Paint of `color` (straight RGB, 0 to 1), as opaque as `opacity`
    /// at full coverage. However often a stroke crosses itself, it lays
    /// at most this much (its coverage is combined in the stroke buffer
    /// before it reaches the layer).
    Paint {
        /// The color, straight RGB from 0 to 1.
        color: [f32; 3],
        /// The stroke's opacity, 0 to 1.
        opacity: f32,
    },
    /// Removes `opacity` of the layer's alpha at full coverage.
    Erase {
        /// The stroke's opacity, 0 to 1.
        opacity: f32,
    },
}

impl Ink {
    /// The ink's opacity, held to 0 to 1.
    fn opacity(&self) -> f32 {
        let opacity = match *self {
            Self::Paint { opacity, .. } | Self::Erase { opacity } => opacity,
        };
        if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Loads straight RGBA samples as premultiplied `f32`.
pub fn premultiply_row<C: Channel>(straight: &[C], premultiplied: &mut [f32]) {
    let (pixels, _) = straight.as_chunks::<4>();
    let (lanes, _) = premultiplied.as_chunks_mut::<4>();
    for (s, p) in pixels.iter().zip(lanes) {
        let a = s[3].to_unit();
        p[0] = s[0].to_unit() * a;
        p[1] = s[1].to_unit() * a;
        p[2] = s[2].to_unit() * a;
        p[3] = a;
    }
}

/// Stores premultiplied `f32` pixels back as straight samples, only where
/// `coverage` is above zero. A pixel left with no alpha keeps its color
/// samples, as a straight layer does.
pub fn unpremultiply_row<C: Channel>(premultiplied: &[f32], coverage: &[u16], straight: &mut [C]) {
    let (lanes, _) = premultiplied.as_chunks::<4>();
    let (pixels, _) = straight.as_chunks_mut::<4>();
    for ((p, s), &c) in lanes.iter().zip(pixels).zip(coverage) {
        if c == 0 {
            continue;
        }
        let a = p[3];
        if a > 0.0 {
            let inverse = 1.0 / a;
            s[0] = C::from_unit(p[0] * inverse);
            s[1] = C::from_unit(p[1] * inverse);
            s[2] = C::from_unit(p[2] * inverse);
        }
        s[3] = C::from_unit(a);
    }
}

/// Lays the premultiplied `color` (alpha in `color[3]`) over
/// premultiplied pixels, each by its coverage.
pub fn over_row(premultiplied: &mut [f32], coverage: &[u16], color: [f32; 4]) {
    let (lanes, _) = premultiplied.as_chunks_mut::<4>();
    for (p, &c) in lanes.iter_mut().zip(coverage) {
        // A division, so full coverage is exactly 1.
        let s = f32::from(c) / f32::from(FULL);
        let keep = 1.0 - color[3] * s;
        p[0] = color[0] * s + p[0] * keep;
        p[1] = color[1] * s + p[1] * keep;
        p[2] = color[2] * s + p[2] * keep;
        p[3] = color[3] * s + p[3] * keep;
    }
}

/// Removes `opacity` of premultiplied pixels, each by its coverage.
pub fn erase_row(premultiplied: &mut [f32], coverage: &[u16], opacity: f32) {
    let (lanes, _) = premultiplied.as_chunks_mut::<4>();
    for (p, &c) in lanes.iter_mut().zip(coverage) {
        let keep = 1.0 - opacity * (f32::from(c) / f32::from(FULL));
        p[0] *= keep;
        p[1] *= keep;
        p[2] *= keep;
        p[3] *= keep;
    }
}

/// Lays `ink` on a row of straight RGBA samples, pixel `i` by
/// `coverage[i]`. The row holds four samples a pixel; extra coverage or
/// pixels beyond the shorter of the two are left alone.
pub fn paint_row<C: Channel>(row: &mut [C], coverage: &[u16], ink: Ink) {
    let opacity = ink.opacity();
    let pixels = (row.len() / 4).min(coverage.len());
    let mut lanes = [0.0_f32; 4 * LANES];
    for (chunk, covered) in row[..pixels * 4]
        .chunks_mut(4 * LANES)
        .zip(coverage[..pixels].chunks(LANES))
    {
        if covered.iter().all(|&c| c == 0) {
            continue;
        }
        let lanes = &mut lanes[..chunk.len()];
        premultiply_row(chunk, lanes);
        match ink {
            Ink::Paint { color, .. } => {
                let clean = |v: f32| {
                    if v.is_finite() {
                        v.clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                };
                let premultiplied = [
                    clean(color[0]) * opacity,
                    clean(color[1]) * opacity,
                    clean(color[2]) * opacity,
                    opacity,
                ];
                over_row(lanes, covered, premultiplied);
            }
            Ink::Erase { .. } => erase_row(lanes, covered, opacity),
        }
        unpremultiply_row(lanes, covered, chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK_HALF: Ink = Ink::Paint {
        color: [0.0, 0.0, 0.0],
        opacity: 0.5,
    };

    #[test]
    fn uncovered_pixels_keep_their_exact_samples() {
        let mut row: Vec<u8> = vec![200, 100, 50, 0, 1, 2, 3, 4, 255, 255, 255, 255];
        let before = row.clone();
        paint_row(&mut row, &[0, 0, 0], BLACK_HALF);
        assert_eq!(row, before);
        // A covered neighbour does not disturb them either.
        paint_row(&mut row, &[0, FULL, 0], BLACK_HALF);
        assert_eq!(row[..4], before[..4]);
        assert_eq!(row[8..], before[8..]);
        assert_eq!(row[4..7], [0, 0, 0]);
        assert!((129..=130).contains(&row[7]), "{}", row[7]);
    }

    #[test]
    fn half_black_over_white_is_mid_gray_for_every_sample_type() {
        let mut bytes = vec![255_u8; 4 * 11];
        paint_row(&mut bytes, &[FULL; 11], BLACK_HALF);
        assert!(bytes.chunks(4).all(|p| p == [128, 128, 128, 255]));
        let mut words = vec![65_535_u16; 4 * 3];
        paint_row(&mut words, &[FULL; 3], BLACK_HALF);
        assert!(
            words
                .chunks(4)
                .all(|p| p == [32_768, 32_768, 32_768, 65_535])
        );
        let mut floats = vec![1.0_f32; 4 * 3];
        paint_row(&mut floats, &[FULL; 3], BLACK_HALF);
        assert!(floats.chunks(4).all(|p| p == [0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn paint_on_a_clear_layer_keeps_its_straight_color() {
        let mut row = vec![0_u8; 4 * 2];
        let ink = Ink::Paint {
            color: [1.0, 0.6, 0.25],
            opacity: 0.4,
        };
        paint_row(&mut row, &[FULL, FULL / 2], ink);
        assert_eq!(row[..4], [255, 153, 64, 102]);
        assert_eq!(row[4..8], [255, 153, 64, 51]);
    }

    #[test]
    fn over_matches_the_straight_formula() {
        // Straight "over" worked out the long way for a spread of pixels.
        for &(dst, cov) in &[
            ([10_u8, 200, 90, 77], 30_000_u16),
            ([255, 0, 0, 255], 65_535),
            ([7, 9, 11, 3], 999),
        ] {
            let ink = [0.2_f32, 0.6, 0.9];
            let opacity = 0.7;
            let mut row = dst.to_vec();
            paint_row(
                &mut row,
                &[cov],
                Ink::Paint {
                    color: ink,
                    opacity,
                },
            );
            let s = opacity * f32::from(cov) / 65_535.0;
            let da = f32::from(dst[3]) / 255.0;
            let oa = s + da * (1.0 - s);
            for c in 0..3 {
                let d = f32::from(dst[c]) / 255.0;
                let want = (ink[c] * s + d * da * (1.0 - s)) / oa;
                assert!(
                    (f32::from(row[c]) - want * 255.0).abs() <= 0.5 + 1.0e-3,
                    "{dst:?} {cov} channel {c}"
                );
            }
            assert!((f32::from(row[3]) - oa * 255.0).abs() <= 0.5 + 1.0e-3);
        }
    }

    #[test]
    fn erasing_lowers_alpha_and_keeps_the_color() {
        let mut row = vec![10_u8, 20, 30, 200, 10, 20, 30, 200];
        paint_row(&mut row, &[FULL, FULL / 2], Ink::Erase { opacity: 1.0 });
        assert_eq!(row[..4], [10, 20, 30, 0]);
        assert_eq!(row[4..], [10, 20, 30, 100]);
    }

    #[test]
    fn a_ragged_row_is_cut_to_what_both_hold() {
        let mut row = vec![255_u8; 4 * 9 + 2];
        paint_row(&mut row, &[FULL; 20], BLACK_HALF);
        assert!(row[..36].chunks(4).all(|p| p == [128, 128, 128, 255]));
        assert_eq!(row[36..], [255, 255]);
    }
}
