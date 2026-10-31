//! 3D drawing figures, as in Clip Studio Paint: a body of seventeen joints
//! that people pose to draw from, on a 3D layer. Each body part is a
//! rounded solid along its bone; posing turns a joint, and everything
//! hanging from it follows.
//!
//! A figure is a model entity of class [`CLASS`] whose pose is its
//! [`POSE`] property: the turn of every joint in [`JOINTS`] order, in
//! degrees about the joint's own x (bend forward and back), y (twist) and
//! z (swing to the side) axes. The 3D layer that shows it draws the posed
//! body with Sue Paint's renderer, so line art, shading and Convert to
//! Lines and Tones work on figures as on any model.
//!
//! Its body shape is two more properties, as Clip Studio Paint's: its
//! [`HEIGHT`] and its [`HEADS`] (head to body ratio, how many heads tall
//! it stands).

use sue_core::txn::{ReplaceComponent, SetProperty};
use sue_core::{Arena, Entity, EntityId, MeshBuffer3D, Value};
use sue_model::{Geometry, PrimKind};

use crate::doc::Edit;

/// The class of a drawing figure's model entity.
pub const CLASS: &str = "sue.paint.figure";
/// The pose, as text (see [`Pose::to_text`]).
pub const POSE: &str = "figure.pose";
/// How tall the figure is, in centimeters (`Float`).
pub const HEIGHT: &str = "figure.height";
/// Its head to body ratio: how many heads tall it is (`Float`).
pub const HEADS: &str = "figure.heads";

/// One joint: its name, the joint it hangs from, where it sits on that
/// joint's bone at rest, its own bone, and the body part's half widths
/// across (x) and through (z) at each end of the bone.
#[derive(Clone, Copy, Debug)]
pub struct Joint {
    /// What people read.
    pub name: &'static str,
    /// The joint it hangs from (none for the hips).
    pub parent: Option<usize>,
    /// Where it sits in its parent's frame at rest.
    pub offset: [f32; 3],
    /// Its bone, in its own frame at rest.
    pub bone: [f32; 3],
    /// Half widths at the bone's start, across and through.
    pub start: [f32; 2],
    /// Half widths at the bone's end.
    pub end: [f32; 2],
    /// The joint on the other side (left for right and back), if any.
    pub mirror: Option<usize>,
}

const fn joint(
    name: &'static str,
    parent: Option<usize>,
    offset: [f32; 3],
    bone: [f32; 3],
    start: [f32; 2],
    end: [f32; 2],
    mirror: Option<usize>,
) -> Joint {
    Joint {
        name,
        parent,
        offset,
        bone,
        start,
        end,
        mirror,
    }
}

