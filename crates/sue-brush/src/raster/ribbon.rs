//! A tip picture laid along a path.

use std::ops::Range;

use super::{Disc, PixelRect, Shape, TipImage, segment_span};

/// A point of a ribbon's path: its disc, and its arc length from the
/// stroke's start in pixels, which places the picture along the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibbonPoint {
    /// Where the path is, how wide the ribbon is (its radius) and how
    /// strong.
    pub disc: Disc,
    /// The arc length from the stroke's start, in pixels.
    pub arc: f32,
}

/// A tip picture laid along a piece of a path and bent with it: across
/// the line it spans the ribbon's width (its top on the left of the way
/// the line goes), along the line it repeats every `tile` pixels of arc
/// length. Each pixel reads the picture at the nearest point of the path
/// (outside a bend, at the corner itself, so the outer side of a bend is
/// filled rather than torn open). The path ends square at both ends.
///
/// A long path is drawn in pieces (the committed points of one update,
/// then the tail), since every pixel looks at every segment of its
/// piece. So that the pieces meet without a seam, a piece may carry
/// context segments before and after the ones it paints
/// ([`Self::painting`]): they take part in finding the nearest point but
/// lay nothing. With context of at least twice the ribbon's width plus
/// two pixels of arc each side, every pixel gets the value the whole path
/// gives it, unless the path comes back within that distance of itself.
#[derive(Clone, Debug)]
pub struct Ribbon<'a> {
    points: &'a [RibbonPoint],
    image: &'a TipImage,
    tile: f32,
    paint: Range<usize>,
}

impl<'a> Ribbon<'a> {
    /// The picture `image` laid along `points`, once every `tile` pixels
    /// (at least one pixel).
    #[must_use]
    pub fn new(points: &'a [RibbonPoint], image: &'a TipImage, tile: f32) -> Self {
        let tile = if tile.is_finite() { tile.max(1.0) } else { 1.0 };
        Self {
            points,
            image,
            tile,
            paint: 0..points.len().saturating_sub(1),
        }
    }

    /// Paints only segments `segments` (segment `i` runs from point `i`
    /// to point `i + 1`); the others are context.
    #[must_use]
    pub fn painting(mut self, segments: Range<usize>) -> Self {
        let count = self.points.len().saturating_sub(1);
        self.paint = segments.start.min(count)..segments.end.min(count);
        self
    }

    /// The points of the painted segments.
    fn painted(&self) -> &[RibbonPoint] {
        if self.paint.is_empty() {
            &[]
        } else {
            &self.points[self.paint.start..=self.paint.end]
        }
    }

    /// The widest radius along the painted segments.
    fn widest(&self) -> f32 {
        self.painted()
            .iter()
            .map(|p| p.disc.radius)
            .fold(0.0, f32::max)
    }
}

/// The nearest point of a ribbon's path to a pixel.
struct Nearest {
    distance: f32,
    segment: usize,
    /// Where along the segment, 0 to 1, before holding to it.
    t: f32,
}

