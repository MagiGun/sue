//! Editing the timeline as Clip Studio Paint does: a selection of frames
//! on several tracks, a clipboard that pastes the same cels again (a
//! reference, never a new layer), cut, copy, paste, delete and move,
//! insert and delete frames on the picked tracks only, clips that merge
//! and split and that start or end at a frame, and track labels.
//!
//! A track of an animation folder ([`super::CELS`]) holds keys: each
//! shows its cel until the next key. Every edit here keeps what shows
//! after the frames it changes, so editing a range never changes the
//! frames past it.

use sue_core::txn::{SetExtension, SetProperty};
use sue_core::{Arena, EntityId, Rate, Time, Value};
use sue_model::{Animation, Clip, Sequence, Track};

use super::{BLANK, CELS, Edit, LENGTH, folders, keys, length, tidy, timeline};

/// A part of the timeline: frames `start` up to `end` (not included) on
/// some tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The tracks, by index, top first.
    pub tracks: Vec<usize>,
    /// The first frame.
    pub start: u32,
    /// The frame after the last.
    pub end: u32,
}

impl Selection {
    /// Frames `start` to `end` (either way round, both included) on
    /// `tracks`, as a drag across the timeline picks them.
    #[must_use]
    pub fn dragged(tracks: Vec<usize>, from: u32, to: u32) -> Self {
        Self {
            tracks,
            start: from.min(to),
            end: from.max(to) + 1,
        }
    }

    /// How many frames it holds.
    #[must_use]
    pub fn span(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// `true` if it holds frame `frame` of track `track`.
    #[must_use]
    pub fn contains(&self, track: usize, frame: u32) -> bool {
        self.tracks.contains(&track) && (self.start..self.end).contains(&frame)
    }
}

/// What Cut or Copy took off the timeline: for each track, its keys by
/// frame from the start of the range (`None` a blank), the first at 0
/// with what showed there. Pasting shows the same cels again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clipboard {
    /// How many frames it covers.
    pub span: u32,
    /// The keys of each track copied, top first.
    pub rows: Vec<Vec<(u32, Option<EntityId>)>>,
}

impl Clipboard {
    /// `true` if it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() || self.span == 0
    }
}

/// The clips of `track` by the frame each starts on, first first.
fn by_frame(track: &Track, rate: Rate) -> Vec<(u32, Clip)> {
    let mut out: Vec<(u32, Clip)> = track
        .clips()
        .iter()
        .map(|clip| (clip.start.frame(rate).max(0) as u32, clip.clone()))
        .collect();
    out.sort_by_key(|(frame, _)| *frame);
    out
}

/// A key at `frame` showing `cel` (`None`: a blank one).
fn key_clip(cel: Option<EntityId>, frame: u32, blank_source: EntityId, rate: Rate) -> Clip {
    let mut clip = Clip::new(
        cel.unwrap_or(blank_source),
        Time::from_frames(i64::from(frame), rate),
        Time::from_frames(1, rate),
    );
    if cel.is_none() {
        clip.properties.set(BLANK, true);
    }
    clip
}

/// `track` (of an animation folder) with its keys in frames `start` up
/// to `end` replaced by `new` (frame and key), keeping what showed at
/// `end` and after.
fn rewrite(
    track: &Track,
    rate: Rate,
    length: u32,
    [start, end]: [u32; 2],
    new: Vec<(u32, Clip)>,
    blank_source: EntityId,
) -> Track {
    let old = by_frame(track, rate);
    let at_end = old.iter().rev().find(|(frame, _)| *frame <= end).cloned();
    let mut clips: Vec<(u32, Clip)> = old
        .into_iter()
        .filter(|(frame, _)| *frame < start || *frame >= end)
        .collect();
    clips.extend(
        new.into_iter()
            .filter(|(frame, _)| (start..end).contains(frame)),
    );
    if end < length && !clips.iter().any(|(frame, _)| *frame == end) {
        let changed_before = clips.iter().any(|(frame, _)| (start..end).contains(frame));
        match at_end {
            Some((_, mut clip)) => {
                clip.start = Time::from_frames(i64::from(end), rate);
                clips.push((end, clip));
            }
            None if changed_before => clips.push((end, key_clip(None, end, blank_source, rate))),
            None => {}
        }
    }
    let mut out = Track::new(track.name.clone(), track.kind.clone());
    out.muted = track.muted;
    out.locked = track.locked;
    out.properties = track.properties.clone();
    for (frame, mut clip) in clips {
        clip.start = Time::from_frames(i64::from(frame), rate);
        out.insert(clip);
    }
    tidy(&out, rate, length)
}

