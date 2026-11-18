//! Every edit, and each one undone and redone exactly through the undo
//! history (backbone rule 2).

use super::*;
use sue_core::{Arena, Entity, Rate, UndoStack};

use crate::model;

const FPS: Rate = Rate::FPS_25;

fn s(seconds: f64) -> Time {
    Time::from_seconds(seconds)
}

fn f(frames: i64) -> Time {
    Time::from_frames(frames, FPS)
}

/// A world with one sequence (V2, V1, A1, A2) and a 20 second source.
struct World {
    arena: Arena,
    history: UndoStack<Arena>,
    id: EntityId,
    film: EntityId,
}

fn media_len(film: EntityId) -> impl Fn(EntityId) -> Option<Time> {
    move |id| (id == film).then(|| s(20.0))
}

impl World {
    fn new() -> Self {
        let mut arena = Arena::new();
        let film = Entity::empty("film.mp4", sue_model::class::VIDEO);
        let film_id = film.id();
        arena.insert_keep_id(film).unwrap();
        let film = film_id;
        let sequence = model::empty_sequence(1920, 1080, FPS, 2, 2);
        let entity = Entity::empty("Edit", sue_model::class::SEQUENCE).with_ext(sequence);
        let id = entity.id();
        arena.insert_keep_id(entity).unwrap();
        Self {
            arena,
            history: UndoStack::new(),
            id,
            film,
        }
    }

    fn sequence(&self) -> Sequence {
        model::sequence(&self.arena, self.id).unwrap().clone()
    }

