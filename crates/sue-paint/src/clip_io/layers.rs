//! Layers that are more than pixels, both ways between Sue's picture
//! model and a [`ClipDocument`] layer: fill, gradient and correction
//! layers, text, vector lines, layer effects and palette color.
//!
//! Clip Studio's own storage of these has not been observed yet, so the
//! `.clip` carries them in Sue's own tables (`docs/clip-sue-tables.md`):
//! Clip Studio shows a layer's pixels (what Sue draws for it, a fill and
//! a gradient as they look, a correction as nothing), and Sue reads the
//! layer back live. The one exception is the layer color effect, which
//! is Clip Studio's own `DrawColor` columns.
//!
//! Each mapping says whether it lost something on the way ("approximated"
//! in the conversion report); what Sue's or Clip Studio's model holds and
//! the other has no place for is named beside each.

use sue_core::{Arena, Color, ColorSpace, Entity, EntityId, RasterTile2D, Value};
use sue_io::clip::{
    ClipAlign, ClipContent, ClipCorrection, ClipDrawColor, ClipEffects, ClipFill, ClipGradient,
    ClipGradientRepeat, ClipGradientShape, ClipLayer, ClipLine, ClipPoint, ClipStop, ClipText,
    ClipTextRun, ClipTone, ClipVector,
};
use sue_model::Response;
use sue_model::picture;

use crate::brush::{Finish, SAME, Sample, Tip};
use crate::doc;
use crate::effects;
use crate::filters::Filter;
use crate::gradient::{Edge, Look, MIN_ASPECT, Shape};
use crate::gradient_map::{GradientMap, MAX_STOPS, Stop, StopColor};
use crate::lettering::Lettering;
use crate::vector::{self, CORNER, Line};
use sue_type::raster::Justify;

/// A value 0 to 1 as 0 to 255.
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A color as straight 8-bit sRGB with alpha.
fn rgba8(color: &Color) -> [u8; 4] {
    let srgb = color.to_space(ColorSpace::Srgb);
    [
        byte(srgb.rgb[0]),
        byte(srgb.rgb[1]),
        byte(srgb.rgb[2]),
        byte(srgb.alpha),
    ]
}

/// A straight 8-bit sRGB color with alpha as Sue's color.
fn color8([r, g, b, a]: [u8; 4]) -> Color {
    Color {
        alpha: f32::from(a) / 255.0,
        ..Color::srgb8(r, g, b)
    }
}

/// The color stored at `key` of `entity`.
fn color_at(entity: &Entity, key: &str) -> Option<Color> {
    match entity.property(key) {
        Some(Value::Color(color)) => Some(*color),
        _ => None,
    }
}

fn number(entity: &Entity, key: &str, or: f64) -> f64 {
    entity.property(key).and_then(Value::as_f64).unwrap_or(or)
}

fn on(entity: &Entity, key: &str) -> bool {
    entity.property(key).and_then(Value::as_bool) == Some(true)
}

/// Two floats as near as a stored `f32` holds them.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
}

// ----- What a layer is, out to the file ----------------------------------------------

/// The content Sue keeps for `entity` (a layer, not a folder) in the
/// file, and whether something of it had no place there. `offset` is
/// where the layer's own pixels sit on the canvas; `dpi` the picture's.
pub(super) fn content_out(
    entity: &Entity,
    offset: [i64; 2],
    dpi: f64,
) -> Option<(ClipContent, bool)> {
    let class = entity.class();
    if class == doc::FILL_LAYER {
        let color = color_at(entity, doc::FILL_COLOR).map_or([0, 0, 0, 255], |c| rgba8(&c));
        return Some((ClipContent::Fill(ClipFill { color }), false));
    }
    if class == doc::GRADIENT_LAYER {
        return Some(gradient_out(entity));
    }
    if class == doc::CORRECTION {
        let filter = entity
            .property(doc::CORRECTION_FILTER)
            .and_then(Value::as_str)
            .and_then(Filter::from_text)?;
        let (correction, lost) = correction_out(&filter)?;
        return Some((ClipContent::Correction(correction), lost));
    }
    let at = offset.map(|value| value as f32);
    if class == doc::TEXT {
        let lettering = Lettering::of(entity)?;
        return Some(text_out(&lettering, at, dpi));
    }
    if class == vector::CLASS {
        let geometry = entity
            .ext::<sue_model::Geometry>()
            .cloned()
            .unwrap_or_default();
        return Some(vector_out(&vector::lines(&geometry), at));
    }
    None
}

