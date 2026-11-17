//! The Hand Scanner's maths: from the 21 points a camera sees of a hand
//! to the turns of a hand figure's joints (the contract "Hand joints" at
//! the top of [`crate::figure`]), and nothing else. No camera, no model
//! and no interface live here, so every step is tested with made-up
//! hands.
//!
//! The steps, in order:
//!
//! 1. [`to_model`] brings the camera's points into the hand figure's own
//!    space (palm toward +z, fingers up +y) and mirrors a right hand
//!    into a left one, since the hand figure is a left hand and a right
//!    hand takes the same numbers.
//! 2. [`Smoother`] steadies the points over time with a one euro filter
//!    per coordinate: still hands stop trembling, quick moves stay
//!    quick.
//! 3. [`solve`] reads the palm's turn from the wrist and the knuckles,
//!    then each finger's curl and spread and the thumb's turns from the
//!    directions of its bones, each kept inside the joint's limits.
//! 4. [`pose_hand`] and [`pose_body`] write the turns into a hand
//!    figure's pose, or into one hand of a body figure.
//!
//! [`points_of`] goes the other way (the points of a posed hand figure):
//! the tests are built on it, and so is anything that wants to show what
//! a pose looks like to a camera.
//!
//! The 21 points are in MediaPipe's order: 0 the wrist; 1 to 4 the thumb
//! (base, two knuckles, tip); then four points (knuckle, two joints,
//! tip) for the index (5), middle (9), ring (13) and little (17) fingers.

use crate::figure::{HAND, Joint, Pose};

/// How many points a hand has.
pub const POINTS: usize = 21;

/// The 21 points of a hand.
pub type Points = [[f32; 3]; POINTS];

/// The turns of a hand's joints, in degrees about x (curl), y (twist)
/// and z (spread), as the hand figure's pose holds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandAngles {
    /// The whole hand's turn (the hand figure's Palm joint).
    pub palm: [f32; 3],
    /// The fifteen finger joints, in the hand figure's order: index,
    /// middle, ring and little finger, then the thumb, each base, middle
    /// and tip.
    pub fingers: [[f32; 3]; 15],
}

/// Which hand of a body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// The body's left hand.
    #[default]
    Left,
    /// The body's right hand.
    Right,
}

impl Side {
    /// Both, in the order people see them.
    pub const ALL: [Self; 2] = [Self::Left, Self::Right];

    /// The word that starts the names of this side's joints.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

/// The limits of the fifteen finger joints (lowest and highest turn
/// about x, y and z), as the contract "Hand joints" gives them.
pub const LIMITS: [[[f32; 2]; 3]; 15] = {
    let base = [[-30.0, 95.0], [-15.0, 15.0], [-30.0, 30.0]];
    let middle = [[0.0, 115.0], [0.0, 0.0], [0.0, 0.0]];
    let tip = [[-10.0, 85.0], [0.0, 0.0], [0.0, 0.0]];
    [
        base,
        middle,
        tip,
        base,
        middle,
        tip,
        base,
        middle,
        tip,
        base,
        middle,
        tip,
        [[-45.0, 75.0], [-60.0, 60.0], [-80.0, 45.0]],
        [[-20.0, 80.0], [-20.0, 20.0], [-30.0, 30.0]],
        [[-30.0, 95.0], [-10.0, 10.0], [-15.0, 15.0]],
    ]
};

fn limited(joint: usize, turn: [f32; 3]) -> [f32; 3] {
    let limits = LIMITS[joint];
    [0, 1, 2].map(|axis| turn[axis].clamp(limits[axis][0], limits[axis][1]))
}

type Mat3 = [[f32; 3]; 3];

const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (column, value) in out_row.iter_mut().enumerate() {
            *value = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

fn transpose(m: &Mat3) -> Mat3 {
    [0, 1, 2].map(|row| [0, 1, 2].map(|column| m[column][row]))
}

fn apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|row| m[row][0] * v[0] + m[row][1] * v[1] + m[row][2] * v[2])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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

fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(v, v).sqrt();
    (length.is_finite() && length > 1e-9).then(|| v.map(|value| value / length))
}

