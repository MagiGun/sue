//! Smart Shape (Clip Studio Paint 5.0): hold the pen still at the end of a
//! stroke and the stroke becomes the clean shape it was meant to be, a
//! straight line, an arc, an ellipse or circle, or a polygon such as a
//! rectangle or triangle. A stroke no shape fits comes back tidied of its
//! wobble rather than untouched: the hold always does something.
//! [`recognize`] reads the stroke's points and answers the shape's
//! outline, densely, for the brush to draw again.
//!
//! The reading walks the stroke the way Clip Studio Paint's QuickShape
//! does: a shortest path over the stroke's own samples, every link paying
//! the mean of its two ends' distances from the candidate figure, so the
//! samples that stray are skipped rather than the whole reading given up
//! on ([`snapped`]).

/// How long the pen rests before the stroke turns into its shape, in
/// milliseconds. Clip Studio Paint's own default for its "Length of
/// long-press" preference (0.7 seconds); the preference itself can say
/// otherwise (`sue_paint.smart_shape_hold_ms`).
pub const HOLD_MS: i64 = 700;
/// The same, in seconds, for the countdown while the pen rests.
pub const HOLD_SECONDS: f64 = HOLD_MS as f64 / 1000.0;
/// How far the pen may wander while resting, screen points.
pub const HOLD_REACH: f32 = 4.0;
/// How many samples ahead one link of the reading graph reaches. The graph
/// walks the stroke's own samples, and a link may only skip a hand's wobble
/// or a small spike; a straight jump from end to end would cost next to
/// nothing on every candidate and they would all tie.
const HOP: usize = 4;

/// The cheapest way over the stroke's samples while hugging the geometry
/// `off` measures the distance to, and what it costs. Clip Studio Paint's
/// QuickShape reads figures the same way: its Dijkstra charges a link the
/// mean of its two ends' distances from the candidate, so the cheapest path
/// skips the samples that stray and the figure is read off what is left.
/// The path comes back in the stroke's own order, ends kept.
fn snapped(points: &[[f32; 2]], off: impl Fn([f32; 2]) -> f32) -> (Vec<[f32; 2]>, f32) {
    let count = points.len();
    let mut cost = vec![f32::MAX; count];
    let mut from = vec![0usize; count];
    cost[0] = 0.0;
    // Links only ever go forward, so one pass in order is the answer.
    for at in 0..count {
        if cost[at] == f32::MAX {
            continue;
        }
        for next in at + 1..=(at + HOP).min(count - 1) {
            let charge = (off(points[at]) + off(points[next])) * 0.5;
            let now = cost[at] + charge;
            if now < cost[next] {
                cost[next] = now;
                from[next] = at;
            }
        }
    }
    let mut path = vec![points[count - 1]];
    let mut at = count - 1;
    while at != 0 {
        at = from[at];
        path.push(points[at]);
    }
    path.reverse();
    (path, cost[count - 1])
}

/// What one link of a snapped path paid on average: how far off the
/// candidate the stroke lay, in pixels.
fn mean_cost(path: &[[f32; 2]], cost: f32) -> f32 {
    cost / (path.len() - 1).max(1) as f32
}