/// What `selection` holds, as a clipboard (Copy).
#[must_use]
pub fn copy(sequence: &Sequence, selection: &Selection) -> Clipboard {
    let rate = sequence.rate;
    let rows = selection
        .tracks
        .iter()
        .filter_map(|index| sequence.tracks.get(*index))
        .filter(|track| track.kind == CELS)
        .map(|track| {
            let all = keys(track, rate);
            let mut row: Vec<(u32, Option<EntityId>)> = Vec::new();
            // What shows at the start, held from before.
            if let Some((frame, cel)) = all
                .iter()
                .rev()
                .find(|(frame, _)| *frame <= selection.start)
                && *frame < selection.start
            {
                row.push((0, *cel));
            }
            row.extend(
                all.iter()
                    .filter(|(frame, _)| (selection.start..selection.end).contains(frame))
                    .map(|(frame, cel)| (frame - selection.start, *cel)),
            );
            row
        })
        .collect();
    Clipboard {
        span: selection.span(),
        rows,
    }
}

/// `sequence` with the frames of `selection` emptied: a blank where
/// something showed, what came after kept (Delete). `blank_source` is
/// the picture.
#[must_use]
pub fn clear(
    sequence: &Sequence,
    selection: &Selection,
    length: u32,
    blank_source: EntityId,
) -> Sequence {
    let mut out = sequence.clone();
    let rate = sequence.rate;
    for index in &selection.tracks {
        let Some(track) = sequence
            .tracks
            .get(*index)
            .filter(|track| track.kind == CELS)
        else {
            continue;
        };
        let showing = super::cel_at(track, rate, selection.start).is_some();
        let new = if showing {
            vec![(
                selection.start,
                key_clip(None, selection.start, blank_source, rate),
            )]
        } else {
            Vec::new()
        };
        out.tracks[*index] = rewrite(
            track,
            rate,
            length,
            [selection.start, selection.end],
            new,
            blank_source,
        );
    }
    out
}

/// What `selection` holds, and `sequence` with it emptied (Cut).
#[must_use]
pub fn cut(
    sequence: &Sequence,
    selection: &Selection,
    length: u32,
    blank_source: EntityId,
) -> (Sequence, Clipboard) {
    (
        clear(sequence, selection, length, blank_source),
        copy(sequence, selection),
    )
}

/// `sequence` with `clipboard` laid over frames from `frame` on, its rows
/// on `tracks` in turn (Paste). A cel goes only where `may` lets it (a
/// track of another animation folder cannot show it: it pastes as a
/// blank). The timeline grows to hold it; its new length is returned.
#[must_use]
pub fn paste(
    sequence: &Sequence,
    clipboard: &Clipboard,
    tracks: &[usize],
    frame: u32,
    length: u32,
    blank_source: EntityId,
    may: impl Fn(usize, EntityId) -> bool,
) -> (Sequence, u32) {
    let mut out = sequence.clone();
    let rate = sequence.rate;
    let end = frame + clipboard.span;
    let length = length.max(end);
    for (row, index) in clipboard.rows.iter().zip(tracks) {
        let Some(track) = sequence
            .tracks
            .get(*index)
            .filter(|track| track.kind == CELS)
        else {
            continue;
        };
        let new = row
            .iter()
            .map(|(offset, cel)| {
                let at = frame + offset;
                let cel = cel.filter(|cel| may(*index, *cel));
                (at, key_clip(cel, at, blank_source, rate))
            })
            .collect();
        out.tracks[*index] = rewrite(track, rate, length, [frame, end], new, blank_source);
    }
    (out, length)
}

