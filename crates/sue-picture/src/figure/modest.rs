//! A body without its anatomical detail: the nipples smoothed into the
//! chest and the shape between the legs made a smooth, neutral
//! mannequin form, for both body types.
//!
//! It is done to the mesh as modeled, once each time a body is shaped:
//! each region is found on the body (so it follows the type, the chart
//! and every slider), worked smooth, and blended back into the skin
//! round it by a soft falloff. Nothing is hidden or cut, the points keep
//! their skin weights, and the surface stays one closed piece, so no
//! seam shows from any side or in any pose.

use super::human::at;

/// A point's place, as modeled: meters, facing +z, its left at +x.
type Point = [f32; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: Point) -> f32 {
    dot(a, a).sqrt()
}

/// 0 at `from`, 1 at `to`, eased in between.
fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Each point's neighbors along the edges of `quads`.
#[must_use]
pub(super) fn neighbors(count: usize, quads: &[[u32; 4]]) -> Vec<Vec<u32>> {
    let mut near: Vec<Vec<u32>> = vec![Vec::new(); count];
    for quad in quads {
        for k in 0..4 {
            let (a, b) = (quad[k], quad[(k + 1) % 4]);
            for (from, to) in [(a, b), (b, a)] {
                let list = &mut near[from as usize];
                if !list.contains(&to) {
                    list.push(to);
                }
            }
        }
    }
    near
}

/// The mean of point `i`'s neighbors.
fn mean_of(points: &[Point], near: &[u32]) -> Option<Point> {
    if near.is_empty() {
        return None;
    }
    let mut sum = [0.0_f32; 3];
    for &j in near {
        for (axis, value) in sum.iter_mut().enumerate() {
            *value += points[j as usize][axis];
        }
    }
    let share = 1.0 / near.len() as f32;
    Some(sum.map(|value| value * share))
}

/// The outward normal at every point, from the quads round it.
fn normals(points: &[Point], quads: &[[u32; 4]]) -> Vec<Point> {
    let mut out = vec![[0.0_f32; 3]; points.len()];
    for quad in quads {
        let [a, b, c, d] = quad.map(|i| points[i as usize]);
        let face = cross(sub(c, a), sub(d, b));
        for &i in quad {
            for (axis, value) in out[i as usize].iter_mut().enumerate() {
                *value += face[axis];
            }
        }
    }
    for normal in &mut out {
        let long = length(*normal);
        if long > 0.0 {
            *normal = normal.map(|value| value / long);
        }
    }
    out
}

