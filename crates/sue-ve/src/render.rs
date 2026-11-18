//! What a sequence shows at a moment, and drawing it.
//!
//! [`layers_at`] turns a sequence and a time into layers, bottom first:
//! which source, which source time (time remapping included), and how it
//! is placed (fit to the frame, then scaled, moved and turned about its
//! anchor point), how opaque and how tinted, with the transitions worked
//! in: dissolves and dips, slides, pushes and zooms (placement only), and
//! wipes and blurs (which the processor draws). Adjustment clips are
//! layers too; what they do is [`crate::compose`]'s.
//!
//! The program monitor draws plain layers on the graphics card through
//! Sue's painter (each source frame a texture, each layer a textured
//! quad). Whatever the quads cannot show (effects, masks, grades, blend
//! modes, motion blur, wipes, adjustment clips, nested sequences) is drawn
//! on the processor by [`crate::compose`], the same code the export runs,
//! so what plays is what is written; [`composite`] is its drawing core
//! for placed pictures.
//!
//! The sources that are not movies are drawn here: stills, Sue Paint
//! pictures and animations (with `sue-picture`'s compositing), titles
//! (with `sue-type`'s text drawing) and solids.

use std::sync::Arc;

use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D, Time};
use sue_model::{Clip, Sequence, Transition};

use crate::fx::key as fx_key;
use crate::media::frames::VideoFrame;
use crate::model::{self, SourceKind, key};

/// A wipe across a layer during a transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wipe {
    /// The way the edge moves, degrees (0: left to right).
    pub angle: f32,
    /// How far, 0 to 1.
    pub progress: f32,
    /// The soft edge, a share of the frame.
    pub feather: f32,
}

impl Wipe {
    /// How much of the layer shows at frame pixel `x`, `y` of a frame of
    /// `size`.
    #[must_use]
    pub fn cover(&self, x: f32, y: f32, size: (f32, f32)) -> f32 {
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let extent = (cos.abs() * size.0 + sin.abs() * size.1).max(1.0);
        let along = ((x - size.0 / 2.0) * cos + (y - size.1 / 2.0) * sin) / extent + 0.5;
        let feather = self.feather.max(1e-3);
        ((self.progress * (1.0 + feather) - along) / feather).clamp(0.0, 1.0)
    }
}

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
    /// The point it turns and scales about, in source pixels from its
    /// middle.
    pub anchor: [f32; 2],
    /// Washed toward a color by an amount (a dip).
    pub tint: Option<([u8; 3], f32)>,
    /// A wipe across it (a transition).
    pub wipe: Option<Wipe>,
    /// Blurred by a transition, sequence pixels.
    pub blur: f32,
}

impl Layer {
    /// A layer of `source` at `source_time`, placed plainly.
    #[must_use]
    pub fn new(source: EntityId, kind: SourceKind, source_time: Time) -> Self {
        Self {
            track: 0,
            clip: 0,
            source,
            kind,
            source_time,
            opacity: 1.0,
            scale: 1.0,
            offset: [0.0, 0.0],
            rotation: 0.0,
            anchor: [0.0, 0.0],
            tint: None,
            wipe: None,
            blur: 0.0,
        }
    }

    /// `true` when the graphics card's quads show it as it is.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.wipe.is_none()
            && self.blur <= 0.0
            && !matches!(self.kind, SourceKind::Adjustment | SourceKind::Sequence)
    }
}

/// The source time `clip` shows at sequence time `time`: held on a freeze
/// frame, remapped when it has time remapping keys.
#[must_use]
pub fn source_time_of(clip: &Clip, time: Time) -> Time {
    if clip
        .properties
        .get(key::FREEZE)
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
    {
        return clip.source_in;
    }
    let plain = clip.source_time(time);
    match clip
        .animation
        .channel(fx_key::REMAP)
        .and_then(|curve| curve.sample(plain))
    {
        Some(seconds) => Time::from_seconds(seconds.max(0.0)),
        None => plain,
    }
}