/// The figure's joints, parents first. The figure stands about 1.9 tall
/// on the ground, facing +z; its left is +x.
pub const JOINTS: [Joint; 17] = [
    joint(
        "Hips",
        None,
        [0.0, 0.95, 0.0],
        [0.0, 0.12, 0.0],
        [0.15, 0.1],
        [0.14, 0.09],
        None,
    ),
    joint(
        "Spine",
        Some(0),
        [0.0, 0.12, 0.0],
        [0.0, 0.2, 0.0],
        [0.13, 0.085],
        [0.15, 0.095],
        None,
    ),
    joint(
        "Chest",
        Some(1),
        [0.0, 0.2, 0.0],
        [0.0, 0.22, 0.0],
        [0.17, 0.105],
        [0.14, 0.09],
        None,
    ),
    joint(
        "Neck",
        Some(2),
        [0.0, 0.22, 0.0],
        [0.0, 0.08, 0.0],
        [0.05, 0.05],
        [0.045, 0.045],
        None,
    ),
    joint(
        "Head",
        Some(3),
        [0.0, 0.08, 0.0],
        [0.0, 0.22, 0.02],
        [0.095, 0.105],
        [0.09, 0.1],
        None,
    ),
    joint(
        "Left upper arm",
        Some(2),
        [0.2, 0.19, 0.0],
        [0.0, -0.29, 0.0],
        [0.05, 0.05],
        [0.042, 0.042],
        Some(8),
    ),
    joint(
        "Left forearm",
        Some(5),
        [0.0, -0.29, 0.0],
        [0.0, -0.26, 0.0],
        [0.04, 0.04],
        [0.032, 0.03],
        Some(9),
    ),
    joint(
        "Left hand",
        Some(6),
        [0.0, -0.26, 0.0],
        [0.0, -0.11, 0.0],
        [0.04, 0.018],
        [0.035, 0.015],
        Some(10),
    ),
    joint(
        "Right upper arm",
        Some(2),
        [-0.2, 0.19, 0.0],
        [0.0, -0.29, 0.0],
        [0.05, 0.05],
        [0.042, 0.042],
        Some(5),
    ),
    joint(
        "Right forearm",
        Some(8),
        [0.0, -0.29, 0.0],
        [0.0, -0.26, 0.0],
        [0.04, 0.04],
        [0.032, 0.03],
        Some(6),
    ),
    joint(
        "Right hand",
        Some(9),
        [0.0, -0.26, 0.0],
        [0.0, -0.11, 0.0],
        [0.04, 0.018],
        [0.035, 0.015],
        Some(7),
    ),
    joint(
        "Left thigh",
        Some(0),
        [0.1, 0.0, 0.0],
        [0.0, -0.44, 0.0],
        [0.08, 0.08],
        [0.058, 0.058],
        Some(14),
    ),
    joint(
        "Left shin",
        Some(11),
        [0.0, -0.44, 0.0],
        [0.0, -0.42, 0.0],
        [0.055, 0.055],
        [0.04, 0.04],
        Some(15),
    ),
    joint(
        "Left foot",
        Some(12),
        [0.0, -0.42, 0.0],
        [0.0, -0.04, 0.17],
        [0.04, 0.03],
        [0.035, 0.02],
        Some(16),
    ),
    joint(
        "Right thigh",
        Some(0),
        [-0.1, 0.0, 0.0],
        [0.0, -0.44, 0.0],
        [0.08, 0.08],
        [0.058, 0.058],
        Some(11),
    ),
    joint(
        "Right shin",
        Some(14),
        [0.0, -0.44, 0.0],
        [0.0, -0.42, 0.0],
        [0.055, 0.055],
        [0.04, 0.04],
        Some(12),
    ),
    joint(
        "Right foot",
        Some(15),
        [0.0, -0.42, 0.0],
        [0.0, -0.04, 0.17],
        [0.04, 0.03],
        [0.035, 0.02],
        Some(13),
    ),
];

/// The middle and size Sue Paint frames a figure by, so posing never
/// changes how large it shows.
pub const FRAME: ([f32; 3], f32) = ([0.0, 0.95, 0.0], 2.1);

/// The figure's body shape: how tall, and how many heads tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Centimeters.
    pub height: f32,
    /// Head to body ratio.
    pub heads: f32,
}

impl Body {
    /// The height [`JOINTS`] are made for, in centimeters.
    pub const HEIGHT: f32 = 170.0;
    /// The head to body ratio [`JOINTS`] are made for.
    pub const HEADS: f32 = 7.5;
    /// The heights people may choose, in centimeters.
    pub const HEIGHTS: std::ops::RangeInclusive<f32> = 50.0..=250.0;
    /// The ratios people may choose.
    pub const RATIOS: std::ops::RangeInclusive<f32> = 2.0..=10.0;

