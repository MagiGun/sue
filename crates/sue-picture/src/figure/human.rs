//! The drawing figures' bodies: a real skinned human mesh with morph
//! targets, shaped to a [`Body`], cut into the hand and head figures, and
//! bent to a [`Pose`].
//!
//! The mesh, its skin weights, joints and morph targets come from
//! MakeHuman's base mesh, rig and targets (CC0 assets; see
//! `assets/figures/LICENSE-ASSETS.md`), converted by Sue's own script to
//! the compact `human.sfig`. Everything else here is Sue's: the joints
//! people pose (the same seventeen as before, then shoulders, toes,
//! fingers, jaw and eyes), the rest stance poses are measured from (arms
//! straight down, as the old figure stood, though the mesh is modeled with
//! its arms out), the hand and head figures cut from the body and made
//! finer, the guide lines, and the veins, tendons and knuckles of the
//! hands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::{
    Body, Build, HAND, HEAD_JOINTS, IDENTITY, JOINTS, Joint, Kind, Mat3, Pose, SLIDERS, Side, add,
    apply, between, euler, mul, transpose, unit,
};
use crate::three::{Relief, Surface};

/// A body joint's number in [`JOINTS`], by what it is.
pub(crate) mod at {
    pub(crate) const HIPS: usize = 0;
    pub(crate) const SPINE: usize = 1;
    pub(crate) const CHEST: usize = 2;
    pub(crate) const NECK: usize = 3;
    pub(crate) const HEAD: usize = 4;
    /// The upper arm, forearm and hand of each side.
    pub(crate) const ARMS: [[usize; 3]; 2] = [[5, 6, 7], [8, 9, 10]];
    /// The thigh, shin and foot of each side.
    pub(crate) const LEGS: [[usize; 3]; 2] = [[11, 12, 13], [14, 15, 16]];
    pub(crate) const SHOULDERS: [usize; 2] = [17, 18];
    /// Each side's first finger joint; fifteen follow in the hand's order.
    pub(crate) const FINGERS: [usize; 2] = [21, 36];
    pub(crate) const JAW: usize = 51;
    pub(crate) const EYES: [usize; 2] = [52, 53];
}

/// One morph target: the points it moves and how far, and how far it
/// moves each joint's two ends.
struct Morph {
    points: Vec<u32>,
    moves: Vec<[f32; 3]>,
    joints: Vec<[[f32; 3]; 2]>,
}

/// The human mesh as shipped: in meters, facing +z, its left at +x.
struct Asset {
    points: Vec<[f32; 3]>,
    quads: Vec<[u32; 4]>,
    /// Up to four joints (numbers in [`JOINTS`]) and weights a point.
    skin: Vec<[(u8, f32); 4]>,
    /// Each joint of [`JOINTS`]: where it starts and where its bone ends.
    joints: Vec<[[f32; 3]; 2]>,
    morphs: HashMap<String, Morph>,
}

