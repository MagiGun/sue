//! Layer keyframes, as Clip Studio Paint's "Enable keyframes on this
//! layer": a layer, folder or animation folder moves, turns, scales and
//! fades along the timeline. The keys are the shared [`Animation`] on the
//! layer (the camera folder's use the same), one curve a value
//! ([`POSITION_X`] to [`OPACITY`]), each key with its own [`Ease`].
//!
//! The keys count only while the layer's [`KEYFRAMES`] flag is on, so
//! turning keyframes off keeps them for later, as Clip Studio Paint does.

use sue_core::txn::{SetExtension, SetProperty};
use sue_core::{Arena, Entity, EntityId, Rate, Time, Value};
use sue_model::{Animation, Curve, Interpolation, Key};

use super::Edit;

/// Keyframes count on this layer (`Bool`): Enable keyframes on this
/// layer.
pub const KEYFRAMES: &str = "anim.keyframes";
/// How far the layer is moved across, in picture pixels.
pub const POSITION_X: &str = "layer.position.x";
/// How far the layer is moved down, in picture pixels.
pub const POSITION_Y: &str = "layer.position.y";
/// How much the layer is scaled (1 as it is).
pub const SCALE: &str = "layer.scale";
/// How far the layer is turned, in degrees, clockwise.
pub const ROTATION: &str = "layer.rotation";
/// The centre the layer turns and scales about, across (picture pixels).
pub const CENTER_X: &str = "layer.center.x";
/// The centre the layer turns and scales about, down (picture pixels).
pub const CENTER_Y: &str = "layer.center.y";
/// The layer's opacity, 0 to 1.
pub const OPACITY: &str = "layer.opacity";

/// Every value a layer keyframe can hold, in the order the timeline
/// shows them (Position, Scale ratio, Rotate, Center of rotation,
/// Opacity).
pub const VALUES: [&str; 7] = [
    POSITION_X, POSITION_Y, SCALE, ROTATION, CENTER_X, CENTER_Y, OPACITY,
];

/// How a key moves on to the next one: Clip Studio Paint's three kinds of
/// keyframe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ease {
    /// Holds its value until the next key.
    Hold,
    /// A straight line to the next key.
    Linear,
    /// A smooth curve through the keys around it.
    Smooth,
}

impl Ease {
    /// The three, in the order Clip Studio Paint lists them.
    pub const ALL: [Self; 3] = [Self::Smooth, Self::Linear, Self::Hold];

    /// The stable name (saved with presets and shortcuts).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Linear => "linear",
            Self::Smooth => "smooth",
        }
    }

    /// The kind named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|ease| ease.name() == name)
    }

    /// The kind of `interpolation`: a Bezier is Smooth.
    #[must_use]
    pub const fn of(interpolation: Interpolation) -> Self {
        match interpolation {
            Interpolation::Constant => Self::Hold,
            Interpolation::Linear => Self::Linear,
            Interpolation::Bezier { .. } => Self::Smooth,
        }
    }

    /// The interpolation it starts as (Smooth's handles are set by
    /// [`smoothed`] once the keys around are known).
    #[must_use]
    pub const fn interpolation(self) -> Interpolation {
        match self {
            Self::Hold => Interpolation::Constant,
            Self::Linear => Interpolation::Linear,
            Self::Smooth => Interpolation::Bezier {
                out: [0.0; 2],
                incoming: [0.0; 2],
            },
        }
    }
}

/// `curve` with the handles of every Smooth key set from the keys around
/// it: each key's slope is the one from the key before to the key after
/// (flat at the first and the last), and a handle reaches a third of the
/// way to the next key. Two keys ease in and out; more pass smoothly
/// through. Called after every change, so the handles never go stale.
#[must_use]
pub fn smoothed(curve: &Curve) -> Curve {
    let keys = curve.keys();
    let slope = |index: usize| -> f64 {
        if index == 0 || index + 1 >= keys.len() {
            return 0.0;
        }
        let (before, after) = (keys[index - 1], keys[index + 1]);
        let span = (after.time - before.time).seconds();
        if span <= 0.0 {
            0.0
        } else {
            (after.value - before.value) / span
        }
    };
    let mut out = Curve::new();
    for (index, key) in keys.iter().enumerate() {
        let mut key = *key;
        if let (Interpolation::Bezier { .. }, Some(next)) = (key.interpolation, keys.get(index + 1))
        {
            let third = (next.time - key.time).seconds() / 3.0;
            key.interpolation = Interpolation::Bezier {
                out: [third, slope(index) * third],
                incoming: [-third, -slope(index + 1) * third],
            };
        }
        out.insert(key);
    }
    out
}