fn gradient_out(entity: &Entity) -> (ClipContent, bool) {
    let gradient = doc::Gradient::of(entity);
    let look = &gradient.look;
    let mut lost = !look.map.mix.is_linear();
    let stops = look
        .map
        .stops()
        .iter()
        .map(|stop| {
            lost |= stop.color != StopColor::Own;
            let [r, g, b] = stop.rgb;
            ClipStop {
                at: stop.at,
                color: [r, g, b, stop.alpha],
            }
        })
        .collect();
    let content = ClipContent::Gradient(ClipGradient {
        shape: match look.shape {
            Shape::Linear => ClipGradientShape::Linear,
            Shape::Circle => ClipGradientShape::Circle,
            Shape::Ellipse => ClipGradientShape::Ellipse,
        },
        start: gradient.line[0],
        end: gradient.line[1],
        ratio: look.aspect,
        repeat: match look.edge {
            Edge::Extend => ClipGradientRepeat::Clamp,
            Edge::Repeat => ClipGradientRepeat::Repeat,
            Edge::Mirror => ClipGradientRepeat::Reflect,
            Edge::Clip => ClipGradientRepeat::Empty,
        },
        stops,
    });
    (content, lost)
}

/// A response curve (0 to 1 each way) as points 0 to 255 each way.
fn curve_out(curve: &Response) -> Vec<[f32; 2]> {
    curve
        .points()
        .iter()
        .map(|[x, y]| [x * 255.0, y * 255.0])
        .collect()
}

/// `filter` as Clip Studio's correction, and whether something was lost;
/// `None` for a filter that is no correction Clip Studio has.
fn correction_out(filter: &Filter) -> Option<(ClipCorrection, bool)> {
    let levels = |[black, white, gamma, out_black, out_white]: [f32; 5]| ClipCorrection::Levels {
        input: [black * 255.0, white * 255.0],
        gamma,
        output: [out_black * 255.0, out_white * 255.0],
    };
    Some(match filter {
        Filter::BrightnessContrast {
            brightness,
            contrast,
        } => (
            ClipCorrection::BrightnessContrast {
                brightness: brightness * 100.0,
                contrast: contrast * 100.0,
            },
            false,
        ),
        Filter::HueSaturation {
            hue,
            saturation,
            luminosity,
        } => (
            ClipCorrection::HueSaturationLuminosity {
                hue: *hue,
                saturation: saturation * 100.0,
                luminosity: luminosity * 100.0,
            },
            false,
        ),
        Filter::Posterize { levels } => (ClipCorrection::Posterize { levels: *levels }, false),
        Filter::Invert => (ClipCorrection::Invert, false),
        Filter::Levels {
            black,
            white,
            gamma,
            out_black,
            out_white,
        } => (
            levels([*black, *white, *gamma, *out_black, *out_white]),
            false,
        ),
        // One set of levels for all channels; the channels' own sets have
        // no place in the file.
        Filter::ChannelLevels { sets, .. } => (
            levels(sets[0]),
            sets[1..]
                .iter()
                .any(|set| *set != crate::filters::LEVELS_SAME),
        ),
        Filter::ToneCurve {
            rgb,
            red,
            green,
            blue,
            ..
        } => (
            ClipCorrection::ToneCurve {
                curves: [
                    curve_out(rgb),
                    curve_out(red),
                    curve_out(green),
                    curve_out(blue),
                ],
            },
            false,
        ),
        Filter::ColorBalance { shifts, .. } => (
            ClipCorrection::ColorBalance {
                shadows: shifts[0].map(|value| value * 100.0),
                midtones: shifts[1].map(|value| value * 100.0),
                highlights: shifts[2].map(|value| value * 100.0),
                keep_luminosity: false,
            },
            false,
        ),
        Filter::Binarize { threshold } => (
            ClipCorrection::Threshold {
                level: threshold * 255.0,
            },
            false,
        ),
        Filter::GradientMap { from, to } => {
            let stop = |at: f32, [r, g, b]: [u8; 3]| ClipStop {
                at,
                color: [r, g, b, 255],
            };
            (
                ClipCorrection::GradientMap {
                    stops: vec![stop(0.0, *from), stop(1.0, *to)],
                },
                false,
            )
        }
        Filter::Gradient { map } => {
            let mut lost = !map.mix.is_linear();
            let stops = map
                .stops()
                .iter()
                .map(|stop| {
                    lost |= stop.color != StopColor::Own;
                    let [r, g, b] = stop.rgb;
                    ClipStop {
                        at: stop.at,
                        color: [r, g, b, stop.alpha],
                    }
                })
                .collect();
            (ClipCorrection::GradientMap { stops }, lost)
        }
        _ => return None,
    })
}

