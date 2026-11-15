//! The parts Sue stores in its own tables of a `.clip`: one encoding per
//! part name, each with a version, laid out as `docs/clip-sue-tables.md`
//! describes. Numbers are Sue's codec's (little-endian floats, LEB128
//! varints, length-prefixed text and bytes); every list is counted first.
//!
//! A part whose name or version this build does not know is never decoded
//! here: the reader keeps its row as it was and the writer puts it back.

use sue_core::codec::{CodecError, Reader, Writer};

use super::kinds::{
    ClipAlign, ClipAnimationFolder, ClipAudio, ClipBalloon, ClipBalloonShape, ClipCamera, ClipCel,
    ClipComp, ClipCompState, ClipContent, ClipCorrection, ClipEffects, ClipFill, ClipFrame,
    ClipGradient, ClipGradientRepeat, ClipGradientShape, ClipInterpolation, ClipKey, ClipLayerPath,
    ClipLine, ClipPanel, ClipPicture, ClipPoint, ClipRuler, ClipRulerKind, ClipStop, ClipTail,
    ClipText, ClipTextRun, ClipThreeD, ClipTimeline, ClipTone, ClipTrack, ClipVector, Rgba,
};

/// The version of every part this build writes.
pub const VERSION: i64 = 1;

/// The most pixels a side of a stored small picture may have.
const PICTURE_SIDE: u32 = 4096;

/// Part names of a layer's content.
pub const CONTENT_KINDS: [&str; 10] = [
    "fill",
    "gradient",
    "correction",
    "text",
    "vector",
    "frame",
    "balloon",
    "3d",
    "animation-folder",
    "camera",
];

type Result<T> = std::result::Result<T, CodecError>;

// ----- Small pieces ----------------------------------------------------------------

fn rgba(w: &mut Writer, color: Rgba) {
    w.raw(&color);
}

fn read_rgba(r: &mut Reader<'_>) -> Result<Rgba> {
    let raw = r.raw(4)?;
    Ok([raw[0], raw[1], raw[2], raw[3]])
}

fn point(w: &mut Writer, at: [f32; 2]) {
    w.f32(at[0]);
    w.f32(at[1]);
}

fn read_point(r: &mut Reader<'_>) -> Result<[f32; 2]> {
    Ok([r.f32()?, r.f32()?])
}

fn points(w: &mut Writer, list: &[[f32; 2]]) {
    w.len(list.len());
    for at in list {
        point(w, *at);
    }
}

fn read_points(r: &mut Reader<'_>) -> Result<Vec<[f32; 2]>> {
    let count = r.len(8)?;
    (0..count).map(|_| read_point(r)).collect()
}

fn stops(w: &mut Writer, list: &[ClipStop]) {
    w.len(list.len());
    for stop in list {
        w.f32(stop.at);
        rgba(w, stop.color);
    }
}

fn read_stops(r: &mut Reader<'_>) -> Result<Vec<ClipStop>> {
    let count = r.len(8)?;
    (0..count)
        .map(|_| {
            Ok(ClipStop {
                at: r.f32()?,
                color: read_rgba(r)?,
            })
        })
        .collect()
}

fn path(w: &mut Writer, layer: &ClipLayerPath) {
    w.len(layer.0.len());
    for index in &layer.0 {
        w.varint(u64::from(*index));
    }
}

fn read_u32(r: &mut Reader<'_>) -> Result<u32> {
    u32::try_from(r.varint()?).map_err(|_| r.error("a number is too large"))
}

fn read_path(r: &mut Reader<'_>) -> Result<ClipLayerPath> {
    let count = r.len(1)?;
    if count > 64 {
        return Err(r.error("a layer path is too deep"));
    }
    Ok(ClipLayerPath(
        (0..count).map(|_| read_u32(r)).collect::<Result<_>>()?,
    ))
}

/// One of `count` values, numbered from 0.
fn pick<T: Copy>(r: &Reader<'_>, value: u8, choices: &[T]) -> Result<T> {
    choices
        .get(usize::from(value))
        .copied()
        .ok_or_else(|| r.error(format!("{value} is not a known choice")))
}