/// The ellipse the stroke sits in: the box of the samples that hugged it
/// (a bulge does not stretch it), `round` true to answer a circle whatever
/// the stroke's own proportions were.
fn ellipse_from(points: &[[f32; 2]], step: f32, round: bool) -> Option<Vec<[f32; 2]>> {
    let [x0, y0, x1, y1] = bounds(points);
    let (center, rx, ry) = (
        [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
        (x1 - x0) / 2.0,
        (y1 - y0) / 2.0,
    );
    if (x1 - x0) < 2.0 || (y1 - y0) < 2.0 {
        return None;
    }
    let (path, _) = snapped(points, |p| ellipse_distance(p, center, rx, ry));
    let [qx0, qy0, qx1, qy1] = bounds(&path);
    let (center, rx, ry) = (
        [(qx0 + qx1) / 2.0, (qy0 + qy1) / 2.0],
        (qx1 - qx0) / 2.0,
        (qy1 - qy0) / 2.0,
    );
    let round = round || (rx - ry).abs() < rx.max(ry) * 0.12;
    let (rx, ry) = if round {
        let r = (rx + ry) / 2.0;
        (r, r)
    } else {
        (rx, ry)
    };
    let first = points[0];
    let around = std::f32::consts::PI * (rx + ry);
    let count = ((around / step.max(0.5)).ceil() as usize).clamp(24, 4096);
    let mut out: Vec<[f32; 2]> = (0..=count)
        .map(|index| {
            // From where the stroke began, round the same way.
            let start = (first[1] - center[1]).atan2(first[0] - center[0]);
            let turn = if turning(points) > 0.0 { 1.0 } else { -1.0 };
            let angle = start + turn * index as f32 / count as f32 * std::f32::consts::TAU;
            [center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]
        })
        .collect();
    out.dedup();
    Some(out)
}

/// The clean shape `points` (a stroke as drawn) were meant to be, as a
/// path of points at most `step` apart. A stroke no shape fits comes back
/// tidied of its wobble rather than untouched, so resting the pen always
/// answers with something; `None` only for a stroke too short to read.
#[must_use]
pub fn recognize(points: &[[f32; 2]], step: f32) -> Option<Vec<[f32; 2]>> {
    if points.len() < 3 {
        return None;
    }
    let [x0, y0, x1, y1] = bounds(points);
    let diagonal = (x1 - x0).hypot(y1 - y0);
    if diagonal < 8.0 {
        return None;
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let length: f32 = points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum();
    // A straight line: hugging the line between its ends costs next to
    // nothing, the samples that stray skipped rather than counted.
    let chord = distance(first, last);
    if chord > diagonal * 0.8 {
        let (path, cost) = snapped(points, |p| line_distance(p, first, last));
        if mean_cost(&path, cost) < chord * 0.06 {
            return Some(densify(&[first, last], step, false));
        }
    }
    let closed = chord < diagonal * 0.22 && length > diagonal * 1.8;
    if closed {
        // A polygon of a few corners, before an ellipse: a rectangle's
        // corners would fit an ellipse badly anyway.
        let corners = simplify_closed(points, diagonal * 0.07);
        if (3..=6).contains(&corners.len()) {
            let sides = sides(&corners);
            let (path, cost) = snapped(points, |p| sides_distance(p, &sides));
            if mean_cost(&path, cost) < diagonal * 0.05 {
                // Straight sides where the hand turned, curves where it did
                // not: a moon drawn in one stroke keeps its points and its
                // round sides.
                return Some(smoothed(points, &corners, step, true));
            }
        }
        let (center, rx, ry) = (
            [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
            (x1 - x0) / 2.0,
            (y1 - y0) / 2.0,
        );
        let error = points
            .iter()
            .map(|p| ellipse_read(*p, center, rx, ry))
            .sum::<f32>()
            / points.len() as f32;
        if error < 0.12 {
            // Read off the samples that hugged the ellipse, not the whole
            // wobbling stroke: a bulge does not stretch it.
            let round = (rx - ry).abs() < rx.max(ry) * 0.12;
            if let Some(shape) = ellipse_from(points, step, round) {
                return Some(shape);
            }
        }
        // A closed stroke with no shape either way (a moon, a tangle):
        // tidy it as drawn, the corners the hand turned held sharp.
        return Some(smoothed(points, &corners, step, true));
    }
    // An arc: a circle through both ends and the middle, close all along.
    if let Some(arc) = arc_path(points, step, 0.08) {
        return Some(arc);
    }
    // An open polyline of a few straight sides, or a curve the stroke
    // leaned into and out of.
    let corners = simplify(points, diagonal * 0.06);
    if (3..=5).contains(&corners.len()) {
        let sides = sides(&corners);
        let (path, cost) = snapped(points, |p| sides_distance(p, &sides));
        if mean_cost(&path, cost) < diagonal * 0.05 {
            return Some(smoothed(points, &corners, step, false));
        }
    }
    // A stroke with no shape in it at all (a scribble, a tangle): tidy it
    // as drawn, so resting the pen always comes back with something. The
    // corners the hand turned stay corners and the wobble between them
    // goes.
    Some(smoothed(points, &corners, step, false))
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How far along `points` each of them lies, starting at zero.
fn along(points: &[[f32; 2]]) -> Vec<f32> {
    let mut out = vec![0.0];
    for pair in points.windows(2) {
        let last = out.last().copied().unwrap_or(0.0);
        out.push(last + distance(pair[0], pair[1]));
    }
    out
}

/// The pen pressure at each point of `path`, taken from `drawn` (the
/// pressure of `points`, the stroke as drawn) at the same distance along
/// the way, so a corrected shape keeps the line's thickening and thinning.
/// A drawn path without a pressure of its own is treated as even.
#[must_use]
pub fn pressures(points: &[[f32; 2]], drawn: &[f32], path: &[[f32; 2]]) -> Vec<f32> {
    let mean = if drawn.is_empty() {
        1.0
    } else {
        drawn.iter().sum::<f32>() / drawn.len() as f32
    };
    if points.len() < 2 || drawn.len() != points.len() {
        return vec![mean; path.len()];
    }
    let (source, target) = (along(points), along(path));
    let (source_total, target_total) = (
        source.last().copied().unwrap_or(0.0),
        target.last().copied().unwrap_or(0.0),
    );
    if source_total < 1e-3 || target_total < 1e-3 {
        return vec![mean; path.len()];
    }
    let mut out = Vec::with_capacity(path.len());
    let mut at = 0;
    for length in &target {
        let s = length / target_total * source_total;
        while at + 2 < source.len() && source[at + 1] < s {
            at += 1;
        }
        let (a, b) = (source[at], source[at + 1]);
        let t = ((s - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
        out.push(drawn[at] + (drawn[at + 1] - drawn[at]) * t);
    }
    out
}

/// The arc the stroke lies on, as a path at most `step` apart: the circle
/// through both its ends and its middle, every point of it within `reach`
/// of that circle's radius (`reach` as a fraction of the radius). `None`
/// when no circle fits, or the stroke turns the other way round from the
/// middle. Starts where the stroke began and rounds the same way.
fn arc_path(points: &[[f32; 2]], step: f32, reach: f32) -> Option<Vec<[f32; 2]>> {
    let [x0, y0, x1, y1] = bounds(points);
    let diagonal = (x1 - x0).hypot(y1 - y0);
    let (center, radius) = circle_fit(points)?;
    if radius >= diagonal * 4.0 {
        return None;
    }
    // How far off the circle the reading forgives, in pixels: the reach of
    // the radius, and never more than the stroke's own size (a huge circle
    // through a tangle would otherwise forgive anything).
    let bite = radius.max(1.0).min(diagonal) * reach;
    // A second fit without the samples the first missed widely, so a bite
    // or a wobble does not pull the circle off the stroke lying on it.
    let kept: Vec<[f32; 2]> = points
        .iter()
        .copied()
        .filter(|p| (distance(*p, center) - radius).abs() <= bite)
        .collect();
    let (center, radius) = match kept.len() {
        3.. => circle_fit(&kept).unwrap_or((center, radius)),
        _ => (center, radius),
    };
    // Hugging the circle costs next to nothing: the reading skips the
    // samples that stray rather than giving up on the whole stroke.
    let (path, cost) = snapped(points, |p| (distance(p, center) - radius).abs());
    if mean_cost(&path, cost) >= bite {
        return None;
    }
    // And once more over what the reading kept, so the figure is read off
    // the samples that stayed with the circle.
    let hugging: Vec<[f32; 2]> = path
        .iter()
        .copied()
        .filter(|p| (distance(*p, center) - radius).abs() <= bite)
        .collect();
    let (center, radius) = match hugging.len() {
        3.. => circle_fit(&hugging).unwrap_or((center, radius)),
        _ => (center, radius),
    };
    // The ends stay the stroke's own; the way round and how far are read
    // off the stroke's own samples. The middle sample cannot say it (the
    // reading may have kept only the ends of a clean arc), and a nearly
    // closed stroke ends a hair from where it began, so the short way
    // round would answer the gap it left rather than the arc it drew.
    let first = path[0];
    let angle = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let mut span = 0.0;
    let mut previous = angle(points[0]);
    for point in &points[1..] {
        span += wrap(angle(*point) - previous);
        previous = angle(*point);
    }
    // A figure winds no further than once round.
    let sweep = span.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
    let a0 = angle(first);
    let count = ((sweep.abs() * radius / step.max(0.5)).ceil() as usize).clamp(8, 4096);
    Some(
        (0..=count)
            .map(|index| {
                let angle = a0 + sweep * index as f32 / count as f32;
                [
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ]
            })
            .collect(),
    )
}

/// The direction from `a` to `b`, in radians.
fn direction(a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[1] - a[1]).atan2(b[0] - a[0])
}

/// The index of the first point of `points` beyond `reach` pixels of
/// `points[from]` going `step` (-1 or 1), and the last one otherwise.
fn reach_at(points: &[[f32; 2]], from: usize, reach: f32, step: isize) -> usize {
    let mut at = from;
    loop {
        let next = at as isize + step;
        if next < 0 || next as usize >= points.len() {
            return at;
        }
        let next = next as usize;
        if distance(points[next], points[from]) > reach {
            return next;
        }
        at = next;
    }
}

/// How sharply the drawn stroke turns at `points[at]`: the direction change
/// over `reach` pixels either side of it, in radians. Zero at the ends,
/// where there is nothing to compare.
fn sharp_turn(points: &[[f32; 2]], at: usize, reach: f32) -> f32 {
    let behind = reach_at(points, at, reach, -1);
    let ahead = reach_at(points, at, reach, 1);
    if behind == at || ahead == at {
        return 0.0;
    }
    wrap(direction(points[at], points[ahead]) - direction(points[behind], points[at])).abs()
}

/// How far along the stroke either side of a waypoint the pen's direction
/// is read when telling a corner from a curve, layer pixels.
const CORNER_REACH: f32 = 16.0;
/// The direction change over that reach that makes a corner: a hand's
/// corner turns about ninety degrees, and a curve up to about twenty. The
/// top of a tight curve (a hairpin flattened almost into a V) reaches the
/// thirties, so the line sits past that.
const CORNER_TURN: f32 = 0.70;

/// [`sharp_turn`] with the stroke read as a ring: the first sample's
/// neighbours are the last ones.
fn sharp_turn_closed(points: &[[f32; 2]], at: usize, reach: f32) -> f32 {
    if at > 0 && at + 1 < points.len() {
        return sharp_turn(points, at, reach);
    }
    let mut behind = points.len() - 1;
    while behind > 0 && distance(points[behind], points[at]) < reach {
        behind -= 1;
    }
    let mut ahead = 0;
    while ahead + 1 < points.len() && distance(points[ahead], points[at]) < reach {
        ahead += 1;
    }
    wrap(direction(points[at], points[ahead]) - direction(points[behind], points[at])).abs()
}

/// The curve through `corners`, at most `step` apart: a Catmull-Rom spline,
/// so a stroke drawn as a curve comes back a smooth curve, with a corner
/// the hand really turned held sharp by a repeated control point. Closed
/// back to the first when `closed`.
fn smoothed(points: &[[f32; 2]], corners: &[[f32; 2]], step: f32, closed: bool) -> Vec<[f32; 2]> {
    let sharp = |corner: [f32; 2]| {
        let Some(at) = points.iter().position(|p| *p == corner) else {
            return false;
        };
        let turn = if closed {
            sharp_turn_closed(points, at, CORNER_REACH)
        } else {
            sharp_turn(points, at, CORNER_REACH)
        };
        turn > CORNER_TURN
    };
    // A stroke with no corner at all is one curve: a circle through it,
    // read loosely, comes back exactly smooth, where a spline through the
    // waypoints of a wobbling hand still weaves. No circle fits every
    // curve, so a loose reading comes next, and the spline after it.
    let loose;
    let corners = if corners.iter().all(|corner| !sharp(*corner)) {
        if !closed && let Some(arc) = arc_path(points, step, 0.15) {
            return arc;
        }
        let [x0, y0, x1, y1] = bounds(points);
        let diagonal = (x1 - x0).hypot(y1 - y0);
        loose = if closed {
            simplify_closed(points, diagonal * 0.14)
        } else {
            simplify(points, diagonal * 0.12)
        };
        &loose[..]
    } else {
        corners
    };
    // The controls: the corners in order, the ends repeated so the spline
    // starts and stops at them, and a sharp corner twice so the curve turns
    // its corner instead of rounding it.
    let mut controls: Vec<[f32; 2]> = Vec::with_capacity(corners.len() * 2 + 4);
    let last = corners[corners.len() - 1];
    if closed {
        controls.push(last);
    } else {
        controls.push(corners[0]);
    }
    for corner in corners {
        controls.push(*corner);
        if sharp(*corner) {
            controls.push(*corner);
        }
    }
    if closed {
        controls.push(corners[0]);
        if sharp(corners[0]) {
            controls.push(corners[0]);
        }
        controls.push(corners[1 % corners.len()]);
    } else {
        controls.push(last);
    }
    let mut out = vec![controls[1]];
    for window in controls.windows(4) {
        let (p0, p1, p2, p3) = (window[0], window[1], window[2], window[3]);
        let span = distance(p1, p2);
        if span < 1e-3 {
            out.push(p2);
            continue;
        }
        let count = (span / step.max(0.5)).ceil() as usize;
        for index in 1..=count {
            let t = index as f32 / count as f32;
            out.push(catmull(p0, p1, p2, p3, t));
        }
    }
    out.dedup();
    out
}

/// The point `t` of the way from `p1` to `p2` along a centripetal
/// Catmull-Rom spline: the knots go with the square root of the spacing,
/// so unevenly spaced controls (the waypoints of a wobbling hand) do not
/// make the curve hook as it reaches the far end of a span.
fn catmull(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let (k0, k1, k2) = (
        distance(p0, p1).sqrt(),
        distance(p1, p2).sqrt(),
        distance(p2, p3).sqrt(),
    );
    let (t0, t1, t2, t3) = (0.0, k0, k0 + k1, k0 + k1 + k2);
    let t = t1 + t * k1;
    let mix = |a: [f32; 2], b: [f32; 2], from: f32, to: f32, t: f32| {
        let span = to - from;
        if span < 1e-6 {
            // A control doubled over its neighbour (a held corner): nothing
            // to weigh, so it is the neighbour.
            return b;
        }
        let u = (t - from) / span;
        [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u]
    };
    let a1 = mix(p0, p1, t0, t1, t);
    let a2 = mix(p1, p2, t1, t2, t);
    let a3 = mix(p2, p3, t2, t3, t);
    let b1 = mix(a1, a2, t0, t2, t);
    let b2 = mix(a2, a3, t1, t3, t);
    mix(b1, b2, t1, t2, t)
}

/// The point a drag of a corrected shape turns and scales it about: the
/// first point of an open one (a line, an arc or a polyline), so its far end
/// follows the pen; the middle of a closed one (an ellipse or a polygon), so
/// the whole shape turns and grows in place. Clip Studio Paint moves a
/// corrected line's end point the same way, and a shape's size and angle.
#[must_use]
pub fn pivot(path: &[[f32; 2]]) -> [f32; 2] {
    let (Some(first), Some(last)) = (path.first(), path.last()) else {
        return [0.0; 2];
    };
    if distance(*first, *last) > 1.0 {
        return *first;
    }
    let [x0, y0, x1, y1] = bounds(path);
    [(x0 + x1) / 2.0, (y0 + y1) / 2.0]
}

/// The point of `path` nearest to `to`: the point of a corrected shape the
/// pen is at, which a drag carries.
#[must_use]
pub fn nearest(path: &[[f32; 2]], to: [f32; 2]) -> [f32; 2] {
    path.iter()
        .copied()
        .min_by(|a, b| distance(*a, to).total_cmp(&distance(*b, to)))
        .unwrap_or(to)
}

/// The shape `path` as a drag that carries its `reference` point to `to`
/// leaves it: turned and scaled about `pivot`, from the shape as it was
/// corrected, so dragging back and forth never drifts. The size stays
/// between a twentieth and twenty times the corrected one, so a pen
/// dragged onto the pivot leaves a shape rather than a point. Answers the
/// shape and how much bigger or smaller the drag left it, so the brush can
/// thicken or thin the line with it the way Clip Studio Paint scales a
/// line's width when the figure is sized.
#[must_use]
pub fn dragged_scaled(
    path: &[[f32; 2]],
    pivot: [f32; 2],
    reference: [f32; 2],
    to: [f32; 2],
) -> (Vec<[f32; 2]>, f32) {
    let arm = |point: [f32; 2]| [point[0] - pivot[0], point[1] - pivot[1]];
    let (from, now) = (arm(reference), arm(to));
    let length = from[0].hypot(from[1]);
    if length < 1e-3 {
        return (path.to_vec(), 1.0);
    }
    let scale = (now[0].hypot(now[1]) / length).clamp(0.05, 20.0);
    let turn = wrap(now[1].atan2(now[0]) - from[1].atan2(from[0]));
    let (sin, cos) = turn.sin_cos();
    let path = path
        .iter()
        .map(|point| {
            let [x, y] = arm(*point);
            let (x, y) = (x * scale, y * scale);
            [pivot[0] + x * cos - y * sin, pivot[1] + x * sin + y * cos]
        })
        .collect();
    (path, scale)
}

fn bounds(points: &[[f32; 2]]) -> [f32; 4] {
    points.iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[x0, y0, x1, y1], p| [x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])],
    )
}

/// How far `p` is from the segment `a` to `b`.
fn line_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
    let t =
        (((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(0.0, 1.0);
    distance(p, [a[0] + ex * t, a[1] + ey * t])
}

/// An angle brought into -pi to pi.
fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// Which way the points go round: positive one way, negative the other.
fn turning(points: &[[f32; 2]]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum()
}

/// The circle the points lie on, by least squares (Kasa): every point
/// pulls on the fit a little, so one stray sample moves it far less than
/// the circle through three chosen points does. `None` when there are too
/// few points or they lie in a line.
fn circle_fit(points: &[[f32; 2]]) -> Option<([f32; 2], f32)> {
    if points.len() < 3 {
        return None;
    }
    let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut sz, mut sjx, mut sjy) = (0.0f64, 0.0f64, 0.0f64);
    for p in points {
        let (x, y) = (p[0] as f64, p[1] as f64);
        let z = x * x + y * y;
        sx += x;
        sy += y;
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
        sz += z;
        sjx += z * x;
        sjy += z * y;
    }
    let count = points.len() as f64;
    // x2 + y2 = a*x + b*y + c, least squares: the centre is (a/2, b/2).
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let base = [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, count]];
    let d = det(base);
    if d == 0.0 || !d.is_finite() {
        return None;
    }
    let a = det([[sjx, sxy, sx], [sjy, syy, sy], [sz, sy, count]]) / d;
    let b = det([[sxx, sjx, sx], [sxy, sjy, sy], [sx, sz, count]]) / d;
    let c = det([[sxx, sxy, sjx], [sxy, syy, sjy], [sx, sy, sz]]) / d;
    let squared = c + a * a / 4.0 + b * b / 4.0;
    if !squared.is_finite() || squared <= 0.0 {
        return None;
    }
    Some(([(a / 2.0) as f32, (b / 2.0) as f32], squared.sqrt() as f32))
}

/// Ramer-Douglas-Peucker: the points that keep the line within `epsilon`.
fn simplify(points: &[[f32; 2]], epsilon: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let (index, far) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(index, p)| (index + 1, line_distance(*p, first, last)))
        .fold(
            (0, 0.0),
            |best, now| if now.1 > best.1 { now } else { best },
        );
    if far <= epsilon {
        return vec![first, last];
    }
    let mut left = simplify(&points[..=index], epsilon);
    let right = simplify(&points[index..], epsilon);
    left.pop();
    left.extend(right);
    left
}

/// The corners of a closed outline: split at the point farthest from the
/// start, simplify both halves, and drop the start when it lies on a side.
fn simplify_closed(points: &[[f32; 2]], epsilon: f32) -> Vec<[f32; 2]> {
    let start = points[0];
    let far = points
        .iter()
        .enumerate()
        .max_by(|a, b| distance(*a.1, start).total_cmp(&distance(*b.1, start)))
        .map_or(0, |(index, _)| index);
    let mut there = simplify(&points[..=far], epsilon);
    let mut back = points[far..].to_vec();
    back.push(start);
    let back = simplify(&back, epsilon);
    there.pop();
    there.extend(back);
    there.pop();
    // The start is a corner only if the outline turns there.
    if there.len() > 3 {
        let (before, after) = (there[there.len() - 1], there[1]);
        if line_distance(there[0], before, after) <= epsilon {
            there.remove(0);
        }
    }
    there
}

/// The sides between each corner and the next, the last back to the first.
fn sides(corners: &[[f32; 2]]) -> Vec<([f32; 2], [f32; 2])> {
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(a, b)| (*a, *b))
        .collect()
}

