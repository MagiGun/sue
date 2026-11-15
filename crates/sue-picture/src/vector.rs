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

use std::sync::Arc;

use sue_brush::input::{Device, Kind, Sample as PenSample, Settings};
use sue_brush::paint::{Behaviours, Colors, PaperChoice, Resources, TextureChoice};
use sue_brush::raster::TipImage;
use sue_brush::replay::{self, StoredStroke};
use sue_brush::texture::Paper;
use sue_core::txn::{RasterEdit, SetExtension};
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D};
use sue_model::brush::{ContentHash, PictureRef, TipSource};
use sue_model::{AttributeData, BrushPreset, Domain, Geometry, PrimKind, Response};

use crate::brush::{Copy2D, Finish, SAME, Sample, Stroke, Tip, TipShape};
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
    /// What the brush engine keeps of a line it paints; `None` for a
    /// line of the legacy engine (Sue 0.0.75 and before), which `tip`
    /// paints from `samples`.
    pub engine: Option<EngineLine>,
}

/// `lines` as the shapes of an SVG drawing (File, Special Export, Export
/// vectors): each line an outline filled in its color, as wide as its
/// brush made it at each sample, so pressure and tapers come along.
#[must_use]
pub fn svg_shapes(lines: &[Line], offset: [f32; 2]) -> Vec<sue_io::svg::SvgShape> {
    lines
        .iter()
        .filter(|line| !line.tip.eraser && line.samples.len() >= 2)
        .map(|line| {
            let count = line.samples.len();
            let half: Vec<f32> = line
                .samples
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let size = line.tip.size
                        * line
                            .tip
                            .size_by
                            .factor(sample.pressure, sample.speed, sample.tilt, 1.0)
                        * sample.width;
                    // The ends come to their tapers' points.
                    let along = index as f32 / (count - 1) as f32;
                    let ends = if line.tip.taper_start > 0.0 || line.tip.taper_end > 0.0 {
                        (along.min(1.0 - along) * 8.0).clamp(0.15, 1.0)
                    } else {
                        1.0
                    };
                    (size * ends / 2.0).max(0.25)
                })
                .collect();
            let at = |index: usize| {
                let p = line.samples[index].at;
                [p[0] + offset[0], p[1] + offset[1]]
            };
            let normal = |index: usize| {
                let (a, b) = (at(index.saturating_sub(1)), at((index + 1).min(count - 1)));
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let length = dx.hypot(dy).max(1e-6);
                [-dy / length, dx / length]
            };
            let mut outline = Vec::with_capacity(count * 2);
            for (index, half) in half.iter().enumerate() {
                let (p, n) = (at(index), normal(index));
                outline.push([p[0] + n[0] * half, p[1] + n[1] * half]);
            }
            for (index, half) in half.iter().enumerate().rev() {
                let (p, n) = (at(index), normal(index));
                outline.push([p[0] - n[0] * half, p[1] - n[1] * half]);
            }
            let rgb = line.colors[0].map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
            sue_io::svg::SvgShape {
                points: outline,
                closed: true,
                stroke: None,
                fill: Some(rgb),
                width: 0.0,
            }
        })
        .collect()
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
            engine: None,
        }
    }

    /// Paints the line onto `raster`: a line of the brush engine through
    /// [`sue_brush::replay`], to the bytes it was drawn with; a legacy
    /// line through the frozen legacy engine. A line of an engine this
    /// build does not know, or whose brush the layer lacks, paints
    /// nothing.
    pub fn paint(&self, raster: &mut RasterTile2D) {
        if let Some(engine) = &self.engine {
            engine.paint(self, raster);
            return;
        }
        let mut stroke = Stroke::new(EntityId::NIL, self.tip, self.colors[0], false);
        stroke.set_sub_color(self.colors[1]);
        stroke.set_copies(self.copies.clone());
        for sample in &self.samples {
            stroke.set_speed(sample.speed);
            stroke.set_tilt(sample.tilt);
            stroke.set_lean(sample.lean);
            stroke.set_shape(sample.width, sample.alpha);
            stroke.to(raster, sample.at, sample.pressure, None);
        }
        stroke.end(raster, self.finish, None);
    }

    /// The same line along other samples.
    pub(crate) fn along(&self, samples: Vec<Sample>, keep_flick: bool) -> Self {
        let engine = self
            .engine
            .as_ref()
            .map(|engine| engine.along(&self.samples, &samples));
        Self {
            engine,
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
                    tilt: 0.0,
                    lean: [0.0, 0.0],
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
                engine: None,
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
                    tilt: 0.0,
                    lean: [0.0, 0.0],
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
                engine: None,
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
    // Lines kept before tilt have none: upright.
    let tilt = floats(geometry, Domain::Point, "tilt", 1).filter(|v| v.len() == count);
    // And before Turn with pen tilt, no direction: dabs keep their turn.
    let lean = floats(geometry, Domain::Point, "lean", 2).filter(|v| v.len() == count * 2);
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
    let engines = EngineAttributes::read(geometry);
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
                        tilt: tilt.map_or(0.0, |values| values[index]),
                        lean: lean.map_or([0.0; 2], |values| {
                            [values[index * 2], values[index * 2 + 1]]
                        }),
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
            let one = single.is_some_and(|values| values[at] == 1);
            if one {
                samples.truncate(1);
                flags.truncate(1);
            }
            let engine = engines.line(at, geometry.prim_points(prim), samples.len());
            if let Some(engine) = &engine {
                engine.show(&mut samples);
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
                engine,
            }
        })
        .collect()
}

/// `lines` as geometry, to keep on a vector layer.
#[must_use]
pub fn to_geometry(lines: &[Line]) -> Geometry {
    let mut geometry = Geometry::new();
    let mut engines = EngineAttributes::default();
    let (mut pressure, mut speed, mut tilt) = (Vec::new(), Vec::new(), Vec::new());
    let mut lean: Vec<f32> = Vec::new();
    let (mut width, mut alpha) = (Vec::new(), Vec::new());
    let mut marks: Vec<i32> = Vec::new();
    let (mut colors, mut subs, mut flicks) = (Vec::new(), Vec::new(), Vec::new());
    let (mut corrections, mut flicked, mut single) = (Vec::new(), Vec::new(), Vec::new());
    let (mut tips, mut copies) = (Vec::new(), Vec::new());
    for line in lines.iter().filter(|line| !line.samples.is_empty()) {
        marks.extend(
            (0..line.samples.len()).map(|at| i32::from(line.marks.get(at).copied().unwrap_or(0))),
        );
        let raw = line
            .engine
            .as_ref()
            .map(|engine| engine.pen_for(&line.samples));
        let mut indices: Vec<u32> = line
            .samples
            .iter()
            .enumerate()
            .map(|(at, sample)| {
                let part = raw.as_ref().map(|raw| raw[at]);
                engines.point(part.as_ref());
                match part {
                    // A line of the brush engine keeps what the pen said.
                    Some(part) => {
                        pressure.push(part.pressure.unwrap_or(f32::NAN));
                        let [x, y] = part.tilt.unwrap_or([f32::NAN; 2]);
                        tilt.push((x * x + y * y).sqrt());
                        lean.extend([x, y]);
                    }
                    None => {
                        pressure.push(sample.pressure);
                        tilt.push(sample.tilt);
                        lean.extend(sample.lean);
                    }
                }
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
            engines.point(raw.as_ref().map(|raw| &raw[0]));
            pressure.push(sample.pressure);
            speed.push(sample.speed);
            tilt.push(sample.tilt);
            lean.extend(sample.lean);
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
        engines.prim(line.engine.as_ref());
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
        (Domain::Point, "tilt", float(1, tilt)),
        (Domain::Point, "lean", float(2, lean)),
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
    engines.write(&mut geometry);
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
        tilt: mix(a.tilt, b.tilt),
        lean: [mix(a.lean[0], b.lean[0]), mix(a.lean[1], b.lean[1])],
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

// ----- Lines of the brush engine ---------------------------------------------------

/// One picture a brush names by hash (a tip, a paper): its 8-bit coverage,
/// kept on the layer so a line paints the same without the library it
/// was drawn from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrushPicture {
    /// Its width in texels.
    pub width: u32,
    /// Its height in texels.
    pub height: u32,
    /// `width * height` coverages, row by row, 0 (none) to 255 (full).
    pub coverage: Vec<u8>,
}

impl BrushPicture {
    /// The picture's hash: SHA-256 of its width and height (little
    /// endian `u32`s) and its coverage.
    #[must_use]
    pub fn hash(&self) -> ContentHash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.width.to_le_bytes());
        hasher.update(self.height.to_le_bytes());
        hasher.update(&self.coverage);
        hasher.finalize().into()
    }

    /// The legacy engine's tip shape `shape` as a picture (its own
    /// colors, which only the legacy engine paints, are left behind).
    #[must_use]
    pub fn of_shape(shape: &TipShape) -> Option<Self> {
        let side = shape.side;
        let count = (side as usize).checked_mul(side as usize)?;
        if side == 0 || shape.cover.len() != count {
            return None;
        }
        Some(Self {
            width: side,
            height: side,
            coverage: shape
                .cover
                .iter()
                .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        })
    }

    fn tip(&self) -> Option<TipImage> {
        TipImage::from_coverage(self.width as usize, self.height as usize, &self.coverage)
    }

    fn paper(&self) -> Option<Paper> {
        Paper::from_gray8(self.width as usize, self.height as usize, &self.coverage)
    }
}

/// The brush a line of the brush engine paints with, and the pictures it
/// names by hash.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBrush {
    /// The brush as it was when the line was drawn.
    pub preset: BrushPreset,
    /// The pictures it names, by hash.
    pub pictures: Vec<(ContentHash, BrushPicture)>,
}