impl Shape for Ribbon<'_> {
    fn bounds(&self) -> PixelRect {
        let reach = self.widest() + 2.0;
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        let painted = self.painted();
        for p in painted {
            for axis in 0..2 {
                min[axis] = min[axis].min(p.disc.at[axis] - reach);
                max[axis] = max[axis].max(p.disc.at[axis] + reach);
            }
        }
        if painted.len() < 2 {
            return PixelRect {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            };
        }
        PixelRect::around(min, max)
    }

    fn span(&self, y: f32) -> Option<(f32, f32)> {
        let reach = self.widest() + 2.0;
        self.painted()
            .windows(2)
            .filter_map(|w| segment_span(w[0].disc.at, w[1].disc.at, reach, y))
            .reduce(|(l0, h0), (l1, h1)| (l0.min(l1), h0.max(h1)))
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        let mut best: Option<Nearest> = None;
        for (segment, w) in self.points.windows(2).enumerate() {
            let (a, b) = (w[0].disc.at, w[1].disc.at);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length_sq = dx * dx + dy * dy;
            if length_sq < 1.0e-12 {
                continue;
            }
            let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length_sq;
            let held = t.clamp(0.0, 1.0);
            let (ex, ey) = (p[0] - a[0] - dx * held, p[1] - a[1] - dy * held);
            let distance = (ex * ex + ey * ey).sqrt();
            if best.as_ref().is_none_or(|b| distance < b.distance) {
                best = Some(Nearest {
                    distance,
                    segment,
                    t,
                });
            }
        }
        let Some(near) = best else {
            return 0.0;
        };
        if !self.paint.contains(&near.segment) {
            return 0.0;
        }
        let last = self.points.len() - 2;
        // Square ends: nothing beyond the first and the last point.
        if (near.segment == 0 && near.t < 0.0) || (near.segment == last && near.t > 1.0) {
            return 0.0;
        }
        let (from, to) = (self.points[near.segment], self.points[near.segment + 1]);
        let t = near.t.clamp(0.0, 1.0);
        let radius = from.disc.radius + (to.disc.radius - from.disc.radius) * t;
        if radius <= 0.0 || near.distance > radius + 1.0 {
            return 0.0;
        }
        let strength = from.disc.strength + (to.disc.strength - from.disc.strength) * t;
        let along = from.arc + (to.arc - from.arc) * t;
        // Positive to the right of the way the line goes (y points down).
        let (dx, dy) = (
            to.disc.at[0] - from.disc.at[0],
            to.disc.at[1] - from.disc.at[1],
        );
        let (qx, qy) = (p[0] - from.disc.at[0], p[1] - from.disc.at[1]);
        let side = if dx * qy - dy * qx < 0.0 { -1.0 } else { 1.0 };
        let across = near.distance * side;
        let (width, height) = (self.image.width() as f32, self.image.height() as f32);
        let tx = along / self.tile * width;
        let ty = (across / (2.0 * radius) + 0.5) * height;
        let footprint = (width / self.tile).max(height / (2.0 * radius));
        self.image.sample(tx, ty, footprint, true) * strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f32, y: f32, arc: f32) -> RibbonPoint {
        RibbonPoint {
            disc: Disc::new([x, y], 5.0, 1.0),
            arc,
        }
    }

    #[test]
    fn a_ribbon_ends_square_and_fills_the_outside_of_a_bend() {
        let image = TipImage::from_coverage(16, 16, &[255; 256]).expect("a full picture");
        let points = [
            point(0.0, 0.0, 0.0),
            point(20.0, 0.0, 20.0),
            point(20.0, 20.0, 40.0),
        ];
        let ribbon = Ribbon::new(&points, &image, 4.0);
        assert_eq!(ribbon.coverage([-1.0, 0.0]), 0.0, "square at the start");
        assert_eq!(ribbon.coverage([20.0, 21.0]), 0.0, "square at the end");
        assert!(ribbon.coverage([10.0, 0.0]) > 0.99);
        // Outside the corner, up and to the right of it.
        assert!(ribbon.coverage([22.0, -2.0]) > 0.99, "the bend is filled");
        assert_eq!(ribbon.coverage([26.0, -6.0]), 0.0);
    }

    #[test]
    fn pieces_with_context_read_the_same_as_the_whole_path() {
        let stripes: Vec<u8> = (0..8 * 4)
            .map(|i| if (i % 8) < 4 { 255 } else { 40 })
            .collect();
        let image = TipImage::from_coverage(8, 4, &stripes).expect("stripes");
        let at = [
            [0.0, 0.0],
            [10.0, 3.0],
            [20.0, 0.0],
            [30.0, 5.0],
            [38.0, 14.0],
            [44.0, 26.0],
            [56.0, 30.0],
        ];
        let mut arc = 0.0;
        let mut points = Vec::new();
        for (i, &[x, y]) in at.iter().enumerate() {
            if i > 0 {
                let [px, py]: [f32; 2] = at[i - 1];
                arc += ((x - px) * (x - px) + (y - py) * (y - py)).sqrt();
            }
            points.push(point(x, y, arc));
        }
        let whole = Ribbon::new(&points, &image, 6.0);
        // Segments 0 to 2, then 3 to 5, each with two segments of context.
        let first = Ribbon::new(&points[..6], &image, 6.0).painting(0..3);
        let second = Ribbon::new(&points[1..], &image, 6.0).painting(2..5);
        for y in -8..40 {
            for x in -4..64 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let (a, b) = (first.coverage(p), second.coverage(p));
                assert!(a == 0.0 || b == 0.0, "{p:?} is painted by one piece only");
                assert!((whole.coverage(p) - a.max(b)).abs() < 1.0e-6, "{p:?}");
            }
        }
    }
}