/// `true` if keyframes count on `entity`.
#[must_use]
pub fn keyframes_on(entity: &Entity) -> bool {
    entity.property(KEYFRAMES).and_then(Value::as_bool) == Some(true)
}

/// The edit that turns keyframes on `layer` on or off (the keys stay).
#[must_use]
pub fn enable_keyframes(layer: EntityId, on: bool) -> Edit {
    Edit::new(tr!("enable-keyframes-on-this-layer")).then(SetProperty::new(layer, KEYFRAMES, on))
}

/// How a layer stands at a frame: where its keyframes put it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Moved across and down (picture pixels).
    pub position: [f32; 2],
    /// Scaled (1 as it is).
    pub scale: f32,
    /// Turned, in degrees clockwise.
    pub rotation: f32,
    /// The point it turns and scales about (picture pixels).
    pub center: [f32; 2],
    /// Its opacity, when a key sets it.
    pub opacity: Option<f32>,
}

impl Pose {
    /// Where a layer stands with no keys: as it is, turning about the
    /// middle of a `size` canvas.
    #[must_use]
    pub fn still(size: [u32; 2]) -> Self {
        Self {
            position: [0.0; 2],
            scale: 1.0,
            rotation: 0.0,
            center: [size[0] as f32 / 2.0, size[1] as f32 / 2.0],
            opacity: None,
        }
    }

    /// `true` if it does not move the layer.
    #[must_use]
    pub fn is_still(&self) -> bool {
        self.position == [0.0; 2] && (self.scale - 1.0).abs() < 1e-6 && self.rotation.abs() < 1e-6
    }

    /// What each canvas point sees on the layer standing so, as an affine
    /// map (see [`crate::composite::compose`]): the layer is scaled and
    /// turned about its centre, then moved.
    #[must_use]
    pub fn map(&self) -> [f32; 6] {
        let s = self.scale.max(0.001);
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        // The inverse of: turn and scale about the centre, then move.
        let (a, b, c, d) = (cos / s, sin / s, -sin / s, cos / s);
        let [cx, cy] = self.center;
        let (px, py) = (cx + self.position[0], cy + self.position[1]);
        [a, b, c, d, cx - (a * px + b * py), cy - (c * px + d * py)]
    }
}

/// Where `layer` stands at `time` by its keyframes on a `size` canvas;
/// `None` when it has none that count.
#[must_use]
pub fn pose(arena: &Arena, layer: EntityId, time: Time, size: [u32; 2]) -> Option<Pose> {
    let entity = arena.get_by_id(layer).ok()?;
    if !keyframes_on(entity) {
        return None;
    }
    let animation = entity.ext::<Animation>()?;
    let value = |channel: &str| {
        animation
            .channel(channel)
            .and_then(|curve| curve.sample(time))
            .map(|value| value as f32)
    };
    let still = Pose::still(size);
    Some(Pose {
        position: [
            value(POSITION_X).unwrap_or(0.0),
            value(POSITION_Y).unwrap_or(0.0),
        ],
        scale: value(SCALE).unwrap_or(1.0),
        rotation: value(ROTATION).unwrap_or(0.0),
        center: [
            value(CENTER_X).unwrap_or(still.center[0]),
            value(CENTER_Y).unwrap_or(still.center[1]),
        ],
        opacity: value(OPACITY).map(|opacity| opacity.clamp(0.0, 1.0)),
    })
}

/// What keyframes do to the layers at a frame: each moved layer with the
/// map it is seen through, and each faded one with its opacity.
pub type Poses = (Vec<(EntityId, [f32; 6])>, Vec<(EntityId, f32)>);

/// The layers of `picture` that keyframes move or fade at `time`: the
/// maps and opacities a frame view shows them with.
#[must_use]
pub fn poses(arena: &Arena, picture: EntityId, time: Time, size: [u32; 2]) -> Poses {
    let mut moves = Vec::new();
    let mut opacities = Vec::new();
    for info in crate::doc::layers(arena, picture) {
        let Some(pose) = pose(arena, info.id, time, size) else {
            continue;
        };
        if !pose.is_still() {
            moves.push((info.id, pose.map()));
        }
        if let Some(opacity) = pose.opacity {
            opacities.push((info.id, opacity));
        }
    }
    (moves, opacities)
}

/// The animation of `layer` (empty when it has none).
fn animation_of(arena: &Arena, layer: EntityId) -> Animation {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .cloned()
        .unwrap_or_default()
}

