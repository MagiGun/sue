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

/// The shapes of an SVG `drawing` as lines: each stroked shape along its
/// outline in its stroke color and width, each filled one along its
/// outline in its fill color, made smaller to fit `size` when larger.
#[must_use]
pub fn from_svg(drawing: &sue_io::svg::SvgDrawing, size: [u32; 2]) -> Vec<Line> {
    let scale = if drawing.width > 0.0 && drawing.height > 0.0 {
        (size[0] as f32 / drawing.width)
            .min(size[1] as f32 / drawing.height)
            .min(1.0)
    } else {
        1.0
    };
    drawing
        .shapes
        .iter()
        .filter_map(|shape| {
            let color = shape.stroke.or(shape.fill)?;
            let rgb = color.map(|channel| f32::from(channel) / 255.0);
            let width = if shape.stroke.is_some() {
                shape.width
            } else {
                1.0
            };
            let mut points = shape.points.clone();
            if shape.closed
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            let samples = points
                .into_iter()
                .map(|[x, y]| Sample {
                    at: [x * scale, y * scale],
                    pressure: 1.0,
                    speed: 0.0,
                    width: 1.0,
                    alpha: 1.0,
                })
                .collect();
            Some(Line {
                samples,
                tip: Tip {
                    size: (width * scale).max(1.0),
                    ..Tip::default()
                },
                colors: [rgb, rgb],
                finish: Finish::default(),
                copies: Vec::new(),
                marks: Vec::new(),
            })
        })
        .collect()
}