/// `sequence` with what `selection` holds moved to start at `to` on the
/// same tracks, the frames it leaves emptied (a drag of picked cels).
/// The timeline grows to hold it; its new length is returned.
#[must_use]
pub fn move_range(
    sequence: &Sequence,
    selection: &Selection,
    to: u32,
    length: u32,
    blank_source: EntityId,
) -> (Sequence, u32) {
    let (emptied, clipboard) = cut(sequence, selection, length, blank_source);
    paste(
        &emptied,
        &clipboard,
        &selection.tracks,
        to,
        length,
        blank_source,
        |_, _| true,
    )
}

/// The key `track` holds in effect at `frame`: its frame and clip.
fn key_at(track: &Track, rate: Rate, frame: u32) -> Option<(u32, Clip)> {
    by_frame(track, rate)
        .into_iter()
        .rev()
        .find(|(at, _)| *at <= frame)
}

/// `sequence` with the clip of track `track` under `frame` split there
/// in two (Split clip): a key there showing the same cel. `None` when a
/// clip starts there already or none is under it.
#[must_use]
pub fn split_clip(sequence: &Sequence, track: usize, frame: u32, length: u32) -> Option<Sequence> {
    let rate = sequence.rate;
    let old = sequence.tracks.get(track)?;
    if old.kind == super::setup::LAYER {
        let mut out = sequence.clone();
        let index = old.clips().iter().position(|clip| {
            clip.start.frame(rate) < i64::from(frame) && clip.end().frame(rate) > i64::from(frame)
        })?;
        out.tracks[track].split(index, Time::from_frames(i64::from(frame), rate))?;
        return Some(out);
    }
    let (at, mut clip) = key_at(old, rate, frame)?;
    if at == frame {
        return None;
    }
    clip.start = Time::from_frames(i64::from(frame), rate);
    let mut track_out = old.clone();
    track_out.insert(clip);
    let mut out = sequence.clone();
    out.tracks[track] = tidy(&track_out, rate, length);
    Some(out)
}

/// `sequence` with the clips of track `track` from `start` up to `end`
/// made one (Merge clips): the first clip's cel shows through them all,
/// what follows kept. `None` when there is nothing to merge.
#[must_use]
pub fn merge_clips(
    sequence: &Sequence,
    track: usize,
    start: u32,
    end: u32,
    length: u32,
    blank_source: EntityId,
) -> Option<Sequence> {
    let rate = sequence.rate;
    let old = sequence.tracks.get(track)?;
    if old.kind == super::setup::LAYER {
        let clips: Vec<&Clip> = old
            .clips()
            .iter()
            .filter(|clip| {
                clip.start.frame(rate) < i64::from(end) && clip.end().frame(rate) > i64::from(start)
            })
            .collect();
        if clips.len() < 2 {
            return None;
        }
        let first = clips.iter().map(|clip| clip.start).min()?;
        let last = clips.iter().map(|clip| clip.end()).max()?;
        let mut merged = clips[0].clone();
        merged.start = first;
        merged.duration = last - first;
        let mut out_track = Track::new(old.name.clone(), old.kind.clone());
        out_track.properties = old.properties.clone();
        out_track.muted = old.muted;
        out_track.locked = old.locked;
        for clip in old.clips().iter().filter(|clip| !clips.contains(clip)) {
            out_track.insert(clip.clone());
        }
        out_track.insert(merged);
        let mut out = sequence.clone();
        out.tracks[track] = out_track;
        return Some(out);
    }
    let (at, first) = key_at(old, rate, start)?;
    let inside = by_frame(old, rate)
        .into_iter()
        .filter(|(frame, _)| *frame > at && *frame < end)
        .count();
    if inside == 0 {
        return None;
    }
    let mut out = sequence.clone();
    out.tracks[track] = rewrite(
        old,
        rate,
        length,
        [at, end],
        vec![(at, first)],
        blank_source,
    );
    Some(out)
}

