//! One stamp of the tip, and the walk that spaces stamps along a path.

use super::{Disc, PixelRect, RoundTip, Shape, TipImage};

/// A turn of the tip, as its cosine and sine. The engine takes a turn
/// this way, not as an angle, so no platform's `sin` and `cos` reach a
/// pixel; replay (P3-8) brings its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    cos: f32,
    sin: f32,
}

impl Turn {
    /// No turn.
    pub const NONE: Self = Self { cos: 1.0, sin: 0.0 };

    /// The turn whose cosine and sine are in proportion to `cos` and
    /// `sin`; no turn when both are zero or not numbers.
    #[must_use]
    pub fn new(cos: f32, sin: f32) -> Self {
        let length = (cos * cos + sin * sin).sqrt();
        if length.is_finite() && length > 1.0e-12 {
            Self {
                cos: cos / length,
                sin: sin / length,
            }
        } else {
            Self::NONE
        }
    }

    /// The turn that points along `(dx, dy)` (a tip that follows the
    /// line); no turn for a zero direction.
    #[must_use]
    pub fn along(dx: f32, dy: f32) -> Self {
        Self::new(dx, dy)
    }

    /// The cosine.
    #[must_use]
    pub const fn cos(&self) -> f32 {
        self.cos
    }

    /// The sine.
    #[must_use]
    pub const fn sin(&self) -> f32 {
        self.sin
    }

    /// `v` in the tip's own frame (turned back by this turn).
    fn back(&self, v: [f32; 2]) -> [f32; 2] {
        [
            self.cos * v[0] + self.sin * v[1],
            -self.sin * v[0] + self.cos * v[1],
        ]
    }

    /// The half width and half height of the box that holds a rectangle
    /// of half sides `hx` by `hy` turned by this turn.
    fn extent(&self, hx: f32, hy: f32) -> [f32; 2] {
        let (c, s) = (self.cos.abs(), self.sin.abs());
        [c * hx + s * hy, s * hx + c * hy]
    }
}

impl Default for Turn {
    fn default() -> Self {
        Self::NONE
    }
}

/// What a dab stamps.
#[derive(Clone, Copy, Debug)]
pub enum DabShape<'a> {
    /// The round tip, made an ellipse by the dab's thickness.
    Round(RoundTip),
    /// A tip picture. Its longer side spans the dab's diameter.
    Image(&'a TipImage),
}

/// One stamp of the tip: a disc (where, how large, how strong), the
/// tip's thickness (its height over its width, 0 to 1) and its turn.
/// It is placed with subpixel precision: the center is a real number and
/// every pixel is sampled at its own center, so a dab moved by a quarter
/// pixel moves its paint by a quarter pixel.
#[derive(Clone, Copy, Debug)]
pub struct Dab<'a> {
    disc: Disc,
    aspect: f32,
    turn: Turn,
    shape: DabShape<'a>,
}

impl<'a> Dab<'a> {
    /// A dab of `shape` over `disc` with thickness `aspect` and turn
    /// `turn`. A round dab thinner than [`Disc::THINNEST`] across is
    /// drawn that thick and lighter by as much, as a thin line is.
    #[must_use]
    pub fn new(disc: Disc, aspect: f32, turn: Turn, shape: DabShape<'a>) -> Self {
        let mut disc = disc.drawn(false);
        let aspect = if aspect.is_finite() {
            aspect.clamp(0.01, 1.0)
        } else {
            1.0
        };
        let aspect = if matches!(shape, DabShape::Round(_)) && disc.radius * aspect < Disc::THINNEST
        {
            let held = Disc::THINNEST / disc.radius;
            disc.strength *= disc.radius * aspect / Disc::THINNEST;
            held.min(1.0)
        } else {
            aspect
        };
        Self {
            disc,
            aspect,
            turn,
            shape,
        }
    }

    /// A round dab: a disc of the round tip `tip`.
    #[must_use]
    pub fn round(disc: Disc, tip: RoundTip) -> Self {
        Self::new(disc, 1.0, Turn::NONE, DabShape::Round(tip))
    }

    /// The disc the dab is stamped over, as drawn.
    #[must_use]
    pub const fn disc(&self) -> Disc {
        self.disc
    }

