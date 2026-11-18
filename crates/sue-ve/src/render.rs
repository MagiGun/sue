//! What a sequence shows at a moment, and drawing it.
//!
//! [`layers_at`] turns a sequence and a time into layers, bottom first:
//! which source, which source time, and how it is placed (fit to the
//! frame, then scaled, moved and turned), how opaque and how tinted, with
//! the dissolves and dips of the transitions worked in. The program
//! monitor draws those layers on the graphics card through Sue's painter
//! (each source frame a texture, each layer a textured quad), and the
//! export draws the very same layers on the processor with
//! [`composite`], so what plays is what is written.
//!
//! The sources that are not movies are drawn here: stills, Sue Paint
//! pictures and animations (with `sue-picture`'s compositing), titles
//! (with `sue-type`'s text drawing) and solids.

use std::sync::Arc;

use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D, Time};
use sue_model::{Clip, Sequence, Transition};

use crate::media::frames::VideoFrame;
use crate::model::{self, SourceKind, key};

/// One layer of a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// The track it is on.
    pub track: usize,
    /// The clip.
    pub clip: u64,
    /// The source entity.
    pub source: EntityId,
    /// What the source is.
    pub kind: SourceKind,
    /// The source time that shows.
    pub source_time: Time,
    /// 0 to 1.
    pub opacity: f32,
    /// 1 is fit to the frame.
    pub scale: f32,
    /// Moved from the middle, in sequence pixels.
    pub offset: [f32; 2],
    /// Turned clockwise, in degrees.
    pub rotation: f32,
    /// Washed toward a color by an amount (a dip).
    pub tint: Option<([u8; 3], f32)>,
}

fn layer_of(track: usize, clip: &Clip, kind: SourceKind, time: Time) -> Layer {
    let get = |name: &str| clip.value_at(name, time, model::default_of(name)) as f32;
    let source_time = if clip
        .properties
        .get(key::FREEZE)
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
    {
        clip.source_in
    } else {
        clip.source_time(time)
    };
    Layer {
        track,
        clip: clip.id,
        source: clip.source,
        kind,
        source_time,
        opacity: get(key::OPACITY).clamp(0.0, 1.0),
        scale: get(key::SCALE).max(0.0),
        offset: [get(key::POS_X), get(key::POS_Y)],
        rotation: get(key::ROTATION),
        tint: None,
    }
}

/// The color a dip goes through.
fn dip_color(transition: &Transition) -> [u8; 3] {
    if transition.kind == "dip-white" {
        [255, 255, 255]
    } else {
        [0, 0, 0]
    }
}

/// The layers sequence `sequence` shows at `time`, bottom first. `kind`
/// says what each source is; sources that show nothing (sound, an
/// adjustment clip in stage 1) are left out.
#[must_use]
pub fn layers_at(
    sequence: &Sequence,
    time: Time,
    kind: &dyn Fn(EntityId) -> SourceKind,
) -> Vec<Layer> {
    let mut out = Vec::new();
    for index in (0..sequence.tracks.len()).rev() {
        let track = &sequence.tracks[index];
        if !model::is_video(track) || track.muted {
            continue;
        }
        let shows = |clip: &Clip| {
            let kind = kind(clip.source);
            (clip.enabled
                && kind.has_pictures()
                && kind != SourceKind::Adjustment
                && kind != SourceKind::Sequence)
                .then_some(kind)
        };
        let transition = track
            .transitions
            .iter()
            .find(|transition| transition.start <= time && time < transition.end());
        if let Some(transition) = transition {
            let progress = transition.progress(time) as f32;
            let from = track
                .clip(transition.from)
                .and_then(|clip| shows(clip).map(|kind| (clip, kind)));
            let to = track
                .clip(transition.to)
                .and_then(|clip| shows(clip).map(|kind| (clip, kind)));
            let dip = transition.kind.starts_with("dip");
            match (from, to) {
                (Some((a, ka)), Some((b, kb))) if dip => {
                    let (clip, kind, amount) = if progress < 0.5 {
                        (a, ka, progress * 2.0)
                    } else {
                        (b, kb, (1.0 - progress) * 2.0)
                    };
                    let mut layer = layer_of(index, clip, kind, time);
                    layer.tint = Some((dip_color(transition), amount));
                    out.push(layer);
                }
                (Some((a, ka)), Some((b, kb))) => {
                    out.push(layer_of(index, a, ka, time));
                    let mut incoming = layer_of(index, b, kb, time);
                    incoming.opacity *= progress;
                    out.push(incoming);
                }
                (None, Some((b, kb))) => {
                    let mut layer = layer_of(index, b, kb, time);
                    if dip {
                        layer.tint = Some((dip_color(transition), 1.0 - progress));
                    } else {
                        layer.opacity *= progress;
                    }
                    out.push(layer);
                }
                (Some((a, ka)), None) => {
                    let mut layer = layer_of(index, a, ka, time);
                    if dip {
                        layer.tint = Some((dip_color(transition), progress));
                    } else {
                        layer.opacity *= 1.0 - progress;
                    }
                    out.push(layer);
                }
                (None, None) => {}
            }
            continue;
        }
        if let Some(clip) = track.at(time).next()
            && let Some(kind) = shows(clip)
        {
            out.push(layer_of(index, clip, kind, time));
        }
    }
    out.retain(|layer| layer.opacity > 0.0);
    out
}