/// `animation` with every channel's Smooth handles set again.
fn resmoothed(mut animation: Animation) -> Animation {
    let names: Vec<String> = animation
        .channels()
        .map(|(name, _)| name.to_owned())
        .collect();
    for name in names {
        let smooth = animation.channel(&name).map(smoothed).unwrap_or_default();
        *animation.channel_mut(&name) = smooth;
    }
    animation
}

/// The edit that keys `values` (channel and value) of `layer` at `time`
/// with `ease`, replacing keys there. Keys turn keyframes on, as adding a
/// keyframe does in Clip Studio Paint.
#[must_use]
pub fn key(arena: &Arena, layer: EntityId, time: Time, values: &[(&str, f64)], ease: Ease) -> Edit {
    let mut animation = animation_of(arena, layer);
    for (channel, value) in values {
        animation.channel_mut(channel).insert(Key {
            time,
            value: *value,
            interpolation: ease.interpolation(),
        });
    }
    let mut edit =
        Edit::new(tr!("add-keyframe")).then(SetExtension::set(layer, resmoothed(animation)));
    let on = arena.get_by_id(layer).is_ok_and(keyframes_on);
    if !on {
        edit = edit.then(SetProperty::new(layer, KEYFRAMES, true));
    }
    edit
}

/// The edit that keys every value of `layer` at `time` as `pose` stands.
#[must_use]
pub fn key_pose(arena: &Arena, layer: EntityId, time: Time, pose: &Pose, ease: Ease) -> Edit {
    let mut values = vec![
        (POSITION_X, f64::from(pose.position[0])),
        (POSITION_Y, f64::from(pose.position[1])),
        (SCALE, f64::from(pose.scale)),
        (ROTATION, f64::from(pose.rotation)),
        (CENTER_X, f64::from(pose.center[0])),
        (CENTER_Y, f64::from(pose.center[1])),
    ];
    if let Some(opacity) = pose.opacity {
        values.push((OPACITY, f64::from(opacity)));
    }
    key(arena, layer, time, &values, ease)
}

