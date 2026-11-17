//! The curve: a centripetal Catmull-Rom spline through the records,
//! built one segment at a time and sampled at even steps of arc length.
//!
//! Records closer than the knot spacing to the last knot only move the
//! floating end; a record becomes a knot once it is far enough away, or
//! when the pen presses harder in place (a tap grows to the press it
//! reached). A segment between two knots needs the knot after it, so
//! the last two segments are provisional: [`Curve::finish`] on a clone
//! gives them for the tail, and on the pipeline's own curve after the
//! lift. Centripetal parameters keep the curve from looping or
//! overshooting where knots crowd (Yuksel, Schaefer and Keyser, 2011).
//! Every value besides the place (pressure, lean, twist, speed, time)
//! goes straight between the two knots of a segment by arc length, so
//! it never rises above both of them.

use std::collections::VecDeque;

use super::ingest::Record;
use super::math;

/// How much harder the pen must press in place to make a new knot.
const GROW: f32 = 0.02;

/// The pieces of a segment's arc-length table are this long at most,
/// in layer pixels.
const PIECE: f32 = 0.5;

/// The most pieces in a segment's table.
const MOST_PIECES: f32 = 64.0;

/// The distance between knots for a stroke drawn at `scale` screen
/// points per layer pixel: one screen point, from a quarter of a layer
/// pixel to four.
#[must_use]
pub(crate) fn knot_spacing(scale: f32) -> f32 {
    (1.0 / scale).clamp(0.25, 4.0)
}

/// One point on the curve, before the brush's dynamics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Base {
    /// The record here; its place is on the curve.
    pub record: Record,
    /// How far along the stroke.
    pub arc: f32,
    /// The way the line runs, degrees from 0 to 360.
    pub direction: f32,
    /// The point's number in the stroke, from 0.
    pub index: u64,
}

/// The curve. See the module.
#[derive(Clone, Debug)]
pub(crate) struct Curve {
    step: f32,
    spacing: f32,
    /// The newest fixed knots, oldest first (four at most).
    fixed: VecDeque<Record>,
    /// The newest record when it is not a knot yet.
    floating: Option<Record>,
    /// The arc length at the end of the last segment drawn.
    arc: f32,
    /// The arc length of the next even point.
    next: f32,
    /// Points made so far.
    count: u64,
    /// The last point's pressure and direction.
    last_pressure: f32,
    last_direction: f32,
    /// The arc length of the last point.
    last_arc: f32,
}

impl Curve {
    pub fn new(step: f32, spacing: f32) -> Self {
        Self {
            step,
            spacing,
            fixed: VecDeque::new(),
            floating: None,
            arc: 0.0,
            next: 0.0,
            count: 0,
            last_pressure: 0.0,
            last_direction: 0.0,
            last_arc: 0.0,
        }
    }

    /// Takes the next record; the points of every segment that became
    /// final go to `out`.
    pub fn push(&mut self, record: Record, out: &mut Vec<Base>) {
        let Some(last) = self.fixed.back() else {
            self.fixed.push_back(record);
            return;
        };
        let far = distance(record.pos, last.pos) >= self.spacing;
        if far || record.pressure > last.pressure + GROW {
            self.floating = None;
            self.fix(record, out);
        } else {
            self.floating = Some(record);
        }
    }

    /// Makes the floating end a knot, however close it is: the record
    /// there is then on the curve, and nothing after it takes values
    /// from before it.
    pub fn settle(&mut self, out: &mut Vec<Base>) {
        if let Some(record) = self.floating.take() {
            self.fix(record, out);
        }
    }

    /// Ends the curve: the floating end becomes the last knot, the last
    /// segment is drawn and the end itself is a point.
    pub fn finish(&mut self, out: &mut Vec<Base>) {
        self.settle(out);
        let n = self.fixed.len();
        if n >= 2 {
            let before = (n >= 3).then(|| self.fixed[n - 3]);
            let (from, to) = (self.fixed[n - 2], self.fixed[n - 1]);
            self.segment(before, from, to, None, out);
        }
        if let Some(end) = self.fixed.back().copied()
            && (self.count == 0 || self.last_arc < self.arc)
        {
            self.emit(end, self.arc, self.last_direction, out);
        }
    }