/// Reads little endian values off the front of a byte slice.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&mut self, count: usize) -> &[u8] {
        let (head, tail) = self.0.split_at(count.min(self.0.len()));
        self.0 = tail;
        head
    }
    fn u8(&mut self) -> u8 {
        self.bytes(1).first().copied().unwrap_or(0)
    }
    fn u16(&mut self) -> u16 {
        let b = self.bytes(2);
        u16::from_le_bytes([b[0], b[1]])
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn u32(&mut self) -> u32 {
        let b = self.bytes(4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    fn point(&mut self) -> [f32; 3] {
        [self.f32(), self.f32(), self.f32()]
    }
    fn text(&mut self) -> String {
        let length = usize::from(self.u8());
        String::from_utf8_lossy(self.bytes(length)).into_owned()
    }
}

/// The shipped human mesh, read once.
fn asset() -> &'static Asset {
    static ASSET: OnceLock<Asset> = OnceLock::new();
    ASSET.get_or_init(|| {
        let packed = include_bytes!("../../assets/figures/human.sfig");
        let data = miniz_oxide::inflate::decompress_to_vec_zlib(packed)
            .expect("the shipped figure asset unpacks");
        let mut r = Reader(&data);
        assert_eq!(r.bytes(8), b"SUEFIG1\0", "the shipped figure asset");
        let (points, quads, joints, morphs) = (
            r.u32() as usize,
            r.u32() as usize,
            r.u32() as usize,
            r.u32() as usize,
        );
        let unit = r.f32();
        let scaled = |p: [f32; 3]| p.map(|value| value * unit);
        let positions: Vec<[f32; 3]> = (0..points).map(|_| scaled(r.point())).collect();
        let faces: Vec<[u32; 4]> = (0..quads)
            .map(|_| [0; 4].map(|_: i32| u32::from(r.u16())))
            .collect();
        let raw_skin: Vec<([u8; 4], [u8; 4])> = (0..points)
            .map(|_| ([0; 4].map(|_: i32| r.u8()), [0; 4].map(|_: i32| r.u8())))
            .collect();
        // The asset's joints, each found in Sue's table by its name.
        let mut slot = vec![0_usize; joints];
        let mut ends = vec![[[0.0_f32; 3]; 2]; JOINTS.len()];
        for slot in slot.iter_mut() {
            let name = r.text();
            let _parent = r.i16();
            let (head, tail) = (scaled(r.point()), scaled(r.point()));
            *slot = JOINTS
                .iter()
                .position(|joint| joint.name == name)
                .expect("the asset's joints are Sue's");
            ends[*slot] = [head, tail];
        }
        let skin = raw_skin
            .iter()
            .map(|(joints, weights)| {
                [0, 1, 2, 3].map(|k| {
                    (
                        slot[usize::from(joints[k])] as u8,
                        f32::from(weights[k]) / 255.0,
                    )
                })
            })
            .collect();
        let mut named = HashMap::new();
        for _ in 0..morphs {
            let name = r.text();
            let step = r.f32() * unit;
            let count = r.u32() as usize;
            let points: Vec<u32> = (0..count).map(|_| u32::from(r.u16())).collect();
            let moves = (0..count)
                .map(|_| [0; 3].map(|_: i32| f32::from(r.i16()) * step))
                .collect();
            let mut moved = vec![[[0.0_f32; 3]; 2]; JOINTS.len()];
            for &slot in &slot {
                moved[slot] = [scaled(r.point()), scaled(r.point())];
            }
            named.insert(
                name,
                Morph {
                    points,
                    moves,
                    joints: moved,
                },
            );
        }
        Asset {
            points: positions,
            quads: faces,
            skin,
            joints: ends,
            morphs: named,
        }
    })
}

/// A figure shaped and ready to pose: its points at rest as modeled, how
/// each follows the joints, its joints as poses turn them, and what lies
/// on its surface.
pub(crate) struct Shaped {
    /// The points as modeled (the bind stance), at scale 1.
    pub rest: Vec<[f32; 3]>,
    /// Up to four joints and weights a point.
    pub skin: Vec<[(u8, f32); 4]>,
    /// The triangles.
    pub tris: Vec<u32>,
    /// The joints, in the stance poses are measured from.
    pub joints: Vec<Joint>,
    /// Each joint as modeled: how it is turned from the stance poses are
    /// measured from, and where it is.
    pub bind: Vec<(Mat3, [f32; 3])>,
    /// The joints, parents first.
    pub order: Vec<usize>,
    /// Guide lines: each end a place along an edge between two points.
    pub guides: Vec<[(u32, u32, f32); 2]>,
    /// Where each point is on the hands' relief (see [`Surface::detail`]).
    pub detail: Vec<[f32; 4]>,
    /// The relief's axes as modeled, and the joint that carries them.
    pub axes: Vec<(usize, [[f32; 3]; 2])>,
    /// The hands' relief.
    pub relief: Option<Arc<Relief>>,
    /// For each of a body's hands: the hand figure's axes in the hand
    /// joint's own frame, which carries a hand pose onto the body.
    pub hands: [Mat3; 2],
    /// For each of a hand's three thumb joints: the axis it really bends
    /// about, in the hand's own axes at rest.
    pub thumb_axes: Vec<[f32; 3]>,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], by: f32) -> [f32; 3] {
    a.map(|value| value * by)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn unit_or(a: [f32; 3], or: [f32; 3]) -> [f32; 3] {
    unit(a).unwrap_or(or)
}

/// The turn whose x, y and z go to `x`, `y` and `z`.
fn columns(x: [f32; 3], y: [f32; 3], z: [f32; 3]) -> Mat3 {
    [0, 1, 2].map(|row| [x[row], y[row], z[row]])
}

/// The turn that takes y to `y` and x as near `x` as stays square to it.
fn framed(x: [f32; 3], y: [f32; 3]) -> Mat3 {
    let y = unit_or(y, [0.0, 1.0, 0.0]);
    let x = unit_or(sub(x, scale(y, dot(x, y))), [1.0, 0.0, 0.0]);
    columns(x, y, cross(x, y))
}

/// `m` mirrored left to right.
fn mirrored(m: &Mat3) -> Mat3 {
    let mut out = *m;
    for (row, line) in out.iter_mut().enumerate() {
        for (column, value) in line.iter_mut().enumerate() {
            if (row == 0) != (column == 0) {
                *value = -*value;
            }
        }
    }
    out
}

/// The body's points and joints (both ends of each) shaped by its type,
/// chart and sliders, in meters as modeled.
fn morphed(body: &Body) -> (Vec<[f32; 3]>, Vec<[[f32; 3]; 2]>) {
    let asset = asset();
    let sex = body.build.key();
    let mut weights: Vec<(String, f32)> = vec![
        (sex.to_string(), 1.0),
        (format!("{sex}:ideal"), 0.5),
    ];
    let mut slide = |name: &str, value: f32| {
        if value > 1e-4 {
            weights.push((format!("{name}+"), value));
        } else if value < -1e-4 {
            weights.push((format!("{name}-"), -value));
        }
    };
    let [whole, head, upper, arms, legs] = body.shape.0;
    slide(&format!("{sex}:fat"), whole[0]);
    slide(&format!("{sex}:muscle"), whole[1]);
    slide("head_fat", head[0]);
    slide("torso_fat", upper[0]);
    slide("torso_muscle", upper[1]);
    slide("arm_fat", arms[0]);
    slide("arm_muscle", arms[1]);
    slide("leg_fat", legs[0]);
    slide("leg_muscle", legs[1]);
    for (index, slider) in SLIDERS.iter().enumerate() {
        let value = body.parts[index];
        match slider.key {
            "veins" | "prominence" => {}
            "bust" => {
                if body.build == Build::Female {
                    slide("bust", value);
                }
            }
            key => slide(key, value),
        }
    }
    let mut points = asset.points.clone();
    let mut joints = asset.joints.clone();
    for (name, weight) in weights {
        let Some(morph) = asset.morphs.get(&name) else {
            continue;
        };
        for (point, by) in morph.points.iter().zip(&morph.moves) {
            let p = &mut points[*point as usize];
            for axis in 0..3 {
                p[axis] += by[axis] * weight;
            }
        }
        for (joint, by) in joints.iter_mut().zip(&morph.joints) {
            for end in 0..2 {
                for axis in 0..3 {
                    joint[end][axis] += by[end][axis] * weight;
                }
            }
        }
    }
    (points, joints)
}

/// How much of point `skin` follows the joints `of`.
fn follows(skin: &[(u8, f32); 4], of: &[usize]) -> f32 {
    skin.iter()
        .filter(|(joint, _)| of.contains(&usize::from(*joint)))
        .map(|(_, weight)| weight)
        .sum()
}

/// The joints of the head: what grows and shrinks with the head to body
/// ratio.
const HEAD_GROUP: [usize; 4] = [at::HEAD, at::JAW, at::EYES[0], at::EYES[1]];

/// Grows or shrinks the head about its joint so the body stands `heads`
/// heads tall, then stands the body on the ground `tall` high.
fn proportioned(
    points: &mut [[f32; 3]],
    joints: &mut [[[f32; 3]; 2]],
    skin: &[[(u8, f32); 4]],
    heads: Option<f32>,
    tall: f32,
) {
    let (low, high) = points
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), p| {
            (low.min(p[1]), high.max(p[1]))
        });
    let mut top = high;
    if let Some(heads) = heads {
        let pivot = joints[at::HEAD][0];
        let chin = points
            .iter()
            .zip(skin)
            .filter(|(_, skin)| follows(skin, &HEAD_GROUP) > 0.95)
            .map(|(p, _)| p[1])
            .fold(f32::INFINITY, f32::min);
        let (whole, head, above) = (high - low, high - chin, high - pivot[1]);
        let by = ((whole - above) / (head * heads - above).max(1e-3)).clamp(0.3, 5.0);
        let grow = |p: [f32; 3], share: f32| add(p, scale(sub(p, pivot), share * (by - 1.0)));
        for (p, skin) in points.iter_mut().zip(skin) {
            let share = follows(skin, &HEAD_GROUP);
            if share > 0.0 {
                *p = grow(*p, share);
            }
        }
        for (index, joint) in joints.iter_mut().enumerate() {
            if HEAD_GROUP.contains(&index) {
                joint[0] = grow(joint[0], 1.0);
                joint[1] = grow(joint[1], 1.0);
            }
        }
        top = pivot[1] + above * by;
    }
    let by = tall / (top - low).max(1e-3);
    let stand = |p: [f32; 3]| [p[0] * by, (p[1] - low) * by, p[2] * by];
    for p in points.iter_mut() {
        *p = stand(*p);
    }
    for joint in joints.iter_mut() {
        *joint = [stand(joint[0]), stand(joint[1])];
    }
}

/// The directions the old hand's thumb bones point at rest, which poses
/// of the thumb are still measured from.
const THUMB_REST: [[f32; 3]; 3] = [
    [-0.016, 0.03, 0.014],
    [-0.006, 0.026, 0.008],
    [-0.003, 0.021, 0.005],
];