/// The four corners of a layer's picture (top left, top right, bottom
/// right, bottom left) in sequence pixels, for a picture of `natural`
/// size in a frame of `frame` size.
#[must_use]
pub fn corners(layer: &Layer, natural: (u32, u32), frame: (u32, u32)) -> [[f32; 2]; 4] {
    let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let (fw, fh) = (frame.0 as f32, frame.1 as f32);
    let fit = (fw / nw).min(fh / nh) * layer.scale;
    let (hw, hh) = (nw * fit / 2.0, nh * fit / 2.0);
    let center = [fw / 2.0 + layer.offset[0], fh / 2.0 + layer.offset[1]];
    let (sin, cos) = layer.rotation.to_radians().sin_cos();
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
        .map(|[x, y]| [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos])
}

/// The size a source is laid out at: its own, or the frame's for what is
/// drawn at the frame's size (titles, solids).
#[must_use]
pub fn natural_size(kind: &SourceKind, image: (u32, u32), frame: (u32, u32)) -> (u32, u32) {
    match kind {
        SourceKind::Title | SourceKind::Solid => frame,
        _ => image,
    }
}

/// A source picture placed in a frame: the layer and its pixels.
#[derive(Clone, Debug)]
pub struct Placed {
    /// How it is placed.
    pub layer: Layer,
    /// Its pixels.
    pub image: Arc<VideoFrame>,
}

fn sample(image: &VideoFrame, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (image.width as i64, image.height as i64);
    let x = u - 0.5;
    let y = v - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let at = |px: i64, py: i64| -> [f32; 4] {
        let (px, py) = (px.clamp(0, w - 1), py.clamp(0, h - 1));
        let index = ((py * w + px) * 4) as usize;
        image.rgba.get(index..index + 4).map_or([0.0; 4], |p| {
            [
                f32::from(p[0]),
                f32::from(p[1]),
                f32::from(p[2]),
                f32::from(p[3]),
            ]
        })
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    );
    let mut out = [0.0; 4];
    for channel in 0..4 {
        let top = a[channel] + (b[channel] - a[channel]) * tx;
        let bottom = c[channel] + (d[channel] - c[channel]) * tx;
        out[channel] = top + (bottom - top) * ty;
    }
    out
}

