//! Correct Line, as in Clip Studio Paint: tools that rework vector lines
//! after they are drawn. Control point moves, adds, deletes, turns into
//! corners, widens, fades and splits at a line's control points; Pinch
//! vector line pulls a stretch of a line along; Simplify vector line
//! smooths what the brush passes over; Connect vector lines joins ends that
//! meet; Adjust line width thickens, narrows, scales or fixes the width;
//! Redraw vector line and Redraw vector line width trace over a line to
//! reshape it or give it the pen's pressure. Remove dust takes small specks
//! off a raster layer.
//!
//! A line's control points are its samples flagged [`CONTROL`], or, for a
//! line without flags, the samples a simplified copy of it keeps.

use crate::brush::Sample;
use crate::vector::{self, CONTROL, CORNER, Line};

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far `p` is from the segment `a` to `b`.
fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    distance(p, [a[0] + dx * t, a[1] + dy * t])
}

/// A point `u` of the way from `p1` to `p2` on a Catmull-Rom curve.
fn catmull_rom([p0, p1, p2, p3]: [[f32; 2]; 4], u: f32) -> [f32; 2] {
    let (u2, u3) = (u * u, u * u * u);
    [0, 1].map(|axis| {
        0.5 * (2.0 * p1[axis]
            + (p2[axis] - p0[axis]) * u
            + (2.0 * p0[axis] - 5.0 * p1[axis] + 4.0 * p2[axis] - p3[axis]) * u2
            + (3.0 * p1[axis] - p0[axis] - 3.0 * p2[axis] + p3[axis]) * u3)
    })
}

/// The indices of `samples` a copy simplified within `tolerance` pixels
/// keeps (Douglas and Peucker), the first and last always.
#[must_use]
pub fn simplified(samples: &[Sample], tolerance: f32) -> Vec<usize> {
    if samples.len() <= 2 {
        return (0..samples.len()).collect();
    }
    let last = samples.len() - 1;
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[last] = true;
    let mut stack = vec![(0, last)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut far, mut index) = (0.0, a);
        for (k, sample) in samples.iter().enumerate().take(b).skip(a + 1) {
            let away = segment_distance(sample.at, samples[a].at, samples[b].at);
            if away > far {
                far = away;
                index = k;
            }
        }
        if far > tolerance {
            keep[index] = true;
            stack.push((a, index));
            stack.push((index, b));
        }
    }
    (0..samples.len()).filter(|index| keep[*index]).collect()
}

/// `points` simplified within `tolerance` pixels.
#[must_use]
pub fn simplified_points(points: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    let samples: Vec<Sample> = points
        .iter()
        .map(|at| Sample {
            at: *at,
            pressure: 1.0,
            speed: 0.0,
            width: 1.0,
            alpha: 1.0,
        })
        .collect();
    simplified(&samples, tolerance)
        .into_iter()
        .map(|index| points[index])
        .collect()
}

impl Line {
    /// The line's control points, as sample indices in order.
    #[must_use]
    pub fn controls(&self) -> Vec<usize> {
        if self.marks.len() == self.samples.len()
            && self.marks.iter().any(|mark| mark & CONTROL != 0)
        {
            (0..self.samples.len())
                .filter(|index| self.marks[*index] & CONTROL != 0)
                .collect()
        } else {
            simplified(&self.samples, (self.tip.size * 0.25).clamp(1.0, 6.0))
        }
    }

    /// `true` if sample `index` is a corner.
    #[must_use]
    pub fn is_corner(&self, index: usize) -> bool {
        self.marks.get(index).is_some_and(|mark| mark & CORNER != 0)
    }

    /// Keeps the control points as flags, so they can change.
    fn fix_marks(&mut self) {
        if self.marks.len() == self.samples.len()
            && self.marks.iter().any(|mark| mark & CONTROL != 0)
        {
            return;
        }
        let controls = self.controls();
        self.marks = vec![0; self.samples.len()];
        for control in controls {
            self.marks[control] |= CONTROL;
        }
    }

    /// The control points around control `index`: the one before and the
    /// one after.
    fn neighbors(&self, index: usize) -> (Option<usize>, Option<usize>) {
        let controls = self.controls();
        let Some(at) = controls.iter().position(|control| *control == index) else {
            return (None, None);
        };
        (
            at.checked_sub(1).map(|before| controls[before]),
            controls.get(at + 1).copied(),
        )
    }

