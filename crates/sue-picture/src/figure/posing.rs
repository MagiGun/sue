//! Posing a figure as people do it with the mouse: joint angle limits,
//! a limb that follows the hand or foot being dragged (inverse
//! kinematics), a part aimed at the pointer, the hand setup that opens,
//! closes and spreads the fingers, and hand poses carried between a hand
//! figure and a body's hands.

use super::human::{self, at};
use super::{
    Body, HAND, IDENTITY, Kind, Mat3, Pose, Side, apply, between, degrees, euler, mul, transpose,
    unit,
};

/// How far each of a joint's three turns may go, in degrees: least and
/// most about x, y and z.
pub type Limits = [[f32; 2]; 3];

const FREE: Limits = [[-180.0, 180.0]; 3];

/// A hand's limits, in [`HAND`]'s order (see the module's Hand joints
/// table).
const HAND_LIMITS: [Limits; 16] = {
    let base: Limits = [[-30.0, 95.0], [-15.0, 15.0], [-30.0, 30.0]];
    let middle: Limits = [[0.0, 115.0], [0.0, 0.0], [0.0, 0.0]];
    let tip: Limits = [[-10.0, 85.0], [0.0, 0.0], [0.0, 0.0]];
    [
        FREE,
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

/// A head's limits, in the head's joint order.
const HEAD_LIMITS: [Limits; 9] = [
    [[-50.0, 50.0], [-80.0, 80.0], [-40.0, 40.0]],
    [[-40.0, 40.0], [-50.0, 50.0], [-30.0, 30.0]],
    [[0.0, 32.0], [-6.0, 6.0], [-6.0, 6.0]],
    FREE,
    FREE,
    [[-30.0, 30.0], [-35.0, 35.0], [0.0, 0.0]],
    [[-30.0, 30.0], [-35.0, 35.0], [0.0, 0.0]],
    FREE,
    FREE,
];

/// The limits of a left side joint as the right side has them: twists
/// and swings the other way.
const fn other_side(limits: Limits) -> Limits {
    [
        limits[0],
        [-limits[1][1], -limits[1][0]],
        [-limits[2][1], -limits[2][0]],
    ]
}

/// How far joint `joint` of a figure like `body` may turn while Joint
/// angle limits is on. A body's finger joints are limited as a hand's
/// are, in the hand's own axes ([`limited`] sees to that), so this gives
/// them free.
#[must_use]
pub fn limits(body: &Body, joint: usize) -> Limits {
    match body.kind {
        Kind::Hand => HAND_LIMITS.get(joint).copied().unwrap_or(FREE),
        Kind::Head => HEAD_LIMITS.get(joint).copied().unwrap_or(FREE),
        Kind::Body => {
            let left: Option<Limits> = match joint {
                at::SPINE => Some([[-30.0, 50.0], [-35.0, 35.0], [-30.0, 30.0]]),
                at::CHEST => Some([[-25.0, 40.0], [-40.0, 40.0], [-25.0, 25.0]]),
                at::NECK => Some([[-45.0, 45.0], [-60.0, 60.0], [-35.0, 35.0]]),
                at::HEAD => Some([[-40.0, 40.0], [-50.0, 50.0], [-30.0, 30.0]]),
                5 | 8 => Some([[-185.0, 70.0], [-100.0, 100.0], [-35.0, 185.0]]),
                6 | 9 => Some([[-155.0, 0.0], [-95.0, 95.0], [0.0, 0.0]]),
                7 | 10 => Some([[-85.0, 85.0], [-25.0, 25.0], [-35.0, 35.0]]),
                11 | 14 => Some([[-130.0, 50.0], [-50.0, 50.0], [-30.0, 70.0]]),
                12 | 15 => Some([[0.0, 155.0], [-12.0, 12.0], [0.0, 0.0]]),
                13 | 16 => Some([[-50.0, 40.0], [-25.0, 25.0], [-25.0, 25.0]]),
                17 | 18 => Some([[-20.0, 20.0], [-20.0, 20.0], [-12.0, 40.0]]),
                19 | 20 => Some([[-45.0, 60.0], [0.0, 0.0], [0.0, 0.0]]),
                at::JAW => Some([[0.0, 32.0], [-6.0, 6.0], [-6.0, 6.0]]),
                52 | 53 => Some([[-30.0, 30.0], [-35.0, 35.0], [0.0, 0.0]]),
                _ => None,
            };
            let right = matches!(joint, 8 | 9 | 10 | 14 | 15 | 16 | 18 | 20);
            match left {
                Some(limits) if right => other_side(limits),
                Some(limits) => limits,
                None => FREE,
            }
        }
        // A file says nothing of how far its bones may turn: when its
        // model is asked to keep to limits, every bone has the same
        // wide ones.
        Kind::Imported if body.limits => [[-120.0, 120.0], [-90.0, 90.0], [-120.0, 120.0]],
        Kind::Imported => FREE,
    }
}

fn clamped(turn: [f32; 3], limits: &Limits) -> [f32; 3] {
    [0, 1, 2].map(|axis| {
        // An angle is the same a whole turn round.
        let mut value = turn[axis] % 360.0;
        if value > 180.0 {
            value -= 360.0;
        } else if value < -180.0 {
            value += 360.0;
        }
        if limits[axis][0] <= -180.0 && limits[axis][1] >= 180.0 {
            turn[axis]
        } else {
            value.clamp(limits[axis][0], limits[axis][1])
        }
    })
}

/// `pose` with every joint kept within its limits.
#[must_use]
pub fn limited(pose: &Pose, body: &Body) -> Pose {
    let mut out = pose.clone();
    for (joint, turn) in out.0.iter_mut().enumerate() {
        *turn = clamped(*turn, &limits(body, joint));
    }
    if body.kind == Kind::Body {
        // The fingers, as a hand's: in the hand's own axes.
        for side in [Side::Left, Side::Right] {
            if pose.0.len() < at::FINGERS[side as usize] + 15 {
                continue;
            }
            let mut hand = hand_from_body(body, &out, side);
            for (joint, turn) in hand.0.iter_mut().enumerate().skip(1) {
                let kept = clamped(*turn, &HAND_LIMITS[joint]);
                // Within its limits already (to the hair's breadth the
                // carrying turn loses): left exactly as it is.
                if (0..3).all(|axis| (kept[axis] - turn[axis]).abs() < 1e-3) {
                    continue;
                }
                *turn = kept;
                let one = one_onto_body(body, side, *turn);
                out.0[at::FINGERS[side as usize] + joint - 1] = one;
            }
        }
    }
    out
}

/// A hand joint's turn (in the hand's axes) as the same joint of a
/// body's `side` hand turns.
fn one_onto_body(body: &Body, side: Side, turn: [f32; 3]) -> [f32; 3] {
    let carry = human::shaped(body).hands[side as usize];
    let turn = match side {
        Side::Left => turn,
        Side::Right => [turn[0], -turn[1], -turn[2]],
    };
    degrees(&mul(&mul(&carry, &euler(turn)), &transpose(&carry)))
}

/// Lays hand pose `hand` (sixteen joints, see the Hand joints table)
/// onto the `side` hand of body pose `pose`: the fifteen finger joints
/// take the hand's turns; the palm's own turn is left out, since the
/// body's hand joint is posed with its arm.
pub fn hand_onto_body(body: &Body, pose: &mut Pose, side: Side, hand: &Pose) {
    let first = at::FINGERS[side as usize];
    if pose.0.len() < first + 15 {
        pose.0.resize(first + 15, [0.0; 3]);
    }
    for joint in 1..HAND.len() {
        let turn = hand.0.get(joint).copied().unwrap_or([0.0; 3]);
        pose.0[first + joint - 1] = one_onto_body(body, side, turn);
    }
}

/// The hand pose the `side` hand of body pose `pose` holds: what
/// [`hand_onto_body`] laid there (the palm unturned).
#[must_use]
pub fn hand_from_body(body: &Body, pose: &Pose, side: Side) -> Pose {
    let first = at::FINGERS[side as usize];
    let carry = human::shaped(body).hands[side as usize];
    let mut hand = Pose(vec![[0.0; 3]; HAND.len()]);
    for joint in 1..HAND.len() {
        let turn = pose.0.get(first + joint - 1).copied().unwrap_or([0.0; 3]);
        let own = degrees(&mul(&mul(&transpose(&carry), &euler(turn)), &carry));
        hand.0[joint] = match side {
            Side::Left => own,
            Side::Right => [own[0], -own[1], -own[2]],
        };
    }
    hand
}

/// The hand setup, as Clip Studio Paint's: how far the fingers are
/// closed and in which way, how far they are spread, and which fingers
/// are locked (kept as they are).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandSetup {
    /// How far the fingers close into a fist, 0 to 1.
    pub fist: f32,
    /// How far the fingers fold flat at the knuckles, 0 to 1 (with
    /// `fist`, at most 1 together).
    pub fold: f32,
    /// How far the fingers spread, -1 (together) to 1 (wide).
    pub spread: f32,
    /// The fingers kept as they are: thumb, index, middle, ring, little.
    pub locks: [bool; 5],
}

impl Default for HandSetup {
    fn default() -> Self {
        Self {
            fist: 0.0,
            fold: 0.0,
            spread: 0.0,
            locks: [false; 5],
        }
    }
}

/// A turn of `angle` degrees about `axis`.
fn about(axis: [f32; 3], angle: f32) -> Mat3 {
    let Some([x, y, z]) = unit(axis) else {
        return IDENTITY;
    };
    let (sin, cos) = angle.to_radians().sin_cos();
    let t = 1.0 - cos;
    [
        [t * x * x + cos, t * x * y - sin * z, t * x * z + sin * y],
        [t * x * y + sin * z, t * y * y + cos, t * y * z - sin * x],
        [t * x * z - sin * y, t * y * z + sin * x, t * z * z + cos],
    ]
}

/// Turn `m` done only `share` of the way (0 none, 1 all of it).
fn partly(m: &Mat3, share: f32) -> Mat3 {
    let mut q = super::skin::quat_of(m);
    if q[0] < 0.0 {
        q = q.map(|value| -value);
    }
    let angle = 2.0 * q[0].clamp(-1.0, 1.0).acos();
    about([q[1], q[2], q[3]], (angle * share).to_degrees())
}

/// Hand pose `base` with its unlocked fingers set as `setup` says.
/// `body` is the hand figure (or the body whose hand it is), which
/// knows how the thumb really bends.
#[allow(clippy::needless_range_loop)]
#[must_use]
pub fn hand_setup(body: &Body, base: &Pose, setup: &HandSetup) -> Pose {
    let mut pose = Pose(
        (0..HAND.len())
            .map(|joint| base.0.get(joint).copied().unwrap_or([0.0; 3]))
            .collect(),
    );
    let fist = setup.fist.clamp(0.0, 1.0);
    let fold = setup.fold.clamp(0.0, 1.0 - fist);
    let spread = setup.spread.clamp(-1.0, 1.0);
    let closed = (fist + fold).min(1.0);
    // Index, middle, ring and little: how each curls down its three
    // joints, and how it spreads.
    let wide = [15.0, 3.0, -9.0, -22.0];
    for finger in 0..4 {
        if setup.locks[finger + 1] {
            continue;
        }
        let first = 1 + finger * 3;
        // The little finger closes a touch more, as a fist does.
        let more = 1.0 + finger as f32 * 0.03;
        let relaxed = (1.0 - closed) * (6.0 + finger as f32 * 2.0) * (1.0 - spread.max(0.0));
        let curl = (fist * 86.0 + fold * 84.0) * more + relaxed * 0.4;
        pose.0[first] = [
            curl,
            gathered(finger, curl) * fist,
            wide[finger] * spread * (1.0 - closed * 0.85),
        ];
        pose.0[first + 1] = [fist * 102.0 * more + fold * 6.0 + relaxed, 0.0, 0.0];
        pose.0[first + 2] = [fist * 64.0 + fold * 3.0 + relaxed * 0.6, 0.0, 0.0];
    }
    if !setup.locks[0] {
        let axes = &human::shaped(body).thumb_axes;
        let axis = |part: usize| axes.get(part).copied().unwrap_or([1.0, 0.0, 0.0]);
        // The thumb swings out with the spread, and wraps the fingers
        // (a fist) or folds along the palm's edge (a flat fold).
        let out = about(
            [0.0, 0.0, 1.0],
            spread.max(0.0) * 22.0 - spread.min(0.0) * -8.0,
        );
        let wrap = thumb_wrap();
        let folds = [18.0, 12.0, 10.0];
        for part in 0..3 {
            let wrapped = partly(&euler(wrap[part]), fist);
            let mut turn = mul(&wrapped, &about(axis(part), fold * folds[part]));
            if part == 0 {
                turn = mul(&out, &turn);
            }
            pose.0[13 + part] = degrees(&turn);
        }
    }
    pose
}

/// A point of the world in the figure's own space: before it is placed
/// (and, for a right hand, mirrored).
fn own_space(body: &Body, p: [f32; 3]) -> [f32; 3] {
    let stand = body.place.matrix();
    let rel = [0, 1, 2].map(|axis| p[axis] - body.place.position[axis]);
    let mut local = apply(&transpose(&stand), rel);
    if human::is_mirrored(body) {
        local[0] = -local[0];
    }
    local
}

fn minus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn long(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// `pose` with joint `joint` turned the least that points its bone from
/// where the joint is toward `target` (a point in the world).
#[must_use]
pub fn aimed_at(pose: &Pose, body: &Body, joint: usize, target: [f32; 3], keep: bool) -> Pose {
    let shaped = human::shaped(body);
    let placed = human::posed(pose, &shaped, body);
    let mut out = pose.clone();
    let Some(table) = shaped.joints.get(joint) else {
        return out;
    };
    if out.0.len() <= joint {
        out.0.resize(joint + 1, [0.0; 3]);
    }
    let (from, frame) = placed[joint];
    let parent = table.parent.map_or(IDENTITY, |parent| placed[parent].1);
    let (Some(toward), Some(now)) = (
        unit(minus(own_space(body, target), from)),
        unit(apply(&frame, table.bone)),
    ) else {
        return out;
    };
    let turned = mul(&between(now, toward), &frame);
    out.0[joint] = degrees(&mul(&transpose(&parent), &turned));
    if keep {
        out = limited(&out, body);
    }
    out
}

/// The limb that carries `joint` to where it is dragged: the upper arm
/// and forearm for a hand or forearm, the thigh and shin for a foot or
/// shin (on an imported model, the two bones above a hand or foot).
/// `None` for a part that turns by itself.
#[must_use]
pub fn limb_of(body: &Body, joint: usize) -> Option<[usize; 3]> {
    human::shaped(body)
        .limbs
        .iter()
        .find(|limb| limb[1] == joint || limb[2] == joint)
        .copied()
}

fn crossed(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dotted(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `pose` with the limb `[upper, lower, end]` bent so that `end` (the
/// wrist or ankle) comes to `target` (a point in the world), or as near
/// as the limb reaches: the lower joint folds in the plane the limb is
/// already bent in (or about its hinge when it is straight), the upper
/// one turns the least that carries the end there.
#[must_use]
pub fn reached(
    pose: &Pose,
    body: &Body,
    [upper, lower, end]: [usize; 3],
    target: [f32; 3],
    keep: bool,
) -> Pose {
    let shaped = human::shaped(body);
    let mut out = pose.clone();
    let most = upper.max(lower).max(end);
    if most >= shaped.joints.len() {
        return out;
    }
    if out.0.len() <= most {
        out.0.resize(most + 1, [0.0; 3]);
    }
    let placed = human::posed(&out, &shaped, body);
    let (a, b, c) = (placed[upper].0, placed[lower].0, placed[end].0);
    let (first, second) = (minus(b, a), minus(c, b));
    let (l1, l2) = (long(first), long(second));
    if l1 < 1e-6 || l2 < 1e-6 {
        return out;
    }
    let to = minus(own_space(body, target), a);
    let far = long(to).clamp((l1 - l2).abs() + 1e-4, l1 + l2 - 1e-4);
    let angle = |cos: f32| cos.clamp(-1.0, 1.0).acos().to_degrees();
    // The angle inside the joint now, and the one that reaches.
    let now = angle(-dotted(first, second) / (l1 * l2));
    let want = angle((l1 * l1 + l2 * l2 - far * far) / (2.0 * l1 * l2));
    // The plane it folds in: the one it is bent in, or square to its
    // hinge when it is straight (a body's elbow and knee turn about
    // their own x; the knee the other way).
    let range = limits(body, lower)[0];
    let way = if range[0].abs() > range[1].abs() {
        -1.0
    } else {
        1.0
    };
    let hinge = apply(&placed[lower].1, [way, 0.0, 0.0]);
    let axis = unit(crossed(first, second))
        .filter(|_| now < 178.0)
        .unwrap_or(hinge);
    let fold = about(axis, now - want);
    let lower_frame = mul(&fold, &placed[lower].1);
    out.0[lower] = degrees(&mul(&transpose(&placed[upper].1), &lower_frame));
    if keep {
        out.0[lower] = clamped(out.0[lower], &limits(body, lower));
    }
    // Where the end now is, turned about the limb's top to the target.
    let placed = human::posed(&out, &shaped, body);
    let reach = minus(placed[end].0, a);
    let parent = shaped.joints[upper]
        .parent
        .map_or(IDENTITY, |parent| placed[parent].1);
    if let (Some(from), Some(toward)) = (unit(reach), unit(to)) {
        let turned = mul(&between(from, toward), &placed[upper].1);
        out.0[upper] = degrees(&mul(&transpose(&parent), &turned));
    }
    if keep {
        out = limited(&out, body);
    }
    out
}

/// `pose` with every limb whose end is in `pins` (hands and feet, by
/// joint) bent back to where that end was in `before`: what keeps a
/// pinned hand or foot in place while the rest of the body is posed.
#[must_use]
pub fn held(pose: &Pose, before: &Pose, body: &Body, pins: &[usize], keep: bool) -> Pose {
    if pins.is_empty() {
        return pose.clone();
    }
    let was = super::solve(before, body);
    let mut out = pose.clone();
    for &pin in pins {
        if let Some(limb) = limb_of(body, pin)
            && limb[2] == pin
            && let Some((target, _)) = was.get(pin)
        {
            out = reached(&out, body, limb, *target, keep);
        }
    }
    out
}

/// The body moved whole by `shift` (in the world), its held hands and
/// feet staying where they are: the pose its limbs take and where the
/// figure then stands. What holds are the `pins`, and the feet of a
/// body that keeps to the ground (so dragging its hips down bends its
/// knees, and it cannot be lifted off the floor; feet that are not
/// pinned go across with it). With nothing held the figure just moves.
#[must_use]
pub fn moved(
    pose: &Pose,
    body: &Body,
    shift: [f32; 3],
    pins: &[usize],
    keep: bool,
) -> (Pose, [f32; 3]) {
    let grounded = body.ground && body.kind == Kind::Body;
    let before = super::solve(pose, body);
    let mut holds: Vec<usize> = pins.to_vec();
    if grounded {
        for foot in [at::LEGS[0][2], at::LEGS[1][2]] {
            if !holds.contains(&foot) {
                holds.push(foot);
            }
        }
    }
    // Where the figure stands afterwards: a body that keeps to the
    // ground is not lifted or sunk, its legs take that up.
    let stood = body.place.position;
    let position = if grounded {
        [stood[0] + shift[0], stood[1], stood[2] + shift[2]]
    } else {
        [0, 1, 2].map(|axis| stood[axis] + shift[axis])
    };
    // The limbs reach for their holds on a copy that does not keep to
    // the ground, placed where the body is to end up.
    let loose = Body {
        ground: false,
        ..*body
    };
    let Some(root) = human::shaped(body).order.first().copied() else {
        return (pose.clone(), position);
    };
    let (Some(now), Some(free)) = (
        before.get(root).map(|placed| placed.0),
        super::solve(pose, &loose).get(root).map(|placed| placed.0),
    ) else {
        return (pose.clone(), position);
    };
    let mut loose = loose;
    loose.place.position = [0, 1, 2].map(|axis| stood[axis] + shift[axis] + now[axis] - free[axis]);
    let mut out = pose.clone();
    for &hold in &holds {
        if let Some(limb) = limb_of(&loose, hold)
            && limb[2] == hold
            && let Some((target, _)) = before.get(hold)
        {
            // A pinned hand or foot stays where it is; a foot only the
            // ground holds goes across with the body and stays down.
            let target = if pins.contains(&hold) {
                *target
            } else {
                [target[0] + shift[0], target[1], target[2] + shift[2]]
            };
            out = reached(&out, &loose, limb, target, keep);
            // The held hand or foot keeps facing the way it did (a
            // foot stays flat on the floor as its knee bends).
            let placed = super::solve(&out, &loose);
            if let (Some((_, above)), Some((_, was))) = (placed.get(limb[1]), before.get(hold)) {
                let turn = degrees(&mul(&transpose(above), was));
                out.0[hold] = if keep {
                    clamped(turn, &limits(body, hold))
                } else {
                    turn
                };
            }
        }
    }
    (out, position)
}

/// `pose` with the trunk leaned so the foot of the neck comes toward
/// `target` (a point in the world): the spine takes half the turn, the
/// chest the rest. On a figure without a trunk of its own (a hand, a
/// head, an imported model) `joint` is aimed there instead.
#[must_use]
pub fn leaned(pose: &Pose, body: &Body, joint: usize, target: [f32; 3], keep: bool) -> Pose {
    if body.kind != Kind::Body {
        return aimed_at(pose, body, joint, target, keep);
    }
    let shaped = human::shaped(body);
    let mut out = pose.clone();
    if out.0.len() <= at::NECK {
        out.0.resize(at::NECK + 1, [0.0; 3]);
    }
    let toward = own_space(body, target);
    for (joint, share) in [(at::SPINE, 0.5), (at::CHEST, 1.0)] {
        let placed = human::posed(&out, &shaped, body);
        let (from, frame) = placed[joint];
        let parent = shaped.joints[joint]
            .parent
            .map_or(IDENTITY, |parent| placed[parent].1);
        let (Some(now), Some(aim)) = (
            unit(minus(placed[at::NECK].0, from)),
            unit(minus(toward, from)),
        ) else {
            continue;
        };
        let turned = mul(&partly(&between(now, aim), share), &frame);
        out.0[joint] = degrees(&mul(&transpose(&parent), &turned));
    }
    if keep {
        out = limited(&out, body);
    }
    out
}

/// A joint's turn with `by` degrees more about its own axis `axis` (0
/// x, 1 y, 2 z): what a ring of the rotation manipulator does.
#[must_use]
pub fn turned(turn: [f32; 3], axis: usize, by: f32) -> [f32; 3] {
    let mut step = [0.0; 3];
    step[axis.min(2)] = by;
    degrees(&mul(&euler(turn), &euler(step)))
}

/// How one of Sue's hand poses holds the hand: each of the index,
/// middle, ring and little fingers' three curls and its spread, then
/// the thumb's three bends (about the axes it really bends about) and
/// how far it swings out.
type HandShape = ([[f32; 4]; 4], [f32; 4]);

const CURLED: [f32; 4] = [86.0, 100.0, 62.0, 0.0];
const STRAIGHT: [f32; 4] = [0.0; 4];
/// What a thumb's swing is set to when it wraps the closed fingers.
const WRAPPED: f32 = 999.0;
/// The thumb folded over the closed fingers, as in a fist.
const THUMB_IN: [f32; 4] = [0.0, 0.0, 0.0, WRAPPED];

/// The directions a thumb's three bones rest along (the stance its
/// turns are measured from), in the hand's own axes.
const THUMB_BONES: [[f32; 3]; 3] = [
    [-0.016, 0.03, 0.014],
    [-0.006, 0.026, 0.008],
    [-0.003, 0.021, 0.005],
];

/// Where a thumb's three bones point when it wraps a fist, in the
/// hand's own axes: out of the palm, then up and across the closed
/// fingers, its tip on the middle finger.
const THUMB_WRAP: [[f32; 3]; 3] = [[-0.5, 0.53, 0.68], [0.39, 0.79, 0.47], [0.96, 0.26, 0.13]];

/// The turns of a thumb's three joints that lay its bones along
/// `toward` (directions in the hand's own axes).
fn thumb_toward(toward: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut parent = IDENTITY;
    let mut out = [[0.0; 3]; 3];
    for part in 0..3 {
        let (Some(bone), Some(target)) = (unit(THUMB_BONES[part]), unit(toward[part])) else {
            continue;
        };
        let turn = between(bone, apply(&transpose(&parent), target));
        out[part] = degrees(&turn);
        parent = mul(&parent, &turn);
    }
    out
}

/// The thumb wrapped over the closed fingers.
fn thumb_wrap() -> [[f32; 3]; 3] {
    thumb_toward(&THUMB_WRAP)
}

/// How far a finger (0 the index to 3 the little) turns toward the
/// middle of the hand when its base is curled `curl` degrees: the
/// fingers of a fist lie together, though an open hand's are apart.
fn gathered(finger: usize, curl: f32) -> f32 {
    let most = [9.0, 2.0, -6.0, -13.0][finger.min(3)];
    most * ((curl - 30.0) / 50.0).clamp(0.0, 1.0)
}

/// Sue's hand poses, in [`super::HAND_PRESETS`]' order.
const HAND_SHAPES: [HandShape; 16] = [
    // Open hand.
    (
        [
            [0.0, 0.0, 0.0, 4.0],
            STRAIGHT,
            [0.0, 0.0, 0.0, -4.0],
            [0.0, 0.0, 0.0, -8.0],
        ],
        [0.0; 4],
    ),
    // Relaxed.
    (
        [
            [10.0, 18.0, 10.0, 3.0],
            [14.0, 22.0, 12.0, 0.0],
            [18.0, 26.0, 14.0, -3.0],
            [22.0, 30.0, 16.0, -7.0],
        ],
        [8.0, 10.0, 10.0, 0.0],
    ),
    // Fist.
    ([CURLED, CURLED, CURLED, [90.0, 102.0, 64.0, 0.0]], THUMB_IN),
    // Pointing.
    ([[0.0, 0.0, 0.0, 2.0], CURLED, CURLED, CURLED], THUMB_IN),
    // Peace sign.
    (
        [[0.0, 0.0, 0.0, 10.0], [0.0, 0.0, 0.0, -8.0], CURLED, CURLED],
        THUMB_IN,
    ),
    // Thumbs up.
    ([CURLED, CURLED, CURLED, CURLED], [-8.0, 0.0, 0.0, 26.0]),
    // Holding.
    (
        [
            [50.0, 48.0, 30.0, 3.0],
            [52.0, 48.0, 30.0, 0.0],
            [54.0, 50.0, 30.0, -3.0],
            [56.0, 52.0, 32.0, -6.0],
        ],
        [18.0, 18.0, 20.0, 8.0],
    ),
    // Spread.
    (
        [
            [0.0, 0.0, 0.0, 16.0],
            [0.0, 0.0, 0.0, 4.0],
            [0.0, 0.0, 0.0, -10.0],
            [0.0, 0.0, 0.0, -24.0],
        ],
        [-4.0, 0.0, 0.0, 30.0],
    ),
    // Flat fold.
    ([[80.0, 6.0, 2.0, 0.0]; 4], [16.0, 10.0, 10.0, 0.0]),
    // OK sign.
    (
        [
            [52.0, 62.0, 38.0, 4.0],
            [6.0, 8.0, 6.0, 0.0],
            [4.0, 6.0, 4.0, -6.0],
            [2.0, 4.0, 4.0, -12.0],
        ],
        [30.0, 30.0, 36.0, 0.0],
    ),
    // Pinch.
    (
        [
            [48.0, 50.0, 30.0, 2.0],
            [46.0, 52.0, 32.0, 0.0],
            [70.0, 80.0, 50.0, -2.0],
            [76.0, 84.0, 52.0, -4.0],
        ],
        [30.0, 28.0, 30.0, 0.0],
    ),
    // Claw.
    (
        [
            [8.0, 88.0, 66.0, 10.0],
            [8.0, 88.0, 66.0, 2.0],
            [8.0, 88.0, 66.0, -8.0],
            [8.0, 88.0, 66.0, -16.0],
        ],
        [10.0, 40.0, 60.0, 14.0],
    ),
    // Rock.
    (
        [
            [0.0, 0.0, 0.0, 10.0],
            CURLED,
            CURLED,
            [0.0, 0.0, 0.0, -14.0],
        ],
        THUMB_IN,
    ),
    // Three.
    (
        [
            [0.0, 0.0, 0.0, 10.0],
            STRAIGHT,
            [0.0, 0.0, 0.0, -10.0],
            CURLED,
        ],
        THUMB_IN,
    ),
    // Call me.
    (
        [CURLED, CURLED, CURLED, [0.0, 0.0, 0.0, -18.0]],
        [-6.0, 0.0, 0.0, 30.0],
    ),
    // Finger gun.
    (
        [[0.0, 0.0, 0.0, 2.0], CURLED, CURLED, CURLED],
        [-8.0, 0.0, 0.0, 24.0],
    ),
];

/// Hand pose `index` of Sue's own ([`super::HAND_PRESETS`]) for the
/// hand figure `body` (or the body whose hand it is): the thumb bends
/// about the axes that hand's thumb really bends about.
#[must_use]
pub fn hand_pose(body: &Body, index: usize) -> Pose {
    let mut pose = Pose(vec![[0.0; 3]; HAND.len()]);
    let Some((fingers, thumb)) = HAND_SHAPES.get(index) else {
        return pose;
    };
    for (finger, [base, middle, tip, spread]) in fingers.iter().enumerate() {
        let first = 1 + finger * 3;
        pose.0[first] = [*base, 0.0, *spread];
        pose.0[first + 1] = [*middle, 0.0, 0.0];
        pose.0[first + 2] = [*tip, 0.0, 0.0];
    }
    // A fist draws its fingers together as it closes.
    for finger in 0..4 {
        let first = 1 + finger * 3;
        pose.0[first][1] = gathered(finger, pose.0[first][0]);
    }
    if thumb[3] == WRAPPED {
        let wrap = thumb_wrap();
        pose.0[13..16].copy_from_slice(&wrap);
        return pose;
    }
    let axes = &human::shaped(body).thumb_axes;
    let axis = |part: usize| axes.get(part).copied().unwrap_or([1.0, 0.0, 0.0]);
    let out = about([0.0, 0.0, 1.0], thumb[3]);
    pose.0[13] = degrees(&mul(&out, &about(axis(0), thumb[0])));
    pose.0[14] = degrees(&about(axis(1), thumb[1]));
    pose.0[15] = degrees(&about(axis(2), thumb[2]));
    pose
}

/// Body pose `index` of Sue's own settled on `body`: the poses whose
/// hands rest on the body (hands on hips, arms crossed) reach for the
/// places on this body's own shape, whatever its build and height.
#[must_use]
pub(crate) fn settled(body: &Body, index: usize, pose: Pose) -> Pose {
    if body.kind != Kind::Body || !matches!(index, 8 | 9) {
        return pose;
    }
    let placed = super::solve(&pose, body);
    let stand = body.place.matrix();
    // The body's own measure: half the width across its shoulders.
    let half = long(minus(placed[5].0, placed[8].0)) / 2.0;
    let from = |joint: usize, [x, y, z]: [f32; 3]| {
        let by = apply(&stand, [x * half, y * half, z * half]);
        [0, 1, 2].map(|axis| placed[joint].0[axis] + by[axis])
    };
    let mut out = pose;
    match index {
        // Hands on hips: each wrist beside its hip, the elbow out.
        8 => {
            out = reached(&out, body, at::ARMS[0], from(11, [0.46, 0.55, 0.1]), false);
            out = reached(&out, body, at::ARMS[1], from(14, [-0.46, 0.55, 0.1]), false);
        }
        // Arms crossed: each wrist at the other elbow, one forearm
        // over the other.
        _ => {
            out = reached(&out, body, at::ARMS[0], from(2, [-0.2, 0.42, 0.8]), false);
            out = reached(&out, body, at::ARMS[1], from(2, [0.26, 0.3, 0.97]), false);
        }
    }
    out
}

/// `pose` (made on a figure with the joints `from`) as it lies on a
/// figure with the joints `to`: every joint takes the turn of the joint
/// of the same name (whatever its case); a joint with no namesake keeps
/// `base`'s turn. `None` when no name is shared.
#[must_use]
pub fn retargeted(
    pose: &Pose,
    from: &[super::Joint],
    to: &[super::Joint],
    base: &Pose,
) -> Option<Pose> {
    let mut out = Pose(
        (0..to.len())
            .map(|joint| base.0.get(joint).copied().unwrap_or([0.0; 3]))
            .collect(),
    );
    let mut shared = false;
    for (at, joint) in to.iter().enumerate() {
        if let Some(same) = from
            .iter()
            .position(|other| other.name.eq_ignore_ascii_case(joint.name))
            && let Some(turn) = pose.0.get(same)
        {
            out.0[at] = *turn;
            shared = true;
        }
    }
    shared.then_some(out)
}

/// The joint `joint` and every joint hanging from it.
#[must_use]
pub fn hanging(joints: &[super::Joint], joint: usize) -> Vec<usize> {
    let mut found = vec![joint];
    let mut at = 0;
    while at < found.len() {
        for (index, other) in joints.iter().enumerate() {
            if other.parent == Some(found[at]) && !found.contains(&index) {
                found.push(index);
            }
        }
        at += 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::{Build, bones, solve};
    use super::*;

    fn far(a: [f32; 3], b: [f32; 3]) -> f32 {
        long(minus(a, b))
    }

    #[test]
    fn a_dragged_hand_or_foot_takes_its_limb_along() {
        let body = Body::default();
        let base = body.standing();
        let before = solve(&base, &body);
        // The left wrist taken forward and up, within reach.
        let wrist = before[7].0;
        let target = [wrist[0] - 0.05, wrist[1] + 0.25, wrist[2] + 0.3];
        let limb = limb_of(&body, 7).unwrap();
        assert_eq!(limb, [5, 6, 7]);
        assert_eq!(limb_of(&body, 6), Some(limb), "the forearm drags it too");
        assert_eq!(limb_of(&body, 2), None);
        let posed = reached(&base, &body, limb, target, true);
        let after = solve(&posed, &body);
        assert!(far(after[7].0, target) < 0.02, "the wrist got there");
        // Only the arm's two joints turned, and the elbow bent as an
        // elbow does (about its own x, forward).
        for joint in 0..base.0.len() {
            if joint != 5 && joint != 6 {
                assert_eq!(posed.0[joint], base.0[joint], "{joint}");
            }
        }
        assert!(posed.0[6][0] < -20.0, "the elbow folds: {:?}", posed.0[6]);
        assert!(posed.0[6][2].abs() < 1e-3, "as a hinge");
        // The rest of the body stayed where it was.
        assert!(far(after[10].0, before[10].0) < 1e-5);
        // Out of reach, the arm straightens toward it.
        let away = [wrist[0] + 3.0, wrist[1] + 1.0, wrist[2]];
        let stretched = reached(&base, &body, limb, away, true);
        assert!(stretched.0[6][0] > -8.0, "{:?}", stretched.0[6]);
        // A foot: the knee bends the other way.
        let ankle = before[13].0;
        let step = [ankle[0], ankle[1] + 0.3, ankle[2] + 0.2];
        let stepped = reached(&base, &body, [11, 12, 13], step, true);
        assert!(stepped.0[12][0] > 20.0, "the knee folds back");
        let placed = solve(
            &stepped,
            &Body {
                ground: false,
                ..body
            },
        );
        assert!(far(placed[13].0, step) < 0.03);
    }

    #[test]
    fn limits_keep_joints_within_what_a_body_can_do() {
        let body = Body::default();
        let mut pose = body.standing();
        pose.0[6] = [40.0, 0.0, 30.0];
        pose.0[12] = [-60.0, 0.0, 0.0];
        pose.0[4] = [0.0, 120.0, 0.0];
        let kept = limited(&pose, &body);
        assert_eq!(kept.0[6], [0.0, 0.0, 0.0], "an elbow does not bend back");
        assert_eq!(kept.0[12][0], 0.0, "nor a knee forward");
        assert_eq!(kept.0[4][1], 50.0, "the head turns so far");
        assert_eq!(kept.0[5], pose.0[5], "what is within stays");
        // Left and right are limited as mirror images.
        let (left, right) = (limits(&body, 5), limits(&body, 8));
        assert_eq!(left[2], [-35.0, 185.0]);
        assert_eq!(right[2], [-185.0, 35.0]);
        // A hand's fingers: no bending backward past a little, and the
        // table in the module's contract holds.
        let hand = Body {
            kind: Kind::Hand,
            ..body
        };
        assert_eq!(limits(&hand, 2), [[0.0, 115.0], [0.0, 0.0], [0.0, 0.0]]);
        assert_eq!(limits(&hand, 13)[2], [-80.0, 45.0]);
        let mut bent = hand.preset(0);
        bent.0[5] = [-50.0, 20.0, 9.0];
        assert_eq!(limited(&bent, &hand).0[5], [0.0, 0.0, 0.0]);
        // Aiming a part at a point respects them when asked.
        let rest = body.standing();
        let elbow = solve(&rest, &body)[6].0;
        let behind = [elbow[0], elbow[1] + 0.2, elbow[2] - 0.5];
        let free = aimed_at(&rest, &body, 6, behind, false);
        let held_back = aimed_at(&rest, &body, 6, behind, true);
        assert!(free.0[6][0] > 10.0, "free, it bends back: {:?}", free.0[6]);
        assert!(held_back.0[6][0] <= 0.0);
        // A ring turn adds to the joint about its own axis.
        let turn = turned([0.0, 0.0, 0.0], 0, 30.0);
        assert!((turn[0] - 30.0).abs() < 1e-3 && turn[1].abs() < 1e-3);
    }

    #[test]
    fn hand_poses_lie_on_a_bodys_hands_and_come_back() {
        for build in [Build::Male, Build::Female] {
            let body = Body {
                build,
                ..Body::default()
            };
            let fist = hand_pose(&body, 2);
            let mut pose = Pose::rest();
            hand_onto_body(&body, &mut pose, Side::Left, &fist);
            hand_onto_body(&body, &mut pose, Side::Right, &fist);
            for side in [Side::Left, Side::Right] {
                let back = hand_from_body(&body, &pose, side);
                for joint in 1..HAND.len() {
                    for axis in 0..3 {
                        assert!(
                            (back.0[joint][axis] - fist.0[joint][axis]).abs() < 0.05,
                            "{side:?} {joint}: {:?} for {:?}",
                            back.0[joint],
                            fist.0[joint]
                        );
                    }
                }
            }
            // The fist closes both hands: each middle fingertip comes
            // back toward its wrist, the same on both sides.
            let open = bones(&Pose::rest(), &body);
            let closed = bones(&pose, &body);
            let reach = |placed: &[([f32; 3], [f32; 3])], wrist: usize, tip: usize| {
                far(placed[wrist].0, placed[tip].1)
            };
            let (left, right) = (reach(&closed, 7, 26), reach(&closed, 10, 41));
            assert!(left < reach(&open, 7, 26) * 0.6, "the left hand closed");
            assert!((left - right).abs() < 0.002, "both alike: {left} {right}");
        }
        // The hand setup: the pad's fist corner closes the unlocked
        // fingers and leaves a locked one as it was.
        let hand = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        let open = hand.preset(0);
        let setup = HandSetup {
            fist: 1.0,
            locks: [false, true, false, false, false],
            ..HandSetup::default()
        };
        let set = hand_setup(&hand, &open, &setup);
        assert_eq!(set.0[1], open.0[1], "the locked index stays");
        assert!(set.0[4][0] > 80.0 && set.0[5][0] > 95.0, "the middle curls");
        assert_ne!(set.0[13], open.0[13], "the thumb folds in");
        let spread = hand_setup(
            &hand,
            &open,
            &HandSetup {
                spread: 1.0,
                ..HandSetup::default()
            },
        );
        assert!(spread.0[1][2] > 10.0 && spread.0[10][2] < -15.0);
    }

    #[test]
    fn the_whole_body_moves_and_leans_with_its_holds_staying_put() {
        // Off the ground: the hips go where they are taken, pinned
        // hands and feet stay, the rest of the body comes along.
        let body = Body {
            ground: false,
            ..Body::default()
        };
        let mut base = body.standing();
        base.0[6] = [-70.0, 0.0, 0.0];
        base.0[9] = [-70.0, 0.0, 0.0];
        let before = solve(&base, &body);
        let shift = [0.06, -0.12, 0.05];
        let pins = [7, 13, 16];
        let (pose, position) = moved(&base, &body, shift, &pins, true);
        let placed = Body {
            place: crate::three::Placement {
                position,
                ..body.place
            },
            ..body
        };
        let after = solve(&pose, &placed);
        for axis in 0..3 {
            assert!((after[0].0[axis] - before[0].0[axis] - shift[axis]).abs() < 1e-4);
        }
        for pin in pins {
            assert!(
                far(after[pin].0, before[pin].0) < 0.02,
                "{pin} stays: {}",
                far(after[pin].0, before[pin].0)
            );
        }
        assert!(far(after[10].0, before[10].0) > 0.1, "the free hand moves");
        assert!(pose.0[12][0] > 20.0, "the knees bend: {:?}", pose.0[12]);
        // Nothing held: the figure just moves, its pose untouched.
        let (same, there) = moved(&base, &body, shift, &[], true);
        assert_eq!(same, base);
        assert_eq!(there, shift);
        // A body that keeps to the ground: its feet hold by themselves,
        // lowering the hips bends the knees and the feet stay on the
        // floor; it is not sunk into it.
        let grounded = Body::default();
        let rest = grounded.standing();
        let stood = solve(&rest, &grounded);
        let (crouched, position) = moved(&rest, &grounded, [0.05, -0.2, 0.0], &[], true);
        assert_eq!(position, [0.05, 0.0, 0.0]);
        let low = solve(
            &crouched,
            &Body {
                place: crate::three::Placement {
                    position,
                    ..grounded.place
                },
                ..grounded
            },
        );
        assert!(
            (low[0].0[1] - (stood[0].0[1] - 0.2)).abs() < 0.03,
            "the hips came down: {} from {}",
            low[0].0[1],
            stood[0].0[1]
        );
        for foot in [13, 16] {
            let went = [stood[foot].0[0] + 0.05, stood[foot].0[1], stood[foot].0[2]];
            assert!(far(low[foot].0, went) < 0.03, "foot {foot} stays down");
        }
        // Leaning: the trunk bends toward the pointer, the hips and
        // legs stay, a pinned hand is brought back by its arm.
        let neck = stood[3].0;
        let target = [neck[0] + 0.25, neck[1] - 0.05, neck[2] + 0.1];
        let lean = leaned(&rest, &grounded, 2, target, true);
        assert!(lean.0[1] != rest.0[1] && lean.0[2] != rest.0[2]);
        assert_eq!(lean.0[0], rest.0[0]);
        let bent = solve(&lean, &grounded);
        assert!(far(bent[3].0, target) < far(neck, target) * 0.5);
        assert!(far(bent[13].0, stood[13].0) < 1e-4);
        let pinned = held(&lean, &rest, &grounded, &[7], true);
        let kept = solve(&pinned, &grounded);
        assert!(far(kept[7].0, stood[7].0) < 0.03, "the pinned hand stays");
        // Dragging a pinned limb's own root (the upper arm) keeps the
        // pin: the elbow swings, the wrist does not move.
        let elbow = stood[6].0;
        let out = [elbow[0] + 0.2, elbow[1] + 0.05, elbow[2] - 0.1];
        let swung = aimed_at(&base, &body, 5, out, true);
        let swung = held(&swung, &base, &body, &[7], true);
        let now = solve(&swung, &body);
        assert!(far(now[7].0, before[7].0) < 0.03, "the wrist is held");
        assert!(far(now[6].0, before[6].0) > 0.02, "the elbow swung");
    }

    #[test]
    fn pins_hold_and_poses_move_between_models_by_joint_name() {
        let body = Body::default();
        let mut base = body.standing();
        base.0[6] = [-60.0, 0.0, 0.0];
        let before = solve(&base, &body);
        // Turning the chest with the left hand pinned: the hand stays.
        let mut leaned = base.clone();
        leaned.0[2] = [0.0, 20.0, 0.0];
        let loose = solve(&leaned, &body);
        assert!(far(loose[7].0, before[7].0) > 0.05, "unpinned, it moves");
        let pinned = held(&leaned, &base, &body, &[7], true);
        let after = solve(&pinned, &body);
        assert!(
            far(after[7].0, before[7].0) < 0.03,
            "pinned, it stays: {} {:?} {:?}",
            far(after[7].0, before[7].0),
            pinned.0[5],
            pinned.0[6]
        );
        assert_eq!(pinned.0[2], leaned.0[2]);
        // A body pose on a model with a few joints of the same names.
        let joints = body.joints();
        let few = [joints[5], joints[6]];
        let onto = retargeted(&base, &joints, &few, &Pose(vec![[1.0; 3]; 2])).unwrap();
        assert_eq!(onto.0, vec![base.0[5], base.0[6]]);
        let mut other = joints[5];
        other.name = "Wing";
        assert!(retargeted(&base, &joints, &[other], &Pose::rest()).is_none());
        // Everything hanging from the left upper arm.
        let hanging = hanging(&joints, 5);
        assert!(hanging.contains(&6) && hanging.contains(&7) && hanging.contains(&21));
        assert!(!hanging.contains(&8));
    }
}
