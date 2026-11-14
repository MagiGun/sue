//! The pen: lines become polygons with round caps and round joins, and
//! polygons become coverage.
//!
//! A line drawn with a round pen is every place within half the pen's
//! width of it. So each straight piece becomes one polygon, the piece
//! with a half circle on both ends, and a whole line is the union of its
//! pieces: where two pieces meet, their half circles are the round join,
//! and at the ends of an open line they are the round caps. The union is
//! taken per pixel as the larger of the coverages, which is exact along
//! the outside of a join, where both pieces have the same round edge.

use super::path::{Point, Polyline};
use super::raster::fill;

/// Coverage under this is the dust of adding up and is dropped.
const DUST: f32 = 1e-5;

/// How far a round end may stray from a true circle, in pixels.
const ROUND: f32 = 0.02;

/// The outline of everything within `half` of the piece from `a` to `b`,
/// added to `lines`. A piece of no length gives a disc.
pub fn capsule(a: Point, b: Point, half: f32, lines: &mut Vec<[Point; 2]>) {
    let half = half.max(1e-3);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    let along = if length > 1e-6 { (dy).atan2(dx) } else { 0.0 };
    let step = 2.0 * (1.0 - (ROUND / half).min(1.0)).acos();
    let pieces = ((std::f32::consts::PI / step.max(1e-3)).ceil() as usize).clamp(4, 256);
    // The corners of a round end lie a hair outside the true circle, so
    // that the many-sided end has the area of the round one: its sides
    // cut inside the circle by as much as its corners stand out.
    let turn = std::f32::consts::PI / pieces as f32;
    let out = half * (turn / turn.sin()).sqrt();
    // Half a turn round `b` from its left side to its right, then half a
    // turn round `a` back to where the outline began. The first and the
    // last corner of each end stay on the sides of the piece.
    let mut corners = Vec::with_capacity(2 * pieces + 2);
    for (middle, from) in [
        (b, along - std::f32::consts::FRAC_PI_2),
        (a, along + std::f32::consts::FRAC_PI_2),
    ] {
        for piece in 0..=pieces {
            let (sin, cos) = (from + turn * piece as f32).sin_cos();
            let far = if piece == 0 || piece == pieces {
                half
            } else {
                out
            };
            corners.push([middle[0] + far * cos, middle[1] + far * sin]);
        }
    }
    for (index, corner) in corners.iter().enumerate() {
        lines.push([*corner, corners[(index + 1) % corners.len()]]);
    }
}

/// Coverage of a square tile, a number from 0 to 1 for every pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    /// The side of the tile, in pixels.
    pub size: usize,
    /// The pixels, row by row from the top.
    pub cells: Vec<f32>,
}

