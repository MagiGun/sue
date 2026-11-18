//! Bending a figure's skin to a pose so its joints keep their volume.
//!
//! Each point follows up to four joints. Blending the joints' moves as
//! matrices (linear blend skinning) pulls the skin toward the joint's
//! middle as it bends, so elbows and knees pinch and a twisted wrist
//! turns into a candy wrapper. Here the moves are blended as dual
//! quaternions (Kavan and others, "Geometric Skinning with Approximate
//! Dual Quaternion Blending", 2008; written for Sue from the paper),
//! which keeps every blend a rigid move, and a limb's twist is spread
//! along its length: the upper arm's skin stays with the shoulder at
//! its top and turns with the elbow at its foot, the forearm's turns
//! with the wrist, as the bones under real skin do.

use super::human::Shaped;
use super::{Mat3, Pose, euler};

/// A part whose twist is spread along its length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Twist {
    /// The joint that carries the part.
    pub joint: usize,
    /// The joint at its far end, when that one's twist is shared with
    /// this part (the hand's with the forearm, the foot's with the shin).
    pub child: Option<usize>,
    /// How much of the child's twist the far end of this part takes.
    pub share: f32,
}

/// A turn as a unit quaternion: w, then x, y, z.
pub(crate) type Quat = [f32; 4];

pub(crate) fn quat_of(m: &Mat3) -> Quat {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).max(0.0).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).max(0.0).sqrt() * 2.0;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).max(0.0).sqrt() * 2.0;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        ]
    };
    let length = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if length > 1e-9 {
        q.map(|value| value / length)
    } else {
        [1.0, 0.0, 0.0, 0.0]
    }
}

pub(crate) fn quat_mul(a: Quat, b: Quat) -> Quat {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

pub(crate) fn quat_turn(q: Quat, v: [f32; 3]) -> [f32; 3] {
    // v + 2w(u x v) + 2 u x (u x v)
    let u = [q[1], q[2], q[3]];
    let c = cross(u, v);
    let cc = cross(u, c);
    [0, 1, 2].map(|axis| v[axis] + 2.0 * (q[0] * c[axis] + cc[axis]))
}

/// The turn `share` of the way from `a` to `b` (0 is `a`, 1 is `b`).
pub(crate) fn quat_between(a: Quat, b: Quat, share: f32) -> Quat {
    let mut cos = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if cos < 0.0 {
        cos = -cos;
        b = b.map(|value| -value);
    }
    let angle = cos.clamp(-1.0, 1.0).acos();
    let sin = angle.sin();
    let (from, to) = if sin < 1e-4 {
        (1.0 - share, share)
    } else {
        (
            ((1.0 - share) * angle).sin() / sin,
            (share * angle).sin() / sin,
        )
    };
    let q = [0, 1, 2, 3].map(|k| a[k] * from + b[k] * to);
    let length = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|value| value / length.max(1e-9))
}

