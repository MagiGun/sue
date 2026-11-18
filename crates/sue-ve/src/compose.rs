//! The whole picture of a sequence at a moment, on the processor: every
//! layer with its effects stack, grade, masks, transition blur, track
//! matte, motion blur, anchor point and blend mode; adjustment clips
//! working on what is under them; nested sequences drawn as pictures.
//!
//! The export draws every frame with [`frame`], and so does the program
//! monitor whenever a frame holds anything the graphics card's plain
//! quads cannot show ([`needs_processing`]): the same function at a
//! smaller size, so a preview is what the export writes (the tests compare
//! the two). Frames the monitor draws this way are kept in the render
//! cache by [`frame_key`], which changes only when something that shows
//! at that moment changes.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use sue_core::{Arena, EntityId, Time, Value};
use sue_model::{BlendMode, Clip, Sequence};

use crate::fx::pixels::{Ctx, Image};
use crate::fx::{self, key as fx_key, mask};
use crate::grade::{self, Baked, Grade};
use crate::media::frames::VideoFrame;
use crate::model::{self, SourceKind};
use crate::render::{self, Draw, Layer};

/// A sequence property: the color under everything (CapCut's canvas;
/// black without it).
pub const CANVAS: &str = "ve.canvas";

/// The canvas color of `sequence`.
#[must_use]
pub fn canvas_of(sequence: &Sequence) -> [u8; 3] {
    let rgba = crate::layers::rgba_of(sequence.properties.get(CANVAS), [0, 0, 0, 255]);
    [rgba[0], rgba[1], rgba[2]]
}

/// How a frame is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Options {
    /// With the clips' grades (else as shot: the Color page's before).
    pub grades: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { grades: true }
    }
}

/// What [`frame`] keeps from one frame to the next: baked grades.
#[derive(Debug, Default)]
pub struct Composer {
    grades: HashMap<u64, Arc<Baked>>,
}

impl Composer {
    fn baked(&mut self, arena: &Arena, clip: &Clip) -> Option<Arc<Baked>> {
        let correctors = grade::grade_of(clip);
        let grade = Grade { correctors };
        if grade.is_neutral() {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{grade:?}").hash(&mut hasher);
        for lut in grade.correctors.iter().filter_map(|c| c.lut) {
            arena
                .get_by_id(lut)
                .map(sue_core::Entity::revision)
                .unwrap_or(0)
                .hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some(found) = self.grades.get(&key) {
            return Some(Arc::clone(found));
        }
        if self.grades.len() > 64 {
            self.grades.clear();
        }
        let luts = |id: EntityId| grade::lut_of(arena, id);
        let baked = Arc::new(grade.bake(grade::BAKE_SIZE, &luts));
        self.grades.insert(key, Arc::clone(&baked));
        Some(baked)
    }
}

/// Gives the pixels of a layer's source (a decoded frame, a drawn title).
pub type Pictures<'a> = dyn FnMut(&Layer) -> Option<Arc<VideoFrame>> + 'a;

/// How deep sequences nest before the drawing stops (a sequence that
/// holds itself shows nothing past this).
const NESTING: usize = 8;

/// The video track a track matte's number names: 1 is the bottom video
/// track (V1).
#[must_use]
pub fn video_track(sequence: &Sequence, number: usize) -> Option<usize> {
    let video: Vec<usize> = (0..sequence.tracks.len())
        .rev()
        .filter(|index| model::is_video(&sequence.tracks[*index]))
        .collect();
    number.checked_sub(1).and_then(|at| video.get(at).copied())
}

fn clip_of<'a>(sequence: &'a Sequence, layer: &Layer) -> Option<&'a Clip> {
    sequence.tracks.get(layer.track)?.clip(layer.clip)
}

fn blend_of(clip: &Clip) -> BlendMode {
    clip.properties
        .get(fx_key::BLEND)
        .and_then(Value::as_str)
        .map_or(BlendMode::Normal, BlendMode::from_name_or_normal)
}

fn matte_of(clip: &Clip, time: Time) -> Option<(usize, bool, bool)> {
    fx::effects(clip)
        .into_iter()
        .find(|effect| effect.enabled && effect.kind == "key.track_matte")
        .map(|effect| {
            let params = effect.at(clip, time);
            (
                params.f("track").round().max(1.0) as usize,
                params.pick("mode") == 1,
                params.on("invert"),
            )
        })
}