impl Coverage {
    /// An empty tile.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            size,
            cells: vec![0.0; size * size],
        }
    }

    /// Lays the inside of the outline `lines` over the tile: each pixel
    /// keeps the larger of what it had and what the outline covers.
    fn lay(&mut self, lines: &[[Point; 2]]) {
        let mut low = [f32::MAX; 2];
        let mut high = [f32::MIN; 2];
        for point in lines.iter().flatten() {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        if lines.is_empty() || !low.iter().chain(&high).all(|number| number.is_finite()) {
            return;
        }
        // The outline is filled in a box of its own with room to spare
        // on every side (two pixels on the right, where the rasterizer
        // keeps its last column for what spills over), so that no edge
        // is ever cut off; what falls outside the tile is dropped
        // after.
        let limit = self.size as f32 + 3.0;
        let origin = [
            (low[0].floor() - 1.0).clamp(-limit, limit),
            (low[1].floor() - 1.0).clamp(-limit, limit),
        ];
        let width = ((high[0].ceil() + 2.0).clamp(-limit, limit) - origin[0]) as usize;
        let height = ((high[1].ceil() + 1.0).clamp(-limit, limit) - origin[1]) as usize;
        if width == 0 || height == 0 {
            return;
        }
        let covered = fill(lines, origin, width, height);
        let (left, top) = (origin[0] as i64, origin[1] as i64);
        for (index, cover) in covered.iter().enumerate() {
            let x = left + (index % width) as i64;
            let y = top + (index / width) as i64;
            if *cover < DUST || x < 0 || y < 0 || x >= self.size as i64 || y >= self.size as i64 {
                continue;
            }
            let cell = &mut self.cells[y as usize * self.size + x as usize];
            *cell = cell.max(*cover);
        }
    }

    /// Draws `line` with a round pen `half` pixels in radius.
    pub fn stroke(&mut self, line: &Polyline, half: f32) {
        let mut outline = Vec::new();
        if let [dot] = line.points[..] {
            capsule(dot, dot, half, &mut outline);
            self.lay(&outline);
            return;
        }
        for (a, b) in line.edges() {
            outline.clear();
            capsule(line.points[a], line.points[b], half, &mut outline);
            self.lay(&outline);
        }
    }

    /// Fills `lines` as one shape, each closed whether it says so or not.
    /// A line that runs the other way round than the one it lies in cuts
    /// a hole into it.
    pub fn fill(&mut self, lines: &[Polyline]) {
        let mut outline = Vec::new();
        for line in lines {
            let count = line.points.len();
            if count < 3 {
                continue;
            }
            for index in 0..count {
                outline.push([line.points[index], line.points[(index + 1) % count]]);
            }
        }
        self.lay(&outline);
    }

    /// Takes away what `other` covers.
    pub fn clear(&mut self, other: &Self) {
        for (cell, cut) in self.cells.iter_mut().zip(&other.cells) {
            *cell *= 1.0 - cut;
        }
    }

    /// The tile as bytes, 0 for nothing and 255 for covered.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.cells
            .iter()
            .map(|cover| (cover.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[Point], closed: bool) -> Polyline {
        Polyline {
            points: points.to_vec(),
            closed,
        }
    }

    fn at(tile: &Coverage, x: usize, y: usize) -> f32 {
        tile.cells[y * tile.size + x]
    }

    fn total(tile: &Coverage) -> f32 {
        tile.cells.iter().sum()
    }

    #[test]
    fn a_dot_is_a_disc_of_the_pen() {
        let mut tile = Coverage::new(24);
        tile.stroke(&line(&[[12.0, 12.0]], true), 4.0);
        let area = std::f32::consts::PI * 16.0;
        assert!((total(&tile) - area).abs() < 0.05, "{}", total(&tile));
        assert!((at(&tile, 12, 12) - 1.0).abs() < 1e-4);
        assert_eq!(at(&tile, 2, 2), 0.0);
        // Round: as far up as to the left.
        assert!((at(&tile, 12, 7) - at(&tile, 7, 12)).abs() < 1e-3);
    }

    #[test]
    fn a_line_has_the_area_of_its_body_and_two_half_discs() {
        let mut tile = Coverage::new(32);
        tile.stroke(&line(&[[8.0, 15.5], [24.0, 15.5]], false), 1.5);
        let area = 16.0 * 3.0 + std::f32::consts::PI * 2.25;
        assert!((total(&tile) - area).abs() < 0.05, "{}", total(&tile));
        // The body is solid three pixels high, on whole pixels.
        for y in [14, 15, 16] {
            assert!((at(&tile, 16, y) - 1.0).abs() < 1e-4);
        }
        assert_eq!(at(&tile, 16, 13), 0.0);
        assert_eq!(at(&tile, 16, 17), 0.0);
        // The cap reaches half the pen beyond the end and no further.
        assert!(at(&tile, 24, 15) > 0.9);
        assert!(at(&tile, 25, 15) > 0.0);
        assert_eq!(at(&tile, 26, 15), 0.0);
    }

    #[test]
    fn a_pen_and_a_half_wide_is_one_whole_pixel_and_half_of_the_next() {
        let mut tile = Coverage::new(24);
        // Its upper side on the pixel edge at 8.
        tile.stroke(&line(&[[4.0, 8.75], [20.0, 8.75]], false), 0.75);
        assert!((at(&tile, 12, 8) - 1.0).abs() < 1e-4);
        assert!((at(&tile, 12, 9) - 0.5).abs() < 1e-4);
        assert_eq!(at(&tile, 12, 7), 0.0);
        assert_eq!(at(&tile, 12, 10), 0.0);
    }

    #[test]
    fn a_join_is_round_and_never_darker_than_the_line() {
        let mut tile = Coverage::new(32);
        let corner = line(&[[6.0, 26.0], [16.0, 6.0], [26.0, 26.0]], false);
        tile.stroke(&corner, 2.0);
        assert!(tile.cells.iter().all(|cover| (0.0..=1.0).contains(cover)));
        // The tip is rounded off: half the pen above the corner is
        // covered, the pixel above that is not.
        assert!(at(&tile, 15, 4) > 0.5);
        assert_eq!(at(&tile, 15, 2), 0.0);
        // The same line drawn from the other end is the same picture.
        let mut back = Coverage::new(32);
        back.stroke(&line(&[[26.0, 26.0], [16.0, 6.0], [6.0, 26.0]], false), 2.0);
        for (a, b) in tile.cells.iter().zip(&back.cells) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn a_closed_line_joins_its_last_point_to_its_first() {
        let square = [[6.0, 6.0], [18.0, 6.0], [18.0, 18.0], [6.0, 18.0]];
        let mut open = Coverage::new(24);
        open.stroke(&line(&square, false), 1.0);
        let mut closed = Coverage::new(24);
        closed.stroke(&line(&square, true), 1.0);
        assert_eq!(at(&open, 5, 12), 0.0);
        assert!((at(&closed, 5, 12) - 1.0).abs() < 1e-4);
        assert!((at(&closed, 6, 12) - 1.0).abs() < 1e-4);
        assert_eq!(at(&closed, 12, 12), 0.0);
    }

    #[test]
    fn a_fill_has_its_area_and_the_other_way_round_is_a_hole() {
        let outer = line(&[[4.0, 4.0], [20.0, 4.0], [20.0, 20.0], [4.0, 20.0]], true);
        let same = line(&[[8.0, 8.0], [16.0, 8.0], [16.0, 16.0], [8.0, 16.0]], true);
        let other = line(&[[8.0, 8.0], [8.0, 16.0], [16.0, 16.0], [16.0, 8.0]], true);
        let mut tile = Coverage::new(24);
        tile.fill(std::slice::from_ref(&outer));
        assert!((total(&tile) - 256.0).abs() < 1e-2);
        let mut solid = Coverage::new(24);
        solid.fill(&[outer.clone(), same]);
        assert!((total(&solid) - 256.0).abs() < 1e-2);
        let mut holed = Coverage::new(24);
        holed.fill(&[outer, other]);
        assert!((total(&holed) - 192.0).abs() < 1e-2);
        assert_eq!(at(&holed, 12, 12), 0.0);
        assert!((at(&holed, 5, 5) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn what_reaches_past_the_tile_is_cut_off_cleanly() {
        let mut tile = Coverage::new(16);
        tile.stroke(&line(&[[-10.0, 8.0], [40.0, 8.0]], false), 2.0);
        for x in 0..16 {
            assert!((at(&tile, x, 7) - 1.0).abs() < 1e-4, "{x}");
            assert_eq!(at(&tile, x, 3), 0.0);
        }
        // Wholly outside: nothing, and no panic.
        let mut empty = Coverage::new(16);
        empty.stroke(&line(&[[30.0, 30.0], [50.0, 30.0]], false), 2.0);
        empty.stroke(&line(&[[-1e9, -1e9], [-1e9, -2e9]], false), 2.0);
        assert_eq!(total(&empty), 0.0);
    }

    #[test]
    fn clearing_takes_coverage_away() {
        let mut tile = Coverage::new(8);
        tile.fill(&[line(
            &[[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]],
            true,
        )]);
        let mut cut = Coverage::new(8);
        cut.fill(&[line(
            &[[0.0, 0.0], [4.0, 0.0], [4.0, 8.0], [0.0, 8.0]],
            true,
        )]);
        tile.clear(&cut);
        assert_eq!(at(&tile, 1, 4), 0.0);
        assert!((at(&tile, 6, 4) - 1.0).abs() < 1e-4);
        assert_eq!(tile.bytes()[4 * 8 + 6], 255);
        assert_eq!(tile.bytes()[4 * 8 + 1], 0);
    }
}
