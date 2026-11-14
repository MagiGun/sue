//! The round tip swept from one disc to the next: the uneven capsule.

use super::{Disc, PixelRect, RoundTip, Shape, segment_span};

/// The round tip swept along a straight segment from disc `a` to disc
/// `b`, its radius and strength changing evenly on the way. Its outline
/// is the two circles and their outer tangents (the convex hull of the
/// circles), and its coverage at a pixel is the most any of the discs
/// along the segment lays there: for a hard tip the rim profile of the
/// exact distance to that outline, for a soft one the profile at the
/// smallest share of a radius any disc reaches the pixel at. Both are
/// found in closed form, without stepping along the segment.
///
/// The legacy engine read the radius at the point of the segment nearest
/// to the pixel (`paint/brush.rs`, the projection approximation), which
/// is not the nearest point of the outline once the radius changes: the
/// flanks of a tapering line bulged and two capsules met with a notch.
/// Here a joint is exactly the disc both capsules share, so a polyline
/// of capsules has no bead and no gap at its samples however sparse
/// they are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    a: Disc,
    b: Disc,
    tip: RoundTip,
}

impl Capsule {
    /// The tip `tip` swept from `a` to `b`. Pass discs that went through
    /// [`Disc::drawn`] (the stroke buffer's painter does).
    #[must_use]
    pub fn new(a: Disc, b: Disc, tip: RoundTip) -> Self {
        Self { a, b, tip }
    }

    /// The first disc.
    #[must_use]
    pub const fn start(&self) -> Disc {
        self.a
    }

    /// The last disc.
    #[must_use]
    pub const fn end(&self) -> Disc {
        self.b
    }

    /// The signed distance in pixels from `p` to the outline (negative
    /// inside), and where along the segment (0 to 1) the nearest disc
    /// lies.
    ///
    /// It is the smallest of `|p - c(t)| - r(t)` over the segment, which
    /// outside the shape is exactly the distance to the union of the
    /// discs. Along the segment's axis that function is convex, and its
    /// one stationary point is found in closed form, so the smallest
    /// value is the stationary point held to the segment.
    #[must_use]
    pub fn distance(&self, p: [f32; 2]) -> (f32, f32) {
        let (a, b) = (self.a, self.b);
        let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
        let (qx, qy) = (p[0] - a.at[0], p[1] - a.at[1]);
        let length_sq = dx * dx + dy * dy;
        let length = length_sq.sqrt();
        if length < 1.0e-6 {
            let d = (qx * qx + qy * qy).sqrt();
            return if b.radius > a.radius {
                (d - b.radius, 1.0)
            } else {
                (d - a.radius, 0.0)
            };
        }
        // `x` along the axis, `y` off it.
        let x = (qx * dx + qy * dy) / length;
        let y = (qx * qx + qy * qy - x * x).max(0.0).sqrt();
        // The radius grows by `slope` per pixel of axis.
        let slope = (b.radius - a.radius) / length;
        let at = |u: f32| {
            let (ex, ey) = (x - u, y);
            (ex * ex + ey * ey).sqrt() - (a.radius + slope * u)
        };
        let u = if slope.abs() < 1.0 {
            // d/du = (u - x) / dist - slope = 0.
            let shift = slope * y / (1.0 - slope * slope).sqrt();
            (x + shift).clamp(0.0, length)
        } else if slope > 0.0 {
            // One circle holds the other: the larger one is the shape.
            length
        } else {
            0.0
        };
        (at(u), u / length)
    }

    /// The smallest share of its own radius at which any disc along the
    /// segment reaches `p`, and where (0 to 1) that disc lies.
    ///
    /// `|p - c(t)| / r(t)` is quasiconvex in `t` (its level sets are the
    /// hulls of scaled circles), with one stationary point, which comes
    /// out of a linear equation; the smallest of it and the two ends is
    /// the answer.
    #[must_use]
    pub fn share(&self, p: [f32; 2]) -> (f32, f32) {
        let (a, b) = (self.a, self.b);
        let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
        let (qx, qy) = (p[0] - a.at[0], p[1] - a.at[1]);
        let grow = b.radius - a.radius;
        let ratio = |t: f32| {
            let (ex, ey) = (qx - dx * t, qy - dy * t);
            (ex * ex + ey * ey).sqrt() / (a.radius + grow * t).max(1.0e-6)
        };
        let mut best = (ratio(0.0), 0.0);
        let end = ratio(1.0);
        if end < best.0 {
            best = (end, 1.0);
        }
        let qd = qx * dx + qy * dy;
        let q2 = qx * qx + qy * qy;
        let denominator = a.radius * (dx * dx + dy * dy) + grow * qd;
        if denominator > 1.0e-12 {
            let t = ((a.radius * qd + grow * q2) / denominator).clamp(0.0, 1.0);
            let inner = ratio(t);
            if inner < best.0 {
                best = (inner, t);
            }
        }
        best
    }
}

