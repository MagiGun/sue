//! Pixel fitting: where the 24 unit canvas lands on a tile of whole
//! pixels.
//!
//! A pen 1.5 pixels wide cannot fill whole pixels on both of its sides,
//! and at 16 or 36 pixels nothing drawn on a 24 unit grid meets the pixel
//! grid by itself. So before an icon is rasterized its straight stems and
//! the tops of its curves (its anchors) are moved, by less than a pixel,
//! to where the side of the pen that faces away from the middle of the
//! canvas lies exactly on a pixel edge. Everything between two anchors
//! moves with them in proportion. The outline of an icon is then sharp at
//! every size, the soft half pixel of the pen always lies on the inside,
//! and a picture that is symmetric on the canvas stays symmetric in
//! pixels.

use super::path::{Point, Polyline};

/// The side of the canvas every icon is drawn on, in units.
pub const CANVAS: f32 = 24.0;

/// How far along an axis a piece may move and still count as straight
/// along the other one.
const FLAT: f32 = 1e-4;

/// The shortest straight piece that is fitted, in units.
const STEM: f32 = 0.75;

/// The cosine of the widest turn that still counts as the top of a curve
/// and not as a corner.
const SMOOTH: f32 = 0.5;

/// How near two anchors count as one, in units.
const NEAR: f32 = 0.05;

/// Finds the anchors of `line` along `axis` (0 across, 1 down): the
/// coordinate of every straight piece that runs along the other axis and
/// of every place where a curve turns back.
pub fn anchors(line: &Polyline, axis: usize, found: &mut Vec<f32>) {
    let points = &line.points;
    let edges: Vec<(usize, usize)> = line.edges().collect();
    let count = edges.len();
    if count == 0 {
        return;
    }
    let delta = |edge: usize| points[edges[edge].1][axis] - points[edges[edge].0][axis];
    let vector = |edge: usize| -> Point {
        let (a, b) = (points[edges[edge].0], points[edges[edge].1]);
        [b[0] - a[0], b[1] - a[1]]
    };
    let flat = |edge: usize| delta(edge).abs() <= FLAT;
    // A closed line is walked from a piece that is not flat, so that no
    // run of flat pieces is cut in two by where the walk starts.
    let first = if line.closed {
        (0..count).find(|&edge| !flat(edge)).unwrap_or(0)
    } else {
        0
    };
    let walk = (0..count)
        .map(|step| (first + step) % count)
        .chain((line.closed && !flat(first)).then_some(first));
    let mut before: Option<usize> = None;
    let mut run = 0.0_f32;
    let mut run_at = 0.0_f32;
    for edge in walk {
        let v = vector(edge);
        if flat(edge) {
            if run == 0.0 {
                run_at = points[edges[edge].0][axis];
            }
            run += v[0].hypot(v[1]);
            continue;
        }
        if run >= STEM {
            found.push(run_at);
        } else if let Some(before) = before {
            let u = vector(before);
            let turned_back = (delta(before) > 0.0) != (delta(edge) > 0.0);
            let cosine =
                (u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1])).max(1e-12);
            if turned_back && cosine >= SMOOTH {
                found.push(if run > 0.0 {
                    run_at
                } else {
                    points[edges[edge].0][axis]
                });
            }
        }
        run = 0.0;
        before = Some(edge);
    }
    if run >= STEM {
        found.push(run_at);
    }
}

/// Where the middle of a pen `width` pixels wide goes for an anchor at
/// `unit`: the nearest place where the side of the pen that faces away
/// from the middle of the canvas lies on a pixel edge.
fn snap(unit: f32, size: f32, width: f32) -> f32 {
    let scale = size / CANVAS;
    let low = |pixel: f32| (pixel - width / 2.0).round() + width / 2.0;
    if unit <= CANVAS / 2.0 {
        low(unit * scale)
    } else {
        // The mirror image of the other half, so that a picture that is
        // symmetric in units is symmetric in pixels.
        size - low((CANVAS - unit) * scale)
    }
}

/// The fitting of one icon at one size: a map from units to pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    /// Per axis, pairs of unit and pixel in rising order, from the near
    /// edge of the canvas to the far one.
    knots: [Vec<[f32; 2]>; 2],
}