    /// Replaces the samples strictly between `from` and `to` with points
    /// `point(u)` (u from 0 to 1), their other values taken along the old
    /// path.
    fn replace_between(&mut self, from: usize, to: usize, point: impl Fn(f32) -> [f32; 2]) {
        let arcs = vector::arcs(&self.samples);
        let span = arcs[to] - arcs[from];
        let steps = span.ceil().max(1.0) as usize;
        let fresh: Vec<Sample> = (1..steps)
            .map(|step| {
                let u = step as f32 / steps as f32;
                Sample {
                    at: point(u),
                    ..vector::at_arc(&self.samples, &arcs, arcs[from] + span * u)
                }
            })
            .collect();
        self.fix_marks();
        let count = fresh.len();
        self.samples.splice(from + 1..to, fresh);
        self.marks
            .splice(from + 1..to, std::iter::repeat_n(0, count));
    }
}

/// The control point within `reach` of `at`, nearest: its line and sample.
#[must_use]
pub fn control_at(lines: &[Line], at: [f32; 2], reach: f32) -> Option<(usize, usize)> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(line, this)| this.controls().into_iter().map(move |index| (line, index)))
        .map(|(line, index)| ((line, index), distance(lines[line].samples[index].at, at)))
        .filter(|(_, away)| *away <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(hit, _)| hit)
}

/// The line within `reach` of `at`, nearest, and how far along it.
#[must_use]
pub fn line_at(lines: &[Line], at: [f32; 2], reach: f32) -> Option<(usize, f32)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let arcs = vector::arcs(&line.samples);
            let along = vector::touch(&line.samples, &arcs, at, reach)?;
            let away = distance(vector::at_arc(&line.samples, &arcs, along).at, at);
            Some(((index, along), away))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(hit, _)| hit)
}

/// How much sample `k` follows control `index`, from 1 there to 0 at the
/// controls either side.
fn follow(line: &Line, arcs: &[f32], index: usize, k: usize) -> f32 {
    let (before, after) = line.neighbors(index);
    if k == index {
        1.0
    } else if k < index {
        before.map_or(1.0, |from| {
            smooth((arcs[k] - arcs[from]) / (arcs[index] - arcs[from]).max(1e-3))
        })
    } else {
        after.map_or(1.0, |to| {
            smooth((arcs[to] - arcs[k]) / (arcs[to] - arcs[index]).max(1e-3))
        })
    }
}

/// The samples control `index` reaches: from the control before it to the
/// one after.
fn reach_of(line: &Line, index: usize) -> std::ops::RangeInclusive<usize> {
    let (before, after) = line.neighbors(index);
    before.unwrap_or(index)..=after.unwrap_or(index)
}

/// Move control points: control `index` of `line` moved by `by`, the path
/// to the controls either side following smoothly.
pub fn move_control(line: &mut Line, index: usize, by: [f32; 2]) {
    let arcs = vector::arcs(&line.samples);
    for k in reach_of(line, index) {
        let weight = follow(line, &arcs, index, k);
        line.samples[k].at[0] += by[0] * weight;
        line.samples[k].at[1] += by[1] * weight;
    }
}

/// Adjust line width (or, with `opacity`, opacity) at control `index`: the
/// width there times `factor`, less and less toward the controls either
/// side.
pub fn shape_control(line: &mut Line, index: usize, factor: f32, opacity: bool) {
    let arcs = vector::arcs(&line.samples);
    for k in reach_of(line, index) {
        let scale = 1.0 + (factor - 1.0) * follow(line, &arcs, index, k);
        let sample = &mut line.samples[k];
        if opacity {
            sample.alpha = (sample.alpha * scale).clamp(0.0, 1.0);
        } else {
            sample.width = (sample.width * scale).clamp(0.0, 20.0);
        }
    }
}