/// `sequence` with the clip of track `track` under `frame` starting at
/// `frame` (Set as first displayed frame): the frames before it in the
/// clip go blank. `None` when it starts there already.
#[must_use]
pub fn set_first_displayed(
    sequence: &Sequence,
    track: usize,
    frame: u32,
    length: u32,
    blank_source: EntityId,
) -> Option<Sequence> {
    let rate = sequence.rate;
    let old = sequence.tracks.get(track)?;
    if old.kind == super::setup::LAYER {
        return trimmed(sequence, track, frame, true);
    }
    let (at, mut clip) = key_at(old, rate, frame)?;
    if at == frame || clip.properties.get(BLANK).and_then(Value::as_bool) == Some(true) {
        return None;
    }
    clip.start = Time::from_frames(i64::from(frame), rate);
    let new = vec![(at, key_clip(None, at, blank_source, rate)), (frame, clip)];
    let mut out = sequence.clone();
    out.tracks[track] = rewrite(old, rate, length, [at, frame + 1], new, blank_source);
    Some(out)
}

/// `sequence` with the clip of track `track` under `frame` ending at
/// `frame` (Set as last displayed frame): a blank right after it. `None`
/// when it ends there already.
#[must_use]
pub fn set_last_displayed(
    sequence: &Sequence,
    track: usize,
    frame: u32,
    length: u32,
    blank_source: EntityId,
) -> Option<Sequence> {
    let rate = sequence.rate;
    let old = sequence.tracks.get(track)?;
    if old.kind == super::setup::LAYER {
        return trimmed(sequence, track, frame, false);
    }
    let (_, clip) = key_at(old, rate, frame)?;
    if clip.properties.get(BLANK).and_then(Value::as_bool) == Some(true) || frame + 1 >= length {
        return None;
    }
    let next = frame + 1;
    if by_frame(old, rate).iter().any(|(at, _)| *at == next) {
        return None;
    }
    let mut track_out = old.clone();
    track_out.insert(key_clip(None, next, blank_source, rate));
    let mut out = sequence.clone();
    out.tracks[track] = tidy(&track_out, rate, length);
    Some(out)
}

/// A layer track's clip under `frame` trimmed to start (`start`) or end
/// at `frame`.
fn trimmed(sequence: &Sequence, track: usize, frame: u32, start: bool) -> Option<Sequence> {
    let rate = sequence.rate;
    let old = sequence.tracks.get(track)?;
    let at = Time::from_frames(i64::from(frame), rate);
    let index = old
        .clips()
        .iter()
        .position(|clip| clip.start <= at && clip.end() > at)?;
    let mut clip = old.clips()[index].clone();
    let one = Time::from_frames(1, rate);
    if start {
        if clip.start == at {
            return None;
        }
        clip.duration = clip.end() - at;
        clip.start = at;
    } else {
        let end = at + one;
        if clip.end() == end {
            return None;
        }
        clip.duration = end - clip.start;
    }
    let mut out = sequence.clone();
    out.tracks[track].remove(index);
    out.tracks[track].insert(clip);
    Some(out)
}