fn text(w: &mut Writer, value: &ClipText) {
    w.len(value.runs.len());
    for run in &value.runs {
        w.str(&run.text);
        w.str(&run.font);
        w.f32(run.size);
        rgba(w, run.color);
        w.u8(u8::from(run.bold) | u8::from(run.italic) << 1);
    }
    point(w, value.at);
    match value.frame {
        Some(size) => {
            w.bool(true);
            point(w, size);
        }
        None => w.bool(false),
    }
    w.bool(value.vertical);
    w.u8(match value.align {
        ClipAlign::Start => 0,
        ClipAlign::Center => 1,
        ClipAlign::End => 2,
        ClipAlign::Justify => 3,
    });
    w.f32(value.line_spacing);
}

fn read_text(r: &mut Reader<'_>) -> Result<ClipText> {
    let count = r.len(11)?;
    let mut runs = Vec::with_capacity(count);
    for _ in 0..count {
        let text = r.str()?;
        let font = r.str()?;
        let size = r.f32()?;
        let color = read_rgba(r)?;
        let style = r.u8()?;
        if style > 3 {
            return Err(r.error("a text style is not known"));
        }
        runs.push(ClipTextRun {
            text,
            font,
            size,
            color,
            bold: style & 1 != 0,
            italic: style & 2 != 0,
        });
    }
    let at = read_point(r)?;
    let frame = if r.bool()? {
        Some(read_point(r)?)
    } else {
        None
    };
    let vertical = r.bool()?;
    let align = r.u8()?;
    let align = pick(
        r,
        align,
        &[
            ClipAlign::Start,
            ClipAlign::Center,
            ClipAlign::End,
            ClipAlign::Justify,
        ],
    )?;
    Ok(ClipText {
        runs,
        at,
        frame,
        vertical,
        align,
        line_spacing: r.f32()?,
    })
}

// ----- Layer content ---------------------------------------------------------------

/// The bytes of a layer's content.
#[must_use]
pub fn encode_content(content: &ClipContent) -> Vec<u8> {
    let mut w = Writer::new();
    match content {
        ClipContent::Fill(fill) => rgba(&mut w, fill.color),
        ClipContent::Gradient(gradient) => {
            w.u8(match gradient.shape {
                ClipGradientShape::Linear => 0,
                ClipGradientShape::Circle => 1,
                ClipGradientShape::Ellipse => 2,
            });
            point(&mut w, gradient.start);
            point(&mut w, gradient.end);
            w.f32(gradient.ratio);
            w.u8(match gradient.repeat {
                ClipGradientRepeat::Clamp => 0,
                ClipGradientRepeat::Repeat => 1,
                ClipGradientRepeat::Reflect => 2,
                ClipGradientRepeat::Empty => 3,
            });
            stops(&mut w, &gradient.stops);
        }
        ClipContent::Correction(correction) => {
            w.u8(correction.number());
            match correction {
                ClipCorrection::BrightnessContrast {
                    brightness,
                    contrast,
                } => {
                    w.f32(*brightness);
                    w.f32(*contrast);
                }
                ClipCorrection::HueSaturationLuminosity {
                    hue,
                    saturation,
                    luminosity,
                } => {
                    w.f32(*hue);
                    w.f32(*saturation);
                    w.f32(*luminosity);
                }
                ClipCorrection::Posterize { levels } => w.varint(u64::from(*levels)),
                ClipCorrection::Invert => {}
                ClipCorrection::Levels {
                    input,
                    gamma,
                    output,
                } => {
                    point(&mut w, *input);
                    w.f32(*gamma);
                    point(&mut w, *output);
                }
                ClipCorrection::ToneCurve { curves } => {
                    for curve in curves {
                        points(&mut w, curve);
                    }
                }
                ClipCorrection::ColorBalance {
                    shadows,
                    midtones,
                    highlights,
                    keep_luminosity,
                } => {
                    for range in [shadows, midtones, highlights] {
                        for value in range {
                            w.f32(*value);
                        }
                    }
                    w.bool(*keep_luminosity);
                }
                ClipCorrection::Threshold { level } => w.f32(*level),
                ClipCorrection::GradientMap { stops: list } => stops(&mut w, list),
            }
        }
        ClipContent::Text(value) => text(&mut w, value),
        ClipContent::Vector(vector) => {
            w.len(vector.lines.len());
            for line in &vector.lines {
                w.len(line.points.len());
                for p in &line.points {
                    point(&mut w, p.at);
                    w.f32(p.pressure);
                    w.f32(p.width);
                    w.f32(p.opacity);
                    w.bool(p.corner);
                }
                w.f32(line.width);
                rgba(&mut w, line.color);
                w.str(&line.brush);
                w.bool(line.closed);
            }
        }
        ClipContent::Frame(frame) => {
            w.len(frame.panels.len());
            for panel in &frame.panels {
                points(&mut w, &panel.points);
                w.f32(panel.border);
                rgba(&mut w, panel.color);
            }
        }
        ClipContent::Balloon(balloon) => {
            w.u8(match balloon.shape {
                ClipBalloonShape::Ellipse => 0,
                ClipBalloonShape::Rectangle => 1,
                ClipBalloonShape::Rounded => 2,
                ClipBalloonShape::Cloud => 3,
                ClipBalloonShape::Flash => 4,
            });
            for value in balloon.rect {
                w.f32(value);
            }
            w.len(balloon.tails.len());
            for tail in &balloon.tails {
                points(&mut w, &tail.points);
                w.f32(tail.width);
            }
            w.f32(balloon.line_width);
            rgba(&mut w, balloon.line_color);
            rgba(&mut w, balloon.fill);
            match &balloon.text {
                Some(value) => {
                    w.bool(true);
                    text(&mut w, value);
                }
                None => w.bool(false),
            }
        }
        ClipContent::ThreeD(three) => w.bytes(&three.scene),
        ClipContent::AnimationFolder(folder) => {
            w.len(folder.cels.len());
            for cel in &folder.cels {
                w.varint(u64::from(cel.frame));
                // 0 is a blank cel; a child is its index plus 1.
                w.varint(cel.child.map_or(0, |child| u64::from(child) + 1));
            }
        }
        ClipContent::Camera(camera) => {
            point(&mut w, camera.center);
            w.f32(camera.scale);
            w.f32(camera.angle);
        }
    }
    w.into_bytes()
}

