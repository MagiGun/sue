//! # Rasterizer primitives
//!
//! The shapes a stroke is made of, as coverage: for every pixel, a share
//! from 0 to 1 of how much paint the shape lays there. They know nothing
//! of layers, colors or undo; a [`crate::buffer::StrokeBuffer`] collects
//! their coverage and the row kernels in [`kernel`] lay it on a layer.
//!
//! - [`Capsule`]: the round tip swept from one [`Disc`] to the next, with
//!   the exact distance to the hull of the two circles (the "uneven
//!   capsule"), so a line that thickens or thins has straight, tangent
//!   flanks and a joint between two capsules is exactly the disc both
//!   share: no bead and no notch, however far apart the samples are.
//! - [`Dab`]: one stamp of the tip, round (with hardness, a thickness and
//!   a turn) or a picture ([`TipImage`]) read from a mip pyramid with
//!   subpixel placement and rotation, so a large picture stamped small
//!   does not alias. [`DabWalker`] spaces dabs along a path by the local
//!   size with a carried remainder.
//! - [`Ribbon`]: a picture laid along a path and bent with it, across the
//!   line its height, along it repeated every tile length.
//!
//! Pixel `(x, y)` covers the square from `(x, y)` to `(x + 1, y + 1)`, so
//! its center is `(x + 0.5, y + 0.5)`; every primitive is sampled there.
//!
//! No value here depends on the thread count, the frame rate or the
//! platform's math library: the primitives use `+`, `-`, `*`, `/` and
//! `sqrt` only (all exactly rounded by IEEE 754), take a turn as its
//! cosine and sine rather than an angle, and pick mip levels without a
//! logarithm. Replay (P3-8) relies on that.

mod capsule;
mod dab;
mod image;
pub mod kernel;
mod ribbon;
mod tip;

pub use capsule::Capsule;
pub use dab::{Dab, DabShape, DabWalker, Turn};
pub use image::TipImage;
pub use ribbon::{Ribbon, RibbonPoint};
pub use tip::RoundTip;

/// A circle of the stroke: where the tip is, its radius in pixels and
/// its strength (the most coverage it lays, 0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    /// The center, in layer pixels.
    pub at: [f32; 2],
    /// The radius, in pixels.
    pub radius: f32,
    /// The most coverage the tip lays here, 0 to 1.
    pub strength: f32,
}

impl Disc {
    /// The smallest radius a tip is drawn at. A thinner line keeps its
    /// weight as lighter ink instead of breaking into dots (the legacy
    /// engine's rule, `paint/brush.rs`).
    pub const THINNEST: f32 = 0.7;

    /// A disc at `at` of `radius` and `strength`.
    #[must_use]
    pub const fn new(at: [f32; 2], radius: f32, strength: f32) -> Self {
        Self {
            at,
            radius,
            strength,
        }
    }

    /// The disc as it is drawn: never thinner than [`Self::THINNEST`],
    /// a thinner one lighter by as much; with `at_least_one` ("never
    /// thinner than a pixel") that wide at full strength. The strength
    /// is held to 0 to 1 and a radius or strength that is not a number
    /// becomes 0.
    #[must_use]
    pub fn drawn(self, at_least_one: bool) -> Self {
        let radius = if self.radius.is_finite() {
            self.radius.max(0.0)
        } else {
            0.0
        };
        let mut strength = if self.strength.is_finite() {
            self.strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let radius = if at_least_one {
            radius.max(Self::THINNEST)
        } else if radius < Self::THINNEST {
            strength *= radius / Self::THINNEST;
            Self::THINNEST
        } else {
            radius
        };
        Self {
            at: self.at,
            radius,
            strength,
        }
    }
}

/// A rectangle of layer pixels: `x0..x1` by `y0..y1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PixelRect {
    /// The first column.
    pub x0: i32,
    /// The first row.
    pub y0: i32,
    /// One past the last column.
    pub x1: i32,
    /// One past the last row.
    pub y1: i32,
}

impl PixelRect {
    /// The pixels whose centers can fall inside `[min, max]`, widened by
    /// one pixel each way. Far out of range coordinates are held to the
    /// range of the layer coordinates the buffer uses.
    #[must_use]
    pub fn around(min: [f32; 2], max: [f32; 2]) -> Self {
        const LIMIT: f32 = 1.0e8;
        let low = |v: f32| (v.clamp(-LIMIT, LIMIT).floor() as i32).saturating_sub(1);
        let high = |v: f32| (v.clamp(-LIMIT, LIMIT).ceil() as i32).saturating_add(1);
        Self {
            x0: low(min[0]),
            y0: low(min[1]),
            x1: high(max[0]),
            y1: high(max[1]),
        }
    }