/// The turn by `angle` radians about unit `axis`.
pub(crate) fn quat_about(axis: [f32; 3], angle: f32) -> Quat {
    let (sin, cos) = (angle * 0.5).sin_cos();
    [cos, axis[0] * sin, axis[1] * sin, axis[2] * sin]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// How far the point of a folded hinge stands out, in the limb's own
/// thickness there, when it is folded right up.
const BULGE: f32 = 0.5;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length > 1e-9 {
        v.map(|value| value / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// How far turn `q` twists about unit `axis`, in radians (-pi to pi),
/// and how far it swings the axis away, in radians.
pub(crate) fn twist_of(q: Quat, axis: [f32; 3]) -> (f32, f32) {
    let along = dot([q[1], q[2], q[3]], axis);
    let mut twist = 2.0 * along.atan2(q[0]);
    if twist > std::f32::consts::PI {
        twist -= std::f32::consts::TAU;
    } else if twist < -std::f32::consts::PI {
        twist += std::f32::consts::TAU;
    }
    let swung = quat_turn(q, axis);
    (twist, dot(swung, axis).clamp(-1.0, 1.0).acos())
}

/// Where each point is along each of the twisting parts it follows (0
/// at the joint, 1 at the bone's end; 0 for the other joints).
pub(crate) fn along(
    points: &[[f32; 3]],
    skin: &[[(u8, f32); 4]],
    ends: &[[[f32; 3]; 2]],
    twists: &[Twist],
) -> Vec<[f32; 4]> {
    if twists.is_empty() {
        return Vec::new();
    }
    points
        .iter()
        .zip(skin)
        .map(|(p, skin)| {
            skin.map(|(joint, weight)| {
                let joint = usize::from(joint);
                if weight <= 0.0 || !twists.iter().any(|twist| twist.joint == joint) {
                    return 0.0;
                }
                let [head, tail] = ends[joint];
                let bone = [0, 1, 2].map(|axis| tail[axis] - head[axis]);
                let from = [0, 1, 2].map(|axis| p[axis] - head[axis]);
                (dot(from, bone) / dot(bone, bone).max(1e-12)).clamp(0.0, 1.0)
            })
        })
        .collect()
}

/// One joint's move for a pose, ready to blend.
#[derive(Clone, Copy)]
struct Move {
    turn: Quat,
    shift: [f32; 3],
    /// For a twisting part: the bone's direction and the joint's place
    /// as modeled, where the joint is now, and the twist at its two
    /// ends (radians).
    twist: Option<([f32; 3], [f32; 3], [f32; 3], [f32; 2])>,
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// For tests and the pictures to compare by: bend as the figures did
// before (a plain blend, no twist spread along the limbs).
#[cfg(test)]
thread_local! {
    pub(crate) static PLAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn is_plain() -> bool {
    #[cfg(test)]
    {
        PLAIN.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// The points of `shaped` bent to `pose`: `placed` is where each joint
/// is and how it is turned ([`super::human::posed`]), `size` the
/// figure's own scale.
pub(crate) fn skinned(
    shaped: &Shaped,
    pose: &Pose,
    placed: &[([f32; 3], Mat3)],
    size: f32,
) -> Vec<[f32; 3]> {
    let mut moves: Vec<Move> = placed
        .iter()
        .zip(&shaped.bind)
        .map(|((at, frame), (bound, from))| {
            let turn = super::mul(frame, &super::transpose(bound));
            let from = from.map(|value| value * size);
            let moved = super::apply(&turn, from);
            Move {
                turn: quat_of(&turn),
                shift: [0, 1, 2].map(|axis| at[axis] - moved[axis]),
                twist: None,
            }
        })
        .collect();
    let local = |joint: usize| quat_of(&euler(pose.0.get(joint).copied().unwrap_or([0.0; 3])));
    let plain_only = is_plain();
    for twist in shaped.twists.iter().filter(|_| !plain_only) {
        let Some(joint) = shaped.joints.get(twist.joint) else {
            continue;
        };
        let bone = unit(joint.bone);
        // Its own twist, faded out as the part swings right round (where
        // twist and swing can no longer be told apart).
        let (own, swing) = twist_of(local(twist.joint), bone);
        let own = own * (1.0 - smoothstep(2.1, 2.9, swing));
        let far = twist.child.map_or(0.0, |child| {
            let (theirs, swing) = twist_of(local(child), bone);
            theirs * twist.share * (1.0 - smoothstep(2.1, 2.9, swing))
        });
        let (bound, from) = &shaped.bind[twist.joint];
        moves[twist.joint].twist = Some((
            unit(super::apply(bound, bone)),
            from.map(|value| value * size),
            placed[twist.joint].0,
            [-own, far],
        ));
    }
    // The helpers: each turned part of the way from its base to the
    // joint it follows (as that joint swings, not as it twists), about
    // the place where that joint starts.
    for (number, helper) in shaped.helpers.iter().enumerate() {
        let (Some(base), Some(follow), Some((_, from))) = (
            moves.get(helper.base).copied(),
            moves.get(helper.follow).copied(),
            shaped.bind.get(placed.len() + number),
        ) else {
            break;
        };
        let mut swung = follow.turn;
        if let Some((axis, _, _, [near, _])) = follow.twist {
            swung = quat_mul(swung, quat_about(axis, near));
        }
        let share = if plain_only { 0.0 } else { helper.share };
        let turn = quat_between(base.turn, swung, share);
        let moved = quat_turn(turn, from.map(|value| value * size));
        let at = placed[helper.follow].0;
        moves.push(Move {
            turn,
            shift: [0, 1, 2].map(|axis| at[axis] - moved[axis]),
            twist: None,
        });
    }
    // The hinges that are bent: where each is, the way out of its bend
    // (toward the point of the elbow, the cap of the knee) and how far
    // the flesh there is pushed out.
    let bulges: Vec<(usize, usize, [f32; 3], [f32; 3], f32, f32)> = shaped
        .hinges
        .iter()
        .filter(|_| !plain_only)
        .filter_map(|&hinge| {
            let joint = shaped.joints.get(hinge)?;
            let parent = joint.parent?;
            let (at, frame) = placed.get(hinge)?;
            let up = sub3(placed.get(parent)?.0, *at);
            let down = super::apply(frame, joint.bone);
            let reach = dot(down, down).sqrt();
            let (up, down) = (unit(up), unit(down));
            // The angle the hinge is folded by: none when straight.
            let fold = (-dot(up, down)).clamp(-1.0, 1.0).acos();
            let inside = [0, 1, 2].map(|axis| up[axis] + down[axis]);
            if fold < 0.2 || dot(inside, inside) < 1e-6 {
                return None;
            }
            let out = unit(inside).map(|value| -value);
            let thick = joint.start[0] * size;
            let push = BULGE * thick * (1.0 - (fold * 0.5).cos());
            Some((hinge, parent, *at, out, push, reach.max(thick)))
        })
        .collect();
    shaped
        .rest
        .iter()
        .zip(&shaped.skin)
        .enumerate()
        .map(|(index, (p, skin))| {
            let p = p.map(|value| value * size);
            let mut real = [0.0_f32; 4];
            let mut dual = [0.0_f32; 4];
            let mut plain = [0.0_f32; 3];
            let mut pivot: Option<Quat> = None;
            let mut total = 0.0;
            let mut share = 0.0;
            for (slot, (joint, weight)) in skin.iter().enumerate() {
                if *weight <= 0.0 {
                    continue;
                }
                let Some(mv) = moves.get(usize::from(*joint)) else {
                    continue;
                };
                let (mut turn, mut shift) = (mv.turn, mv.shift);
                if let Some((axis, from, at, [near, far])) = mv.twist {
                    let t = shaped.along.get(index).map_or(0.0, |along| along[slot]);
                    let angle = near * (1.0 - t) + far * t;
                    if angle.abs() > 1e-5 {
                        turn = quat_mul(turn, quat_about(axis, angle));
                        let moved = quat_turn(turn, from);
                        shift = [0, 1, 2].map(|axis| at[axis] - moved[axis]);
                    }
                }
                let moved = quat_turn(turn, p);
                for axis in 0..3 {
                    plain[axis] += (moved[axis] + shift[axis]) * weight;
                }
                // The nearer of the two quaternions that are this turn.
                let sign = match pivot {
                    Some(first)
                        if first[0] * turn[0]
                            + first[1] * turn[1]
                            + first[2] * turn[2]
                            + first[3] * turn[3]
                            < 0.0 =>
                    {
                        -1.0
                    }
                    Some(_) => 1.0,
                    None => {
                        pivot = Some(turn);
                        1.0
                    }
                };
                let half = quat_mul([0.0, shift[0], shift[1], shift[2]], turn);
                for k in 0..4 {
                    real[k] += turn[k] * weight * sign;
                    dual[k] += half[k] * 0.5 * weight * sign;
                }
                total += weight;
                share += weight * shaped.dual.get(usize::from(*joint)).copied().unwrap_or(1.0);
            }
            if total <= 0.0 {
                return p;
            }
            let plain = plain.map(|value| value / total);
            let norm =
                (real[0] * real[0] + real[1] * real[1] + real[2] * real[2] + real[3] * real[3])
                    .sqrt();
            if norm < 1e-6 || plain_only {
                return plain;
            }
            let real = real.map(|value| value / norm);
            let dual = dual.map(|value| value / norm);
            let turned = quat_turn(real, p);
            // The shift of the blend: 2 * dual * conjugate(real).
            let u = [real[1], real[2], real[3]];
            let d = [dual[1], dual[2], dual[3]];
            let c = cross(u, d);
            let out: [f32; 3] = [0, 1, 2]
                .map(|axis| turned[axis] + 2.0 * (real[0] * d[axis] - dual[0] * u[axis] + c[axis]));
            let mut out: [f32; 3] =
                [0, 1, 2].map(|axis| plain[axis] + (out[axis] - plain[axis]) * share / total);
            for (hinge, parent, at, way, push, reach) in &bulges {
                let weight = |joint: usize| {
                    skin.iter()
                        .filter(|(j, _)| usize::from(*j) == joint)
                        .map(|(_, w)| *w)
                        .sum::<f32>()
                };
                let (below, above) = (weight(*hinge), weight(*parent));
                if below + above < 0.5 {
                    continue;
                }
                // Only the skin on the outside of the bend, near it.
                let from = sub3(out, *at);
                let far = dot(from, from).sqrt();
                let facing = dot(from, *way) / far.max(1e-6);
                let near = 1.0 - smoothstep(0.25 * reach, 0.6 * reach, far);
                let amount = push * near * smoothstep(0.0, 0.7, facing);
                if amount > 0.0 {
                    out = [0, 1, 2].map(|axis| out[axis] + way[axis] * amount);
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternions_turn_as_their_matrices_do() {
        for turn in [[30.0, -70.0, 110.0], [-170.0, 20.0, 5.0], [0.0, 179.0, 0.0]] {
            let m = euler(turn);
            let q = quat_of(&m);
            let v = [0.3, -0.7, 0.2];
            let (a, b) = (super::super::apply(&m, v), quat_turn(q, v));
            for axis in 0..3 {
                assert!((a[axis] - b[axis]).abs() < 1e-4, "{turn:?}: {a:?} {b:?}");
            }
        }
        // A twist about y is read back, whatever swing follows it.
        let q = quat_of(&super::super::mul(
            &euler([40.0, 0.0, 0.0]),
            &euler([0.0, 50.0, 0.0]),
        ));
        let (twist, swing) = twist_of(q, [0.0, 1.0, 0.0]);
        assert!((twist.to_degrees() - 50.0).abs() < 0.01, "{twist}");
        assert!((swing.to_degrees() - 40.0).abs() < 0.01, "{swing}");
    }

    use super::super::{Body, Kind, Pose, hand_pose, solve, surface};

    /// The volume the surface encloses.
    fn volume(pose: &Pose, body: &Body) -> f32 {
        let surface = surface(pose, body);
        surface
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| surface.positions[i as usize]);
                dot(a, cross(b, c)) / 6.0
            })
            .sum()
    }

    fn plainly<T>(run: impl FnOnce() -> T) -> T {
        PLAIN.with(|plain| plain.set(true));
        let out = run();
        PLAIN.with(|plain| plain.set(false));
        out
    }

    #[test]
    fn bent_joints_keep_their_volume() {
        let body = Body::default();
        let rest = body.standing();
        let whole = volume(&rest, &body);
        assert!(whole > 0.04, "a body holds some liters: {whole}");
        // Elbows and knees folded right up, the arms raised, a thigh
        // lifted: the plain blend loses flesh at every one of them.
        let mut bent = rest.clone();
        bent.0[6] = [-150.0, 0.0, 0.0];
        bent.0[9] = [-150.0, 0.0, 0.0];
        bent.0[12] = [150.0, 0.0, 0.0];
        bent.0[15] = [150.0, 0.0, 0.0];
        let kept = volume(&bent, &body);
        let lost = plainly(|| volume(&bent, &body));
        assert!(
            (kept - whole).abs() < (lost - whole).abs() * 0.7,
            "whole {whole}, kept {kept}, plain {lost}"
        );
        assert!((kept - whole).abs() < whole * 0.016, "{kept} of {whole}");
    }

    #[test]
    fn a_twist_is_spread_along_the_limb() {
        let body = Body::default();
        let rest = body.standing();
        let mut twisted = rest.clone();
        twisted.0[6] = [0.0, 90.0, 0.0];
        let (before, after) = (surface(&rest, &body), surface(&twisted, &body));
        let joints = solve(&rest, &body);
        let (elbow, wrist) = (joints[6].0, joints[7].0);
        let bone = unit([0, 1, 2].map(|axis| wrist[axis] - elbow[axis]));
        // How far the skin has turned about the forearm at three
        // places along it: none at the elbow, half way in the middle,
        // all of it at the wrist.
        let turned = |from: f32, to: f32, positions: &[[f32; 3]]| {
            let mut sum = 0.0;
            let mut count = 0.0;
            for (p, q) in before.positions.iter().zip(positions) {
                let rel = [0, 1, 2].map(|axis| p[axis] - elbow[axis]);
                let along =
                    dot(rel, bone) / dot([0, 1, 2].map(|axis| wrist[axis] - elbow[axis]), bone);
                let out = [0, 1, 2].map(|axis| rel[axis] - bone[axis] * dot(rel, bone));
                if along < from || along > to || dot(out, out).sqrt() > 0.06 {
                    continue;
                }
                let now = [0, 1, 2].map(|axis| q[axis] - elbow[axis]);
                let now = [0, 1, 2].map(|axis| now[axis] - bone[axis] * dot(now, bone));
                let (a, b) = (unit(out), unit(now));
                sum += dot(cross(a, b), bone).atan2(dot(a, b)).to_degrees().abs();
                count += 1.0;
            }
            assert!(count > 5.0, "points there: {count}");
            sum / count
        };
        let near = turned(0.05, 0.2, &after.positions);
        let middle = turned(0.4, 0.6, &after.positions);
        let far = turned(0.85, 0.98, &after.positions);
        assert!(near < 25.0, "little at the elbow: {near}");
        assert!(
            (30.0..60.0).contains(&middle),
            "half in the middle: {middle}"
        );
        assert!(far > 70.0, "all of it at the wrist: {far}");
        // The plain blend turned the whole forearm at once.
        let plain = plainly(|| surface(&twisted, &body));
        assert!(turned(0.2, 0.35, &plain.positions) > 75.0);
    }

    #[test]
    fn a_fist_wraps_its_thumb_over_the_fingers() {
        let hand = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        let fist = hand_pose(&hand, 2);
        let placed = super::super::bones(&fist, &hand);
        // The thumb's tip lies in front of the middle finger's middle
        // bone (out of the palm, +z), within a finger's width of it.
        let tip = placed[15].1;
        let (from, to) = placed[5];
        let middle = [0, 1, 2].map(|axis| (from[axis] + to[axis]) / 2.0);
        let far = [0, 1, 2].map(|axis| tip[axis] - middle[axis]);
        assert!(tip[2] > middle[2], "in front: {tip:?} {middle:?}");
        assert!(dot(far, far).sqrt() < 0.035, "{tip:?} {middle:?}");
        // And the closed fingers draw together.
        assert!(fist.0[1][1] > 5.0 && fist.0[10][1] < -8.0, "{:?}", fist.0);
        let open = hand_pose(&hand, 0);
        assert_eq!(open.0[1][1], 0.0, "an open hand's stay apart");
    }
}