impl Fit {
    /// The fitting for a tile `size` pixels wide, drawn with a pen
    /// `stroke` pixels wide: `stroked` are the lines the pen follows,
    /// `filled` the outlines that are filled. Stroked anchors go first;
    /// a filled edge is fitted (to the nearest pixel edge) only where no
    /// stroke already holds the place.
    #[must_use]
    pub fn new(stroked: &[Polyline], filled: &[Polyline], size: usize, stroke: f32) -> Self {
        let size = size as f32;
        let knots = [0, 1].map(|axis| {
            let mut places: Vec<[f32; 2]> = Vec::new();
            for (lines, width) in [(stroked, stroke), (filled, 0.0)] {
                let mut found = Vec::new();
                for line in lines {
                    anchors(line, axis, &mut found);
                }
                for unit in found {
                    if !places.iter().any(|place| (place[0] - unit).abs() < NEAR) {
                        places.push([unit, snap(unit, size, width)]);
                    }
                }
            }
            places.retain(|place| place[0] > NEAR && place[0] < CANVAS - NEAR);
            places.sort_by(|a, b| a[0].total_cmp(&b[0]));
            let scale = size / CANVAS;
            let mut knots = vec![[0.0, 0.0]];
            for [unit, pixel] in places {
                let [before_unit, before_pixel] = knots[knots.len() - 1];
                let plain = (unit - before_unit) * scale;
                // Two anchors never swap places or close up: one that
                // would is kept at its own distance from the one before.
                let pixel = if pixel - before_pixel < plain / 2.0 {
                    before_pixel + plain
                } else {
                    pixel
                };
                knots.push([unit, pixel.min(size)]);
            }
            knots.push([CANVAS, size]);
            knots
        });
        Self { knots }
    }

    /// No fitting: the canvas scaled to `size` pixels and nothing else.
    #[must_use]
    pub fn plain(size: usize) -> Self {
        let knots = || vec![[0.0, 0.0], [CANVAS, size as f32]];
        Self {
            knots: [knots(), knots()],
        }
    }

    fn along(&self, axis: usize, unit: f32) -> f32 {
        let knots = &self.knots[axis];
        let after = knots
            .iter()
            .position(|knot| knot[0] >= unit)
            .unwrap_or(knots.len() - 1)
            .max(1);
        let ([u0, p0], [u1, p1]) = (knots[after - 1], knots[after]);
        if u1 - u0 <= f32::EPSILON {
            return p1;
        }
        p0 + (unit - u0) * (p1 - p0) / (u1 - u0)
    }

    /// Where a point of the canvas goes, in pixels.
    #[must_use]
    pub fn point(&self, point: Point) -> Point {
        [self.along(0, point[0]), self.along(1, point[1])]
    }
}

#[cfg(test)]
mod tests {
    use super::super::path::parse;
    use super::*;

    fn lines(data: &str) -> Vec<Polyline> {
        parse(data)
            .unwrap()
            .iter()
            .map(|path| path.flatten(0.01))
            .collect()
    }

    fn found(data: &str, axis: usize) -> Vec<f32> {
        let mut found = Vec::new();
        for line in lines(data) {
            anchors(&line, axis, &mut found);
        }
        found.sort_by(f32::total_cmp);
        found
    }