/// Draws `layers` (bottom first) into an opaque `frame` sized RGBA
/// picture over black, on as many threads as the machine has.
#[must_use]
pub fn composite(frame: (u32, u32), layers: &[Placed]) -> Vec<u8> {
    let (width, height) = (frame.0 as usize, frame.1 as usize);
    let mut out = vec![0_u8; width * height * 4];
    for pixel in out.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    // Each layer's inverse placement: from a frame pixel to its picture.
    struct Map<'a> {
        image: &'a VideoFrame,
        center: [f32; 2],
        cos: f32,
        sin: f32,
        sx: f32,
        sy: f32,
        opacity: f32,
        tint: Option<([f32; 3], f32)>,
        bounds: [f32; 4],
        identity: bool,
    }
    let maps: Vec<Map<'_>> = layers
        .iter()
        .filter(|placed| placed.image.width > 0 && placed.image.height > 0)
        .map(|placed| {
            let layer = &placed.layer;
            let image = &*placed.image;
            let natural = natural_size(&layer.kind, (image.width, image.height), frame);
            let quad = corners(layer, natural, frame);
            let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
            let fit = (frame.0 as f32 / nw).min(frame.1 as f32 / nh) * layer.scale.max(1e-6);
            let (sin, cos) = layer.rotation.to_radians().sin_cos();
            let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for [x, y] in quad {
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
            let sx = image.width as f32 / (nw * fit);
            let sy = image.height as f32 / (nh * fit);
            let identity = layer.rotation == 0.0
                && (sx - 1.0).abs() < 1e-4
                && (sy - 1.0).abs() < 1e-4
                && (quad[0][0] - quad[0][0].round()).abs() < 1e-3
                && (quad[0][1] - quad[0][1].round()).abs() < 1e-3;
            Map {
                image,
                center: [
                    frame.0 as f32 / 2.0 + layer.offset[0],
                    frame.1 as f32 / 2.0 + layer.offset[1],
                ],
                cos,
                sin,
                sx,
                sy,
                opacity: layer.opacity,
                tint: layer
                    .tint
                    .map(|(rgb, amount)| (rgb.map(f32::from), amount.clamp(0.0, 1.0))),
                bounds,
                identity,
            }
        })
        .collect();
    let threads = std::thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .min(16);
    let rows_each = height.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (part, rows) in out.chunks_mut(rows_each * width * 4).enumerate() {
            let maps = &maps;
            scope.spawn(move || {
                let first = part * rows_each;
                for (row, line) in rows.chunks_exact_mut(width * 4).enumerate() {
                    let y = (first + row) as f32 + 0.5;
                    for map in maps {
                        if y < map.bounds[1] || y > map.bounds[3] {
                            continue;
                        }
                        let x0 = map.bounds[0].floor().max(0.0) as usize;
                        let x1 = (map.bounds[2].ceil().max(0.0) as usize).min(width);
                        for x in x0..x1 {
                            let px = x as f32 + 0.5;
                            let (dx, dy) = (px - map.center[0], y - map.center[1]);
                            // Turn back, then into the picture's pixels.
                            let (qx, qy) =
                                (dx * map.cos + dy * map.sin, -dx * map.sin + dy * map.cos);
                            let u = qx * map.sx + map.image.width as f32 / 2.0;
                            let v = qy * map.sy + map.image.height as f32 / 2.0;
                            if u < 0.0
                                || v < 0.0
                                || u > map.image.width as f32
                                || v > map.image.height as f32
                            {
                                continue;
                            }
                            let mut src = if map.identity {
                                let (ix, iy) = (u as usize, v as usize);
                                let index = (iy * map.image.width as usize + ix) * 4;
                                map.image.rgba.get(index..index + 4).map_or([0.0; 4], |p| {
                                    [
                                        f32::from(p[0]),
                                        f32::from(p[1]),
                                        f32::from(p[2]),
                                        f32::from(p[3]),
                                    ]
                                })
                            } else {
                                sample(map.image, u, v)
                            };
                            if let Some((rgb, amount)) = map.tint {
                                for channel in 0..3 {
                                    src[channel] += (rgb[channel] - src[channel]) * amount;
                                }
                            }
                            let alpha = src[3] / 255.0 * map.opacity;
                            if alpha <= 0.0 {
                                continue;
                            }
                            let dst = &mut line[x * 4..x * 4 + 3];
                            for channel in 0..3 {
                                let value =
                                    src[channel] * alpha + f32::from(dst[channel]) * (1.0 - alpha);
                                dst[channel] = value.round().clamp(0.0, 255.0) as u8;
                            }
                        }
                    }
                }
            });
        }
    });
    out
}

/// A raster's pixels as RGBA bytes.
#[must_use]
pub fn rgba_of(tile: &RasterTile2D) -> Option<Vec<u8>> {
    match tile.format() {
        PixelFormat::Rgba8 => tile.bytes().map(<[u8]>::to_vec),
        PixelFormat::Gray8 => tile
            .bytes()
            .map(|gray| gray.iter().flat_map(|v| [*v, *v, *v, 255]).collect()),
        PixelFormat::RgbaF32 => tile.f32_samples().map(|samples| {
            samples
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect()
        }),
    }
}

