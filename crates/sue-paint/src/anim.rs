//! Animation, as Clip Studio Paint's timeline and animation folders work.
//! A picture's timeline is the shared [`Sequence`] on the picture (the same
//! model a video editor or a DAW edits): its rate is the frame rate, each
//! track of kind [`CELS`] is an animation folder, and each clip is a key
//! that shows one cel (a layer of the picture, by id) from its frame until
//! the next key. A blank key shows nothing. The timeline's length in
//! frames is the picture's [`LENGTH`].
//!
//! At any frame, the cels no folder shows are hidden; onion skin shows the
//! cels before (blue) and after (green) faintly, as its settings say
//! ([`Onion`]), and the light table shows the layers registered to the cel
//! being drawn ([`LIGHT_TABLE`]).

use std::collections::HashSet;

use sue_core::txn::{Batch, SetExtension, SetProperty};
use sue_core::{Arena, EntityId, Rate, Time, Value};
use sue_model::{Clip, Sequence, Track};

use crate::composite::{FrameView, Skin};

/// The kind of track that is an animation folder.
pub const CELS: &str = "sue.paint.cels";
/// A picture's timeline length in frames (`Int`).
pub const LENGTH: &str = "anim.length";
/// A key that shows nothing (a clip property).
const BLANK: &str = "anim.blank";
/// The kind of track that holds sound: each clip plays an audio entity
/// from its start.
pub const AUDIO: &str = "sue.paint.audio";
/// The rate the timeline's sound is mixed at.
pub const MIX_RATE: u32 = 48_000;
/// A picture's onion skin settings ([`Onion::to_text`]).
pub const ONION: &str = "anim.onion";
/// The relation from a cel to a layer on its light table, shown to trace
/// from while the cel is drawn.
pub const LIGHT_TABLE: &str = "sue.paint.light_table";

/// How onion skin shows the cels around the current one: Clip Studio
/// Paint's Onion Skin Settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onion {
    /// How many cels before the current one show.
    pub before: u32,
    /// How many cels after it show.
    pub after: u32,
    /// Color mode: 0 Color (the cels' own), 1 Half color, 2 Monochrome.
    pub mode: u8,
    /// The color of the cels before.
    pub before_color: [u8; 3],
    /// The color of the cels after.
    pub after_color: [u8; 3],
    /// Specify opacity level: the nearest cel's opacity and how much less
    /// each one further shows (0 to 1); `None` shows them fully.
    pub opacity: Option<(f32, f32)>,
}

impl Default for Onion {
    fn default() -> Self {
        Self {
            before: 1,
            after: 1,
            mode: 2,
            before_color: [0, 0, 255],
            after_color: [0, 255, 0],
            opacity: Some((0.8, 0.1)),
        }
    }
}

impl Onion {
    /// The color modes, in order.
    pub const MODES: [&str; 3] = ["Color", "Half color", "Monochrome"];

    /// The settings as text, for the picture.
    #[must_use]
    pub fn to_text(&self) -> String {
        let hex = |[r, g, b]: [u8; 3]| format!("{r:02x}{g:02x}{b:02x}");
        let (on, start, step) = self
            .opacity
            .map_or((0, 0.8, 0.1), |(start, step)| (1, start, step));
        format!(
            "{},{},{},{},{},{on},{start},{step}",
            self.before,
            self.after,
            self.mode,
            hex(self.before_color),
            hex(self.after_color),
        )
    }

    /// Settings read back from [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        let [
            before,
            after,
            mode,
            before_color,
            after_color,
            on,
            start,
            step,
        ] = parts.as_slice()
        else {
            return None;
        };
        let hex = |value: &str| -> Option<[u8; 3]> {
            let byte = |at: usize| u8::from_str_radix(value.get(at..at + 2)?, 16).ok();
            Some([byte(0)?, byte(2)?, byte(4)?])
        };
        Some(Self {
            before: before.parse().ok()?,
            after: after.parse().ok()?,
            mode: mode.parse::<u8>().ok()?.min(2),
            before_color: hex(before_color)?,
            after_color: hex(after_color)?,
            opacity: (*on == "1")
                .then(|| Some((start.parse().ok()?, step.parse().ok()?)))
                .flatten(),
        })
    }

    /// The settings on `picture` (the default when it has none).
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Self {
        arena
            .get_by_id(picture)
            .ok()
            .and_then(|entity| entity.property(ONION))
            .and_then(Value::as_str)
            .and_then(Self::from_text)
            .unwrap_or_default()
    }

    /// How much the cel `away` cels from the current one shows (1 is the
    /// nearest).
    #[must_use]
    pub fn opacity_at(&self, away: u32) -> f32 {
        self.opacity.map_or(1.0, |(start, step)| {
            (start - step * away.saturating_sub(1) as f32).clamp(0.0, 1.0)
        })
    }

    /// How much of a cel's color gives way to the skin color.
    #[must_use]
    pub fn tint(&self) -> f32 {
        match self.mode {
            0 => 0.0,
            1 => 0.5,
            _ => 1.0,
        }
    }
}

/// The relation from a picture to a layer on its general light table,
/// shown on every cel.
pub const GENERAL_LIGHT_TABLE: &str = "sue.paint.light_table.general";
/// Where a layer sits on a light table, moved and turned by the Light
/// table tool without changing it: `dx,dy,degrees` (`Text`).
pub const PLACE: &str = "light_table.place";
/// A layer that shows only on light tables (a file registered with Select
/// and register file), never in the picture itself (`Bool`).
pub const ONLY: &str = "light_table.only";

/// A layer's place on light tables: across, down and turned (degrees).
#[must_use]
pub fn place_of(entity: &sue_core::Entity) -> [f32; 3] {
    let values: Vec<f32> = entity
        .property(PLACE)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect();
    match values.as_slice() {
        &[dx, dy, degrees] => [dx, dy, degrees],
        _ => [0.0; 3],
    }
}