/// Sue's text size (pixels) as points on a picture of `dpi`.
fn points(pixels: f32, dpi: f64) -> f32 {
    (f64::from(pixels) * 72.0 / dpi.max(1.0)) as f32
}

fn text_out(lettering: &Lettering, at: [f32; 2], dpi: f64) -> (ClipContent, bool) {
    let [r, g, b] = lettering.rgb;
    // Letter spacing, words along a curve, readings over words and words
    // set around a middle point have no place in the file.
    let lost = lettering.tracking != 0.0
        || !lettering.path.is_empty()
        || !lettering.ruby.is_empty()
        || lettering.center;
    let text = ClipText {
        runs: vec![ClipTextRun {
            text: lettering.content.clone(),
            font: lettering.font.clone(),
            size: points(lettering.size, dpi),
            color: [r, g, b, 255],
            bold: false,
            italic: false,
        }],
        at: [lettering.at[0] + at[0], lettering.at[1] + at[1]],
        frame: None,
        vertical: lettering.vertical,
        align: match lettering.justify {
            Justify::Start => ClipAlign::Start,
            Justify::Middle => ClipAlign::Center,
            Justify::End => ClipAlign::End,
        },
        line_spacing: if lettering.leading > 0.0 {
            lettering.leading
        } else {
            1.0
        },
    };
    (ClipContent::Text(text), lost)
}

fn vector_out(lines: &[Line], at: [f32; 2]) -> (ClipContent, bool) {
    let mut lost = false;
    let lines = lines
        .iter()
        .map(|line| {
            // The brush engine's record of the pen (times, tilt, its own
            // brush) and the copies a symmetry ruler made have no place:
            // the line comes back from its points.
            lost |= line.engine.is_some()
                || line.copies.iter().any(|copy| *copy != SAME)
                || line.finish != Finish::default()
                || line.samples.iter().any(|sample| {
                    sample.speed != 0.0 || sample.tilt != 0.0 || sample.lean != [0.0, 0.0]
                })
                || line.colors[1] != line.colors[0];
            let [r, g, b] = crate::color::bytes(line.colors[0]);
            ClipLine {
                points: line
                    .samples
                    .iter()
                    .enumerate()
                    .map(|(index, sample)| ClipPoint {
                        at: [sample.at[0] + at[0], sample.at[1] + at[1]],
                        pressure: sample.pressure,
                        width: sample.width,
                        opacity: sample.alpha,
                        corner: line.marks.get(index).is_some_and(|mark| mark & CORNER != 0),
                    })
                    .collect(),
                width: line.tip.size,
                color: [r, g, b, byte(line.tip.opacity)],
                brush: line.tip.to_text(),
                closed: false,
            }
        })
        .collect();
    (ClipContent::Vector(ClipVector { lines }), lost)
}