/// A hand's fifteen finger joints (both ends of each, in the hand's own
/// axes: palm +z, fingers +y, thumb -x): how each is turned as modeled
/// from the stance poses are measured from (four fingers straight up,
/// the thumb as the old hand held it), and the axis each thumb joint
/// really bends about, in its own frame.
fn finger_frames(ends: &[[[f32; 3]; 2]]) -> (Vec<Mat3>, [[f32; 3]; 3]) {
    let mut frames = Vec::with_capacity(15);
    for (index, [head, tail]) in ends.iter().enumerate().take(12) {
        let _ = index;
        frames.push(framed([1.0, 0.0, 0.0], sub(*tail, *head)));
    }
    let mut parent = IDENTITY;
    let mut axes = [[1.0, 0.0, 0.0]; 3];
    // The thumb closes toward the foot of the little finger.
    let toward = ends[9][0];
    for part in 0..3 {
        let [head, tail] = ends[12 + part];
        let rest = unit_or(THUMB_REST[part], [0.0, 1.0, 0.0]);
        let along = unit_or(sub(tail, head), [0.0, 1.0, 0.0]);
        let frame = mul(&between(apply(&parent, rest), along), &parent);
        let axis = unit_or(cross(along, sub(toward, head)), [1.0, 0.0, 0.0]);
        // Turning about it by a positive angle closes the thumb.
        axes[part] = apply(&transpose(&frame), axis);
        frames.push(frame);
        parent = frame;
    }
    (frames, axes)
}

/// A hand's own axes as modeled on the body: across (toward the little
/// finger on a left hand, so the palm faces +z), along the fingers, and
/// out of the palm.
fn hand_axes(joints: &[[[f32; 3]; 2]], side: usize) -> Mat3 {
    let first = at::FINGERS[side];
    let wrist = joints[at::ARMS[side][2]][0];
    let (index, middle, little) = (joints[first][0], joints[first + 3][0], joints[first + 9][0]);
    let across = if side == 0 {
        sub(little, index)
    } else {
        sub(index, little)
    };
    framed(across, sub(middle, wrist))
}

/// How each body joint is turned as modeled from the stance poses are
/// measured from: the trunk, head and legs as modeled; the arms hanging
/// straight down, bending forward at the elbow; the fingers as the hand
/// figure's. Also each hand's carrying turn and the thumbs' real axes.
fn body_frames(joints: &[[[f32; 3]; 2]]) -> (Vec<Mat3>, [Mat3; 2], Vec<[f32; 3]>) {
    let mut frames = vec![IDENTITY; joints.len()];
    let mut hands = [IDENTITY; 2];
    let mut thumbs = Vec::new();
    let down = [0.0, -1.0, 0.0];
    for side in 0..2 {
        let [upper, fore, hand] = at::ARMS[side];
        let along = |joint: usize| unit_or(sub(joints[joint][1], joints[joint][0]), down);
        let (u, f) = (along(upper), along(fore));
        // The elbow's hinge: square to both, pointing across (+x).
        let mut hinge = unit_or(cross(u, f), [1.0, 0.0, 0.0]);
        if hinge[0] < 0.0 {
            hinge = scale(hinge, -1.0);
        }
        frames[upper] = framed(hinge, scale(u, -1.0));
        frames[fore] = framed(hinge, scale(f, -1.0));
        frames[hand] = mul(
            &between(apply(&frames[fore], down), along(hand)),
            &frames[fore],
        );
        let axes = hand_axes(joints, side);
        let carry = mul(&transpose(&frames[hand]), &axes);
        hands[side] = carry;
        // The fingers in the hand's own axes, the right hand's mirrored
        // so both are worked out as a left hand.
        let wrist = joints[hand][0];
        let first = at::FINGERS[side];
        let own = |p: [f32; 3]| {
            let mut local = apply(&transpose(&axes), sub(p, wrist));
            if side == 1 {
                local[0] = -local[0];
            }
            local
        };
        let ends: Vec<[[f32; 3]; 2]> = (0..15)
            .map(|k| [own(joints[first + k][0]), own(joints[first + k][1])])
            .collect();
        let (fingers, thumb) = finger_frames(&ends);
        for (k, frame) in fingers.iter().enumerate() {
            let frame = if side == 1 { mirrored(frame) } else { *frame };
            frames[first + k] = mul(&mul(&axes, &frame), &transpose(&carry));
        }
        // The thumb's real axes, in the hand's own axes: the left
        // hand's serve both.
        if side == 0 {
            thumbs = thumb.to_vec();
        }
    }
    (frames, hands, thumbs)
}

/// The joints of `table` placed at `ends` and turned as `frames` say, as
/// poses see them: each joint's place in its parent's frame and its bone
/// in its own. Widths are measured off the mesh.
fn rigged(
    table: &[Joint],
    ends: &[[[f32; 3]; 2]],
    frames: &[Mat3],
    points: &[[f32; 3]],
    skin: &[[(u8, f32); 4]],
) -> Vec<Joint> {
    // How thick each part is: the mean distance of its own points from
    // its bone.
    let mut sums = vec![(0.0_f32, 0.0_f32); table.len()];
    for (p, skin) in points.iter().zip(skin) {
        let (joint, weight) = skin[0];
        let joint = usize::from(joint);
        if weight < 0.6 || joint >= table.len() {
            continue;
        }
        let [head, tail] = ends[joint];
        let bone = sub(tail, head);
        let along = (dot(sub(*p, head), bone) / dot(bone, bone).max(1e-9)).clamp(0.0, 1.0);
        sums[joint].0 += length(sub(*p, add(head, scale(bone, along))));
        sums[joint].1 += 1.0;
    }
    table
        .iter()
        .enumerate()
        .map(|(index, joint)| {
            let turned = transpose(&frames[index]);
            let offset = match joint.parent {
                Some(parent) => apply(
                    &transpose(&frames[parent]),
                    sub(ends[index][0], ends[parent][0]),
                ),
                None => ends[index][0],
            };
            let half = if sums[index].1 > 0.0 {
                sums[index].0 / sums[index].1
            } else {
                0.01
            };
            Joint {
                offset,
                bone: apply(&turned, sub(ends[index][1], ends[index][0])),
                start: [half, half],
                end: [half, half],
                ..*joint
            }
        })
        .collect()
}

/// The joints, parents first.
fn ordered(joints: &[Joint]) -> Vec<usize> {
    let mut order = Vec::with_capacity(joints.len());
    let mut done = vec![false; joints.len()];
    while order.len() < joints.len() {
        let before = order.len();
        for (index, joint) in joints.iter().enumerate() {
            if !done[index] && joint.parent.is_none_or(|parent| done[parent]) {
                done[index] = true;
                order.push(index);
            }
        }
        if order.len() == before {
            break;
        }
    }
    order
}

/// Quads as triangles, each cut along its shorter diagonal.
fn triangles(points: &[[f32; 3]], faces: &[Vec<u32>]) -> Vec<u32> {
    let mut out = Vec::with_capacity(faces.len() * 6);
    for face in faces {
        match face.as_slice() {
            [a, b, c] => out.extend([*a, *b, *c]),
            [a, b, c, d] => {
                let far = |x: u32, y: u32| length(sub(points[x as usize], points[y as usize]));
                if far(*a, *c) <= far(*b, *d) {
                    out.extend([*a, *b, *c, *a, *c, *d]);
                } else {
                    out.extend([*a, *b, *d, *b, *c, *d]);
                }
            }
            more => {
                for k in 1..more.len().saturating_sub(1) {
                    out.extend([more[0], more[k], more[k + 1]]);
                }
            }
        }
    }
    out
}