    /// The body shape of figure model `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let number = |key: &str, or: f32, range: std::ops::RangeInclusive<f32>| {
            entity
                .property(key)
                .and_then(Value::as_f64)
                .map_or(or, |value| {
                    (value as f32).clamp(*range.start(), *range.end())
                })
        };
        Self {
            height: number(HEIGHT, Self::HEIGHT, Self::HEIGHTS),
            heads: number(HEADS, Self::HEADS, Self::RATIOS),
        }
    }

    /// The joints shaped for this body: the head grown or shrunk to its
    /// ratio and the rest lengthened or shortened to keep the height, then
    /// all scaled to the height. Lengths (up and down) change; shoulders and
    /// hips keep their width, so a short ratio reads as a child or a
    /// chibi, not a thin adult.
    #[must_use]
    pub fn joints(&self) -> [Joint; 17] {
        let scale = self.height / Self::HEIGHT;
        let head = Self::HEADS / self.heads;
        let rest = (1.0 - 1.0 / self.heads) / (1.0 - 1.0 / Self::HEADS);
        let mut joints = JOINTS;
        for (at, joint) in joints.iter_mut().enumerate() {
            if at == HEAD {
                joint.bone = joint.bone.map(|value| value * head * scale);
                joint.start = joint.start.map(|value| value * head * scale);
                joint.end = joint.end.map(|value| value * head * scale);
                joint.offset = joint.offset.map(|value| value * rest * scale);
            } else {
                let long = |v: [f32; 3]| [v[0] * scale, v[1] * rest * scale, v[2] * scale];
                joint.bone = long(joint.bone);
                joint.offset = long(joint.offset);
                joint.start = joint.start.map(|value| value * scale);
                joint.end = joint.end.map(|value| value * scale);
            }
        }
        joints
    }
}

impl Default for Body {
    fn default() -> Self {
        Self {
            height: Self::HEIGHT,
            heads: Self::HEADS,
        }
    }
}

/// The head, in [`JOINTS`].
const HEAD: usize = 4;

/// How every joint is turned, in degrees about its x, y and z.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose(pub Vec<[f32; 3]>);

impl Default for Pose {
    fn default() -> Self {
        preset(0)
    }
}

impl Pose {
    /// No joint turned: arms straight down, legs together.
    #[must_use]
    pub fn rest() -> Self {
        Self(vec![[0.0; 3]; JOINTS.len()])
    }

    /// The pose as text: `v1;` then every joint's `x,y,z`, `;` between.
    #[must_use]
    pub fn to_text(&self) -> String {
        let joints: Vec<String> = self
            .0
            .iter()
            .map(|[x, y, z]| format!("{x},{y},{z}"))
            .collect();
        format!("v1;{}", joints.join(";"))
    }

    /// A pose from [`Self::to_text`]; joints it lacks stay unturned.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut pose = Self::rest();
        let Some(body) = text.strip_prefix("v1;") else {
            return pose;
        };
        for (at, joint) in body.split(';').enumerate().take(JOINTS.len()) {
            let values: Vec<f32> = joint
                .split(',')
                .filter_map(|value| value.parse().ok())
                .collect();
            if let [x, y, z] = values.as_slice() {
                pose.0[at] = [*x, *y, *z];
            }
        }
        pose
    }

    /// The pose of figure model `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        entity
            .property(POSE)
            .and_then(Value::as_str)
            .map_or_else(Self::default, Self::from_text)
    }

    /// The pose mirrored left to right (Clip Studio Paint's Flip pose):
    /// each side takes the other's turns, swung and twisted the other
    /// way.
    #[must_use]
    pub fn flipped(&self) -> Self {
        let mut out = self.clone();
        for (at, joint) in JOINTS.iter().enumerate() {
            let [x, y, z] = self.0[joint.mirror.unwrap_or(at)];
            out.0[at] = [x, -y, -z];
        }
        out
    }
}

/// What people read for the pose presets, in order.
pub const PRESETS: [&str; 7] = ["Stand", "T-pose", "Walk", "Run", "Sit", "Arms up", "Think"];

