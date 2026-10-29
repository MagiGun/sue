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

/// The layers on `cel`'s light table, in the order they were registered.
#[must_use]
pub fn light_table(arena: &Arena, cel: EntityId) -> Vec<EntityId> {
    arena.relations().targets(cel, LIGHT_TABLE)
}

/// The edit that puts `layer` on `cel`'s light table.
#[must_use]
pub fn register(cel: EntityId, layer: EntityId) -> Edit {
    Edit::new("Register to Light Table").then(sue_core::txn::Relate::new(sue_core::Relation::new(
        cel,
        LIGHT_TABLE,
        layer,
    )))
}

/// The edit that takes `layers` off `cel`'s light table.
#[must_use]
pub fn deregister(cel: EntityId, layers: &[EntityId]) -> Edit {
    layers
        .iter()
        .fold(Edit::new("Deregister from Light Table"), |edit, layer| {
            edit.then(sue_core::txn::Unrelate::new(cel, LIGHT_TABLE, *layer))
        })
}

impl FrameView {
    /// The same with `layers` (a light table) shown at `opacity` in their
    /// own colors when the frame hides them.
    #[must_use]
    pub fn with_light_table(mut self, layers: &[EntityId], opacity: f32) -> Self {
        for layer in layers {
            if !self.onion.iter().any(|skin| skin.cel == *layer) {
                self.onion.push(Skin {
                    cel: *layer,
                    color: [0.0; 3],
                    tint: 0.0,
                    opacity,
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
        Edit::new("Edit Timeline")
            .then(SetExtension::set(picture, retimed))
            .then(SetProperty::new(picture, LENGTH, i64::from(length))),
    )
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
        "Animation Folder 1",
    );
    Edit::new("New Timeline")
        .then(SetExtension::set(picture, sequence))
        .then(SetProperty::new(picture, LENGTH, i64::from(length.max(1))))
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
        let traced = view(&sequence, 6, None).with_light_table(&[reference], 0.5);
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
