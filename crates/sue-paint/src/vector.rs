//! Vector layers, as in Clip Studio Paint: every line drawn on one keeps
//! its path and brush, so it can be erased where the eraser touches it, up
//! to where it crosses other lines, or whole, and is painted again exactly.
//!
//! A vector layer is a raster layer of class [`CLASS`] (its pixels are what
//! shows and composites) with a [`Geometry`] holding its lines: a polyline
//! a line over the pen's samples as they were drawn, with their pressure
//! and speed, and on each primitive the brush, colors, finish and copies
//! that paint it. Painting a line again from these gives the same pixels,
//! since strokes take their chances from their own path.

use sue_core::txn::{RasterEdit, SetExtension};
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D};
use sue_model::{AttributeData, Domain, Geometry, PrimKind};

use crate::brush::{Copy2D, Finish, SAME, Sample, Stroke, Tip};
use crate::doc::Edit;

/// The class of a vector layer.
pub const CLASS: &str = "sue.paint.vector";

/// `true` if `layer` is a vector layer.
#[must_use]
pub fn is_vector(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .is_ok_and(|entity| entity.class() == CLASS)
}

/// The geometry a vector layer keeps its lines in.
#[must_use]
pub fn geometry(arena: &Arena, layer: EntityId) -> Geometry {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.ext::<Geometry>())
        .cloned()
        .unwrap_or_default()
}

/// Keeps `lines` on the vector `layer` in place (its pixels, `pixels`,
/// already painted), returning the edit that records both as one step
/// named `label`; `old` is the geometry from before.
#[must_use]
pub fn keep(
    arena: &mut Arena,
    layer: EntityId,
    old: Geometry,
    lines: &[Line],
    pixels: Option<RasterEdit>,
    label: &str,
) -> Edit {
    if let Ok(entity) = arena.get_by_id_mut(layer) {
        entity.set_ext(Box::new(to_geometry(lines)));
    }
    let mut edit = Edit::new(label);
    if let Some(pixels) = pixels {
        edit = edit.then(pixels);
    }
    // Already in place: undoing swaps the old geometry back in.
    edit.then(SetExtension::set(layer, old))
}

/// One line on a vector layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The pen's samples, in the layer's pixels.
    pub samples: Vec<Sample>,
    /// Its brush.
    pub tip: Tip,
    /// The color it began with and the sub color (sRGB, 0 to 1).
    pub colors: [[f32; 3]; 2],
    /// How it was finished when the pen lifted.
    pub finish: Finish,
    /// The copies it paints through (symmetry).
    pub copies: Vec<Copy2D>,
    /// A flag a sample ([`CONTROL`], [`CORNER`]) for Correct Line, or none
    /// to find the control points from the path.
    pub marks: Vec<u8>,
}

/// A sample that is a control point (Correct Line).
pub const CONTROL: u8 = 1;
/// A control point that is a corner.
pub const CORNER: u8 = 2;

impl Line {
    /// The line a finished `stroke` drew from `samples` (its samples
    /// before it was finished).
    #[must_use]
    pub fn of(stroke: &Stroke, samples: Vec<Sample>, finish: Finish) -> Self {
        let (color, sub) = stroke.colors();
        Self {
            samples,
            tip: *stroke.tip(),
            colors: [color, sub],
            finish,
            copies: stroke.copies().to_vec(),
            marks: Vec::new(),
        }
    }

    /// Paints the line onto `raster`.
    pub fn paint(&self, raster: &mut RasterTile2D) {
        let mut stroke = Stroke::new(EntityId::NIL, self.tip, self.colors[0], false);
        stroke.set_sub_color(self.colors[1]);
        stroke.set_copies(self.copies.clone());
        for sample in &self.samples {
            stroke.set_speed(sample.speed);
            stroke.set_shape(sample.width, sample.alpha);
            stroke.to(raster, sample.at, sample.pressure, None);
        }
        stroke.end(raster, self.finish, None);
    }