/// `sequence` with the keys after `frame` on the tracks `only` (every
/// track when `None`) moved `by` frames: Insert Frame (`by` above 0) and
/// Delete Frame (below 0). A key moved onto or before `frame` by a
/// deletion goes. The clip of a layer track under `frame` grows or
/// shrinks with it.
#[must_use]
pub fn shift_tracks(
    sequence: &Sequence,
    frame: u32,
    by: i64,
    length: u32,
    only: Option<&[usize]>,
) -> Sequence {
    let mut out = sequence.clone();
    let rate = out.rate;
    let picked = |index: usize| only.is_none_or(|tracks| tracks.contains(&index));
    for index in folders(sequence).into_iter().filter(|index| picked(*index)) {
        let old = &sequence.tracks[index];
        let mut moved = Track::new(old.name.clone(), old.kind.clone());
        moved.properties = old.properties.clone();
        moved.muted = old.muted;
        moved.locked = old.locked;
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
    let after = Time::from_frames(i64::from(frame), rate);
    let by_time = Time::from_frames(by, rate);
    for index in super::setup::layer_tracks(sequence)
        .into_iter()
        .filter(|index| picked(*index))
    {
        let old = &sequence.tracks[index];
        let mut moved = Track::new(old.name.clone(), old.kind.clone());
        moved.properties = old.properties.clone();
        moved.muted = old.muted;
        moved.locked = old.locked;
        for clip in old.clips() {
            let mut clip = clip.clone();
            if clip.start > after {
                clip.start = Time((clip.start + by_time).flicks().max(after.flicks()));
            } else if clip.end() > after {
                let one = Time::from_frames(1, rate).flicks();
                clip.duration = Time((clip.duration + by_time).flicks().max(one));
            }
            moved.insert(clip);
        }
        out.tracks[index] = moved;
    }
    out
}

/// The edit that inserts (`count` above 0) or deletes (below 0) frames
/// right after `frame` (Insert Frame, Delete Frame): on the picked
/// tracks only when `only` names them ("Selected layers only", the
/// timeline keeps its length), else on every track, the timeline
/// growing or shrinking with them. Layer and camera keyframes of the
/// tracks it changes move too. `None` without a timeline.
#[must_use]
pub fn insert_frames(
    arena: &Arena,
    picture: EntityId,
    frame: u32,
    count: i64,
    only: Option<&[usize]>,
) -> Option<Edit> {
    let sequence = timeline(arena, picture)?;
    let length = length(arena, picture);
    let new_length = if only.is_some() {
        length
    } else {
        (i64::from(length) + count).clamp(1, 100_000) as u32
    };
    let shifted = shift_tracks(sequence, frame, count, new_length, only);
    let label = if count > 0 {
        tr!("insert-frame")
    } else {
        tr!("delete-frame")
    };
    let mut edit = Edit::new(label).then(SetExtension::set(picture, shifted));
    if new_length != length {
        edit = edit.then(SetProperty::new(picture, LENGTH, i64::from(new_length)));
    }
    // The keyframes of what moved: each keyed layer or camera on a track
    // that moved, or every one.
    let moved: Option<Vec<EntityId>> = only.map(|tracks| {
        tracks
            .iter()
            .filter_map(|index| sequence.tracks.get(*index))
            .filter_map(|track| {
                super::setup::track_layer(track).or_else(|| super::track_folder(track))
            })
            .collect()
    });
    for info in crate::doc::layers(arena, picture) {
        if moved.as_ref().is_some_and(|ids| !ids.contains(&info.id)) {
            continue;
        }
        let Some(animation) = arena
            .get_by_id(info.id)
            .ok()
            .and_then(|entity| entity.ext::<Animation>())
        else {
            continue;
        };
        let shifted = super::motion::shifted(animation, frame, count, sequence.rate);
        if shifted != *animation {
            edit = edit.then(SetExtension::set(info.id, shifted));
        }
    }
    Some(edit)
}

/// `sequence` with `cels` shown in turn on track `track` from frame
/// `start`, each for `every` frames (Assign multiple cels); what showed
/// after the last is kept. The timeline grows to hold them; its new
/// length is returned.
#[must_use]
pub fn assign_in_turn(
    sequence: &Sequence,
    track: usize,
    start: u32,
    cels: &[EntityId],
    every: u32,
    length: u32,
    blank_source: EntityId,
) -> (Sequence, u32) {
    let every = every.max(1);
    let end = start + every * cels.len() as u32;
    let length = length.max(end);
    let mut out = sequence.clone();
    let rate = sequence.rate;
    if let Some(old) = sequence.tracks.get(track).filter(|old| old.kind == CELS) {
        let new = cels
            .iter()
            .enumerate()
            .map(|(index, cel)| {
                let at = start + every * index as u32;
                (at, key_clip(Some(*cel), at, blank_source, rate))
            })
            .collect();
        out.tracks[track] = rewrite(old, rate, length, [start, end], new, blank_source);
    }
    (out, length)
}

/// The kinds of track label, as Clip Studio Paint draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackLabel {
    /// An in-between to draw (an open circle).
    InBetween,
    /// An in-between drawn (a filled circle).
    InBetweenDone,
    /// A reverse sheet mark.
    ReverseSheet,
}

