//! Sharp angles. With it on (the default) a corner the pen makes stays a
//! corner; off, corners are rounded within the brush's size, so a quick
//! zigzag reads as a soft wave. Straight runs and gentle curves are left
//! exactly where they are: only where the line turns sharply is a point
//! moved, toward the mean of the line around it.

use super::math::smoothstep;
use crate::input::StrokePoint;

/// How far around a point the rounding reads, in the point's sizes.
pub const WINDOW: f32 = 1.5;

/// The place of point `i` of `points` with its corner rounded, reading
/// the points within [`WINDOW`] sizes of arc either way.
#[must_use]
pub fn rounded(points: &[StrokePoint], i: usize) -> [f32; 2] {
    let p = &points[i];
    let window = (p.size * WINDOW).max(1.0);
    // The line `window` back and ahead, and the mean of the points
    // between, weighted by how near they are along the line.
    let mut back = p.pos;
    let mut ahead = p.pos;
    let mut sum = [0.0_f32; 2];
    let mut weight = 0.0_f32;
    for q in points[..=i].iter().rev() {
        let d = p.arc - q.arc;
        if d > window {
            break;
        }
        back = q.pos;
        let w = 1.0 - d / window;
        sum = [sum[0] + q.pos[0] * w, sum[1] + q.pos[1] * w];
        weight += w;
    }
    for q in &points[i + 1..] {
        let d = q.arc - p.arc;
        if d > window {
            break;
        }
        ahead = q.pos;
        let w = 1.0 - d / window;
        sum = [sum[0] + q.pos[0] * w, sum[1] + q.pos[1] * w];
        weight += w;
    }
    let (ux, uy) = (p.pos[0] - back[0], p.pos[1] - back[1]);
    let (vx, vy) = (ahead[0] - p.pos[0], ahead[1] - p.pos[1]);
    let (lu, lv) = ((ux * ux + uy * uy).sqrt(), (vx * vx + vy * vy).sqrt());
    if lu < 1.0e-3 || lv < 1.0e-3 || weight <= 0.0 {
        return p.pos;
    }
    // The cosine of the turn: 1 straight on, -1 straight back.
    let turn = (ux * vx + uy * vy) / (lu * lv);
    // A turn of up to about 25 degrees is a curve, from about 70 a
    // corner.
    let corner = smoothstep(0.9, 0.35, turn);
    let mean = [sum[0] / weight, sum[1] / weight];
    [
        p.pos[0] + (mean[0] - p.pos[0]) * corner,
        p.pos[1] + (mean[1] - p.pos[1]) * corner,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pos: [f32; 2], arc: f32) -> StrokePoint {
        StrokePoint {
            pos,
            arc,
            time: 0.0,
            pressure: 1.0,
            speed: 0.0,
            tilt: [0.0; 2],
            twist: 0.0,
            direction: 0.0,
            taper: 1.0,
            size: 16.0,
            opacity: 1.0,
            flow: 1.0,
            density: 1.0,
            hardness: 1.0,
            aspect: 1.0,
            angle: 0.0,
            spacing: 0.1,
            scatter: 0.0,
            texture_strength: 1.0,
            hue: 0.0,
            saturation: 0.0,
            luminosity: 0.0,
            extra: Vec::new(),
        }
    }

    /// Right along y = 0 to x = 40, then down to y = 40.
    fn elbow() -> Vec<StrokePoint> {
        let mut points = Vec::new();
        for k in 0..=40 {
            points.push(at([k as f32, 0.0], k as f32));
        }
        for k in 1..=40 {
            points.push(at([40.0, k as f32], 40.0 + k as f32));
        }
        points
    }

    #[test]
    fn a_corner_is_rounded_and_a_straight_run_is_left_alone() {
        let points = elbow();
        let corner = rounded(&points, 40);
        assert!(corner[0] < 37.0 && corner[1] > 3.0, "{corner:?}");
        assert_eq!(rounded(&points, 10), [10.0, 0.0]);
        assert_eq!(rounded(&points, 70), [40.0, 30.0]);
    }

    #[test]
    fn a_gentle_curve_is_not_moved() {
        // A circle of radius 200: over the window it turns a few degrees.
        let points: Vec<StrokePoint> = (0..100)
            .map(|k| {
                let a = k as f64 * 0.005;
                at(
                    [(200.0 * a.cos()) as f32, (200.0 * a.sin()) as f32],
                    (k as f64 * 200.0 * 0.005) as f32,
                )
            })
            .collect();
        let p = rounded(&points, 50);
        let q = points[50].pos;
        assert!((p[0] - q[0]).abs() < 1.0e-4 && (p[1] - q[1]).abs() < 1.0e-4);
    }
}