/// A weighted mean of skin weights, kept to its four strongest joints.
fn blended(parts: &[(&[(u8, f32); 4], f32)]) -> [(u8, f32); 4] {
    let mut sum: Vec<(u8, f32)> = Vec::with_capacity(8);
    for (skin, share) in parts {
        for (joint, weight) in skin.iter() {
            if *weight <= 0.0 {
                continue;
            }
            match sum.iter_mut().find(|(j, _)| j == joint) {
                Some(found) => found.1 += weight * share,
                None => sum.push((*joint, weight * share)),
            }
        }
    }
    sum.sort_by(|a, b| b.1.total_cmp(&a.1));
    sum.truncate(4);
    let total: f32 = sum.iter().map(|(_, weight)| weight).sum();
    let mut out = [(0_u8, 0.0_f32); 4];
    for (slot, (joint, weight)) in out.iter_mut().zip(sum) {
        *slot = (joint, weight / total.max(1e-9));
    }
    out
}

/// One step of Catmull-Clark subdivision: every face split into quads
/// round its middle and the whole surface smoothed, the skin weights
/// carried along.
fn subdivided(
    points: &[[f32; 3]],
    skin: &[[(u8, f32); 4]],
    faces: &[Vec<u32>],
) -> (Vec<[f32; 3]>, Vec<[(u8, f32); 4]>, Vec<Vec<u32>>) {
    let count = points.len();
    // Each new point as shares of the old ones.
    let mut shares: Vec<Vec<(u32, f32)>> = Vec::new();
    let face_point = |face: &[u32]| -> Vec<(u32, f32)> {
        face.iter().map(|&v| (v, 1.0 / face.len() as f32)).collect()
    };
    let mut edges: HashMap<(u32, u32), (usize, Vec<usize>)> = HashMap::new();
    let mut edge_list: Vec<(u32, u32)> = Vec::new();
    let mut faces_of: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut edges_of: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (index, face) in faces.iter().enumerate() {
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            faces_of[a as usize].push(index);
            let key = (a.min(b), a.max(b));
            let entry = edges.entry(key).or_insert_with(|| {
                edge_list.push(key);
                edges_of[key.0 as usize].push(edge_list.len() - 1);
                edges_of[key.1 as usize].push(edge_list.len() - 1);
                (edge_list.len() - 1, Vec::new())
            });
            entry.1.push(index);
        }
    }
    // Old points moved, then one point a face, then one an edge.
    for v in 0..count {
        let (around, lines) = (&faces_of[v], &edges_of[v]);
        let border: Vec<u32> = lines
            .iter()
            .filter(|&&e| edges[&edge_list[e]].1.len() == 1)
            .map(|&e| {
                let (a, b) = edge_list[e];
                if a as usize == v { b } else { a }
            })
            .collect();
        let mut mine: Vec<(u32, f32)> = Vec::new();
        if border.len() >= 2 || around.is_empty() {
            mine.push((v as u32, 0.75));
            for other in border.iter().take(2) {
                mine.push((*other, 0.125));
            }
            if border.len() < 2 {
                mine = vec![(v as u32, 1.0)];
            }
        } else {
            let n = around.len() as f32;
            mine.push((v as u32, (n - 3.0) / n));
            for &face in around {
                for (other, share) in face_point(&faces[face]) {
                    mine.push((other, share / (n * n)));
                }
            }
            let lines_n = lines.len() as f32;
            for &e in lines {
                let (a, b) = edge_list[e];
                mine.push((a, 1.0 / (n * lines_n)));
                mine.push((b, 1.0 / (n * lines_n)));
            }
        }
        shares.push(mine);
    }
    for face in faces {
        shares.push(face_point(face));
    }
    for key in &edge_list {
        let around = &edges[key].1;
        let mut mine = Vec::new();
        if around.len() == 2 {
            mine.push((key.0, 0.25));
            mine.push((key.1, 0.25));
            for &face in around {
                for (other, share) in face_point(&faces[face]) {
                    mine.push((other, share * 0.25));
                }
            }
        } else {
            mine.push((key.0, 0.5));
            mine.push((key.1, 0.5));
        }
        shares.push(mine);
    }
    let new_points = shares
        .iter()
        .map(|mine| {
            let mut p = [0.0_f32; 3];
            for (other, share) in mine {
                for axis in 0..3 {
                    p[axis] += points[*other as usize][axis] * share;
                }
            }
            p
        })
        .collect();
    let new_skin = shares
        .iter()
        .map(|mine| {
            let parts: Vec<(&[(u8, f32); 4], f32)> = mine
                .iter()
                .map(|(other, share)| (&skin[*other as usize], *share))
                .collect();
            blended(&parts)
        })
        .collect();
    let mut new_faces = Vec::with_capacity(faces.len() * 4);
    for (index, face) in faces.iter().enumerate() {
        let middle = (count + index) as u32;
        let edge = |a: u32, b: u32| (count + faces.len() + edges[&(a.min(b), a.max(b))].0) as u32;
        for k in 0..face.len() {
            let (before, here, after) = (
                face[(k + face.len() - 1) % face.len()],
                face[k],
                face[(k + 1) % face.len()],
            );
            new_faces.push(vec![here, edge(here, after), middle, edge(before, here)]);
        }
    }
    (new_points, new_skin, new_faces)
}

/// The part of the body whose points `keep` picks, closed where it was
/// cut away: its points, their skin and its faces.
fn cut(
    points: &[[f32; 3]],
    skin: &[[(u8, f32); 4]],
    keep: impl Fn(usize) -> bool,
    flat: ([f32; 3], [f32; 3]),
) -> (Vec<[f32; 3]>, Vec<[(u8, f32); 4]>, Vec<Vec<u32>>) {
    let asset = asset();
    let mut slot: Vec<Option<u32>> = vec![None; points.len()];
    let (mut out_points, mut out_skin, mut faces) = (Vec::new(), Vec::new(), Vec::new());
    for quad in &asset.quads {
        if !quad.iter().all(|&v| keep(v as usize)) {
            continue;
        }
        let face: Vec<u32> = quad
            .iter()
            .map(|&v| {
                *slot[v as usize].get_or_insert_with(|| {
                    out_points.push(points[v as usize]);
                    out_skin.push(skin[v as usize]);
                    (out_points.len() - 1) as u32
                })
            })
            .collect();
        faces.push(face);
    }
    // The open edges, each walked round into a loop and closed with a
    // fan from its middle.
    let mut open: HashMap<u32, u32> = HashMap::new();
    let mut seen: HashMap<(u32, u32), i32> = HashMap::new();
    for face in &faces {
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            *seen.entry((a, b)).or_insert(0) += 1;
        }
    }
    for &(a, b) in seen.keys() {
        if !seen.contains_key(&(b, a)) {
            // The cap walks the edge the other way round.
            open.insert(b, a);
        }
    }
    while let Some(start) = open.keys().next().copied() {
        let mut ring = vec![start];
        let mut here = start;
        while let Some(next) = open.remove(&here) {
            if next == start {
                break;
            }
            ring.push(next);
            here = next;
        }
        if ring.len() < 3 {
            continue;
        }
        // A real cut (a large ring) ends flat on the plane it was cut
        // by, so the figure stands on a clean foot.
        if ring.len() > 10 {
            let (through, normal) = flat;
            for &v in &ring {
                let p = &mut out_points[v as usize];
                let off = dot(sub(*p, through), normal);
                *p = sub(*p, scale(normal, off));
            }
        }
        let mut middle = [0.0_f32; 3];
        for &v in &ring {
            middle = add(middle, out_points[v as usize]);
        }
        let middle = scale(middle, 1.0 / ring.len() as f32);
        let parts: Vec<(&[(u8, f32); 4], f32)> = ring
            .iter()
            .map(|&v| (&out_skin[v as usize], 1.0))
            .collect();
        let middle_skin = blended(&parts);
        let hub = out_points.len() as u32;
        out_points.push(middle);
        out_skin.push(middle_skin);
        for k in 0..ring.len() {
            faces.push(vec![ring[k], ring[(k + 1) % ring.len()], hub]);
        }
    }
    (out_points, out_skin, faces)
}