/// Add control point: a control point on `line` at `along` (distance along
/// it); returns its sample.
pub fn add_control(line: &mut Line, along: f32) -> usize {
    line.fix_marks();
    let arcs = vector::arcs(&line.samples);
    let near = arcs
        .partition_point(|arc| *arc < along)
        .min(line.samples.len() - 1);
    let closest = [near.saturating_sub(1), near]
        .into_iter()
        .min_by(|a, b| {
            (arcs[*a] - along)
                .abs()
                .total_cmp(&(arcs[*b] - along).abs())
        })
        .unwrap_or(near);
    let index = if (arcs[closest] - along).abs() <= 0.5 {
        closest
    } else {
        let sample = vector::at_arc(&line.samples, &arcs, along);
        line.samples.insert(near, sample);
        line.marks.insert(near, 0);
        near
    };
    line.marks[index] |= CONTROL;
    index
}

/// Delete control point `index`: the path between the controls either side
/// runs smoothly through the ones left, and an end goes back to the next
/// control. `false` when the line has too few to lose one.
pub fn delete_control(line: &mut Line, index: usize) -> bool {
    let controls = line.controls();
    let Some(at) = controls.iter().position(|control| *control == index) else {
        return false;
    };
    if controls.len() <= 2 {
        return false;
    }
    line.fix_marks();
    if at == 0 {
        line.samples.drain(..controls[1]);
        line.marks.drain(..controls[1]);
        return true;
    }
    if at == controls.len() - 1 {
        line.samples.truncate(controls[at - 1] + 1);
        line.marks.truncate(controls[at - 1] + 1);
        return true;
    }
    let point = |k: Option<usize>| k.map(|k| line.samples[k].at);
    let (before, after) = (controls[at - 1], controls[at + 1]);
    let p1 = line.samples[before].at;
    let p2 = line.samples[after].at;
    let p0 = point(at.checked_sub(2).map(|k| controls[k])).unwrap_or(p1);
    let p3 = point(controls.get(at + 2).copied()).unwrap_or(p2);
    line.marks[index] = 0;
    line.replace_between(before, after, |u| catmull_rom([p0, p1, p2, p3], u));
    true
}

/// Switch corner at control `index`: a corner runs straight to the
/// controls either side, a smooth point curves through them.
pub fn switch_corner(line: &mut Line, index: usize) {
    line.fix_marks();
    let corner = !line.is_corner(index);
    let controls = line.controls();
    let Some(at) = controls.iter().position(|control| *control == index) else {
        return;
    };
    if corner {
        line.marks[index] |= CORNER;
    } else {
        line.marks[index] &= !CORNER;
    }
    // Where the controls are, before any of the path moves.
    let points: Vec<[f32; 2]> = controls.iter().map(|k| line.samples[*k].at).collect();
    let position = |k: usize| points[k];
    // After first, so the indices before it stay put.
    if let Some(&after) = controls.get(at + 1) {
        let (p1, p2) = (position(at), position(at + 1));
        let p0 = at.checked_sub(1).map_or(p1, position);
        let p3 = if at + 2 < points.len() {
            position(at + 2)
        } else {
            p2
        };
        line.replace_between(index, after, |u| {
            if corner {
                [p1[0] + (p2[0] - p1[0]) * u, p1[1] + (p2[1] - p1[1]) * u]
            } else {
                catmull_rom([p0, p1, p2, p3], u)
            }
        });
    }
    if let Some(before) = at.checked_sub(1).map(|k| controls[k]) {
        let (p1, p2) = (position(at - 1), position(at));
        let p0 = at.checked_sub(2).map_or(p1, position);
        let p3 = if at + 1 < points.len() {
            position(at + 1)
        } else {
            p2
        };
        line.replace_between(before, index, |u| {
            if corner {
                [p1[0] + (p2[0] - p1[0]) * u, p1[1] + (p2[1] - p1[1]) * u]
            } else {
                catmull_rom([p0, p1, p2, p3], u)
            }
        });
    }
}

/// Split line: `lines[line]` becomes two at `along`.
pub fn split(lines: &mut Vec<Line>, line: usize, along: f32) -> bool {
    let this = &lines[line];
    let arcs = vector::arcs(&this.samples);
    let total = arcs.last().copied().unwrap_or(0.0);
    if along <= 0.5 || total - along <= 0.5 {
        return false;
    }
    let first = this.along(vector::part(&this.samples, &arcs, 0.0, along), false);
    let second = this.along(vector::part(&this.samples, &arcs, along, total), true);
    lines.splice(line..=line, [first, second]);
    true
}