impl LineBrush {
    /// The hash a line names its brush by: SHA-256 of
    /// [`BrushPreset::to_bytes`].
    #[must_use]
    pub fn hash(&self) -> ContentHash {
        use sha2::{Digest, Sha256};
        Sha256::digest(self.preset.to_bytes()).into()
    }

    fn picture(&self, hash: &ContentHash) -> Option<&BrushPicture> {
        self.pictures
            .iter()
            .find(|(known, _)| known == hash)
            .map(|(_, picture)| picture)
    }

    /// The pictures the brush's tip, dual brush and paper read, decoded.
    fn decoded(&self) -> Decoded {
        let behaviours = Behaviours::from_preset(&self.preset);
        let tips = match &self.preset.tip {
            TipSource::Images { pictures, .. } => pictures
                .iter()
                .filter_map(|picture| match picture {
                    PictureRef::Hash(hash) => self.picture(hash)?.tip(),
                    PictureRef::Named(_) => None,
                })
                .collect(),
            TipSource::Round => Vec::new(),
        };
        let dual = behaviours
            .dual
            .as_ref()
            .map(|dual| {
                dual.pictures
                    .iter()
                    .filter_map(|hash| self.picture(hash)?.tip())
                    .collect()
            })
            .unwrap_or_default();
        let paper = behaviours
            .texture
            .as_ref()
            .and_then(|texture| match &texture.paper {
                PaperChoice::Builtin(name) => Paper::builtin(name),
                PaperChoice::Picture(hash) => self.picture(hash)?.paper(),
            });
        Decoded { tips, dual, paper }
    }
}

/// A brush's pictures, decoded for the engine.
struct Decoded {
    tips: Vec<TipImage>,
    dual: Vec<TipImage>,
    paper: Option<Paper>,
}

/// What the pen said at one sample of a line of the brush engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenPart {
    /// Seconds since the landing.
    pub time: f32,
    /// The pressure, when the pen said one.
    pub pressure: Option<f32>,
    /// The tilt, `[x, y]`, when the pen said one.
    pub tilt: Option<[f32; 2]>,
    /// The twist in degrees, when the pen said one.
    pub twist: Option<f32>,
}

impl PenPart {
    /// The part `t` of the way from `self` to `other`; a part only one
    /// of them said is taken from that one.
    fn mix(&self, other: &Self, t: f32) -> Self {
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let either = |a: Option<f32>, b: Option<f32>| match (a, b) {
            (Some(a), Some(b)) => Some(lerp(a, b)),
            (a, b) => a.or(b),
        };
        Self {
            time: lerp(self.time, other.time),
            pressure: either(self.pressure, other.pressure),
            tilt: match (self.tilt, other.tilt) {
                (Some(a), Some(b)) => Some([lerp(a[0], b[0]), lerp(a[1], b[1])]),
                (a, b) => a.or(b),
            },
            twist: either(self.twist, other.twist),
        }
    }
}

/// What a line of the brush engine keeps beyond its places: what replays
/// it ([`sue_brush::replay`]). The places are the line's samples, so a
/// line moved, transformed or corrected moves with them.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineLine {
    /// The engine version that paints it ([`sue_brush::replay::ENGINE`]).
    pub engine: u32,
    /// The seed of its chances.
    pub seed: u64,
    /// The hash of its brush ([`LineBrush::hash`]).
    pub brush_hash: ContentHash,
    /// Its brush, when the layer has it.
    pub brush: Option<Arc<LineBrush>>,
    /// The device that drew it.
    pub device: Device,
    /// Screen points per layer pixel when it was drawn.
    pub scale: f32,
    /// The person's pressure curve when it was drawn.
    pub curve: Response,
    /// What the pen said at each sample, beside the line's samples.
    pub pen: Vec<PenPart>,
}

impl EngineLine {
    /// What the pen said at each of `samples`: its own record when it
    /// has one a sample, else (a tool that knows nothing of the engine
    /// changed the samples) made again from them, time spread by arc
    /// length over the stroke's own span.
    #[must_use]
    pub fn pen_for(&self, samples: &[Sample]) -> Vec<PenPart> {
        if self.pen.len() == samples.len() {
            return self.pen.clone();
        }
        let arcs = arcs(samples);
        let total = arcs.last().copied().unwrap_or(0.0);
        let (first, last) = (
            self.pen.first().map_or(0.0, |p| p.time),
            self.pen.last().map_or(0.0, |p| p.time),
        );
        let said_tilt = self.pen.iter().any(|p| p.tilt.is_some());
        samples
            .iter()
            .zip(&arcs)
            .enumerate()
            .map(|(at, (sample, arc))| PenPart {
                time: if total > 0.0 {
                    first + (last - first) * (arc / total)
                } else {
                    first + (last - first) * at as f32 / (samples.len().max(2) - 1) as f32
                },
                pressure: Some(sample.pressure),
                tilt: said_tilt.then_some(sample.lean),
                twist: None,
            })
            .collect()
    }