    /// `true` when the rectangle holds no pixel.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
}

/// What a primitive hands the buffer: where it may lay coverage, the
/// columns of each row it can reach, and its coverage at a pixel center.
pub trait Shape {
    /// The pixels the shape may touch.
    fn bounds(&self) -> PixelRect;

    /// The span of `x` (pixel center coordinates) on the row whose
    /// centers are at `y` outside which the shape lays nothing, or
    /// `None` when the row is empty. The default is the whole bounds.
    fn span(&self, y: f32) -> Option<(f32, f32)> {
        let _ = y;
        let bounds = self.bounds();
        Some((bounds.x0 as f32, bounds.x1 as f32))
    }

    /// The coverage at the pixel center `p`, 0 to 1.
    fn coverage(&self, p: [f32; 2]) -> f32;
}

/// `lo <= c * x + e <= hi` as a range of `x`; the whole line when `c` is
/// zero and `e` lies in the range, `None` when nothing does.
fn solve_band(c: f32, e: f32, lo: f32, hi: f32) -> Option<(f32, f32)> {
    if c.abs() < 1.0e-9 {
        return (lo <= e && e <= hi).then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let (p, q) = ((lo - e) / c, (hi - e) / c);
    Some((p.min(q), p.max(q)))
}

/// The columns at row `y` of the round-ended band of radius `reach`
/// around the segment from `a` to `b` (a capsule of even width), which
/// holds every pixel a capsule or ribbon on that segment can touch.
fn segment_span(a: [f32; 2], b: [f32; 2], reach: f32, y: f32) -> Option<(f32, f32)> {
    let disc = |c: [f32; 2]| {
        let dy = y - c[1];
        let h = reach * reach - dy * dy;
        (h >= 0.0).then(|| {
            let h = h.sqrt();
            (c[0] - h, c[0] + h)
        })
    };
    let mut span: Option<(f32, f32)> = None;
    let mut join = |piece: Option<(f32, f32)>| {
        if let Some((lo, hi)) = piece {
            span = Some(match span {
                Some((l, h)) => (l.min(lo), h.max(hi)),
                None => (lo, hi),
            });
        }
    };
    join(disc(a));
    join(disc(b));
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = (dx * dx + dy * dy).sqrt();
    if length > 1.0e-6 {
        // Across the segment within `reach` and along it within its
        // length, both linear in `x` along the row.
        let (nx, ny) = (-dy / length, dx / length);
        let (ux, uy) = (dx / length, dy / length);
        let ry = y - a[1];
        let across = solve_band(nx, ny * ry, -reach, reach);
        let along = solve_band(ux, uy * ry, 0.0, length);
        if let (Some((l0, h0)), Some((l1, h1))) = (across, along) {
            let (lo, hi) = (l0.max(l1), h0.min(h1));
            if lo <= hi {
                join(Some((a[0] + lo, a[0] + hi)));
            }
        }
    }
    span
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thin_disc_keeps_its_weight_as_lighter_ink() {
        let thin = Disc::new([0.0, 0.0], 0.35, 1.0).drawn(false);
        assert_eq!(thin.radius, Disc::THINNEST);
        assert!((thin.strength - 0.5).abs() < 1.0e-6);
        let held = Disc::new([0.0, 0.0], 0.1, 1.0).drawn(true);
        assert_eq!(held.strength, 1.0);
        assert_eq!(held.radius, Disc::THINNEST);
        let broken = Disc::new([0.0, 0.0], f32::NAN, f32::INFINITY).drawn(false);
        assert_eq!(broken.strength, 0.0);
    }

    #[test]
    fn the_segment_span_holds_every_point_within_reach() {
        let (a, b, reach) = ([3.0, 4.0], [40.0, 22.5], 6.5);
        for row in -10..40 {
            let y = row as f32 + 0.5;
            let span = segment_span(a, b, reach, y);
            for col in -20..60 {
                let x = col as f32 + 0.5;
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let t = (((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                let (cx, cy) = (a[0] + dx * t, a[1] + dy * t);
                let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt();
                if d <= reach - 1.0e-3 {
                    let (lo, hi) = span.expect("a row that holds a near point");
                    assert!(
                        lo <= x && x <= hi,
                        "({x}, {y}) at {d} is outside {lo}..{hi}"
                    );
                }
                if let Some((lo, hi)) = span
                    && lo + 1.0e-3 < x
                    && x < hi - 1.0e-3
                {
                    assert!(
                        d <= reach + 1.0e-3,
                        "({x}, {y}) inside the span is {d} away"
                    );
                }
            }
        }
    }
}