    /// The same line along other samples.
    pub(crate) fn along(&self, samples: Vec<Sample>, keep_flick: bool) -> Self {
        Self {
            samples,
            marks: Vec::new(),
            finish: Finish {
                flick: self.finish.flick.filter(|_| keep_flick),
                ..self.finish
            },
            ..self.clone()
        }
    }
}

fn floats<'a>(geometry: &'a Geometry, domain: Domain, name: &str, width: u8) -> Option<&'a [f32]> {
    match geometry.attribute(domain, name)? {
        AttributeData::Float {
            width: have,
            values,
        } if *have == width => Some(values),
        _ => None,
    }
}

fn ints<'a>(geometry: &'a Geometry, domain: Domain, name: &str) -> Option<&'a [i32]> {
    match geometry.attribute(domain, name)? {
        AttributeData::Int { width: 1, values } => Some(values),
        _ => None,
    }
}

fn texts<'a>(geometry: &'a Geometry, domain: Domain, name: &str) -> Option<&'a [String]> {
    match geometry.attribute(domain, name)? {
        AttributeData::Text(values) => Some(values),
        _ => None,
    }
}

/// The lines kept in `geometry`.
#[must_use]
pub fn lines(geometry: &Geometry) -> Vec<Line> {
    let points = geometry.points();
    let count = points.len();
    let pressure = floats(geometry, Domain::Point, "pressure", 1).filter(|v| v.len() == count);
    let speed = floats(geometry, Domain::Point, "speed", 1).filter(|v| v.len() == count);
    let width = floats(geometry, Domain::Point, "width", 1).filter(|v| v.len() == count);
    let alpha = floats(geometry, Domain::Point, "alpha", 1).filter(|v| v.len() == count);
    let prims = geometry.prims().len();
    let per = |name: &str, width: u8| {
        floats(geometry, Domain::Primitive, name, width)
            .filter(|v| v.len() == prims * usize::from(width))
    };
    let (colors, subs, flicks) = (per("color", 3), per("sub", 3), per("flick", 2));
    let corrections = ints(geometry, Domain::Primitive, "correction").filter(|v| v.len() == prims);
    let flicked = ints(geometry, Domain::Primitive, "flicked").filter(|v| v.len() == prims);
    let single = ints(geometry, Domain::Primitive, "single").filter(|v| v.len() == prims);
    let tips = texts(geometry, Domain::Primitive, "tip").filter(|v| v.len() == prims);
    let marks = ints(geometry, Domain::Point, "mark").filter(|v| v.len() == count);
    let copies = texts(geometry, Domain::Primitive, "copies").filter(|v| v.len() == prims);
    let three = |values: Option<&[f32]>, at: usize, or: [f32; 3]| {
        values.map_or(or, |values| {
            [values[at * 3], values[at * 3 + 1], values[at * 3 + 2]]
        })
    };
    geometry
        .prims()
        .iter()
        .enumerate()
        .map(|(at, prim)| {
            let mut samples: Vec<Sample> = geometry
                .prim_points(prim)
                .iter()
                .map(|&point| {
                    let index = point as usize;
                    let [x, y, _] = points[index];
                    Sample {
                        at: [x as f32, y as f32],
                        pressure: pressure.map_or(1.0, |values| values[index]),
                        speed: speed.map_or(0.0, |values| values[index]),
                        width: width.map_or(1.0, |values| values[index]),
                        alpha: alpha.map_or(1.0, |values| values[index]),
                    }
                })
                .collect();
            let mut flags: Vec<u8> = marks
                .map(|values| {
                    geometry
                        .prim_points(prim)
                        .iter()
                        .map(|&point| values[point as usize].clamp(0, 255) as u8)
                        .collect()
                })
                .unwrap_or_default();
            if single.is_some_and(|values| values[at] == 1) {
                samples.truncate(1);
                flags.truncate(1);
            }
            if flags.iter().all(|flag| *flag == 0) {
                flags.clear();
            }
            let copies = copies
                .map(|values| {
                    values[at]
                        .split(';')
                        .filter_map(|copy| {
                            let numbers: Vec<f32> = copy
                                .split(',')
                                .filter_map(|value| value.parse().ok())
                                .collect();
                            <[f32; 6]>::try_from(numbers.as_slice()).ok()
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|copies| !copies.is_empty())
                .unwrap_or_else(|| vec![SAME]);
            Line {
                samples,
                tip: tips.map_or_else(Tip::default, |values| Tip::from_text(&values[at])),
                colors: [three(colors, at, [0.0; 3]), three(subs, at, [1.0; 3])],
                finish: Finish {
                    correction: corrections.map_or(0, |values| values[at].max(0) as u32),
                    flick: flicked
                        .zip(flicks)
                        .filter(|(on, _)| on[at] == 1)
                        .map(|(_, values)| [values[at * 2], values[at * 2 + 1]]),
                },
                copies,
                marks: flags,
            }
        })
        .collect()
}

/// `lines` as geometry, to keep on a vector layer.
#[must_use]
pub fn to_geometry(lines: &[Line]) -> Geometry {
    let mut geometry = Geometry::new();
    let (mut pressure, mut speed) = (Vec::new(), Vec::new());
    let (mut width, mut alpha) = (Vec::new(), Vec::new());
    let mut marks: Vec<i32> = Vec::new();
    let (mut colors, mut subs, mut flicks) = (Vec::new(), Vec::new(), Vec::new());
    let (mut corrections, mut flicked, mut single) = (Vec::new(), Vec::new(), Vec::new());
    let (mut tips, mut copies) = (Vec::new(), Vec::new());
    for line in lines.iter().filter(|line| !line.samples.is_empty()) {
        marks.extend(
            (0..line.samples.len()).map(|at| i32::from(line.marks.get(at).copied().unwrap_or(0))),
        );
        let mut indices: Vec<u32> = line
            .samples
            .iter()
            .map(|sample| {
                pressure.push(sample.pressure);
                speed.push(sample.speed);
                width.push(sample.width);
                alpha.push(sample.alpha);
                geometry.add_point([f64::from(sample.at[0]), f64::from(sample.at[1]), 0.0])
            })
            .collect();
        // A dot is one sample: a polyline needs two points, so it doubles.
        let dot = indices.len() == 1;
        if dot {
            marks.push(0);
            let sample = line.samples[0];
            pressure.push(sample.pressure);
            speed.push(sample.speed);
            width.push(sample.width);
            alpha.push(sample.alpha);
            indices.push(geometry.add_point([
                f64::from(sample.at[0]),
                f64::from(sample.at[1]),
                0.0,
            ]));
        }
        if geometry.add_prim(PrimKind::Polyline, &indices).is_err() {
            continue;
        }
        colors.extend(line.colors[0]);
        subs.extend(line.colors[1]);
        let flick = line.finish.flick;
        flicks.extend(flick.unwrap_or([0.0; 2]));
        flicked.push(i32::from(flick.is_some()));
        corrections.push(line.finish.correction as i32);
        single.push(i32::from(dot));
        tips.push(line.tip.to_text());
        copies.push(
            line.copies
                .iter()
                .map(|copy| {
                    copy.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect::<Vec<_>>()
                .join(";"),
        );
    }
    let float = |width: u8, values: Vec<f32>| AttributeData::Float { width, values };
    let int = |values: Vec<i32>| AttributeData::Int { width: 1, values };
    for (domain, name, data) in [
        (Domain::Point, "pressure", float(1, pressure)),
        (Domain::Point, "speed", float(1, speed)),
        (Domain::Point, "width", float(1, width)),
        (Domain::Point, "alpha", float(1, alpha)),
        (Domain::Point, "mark", int(marks)),
        (Domain::Primitive, "color", float(3, colors)),
        (Domain::Primitive, "sub", float(3, subs)),
        (Domain::Primitive, "flick", float(2, flicks)),
        (Domain::Primitive, "flicked", int(flicked)),
        (Domain::Primitive, "correction", int(corrections)),
        (Domain::Primitive, "single", int(single)),
        (Domain::Primitive, "tip", AttributeData::Text(tips)),
        (Domain::Primitive, "copies", AttributeData::Text(copies)),
    ] {
        // Each holds one value an element, so this always fits.
        let _ = geometry.set_attribute(domain, name, data);
    }
    geometry
}

/// `lines` painted onto a clear `width` by `height` layer.
#[must_use]
pub fn render(lines: &[Line], width: u32, height: u32) -> Option<RasterTile2D> {
    let mut raster = RasterTile2D::new(width, height, PixelFormat::Rgba8).ok()?;
    for line in lines {
        line.paint(&mut raster);
    }
    Some(raster)
}

/// How the eraser takes vector lines away (Clip Studio Paint's Vector
/// eraser).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erase {
    /// Only where the eraser touches, splitting the line there.
    Touched,
    /// The part of the line between the lines it crosses either side.
    Intersection,
    /// Every line it touches, whole.
    Whole,
}

impl Erase {
    /// What people read, in order.
    pub const NAMES: [&str; 3] = [
        "Erase touched area",
        "Erase up to intersection",
        "Whole line",
    ];

    /// The mode an eraser sub tool's kind names.
    #[must_use]
    pub const fn of(kind: u8) -> Self {
        match kind {
            0 => Self::Touched,
            1 => Self::Intersection,
            _ => Self::Whole,
        }
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The distances along `samples` of each of them.
pub(crate) fn arcs(samples: &[Sample]) -> Vec<f32> {
    let mut total = 0.0;
    let mut out = Vec::with_capacity(samples.len());
    for (index, sample) in samples.iter().enumerate() {
        if index > 0 {
            total += distance(samples[index - 1].at, sample.at);
        }
        out.push(total);
    }
    out
}

/// How far along `samples` the point nearest `at` is, when it is within
/// `reach`.
pub(crate) fn touch(samples: &[Sample], arcs: &[f32], at: [f32; 2], reach: f32) -> Option<f32> {
    if samples.len() == 1 {
        return (distance(samples[0].at, at) <= reach).then_some(0.0);
    }
    let mut best: Option<(f32, f32)> = None;
    for index in 1..samples.len() {
        let (a, b) = (samples[index - 1].at, samples[index].at);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let near = [a[0] + dx * t, a[1] + dy * t];
        let away = distance(near, at);
        if away <= reach && best.is_none_or(|(_, closest)| away < closest) {
            best = Some((arcs[index - 1] + (arcs[index] - arcs[index - 1]) * t, away));
        }
    }
    best.map(|(along, _)| along)
}

/// The sample `along` the line, between its samples.
pub(crate) fn at_arc(samples: &[Sample], arcs: &[f32], along: f32) -> Sample {
    let index = arcs
        .partition_point(|arc| *arc < along)
        .clamp(1, samples.len() - 1);
    let (a, b) = (samples[index - 1], samples[index]);
    let span = arcs[index] - arcs[index - 1];
    let t = if span > 0.0 {
        ((along - arcs[index - 1]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Sample {
        at: [mix(a.at[0], b.at[0]), mix(a.at[1], b.at[1])],
        pressure: mix(a.pressure, b.pressure),
        speed: mix(a.speed, b.speed),
        width: mix(a.width, b.width),
        alpha: mix(a.alpha, b.alpha),
    }
}

/// The part of the line from `from` to `to` along it, cut exactly.
pub(crate) fn part(samples: &[Sample], arcs: &[f32], from: f32, to: f32) -> Vec<Sample> {
    let mut out = vec![at_arc(samples, arcs, from)];
    out.extend(
        samples
            .iter()
            .zip(arcs)
            .filter(|(_, arc)| **arc > from && **arc < to)
            .map(|(sample, _)| *sample),
    );
    out.push(at_arc(samples, arcs, to));
    out
}

/// Where `a` crosses `b`, as distances along `a`.
fn crossings(a: &[Sample], arcs: &[f32], b: &[Sample]) -> Vec<f32> {
    let mut out = Vec::new();
    for i in 1..a.len() {
        let (p, r) = (
            a[i - 1].at,
            [a[i].at[0] - a[i - 1].at[0], a[i].at[1] - a[i - 1].at[1]],
        );
        for j in 1..b.len() {
            let (q, s) = (
                b[j - 1].at,
                [b[j].at[0] - b[j - 1].at[0], b[j].at[1] - b[j - 1].at[1]],
            );
            let cross = r[0] * s[1] - r[1] * s[0];
            if cross.abs() < 1e-6 {
                continue;
            }
            let (qx, qy) = (q[0] - p[0], q[1] - p[1]);
            let t = (qx * s[1] - qy * s[0]) / cross;
            let u = (qx * r[1] - qy * r[0]) / cross;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                out.push(arcs[i - 1] + (arcs[i] - arcs[i - 1]) * t);
            }
        }
    }
    out
}

/// The lines after the eraser, `radius` wide, touched `at`; `None` when it
/// touched none.
#[must_use]
pub fn erase(lines: &[Line], at: [f32; 2], radius: f32, mode: Erase) -> Option<Vec<Line>> {
    let mut touched = false;
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.samples.is_empty() {
            continue;
        }
        let arcs = arcs(&line.samples);
        // The eraser reaches the line's edge, not only its middle.
        let reach = radius + line.tip.size * 0.25;
        let Some(hit) = touch(&line.samples, &arcs, at, reach) else {
            out.push(line.clone());
            continue;
        };
        touched = true;
        let total = arcs.last().copied().unwrap_or(0.0);
        match mode {
            Erase::Whole => {}
            Erase::Intersection => {
                let cuts: Vec<f32> = lines
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .flat_map(|(_, other)| crossings(&line.samples, &arcs, &other.samples))
                    .collect();
                let below = cuts
                    .iter()
                    .copied()
                    .filter(|cut| *cut < hit)
                    .reduce(f32::max);
                let above = cuts
                    .iter()
                    .copied()
                    .filter(|cut| *cut > hit)
                    .reduce(f32::min);
                if let Some(low) = below.filter(|low| *low > 0.5) {
                    out.push(line.along(part(&line.samples, &arcs, 0.0, low), false));
                }
                if let Some(high) = above.filter(|high| total - *high > 0.5) {
                    out.push(line.along(part(&line.samples, &arcs, high, total), true));
                }
            }
            Erase::Touched => {
                // Every stretch of the line outside the eraser stays, cut
                // where it meets the eraser's edge.
                let step = 0.5_f32;
                let count = (total / step).ceil() as usize;
                let mut start: Option<f32> = None;
                for n in 0..=count {
                    let along = (n as f32 * step).min(total);
                    let outside = distance(at_arc(&line.samples, &arcs, along).at, at) > reach;
                    match (outside, start) {
                        (true, None) => start = Some(along),
                        (false, Some(from)) => {
                            if along - from > 0.5 {
                                out.push(
                                    line.along(part(&line.samples, &arcs, from, along), false),
                                );
                            }
                            start = None;
                        }
                        _ => {}
                    }
                }
                if let Some(from) = start.filter(|from| total - *from > 0.5) {
                    out.push(line.along(part(&line.samples, &arcs, from, total), true));
                }
            }
        }
    }
    touched.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Dynamics, TaperMode};

    fn line(from: [f32; 2], to: [f32; 2]) -> Line {
        let samples = (0..=20)
            .map(|step| {
                let t = step as f32 / 20.0;
                Sample {
                    at: [
                        from[0] + (to[0] - from[0]) * t,
                        from[1] + (to[1] - from[1]) * t,
                    ],
                    pressure: 0.5 + t * 0.5,
                    speed: 0.2,
                    width: 1.0,
                    alpha: 1.0,
                }
            })
            .collect();
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

    #[test]
    fn a_brush_reads_back_from_its_text() {
        let mut tip = Tip {
            size: 13.5,
            opacity: 0.7,
            build_up: true,
            taper_mode: TaperMode::Fade,
            taper_end: 30.0,
            mix: 0.4,
            size_by: Dynamics::pressure(0.3),
            ..Tip::default()
        };
        tip.opacity_by.velocity = true;
        tip.opacity_by.velocity_curve = sue_model::Response::new(&[[0.0, 0.2], [1.0, 0.9]]);
        tip.jitter.stroke = true;
        tip.jitter.stroke_hue = 0.25;
        assert_eq!(Tip::from_text(&tip.to_text()), tip);
        assert_eq!(
            Tip::from_text("size=3;unknown=1"),
            Tip {
                size: 3.0,
                ..Tip::default()
            }
        );
    }

    #[test]
    fn lines_keep_in_geometry_and_paint_again_exactly() {
        // A stroke drawn live, and the same painted again from its line.
        let tip = Tip {
            size: 6.0,
            taper_end: 10.0,
            ..Tip::default()
        };
        let mut live = RasterTile2D::new(64, 64, PixelFormat::Rgba8).unwrap();
        let mut stroke = Stroke::new(EntityId::NIL, tip, [0.2, 0.1, 0.6], false);
        for step in 0..12 {
            stroke.set_speed(0.3);
            let x = 8.0 + step as f32 * 4.0;
            stroke.to(&mut live, [x, 20.0 + (x * 0.2).sin() * 6.0], 0.8, None);
        }
        let samples = stroke.samples().to_vec();
        let finish = Finish {
            correction: 3,
            flick: None,
        };
        stroke.end(&mut live, finish, None);
        let drawn = Line::of(&stroke, samples, finish);
        let dot = Line {
            samples: vec![Sample {
                at: [50.0, 50.0],
                pressure: 1.0,
                speed: 0.0,
                width: 1.0,
                alpha: 1.0,
            }],
            ..drawn.clone()
        };
        let kept = lines(&to_geometry(&[drawn.clone(), dot.clone()]));
        assert_eq!(kept, vec![drawn.clone(), dot]);
        let again = render(&[drawn], 64, 64).unwrap();
        assert_eq!(again.bytes(), live.bytes(), "the same pixels");
    }

    #[test]
    fn the_vector_eraser_takes_lines_away_three_ways() {
        // A level line crossed by two upright ones.
        let lines = vec![
            line([0.0, 50.0], [100.0, 50.0]),
            line([30.0, 0.0], [30.0, 100.0]),
            line([70.0, 0.0], [70.0, 100.0]),
        ];
        assert_eq!(
            erase(&lines, [50.0, 20.0], 3.0, Erase::Whole),
            None,
            "nothing there"
        );
        let whole = erase(&lines, [50.0, 50.0], 3.0, Erase::Whole).unwrap();
        assert_eq!(whole.len(), 2);
        // Up to intersection: the level line keeps its ends outside the
        // uprights.
        let cut = erase(&lines, [50.0, 50.0], 3.0, Erase::Intersection).unwrap();
        assert_eq!(cut.len(), 4);
        let ends: Vec<[f32; 2]> = cut[..2]
            .iter()
            .map(|piece| [piece.samples[0].at[0], piece.samples.last().unwrap().at[0]])
            .collect();
        assert!(
            (ends[0][0] - 0.0).abs() < 1e-3 && (ends[0][1] - 30.0).abs() < 1e-3,
            "{ends:?}"
        );
        assert!(
            (ends[1][0] - 70.0).abs() < 1e-3 && (ends[1][1] - 100.0).abs() < 1e-3,
            "{ends:?}"
        );
        // Touched area: only around the eraser.
        let touched = erase(&lines, [50.0, 50.0], 3.0, Erase::Touched).unwrap();
        assert_eq!(touched.len(), 4);
        let left = touched[0].samples.last().unwrap().at[0];
        let right = touched[1].samples[0].at[0];
        assert!(
            (45.0..=48.0).contains(&left) && (52.0..=55.0).contains(&right),
            "{left} {right}"
        );
    }
}