/// `true` when the frame at `time` holds something the plain quads of the
/// graphics card cannot show, so the processor must draw it.
#[must_use]
pub fn needs_processing(
    sequence: &Sequence,
    time: Time,
    kind: &dyn Fn(EntityId) -> SourceKind,
) -> bool {
    render::layers_at(sequence, time, kind).iter().any(|layer| {
        !layer.is_plain() || clip_of(sequence, layer).is_some_and(fx::needs_processing)
    })
}

/// The frame of `sequence` at `time`, `size` large, as opaque RGBA.
/// `pictures` gives the sources' pixels.
pub fn frame(
    arena: &Arena,
    sequence: &Sequence,
    time: Time,
    size: (u32, u32),
    options: Options,
    pictures: &mut Pictures<'_>,
    composer: &mut Composer,
) -> Vec<u8> {
    let [r, g, b] = canvas_of(sequence);
    let out_len = size.0 as usize * size.1 as usize * 4;
    let mut out = [r, g, b, 255].repeat(out_len / 4);
    draw_tracks(
        arena, sequence, time, size, options, None, &mut out, pictures, composer, 0,
    );
    out
}

/// What a source time's frame index is, for the grain that moves.
fn frame_index(sequence: &Sequence, time: Time) -> i64 {
    time.frame(sequence.rate)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one frame's whole drawing: the data, the moment, the size, the options, which tracks, the canvas, the sources and the caches"
)]
fn draw_tracks(
    arena: &Arena,
    sequence: &Sequence,
    time: Time,
    size: (u32, u32),
    options: Options,
    only: Option<usize>,
    canvas: &mut [u8],
    pictures: &mut Pictures<'_>,
    composer: &mut Composer,
    depth: usize,
) {
    if depth > NESTING {
        return;
    }
    let kind = |id: EntityId| {
        arena
            .get_by_id(id)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let layers = render::layers_at(sequence, time, &kind);
    let seq_size = model::frame_size(sequence);
    // Sequence pixels to frame pixels.
    let unit = size.0 as f32 / seq_size.0.max(1) as f32;
    // The tracks other layers use as mattes are not drawn themselves.
    let mut mattes: Vec<usize> = Vec::new();
    if only.is_none() {
        for layer in &layers {
            if let Some((number, _, _)) =
                clip_of(sequence, layer).and_then(|clip| matte_of(clip, time))
                && let Some(track) = video_track(sequence, number)
                && track != layer.track
            {
                mattes.push(track);
            }
        }
    }
    for layer in &layers {
        if only.is_some_and(|track| track != layer.track) || mattes.contains(&layer.track) {
            continue;
        }
        let Some(clip) = clip_of(sequence, layer) else {
            continue;
        };
        if layer.kind == SourceKind::Adjustment {
            adjust(
                arena, sequence, clip, layer, time, size, unit, options, canvas, composer,
            );
            continue;
        }
        // The source's pixels and the size it is laid out at.
        let (picture, natural) = if layer.kind == SourceKind::Sequence {
            let Some(inner) = model::sequence(arena, layer.source) else {
                continue;
            };
            let inner_size = model::frame_size(inner);
            let drawn = (
                ((inner_size.0 as f32 * unit).round() as u32).max(2),
                ((inner_size.1 as f32 * unit).round() as u32).max(2),
            );
            let mut inner_canvas = vec![0_u8; drawn.0 as usize * drawn.1 as usize * 4];
            for pixel in inner_canvas.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
            draw_tracks(
                arena,
                inner,
                layer.source_time,
                drawn,
                options,
                None,
                &mut inner_canvas,
                pictures,
                composer,
                depth + 1,
            );
            (
                Arc::new(VideoFrame::new(drawn.0, drawn.1, inner_canvas, 0)),
                inner_size,
            )
        } else if let (SourceKind::Title, Some(animator)) =
            (&layer.kind, crate::layers::animator(clip))
        {
            let range = clip.value_at(crate::layers::RANGE, time, 100.0);
            let Some(drawn) =
                crate::layers::animated_title(arena, layer.source, size, unit, &animator, range)
            else {
                continue;
            };
            (Arc::new(drawn), seq_size)
        } else {
            let Some(picture) = pictures(layer) else {
                continue;
            };
            let natural = match layer.kind {
                SourceKind::Title | SourceKind::Solid | SourceKind::Shape => seq_size,
                _ => model::size_of(arena, layer.source).unwrap_or((picture.width, picture.height)),
            };
            (picture, natural)
        };
        let scale = picture.width as f32 / natural.0.max(1) as f32;
        let image = process(
            arena, sequence, clip, layer, &picture, scale, time, options, composer,
        );
        let (rgba, image_size): (&[u8], (u32, u32)) = match &image {
            Some(image) => (&image.rgba, (image.width, image.height)),
            None => (&picture.rgba, (picture.width, picture.height)),
        };
        // The matte this layer is cut by, in frame pixels.
        let matte = if only.is_none() {
            matte_of(clip, time).and_then(|(number, luma, invert)| {
                let track = video_track(sequence, number)?;
                (track != layer.track).then(|| {
                    let mut plane = vec![0_u8; canvas.len()];
                    draw_tracks(
                        arena,
                        sequence,
                        time,
                        size,
                        options,
                        Some(track),
                        &mut plane,
                        pictures,
                        composer,
                        depth + 1,
                    );
                    plane
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| {
                            let alpha = f32::from(p[3]) / 255.0;
                            let value = if luma {
                                (0.2126 * f32::from(p[0])
                                    + 0.7152 * f32::from(p[1])
                                    + 0.0722 * f32::from(p[2]))
                                    / 255.0
                                    * alpha
                            } else {
                                alpha
                            };
                            if invert { 1.0 - value } else { value }
                        })
                        .collect::<Vec<f32>>()
                })
            })
        } else {
            None
        };
        let mut placed = layer.clone();
        placed.offset = [layer.offset[0] * unit, layer.offset[1] * unit];
        let blend = blend_of(clip);
        let shutter = clip
            .properties
            .get(fx_key::MOTION_BLUR)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 720.0);
        if shutter > 0.0 {
            motion_blurred(
                canvas,
                size,
                sequence,
                clip,
                layer,
                rgba,
                image_size,
                natural,
                unit,
                time,
                shutter,
                blend,
                matte.as_deref(),
            );
        } else {
            render::draw(
                canvas,
                size,
                &Draw {
                    rgba,
                    size: image_size,
                    natural,
                    layer: &placed,
                    blend,
                    cover: matte.as_deref(),
                },
            );
        }
    }
}

