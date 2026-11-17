//! Smart Shape (Clip Studio Paint 5.0): hold the pen still at the end of a
//! stroke and the stroke becomes the clean shape it was meant to be, a
//! straight line, an arc, an ellipse or circle, or a polygon such as a
//! rectangle or triangle. [`recognize`] reads the stroke's points and
//! answers the shape's outline, densely, for the brush to draw again.

/// How long the pen rests before the stroke turns into its shape, seconds.
pub const HOLD_SECONDS: f64 = 0.7;
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
    // A hand that slips once (a spike, a small correction) still drew a
    // line: the samples that strayed are left out of the reading, as
    // long as they are few.
    let off = most(points.iter().map(|p| line_distance(*p, first, last)));
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
    // An arc: the circle that fits the samples best (least squares),
    // fitted again over the samples that stayed with it, so a bite out
    // of the arc does not pull the circle away or throw the reading out.
    let middle = points[points.len() / 2];
    let near = |center: [f32; 2], radius: f32| {
        move |p: &[f32; 2]| (distance(*p, center) - radius).abs() < radius.max(1.0) * 0.08
    };
    let circle = circle_fit(&points)
        .map(|(center, radius)| {
            let kept: Vec<[f32; 2]> = points
                .iter()
                .copied()
                .filter(near(center, radius))
                .collect();
            circle_fit(&kept).unwrap_or((center, radius))
        })
        .or_else(|| circle_through(first, middle, last));
    if let Some((center, radius)) = circle
        && radius < diagonal * 4.0
    {
        let with_it = points.iter().filter(|p| near(center, radius)(p)).count();
        let fits = with_it as f32 >= points.len() as f32 * KEPT
            && near(center, radius)(&first)
            && near(center, radius)(&last);
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

/// The share of a stroke's samples a shape has to hold: the few that
/// stray (a spike, a bite) do not count against it.
const KEPT: f32 = 0.9;

/// The largest of `values` once the few that stray most are left out
/// (the share past [`KEPT`]).
fn most(values: impl Iterator<Item = f32>) -> f32 {
    let mut values: Vec<f32> = values.collect();
    values.sort_by(f32::total_cmp);
    let kept = ((values.len() as f32 * KEPT).ceil() as usize).clamp(1, values.len().max(1));
    values.get(kept - 1).copied().unwrap_or(0.0)
}

/// The circle that lies closest to `points` in the least-squares sense
/// (the algebraic fit: x^2 + y^2 + a x + b y + c = 0 solved for a, b and
/// c). `None` for fewer than three points or points on a line.
fn circle_fit(points: &[[f32; 2]]) -> Option<([f32; 2], f32)> {
    if points.len() < 3 {
        return None;
    }
    let count = points.len() as f64;
    // About the mean, for the sums to stay small.
    let (mx, my) = points.iter().fold((0.0_f64, 0.0_f64), |(x, y), p| {
        (x + f64::from(p[0]), y + f64::from(p[1]))
    });
    let (mx, my) = (mx / count, my / count);
    let (mut suu, mut suv, mut svv, mut suuu, mut svvv, mut suvv, mut svuu) = (
        0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64,
    );
    for p in points {
        let (u, v) = (f64::from(p[0]) - mx, f64::from(p[1]) - my);
        suu += u * u;
        suv += u * v;
        svv += v * v;
        suuu += u * u * u;
        svvv += v * v * v;
        suvv += u * v * v;
        svuu += v * u * u;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-6 * (suu + svv).max(1.0).powi(2) {
        return None;
    }
    let (right_u, right_v) = ((suuu + suvv) / 2.0, (svvv + svuu) / 2.0);
    let uc = (right_u * svv - right_v * suv) / det;
    let vc = (right_v * suu - right_u * suv) / det;
    let radius = (uc * uc + vc * vc + (suu + svv) / count).sqrt();
    radius
        .is_finite()
        .then_some(([(uc + mx) as f32, (vc + my) as f32], radius as f32))
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
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

/// The shape `path` as a drag that carries its `reference` point to `to`
/// leaves it: turned and scaled about `pivot`, from the shape as it was
/// corrected, so dragging back and forth never drifts. The size stays
/// between a twentieth and twenty times the corrected one, so a pen
/// dragged onto the pivot leaves a shape rather than a point.
#[must_use]
pub fn dragged(
    path: &[[f32; 2]],
    pivot: [f32; 2],
    reference: [f32; 2],
    to: [f32; 2],
) -> Vec<[f32; 2]> {
    let arm = |point: [f32; 2]| [point[0] - pivot[0], point[1] - pivot[1]];
    let (from, now) = (arm(reference), arm(to));
    let length = from[0].hypot(from[1]);
    if length < 1e-3 {
        return path.to_vec();
    }
    let scale = (now[0].hypot(now[1]) / length).clamp(0.05, 20.0);
    let turn = wrap(now[1].atan2(now[0]) - from[1].atan2(from[0]));
    let (sin, cos) = turn.sin_cos();
    path.iter()
        .map(|point| {
            let [x, y] = arm(*point);
            let (x, y) = (x * scale, y * scale);
            [pivot[0] + x * cos - y * sin, pivot[1] + x * sin + y * cos]
        })
        .collect()
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

    /// A hand that slips once still drew the shape: a line with a spike
    /// 40 pixels high is read as the line, and an arc with a bite out of
    /// it as the arc, on the circle it was drawn on.
    #[test]
    fn a_spike_in_a_line_and_a_bite_in_an_arc_are_read_past() {
        let mut line: Vec<[f32; 2]> = (0..=100)
            .map(|i| [i as f32 * 4.0, i as f32 * 1.5])
            .collect();
        for (index, up) in [(48, 20.0), (49, 40.0), (50, 40.0), (51, 20.0)] {
            line[index][1] -= up;
        }
        let read = recognize(&line, 2.0).expect("a line");
        assert_eq!(kind(&line), "line");
        let (first, last) = (read[0], read[read.len() - 1]);
        assert!(distance(first, [0.0, 0.0]) < 1.0 && distance(last, [400.0, 150.0]) < 1.0);

        let mut arc: Vec<[f32; 2]> = (0..=80)
            .map(|i| {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 80.0 * 0.7);
                [100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin()]
            })
            .collect();
        for point in &mut arc[38..43] {
            // A bite toward the middle of the circle.
            point[0] += (100.0 - point[0]) * 0.2;
            point[1] += (100.0 - point[1]) * 0.2;
        }
        assert_eq!(kind(&arc), "arc");
        let read = recognize(&arc, 2.0).expect("an arc");
        for point in &read {
            let off = (distance(*point, [100.0, 100.0]) - 80.0).abs();
            assert!(off < 2.0, "the arc lies on its circle: {off}");
        }
    }

    #[test]
    fn the_best_circle_through_samples_is_found() {
        let circle: Vec<[f32; 2]> = (0..30)
            .map(|i| {
                let a = i as f32 / 30.0 * 2.0;
                [40.0 + 25.0 * a.cos(), -10.0 + 25.0 * a.sin()]
            })
            .collect();
        let (center, radius) = circle_fit(&circle).expect("a circle");
        assert!(distance(center, [40.0, -10.0]) < 0.05 && (radius - 25.0).abs() < 0.05);
        let line: Vec<[f32; 2]> = (0..30).map(|i| [i as f32, i as f32 * 2.0]).collect();
        assert_eq!(circle_fit(&line), None);
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
}