/// A layer's content from the part `kind`'s bytes.
///
/// # Errors
/// When `kind` is not a content part or the bytes are not laid out as
/// this build writes them.
#[allow(clippy::too_many_lines)]
pub fn decode_content(kind: &str, bytes: &[u8]) -> Result<ClipContent> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let content = match kind {
        "fill" => ClipContent::Fill(ClipFill {
            color: read_rgba(r)?,
        }),
        "gradient" => {
            let shape = r.u8()?;
            let shape = pick(
                r,
                shape,
                &[
                    ClipGradientShape::Linear,
                    ClipGradientShape::Circle,
                    ClipGradientShape::Ellipse,
                ],
            )?;
            let start = read_point(r)?;
            let end = read_point(r)?;
            let ratio = r.f32()?;
            let repeat = r.u8()?;
            let repeat = pick(
                r,
                repeat,
                &[
                    ClipGradientRepeat::Clamp,
                    ClipGradientRepeat::Repeat,
                    ClipGradientRepeat::Reflect,
                    ClipGradientRepeat::Empty,
                ],
            )?;
            ClipContent::Gradient(ClipGradient {
                shape,
                start,
                end,
                ratio,
                repeat,
                stops: read_stops(r)?,
            })
        }
        "correction" => ClipContent::Correction(match r.u8()? {
            0 => ClipCorrection::BrightnessContrast {
                brightness: r.f32()?,
                contrast: r.f32()?,
            },
            1 => ClipCorrection::HueSaturationLuminosity {
                hue: r.f32()?,
                saturation: r.f32()?,
                luminosity: r.f32()?,
            },
            2 => ClipCorrection::Posterize {
                levels: read_u32(r)?,
            },
            3 => ClipCorrection::Invert,
            4 => ClipCorrection::Levels {
                input: read_point(r)?,
                gamma: r.f32()?,
                output: read_point(r)?,
            },
            5 => ClipCorrection::ToneCurve {
                curves: [
                    read_points(r)?,
                    read_points(r)?,
                    read_points(r)?,
                    read_points(r)?,
                ],
            },
            6 => {
                let mut ranges = [[0.0; 3]; 3];
                for range in &mut ranges {
                    for value in range.iter_mut() {
                        *value = r.f32()?;
                    }
                }
                ClipCorrection::ColorBalance {
                    shadows: ranges[0],
                    midtones: ranges[1],
                    highlights: ranges[2],
                    keep_luminosity: r.bool()?,
                }
            }
            7 => ClipCorrection::Threshold { level: r.f32()? },
            8 => ClipCorrection::GradientMap {
                stops: read_stops(r)?,
            },
            other => return Err(r.error(format!("correction {other} is not known"))),
        }),
        "text" => ClipContent::Text(read_text(r)?),
        "vector" => {
            let count = r.len(17)?;
            let mut lines = Vec::with_capacity(count);
            for _ in 0..count {
                let many = r.len(21)?;
                let mut list = Vec::with_capacity(many);
                for _ in 0..many {
                    list.push(ClipPoint {
                        at: read_point(r)?,
                        pressure: r.f32()?,
                        width: r.f32()?,
                        opacity: r.f32()?,
                        corner: r.bool()?,
                    });
                }
                lines.push(ClipLine {
                    points: list,
                    width: r.f32()?,
                    color: read_rgba(r)?,
                    brush: r.str()?,
                    closed: r.bool()?,
                });
            }
            ClipContent::Vector(ClipVector { lines })
        }
        "frame" => {
            let count = r.len(9)?;
            let mut panels = Vec::with_capacity(count);
            for _ in 0..count {
                panels.push(ClipPanel {
                    points: read_points(r)?,
                    border: r.f32()?,
                    color: read_rgba(r)?,
                });
            }
            ClipContent::Frame(ClipFrame { panels })
        }
        "balloon" => {
            let shape = r.u8()?;
            let shape = pick(
                r,
                shape,
                &[
                    ClipBalloonShape::Ellipse,
                    ClipBalloonShape::Rectangle,
                    ClipBalloonShape::Rounded,
                    ClipBalloonShape::Cloud,
                    ClipBalloonShape::Flash,
                ],
            )?;
            let rect = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
            let count = r.len(5)?;
            let mut tails = Vec::with_capacity(count);
            for _ in 0..count {
                tails.push(ClipTail {
                    points: read_points(r)?,
                    width: r.f32()?,
                });
            }
            ClipContent::Balloon(ClipBalloon {
                shape,
                rect,
                tails,
                line_width: r.f32()?,
                line_color: read_rgba(r)?,
                fill: read_rgba(r)?,
                text: if r.bool()? { Some(read_text(r)?) } else { None },
            })
        }
        "3d" => ClipContent::ThreeD(ClipThreeD {
            scene: r.bytes()?.to_vec(),
        }),
        "animation-folder" => {
            let count = r.len(2)?;
            let mut cels = Vec::with_capacity(count);
            for _ in 0..count {
                let frame = read_u32(r)?;
                let child = match r.varint()? {
                    0 => None,
                    n => Some(u32::try_from(n - 1).map_err(|_| r.error("a cel is too far"))?),
                };
                cels.push(ClipCel { frame, child });
            }
            ClipContent::AnimationFolder(ClipAnimationFolder { cels })
        }
        "camera" => ClipContent::Camera(ClipCamera {
            center: read_point(r)?,
            scale: r.f32()?,
            angle: r.f32()?,
        }),
        other => return Err(r.error(format!("{other} is not a layer content"))),
    };
    finished(r)?;
    Ok(content)
}

