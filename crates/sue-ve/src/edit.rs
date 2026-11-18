//! The edit algebra: pure functions that change a [`Sequence`] the way an
//! editor's tools do. Each takes the sequence by `&mut` (the copy
//! [`crate::model::plan`] makes) and either does the whole edit or
//! returns an error, after which the copy is thrown away, so an edit is
//! all or nothing. Clips on a track never overlap afterwards
//! ([`check`]).
//!
//! The semantics follow Premiere Pro and DaVinci Resolve:
//!
//! - **Overwrite** replaces what is under the new clip on its track.
//! - **Insert** opens room on the target tracks and every sync locked,
//!   unlocked track, then places the clip.
//! - **Lift** leaves a gap; **extract** closes it on the target and sync
//!   locked tracks. **Ripple delete** removes clips and closes the gap,
//!   and refuses when a sync locked track has something in it.
//! - Trims: regular and ripple, **roll** (a cut moves, the length does
//!   not), **slip** (other frames in the same place) and **slide** (the
//!   clip moves, its neighbours give and take), all held by the media's
//!   handles and by the neighbours.
//!
//! Adapted from FilmCraft's edit algebra (`crates/edit/src/lib.rs` of
//! <https://github.com/storytold/filmcraft>, MIT OR Apache-2.0, Copyright
//! (c) 2026 ArtCraft Team and the FilmCraft contributors): the shape of
//! the split, clear, shift, insert, overwrite, lift, extract, trim, roll,
//! slip and slide functions and how they clamp to handles and neighbours.
//! Rewritten for Sue's shared [`Sequence`] (flicks, clip ids, link groups,
//! transitions on tracks).

use std::collections::HashMap;

use sue_core::{EntityId, Time, Value};
use sue_model::{Clip, Marker, Sequence, Track, Transition};

use crate::model::{self, key};

/// Why an edit could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// The entity holds no sequence.
    NoSequence,
    /// No clip has that id.
    NoClip(u64),
    /// No track has that index.
    NoTrack(usize),
    /// The track is locked.
    Locked,
    /// The media has no frames beyond the clip's edge to give.
    NoHandles,
    /// The clip would be shorter than a frame.
    TooShort,
    /// The two clips do not meet.
    NotAdjacent,
    /// A sync locked track (named) has a clip in the way.
    SyncConflict(String),
    /// A clip cannot go on a track of another kind.
    WrongKind,
    /// There was nothing to do.
    Nothing,
}

impl EditError {
    /// What people read.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::NoSequence => tr!("edit-no-sequence"),
            Self::NoClip(_) => tr!("edit-no-clip"),
            Self::NoTrack(_) => tr!("edit-no-track"),
            Self::Locked => tr!("edit-locked"),
            Self::NoHandles => tr!("edit-no-handles"),
            Self::TooShort => tr!("edit-too-short"),
            Self::NotAdjacent => tr!("edit-not-adjacent"),
            Self::SyncConflict(track) => tr!("edit-sync-conflict", track = track.clone()),
            Self::WrongKind => tr!("edit-wrong-kind"),
            Self::Nothing => tr!("edit-nothing"),
        }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}

impl std::error::Error for EditError {}

/// What edits need to know besides the sequence: how long each source
/// is.
pub struct EditCtx<'a> {
    /// A source's length; `None` for one without (a still, a title).
    pub media_len: &'a dyn Fn(EntityId) -> Option<Time>,
}

impl std::fmt::Debug for EditCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditCtx").finish_non_exhaustive()
    }
}

impl EditCtx<'_> {
    /// Sources of no length at all: for edits that never look.
    pub const ENDLESS: EditCtx<'static> = EditCtx {
        media_len: &|_| None,
    };
}

/// A clip's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Its start.
    In,
    /// Its end.
    Out,
}

/// One frame of the sequence: the shortest a clip may be.
#[must_use]
pub fn frame(sequence: &Sequence) -> Time {
    Time::from_frames(1, sequence.rate)
}

fn scaled(time: Time, speed: f64) -> Time {
    Time((time.flicks() as f64 * speed.abs()).round() as i64)
}

fn unscaled(time: Time, speed: f64) -> Time {
    let speed = speed.abs().max(1e-6);
    Time((time.flicks() as f64 / speed).floor() as i64)
}

