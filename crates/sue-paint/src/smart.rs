//! Smart Shape (Clip Studio Paint 5.0): hold the pen still at the end of a
//! stroke and the stroke becomes the clean shape it was meant to be, a
//! straight line, an arc, an ellipse or circle, or a polygon such as a
//! rectangle or triangle. [`recognize`] reads the stroke's points and
//! answers the shape's outline, densely, for the brush to draw again.

/// How long the pen rests before the stroke turns into its shape, seconds.
pub const HOLD_SECONDS: f64 = 0.6;
/// How far the pen may wander while resting, screen points.
pub const HOLD_REACH: f32 = 4.0;

/// The clean shape `points` (a stroke as drawn) were meant to be, as a
/// path of points at most `step` apart; `None` when no shape fits well.
#[must_use]
pub fn recognize(points: &[[f32; 2]], step: f32) -> Option<Vec<[f32; 2]>> {
    let points: Vec<[f32; 2]> = points.to_vec();
    if points.len() < 3 {
        return None;
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let [x0, y0, x1, y1] = bounds(&points);
    let diagonal = (x1 - x0).hypot(y1 - y0);
    if diagonal < 8.0 {
        return None;
    }
    let length: f32 = points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum();
    // A straight line: nowhere far from the line between its ends.
    let chord = distance(first, last);
    let off = points
        .iter()
        .map(|p| line_distance(*p, first, last))
        .fold(0.0, f32::max);
    if chord > diagonal * 0.8 && off < chord * 0.06 {
        return Some(densify(&[first, last], step, false));
    }
    let closed = chord < diagonal * 0.22 && length > diagonal * 1.8;
    if closed {
        // A polygon of a few corners, before an ellipse: a rectangle's
        // corners would fit an ellipse badly anyway.
        let corners = simplify_closed(&points, diagonal * 0.07);
        if (3..=6).contains(&corners.len()) && polygon_fits(&points, &corners, diagonal * 0.05) {
            return Some(densify(&corners, step, true));
        }
        let (center, rx, ry) = (
            [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
            (x1 - x0) / 2.0,
            (y1 - y0) / 2.0,
        );
        let error = points
            .iter()
            .map(|p| {
                let (dx, dy) = (
                    (p[0] - center[0]) / rx.max(1.0),
                    (p[1] - center[1]) / ry.max(1.0),
                );
                (dx.hypot(dy) - 1.0).abs()
            })
            .sum::<f32>()
            / points.len() as f32;
        if error < 0.12 {
            // Nearly round is a circle.
            let (rx, ry) = if (rx - ry).abs() < rx.max(ry) * 0.12 {
                let r = (rx + ry) / 2.0;
                (r, r)
            } else {
                (rx, ry)
            };
            let around = std::f32::consts::PI * (rx + ry);
            let count = ((around / step.max(0.5)).ceil() as usize).clamp(24, 4096);
            let mut out: Vec<[f32; 2]> = (0..=count)
                .map(|index| {
                    // From where the stroke began, round the same way.
                    let start = (first[1] - center[1]).atan2(first[0] - center[0]);
                    let turn = if turning(&points) > 0.0 { 1.0 } else { -1.0 };
                    let angle = start + turn * index as f32 / count as f32 * std::f32::consts::TAU;
                    [center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]
                })
                .collect();
            out.dedup();
            return Some(out);
        }
        return None;
    }
    // An arc: a circle through both ends and the middle, close all along.
    let middle = points[points.len() / 2];
    if let Some((center, radius)) = circle_through(first, middle, last)
        && radius < diagonal * 4.0
    {
        let fits = points
            .iter()
            .all(|p| (distance(*p, center) - radius).abs() < radius.max(1.0) * 0.08);
        if fits {
            let angle = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
            let (a0, am, a1) = (angle(first), angle(middle), angle(last));
            // Sweep from the first end through the middle to the last.
            let mut sweep = a1 - a0;
            let toward_middle = wrap(am - a0);
            if wrap(sweep).signum() != toward_middle.signum()
                || wrap(sweep).abs() < toward_middle.abs()
            {
                sweep = if toward_middle > 0.0 {
                    wrap(sweep).rem_euclid(std::f32::consts::TAU)
                } else {
                    -(-wrap(sweep)).rem_euclid(std::f32::consts::TAU)
                };
            } else {
                sweep = wrap(sweep);
            }
            let count = ((sweep.abs() * radius / step.max(0.5)).ceil() as usize).clamp(8, 4096);
            return Some(
                (0..=count)
                    .map(|index| {
                        let angle = a0 + sweep * index as f32 / count as f32;
                        [
                            center[0] + radius * angle.cos(),
                            center[1] + radius * angle.sin(),
                        ]
                    })
                    .collect(),
            );
        }
    }
    // An open polyline of a few straight sides.
    let corners = simplify(&points, diagonal * 0.06);
    if (3..=5).contains(&corners.len()) && polygon_fits(&points, &corners, diagonal * 0.05) {
        return Some(densify(&corners, step, false));
    }
    None
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
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

/// The circle through three points, if they are not in a line.
fn circle_through(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Option<([f32; 2], f32)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-6 {
        return None;
    }
    let sq = |p: [f32; 2]| p[0] * p[0] + p[1] * p[1];
    let x = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let y = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let center = [x, y];
    Some((center, distance(center, a)))
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

/// Whether every point lies within `reach` of the sides through `corners`.
fn polygon_fits(points: &[[f32; 2]], corners: &[[f32; 2]], reach: f32) -> bool {
    let sides: Vec<([f32; 2], [f32; 2])> = corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(a, b)| (*a, *b))
        .collect();
    points.iter().all(|p| {
        sides
            .iter()
            .map(|(a, b)| line_distance(*p, *a, *b))
            .fold(f32::MAX, f32::min)
            <= reach
    })
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
        // A scribble stays as drawn.
        let scribble: Vec<[f32; 2]> = (0..80)
            .map(|i| {
                let t = i as f32 * 0.37;
                [
                    t * 9.0 + (t * 3.1).sin() * 30.0,
                    (t * 1.7).cos() * 40.0 + (t * 5.3).sin() * 25.0,
                ]
            })
            .collect();
        assert_eq!(kind(&scribble), "none");
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
}