    /// Makes `record` a knot, and draws the segment that now has both
    /// its neighbours.
    fn fix(&mut self, record: Record, out: &mut Vec<Base>) {
        self.fixed.push_back(record);
        let n = self.fixed.len();
        if n >= 3 {
            let before = (n >= 4).then(|| self.fixed[n - 4]);
            let (from, to, after) = (self.fixed[n - 3], self.fixed[n - 2], self.fixed[n - 1]);
            self.segment(before, from, to, Some(after), out);
        }
        while self.fixed.len() > 4 {
            self.fixed.pop_front();
        }
    }

    /// Draws the segment from `from` to `to`, whose neighbours are
    /// `before` and `after` (mirrored where missing).
    fn segment(
        &mut self,
        before: Option<Record>,
        from: Record,
        to: Record,
        after: Option<Record>,
        out: &mut Vec<Base>,
    ) {
        let p1 = from.pos;
        let p2 = to.pos;
        let p0 = before.map_or([2.0 * p1[0] - p2[0], 2.0 * p1[1] - p2[1]], |r| r.pos);
        let p3 = after.map_or([2.0 * p2[0] - p1[0], 2.0 * p2[1] - p1[1]], |r| r.pos);
        let chord = distance(p1, p2);
        if self.count == 0 {
            let direction = math::direction(sub(p2, p1)).unwrap_or(0.0);
            self.emit(from, 0.0, direction, out);
            self.next = self.step;
        }
        if chord <= 1e-6 {
            if to.pressure > self.last_pressure + GROW * 0.5 {
                self.emit(to, self.arc, self.last_direction, out);
            }
            return;
        }
        let spline = Spline::new([p0, p1, p2, p3]);
        let pieces = (chord / PIECE).ceil().clamp(2.0, MOST_PIECES) as usize;
        let mut table = Vec::with_capacity(pieces + 1);
        let mut length = 0.0_f32;
        let mut at = p1;
        table.push((0.0_f32, p1));
        for k in 1..=pieces {
            let next = spline.at(k as f32 / pieces as f32);
            length += distance(at, next);
            at = next;
            table.push((length, next));
        }
        let mut piece = 0;
        while self.next <= self.arc + length {
            let local = self.next - self.arc;
            while piece + 1 < pieces && table[piece + 1].0 < local {
                piece += 1;
            }
            let (s0, a) = table[piece];
            let (s1, b) = table[piece + 1];
            let f = if s1 > s0 {
                ((local - s0) / (s1 - s0)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let u = (piece as f32 + f) / pieces as f32;
            let mut record = from.lerp(&to, (local / length).clamp(0.0, 1.0));
            record.pos = spline.at(u);
            let direction = math::direction(sub(b, a)).unwrap_or(self.last_direction);
            self.emit(record, self.next, direction, out);
            self.next += self.step;
        }
        self.arc += length;
        if length < self.step && to.pressure > self.last_pressure + GROW * 0.5 {
            self.emit(to, self.arc, self.last_direction, out);
        }
    }

    /// Adds a point.
    fn emit(&mut self, record: Record, arc: f32, direction: f32, out: &mut Vec<Base>) {
        out.push(Base {
            record,
            arc,
            direction,
            index: self.count,
        });
        self.count += 1;
        self.last_pressure = record.pressure;
        self.last_direction = direction;
        self.last_arc = arc;
    }
}

/// A centripetal Catmull-Rom segment between its middle two points.
pub(crate) struct Spline {
    p: [[f32; 2]; 4],
    t: [f32; 4],
}

impl Spline {
    pub(crate) fn new(p: [[f32; 2]; 4]) -> Self {
        let mut t = [0.0_f32; 4];
        for i in 1..4 {
            // The square root of the distance, kept off zero.
            t[i] = t[i - 1] + distance(p[i - 1], p[i]).sqrt().max(1e-3);
        }
        Self { p, t }
    }

    /// The point `u` of the way along (0 is the second point, 1 the
    /// third), by the Barry and Goldman pyramid.
    pub(crate) fn at(&self, u: f32) -> [f32; 2] {
        let [t0, t1, t2, t3] = self.t;
        let [p0, p1, p2, p3] = self.p;
        let t = t1 + (t2 - t1) * u;
        let mix = |a: [f32; 2], b: [f32; 2], ta: f32, tb: f32| {
            let w = (t - ta) / (tb - ta);
            [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w]
        };
        let a1 = mix(p0, p1, t0, t1);
        let a2 = mix(p1, p2, t1, t2);
        let a3 = mix(p2, p3, t2, t3);
        let b1 = mix(a1, a2, t0, t2);
        let b2 = mix(a2, a3, t1, t3);
        mix(b1, b2, t1, t2)
    }
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}