/// Preset pose `index` (see [`PRESETS`]).
#[must_use]
pub fn preset(index: usize) -> Pose {
    let mut pose = Pose::rest();
    let mut set = |joint: usize, turn: [f32; 3]| pose.0[joint] = turn;
    match index {
        1 => {
            set(5, [0.0, 0.0, 90.0]);
            set(8, [0.0, 0.0, -90.0]);
        }
        2 => {
            set(11, [-25.0, 0.0, 2.0]);
            set(12, [10.0, 0.0, 0.0]);
            set(13, [-5.0, 0.0, 0.0]);
            set(14, [18.0, 0.0, -2.0]);
            set(15, [30.0, 0.0, 0.0]);
            set(16, [-15.0, 0.0, 0.0]);
            set(5, [20.0, 0.0, 8.0]);
            set(6, [-15.0, 0.0, 0.0]);
            set(8, [-22.0, 0.0, -8.0]);
            set(9, [-25.0, 0.0, 0.0]);
        }
        3 => {
            set(0, [10.0, 0.0, 0.0]);
            set(11, [-55.0, 0.0, 0.0]);
            set(12, [70.0, 0.0, 0.0]);
            set(14, [35.0, 0.0, 0.0]);
            set(15, [75.0, 0.0, 0.0]);
            set(16, [-25.0, 0.0, 0.0]);
            set(5, [45.0, 0.0, 10.0]);
            set(6, [-80.0, 0.0, 0.0]);
            set(8, [-50.0, 0.0, -10.0]);
            set(9, [-85.0, 0.0, 0.0]);
        }
        4 => {
            set(11, [-90.0, 0.0, 4.0]);
            set(12, [90.0, 0.0, 0.0]);
            set(14, [-90.0, 0.0, -4.0]);
            set(15, [90.0, 0.0, 0.0]);
            set(5, [-30.0, 0.0, 8.0]);
            set(6, [-40.0, 0.0, 0.0]);
            set(8, [-30.0, 0.0, -8.0]);
            set(9, [-40.0, 0.0, 0.0]);
        }
        5 => {
            set(5, [0.0, 0.0, 165.0]);
            set(8, [0.0, 0.0, -165.0]);
        }
        6 => {
            set(4, [12.0, 0.0, -6.0]);
            set(8, [-45.0, 0.0, -10.0]);
            set(9, [-135.0, 0.0, 0.0]);
            set(5, [10.0, 0.0, 15.0]);
            set(6, [-90.0, 0.0, 0.0]);
        }
        _ => {
            set(5, [0.0, 0.0, 12.0]);
            set(8, [0.0, 0.0, -12.0]);
            set(6, [-8.0, 0.0, 0.0]);
            set(9, [-8.0, 0.0, 0.0]);
        }
    }
    pose
}

type Mat3 = [[f32; 3]; 3];

fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (column, value) in out_row.iter_mut().enumerate() {
            *value = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

fn apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|row| m[row][0] * v[0] + m[row][1] * v[1] + m[row][2] * v[2])
}

