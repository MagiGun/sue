//! Keyframes beyond adding and removing them: eases (After Effects'
//! Easy Ease, Ease In and Ease Out, linear and hold steps), moving a key
//! in time and value, a curve's speed (what the graph editor's speed
//! graph draws), time remapping with speed ramps, the rate stretch tool,
//! and clip markers. Pure functions over a sequence, as the edit algebra
//! is: the studio runs them on a copy and makes one undo step.

use sue_core::{Time, Value};
use sue_model::{Clip, Curve, Interpolation, Key, Sequence};

use crate::edit::{Edge, EditError};
use crate::fx::key as fx_key;

/// How a key is eased.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    /// Slow out of and into the key (After Effects' F9).
    Both,
    /// Slow into the key.
    In,
    /// Slow out of the key.
    Out,
    /// Straight lines on both sides.
    Linear,
    /// Holds its value until the next key.
    Hold,
}

/// How far an ease's handle reaches, a share of its segment (After
/// Effects' 33.33 % influence).
pub const INFLUENCE: f64 = 1.0 / 3.0;

/// Eases the key at source time `at` on `curve` as `ease` says.
/// `false` when there is no key there.
pub fn ease_key(curve: &mut Curve, at: Time, ease: Ease) -> bool {
    let keys: Vec<Key> = curve.keys().to_vec();
    let Some(index) = keys.iter().position(|key| key.time == at) else {
        return false;
    };
    let span = |a: usize, b: usize| (keys[b].time - keys[a].time).seconds();
    // The segment leaving this key is set on this key; the one arriving
    // is set on the key before.
    let leaving = keys.get(index + 1).map(|_| span(index, index + 1));
    let arriving = index
        .checked_sub(1)
        .map(|before| (before, span(before, index)));
    let set = |curve: &mut Curve, at: usize, interpolation: Interpolation| {
        curve.insert(Key {
            interpolation,
            ..keys[at]
        });
    };
    let handles = |interpolation: Interpolation| match interpolation {
        Interpolation::Bezier { out, incoming } => (out, incoming),
        _ => ([0.0, 0.0], [0.0, 0.0]),
    };
    match ease {
        Ease::Hold => set(curve, index, Interpolation::Constant),
        Ease::Linear => {
            set(curve, index, Interpolation::Linear);
            if let Some((before, _)) = arriving
                && matches!(keys[before].interpolation, Interpolation::Bezier { .. })
            {
                set(curve, before, Interpolation::Linear);
            }
        }
        Ease::Both | Ease::In | Ease::Out => {
            if matches!(ease, Ease::Both | Ease::Out)
                && let Some(span) = leaving
            {
                let (_, incoming) = handles(keys[index].interpolation);
                let incoming = if matches!(keys[index].interpolation, Interpolation::Bezier { .. })
                {
                    incoming
                } else {
                    [
                        -span * INFLUENCE,
                        (keys[index + 1].value - keys[index].value) * -INFLUENCE,
                    ]
                };
                set(
                    curve,
                    index,
                    Interpolation::Bezier {
                        out: [span * INFLUENCE, 0.0],
                        incoming,
                    },
                );
            }
            if matches!(ease, Ease::Both | Ease::In)
                && let Some((before, span)) = arriving
            {
                let current = curve
                    .keys()
                    .iter()
                    .find(|key| key.time == keys[before].time)
                    .map_or(keys[before].interpolation, |key| key.interpolation);
                let out = match current {
                    Interpolation::Bezier { out, .. } => out,
                    _ => [
                        span * INFLUENCE,
                        (keys[index].value - keys[before].value) * INFLUENCE,
                    ],
                };
                set(
                    curve,
                    before,
                    Interpolation::Bezier {
                        out,
                        incoming: [-span * INFLUENCE, 0.0],
                    },
                );
            }
        }
    }
    true
}

/// Moves the key at source time `from` on `curve` to `to` with `value`
/// (it may not pass the keys beside it). The key's new time.
pub fn move_key(curve: &mut Curve, from: Time, to: Time, value: f64) -> Option<Time> {
    let keys = curve.keys().to_vec();
    let index = keys.iter().position(|key| key.time == from)?;
    let step = Time(1);
    let low = index.checked_sub(1).map(|before| keys[before].time + step);
    let high = keys.get(index + 1).map(|next| next.time - step);
    let mut time = to;
    if let Some(low) = low {
        time = time.max(low);
    }
    if let Some(high) = high {
        time = time.min(high);
    }
    let key = curve.remove(from)?;
    curve.insert(Key { time, value, ..key });
    Some(time)
}

/// The speed of `curve` at source time `time`: its change a second.
#[must_use]
pub fn speed(curve: &Curve, time: Time) -> f64 {
    let h = 1.0 / 240.0;
    let at = |t: f64| curve.sample(Time::from_seconds(t)).unwrap_or(0.0);
    let t = time.seconds();
    (at(t + h) - at(t - h)) / (2.0 * h)
}