/// How far `p` lies from the nearest of `sides`.
fn sides_distance(p: [f32; 2], sides: &[([f32; 2], [f32; 2])]) -> f32 {
    sides
        .iter()
        .map(|(a, b)| line_distance(p, *a, *b))
        .fold(f32::MAX, f32::min)
}

/// How far along its radius the ellipse takes `p`, as a fraction: zero on
/// it, positive outside.
fn ellipse_read(p: [f32; 2], center: [f32; 2], rx: f32, ry: f32) -> f32 {
    let (dx, dy) = (
        (p[0] - center[0]) / rx.max(1.0),
        (p[1] - center[1]) / ry.max(1.0),
    );
    (dx.hypot(dy) - 1.0).abs()
}

/// The same as [`ellipse_read`] in pixels, for the reading graph's charges.
fn ellipse_distance(p: [f32; 2], center: [f32; 2], rx: f32, ry: f32) -> f32 {
    ellipse_read(p, center, rx, ry) * (rx + ry) * 0.5
}

/// Points at most `step` apart along the sides through `corners` (closed
/// back to the first when `closed`).
fn densify(corners: &[[f32; 2]], step: f32, closed: bool) -> Vec<[f32; 2]> {
    let mut ends = corners.to_vec();
    if closed && let Some(first) = corners.first() {
        ends.push(*first);
    }
    let mut out = vec![ends[0]];
    for pair in ends.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let count = ((distance(a, b) / step.max(0.5)).ceil() as usize).max(1);
        for index in 1..=count {
            let t = index as f32 / count as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape a drag leaves, without the width scale (`dragged_scaled`).
    fn dragged(
        path: &[[f32; 2]],
        pivot: [f32; 2],
        reference: [f32; 2],
        to: [f32; 2],
    ) -> Vec<[f32; 2]> {
        dragged_scaled(path, pivot, reference, to).0
    }

    /// A hand's wobble: points along `path` nudged a little either way.
    fn wobbly(path: &[[f32; 2]]) -> Vec<[f32; 2]> {
        path.iter()
            .enumerate()
            .map(|(index, p)| {
                let nudge = ((index * 7919) % 13) as f32 / 13.0 - 0.5;
                [p[0] + nudge * 2.0, p[1] - nudge * 2.0]
            })
            .collect()
    }

    fn kind(points: &[[f32; 2]]) -> &'static str {
        let Some(shape) = recognize(points, 2.0) else {
            return "none";
        };
        let corners = simplify(&shape, 1.0).len();
        if distance(shape[0], shape[shape.len() - 1]) < 3.0 {
            if corners > 8 { "ellipse" } else { "polygon" }
        } else if corners == 2 {
            "line"
        } else if corners > 8 {
            "arc"
        } else {
            "polyline"
        }
    }

    #[test]
    fn strokes_become_the_shapes_they_were_meant_to_be() {
        let line: Vec<[f32; 2]> = (0..=50).map(|i| [i as f32 * 4.0, i as f32 * 1.5]).collect();
        assert_eq!(kind(&wobbly(&line)), "line");
        let circle: Vec<[f32; 2]> = (0..=60)
            .map(|i| {
                let a = i as f32 / 60.0 * std::f32::consts::TAU;
                [100.0 + 60.0 * a.cos(), 100.0 + 40.0 * a.sin()]
            })
            .collect();
        assert_eq!(kind(&wobbly(&circle)), "ellipse");
        let square = densify(
            &[[0.0, 0.0], [120.0, 0.0], [120.0, 90.0], [0.0, 90.0]],
            4.0,
            true,
        );
        assert_eq!(kind(&wobbly(&square)), "polygon");
        let triangle = densify(&[[0.0, 100.0], [60.0, 0.0], [120.0, 100.0]], 4.0, true);
        assert_eq!(kind(&wobbly(&triangle)), "polygon");
        let arc: Vec<[f32; 2]> = (0..=40)
            .map(|i| {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 40.0 * 0.7);
                [100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin()]
            })
            .collect();
        assert_eq!(kind(&wobbly(&arc)), "arc");
        let zigzag = densify(
            &[[0.0, 0.0], [50.0, 60.0], [100.0, 0.0], [150.0, 60.0]],
            4.0,
            false,
        );
        assert_eq!(kind(&wobbly(&zigzag)), "polyline");
        // A scribble comes back tidied rather than as a shape: the wobble
        // between its corners goes, so the hold leaves something.
        let scribble: Vec<[f32; 2]> = (0..80)
            .map(|i| {
                let t = i as f32 * 0.37;
                [
                    t * 9.0 + (t * 3.1).sin() * 30.0,
                    (t * 1.7).cos() * 40.0 + (t * 5.3).sin() * 25.0,
                ]
            })
            .collect();
        let along = |points: &[[f32; 2]]| {
            points
                .windows(2)
                .map(|pair| distance(pair[0], pair[1]))
                .sum::<f32>()
        };
        let tidied = recognize(&scribble, 2.0).expect("a scribble is tidied, not left");
        assert!(
            along(&tidied) < along(&scribble) * 0.97,
            "the wobble went: {:.0} of {:.0}",
            along(&tidied),
            along(&scribble)
        );
        for point in scribble.iter().step_by(5) {
            let near = tidied
                .iter()
                .map(|p| distance(*p, *point))
                .fold(f32::MAX, f32::min);
            assert!(near < 20.0, "strays {near:.0} from the stroke");
        }
        // And a stroke too short to read has nothing to tidy.
        assert_eq!(recognize(&[[0.0, 0.0], [1.0, 1.0]], 2.0), None);
    }

    #[test]
    fn shapes_are_dense_enough_to_draw() {
        let line: Vec<[f32; 2]> = (0..=20).map(|i| [i as f32 * 10.0, 0.0]).collect();
        let shape = recognize(&line, 2.0).unwrap();
        assert!(
            shape
                .windows(2)
                .all(|pair| distance(pair[0], pair[1]) <= 2.0 + 1e-3)
        );
        assert_eq!(shape[0], [0.0, 0.0]);
        assert_eq!(*shape.last().unwrap(), [200.0, 0.0]);
    }

    /// A corrected shape keeps the drawn line's pressure, read at the same
    /// distance along the way: a line drawn bearing down stays a line that
    /// thickens.
    #[test]
    fn a_corrected_shape_keeps_the_drawn_lines_pressure() {
        let drawn: Vec<[f32; 2]> = (0..=40)
            .map(|step| [step as f32 * 3.0, (step as f32 / 4.0).sin() * 2.0])
            .collect();
        let pen: Vec<f32> = (0..=40)
            .map(|step| 0.2 + 0.8 * step as f32 / 40.0)
            .collect();
        let path = recognize(&drawn, 2.0).expect("a straight line");
        let profile = pressures(&drawn, &pen, &path);
        assert_eq!(profile.len(), path.len());
        assert!((profile[0] - 0.2).abs() < 0.05, "the start: {}", profile[0]);
        let end = profile[profile.len() - 1];
        assert!((end - 1.0).abs() < 0.05, "the end: {end}");
        assert!(
            profile.windows(2).all(|pair| pair[1] >= pair[0] - 1e-4),
            "it only bears down: {profile:?}"
        );
    }

    /// The worst direction change of the stroke over `reach` pixels, in
    /// degrees.
    fn worst_turn(points: &[[f32; 2]], reach: f32) -> f32 {
        (0..points.len())
            .map(|at| sharp_turn(points, at, reach))
            .fold(0.0, f32::max)
            .to_degrees()
    }

    /// A stroke drawn as a curve comes back a smooth curve: no sharp
    /// corners, and the waypoints of a drawn curve the arc fit missed are
    /// still on it.
    #[test]
    fn a_curved_stroke_comes_back_a_curve_not_a_polygon() {
        let arc = |center: [f32; 2], rx: f32, ry: f32, from: f32, to: f32, count: usize| {
            (0..=count)
                .map(|i| {
                    let a = from + (to - from) * i as f32 / count as f32;
                    [center[0] + rx * a.cos(), center[1] + ry * a.sin()]
                })
                .collect::<Vec<[f32; 2]>>()
        };
        let shaky = |path: &[[f32; 2]], amount: f32| {
            path.iter()
                .enumerate()
                .map(|(index, p)| {
                    let nudge = ((index * 7919) % 13) as f32 / 13.0 - 0.5;
                    [p[0] + nudge * amount, p[1] - nudge * amount * 0.7]
                })
                .collect::<Vec<[f32; 2]>>()
        };
        let tau = std::f32::consts::TAU;
        let cases = [
            (
                "a flattened half circle",
                shaky(
                    &arc([150.0, 150.0], 150.0, 60.0, std::f32::consts::PI, tau, 60),
                    12.0,
                ),
            ),
            (
                "a shallow smile",
                arc(
                    [150.0, 150.0],
                    200.0,
                    200.0,
                    std::f32::consts::PI * 1.15,
                    std::f32::consts::PI * 1.85,
                    40,
                ),
            ),
            (
                "a half circle",
                arc([150.0, 150.0], 100.0, 100.0, std::f32::consts::PI, tau, 60),
            ),
            (
                "an S drawn as one stroke",
                (0..=60)
                    .map(|step| {
                        let x = step as f32 * 5.0;
                        [x, 60.0 * (x / 300.0 * tau).sin()]
                    })
                    .collect::<Vec<[f32; 2]>>(),
            ),
        ];
        for (name, drawn) in cases {
            let Some(shape) = recognize(&drawn, 2.0) else {
                panic!("{name}: no shape at all");
            };
            let worst = worst_turn(&shape, 4.0);
            assert!(worst < 25.0, "{name}: still kinks, {worst:.0} degrees");
            // And it follows the stroke it came from, rather than some
            // other curve.
            for point in drawn.iter().step_by(7) {
                let near = shape
                    .iter()
                    .map(|p| distance(*p, *point))
                    .fold(f32::MAX, f32::min);
                assert!(near < 30.0, "{name}: strays {near:.0} from the stroke");
            }
        }
    }

    /// A corner the hand really turned stays a corner.
    #[test]
    fn a_corner_the_hand_turned_stays_sharp() {
        let zigzag = densify(
            &[[0.0, 0.0], [50.0, 60.0], [100.0, 0.0], [150.0, 60.0]],
            4.0,
            false,
        );
        let shape = recognize(&zigzag, 2.0).expect("a polyline");
        for corner in [[50.0, 60.0], [100.0, 0.0]] {
            let at = shape
                .iter()
                .position(|p| *p == corner)
                .unwrap_or_else(|| panic!("{corner:?} is not on the shape"));
            let turn = sharp_turn(&shape, at, 12.0).to_degrees();
            assert!(
                turn > 50.0,
                "{corner:?} came back rounded: {turn:.0} degrees"
            );
        }
    }

    /// A shape no simple reading catches (a moon's two arcs and its two
    /// points) still comes back tidied when the pen rests.
    #[test]
    fn a_complicated_shape_is_tidied_not_left_alone() {
        let center = [150.0, 150.0];
        let arc = |center: [f32; 2], radius: f32, from: f32, to: f32, count: usize| {
            (0..=count)
                .map(|i| {
                    let a = from + (to - from) * i as f32 / count as f32;
                    [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
                })
                .collect::<Vec<[f32; 2]>>()
        };
        // The outer arc left, the inner arc back, the tips at 55.2 degrees.
        let drawn: Vec<[f32; 2]> = arc(center, 100.0, 0.963, 5.320, 80)
            .into_iter()
            .chain(arc([center[0] + 35.0, center[1]], 85.0, 1.307, 4.976, 60))
            .collect();
        let shape = recognize(&drawn, 2.0).expect("a moon is tidied, not left");
        // The tips are points: the shape turns sharply there and nowhere
        // else (the run at the start and the one at the end are one tip,
        // the stroke's own start).
        let sharp: Vec<bool> = (0..shape.len())
            .map(|at| sharp_turn(&shape, at, 12.0).to_degrees() > 50.0)
            .collect();
        let runs = sharp
            .iter()
            .fold((0, false), |(runs, was), now| {
                (runs + usize::from(*now && !was), *now)
            })
            .0;
        assert!(
            (2..=3).contains(&runs),
            "not two tips: {runs} sharp stretches"
        );
        // And the whole of it follows the drawn moon.
        for point in drawn.iter().step_by(7) {
            let near = shape
                .iter()
                .map(|p| distance(*p, *point))
                .fold(f32::MAX, f32::min);
            assert!(near < 25.0, "strays {near:.0} from the moon");
        }
    }

    /// A drag while the pen is still down, after the correction: a line's
    /// far end follows the pen, and a shape turns and grows about its
    /// middle, the way Clip Studio Paint does it.
    #[test]
    fn a_drag_after_the_correction_moves_the_end_or_whole_shape() {
        // A corrected line: its far end is what a drag carries.
        let line = vec![[0.0, 0.0], [25.0, 0.0], [50.0, 0.0], [100.0, 0.0]];
        assert_eq!(pivot(&line), [0.0, 0.0]);
        let far = dragged(&line, pivot(&line), [100.0, 0.0], [200.0, 0.0]);
        assert_eq!(
            far,
            vec![[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [200.0, 0.0]]
        );
        // Pulled the same distance up, it stands on end.
        let up = dragged(&line, pivot(&line), [100.0, 0.0], [0.0, 100.0]);
        assert!(distance(up[0], [0.0, 0.0]) < 0.01, "{:?}", up[0]);
        assert!(distance(up[3], [0.0, 100.0]) < 0.01, "{:?}", up[3]);
        assert!(
            distance(up[1], [0.0, 25.0]) < 0.01,
            "the line stays straight"
        );
        // Dragging back to where it was leaves the line as corrected.
        let back = dragged(&line, pivot(&line), [100.0, 0.0], [100.0, 0.0]);
        assert!(back.iter().zip(&line).all(|(a, b)| distance(*a, *b) < 0.01));

        // A corrected circle: the whole shape turns and grows about its
        // middle, and stays a circle.
        let round: Vec<[f32; 2]> = (0..=32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                [100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin()]
            })
            .collect();
        let middle = pivot(&round);
        assert!(distance(middle, [100.0, 100.0]) < 0.01, "{middle:?}");
        let grabbed = round[8];
        let twice = dragged(&round, middle, grabbed, [middle[0] + 100.0, middle[1]]);
        for point in &twice {
            assert!(
                (distance(*point, middle) - 100.0).abs() < 0.5,
                "not twice as far: {:?}",
                distance(*point, middle)
            );
        }
        // And the same shape read off a stroke as people draw it: a rough
        // line's pivot is its first point, a rough circle's its middle.
        let rough: Vec<[f32; 2]> = (0..=20).map(|i| [i as f32 * 5.0, 0.0]).collect();
        let corrected = recognize(&wobbly(&rough), 2.0).expect("a corrected line");
        assert!(distance(pivot(&corrected), corrected[0]) < 0.01);
        let drawn: Vec<[f32; 2]> = (0..=60)
            .map(|i| {
                let a = i as f32 / 60.0 * std::f32::consts::TAU;
                [100.0 + 60.0 * a.cos(), 100.0 + 40.0 * a.sin()]
            })
            .collect();
        let corrected = recognize(&wobbly(&drawn), 2.0).expect("a corrected ellipse");
        assert!(distance(pivot(&corrected), [100.0, 100.0]) < 2.0);
    }

    /// A stroke that strays in one place is read by what it mostly lies on:
    /// the reading skips the samples that stray, where a test of every
    /// sample gives up on the whole stroke. Clip Studio Paint's QuickShape
    /// reads figures the same way.
    #[test]
    fn a_stroke_that_strays_in_one_place_is_still_read() {
        // A line with a spike in it comes back the line, not the spike.
        let mut line: Vec<[f32; 2]> = (0..=50).map(|i| [i as f32 * 4.0, 0.0]).collect();
        for point in line.iter_mut().take(27).skip(24) {
            point[1] += 40.0;
        }
        let shape = recognize(&line, 2.0).expect("the line, spike and all");
        assert_eq!(simplify(&shape, 1.0).len(), 2, "not the line: {shape:?}");
        assert!(distance(shape[0], [0.0, 0.0]) < 1.0);
        assert!(distance(*shape.last().unwrap(), [200.0, 0.0]) < 1.0);

        // An arc with a bite out of it comes back its circle: the shape
        // follows the circle, not the bite.
        let ideal = |at: usize| {
            let angle = std::f32::consts::PI * (1.05 + at as f32 / 60.0 * 0.9);
            [150.0 + 100.0 * angle.cos(), 150.0 + 100.0 * angle.sin()]
        };
        let mut bitten: Vec<[f32; 2]> = (0..=60).map(ideal).collect();
        for point in bitten.iter_mut().take(34).skip(26) {
            point[0] += 30.0;
            point[1] += 30.0;
        }
        let shape = recognize(&bitten, 2.0).expect("the arc, bite and all");
        for at in 28..=32 {
            let near = shape
                .iter()
                .map(|p| distance(*p, ideal(at)))
                .fold(f32::MAX, f32::min);
            assert!(near < 12.0, "the bite was followed, {near:.0} out");
        }
    }

    /// A nearly closed circle comes back as the arc it drew, not the gap
    /// it left: the reading used to answer the short way round whenever it
    /// had kept only the ends of a clean arc on a small, quickly drawn
    /// stroke, and the figure collapsed to a short dash.
    #[test]
    fn a_nearly_closed_circle_comes_back_as_the_arc_it_drew() {
        let r = 40.0f32;
        let points: Vec<[f32; 2]> = (0..=37)
            .map(|i| {
                let a = (std::f32::consts::TAU - 50.0f32.to_radians()) * i as f32 / 37.0;
                [200.0 + r * a.cos(), 200.0 + r * a.sin()]
            })
            .collect();
        let shape = recognize(&points, 2.0).expect("an arc");
        let len: f32 = shape.windows(2).map(|w| distance(w[0], w[1])).sum();
        let round = std::f32::consts::TAU * r;
        assert!(
            len > round * 0.8,
            "the whole arc, not its gap: {len:.1} of {round:.1}"
        );
    }

    /// A drag reports how far it sized the figure, so the brush can thicken
    /// or thin the line with it, as Clip Studio Paint scales a line's width
    /// when a figure grows.
    #[test]
    fn a_drag_reports_how_far_it_sized_the_shape() {
        let line = [[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]];
        let (_, scale) = dragged_scaled(&line, [0.0, 0.0], [100.0, 0.0], [200.0, 0.0]);
        assert!((scale - 2.0).abs() < 1e-3, "scale {scale}");
        let (_, smaller) = dragged_scaled(&line, [0.0, 0.0], [100.0, 0.0], [50.0, 0.0]);
        assert!((smaller - 0.5).abs() < 1e-3, "scale {smaller}");
    }
}