/// Fails when bytes are left over: a part laid out otherwise is kept as
/// it was rather than half read.
fn finished(r: &Reader<'_>) -> Result<()> {
    if r.is_empty() {
        Ok(())
    } else {
        Err(r.error("bytes are left after the part"))
    }
}

// ----- Other layer parts -----------------------------------------------------------

/// The bytes of a layer's effects.
#[must_use]
pub fn encode_effects(effects: &ClipEffects) -> Vec<u8> {
    let mut w = Writer::new();
    let flags = u8::from(effects.border.is_some())
        | u8::from(effects.tone.is_some()) << 1
        | u8::from(effects.extract_line.is_some()) << 2
        | u8::from(effects.sub_color.is_some()) << 3;
    w.u8(flags);
    if let Some((width, color)) = effects.border {
        w.f32(width);
        rgba(&mut w, color);
    }
    if let Some(tone) = effects.tone {
        w.f32(tone.frequency);
        w.f32(tone.angle);
        w.u8(tone.dot);
    }
    if let Some(threshold) = effects.extract_line {
        w.f32(threshold);
    }
    if let Some(color) = effects.sub_color {
        w.raw(&color);
    }
    w.into_bytes()
}

/// A layer's effects from their bytes.
///
/// # Errors
/// When the bytes are not laid out as this build writes them.
pub fn decode_effects(bytes: &[u8]) -> Result<ClipEffects> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let flags = r.u8()?;
    if flags > 15 {
        return Err(r.error("an effect is not known"));
    }
    let mut effects = ClipEffects::default();
    if flags & 1 != 0 {
        effects.border = Some((r.f32()?, read_rgba(r)?));
    }
    if flags & 2 != 0 {
        effects.tone = Some(ClipTone {
            frequency: r.f32()?,
            angle: r.f32()?,
            dot: r.u8()?,
        });
    }
    if flags & 4 != 0 {
        effects.extract_line = Some(r.f32()?);
    }
    if flags & 8 != 0 {
        let raw = r.raw(3)?;
        effects.sub_color = Some([raw[0], raw[1], raw[2]]);
    }
    finished(r)?;
    Ok(effects)
}