fn bell(r: f32) -> f32 {
    if r >= 1.0 {
        0.0
    } else {
        let k = 1.0 - r * r;
        k * k
    }
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far `p` is from the line from `a` to `b`, and how far along it
/// (0 to 1) the nearest point is.
fn to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let along =
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
    let (qx, qy) = (a[0] + dx * along, a[1] + dy * along);
    (((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt(), along)
}

/// The veins on the back of a hand, each a line of points across (-1 at
/// the index knuckle, 1 at the little finger's) and up (0 at the wrist,
/// 1 at the knuckles): two long veins down either side that run on up
/// the forearm, joined by an arch below the knuckles, with branches
/// between the fingers. Drawn by hand for Sue.
const VEINS: [&[[f32; 2]]; 7] = [
    &[
        [-0.74, 0.93],
        [-0.63, 0.76],
        [-0.5, 0.57],
        [-0.5, 0.4],
        [-0.63, 0.22],
        [-0.76, 0.04],
        [-0.8, -0.22],
        [-0.72, -0.62],
        [-0.76, -0.95],
    ],
    &[[-0.06, 0.96], [-0.13, 0.79], [-0.29, 0.64], [-0.5, 0.57]],
    &[
        [0.56, 0.91],
        [0.5, 0.74],
        [0.37, 0.6],
        [0.3, 0.42],
        [0.4, 0.24],
        [0.55, 0.06],
        [0.63, -0.22],
        [0.56, -0.62],
        [0.6, -0.95],
    ],
    &[[-0.13, 0.79], [0.1, 0.68], [0.37, 0.6]],
    &[
        [0.3, 0.42],
        [0.06, 0.28],
        [-0.08, 0.09],
        [-0.04, -0.2],
        [-0.12, -0.6],
        [-0.08, -0.95],
    ],
    &[[0.95, 0.8], [0.84, 0.6], [0.7, 0.4], [0.55, 0.06]],
    &[[-0.5, 0.4], [-0.34, 0.24], [-0.3, 0.02], [-0.4, -0.3]],
];

/// A line of points with its corners rounded, twice.
fn rounded(line: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut points = line.to_vec();
    for _ in 0..2 {
        let mut next = vec![points[0]];
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            next.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25]);
            next.push([a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75]);
        }
        next.push(points[points.len() - 1]);
        points = next;
    }
    points
}

/// The back of a hand in relief: its knuckles and the tendons that run
/// to them, and its veins, over the hand's own across and up (meters,
/// the wrist at 0).
struct Back {
    /// The knuckles: where, how wide and how high.
    knuckles: Vec<([f32; 2], f32, f32)>,
    /// The tendons, from the wrist to each knuckle.
    tendons: Vec<([f32; 2], [f32; 2])>,
    /// The veins.
    veins: Vec<Vec<[f32; 2]>>,
    prominence: f32,
    vein: f32,
    /// The hand's size against a grown hand's.
    size: f32,
}

impl Back {
    /// For a hand whose fifteen finger joints start at `heads` (in the
    /// hand's own axes).
    fn new(heads: &[[f32; 3]], prominence: f32, vein: f32) -> Self {
        let flat = |p: [f32; 3]| [p[0], p[1]];
        let (index, middle, little) = (flat(heads[0]), flat(heads[3]), flat(heads[9]));
        let size = (middle[1] / 0.09).clamp(0.2, 5.0);
        let mut knuckles = Vec::new();
        let mut tendons = Vec::new();
        for finger in 0..4 {
            let base = flat(heads[finger * 3]);
            knuckles.push((base, 0.0105 * size, 0.0022 * size));
            knuckles.push((flat(heads[finger * 3 + 1]), 0.0075 * size, 0.0008 * size));
            knuckles.push((flat(heads[finger * 3 + 2]), 0.0056 * size, 0.0004 * size));
            tendons.push(([base[0] * 0.3, 0.012 * size], base));
        }
        knuckles.push((flat(heads[13]), 0.009 * size, 0.0016 * size));
        knuckles.push((flat(heads[14]), 0.0065 * size, 0.0008 * size));
        tendons.push(([flat(heads[12])[0] * 0.6, 0.012 * size], flat(heads[13])));
        // The veins laid between the index and little knuckles.
        let (center, half) = ((index[0] + little[0]) / 2.0, (little[0] - index[0]) / 2.0);
        let veins = VEINS
            .iter()
            .map(|line| {
                rounded(line)
                    .iter()
                    .map(|[x, y]| [center + x * half, y * middle[1]])
                    .collect()
            })
            .collect();
        Self {
            knuckles,
            tendons,
            veins,
            prominence,
            vein,
            size,
        }
    }

    /// How high the knuckles and tendons stand at `p`.
    fn bones(&self, p: [f32; 2]) -> f32 {
        let mut height = 0.0_f32;
        for (at, wide, high) in &self.knuckles {
            let far = ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2)).sqrt();
            height += high * bell(far / wide);
        }
        let mut ridge = 0.0_f32;
        for (from, to) in &self.tendons {
            let (far, along) = to_segment(p, *from, *to);
            ridge = ridge.max(
                0.001 * self.size
                    * bell(far / (0.0046 * self.size))
                    * smoothstep(0.15, 0.75, along)
                    * (1.0 - smoothstep(0.93, 1.0, along) * 0.5),
            );
        }
        (height + ridge) * self.prominence
    }

    /// How high the veins stand at `p`.
    fn veins(&self, p: [f32; 2]) -> f32 {
        if self.vein <= 0.0 {
            return 0.0;
        }
        let mut height = 0.0_f32;
        for line in &self.veins {
            for pair in line.windows(2) {
                let (far, _) = to_segment(p, pair[0], pair[1]);
                height = height.max(bell(far / (0.0034 * self.size)));
            }
        }
        height * 0.0011 * self.size * self.vein
    }
}

/// The rectangle of a hand's own across and up (meters) its relief
/// covers, for a grown hand: from, and size.
const RELIEF_BOX: ([f32; 2], [f32; 2]) = ([-0.07, -0.1], [0.14, 0.32]);