fn layer_of(track: usize, clip: &Clip, kind: SourceKind, time: Time) -> Layer {
    let get = |name: &str| clip.value_at(name, time, model::default_of(name)) as f32;
    let mut layer = Layer::new(clip.source, kind, source_time_of(clip, time));
    layer.track = track;
    layer.clip = clip.id;
    layer.opacity = get(key::OPACITY).clamp(0.0, 1.0);
    layer.scale = get(key::SCALE).max(0.0);
    layer.offset = [get(key::POS_X), get(key::POS_Y)];
    layer.rotation = get(key::ROTATION);
    layer.anchor = [
        clip.value_at(fx_key::ANCHOR_X, time, 0.0) as f32,
        clip.value_at(fx_key::ANCHOR_Y, time, 0.0) as f32,
    ];
    layer
}

/// The placement of `clip` at sequence time `time` (motion blur samples
/// it between frames).
#[must_use]
pub fn placed_at(track: usize, clip: &Clip, kind: SourceKind, time: Time) -> Layer {
    layer_of(track, clip, kind, time)
}

/// The color a dip goes through.
fn dip_color(transition: &Transition) -> [u8; 3] {
    if transition.kind == "dip-white" {
        [255, 255, 255]
    } else {
        [0, 0, 0]
    }
}

/// The transition kinds Sue VE knows, as the effects browser lists them.
pub const TRANSITIONS: [&str; 8] = [
    "dissolve",
    "dip-black",
    "dip-white",
    "wipe",
    "slide",
    "push",
    "zoom",
    "blur",
];

/// A transition's direction (degrees: 0 moves left to right).
#[must_use]
pub fn transition_angle(transition: &Transition) -> f32 {
    transition
        .properties
        .get("angle")
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(0.0) as f32
}