/// Pinch vector line: the stretch of `base` within `range` of `along`
/// moved by `by`, sharper the higher `level` (1 to 5); with `fixed` the
/// line's ends stay where they are.
#[must_use]
pub fn pinch(base: &Line, along: f32, by: [f32; 2], range: f32, level: u32, fixed: bool) -> Line {
    let mut line = base.clone();
    let arcs = vector::arcs(&base.samples);
    let total = arcs.last().copied().unwrap_or(0.0);
    let hold = along.min(total - along).max(1.0);
    for (k, sample) in line.samples.iter_mut().enumerate() {
        let away = (arcs[k] - along).abs();
        if away >= range {
            continue;
        }
        let mut weight = smooth(1.0 - away / range.max(1e-3)).powi(level.clamp(1, 5) as i32);
        if fixed {
            weight *= (arcs[k].min(total - arcs[k]) / hold).clamp(0.0, 1.0);
        }
        sample.at[0] += by[0] * weight;
        sample.at[1] += by[1] * weight;
    }
    line
}

/// Simplify vector line: the parts of lines within `radius` of `at` drawn
/// toward a smooth curve, more passes the higher `level`; with `short`,
/// lines it touches that are shorter than the brush go.
pub fn simplify(lines: &mut Vec<Line>, at: [f32; 2], radius: f32, level: u32, short: bool) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index < lines.len() {
        let line = &mut lines[index];
        let reach = radius + line.tip.size * 0.25;
        let inside: Vec<usize> = (0..line.samples.len())
            .filter(|k| distance(line.samples[*k].at, at) <= reach)
            .collect();
        if inside.is_empty() {
            index += 1;
            continue;
        }
        changed = true;
        let total = vector::arcs(&line.samples).last().copied().unwrap_or(0.0);
        if short && total < radius * 2.0 {
            lines.remove(index);
            continue;
        }
        let last = line.samples.len() - 1;
        for _ in 0..level.clamp(1, 5) * 2 {
            let before: Vec<[f32; 2]> = line.samples.iter().map(|sample| sample.at).collect();
            for &k in inside.iter().filter(|k| **k > 0 && **k < last) {
                let weight = (1.0 - distance(before[k], at) / reach).clamp(0.0, 1.0);
                let (a, here, b) = (before[k - 1], before[k], before[k + 1]);
                line.samples[k].at = [0, 1].map(|axis| {
                    let mean = (a[axis] + here[axis] * 2.0 + b[axis]) / 4.0;
                    here[axis] + (mean - here[axis]) * weight
                });
            }
        }
        line.marks.clear();
        index += 1;
    }
    changed
}

/// Connect vector lines: the two nearest ends of different lines within
/// `radius` of `at` join into one line, the first one's brush.
pub fn connect(lines: &mut Vec<Line>, at: [f32; 2], radius: f32) -> bool {
    let ends: Vec<(usize, bool, [f32; 2])> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.samples.is_empty())
        .flat_map(|(index, line)| {
            [
                (index, true, line.samples[0].at),
                (index, false, line.samples[line.samples.len() - 1].at),
            ]
        })
        .filter(|(index, _, end)| distance(*end, at) <= radius + lines[*index].tip.size * 0.25)
        .collect();
    // Two ends (line, whether it is the start) and the gap between them.
    type Pair = ((usize, bool), (usize, bool), f32);
    let mut best: Option<Pair> = None;
    for (i, a) in ends.iter().enumerate() {
        for b in &ends[i + 1..] {
            if a.0 == b.0 {
                continue;
            }
            let gap = distance(a.2, b.2);
            if best.is_none_or(|(_, _, closest)| gap < closest) {
                best = Some(((a.0, a.1), (b.0, b.1), gap));
            }
        }
    }
    let Some(((first, first_start), (second, second_start), _)) = best else {
        return false;
    };
    let mut joined = lines[first].clone();
    let mut tail = lines[second].clone();
    joined.fix_marks();
    tail.fix_marks();
    if first_start {
        joined.samples.reverse();
        joined.marks.reverse();
    }
    if !second_start {
        tail.samples.reverse();
        tail.marks.reverse();
    }
    joined.samples.extend(tail.samples);
    joined.marks.extend(tail.marks);
    joined.finish.flick = None;
    lines[first] = joined;
    lines.remove(second);
    true
}