    fn near(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    #[test]
    fn straight_pieces_are_anchors_along_the_other_axis() {
        let frame = "M4.75 3.75 H19.25 V20.25 H4.75 Z";
        assert!(near(&found(frame, 0), &[4.75, 19.25]));
        assert!(near(&found(frame, 1), &[3.75, 20.25]));
        // An open line: one anchor across its direction, none along it.
        assert!(near(&found("M4 12 H20", 1), &[12.0]));
        assert!(found("M4 12 H20", 0).is_empty());
        // Too short to be a stem.
        assert!(found("M4 12 H4.5", 1).is_empty());
        // A slanted line has none.
        assert!(found("M4 4 L20 20", 0).is_empty());
        assert!(found("M4 4 L20 20", 1).is_empty());
    }

    #[test]
    fn the_four_sides_of_a_circle_are_anchors() {
        let circle = "M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z";
        assert!(near(&found(circle, 0), &[2.75, 21.25]));
        assert!(near(&found(circle, 1), &[2.75, 21.25]));
        // An open half circle has its top, and no ends.
        let half = "M4 12 A8 8 0 0 1 20 12";
        assert!(near(&found(half, 1), &[4.0]));
        assert!(found(half, 0).is_empty());
    }

    #[test]
    fn a_corner_is_not_an_anchor_but_a_rounded_box_has_its_sides() {
        assert!(found("M4 6 L12 18 L20 6", 1).is_empty());
        assert!(found("M8 4 L16 12 L8 20", 0).is_empty());
        let rounded = "M6 4 H18 A2 2 0 0 1 20 6 V18 A2 2 0 0 1 18 20 H6 A2 2 0 0 1 4 18 V6 \
                       A2 2 0 0 1 6 4 Z";
        assert!(near(&found(rounded, 0), &[4.0, 20.0]));
        assert!(near(&found(rounded, 1), &[4.0, 20.0]));
    }

    #[test]
    fn the_far_side_of_the_pen_lands_on_a_pixel_edge() {
        for size in [12_usize, 16, 20, 24, 30, 32, 36, 40, 48, 64, 96] {
            for stroke in [1.0_f32, 1.25, 1.5, 1.875, 2.25, 3.0] {
                for unit in [2.75_f32, 3.0, 4.25, 6.75, 9.5, 11.0, 12.0] {
                    let low = snap(unit, size as f32, stroke);
                    let edge = low - stroke / 2.0;
                    assert!((edge - edge.round()).abs() < 1e-4, "{size} {stroke} {unit}");
                    assert!((low - unit * size as f32 / CANVAS).abs() <= 0.5 + 1e-4);
                    // The other half is the mirror image.
                    let high = snap(CANVAS - unit, size as f32, stroke);
                    if unit < 12.0 {
                        assert!((low + high - size as f32).abs() < 1e-4);
                        let edge = high + stroke / 2.0;
                        assert!((edge - edge.round()).abs() < 1e-4);
                    }
                }
            }
        }
    }

    #[test]
    fn stems_on_the_quarter_grid_stay_where_they_are_at_24_and_48() {
        // Left and top of the middle on .75, right and bottom on .25.
        let frame = lines("M3.75 4.75 H20.25 V19.25 H3.75 Z");
        for (size, stroke) in [(24_usize, 1.5_f32), (48, 3.0)] {
            let fit = Fit::new(&frame, &[], size, stroke);
            let scale = size as f32 / CANVAS;
            for point in [[3.75, 4.75], [20.25, 19.25], [12.0, 12.0], [7.0, 16.5]] {
                let to = fit.point(point);
                assert!((to[0] - point[0] * scale).abs() < 1e-4, "{point:?} {to:?}");
                assert!((to[1] - point[1] * scale).abs() < 1e-4, "{point:?} {to:?}");
            }
        }
    }

    #[test]
    fn a_frame_is_sharp_and_symmetric_at_every_size() {
        let frame = lines("M3.75 4.75 H20.25 V19.25 H3.75 Z");
        for size in [14_usize, 16, 18, 20, 22, 28, 32, 36, 40] {
            let stroke = 1.25 * size as f32 / 16.0;
            let fit = Fit::new(&frame, &[], size, stroke);
            let (near, far) = (fit.point([3.75, 4.75]), fit.point([20.25, 19.25]));
            for (low, high) in [(near[0], far[0]), (near[1], far[1])] {
                let outer = low - stroke / 2.0;
                assert!((outer - outer.round()).abs() < 1e-4, "{size}: {low}");
                assert!(
                    (low + high - size as f32).abs() < 1e-4,
                    "{size}: {low} {high}"
                );
            }
            // What lies between moves in proportion.
            let middle = fit.point([12.0, 12.0]);
            assert!((middle[0] - size as f32 / 2.0).abs() < 1e-4);
            assert!((middle[1] - size as f32 / 2.0).abs() < 1e-4);
        }
    }

    #[test]
    fn the_ends_of_the_canvas_stay_and_the_map_never_turns_back() {
        let busy = lines("M3 3 V21 M4.5 3 V21 M6 3 V21 M12 3 V21 M20.5 3 V21 M3 12 H21");
        for size in [8_usize, 12, 16, 24, 33] {
            let fit = Fit::new(&busy, &[], size, 1.25);
            assert_eq!(fit.point([0.0, 0.0]), [0.0, 0.0]);
            assert_eq!(fit.point([CANVAS, CANVAS]), [size as f32, size as f32]);
            let mut last = -1.0_f32;
            for step in 0..=96 {
                let x = fit.point([step as f32 / 4.0, 0.0])[0];
                assert!(x >= last, "{size}: {x} after {last}");
                last = x;
            }
        }
    }

    #[test]
    fn a_filled_edge_goes_to_the_nearest_pixel_edge_unless_a_stroke_is_there() {
        let stroked = lines("M3.75 4 V20");
        let filled = lines("M3.75 8 H10.4 V12.2 H3.75 Z");
        let fit = Fit::new(&stroked, &filled, 16, 1.25);
        let scale = 16.0 / CANVAS;
        // The stroke keeps 3.75: its outer side is on a pixel edge.
        let stem = fit.point([3.75, 0.0])[0];
        assert!(((stem - 0.625) - (stem - 0.625).round()).abs() < 1e-4);
        // The fill's own edges are on pixel edges.
        for (axis, unit) in [(0, 10.4_f32), (1, 8.0), (1, 12.2)] {
            let mut point = [0.0, 0.0];
            point[axis] = unit;
            let pixel = fit.point(point)[axis];
            assert!((pixel - pixel.round()).abs() < 1e-4, "{unit}: {pixel}");
            assert!((pixel - unit * scale).abs() <= 0.5);
        }
    }

    #[test]
    fn no_fitting_is_plain_scaling() {
        let fit = Fit::plain(48);
        assert_eq!(fit.point([3.3, 12.0]), [6.6, 24.0]);
    }
}