/// The layers sequence `sequence` shows at `time`, bottom first. `kind`
/// says what each source is; sources that show nothing (sound) are left
/// out. Adjustment clips and nested sequences are layers too.
#[must_use]
pub fn layers_at(
    sequence: &Sequence,
    time: Time,
    kind: &dyn Fn(EntityId) -> SourceKind,
) -> Vec<Layer> {
    let (frame_w, frame_h) = model::frame_size(sequence);
    let (frame_w, frame_h) = (frame_w as f32, frame_h as f32);
    let mut out = Vec::new();
    for index in (0..sequence.tracks.len()).rev() {
        let track = &sequence.tracks[index];
        if !model::is_video(track) || track.muted {
            continue;
        }
        let shows = |clip: &Clip| {
            let kind = kind(clip.source);
            (clip.enabled && kind.has_pictures()).then_some(kind)
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
            let angle = transition_angle(transition);
            let (sin, cos) = angle.to_radians().sin_cos();
            let extent = cos.abs() * frame_w + sin.abs() * frame_h;
            let travel = [cos * extent, sin * extent];
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
                    let mut outgoing = layer_of(index, a, ka, time);
                    let mut incoming = layer_of(index, b, kb, time);
                    match transition.kind.as_str() {
                        "wipe" => {
                            incoming.wipe = Some(Wipe {
                                angle,
                                progress,
                                feather: 0.02,
                            });
                        }
                        "slide" => {
                            incoming.offset[0] -= travel[0] * (1.0 - progress);
                            incoming.offset[1] -= travel[1] * (1.0 - progress);
                        }
                        "push" => {
                            incoming.offset[0] -= travel[0] * (1.0 - progress);
                            incoming.offset[1] -= travel[1] * (1.0 - progress);
                            outgoing.offset[0] += travel[0] * progress;
                            outgoing.offset[1] += travel[1] * progress;
                        }
                        "zoom" => {
                            outgoing.scale *= 1.0 + progress;
                            outgoing.opacity *= 1.0 - progress;
                            incoming.scale *= 0.5 + progress / 2.0;
                            incoming.opacity *= progress;
                        }
                        "blur" => {
                            let amount = (progress * std::f32::consts::PI).sin() * 40.0;
                            outgoing.blur = amount;
                            incoming.blur = amount;
                            incoming.opacity *= progress;
                        }
                        _ => incoming.opacity *= progress,
                    }
                    out.push(outgoing);
                    out.push(incoming);
                }
                (None, Some((b, kb))) => {
                    let mut layer = layer_of(index, b, kb, time);
                    if dip {
                        layer.tint = Some((dip_color(transition), 1.0 - progress));
                    } else if transition.kind == "wipe" {
                        layer.wipe = Some(Wipe {
                            angle,
                            progress,
                            feather: 0.02,
                        });
                    } else {
                        layer.opacity *= progress;
                    }
                    out.push(layer);
                }
                (Some((a, ka)), None) => {
                    let mut layer = layer_of(index, a, ka, time);
                    if dip {
                        layer.tint = Some((dip_color(transition), progress));
                    } else if transition.kind == "wipe" {
                        layer.wipe = Some(Wipe {
                            angle: angle + 180.0,
                            progress: 1.0 - progress,
                            feather: 0.02,
                        });
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
/// size in a frame of `frame` size: fit, scaled and turned about its
/// anchor point, which lands at the middle plus its offset.
#[must_use]
pub fn corners(layer: &Layer, natural: (u32, u32), frame: (u32, u32)) -> [[f32; 2]; 4] {
    let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let (fw, fh) = (frame.0 as f32, frame.1 as f32);
    let fit = (fw / nw).min(fh / nh) * layer.scale;
    let (hw, hh) = (nw * fit / 2.0, nh * fit / 2.0);
    let (ax, ay) = (layer.anchor[0] * fit, layer.anchor[1] * fit);
    let center = [fw / 2.0 + layer.offset[0], fh / 2.0 + layer.offset[1]];
    let (sin, cos) = layer.rotation.to_radians().sin_cos();
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map(|[x, y]| {
        let (x, y) = (x - ax, y - ay);
        [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos]
    })
}

/// The size a source is laid out at: its own, or the frame's for what is
/// drawn at the frame's size (titles, solids, adjustment clips, nested
/// sequences).
#[must_use]
pub fn natural_size(kind: &SourceKind, image: (u32, u32), frame: (u32, u32)) -> (u32, u32) {
    match kind {
        SourceKind::Title
        | SourceKind::Solid
        | SourceKind::Shape
        | SourceKind::Adjustment
        | SourceKind::Sequence => frame,
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

/// Draws `layers` (bottom first) into an opaque `frame` sized RGBA
/// picture over black, on as many threads as the machine has.
#[must_use]
pub fn composite(frame: (u32, u32), layers: &[Placed]) -> Vec<u8> {
    let mut out = vec![0_u8; frame.0 as usize * frame.1 as usize * 4];
    for pixel in out.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    for placed in layers {
        let natural = natural_size(
            &placed.layer.kind,
            (placed.image.width, placed.image.height),
            frame,
        );
        draw(
            &mut out,
            frame,
            &Draw {
                rgba: &placed.image.rgba,
                size: (placed.image.width, placed.image.height),
                natural,
                layer: &placed.layer,
                blend: sue_model::BlendMode::Normal,
                cover: None,
            },
        );
    }
    out
}

/// One picture to draw into a frame.
#[derive(Clone, Copy, Debug)]
pub struct Draw<'a> {
    /// Its pixels, straight alpha.
    pub rgba: &'a [u8],
    /// Its size.
    pub size: (u32, u32),
    /// The size it is laid out at (its source's, or the frame's).
    pub natural: (u32, u32),
    /// Its placement, opacity, tint and wipe; offsets in frame pixels.
    pub layer: &'a Layer,
    /// How it mixes with what is under it.
    pub blend: sue_model::BlendMode,
    /// A share of each frame pixel it shows on (a track matte), row by
    /// row.
    pub cover: Option<&'a [f32]>,
}

fn sample(rgba: &[u8], width: u32, height: u32, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (i64::from(width), i64::from(height));
    let x = u - 0.5;
    let y = v - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let at = |px: i64, py: i64| -> [f32; 4] {
        let (px, py) = (px.clamp(0, w - 1), py.clamp(0, h - 1));
        let index = ((py * w + px) * 4) as usize;
        rgba.get(index..index + 4).map_or([0.0; 4], |p| {
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
        // Mixed by alpha, so clear neighbours lend no color.
        let top = a[channel] * a[3] + (b[channel] * b[3] - a[channel] * a[3]) * tx;
        let bottom = c[channel] * c[3] + (d[channel] * d[3] - c[channel] * c[3]) * tx;
        out[channel] = top + (bottom - top) * ty;
    }
    let alpha_top = a[3] + (b[3] - a[3]) * tx;
    let alpha_bottom = c[3] + (d[3] - c[3]) * tx;
    let alpha = alpha_top + (alpha_bottom - alpha_top) * ty;
    if alpha > 0.0 {
        for value in out.iter_mut().take(3) {
            *value /= alpha;
        }
    }
    out[3] = alpha;
    out
}

/// Draws one picture into `out`, an opaque frame of `frame` size, as its
/// layer places it: fit, scaled, turned about its anchor and moved, faded,
/// tinted, wiped, cut by a matte and mixed by its blend mode.
pub fn draw(out: &mut [u8], frame: (u32, u32), picture: &Draw<'_>) {
    let (width, height) = (frame.0 as usize, frame.1 as usize);
    let (iw, ih) = picture.size;
    if iw == 0 || ih == 0 || width == 0 || height == 0 {
        return;
    }
    let layer = picture.layer;
    let quad = corners(layer, picture.natural, frame);
    let (nw, nh) = (
        picture.natural.0.max(1) as f32,
        picture.natural.1.max(1) as f32,
    );
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
    let sx = iw as f32 / (nw * fit);
    let sy = ih as f32 / (nh * fit);
    // The anchor in the picture's own pixels.
    let anchor = [
        layer.anchor[0] * iw as f32 / nw,
        layer.anchor[1] * ih as f32 / nh,
    ];
    let center = [
        frame.0 as f32 / 2.0 + layer.offset[0],
        frame.1 as f32 / 2.0 + layer.offset[1],
    ];
    let identity = layer.rotation == 0.0
        && (sx - 1.0).abs() < 1e-4
        && (sy - 1.0).abs() < 1e-4
        && (quad[0][0] - quad[0][0].round()).abs() < 1e-3
        && (quad[0][1] - quad[0][1].round()).abs() < 1e-3;
    let tint = layer
        .tint
        .map(|(rgb, amount)| (rgb.map(f32::from), amount.clamp(0.0, 1.0)));
    let opacity = layer.opacity;
    let wipe = layer.wipe;
    let normal = matches!(
        picture.blend,
        sue_model::BlendMode::Normal | sue_model::BlendMode::Through
    );
    let size_f = (frame.0 as f32, frame.1 as f32);
    let rgba = picture.rgba;
    let cover = picture.cover;
    let blend = picture.blend;
    crate::fx::pixels::strips(out, width * 4, height, &|first, rows| {
        for (row, line) in rows.chunks_exact_mut(width * 4).enumerate() {
            let y_index = first + row;
            let y = y_index as f32 + 0.5;
            if y < bounds[1] || y > bounds[3] {
                continue;
            }
            let x0 = bounds[0].floor().max(0.0) as usize;
            let x1 = (bounds[2].ceil().max(0.0) as usize).min(width);
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let (dx, dy) = (px - center[0], y - center[1]);
                // Turn back, then into the picture's pixels.
                let (qx, qy) = (dx * cos + dy * sin, -dx * sin + dy * cos);
                let u = qx * sx + iw as f32 / 2.0 + anchor[0];
                let v = qy * sy + ih as f32 / 2.0 + anchor[1];
                if u < 0.0 || v < 0.0 || u > iw as f32 || v > ih as f32 {
                    continue;
                }
                let mut src = if identity {
                    let (ix, iy) = (
                        (u as usize).min(iw as usize - 1),
                        (v as usize).min(ih as usize - 1),
                    );
                    let index = (iy * iw as usize + ix) * 4;
                    rgba.get(index..index + 4).map_or([0.0; 4], |p| {
                        [
                            f32::from(p[0]),
                            f32::from(p[1]),
                            f32::from(p[2]),
                            f32::from(p[3]),
                        ]
                    })
                } else {
                    sample(rgba, iw, ih, u, v)
                };
                if let Some((rgb, amount)) = tint {
                    for channel in 0..3 {
                        src[channel] += (rgb[channel] - src[channel]) * amount;
                    }
                }
                let mut alpha = src[3] / 255.0 * opacity;
                if let Some(wipe) = wipe {
                    alpha *= wipe.cover(px, y, size_f);
                }
                if let Some(cover) = cover {
                    alpha *= cover.get(y_index * width + x).copied().unwrap_or(0.0);
                }
                if alpha <= 0.0 {
                    continue;
                }
                let alpha = alpha.min(1.0);
                let dst = &mut line[x * 4..x * 4 + 4];
                let under = f32::from(dst[3]) / 255.0;
                if normal {
                    // Over what is there, whose alpha may be less than
                    // whole (a matte being drawn); over an opaque frame
                    // this is the plain mix.
                    let kept = under * (1.0 - alpha);
                    let total = alpha + kept;
                    if total <= 0.0 {
                        continue;
                    }
                    for channel in 0..3 {
                        let value = (src[channel] * alpha + f32::from(dst[channel]) * kept) / total;
                        dst[channel] = value.round().clamp(0.0, 255.0) as u8;
                    }
                    dst[3] = (total * 255.0).round().clamp(0.0, 255.0) as u8;
                } else {
                    let back = [
                        f32::from(dst[0]) / 255.0,
                        f32::from(dst[1]) / 255.0,
                        f32::from(dst[2]) / 255.0,
                        under,
                    ];
                    let front = [src[0] / 255.0, src[1] / 255.0, src[2] / 255.0, alpha];
                    let mixed = blend.composite(back, front);
                    for channel in 0..4 {
                        dst[channel] = (mixed[channel] * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    });
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
/// size and color, centred), its letters `unit` frame pixels to the
/// sequence pixel (a smaller preview draws them smaller).
#[must_use]
pub fn title(arena: &Arena, id: EntityId, frame: (u32, u32), unit: f32) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let text = entity
        .string_buffer()
        .map(|buffer| buffer.as_str().to_owned())
        .unwrap_or_default();
    let (width, height) = frame;
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let color = crate::media::import::color_of(entity, [255, 255, 255, 255]);
    let size = model::number(entity, key::SIZE).unwrap_or(96.0) as f32 * unit;
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
/// animation, a title or a solid, for a frame of `frame` size that is
/// `unit` frame pixels to the sequence pixel. `None` for movies (they
/// decode) and what shows nothing.
#[must_use]
pub fn generated(
    arena: &Arena,
    id: EntityId,
    kind: &SourceKind,
    source_time: Time,
    frame: (u32, u32),
    unit: f32,
) -> Option<VideoFrame> {
    match kind {
        SourceKind::Still => still(arena, id),
        SourceKind::Picture => picture(arena, id, source_time, frame.0.max(frame.1).max(256)),
        SourceKind::Title => title(arena, id, frame, unit),
        SourceKind::Shape => crate::layers::draw_shape(arena, id, frame, unit),
        SourceKind::Solid => solid(arena, id),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