/// How Adjust line width changes a width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widen {
    /// Wider by the amount.
    Thicken,
    /// Narrower by the amount.
    Narrow,
    /// A little wider, in proportion.
    ScaleUp,
    /// A little narrower, in proportion.
    ScaleDown,
    /// The amount exactly.
    Fix,
}

impl Widen {
    /// What people read, in order.
    pub const NAMES: [&str; 5] = [
        "Thicken",
        "Narrow",
        "Scale up width",
        "Scale down width",
        "Fix width",
    ];

    /// The mode an Adjust line width sub tool's kind names.
    #[must_use]
    pub const fn of(kind: u8) -> Self {
        match kind {
            0 => Self::Thicken,
            1 => Self::Narrow,
            2 => Self::ScaleUp,
            3 => Self::ScaleDown,
            _ => Self::Fix,
        }
    }
}

/// Adjust line width: the lines within `radius` of `at` made wider or
/// narrower by `amount` pixels (a share of a pass, so a drag builds up), or
/// fixed at it; with `whole`, every point of each touched line, once
/// (`done` keeps the lines already done).
pub fn adjust_width(
    lines: &mut [Line],
    at: [f32; 2],
    radius: f32,
    mode: Widen,
    amount: f32,
    whole: Option<&mut Vec<usize>>,
) -> bool {
    let mut changed = false;
    let mut done = whole;
    for (index, line) in lines.iter_mut().enumerate() {
        let reach = radius + line.tip.size * 0.25;
        let weights: Vec<f32> = line
            .samples
            .iter()
            .map(|sample| (1.0 - distance(sample.at, at) / reach).max(0.0))
            .collect();
        if weights.iter().all(|weight| *weight <= 0.0) {
            continue;
        }
        let weights = match done.as_deref_mut() {
            Some(done) if done.contains(&index) => continue,
            Some(done) => {
                done.push(index);
                vec![1.0; weights.len()]
            }
            None => weights.into_iter().map(smooth).collect(),
        };
        changed = true;
        for (sample, weight) in line.samples.iter_mut().zip(weights) {
            if weight <= 0.0 {
                continue;
            }
            // The pixels wide this point is at its own pressure.
            let base = (line.tip.dab_at(sample.pressure, sample.speed, 1.0).0 * 2.0).max(0.1);
            let now = sample.width * base;
            let step = if mode == Widen::Fix || done.is_some() {
                1.0
            } else {
                0.25
            };
            let wanted = match mode {
                Widen::Thicken => now + amount * weight * step,
                Widen::Narrow => now - amount * weight * step,
                Widen::ScaleUp => now * (1.0 + 0.2 * weight * step),
                Widen::ScaleDown => now * (1.0 - 0.2 * weight * step),
                Widen::Fix => now + (amount - now) * weight,
            };
            sample.width = (wanted / base).clamp(0.0, 20.0);
        }
    }
    changed
}

/// Redraw vector line: the line nearest where `path` begins takes the
/// path's shape from there to where it ends, keeping its pressure and
/// width along the way.
pub fn redraw(lines: &mut [Line], path: &[[f32; 2]], reach: f32) -> bool {
    let (Some(&start), Some(&end)) = (path.first(), path.last()) else {
        return false;
    };
    if path.len() < 2 {
        return false;
    }
    let Some((index, from)) = line_at(lines, start, reach) else {
        return false;
    };
    let line = &mut lines[index];
    let arcs = vector::arcs(&line.samples);
    let total = arcs.last().copied().unwrap_or(0.0);
    let to = vector::touch(&line.samples, &arcs, end, reach).unwrap_or_else(|| {
        // Past the line: to the end it heads for.
        let first = line.samples[0].at;
        let last = line.samples[line.samples.len() - 1].at;
        if distance(end, last) < distance(end, first) {
            total
        } else {
            0.0
        }
    });
    let (from, to, path): (f32, f32, Vec<[f32; 2]>) = if from <= to {
        (from, to, path.to_vec())
    } else {
        (to, from, path.iter().rev().copied().collect())
    };
    let path_arcs: Vec<f32> = {
        let mut total = 0.0;
        let mut out = vec![0.0];
        for pair in path.windows(2) {
            total += distance(pair[0], pair[1]);
            out.push(total);
        }
        out
    };
    let length = path_arcs.last().copied().unwrap_or(0.0).max(1e-3);
    let middle = path.iter().zip(&path_arcs).map(|(point, arc)| Sample {
        at: *point,
        ..vector::at_arc(&line.samples, &arcs, from + (to - from) * arc / length)
    });
    let mut samples: Vec<Sample> = line
        .samples
        .iter()
        .zip(&arcs)
        .filter(|(_, arc)| **arc < from)
        .map(|(sample, _)| *sample)
        .collect();
    samples.extend(middle);
    samples.extend(
        line.samples
            .iter()
            .zip(&arcs)
            .filter(|(_, arc)| **arc > to)
            .map(|(sample, _)| *sample),
    );
    line.samples = samples;
    line.marks.clear();
    true
}

