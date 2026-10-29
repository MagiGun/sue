//! Animation, as Clip Studio Paint's timeline and animation folders work.
//! A picture's timeline is the shared [`Sequence`] on the picture (the same
//! model a video editor or a DAW edits): its rate is the frame rate, each
//! track of kind [`CELS`] is an animation folder, and each clip is a key
//! that shows one cel (a layer of the picture, by id) from its frame until
//! the next key. A blank key shows nothing. The timeline's length in
//! frames is the picture's [`LENGTH`].
//!
//! At any frame, the cels no folder shows are hidden; onion skin shows the
//! cels just before (blue) and after (green) faintly.

use std::collections::HashSet;

use sue_core::txn::{Batch, SetExtension, SetProperty};
use sue_core::{Arena, EntityId, Rate, Time, Value};
use sue_model::{Clip, Sequence, Track};

use crate::composite::FrameView;

/// The kind of track that is an animation folder.
pub const CELS: &str = "sue.paint.cels";
/// A picture's timeline length in frames (`Int`).
pub const LENGTH: &str = "anim.length";
/// A key that shows nothing (a clip property).
const BLANK: &str = "anim.blank";

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
/// with onion skin, each folder's cels just before and after show faintly.
#[must_use]
pub fn view(sequence: &Sequence, frame: u32, onion: bool) -> FrameView {
    let mut cels = HashSet::new();
    let mut shown = HashSet::new();
    let mut faint = Vec::new();
    for index in folders(sequence) {
        let track = &sequence.tracks[index];
        let keys = keys(track, sequence.rate);
        cels.extend(keys.iter().filter_map(|(_, cel)| *cel));
        let now = cel_at(track, sequence.rate, frame);
        shown.extend(now);
        if onion {
            let current = keys.iter().rposition(|(at, _)| *at <= frame);
            let before = current
                .and_then(|at| at.checked_sub(1))
                .and_then(|at| keys[at].1);
            let after = keys
                .iter()
                .find(|(at, _)| *at > frame)
                .and_then(|(_, cel)| *cel);
            if let Some(cel) = before.filter(|cel| Some(*cel) != now) {
                faint.push((cel, [0.25, 0.45, 1.0], 0.35));
            }
            if let Some(cel) = after.filter(|cel| Some(*cel) != now) {
                faint.push((cel, [0.2, 0.75, 0.35], 0.35));
            }
        }
    }
    FrameView {
        hidden: cels.difference(&shown).copied().collect(),
        onion: faint,
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
        let at = view(&sequence, 1, false);
        assert!(at.hidden.contains(&two) && !at.hidden.contains(&one));
        // Onion skin: the next cel shows faintly in green.
        let at = view(&sequence, 1, true);
        assert_eq!(at.onion.len(), 1);
        assert_eq!(at.onion[0].0, two);
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