/// The text of a place ([`PLACE`]).
#[must_use]
pub fn place_text([dx, dy, degrees]: [f32; 3]) -> String {
    format!("{dx},{dy},{degrees}")
}

/// What a canvas point sees on a layer placed at `place` on a `size`
/// canvas: turned about the canvas's middle, then moved (an affine map,
/// as camera folders use). `None` for a layer where it is.
#[must_use]
pub fn place_map(place: [f32; 3], size: [u32; 2]) -> Option<[f32; 6]> {
    if place == [0.0; 3] {
        return None;
    }
    let [dx, dy, degrees] = place;
    let (cx, cy) = (size[0] as f32 / 2.0, size[1] as f32 / 2.0);
    let (sin, cos) = (-degrees.to_radians()).sin_cos();
    // Undo the move, then the turn about the middle.
    let (ox, oy) = (-dx - cx, -dy - cy);
    Some([
        cos,
        -sin,
        sin,
        cos,
        cos * ox - sin * oy + cx,
        sin * ox + cos * oy + cy,
    ])
}

/// The layers on `picture`'s general light table, shown on every cel.
#[must_use]
pub fn general_light_table(arena: &Arena, picture: EntityId) -> Vec<EntityId> {
    arena.relations().targets(picture, GENERAL_LIGHT_TABLE)
}

/// The edit that puts `layer` on `picture`'s general light table.
#[must_use]
pub fn register_general(picture: EntityId, layer: EntityId) -> Edit {
    Edit::new(tr!("register-to-light-table")).then(sue_core::txn::Relate::new(
        sue_core::Relation::new(picture, GENERAL_LIGHT_TABLE, layer),
    ))
}

/// The edit that takes `layers` off `picture`'s general light table.
#[must_use]
pub fn deregister_general(picture: EntityId, layers: &[EntityId]) -> Edit {
    layers.iter().fold(
        Edit::new(tr!("deregister-from-light-table")),
        |edit, layer| {
            edit.then(sue_core::txn::Unrelate::new(
                picture,
                GENERAL_LIGHT_TABLE,
                *layer,
            ))
        },
    )
}

/// The edit that puts `layers` back where they are on their own (Reset
/// position of layers on light table).
#[must_use]
pub fn reset_places(arena: &Arena, layers: &[EntityId]) -> Edit {
    layers
        .iter()
        .filter(|layer| {
            arena
                .get_by_id(**layer)
                .is_ok_and(|entity| place_of(entity) != [0.0; 3])
        })
        .fold(
            Edit::new(tr!("reset-position-layers-light-table")),
            |edit, layer| edit.then(SetProperty::new(*layer, PLACE, place_text([0.0; 3]))),
        )
}

/// A layer as a light table shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traced {
    /// The layer.
    pub layer: EntityId,
    /// What each canvas point sees on it, when it is moved.
    pub place: Option<[f32; 6]>,
    /// It shows only on light tables.
    pub only: bool,
}