/// Where the hand point `p` (in the hand's own axes, `normal` its
/// normal) is on the relief, and how much of it it takes: the back of
/// the hand takes it whole, the sides less, the palm none.
fn on_relief(p: [f32; 3], normal: [f32; 3], size: f32) -> [f32; 3] {
    let (from, span) = RELIEF_BOX;
    let u = (p[0] / size - from[0]) / span[0];
    let v = (p[1] / size - from[1]) / span[1];
    let back = smoothstep(0.05, 0.55, -normal[2]);
    let inside = smoothstep(0.0, 0.04, u.min(1.0 - u).min(v).min(1.0 - v));
    [u, v, back * inside]
}

/// The relief of `back` as heights over [`RELIEF_BOX`].
fn relief_of(back: &Back) -> Relief {
    let (from, span) = RELIEF_BOX;
    let cell = 0.0006 * back.size;
    let size = [
        (span[0] * back.size / cell) as usize,
        (span[1] * back.size / cell) as usize,
    ];
    let mut heights = Vec::with_capacity(size[0] * size[1]);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let p = [
                from[0] * back.size + x as f32 * cell,
                from[1] * back.size + y as f32 * cell,
            ];
            // The knuckles and tendons are in the mesh already; the
            // light only sharpens them.
            heights.push(0.7 * back.veins(p) + 0.35 * back.bones(p));
        }
    }
    Relief {
        size,
        cell,
        heights,
    }
}

/// Where the plane through `through` square to `normal` cuts the
/// triangles `keep` picks, as guide line pieces.
fn sliced(
    points: &[[f32; 3]],
    tris: &[u32],
    through: [f32; 3],
    normal: [f32; 3],
    keep: impl Fn(&[u32; 3]) -> bool,
    guides: &mut Vec<[(u32, u32, f32); 2]>,
) {
    let side = |v: u32| dot(sub(points[v as usize], through), normal);
    for t in tris.as_chunks::<3>().0 {
        let d = t.map(side);
        if (d[0] > 0.0) == (d[1] > 0.0) && (d[1] > 0.0) == (d[2] > 0.0) {
            continue;
        }
        if !keep(t) {
            continue;
        }
        let mut ends = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            if (d[a] > 0.0) != (d[b] > 0.0) {
                ends.push((t[a], t[b], d[a] / (d[a] - d[b])));
            }
        }
        if let [a, b] = ends.as_slice() {
            guides.push([*a, *b]);
        }
    }
}