/// The turn by `[x, y, z]` degrees: about z, then y, then x, as the
/// figure's joints turn.
fn euler([x, y, z]: [f32; 3]) -> Mat3 {
    let (sx, cx) = x.to_radians().sin_cos();
    let (sy, cy) = y.to_radians().sin_cos();
    let (sz, cz) = z.to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(&mul(&rz, &ry), &rx)
}

/// The degrees `[x, y, z]` whose [`euler`] turn is `m`.
fn degrees(m: &Mat3) -> [f32; 3] {
    let y = (-m[2][0]).clamp(-1.0, 1.0).asin();
    let (x, z) = if m[2][0].abs() < 0.9999 {
        (m[2][1].atan2(m[2][2]), m[1][0].atan2(m[0][0]))
    } else {
        // Straight up or down about y: x and z turn about the same line,
        // so all of it is given to x.
        ((-m[1][2]).atan2(m[1][1]), 0.0)
    };
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

/// The shortest turn that brings direction `from` onto `to` (both of
/// length one).
fn between(from: [f32; 3], to: [f32; 3]) -> Mat3 {
    let cosine = dot(from, to).clamp(-1.0, 1.0);
    let axis = match unit(cross(from, to)) {
        Some(axis) => axis,
        None if cosine > 0.0 => return IDENTITY,
        // Opposite directions: half a turn about any line across them.
        None => {
            let other = if from[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            unit(cross(from, other)).unwrap_or([0.0, 0.0, 1.0])
        }
    };
    let sine = (1.0 - cosine * cosine).sqrt();
    let [x, y, z] = axis;
    let t = 1.0 - cosine;
    [
        [
            t * x * x + cosine,
            t * x * y - sine * z,
            t * x * z + sine * y,
        ],
        [
            t * x * y + sine * z,
            t * y * y + cosine,
            t * y * z - sine * x,
        ],
        [
            t * x * z - sine * y,
            t * y * z + sine * x,
            t * z * z + cosine,
        ],
    ]
}

/// The first point of each finger, and the hand figure joint its first
/// bone belongs to: index, middle, ring, little.
const FINGERS: [(usize, usize); 4] = [(5, 1), (9, 4), (13, 7), (17, 10)];
/// The hand figure's first thumb joint.
const THUMB: usize = 13;

/// Where every joint of the hand figure is and how it is turned, for
/// the turns in `pose` (joints it lacks stay unturned).
fn placed(pose: &[[f32; 3]]) -> [([f32; 3], Mat3); 16] {
    let mut out = [([0.0; 3], IDENTITY); 16];
    for (at, joint) in HAND.iter().enumerate() {
        let turn = euler(pose.get(at).copied().unwrap_or([0.0; 3]));
        out[at] = match joint.parent {
            Some(parent) => {
                let (from, frame) = out[parent];
                (add(from, apply(&frame, joint.offset)), mul(&frame, &turn))
            }
            None => (joint.offset, turn),
        };
    }
    out
}

/// The 21 points of the hand figure posed as `pose` (a hand figure's
/// pose: sixteen joints), in the figure's own space and meters: what
/// [`solve`] reads back.
#[must_use]
pub fn points_of(pose: &Pose) -> Points {
    let placed = placed(&pose.0);
    let end = |at: usize| add(placed[at].0, apply(&placed[at].1, HAND[at].bone));
    let mut points = [[0.0; 3]; POINTS];
    points[0] = placed[0].0;
    for (first, joint) in FINGERS.into_iter().chain([(1, THUMB)]) {
        for step in 0..3 {
            points[first + step] = placed[joint + step].0;
        }
        points[first + 3] = end(joint + 2);
    }
    points
}

/// The palm's frame from its wrist and knuckles: x toward the little
/// finger, y toward the fingers, z out of the palm.
fn palm_frame(points: &Points) -> Option<Mat3> {
    let up = unit(sub(points[9], points[0]))?;
    let across = sub(points[17], points[5]);
    let out = unit(cross(across, up))?;
    let across = cross(up, out);
    // The frame's columns are its axes.
    Some([0, 1, 2].map(|row| [across[row], up[row], out[row]]))
}

/// The camera's points (`world`: x to the picture's right, y down, z
/// away from the camera, as a hand tracker gives them) in the hand
/// figure's space: x right, y up, z toward the camera, so a palm held to
/// the camera faces +z as the figure's does. A right hand is mirrored
/// into the left hand the figure is.
#[must_use]
pub fn to_model(world: &Points, right: bool) -> Points {
    let side = if right { -1.0 } else { 1.0 };
    world.map(|[x, y, z]| [x * side, -y, -z])
}

/// The turns that pose the hand figure like the hand at `points` (in
/// the figure's space, see [`to_model`]), every finger joint inside its
/// limits. `None` when the points do not span a hand (all in one place,
/// or not numbers).
#[must_use]
pub fn solve(points: &Points) -> Option<HandAngles> {
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return None;
    }
    // The palm's turn: from the figure's palm at rest to this one.
    let rest = palm_frame(&points_of(&Pose(vec![[0.0; 3]; HAND.len()])))?;
    let palm = mul(&palm_frame(points)?, &transpose(&rest));
    let back = transpose(&palm);
    // Every point as the unturned palm would hold it.
    let local = points.map(|point| apply(&back, sub(point, points[0])));
    let bone = |from: usize| unit(sub(local[from + 1], local[from]));
    let mut fingers = [[0.0; 3]; 15];

    for (first, joint) in FINGERS {
        let at = joint - 1;
        // The first bone: how far it leans out of the palm's plane is
        // the curl, where it points within the plane is the spread. A
        // finger curled right over has no spread left to read.
        let mut frame = IDENTITY;
        if let Some(d) = bone(first) {
            let flat = d[0].hypot(d[1]);
            let mut curl = d[2].atan2(flat).to_degrees();
            if d[1] < 0.0 && d[0].abs() < d[1].abs() {
                curl = if d[2] >= 0.0 {
                    180.0 - curl
                } else {
                    -180.0 - curl
                };
            }
            let spread = if d[1] > 0.0 {
                (-d[0]).atan2(d[1]).to_degrees() * (flat / 0.35).clamp(0.0, 1.0)
            } else {
                0.0
            };
            fingers[at] = limited(at, [curl, 0.0, spread]);
            frame = euler(fingers[at]);
        }
        // The next two bones only curl, each against the bone before.
        for step in 1..3 {
            if let Some(d) = bone(first + step) {
                let d = apply(&transpose(&frame), d);
                fingers[at + step] = limited(at + step, [d[2].atan2(d[1]).to_degrees(), 0.0, 0.0]);
            }
            frame = mul(&frame, &euler(fingers[at + step]));
        }
    }

    // The thumb's bones do not point up at rest, so each joint takes the
    // shortest turn from where its bone rests to where it points.
    let mut frame = IDENTITY;
    for step in 0..3 {
        let at = THUMB - 1 + step;
        if let (Some(d), Some(rest)) = (bone(1 + step), unit(HAND[THUMB + step].bone)) {
            let d = apply(&transpose(&frame), d);
            fingers[at] = limited(at, degrees(&between(rest, d)));
        }
        frame = mul(&frame, &euler(fingers[at]));
    }

    Some(HandAngles {
        palm: degrees(&palm),
        fingers,
    })
}

/// Writes `angles` into a hand figure's `pose` (sixteen joints): the
/// fingers always, the whole hand's turn when `turn_hand` is on.
pub fn pose_hand(pose: &mut Pose, angles: &HandAngles, turn_hand: bool) {
    pose.0.resize(HAND.len().max(pose.0.len()), [0.0; 3]);
    if turn_hand {
        pose.0[0] = angles.palm;
    }
    pose.0[1..=15].copy_from_slice(&angles.fingers);
}

/// The place in `joints` (a body figure's) of each finger joint of its
/// `side` hand, in the hand figure's order, found by name: `Index finger
/// base` of the hand figure is `Left index finger base` on a body.
/// `None` when the body has no finger joints.
#[must_use]
pub fn body_fingers(joints: &[Joint], side: Side) -> Option<[usize; 15]> {
    let mut out = [0; 15];
    for (slot, joint) in out.iter_mut().zip(&HAND[1..]) {
        let name = format!("{} {}", side.word(), joint.name.to_lowercase());
        *slot = joints
            .iter()
            .position(|found| found.name.eq_ignore_ascii_case(&name))?;
    }
    Some(out)
}

/// Writes the fingers of `angles` into one hand of a body figure's
/// `pose`, whose joints are `joints`. A right hand is the left one's
/// mirror image and takes the same numbers. `false` (and nothing
/// written) when the body has no finger joints.
pub fn pose_body(pose: &mut Pose, joints: &[Joint], side: Side, angles: &HandAngles) -> bool {
    let Some(places) = body_fingers(joints, side) else {
        return false;
    };
    for (place, turn) in places.into_iter().zip(angles.fingers) {
        if let Some(slot) = pose.0.get_mut(place) {
            *slot = turn;
        }
    }
    true
}

/// A one euro filter (Casiez, Roussel and Vogel, 2012): a low pass
/// whose cutoff rises with speed, so a still value is steady and a
/// moving one keeps up.
#[derive(Clone, Copy, Debug, Default)]
struct OneEuro {
    value: Option<f32>,
    speed: f32,
}

impl OneEuro {
    fn blend(cutoff: f32, seconds: f32) -> f32 {
        let tau = 1.0 / (std::f32::consts::TAU * cutoff);
        1.0 / (1.0 + tau / seconds)
    }

    fn step(&mut self, value: f32, seconds: f32, slowest: f32, quickening: f32) -> f32 {
        let Some(last) = self.value else {
            self.value = Some(value);
            return value;
        };
        let speed = (value - last) / seconds;
        self.speed += Self::blend(1.0, seconds) * (speed - self.speed);
        let cutoff = slowest + quickening * self.speed.abs();
        let next = last + Self::blend(cutoff, seconds) * (value - last);
        self.value = Some(next);
        next
    }
}

/// Steadies a hand's points over time, one one euro filter per
/// coordinate. Made for points in meters (a hand is about 0.2 across).
#[derive(Clone, Debug)]
pub struct Smoother {
    filters: [[OneEuro; 3]; POINTS],
    /// The cutoff of a still hand, in hertz: lower is steadier and
    /// slower to settle.
    pub slowest: f32,
    /// How much the cutoff rises per meter a second.
    pub quickening: f32,
}

impl Default for Smoother {
    fn default() -> Self {
        Self {
            filters: [[OneEuro::default(); 3]; POINTS],
            slowest: 1.6,
            quickening: 45.0,
        }
    }
}

impl Smoother {
    /// The steadied points for `points`, seen `seconds` after the ones
    /// before. Points that are not numbers are passed over, and so is a
    /// time that does not move forward.
    pub fn step(&mut self, points: &Points, seconds: f32) -> Points {
        if !(seconds.is_finite() && seconds > 0.0)
            || points.iter().flatten().any(|v| !v.is_finite())
        {
            return self.last().unwrap_or(*points);
        }
        let mut out = *points;
        for (point, filters) in out.iter_mut().zip(&mut self.filters) {
            for (value, filter) in point.iter_mut().zip(filters) {
                *value = filter.step(*value, seconds.min(0.5), self.slowest, self.quickening);
            }
        }
        out
    }

    /// The points it last gave, if any.
    fn last(&self) -> Option<Points> {
        self.filters[0][0].value?;
        Some(
            self.filters
                .map(|point| point.map(|filter| filter.value.unwrap_or(0.0))),
        )
    }

    /// Forgets the hand, so the next points are taken as they are (after
    /// a pause, or when the hand was lost).
    pub fn reset(&mut self) {
        self.filters = [[OneEuro::default(); 3]; POINTS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figure::hand_preset;

    fn rest() -> Pose {
        Pose(vec![[0.0; 3]; HAND.len()])
    }

    /// The pose [`solve`] reads from the points of `pose`.
    fn round_trip(pose: &Pose) -> Pose {
        let angles = solve(&points_of(pose)).expect("a hand");
        let mut out = rest();
        pose_hand(&mut out, &angles, true);
        out
    }

    /// The largest distance between the points `range` of two poses, in
    /// meters.
    fn apart(a: &Pose, b: &Pose, range: std::ops::Range<usize>) -> f32 {
        let (a, b) = (points_of(a), points_of(b));
        range
            .map(|at| dot(sub(a[at], b[at]), sub(a[at], b[at])).sqrt())
            .fold(0.0, f32::max)
    }

    fn close(a: [f32; 3], b: [f32; 3], within: f32) -> bool {
        (0..3).all(|axis| (a[axis] - b[axis]).abs() <= within)
    }

    /// The four fingers' joints of `pose` are those of `want`, to a
    /// degree, and their points land in the same places to a millimeter.
    /// The thumb reaches its points by the shortest turns, which are not
    /// always the ones a pose was made with, and inside its limits: its
    /// points land within half a centimeter.
    fn assert_same(pose: &Pose, want: &Pose) {
        for (at, joint) in HAND.iter().enumerate().take(13).skip(1) {
            assert!(
                close(pose.0[at], want.0[at], 1.0),
                "{}: {:?} is not {:?}",
                joint.name,
                pose.0[at],
                want.0[at]
            );
        }
        let fingers = apart(pose, want, 5..POINTS);
        assert!(fingers < 0.001, "finger points {fingers} m apart");
        let thumb = apart(pose, want, 0..5);
        assert!(thumb < 0.005, "thumb points {thumb} m apart");
    }

    #[test]
    fn an_open_hand_is_the_rest_pose() {
        let pose = round_trip(&rest());
        for (at, turn) in pose.0.iter().enumerate() {
            assert!(close(*turn, [0.0; 3], 0.01), "{}: {turn:?}", HAND[at].name);
        }
    }

    #[test]
    fn a_fist_curls_every_finger() {
        let fist = hand_preset(2);
        let pose = round_trip(&fist);
        assert_same(&pose, &fist);
        for first in [1, 4, 7, 10] {
            assert!(pose.0[first][0] > 80.0 && pose.0[first + 1][0] > 90.0);
        }
    }

    #[test]
    fn pointing_keeps_the_index_finger_straight() {
        let pointing = hand_preset(3);
        let pose = round_trip(&pointing);
        assert_same(&pose, &pointing);
        assert!(close(pose.0[1], [0.0; 3], 1.0) && close(pose.0[2], [0.0; 3], 1.0));
        assert!(pose.0[4][0] > 80.0 && pose.0[10][0] > 80.0);
    }

    #[test]
    fn a_peace_sign_spreads_two_straight_fingers() {
        let peace = hand_preset(4);
        let pose = round_trip(&peace);
        assert_same(&pose, &peace);
        assert!(
            (pose.0[1][2] - 9.0).abs() < 1.0,
            "the index finger's spread"
        );
        assert!(
            (pose.0[4][2] + 7.0).abs() < 1.0,
            "the middle finger's spread"
        );
        assert!(pose.0[1][0].abs() < 1.0 && pose.0[7][0] > 80.0);
    }

    #[test]
    fn a_pinch_brings_thumb_and_index_together() {
        let mut pinch = rest();
        pinch.0[1] = [48.0, 0.0, 4.0];
        pinch.0[2] = [52.0, 0.0, 0.0];
        pinch.0[3] = [30.0, 0.0, 0.0];
        pinch.0[13] = [38.0, 0.0, -28.0];
        pinch.0[14] = [22.0, 0.0, 0.0];
        pinch.0[15] = [26.0, 0.0, 0.0];
        let pose = round_trip(&pinch);
        assert_same(&pose, &pinch);
        // The other fingers stay open.
        for at in 4..=12 {
            assert!(close(pose.0[at], [0.0; 3], 1.0));
        }
        // And the thumb's tip is where the pinch put it, near the index
        // finger's.
        let (made, want) = (points_of(&pose), points_of(&pinch));
        let gap =
            |points: &Points| dot(sub(points[4], points[8]), sub(points[4], points[8])).sqrt();
        assert!((gap(&made) - gap(&want)).abs() < 0.005);
    }

    #[test]
    fn every_preset_survives_the_round_trip() {
        for index in 0..7 {
            let preset = hand_preset(index);
            assert_same(&round_trip(&preset), &preset);
        }
    }

    #[test]
    fn the_hands_turn_is_read_from_the_palm() {
        for turn in [[0.0, 35.0, 0.0], [20.0, -40.0, 15.0], [-30.0, 10.0, 70.0]] {
            let mut posed = hand_preset(6);
            posed.0[0] = turn;
            let pose = round_trip(&posed);
            assert!(
                close(pose.0[0], turn, 0.5),
                "{:?} is not {turn:?}",
                pose.0[0]
            );
            assert_same(&pose, &posed);
        }
    }

    #[test]
    fn where_and_how_large_the_hand_is_does_not_matter() {
        let posed = hand_preset(4);
        let moved =
            points_of(&posed).map(|[x, y, z]| [x * 1.7 + 0.3, y * 1.7 - 2.0, z * 1.7 + 5.0]);
        let angles = solve(&moved).expect("a hand");
        let mut pose = rest();
        pose_hand(&mut pose, &angles, true);
        assert_same(&pose, &posed);
    }

    #[test]
    fn a_right_hand_is_mirrored_into_the_same_numbers() {
        let posed = hand_preset(3);
        // What a camera sees of the figure's (left) hand, palm to it...
        let left = points_of(&posed).map(|[x, y, z]| [x, -y, -z]);
        // ...and of a right hand in the same pose: its mirror image.
        let right = left.map(|[x, y, z]| [-x, y, z]);
        let from_left = solve(&to_model(&left, false)).expect("a hand");
        let from_right = solve(&to_model(&right, true)).expect("a hand");
        assert_eq!(from_left, from_right);
        let mut pose = rest();
        pose_hand(&mut pose, &from_right, true);
        assert_same(&pose, &posed);
    }

    /// The points MediaPipe's model gave for a photo of a right hand
    /// held open to the camera, fingers spread and a little bent (the
    /// OpenCV Zoo's demo picture): meters, x right, y down, z away.
    const REAL_OPEN_RIGHT: Points = [
        [-0.031_380, 0.057_301, 0.028_195],
        [-0.001_359, 0.049_600, 0.017_886],
        [0.025_149, 0.039_174, 0.012_015],
        [0.053_742, 0.021_938, -0.001_985],
        [0.072_619, 0.013_273, -0.003_210],
        [0.014_306, 0.004_315, -0.004_516],
        [0.038_067, -0.015_608, -0.005_543],
        [0.052_163, -0.030_166, -0.002_452],
        [0.067_230, -0.048_202, -0.002_566],
        [0.004_257, -0.001_526, -0.000_942],
        [0.021_585, -0.021_180, 0.001_246],
        [0.034_185, -0.044_937, -0.000_171],
        [0.046_241, -0.058_997, -0.009_080],
        [-0.009_233, -0.003_767, 0.004_417],
        [-0.003_521, -0.028_533, 0.000_870],
        [0.004_384, -0.047_110, 0.001_339],
        [0.012_555, -0.066_745, 0.003_170],
        [-0.027_749, -0.002_816, 0.006_978],
        [-0.028_430, -0.024_867, 0.004_141],
        [-0.026_145, -0.040_928, 0.001_574],
        [-0.022_686, -0.051_237, -0.002_164],
    ];

    #[test]
    fn a_real_open_hand_reads_as_open_and_facing_the_camera() {
        let angles = solve(&to_model(&REAL_OPEN_RIGHT, true)).expect("a hand");
        println!("{angles:?}");
        // Every finger is nearly straight: no joint curled past a third
        // of a fist.
        for (at, turn) in angles.fingers[..12].iter().enumerate() {
            assert!(
                turn[0].abs() < 35.0,
                "{}: curled {}",
                HAND[at + 1].name,
                turn[0]
            );
        }
        // The fingers fan out: the index toward the thumb, the little
        // finger away from it.
        assert!(angles.fingers[0][2] > angles.fingers[9][2] + 10.0);
        // The palm faces the camera, give or take: turned less than a
        // quarter turn about every axis.
        let palm = euler(angles.palm);
        assert!(palm[2][2] > 0.5, "the palm faces away: {:?}", angles.palm);
        // The thumb stands out to its side, not across the palm.
        let mut pose = rest();
        pose_hand(&mut pose, &angles, false);
        let points = points_of(&pose);
        assert!(
            points[4][0] < points[2][0],
            "the thumb points across the palm"
        );
    }

    #[test]
    fn turns_stay_inside_the_joints_limits() {
        // A finger bent backward far past what a hand can do, and a
        // thumb folded right across.
        let mut broken = rest();
        broken.0[1] = [-80.0, 0.0, 60.0];
        broken.0[5] = [-60.0, 0.0, 0.0];
        broken.0[13] = [120.0, 0.0, -150.0];
        let angles = solve(&points_of(&broken)).expect("a hand");
        for (at, turn) in angles.fingers.iter().enumerate() {
            for axis in 0..3 {
                assert!(
                    (LIMITS[at][axis][0]..=LIMITS[at][axis][1]).contains(&turn[axis]),
                    "{} axis {axis}: {}",
                    HAND[at + 1].name,
                    turn[axis]
                );
            }
        }
        assert!((angles.fingers[0][0] + 30.0).abs() < 0.01);
        assert!(angles.fingers[4][0].abs() < 0.01);
    }

    #[test]
    fn points_that_span_no_hand_give_no_pose() {
        assert!(solve(&[[0.0; 3]; POINTS]).is_none());
        let mut points = points_of(&rest());
        points[7][1] = f32::NAN;
        assert!(solve(&points).is_none());
    }

    #[test]
    fn only_the_fingers_are_written_when_the_hand_keeps_its_turn() {
        let mut pose = rest();
        pose.0[0] = [10.0, 20.0, 30.0];
        let angles = solve(&points_of(&hand_preset(2))).expect("a hand");
        pose_hand(&mut pose, &angles, false);
        assert_eq!(pose.0[0], [10.0, 20.0, 30.0]);
        assert!(pose.0[1][0] > 80.0);
    }

    #[test]
    fn a_bodys_hand_takes_the_fingers_by_name() {
        // The body's own finger joints, named with their side.
        let joints: &[Joint] = &crate::figure::JOINTS;
        let angles = solve(&points_of(&hand_preset(2))).expect("a hand");
        let mut pose = Pose(vec![[0.0; 3]; joints.len()]);
        assert!(pose_body(&mut pose, joints, Side::Right, &angles));
        let right = body_fingers(joints, Side::Right).expect("right fingers");
        let left = body_fingers(joints, Side::Left).expect("left fingers");
        for (place, turn) in right.into_iter().zip(angles.fingers) {
            assert_eq!(pose.0[place], turn);
        }
        for place in left {
            assert_eq!(pose.0[place], [0.0; 3], "the left hand is left alone");
        }
        let touched = pose.0.iter().filter(|turn| **turn != [0.0; 3]).count();
        assert!(touched <= 15, "only the right hand's fingers turn");
        // A body without finger joints takes nothing.
        let plain_joints: Vec<Joint> = joints
            .iter()
            .filter(|joint| {
                !joint.name.to_lowercase().contains("finger")
                    && !joint.name.to_lowercase().contains("thumb")
            })
            .copied()
            .collect();
        let mut plain = Pose(vec![[0.0; 3]; plain_joints.len()]);
        assert!(!pose_body(&mut plain, &plain_joints, Side::Left, &angles));
        assert!(plain.0.iter().all(|turn| *turn == [0.0; 3]));
    }

    #[test]
    fn a_still_hand_stops_trembling_and_a_moving_one_keeps_up() {
        let still = points_of(&rest());
        let mut smoother = Smoother::default();
        let mut widest = 0.0_f32;
        for frame in 0..120 {
            // A millimeter of tremble, back and forth.
            let noise = if frame % 2 == 0 { 0.001 } else { -0.001 };
            let seen = still.map(|[x, y, z]| [x + noise, y - noise, z + noise]);
            let out = smoother.step(&seen, 1.0 / 30.0);
            if frame > 30 {
                widest = widest.max((out[8][0] - still[8][0]).abs());
            }
        }
        assert!(widest < 0.0004, "trembles by {widest} m");
        // The hand moves 10 cm in a third of a second: the filter is no
        // more than 2 cm behind at the end.
        let mut last = still;
        for frame in 1..=10 {
            let seen = still.map(|[x, y, z]| [x + 0.01 * frame as f32, y, z]);
            last = smoother.step(&seen, 1.0 / 30.0);
        }
        assert!(
            last[8][0] - still[8][0] > 0.08,
            "lags: {}",
            last[8][0] - still[8][0]
        );
    }

    #[test]
    fn the_smoother_passes_over_bad_input_and_starts_anew_after_a_reset() {
        let first = points_of(&rest());
        let mut smoother = Smoother::default();
        assert_eq!(smoother.step(&first, 0.033), first);
        let mut bad = first;
        bad[3][2] = f32::INFINITY;
        assert_eq!(smoother.step(&bad, 0.033), first);
        assert_eq!(smoother.step(&first, 0.0), first);
        let far = first.map(|[x, y, z]| [x + 1.0, y, z]);
        smoother.reset();
        assert_eq!(smoother.step(&far, 0.033), far);
    }
}