    /// Pixels per texel of a picture, and the picture's half sides in
    /// pixels before the turn.
    fn picture_scale(&self, image: &TipImage) -> (f32, [f32; 2]) {
        let longest = image.width().max(image.height()) as f32;
        let scale = 2.0 * self.disc.radius / longest;
        (
            scale,
            [
                image.width() as f32 * scale * 0.5,
                image.height() as f32 * scale * self.aspect * 0.5,
            ],
        )
    }
}

impl Shape for Dab<'_> {
    fn bounds(&self) -> PixelRect {
        let [hx, hy, margin] = match self.shape {
            DabShape::Round(tip) => [
                self.disc.radius,
                self.disc.radius * self.aspect,
                tip.reach() + 0.5,
            ],
            DabShape::Image(image) => {
                let (scale, [hx, hy]) = self.picture_scale(image);
                // Bilinear reading spreads half a texel; a level up the
                // pyramid up to two pixels.
                [hx, hy, 2.0 + scale]
            }
        };
        let [ex, ey] = self.turn.extent(hx, hy);
        let at = self.disc.at;
        PixelRect::around(
            [at[0] - ex - margin, at[1] - ey - margin],
            [at[0] + ex + margin, at[1] + ey + margin],
        )
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        let at = self.disc.at;
        let [ux, uy] = self.turn.back([p[0] - at[0], p[1] - at[1]]);
        let profile = match self.shape {
            DabShape::Round(tip) => {
                let rx = self.disc.radius;
                let ry = rx * self.aspect;
                if self.aspect >= 1.0 {
                    tip.disc_at((ux * ux + uy * uy).sqrt(), rx)
                } else {
                    let (nx, ny) = (ux / rx, uy / ry);
                    let share = (nx * nx + ny * ny).sqrt();
                    if !tip.is_hard() {
                        // In pixels across the narrower side.
                        tip.soft_at(share * ry, ry)
                    } else if share < 1.0e-6 {
                        1.0
                    } else {
                        // The distance to the ellipse to first order:
                        // the level's excess over its gradient.
                        let (gx, gy) = (nx / rx, ny / ry);
                        let gradient = (gx * gx + gy * gy).sqrt() / share;
                        tip.hard_at((share - 1.0) / gradient, ry)
                    }
                }
            }
            DabShape::Image(image) => {
                let (scale, _) = self.picture_scale(image);
                let tx = ux / scale + image.width() as f32 * 0.5;
                let ty = uy / (scale * self.aspect) + image.height() as f32 * 0.5;
                image.sample(tx, ty, 1.0 / (scale * self.aspect), false)
            }
        };
        profile * self.disc.strength
    }
}

/// Spaces dabs along a path: one at the path's start, then one every
/// `gap(radius)` pixels of arc length, the gap read from the radius
/// where the last dab fell (as the size changes, so does the spacing,
/// which the legacy engine took from the larger end of each segment).
/// What is left of a gap carries over to the next segment, so how the
/// path is cut into segments does not move a dab.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DabWalker {
    /// Arc length still to go to the next dab, or `None` before the
    /// first.
    to_next: Option<f32>,
}

impl DabWalker {
    /// The smallest gap: a walk never stamps more than four dabs a pixel.
    pub const SMALLEST_GAP: f32 = 0.25;