fn is_hold(clip: &Clip) -> bool {
    clip.properties
        .get(key::FREEZE)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Media a clip may still show past its out point, in sequence time
/// (`Time(i64::MAX)` without limit).
fn tail_room(clip: &Clip, ctx: &EditCtx<'_>) -> Time {
    if is_hold(clip) {
        return Time(i64::MAX);
    }
    match (ctx.media_len)(clip.source) {
        Some(length) => unscaled((length - clip.source_out()).max(Time::ZERO), clip.speed),
        None => Time(i64::MAX),
    }
}

/// Media before a clip's in point, in sequence time.
fn head_room(clip: &Clip, ctx: &EditCtx<'_>) -> Time {
    if is_hold(clip) || (ctx.media_len)(clip.source).is_none() {
        return Time(i64::MAX);
    }
    unscaled(clip.source_in.max(Time::ZERO), clip.speed)
}

fn track_mut(sequence: &mut Sequence, index: usize) -> Result<&mut Track, EditError> {
    sequence
        .tracks
        .get_mut(index)
        .ok_or(EditError::NoTrack(index))
}

fn unlocked(sequence: &Sequence, index: usize) -> Result<(), EditError> {
    match sequence.tracks.get(index) {
        None => Err(EditError::NoTrack(index)),
        Some(track) if track.locked => Err(EditError::Locked),
        Some(_) => Ok(()),
    }
}

/// The track a clip is on, refusing a locked one.
fn clip_track(sequence: &Sequence, id: u64) -> Result<usize, EditError> {
    let (index, _) = sequence.find(id).ok_or(EditError::NoClip(id))?;
    unlocked(sequence, index)?;
    Ok(index)
}

/// Checks the invariants: on every track the clips are in order and do
/// not overlap, every clip lasts at least a frame and every id is unique.
///
/// # Errors
/// [`EditError::TooShort`] or [`EditError::SyncConflict`] naming the
/// track where the invariant broke.
pub fn check(sequence: &Sequence) -> Result<(), EditError> {
    let one = frame(sequence);
    let mut seen = std::collections::HashSet::new();
    for track in &sequence.tracks {
        for clip in track.clips() {
            if clip.duration < one {
                return Err(EditError::TooShort);
            }
            if clip.id != 0 && !seen.insert(clip.id) {
                return Err(EditError::SyncConflict(track.name.clone()));
            }
        }
        if track
            .clips()
            .windows(2)
            .any(|pair| pair[0].end() > pair[1].start)
            || track
                .clips()
                .first()
                .is_some_and(|clip| clip.start < Time::ZERO)
        {
            return Err(EditError::SyncConflict(track.name.clone()));
        }
    }
    Ok(())
}

// ----- Track primitives -------------------------------------------------------

/// Cuts the clip strictly holding `at` on track `index` in two. Returns
/// the right half's id. `links` maps the link groups of left halves to
/// those of right ones, so a picture and its sound cut together stay
/// linked to each other.
fn split_at(
    sequence: &mut Sequence,
    index: usize,
    at: Time,
    links: &mut HashMap<u64, u64>,
) -> Option<u64> {
    let position = sequence
        .tracks
        .get(index)?
        .clips()
        .iter()
        .position(|clip| clip.start < at && at < clip.end())?;
    let fresh = sequence.alloc_id();
    let old_link = sequence.tracks.get(index)?.clips().get(position)?.link;
    let new_link = match old_link {
        0 => 0,
        link => match links.get(&link) {
            Some(mapped) => *mapped,
            None => {
                let mapped = sequence.alloc_id();
                links.insert(link, mapped);
                mapped
            }
        },
    };
    let track = sequence.tracks.get_mut(index)?;
    let left = track.clips_mut().get_mut(position)?;
    let old_id = left.id;
    let mut right = left.clone();
    right.id = fresh;
    right.link = new_link;
    right.start = at;
    right.duration = left.end() - at;
    if !is_hold(left) {
        right.source_in = left.source_time(at);
    }
    left.duration = at - left.start;
    // Transitions after the cut that left the old clip now leave the
    // right half; one across the cut inside the clip goes.
    track.transitions.retain(|transition| {
        !(transition.start < at
            && at < transition.end()
            && transition.from == old_id
            && transition.to != 0)
    });
    for transition in &mut track.transitions {
        if transition.start >= at && transition.from == old_id {
            transition.from = fresh;
        }
    }
    track.clips_mut().insert(position + 1, right);
    Some(fresh)
}

/// Removes everything on track `index` between `start` and `end`,
/// cutting clips at both ends. Returns the removed clips' ids.
fn clear_range(
    sequence: &mut Sequence,
    index: usize,
    start: Time,
    end: Time,
    links: &mut HashMap<u64, u64>,
) -> Vec<u64> {
    if end <= start {
        return Vec::new();
    }
    split_at(sequence, index, start, links);
    split_at(sequence, index, end, links);
    let mut removed = Vec::new();
    if let Some(track) = sequence.tracks.get_mut(index) {
        track.clips_mut().retain(|clip| {
            let inside = clip.start >= start && clip.end() <= end;
            if inside {
                removed.push(clip.id);
            }
            !inside
        });
        remove_orphan_transitions(track);
    }
    removed
}

/// Moves everything on track `index` that starts at or after `at` by
/// `delta`.
fn shift_from(sequence: &mut Sequence, index: usize, at: Time, delta: Time) {
    if let Some(track) = sequence.tracks.get_mut(index) {
        for clip in track.clips_mut() {
            if clip.start >= at {
                clip.start = clip.start + delta;
            }
        }
        for transition in &mut track.transitions {
            if transition.start >= at || (transition.end() > at && transition.from == 0) {
                transition.start = transition.start + delta;
            }
        }
        track.sort();
    }
}

/// Drops transitions whose clips are gone, or no longer meet where the
/// transition is.
fn remove_orphan_transitions(track: &mut Track) {
    let spans: HashMap<u64, (Time, Time)> = track
        .clips()
        .iter()
        .map(|clip| (clip.id, (clip.start, clip.end())))
        .collect();
    track
        .transitions
        .retain(|transition| match (transition.from, transition.to) {
            (0, 0) => false,
            (from, 0) => spans.contains_key(&from),
            (0, to) => spans.contains_key(&to),
            (from, to) => matches!(
                (spans.get(&from), spans.get(&to)),
                (Some((_, end)), Some((start, _))) if end == start
            ),
        });
}

/// Keeps every transition on its cut after the cuts moved.
fn transitions_follow(before: &Sequence, after: &mut Sequence) {
    for (index, track) in after.tracks.iter_mut().enumerate() {
        let Some(old) = before.tracks.get(index) else {
            continue;
        };
        let anchor = |track: &Track, transition: &Transition| -> Option<Time> {
            if transition.to != 0 {
                track.clip(transition.to).map(|clip| clip.start)
            } else {
                track.clip(transition.from).map(Clip::end)
            }
        };
        let shifts: Vec<Time> = track
            .transitions
            .iter()
            .map(
                |transition| match (anchor(old, transition), anchor(track, transition)) {
                    (Some(a), Some(b)) => b - a,
                    _ => Time::ZERO,
                },
            )
            .collect();
        for (transition, shift) in track.transitions.iter_mut().zip(shifts) {
            transition.start = transition.start + shift;
        }
        remove_orphan_transitions(track);
    }
}

fn place(sequence: &mut Sequence, index: usize, mut clip: Clip) -> Result<u64, EditError> {
    if clip.id == 0 || sequence.find(clip.id).is_some() {
        clip.id = sequence.alloc_id();
    }
    let id = clip.id;
    track_mut(sequence, index)?.insert(clip);
    Ok(id)
}

// ----- Placing clips ----------------------------------------------------------

/// Overwrites: each clip replaces what is under it on its track. Returns
/// the new clips' ids.
///
/// # Errors
/// A locked or missing track, or a clip on a track of the other kind.
pub fn overwrite(
    sequence: &mut Sequence,
    placements: Vec<(usize, Clip)>,
) -> Result<Vec<u64>, EditError> {
    if placements.is_empty() {
        return Err(EditError::Nothing);
    }
    for (index, _) in &placements {
        unlocked(sequence, *index)?;
    }
    let mut links = HashMap::new();
    let mut ids = Vec::new();
    for (index, clip) in placements {
        clear_range(sequence, index, clip.start, clip.end(), &mut links);
        ids.push(place(sequence, index, clip)?);
    }
    check(sequence)?;
    Ok(ids)
}

/// Inserts: opens room from the earliest start for the longest clip on
/// the clips' tracks and on every sync locked, unlocked track, then
/// places the clips. Returns their ids.
///
/// # Errors
/// A locked or missing track.
pub fn insert(
    sequence: &mut Sequence,
    placements: Vec<(usize, Clip)>,
) -> Result<Vec<u64>, EditError> {
    if placements.is_empty() {
        return Err(EditError::Nothing);
    }
    for (index, _) in &placements {
        unlocked(sequence, *index)?;
    }
    let at = placements
        .iter()
        .map(|(_, clip)| clip.start)
        .min()
        .unwrap_or_default();
    let end = placements
        .iter()
        .map(|(_, clip)| clip.end())
        .max()
        .unwrap_or_default();
    let targets: Vec<usize> = placements.iter().map(|(index, _)| *index).collect();
    let mut links = HashMap::new();
    for index in 0..sequence.tracks.len() {
        let track = &sequence.tracks[index];
        if !track.locked && (targets.contains(&index) || model::flag(track, key::SYNC_LOCK)) {
            split_at(sequence, index, at, &mut links);
            shift_from(sequence, index, at, end - at);
        }
    }
    let mut ids = Vec::new();
    for (index, clip) in placements {
        ids.push(place(sequence, index, clip)?);
    }
    check(sequence)?;
    Ok(ids)
}

/// Cuts every clip under `at` on `tracks` (every unlocked track when
/// empty). Returns the new right halves' ids.
pub fn razor(sequence: &mut Sequence, tracks: &[usize], at: Time) -> Vec<u64> {
    let mut links = HashMap::new();
    let mut out = Vec::new();
    for index in 0..sequence.tracks.len() {
        if sequence.tracks[index].locked || (!tracks.is_empty() && !tracks.contains(&index)) {
            continue;
        }
        if let Some(id) = split_at(sequence, index, at, &mut links) {
            out.push(id);
        }
    }
    out
}

/// Cuts only the clips `ids` (where they hold `at`).
pub fn razor_clips(sequence: &mut Sequence, ids: &[u64], at: Time) -> Vec<u64> {
    let mut links = HashMap::new();
    let mut out = Vec::new();
    for &id in ids {
        let Some((index, clip)) = sequence.find(id) else {
            continue;
        };
        if sequence.tracks[index].locked || !(clip.start < at && at < clip.end()) {
            continue;
        }
        if let Some(right) = split_at(sequence, index, at, &mut links) {
            out.push(right);
        }
    }
    out
}

/// Lifts `start` to `end` off `tracks`, leaving a gap.
pub fn lift(sequence: &mut Sequence, tracks: &[usize], start: Time, end: Time) -> Vec<u64> {
    let mut links = HashMap::new();
    let mut removed = Vec::new();
    for &index in tracks {
        if sequence
            .tracks
            .get(index)
            .is_some_and(|track| !track.locked)
        {
            removed.extend(clear_range(sequence, index, start, end, &mut links));
        }
    }
    removed
}

/// Extracts `start` to `end` from `tracks` and every sync locked track,
/// closing the gap.
pub fn extract(sequence: &mut Sequence, tracks: &[usize], start: Time, end: Time) -> Vec<u64> {
    let mut links = HashMap::new();
    let mut removed = Vec::new();
    if end <= start {
        return removed;
    }
    for index in 0..sequence.tracks.len() {
        let track = &sequence.tracks[index];
        if track.locked || !(tracks.contains(&index) || model::flag(track, key::SYNC_LOCK)) {
            continue;
        }
        removed.extend(clear_range(sequence, index, start, end, &mut links));
        shift_from(sequence, index, end, start - end);
    }
    removed
}

/// Deletes clips, leaving gaps. Returns how many went.
pub fn delete(sequence: &mut Sequence, ids: &[u64]) -> usize {
    let mut count = 0;
    for track in &mut sequence.tracks {
        if track.locked {
            continue;
        }
        let before = track.clips().len();
        track.clips_mut().retain(|clip| !ids.contains(&clip.id));
        count += before - track.clips().len();
        remove_orphan_transitions(track);
    }
    count
}

/// Deletes clips and closes the gaps they leave on their tracks and the
/// sync locked ones.
///
/// # Errors
/// [`EditError::SyncConflict`] when a sync locked track (or a track that
/// lost a clip) still has something where a gap would close.
pub fn ripple_delete(sequence: &mut Sequence, ids: &[u64]) -> Result<usize, EditError> {
    let mut ranges: Vec<(Time, Time)> = Vec::new();
    let mut touched = Vec::new();
    for &id in ids {
        let index = clip_track(sequence, id)?;
        let (_, clip) = sequence.find(id).ok_or(EditError::NoClip(id))?;
        ranges.push((clip.start, clip.end()));
        if !touched.contains(&index) {
            touched.push(index);
        }
    }
    let count = delete(sequence, ids);
    if count == 0 {
        return Err(EditError::Nothing);
    }
    // Merge the ranges, then close the latest first so earlier ones stay
    // where they are.
    ranges.sort_by_key(|range| range.0);
    let mut merged: Vec<(Time, Time)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let rippling: Vec<usize> = (0..sequence.tracks.len())
        .filter(|index| {
            let track = &sequence.tracks[*index];
            !track.locked && (touched.contains(index) || model::flag(track, key::SYNC_LOCK))
        })
        .collect();
    for (start, end) in merged.into_iter().rev() {
        for &index in &rippling {
            let track = &sequence.tracks[index];
            if track
                .clips()
                .iter()
                .any(|clip| clip.start < end && clip.end() > start)
            {
                return Err(EditError::SyncConflict(track.name.clone()));
            }
        }
        for &index in &rippling {
            shift_from(sequence, index, end, start - end);
        }
    }
    check(sequence)?;
    Ok(count)
}

/// Closes the gap on track `index` that holds `at`, moving everything
/// after it on that track left.
///
/// # Errors
/// No gap there, or a locked track.
pub fn close_gap(sequence: &mut Sequence, index: usize, at: Time) -> Result<Time, EditError> {
    unlocked(sequence, index)?;
    let track = &sequence.tracks[index];
    if track
        .clips()
        .iter()
        .any(|clip| clip.start <= at && at < clip.end())
    {
        return Err(EditError::Nothing);
    }
    let start = track
        .clips()
        .iter()
        .filter(|clip| clip.end() <= at)
        .map(Clip::end)
        .max()
        .unwrap_or(Time::ZERO);
    let end = track
        .clips()
        .iter()
        .filter(|clip| clip.start > at)
        .map(|clip| clip.start)
        .min()
        .ok_or(EditError::Nothing)?;
    shift_from(sequence, index, end, start - end);
    Ok(end - start)
}

// ----- Trimming ---------------------------------------------------------------

fn neighbours(track: &Track, id: u64) -> (Time, Time) {
    let clips = track.clips();
    let position = clips.iter().position(|clip| clip.id == id).unwrap_or(0);
    let before = position
        .checked_sub(1)
        .and_then(|at| clips.get(at))
        .map_or(Time::ZERO, Clip::end);
    let after = clips
        .get(position + 1)
        .map_or(Time(i64::MAX), |clip| clip.start);
    (before, after)
}

/// How far edge `edge` of clip `id` may move by `delta` (`ripple` lets
/// it ignore its neighbours): `delta` clamped to the media's handles, to
/// a frame of length, and, for a regular trim, to the neighbours.
///
/// # Errors
/// No such clip, or a locked track.
pub fn clamp_trim(
    sequence: &Sequence,
    id: u64,
    edge: Edge,
    ripple: bool,
    delta: Time,
    ctx: &EditCtx<'_>,
) -> Result<Time, EditError> {
    let index = clip_track(sequence, id)?;
    let track = &sequence.tracks[index];
    let clip = track.clip(id).ok_or(EditError::NoClip(id))?;
    let (before, after) = neighbours(track, id);
    let one = frame(sequence);
    Ok(match edge {
        Edge::In => {
            let room = head_room(clip, ctx);
            let mut low = Time(-room.flicks().min(i64::MAX / 2));
            if !ripple {
                low = low.max(before - clip.start);
            }
            let high = clip.duration - one;
            delta.clamp(low.min(high), high)
        }
        Edge::Out => {
            let low = one - clip.duration;
            let mut high = tail_room(clip, ctx);
            if !ripple {
                high = high.min(after - clip.end());
            }
            delta.clamp(low, high.max(low))
        }
    })
}

/// Moves `edge` of clip `clip` by `delta` (already clamped): its length,
/// where it starts in its source, and for the in edge of a regular trim,
/// where it starts on the track.
fn move_edge(clip: &mut Clip, edge: Edge, delta: Time, move_start: bool) {
    match edge {
        Edge::In => {
            if !is_hold(clip) {
                clip.source_in = clip.source_in + scaled(delta, clip.speed);
            }
            clip.duration = clip.duration - delta;
            if move_start {
                clip.start = clip.start + delta;
            }
        }
        Edge::Out => clip.duration = clip.duration + delta,
    }
}

/// Trims `edge` of every clip of `ids` (a clip and its linked partners,
/// or a selection) by one `delta`, clamped by the clip that may move
/// least. A ripple trim moves what follows on their tracks and the sync
/// locked ones by the change. Returns the delta made.
///
/// # Errors
/// A missing clip, a locked track, or a ripple that would push a sync
/// locked track's clip over another.
pub fn trim(
    sequence: &mut Sequence,
    ids: &[u64],
    edge: Edge,
    ripple: bool,
    delta: Time,
    ctx: &EditCtx<'_>,
) -> Result<Time, EditError> {
    let mut d = delta;
    for &id in ids {
        let allowed = clamp_trim(sequence, id, edge, ripple, d, ctx)?;
        if allowed.flicks().abs() < d.flicks().abs()
            || allowed.flicks().signum() != d.flicks().signum()
        {
            d = if allowed.flicks().signum() == d.flicks().signum() {
                allowed
            } else {
                Time::ZERO
            };
        }
    }
    if d == Time::ZERO {
        return Ok(d);
    }
    let before = sequence.clone();
    // Where the shift starts on each member track.
    let mut origins: Vec<(usize, Time)> = Vec::new();
    for &id in ids {
        let (index, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
        let from = match edge {
            Edge::In => clip.start + Time(1),
            Edge::Out => clip.end(),
        };
        move_edge(clip, edge, d, !ripple);
        if !origins.iter().any(|(track, _)| *track == index) {
            origins.push((index, from));
        }
    }
    if ripple {
        let shift = match edge {
            Edge::In => -d,
            Edge::Out => d,
        };
        let main = origins.first().map_or(Time::ZERO, |origin| origin.1);
        for index in 0..sequence.tracks.len() {
            let track = &sequence.tracks[index];
            if track.locked {
                continue;
            }
            let from = match origins.iter().find(|(member, _)| *member == index) {
                Some((_, from)) => *from,
                None if model::flag(track, key::SYNC_LOCK) => main,
                None => continue,
            };
            let track = &mut sequence.tracks[index];
            for clip in track.clips_mut() {
                if !ids.contains(&clip.id) && clip.start >= from {
                    clip.start = clip.start + shift;
                }
            }
            track.sort();
        }
        transitions_follow(&before, sequence);
    } else {
        for track in &mut sequence.tracks {
            remove_orphan_transitions(track);
        }
    }
    if let Err(error) = check(sequence) {
        return Err(match error {
            EditError::TooShort => EditError::TooShort,
            other => other,
        });
    }
    Ok(d)
}

/// Rolls the cut between clips `left` and `right` (which meet) by
/// `delta`: one grows as the other shrinks, nothing else moves. Returns
/// the delta made.
///
/// # Errors
/// Clips that do not meet, a missing clip or a locked track.
pub fn roll(
    sequence: &mut Sequence,
    left: u64,
    right: u64,
    delta: Time,
    ctx: &EditCtx<'_>,
) -> Result<Time, EditError> {
    clip_track(sequence, left)?;
    clip_track(sequence, right)?;
    let (_, l) = sequence.find(left).ok_or(EditError::NoClip(left))?;
    let (_, r) = sequence.find(right).ok_or(EditError::NoClip(right))?;
    if l.end() != r.start {
        return Err(EditError::NotAdjacent);
    }
    let one = frame(sequence);
    let d = delta
        .max(one - l.duration)
        .min(r.duration - one)
        .min(tail_room(l, ctx))
        .max(-head_room(r, ctx));
    if d == Time::ZERO {
        return Ok(d);
    }
    let before = sequence.clone();
    if let Some((_, l)) = sequence.find_mut(left) {
        move_edge(l, Edge::Out, d, false);
    }
    if let Some((_, r)) = sequence.find_mut(right) {
        move_edge(r, Edge::In, d, true);
    }
    transitions_follow(&before, sequence);
    check(sequence)?;
    Ok(d)
}

/// Slips clip `id`: it shows other frames of its source, `delta` of
/// source time later, in the same place. Returns the delta made.
///
/// # Errors
/// A missing clip or a locked track.
pub fn slip(
    sequence: &mut Sequence,
    id: u64,
    delta: Time,
    ctx: &EditCtx<'_>,
) -> Result<Time, EditError> {
    clip_track(sequence, id)?;
    let (_, clip) = sequence.find(id).ok_or(EditError::NoClip(id))?;
    let used = scaled(clip.duration, clip.speed);
    let (low, high) = match (ctx.media_len)(clip.source) {
        Some(length) => (Time::ZERO, (length - used).max(Time::ZERO)),
        None => (Time(i64::MIN / 2), Time(i64::MAX / 2)),
    };
    let new_in = (clip.source_in + delta).clamp(low, high.max(low));
    let d = new_in - clip.source_in;
    if let Some((_, clip)) = sequence.find_mut(id) {
        clip.source_in = new_in;
    }
    Ok(d)
}

/// Slides clip `id` by `delta` between its neighbours: the one before
/// it grows or shrinks at its end, the one after at its start, so the
/// whole stays the same length. Returns the delta made.
///
/// # Errors
/// A missing clip or a locked track.
pub fn slide(
    sequence: &mut Sequence,
    id: u64,
    delta: Time,
    ctx: &EditCtx<'_>,
) -> Result<Time, EditError> {
    let index = clip_track(sequence, id)?;
    let track = &sequence.tracks[index];
    let clips = track.clips();
    let position = clips
        .iter()
        .position(|clip| clip.id == id)
        .ok_or(EditError::NoClip(id))?;
    let me = clips[position].clone();
    let prev = position
        .checked_sub(1)
        .and_then(|at| clips.get(at))
        .filter(|clip| clip.end() == me.start)
        .cloned();
    let next = clips
        .get(position + 1)
        .filter(|clip| clip.start == me.end())
        .cloned();
    let (gap_before, gap_after) = neighbours(track, id);
    let one = frame(sequence);
    let mut d = delta;
    match &prev {
        Some(clip) => d = d.max(one - clip.duration).min(tail_room(clip, ctx)),
        None => d = d.max(gap_before - me.start),
    }
    match &next {
        Some(clip) => d = d.min(clip.duration - one).max(-head_room(clip, ctx)),
        None => d = d.min(gap_after - me.end()),
    }
    if d == Time::ZERO {
        return Ok(d);
    }
    let before = sequence.clone();
    if let Some(clip) = &prev
        && let Some((_, clip)) = sequence.find_mut(clip.id)
    {
        move_edge(clip, Edge::Out, d, false);
    }
    if let Some(clip) = &next
        && let Some((_, clip)) = sequence.find_mut(clip.id)
    {
        move_edge(clip, Edge::In, d, true);
    }
    if let Some((_, clip)) = sequence.find_mut(id) {
        clip.start = clip.start + d;
    }
    transitions_follow(&before, sequence);
    check(sequence)?;
    Ok(d)
}

// ----- Moving -----------------------------------------------------------------

/// Moves clips `ids` by `delta` in time and `tracks` tracks down (the
/// same kind of track: a picture never lands on a sound track). In
/// `insert` mode what is at the destination moves right; otherwise it is
/// overwritten. Returns the moved clips' ids.
///
/// # Errors
/// A destination off the tracks, of the other kind or locked.
pub fn move_clips(
    sequence: &mut Sequence,
    ids: &[u64],
    delta: Time,
    tracks: isize,
    insert_mode: bool,
) -> Result<Vec<u64>, EditError> {
    let mut moving = Vec::new();
    for &id in ids {
        let index = clip_track(sequence, id)?;
        let to = index
            .checked_add_signed(tracks)
            .filter(|to| *to < sequence.tracks.len())
            .ok_or(EditError::NoTrack(index))?;
        if sequence.tracks[to].kind != sequence.tracks[index].kind {
            return Err(EditError::WrongKind);
        }
        unlocked(sequence, to)?;
        let (_, clip) = sequence.find(id).ok_or(EditError::NoClip(id))?;
        let mut clip = clip.clone();
        clip.start = (clip.start + delta).max(Time::ZERO);
        moving.push((to, clip));
    }
    delete(sequence, ids);
    if insert_mode {
        insert(sequence, moving)
    } else {
        overwrite(sequence, moving)
    }
}

/// Copies clips `ids` to `at` (the earliest of them lands there) on
/// their own tracks, overwriting; the copies get new ids and new link
/// groups. Returns the copies' ids.
///
/// # Errors
/// A locked track.
pub fn paste(
    sequence: &mut Sequence,
    clips: &[(usize, Clip)],
    at: Time,
) -> Result<Vec<u64>, EditError> {
    let Some(first) = clips.iter().map(|(_, clip)| clip.start).min() else {
        return Err(EditError::Nothing);
    };
    let mut links: HashMap<u64, u64> = HashMap::new();
    let mut placed = Vec::new();
    for (index, clip) in clips {
        let mut copy = clip.clone();
        copy.id = 0;
        copy.start = at + (clip.start - first);
        if copy.link != 0 {
            copy.link = match links.get(&clip.link) {
                Some(mapped) => *mapped,
                None => {
                    let mapped = sequence.alloc_id();
                    links.insert(clip.link, mapped);
                    mapped
                }
            };
        }
        if *index < sequence.tracks.len() {
            placed.push((*index, copy));
        }
    }
    overwrite(sequence, placed)
}

// ----- Clip changes -----------------------------------------------------------

/// Sets clip `id`'s speed (1 normal, 2 twice as fast, negative backward):
/// its length follows so it shows the same frames. With `ripple` what
/// follows on its track moves too; without, the clip may not grow past
/// its neighbour.
///
/// # Errors
/// A missing clip, a locked track, a speed of zero.
pub fn set_speed(
    sequence: &mut Sequence,
    id: u64,
    speed: f64,
    ripple: bool,
) -> Result<(), EditError> {
    if !speed.is_finite() || speed.abs() < 0.01 {
        return Err(EditError::Nothing);
    }
    let index = clip_track(sequence, id)?;
    let track = &sequence.tracks[index];
    let (_, after) = neighbours(track, id);
    let clip = track.clip(id).ok_or(EditError::NoClip(id))?;
    let used = scaled(clip.duration, clip.speed);
    let length = unscaled(used, speed).max(frame(sequence));
    let grow = length - clip.duration;
    let end = clip.end();
    if !ripple && clip.start + length > after {
        return Err(EditError::SyncConflict(track.name.clone()));
    }
    let before = sequence.clone();
    if let Some((_, clip)) = sequence.find_mut(id) {
        clip.speed = speed;
        clip.duration = length;
    }
    if ripple {
        let track = &mut sequence.tracks[index];
        for clip in track.clips_mut() {
            if clip.id != id && clip.start >= end {
                clip.start = clip.start + grow;
            }
        }
        track.sort();
    }
    transitions_follow(&before, sequence);
    check(sequence)
}

/// Freezes clip `id` from `at` on: it is cut there and the right half
/// holds the frame at `at` for as long as it lasts. Returns the hold's id.
///
/// # Errors
/// A missing clip or a locked track.
pub fn freeze_frame(sequence: &mut Sequence, id: u64, at: Time) -> Result<u64, EditError> {
    let index = clip_track(sequence, id)?;
    let (_, clip) = sequence.find(id).ok_or(EditError::NoClip(id))?;
    let source = clip.source_time(at);
    let hold = if clip.start < at && at < clip.end() {
        split_at(sequence, index, at, &mut HashMap::new()).ok_or(EditError::Nothing)?
    } else {
        id
    };
    let (_, clip) = sequence.find_mut(hold).ok_or(EditError::NoClip(hold))?;
    clip.source_in = source;
    clip.properties.set(key::FREEZE, true);
    Ok(hold)
}

/// Sets property `name` of every clip of `ids` to `value` (`None`
/// removes it).
///
/// # Errors
/// A missing clip.
pub fn set_property(
    sequence: &mut Sequence,
    ids: &[u64],
    name: &str,
    value: Option<Value>,
) -> Result<(), EditError> {
    for &id in ids {
        let (_, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
        match &value {
            Some(value) => {
                clip.properties.set(name, value.clone());
            }
            None => {
                clip.properties.remove(name);
            }
        }
    }
    Ok(())
}

/// Turns clips on or off.
///
/// # Errors
/// A missing clip.
pub fn set_enabled(sequence: &mut Sequence, ids: &[u64], enabled: bool) -> Result<(), EditError> {
    for &id in ids {
        sequence
            .find_mut(id)
            .ok_or(EditError::NoClip(id))?
            .1
            .enabled = enabled;
    }
    Ok(())
}

/// Puts a keyframe of `name` on clip `id` at sequence time `at`.
///
/// # Errors
/// A missing clip.
pub fn set_key(
    sequence: &mut Sequence,
    id: u64,
    name: &str,
    at: Time,
    value: f64,
) -> Result<(), EditError> {
    let (_, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
    let time = clip.source_time(at);
    clip.animation.channel_mut(name).insert(sue_model::Key {
        time,
        value,
        interpolation: sue_model::Interpolation::Linear,
    });
    Ok(())
}

/// Takes the keyframe of `name` at `at` off clip `id` (the whole curve
/// when it was the last).
///
/// # Errors
/// A missing clip, or no key there.
pub fn remove_key(sequence: &mut Sequence, id: u64, name: &str, at: Time) -> Result<(), EditError> {
    let (_, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
    let time = clip.source_time(at);
    let curve = clip.animation.channel_mut(name);
    let nearest = curve
        .keys()
        .iter()
        .min_by_key(|key| (key.time - time).flicks().abs())
        .map(|key| key.time)
        .ok_or(EditError::Nothing)?;
    curve.remove(nearest);
    if curve.keys().is_empty() {
        clip.animation.remove_channel(name);
    }
    Ok(())
}

/// Links clips into one group (a picture with its sound).
///
/// # Errors
/// A missing clip.
pub fn link(sequence: &mut Sequence, ids: &[u64]) -> Result<u64, EditError> {
    let group = sequence.alloc_id();
    for &id in ids {
        sequence.find_mut(id).ok_or(EditError::NoClip(id))?.1.link = group;
    }
    Ok(group)
}

/// Unlinks clips.
///
/// # Errors
/// A missing clip.
pub fn unlink(sequence: &mut Sequence, ids: &[u64]) -> Result<(), EditError> {
    for &id in ids {
        sequence.find_mut(id).ok_or(EditError::NoClip(id))?.1.link = 0;
    }
    Ok(())
}

// ----- Transitions ------------------------------------------------------------

/// The transition kinds stage 1 draws.
pub const TRANSITIONS: [&str; 3] = ["dissolve", "dip-black", "dip-white"];

/// Puts a transition of `kind` lasting `length` on the cut at `at` on
/// track `index`: across the cut between two clips (centred, shortened
/// to what both have), or at the start or end of a lone clip (a fade).
/// Replaces one already there. Returns where it starts.
///
/// # Errors
/// No clip meets `at`, or a locked track.
pub fn add_transition(
    sequence: &mut Sequence,
    index: usize,
    kind: &str,
    at: Time,
    length: Time,
) -> Result<Time, EditError> {
    unlocked(sequence, index)?;
    let track = &sequence.tracks[index];
    let left = track.clips().iter().find(|clip| clip.end() == at).cloned();
    let right = track.clips().iter().find(|clip| clip.start == at).cloned();
    let one = frame(sequence);
    let (start, duration, from, to) = match (&left, &right) {
        (Some(l), Some(r)) => {
            let half = Time(length.flicks() / 2)
                .min(l.duration - one)
                .min(r.duration - one)
                .max(one);
            (at - half, half + half, l.id, r.id)
        }
        (None, Some(r)) => {
            let length = length.min(r.duration).max(one);
            (r.start, length, 0, r.id)
        }
        (Some(l), None) => {
            let length = length.min(l.duration).max(one);
            (l.end() - length, length, l.id, 0)
        }
        (None, None) => return Err(EditError::NotAdjacent),
    };
    let track = &mut sequence.tracks[index];
    track
        .transitions
        .retain(|transition| !(transition.from == from && transition.to == to));
    track
        .transitions
        .push(Transition::new(kind, start, duration, from, to));
    track.sort();
    Ok(start)
}

/// Removes the transitions on track `index` that cover `at`.
pub fn remove_transitions(sequence: &mut Sequence, index: usize, at: Time) -> usize {
    let Some(track) = sequence.tracks.get_mut(index) else {
        return 0;
    };
    let before = track.transitions.len();
    track
        .transitions
        .retain(|transition| !(transition.start <= at && at <= transition.end()));
    before - track.transitions.len()
}

// ----- Marks, markers and tracks ----------------------------------------------

/// Sets (or with `None` clears) the program's in or out mark.
pub fn set_mark(sequence: &mut Sequence, out: bool, at: Option<Time>) {
    let name = if out { key::MARK_OUT } else { key::MARK_IN };
    match at {
        Some(at) => {
            sequence.properties.set(name, at);
        }
        None => {
            sequence.properties.remove(name);
        }
    }
}

/// Adds a marker at `at`.
pub fn add_marker(sequence: &mut Sequence, at: Time, name: &str) {
    let index = sequence.markers.partition_point(|marker| marker.at <= at);
    sequence.markers.insert(index, Marker::new(at, name));
}

/// Removes the marker nearest `at` within `within`.
pub fn remove_marker(sequence: &mut Sequence, at: Time, within: Time) -> bool {
    let found = sequence
        .markers
        .iter()
        .enumerate()
        .filter(|(_, marker)| (marker.at - at).flicks().abs() <= within.flicks())
        .min_by_key(|(_, marker)| (marker.at - at).flicks().abs())
        .map(|(index, _)| index);
    found.is_some_and(|index| {
        sequence.markers.remove(index);
        true
    })
}

/// Adds a track of `kind` (`video` on top of the others, `audio` below
/// them), named after its place. Returns its index.
pub fn add_track(sequence: &mut Sequence, kind: &str) -> usize {
    let count = sequence
        .tracks
        .iter()
        .filter(|track| track.kind == kind)
        .count();
    if kind == model::VIDEO {
        sequence
            .tracks
            .insert(0, Track::new(format!("V{}", count + 1), kind));
        0
    } else {
        sequence
            .tracks
            .push(Track::new(format!("A{}", count + 1), kind));
        sequence.tracks.len() - 1
    }
}

/// Sets a flag of track `index` (target, sync lock, solo).
///
/// # Errors
/// No such track.
pub fn set_track_flag(
    sequence: &mut Sequence,
    index: usize,
    name: &str,
    on: bool,
) -> Result<(), EditError> {
    track_mut(sequence, index)?.properties.set(name, on);
    Ok(())
}

// ----- Snapping ---------------------------------------------------------------

/// Where things snap to: the start and end of every clip (but those of
/// `except`), the markers, the marks and `extra` (the playhead).
#[must_use]
pub fn snap_points(sequence: &Sequence, except: &[u64], extra: &[Time]) -> Vec<Time> {
    let mut points: Vec<Time> = sequence
        .tracks
        .iter()
        .flat_map(|track| track.clips())
        .filter(|clip| !except.contains(&clip.id))
        .flat_map(|clip| [clip.start, clip.end()])
        .chain(sequence.markers.iter().map(|marker| marker.at))
        .chain(extra.iter().copied())
        .collect();
    let (mark_in, mark_out) = model::marks(sequence);
    points.extend(mark_in);
    points.extend(mark_out);
    points.push(Time::ZERO);
    points.sort();
    points.dedup();
    points
}

/// The point of `points` nearest `at` within `within`, if any.
#[must_use]
pub fn snap(at: Time, points: &[Time], within: Time) -> Option<Time> {
    points
        .iter()
        .copied()
        .filter(|point| (*point - at).flicks().abs() <= within.flicks())
        .min_by_key(|point| (*point - at).flicks().abs())
}

#[cfg(test)]
mod tests;