/// Redraw vector line width: points of lines within `reach` of `path` take
/// the pen's pressure there (as their width, for brushes pressure does not
/// size).
pub fn redraw_width(lines: &mut [Line], path: &[([f32; 2], f32)], reach: f32) -> bool {
    let mut changed = false;
    for line in lines.iter_mut() {
        let by_pressure = line.tip.size_by.pressure;
        for sample in &mut line.samples {
            let Some(&(_, pressure)) = path
                .iter()
                .filter(|(point, _)| distance(*point, sample.at) <= reach)
                .min_by(|a, b| distance(a.0, sample.at).total_cmp(&distance(b.0, sample.at)))
            else {
                continue;
            };
            if by_pressure {
                sample.pressure = pressure;
            } else {
                sample.width = pressure;
            }
            changed = true;
        }
    }
    changed
}

/// Remove dust: every speck of `pixels` (RGBA8, `width` by `height`) no
/// wider or taller than `size` that lies inside `rect` (`[x0, y0, x1, y1]`)
/// is cleared. Returns how many went.
pub fn remove_dust(pixels: &mut [u8], width: u32, height: u32, rect: [u32; 4], size: u32) -> usize {
    let (w, h) = (width as usize, height as usize);
    let [x0, y0, x1, y1] = rect.map(|value| value as usize);
    let (x1, y1) = (x1.min(w), y1.min(h));
    let ink: Vec<bool> = (0..w * h).map(|index| pixels[index * 4 + 3] > 0).collect();
    let inked = |index: usize| ink[index];
    let mut seen = vec![false; w * h];
    let mut removed = 0;
    for start in 0..w * h {
        if seen[start] || !inked(start) {
            continue;
        }
        // One speck: its pixels, touching corners too.
        let mut speck = vec![start];
        seen[start] = true;
        let mut at = 0;
        while at < speck.len() {
            let index = speck[at];
            at += 1;
            let (x, y) = (index % w, index / w);
            for dy in -1_i64..=1 {
                for dx in -1_i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let next = ny as usize * w + nx as usize;
                    if !seen[next] && inked(next) {
                        seen[next] = true;
                        speck.push(next);
                    }
                }
            }
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
        for index in &speck {
            let (x, y) = (index % w, index / w);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let small = max_x - min_x < size as usize && max_y - min_y < size as usize;
        let inside = min_x >= x0 && min_y >= y0 && max_x < x1 && max_y < y1;
        if small && inside {
            for index in speck {
                pixels[index * 4..index * 4 + 4].fill(0);
            }
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Finish, SAME, Tip};

    fn line(points: &[[f32; 2]]) -> Line {
        let mut samples = Vec::new();
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = distance(a, b).ceil().max(1.0) as usize;
            for step in 0..steps {
                let t = step as f32 / steps as f32;
                samples.push(Sample {
                    at: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                    pressure: 1.0,
                    speed: 0.0,
                    width: 1.0,
                    alpha: 1.0,
                });
            }
        }
        let last = *points.last().unwrap();
        samples.push(Sample {
            at: last,
            pressure: 1.0,
            speed: 0.0,
            width: 1.0,
            alpha: 1.0,
        });
        Line {
            samples,
            tip: Tip {
                size: 4.0,
                ..Tip::default()
            },
            colors: [[0.0; 3], [1.0; 3]],
            finish: Finish::default(),
            copies: vec![SAME],
            marks: Vec::new(),
        }
    }

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        distance(a, b) < 0.6
    }

    #[test]
    fn control_points_move_add_delete_corner_and_split() {
        // An L: its corner is a control point.
        let mut l = line(&[[0.0, 0.0], [40.0, 0.0], [40.0, 40.0]]);
        let controls = l.controls();
        assert_eq!(controls.len(), 3);
        let corner = controls[1];
        assert!(near(l.samples[corner].at, [40.0, 0.0]));
        assert_eq!(
            control_at(&[l.clone()], [41.0, 1.0], 4.0),
            Some((0, corner))
        );
        // Moving it bends both arms; the ends stay.
        move_control(&mut l, corner, [10.0, -10.0]);
        assert!(near(l.samples[corner].at, [50.0, -10.0]));
        assert!(
            near(l.samples[0].at, [0.0, 0.0]) && near(l.samples.last().unwrap().at, [40.0, 40.0])
        );
        // A control added halfway along the first arm.
        let mut l = line(&[[0.0, 0.0], [40.0, 0.0], [40.0, 40.0]]);
        let added = add_control(&mut l, 20.0);
        assert_eq!(l.controls().len(), 4);
        assert!(near(l.samples[added].at, [20.0, 0.0]));
        // Deleting the corner rounds it off.
        let corner = l.controls()[2];
        assert!(delete_control(&mut l, corner));
        assert_eq!(l.controls().len(), 3);
        assert!(
            l.samples
                .iter()
                .all(|sample| distance(sample.at, [40.0, 0.0]) > 3.0)
        );
        // Width at a control: widest there.
        let mut l = line(&[[0.0, 0.0], [40.0, 0.0], [40.0, 40.0]]);
        let corner = l.controls()[1];
        shape_control(&mut l, corner, 2.0, false);
        assert!(
            (l.samples[corner].width - 2.0).abs() < 1e-3 && (l.samples[0].width - 1.0).abs() < 1e-3
        );
        // Split at the corner: two lines.
        let mut lines = vec![line(&[[0.0, 0.0], [40.0, 0.0], [40.0, 40.0]])];
        assert!(split(&mut lines, 0, 40.0));
        assert_eq!(lines.len(), 2);
        assert!(near(lines[0].samples.last().unwrap().at, [40.0, 0.0]));
        // A smooth point made a corner and back.
        let mut l = line(&[[0.0, 0.0], [20.0, 10.0], [40.0, 0.0]]);
        let middle = l.controls()[1];
        switch_corner(&mut l, middle);
        assert!(l.is_corner(l.controls()[1]));
    }

    #[test]
    fn pinch_simplify_connect_and_widen() {
        // Pinch the middle of a level line up; its ends stay.
        let base = line(&[[0.0, 0.0], [100.0, 0.0]]);
        let pulled = pinch(&base, 50.0, [0.0, -20.0], 30.0, 2, true);
        let middle = pulled
            .samples
            .iter()
            .map(|sample| sample.at[1])
            .fold(0.0, f32::min);
        assert!((middle + 20.0).abs() < 0.5, "{middle}");
        assert_eq!(pulled.samples[0].at, [0.0, 0.0]);
        assert!(
            pulled
                .samples
                .iter()
                .filter(|sample| sample.at[0] < 15.0)
                .all(|sample| sample.at[1] == 0.0)
        );
        // Simplify smooths a zigzag where the brush is.
        let mut lines = vec![line(&[
            [0.0, 0.0],
            [10.0, 6.0],
            [20.0, 0.0],
            [30.0, 6.0],
            [40.0, 0.0],
        ])];
        let rough = lines[0]
            .samples
            .iter()
            .map(|sample| sample.at[1])
            .fold(0.0, f32::max);
        assert!(simplify(&mut lines, [20.0, 3.0], 30.0, 3, false));
        let smoothed = lines[0]
            .samples
            .iter()
            .map(|sample| sample.at[1])
            .fold(0.0, f32::max);
        assert!(smoothed < rough, "{smoothed} against {rough}");
        // Delete short lines.
        let mut lines = vec![
            line(&[[0.0, 0.0], [3.0, 0.0]]),
            line(&[[50.0, 0.0], [90.0, 0.0]]),
        ];
        assert!(simplify(&mut lines, [1.0, 0.0], 5.0, 1, true));
        assert_eq!(lines.len(), 1);
        // Connect two lines whose ends meet.
        let mut lines = vec![
            line(&[[0.0, 0.0], [20.0, 0.0]]),
            line(&[[22.0, 0.0], [40.0, 0.0]]),
        ];
        assert!(connect(&mut lines, [21.0, 0.0], 4.0));
        assert_eq!(lines.len(), 1);
        assert!(
            near(lines[0].samples[0].at, [0.0, 0.0])
                && near(lines[0].samples.last().unwrap().at, [40.0, 0.0])
        );
        // Fix width makes the touched part the amount wide.
        let mut lines = vec![line(&[[0.0, 0.0], [40.0, 0.0]])];
        assert!(adjust_width(
            &mut lines,
            [20.0, 0.0],
            3.0,
            Widen::Fix,
            8.0,
            None
        ));
        let base = lines[0].tip.dab_at(1.0, 0.0, 1.0).0 * 2.0;
        assert!((lines[0].samples[20].width * base - 8.0).abs() < 1e-3);
        assert!(
            (lines[0].samples[0].width - 1.0).abs() < 1e-6,
            "outside the brush"
        );
        // Process whole line: every point, once.
        let mut lines = vec![line(&[[0.0, 0.0], [40.0, 0.0]])];
        let mut done = Vec::new();
        adjust_width(
            &mut lines,
            [20.0, 0.0],
            3.0,
            Widen::Thicken,
            4.0,
            Some(&mut done),
        );
        adjust_width(
            &mut lines,
            [22.0, 0.0],
            3.0,
            Widen::Thicken,
            4.0,
            Some(&mut done),
        );
        let wide = lines[0].samples[0].width * base;
        assert!((wide - (base + 4.0)).abs() < 1e-3, "{wide}");
    }

    #[test]
    fn redraw_reshapes_and_rewidths_lines_and_dust_goes() {
        // Trace a bump over the middle of a level line.
        let mut lines = vec![line(&[[0.0, 0.0], [100.0, 0.0]])];
        let bump: Vec<[f32; 2]> = (0..=40)
            .map(|step| {
                let x = 30.0 + step as f32;
                [
                    x,
                    -((x - 50.0) / 20.0 * std::f32::consts::FRAC_PI_2)
                        .cos()
                        .max(0.0)
                        * 10.0,
                ]
            })
            .collect();
        assert!(redraw(&mut lines, &bump, 4.0));
        let top = lines[0]
            .samples
            .iter()
            .map(|sample| sample.at[1])
            .fold(0.0, f32::min);
        assert!(top < -9.0);
        assert!(
            near(lines[0].samples[0].at, [0.0, 0.0])
                && near(lines[0].samples.last().unwrap().at, [100.0, 0.0])
        );
        // Redraw width: the pressure along the trace.
        let mut lines = vec![line(&[[0.0, 0.0], [40.0, 0.0]])];
        let trace: Vec<([f32; 2], f32)> = (0..=40).map(|x| ([x as f32, 0.5], 0.3)).collect();
        assert!(redraw_width(&mut lines, &trace, 2.0));
        assert!(
            lines[0]
                .samples
                .iter()
                .all(|sample| (sample.pressure - 0.3).abs() < 1e-6)
        );
        // Remove dust: a dot goes, a line stays.
        let (w, h) = (40_u32, 20_u32);
        let mut pixels = vec![0_u8; (w * h * 4) as usize];
        let mut ink = |x: u32, y: u32| pixels[((y * w + x) * 4 + 3) as usize] = 255;
        ink(5, 5);
        ink(6, 5);
        for x in 10..35 {
            ink(x, 12);
        }
        assert_eq!(remove_dust(&mut pixels, w, h, [0, 0, w, h], 4), 1);
        assert_eq!(pixels[((5 * w + 5) * 4 + 3) as usize], 0);
        assert_eq!(pixels[((12 * w + 20) * 4 + 3) as usize], 255);
    }
}