/// Draws the title `id` at the frame size `frame` (its text in its font,
/// size and color, centred).
#[must_use]
pub fn title(arena: &Arena, id: EntityId, frame: (u32, u32)) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let text = entity
        .string_buffer()
        .map(|buffer| buffer.as_str().to_owned())
        .unwrap_or_default();
    let (width, height) = frame;
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let color = crate::media::import::color_of(entity, [255, 255, 255, 255]);
    let size = model::number(entity, key::SIZE).unwrap_or(96.0) as f32;
    let spec = sue_type::raster::TextSpec {
        text: &text,
        font: model::text(entity, key::FONT).unwrap_or(""),
        size,
        rgb: [color[0], color[1], color[2]],
        at: [width as f32 / 2.0, height as f32 / 2.0],
        center: true,
        vertical: false,
        tracking: 0.0,
        leading: 0.0,
        justify: sue_type::raster::Justify::default(),
        path: &[],
        ruby: &[],
        style: sue_type::raster::TextStyle::default(),
    };
    sue_type::raster::draw_text(&mut pixels, width, height, &spec);
    if color[3] < 255 {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = ((u16::from(pixel[3]) * u16::from(color[3])) / 255) as u8;
        }
    }
    Some(VideoFrame::new(width, height, pixels, 0))
}

/// A solid's color as a small picture (it is laid out at the frame's
/// size).
#[must_use]
pub fn solid(arena: &Arena, id: EntityId) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let color = crate::media::import::color_of(entity, [0, 0, 0, 255]);
    Some(VideoFrame::new(8, 8, color.repeat(64), 0))
}

/// A still picture's pixels.
#[must_use]
pub fn still(arena: &Arena, id: EntityId) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let tile = entity
        .component(sue_core::ComponentKind::RasterTile2D)?
        .as_raster()?;
    Some(VideoFrame::new(
        tile.width(),
        tile.height(),
        rgba_of(tile)?,
        0,
    ))
}

/// A Sue Paint picture (or the frame of its animation that shows at
/// `source_time`), composited as Sue Paint shows it, at most `max`
/// pixels on its longer side.
#[must_use]
pub fn picture(arena: &Arena, id: EntityId, source_time: Time, max: u32) -> Option<VideoFrame> {
    let options = sue_picture::composite::RenderOptions::default();
    let (tile, index) = match sue_picture::anim::timeline(arena, id) {
        Some(sequence) if sue_picture::anim::length(arena, id) > 0 => {
            let length = sue_picture::anim::length(arena, id);
            let frame = source_time
                .frame(sequence.rate)
                .clamp(0, i64::from(length) - 1) as u32;
            let view = sue_picture::anim::view(sequence, frame, None);
            (
                sue_picture::composite::flatten_view(arena, id, options, Some(&view))?,
                i64::from(frame),
            )
        }
        _ => (sue_picture::composite::flatten(arena, id, options)?, 0),
    };
    let rgba = rgba_of(&tile)?;
    let (width, height) = (tile.width(), tile.height());
    if width.max(height) <= max {
        return Some(VideoFrame::new(width, height, rgba, index));
    }
    // Shrink by whole steps, averaging, to at most `max`.
    let step = width.max(height).div_ceil(max.max(1)).max(1);
    let (w, h) = (width.div_ceil(step), height.div_ceil(step));
    let mut small = vec![0_u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0_u32; 4];
            let mut count = 0;
            for sy in y * step..((y + 1) * step).min(height) {
                for sx in x * step..((x + 1) * step).min(width) {
                    let at = ((sy * width + sx) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += u32::from(rgba[at + channel]);
                    }
                    count += 1;
                }
            }
            let at = ((y * w + x) * 4) as usize;
            for channel in 0..4 {
                small[at + channel] = (sum[channel] / count.max(1)) as u8;
            }
        }
    }
    Some(VideoFrame::new(w, h, small, index))
}

/// The picture of a source that is not a movie: a still, a picture or
/// animation, a title or a solid. `None` for movies (they decode) and
/// what shows nothing.
#[must_use]
pub fn generated(
    arena: &Arena,
    id: EntityId,
    kind: &SourceKind,
    source_time: Time,
    frame: (u32, u32),
) -> Option<VideoFrame> {
    match kind {
        SourceKind::Still => still(arena, id),
        SourceKind::Picture => picture(arena, id, source_time, frame.0.max(frame.1).max(256)),
        SourceKind::Title => title(arena, id, frame),
        SourceKind::Solid => solid(arena, id),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
