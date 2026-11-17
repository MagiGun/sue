//! The camera folder, as in Clip Studio Paint's animation: a layer folder
//! whose layers are seen through a camera that moves, zooms and turns along
//! the timeline. A camera folder is an ordinary folder flagged [`CAMERA`];
//! where the camera stands is its [`X`], [`Y`] (the middle of its view, in
//! picture pixels), [`SCALE`] and [`ANGLE`] (radians) properties, and its
//! keyframes are the shared [`Animation`] on the folder, a curve each.
//!
//! At a frame the compositor shows the folder's layers as the camera sees
//! them: what is in the camera's view fills the canvas.
//!
//! As in Clip Studio Paint, the camera is a track on the timeline (kind
//! [`crate::anim::setup::CAMERA`]); a new camera folder is empty, each
//! value is keyed on its own with its own ease, the camera turns about a
//! centre of rotation ([`PIVOT_X`], [`PIVOT_Y`]; its own middle when
//! none) and keys the folder's opacity ([`OPACITY`]).

use sue_core::txn::{SetExtension, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, Relation, Time, Value};
use sue_model::{Animation, Interpolation, Key, Sequence};

use crate::anim::motion::{Ease, smoothed};

use crate::doc::{self, Edit};

/// A folder that is a camera folder (`Bool`).
pub const CAMERA: &str = "folder.camera";
/// The middle of the camera's view, across (picture pixels).
pub const X: &str = "camera.x";
/// The middle of the camera's view, down (picture pixels).
pub const Y: &str = "camera.y";
/// How much the camera zooms in, 1 showing the canvas as it is.
pub const SCALE: &str = "camera.scale";
/// How far the camera turns, in radians.
pub const ANGLE: &str = "camera.angle";
/// The centre the camera turns about, across (picture pixels).
pub const PIVOT_X: &str = "camera.pivot.x";
/// The centre the camera turns about, down (picture pixels).
pub const PIVOT_Y: &str = "camera.pivot.y";
/// The opacity of the camera folder's layers, 0 to 1.
pub const OPACITY: &str = "camera.opacity";
/// Every value a camera key can hold, in the order Clip Studio Paint's
/// Tool Property lists them.
pub const VALUES: [&str; 7] = [X, Y, SCALE, ANGLE, PIVOT_X, PIVOT_Y, OPACITY];

/// Where a camera stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The middle of its view (picture pixels).
    pub center: [f32; 2],
    /// How much it zooms in.
    pub scale: f32,
    /// How far it turns (radians).
    pub angle: f32,
}

/// The value `key` of `entity`'s camera at `time`: its keyframes there,
/// else its own property.
fn sampled(entity: &Entity, time: Option<Time>, key: &str) -> Option<f64> {
    time.and_then(|time| entity.ext::<Animation>()?.channel(key)?.sample(time))
        .or_else(|| entity.property(key).and_then(Value::as_f64))
}

/// The seven values of `folder`'s camera at `time`, in the order of
/// [`VALUES`], as its keys there say, else as it is set, else the camera
/// that shows a `size` canvas as it is. The centre of rotation is the
/// middle of the view when nothing sets it, and the opacity 1.
#[must_use]
pub fn values(arena: &Arena, folder: EntityId, time: Option<Time>, size: [u32; 2]) -> [f64; 7] {
    let middle = [f64::from(size[0]) / 2.0, f64::from(size[1]) / 2.0];
    let Ok(entity) = arena.get_by_id(folder) else {
        return [middle[0], middle[1], 1.0, 0.0, middle[0], middle[1], 1.0];
    };
    let value = |key: &str, or: f64| sampled(entity, time, key).unwrap_or(or);
    let (x, y) = (value(X, middle[0]), value(Y, middle[1]));
    [
        x,
        y,
        value(SCALE, 1.0),
        value(ANGLE, 0.0),
        value(PIVOT_X, x),
        value(PIVOT_Y, y),
        value(OPACITY, 1.0).clamp(0.0, 1.0),
    ]
}