/// Turns time remapping on for `clip`: keys at its in and out points
/// that play it as it plays now. `false` when it has them already.
pub fn enable_remap(clip: &mut Clip) -> bool {
    if clip.animation.channel(fx_key::REMAP).is_some() {
        return false;
    }
    let (start, end) = (clip.source_in, clip.source_out());
    let curve = clip.animation.channel_mut(fx_key::REMAP);
    for time in [start, end] {
        curve.insert(Key {
            time,
            value: time.seconds(),
            interpolation: Interpolation::Linear,
        });
    }
    true
}

/// A speed ramp: from sequence time `at` on, `clip` plays at `speed`
/// (1 is normal) until its next remap key, which moves to keep it so; a
/// key is added at `at` first. Time remapping is turned on if it was off.
///
/// # Errors
/// [`EditError::Nothing`] when `at` is outside the clip.
pub fn ramp(clip: &mut Clip, at: Time, speed: f64) -> Result<(), EditError> {
    if at < clip.start || at >= clip.end() {
        return Err(EditError::Nothing);
    }
    enable_remap(clip);
    let local = clip.source_time(at);
    let current = crate::render::source_time_of(clip, at).seconds();
    let curve = clip.animation.channel_mut(fx_key::REMAP);
    if !curve.keys().iter().any(|key| key.time == local) {
        curve.insert(Key {
            time: local,
            value: current,
            interpolation: Interpolation::Linear,
        });
    }
    let keys = curve.keys().to_vec();
    let index = keys
        .iter()
        .position(|key| key.time == local)
        .ok_or(EditError::Nothing)?;
    if let Some(next) = keys.get(index + 1) {
        let span = (next.time - local).seconds();
        // What follows keeps its own speed: it moves by the same amount.
        let shift = current + span * speed - next.value;
        for later in &keys[index + 1..] {
            curve.insert(Key {
                value: (later.value + shift).max(0.0),
                ..*later
            });
        }
    }
    Ok(())
}