/// Lines traced along the middle of what straight RGBA8 `pixels` cover
/// (Convert Layer to a vector layer): the covered area thinned to its
/// center lines, each line as wide as the stroke it follows, in the
/// average color of the pixels.
#[must_use]
pub fn trace(pixels: &[u8], width: u32, height: u32) -> Vec<Line> {
    let (w, h) = (width as usize, height as usize);
    if pixels.len() != w * h * 4 || w == 0 || h == 0 {
        return Vec::new();
    }
    let ink: Vec<bool> = pixels.chunks(4).map(|pixel| pixel[3] > 127).collect();
    let (mut sum, mut count) = ([0.0_f32; 3], 0.0_f32);
    for (pixel, inked) in pixels.chunks(4).zip(&ink) {
        if *inked {
            for channel in 0..3 {
                sum[channel] += f32::from(pixel[channel]) / 255.0;
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return Vec::new();
    }
    let color = sum.map(|value| value / count);
    // How far each inked pixel is from the nearest clear one (chamfer).
    let mut far = vec![f32::MAX; w * h];
    for (index, inked) in ink.iter().enumerate() {
        if !inked {
            far[index] = 0.0;
        }
    }
    let step = |far: &mut Vec<f32>, x: usize, y: usize, dx: isize, dy: isize, cost: f32| {
        let (nx, ny) = (x as isize + dx, y as isize + dy);
        let from = if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
            0.0
        } else {
            far[ny as usize * w + nx as usize]
        };
        let here = &mut far[y * w + x];
        *here = here.min(from + cost);
    };
    for y in 0..h {
        for x in 0..w {
            for (dx, dy, cost) in [(-1, 0, 1.0), (0, -1, 1.0), (-1, -1, 1.414), (1, -1, 1.414)] {
                step(&mut far, x, y, dx, dy, cost);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            for (dx, dy, cost) in [(1, 0, 1.0), (0, 1, 1.0), (1, 1, 1.414), (-1, 1, 1.414)] {
                step(&mut far, x, y, dx, dy, cost);
            }
        }
    }
    // Zhang and Suen's thinning.
    let mut on = ink.clone();
    let at = |on: &Vec<bool>, x: isize, y: isize| {
        x >= 0 && y >= 0 && x < w as isize && y < h as isize && on[y as usize * w + x as usize]
    };
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let mut gone = Vec::new();
            for y in 0..h as isize {
                for x in 0..w as isize {
                    if !at(&on, x, y) {
                        continue;
                    }
                    let p = [
                        at(&on, x, y - 1),
                        at(&on, x + 1, y - 1),
                        at(&on, x + 1, y),
                        at(&on, x + 1, y + 1),
                        at(&on, x, y + 1),
                        at(&on, x - 1, y + 1),
                        at(&on, x - 1, y),
                        at(&on, x - 1, y - 1),
                    ];
                    let neighbors = p.iter().filter(|on| **on).count();
                    let turns = (0..8).filter(|i| !p[*i] && p[(*i + 1) % 8]).count();
                    let (a, b) = if pass == 0 {
                        (p[0] && p[2] && p[4], p[2] && p[4] && p[6])
                    } else {
                        (p[0] && p[2] && p[6], p[0] && p[4] && p[6])
                    };
                    if (2..=6).contains(&neighbors) && turns == 1 && !a && !b {
                        gone.push(y as usize * w + x as usize);
                    }
                }
            }
            changed |= !gone.is_empty();
            for index in gone {
                on[index] = false;
            }
        }
        if !changed {
            break;
        }
    }
    // Walk the center lines from their ends and joins, then the loops.
    let around = |index: usize| -> Vec<usize> {
        let (x, y) = ((index % w) as isize, (index / w) as isize);
        let mut out = Vec::new();
        for (dx, dy) in [
            (0, -1),
            (1, 0),
            (0, 1),
            (-1, 0),
            (1, -1),
            (1, 1),
            (-1, 1),
            (-1, -1),
        ] {
            if at(&on, x + dx, y + dy) {
                out.push((y + dy) as usize * w + (x + dx) as usize);
            }
        }
        out
    };
    let mut used = std::collections::HashSet::new();
    let mut seen = vec![false; w * h];
    let mut chains: Vec<Vec<usize>> = Vec::new();
    let walk = |start: usize,
                next: usize,
                used: &mut std::collections::HashSet<(usize, usize)>,
                seen: &mut Vec<bool>| {
        let mut chain = vec![start, next];
        used.insert((start.min(next), start.max(next)));
        seen[start] = true;
        let mut here = next;
        loop {
            seen[here] = true;
            let ways = around(here);
            if ways.len() != 2 {
                break;
            }
            let Some(onward) = ways
                .into_iter()
                .find(|way| !used.contains(&((*way).min(here), (*way).max(here))))
            else {
                break;
            };
            used.insert((here.min(onward), here.max(onward)));
            chain.push(onward);
            here = onward;
        }
        chain
    };
    for index in 0..w * h {
        if !on[index] {
            continue;
        }
        let ways = around(index);
        if ways.len() == 2 {
            continue;
        }
        if ways.is_empty() {
            chains.push(vec![index]);
            seen[index] = true;
            continue;
        }
        for way in ways {
            if !used.contains(&(index.min(way), index.max(way))) {
                chains.push(walk(index, way, &mut used, &mut seen));
            }
        }
    }
    for index in 0..w * h {
        if on[index]
            && !seen[index]
            && let Some(way) = around(index).into_iter().next()
        {
            chains.push(walk(index, way, &mut used, &mut seen));
        }
    }
    chains
        .into_iter()
        .filter_map(|chain| {
            let points: Vec<([f32; 2], f32)> = chain
                .iter()
                .map(|index| {
                    let (x, y) = ((index % w) as f32 + 0.5, (index / w) as f32 + 0.5);
                    ([x, y], (far[*index] * 2.0 - 1.0).max(1.0))
                })
                .collect();
            let kept = simplify(&points, 0.75);
            let size = kept.iter().map(|(_, width)| *width).fold(1.0_f32, f32::max);
            let mut samples: Vec<Sample> = kept
                .iter()
                .map(|(at, width)| Sample {
                    at: *at,
                    pressure: 1.0,
                    speed: 0.0,
                    width: width / size,
                    alpha: 1.0,
                })
                .collect();
            if samples.len() == 1 {
                samples.push(samples[0]);
            }
            (!samples.is_empty()).then(|| Line {
                samples,
                tip: Tip {
                    size,
                    size_by: crate::brush::Dynamics::NONE,
                    ..Tip::default()
                },
                colors: [color, color],
                finish: Finish::default(),
                copies: Vec::new(),
                marks: Vec::new(),
            })
        })
        .collect()
}