/// The guide lines of a body: the middle line down the trunk and head,
/// and a ring round the neck, the chest, the waist and each limb's
/// joints, as on an artist's mannequin.
fn body_guides(
    points: &[[f32; 3]],
    skin: &[[(u8, f32); 4]],
    tris: &[u32],
    joints: &[[[f32; 3]; 2]],
    tall: f32,
) -> Vec<[(u32, u32, f32); 2]> {
    let mut guides = Vec::new();
    let body = asset().points.len() - 144;
    let within = |t: &[u32; 3], of: &[usize], least: f32| {
        t.iter()
            .all(|&v| (v as usize) < body && follows(&skin[v as usize], of) >= least)
    };
    let near = |t: &[u32; 3], at: [f32; 3], reach: f32| {
        t.iter()
            .all(|&v| length(sub(points[v as usize], at)) < reach)
    };
    let trunk = [
        at::HIPS,
        at::SPINE,
        at::CHEST,
        at::NECK,
        at::HEAD,
        at::JAW,
    ];
    sliced(
        points,
        tris,
        [0.0004 * tall, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        |t| within(t, &trunk, 0.6),
        &mut guides,
    );
    let up = [0.0, 1.0, 0.0];
    let along = |joint: usize, share: f32| {
        add(
            joints[joint][0],
            scale(sub(joints[joint][1], joints[joint][0]), share),
        )
    };
    // Round the trunk: the waist, the chest and the neck.
    sliced(
        points,
        tris,
        along(at::SPINE, 0.45),
        up,
        |t| within(t, &[at::HIPS, at::SPINE, at::CHEST], 0.9),
        &mut guides,
    );
    sliced(
        points,
        tris,
        along(at::CHEST, 0.5),
        up,
        |t| within(t, &[at::SPINE, at::CHEST, 17, 18], 0.9) && near(t, along(at::CHEST, 0.5), 0.13 * tall),
        &mut guides,
    );
    sliced(
        points,
        tris,
        along(at::NECK, 0.45),
        sub(joints[at::NECK][1], joints[at::NECK][0]),
        |t| within(t, &[at::NECK, at::HEAD, at::CHEST, at::JAW], 0.9) && near(t, along(at::NECK, 0.45), 0.06 * tall),
        &mut guides,
    );
    // The eye line round the head.
    sliced(
        points,
        tris,
        joints[at::EYES[0]][0],
        up,
        |t| within(t, &HEAD_GROUP, 0.9),
        &mut guides,
    );
    for side in 0..2 {
        let [upper, fore, hand] = at::ARMS[side];
        let [thigh, shin, foot] = at::LEGS[side];
        let shoulder = at::SHOULDERS[side];
        let rings: [(usize, f32, &[usize], f32); 6] = [
            (upper, 0.16, &[upper, shoulder, at::CHEST], 0.07),
            (upper, 0.97, &[upper, fore], 0.06),
            (fore, 0.96, &[fore, hand], 0.05),
            (thigh, 0.14, &[thigh, at::HIPS], 0.1),
            (thigh, 0.97, &[thigh, shin], 0.08),
            (shin, 0.95, &[shin, foot], 0.06),
        ];
        for (joint, share, of, reach) in rings {
            let through = along(joint, share);
            sliced(
                points,
                tris,
                through,
                sub(joints[joint][1], joints[joint][0]),
                |t| within(t, of, 0.9) && near(t, through, reach * tall),
                &mut guides,
            );
        }
    }
    guides
}

/// The hand joints' ends in the hand's own axes, from the body's.
fn hand_ends(joints: &[[[f32; 3]; 2]], side: usize) -> (Mat3, [f32; 3], Vec<[[f32; 3]; 2]>) {
    let axes = hand_axes(joints, side);
    let wrist = joints[at::ARMS[side][2]][0];
    let own = |p: [f32; 3]| apply(&transpose(&axes), sub(p, wrist));
    let first = at::FINGERS[side];
    let ends = (0..15)
        .map(|k| [own(joints[first + k][0]), own(joints[first + k][1])])
        .collect();
    (axes, wrist, ends)
}

/// Raises the knuckles, tendons (and veins, on a mesh fine enough to
/// hold them) of a hand out of its points, and says where each point is
/// on the relief. `own` takes a point and its normal into the hand's own
/// axes; `out` takes a direction back.
#[allow(clippy::too_many_arguments)]
fn embossed(
    points: &mut [[f32; 3]],
    normals: &[[f32; 3]],
    detail: &mut [[f32; 4]],
    picks: impl Fn(usize) -> bool,
    own: impl Fn([f32; 3], bool) -> [f32; 3],
    back: &Back,
    veins: bool,
    group: f32,
) {
    for index in 0..points.len() {
        if !picks(index) {
            continue;
        }
        let p = own(points[index], true);
        let n = own(normals[index], false);
        let [u, v, amount] = on_relief(p, n, back.size);
        if amount <= 0.0 {
            continue;
        }
        detail[index] = [u, v, amount, group];
        let flat = [p[0], p[1]];
        let mut height = back.bones(flat);
        if veins {
            height += back.veins(flat);
        }
        points[index] = add(points[index], scale(normals[index], height * amount));
    }
}

/// A body, shaped.
fn shaped_body(body: &Body) -> Shaped {
    let asset = asset();
    let (mut points, mut ends) = morphed(body);
    let tall = 1.9 * body.height / Body::HEIGHT;
    proportioned(&mut points, &mut ends, &asset.skin, Some(body.heads), tall);
    let (frames, hands, thumb_axes) = body_frames(&ends);
    let joints = rigged(&JOINTS, &ends, &frames, &points, &asset.skin);
    let faces: Vec<Vec<u32>> = asset.quads.iter().map(|quad| quad.to_vec()).collect();
    let tris = triangles(&points, &faces);
    let guides = body_guides(&points, &asset.skin, &tris, &ends, tall);
    // The hands' relief: one relief (a left hand's), both hands on it.
    let normals = crate::three::smooth_normals(&points, &tris);
    let mut detail = vec![[0.0_f32; 4]; points.len()];
    let mut axes = Vec::new();
    let mut relief = None;
    let (vein, prominence) = (body.slider("veins"), body.slider("prominence"));
    for side in 0..2 {
        let (hand_frame, wrist, mut hand) = hand_ends(&ends, side);
        if side == 1 {
            for end in hand.iter_mut().flatten() {
                end[0] = -end[0];
            }
        }
        let heads: Vec<[f32; 3]> = hand.iter().map(|end| end[0]).collect();
        let back = Back::new(&heads, prominence, vein);
        let chain: Vec<usize> = std::iter::once(at::ARMS[side][2])
            .chain(at::FINGERS[side]..at::FINGERS[side] + 15)
            .chain(std::iter::once(at::ARMS[side][1]))
            .collect();
        let flip = if side == 1 { -1.0 } else { 1.0 };
        embossed(
            &mut points,
            &normals,
            &mut detail,
            |index| follows(&asset.skin[index], &chain) > 0.5,
            |p, place| {
                let mut local = apply(
                    &transpose(&hand_frame),
                    if place { sub(p, wrist) } else { p },
                );
                local[0] *= flip;
                local
            },
            &back,
            false,
            side as f32,
        );
        let across = apply(&hand_frame, [flip, 0.0, 0.0]);
        let up = apply(&hand_frame, [0.0, 1.0, 0.0]);
        axes.push((at::ARMS[side][2], [across, up]));
        if side == 0 {
            relief = Some(Arc::new(relief_of(&back)));
        }
    }
    let bind = frames.iter().zip(&ends).map(|(f, e)| (*f, e[0])).collect();
    Shaped {
        order: ordered(&joints),
        rest: points,
        skin: asset.skin.clone(),
        tris,
        joints,
        bind,
        guides,
        detail,
        axes,
        relief,
        hands,
        thumb_axes,
    }
}

/// A hand figure, shaped: the body's left hand and a little of its
/// forearm, in the hand's own axes, made finer.
fn shaped_hand(body: &Body) -> Shaped {
    let asset = asset();
    let (mut points, mut ends) = morphed(body);
    let tall = 1.7 * body.height / Body::HEIGHT;
    proportioned(&mut points, &mut ends, &asset.skin, None, tall);
    let (axes, wrist, hand) = hand_ends(&ends, 0);
    let own = |p: [f32; 3]| apply(&transpose(&axes), sub(p, wrist));
    let chain: Vec<usize> = std::iter::once(at::ARMS[0][2])
        .chain(at::FINGERS[0]..at::FINGERS[0] + 15)
        .collect();
    let size = hand[3][0][1] / 0.09;
    let local: Vec<[f32; 3]> = points.iter().map(|p| own(*p)).collect();
    // The hand's joints: the palm, then the fingers.
    let to_hand = |joint: u8| -> u8 {
        let joint = usize::from(joint);
        if (at::FINGERS[0]..at::FINGERS[0] + 15).contains(&joint) {
            (joint - at::FINGERS[0] + 1) as u8
        } else {
            0
        }
    };
    let skin: Vec<[(u8, f32); 4]> = asset
        .skin
        .iter()
        .map(|skin| blended(&[(&skin.map(|(joint, weight)| (to_hand(joint), weight)), 1.0)]))
        .collect();
    let foot = [0.0, -0.075 * size, 0.0];
    let (mut points, mut skin, mut faces) = cut(
        &local,
        &skin,
        |index| {
            local[index][1] > foot[1]
                && length(local[index]) < 0.3 * size
                && follows(&asset.skin[index], &chain) + follows(&asset.skin[index], &[at::ARMS[0][1]])
                    > 0.5
        },
        (foot, [0.0, 1.0, 0.0]),
    );
    for _ in 0..2 {
        (points, skin, faces) = subdivided(&points, &skin, &faces);
    }
    let tris = triangles(&points, &faces);
    let normals = crate::three::smooth_normals(&points, &tris);
    let heads: Vec<[f32; 3]> = hand.iter().map(|end| end[0]).collect();
    let back = Back::new(&heads, body.slider("prominence"), body.slider("veins"));
    let mut detail = vec![[0.0_f32; 4]; points.len()];
    embossed(
        &mut points,
        &normals,
        &mut detail,
        |_| true,
        |p, _| p,
        &back,
        true,
        0.0,
    );
    let mut all_ends = vec![[[0.0_f32; 3], hand[3][0]]];
    all_ends.extend(hand.iter().copied());
    let (fingers, thumb) = finger_frames(&hand);
    let mut frames = vec![IDENTITY];
    frames.extend(fingers);
    let joints = rigged(&HAND, &all_ends, &frames, &points, &skin);
    let bind = frames
        .iter()
        .zip(&all_ends)
        .map(|(f, e)| (*f, e[0]))
        .collect();
    Shaped {
        order: ordered(&joints),
        rest: points,
        skin,
        tris,
        joints,
        bind,
        guides: Vec::new(),
        detail,
        axes: vec![(0, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])],
        relief: Some(Arc::new(relief_of(&back))),
        hands: [IDENTITY; 2],
        thumb_axes: thumb.to_vec(),
    }
}