/// The rate stretch tool: changes clips' speed so their frames fill the
/// new length when edge `edge` moves by `delta` (Premiere Pro's Rate
/// Stretch): the same frames, played faster or slower.
///
/// # Errors
/// A missing clip, a clip that would be shorter than a frame, or one that
/// would run into its neighbour.
pub fn rate_stretch(
    sequence: &mut Sequence,
    ids: &[u64],
    edge: Edge,
    delta: Time,
) -> Result<(), EditError> {
    let frame = crate::edit::frame(sequence);
    for &id in ids {
        let (track, clip) = sequence.find(id).ok_or(EditError::NoClip(id))?;
        let (start, end) = (clip.start, clip.end());
        let (new_start, new_end) = match edge {
            Edge::In => (start + delta, end),
            Edge::Out => (start, end + delta),
        };
        if new_end - new_start < frame {
            return Err(EditError::TooShort);
        }
        // Not over a neighbour.
        let clear = sequence.tracks[track]
            .clips()
            .iter()
            .all(|other| other.id == id || other.end() <= new_start || other.start >= new_end);
        if !clear {
            return Err(EditError::NotAdjacent);
        }
        let (_, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
        let ratio = clip.duration.seconds() / (new_end - new_start).seconds();
        clip.speed *= ratio;
        clip.start = new_start;
        clip.duration = new_end - new_start;
        sequence.tracks[track].sort();
    }
    Ok(())
}

/// A clip's markers: source times (so they move with the frames), each
/// with a name.
#[must_use]
pub fn clip_markers(clip: &Clip) -> Vec<(Time, String)> {
    match clip.properties.get(MARKERS) {
        Some(Value::List(list)) => list
            .iter()
            .filter_map(|value| match value {
                Value::Map(map) => Some((
                    match map.get("at") {
                        Some(Value::Time(time)) => *time,
                        _ => return None,
                    },
                    map.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                )),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The clip property its markers are kept in.
pub const MARKERS: &str = "ve.markers";

/// Adds a marker named `name` to `clip` at sequence time `at`.
///
/// # Errors
/// [`EditError::Nothing`] when `at` is outside the clip.
pub fn add_clip_marker(clip: &mut Clip, at: Time, name: &str) -> Result<(), EditError> {
    if at < clip.start || at > clip.end() {
        return Err(EditError::Nothing);
    }
    let time = clip.source_time(at);
    let mut list = clip_markers(clip);
    list.retain(|(existing, _)| *existing != time);
    list.push((time, name.to_owned()));
    list.sort_by_key(|(time, _)| *time);
    clip.properties.set(
        MARKERS,
        Value::List(
            list.into_iter()
                .map(|(at, name)| {
                    let mut map = std::collections::BTreeMap::new();
                    map.insert("at".to_owned(), Value::Time(at));
                    map.insert("name".to_owned(), Value::Text(name));
                    Value::Map(map)
                })
                .collect(),
        ),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{EntityId, Rate};

    fn s(seconds: f64) -> Time {
        Time::from_seconds(seconds)
    }

    fn curve() -> Curve {
        let mut curve = Curve::new();
        for (time, value) in [(0.0, 0.0), (1.0, 100.0), (3.0, 100.0)] {
            curve.insert(Key {
                time: s(time),
                value,
                interpolation: Interpolation::Linear,
            });
        }
        curve
    }

    #[test]
    fn easy_ease_slows_both_sides_and_the_others_shape_one() {
        let mut eased = curve();
        assert!(ease_key(&mut eased, s(1.0), Ease::Both));
        // Arriving at 100 slowly: the speed just before the key is near 0.
        assert!(speed(&eased, s(0.98)) < 30.0, "{}", speed(&eased, s(0.98)));
        assert!(speed(&curve(), s(0.98)) > 99.0);
        // With the first key eased too, half way is half way (symmetric).
        assert!(ease_key(&mut eased, s(0.0), Ease::Both));
        assert!((eased.sample(s(0.5)).unwrap() - 50.0).abs() < 1e-6);
        assert!(speed(&eased, s(0.02)) < 30.0);
        let mut held = curve();
        assert!(ease_key(&mut held, s(0.0), Ease::Hold));
        assert_eq!(held.sample(s(0.9)), Some(0.0));
        assert!(ease_key(&mut held, s(0.0), Ease::Linear));
        assert_eq!(held.sample(s(0.5)), Some(50.0));
        let mut out = curve();
        assert!(ease_key(&mut out, s(0.0), Ease::Out));
        assert!(speed(&out, s(0.02)) < 30.0);
        assert!(!ease_key(&mut out, s(0.5), Ease::Both), "no key there");
    }

    #[test]
    fn a_key_moves_but_never_past_its_neighbours() {
        let mut moved = curve();
        assert_eq!(move_key(&mut moved, s(1.0), s(2.0), 50.0), Some(s(2.0)));
        assert_eq!(moved.sample(s(2.0)), Some(50.0));
        let stopped = move_key(&mut moved, s(2.0), s(9.0), 50.0).unwrap();
        assert!(stopped < s(3.0));
        assert_eq!(moved.keys().len(), 3);
    }

    #[test]
    fn time_remapping_and_speed_ramps() {
        let mut clip = Clip::new(EntityId::new(), s(10.0), s(4.0));
        clip.source_in = s(2.0);
        assert!(enable_remap(&mut clip));
        assert!(!enable_remap(&mut clip));
        assert_eq!(crate::render::source_time_of(&clip, s(11.0)), s(3.0));
        // From 12 s on: half speed until the out point's key.
        ramp(&mut clip, s(12.0), 0.5).unwrap();
        let at = |seconds: f64| crate::render::source_time_of(&clip, s(seconds)).seconds();
        assert!((at(12.0) - 4.0).abs() < 1e-6);
        assert!((at(13.0) - 4.5).abs() < 1e-6, "{}", at(13.0));
        let curve = clip.animation.channel(fx_key::REMAP).unwrap();
        assert!((speed(curve, s(5.0)) - 0.5).abs() < 1e-3);
        assert!(ramp(&mut clip, s(30.0), 2.0).is_err());
    }

    #[test]
    fn rate_stretch_changes_speed_not_frames() {
        let mut sequence = crate::model::empty_sequence(64, 36, Rate::FPS_25, 1, 0);
        let mut a = Clip::new(EntityId::new(), Time::ZERO, s(2.0));
        a.id = 1;
        let mut b = Clip::new(EntityId::new(), s(5.0), s(1.0));
        b.id = 2;
        sequence.tracks[0].insert(a);
        sequence.tracks[0].insert(b);
        sequence.next_id = 3;
        rate_stretch(&mut sequence, &[1], Edge::Out, s(2.0)).unwrap();
        let (_, clip) = sequence.find(1).unwrap();
        assert_eq!(clip.duration, s(4.0));
        assert!((clip.speed - 0.5).abs() < 1e-9);
        assert_eq!(clip.source_out(), s(2.0), "the same frames");
        assert_eq!(
            rate_stretch(&mut sequence, &[1], Edge::Out, s(2.0)),
            Err(EditError::NotAdjacent)
        );
        rate_stretch(&mut sequence, &[2], Edge::In, s(0.5)).unwrap();
        let (_, clip) = sequence.find(2).unwrap();
        assert_eq!(clip.start, s(5.5));
        assert!((clip.speed - 2.0).abs() < 1e-9);
    }

    #[test]
    fn clip_markers_follow_the_frames() {
        let mut clip = Clip::new(EntityId::new(), s(10.0), s(4.0));
        clip.source_in = s(1.0);
        add_clip_marker(&mut clip, s(11.0), "Look").unwrap();
        add_clip_marker(&mut clip, s(10.5), "First").unwrap();
        assert_eq!(
            clip_markers(&clip),
            [(s(1.5), "First".to_owned()), (s(2.0), "Look".to_owned())]
        );
        assert!(add_clip_marker(&mut clip, s(20.0), "Out").is_err());
    }
}