/// `points` with those closer than `tolerance` to the line between their
/// neighbors left out (Ramer, Douglas and Peucker).
fn simplify(points: &[([f32; 2], f32)], tolerance: f32) -> Vec<([f32; 2], f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let (first, last) = (points[0].0, points[points.len() - 1].0);
    let (dx, dy) = (last[0] - first[0], last[1] - first[1]);
    let length = dx.hypot(dy);
    let (mut far, mut index) = (0.0, 0);
    for (at, (point, _)) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let distance = if length < 1e-6 {
            (point[0] - first[0]).hypot(point[1] - first[1])
        } else {
            ((point[0] - first[0]) * dy - (point[1] - first[1]) * dx).abs() / length
        };
        if distance > far {
            far = distance;
            index = at;
        }
    }
    if far <= tolerance {
        return vec![points[0], points[points.len() - 1]];
    }
    let mut left = simplify(&points[..=index], tolerance);
    let right = simplify(&points[index..], tolerance);
    left.pop();
    left.extend(right);
    left
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

/// `lines` with the ones at `picked` moved by `by`.
#[must_use]
pub fn moved(lines: &[Line], picked: &[usize], by: [f32; 2]) -> Vec<Line> {
    let mut out = lines.to_vec();
    for index in picked {
        if let Some(line) = out.get_mut(*index) {
            for sample in &mut line.samples {
                sample.at[0] += by[0];
                sample.at[1] += by[1];
            }
        }
    }
    out
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
    // A line of one sample is that sample (and `clamp` below would panic).
    if let [only] = samples {
        return *only;
    }
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
    fn a_painted_stroke_traces_to_one_line_down_its_middle() {
        let (w, h) = (60_u32, 30_u32);
        let mut pixels = vec![0_u8; (w * h * 4) as usize];
        // A bar 5 pixels thick from x 10 to 50.
        for y in 13..18 {
            for x in 10..50 {
                pixels[((y * w + x) * 4) as usize..][..4].copy_from_slice(&[200, 0, 0, 255]);
            }
        }
        let lines = trace(&pixels, w, h);
        assert_eq!(lines.len(), 1, "one center line");
        let line = &lines[0];
        let xs: Vec<f32> = line.samples.iter().map(|sample| sample.at[0]).collect();
        let (low, high) = (
            xs.iter().copied().fold(f32::MAX, f32::min),
            xs.iter().copied().fold(0.0, f32::max),
        );
        assert!(low < 15.0 && high > 45.0, "it runs the bar's length");
        assert!(
            line.samples
                .iter()
                .all(|sample| (sample.at[1] - 15.5).abs() < 1.5),
            "down the middle"
        );
        assert!(
            line.tip.size >= 3.0 && line.tip.size <= 7.0,
            "about as wide as the bar"
        );
        assert!((line.colors[0][0] - 200.0 / 255.0).abs() < 1e-3);
        assert!(line.samples.len() <= 4, "a straight bar needs few points");
        assert!(trace(&vec![0; (w * h * 4) as usize], w, h).is_empty());
    }

    #[test]
    fn an_svg_drawing_becomes_lines_that_fit_the_canvas() {
        let drawing = sue_io::svg::parse_svg(
            r##"<svg width="200" height="100"><path d="M0 0 L200 100" stroke="#ff0000" stroke-width="4"/>
            <rect x="10" y="10" width="20" height="20" fill="blue"/></svg>"##,
        )
        .unwrap();
        let lines = from_svg(&drawing, [100, 100]);
        assert_eq!(lines.len(), 2);
        let last = lines[0].samples.last().unwrap().at;
        assert!(
            (last[0] - 100.0).abs() < 1e-3 && (last[1] - 50.0).abs() < 1e-3,
            "halved to fit"
        );
        assert!((lines[0].tip.size - 2.0).abs() < 1e-3);
        assert_eq!(lines[0].colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(lines[1].samples.len(), 5, "the square closes");
        assert!(render(&lines, 100, 100).is_some());
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