/// The effects of `entity` that Sue keeps in the file, beyond its layer
/// color, and whether some of its effects had no place there.
pub(super) fn effects_out(entity: &Entity) -> (Option<ClipEffects>, bool) {
    let mut out = ClipEffects::default();
    let mut lost = false;
    if on(entity, effects::BORDER) {
        let color = color_at(entity, effects::BORDER_COLOR)
            .unwrap_or_else(|| effects::default_color(effects::BORDER_COLOR));
        out.border = Some((
            number(entity, effects::BORDER_WIDTH, 4.0).max(0.0) as f32,
            rgba8(&color),
        ));
        lost |= entity.property(effects::BORDER_AA).and_then(Value::as_bool) == Some(false);
    }
    if on(entity, effects::TONE) {
        let shape = entity
            .property(effects::TONE_SHAPE)
            .and_then(Value::as_i64)
            .unwrap_or(0);
        out.tone = Some(ClipTone {
            frequency: number(entity, effects::TONE_LINES, 60.0) as f32,
            angle: number(entity, effects::TONE_ANGLE, 45.0) as f32,
            // Sue's shapes: circle, square, diamond, line, cross, noise.
            dot: match shape {
                1 => 1,
                2 => 2,
                3 => 3,
                5 => 4,
                _ => 0,
            },
        });
        lost |= shape == 4
            || [
                effects::TONE_SOURCE,
                effects::TONE_LEVELS,
                effects::TONE_REFLECT_OPACITY,
                effects::TONE_DOT_X,
                effects::TONE_DOT_Y,
            ]
            .iter()
            .any(|key| {
                entity.property(key).is_some_and(|value| {
                    value.as_f64().is_some_and(|v| v != 0.0) || value.as_bool() == Some(true)
                })
            });
    }
    if on(entity, effects::EXTRACT) {
        out.extract_line =
            Some((number(entity, effects::EXTRACT_THRESHOLD, 0.25).clamp(0.0, 1.0) * 255.0) as f32);
        lost |= number(entity, effects::EXTRACT_WIDTH, 1.0) != 1.0;
    }
    if on(entity, effects::COLOR)
        && let Some(sub) = color_at(entity, effects::COLOR_SUB)
    {
        let [r, g, b, _] = rgba8(&sub);
        out.sub_color = Some([r, g, b]);
    }
    // Watercolor edge and expression color have no place in the file.
    lost |= on(entity, effects::WATERCOLOR)
        || entity
            .property(effects::EXPRESSION)
            .and_then(Value::as_i64)
            .is_some_and(|expression| expression != 0);
    let any = out != ClipEffects::default();
    (any.then_some(out), lost)
}

/// The layer color effect of `entity` as Clip Studio's own columns.
pub(super) fn draw_color_out(entity: &Entity) -> Option<ClipDrawColor> {
    if !on(entity, effects::COLOR) {
        return None;
    }
    let color = color_at(entity, effects::COLOR_VALUE)
        .unwrap_or_else(|| effects::default_color(effects::COLOR_VALUE));
    let [r, g, b, _] = rgba8(&color);
    Some(ClipDrawColor::rgb([r, g, b]))
}

/// The palette color of `entity`.
pub(super) fn palette_out(entity: &Entity) -> Option<[u8; 3]> {
    let [r, g, b, _] = rgba8(&color_at(entity, doc::PALETTE_COLOR)?);
    Some([r, g, b])
}