/// `true` if `folder`'s camera has a key of `channel` at `time`.
#[must_use]
pub fn keyed(arena: &Arena, folder: EntityId, channel: &str, time: Time) -> bool {
    arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .and_then(|animation| animation.channel(channel))
        .is_some_and(|curve| curve.keys().iter().any(|key| key.time == time))
}

/// `true` if `folder` is a camera folder.
#[must_use]
pub fn is_camera(arena: &Arena, folder: EntityId) -> bool {
    doc::is_folder(arena, folder)
        && arena
            .get_by_id(folder)
            .ok()
            .and_then(|entity| entity.property(CAMERA))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

impl Camera {
    /// Where the camera of `folder` stands at `time` (its keyframes there,
    /// else its own values), on a canvas of `size`.
    #[must_use]
    pub fn of(arena: &Arena, folder: EntityId, time: Option<Time>, size: [u32; 2]) -> Self {
        Self::from_values(&values(arena, folder, time, size))
    }

    /// The camera that the seven values of [`VALUES`] (in that order)
    /// make: the middle of its view swung round the centre of rotation by
    /// its angle.
    #[must_use]
    pub fn from_values(values: &[f64; 7]) -> Self {
        let [x, y, scale, angle, px, py, _] = values.map(|value| value as f32);
        let mut camera = Self {
            center: [x, y],
            scale: scale.max(0.01),
            angle,
        };
        // Turned about a centre of its own: the middle of the view swings
        // round it.
        let (sin, cos) = angle.sin_cos();
        let (dx, dy) = (x - px, y - py);
        camera.center = [px + cos * dx - sin * dy, py + sin * dx + cos * dy];
        camera
    }

    /// The middle of the view before the swing round `pivot`: what
    /// [`X`] and [`Y`] hold for this camera to stand where it does.
    #[must_use]
    pub fn unswung(&self, pivot: [f32; 2]) -> [f32; 2] {
        let (sin, cos) = (-self.angle).sin_cos();
        let (dx, dy) = (self.center[0] - pivot[0], self.center[1] - pivot[1]);
        [
            pivot[0] + cos * dx - sin * dy,
            pivot[1] + sin * dx + cos * dy,
        ]
    }

    /// [`Self::unswung`] when the centre of rotation is set on one axis
    /// alone (`None` on the other), the missing axis following the middle
    /// of the view before its swing, as [`values`] reads it; with neither
    /// set the view turns about its own middle and nothing swings.
    #[must_use]
    pub fn unswung_about(&self, pivot: [Option<f32>; 2]) -> [f32; 2] {
        let [cx, cy] = self.center;
        let (sin, cos) = self.angle.sin_cos();
        // Turned a quarter, the one known axis fixes nothing on its own:
        // the middle of the view is taken unswung there.
        let steep = cos.abs() < 1e-4;
        match pivot {
            [Some(px), Some(py)] => self.unswung([px, py]),
            [None, None] => self.center,
            // The centre at (px, y): the view's middle swings from
            // (px + d, y) to (px + cos d, y + sin d).
            [Some(px), None] => {
                let d = if steep { 0.0 } else { (cx - px) / cos };
                [px + d, cy - sin * d]
            }
            // The centre at (x, py): from (x, py + e) to (x - sin e,
            // py + cos e).
            [None, Some(py)] => {
                let e = if steep { 0.0 } else { (cy - py) / cos };
                [cx + sin * e, py + e]
            }
        }
    }

    /// The opacity the camera of `folder` gives its layers at `time`, when
    /// a key sets one.
    #[must_use]
    pub fn opacity(arena: &Arena, folder: EntityId, time: Time) -> Option<f32> {
        let entity = arena.get_by_id(folder).ok()?;
        entity
            .ext::<Animation>()?
            .channel(OPACITY)?
            .sample(time)
            .map(|value| value.clamp(0.0, 1.0) as f32)
    }

    /// The camera showing the canvas as it is.
    #[must_use]
    pub fn still(size: [u32; 2]) -> Self {
        Self {
            center: [size[0] as f32 / 2.0, size[1] as f32 / 2.0],
            scale: 1.0,
            angle: 0.0,
        }
    }

    /// Where on the folder's layers each canvas point is seen, as an affine
    /// map `[a, b, c, d, tx, ty]` (a canvas point `(x, y)` sees
    /// `(a x + b y + tx, c x + d y + ty)`).
    #[must_use]
    pub fn map(&self, size: [u32; 2]) -> [f32; 6] {
        let (sin, cos) = self.angle.sin_cos();
        let s = self.scale.max(0.01);
        let (a, b, c, d) = (cos / s, -sin / s, sin / s, cos / s);
        let middle = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
        [
            a,
            b,
            c,
            d,
            self.center[0] - (a * middle[0] + b * middle[1]),
            self.center[1] - (c * middle[0] + d * middle[1]),
        ]
    }

    /// The corners of what the camera sees, on the folder's layers.
    #[must_use]
    pub fn frame(&self, size: [u32; 2]) -> [[f32; 2]; 4] {
        let m = self.map(size);
        let (w, h) = (size[0] as f32, size[1] as f32);
        [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
            .map(|[x, y]| [m[0] * x + m[1] * y + m[4], m[2] * x + m[3] * y + m[5]])
    }
}

/// `true` if `p` is inside the convex `quad`.
#[must_use]
pub fn inside(quad: &[[f32; 2]; 4], p: [f32; 2]) -> bool {
    let side =
        |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let sides: Vec<f32> = (0..4).map(|i| side(quad[i], quad[(i + 1) % 4])).collect();
    sides.iter().all(|value| *value >= 0.0) || sides.iter().all(|value| *value <= 0.0)
}

/// The time of frame `frame` at `rate`.
#[must_use]
pub fn time_of(frame: u32, rate: sue_core::Rate) -> Time {
    Time::from_frames(i64::from(frame), rate)
}

/// Every camera folder in `picture` and the map it sees through at `time`.
#[must_use]
pub fn views(
    arena: &Arena,
    picture: EntityId,
    time: Option<Time>,
    size: [u32; 2],
) -> Vec<(EntityId, [f32; 6])> {
    doc::layers(arena, picture)
        .into_iter()
        .filter(|info| is_camera(arena, info.id))
        .map(|info| (info.id, Camera::of(arena, info.id, time, size).map(size)))
        .collect()
}

/// The opacities the camera folders of `picture` key at `time`.
#[must_use]
pub fn opacities(arena: &Arena, picture: EntityId, time: Time) -> Vec<(EntityId, f32)> {
    doc::layers(arena, picture)
        .into_iter()
        .filter(|info| is_camera(arena, info.id))
        .filter_map(|info| Some((info.id, Camera::opacity(arena, info.id, time)?)))
        .collect()
}

/// The edit that makes an empty camera folder named `Camera` and a number
/// where a new layer goes with `layer` picked (as Clip Studio Paint's 2D
/// camera folder), and its id. A cel picked stays in its animation
/// folder: the camera folder goes beside that folder. On a timeline the
/// camera gets its track.
#[must_use]
pub fn new_camera(arena: &Arena, picture: EntityId, layer: Option<EntityId>) -> (Edit, EntityId) {
    let count = doc::layers(arena, picture)
        .iter()
        .filter(|info| is_camera(arena, info.id))
        .count();
    let name = tr!("camera", value = count + 1);
    let folder = Entity::empty(name.clone(), sue_model::class::FOLDER)
        .with_property(doc::OPEN, true)
        .with_property(CAMERA, true);
    let id = folder.id();
    // Right above the picked layer in its folder; a cel's animation
    // folder stands in for the cel, and a folder picked is not entered.
    let beside = crate::anim::outside_animation(arena, layer);
    let order = doc::key_above(arena, picture, beside);
    let mut spawn =
        SpawnEntity::new(folder).with_relation(Relation::new(picture, CHILD, id).ordered(order));
    if let Some(outer) = beside.and_then(|beside| doc::folder_of(arena, beside)) {
        spawn = spawn.with_relation(Relation::new(outer, doc::MEMBER, id));
    }
    let mut edit = Edit::new(tr!("new-camera-folder")).then(spawn);
    if let Some(sequence) = crate::anim::timeline(arena, picture) {
        let mut out: Sequence = sequence.clone();
        let length = crate::anim::length(arena, picture);
        out.tracks.push(crate::anim::setup::camera_track(
            &name, id, out.rate, length,
        ));
        edit = edit.then(SetExtension::set(picture, out));
    }
    (edit, id)
}

/// The edit that keys one value (`channel`, one of [`VALUES`]) of
/// `folder`'s camera at `time` with `ease` (a key button per value).
#[must_use]
pub fn key_value(
    arena: &Arena,
    folder: EntityId,
    channel: &str,
    time: Time,
    value: f64,
    ease: Ease,
) -> Edit {
    let mut animation = arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .cloned()
        .unwrap_or_default();
    animation.channel_mut(channel).insert(Key {
        time,
        value,
        interpolation: ease.interpolation(),
    });
    let curve = animation.channel(channel).map(smoothed).unwrap_or_default();
    *animation.channel_mut(channel) = curve;
    Edit::new(tr!("camera-keyframe")).then(SetExtension::set(folder, animation))
}

/// The edit that switches `folder`'s camera keys at `time` to `ease`;
/// `None` when none change.
#[must_use]
pub fn set_ease(arena: &Arena, folder: EntityId, time: Time, ease: Ease) -> Option<Edit> {
    let mut animation = arena.get_by_id(folder).ok()?.ext::<Animation>()?.clone();
    let mut any = false;
    for channel in VALUES {
        let Some(curve) = animation.channel(channel) else {
            continue;
        };
        let mut curve = curve.clone();
        if let Some(mut key) = curve.keys().iter().find(|key| key.time == time).copied()
            && Ease::of(key.interpolation) != ease
        {
            key.interpolation = ease.interpolation();
            curve.insert(key);
            *animation.channel_mut(channel) = smoothed(&curve);
            any = true;
        }
    }
    any.then(|| {
        Edit::new(tr!("switch-keyframe-interpolation")).then(SetExtension::set(folder, animation))
    })
}

/// The times `folder`'s camera has keyframes at.
#[must_use]
pub fn keys(arena: &Arena, folder: EntityId) -> Vec<Time> {
    let mut out: Vec<Time> = arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .map(|animation| {
            animation
                .channels()
                .flat_map(|(_, curve)| curve.keys().iter().map(|key| key.time))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// What [`X`] and [`Y`] of `folder`'s camera hold at `time` for it to
/// stand as `camera` does: the middle of its view before the swing round
/// the centre of rotation its keys (or own values) set there. A centre
/// set on one axis alone follows the middle of the view on the other,
/// as [`values`] reads it, and is solved for so.
#[must_use]
pub fn unswung_at(arena: &Arena, folder: EntityId, time: Time, camera: &Camera) -> [f32; 2] {
    let pivot = arena.get_by_id(folder).map_or([None, None], |entity| {
        [PIVOT_X, PIVOT_Y].map(|key| sampled(entity, Some(time), key).map(|value| value as f32))
    });
    camera.unswung_about(pivot)
}

/// The edit that keys `folder`'s camera at `time` standing as `camera`.
#[must_use]
pub fn key(arena: &Arena, folder: EntityId, time: Time, camera: Camera) -> Edit {
    let mut animation = arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .cloned()
        .unwrap_or_default();
    // The middle of the view is kept before its swing round a centre of
    // rotation, so the camera stands where it was shown.
    let center = unswung_at(arena, folder, time, &camera);
    for (channel, value) in [
        (X, center[0]),
        (Y, center[1]),
        (SCALE, camera.scale),
        (ANGLE, camera.angle),
    ] {
        animation.channel_mut(channel).insert(Key {
            time,
            value: f64::from(value),
            interpolation: Interpolation::Linear,
        });
    }
    Edit::new(tr!("camera-keyframe")).then(SetExtension::set(folder, animation))
}

/// The edit that takes the key of `channel` (one of [`VALUES`]) of
/// `folder`'s camera at `time` away, if it has one: the other values
/// keep theirs.
#[must_use]
pub fn delete_value_key(
    arena: &Arena,
    folder: EntityId,
    channel: &str,
    time: Time,
) -> Option<Edit> {
    let mut animation = arena.get_by_id(folder).ok()?.ext::<Animation>()?.clone();
    animation.channel(channel)?;
    animation.channel_mut(channel).remove(time)?;
    let curve = animation.channel(channel).map(smoothed).unwrap_or_default();
    *animation.channel_mut(channel) = curve;
    Some(Edit::new(tr!("delete-camera-keyframe")).then(SetExtension::set(folder, animation)))
}

/// The edit that takes `folder`'s camera keyframe at `time` away, if it has
/// one.
#[must_use]
pub fn delete_key(arena: &Arena, folder: EntityId, time: Time) -> Option<Edit> {
    let mut animation = arena.get_by_id(folder).ok()?.ext::<Animation>()?.clone();
    let mut any = false;
    for channel in VALUES {
        if animation.channel(channel).is_some() {
            any |= animation.channel_mut(channel).remove(time).is_some();
        }
    }
    any.then(|| Edit::new(tr!("delete-camera-keyframe")).then(SetExtension::set(folder, animation)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{NewPicture, create};

    fn see(map: [f32; 6], [x, y]: [f32; 2]) -> [f32; 2] {
        [
            map[0] * x + map[1] * y + map[4],
            map[2] * x + map[3] * y + map[5],
        ]
    }

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-2
    }

    #[test]
    fn a_camera_moves_zooms_and_turns_between_keyframes() {
        let size = [200, 100];
        let still = Camera::still(size).map(size);
        assert!(
            near(see(still, [30.0, 40.0]), [30.0, 40.0]),
            "it sees the canvas as it is"
        );
        // Zoomed in twice on a point: the canvas middle sees that point,
        // and its corner sees half way out.
        let zoomed = Camera {
            center: [50.0, 50.0],
            scale: 2.0,
            angle: 0.0,
        };
        assert!(near(see(zoomed.map(size), [100.0, 50.0]), [50.0, 50.0]));
        assert!(near(zoomed.frame(size)[0], [0.0, 25.0]));
        // A quarter turn.
        let turned = Camera {
            angle: std::f32::consts::FRAC_PI_2,
            ..Camera::still(size)
        };
        assert!(near(see(turned.map(size), [110.0, 50.0]), [100.0, 60.0]));

        // A camera folder keyed at frames 0 and 10 is half way at 5.
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 200,
            height: 100,
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let layer = doc::layers(&arena, picture).last().map(|info| info.id);
        let (mut edit, folder) = new_camera(&arena, picture, layer);
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        assert!(is_camera(&arena, folder));
        assert_eq!(
            doc::folder_of(&arena, layer.unwrap()),
            None,
            "it is made empty"
        );
        assert!(doc::members(&arena, picture, folder).is_empty());
        let mut into = doc::set_folder(&arena, layer.unwrap(), Some(folder)).unwrap();
        sue_core::ReversibleTransaction::apply(&mut into, &mut arena).unwrap();
        let rate = sue_core::Rate::new(10, 1);
        let mut first = key(&arena, folder, time_of(0, rate), Camera::still(size));
        sue_core::ReversibleTransaction::apply(&mut first, &mut arena).unwrap();
        let later = Camera {
            center: [150.0, 50.0],
            scale: 3.0,
            angle: 0.0,
        };
        let mut second = key(&arena, folder, time_of(10, rate), later);
        sue_core::ReversibleTransaction::apply(&mut second, &mut arena).unwrap();
        assert_eq!(keys(&arena, folder).len(), 2);
        let half = Camera::of(&arena, folder, Some(time_of(5, rate)), size);
        assert!(
            near(half.center, [125.0, 50.0]) && (half.scale - 2.0).abs() < 1e-3,
            "{half:?}"
        );
        let views = views(&arena, picture, Some(time_of(10, rate)), size);
        assert_eq!(views.len(), 1);
        assert!(near(see(views[0].1, [100.0, 50.0]), [150.0, 50.0]));
        // Deleting a key, and undoing that.
        let mut gone = delete_key(&arena, folder, time_of(10, rate)).unwrap();
        sue_core::ReversibleTransaction::apply(&mut gone, &mut arena).unwrap();
        assert_eq!(keys(&arena, folder).len(), 1);
        sue_core::ReversibleTransaction::revert(&mut gone, &mut arena).unwrap();
        assert_eq!(keys(&arena, folder).len(), 2);
    }

    #[test]
    fn the_compositor_shows_a_camera_folder_through_its_camera() {
        let size = [200, 100];
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 200,
            height: 100,
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        // A dark dot at (50, 50) on the layer in the camera folder.
        let layer = doc::layers(&arena, picture).last().unwrap().id;
        let raster = doc::raster_mut(&mut arena, layer).unwrap();
        let width = raster.width();
        let bytes = raster.bytes_mut().unwrap();
        for y in 48..52 {
            for x in 48..52 {
                bytes[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let (mut edit, folder) = new_camera(&arena, picture, Some(layer));
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let mut into = doc::set_folder(&arena, layer, Some(folder)).unwrap();
        sue_core::ReversibleTransaction::apply(&mut into, &mut arena).unwrap();
        let shown = |camera: Camera| {
            let view = crate::composite::FrameView {
                cameras: vec![(folder, camera.map(size))],
                ..crate::composite::FrameView::default()
            };
            let options = crate::composite::RenderOptions {
                paper: true,
                drafts: false,
            };
            crate::composite::flatten_view(&arena, picture, options, Some(&view)).unwrap()
        };
        let gray = |tile: &sue_core::RasterTile2D, x: u32, y: u32| {
            tile.bytes().unwrap()[((y * 200 + x) * 4) as usize]
        };
        let still = shown(Camera::still(size));
        assert!(gray(&still, 50, 50) < 50 && gray(&still, 100, 50) > 200);
        // Looking at the dot, zoomed four times: it fills the middle.
        let close = shown(Camera {
            center: [50.0, 50.0],
            scale: 4.0,
            angle: 0.0,
        });
        assert!(
            gray(&close, 100, 50) < 50 && gray(&close, 106, 50) < 80,
            "the dot, four times bigger"
        );
        assert!(gray(&close, 50, 50) > 200, "where it was is paper now");
    }

    #[test]
    fn a_new_camera_is_an_empty_track_and_a_cel_stays_in_its_folder() {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture::default()).unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let mut timeline = crate::anim::new_timeline(&arena, picture, 12, 12);
        sue_core::ReversibleTransaction::apply(&mut timeline, &mut arena).unwrap();
        let sequence = crate::anim::timeline(&arena, picture).unwrap();
        let animation = crate::anim::track_folder(&sequence.tracks[0]).unwrap();
        let (mut made, cel) = crate::anim::new_cel(&arena, picture, animation, 0).unwrap();
        sue_core::ReversibleTransaction::apply(&mut made, &mut arena).unwrap();
        let before = format!("{:?}", crate::anim::timeline(&arena, picture));
        // The cel is current: the camera goes beside its folder (H-6).
        let (mut edit, camera) = new_camera(&arena, picture, Some(cel));
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        assert_eq!(crate::anim::cel_folder(&arena, cel), Some(animation));
        assert_eq!(doc::folder_of(&arena, camera), None);
        assert!(
            doc::members(&arena, picture, camera).is_empty(),
            "empty when made"
        );
        assert_eq!(arena.get_by_id(camera).unwrap().name(), "Camera 1");
        let sequence = crate::anim::timeline(&arena, picture).unwrap();
        let track = crate::anim::setup::track_of_layer(sequence, camera).expect("a camera track");
        assert_eq!(sequence.tracks[track].kind, crate::anim::setup::CAMERA);
        let stack: Vec<EntityId> = doc::layers(&arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect();
        let at = |id: EntityId| stack.iter().position(|known| *known == id).unwrap();
        assert_eq!(
            at(camera),
            at(animation) + 1,
            "right above the animation folder"
        );
        // Deleting the folder takes its track.
        let mut gone = crate::anim::delete_folder(&arena, picture, camera);
        sue_core::ReversibleTransaction::apply(&mut gone, &mut arena).unwrap();
        let sequence = crate::anim::timeline(&arena, picture).unwrap();
        assert!(crate::anim::setup::track_of_layer(sequence, camera).is_none());
        sue_core::ReversibleTransaction::revert(&mut gone, &mut arena).unwrap();
        sue_core::ReversibleTransaction::revert(&mut edit, &mut arena).unwrap();
        assert!(arena.get_by_id(camera).is_err());
        assert_eq!(
            format!("{:?}", crate::anim::timeline(&arena, picture)),
            before
        );
    }

    #[test]
    fn camera_values_key_one_by_one_turn_about_a_centre_and_fade() {
        let size = [100, 100];
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 100,
            height: 100,
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let (mut edit, folder) = new_camera(&arena, picture, None);
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let rate = sue_core::Rate::new(10, 1);
        let at = |frame: u32| time_of(frame, rate);
        // Only the angle and the opacity are keyed.
        let quarter = f64::from(std::f32::consts::FRAC_PI_2);
        for (channel, frame, value, ease) in [
            (ANGLE, 0, 0.0, Ease::Linear),
            (ANGLE, 10, quarter, Ease::Linear),
            (OPACITY, 0, 1.0, Ease::Hold),
            (OPACITY, 10, 0.0, Ease::Hold),
        ] {
            let mut edit = key_value(&arena, folder, channel, at(frame), value, ease);
            sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        }
        let animation = arena.get_by_id(folder).unwrap().ext::<Animation>().unwrap();
        assert!(animation.channel(X).is_none(), "the position was not keyed");
        assert_eq!(Camera::opacity(&arena, folder, at(5)), Some(1.0), "held");
        assert_eq!(Camera::opacity(&arena, folder, at(10)), Some(0.0));
        assert_eq!(opacities(&arena, picture, at(10)), vec![(folder, 0.0)]);
        // Turned about the canvas corner a quarter: the view's middle
        // swings from (50, 50) to (50, -50)... about (0, 0) it goes to
        // (-50, 50).
        let mut pivot = Edit::new("Pivot")
            .then(sue_core::txn::SetProperty::new(folder, PIVOT_X, 0.0))
            .then(sue_core::txn::SetProperty::new(folder, PIVOT_Y, 0.0));
        sue_core::ReversibleTransaction::apply(&mut pivot, &mut arena).unwrap();
        let turned = Camera::of(&arena, folder, Some(at(10)), size);
        assert!(near(turned.center, [-50.0, 50.0]), "{turned:?}");
        sue_core::ReversibleTransaction::revert(&mut pivot, &mut arena).unwrap();
        let middle = Camera::of(&arena, folder, Some(at(10)), size);
        assert!(
            near(middle.center, [50.0, 50.0]),
            "its own middle: {middle:?}"
        );
        // Switching the angle keys to Smooth, and back by undo.
        let mut smooth = set_ease(&arena, folder, at(0), Ease::Smooth).unwrap();
        sue_core::ReversibleTransaction::apply(&mut smooth, &mut arena).unwrap();
        let early = Camera::of(&arena, folder, Some(at(2)), size).angle;
        assert!(
            early < 0.2 * std::f32::consts::FRAC_PI_2,
            "eases out: {early}"
        );
        sue_core::ReversibleTransaction::revert(&mut smooth, &mut arena).unwrap();
        let linear = Camera::of(&arena, folder, Some(at(2)), size).angle;
        assert!((linear - 0.2 * std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn a_camera_keyed_about_a_centre_stands_where_it_was_shown() {
        let size = [100, 100];
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 100,
            height: 100,
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let (mut edit, folder) = new_camera(&arena, picture, None);
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let rate = sue_core::Rate::new(10, 1);
        let at = time_of(4, rate);
        // Nothing set: the canvas as it is, turning about its middle.
        assert_eq!(
            values(&arena, folder, Some(at), size),
            [50.0, 50.0, 1.0, 0.0, 50.0, 50.0, 1.0]
        );
        for (channel, value) in [(PIVOT_X, 10.0), (PIVOT_Y, 20.0)] {
            let mut edit = key_value(&arena, folder, channel, at, value, Ease::Linear);
            sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        }
        assert!(keyed(&arena, folder, PIVOT_X, at));
        assert!(!keyed(&arena, folder, X, at));
        // Keyed turned and moved: shown again exactly so.
        let shown = Camera {
            center: [70.0, 35.0],
            scale: 1.5,
            angle: 0.6,
        };
        let mut edit = key(&arena, folder, at, shown);
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let again = Camera::of(&arena, folder, Some(at), size);
        assert!(near(again.center, shown.center), "{again:?}");
        assert!((again.angle - 0.6).abs() < 1e-5 && (again.scale - 1.5).abs() < 1e-5);
        // One value's key goes, the others stay; undo brings it back with
        // the arena as it was.
        let before = format!("{:?}", arena.get_by_id(folder).unwrap().ext::<Animation>());
        let mut gone = delete_value_key(&arena, folder, PIVOT_Y, at).unwrap();
        sue_core::ReversibleTransaction::apply(&mut gone, &mut arena).unwrap();
        assert!(!keyed(&arena, folder, PIVOT_Y, at));
        assert!(keyed(&arena, folder, PIVOT_X, at) && keyed(&arena, folder, ANGLE, at));
        assert!(delete_value_key(&arena, folder, PIVOT_Y, at).is_none());
        let after = format!("{:?}", arena.get_by_id(folder).unwrap().ext::<Animation>());
        sue_core::ReversibleTransaction::revert(&mut gone, &mut arena).unwrap();
        assert_eq!(
            format!("{:?}", arena.get_by_id(folder).unwrap().ext::<Animation>()),
            before
        );
        sue_core::ReversibleTransaction::apply(&mut gone, &mut arena).unwrap();
        assert_eq!(
            format!("{:?}", arena.get_by_id(folder).unwrap().ext::<Animation>()),
            after
        );
    }

    #[test]
    fn a_camera_keyed_with_its_centre_set_on_one_axis_stands_where_it_was_shown() {
        let size = [100, 100];
        let rate = sue_core::Rate::new(10, 1);
        let at = time_of(2, rate);
        for (channel, value) in [(PIVOT_X, 10.0), (PIVOT_Y, 80.0)] {
            for angle in [0.6_f32, -2.2, std::f32::consts::FRAC_PI_2] {
                let mut arena = Arena::new();
                let (mut edit, picture) = create(&NewPicture {
                    width: 100,
                    height: 100,
                    ..NewPicture::default()
                })
                .unwrap();
                sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
                let (mut edit, folder) = new_camera(&arena, picture, None);
                sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
                // One axis of the centre keyed alone, as its key button
                // in Tool Property does.
                let mut edit = key_value(&arena, folder, channel, at, value, Ease::Linear);
                sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
                let shown = Camera {
                    center: [64.0, 27.0],
                    scale: 1.25,
                    angle,
                };
                let mut edit = key(&arena, folder, at, shown);
                sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
                let again = Camera::of(&arena, folder, Some(at), size);
                // A quarter turn pins the view's middle to the centre on
                // that axis: only the other axis can be kept.
                let steep = (angle.cos()).abs() < 1e-3;
                let kept = if steep {
                    let axis = usize::from(channel == PIVOT_Y);
                    (again.center[1 - axis] - shown.center[1 - axis]).abs() < 1e-2
                } else {
                    near(again.center, shown.center)
                };
                assert!(kept, "{channel} {angle}: {again:?} against {shown:?}");
                assert!((again.angle - angle).abs() < 1e-5);
            }
        }
    }
}