/// A layer's pixels with its effects, grade, masks and transition blur,
/// or `None` when it has none of them (the source's pixels are drawn as
/// they are, with no copy).
#[expect(
    clippy::too_many_arguments,
    reason = "a layer's processing needs its clip, its picture, its scales, the moment and the caches"
)]
fn process(
    arena: &Arena,
    sequence: &Sequence,
    clip: &Clip,
    layer: &Layer,
    picture: &VideoFrame,
    scale: f32,
    time: Time,
    options: Options,
    composer: &mut Composer,
) -> Option<Image> {
    let effects: Vec<fx::Effect> = fx::effects(clip)
        .into_iter()
        .filter(|effect| effect.enabled && effect.kind != "key.track_matte")
        .collect();
    let baked = if options.grades {
        composer.baked(arena, clip)
    } else {
        None
    };
    let masks = mask::masks_at(clip, time);
    if effects.is_empty() && baked.is_none() && masks.is_empty() && layer.blur <= 0.0 {
        return None;
    }
    let mut image = Image::new(picture.width, picture.height, picture.rgba.clone());
    let ctx = Ctx {
        scale,
        frame: frame_index(sequence, time),
    };
    for effect in &effects {
        let params = effect.at(clip, time);
        fx::pixels::apply(&effect.kind, &params, &mut image, &ctx);
    }
    if let Some(baked) = baked {
        baked.apply(&mut image.rgba, image.width as usize);
    }
    if let Some(cover) = mask::combined(&masks, image.width as usize, image.height as usize, scale)
    {
        mask::cut(&mut image, &cover);
    }
    if layer.blur > 0.0 {
        // The transition's blur, in source pixels as the effects' are.
        fx::pixels::blur(&mut image, layer.blur * scale);
    }
    Some(image)
}