/// The bytes of a palette color.
#[must_use]
pub fn encode_color(color: [u8; 3]) -> Vec<u8> {
    color.to_vec()
}

/// A palette color from its bytes.
///
/// # Errors
/// When there are not exactly three bytes.
pub fn decode_color(bytes: &[u8]) -> Result<[u8; 3]> {
    <[u8; 3]>::try_from(bytes).map_err(|_| CodecError::new("a color is three bytes", 0))
}

/// The bytes of a layer's rulers.
#[must_use]
pub fn encode_rulers(rulers: &[ClipRuler]) -> Vec<u8> {
    let mut w = Writer::new();
    w.len(rulers.len());
    for ruler in rulers {
        w.u8(match ruler.kind {
            ClipRulerKind::Linear => 0,
            ClipRulerKind::Curve => 1,
            ClipRulerKind::Figure => 2,
            ClipRulerKind::Parallel => 3,
            ClipRulerKind::Concentric => 4,
            ClipRulerKind::Radial => 5,
            ClipRulerKind::Symmetry => 6,
            ClipRulerKind::Perspective => 7,
        });
        points(&mut w, &ruler.points);
        w.varint(u64::from(ruler.lines));
        w.u8(u8::from(ruler.snap) | u8::from(ruler.everywhere) << 1);
    }
    w.into_bytes()
}

/// A layer's rulers from their bytes.
///
/// # Errors
/// When the bytes are not laid out as this build writes them.
pub fn decode_rulers(bytes: &[u8]) -> Result<Vec<ClipRuler>> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let count = r.len(4)?;
    let mut rulers = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = r.u8()?;
        let kind = pick(
            r,
            kind,
            &[
                ClipRulerKind::Linear,
                ClipRulerKind::Curve,
                ClipRulerKind::Figure,
                ClipRulerKind::Parallel,
                ClipRulerKind::Concentric,
                ClipRulerKind::Radial,
                ClipRulerKind::Symmetry,
                ClipRulerKind::Perspective,
            ],
        )?;
        let list = read_points(r)?;
        let lines = read_u32(r)?;
        let flags = r.u8()?;
        if flags > 3 {
            return Err(r.error("a ruler setting is not known"));
        }
        rulers.push(ClipRuler {
            kind,
            points: list,
            lines,
            snap: flags & 1 != 0,
            everywhere: flags & 2 != 0,
        });
    }
    finished(r)?;
    Ok(rulers)
}

/// The bytes of a small picture: its size, then its pixels zlib
/// compressed.
#[must_use]
pub fn encode_picture(picture: &ClipPicture) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(u64::from(picture.width));
    w.varint(u64::from(picture.height));
    w.bytes(&miniz_oxide::deflate::compress_to_vec_zlib(
        &picture.rgba,
        6,
    ));
    w.into_bytes()
}

/// A small picture from its bytes.
///
/// # Errors
/// When the size is too large or the pixels do not fill it.
pub fn decode_picture(bytes: &[u8]) -> Result<ClipPicture> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let width = read_u32(r)?;
    let height = read_u32(r)?;
    if width > PICTURE_SIDE || height > PICTURE_SIDE {
        return Err(r.error("the picture is too large"));
    }
    let want = width as usize * height as usize * 4;
    let packed = r.bytes()?;
    let rgba = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(packed, want.max(1))
        .map_err(|_| r.error("the picture's pixels do not inflate"))?;
    if rgba.len() != want {
        return Err(r.error("the picture's pixels do not fill it"));
    }
    finished(r)?;
    Ok(ClipPicture {
        width,
        height,
        rgba,
    })
}