    /// A walk that has placed nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { to_next: None }
    }

    /// Walks from `a` to `b`, the disc changing evenly on the way, and
    /// hands each dab's disc to `stamp`.
    pub fn walk(
        &mut self,
        a: Disc,
        b: Disc,
        gap: impl Fn(f32) -> f32,
        mut stamp: impl FnMut(Disc),
    ) {
        let next_gap = |radius: f32| {
            let g = gap(radius);
            if g.is_finite() {
                g.max(Self::SMALLEST_GAP)
            } else {
                Self::SMALLEST_GAP
            }
        };
        let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let at = |t: f32| Disc {
            at: [a.at[0] + dx * t, a.at[1] + dy * t],
            radius: a.radius + (b.radius - a.radius) * t,
            strength: a.strength + (b.strength - a.strength) * t,
        };
        let mut position = self.to_next.unwrap_or(0.0);
        if !length.is_finite() {
            return;
        }
        while position <= length {
            let t = if length > 0.0 { position / length } else { 0.0 };
            let disc = at(t);
            stamp(disc);
            position += next_gap(disc.radius);
        }
        self.to_next = Some(position - length);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dabs_fall_at_the_same_places_however_the_path_is_cut() {
        let gap = |r: f32| r * 0.5;
        let (a, b) = (
            Disc::new([0.0, 0.0], 4.0, 1.0),
            Disc::new([101.0, 0.0], 4.0, 1.0),
        );
        let mut whole = Vec::new();
        DabWalker::new().walk(a, b, gap, |d| whole.push(d.at[0]));
        let mut cut = Vec::new();
        let mut walker = DabWalker::new();
        let mut from = a;
        for x in [13.0, 13.5, 40.0, 77.7, 101.0] {
            let to = Disc::new([x, 0.0], 4.0, 1.0);
            walker.walk(from, to, gap, |d| cut.push(d.at[0]));
            from = to;
        }
        assert_eq!(whole.len(), 51);
        assert_eq!(whole.len(), cut.len());
        for (w, c) in whole.iter().zip(&cut) {
            assert!((w - c).abs() < 1.0e-3, "{w} against {c}");
        }
    }

    #[test]
    fn the_gap_follows_the_local_size() {
        let mut gaps = Vec::new();
        let mut last = None;
        DabWalker::new().walk(
            Disc::new([0.0, 0.0], 2.0, 1.0),
            Disc::new([200.0, 0.0], 20.0, 1.0),
            |r| r,
            |d| {
                if let Some(l) = last {
                    gaps.push((d.at[0] - l, d.radius));
                }
                last = Some(d.at[0]);
            },
        );
        let first = gaps.first().expect("gaps").0;
        let final_gap = gaps.last().expect("gaps").0;
        assert!(first < 3.0 && final_gap > 15.0, "{first} then {final_gap}");
    }

    #[test]
    fn a_tap_is_one_dab_and_a_zero_gap_is_held() {
        let mut count = 0;
        let here = Disc::new([5.0, 5.0], 3.0, 1.0);
        DabWalker::new().walk(here, here, |_| 0.0, |_| count += 1);
        assert_eq!(count, 1);
        let mut count = 0;
        DabWalker::new().walk(
            here,
            Disc::new([6.0, 5.0], 3.0, 1.0),
            |_| f32::NAN,
            |_| count += 1,
        );
        assert_eq!(count, 5);
    }

    #[test]
    fn a_turned_ellipse_lies_along_its_turn() {
        let tip = RoundTip::hard(1.0);
        let disc = Disc::new([0.0, 0.0], 10.0, 1.0);
        let flat = Dab::new(disc, 0.3, Turn::NONE, DabShape::Round(tip));
        assert_eq!(flat.coverage([9.0, 0.0]), 1.0);
        assert_eq!(flat.coverage([0.0, 9.0]), 0.0);
        let upright = Dab::new(disc, 0.3, Turn::new(0.0, 1.0), DabShape::Round(tip));
        assert_eq!(upright.coverage([0.0, 9.0]), 1.0);
        assert_eq!(upright.coverage([9.0, 0.0]), 0.0);
        // Half covered on the rim, along both axes.
        assert!((flat.coverage([10.0, 0.0]) - 0.5).abs() < 1.0e-4);
        assert!((flat.coverage([0.0, 3.0]) - 0.5).abs() < 1.0e-3);
        // Solid all along its long axis, not an outline.
        for k in 0..20 {
            let x = k as f32 * 0.45;
            assert_eq!(flat.coverage([x, 0.0]), 1.0, "at {x}");
        }
    }

    #[test]
    fn a_thin_ellipse_keeps_its_weight_as_lighter_ink() {
        let tip = RoundTip::hard(1.0);
        let thin = Dab::new(
            Disc::new([0.0, 0.0], 10.0, 1.0),
            0.035,
            Turn::NONE,
            DabShape::Round(tip),
        );
        assert!((thin.disc().strength - 0.5).abs() < 1.0e-4);
        assert!((thin.aspect * 10.0 - Disc::THINNEST).abs() < 1.0e-4);
    }
}