/// An adjustment clip: its effects, grade and masks worked on the frame
/// under it, mixed back by its opacity (and masks).
#[expect(
    clippy::too_many_arguments,
    reason = "an adjustment works on the canvas with its clip's settings at the moment"
)]
fn adjust(
    arena: &Arena,
    sequence: &Sequence,
    clip: &Clip,
    layer: &Layer,
    time: Time,
    size: (u32, u32),
    unit: f32,
    options: Options,
    canvas: &mut [u8],
    composer: &mut Composer,
) {
    let before = canvas.to_vec();
    let mut image = Image::new(size.0, size.1, before.clone());
    let ctx = Ctx {
        scale: unit,
        frame: frame_index(sequence, time),
    };
    let mut changed = false;
    for effect in fx::effects(clip).iter().filter(|effect| effect.enabled) {
        let params = effect.at(clip, time);
        fx::pixels::apply(&effect.kind, &params, &mut image, &ctx);
        changed = true;
    }
    if options.grades
        && let Some(baked) = composer.baked(arena, clip)
    {
        baked.apply(&mut image.rgba, size.0 as usize);
        changed = true;
    }
    if !changed {
        return;
    }
    let (w, h) = (size.0 as usize, size.1 as usize);
    let cover = mask::combined(&mask::masks_at(clip, time), w, h, unit);
    let opacity = layer.opacity;
    let wipe = layer.wipe;
    for (index, (pixel, (was, now))) in canvas
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(
            before
                .as_chunks::<4>()
                .0
                .iter()
                .zip(image.rgba.as_chunks::<4>().0),
        )
        .enumerate()
    {
        let mut amount = opacity;
        if let Some(cover) = &cover {
            amount *= cover[index];
        }
        if let Some(wipe) = wipe {
            amount *= wipe.cover(
                (index % w) as f32 + 0.5,
                (index / w) as f32 + 0.5,
                (w as f32, h as f32),
            );
        }
        for channel in 0..3 {
            let value = f32::from(was[channel])
                + (f32::from(now[channel]) - f32::from(was[channel])) * amount;
            pixel[channel] = value.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// How many placements motion blur averages.
const MOTION_SAMPLES: usize = 8;

/// A layer drawn with motion blur: its placement sampled across the
/// shutter, averaged, then laid over the canvas once.
#[expect(
    clippy::too_many_arguments,
    reason = "motion blur needs the layer's pixels and placement over the shutter"
)]
fn motion_blurred(
    canvas: &mut [u8],
    size: (u32, u32),
    sequence: &Sequence,
    clip: &Clip,
    layer: &Layer,
    rgba: &[u8],
    image_size: (u32, u32),
    natural: (u32, u32),
    unit: f32,
    time: Time,
    shutter: f64,
    blend: BlendMode,
    matte: Option<&[f32]>,
) {
    let pixels = size.0 as usize * size.1 as usize;
    let mut sum = vec![0.0_f32; pixels * 4];
    let frame = 1.0 / sequence.rate.as_f64();
    let open = frame * shutter / 360.0;
    let mut sample = vec![0_u8; pixels * 4];
    for step in 0..MOTION_SAMPLES {
        let t = time + Time::from_seconds(open * (step as f64 / (MOTION_SAMPLES - 1) as f64 - 0.5));
        let mut placed = render::placed_at(layer.track, clip, layer.kind.clone(), t);
        placed.opacity = 1.0;
        placed.tint = layer.tint;
        placed.wipe = layer.wipe;
        placed.offset = [placed.offset[0] * unit, placed.offset[1] * unit];
        sample.fill(0);
        render::draw(
            &mut sample,
            size,
            &Draw {
                rgba,
                size: image_size,
                natural,
                layer: &placed,
                blend: BlendMode::Normal,
                cover: None,
            },
        );
        for (total, p) in sum
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(sample.as_chunks::<4>().0)
        {
            let alpha = f32::from(p[3]) / 255.0;
            for channel in 0..3 {
                total[channel] += f32::from(p[channel]) * alpha;
            }
            total[3] += alpha;
        }
    }
    let count = MOTION_SAMPLES as f32;
    let averaged: Vec<u8> = sum
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|total| {
            let alpha = total[3] / count;
            let color = |channel: usize| {
                if total[3] > 0.0 {
                    (total[channel] / total[3]).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            [color(0), color(1), color(2), (alpha * 255.0).round() as u8]
        })
        .collect();
    let mut flat = Layer::new(layer.source, SourceKind::Solid, layer.source_time);
    flat.opacity = layer.opacity;
    render::draw(
        canvas,
        size,
        &Draw {
            rgba: &averaged,
            size,
            natural: size,
            layer: &flat,
            blend,
            cover: matte,
        },
    );
}

/// A number that changes whenever anything that shows in the frame of
/// `sequence` at `time` changes (its clips, their sources, effects,
/// grades, masks, transitions), drawn `size` large with `options`: what
/// the render cache keeps frames by. Unchanged stretches keep their key
/// across edits elsewhere.
#[must_use]
pub fn frame_key(
    arena: &Arena,
    sequence: &Sequence,
    time: Time,
    size: (u32, u32),
    options: Options,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    size.hash(&mut hasher);
    options.hash(&mut hasher);
    key_into(arena, sequence, time, &mut hasher, 0);
    hasher.finish()
}

fn key_into(
    arena: &Arena,
    sequence: &Sequence,
    time: Time,
    hasher: &mut impl Hasher,
    depth: usize,
) {
    if depth > NESTING {
        return;
    }
    model::frame_size(sequence).hash(hasher);
    for track in &sequence.tracks {
        if !model::is_video(track) {
            continue;
        }
        track.muted.hash(hasher);
        let mut writer = sue_core::codec::Writer::new();
        for clip in track.clips() {
            let reach_start = track
                .transitions
                .iter()
                .filter(|t| t.to == clip.id)
                .map(|t| t.start)
                .min()
                .unwrap_or(clip.start)
                .min(clip.start);
            let reach_end = track
                .transitions
                .iter()
                .filter(|t| t.from == clip.id)
                .map(|t| t.end())
                .max()
                .unwrap_or(clip.end())
                .max(clip.end());
            // Motion blur reaches half a frame either way.
            let slack = Time::from_frames(1, sequence.rate);
            if time + slack < reach_start || time >= reach_end + slack {
                continue;
            }
            clip.id.hash(hasher);
            clip.source.hash(hasher);
            clip.enabled.hash(hasher);
            clip.start.flicks().hash(hasher);
            clip.duration.flicks().hash(hasher);
            clip.speed.to_bits().hash(hasher);
            render::source_time_of(clip, time).flicks().hash(hasher);
            clip.properties.encode(&mut writer);
            for (name, curve) in clip.animation.channels() {
                name.hash(hasher);
                for key in curve.keys() {
                    key.time.flicks().hash(hasher);
                    key.value.to_bits().hash(hasher);
                    format!("{:?}", key.interpolation).hash(hasher);
                }
            }
            if let Ok(entity) = arena.get_by_id(clip.source) {
                entity.revision().hash(hasher);
                // Effects that read other entities (LUTs).
                for lut in grade::grade_of(clip).iter().filter_map(|c| c.lut) {
                    arena
                        .get_by_id(lut)
                        .map(sue_core::Entity::revision)
                        .unwrap_or(0)
                        .hash(hasher);
                }
                if let Some(inner) = entity.ext::<Sequence>() {
                    key_into(
                        arena,
                        inner,
                        render::source_time_of(clip, time),
                        hasher,
                        depth + 1,
                    );
                }
            }
        }
        writer.into_bytes().hash(hasher);
        for transition in &track.transitions {
            if transition.start <= time && time < transition.end() {
                transition.kind.hash(hasher);
                transition.progress(time).to_bits().hash(hasher);
                let mut props = sue_core::codec::Writer::new();
                transition.properties.encode(&mut props);
                props.into_bytes().hash(hasher);
            }
        }
    }
    time.frame(sequence.rate).hash(hasher);
}

#[cfg(test)]
mod tests;