// ----- Document parts --------------------------------------------------------------

/// The bytes of the layer comps.
#[must_use]
pub fn encode_comps(comps: &[ClipComp]) -> Vec<u8> {
    let mut w = Writer::new();
    w.len(comps.len());
    for comp in comps {
        w.str(&comp.name);
        w.len(comp.states.len());
        for state in &comp.states {
            path(&mut w, &state.layer);
            w.bool(state.visible);
            w.f32(state.opacity);
            w.varint_signed(state.composite);
        }
    }
    w.into_bytes()
}

/// The layer comps from their bytes.
///
/// # Errors
/// When the bytes are not laid out as this build writes them.
pub fn decode_comps(bytes: &[u8]) -> Result<Vec<ClipComp>> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let count = r.len(2)?;
    let mut comps = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.str()?;
        let many = r.len(7)?;
        let mut states = Vec::with_capacity(many);
        for _ in 0..many {
            states.push(ClipCompState {
                layer: read_path(r)?,
                visible: r.bool()?,
                opacity: r.f32()?,
                composite: r.varint_signed()?,
            });
        }
        comps.push(ClipComp { name, states });
    }
    finished(r)?;
    Ok(comps)
}

/// The bytes of the timeline.
#[must_use]
pub fn encode_timeline(timeline: &ClipTimeline) -> Vec<u8> {
    let mut w = Writer::new();
    w.str(&timeline.name);
    w.f32(timeline.fps);
    w.varint(u64::from(timeline.frames));
    w.varint(u64::from(timeline.playback[0]));
    w.varint(u64::from(timeline.playback[1]));
    w.len(timeline.tracks.len());
    for track in &timeline.tracks {
        path(&mut w, &track.layer);
        w.str(&track.property);
        w.len(track.keys.len());
        for key in &track.keys {
            w.varint(u64::from(key.frame));
            w.f64(key.value);
            w.u8(match key.interpolation {
                ClipInterpolation::Hold => 0,
                ClipInterpolation::Linear => 1,
                ClipInterpolation::Smooth => 2,
            });
        }
    }
    w.len(timeline.audio.len());
    for audio in &timeline.audio {
        w.str(&audio.name);
        w.varint(u64::from(audio.start));
        w.f32(audio.volume);
        w.bool(audio.muted);
        w.bytes(&audio.file);
    }
    w.into_bytes()
}

/// The timeline from its bytes.
///
/// # Errors
/// When the bytes are not laid out as this build writes them.
pub fn decode_timeline(bytes: &[u8]) -> Result<ClipTimeline> {
    let mut r = Reader::new(bytes);
    let r = &mut r;
    let name = r.str()?;
    let fps = r.f32()?;
    let frames = read_u32(r)?;
    let playback = [read_u32(r)?, read_u32(r)?];
    let count = r.len(3)?;
    let mut tracks = Vec::with_capacity(count);
    for _ in 0..count {
        let layer = read_path(r)?;
        let property = r.str()?;
        let many = r.len(10)?;
        let mut keys = Vec::with_capacity(many);
        for _ in 0..many {
            let frame = read_u32(r)?;
            let value = r.f64()?;
            let interpolation = r.u8()?;
            let interpolation = pick(
                r,
                interpolation,
                &[
                    ClipInterpolation::Hold,
                    ClipInterpolation::Linear,
                    ClipInterpolation::Smooth,
                ],
            )?;
            keys.push(ClipKey {
                frame,
                value,
                interpolation,
            });
        }
        tracks.push(ClipTrack {
            layer,
            property,
            keys,
        });
    }
    let count = r.len(8)?;
    let mut audio = Vec::with_capacity(count);
    for _ in 0..count {
        audio.push(ClipAudio {
            name: r.str()?,
            start: read_u32(r)?,
            volume: r.f32()?,
            muted: r.bool()?,
            file: r.bytes()?.to_vec(),
        });
    }
    finished(r)?;
    Ok(ClipTimeline {
        name,
        fps,
        frames,
        playback,
        tracks,
        audio,
    })
}
