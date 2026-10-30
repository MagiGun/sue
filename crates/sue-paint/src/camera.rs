//! The camera folder, as in Clip Studio Paint's animation: a layer folder
//! whose layers are seen through a camera that moves, zooms and turns along
//! the timeline. A camera folder is an ordinary folder flagged [`CAMERA`];
//! where the camera stands is its [`X`], [`Y`] (the middle of its view, in
//! picture pixels), [`SCALE`] and [`ANGLE`] (radians) properties, and its
//! keyframes are the shared [`Animation`] on the folder, a curve each.
//!
//! At a frame the compositor shows the folder's layers as the camera sees
//! them: what is in the camera's view fills the canvas.

use sue_core::txn::{Rename, SetExtension, SetProperty};
use sue_core::{Arena, EntityId, Time, Value};
use sue_model::{Animation, Interpolation, Key};

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
        let Ok(entity) = arena.get_by_id(folder) else {
            return Self::still(size);
        };
        let keyed = time
            .and_then(|time| {
                entity
                    .ext::<Animation>()
                    .map(|animation| animation.sample(time))
            })
            .unwrap_or_default();
        let value = |key: &str, or: f32| {
            keyed
                .get(key)
                .or_else(|| entity.property(key))
                .and_then(Value::as_f64)
                .map_or(or, |value| value as f32)
        };
        let still = Self::still(size);
        Self {
            center: [value(X, still.center[0]), value(Y, still.center[1])],
            scale: value(SCALE, 1.0).max(0.01),
            angle: value(ANGLE, 0.0),
        }
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

/// The edit that makes a camera folder right above `layer` (with `layer`
/// in it), named `Camera` and a number, and its id.
#[must_use]
pub fn new_camera(arena: &Arena, picture: EntityId, layer: Option<EntityId>) -> (Edit, EntityId) {
    let count = doc::layers(arena, picture)
        .iter()
        .filter(|info| is_camera(arena, info.id))
        .count();
    let (edit, id) = doc::new_folder(arena, picture, layer, layer.is_some());
    let mut edit = Edit::new(tr!("new-camera-folder")).then(edit);
    edit = edit
        .then(SetProperty::new(id, CAMERA, true))
        .then(Rename::new(id, tr!("camera", value = count + 1)));
    (edit, id)
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

/// The edit that keys `folder`'s camera at `time` standing as `camera`.
#[must_use]
pub fn key(arena: &Arena, folder: EntityId, time: Time, camera: Camera) -> Edit {
    let mut animation = arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.ext::<Animation>())
        .cloned()
        .unwrap_or_default();
    for (channel, value) in [
        (X, camera.center[0]),
        (Y, camera.center[1]),
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

/// The edit that takes `folder`'s camera keyframe at `time` away, if it has
/// one.
#[must_use]
pub fn delete_key(arena: &Arena, folder: EntityId, time: Time) -> Option<Edit> {
    let mut animation = arena.get_by_id(folder).ok()?.ext::<Animation>()?.clone();
    let mut any = false;
    for channel in [X, Y, SCALE, ANGLE] {
        any |= animation.channel_mut(channel).remove(time).is_some();
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
            Some(folder),
            "the layer goes in"
        );
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
}