/// Where each nipple is: on each side of the chest, the point that
/// stands out most sharply from the skin round it (its own small bump,
/// not the chest's or bust's broad curve).
fn nipples(
    points: &[Point],
    near: &[Vec<u32>],
    normal: &[Point],
    ends: &[[Point; 2]],
) -> Vec<Point> {
    let chest = ends[at::CHEST][0];
    let neck = ends[at::NECK][0];
    let shoulders = [0, 1].map(|side| ends[at::ARMS[side][0]][0]);
    // How sharply each point stands out, smoothed once over its
    // neighbors so the nipple's middle wins over a single bent point.
    let sharp: Vec<f32> = (0..points.len())
        .map(|i| {
            let Some(mean) = mean_of(points, &near[i]) else {
                return 0.0;
            };
            let spread = near[i]
                .iter()
                .map(|&j| length(sub(points[j as usize], points[i])))
                .sum::<f32>()
                / near[i].len() as f32;
            dot(sub(points[i], mean), normal[i]) / (spread * spread).max(1e-8)
        })
        .collect();
    let mut found = Vec::new();
    for (side, shoulder) in shoulders.iter().enumerate() {
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let reach = shoulder[0] * sign;
        let best = (0..points.len())
            .filter(|&i| {
                let p = points[i];
                let across = p[0] * sign;
                across > reach * 0.2
                    && across < reach * 0.95
                    && p[1] > chest[1] - 0.05
                    && p[1] < neck[1] - 0.04
                    && p[2] > chest[2]
                    && normal[i][2] > 0.5
            })
            .map(|i| {
                let around: f32 = near[i].iter().map(|&j| sharp[j as usize]).sum();
                (i, sharp[i] + around / near[i].len().max(1) as f32)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            found.push(points[i]);
        }
    }
    found
}

/// Smooths the points `weight` marks (above 0), the rest held, by
/// `steps` rounds of Taubin smoothing (a step toward the neighbors' mean,
/// then a slightly larger one back, which takes out small bumps and
/// keeps the broad form without shrinking it); each point then moves
/// `weight` of the way to its smoothed place.
fn smoothed(points: &mut [Point], near: &[Vec<u32>], weight: &[f32], steps: usize) {
    let free: Vec<usize> = (0..points.len()).filter(|&i| weight[i] > 0.0).collect();
    if free.is_empty() {
        return;
    }
    let mut work = points.to_vec();
    let mut next = vec![[0.0_f32; 3]; free.len()];
    for step in 0..steps * 2 {
        let by = if step % 2 == 0 { 0.6 } else { -0.63 };
        for (slot, &i) in next.iter_mut().zip(&free) {
            *slot = mean_of(&work, &near[i]).map_or(work[i], |mean| {
                [0, 1, 2].map(|axis| work[i][axis] + by * (mean[axis] - work[i][axis]))
            });
        }
        for (made, &i) in next.iter().zip(&free) {
            work[i] = *made;
        }
    }
    for &i in &free {
        let w = weight[i];
        points[i] = [0, 1, 2].map(|axis| points[i][axis] + w * (work[i][axis] - points[i][axis]));
    }
}

/// Solves the 6 by 6 system `m x = r` (Gaussian elimination with
/// partial pivoting); `None` when it has no single answer.
fn solve6(mut m: [[f64; 6]; 6], mut r: [f64; 6]) -> Option<[f64; 6]> {
    for col in 0..6 {
        let pivot = (col..6).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        r.swap(col, pivot);
        for row in col + 1..6 {
            let by = m[row][col] / m[col][col];
            let pivot_row = m[col];
            for (value, above) in m[row].iter_mut().zip(pivot_row).skip(col) {
                *value -= by * above;
            }
            r[row] -= by * r[col];
        }
    }
    let mut x = [0.0_f64; 6];
    for row in (0..6).rev() {
        let mut sum = r[row];
        for k in row + 1..6 {
            sum -= m[row][k] * x[k];
        }
        x[row] = sum / m[row][row];
    }
    Some(x)
}

/// Lays the skin round `at` (within `outer` of it, whole within `inner`)
/// on the smooth surface the skin just outside it makes: a curved patch
/// (height above the skin's own plane as a quadratic of the place
/// across it) fitted to the ring of skin between `inner` and a little
/// past `outer`, so the chest or bust goes on over the place without a
/// bump, a dip or an edge. Marks how far each point moved in `worked`.
fn flattened(
    points: &mut [Point],
    normal: &[Point],
    at: Point,
    inner: f32,
    outer: f32,
    worked: &mut [f32],
) {
    // The skin's own facing there, and two directions across it.
    let mut facing = [0.0_f32; 3];
    for (p, n) in points.iter().zip(normal) {
        if length(sub(*p, at)) < outer {
            for axis in 0..3 {
                facing[axis] += n[axis];
            }
        }
    }
    let long = length(facing);
    if long <= 0.0 {
        return;
    }
    let up = facing.map(|value| value / long);
    let side = if up[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let across = cross(up, side);
    let across = across.map(|value| value / length(across).max(1e-9));
    let along = cross(up, across);
    // The points on this side of the body near the place, in its frame.
    let local: Vec<(usize, f32, f32, f32)> = points
        .iter()
        .enumerate()
        .filter(|(i, p)| length(sub(**p, at)) < outer * 1.4 && dot(normal[*i], up) > 0.2)
        .map(|(i, p)| {
            let d = sub(*p, at);
            (i, dot(d, across), dot(d, along), dot(d, up))
        })
        .collect();
    let terms = |x: f32, y: f32| [1.0, x, y, x * x, x * y, y * y].map(f64::from);
    let mut m = [[0.0_f64; 6]; 6];
    let mut r = [0.0_f64; 6];
    for &(_, x, y, h) in &local {
        let reach = (x * x + y * y).sqrt();
        if reach < inner || reach > outer * 1.4 {
            continue;
        }
        let t = terms(x, y);
        for row in 0..6 {
            for col in 0..6 {
                m[row][col] += t[row] * t[col];
            }
            r[row] += t[row] * f64::from(h);
        }
    }
    let Some(fit) = solve6(m, r) else {
        return;
    };
    for &(i, x, y, h) in &local {
        let reach = (x * x + y * y).sqrt();
        let w = 1.0 - smoothstep(inner, outer, reach);
        if w <= 0.0 {
            continue;
        }
        let t = terms(x, y);
        let target = (0..6).map(|k| fit[k] * t[k]).sum::<f64>() as f32;
        let by = w * (target - h);
        for axis in 0..3 {
            points[i][axis] += by * up[axis];
        }
        worked[i] = worked[i].max(w);
    }
}

/// How many rounds [`smoothed`] gives the skin between the legs.
const CROTCH_STEPS: usize = 250;

/// Takes the anatomical detail off a body's `points` (as modeled, with
/// its joints' `ends`), its mesh `quads` and their `near` neighbors.
/// Answers how much each point was worked (0 where it was left alone).
pub(super) fn modest(
    points: &mut [Point],
    near: &[Vec<u32>],
    quads: &[[u32; 4]],
    ends: &[[Point; 2]],
) -> Vec<f32> {
    // The nipples: the chest or bust carried smoothly over where each
    // was.
    let normal = normals(points, quads);
    let chest_scale = length(sub(ends[at::ARMS[0][0]][0], ends[at::ARMS[1][0]][0])).max(0.1);
    let mut worked = vec![0.0_f32; points.len()];
    for nipple in nipples(points, near, &normal, ends) {
        flattened(
            points,
            &normal,
            nipple,
            chest_scale * 0.06,
            chest_scale * 0.12,
            &mut worked,
        );
    }
    // Between the legs: the genitals lie below and in front of the
    // middle of the thighs' heads. The skin there (and a little behind)
    // is smoothed into one rounded form, its broad shape kept.
    let thighs = [0, 1].map(|side| ends[at::LEGS[side][0]][0]);
    let wide = (thighs[0][0] - thighs[1][0]).abs().max(0.05);
    let top = [
        0.0,
        (thighs[0][1] + thighs[1][1]) / 2.0,
        (thighs[0][2] + thighs[1][2]) / 2.0,
    ];
    let middle = [0.0, top[1] - wide * 0.3, top[2] + wide * 0.3];
    let weight: Vec<f32> = points
        .iter()
        .map(|p| {
            let d = sub(*p, middle);
            // An egg a little longer up and down than across.
            let reach = length([d[0] * 1.15, d[1] * 0.85, d[2]]);
            let near_middle = 1.0 - smoothstep(wide * 0.3, wide * 0.6, reach);
            let in_front = smoothstep(top[2] - wide * 0.5, top[2] - wide * 0.2, p[2]);
            near_middle * in_front
        })
        .collect();
    smoothed(points, near, &weight, CROTCH_STEPS);
    for (worked, weight) in worked.iter_mut().zip(weight) {
        *worked = worked.max(weight);
    }
    worked
}