/// `layers` as a light table on a `size` canvas shows them.
#[must_use]
pub fn traced(arena: &Arena, layers: &[EntityId], size: [u32; 2]) -> Vec<Traced> {
    layers
        .iter()
        .filter_map(|layer| {
            let entity = arena.get_by_id(*layer).ok()?;
            Some(Traced {
                layer: *layer,
                place: place_map(place_of(entity), size),
                only: entity.property(ONLY).and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// The layers on `cel`'s light table, in the order they were registered.
#[must_use]
pub fn light_table(arena: &Arena, cel: EntityId) -> Vec<EntityId> {
    arena.relations().targets(cel, LIGHT_TABLE)
}

/// The edit that puts `layer` on `cel`'s light table.
#[must_use]
pub fn register(cel: EntityId, layer: EntityId) -> Edit {
    Edit::new(tr!("register-to-light-table")).then(sue_core::txn::Relate::new(
        sue_core::Relation::new(cel, LIGHT_TABLE, layer),
    ))
}

/// The edit that turns `cel`'s light table upside down: its layers in
/// the reverse order.
#[must_use]
pub fn reversed(arena: &Arena, cel: EntityId) -> Edit {
    let layers = light_table(arena, cel);
    let off = layers.iter().fold(
        Edit::new(tr!("reverse-layers-vertically-light-table")),
        |edit, layer| edit.then(sue_core::txn::Unrelate::new(cel, LIGHT_TABLE, *layer)),
    );
    layers.iter().rev().fold(off, |edit, layer| {
        edit.then(sue_core::txn::Relate::new(sue_core::Relation::new(
            cel,
            LIGHT_TABLE,
            *layer,
        )))
    })
}

/// The edit that takes `layers` off `cel`'s light table.
#[must_use]
pub fn deregister(cel: EntityId, layers: &[EntityId]) -> Edit {
    layers.iter().fold(
        Edit::new(tr!("deregister-from-light-table")),
        |edit, layer| edit.then(sue_core::txn::Unrelate::new(cel, LIGHT_TABLE, *layer)),
    )
}

impl FrameView {
    /// The same with `layers` (a light table) shown at `opacity` when the
    /// frame hides them: in their own colors, or tinted `color` (the light
    /// table's Color mode).
    #[must_use]
    pub fn with_light_table(
        mut self,
        layers: &[Traced],
        opacity: f32,
        color: Option<[f32; 3]>,
    ) -> Self {
        for traced in layers {
            // A file on the light table shows only here; a moved layer
            // shows moved.
            if traced.only || traced.place.is_some() {
                self.hidden.insert(traced.layer);
            }
            if !self.onion.iter().any(|skin| skin.cel == traced.layer) {
                self.onion.push(Skin {
                    cel: traced.layer,
                    color: color.unwrap_or([0.0; 3]),
                    tint: if color.is_some() { 1.0 } else { 0.0 },
                    opacity,
                    place: traced.place,
                    force: traced.only,
                });
            }
        }
        self
    }
}

type Edit = Batch<Arena>;

/// The picture's timeline, if it has one.
#[must_use]
pub fn timeline(arena: &Arena, picture: EntityId) -> Option<&Sequence> {
    arena.get_by_id(picture).ok()?.ext::<Sequence>()
}

/// The picture's timeline length in frames.
#[must_use]
pub fn length(arena: &Arena, picture: EntityId) -> u32 {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(LENGTH))
        .and_then(Value::as_i64)
        .map_or(24, |frames| frames.clamp(1, 100_000) as u32)
}

/// Frames a second.
#[must_use]
pub fn fps(sequence: &Sequence) -> f64 {
    f64::from(sequence.rate.numerator()) / f64::from(sequence.rate.denominator().max(1))
}

/// The tracks that are animation folders, by index.
#[must_use]
pub fn folders(sequence: &Sequence) -> Vec<usize> {
    (0..sequence.tracks.len())
        .filter(|&index| sequence.tracks[index].kind == CELS)
        .collect()
}

/// A track's keys: their frames and cels (`None` for a blank key).
#[must_use]
pub fn keys(track: &Track, rate: Rate) -> Vec<(u32, Option<EntityId>)> {
    let mut keys: Vec<(u32, Option<EntityId>)> = track
        .clips()
        .iter()
        .map(|clip| {
            let frame = clip.start.frame(rate).max(0) as u32;
            let blank = clip.properties.get(BLANK).and_then(Value::as_bool) == Some(true);
            (frame, (!blank).then_some(clip.source))
        })
        .collect();
    keys.sort_by_key(|(frame, _)| *frame);
    keys
}

/// The cel a track shows at `frame`: its last key at or before it.
#[must_use]
pub fn cel_at(track: &Track, rate: Rate, frame: u32) -> Option<EntityId> {
    keys(track, rate)
        .into_iter()
        .rev()
        .find(|(at, _)| *at <= frame)
        .and_then(|(_, cel)| cel)
}

/// How the layers look at `frame`: every cel no folder shows is hidden;
/// with `onion` skin, each folder's cels before and after show faintly.
#[must_use]
pub fn view(sequence: &Sequence, frame: u32, onion: Option<&Onion>) -> FrameView {
    let mut cels = HashSet::new();
    let mut shown = HashSet::new();
    let mut faint = Vec::new();
    for index in folders(sequence) {
        let track = &sequence.tracks[index];
        let keys = keys(track, sequence.rate);
        cels.extend(keys.iter().filter_map(|(_, cel)| *cel));
        let now = cel_at(track, sequence.rate, frame);
        shown.extend(now);
        if let Some(onion) = onion {
            let current = keys.iter().rposition(|(at, _)| *at <= frame);
            let before = keys[..current.unwrap_or(0)]
                .iter()
                .rev()
                .filter_map(|(_, cel)| *cel)
                .take(onion.before as usize);
            let after = keys
                .iter()
                .filter(|(at, _)| *at > frame)
                .filter_map(|(_, cel)| *cel)
                .take(onion.after as usize);
            let unit = |[r, g, b]: [u8; 3]| [r, g, b].map(|value| f32::from(value) / 255.0);
            for (cels, color) in [
                (before.collect::<Vec<_>>(), onion.before_color),
                (after.collect::<Vec<_>>(), onion.after_color),
            ] {
                for (away, cel) in cels.into_iter().enumerate() {
                    if Some(cel) == now || faint.iter().any(|skin: &Skin| skin.cel == cel) {
                        continue;
                    }
                    faint.push(Skin {
                        cel,
                        color: unit(color),
                        tint: onion.tint(),
                        opacity: onion.opacity_at(away as u32 + 1),
                        place: None,
                        force: false,
                    });
                }
            }
        }
    }
    FrameView {
        hidden: cels.difference(&shown).copied().collect(),
        onion: faint,
        cameras: Vec::new(),
    }
}

/// A track with its keys' lengths running to the next key (the last to
/// the timeline's end), as the shared model expects of clips.
fn tidy(track: &Track, rate: Rate, length: u32) -> Track {
    let mut clips: Vec<Clip> = track.clips().to_vec();
    clips.sort_by_key(|clip| clip.start);
    let mut out = Track::new(track.name.clone(), track.kind.clone());
    out.muted = track.muted;
    out.locked = track.locked;
    out.properties = track.properties.clone();
    for index in 0..clips.len() {
        let mut clip = clips[index].clone();
        let end = clips
            .get(index + 1)
            .map_or(Time::from_frames(i64::from(length), rate), |next| {
                next.start
            });
        clip.duration = Time(
            end.flicks()
                .saturating_sub(clip.start.flicks())
                .max(Time::from_frames(1, rate).flicks()),
        );
        out.insert(clip);
    }
    out
}

/// A key on `track` at `frame` showing `cel` (`None`: blank), replacing
/// one there.
#[must_use]
pub fn assign(
    sequence: &Sequence,
    track: usize,
    frame: u32,
    cel: Option<EntityId>,
    blank_source: EntityId,
    length: u32,
) -> Sequence {
    let mut out = sequence.clone();
    let Some(old) = out.tracks.get(track) else {
        return out;
    };
    let rate = out.rate;
    let start = Time::from_frames(i64::from(frame), rate);
    let mut kept = Track::new(old.name.clone(), old.kind.clone());
    kept.properties = old.properties.clone();
    for clip in old.clips().iter().filter(|clip| clip.start != start) {
        kept.insert(clip.clone());
    }
    let mut clip = Clip::new(
        cel.unwrap_or(blank_source),
        start,
        Time::from_frames(1, rate),
    );
    if cel.is_none() {
        clip.properties.set(BLANK, true);
    }
    kept.insert(clip);
    out.tracks[track] = tidy(&kept, rate, length);
    out
}

/// `track` without its key at `frame`.
#[must_use]
pub fn unassign(sequence: &Sequence, track: usize, frame: u32, length: u32) -> Sequence {
    let mut out = sequence.clone();
    let Some(old) = out.tracks.get(track) else {
        return out;
    };
    let rate = out.rate;
    let start = Time::from_frames(i64::from(frame), rate);
    let mut kept = Track::new(old.name.clone(), old.kind.clone());
    kept.properties = old.properties.clone();
    for clip in old.clips().iter().filter(|clip| clip.start != start) {
        kept.insert(clip.clone());
    }
    out.tracks[track] = tidy(&kept, rate, length);
    out
}

/// Every folder's keys after `frame` moved `by` frames (Insert Frame and
/// Delete Frame); a key moved onto or before `frame` by a deletion goes.
#[must_use]
pub fn shift(sequence: &Sequence, frame: u32, by: i64, length: u32) -> Sequence {
    let mut out = sequence.clone();
    let rate = out.rate;
    for index in folders(sequence) {
        let old = &sequence.tracks[index];
        let mut moved = Track::new(old.name.clone(), old.kind.clone());
        moved.properties = old.properties.clone();
        for clip in old.clips() {
            let at = clip.start.frame(rate);
            let mut clip = clip.clone();
            if at > i64::from(frame) {
                let to = at + by;
                if to <= i64::from(frame) {
                    continue;
                }
                clip.start = Time::from_frames(to, rate);
            }
            moved.insert(clip);
        }
        out.tracks[index] = tidy(&moved, rate, length);
    }
    out
}

/// `sequence` at `rate` instead, every key on the frame it was on and
/// `length` frames long (Edit Timeline).
#[must_use]
pub fn retimed(sequence: &Sequence, rate: Rate, length: u32) -> Sequence {
    let old = sequence.rate;
    let mut out = sequence.clone();
    out.rate = rate;
    for index in folders(sequence) {
        let track = &sequence.tracks[index];
        let mut moved = Track::new(track.name.clone(), track.kind.clone());
        moved.properties = track.properties.clone();
        moved.muted = track.muted;
        moved.locked = track.locked;
        for clip in track.clips() {
            let frame = clip.start.frame(old);
            if frame >= i64::from(length) {
                continue;
            }
            let mut clip = clip.clone();
            clip.start = Time::from_frames(frame, rate);
            moved.insert(clip);
        }
        out.tracks[index] = tidy(&moved, rate, length);
    }
    out
}

/// The edit that sets `picture`'s timeline to `fps` and `length` frames.
#[must_use]
pub fn edit_timeline(arena: &Arena, picture: EntityId, fps: u32, length: u32) -> Option<Edit> {
    let sequence = timeline(arena, picture)?;
    let length = length.max(1);
    let retimed = retimed(sequence, Rate::new(fps.max(1), 1), length);
    Some(
        Edit::new(tr!("edit-timeline"))
            .then(SetExtension::set(picture, retimed))
            .then(SetProperty::new(picture, LENGTH, i64::from(length))),
    )
}

/// The audio tracks of `sequence`, by index.
#[must_use]
pub fn audio_tracks(sequence: &Sequence) -> Vec<usize> {
    sequence
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.kind == AUDIO)
        .map(|(index, _)| index)
        .collect()
}

/// The edit that puts the sound `pcm` named `name` on a new audio track
/// of `picture`'s timeline `sequence`, starting at `frame` (File, Import,
/// Audio). The sound is an audio entity of its own; its id is returned too.
#[must_use]
pub fn add_audio(
    picture: EntityId,
    sequence: &Sequence,
    name: &str,
    pcm: sue_core::PCMFloat1D,
    frame: u32,
) -> (Edit, EntityId) {
    let duration = Time::from_samples(pcm.frames() as i64, pcm.sample_rate());
    let sound = sue_core::Entity::new(name, pcm).with_class(sue_model::class::AUDIO);
    let id = sound.id();
    let mut out = sequence.clone();
    let mut track = Track::new(name, AUDIO);
    track.insert(Clip::new(
        id,
        Time::from_frames(i64::from(frame), sequence.rate),
        duration,
    ));
    out.tracks.push(track);
    let edit = Edit::new(tr!("import-audio", name = name))
        .then(sue_core::txn::SpawnEntity::new(sound))
        .then(SetExtension::set(picture, out));
    (edit, id)
}

/// `sequence` with the sound of audio track `track` starting at `frame`.
#[must_use]
pub fn audio_moved(sequence: &Sequence, track: usize, frame: i64) -> Sequence {
    let mut out = sequence.clone();
    if let Some(old) = sequence.tracks.get(track) {
        let mut moved = Track::new(old.name.clone(), old.kind.clone());
        moved.properties = old.properties.clone();
        moved.muted = old.muted;
        moved.locked = old.locked;
        for clip in old.clips() {
            let mut clip = clip.clone();
            clip.start = Time::from_frames(frame.max(0), sequence.rate);
            moved.insert(clip);
        }
        out.tracks[track] = moved;
    }
    out
}

/// `sequence` without track `track`.
#[must_use]
pub fn without_track(sequence: &Sequence, track: usize) -> Sequence {
    let mut out = sequence.clone();
    if track < out.tracks.len() {
        out.tracks.remove(track);
    }
    out
}

/// Every unmuted audio track of `sequence` mixed into one stereo sound at
/// [`MIX_RATE`] from the timeline's start, each clip from its start, or
/// `None` when there is nothing to hear.
#[must_use]
pub fn mix(arena: &Arena, sequence: &Sequence) -> Option<sue_core::PCMFloat1D> {
    let mut placed = Vec::new();
    for index in audio_tracks(sequence) {
        let track = &sequence.tracks[index];
        if track.muted {
            continue;
        }
        for clip in track.clips() {
            let Some(pcm) = arena
                .get_by_id(clip.source)
                .ok()
                .and_then(|entity| entity.component(sue_core::ComponentKind::PCMFloat1D))
                .and_then(|component| component.as_pcm())
            else {
                continue;
            };
            let start = clip.start.seconds().max(0.0);
            placed.push((pcm, start));
        }
    }
    let frames = placed
        .iter()
        .map(|(pcm, start)| {
            (start * f64::from(MIX_RATE)) as usize
                + (pcm.frames() as f64 * f64::from(MIX_RATE) / f64::from(pcm.sample_rate().max(1)))
                    as usize
        })
        .max()?;
    let mut out = vec![0.0_f32; frames * 2];
    for (pcm, start) in placed {
        let ratio = f64::from(pcm.sample_rate().max(1)) / f64::from(MIX_RATE);
        let offset = (start * f64::from(MIX_RATE)) as usize;
        let channels = usize::from(pcm.channels().max(1));
        let samples = pcm.samples();
        let last = pcm.frames().saturating_sub(1);
        for frame in 0..frames.saturating_sub(offset) {
            let t = frame as f64 * ratio;
            if t >= last as f64 {
                break;
            }
            let (i0, mixing) = (t.floor() as usize, (t - t.floor()) as f32);
            let at = |i: usize, channel: usize| samples[i * channels + channel.min(channels - 1)];
            for channel in 0..2 {
                let value = at(i0, channel) + (at(i0 + 1, channel) - at(i0, channel)) * mixing;
                out[(offset + frame) * 2 + channel] += value;
            }
        }
    }
    for value in &mut out {
        *value = value.clamp(-1.0, 1.0);
    }
    sue_core::PCMFloat1D::new(MIX_RATE, 2, out).ok()
}

/// `sequence` with a new animation folder named `name`.
#[must_use]
pub fn with_folder(sequence: &Sequence, name: &str) -> Sequence {
    let mut out = sequence.clone();
    out.tracks.push(Track::new(name, CELS));
    out
}

/// The edit that gives `picture` a timeline of `length` frames at `fps`
/// with one animation folder.
#[must_use]
pub fn new_timeline(picture: EntityId, fps: u32, length: u32) -> Edit {
    let sequence = with_folder(
        &Sequence::new(Rate::new(fps.max(1), 1), 48_000),
        &tr!("animation-folder-1"),
    );
    Edit::new(tr!("new-timeline"))
        .then(SetExtension::set(picture, sequence))
        .then(SetProperty::new(picture, LENGTH, i64::from(length.max(1))))
}

/// The edit that brings `frames` (a movie's) into `picture` as cels of a
/// new animation folder named `name`, one frame each from the start, at
/// `fps` when the picture has no timeline yet (File, Import, Movie). Each
/// frame is made to fit the canvas. Returns the folder's track too.
///
/// # Errors
/// If there are no frames or the picture has no size.
pub fn import_frames(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: &str,
    frames: &[sue_core::RasterTile2D],
    fps: f64,
) -> Result<(Edit, usize), String> {
    if frames.is_empty() {
        return Err(tr!("the-movie-has-no-frames"));
    }
    let size = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .ok_or(tr!("the-picture-has-no-size"))?;
    let cels: Vec<(String, sue_core::RasterTile2D)> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            let tile = if [frame.width(), frame.height()] == size {
                frame.clone()
            } else {
                crate::materials::fitted(frame, size)?
            };
            Some((format!("{name} {}", index + 1), tile))
        })
        .collect();
    let count = cels.len() as u32;
    let (spawns, ids) = crate::doc::add_layers_pixels(arena, picture, below, cels)?;
    let (sequence, length) = match timeline(arena, picture) {
        Some(sequence) => (sequence.clone(), length(arena, picture).max(count)),
        None => (
            Sequence::new(Rate::new(fps.round().clamp(1.0, 120.0) as u32, 1), MIX_RATE),
            count,
        ),
    };
    let mut sequence = with_folder(&sequence, name);
    let track = sequence.tracks.len() - 1;
    for (frame, cel) in ids.iter().enumerate() {
        sequence = assign(&sequence, track, frame as u32, Some(*cel), picture, length);
    }
    let mut edit = Edit::new(tr!("import-movie", name = name));
    for spawn in spawns {
        edit = edit.then(spawn);
    }
    let edit = edit
        .then(SetExtension::set(picture, sequence))
        .then(SetProperty::new(picture, LENGTH, i64::from(length)));
    Ok((edit, track))
}

/// What a folder shows at each frame, as an exposure sheet writes it:
/// the cel a key starts (`Some(Some(cel))`), a blank key (`Some(None)`),
/// or `None` where the frame holds what came before.
#[must_use]
pub fn sheet(track: &Track, rate: Rate, length: u32) -> Vec<Option<Option<EntityId>>> {
    let mut out = vec![None; length as usize];
    for (frame, cel) in keys(track, rate) {
        if let Some(slot) = out.get_mut(frame as usize) {
            *slot = Some(cel);
        }
    }
    out
}

/// The exposure sheet as CSV: a header of `Frame` and the folders' names,
/// then a row a frame with each folder's cel name where a key starts, `x`
/// for a blank key and nothing where the frame holds.
#[must_use]
pub fn sheet_csv(sequence: &Sequence, length: u32, name: impl Fn(EntityId) -> String) -> String {
    let quote = |text: &str| {
        if text.contains([',', '"', '\n']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_owned()
        }
    };
    let folders = folders(sequence);
    let columns: Vec<Vec<Option<Option<EntityId>>>> = folders
        .iter()
        .map(|track| sheet(&sequence.tracks[*track], sequence.rate, length))
        .collect();
    let mut out = String::from("Frame");
    for track in &folders {
        out.push(',');
        out.push_str(&quote(&sequence.tracks[*track].name));
    }
    out.push('\n');
    for frame in 0..length as usize {
        out.push_str(&(frame + 1).to_string());
        for column in &columns {
            out.push(',');
            match column[frame] {
                Some(Some(cel)) => out.push_str(&quote(&name(cel))),
                Some(None) => out.push('x'),
                None => {}
            }
        }
        out.push('\n');
    }
    out
}

/// `sequence` with its folders' keys read from an exposure sheet's CSV
/// (as [`sheet_csv`] writes it): each folder named in the header takes the
/// keys of its column, a cel found by its name through `cel`, `x` a blank;
/// folders the sheet does not name keep theirs. Answers the timeline and
/// its length (the sheet's rows, if more).
#[must_use]
pub fn from_sheet_csv(
    sequence: &Sequence,
    length: u32,
    text: &str,
    blank_source: EntityId,
    cel: impl Fn(&str) -> Option<EntityId>,
) -> (Sequence, u32) {
    let rows: Vec<Vec<String>> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(csv_fields)
        .collect();
    let Some((header, rows)) = rows.split_first() else {
        return (sequence.clone(), length);
    };
    let length = length.max(rows.len() as u32);
    let mut out = sequence.clone();
    for (column, name) in header.iter().enumerate().skip(1) {
        let Some(track) = folders(&out)
            .into_iter()
            .find(|track| out.tracks[*track].name == *name)
        else {
            continue;
        };
        // The folder's keys, all from the sheet.
        for (frame, _) in keys(&out.tracks[track], out.rate) {
            out = unassign(&out, track, frame, length);
        }
        for (frame, row) in rows.iter().enumerate() {
            let value = row.get(column).map_or("", |value| value.trim());
            if value.is_empty() {
                continue;
            }
            let shown = if value.eq_ignore_ascii_case("x") {
                None
            } else {
                match cel(value) {
                    Some(found) => Some(found),
                    None => continue,
                }
            };
            out = assign(&out, track, frame as u32, shown, blank_source, length);
        }
    }
    (out, length)
}

/// One CSV line's fields, quotes taken off.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                if let Some(field) = fields.last_mut() {
                    field.push('"');
                }
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            _ => {
                if let Some(field) = fields.last_mut() {
                    field.push(ch);
                }
            }
        }
    }
    fields
}