    /// Runs `change` as one undo step, then checks that undo gives back
    /// the sequence exactly as it was and redo exactly as it became.
    fn edit<T>(
        &mut self,
        change: impl FnOnce(&mut Sequence, &EditCtx<'_>) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        let before = self.sequence();
        let length = media_len(self.film);
        let ctx = EditCtx { media_len: &length };
        let (step, out) = model::plan(&self.arena, self.id, "Edit", |sequence| {
            change(sequence, &ctx)
        })?;
        if let Some(step) = step {
            self.history.apply(&mut self.arena, step).unwrap();
            let after = self.sequence();
            check(&after).unwrap();
            assert!(self.history.undo(&mut self.arena).unwrap());
            let mut expected = before.clone();
            expected.ensure_ids();
            assert!(
                self.sequence() == before || self.sequence() == expected,
                "undo gives back exactly what was"
            );
            assert!(self.history.redo(&mut self.arena).unwrap());
            assert_eq!(
                self.sequence(),
                after,
                "redo gives back exactly what was made"
            );
        }
        Ok(out)
    }

    fn clip(&self, start: f64, length: f64) -> Clip {
        Clip::new(self.film, s(start), s(length))
    }

    /// The clips of track `index` as (start, length, source in) seconds.
    fn shape(&self, index: usize) -> Vec<(f64, f64, f64)> {
        self.sequence().tracks[index]
            .clips()
            .iter()
            .map(|clip| {
                let r = |t: Time| (t.seconds() * 1000.0).round() / 1000.0;
                (r(clip.start), r(clip.duration), r(clip.source_in))
            })
            .collect()
    }
}

// Track 0 is V2, 1 is V1, 2 is A1, 3 is A2.
const V2: usize = 0;
const V1: usize = 1;
const A1: usize = 2;
const A2: usize = 3;

#[test]
fn overwrite_replaces_what_is_under_the_clip() {
    let mut w = World::new();
    let (a, b) = (w.clip(0.0, 10.0), w.clip(4.0, 2.0));
    w.edit(|q, _| overwrite(q, vec![(V1, a)])).unwrap();
    w.edit(|q, _| overwrite(q, vec![(V1, b)])).unwrap();
    assert_eq!(
        w.shape(V1),
        [(0.0, 4.0, 0.0), (4.0, 2.0, 0.0), (6.0, 4.0, 6.0)]
    );
}

#[test]
fn insert_opens_room_on_the_target_and_sync_locked_tracks() {
    let mut w = World::new();
    let (a, m, b) = (w.clip(0.0, 10.0), w.clip(0.0, 10.0), w.clip(4.0, 2.0));
    w.edit(|q, _| overwrite(q, vec![(V1, a), (A1, m)])).unwrap();
    w.edit(|q, _| {
        set_track_flag(q, A2, key::SYNC_LOCK, false)?;
        insert(q, vec![(V1, b)])
    })
    .unwrap();
    assert_eq!(
        w.shape(V1),
        [(0.0, 4.0, 0.0), (4.0, 2.0, 0.0), (6.0, 6.0, 4.0)]
    );
    // A1 is sync locked: it is cut and moved too.
    assert_eq!(w.shape(A1), [(0.0, 4.0, 0.0), (6.0, 6.0, 4.0)]);
}

#[test]
fn razor_lift_and_extract() {
    let mut w = World::new();
    let (a, m) = (w.clip(0.0, 10.0), w.clip(0.0, 10.0));
    w.edit(|q, _| overwrite(q, vec![(V1, a), (A1, m)])).unwrap();
    let cut = w.edit(|q, _| Ok(razor(q, &[V1], s(3.0)))).unwrap();
    assert_eq!(cut.len(), 1);
    assert_eq!(w.shape(V1), [(0.0, 3.0, 0.0), (3.0, 7.0, 3.0)]);
    assert_eq!(w.shape(A1), [(0.0, 10.0, 0.0)], "only V1 was cut");
    w.edit(|q, _| Ok(lift(q, &[V1], s(1.0), s(2.0)))).unwrap();
    assert_eq!(
        w.shape(V1),
        [(0.0, 1.0, 0.0), (2.0, 1.0, 2.0), (3.0, 7.0, 3.0)]
    );
    w.edit(|q, _| Ok(extract(q, &[V1], s(5.0), s(7.0))))
        .unwrap();
    assert_eq!(
        w.shape(V1),
        [
            (0.0, 1.0, 0.0),
            (2.0, 1.0, 2.0),
            (3.0, 2.0, 3.0),
            (5.0, 3.0, 7.0)
        ]
    );
    assert_eq!(
        w.shape(A1),
        [(0.0, 5.0, 0.0), (5.0, 3.0, 7.0)],
        "sync locked"
    );
}

#[test]
fn ripple_delete_closes_the_gap_or_refuses_to_break_sync() {
    let mut w = World::new();
    let (a, b, c) = (w.clip(0.0, 2.0), w.clip(2.0, 3.0), w.clip(5.0, 2.0));
    let ids = w
        .edit(|q, _| overwrite(q, vec![(V1, a), (V1, b), (V1, c)]))
        .unwrap();
    w.edit(|q, _| ripple_delete(q, &[ids[1]])).unwrap();
    assert_eq!(w.shape(V1), [(0.0, 2.0, 0.0), (2.0, 2.0, 0.0)]);
    // A sound under the gap on a sync locked track: refused, nothing changes.
    let music = w.clip(0.0, 10.0);
    w.edit(|q, _| overwrite(q, vec![(A1, music)])).unwrap();
    let before = w.sequence();
    let error = w.edit(|q, _| ripple_delete(q, &[ids[0]])).unwrap_err();
    assert_eq!(error, EditError::SyncConflict("A1".into()));
    assert_eq!(w.sequence(), before);
}

#[test]
fn regular_and_ripple_trims_hold_to_handles_and_neighbours() {
    let mut w = World::new();
    let (mut a, b) = (w.clip(0.0, 4.0), w.clip(4.0, 4.0));
    a.source_in = s(2.0);
    let ids = w.edit(|q, _| overwrite(q, vec![(V1, a), (V1, b)])).unwrap();
    // The out edge of the first may not run into the second.
    let made = w
        .edit(|q, c| trim(q, &[ids[0]], Edge::Out, false, s(1.0), c))
        .unwrap();
    assert_eq!(made, Time::ZERO);
    // Shorter, leaving a gap.
    w.edit(|q, c| trim(q, &[ids[0]], Edge::Out, false, s(-1.0), c))
        .unwrap();
    assert_eq!(w.shape(V1), [(0.0, 3.0, 2.0), (4.0, 4.0, 0.0)]);
    // Ripple: the second follows.
    w.edit(|q, c| trim(q, &[ids[0]], Edge::Out, true, s(2.0), c))
        .unwrap();
    assert_eq!(w.shape(V1), [(0.0, 5.0, 2.0), (6.0, 4.0, 0.0)]);
    // The in edge can only reach back as far as the media goes: 2 s.
    let made = w
        .edit(|q, c| trim(q, &[ids[0]], Edge::In, true, s(-5.0), c))
        .unwrap();
    assert_eq!(made, s(-2.0));
    assert_eq!(w.shape(V1), [(0.0, 7.0, 0.0), (8.0, 4.0, 0.0)]);
    // Never shorter than a frame.
    let made = w
        .edit(|q, c| trim(q, &[ids[1]], Edge::Out, false, s(-10.0), c))
        .unwrap();
    assert_eq!(made, f(1) - s(4.0));
}

#[test]
fn roll_slip_and_slide() {
    let mut w = World::new();
    let (a, mut b, mut c) = (w.clip(0.0, 4.0), w.clip(4.0, 4.0), w.clip(8.0, 4.0));
    b.source_in = s(5.0);
    c.source_in = s(2.0);
    let ids = w
        .edit(|q, _| overwrite(q, vec![(V1, a), (V1, b), (V1, c)]))
        .unwrap();
    // Roll the first cut later by a second: the length stays.
    w.edit(|q, c| roll(q, ids[0], ids[1], s(1.0), c)).unwrap();
    assert_eq!(
        w.shape(V1),
        [(0.0, 5.0, 0.0), (5.0, 3.0, 6.0), (8.0, 4.0, 2.0)]
    );
    // Slip the middle clip: other frames, same place.
    w.edit(|q, c| slip(q, ids[1], s(2.0), c)).unwrap();
    assert_eq!(w.shape(V1)[1], (5.0, 3.0, 8.0));
    // Slide it back: the neighbours give and take.
    w.edit(|q, c| slide(q, ids[1], s(-1.0), c)).unwrap();
    assert_eq!(
        w.shape(V1),
        [(0.0, 4.0, 0.0), (4.0, 3.0, 8.0), (7.0, 5.0, 1.0)]
    );
    // Not adjacent: refused.
    assert_eq!(
        w.edit(|q, c| roll(q, ids[0], ids[2], s(1.0), c))
            .unwrap_err(),
        EditError::NotAdjacent
    );
    // A slip is held by the source: 20 s long, 3 s used.
    let made = w.edit(|q, c| slip(q, ids[1], s(100.0), c)).unwrap();
    assert_eq!(made, s(9.0));
}

#[test]
fn linked_clips_trim_together_and_a_j_cut_trims_one_side() {
    let mut w = World::new();
    let (a, m, b, n) = (
        w.clip(0.0, 4.0),
        w.clip(0.0, 4.0),
        w.clip(4.0, 4.0),
        w.clip(4.0, 4.0),
    );
    let ids = w
        .edit(|q, _| {
            let ids = overwrite(q, vec![(V1, a), (A1, m), (V1, b), (A1, n)])?;
            link(q, &[ids[0], ids[1]])?;
            link(q, &[ids[2], ids[3]])?;
            Ok(ids)
        })
        .unwrap();
    let group = w.sequence().linked(ids[0]);
    assert_eq!(group.len(), 2);
    // Both trimmed as one (ripple): the next pair follows on both tracks.
    w.edit(|q, c| trim(q, &group, Edge::Out, true, s(-1.0), c))
        .unwrap();
    assert_eq!(w.shape(V1), [(0.0, 3.0, 0.0), (3.0, 4.0, 0.0)]);
    assert_eq!(w.shape(A1), [(0.0, 3.0, 0.0), (3.0, 4.0, 0.0)]);
    // An L cut: roll only the sound's cut, the picture's stays.
    w.edit(|q, c| roll(q, ids[1], ids[3], s(1.0), c)).unwrap();
    assert_eq!(w.shape(V1), [(0.0, 3.0, 0.0), (3.0, 4.0, 0.0)]);
    assert_eq!(w.shape(A1), [(0.0, 4.0, 0.0), (4.0, 3.0, 1.0)]);
    // Razor through a linked pair: the right halves are linked together.
    let right = w.edit(|q, _| Ok(razor(q, &[V1, A1], s(1.0)))).unwrap();
    let q = w.sequence();
    let (_, r0) = q.find(right[0]).unwrap();
    let (_, r1) = q.find(right[1]).unwrap();
    assert!(r0.link != 0 && r0.link == r1.link && r0.link != q.find(ids[0]).unwrap().1.link);
}

#[test]
fn moving_clips_overwrites_or_inserts_and_keeps_kinds_apart() {
    let mut w = World::new();
    let (a, b) = (w.clip(0.0, 2.0), w.clip(5.0, 2.0));
    let ids = w.edit(|q, _| overwrite(q, vec![(V1, a), (V1, b)])).unwrap();
    w.edit(|q, _| move_clips(q, &[ids[0]], s(4.0), -1, false))
        .unwrap();
    assert_eq!(w.shape(V2), [(4.0, 2.0, 0.0)]);
    assert_eq!(w.shape(V1), [(5.0, 2.0, 0.0)]);
    assert_eq!(
        w.edit(|q, _| move_clips(q, &[ids[1]], Time::ZERO, 1, false))
            .unwrap_err(),
        EditError::WrongKind
    );
    // Insert mode on V1: the clip already there moves right.
    let moved = w
        .edit(|q, _| move_clips(q, &[ids[0]], s(1.0), 1, true))
        .unwrap();
    assert_eq!(moved.len(), 1);
    assert_eq!(w.shape(V1), [(5.0, 2.0, 0.0), (7.0, 2.0, 0.0)]);
}

#[test]
fn speed_freeze_and_keyframes() {
    let mut w = World::new();
    let (a, b) = (w.clip(0.0, 4.0), w.clip(4.0, 2.0));
    let ids = w.edit(|q, _| overwrite(q, vec![(V1, a), (V1, b)])).unwrap();
    // Twice as fast: the same frames in half the time, ripple.
    w.edit(|q, _| set_speed(q, ids[0], 2.0, true)).unwrap();
    assert_eq!(w.shape(V1), [(0.0, 2.0, 0.0), (2.0, 2.0, 0.0)]);
    assert_eq!(w.sequence().find(ids[0]).unwrap().1.source_out(), s(4.0));
    // Half speed without ripple runs into the neighbour: refused.
    assert!(w.edit(|q, _| set_speed(q, ids[0], 0.5, false)).is_err());
    // A freeze frame from 1 s: the rest of the clip holds source frame 2 s.
    let hold = w.edit(|q, _| freeze_frame(q, ids[0], s(1.0))).unwrap();
    let q = w.sequence();
    let (_, held) = q.find(hold).unwrap();
    assert_eq!(held.source_in, s(2.0));
    assert_eq!(held.properties.get(key::FREEZE), Some(&Value::Bool(true)));
    // Keys on opacity: half way at 1.5 s.
    w.edit(|q, _| {
        set_key(q, ids[1], key::OPACITY, s(2.0), 0.0)?;
        set_key(q, ids[1], key::OPACITY, s(3.0), 1.0)
    })
    .unwrap();
    let q = w.sequence();
    let clip = q.find(ids[1]).unwrap().1;
    assert!((clip.value_at(key::OPACITY, s(2.5), 1.0) - 0.5).abs() < 1e-6);
    w.edit(|q, _| remove_key(q, ids[1], key::OPACITY, s(2.0)))
        .unwrap();
    assert_eq!(
        w.sequence()
            .find(ids[1])
            .unwrap()
            .1
            .animation
            .channel(key::OPACITY)
            .unwrap()
            .keys()
            .len(),
        1
    );
    // Off and on.
    w.edit(|q, _| set_enabled(q, &[ids[1]], false)).unwrap();
    assert!(!w.sequence().find(ids[1]).unwrap().1.enabled);
}

#[test]
fn transitions_sit_on_cuts_and_follow_them() {
    let mut w = World::new();
    let (a, mut b) = (w.clip(0.0, 4.0), w.clip(4.0, 4.0));
    b.source_in = s(5.0);
    let ids = w.edit(|q, _| overwrite(q, vec![(V1, a), (V1, b)])).unwrap();
    w.edit(|q, _| add_transition(q, V1, "dissolve", s(4.0), s(1.0)))
        .unwrap();
    let q = w.sequence();
    let dissolve = &q.tracks[V1].transitions[0];
    assert_eq!((dissolve.start, dissolve.duration), (s(3.5), s(1.0)));
    assert_eq!((dissolve.from, dissolve.to), (ids[0], ids[1]));
    // A fade from black at the very start.
    w.edit(|q, _| add_transition(q, V1, "dip-black", Time::ZERO, s(0.5)))
        .unwrap();
    assert_eq!(w.sequence().tracks[V1].transitions.len(), 2);
    // The cut rolls: the dissolve goes with it.
    w.edit(|q, c| roll(q, ids[0], ids[1], s(-1.0), c)).unwrap();
    let dissolve = w.sequence().tracks[V1].transitions[1].clone();
    assert_eq!(dissolve.start, s(2.5));
    // Ripple delete the second clip: its dissolve goes, the fade stays.
    w.edit(|q, _| ripple_delete(q, &[ids[1]])).unwrap();
    assert_eq!(w.sequence().tracks[V1].transitions.len(), 1);
    assert_eq!(
        w.edit(|q, _| Ok(remove_transitions(q, V1, s(0.2))))
            .unwrap(),
        1
    );
}

#[test]
fn marks_markers_tracks_and_snapping() {
    let mut w = World::new();
    let a = w.clip(1.0, 2.0);
    w.edit(|q, _| overwrite(q, vec![(V1, a)])).unwrap();
    w.edit(|q, _| {
        set_mark(q, false, Some(s(1.0)));
        set_mark(q, true, Some(s(3.0)));
        add_marker(q, s(2.0), "Beat");
        Ok(add_track(q, model::VIDEO))
    })
    .unwrap();
    let q = w.sequence();
    assert_eq!(model::marks(&q), (Some(s(1.0)), Some(s(3.0))));
    assert_eq!(q.tracks[0].name, "V3");
    let points = snap_points(&q, &[], &[s(7.0)]);
    assert_eq!(snap(s(2.98), &points, s(0.1)), Some(s(3.0)));
    assert_eq!(snap(s(5.0), &points, s(0.1)), None);
    assert!(
        w.edit(|q, _| Ok(remove_marker(q, s(2.05), s(0.1))))
            .unwrap()
    );
    assert!(w.sequence().markers.is_empty());
}

#[test]
fn a_locked_track_refuses_every_edit() {
    let mut w = World::new();
    let a = w.clip(0.0, 4.0);
    let ids = w.edit(|q, _| overwrite(q, vec![(V1, a)])).unwrap();
    w.edit(|q, _| {
        q.tracks[V1].locked = true;
        Ok(())
    })
    .unwrap();
    let b = w.clip(0.0, 1.0);
    assert_eq!(
        w.edit(|q, _| overwrite(q, vec![(V1, b)])).unwrap_err(),
        EditError::Locked
    );
    assert_eq!(
        w.edit(|q, c| trim(q, &ids, Edge::Out, false, s(-1.0), c))
            .unwrap_err(),
        EditError::Locked
    );
    assert!(w.edit(|q, _| Ok(razor(q, &[], s(2.0)))).unwrap().is_empty());
    assert_eq!(
        w.edit(|q, _| ripple_delete(q, &ids)).unwrap_err(),
        EditError::Locked
    );
}

#[test]
fn paste_makes_copies_with_their_own_ids_and_links() {
    let mut w = World::new();
    let (a, m) = (w.clip(0.0, 2.0), w.clip(0.0, 2.0));
    let ids = w
        .edit(|q, _| {
            let ids = overwrite(q, vec![(V1, a), (A1, m)])?;
            link(q, &ids)?;
            Ok(ids)
        })
        .unwrap();
    let q = w.sequence();
    let copied: Vec<(usize, Clip)> = ids
        .iter()
        .map(|id| {
            let (track, clip) = q.find(*id).unwrap();
            (track, clip.clone())
        })
        .collect();
    let pasted = w.edit(|q, _| paste(q, &copied, s(5.0))).unwrap();
    let q = w.sequence();
    let (one, two) = (q.find(pasted[0]).unwrap().1, q.find(pasted[1]).unwrap().1);
    assert_eq!(one.start, s(5.0));
    assert!(one.link == two.link && one.link != q.find(ids[0]).unwrap().1.link);
}