impl Shape for Capsule {
    fn bounds(&self) -> PixelRect {
        let reach = self.tip.reach() + 0.5;
        let (a, b) = (self.a, self.b);
        PixelRect::around(
            [
                (a.at[0] - a.radius).min(b.at[0] - b.radius) - reach,
                (a.at[1] - a.radius).min(b.at[1] - b.radius) - reach,
            ],
            [
                (a.at[0] + a.radius).max(b.at[0] + b.radius) + reach,
                (a.at[1] + a.radius).max(b.at[1] + b.radius) + reach,
            ],
        )
    }

    fn span(&self, y: f32) -> Option<(f32, f32)> {
        let reach = self.a.radius.max(self.b.radius) + self.tip.reach() + 0.5;
        segment_span(self.a.at, self.b.at, reach, y)
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        let radius = |t: f32| self.a.radius + (self.b.radius - self.a.radius) * t;
        let (profile, t) = if self.tip.is_hard() {
            let (distance, t) = self.distance(p);
            (self.tip.hard_at(distance, radius(t)), t)
        } else {
            let (share, t) = self.share(p);
            let r = radius(t);
            (self.tip.soft_at(share * r, r), t)
        };
        if profile <= 0.0 {
            return 0.0;
        }
        let strength = self.a.strength + (self.b.strength - self.a.strength) * t;
        profile * strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest of `f(t)` over 20,001 steps of the segment.
    fn brute(capsule: &Capsule, p: [f32; 2], f: impl Fn(f32, f32) -> f32) -> f32 {
        let (a, b) = (capsule.start(), capsule.end());
        (0..=20_000)
            .map(|i| {
                let t = i as f32 / 20_000.0;
                let c = [
                    a.at[0] + (b.at[0] - a.at[0]) * t,
                    a.at[1] + (b.at[1] - a.at[1]) * t,
                ];
                let d = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt();
                f(d, a.radius + (b.radius - a.radius) * t)
            })
            .fold(f32::INFINITY, f32::min)
    }

    fn capsules() -> Vec<Capsule> {
        let tip = RoundTip::default();
        vec![
            Capsule::new(
                Disc::new([10.0, 10.0], 2.0, 1.0),
                Disc::new([60.0, 30.0], 12.0, 1.0),
                tip,
            ),
            Capsule::new(
                Disc::new([50.0, 5.0], 15.0, 1.0),
                Disc::new([20.0, 40.0], 1.0, 1.0),
                tip,
            ),
            Capsule::new(
                Disc::new([30.0, 30.0], 4.0, 1.0),
                Disc::new([33.0, 31.0], 9.0, 1.0),
                tip,
            ),
            Capsule::new(
                Disc::new([30.0, 30.0], 6.0, 1.0),
                Disc::new([30.0, 30.0], 6.0, 1.0),
                tip,
            ),
            Capsule::new(
                Disc::new([5.0, 50.0], 7.0, 1.0),
                Disc::new([70.0, 50.0], 7.0, 1.0),
                tip,
            ),
        ]
    }

    #[test]
    fn the_distance_is_the_nearest_disc_found_by_brute_force() {
        for capsule in capsules() {
            for y in (-10..80).step_by(3) {
                for x in (-10..90).step_by(3) {
                    let p = [x as f32 + 0.5, y as f32 + 0.5];
                    let exact = capsule.distance(p).0;
                    let slow = brute(&capsule, p, |d, r| d - r);
                    assert!(
                        (exact - slow).abs() < 2.0e-3,
                        "{capsule:?} at {p:?}: {exact} against {slow}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_share_is_the_nearest_disc_found_by_brute_force() {
        for capsule in capsules() {
            for y in (-10..80).step_by(3) {
                for x in (-10..90).step_by(3) {
                    let p = [x as f32 + 0.5, y as f32 + 0.5];
                    let exact = capsule.share(p).0;
                    let slow = brute(&capsule, p, |d, r| d / r);
                    assert!(
                        (exact - slow).abs() < 1.0e-3 * slow.max(1.0),
                        "{capsule:?} at {p:?}: {exact} against {slow}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_flanks_are_the_tangents_of_the_two_circles() {
        // Circles of radius 2 at x = 0 and 10 at x = 40: the outer
        // tangent leaves the axis at sin = 8 / 40.
        let capsule = Capsule::new(
            Disc::new([0.0, 0.0], 2.0, 1.0),
            Disc::new([40.0, 0.0], 10.0, 1.0),
            RoundTip::hard(0.0),
        );
        let sin: f32 = 8.0 / 40.0;
        let cos = (1.0 - sin * sin).sqrt();
        // The tangent point on the small circle, and on the large one.
        let touch_a = [-2.0 * sin, 2.0 * cos];
        let touch_b = [40.0 - 10.0 * sin, 10.0 * cos];
        for s in 0..=10 {
            let t = s as f32 / 10.0;
            let p = [
                touch_a[0] + (touch_b[0] - touch_a[0]) * t,
                touch_a[1] + (touch_b[1] - touch_a[1]) * t,
            ];
            assert!(
                capsule.distance(p).0.abs() < 1.0e-3,
                "{p:?} lies on the flank"
            );
        }
    }
}