/// What `layer` (a layer that is not plain pixels: a fill, a gradient, a
/// material...) shows by itself, as pixels where they sit: what Clip
/// Studio is given to show for it. `None` when it shows nothing (a
/// correction) or cannot be drawn.
pub(super) fn shown_pixels(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Option<(RasterTile2D, [i64; 2])> {
    let mut alone = arena.clone();
    // Rasterize refuses a locked layer; its look does not depend on it.
    let mut unlock = sue_core::txn::SetProperty::new(layer, picture::LOCKED, false);
    sue_core::ReversibleTransaction::apply(&mut unlock, &mut alone).ok()?;
    let (mut edit, id) = doc::rasterize_any(&alone, picture, layer).ok()?;
    sue_core::ReversibleTransaction::apply(&mut edit, &mut alone).ok()?;
    let entity = alone.get_by_id(id).ok()?;
    let at = [
        entity
            .property(picture::X)
            .and_then(Value::as_i64)
            .unwrap_or(0),
        entity
            .property(picture::Y)
            .and_then(Value::as_i64)
            .unwrap_or(0),
    ];
    Some((doc::raster(&alone, id)?.clone(), at))
}

// ----- What a layer is, in from the file -----------------------------------------------

/// A layer Sue makes from content that has no pixels of its own (a fill,
/// a gradient, a correction), named `name`, and whether something was
/// lost. `None` for content that rides on pixels.
pub(super) fn generated_in(name: &str, content: &ClipContent) -> Option<(Entity, bool)> {
    match content {
        ClipContent::Fill(fill) => Some((
            Entity::empty(name, doc::FILL_LAYER).with_property(doc::FILL_COLOR, color8(fill.color)),
            false,
        )),
        ClipContent::Gradient(gradient) => {
            let (gradient, lost) = gradient_in(gradient);
            let mut entity = Entity::empty(name, doc::GRADIENT_LAYER);
            for (key, value) in gradient.properties() {
                entity = entity.with_property(key, value);
            }
            Some((entity, lost))
        }
        ClipContent::Correction(correction) => {
            let (filter, lost) = correction_in(correction);
            let text = filter.to_text()?;
            Some((
                Entity::empty(name, doc::CORRECTION).with_property(doc::CORRECTION_FILTER, text),
                lost,
            ))
        }
        _ => None,
    }
}

/// Stops of the file as Sue's, and whether some did not fit.
fn stops_in(stops: &[ClipStop]) -> (Option<GradientMap>, bool) {
    let stops: Vec<Stop> = stops
        .iter()
        .map(|stop| {
            let [r, g, b, a] = stop.color;
            Stop {
                alpha: a,
                ..Stop::new(stop.at, [r, g, b])
            }
        })
        .collect();
    (GradientMap::of(&stops), stops.len() > MAX_STOPS)
}

fn gradient_in(gradient: &ClipGradient) -> (doc::Gradient, bool) {
    let (map, mut lost) = stops_in(&gradient.stops);
    lost |= map.is_none();
    let aspect = gradient.ratio.clamp(MIN_ASPECT, 1.0);
    lost |= gradient.shape == ClipGradientShape::Ellipse && !near(aspect, gradient.ratio);
    (
        doc::Gradient {
            line: [gradient.start, gradient.end],
            look: Look {
                map: map.unwrap_or_default(),
                shape: match gradient.shape {
                    ClipGradientShape::Linear => Shape::Linear,
                    ClipGradientShape::Circle => Shape::Circle,
                    ClipGradientShape::Ellipse => Shape::Ellipse,
                },
                edge: match gradient.repeat {
                    ClipGradientRepeat::Clamp => Edge::Extend,
                    ClipGradientRepeat::Repeat => Edge::Repeat,
                    ClipGradientRepeat::Reflect => Edge::Mirror,
                    ClipGradientRepeat::Empty => Edge::Clip,
                },
                aspect,
            },
        },
        lost,
    )
}

/// Points 0 to 255 each way as a response curve, and whether some did
/// not fit.
fn curve_in(points: &[[f32; 2]]) -> (Response, bool) {
    let scaled: Vec<[f32; 2]> = points
        .iter()
        .map(|[x, y]| [(x / 255.0).clamp(0.0, 1.0), (y / 255.0).clamp(0.0, 1.0)])
        .collect();
    if scaled.len() < 2 {
        return (Response::LINEAR, !scaled.is_empty());
    }
    let curve = Response::new(&scaled);
    let lost = curve.points().len() != scaled.len();
    (curve, lost)
}

fn correction_in(correction: &ClipCorrection) -> (Filter, bool) {
    match correction {
        ClipCorrection::BrightnessContrast {
            brightness,
            contrast,
        } => (
            Filter::BrightnessContrast {
                brightness: brightness / 100.0,
                contrast: contrast / 100.0,
            },
            false,
        ),
        ClipCorrection::HueSaturationLuminosity {
            hue,
            saturation,
            luminosity,
        } => (
            Filter::HueSaturation {
                hue: *hue,
                saturation: saturation / 100.0,
                luminosity: luminosity / 100.0,
            },
            false,
        ),
        ClipCorrection::Posterize { levels } => (Filter::Posterize { levels: *levels }, false),
        ClipCorrection::Invert => (Filter::Invert, false),
        ClipCorrection::Levels {
            input,
            gamma,
            output,
        } => (
            Filter::Levels {
                black: input[0] / 255.0,
                white: input[1] / 255.0,
                gamma: *gamma,
                out_black: output[0] / 255.0,
                out_white: output[1] / 255.0,
            },
            false,
        ),
        ClipCorrection::ToneCurve { curves } => {
            let [(rgb, a), (red, b), (green, c), (blue, d)] =
                [0, 1, 2, 3].map(|index| curve_in(&curves[index]));
            (
                Filter::ToneCurve {
                    rgb,
                    red,
                    green,
                    blue,
                    channel: 0,
                },
                a || b || c || d,
            )
        }
        ClipCorrection::ColorBalance {
            shadows,
            midtones,
            highlights,
            keep_luminosity,
        } => (
            Filter::ColorBalance {
                shifts: [shadows, midtones, highlights].map(|shifts| shifts.map(|v| v / 100.0)),
                range: 0,
            },
            *keep_luminosity,
        ),
        ClipCorrection::Threshold { level } => (
            Filter::Binarize {
                threshold: level / 255.0,
            },
            false,
        ),
        ClipCorrection::GradientMap { stops } => {
            let (map, lost) = stops_in(stops);
            match map {
                Some(map) => (Filter::Gradient { map }, lost),
                None => (
                    Filter::GradientMap {
                        from: [0; 3],
                        to: [255; 3],
                    },
                    true,
                ),
            }
        }
    }
}

/// Sue's text size (pixels) from points on a picture of `dpi`.
fn pixels(points: f32, dpi: f64) -> f32 {
    (f64::from(points) * dpi.max(1.0) / 72.0) as f32
}

/// `entity` (a raster layer whose pixels sit at `offset` on a picture of
/// `dpi`) made the layer `content` says it is, when that rides on pixels
/// (text, vector lines), and whether something was lost; any other
/// content leaves it as it is.
pub(super) fn on_pixels_in(
    entity: Entity,
    content: &ClipContent,
    offset: [i64; 2],
    dpi: f64,
) -> (Entity, bool) {
    let at = offset.map(|value| value as f32);
    match content {
        ClipContent::Text(text) => text_in(entity, text, at, dpi),
        ClipContent::Vector(lines) => vector_in(entity, lines, at),
        _ => (entity, false),
    }
}

fn text_in(entity: Entity, text: &ClipText, at: [f32; 2], dpi: f64) -> (Entity, bool) {
    let first = text.runs.first();
    let [r, g, b, _] = first.map_or([0, 0, 0, 255], |run| run.color);
    let mut lettering = Lettering::new([text.at[0] - at[0], text.at[1] - at[1]], [r, g, b]);
    lettering.content = text.plain();
    if let Some(run) = first {
        lettering.font = run.font.clone();
        lettering.size = pixels(run.size, dpi);
    }
    lettering.vertical = text.vertical;
    lettering.justify = match text.align {
        ClipAlign::Center => Justify::Middle,
        ClipAlign::End => Justify::End,
        ClipAlign::Start | ClipAlign::Justify => Justify::Start,
    };
    lettering.leading = if near(text.line_spacing, 1.0) {
        0.0
    } else {
        text.line_spacing
    };
    // One style for all the words: runs of other fonts, sizes or colors,
    // bold and italic, a wrapping box and justified lines are lost.
    let lost = text.runs.iter().any(|run| {
        run.bold
            || run.italic
            || first.is_some_and(|first| {
                run.font != first.font || !near(run.size, first.size) || run.color != first.color
            })
            || run.color[3] != 255
    }) || text.frame.is_some()
        || text.align == ClipAlign::Justify;
    // The words' properties as Sue writes them (the area they cover
    // included); the pixels stay the file's.
    let (width, height) = entity
        .component(sue_core::ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
        .map_or((1, 1), |raster| (raster.width(), raster.height()));
    let mut entity = entity.with_class(doc::TEXT);
    if let Some(made) = crate::lettering::text_layer(&lettering, width, height) {
        for (key, value) in made.properties().iter() {
            entity.properties_mut().set(key, value.clone());
        }
    }
    (entity, lost)
}

fn vector_in(entity: Entity, vector: &ClipVector, at: [f32; 2]) -> (Entity, bool) {
    let mut lost = false;
    let lines: Vec<Line> = vector
        .lines
        .iter()
        .filter(|line| !line.points.is_empty())
        .map(|line| {
            lost |= line.closed;
            let mut tip = Tip::from_text(&line.brush);
            tip.size = line.width;
            tip.opacity = f32::from(line.color[3]) / 255.0;
            let rgb = [line.color[0], line.color[1], line.color[2]].map(|v| f32::from(v) / 255.0);
            let corners = line.points.iter().any(|point| point.corner);
            Line {
                samples: line
                    .points
                    .iter()
                    .map(|point| Sample {
                        at: [point.at[0] - at[0], point.at[1] - at[1]],
                        pressure: point.pressure,
                        speed: 0.0,
                        tilt: 0.0,
                        lean: [0.0, 0.0],
                        width: point.width,
                        alpha: point.opacity,
                    })
                    .collect(),
                tip,
                colors: [rgb, rgb],
                finish: Finish::default(),
                copies: Vec::new(),
                marks: if corners {
                    line.points
                        .iter()
                        .map(|point| {
                            if point.corner {
                                vector::CONTROL | CORNER
                            } else {
                                0
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                engine: None,
            }
        })
        .collect();
    let entity = entity
        .with_class(vector::CLASS)
        .with_ext(vector::to_geometry(&lines));
    (entity, lost)
}

/// `entity` with the effects, layer color and palette color of `layer`,
/// and whether something was lost.
pub(super) fn styles_in(mut entity: Entity, layer: &ClipLayer) -> (Entity, bool) {
    let mut lost = false;
    if !layer.is_paper() && layer.draw_color.enabled() {
        let [r, g, b] = layer.draw_color.color();
        entity = entity
            .with_property(effects::COLOR, true)
            .with_property(effects::COLOR_VALUE, Color::srgb8(r, g, b));
    }
    if let Some(found) = &layer.effects {
        if let Some((width, color)) = found.border {
            entity = entity
                .with_property(effects::BORDER, true)
                .with_property(effects::BORDER_WIDTH, f64::from(width))
                .with_property(effects::BORDER_COLOR, color8(color));
            lost |= color[3] != 255;
        }
        if let Some(tone) = found.tone {
            let shape: i64 = match tone.dot {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 5,
                _ => 0,
            };
            entity = entity
                .with_property(effects::TONE, true)
                .with_property(effects::TONE_LINES, f64::from(tone.frequency))
                .with_property(effects::TONE_ANGLE, f64::from(tone.angle))
                .with_property(effects::TONE_SHAPE, shape);
        }
        if let Some(threshold) = found.extract_line {
            entity = entity.with_property(effects::EXTRACT, true).with_property(
                effects::EXTRACT_THRESHOLD,
                f64::from((threshold / 255.0).clamp(0.01, 1.0)),
            );
        }
        if let Some([r, g, b]) = found.sub_color {
            if layer.draw_color.enabled() {
                entity = entity.with_property(effects::COLOR_SUB, Color::srgb8(r, g, b));
            } else {
                lost = true;
            }
        }
    }
    if let Some([r, g, b]) = layer.palette_color {
        entity = entity.with_property(doc::PALETTE_COLOR, Color::srgb8(r, g, b));
    }
    (entity, lost)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every correction Clip Studio and Sue share comes back as it went.
    #[test]
    fn corrections_go_out_and_come_back() {
        let map = GradientMap::of(&[
            Stop::new(0.0, [10, 20, 30]),
            Stop::new(0.4, [200, 30, 90]),
            Stop::new(1.0, [250, 250, 240]),
        ])
        .unwrap();
        let filters = [
            Filter::BrightnessContrast {
                brightness: 0.25,
                contrast: -0.5,
            },
            Filter::HueSaturation {
                hue: 40.0,
                saturation: -0.3,
                luminosity: 0.1,
            },
            Filter::Posterize { levels: 5 },
            Filter::Invert,
            Filter::Levels {
                black: 0.1,
                white: 0.9,
                gamma: 1.4,
                out_black: 0.05,
                out_white: 0.95,
            },
            Filter::ToneCurve {
                rgb: Response::new(&[[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]),
                red: Response::LINEAR,
                green: Response::new(&[[0.0, 0.2], [1.0, 0.8]]),
                blue: Response::LINEAR,
                channel: 0,
            },
            Filter::ColorBalance {
                shifts: [[0.1, 0.0, -0.2], [0.0, 0.3, 0.0], [-0.5, 0.0, 0.25]],
                range: 0,
            },
            Filter::Binarize { threshold: 0.5 },
            Filter::Gradient { map },
        ];
        for filter in filters {
            let (clip, lost) = correction_out(&filter).expect("a correction");
            assert!(!lost, "{filter:?}");
            let (back, lost) = correction_in(&clip);
            assert!(!lost, "{filter:?}");
            assert_eq!(
                back.to_text(),
                filter.to_text(),
                "{filter:?} came back as {back:?}"
            );
        }
        assert!(correction_out(&Filter::GaussianBlur { radius: 2.0 }).is_none());
    }
}