/// A track's labels: `frame:kind` pairs joined by `;` (`Text`, a track
/// property).
pub const TRACK_LABELS: &str = "anim.track.labels";

impl TrackLabel {
    /// Every kind, in menu order.
    pub const ALL: [Self; 3] = [Self::InBetween, Self::InBetweenDone, Self::ReverseSheet];

    /// The stable name saved with the track.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InBetween => "inbetween",
            Self::InBetweenDone => "inbetween-done",
            Self::ReverseSheet => "reverse-sheet",
        }
    }

    /// The kind named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// The labels of `track` by frame, first first. Labels of a kind this
/// Sue does not know are left out (and kept, see [`with_track_label`]).
#[must_use]
pub fn track_labels(track: &Track) -> Vec<(u32, TrackLabel)> {
    let mut out: Vec<(u32, TrackLabel)> = raw_labels(track)
        .into_iter()
        .filter_map(|(frame, kind)| Some((frame, TrackLabel::from_name(&kind)?)))
        .collect();
    out.sort_by_key(|(frame, _)| *frame);
    out
}

fn raw_labels(track: &Track) -> Vec<(u32, String)> {
    track
        .properties
        .get(TRACK_LABELS)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(';')
        .filter_map(|pair| {
            let (frame, kind) = pair.split_once(':')?;
            Some((frame.trim().parse().ok()?, kind.trim().to_owned()))
        })
        .collect()
}