/// A head figure, shaped: the body's head and neck, the neck's foot at
/// the middle, made finer.
fn shaped_head(body: &Body) -> Shaped {
    let asset = asset();
    let (mut points, mut ends) = morphed(body);
    let tall = 1.7 * body.height / Body::HEIGHT;
    proportioned(&mut points, &mut ends, &asset.skin, None, tall);
    let foot = ends[at::NECK][0];
    let local: Vec<[f32; 3]> = points.iter().map(|p| sub(*p, foot)).collect();
    let group = [at::NECK, at::HEAD, at::JAW, at::EYES[0], at::EYES[1]];
    // The head's joints: neck, skull, jaw, then the eyes; the nose, brow
    // and ears of the old head stay as joints that hold nothing.
    let to_head = |joint: u8| -> u8 {
        match usize::from(joint) {
            at::HEAD => 1,
            at::JAW => 2,
            52 => 5,
            53 => 6,
            _ => 0,
        }
    };
    let skin: Vec<[(u8, f32); 4]> = asset
        .skin
        .iter()
        .map(|skin| blended(&[(&skin.map(|(joint, weight)| (to_head(joint), weight)), 1.0)]))
        .collect();
    // Cut square to the neck, a little above its foot.
    let neck = sub(ends[at::NECK][1], foot);
    let normal = unit_or(neck, [0.0, 1.0, 0.0]);
    let through = scale(neck, 0.12);
    let (mut points, mut skin, mut faces) = cut(
        &local,
        &skin,
        |index| {
            follows(&asset.skin[index], &group) > 0.2
                && dot(sub(local[index], through), normal) > 0.0
        },
        (through, normal),
    );
    (points, skin, faces) = subdivided(&points, &skin, &faces);
    let tris = triangles(&points, &faces);
    let place = |joint: usize| [sub(ends[joint][0], foot), sub(ends[joint][1], foot)];
    let skull = place(at::HEAD);
    let on_skull = |offset: [f32; 3], bone: [f32; 3]| {
        let from = add(skull[0], offset);
        [from, add(from, bone)]
    };
    let eye = |side: usize| {
        let [head, tail] = place(at::EYES[side]);
        [head, add(head, scale(sub(tail, head), 0.3))]
    };
    let nose = sub(place(at::EYES[0])[0], skull[0]);
    let all_ends = vec![
        place(at::NECK),
        skull,
        place(at::JAW),
        on_skull([0.0, nose[1] - 0.02, nose[2] + 0.015], [0.0, -0.025, 0.015]),
        on_skull([0.0, nose[1] + 0.02, nose[2]], [0.0, 0.0, 0.01]),
        eye(0),
        eye(1),
        on_skull([0.075, nose[1], 0.0], [0.004, -0.04, 0.0]),
        on_skull([-0.075, nose[1], 0.0], [-0.004, -0.04, 0.0]),
    ];
    let frames = vec![IDENTITY; all_ends.len()];
    let joints = rigged(&HEAD_JOINTS, &all_ends, &frames, &points, &skin);
    let mut guides = Vec::new();
    sliced(
        &points,
        &tris,
        [0.0004, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        |t| t.iter().all(|&v| follows(&skin[v as usize], &[5, 6]) < 0.5),
        &mut guides,
    );
    sliced(
        &points,
        &tris,
        all_ends[5][0],
        [0.0, 1.0, 0.0],
        |t| t.iter().all(|&v| follows(&skin[v as usize], &[1, 2]) > 0.9),
        &mut guides,
    );
    let bind = frames
        .iter()
        .zip(&all_ends)
        .map(|(f, e)| (*f, e[0]))
        .collect();
    Shaped {
        order: ordered(&joints),
        rest: points,
        skin,
        tris,
        joints,
        bind,
        guides,
        detail: Vec::new(),
        axes: Vec::new(),
        relief: None,
        hands: [IDENTITY; 2],
        thumb_axes: Vec::new(),
    }
}

/// `body` shaped, from the last few shaped when it was among them.
pub(crate) fn shaped(body: &Body) -> Arc<Shaped> {
    static SHAPED: Mutex<Vec<(Body, Arc<Shaped>)>> = Mutex::new(Vec::new());
    // What does not change the shape: where it stands, which hand it
    // is, and what is drawn on it.
    let key = Body {
        place: crate::three::Placement::default(),
        side: Side::Left,
        guides: true,
        follow: false,
        ..*body
    };
    let mut kept = SHAPED.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(at) = kept.iter().position(|(body, _)| *body == key) {
        let found = kept.remove(at);
        let shaped = Arc::clone(&found.1);
        kept.push(found);
        return shaped;
    }
    drop(kept);
    let made = Arc::new(match body.kind {
        Kind::Body => shaped_body(&key),
        Kind::Hand => shaped_hand(&key),
        Kind::Head => shaped_head(&key),
    });
    let mut kept = SHAPED.lock().unwrap_or_else(|poison| poison.into_inner());
    if kept.len() >= 8 {
        kept.remove(0);
    }
    kept.push((key, Arc::clone(&made)));
    made
}

/// Where every joint is and how it is turned for `pose`, in the figure's
/// own space (not yet placed), at the body's own size.
pub(crate) fn solved(pose: &Pose, shaped: &Shaped, size: f32) -> Vec<([f32; 3], Mat3)> {
    let mut placed = vec![([0.0_f32; 3], IDENTITY); shaped.joints.len()];
    for &index in &shaped.order {
        let joint = &shaped.joints[index];
        let turn = euler(pose.0.get(index).copied().unwrap_or([0.0; 3]));
        placed[index] = match joint.parent {
            Some(parent) => {
                let (from, frame) = placed[parent];
                (
                    add(from, apply(&frame, scale(joint.offset, size))),
                    mul(&frame, &turn),
                )
            }
            None => (scale(joint.offset, size), turn),
        };
    }
    placed
}

/// Whether `body` is drawn mirrored: a right hand is the left hand's
/// mirror image.
pub(crate) fn is_mirrored(body: &Body) -> bool {
    body.kind == Kind::Hand && body.side == Side::Right
}

/// `body` bent to `pose` and placed, as a surface to draw.
pub(crate) fn surface(pose: &Pose, body: &Body) -> Surface {
    let shaped = shaped(body);
    let size = body.place.scale;
    let placed = solved(pose, &shaped, size);
    // Each joint's turn and shift from where it was modeled.
    let moves: Vec<(Mat3, [f32; 3])> = placed
        .iter()
        .zip(&shaped.bind)
        .map(|((at, frame), (bound, from))| {
            let turn = mul(frame, &transpose(bound));
            (turn, sub(*at, apply(&turn, scale(*from, size))))
        })
        .collect();
    let stand = body.place.matrix();
    let flip = is_mirrored(body);
    let finish = |mut p: [f32; 3], place: bool| {
        if flip {
            p[0] = -p[0];
        }
        let turned = apply(&stand, p);
        if place {
            add(body.place.position, turned)
        } else {
            turned
        }
    };
    let positions: Vec<[f32; 3]> = shaped
        .rest
        .iter()
        .zip(&shaped.skin)
        .map(|(p, skin)| {
            let p = scale(*p, size);
            let mut out = [0.0_f32; 3];
            for (joint, weight) in skin {
                if *weight <= 0.0 {
                    continue;
                }
                let (turn, shift) = &moves[usize::from(*joint)];
                let moved = add(apply(turn, p), *shift);
                for axis in 0..3 {
                    out[axis] += moved[axis] * weight;
                }
            }
            finish(out, true)
        })
        .collect();
    let mut indices = shaped.tris.clone();
    if flip {
        for t in indices.as_chunks_mut::<3>().0 {
            t.swap(1, 2);
        }
    }
    let normals = crate::three::smooth_normals(&positions, &indices);
    let guides = if body.guides {
        shaped
            .guides
            .iter()
            .map(|ends| {
                ends.map(|(a, b, along)| {
                    let (a, b) = (positions[a as usize], positions[b as usize]);
                    add(a, scale(sub(b, a), along))
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let axes = shaped
        .axes
        .iter()
        .map(|(joint, pair)| pair.map(|axis| finish(apply(&moves[*joint].0, axis), false)))
        .collect();
    Surface {
        positions,
        normals,
        indices,
        guides,
        detail: shaped.detail.clone(),
        axes,
        relief: shaped.relief.clone(),
    }
}