/// The times `layer` has keyframes at, on any value.
#[must_use]
pub fn key_times(arena: &Arena, layer: EntityId) -> Vec<Time> {
    let mut out: Vec<Time> = animation_of(arena, layer)
        .channels()
        .flat_map(|(_, curve)| curve.keys().iter().map(|key| key.time))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The edit that takes `layer`'s keys at `time` away (every value, or
/// only `channel`); `None` when there are none.
#[must_use]
pub fn delete_key(
    arena: &Arena,
    layer: EntityId,
    time: Time,
    channel: Option<&str>,
) -> Option<Edit> {
    let mut animation = animation_of(arena, layer);
    let names: Vec<String> = animation
        .channels()
        .map(|(name, _)| name.to_owned())
        .filter(|name| channel.is_none_or(|only| only == name))
        .collect();
    let mut any = false;
    for name in names {
        any |= animation.channel_mut(&name).remove(time).is_some();
    }
    any.then(|| {
        Edit::new(tr!("delete-keyframe")).then(SetExtension::set(layer, resmoothed(animation)))
    })
}

/// The edit that takes every key of `layer` away (Delete all keyframes).
#[must_use]
pub fn delete_all_keys(arena: &Arena, layer: EntityId) -> Option<Edit> {
    let animation = arena.get_by_id(layer).ok()?.ext::<Animation>()?;
    let mut empty = animation.clone();
    let names: Vec<String> = empty.channels().map(|(name, _)| name.to_owned()).collect();
    for name in &names {
        empty.remove_channel(name);
    }
    Some(Edit::new(tr!("delete-all-keyframes")).then(SetExtension::set(layer, empty)))
}

/// The edit that switches `layer`'s keys at `times` to `ease` (Switch
/// keyframe to Hold, Linear or Smooth interpolation); `None` when none
/// change.
#[must_use]
pub fn set_ease(arena: &Arena, layer: EntityId, times: &[Time], ease: Ease) -> Option<Edit> {
    let mut animation = animation_of(arena, layer);
    let names: Vec<String> = animation
        .channels()
        .map(|(name, _)| name.to_owned())
        .collect();
    let mut any = false;
    for name in names {
        let curve = animation.channel_mut(&name);
        let keys: Vec<Key> = curve.keys().to_vec();
        for mut key in keys {
            if times.contains(&key.time) && Ease::of(key.interpolation) != ease {
                key.interpolation = ease.interpolation();
                curve.insert(key);
                any = true;
            }
        }
    }
    any.then(|| {
        Edit::new(tr!("switch-keyframe-interpolation"))
            .then(SetExtension::set(layer, resmoothed(animation)))
    })
}

/// A key of one value as Copy Frames takes it: its frame from the first
/// frame copied, the value's channel, its value and how it moves on.
#[derive(Clone, Debug, PartialEq)]
pub struct CopiedKey {
    /// Frames after the first frame copied.
    pub offset: u32,
    /// The value's channel.
    pub channel: String,
    /// The value.
    pub value: f64,
    /// How it moves on to the next key.
    pub interpolation: sue_model::Interpolation,
}

/// The keys of `layer` on frames `start` to before `end` at `rate` (Copy
/// Frames on its track).
#[must_use]
pub fn copy_keys(
    arena: &Arena,
    layer: EntityId,
    rate: Rate,
    start: u32,
    end: u32,
) -> Vec<CopiedKey> {
    let mut out = Vec::new();
    for (name, curve) in animation_of(arena, layer).channels() {
        for key in curve.keys() {
            let frame = key.time.frame(rate);
            if frame >= i64::from(start) && frame < i64::from(end) {
                out.push(CopiedKey {
                    offset: (frame - i64::from(start)) as u32,
                    channel: name.to_owned(),
                    value: key.value,
                    interpolation: key.interpolation,
                });
            }
        }
    }
    out
}

/// The edit that lays `keys` on `layer` from frame `at` at `rate`,
/// replacing keys there (Paste Frames on its track); `None` when there are
/// none.
#[must_use]
pub fn paste_keys(
    arena: &Arena,
    layer: EntityId,
    rate: Rate,
    at: u32,
    keys: &[CopiedKey],
) -> Option<Edit> {
    if keys.is_empty() {
        return None;
    }
    let mut animation = animation_of(arena, layer);
    for key in keys {
        animation.channel_mut(&key.channel).insert(Key {
            time: Time::from_frames(i64::from(at + key.offset), rate),
            value: key.value,
            interpolation: key.interpolation,
        });
    }
    let mut edit =
        Edit::new(tr!("paste-frames")).then(SetExtension::set(layer, resmoothed(animation)));
    if !arena.get_by_id(layer).is_ok_and(keyframes_on) {
        edit = edit.then(SetProperty::new(layer, KEYFRAMES, true));
    }
    Some(edit)
}

/// The kind of `layer`'s key at `time` (the first value keyed there).
#[must_use]
pub fn ease_at(arena: &Arena, layer: EntityId, time: Time) -> Option<Ease> {
    animation_of(arena, layer)
        .channels()
        .find_map(|(_, curve)| {
            curve
                .keys()
                .iter()
                .find(|key| key.time == time)
                .map(|key| Ease::of(key.interpolation))
        })
}

/// `animation` with every key after `frame` moved `by` frames at `rate`; a
/// key moved onto or before `frame` by a deletion goes.
#[must_use]
pub fn shifted(animation: &Animation, frame: u32, by: i64, rate: Rate) -> Animation {
    let mut out = animation.clone();
    let names: Vec<String> = animation
        .channels()
        .map(|(name, _)| name.to_owned())
        .collect();
    for name in names {
        let Some(curve) = animation.channel(&name) else {
            continue;
        };
        let mut moved = Curve::new();
        for key in curve.keys() {
            let at = key.time.frame(rate);
            let mut key = *key;
            if at > i64::from(frame) {
                let to = at + by;
                if to <= i64::from(frame) {
                    continue;
                }
                key.time = Time::from_frames(to, rate);
            }
            moved.insert(key);
        }
        *out.channel_mut(&name) = smoothed(&moved);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{NewPicture, create};
    use sue_core::ReversibleTransaction;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn eases_read_back_by_name_and_map_to_interpolations() {
        for ease in Ease::ALL {
            assert_eq!(Ease::from_name(ease.name()), Some(ease));
            assert_eq!(Ease::of(ease.interpolation()), ease);
        }
        assert_eq!(Ease::from_name("bounce"), None);
    }

    #[test]
    fn a_tween_between_two_keys_stands_as_its_ease_says_at_the_middle() {
        let size = [100, 100];
        let rate = Rate::new(10, 1);
        let at = |frame: i64| Time::from_frames(frame, rate);
        for (ease, middle, quarter) in [
            (Ease::Hold, 0.0, 0.0),
            (Ease::Linear, 20.0, 10.0),
            // Smooth eases out of the first key: the middle is half way
            // (it is symmetric), a quarter of the way is behind linear.
            (Ease::Smooth, 20.0, f32::NAN),
        ] {
            let mut arena = Arena::new();
            let (mut made, picture) = create(&NewPicture {
                width: 100,
                height: 100,
                ..NewPicture::default()
            })
            .unwrap();
            made.apply(&mut arena).unwrap();
            let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
            let before = arena.get_by_id(layer).unwrap().clone();
            let mut first = key(
                &arena,
                layer,
                at(0),
                &[(POSITION_X, 0.0), (OPACITY, 1.0)],
                ease,
            );
            first.apply(&mut arena).unwrap();
            let mut second = key(
                &arena,
                layer,
                at(8),
                &[(POSITION_X, 40.0), (OPACITY, 0.0)],
                ease,
            );
            second.apply(&mut arena).unwrap();
            let now = pose(&arena, layer, at(4), size).expect("keyframes are on");
            assert!(near(now.position[0], middle), "{ease:?}: {now:?}");
            let fourth = pose(&arena, layer, at(2), size).unwrap();
            if quarter.is_nan() {
                assert!(
                    fourth.position[0] > 0.0 && fourth.position[0] < 10.0,
                    "smooth starts slowly: {fourth:?}"
                );
            } else {
                assert!(near(fourth.position[0], quarter), "{ease:?}: {fourth:?}");
            }
            // The composite shows it: a pixel at the left edge is seen
            // 20 pixels on in the middle of a linear tween.
            let map = now.map();
            if ease != Ease::Hold {
                assert!(near(map[4], -20.0), "{map:?}");
            }
            // Undo, both keys: the layer is as it was.
            second.revert(&mut arena).unwrap();
            first.revert(&mut arena).unwrap();
            assert_eq!(arena.get_by_id(layer).unwrap(), &before);
        }
    }

    #[test]
    fn three_smooth_keys_pass_smoothly_through_the_middle_one() {
        let rate = Rate::new(10, 1);
        let mut curve = Curve::new();
        for (frame, value) in [(0, 0.0), (10, 10.0), (20, 20.0)] {
            curve.insert(Key {
                time: Time::from_frames(frame, rate),
                value,
                interpolation: Ease::Smooth.interpolation(),
            });
        }
        let curve = smoothed(&curve);
        // On a straight run the middle key keeps the run's slope.
        let near_middle = curve.sample(Time::from_frames(11, rate)).unwrap();
        assert!((near_middle - 11.0).abs() < 0.3, "{near_middle}");
    }

    #[test]
    fn keys_switch_ease_go_and_turn_off_with_undo() {
        let mut arena = Arena::new();
        let (mut made, picture) = create(&NewPicture::default()).unwrap();
        made.apply(&mut arena).unwrap();
        let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
        let rate = Rate::new(24, 1);
        let at = |frame: i64| Time::from_frames(frame, rate);
        let mut pose_now = Pose::still([64, 64]);
        let mut one = key_pose(&arena, layer, at(0), &pose_now, Ease::Linear);
        one.apply(&mut arena).unwrap();
        pose_now.rotation = 90.0;
        let mut two = key_pose(&arena, layer, at(12), &pose_now, Ease::Linear);
        two.apply(&mut arena).unwrap();
        assert_eq!(key_times(&arena, layer), vec![at(0), at(12)]);
        let snapshot = arena.get_by_id(layer).unwrap().clone();
        let mut hold = set_ease(&arena, layer, &[at(0)], Ease::Hold).unwrap();
        hold.apply(&mut arena).unwrap();
        assert_eq!(ease_at(&arena, layer, at(0)), Some(Ease::Hold));
        assert!(near(
            pose(&arena, layer, at(6), [64, 64]).unwrap().rotation,
            0.0
        ));
        assert!(
            set_ease(&arena, layer, &[at(0)], Ease::Hold).is_none(),
            "nothing to change"
        );
        hold.revert(&mut arena).unwrap();
        assert_eq!(arena.get_by_id(layer).unwrap(), &snapshot);
        // Off keeps the keys but they no longer count.
        let mut off = enable_keyframes(layer, false);
        off.apply(&mut arena).unwrap();
        assert!(pose(&arena, layer, at(6), [64, 64]).is_none());
        assert_eq!(key_times(&arena, layer).len(), 2);
        off.revert(&mut arena).unwrap();
        let mut gone = delete_key(&arena, layer, at(12), Some(ROTATION)).unwrap();
        gone.apply(&mut arena).unwrap();
        assert_eq!(key_times(&arena, layer).len(), 2, "the other values stay");
        gone.revert(&mut arena).unwrap();
        let mut all = delete_all_keys(&arena, layer).unwrap();
        all.apply(&mut arena).unwrap();
        assert!(key_times(&arena, layer).is_empty());
        all.revert(&mut arena).unwrap();
        assert_eq!(arena.get_by_id(layer).unwrap(), &snapshot);
    }
}