/// The edit that sets `picture`'s timeline to `sequence`.
#[must_use]
pub fn set(picture: EntityId, sequence: Sequence, label: &str) -> Edit {
    Edit::new(label).then(SetExtension::set(picture, sequence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Entity, ReversibleTransaction};

    #[test]
    fn light_table_places_move_and_turn_what_a_point_sees() {
        assert_eq!(place_map([0.0; 3], [100, 100]), None, "where it is");
        // Moved 10 across and 5 down: the canvas point (60, 55) sees the
        // layer's (50, 50).
        let moved = place_map([10.0, 5.0, 0.0], [100, 100]).unwrap();
        let see =
            |m: [f32; 6], x: f32, y: f32| [m[0] * x + m[1] * y + m[4], m[2] * x + m[3] * y + m[5]];
        let [x, y] = see(moved, 60.0, 55.0);
        assert!(
            (x - 50.0).abs() < 1e-4 && (y - 50.0).abs() < 1e-4,
            "{x} {y}"
        );
        // Turned a quarter about the middle: the middle stays.
        let turned = place_map([0.0, 0.0, 90.0], [100, 100]).unwrap();
        let [x, y] = see(turned, 50.0, 50.0);
        assert!((x - 50.0).abs() < 1e-4 && (y - 50.0).abs() < 1e-4);
        assert_eq!(place_text([1.5, -2.0, 30.0]), "1.5,-2,30");
    }

    #[test]
    fn the_exposure_sheet_shows_keys_holds_and_blanks() {
        let picture = EntityId::new();
        let (a, b) = (EntityId::new(), EntityId::new());
        let mut sequence = Sequence::new(Rate::new(12, 1), MIX_RATE);
        sequence = with_folder(&sequence, "A");
        let track = folders(&sequence)[0];
        sequence = assign(&sequence, track, 0, Some(a), picture, 6);
        sequence = assign(&sequence, track, 2, None, picture, 6);
        sequence = assign(&sequence, track, 4, Some(b), picture, 6);
        let column = sheet(&sequence.tracks[track], sequence.rate, 6);
        assert_eq!(
            column,
            vec![Some(Some(a)), None, Some(None), None, Some(Some(b)), None]
        );
    }

    #[test]
    fn exposure_sheets_write_and_read_as_csv() {
        let picture = EntityId::new();
        let (a, b) = (EntityId::new(), EntityId::new());
        let mut sequence = Sequence::new(Rate::new(12, 1), MIX_RATE);
        sequence = with_folder(&sequence, "Body, front");
        let track = folders(&sequence)[0];
        sequence = assign(&sequence, track, 0, Some(a), picture, 5);
        sequence = assign(&sequence, track, 2, None, picture, 5);
        sequence = assign(&sequence, track, 3, Some(b), picture, 5);
        let name = |cel: EntityId| {
            if cel == a {
                "1".to_owned()
            } else {
                "2".to_owned()
            }
        };
        let text = sheet_csv(&sequence, 5, name);
        assert_eq!(text, "Frame,\"Body, front\"\n1,1\n2,\n3,x\n4,2\n5,\n");
        // Read back onto an empty folder: the same keys.
        let mut empty = Sequence::new(Rate::new(12, 1), MIX_RATE);
        empty = with_folder(&empty, "Body, front");
        let find = |name: &str| match name {
            "1" => Some(a),
            "2" => Some(b),
            _ => None,
        };
        let (read, length) = from_sheet_csv(&empty, 5, &text, picture, find);
        assert_eq!(length, 5);
        assert_eq!(
            sheet(&read.tracks[track], read.rate, 5),
            sheet(&sequence.tracks[track], sequence.rate, 5)
        );
        // A longer sheet makes the timeline longer.
        let (_, longer) = from_sheet_csv(&empty, 2, &text, picture, find);
        assert_eq!(longer, 5);
    }

    #[test]
    fn a_movie_comes_in_as_cels_one_frame_each() {
        let mut arena = Arena::new();
        let (mut edit, picture) = crate::doc::create(&crate::doc::NewPicture {
            name: "Walk".to_owned(),
            width: 8,
            height: 8,
            ..crate::doc::NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        // Three frames, the last a different size (fitted in).
        let frame = |w: u32, h: u32, shade: u8| {
            sue_core::RasterTile2D::from_bytes(
                w,
                h,
                sue_core::PixelFormat::Rgba8,
                [shade, 0, 0, 255].repeat((w * h) as usize),
            )
            .unwrap()
        };
        let frames = [frame(8, 8, 10), frame(8, 8, 20), frame(16, 8, 30)];
        let (mut edit, track) =
            import_frames(&arena, picture, None, "Clip", &frames, 12.0).unwrap();
        edit.apply(&mut arena).unwrap();
        let sequence = timeline(&arena, picture).unwrap().clone();
        assert_eq!(fps(&sequence), 12.0);
        assert_eq!(length(&arena, picture), 3);
        let cels: Vec<EntityId> = (0..3)
            .filter_map(|f| cel_at(&sequence.tracks[track], sequence.rate, f))
            .collect();
        assert_eq!(cels.len(), 3);
        let shade = |cel: EntityId, y: usize| {
            crate::doc::raster(&arena, cel).unwrap().bytes().unwrap()[y * 8 * 4]
        };
        assert_eq!(shade(cels[1], 4), 20);
        assert_eq!(shade(cels[2], 0), 0, "the wide frame is letterboxed");
        assert_eq!(shade(cels[2], 4), 30);
        edit.revert(&mut arena).unwrap();
        assert!(
            timeline(&arena, picture).is_none(),
            "undo takes it all away"
        );
    }

    #[test]
    fn audio_tracks_import_move_mix_and_go() {
        let mut arena = Arena::new();
        let entity = Entity::empty("Walk", sue_model::class::IMAGE);
        let id = entity.id();
        arena.insert(entity);
        let mut edit = new_timeline(id, 24, 48);
        edit.apply(&mut arena).unwrap();
        let sequence = timeline(&arena, id).unwrap().clone();
        // Half a second of a steady tone at 24 kHz, from frame 12.
        let tone = sue_core::PCMFloat1D::new(24_000, 1, vec![0.25; 12_000]).unwrap();
        let (mut edit, sound) = add_audio(id, &sequence, "Tone", tone, 12);
        edit.apply(&mut arena).unwrap();
        let sequence = timeline(&arena, id).unwrap().clone();
        assert_eq!(audio_tracks(&sequence).len(), 1);
        assert_eq!(folders(&sequence).len(), 1, "the folders stay");
        assert!(arena.get_by_id(sound).is_ok(), "the sound is an entity");
        let mixed = mix(&arena, &sequence).unwrap();
        assert_eq!((mixed.sample_rate(), mixed.channels()), (MIX_RATE, 2));
        let at = |pcm: &sue_core::PCMFloat1D, seconds: f64| {
            pcm.samples()[(seconds * f64::from(MIX_RATE)) as usize * 2]
        };
        assert_eq!(at(&mixed, 0.25), 0.0, "silent before frame 12");
        assert!(
            (at(&mixed, 0.75) - 0.25).abs() < 1e-3,
            "the tone from half a second"
        );
        // Moved to frame 0 it starts at once; muted or gone, silence.
        let track = audio_tracks(&sequence)[0];
        let moved = audio_moved(&sequence, track, 0);
        assert!((at(&mix(&arena, &moved).unwrap(), 0.25) - 0.25).abs() < 1e-3);
        let gone = without_track(&sequence, track);
        assert!(mix(&arena, &gone).is_none());
        edit.revert(&mut arena).unwrap();
        assert!(arena.get_by_id(sound).is_err(), "undo takes it away");
    }

    #[test]
    fn edit_timeline_keeps_keys_on_their_frames() {
        let mut arena = Arena::new();
        let entity = Entity::empty("Walk", sue_model::class::IMAGE);
        let id = entity.id();
        arena.insert(entity);
        let mut edit = new_timeline(id, 12, 24);
        edit.apply(&mut arena).unwrap();
        let mut sequence = timeline(&arena, id).unwrap().clone();
        let (one, two, three) = (EntityId::new(), EntityId::new(), EntityId::new());
        sequence = assign(&sequence, 0, 0, Some(one), id, 24);
        sequence = assign(&sequence, 0, 6, Some(two), id, 24);
        sequence = assign(&sequence, 0, 20, Some(three), id, 24);
        let mut set_it = set(id, sequence, "Keys");
        set_it.apply(&mut arena).unwrap();
        // 24 frames a second, 12 frames long: the keys stay at frames 0
        // and 6; the one at 20 is past the end.
        let mut edit = edit_timeline(&arena, id, 24, 12).unwrap();
        edit.apply(&mut arena).unwrap();
        let sequence = timeline(&arena, id).unwrap();
        assert_eq!(fps(sequence), 24.0);
        assert_eq!(length(&arena, id), 12);
        assert_eq!(
            keys(&sequence.tracks[0], sequence.rate),
            vec![(0, Some(one)), (6, Some(two))]
        );
        edit.revert(&mut arena).unwrap();
        assert_eq!(length(&arena, id), 24, "undone");
    }

    #[test]
    fn onion_skin_settings_show_as_many_cels_as_asked_fading_away() {
        let mut arena = Arena::new();
        let entity = Entity::empty("Walk", sue_model::class::IMAGE);
        let id = entity.id();
        arena.insert(entity);
        let mut edit = new_timeline(id, 12, 12);
        edit.apply(&mut arena).unwrap();
        let mut sequence = timeline(&arena, id).unwrap().clone();
        let layer = |arena: &mut Arena| {
            let entity = Entity::empty("Cel", "sue.layer");
            let id = entity.id();
            arena.insert_keep_id(entity).unwrap();
            id
        };
        let cels: Vec<EntityId> = (0..6).map(|_| layer(&mut arena)).collect();
        for (index, cel) in cels.iter().enumerate() {
            sequence = assign(&sequence, 0, index as u32 * 2, Some(*cel), id, 12);
        }
        // At cel 3 (frame 6): two before and one after, fading by a step.
        let onion = Onion {
            before: 2,
            after: 1,
            mode: 1,
            opacity: Some((0.6, 0.2)),
            ..Onion::default()
        };
        let at = view(&sequence, 6, Some(&onion));
        let shown: Vec<(EntityId, f32)> = at
            .onion
            .iter()
            .map(|skin| (skin.cel, skin.opacity))
            .collect();
        assert_eq!(shown.len(), 3);
        assert_eq!(shown[0].0, cels[2]);
        assert!((shown[0].1 - 0.6).abs() < 1e-6);
        assert_eq!(shown[1].0, cels[1]);
        assert!((shown[1].1 - 0.4).abs() < 1e-6, "one step fainter");
        assert_eq!(shown[2].0, cels[4]);
        assert!(
            at.onion.iter().all(|skin| (skin.tint - 0.5).abs() < 1e-6),
            "half color"
        );
        // The settings read back from their text, and none shows fully.
        assert_eq!(Onion::from_text(&onion.to_text()), Some(onion));
        let full = Onion {
            opacity: None,
            ..onion
        };
        assert_eq!(Onion::from_text(&full.to_text()), Some(full));
        assert_eq!(full.opacity_at(3), 1.0);
        // A light table shows its layers in their own colors.
        let reference = layer(&mut arena);
        let traced = view(&sequence, 6, None).with_light_table(
            &[Traced {
                layer: reference,
                place: None,
                only: false,
            }],
            0.5,
            None,
        );
        assert_eq!(traced.onion.len(), 1);
        assert_eq!(traced.onion[0].tint, 0.0);
        let mut put = register(cels[3], reference);
        put.apply(&mut arena).unwrap();
        assert_eq!(light_table(&arena, cels[3]), vec![reference]);
        let mut off = deregister(cels[3], &[reference]);
        off.apply(&mut arena).unwrap();
        assert!(light_table(&arena, cels[3]).is_empty());
        off.revert(&mut arena).unwrap();
        assert_eq!(
            light_table(&arena, cels[3]),
            vec![reference],
            "and back on undo"
        );
    }

    #[test]
    fn folders_show_one_cel_at_a_time_with_onion_skin() {
        let mut arena = Arena::new();
        let picture = Entity::empty("Walk", sue_model::class::IMAGE);
        let id = picture.id();
        arena.insert(picture);
        let mut edit = new_timeline(id, 12, 8);
        edit.apply(&mut arena).unwrap();
        let sequence = timeline(&arena, id).unwrap().clone();
        assert_eq!(fps(&sequence), 12.0);
        assert_eq!(length(&arena, id), 8);
        assert_eq!(folders(&sequence), vec![0]);
        // Cels 1 at frame 0, 2 at frame 3, blank at frame 6.
        let (one, two) = (EntityId::new(), EntityId::new());
        let sequence = assign(&sequence, 0, 0, Some(one), id, 8);
        let sequence = assign(&sequence, 0, 3, Some(two), id, 8);
        let sequence = assign(&sequence, 0, 6, None, id, 8);
        let track = &sequence.tracks[0];
        assert_eq!(cel_at(track, sequence.rate, 2), Some(one));
        assert_eq!(cel_at(track, sequence.rate, 3), Some(two));
        assert_eq!(cel_at(track, sequence.rate, 7), None, "blank");
        assert_eq!(
            track.clips()[0].duration,
            Time::from_frames(3, sequence.rate)
        );
        let at = view(&sequence, 1, None);
        assert!(at.hidden.contains(&two) && !at.hidden.contains(&one));
        // Onion skin: the next cel shows faintly in green.
        let onion = Onion::default();
        let at = view(&sequence, 1, Some(&onion));
        assert_eq!(at.onion.len(), 1);
        assert_eq!(at.onion[0].cel, two);
        assert_eq!(at.onion[0].color, [0.0, 1.0, 0.0]);
        assert!((at.onion[0].opacity - 0.8).abs() < 1e-6);
        // Insert a frame at 1: later keys move one on; deleting moves back.
        let longer = shift(&sequence, 1, 1, 9);
        assert_eq!(cel_at(&longer.tracks[0], longer.rate, 3), Some(one));
        assert_eq!(cel_at(&longer.tracks[0], longer.rate, 4), Some(two));
        let back = shift(&longer, 1, -1, 8);
        assert_eq!(
            keys(&back.tracks[0], back.rate),
            keys(&sequence.tracks[0], sequence.rate)
        );
        let fewer = unassign(&sequence, 0, 3, 8);
        assert_eq!(cel_at(&fewer.tracks[0], fewer.rate, 4), Some(one));
        // A second folder.
        assert_eq!(folders(&with_folder(&sequence, "Folder 2")).len(), 2);
    }

    #[test]
    fn a_timeline_saves_with_its_picture() {
        let mut arena = Arena::new();
        let picture = Entity::empty("Walk", sue_model::class::IMAGE);
        let id = picture.id();
        arena.insert(picture);
        let mut edit = new_timeline(id, 24, 12);
        edit.apply(&mut arena).unwrap();
        let cel = EntityId::new();
        let sequence = assign(timeline(&arena, id).unwrap(), 0, 5, Some(cel), id, 12);
        let mut edit = set(id, sequence.clone(), "Assign");
        edit.apply(&mut arena).unwrap();
        let mut file = sue_core::SueFile::new();
        file.set_arena(&arena);
        let loaded = sue_core::SueFile::from_bytes(&file.to_bytes())
            .unwrap()
            .arena(&sue_model::registry())
            .unwrap();
        assert_eq!(timeline(&loaded, id), Some(&sequence));
    }
}