/// The turn by `[x, y, z]` degrees: about z, then y, then x.
fn euler([x, y, z]: [f32; 3]) -> Mat3 {
    let (sx, cx) = x.to_radians().sin_cos();
    let (sy, cy) = y.to_radians().sin_cos();
    let (sz, cz) = z.to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(&mul(&rz, &ry), &rx)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Where every joint is and how it is turned, for `pose`.
#[must_use]
pub fn solve(pose: &Pose, body: &Body) -> Vec<([f32; 3], Mat3)> {
    let mut placed: Vec<([f32; 3], Mat3)> = Vec::with_capacity(JOINTS.len());
    for (at, joint) in body.joints().iter().enumerate() {
        let turn = euler(pose.0.get(at).copied().unwrap_or([0.0; 3]));
        let (position, rotation) = match joint.parent {
            Some(parent) => {
                let (from, frame) = placed[parent];
                (add(from, apply(&frame, joint.offset)), mul(&frame, &turn))
            }
            None => (joint.offset, turn),
        };
        placed.push((position, rotation));
    }
    placed
}

/// Each joint's bone, start and end, for `pose`.
#[must_use]
pub fn bones(pose: &Pose, body: &Body) -> Vec<([f32; 3], [f32; 3])> {
    solve(pose, body)
        .iter()
        .zip(body.joints().iter())
        .map(|((start, frame), joint)| (*start, add(*start, apply(frame, joint.bone))))
        .collect()
}

/// The posed body as one shape: a rounded solid along every bone.
#[must_use]
pub fn geometry(pose: &Pose, body: &Body) -> Geometry {
    let mut geometry = Geometry::new();
    let placed = solve(pose, body);
    let around = 12;
    for ((start, frame), joint) in placed.iter().zip(body.joints().iter()) {
        let length = joint
            .bone
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if length <= 0.0 {
            continue;
        }
        let along = joint.bone.map(|value| value / length);
        // The part's own across and through, square to the bone.
        let (across, through) = if along[1].abs() > 0.7 {
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        } else {
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        };
        // Rings from a rounded start to a rounded end: (place along the
        // bone, 0 to 1, and how wide there, 0 to 1 of the half widths).
        let mut rings: Vec<(f32, f32, [f32; 2])> = Vec::new();
        let caps = 4;
        let reach = |half: [f32; 2]| half[0].max(half[1]) / length.max(1e-3);
        for step in 0..=caps {
            let angle = std::f32::consts::FRAC_PI_2 * (1.0 - step as f32 / caps as f32);
            rings.push((
                -angle.sin() * reach(joint.start) * 0.6,
                angle.cos(),
                joint.start,
            ));
        }
        for step in 0..=caps {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / caps as f32;
            rings.push((
                1.0 + angle.sin() * reach(joint.end) * 0.6,
                angle.cos(),
                joint.end,
            ));
        }
        let point = |t: f32, scale: f32, half: [f32; 2], i: usize| {
            let theta = i as f32 / around as f32 * std::f32::consts::TAU;
            let (c, s) = (theta.cos(), theta.sin());
            let local = [0, 1, 2].map(|axis| {
                joint.bone[axis] * t
                    + across[axis] * c * half[0] * scale
                    + through[axis] * s * half[1] * scale
            });
            let world = add(*start, apply(frame, local));
            world.map(f64::from)
        };
        for pair in rings.windows(2) {
            let ((t0, s0, h0), (t1, s1, h1)) = (pair[0], pair[1]);
            for i in 0..around {
                let ids: Vec<u32> = [
                    point(t0, s0, h0, i),
                    point(t0, s0, h0, i + 1),
                    point(t1, s1, h1, i + 1),
                    point(t1, s1, h1, i),
                ]
                .iter()
                .map(|position| geometry.add_point(*position))
                .collect();
                let _ = geometry.add_prim(PrimKind::Polygon, &ids);
            }
        }
    }
    geometry
}

/// The posed body as triangles, for the renderer.
#[must_use]
pub fn mesh(pose: &Pose, body: &Body) -> MeshBuffer3D {
    geometry(pose, body).to_mesh()
}

/// A new figure model, standing.
#[must_use]
pub fn new_model(name: &str) -> Entity {
    Entity::empty(name, CLASS).with_property(POSE, Value::from(Pose::default().to_text()))
}

/// `true` if `entity` is a drawing figure.
#[must_use]
pub fn is_figure(entity: &Entity) -> bool {
    entity.class() == CLASS
}

/// The figure model shown by 3D layer `layer`, if it shows one.
#[must_use]
pub fn of_layer(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    let model = crate::three::model_of(arena, layer)?;
    arena
        .get_by_id(model)
        .ok()
        .filter(|entity| is_figure(entity))
        .map(Entity::id)
}

/// The edit that poses the figure of 3D layer `layer` as `pose` and draws
/// the layer again, as one step named `label`.
#[must_use]
pub fn set_pose(arena: &Arena, layer: EntityId, pose: &Pose, label: &str) -> Option<Edit> {
    let model = of_layer(arena, layer)?;
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
    let body = Body::of(arena.get_by_id(model).ok()?);
    let buffers = crate::three::render_framed(&mesh(pose, &body), &view, size, Some(FRAME));
    let drawn = crate::three::draw(&buffers, &view)?;
    Some(
        Edit::new(label)
            .then(SetProperty::new(model, POSE, pose.to_text()))
            .then(ReplaceComponent::new(layer, drawn, label)),
    )
}

/// The edit that shapes the figure of 3D layer `layer` as `body` and draws
/// the layer again, as one step named `label`.
#[must_use]
pub fn set_body(arena: &Arena, layer: EntityId, body: &Body, label: &str) -> Option<Edit> {
    let model = of_layer(arena, layer)?;
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
    let pose = Pose::of(arena.get_by_id(model).ok()?);
    let buffers = crate::three::render_framed(&mesh(&pose, body), &view, size, Some(FRAME));
    let drawn = crate::three::draw(&buffers, &view)?;
    Some(
        Edit::new(label)
            .then(SetProperty::new(model, HEIGHT, f64::from(body.height)))
            .then(SetProperty::new(model, HEADS, f64::from(body.heads)))
            .then(ReplaceComponent::new(layer, drawn, label)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_takes_its_height_and_head_ratio() {
        let rest = Pose::rest();
        let top = |body: &Body| {
            bones(&rest, body)
                .iter()
                .flat_map(|(a, b)| [a[1], b[1]])
                .fold(f32::MIN, f32::max)
        };
        let head = |body: &Body| {
            let (a, b) = bones(&rest, body)[HEAD];
            b[1] - a[1]
        };
        let still = Body::default();
        let tall = Body {
            height: 204.0,
            ..still
        };
        assert!((top(&tall) / top(&still) - 1.2).abs() < 0.01, "taller");
        let chibi = Body {
            heads: 3.0,
            ..still
        };
        assert!(
            (head(&chibi) / head(&still) - 2.5).abs() < 0.01,
            "a bigger head"
        );
        assert!((top(&chibi) - top(&still)).abs() < 0.08, "about as tall");
        // Kept as properties, and clamped.
        let model = new_model("Figure")
            .with_property(HEIGHT, Value::from(180.0))
            .with_property(HEADS, Value::from(99.0));
        assert_eq!(
            Body::of(&model),
            Body {
                height: 180.0,
                heads: 10.0
            }
        );
    }

    #[test]
    fn a_pose_round_trips_as_text_and_flips_back() {
        let pose = preset(2);
        assert_eq!(Pose::from_text(&pose.to_text()), pose);
        assert_eq!(pose.flipped().flipped(), pose, "flipping twice is the same");
        // Walking, the left thigh forward; flipped, the right one is.
        let flipped = pose.flipped();
        assert_eq!(flipped.0[14][0], pose.0[11][0]);
        assert!(Pose::from_text("nonsense") == Pose::rest());
    }

    #[test]
    fn turning_a_joint_moves_what_hangs_from_it_only() {
        let rest = Pose::rest();
        let before = bones(&rest, &Body::default());
        let mut raised = rest.clone();
        raised.0[5] = [0.0, 0.0, 90.0];
        let after = bones(&raised, &Body::default());
        // The left hand rises to shoulder height and out to the left.
        let hand = after[7].1;
        assert!(hand[0] > 0.6, "out to the left: {hand:?}");
        assert!(
            (hand[1] - before[5].0[1]).abs() < 0.05,
            "at shoulder height"
        );
        // The right arm and the legs stay.
        assert_eq!(after[10], before[10]);
        assert_eq!(after[16], before[16]);
        // Bending the knee forward sends the foot back.
        let mut knee = rest;
        knee.0[12] = [90.0, 0.0, 0.0];
        assert!(
            bones(&knee, &Body::default())[13].0[2] < -0.3,
            "the foot goes behind"
        );
    }

    #[test]
    fn the_body_is_a_closed_shape_about_two_tall() {
        let geometry = geometry(&Pose::default(), &Body::default());
        let (min, max) = geometry.bounds().unwrap();
        let height = max[1] - min[1];
        assert!((1.7..2.1).contains(&height), "about two tall: {height}");
        assert!(min[1] > -0.1, "standing on the ground");
        assert!(
            !mesh(&Pose::default(), &Body::default())
                .indices()
                .is_empty()
        );
    }
}