/// `sequence` with track `track`'s label at `frame` set to `label`
/// (`None`: taken away).
#[must_use]
pub fn with_track_label(
    sequence: &Sequence,
    track: usize,
    frame: u32,
    label: Option<TrackLabel>,
) -> Sequence {
    let mut out = sequence.clone();
    let Some(target) = out.tracks.get_mut(track) else {
        return out;
    };
    let mut labels: Vec<(u32, String)> = raw_labels(target)
        .into_iter()
        .filter(|(at, _)| *at != frame)
        .collect();
    if let Some(label) = label {
        labels.push((frame, label.name().to_owned()));
    }
    labels.sort();
    if labels.is_empty() {
        target.properties.remove(TRACK_LABELS);
    } else {
        let text: Vec<String> = labels
            .iter()
            .map(|(at, kind)| format!("{at}:{kind}"))
            .collect();
        target.properties.set(TRACK_LABELS, text.join(";"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{MIX_RATE, assign, cel_at, with_folder};
    use super::*;

    /// A timeline of `length` frames with one folder showing `keys`.
    fn timeline_of(keys: &[(u32, Option<EntityId>)], length: u32) -> Sequence {
        let mut sequence = with_folder(&Sequence::new(Rate::new(12, 1), MIX_RATE), "A");
        for (frame, cel) in keys {
            sequence = assign(&sequence, 0, *frame, *cel, EntityId::NIL, length);
        }
        sequence
    }

    fn shown(sequence: &Sequence, track: usize, length: u32) -> Vec<Option<EntityId>> {
        (0..length)
            .map(|frame| cel_at(&sequence.tracks[track], sequence.rate, frame))
            .collect()
    }

    #[test]
    fn copy_and_paste_show_the_same_cels_again_and_keep_what_follows() {
        let (a, b, c) = (EntityId::new(), EntityId::new(), EntityId::new());
        let sequence = timeline_of(&[(0, Some(a)), (2, Some(b)), (5, Some(c))], 8);
        // Frames 1 to 3: a held, then b.
        let picked = Selection::dragged(vec![0], 3, 1);
        assert_eq!((picked.start, picked.end, picked.span()), (1, 4, 3));
        let clipboard = copy(&sequence, &picked);
        assert_eq!(clipboard.rows, vec![vec![(0, Some(a)), (1, Some(b))]]);
        // Pasted at 5: a b b, then c comes back at 8 (the timeline grows).
        let (pasted, length) = paste(&sequence, &clipboard, &[0], 5, 8, EntityId::NIL, |_, _| {
            true
        });
        assert_eq!(length, 8);
        assert_eq!(
            shown(&pasted, 0, 8),
            vec![
                Some(a),
                Some(a),
                Some(b),
                Some(b),
                Some(b),
                Some(a),
                Some(b),
                Some(b)
            ]
        );
        // Pasted at 3: frames 3 to 5 change; c still starts at 6? No: c
        // started at 5, inside the paste, so what showed at 6 (c) stays.
        let (over, _) = paste(&sequence, &clipboard, &[0], 3, 8, EntityId::NIL, |_, _| {
            true
        });
        assert_eq!(
            shown(&over, 0, 8),
            vec![
                Some(a),
                Some(a),
                Some(b),
                Some(a),
                Some(b),
                Some(b),
                Some(c),
                Some(c)
            ]
        );
        // A track that may not show them pastes blanks.
        let (refused, _) = paste(&sequence, &clipboard, &[0], 5, 8, EntityId::NIL, |_, _| {
            false
        });
        assert_eq!(shown(&refused, 0, 8)[5..], [None, None, None]);
    }

    #[test]
    fn cut_leaves_empty_frames_and_move_carries_cels() {
        let (a, b) = (EntityId::new(), EntityId::new());
        let sequence = timeline_of(&[(0, Some(a)), (4, Some(b))], 8);
        let picked = Selection::dragged(vec![0], 1, 2);
        let (cut_out, clipboard) = cut(&sequence, &picked, 8, EntityId::NIL);
        assert_eq!(clipboard.rows, vec![vec![(0, Some(a))]]);
        assert_eq!(
            shown(&cut_out, 0, 8),
            vec![
                Some(a),
                None,
                None,
                Some(a),
                Some(b),
                Some(b),
                Some(b),
                Some(b)
            ]
        );
        // b (frames 2 and 3) dragged on to frame 4, over c.
        let (a, b, c) = (EntityId::new(), EntityId::new(), EntityId::new());
        let sequence = timeline_of(&[(0, Some(a)), (2, Some(b)), (4, Some(c))], 6);
        let (moved, length) = move_range(
            &sequence,
            &Selection::dragged(vec![0], 2, 3),
            4,
            6,
            EntityId::NIL,
        );
        assert_eq!(length, 6);
        assert_eq!(
            shown(&moved, 0, 6),
            vec![Some(a), Some(a), None, None, Some(b), Some(b)]
        );
    }

    #[test]
    fn clips_split_merge_and_start_or_end_at_a_frame() {
        let (a, b) = (EntityId::new(), EntityId::new());
        let sequence = timeline_of(&[(0, Some(a)), (3, Some(b))], 8);
        let split = split_clip(&sequence, 0, 1, 8).unwrap();
        assert_eq!(keys(&split.tracks[0], split.rate).len(), 3);
        assert_eq!(
            shown(&split, 0, 8),
            shown(&sequence, 0, 8),
            "nothing shows differently"
        );
        assert!(split_clip(&split, 0, 1, 8).is_none(), "a clip starts there");
        let merged = merge_clips(&split, 0, 0, 3, 8, EntityId::NIL).unwrap();
        assert_eq!(
            keys(&merged.tracks[0], merged.rate),
            keys(&sequence.tracks[0], sequence.rate)
        );
        // a and b made one: a shows through, nothing after changes.
        let one = merge_clips(&sequence, 0, 0, 5, 8, EntityId::NIL).unwrap();
        assert_eq!(
            shown(&one, 0, 8),
            vec![
                Some(a),
                Some(a),
                Some(a),
                Some(a),
                Some(a),
                Some(b),
                Some(b),
                Some(b)
            ]
        );
        let late = set_first_displayed(&sequence, 0, 1, 8, EntityId::NIL).unwrap();
        assert_eq!(shown(&late, 0, 4), vec![None, Some(a), Some(a), Some(b)]);
        let early = set_last_displayed(&sequence, 0, 1, 8, EntityId::NIL).unwrap();
        assert_eq!(shown(&early, 0, 4), vec![Some(a), Some(a), None, Some(b)]);
        assert!(
            set_last_displayed(&sequence, 0, 2, 8, EntityId::NIL).is_none(),
            "it ends there"
        );
    }

    #[test]
    fn insert_and_delete_frame_move_the_picked_tracks_only() {
        let (a, b, c, d) = (
            EntityId::new(),
            EntityId::new(),
            EntityId::new(),
            EntityId::new(),
        );
        let mut sequence = timeline_of(&[(0, Some(a)), (2, Some(b))], 6);
        sequence = with_folder(&sequence, "B");
        sequence = assign(&sequence, 1, 0, Some(c), EntityId::NIL, 6);
        sequence = assign(&sequence, 1, 2, Some(d), EntityId::NIL, 6);
        let only = shift_tracks(&sequence, 0, 2, 6, Some(&[1]));
        assert_eq!(
            keys(&only.tracks[0], only.rate),
            keys(&sequence.tracks[0], sequence.rate),
            "track A stays"
        );
        assert_eq!(
            keys(&only.tracks[1], only.rate),
            vec![(0, Some(c)), (4, Some(d))]
        );
        let every = shift_tracks(&sequence, 0, 2, 8, None);
        assert_eq!(
            keys(&every.tracks[0], every.rate),
            vec![(0, Some(a)), (4, Some(b))]
        );
        let back = shift_tracks(&every, 0, -2, 6, None);
        assert_eq!(
            keys(&back.tracks[0], back.rate),
            keys(&sequence.tracks[0], sequence.rate)
        );
    }

    #[test]
    fn several_cels_are_assigned_in_turn() {
        let (a, b, c, d) = (
            EntityId::new(),
            EntityId::new(),
            EntityId::new(),
            EntityId::new(),
        );
        let sequence = timeline_of(&[(0, Some(d))], 8);
        let (out, length) = assign_in_turn(&sequence, 0, 1, &[a, b, c], 2, 8, EntityId::NIL);
        assert_eq!(length, 8);
        assert_eq!(
            shown(&out, 0, 8),
            vec![
                Some(d),
                Some(a),
                Some(a),
                Some(b),
                Some(b),
                Some(c),
                Some(c),
                Some(d)
            ]
        );
        let (longer, length) = assign_in_turn(&sequence, 0, 4, &[a, b], 3, 8, EntityId::NIL);
        assert_eq!(length, 10);
        assert_eq!(
            shown(&longer, 0, 10)[4..],
            [Some(a), Some(a), Some(a), Some(b), Some(b), Some(b)]
        );
    }

    #[test]
    fn track_labels_set_read_and_go() {
        let sequence = timeline_of(&[], 8);
        let labelled = with_track_label(&sequence, 0, 7, Some(TrackLabel::InBetween));
        let labelled = with_track_label(&labelled, 0, 3, Some(TrackLabel::InBetweenDone));
        assert_eq!(
            track_labels(&labelled.tracks[0]),
            vec![(3, TrackLabel::InBetweenDone), (7, TrackLabel::InBetween)]
        );
        let gone = with_track_label(&labelled, 0, 3, None);
        let gone = with_track_label(&gone, 0, 7, None);
        assert_eq!(gone, sequence, "no labels leaves no property");
        for kind in TrackLabel::ALL {
            assert_eq!(TrackLabel::from_name(kind.name()), Some(kind));
        }
    }
}
