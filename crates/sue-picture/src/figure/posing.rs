//! Posing a figure as people do it with the mouse: joint angle limits,
//! a limb that follows the hand or foot being dragged (inverse
//! kinematics), a part aimed at the pointer, the hand setup that opens,
//! closes and spreads the fingers, and hand poses carried between a hand
//! figure and a body's hands.

use super::human::{self, at};
use super::{
    Body, HAND, IDENTITY, Kind, Mat3, Pose, Side, add, apply, between, degrees, euler, mul,
    transpose, unit,
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
                if kept != *turn {
                    *turn = kept;
                } else {
                    continue;
                }
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

/// Hand pose `base` with its unlocked fingers set as `setup` says.
/// `body` is the hand figure (or the body whose hand it is), which
/// knows how the thumb really bends.
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
        pose.0[first] = [
            (fist * 86.0 + fold * 84.0) * more + relaxed * 0.4,
            0.0,
            wide[finger] * spread * (1.0 - closed * 0.85),
        ];
        pose.0[first + 1] = [fist * 102.0 * more + fold * 6.0 + relaxed, 0.0, 0.0];
        pose.0[first + 2] = [fist * 64.0 + fold * 3.0 + relaxed * 0.6, 0.0, 0.0];
    }
    if !setup.locks[0] {
        let axes = &human::shaped(body).thumb_axes;
        let axis = |part: usize| axes.get(part).copied().unwrap_or([1.0, 0.0, 0.0]);
        // The thumb swings out with the spread, and folds across the
        // fingers (a fist) or along the palm's edge (a flat fold).
        let out = about([0.0, 0.0, 1.0], spread.max(0.0) * 22.0 - spread.min(0.0) * -8.0);
        let base_turn = mul(&out, &about(axis(0), fist * 30.0 + fold * 18.0));
        pose.0[13] = degrees(&base_turn);
        pose.0[14] = degrees(&about(axis(1), fist * 38.0 + fold * 12.0));
        pose.0[15] = degrees(&about(axis(2), fist * 52.0 + fold * 10.0));
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
    let placed = human::solved(pose, &shaped, body.place.scale);
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

/// The two joints that carry `joint` to where it is dragged: the upper
/// arm and forearm for a hand or forearm, the thigh and shin for a foot
/// or shin. `None` for a part that turns by itself.
#[must_use]
pub fn limb_of(body: &Body, joint: usize) -> Option<[usize; 3]> {
    if body.kind != Kind::Body {
        return None;
    }
    at::ARMS
        .iter()
        .chain(at::LEGS.iter())
        .find(|limb| limb[1] == joint || limb[2] == joint)
        .copied()
}

/// `pose` with the limb `[upper, lower, end]` bent so that `end` (the
/// wrist or ankle) comes to `target` (a point in the world), or as near
/// as the limb reaches: the lower joint bends as a hinge, the upper one
/// turns the least that carries the end there.
#[must_use]
pub fn reached(
    pose: &Pose,
    body: &Body,
    [upper, lower, end]: [usize; 3],
    target: [f32; 3],
    keep: bool,
) -> Pose {
    let shaped = human::shaped(body);
    let size = body.place.scale;
    let mut out = pose.clone();
    if out.0.len() <= end {
        out.0.resize(end + 1, [0.0; 3]);
    }
    let placed = human::solved(&out, &shaped, size);
    let joints = &shaped.joints;
    let (first, second) = (
        joints[lower].offset.map(|value| value * size),
        joints[end].offset.map(|value| value * size),
    );
    let (l1, l2) = (long(first), long(second));
    let from = placed[upper].0;
    let to = minus(own_space(body, target), from);
    let far = long(to).clamp((l1 - l2).abs() + 1e-4, l1 + l2 - 1e-4);
    // The hinge: straight at the limb's full reach, folded as the end
    // comes in. Found by halving, since the two bones are not quite in
    // line at rest.
    let range = limits(body, lower)[0];
    let way = if range[0].abs() > range[1].abs() {
        -1.0
    } else {
        1.0
    };
    let twist = out.0[lower][1];
    let span = |bend: f32| long(add(first, apply(&euler([bend, twist, 0.0]), second)));
    let (mut open, mut shut) = (0.0_f32, 179.0_f32);
    for _ in 0..24 {
        let middle = (open + shut) / 2.0;
        if span(way * middle) > far {
            open = middle;
        } else {
            shut = middle;
        }
    }
    let mut bend = way * (open + shut) / 2.0;
    if keep {
        bend = bend.clamp(range[0], range[1]);
    }
    out.0[lower] = [bend, out.0[lower][1], 0.0];
    // Where the end now is in the upper joint's frame, turned to the
    // target.
    let reach = add(first, apply(&euler(out.0[lower]), second));
    let frame = placed[upper].1;
    let parent = joints[upper].parent.map_or(IDENTITY, |parent| placed[parent].1);
    if let (Some(now), Some(toward)) = (unit(apply(&frame, reach)), unit(to)) {
        let turned = mul(&between(now, toward), &frame);
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
    if body.kind != Kind::Body || pins.is_empty() {
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

/// A joint's turn with `by` degrees more about its own axis `axis` (0
/// x, 1 y, 2 z): what a ring of the rotation manipulator does.
#[must_use]
pub fn turned(turn: [f32; 3], axis: usize, by: f32) -> [f32; 3] {
    let mut step = [0.0; 3];
    step[axis.min(2)] = by;
    degrees(&mul(&euler(turn), &euler(step)))
}