    /// The same line along `new` samples cut from `old` (the vector
    /// eraser): each takes what the pen said at its place along the old
    /// line.
    #[must_use]
    pub fn along(&self, old: &[Sample], new: &[Sample]) -> Self {
        let pen = self.pen_for(old);
        let arcs = arcs(old);
        let parts = new
            .iter()
            .map(|sample| {
                if old.len() < 2 {
                    return pen.first().copied().unwrap_or(PenPart {
                        time: 0.0,
                        pressure: None,
                        tilt: None,
                        twist: None,
                    });
                }
                let along = touch(old, &arcs, sample.at, f32::INFINITY).unwrap_or(0.0);
                let index = arcs
                    .partition_point(|arc| *arc < along)
                    .clamp(1, old.len() - 1);
                let span = arcs[index] - arcs[index - 1];
                let t = if span > 0.0 {
                    ((along - arcs[index - 1]) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                pen[index - 1].mix(&pen[index], t)
            })
            .collect();
        Self {
            pen: parts,
            ..self.clone()
        }
    }

    /// Fills what the legacy tools read of `samples` (pressure, tilt and
    /// lean) from what the pen said, a part it did not say from the one
    /// before (or after) it.
    pub fn show(&self, samples: &mut [Sample]) {
        let pen = self.pen_for(samples);
        let first = pen.iter().find_map(|p| p.pressure).unwrap_or(1.0);
        let mut pressure = first;
        let mut lean = [0.0; 2];
        for (sample, part) in samples.iter_mut().zip(&pen) {
            pressure = part.pressure.filter(|p| p.is_finite()).unwrap_or(pressure);
            lean = part.tilt.unwrap_or(lean);
            sample.pressure = pressure;
            sample.lean = lean;
            sample.tilt = (lean[0] * lean[0] + lean[1] * lean[1]).sqrt().min(1.0);
        }
    }

    /// The stroke `line` keeps, as the engine reads it: its first sample
    /// the landing, its last the lift.
    #[must_use]
    pub fn stored(&self, line: &Line) -> StoredStroke {
        let settings = Settings {
            device: self.device,
            curve: self.curve,
            to_layer: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            scale: self.scale,
            seed: self.seed,
            step: None,
        };
        // A line drawn once keeps one copy that leaves it where it is.
        let copies = if line.copies.iter().all(|copy| *copy == SAME) {
            Vec::new()
        } else {
            line.copies.clone()
        };
        let mut stroke = StoredStroke::new(
            self.engine,
            self.brush_hash,
            &settings,
            Colors {
                main: line.colors[0],
                sub: line.colors[1],
            },
            copies,
        );
        let pen = self.pen_for(&line.samples);
        let last = line.samples.len().saturating_sub(1);
        stroke.samples = line
            .samples
            .iter()
            .zip(&pen)
            .enumerate()
            .map(|(at, (sample, part))| PenSample {
                pos: sample.at,
                time: f64::from(part.time),
                pressure: part.pressure,
                tilt: part.tilt,
                twist: part.twist,
                kind: if at == 0 {
                    Kind::Down
                } else if at == last {
                    Kind::Up
                } else {
                    Kind::Move
                },
            })
            .collect();
        stroke
    }

    /// Paints `line` onto `raster` again.
    fn paint(&self, line: &Line, raster: &mut RasterTile2D) {
        let Some(brush) = &self.brush else {
            return;
        };
        let decoded = brush.decoded();
        let resources = Resources {
            pictures: &decoded.tips,
            dual_pictures: &decoded.dual,
            paper: decoded.paper.as_ref(),
            overflow: None,
        };
        // A line of an engine this build does not know paints nothing:
        // its version's code paints it.
        let _ = replay::paint(
            &self.stored(line),
            &brush.preset,
            resources,
            raster,
            [0, 0],
            false,
        );
    }
}

/// The hash as 64 hexadecimal digits.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The bytes `text` spells in hexadecimal digits.
fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

fn hash_of(text: &str) -> Option<ContentHash> {
    unhex(text)?.try_into().ok()
}

/// A pressure curve as text: its points, `x,y x,y ...`.
fn curve_text(curve: &Response) -> String {
    curve
        .points()
        .iter()
        .map(|[x, y]| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn curve_of(text: &str) -> Response {
    let points: Vec<[f32; 2]> = text
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some([x.parse().ok()?, y.parse().ok()?])
        })
        .collect();
    if points.len() < 2 {
        Response::LINEAR
    } else {
        Response::new(&points)
    }
}

/// The attributes lines of the brush engine add to a vector layer's
/// geometry (`docs/stored-stroke-format.md`): written only when a line
/// of the engine is there, so a layer of legacy lines keeps the
/// attributes Sue 0.0.75 wrote.
#[derive(Default)]
struct EngineAttributes {
    any: bool,
    engine: Vec<i32>,
    seed: Vec<String>,
    brush: Vec<String>,
    device: Vec<f32>,
    scale: Vec<f32>,
    curve: Vec<String>,
    time: Vec<f32>,
    twist: Vec<f32>,
    raw_pressure: Vec<f32>,
    raw_lean: Vec<f32>,
    brushes: Vec<(ContentHash, Arc<LineBrush>)>,
}

impl EngineAttributes {
    /// One point; `part` for a point of a line of the brush engine.
    fn point(&mut self, part: Option<&PenPart>) {
        self.time.push(part.map_or(0.0, |p| p.time));
        self.twist
            .push(part.and_then(|p| p.twist).unwrap_or(f32::NAN));
    }

    /// One line; `engine` for a line of the brush engine.
    fn prim(&mut self, engine: Option<&EngineLine>) {
        match engine {
            Some(e) => {
                self.any = true;
                self.engine
                    .push(i32::try_from(e.engine).unwrap_or(i32::MAX));
                self.seed.push(e.seed.to_string());
                self.brush.push(hex(&e.brush_hash));
                self.device.push(match e.device {
                    Device::Pen => -1.0,
                    Device::Constant(pressure) => pressure,
                });
                self.scale.push(e.scale);
                self.curve.push(curve_text(&e.curve));
                if let Some(brush) = &e.brush
                    && !self.brushes.iter().any(|(hash, _)| *hash == e.brush_hash)
                {
                    self.brushes.push((e.brush_hash, Arc::clone(brush)));
                }
            }
            None => {
                self.engine.push(0);
                self.seed.push(String::new());
                self.brush.push(String::new());
                self.device.push(-1.0);
                self.scale.push(1.0);
                self.curve.push(String::new());
            }
        }
    }

    /// Writes the attributes into `geometry`, when a line needs them.
    fn write(self, geometry: &mut Geometry) {
        if !self.any {
            return;
        }
        let mut pictures: Vec<(ContentHash, &BrushPicture)> = Vec::new();
        for (_, brush) in &self.brushes {
            for (hash, picture) in &brush.pictures {
                if !pictures.iter().any(|(known, _)| known == hash) {
                    pictures.push((*hash, picture));
                }
            }
        }
        let brushes = self
            .brushes
            .iter()
            .map(|(_, brush)| hex(&brush.preset.to_bytes()))
            .collect::<Vec<_>>()
            .join("\n");
        let pictures = pictures
            .iter()
            .map(|(hash, p)| {
                format!(
                    "{} {} {} {}",
                    hex(hash),
                    p.width,
                    p.height,
                    hex(&p.coverage)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let float = |values: Vec<f32>| AttributeData::Float { width: 1, values };
        for (domain, name, data) in [
            (
                Domain::Primitive,
                "engine",
                AttributeData::Int {
                    width: 1,
                    values: self.engine,
                },
            ),
            (Domain::Primitive, "seed", AttributeData::Text(self.seed)),
            (Domain::Primitive, "brush", AttributeData::Text(self.brush)),
            (Domain::Primitive, "device", float(self.device)),
            (Domain::Primitive, "scale", float(self.scale)),
            (Domain::Primitive, "curve", AttributeData::Text(self.curve)),
            (Domain::Point, "time", float(self.time)),
            (Domain::Point, "twist", float(self.twist)),
            (
                Domain::Detail,
                "brushes",
                AttributeData::Text(vec![brushes]),
            ),
            (
                Domain::Detail,
                "pictures",
                AttributeData::Text(vec![pictures]),
            ),
        ] {
            // Each holds one value an element, so this always fits.
            let _ = geometry.set_attribute(domain, name, data);
        }
    }

    /// What a layer's `geometry` holds of lines of the brush engine.
    fn read(geometry: &Geometry) -> Self {
        let mut out = Self::default();
        let prims = geometry.prims().len();
        let count = geometry.points().len();
        let Some(engine) = ints(geometry, Domain::Primitive, "engine").filter(|v| v.len() == prims)
        else {
            return out;
        };
        out.any = engine.iter().any(|e| *e > 0);
        if !out.any {
            return out;
        }
        out.engine = engine.to_vec();
        let text = |name: &str| {
            texts(geometry, Domain::Primitive, name)
                .filter(|v| v.len() == prims)
                .map(<[String]>::to_vec)
                .unwrap_or_else(|| vec![String::new(); prims])
        };
        let float = |domain: Domain, name: &str, len: usize, or: f32| {
            floats(geometry, domain, name, 1)
                .filter(|v| v.len() == len)
                .map_or_else(|| vec![or; len], <[f32]>::to_vec)
        };
        out.seed = text("seed");
        out.brush = text("brush");
        out.curve = text("curve");
        out.device = float(Domain::Primitive, "device", prims, -1.0);
        out.scale = float(Domain::Primitive, "scale", prims, 1.0);
        out.time = float(Domain::Point, "time", count, 0.0);
        out.twist = float(Domain::Point, "twist", count, f32::NAN);
        out.raw_pressure = float(Domain::Point, "pressure", count, f32::NAN);
        out.raw_lean = floats(geometry, Domain::Point, "lean", 2)
            .filter(|v| v.len() == count * 2)
            .map_or_else(|| vec![f32::NAN; count * 2], <[f32]>::to_vec);
        let detail = |name: &str| {
            texts(geometry, Domain::Detail, name)
                .and_then(|v| v.first().cloned())
                .unwrap_or_default()
        };
        let pictures: Vec<(ContentHash, BrushPicture)> = detail("pictures")
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let hash = hash_of(words.next()?)?;
                let width: u32 = words.next()?.parse().ok()?;
                let height: u32 = words.next()?.parse().ok()?;
                let coverage = unhex(words.next().unwrap_or(""))?;
                (coverage.len() == width as usize * height as usize).then_some((
                    hash,
                    BrushPicture {
                        width,
                        height,
                        coverage,
                    },
                ))
            })
            .collect();
        for line in detail("brushes").lines() {
            let Some(preset) = unhex(line).and_then(|b| BrushPreset::from_bytes(&b).ok()) else {
                continue;
            };
            let named = named_hashes(&preset);
            let brush = LineBrush {
                pictures: pictures
                    .iter()
                    .filter(|(hash, _)| named.contains(hash))
                    .cloned()
                    .collect(),
                preset,
            };
            out.brushes.push((brush.hash(), Arc::new(brush)));
        }
        out
    }

    /// Line `at`, over `points`, cut to its first `len` samples, when it
    /// is a line of the brush engine.
    fn line(&self, at: usize, points: &[u32], len: usize) -> Option<EngineLine> {
        let engine = u32::try_from(*self.engine.get(at)?)
            .ok()
            .filter(|e| *e > 0)?;
        let brush_hash = hash_of(self.brush.get(at)?).unwrap_or([0; 32]);
        let finite = |value: f32| value.is_finite().then_some(value);
        let pen = points
            .iter()
            .take(len)
            .map(|&point| {
                let i = point as usize;
                let tilt = [self.raw_lean[i * 2], self.raw_lean[i * 2 + 1]];
                PenPart {
                    time: self.time[i],
                    pressure: finite(self.raw_pressure[i]),
                    tilt: (tilt[0].is_finite() && tilt[1].is_finite()).then_some(tilt),
                    twist: finite(self.twist[i]),
                }
            })
            .collect();
        Some(EngineLine {
            engine,
            seed: self.seed.get(at)?.parse().unwrap_or(0),
            brush: self
                .brushes
                .iter()
                .find(|(hash, _)| *hash == brush_hash)
                .map(|(_, brush)| Arc::clone(brush)),
            brush_hash,
            device: match self.device[at] {
                pressure if pressure >= 0.0 => Device::Constant(pressure),
                _ => Device::Pen,
            },
            scale: self.scale[at],
            curve: curve_of(&self.curve[at]),
            pen,
        })
    }
}

/// The picture hashes `preset` names: its tip's, its dual brush's and its
/// paper's.
fn named_hashes(preset: &BrushPreset) -> Vec<ContentHash> {
    let mut out = Vec::new();
    if let TipSource::Images { pictures, .. } = &preset.tip {
        out.extend(pictures.iter().filter_map(|p| match p {
            PictureRef::Hash(hash) => Some(*hash),
            PictureRef::Named(_) => None,
        }));
    }
    let behaviours = Behaviours::from_preset(preset);
    if let Some(dual) = &behaviours.dual {
        out.extend(dual.pictures.iter().copied());
    }
    if let Some(TextureChoice {
        paper: PaperChoice::Picture(hash),
        ..
    }) = &behaviours.texture
    {
        out.push(*hash);
    }
    out
}

// ----- Update lines to the new brush engine ----------------------------------------

/// How many samples a legacy line dwells at its landing when it becomes
/// a line of the brush engine, and how far apart in seconds: long enough
/// for the engine's limit on how fast pressure rises (a full press in
/// 10 ms) to let the first pressure through whole.
const LANDING: usize = 3;
const LANDING_STEP: f32 = 0.004;

/// The time between two samples of a legacy line, which kept none: as
/// long as its stored speed says the pen took over the distance, from a
/// millisecond to a twentieth of a second.
fn legacy_step(from: &Sample, to: &Sample) -> f32 {
    let (dx, dy) = (to.at[0] - from.at[0], to.at[1] - from.at[1]);
    let distance = (dx * dx + dy * dy).sqrt();
    // The legacy speed is screen points a second over 2,000.
    let speed = (0.5 * (from.speed + to.speed) * 2_000.0).max(40.0);
    (distance / speed).clamp(0.001, 0.05)
}

/// The brush of the engine a legacy `tip` finished with `finish` is: its
/// text read as a preset (`BrushPreset::from_legacy_text`), with its tip
/// pictures kept by hash.
fn legacy_brush(tip: &Tip, finish: Finish) -> LineBrush {
    let mut preset = BrushPreset::from_legacy_text(&tip.to_text());
    // The finish it was drawn with is the brush's in the engine.
    preset.input.post_correction = finish.correction;
    preset.input.flick = finish.flick.is_some();
    let mut pictures = Vec::new();
    if let TipSource::Images { pictures: refs, .. } = &mut preset.tip {
        let count = tip.shape_count.max(1);
        let mut kept = Vec::new();
        for id in tip.shape..tip.shape.saturating_add(count) {
            if let Some(picture) = crate::brush::shape(id)
                .as_deref()
                .and_then(BrushPicture::of_shape)
            {
                let hash = picture.hash();
                kept.push(PictureRef::Hash(hash));
                pictures.push((hash, picture));
            }
        }
        if !kept.is_empty() {
            *refs = kept;
        }
    }
    LineBrush { preset, pictures }
}

/// The legacy `line` as a line of the brush engine: its brush read from
/// its text (`BrushPreset::from_legacy_text`) with its tip pictures kept
/// by hash, the pen's pressure and lean as it said them, time made from
/// the speeds it kept, the pen dwelling a moment where it landed, a lift
/// where it ended, and the seed the legacy
/// engine took from its first place. `None` for a line already of the
/// engine, or one with no samples.
#[must_use]
pub fn to_engine(line: &Line) -> Option<Line> {
    if line.engine.is_some() || line.samples.is_empty() {
        return None;
    }
    let brush = legacy_brush(&line.tip, line.finish);
    // The legacy engine painted the first sample at its full pressure;
    // the engine lets a landing's pressure rise over a few milliseconds
    // (against spikes), so the pen dwells there that long first.
    let mut samples = vec![line.samples[0]; LANDING];
    samples.extend_from_slice(&line.samples);
    let mut time = 0.0;
    let mut pen = Vec::with_capacity(samples.len() + 1);
    for (at, sample) in samples.iter().enumerate() {
        if at > 0 {
            time += if at <= LANDING {
                LANDING_STEP
            } else {
                legacy_step(&samples[at - 1], sample)
            };
        }
        let leaned = sample.lean != [0.0, 0.0];
        pen.push(PenPart {
            time,
            pressure: Some(sample.pressure),
            tilt: leaned.then_some(sample.lean),
            twist: None,
        });
    }
    // The lift, where the line ended.
    let last = *samples.last()?;
    samples.push(last);
    pen.push(PenPart {
        time: time + 0.004,
        ..*pen.last()?
    });
    let first = samples[0].at;
    let seed = u64::from((first[0].to_bits() ^ first[1].to_bits().rotate_left(16)) | 1);
    let mut marks = line.marks.clone();
    if !marks.is_empty() {
        marks.splice(0..0, [0; LANDING]);
        marks.push(0);
    }
    Some(Line {
        samples,
        marks,
        engine: Some(EngineLine {
            engine: replay::ENGINE,
            seed,
            brush_hash: brush.hash(),
            brush: Some(Arc::new(brush)),
            device: Device::Pen,
            scale: 1.0,
            curve: Response::LINEAR,
            pen,
        }),
        ..line.clone()
    })
}

/// "Update lines to the new brush engine": every legacy line of the
/// vector `layer` becomes a line of the brush engine ([`to_engine`]) and
/// the layer is painted again from its lines, as one step named `label`
/// that undo takes back whole. `None` when the layer is not a vector
/// layer or holds no legacy line.
#[must_use]
pub fn update_lines(arena: &mut Arena, layer: EntityId, label: &str) -> Option<Edit> {
    relined(arena, layer, label, |before| {
        if before.iter().all(|line| line.engine.is_some()) {
            return None;
        }
        Some(
            before
                .iter()
                .map(|line| to_engine(line).unwrap_or_else(|| line.clone()))
                .collect(),
        )
    })
}

/// What [`update_lines_kept`] did: how many lines it updated, and how
/// many legacy lines it left as they were because the engine cannot yet
/// draw them as they look (a flick, or post correction).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Updated {
    /// Lines now of the brush engine.
    pub updated: usize,
    /// Legacy lines left to the legacy engine.
    pub kept: usize,
}

/// `true` when the brush engine draws `line` with everything the legacy
/// engine gave it: engine 1 has no flick and no post correction, so a
/// line finished with either would lose it.
#[must_use]
pub fn updates_unchanged(line: &Line) -> bool {
    line.finish.flick.is_none() && line.finish.correction == 0
}

/// The menu's "Update Lines to New Brush Engine": [`update_lines`], but
/// a legacy line that would lose its flick or its post correction stays
/// a legacy line ([`updates_unchanged`]), so nothing a person drew
/// changes how it ends or how smooth it is. `None` when there is
/// nothing to update; else the step and what it did.
#[must_use]
pub fn update_lines_kept(
    arena: &mut Arena,
    layer: EntityId,
    label: &str,
) -> Option<(Edit, Updated)> {
    let mut did = Updated::default();
    let edit = relined(arena, layer, label, |before| {
        let after: Vec<Line> = before
            .iter()
            .map(|line| {
                if line.engine.is_some() {
                    return line.clone();
                }
                if !updates_unchanged(line) {
                    did.kept += 1;
                    return line.clone();
                }
                match to_engine(line) {
                    Some(updated) => {
                        did.updated += 1;
                        updated
                    }
                    None => line.clone(),
                }
            })
            .collect();
        (did.updated > 0).then_some(after)
    });
    edit.map(|edit| (edit, did))
}

/// How many legacy lines the vector `layer` holds, and how many of them
/// [`update_lines_kept`] would leave as they are.
#[must_use]
pub fn legacy_lines(arena: &Arena, layer: EntityId) -> Updated {
    let mut out = Updated::default();
    if !is_vector(arena, layer) {
        return out;
    }
    for line in lines(&geometry(arena, layer)) {
        if line.engine.is_none() && !line.samples.is_empty() {
            if updates_unchanged(&line) {
                out.updated += 1;
            } else {
                out.kept += 1;
            }
        }
    }
    out
}

/// The lines of the vector `layer` changed by `change` (from the lines
/// there, `None` to change nothing), the layer painted again from them,
/// as one step named `label` that undo takes back whole. `None` when the
/// layer is not a vector layer or nothing changed.
#[must_use]
pub fn relined(
    arena: &mut Arena,
    layer: EntityId,
    label: &str,
    change: impl FnOnce(&[Line]) -> Option<Vec<Line>>,
) -> Option<Edit> {
    if !is_vector(arena, layer) {
        return None;
    }
    let old = geometry(arena, layer);
    let before = lines(&old);
    let after = change(&before)?;
    let raster = crate::doc::raster_mut(arena, layer)?;
    let (width, height) = (raster.width(), raster.height());
    let was = raster.bytes()?.to_vec();
    let painted = render(&after, width, height)?;
    raster.bytes_mut()?.copy_from_slice(painted.bytes()?);
    let pixels = crate::doc::diff_patch(&was, painted.bytes()?, width, height)
        .map(|patch| RasterEdit::new(layer, label, vec![patch]));
    Some(keep(arena, layer, old, &after, pixels, label))
}

// ----- Editing lines: Tool Property, the Object tool's box, figures --------------

/// What Tool Property sets on picked lines: each part `None` stays as
/// each line has it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Restyle {
    /// The main color (sRGB, 0 to 1).
    pub main: Option<[f32; 3]>,
    /// The sub color.
    pub sub: Option<[f32; 3]>,
    /// The brush size, in layer pixels.
    pub size: Option<f32>,
    /// The brush shape: a sub tool's brush, taken with each line's own
    /// size and its eraser setting.
    pub shape: Option<Tip>,
}

impl Restyle {
    /// `true` when it changes nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The smallest brush a line keeps, in layer pixels.
const MIN_SIZE: f32 = 0.1;

/// The brush of a line of the engine changed to `tip` (a new shape) or
/// to `size`: its preset made again, its hash with it.
fn rebrushed(engine: &mut EngineLine, tip: Option<&Tip>, finish: Finish, size: f32) {
    let mut brush = match (tip, &engine.brush) {
        (Some(tip), _) => legacy_brush(tip, finish),
        (None, Some(brush)) => (**brush).clone(),
        // A brush the layer lacks paints nothing; nothing to resize.
        (None, None) => return,
    };
    brush.preset.set(sue_model::brush::param::SIZE, size);
    engine.brush_hash = brush.hash();
    engine.brush = Some(Arc::new(brush));
}

/// `lines` with the ones at `picked` restyled as `restyle` says.
#[must_use]
pub fn restyled(lines: &[Line], picked: &[usize], restyle: &Restyle) -> Vec<Line> {
    let mut out = lines.to_vec();
    for index in picked {
        let Some(line) = out.get_mut(*index) else {
            continue;
        };
        if let Some(main) = restyle.main {
            line.colors[0] = main.map(|value| value.clamp(0.0, 1.0));
        }
        if let Some(sub) = restyle.sub {
            line.colors[1] = sub.map(|value| value.clamp(0.0, 1.0));
        }
        if restyle.shape.is_none() && restyle.size.is_none() {
            continue;
        }
        let size = restyle.size.unwrap_or(line.tip.size).max(MIN_SIZE);
        if let Some(shape) = restyle.shape {
            line.tip = Tip {
                size,
                eraser: line.tip.eraser,
                ..shape
            };
        } else {
            line.tip.size = size;
        }
        let (tip, finish) = (line.tip, line.finish);
        if let Some(engine) = &mut line.engine {
            rebrushed(
                engine,
                restyle.shape.is_some().then_some(&tip),
                finish,
                size,
            );
        }
    }
    out
}

/// The box round the lines at `picked`: `[left, top, right, bottom]` of
/// their paths in layer pixels; `None` when none of them has a sample.
#[must_use]
pub fn bounds(lines: &[Line], picked: &[usize]) -> Option<[f32; 4]> {
    let mut out = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for line in picked.iter().filter_map(|index| lines.get(*index)) {
        for sample in &line.samples {
            out[0] = out[0].min(sample.at[0]);
            out[1] = out[1].min(sample.at[1]);
            out[2] = out[2].max(sample.at[0]);
            out[3] = out[3].max(sample.at[1]);
        }
    }
    out[0].is_finite().then_some(out)
}

/// The corners of the box `[left, top, right, bottom]`: top left, top
/// right, bottom right, bottom left.
#[must_use]
pub fn corners(rect: [f32; 4]) -> [[f32; 2]; 4] {
    [
        [rect[0], rect[1]],
        [rect[2], rect[1]],
        [rect[2], rect[3]],
        [rect[0], rect[3]],
    ]
}

/// The place `at` in the box `from` (`[left, top, right, bottom]`) takes
/// when the box's corners go to `to` (top left, top right, bottom right,
/// bottom left): blended between them as it sits in the box, so a box
/// scaled stays a box and a corner moved alone bends it (free transform).
#[must_use]
pub fn map_point(at: [f32; 2], from: [f32; 4], to: &[[f32; 2]; 4]) -> [f32; 2] {
    let part = |value: f32, low: f32, high: f32| {
        if (high - low).abs() > f32::EPSILON {
            (value - low) / (high - low)
        } else {
            0.0
        }
    };
    let (u, v) = (part(at[0], from[0], from[2]), part(at[1], from[1], from[3]));
    let mix = |axis: usize| {
        (1.0 - u) * (1.0 - v) * to[0][axis]
            + u * (1.0 - v) * to[1][axis]
            + u * v * to[2][axis]
            + (1.0 - u) * v * to[3][axis]
    };
    [mix(0), mix(1)]
}

/// The area inside the four corners `quad`, in order round it.
fn quad_area(quad: &[[f32; 2]; 4]) -> f32 {
    let mut twice = 0.0;
    for index in 0..4 {
        let (a, b) = (quad[index], quad[(index + 1) % 4]);
        twice += a[0] * b[1] - b[0] * a[1];
    }
    (twice / 2.0).abs()
}

/// `lines` with the ones at `picked` mapped from their box `from` to the
/// corners `to` ([`map_point`]). With `thicken` (Adjust line thickness
/// when scaling), each line's brush grows or shrinks as the box's area
/// does, by the square root of its change.
#[must_use]
pub fn transformed(
    lines: &[Line],
    picked: &[usize],
    from: [f32; 4],
    to: &[[f32; 2]; 4],
    thicken: bool,
) -> Vec<Line> {
    let before = (from[2] - from[0]).abs() * (from[3] - from[1]).abs();
    let scale = if thicken && before > f32::EPSILON {
        (quad_area(to) / before).sqrt()
    } else {
        1.0
    };
    let mut out = lines.to_vec();
    for index in picked {
        let Some(line) = out.get_mut(*index) else {
            continue;
        };
        for sample in &mut line.samples {
            sample.at = map_point(sample.at, from, to);
        }
        if (scale - 1.0).abs() > f32::EPSILON {
            let size = (line.tip.size * scale).max(MIN_SIZE);
            line.tip.size = size;
            let finish = line.finish;
            if let Some(engine) = &mut line.engine {
                rebrushed(engine, None, finish, size);
            }
        }
    }
    out
}

/// `true` when `at` is inside the closed loop `path` (even and odd
/// crossings).
fn inside(path: &[[f32; 2]], at: [f32; 2]) -> bool {
    let Some(mut last) = path.last().copied() else {
        return false;
    };
    let mut odd = false;
    for point in path {
        let (a, b) = (last, *point);
        if (a[1] > at[1]) != (b[1] > at[1]) {
            let cross = a[0] + (at[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if at[0] < cross {
                odd = !odd;
            }
        }
        last = b;
    }
    odd
}

/// The lines a freehand loop `path` (layer pixels) drawn round them
/// picks: those with at least half of their path inside it, so a loose
/// loop that cuts a line's tip still takes it and one that only crosses
/// a line does not.
#[must_use]
pub fn in_loop(lines: &[Line], path: &[[f32; 2]]) -> Vec<usize> {
    if path.len() < 3 {
        return Vec::new();
    }
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let count = line.samples.len();
            let within = line
                .samples
                .iter()
                .filter(|sample| inside(path, sample.at))
                .count();
            count > 0 && within * 2 >= count
        })
        .map(|(index, _)| index)
        .collect()
}

/// A line a figure lays on a vector layer: along `points` (layer pixels)
/// with the brush `tip` and `colors`, at full pressure; every sample a
/// corner when `corners` (a rectangle or polyline, which Correct Line
/// edits by its corners), else its control points found from its path
/// (an ellipse or a curve).
#[must_use]
pub fn figure_line(points: &[[f32; 2]], tip: Tip, colors: [[f32; 3]; 2], corners: bool) -> Line {
    let samples: Vec<Sample> = points
        .iter()
        .map(|at| Sample {
            at: *at,
            pressure: 1.0,
            speed: 0.0,
            tilt: 0.0,
            lean: [0.0, 0.0],
            width: 1.0,
            alpha: 1.0,
        })
        .collect();
    let marks = if corners {
        vec![CONTROL | CORNER; samples.len()]
    } else {
        Vec::new()
    };
    Line {
        samples,
        tip,
        colors,
        finish: Finish::default(),
        copies: Vec::new(),
        marks,
        engine: None,
    }
}

/// The ends of `line` within `reach` of an end of one of `others`, or
/// of its own other end, put there (Vector magnet; an open end drawn or
/// dragged onto the other closes the shape). The line follows the end it
/// moves over `reach` of its length, so it bends there instead of
/// jumping. `true` when an end moved.
pub fn snap_ends(line: &mut Line, others: &[Line], reach: f32) -> bool {
    let count = line.samples.len();
    if count < 2 || reach <= 0.0 {
        return false;
    }
    let arcs = arcs(&line.samples);
    let total = arcs.last().copied().unwrap_or(0.0);
    let mut moved = false;
    for (end, far) in [(0, count - 1), (count - 1, 0)] {
        let at = line.samples[end].at;
        let mut target: Option<([f32; 2], f32)> = None;
        let mut consider = |to: [f32; 2]| {
            let away = distance(at, to);
            if away > 0.0 && away <= reach && target.is_none_or(|(_, best)| away < best) {
                target = Some((to, away));
            }
        };
        // Its own other end, when the line is long enough to close, and
        // only from its last end (the first stays where it began).
        if end == count - 1 && total > reach * 3.0 {
            consider(line.samples[far].at);
        }
        for other in others {
            if let (Some(a), Some(b)) = (other.samples.first(), other.samples.last()) {
                consider(a.at);
                consider(b.at);
            }
        }
        let Some((to, _)) = target else {
            continue;
        };
        let by = [to[0] - at[0], to[1] - at[1]];
        // Over at most a third of the line, so a short line is not
        // dragged whole.
        let span = reach.min(total / 3.0).max(f32::EPSILON);
        for index in 0..count {
            let t = 1.0 - ((arcs[index] - arcs[end]).abs() / span).min(1.0);
            if t > 0.0 {
                let sample = &mut line.samples[index];
                sample.at[0] += by[0] * t;
                sample.at[1] += by[1] * t;
            }
        }
        moved = true;
    }
    moved
}

/// The lines of each vector layer the eraser, `radius` wide, touched
/// `at` (each in its own layer pixels): for every layer it touched, the
/// lines left. The Vector eraser over several picked layers.
#[must_use]
pub fn erase_layers(
    layers: &[(EntityId, Vec<Line>, [f32; 2])],
    radius: f32,
    mode: Erase,
) -> Vec<(EntityId, Vec<Line>)> {
    layers
        .iter()
        .filter_map(|(layer, lines, at)| Some((*layer, erase(lines, *at, radius, mode)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Dynamics, TaperMode};
    use sue_core::ReversibleTransaction as _;

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
                    tilt: 0.0,
                    lean: [0.0, 0.0],
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
            engine: None,
        }
    }

    #[test]
    fn vector_lines_export_as_outlines_as_wide_as_drawn() {
        let mut thick = line([10.0, 20.0], [90.0, 20.0]);
        thick.tip.size = 10.0;
        thick.colors[0] = [1.0, 0.0, 0.0];
        let mut eraser = line([0.0, 0.0], [5.0, 5.0]);
        eraser.tip.eraser = true;
        let shapes = svg_shapes(&[thick, eraser], [100.0, 0.0]);
        assert_eq!(shapes.len(), 1, "erasing lines leave nothing to draw");
        let shape = &shapes[0];
        assert!(shape.closed && shape.fill == Some([255, 0, 0]));
        let ys: Vec<f32> = shape.points.iter().map(|p| p[1]).collect();
        let (low, high) = (
            ys.iter().copied().fold(f32::MAX, f32::min),
            ys.iter().copied().fold(f32::MIN, f32::max),
        );
        assert!(
            (high - low - 10.0).abs() < 0.5,
            "as wide as the brush: {}",
            high - low
        );
        assert!(
            shape.points.iter().all(|p| p[0] >= 110.0 - 1e-3),
            "moved by the offset"
        );
        // And it reads back from the SVG it writes.
        let text = sue_io::svg::write_svg(&sue_io::svg::SvgDrawing {
            width: 200.0,
            height: 50.0,
            shapes: shapes.clone(),
        });
        assert_eq!(sue_io::svg::parse_svg(&text).unwrap().shapes.len(), 1);
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
        tip.size_by.tilt = true;
        tip.size_by.tilt_min = 0.35;
        tip.size_by.tilt_curve = sue_model::Response::new(&[[0.0, 0.1], [1.0, 1.0]]);
        assert_eq!(Tip::from_text(&tip.to_text()), tip);
        // A brush saved before tilt existed loads with tilt off.
        let old: String = tip
            .to_text()
            .split(';')
            .filter(|pair| !pair.contains(".tilt"))
            .collect::<Vec<_>>()
            .join(";");
        let loaded = Tip::from_text(&old);
        assert!(!loaded.size_by.tilt && !loaded.opacity_by.tilt);
        assert_eq!(loaded.size_by.tilt_min, 0.0);
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
            // The pen's lean is kept too, and the way it leans.
            size_by: Dynamics {
                tilt: true,
                tilt_min: 0.5,
                ..Dynamics::pressure(0.15)
            },
            follow_tilt: true,
            ..Tip::default()
        };
        let mut live = RasterTile2D::new(64, 64, PixelFormat::Rgba8).unwrap();
        let mut stroke = Stroke::new(EntityId::NIL, tip, [0.2, 0.1, 0.6], false);
        for step in 0..12 {
            stroke.set_speed(0.3);
            stroke.set_tilt(step as f32 / 12.0);
            stroke.set_lean([step as f32 / 24.0, -0.25]);
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
                tilt: 0.0,
                lean: [0.0, 0.0],
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

    /// The vector layer of the picture Sue 0.0.75 wrote for the golden
    /// tests (`crates/sue-paint/tests/golden/picture.sue`).
    fn golden_layer() -> (sue_core::Arena, EntityId) {
        let bytes = include_bytes!("../../sue-paint/tests/golden/picture.sue");
        let file = sue_core::SueFile::from_bytes(bytes).expect("a 0.0.75 file reads");
        let arena = file.arena(&sue_model::registry()).expect("its arena reads");
        let layer = arena
            .iter()
            .map(|(_, entity)| entity)
            .find(|entity| entity.class() == CLASS)
            .expect("the vector layer")
            .id();
        (arena, layer)
    }

    #[test]
    fn the_golden_vector_lines_of_0_0_75_paint_again_to_their_pixels() {
        // The decoration shapes are known from the moment the tools are.
        let _ = crate::tools::Tool::Decoration.sub_tools();
        let (arena, layer) = golden_layer();
        assert_eq!(layer.to_string(), "1de746c6-556a-45f2-9839-f99d81f7ba24");
        let lines = lines(&geometry(&arena, layer));
        let counts: Vec<usize> = lines.iter().map(|line| line.samples.len()).collect();
        assert_eq!(counts, [33, 33, 1, 13, 11]);
        assert!(
            lines.iter().all(|line| line.engine.is_none()),
            "no engine attribute: every line is the legacy engine's"
        );
        // Painted again by the frozen legacy engine, the same pixels.
        let stored = crate::doc::raster(&arena, layer).expect("its pixels");
        let again = render(&lines, stored.width(), stored.height()).expect("painted again");
        assert_eq!(again.bytes(), stored.bytes(), "the pixels Sue 0.0.75 saved");
        // And kept again, the geometry is the one that was read.
        assert_eq!(to_geometry(&lines), geometry(&arena, layer));
    }

    /// The whole arena as the bytes of a `.sue` file.
    fn saved(arena: &sue_core::Arena) -> Vec<u8> {
        let mut file = sue_core::SueFile::new();
        file.set_arena(arena);
        file.to_bytes()
    }

    /// The arena of `bytes`.
    fn opened(bytes: &[u8]) -> sue_core::Arena {
        sue_core::SueFile::from_bytes(bytes)
            .expect("the file reads")
            .arena(&sue_model::registry())
            .expect("its arena reads")
    }

    /// With `SUE_PICTURE_SHOTS` set to a folder, `raster` over paper as
    /// `name` there, four times as large, to be looked at.
    fn shot(name: &str, raster: &RasterTile2D) {
        let Some(dir) = std::env::var_os("SUE_PICTURE_SHOTS") else {
            return;
        };
        let (w, h) = (raster.width(), raster.height());
        let bytes = raster.bytes().expect("bytes");
        let mut out = Vec::with_capacity((w * h * 64) as usize);
        for y in 0..h * 4 {
            for x in 0..w * 4 {
                let i = (((y / 4) * w + x / 4) * 4) as usize;
                let a = f32::from(bytes[i + 3]) / 255.0;
                for (c, paper) in bytes[i..i + 3].iter().zip([250.0_f32, 248.0, 242.0]) {
                    out.push((f32::from(*c) * a + paper * (1.0 - a) + 0.5) as u8);
                }
                out.push(255);
            }
        }
        let png = sue_io::encode_png_rgba(w * 4, h * 4, &out).expect("a PNG");
        std::fs::write(std::path::Path::new(&dir).join(name), png).expect("written");
    }

    #[test]
    fn updating_the_golden_lines_to_the_new_engine_is_one_step_undo_takes_back() {
        let _ = crate::tools::Tool::Decoration.sub_tools();
        let (mut arena, layer) = golden_layer();
        shot(
            "golden-legacy.png",
            crate::doc::raster(&arena, layer).unwrap(),
        );
        let before = saved(&arena);
        let mut edit = update_lines(&mut arena, layer, "Update lines").expect("legacy lines");
        let after = saved(&arena);
        assert_ne!(after, before);
        let updated = lines(&geometry(&arena, layer));
        assert_eq!(updated.len(), 5);
        assert!(updated.iter().all(|line| {
            line.engine
                .as_ref()
                .is_some_and(|e| e.engine == replay::ENGINE && e.brush.is_some())
        }));
        // Every line gained its landing's dwell and its lift.
        let counts: Vec<usize> = updated.iter().map(|line| line.samples.len()).collect();
        assert_eq!(counts, [37, 37, 5, 17, 15]);
        // The decoration line keeps its stars by hash, on the layer.
        let star = updated[1].engine.as_ref().unwrap().brush.as_ref().unwrap();
        assert!(matches!(
            &star.preset.tip,
            TipSource::Images { pictures, .. } if matches!(pictures[..], [PictureRef::Hash(_)])
        ));
        assert_eq!(star.pictures.len(), 1);
        let pixels = crate::doc::raster(&arena, layer).unwrap().clone();
        shot("golden-new-engine.png", &pixels);
        assert!(
            pixels
                .bytes()
                .unwrap()
                .chunks(4)
                .filter(|p| p[3] > 0)
                .count()
                > 900,
            "the lines are painted"
        );
        // Nothing left to update.
        assert!(update_lines(&mut arena, layer, "Update lines").is_none());
        // Undo: the arena as it was, ids, geometry and pixels; redo: as
        // it became.
        edit.revert(&mut arena).expect("undone");
        assert_eq!(saved(&arena), before, "undo takes the whole step back");
        edit.apply(&mut arena).expect("redone");
        assert_eq!(saved(&arena), after, "redo makes it again");
    }

    #[test]
    fn a_line_of_the_new_engine_saved_and_opened_paints_the_same_bytes() {
        let _ = crate::tools::Tool::Decoration.sub_tools();
        let (mut arena, layer) = golden_layer();
        let _ = update_lines(&mut arena, layer, "Update lines").expect("legacy lines");
        let painted = crate::doc::raster(&arena, layer).unwrap().clone();
        let kept = lines(&geometry(&arena, layer));
        // Saved as a file and opened again.
        let opened = opened(&saved(&arena));
        let read = lines(&geometry(&opened, layer));
        assert_eq!(read, kept, "every line reads back as it was kept");
        // (Compared as text: a part the pen did not say is kept as not a
        // number, which is never equal to itself.)
        assert_eq!(
            format!("{:?}", to_geometry(&read)),
            format!("{:?}", geometry(&opened, layer))
        );
        let again = render(&read, painted.width(), painted.height()).expect("painted");
        assert_eq!(again.bytes(), painted.bytes(), "the same bytes");
        // What the engine keeps of a line is the stroke it replays.
        let stored = read[0].engine.as_ref().unwrap().stored(&read[0]);
        assert!(stored.is_finished());
        assert_eq!(
            sue_brush::replay::StoredStroke::from_bytes(&stored.to_bytes()),
            Ok(stored)
        );
    }

    #[test]
    fn a_layer_without_new_lines_keeps_the_attributes_0_0_75_wrote() {
        let kept = to_geometry(&[line([0.0, 0.0], [10.0, 0.0])]);
        for name in ["engine", "seed", "brush", "time", "twist", "brushes"] {
            for domain in [Domain::Point, Domain::Primitive, Domain::Detail] {
                assert!(kept.attribute(domain, name).is_none(), "{name}");
            }
        }
    }

    #[test]
    fn the_vector_eraser_cuts_a_line_of_the_new_engine_where_the_pen_was() {
        let legacy = line([0.0, 50.0], [100.0, 50.0]);
        let new = to_engine(&legacy).expect("a legacy line");
        assert!(to_engine(&new).is_none(), "already the engine's");
        let pieces = erase(
            std::slice::from_ref(&new),
            [50.0, 50.0],
            3.0,
            Erase::Touched,
        )
        .expect("touched");
        assert_eq!(pieces.len(), 2);
        let pen = &new.engine.as_ref().unwrap().pen;
        for piece in &pieces {
            let engine = piece.engine.as_ref().expect("still the engine's");
            assert_eq!(engine.pen.len(), piece.samples.len());
            // Each cut takes the time and pressure the pen had there.
            let times: Vec<f32> = engine.pen.iter().map(|p| p.time).collect();
            assert!(times.windows(2).all(|t| t[0] <= t[1]), "{times:?}");
            assert!(times[0] >= pen[0].time && *times.last().unwrap() <= pen.last().unwrap().time);
        }
        let first = &pieces[0].engine.as_ref().unwrap().pen;
        let cut = pieces[0].samples.last().unwrap();
        // The legacy line pressed 0.5 to 1 along x from 0 to 100.
        let expected = 0.5 + cut.at[0] / 100.0 * 0.5;
        assert!((first.last().unwrap().pressure.unwrap() - expected).abs() < 0.02);
        let mut raster = RasterTile2D::new(100, 100, PixelFormat::Rgba8).unwrap();
        for piece in &pieces {
            piece.paint(&mut raster);
        }
        let inked = |x0: u32, x1: u32| {
            (x0..x1)
                .map(|x| raster.bytes().unwrap()[((50 * 100 + x) * 4 + 3) as usize])
                .max()
                .unwrap()
        };
        assert!(inked(10, 40) > 0 && inked(60, 90) > 0, "both pieces paint");
        assert_eq!(inked(49, 52), 0, "the cut stays clear");
    }

    #[test]
    fn a_line_whose_samples_a_tool_changed_still_paints() {
        // Correct Line knows nothing of the engine: it changes samples,
        // and what the pen said is made again from them.
        let mut new = to_engine(&line([0.0, 20.0], [60.0, 20.0])).unwrap();
        new.samples.insert(3, new.samples[3]);
        let engine = new.engine.as_ref().unwrap();
        let pen = engine.pen_for(&new.samples);
        assert_eq!(pen.len(), new.samples.len());
        assert!(pen.windows(2).all(|p| p[0].time <= p[1].time));
        let mut raster = RasterTile2D::new(64, 40, PixelFormat::Rgba8).unwrap();
        new.paint(&mut raster);
        assert!(raster.bytes().unwrap().chunks(4).any(|p| p[3] > 0));
    }

    #[test]
    fn a_dot_updated_to_the_new_engine_stays_about_as_large() {
        let dot = Line {
            samples: vec![Sample {
                at: [20.0, 20.0],
                pressure: 0.8,
                speed: 0.0,
                tilt: 0.0,
                lean: [0.0, 0.0],
                width: 1.0,
                alpha: 1.0,
            }],
            ..line([0.0, 0.0], [1.0, 0.0])
        };
        let mut dot = dot;
        dot.tip.size = 10.0;
        let inked = |line: &Line| {
            let mut raster = RasterTile2D::new(40, 40, PixelFormat::Rgba8).unwrap();
            line.paint(&mut raster);
            raster
                .bytes()
                .unwrap()
                .chunks(4)
                .filter(|p| p[3] > 127)
                .count()
        };
        let (old, new) = (inked(&dot), inked(&to_engine(&dot).unwrap()));
        assert!(old > 20, "{old}");
        assert!(
            new * 10 >= old * 8 && new * 10 <= old * 12,
            "legacy {old} pixels, new {new}"
        );
    }

    // ----- Editing lines (P4-14) ---------------------------------------------------

    #[test]
    fn restyling_picked_lines_recolors_and_resizes_only_them() {
        let lines = vec![line([0.0, 0.0], [10.0, 0.0]), line([0.0, 5.0], [10.0, 5.0])];
        let restyle = Restyle {
            main: Some([0.08, 0.08, 0.08]),
            sub: Some([1.5, -1.0, 0.5]),
            size: Some(12.0),
            shape: None,
        };
        let out = restyled(&lines, &[1], &restyle);
        assert_eq!(out[0], lines[0], "a line not picked stays");
        assert_eq!(out[1].colors, [[0.08; 3], [1.0, 0.0, 0.5]]);
        assert!((out[1].tip.size - 12.0).abs() < f32::EPSILON);
        assert_eq!(out[1].samples, lines[1].samples, "the path stays");
        assert!(Restyle::default().is_empty() && !restyle.is_empty());
        assert_eq!(restyled(&lines, &[1], &Restyle::default()), lines);
    }

    #[test]
    fn a_brush_shape_keeps_each_lines_size_and_eraser() {
        let mut eraser = line([0.0, 0.0], [10.0, 0.0]);
        eraser.tip.eraser = true;
        let lines = vec![line([0.0, 0.0], [10.0, 0.0]), eraser];
        let shape = Tip {
            size: 40.0,
            hardness: 0.25,
            grain: 0.5,
            ..Tip::default()
        };
        let restyle = Restyle {
            shape: Some(shape),
            ..Restyle::default()
        };
        let out = restyled(&lines, &[0, 1], &restyle);
        for (was, now) in lines.iter().zip(&out) {
            assert!((now.tip.size - was.tip.size).abs() < f32::EPSILON);
            assert_eq!(now.tip.eraser, was.tip.eraser);
            assert!((now.tip.hardness - 0.25).abs() < f32::EPSILON);
            assert!((now.tip.grain - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn a_line_of_the_engine_resized_names_its_new_brush() {
        let engine = to_engine(&line([0.0, 0.0], [30.0, 0.0])).expect("an engine line");
        let old = engine.engine.clone().unwrap();
        let out = restyled(
            &[engine],
            &[0],
            &Restyle {
                size: Some(9.0),
                ..Restyle::default()
            },
        );
        let new = out[0].engine.as_ref().unwrap();
        let brush = new.brush.as_ref().unwrap();
        assert_ne!(new.brush_hash, old.brush_hash, "a new brush, a new hash");
        assert_eq!(new.brush_hash, brush.hash());
        assert_eq!(brush.preset.value(sue_model::brush::param::SIZE), Some(9.0));
        // Kept and read again, the line is the same.
        let read = lines(&to_geometry(&out));
        assert_eq!(read[0].engine.as_ref().unwrap().brush_hash, new.brush_hash);
    }

    #[test]
    fn the_box_maps_lines_and_thickens_them_when_asked() {
        let lines = vec![
            line([0.0, 0.0], [10.0, 10.0]),
            line([50.0, 50.0], [60.0, 50.0]),
        ];
        let from = bounds(&lines, &[0]).expect("a box");
        assert_eq!(from, [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(bounds(&lines, &[]), None);
        // The same corners leave every point where it was.
        let same = map_point([3.0, 7.0], from, &corners(from));
        assert!((same[0] - 3.0).abs() < 1e-4 && (same[1] - 7.0).abs() < 1e-4);
        // Twice as wide and twice as tall from the top left.
        let to = corners([0.0, 0.0, 20.0, 20.0]);
        let thin = transformed(&lines, &[0], from, &to, false);
        let last = thin[0].samples.last().unwrap().at;
        assert!((last[0] - 20.0).abs() < 1e-4 && (last[1] - 20.0).abs() < 1e-4);
        assert!((thin[0].tip.size - lines[0].tip.size).abs() < f32::EPSILON);
        assert_eq!(thin[1], lines[1], "a line not picked stays");
        let thick = transformed(&lines, &[0], from, &to, true);
        assert!((thick[0].tip.size - lines[0].tip.size * 2.0).abs() < 1e-4);
        // Free transform: one corner moved alone bends the box.
        let mut bent = corners(from);
        bent[2] = [20.0, 10.0];
        assert_eq!(map_point([10.0, 10.0], from, &bent), [20.0, 10.0]);
        assert_eq!(map_point([0.0, 10.0], from, &bent), [0.0, 10.0]);
        let middle = map_point([5.0, 5.0], from, &bent);
        assert!((middle[0] - 7.5).abs() < 1e-4 && (middle[1] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn a_loop_picks_the_lines_mostly_inside_it() {
        let lines = vec![
            line([10.0, 10.0], [20.0, 10.0]),
            // Two thirds inside: picked.
            line([20.0, 20.0], [35.0, 20.0]),
            // Only crossed: left.
            line([-20.0, 15.0], [60.0, 15.0]),
            line([100.0, 100.0], [110.0, 100.0]),
        ];
        let square = [[0.0, 0.0], [30.0, 0.0], [30.0, 30.0], [0.0, 30.0]];
        assert_eq!(in_loop(&lines, &square), [0, 1]);
        assert!(in_loop(&lines, &square[..2]).is_empty(), "no loop yet");
    }

    #[test]
    fn a_figure_line_keeps_its_corners_or_finds_them() {
        let tip = Tip {
            size: 12.0,
            grain: 0.8,
            ..Tip::default()
        };
        let square = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        let kept = figure_line(&square, tip, [[0.2; 3], [1.0; 3]], true);
        assert_eq!(kept.samples.len(), 5);
        assert!(kept.marks.iter().all(|mark| *mark == CONTROL | CORNER));
        assert!(kept.samples.iter().all(|sample| sample.pressure == 1.0));
        assert_eq!(kept.tip, tip, "the sub tool's brush shape");
        let round = figure_line(&square, tip, [[0.2; 3], [1.0; 3]], false);
        assert!(round.marks.is_empty());
        // It reads back as it was kept, and it paints.
        let read = lines(&to_geometry(std::slice::from_ref(&kept)));
        assert_eq!(read[0].samples, kept.samples);
        assert_eq!(read[0].marks, kept.marks);
        assert!((read[0].tip.size - 12.0).abs() < f32::EPSILON);
        assert!((read[0].tip.grain - 0.8).abs() < 1e-6);
        assert!(
            render(&[kept], 20, 20)
                .unwrap()
                .bytes()
                .unwrap()
                .chunks(4)
                .any(|p| p[3] > 0)
        );
    }

    #[test]
    fn an_open_end_near_the_other_closes_the_shape_and_the_magnet_joins_lines() {
        // Three sides of a square whose end stops 3 pixels short.
        let mut open = figure_line(
            &[
                [0.0, 0.0],
                [40.0, 0.0],
                [40.0, 40.0],
                [0.0, 40.0],
                [0.0, 3.0],
            ],
            Tip::default(),
            [[0.0; 3], [1.0; 3]],
            false,
        );
        let start = open.samples[0].at;
        assert!(snap_ends(&mut open, &[], 6.0));
        assert_eq!(open.samples.last().unwrap().at, start, "closed");
        assert_eq!(open.samples[0].at, start, "the first end stays");
        // Too far: nothing moves.
        let mut far = line([0.0, 0.0], [30.0, 0.0]);
        let was = far.clone();
        assert!(!snap_ends(
            &mut far,
            &[line([0.0, 20.0], [30.0, 20.0])],
            6.0
        ));
        assert_eq!(far, was);
        // The magnet: a new line's end goes onto another line's end.
        let other = line([50.0, 0.0], [80.0, 0.0]);
        let mut new = line([0.0, 2.0], [48.0, 2.0]);
        assert!(snap_ends(&mut new, std::slice::from_ref(&other), 6.0));
        assert_eq!(new.samples.last().unwrap().at, [50.0, 0.0]);
        assert_eq!(new.samples[0].at, [0.0, 2.0], "the far end stays");
    }

    #[test]
    fn the_vector_eraser_over_several_layers_erases_each_where_it_touched() {
        let (a, b) = (EntityId::from_u128(1), EntityId::from_u128(2));
        let layers = vec![
            (a, vec![line([0.0, 0.0], [40.0, 0.0])], [20.0, 0.0]),
            // The second layer sits 100 pixels to the right: in its own
            // pixels the eraser is far from its line.
            (b, vec![line([0.0, 0.0], [40.0, 0.0])], [-80.0, 0.0]),
        ];
        let out = erase_layers(&layers, 3.0, Erase::Whole);
        assert_eq!(out.len(), 1, "only the layer it touched");
        assert_eq!(out[0].0, a);
        assert!(out[0].1.is_empty());
    }

    #[test]
    fn updating_keeps_lines_that_would_lose_their_flick_or_correction() {
        let (mut arena, layer) = golden_layer();
        // One line flicked and one post corrected, as people draw them.
        let mut drawn = lines(&geometry(&arena, layer));
        drawn[0].finish.flick = Some([400.0, 0.0]);
        drawn[2].finish.correction = 3;
        if let Ok(entity) = arena.get_by_id_mut(layer) {
            entity.set_ext(Box::new(to_geometry(&drawn)));
        }
        let expected = Updated {
            updated: drawn.iter().filter(|line| updates_unchanged(line)).count(),
            kept: drawn.iter().filter(|line| !updates_unchanged(line)).count(),
        };
        assert!(expected.updated > 0 && expected.kept >= 2, "{expected:?}");
        assert_eq!(legacy_lines(&arena, layer), expected);
        let before = saved(&arena);
        let (mut edit, did) = update_lines_kept(&mut arena, layer, "Update lines").expect("lines");
        assert_eq!(did, expected);
        let after = saved(&arena);
        let now = lines(&geometry(&arena, layer));
        assert!(now[0].engine.is_none() && now[0].finish.flick.is_some());
        assert!(now[2].engine.is_none() && now[2].finish.correction == 3);
        for (was, now) in drawn.iter().zip(&now) {
            assert_eq!(now.engine.is_some(), updates_unchanged(was));
        }
        // Nothing left that it can update.
        assert!(update_lines_kept(&mut arena, layer, "Update lines").is_none());
        assert_eq!(legacy_lines(&arena, layer).updated, 0);
        edit.revert(&mut arena).expect("undone");
        assert_eq!(saved(&arena), before, "undo takes the whole step back");
        edit.apply(&mut arena).expect("redone");
        assert_eq!(saved(&arena), after, "redo makes it again");
    }

    #[test]
    fn restyling_a_layer_is_one_step_undo_takes_back() {
        let (mut arena, layer) = golden_layer();
        let before = saved(&arena);
        let restyle = Restyle {
            main: Some([0.9, 0.1, 0.1]),
            size: Some(6.0),
            ..Restyle::default()
        };
        let mut edit = relined(&mut arena, layer, "Line style", |lines| {
            Some(restyled(lines, &[0, 1], &restyle))
        })
        .expect("a vector layer");
        let after = saved(&arena);
        assert_ne!(after, before);
        let now = lines(&geometry(&arena, layer));
        assert_eq!(now[0].colors[0], [0.9, 0.1, 0.1]);
        assert!(relined(&mut arena, layer, "Nothing", |_| None).is_none());
        edit.revert(&mut arena).expect("undone");
        assert_eq!(saved(&arena), before);
        edit.apply(&mut arena).expect("redone");
        assert_eq!(saved(&arena), after);
    }
}
