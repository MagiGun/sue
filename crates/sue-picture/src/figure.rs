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
//!
//! A figure is of a [`Kind`] ([`KIND`]): the whole body, or Clip Studio
//! Paint 5.0's posable hand (a palm and fifteen finger joints) or 5.1's
//! head (neck, skull, jaw, nose, eyes and ears, to turn and tilt). Each
//! kind has its own joints, framing and preset poses, and poses, draws
//! and is posed by dragging its parts the same way.
//!
//! # Hand joints
//!
//! The stable contract for everything that reads or writes a hand's pose
//! (the Hand Scanner, hand pose materials, the hand setup pad). A hand
//! figure ([`Kind::Hand`]) has the sixteen joints of [`HAND`], in this
//! order, and its [`Pose`] holds one `[x, y, z]` turn in degrees for each:
//!
//! | Index | Name | x (curl) | y (twist) | z (spread) |
//! | --- | --- | --- | --- | --- |
//! | 0 | Palm | free | free | free |
//! | 1 | Index finger base | -30 to 95 | -15 to 15 | -30 to 30 |
//! | 2 | Index finger middle | 0 to 115 | 0 | 0 |
//! | 3 | Index finger tip | -10 to 85 | 0 | 0 |
//! | 4 | Middle finger base | -30 to 95 | -15 to 15 | -30 to 30 |
//! | 5 | Middle finger middle | 0 to 115 | 0 | 0 |
//! | 6 | Middle finger tip | -10 to 85 | 0 | 0 |
//! | 7 | Ring finger base | -30 to 95 | -15 to 15 | -30 to 30 |
//! | 8 | Ring finger middle | 0 to 115 | 0 | 0 |
//! | 9 | Ring finger tip | -10 to 85 | 0 | 0 |
//! | 10 | Little finger base | -30 to 95 | -15 to 15 | -30 to 30 |
//! | 11 | Little finger middle | 0 to 115 | 0 | 0 |
//! | 12 | Little finger tip | -10 to 85 | 0 | 0 |
//! | 13 | Thumb base | -45 to 75 | -60 to 60 | -80 to 45 |
//! | 14 | Thumb middle | -20 to 80 | -20 to 20 | -30 to 30 |
//! | 15 | Thumb tip | -30 to 95 | -10 to 10 | -15 to 15 |
//!
//! The axes are the hand's own, the same for every joint at rest: the
//! palm faces +z, the fingers point up +y, the thumb is on the -x side
//! and the little finger on the +x side (a left hand; a right hand is
//! its mirror image and takes the same numbers). A joint with no turn is
//! the open flat hand. Turning about x by a positive angle curls the
//! joint toward the palm (a fist is about 88, 100, 62 down a finger).
//! Turning about z by a positive angle swings the finger toward the
//! thumb side, a negative one toward the little finger side. Turning
//! about y twists the finger about its own length. The three turns
//! compose as z, then y, then x (`Rz * Ry * Rx`), and each joint's turn
//! is in its parent's frame: the base joints hang from the palm, each
//! middle joint from its base, each tip from its middle.
//!
//! The ranges are the joint angle limits ([`limits`]) that posing keeps
//! to while Joint angle limits is on; a pose text may hold any numbers
//! and still loads. A pose is kept as text by [`Pose::to_text`]
//! (`v1;x,y,z;x,y,z;...`, joints in the order above) and read back by
//! [`Pose::from_text_for`]; joints a text lacks stay unturned and
//! joints it has too many of are dropped, so older and newer texts both
//! load. [`Pose::of`] reads a figure model's pose, [`set_pose`] is the
//! one undo step that poses it, and [`aimed`] makes a pose from the
//! directions bones should point in (what a camera sees).
//!
//! A right hand ([`SIDE`] `right`, [`Side::Right`]) is drawn as the left
//! hand's mirror image and keeps the left hand's numbers and joints:
//! [`Body::joints`] and [`aimed`] work in the left hand's space, so
//! directions measured off a right hand are mirrored (x negated) before
//! they are given to [`aimed`], and the pose that comes back shows
//! right on the right hand. [`solve`], [`bones`] and [`surface`] give
//! the mirrored hand as it is drawn.
//!
//! A body figure's hands carry the same fifteen finger joints, named
//! with their side (`Left index finger base` and so on) after the
//! body's first seventeen joints, so body pose texts from before the
//! fingers existed load unchanged. [`hand_onto_body`] lays a hand pose
//! onto one of a body's hands and [`hand_from_body`] reads it back.

use sue_core::txn::{ReplaceComponent, SetProperty};
use sue_core::{Arena, Entity, EntityId, MeshBuffer3D, Value};
use sue_model::{Geometry, PrimKind};

use crate::doc::Edit;

pub(crate) mod human;
mod posing;
pub(crate) mod skin;
pub use posing::{
    HandSetup, Limits, aimed_at, hand_from_body, hand_onto_body, hand_pose, hand_setup, hanging,
    held, leaned, limb_of, limited, limits, moved, reached, retargeted, turned,
};

/// How far the eyes may look across and up or down, in degrees.
pub const GAZE: [f32; 2] = [35.0, 30.0];

/// Where the eyes of `pose` look: degrees across (positive toward the
/// figure's left) and up, the mean of both eyes.
#[must_use]
pub fn gaze(pose: &Pose, body: &Body) -> [f32; 2] {
    let Some(eyes) = body.eyes() else {
        return [0.0; 2];
    };
    let turn = |eye: usize| pose.0.get(eye).copied().unwrap_or([0.0; 3]);
    let (left, right) = (turn(eyes[0]), turn(eyes[1]));
    [(left[1] + right[1]) / 2.0, -(left[0] + right[0]) / 2.0]
}

/// `pose` with both eyes looking `across` and `up` degrees (kept
/// within [`GAZE`]).
#[must_use]
pub fn with_gaze(pose: &Pose, body: &Body, [across, up]: [f32; 2]) -> Pose {
    let mut out = pose.clone();
    let Some(eyes) = body.eyes() else {
        return out;
    };
    let turn = [
        -up.clamp(-GAZE[1], GAZE[1]),
        across.clamp(-GAZE[0], GAZE[0]),
        0.0,
    ];
    for eye in eyes {
        if out.0.len() <= eye {
            out.0.resize(eye + 1, [0.0; 3]);
        }
        out.0[eye] = turn;
    }
    out
}

/// The class of a drawing figure's model entity.
pub const CLASS: &str = "sue.paint.figure";
/// The pose, as text (see [`Pose::to_text`]).
pub const POSE: &str = "figure.pose";
/// How tall the figure is, in centimeters (`Float`).
pub const HEIGHT: &str = "figure.height";
/// Its head to body ratio: how many heads tall it is (`Float`).
pub const HEADS: &str = "figure.heads";
/// What the figure is: `body`, `hand` or `head` (`Text`).
pub const KIND: &str = "figure.kind";
/// A body's type: `male` or `female` (`Text`).
pub const BUILD: &str = "figure.build";
/// A body's shape on the body shape chart, whole and part by part (see
/// [`Shape::to_text`], `Text`).
pub const SHAPE: &str = "figure.shape";
/// The head to body ratio follows the height, as a real person's does
/// (`Bool`, Clip Studio Paint's Adjust head to body ratio with height).
pub const FOLLOW: &str = "figure.follow_height";
/// The body shape's sliders that are not at rest, as text: `key=value`
/// with `;` between (see [`SLIDERS`], `Text`).
pub const PARTS_TEXT: &str = "figure.parts";
/// Which hand a hand figure is: `left` or `right` (`Text`).
pub const SIDE: &str = "figure.side";
/// The figure shows its guide lines (`Bool`, on unless set).
pub const GUIDES: &str = "figure.guides";
/// A body keeps to the ground as it is posed: it comes down when it
/// sits or crouches (`Bool`; a figure from before this was kept does
/// not).
pub const GROUND: &str = "figure.ground";
/// An imported figure keeps to joint angle limits (`Bool`; off unless
/// set on: its file says nothing of how far its bones may turn, so the
/// limits are the same wide ones for every bone).
pub const LIMITS: &str = "figure.limits";
/// The hands and feet pinned where they are while the rest of the body
/// is posed: their joint numbers, `,` between (`Text`).
pub const PINS: &str = "figure.pins";

/// A body's type: how its shoulders, waist and hips are built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Build {
    /// Broad shoulders, narrow hips: the figure as Sue Paint has made it.
    #[default]
    Male,
    /// Narrower shoulders and waist, wider hips.
    Female,
}

impl Build {
    /// Every type, in the order people see them.
    pub const ALL: [Self; 2] = [Self::Male, Self::Female];

    /// Its name in [`BUILD`].
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
        }
    }

    /// What people read, kept in English (`localize` shows it).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Male => "Male",
            Self::Female => "Female",
        }
    }

    /// The type named `key`; male for anything else.
    #[must_use]
    pub fn from_key(key: &str) -> Self {
        match key {
            "female" => Self::Female,
            _ => Self::Male,
        }
    }
}

/// The parts of a body the body shape chart shapes, in order: the first
/// is the whole body, the rest add to it.
pub const PARTS: [&str; 5] = ["Full body", "Head", "Upper body", "Arms", "Legs"];

/// A body's place on the body shape chart, whole and part by part: across
/// from thin (-1) to fat (1), up from less muscle (-1) to more (1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape(pub [[f32; 2]; PARTS.len()]);

impl Shape {
    /// The shape as text: `v1;` then every part's `thin to fat,muscle`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|[fat, muscle]| format!("{fat},{muscle}"))
            .collect();
        format!("v1;{}", parts.join(";"))
    }

    /// The shape `text` holds; the plain one for anything unreadable, and
    /// every value kept on the chart.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut shape = Self::default();
        let Some(rest) = text.strip_prefix("v1;") else {
            return shape;
        };
        for (part, pair) in shape.0.iter_mut().zip(rest.split(';')) {
            let mut values = pair
                .split(',')
                .map(|value| value.trim().parse::<f32>().unwrap_or(0.0));
            for value in part.iter_mut() {
                let read = values.next().unwrap_or(0.0);
                *value = if read.is_finite() {
                    read.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        shape
    }

    /// Which part of [`PARTS`] joint `at` of [`JOINTS`] belongs to.
    #[must_use]
    pub fn part_of(at: usize) -> usize {
        match at {
            4 => 1,
            0..=3 => 2,
            5..=10 => 3,
            _ => 4,
        }
    }

    /// How much wider across and through joint `at` is drawn: the whole
    /// body's place on the chart and its part's, added and kept on it.
    #[must_use]
    pub fn widths(&self, at: usize) -> [f32; 2] {
        let part = Self::part_of(at);
        let [fat, muscle] =
            [0, 1].map(|axis| (self.0[0][axis] + self.0[part][axis]).clamp(-1.0, 1.0));
        // The head and the trunk carry fat; the limbs carry muscle too.
        let (bulk, depth) = match part {
            1 => (0.05, 0.04),
            2 => (0.1, 0.16),
            _ => (0.22, 0.18),
        };
        let soft = 1.0 + 0.4 * fat;
        [soft * (1.0 + bulk * muscle), soft * (1.0 + depth * muscle)]
    }
}

/// Which hand a hand figure is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// A left hand, as the hand figure is modeled.
    #[default]
    Left,
    /// A right hand: the left hand's mirror image.
    Right,
}

impl Side {
    /// Its name in [`SIDE`].
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// The side named `key`; left for anything else.
    #[must_use]
    pub fn from_key(key: &str) -> Self {
        match key {
            "right" => Self::Right,
            _ => Self::Left,
        }
    }
}

/// One slider of the body shape: a part of the body made longer, wider
/// or more marked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    /// Its name in [`PARTS_TEXT`] and in body shape materials.
    pub key: &'static str,
    /// What people read, kept in English (`localize` shows it).
    pub name: &'static str,
    /// The group it is listed under, as people read it.
    pub group: &'static str,
    /// Its least and most.
    pub range: [f32; 2],
    /// Its value on a new figure.
    pub rest: f32,
    /// Hand figures have it too.
    pub hand: bool,
}

const fn slider(
    key: &'static str,
    name: &'static str,
    group: &'static str,
    range: [f32; 2],
    rest: f32,
    hand: bool,
) -> Slider {
    Slider {
        key,
        name,
        group,
        range,
        rest,
        hand,
    }
}

const WHOLE: [f32; 2] = [-1.0, 1.0];
const PART: [f32; 2] = [0.0, 1.0];

/// The body shape's sliders, part by part, in the order people see
/// them.
pub const SLIDERS: [Slider; 54] = [
    slider("age", "Age", "Whole body", WHOLE, 0.0, false),
    slider("head_width", "Head width", "Head", WHOLE, 0.0, false),
    slider("head_length", "Head length", "Head", WHOLE, 0.0, false),
    slider("face_depth", "Head depth", "Head", WHOLE, 0.0, false),
    slider("anime", "Drawn style", "Face", PART, 0.0, false),
    slider("face_age", "Face age", "Face", WHOLE, 0.0, false),
    slider("forehead", "Forehead height", "Face", WHOLE, 0.0, false),
    slider("temples", "Temples", "Face", WHOLE, 0.0, false),
    slider("cheeks", "Cheeks", "Face", WHOLE, 0.0, false),
    slider("cheekbones", "Cheekbones", "Face", WHOLE, 0.0, false),
    slider("jaw", "Jaw", "Face", WHOLE, 0.0, false),
    slider("chin", "Chin length", "Face", WHOLE, 0.0, false),
    slider("chin_width", "Chin width", "Face", WHOLE, 0.0, false),
    slider("chin_forward", "Chin forward", "Face", WHOLE, 0.0, false),
    slider("eye_size", "Eye size", "Eyes", WHOLE, 0.0, false),
    slider("eye_spacing", "Eye spacing", "Eyes", WHOLE, 0.0, false),
    slider("eye_height", "Eye height", "Eyes", WHOLE, 0.0, false),
    slider("eye_slant", "Eye slant", "Eyes", WHOLE, 0.0, false),
    slider("brows", "Eyebrow height", "Eyes", WHOLE, 0.0, false),
    slider("nose_length", "Nose length", "Nose", WHOLE, 0.0, false),
    slider("nose_width", "Nose width", "Nose", WHOLE, 0.0, false),
    slider("nose_depth", "Nose depth", "Nose", WHOLE, 0.0, false),
    slider("nose_height", "Nose height", "Nose", WHOLE, 0.0, false),
    slider("nose_bridge", "Nose bridge", "Nose", WHOLE, 0.0, false),
    slider("mouth_width", "Mouth width", "Mouth", WHOLE, 0.0, false),
    slider("mouth_height", "Mouth height", "Mouth", WHOLE, 0.0, false),
    slider("lips", "Lips", "Mouth", WHOLE, 0.0, false),
    slider("ear_size", "Ear size", "Ears", WHOLE, 0.0, false),
    slider("ear_wing", "Ears out", "Ears", WHOLE, 0.0, false),
    slider("smile", "Smile", "Expression", PART, 0.0, false),
    slider("angry", "Angry", "Expression", PART, 0.0, false),
    slider("surprised", "Surprised", "Expression", PART, 0.0, false),
    slider("sad", "Sad", "Expression", PART, 0.0, false),
    slider("eyes_closed", "Eyes closed", "Expression", PART, 0.0, false),
    slider("mouth_open", "Mouth open", "Expression", PART, 0.0, false),
    slider("neck_length", "Neck length", "Neck", WHOLE, 0.0, false),
    slider("neck_width", "Neck width", "Neck", WHOLE, 0.0, false),
    slider(
        "shoulders",
        "Shoulder width",
        "Shoulders",
        WHOLE,
        0.0,
        false,
    ),
    slider("chest", "Chest", "Chest", WHOLE, 0.0, false),
    slider("bust", "Bust", "Chest", WHOLE, 0.0, false),
    slider("torso_length", "Torso length", "Chest", WHOLE, 0.0, false),
    slider("waist", "Waist", "Waist", WHOLE, 0.0, false),
    slider("hips", "Hips", "Hips", WHOLE, 0.0, false),
    slider("arm_length", "Arm length", "Arms", WHOLE, 0.0, false),
    slider("arm_width", "Arm width", "Arms", WHOLE, 0.0, true),
    slider("hand_size", "Hand size", "Hands", WHOLE, 0.0, true),
    slider("finger_length", "Finger length", "Hands", WHOLE, 0.0, true),
    slider(
        "finger_width",
        "Finger thickness",
        "Hands",
        WHOLE,
        0.0,
        true,
    ),
    slider("veins", "Veins", "Hands", PART, 0.3, true),
    slider("vein_pattern", "Vein pattern", "Hands", PART, 0.0, true),
    slider(
        "prominence",
        "Tendon and knuckle prominence",
        "Hands",
        PART,
        0.35,
        true,
    ),
    slider("leg_length", "Leg length", "Legs", WHOLE, 0.0, false),
    slider("leg_width", "Leg width", "Legs", WHOLE, 0.0, false),
    slider("foot_size", "Foot size", "Feet", WHOLE, 0.0, false),
];

/// The groups of [`SLIDERS`] that shape the head and face: what a head
/// figure has, and what the Face section lists for a body.
pub const FACE_GROUPS: [&str; 7] = [
    "Head",
    "Face",
    "Eyes",
    "Nose",
    "Mouth",
    "Ears",
    "Expression",
];

/// The expressions: what people read, and the slider that makes each
/// (none for the neutral face).
pub const EXPRESSIONS: [(&str, Option<&str>); 5] = [
    ("Neutral", None),
    ("Smile", Some("smile")),
    ("Angry", Some("angry")),
    ("Surprised", Some("surprised")),
    ("Sad", Some("sad")),
];

impl Slider {
    /// Whether a figure of `kind` has this slider: a body all of them, a
    /// hand its own, a head those of the head, the face and the neck.
    #[must_use]
    pub fn suits(&self, kind: Kind) -> bool {
        match kind {
            Kind::Body => true,
            Kind::Hand => self.hand,
            Kind::Head => FACE_GROUPS.contains(&self.group) || self.group == "Neck",
            Kind::Imported => false,
        }
    }

    /// Whether it shapes the head or face ([`FACE_GROUPS`]).
    #[must_use]
    pub fn of_face(&self) -> bool {
        FACE_GROUPS.contains(&self.group)
    }
}

/// The sliders' values on a new figure.
const SLIDERS_REST: [f32; SLIDERS.len()] = {
    let mut rest = [0.0; SLIDERS.len()];
    let mut at = 0;
    while at < SLIDERS.len() {
        rest[at] = SLIDERS[at].rest;
        at += 1;
    }
    rest
};

/// What a figure is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A whole body ([`JOINTS`]).
    #[default]
    Body,
    /// A hand ([`HAND`]).
    Hand,
    /// A head ([`HEAD_JOINTS`]).
    Head,
    /// A model imported from a file, posed by its own bones.
    Imported,
}

impl Kind {
    /// Its name in [`KIND`].
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Hand => "hand",
            Self::Head => "head",
            Self::Imported => "imported",
        }
    }

    /// The kind named `key`; a body for anything else.
    #[must_use]
    pub fn from_key(key: &str) -> Self {
        match key {
            "hand" => Self::Hand,
            "head" => Self::Head,
            "imported" => Self::Imported,
            _ => Self::Body,
        }
    }
}

/// A finger of three joints: proximal, middle and distal, from `at` on
/// the palm, each bone up, `lengths` long and narrowing from `width`.
const fn finger(
    names: [&'static str; 3],
    first: usize,
    at: [f32; 3],
    lengths: [f32; 3],
    width: f32,
) -> [Joint; 3] {
    [
        joint(
            names[0],
            Some(0),
            at,
            [0.0, lengths[0], 0.0],
            [width, width * 0.95],
            [width * 0.92, width * 0.86],
            None,
        ),
        joint(
            names[1],
            Some(first),
            [0.0, lengths[0], 0.0],
            [0.0, lengths[1], 0.0],
            [width * 0.9, width * 0.85],
            [width * 0.84, width * 0.8],
            None,
        ),
        joint(
            names[2],
            Some(first + 1),
            [0.0, lengths[1], 0.0],
            [0.0, lengths[2], 0.0],
            [width * 0.82, width * 0.78],
            [width * 0.72, width * 0.66],
            None,
        ),
    ]
}

const INDEX: [Joint; 3] = finger(
    [
        "Index finger base",
        "Index finger middle",
        "Index finger tip",
    ],
    1,
    [-0.028, 0.09, 0.0],
    [0.042, 0.026, 0.021],
    0.0095,
);
const MIDDLE: [Joint; 3] = finger(
    [
        "Middle finger base",
        "Middle finger middle",
        "Middle finger tip",
    ],
    4,
    [-0.009, 0.092, 0.0],
    [0.046, 0.029, 0.022],
    0.0098,
);
const RING: [Joint; 3] = finger(
    ["Ring finger base", "Ring finger middle", "Ring finger tip"],
    7,
    [0.01, 0.09, 0.0],
    [0.043, 0.027, 0.021],
    0.0092,
);
const LITTLE: [Joint; 3] = finger(
    [
        "Little finger base",
        "Little finger middle",
        "Little finger tip",
    ],
    10,
    [0.028, 0.084, 0.0],
    [0.034, 0.02, 0.018],
    0.0082,
);

/// A hand (Clip Studio Paint 5.0's 3D hand): the palm facing +z with its
/// fingers up, then the index, middle, ring and little fingers, then the
/// thumb, each finger three joints. Bending a finger (about x) curls it
/// toward the palm.
pub const HAND: [Joint; 16] = [
    joint(
        "Palm",
        None,
        [0.0, 0.0, 0.0],
        [0.0, 0.09, 0.0],
        [0.04, 0.014],
        [0.046, 0.013],
        None,
    ),
    INDEX[0],
    INDEX[1],
    INDEX[2],
    MIDDLE[0],
    MIDDLE[1],
    MIDDLE[2],
    RING[0],
    RING[1],
    RING[2],
    LITTLE[0],
    LITTLE[1],
    LITTLE[2],
    joint(
        "Thumb base",
        Some(0),
        [-0.03, 0.015, 0.012],
        [-0.016, 0.03, 0.014],
        [0.012, 0.011],
        [0.011, 0.0095],
        None,
    ),
    joint(
        "Thumb middle",
        Some(13),
        [-0.016, 0.03, 0.014],
        [-0.006, 0.026, 0.008],
        [0.0098, 0.0092],
        [0.0092, 0.0086],
        None,
    ),
    joint(
        "Thumb tip",
        Some(14),
        [-0.006, 0.026, 0.008],
        [-0.003, 0.021, 0.005],
        [0.0088, 0.0082],
        [0.0074, 0.0068],
        None,
    ),
];

/// A head (Clip Studio Paint 5.1's 3D head), facing +z: the neck, the
/// skull, then the jaw, nose, brow, eyes and ears on it, to turn, tilt
/// and open the mouth.
pub const HEAD_JOINTS: [Joint; 9] = [
    joint(
        "Neck",
        None,
        [0.0, 0.0, 0.0],
        [0.0, 0.09, 0.0],
        [0.05, 0.052],
        [0.048, 0.05],
        None,
    ),
    joint(
        "Skull",
        Some(0),
        [0.0, 0.09, 0.0],
        [0.0, 0.085, -0.01],
        [0.075, 0.085],
        [0.082, 0.095],
        None,
    ),
    joint(
        "Jaw",
        Some(1),
        [0.0, 0.012, 0.022],
        [0.0, -0.042, 0.036],
        [0.066, 0.042],
        [0.042, 0.03],
        None,
    ),
    joint(
        "Nose",
        Some(1),
        [0.0, 0.045, 0.078],
        [0.0, -0.028, 0.018],
        [0.011, 0.009],
        [0.018, 0.013],
        None,
    ),
    joint(
        "Brow",
        Some(1),
        [0.0, 0.082, 0.068],
        [0.0, 0.0, 0.012],
        [0.052, 0.012],
        [0.05, 0.011],
        None,
    ),
    joint(
        "Left eye",
        Some(1),
        [0.032, 0.066, 0.068],
        [0.0, 0.0, 0.008],
        [0.016, 0.016],
        [0.015, 0.015],
        Some(6),
    ),
    joint(
        "Right eye",
        Some(1),
        [-0.032, 0.066, 0.068],
        [0.0, 0.0, 0.008],
        [0.016, 0.016],
        [0.015, 0.015],
        Some(5),
    ),
    joint(
        "Left ear",
        Some(1),
        [0.08, 0.06, -0.005],
        [0.004, -0.04, 0.0],
        [0.01, 0.022],
        [0.009, 0.017],
        Some(8),
    ),
    joint(
        "Right ear",
        Some(1),
        [-0.08, 0.06, -0.005],
        [-0.004, -0.04, 0.0],
        [0.01, 0.022],
        [0.009, 0.017],
        Some(7),
    ),
];

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
const BODY17: [Joint; 17] = [
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

/// What people read for a hand's fifteen finger joints on a body, the
/// left hand's then the right's, in the hand's own order.
const FINGER_NAMES: [[&str; 15]; 2] = [
    [
        "Left index finger base",
        "Left index finger middle",
        "Left index finger tip",
        "Left middle finger base",
        "Left middle finger middle",
        "Left middle finger tip",
        "Left ring finger base",
        "Left ring finger middle",
        "Left ring finger tip",
        "Left little finger base",
        "Left little finger middle",
        "Left little finger tip",
        "Left thumb base",
        "Left thumb middle",
        "Left thumb tip",
    ],
    [
        "Right index finger base",
        "Right index finger middle",
        "Right index finger tip",
        "Right middle finger base",
        "Right middle finger middle",
        "Right middle finger tip",
        "Right ring finger base",
        "Right ring finger middle",
        "Right ring finger tip",
        "Right little finger base",
        "Right little finger middle",
        "Right little finger tip",
        "Right thumb base",
        "Right thumb middle",
        "Right thumb tip",
    ],
];

/// The figure's joints. The first seventeen are the body as it has
/// always been posed (so a pose text from before there were more still
/// loads): hips, spine, chest, neck, head, each arm's upper arm, forearm
/// and hand, each leg's thigh, shin and foot. Then the shoulders (17,
/// 18), the toes (19, 20), the left hand's fifteen finger joints (21 to
/// 35, in [`HAND`]'s order) and the right's (36 to 50), the jaw (51) and
/// the eyes (52, 53). The figure stands about 1.9 tall on the ground,
/// facing +z; its left is +x. Where each joint really sits and how long
/// its bone is comes from the body's mesh ([`Body::joints`]); the
/// numbers here are only the old figure's.
pub const JOINTS: [Joint; 54] = {
    let none = joint(
        "",
        None,
        [0.0; 3],
        [0.0, 0.03, 0.0],
        [0.01; 2],
        [0.01; 2],
        None,
    );
    let mut all = [none; 54];
    let mut at = 0;
    while at < 17 {
        all[at] = BODY17[at];
        at += 1;
    }
    all[17] = joint(
        "Left shoulder",
        Some(2),
        [0.03, 0.17, 0.0],
        [0.17, 0.02, 0.0],
        [0.04; 2],
        [0.04; 2],
        Some(18),
    );
    all[18] = joint(
        "Right shoulder",
        Some(2),
        [-0.03, 0.17, 0.0],
        [-0.17, 0.02, 0.0],
        [0.04; 2],
        [0.04; 2],
        Some(17),
    );
    all[5].parent = Some(17);
    all[8].parent = Some(18);
    all[19] = joint(
        "Left toes",
        Some(13),
        [0.0, -0.04, 0.17],
        [0.0, 0.0, 0.06],
        [0.03, 0.015],
        [0.03, 0.01],
        Some(20),
    );
    all[20] = joint(
        "Right toes",
        Some(16),
        [0.0, -0.04, 0.17],
        [0.0, 0.0, 0.06],
        [0.03, 0.015],
        [0.03, 0.01],
        Some(19),
    );
    let mut side = 0;
    while side < 2 {
        let first = 21 + side * 15;
        let hand = if side == 0 { 7 } else { 10 };
        let mut k = 0;
        while k < 15 {
            let parent = if k % 3 == 0 { hand } else { first + k - 1 };
            let mut finger = HAND[k + 1];
            finger.name = FINGER_NAMES[side][k];
            finger.parent = Some(parent);
            finger.mirror = Some(21 + (1 - side) * 15 + k);
            all[first + k] = finger;
            k += 1;
        }
        side += 1;
    }
    all[51] = joint(
        "Jaw",
        Some(4),
        [0.0, 0.0, 0.03],
        [0.0, -0.06, 0.06],
        [0.04; 2],
        [0.03; 2],
        None,
    );
    all[52] = joint(
        "Left eye",
        Some(4),
        [0.03, 0.03, 0.09],
        [0.0, 0.0, 0.01],
        [0.012; 2],
        [0.012; 2],
        Some(53),
    );
    all[53] = joint(
        "Right eye",
        Some(4),
        [-0.03, 0.03, 0.09],
        [0.0, 0.0, 0.01],
        [0.012; 2],
        [0.012; 2],
        Some(52),
    );
    all
};

/// The middle and size Sue Paint frames a figure by, so posing never
/// changes how large it shows.
pub const FRAME: ([f32; 3], f32) = ([0.0, 0.95, 0.0], 2.1);

/// The figure's body shape: how tall, and how many heads tall, and what
/// kind of figure it is, its type and its place on the body shape chart,
/// and where it stands on its 3D layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Centimeters.
    pub height: f32,
    /// Head to body ratio.
    pub heads: f32,
    /// A body, a hand or a head.
    pub kind: Kind,
    /// A body's type (hands and heads have none).
    pub build: Build,
    /// A body's place on the body shape chart.
    pub shape: Shape,
    /// The head to body ratio follows the height.
    pub follow: bool,
    /// The body shape's sliders, in [`SLIDERS`] order.
    pub parts: [f32; SLIDERS.len()],
    /// Which hand a hand figure is.
    pub side: Side,
    /// It shows its guide lines.
    pub guides: bool,
    /// A body keeps to the ground as it is posed.
    pub ground: bool,
    /// An imported model's skeleton, by number (0 for Sue's own figures).
    pub rig: u64,
    /// An imported model keeps to joint angle limits ([`LIMITS`]).
    pub limits: bool,
    /// Where it stands, how it is turned and how large it is.
    pub place: crate::three::Placement,
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
            kind: entity
                .property(KIND)
                .and_then(Value::as_str)
                .map_or(Kind::Body, Kind::from_key),
            build: entity
                .property(BUILD)
                .and_then(Value::as_str)
                .map_or(Build::Male, Build::from_key),
            shape: entity
                .property(SHAPE)
                .and_then(Value::as_str)
                .map_or_else(Shape::default, Shape::from_text),
            follow: entity
                .property(FOLLOW)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            parts: entity
                .property(PARTS_TEXT)
                .and_then(Value::as_str)
                .map_or(SLIDERS_REST, Self::parts_from_text),
            side: entity
                .property(SIDE)
                .and_then(Value::as_str)
                .map_or(Side::Left, Side::from_key),
            guides: entity
                .property(GUIDES)
                .and_then(Value::as_bool)
                .unwrap_or(true),
            ground: entity
                .property(GROUND)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rig: if entity.property(KIND).and_then(Value::as_str) == Some("imported") {
                crate::model3d::rig_of(entity)
            } else {
                0
            },
            limits: entity
                .property(LIMITS)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            place: crate::three::Placement::of(entity),
        }
    }

    /// The value of the slider named `key` (0 when there is none).
    #[must_use]
    pub fn slider(&self, key: &str) -> f32 {
        SLIDERS
            .iter()
            .position(|slider| slider.key == key)
            .map_or(0.0, |at| self.parts[at])
    }

    /// The same body with the slider named `key` at `value`, kept in
    /// its range.
    #[must_use]
    pub fn with_slider(mut self, key: &str, value: f32) -> Self {
        if let Some(at) = SLIDERS.iter().position(|slider| slider.key == key)
            && value.is_finite()
        {
            let [least, most] = SLIDERS[at].range;
            self.parts[at] = value.clamp(least, most);
        }
        self
    }

    /// The same body wearing expression `index` of [`EXPRESSIONS`]
    /// whole, the other expressions gone (the first is the neutral
    /// face).
    #[must_use]
    pub fn with_expression(self, index: usize) -> Self {
        let mut body = self;
        for (at, (_, key)) in EXPRESSIONS.iter().enumerate() {
            if let Some(key) = key {
                body = body.with_slider(key, if at == index { 1.0 } else { 0.0 });
            }
        }
        body
    }

    /// Which of [`EXPRESSIONS`] the body wears whole and alone, if any.
    #[must_use]
    pub fn expression(&self) -> Option<usize> {
        let worn: Vec<f32> = EXPRESSIONS
            .iter()
            .map(|(_, key)| key.map_or(0.0, |key| self.slider(key)))
            .collect();
        if worn.iter().all(|value| *value == 0.0) {
            return Some(0);
        }
        let at = worn.iter().position(|value| *value == 1.0)?;
        (worn.iter().filter(|value| **value != 0.0).count() == 1).then_some(at)
    }

    /// The joints of this kind's two eyes (left, right), when it has
    /// eyes to turn.
    #[must_use]
    pub fn eyes(&self) -> Option<[usize; 2]> {
        match self.kind {
            Kind::Body => Some(human::at::EYES),
            Kind::Head => Some([5, 6]),
            _ => None,
        }
    }

    /// The sliders that are not at rest as text: `key=value`, `;`
    /// between.
    #[must_use]
    pub fn parts_text(&self) -> String {
        SLIDERS
            .iter()
            .zip(self.parts)
            .filter(|(slider, value)| (*value - slider.rest).abs() > 1e-6)
            .map(|(slider, value)| format!("{}={value}", slider.key))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The sliders `text` ([`Self::parts_text`]) holds; what it does not
    /// name, or names unreadably, is at rest.
    #[must_use]
    pub fn parts_from_text(text: &str) -> [f32; SLIDERS.len()] {
        let mut body = Self::default();
        for pair in text.split(';') {
            if let Some((key, value)) = pair.split_once('=')
                && let Ok(value) = value.trim().parse::<f32>()
            {
                body = body.with_slider(key.trim(), value);
            }
        }
        body.parts
    }

    /// The head to body ratio of a person `height` centimeters tall, as
    /// Adjust head to body ratio with height sets it: four heads at 50 cm,
    /// seven and a half at 170, kept among [`Self::RATIOS`].
    #[must_use]
    pub fn heads_for(height: f32) -> f32 {
        (4.0 + 3.5 * (height - 50.0) / (Self::HEIGHT - 50.0))
            .clamp(*Self::RATIOS.start(), *Self::RATIOS.end())
    }

    /// The same body `height` tall, its ratio following when it follows.
    #[must_use]
    pub fn with_height(self, height: f32) -> Self {
        let height = height.clamp(*Self::HEIGHTS.start(), *Self::HEIGHTS.end());
        Self {
            height,
            heads: if self.follow {
                Self::heads_for(height)
            } else {
                self.heads
            },
            ..self
        }
    }

    /// The same body with its shape as made: height, ratio, type and chart
    /// back to Sue Paint's (Clip Studio Paint's Initialize). Its kind and
    /// where it stands stay.
    #[must_use]
    pub fn initialized(self) -> Self {
        Self {
            kind: self.kind,
            side: self.side,
            ground: self.ground,
            rig: self.rig,
            limits: self.limits,
            place: self.place,
            ..Self::default()
        }
    }

    /// The body shape as text, for a body shape material: one `key=value`
    /// line each for the type, height, ratio, chart and whether the ratio
    /// follows the height.
    #[must_use]
    pub fn shape_text(&self) -> String {
        format!(
            "build={}\nheight={}\nheads={}\nshape={}\nfollow={}\nparts={}\n",
            self.build.key(),
            self.height,
            self.heads,
            self.shape.to_text(),
            self.follow,
            self.parts_text(),
        )
    }

    /// This body shaped as `text` ([`Self::shape_text`]) says; what it
    /// does not say stays. `None` when it says nothing readable.
    #[must_use]
    pub fn with_shape_text(self, text: &str) -> Option<Self> {
        let mut body = self;
        let mut read = false;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let number = value.parse::<f32>().ok().filter(|v| v.is_finite());
            match key.trim() {
                "build" => body.build = Build::from_key(value),
                "height" => {
                    let Some(height) = number else { continue };
                    body.height = height.clamp(*Self::HEIGHTS.start(), *Self::HEIGHTS.end());
                }
                "heads" => {
                    let Some(heads) = number else { continue };
                    body.heads = heads.clamp(*Self::RATIOS.start(), *Self::RATIOS.end());
                }
                "shape" => body.shape = Shape::from_text(value),
                "follow" => body.follow = value == "true",
                "parts" => body.parts = Self::parts_from_text(value),
                _ => continue,
            }
            read = true;
        }
        read.then_some(body)
    }

    /// The middle and size Sue Paint frames this figure by, so posing
    /// never changes how large it shows.
    #[must_use]
    pub fn frame(&self) -> ([f32; 3], f32) {
        let scale = self.height / Self::HEIGHT;
        let (middle, size) = match self.kind {
            Kind::Body => FRAME,
            Kind::Hand => ([0.0, 0.07, 0.0], 0.3),
            Kind::Head => ([0.0, 0.11, 0.02], 0.38),
            Kind::Imported => {
                return human::shaped(self).frame.unwrap_or(([0.0; 3], 2.0));
            }
        };
        (middle.map(|value| value * scale), size * scale)
    }

    /// What people read for this kind's preset poses, in order.
    #[must_use]
    pub fn presets(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Body => &PRESETS,
            Kind::Hand => &HAND_PRESETS,
            Kind::Head => &HEAD_PRESETS,
            Kind::Imported => &[],
        }
    }

    /// This kind's preset pose `index` (the first is its rest).
    #[must_use]
    pub fn preset(&self, index: usize) -> Pose {
        match self.kind {
            Kind::Body => {
                // The body's pose, its hands relaxed (or as the pose
                // holds them).
                let mut pose = posing::settled(self, index, preset(index));
                let hands = match index {
                    15 => [2, 2],
                    10 => [1, 0],
                    5 | 14 => [0, 0],
                    _ => [1, 1],
                };
                for (side, hand) in [Side::Left, Side::Right].into_iter().zip(hands) {
                    hand_onto_body(self, &mut pose, side, &hand_pose(self, hand));
                }
                pose
            }
            Kind::Hand => hand_pose(self, index),
            Kind::Head => head_preset(index),
            Kind::Imported => Pose(vec![[0.0; 3]; human::shaped(self).joints.len()]),
        }
    }

    /// This kind's first preset, the pose Reset pose goes back to.
    #[must_use]
    pub fn standing(&self) -> Pose {
        self.preset(0)
    }

    /// The joints shaped for this body, as poses turn them: where each
    /// sits in its parent's frame and its bone in its own, measured off
    /// the body's mesh once its type, chart, sliders, head to body ratio
    /// and height have shaped it. A right hand's are the left hand's (it
    /// is drawn mirrored), so the same pose numbers suit both.
    #[must_use]
    pub fn joints(&self) -> Vec<Joint> {
        let scale = self.place.scale;
        human::shaped(self)
            .joints
            .iter()
            .map(|joint| Joint {
                offset: joint.offset.map(|value| value * scale),
                bone: joint.bone.map(|value| value * scale),
                start: joint.start.map(|value| value * scale),
                end: joint.end.map(|value| value * scale),
                ..*joint
            })
            .collect()
    }
}

impl Default for Body {
    fn default() -> Self {
        Self {
            height: Self::HEIGHT,
            heads: Self::HEADS,
            kind: Kind::Body,
            build: Build::Male,
            shape: Shape::default(),
            follow: false,
            parts: SLIDERS_REST,
            side: Side::Left,
            guides: true,
            ground: true,
            rig: 0,
            limits: false,
            place: crate::three::Placement::default(),
        }
    }
}

/// The head, in [`JOINTS`].
#[cfg(test)]
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
        Self::from_text_for(text, JOINTS.len())
    }

    /// A pose of `count` joints from [`Self::to_text`].
    #[must_use]
    pub fn from_text_for(text: &str, count: usize) -> Self {
        let mut pose = Self(vec![[0.0; 3]; count]);
        let Some(body) = text.strip_prefix("v1;") else {
            return pose;
        };
        for (at, joint) in body.split(';').enumerate().take(count) {
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
        let body = Body::of(entity);
        let count = body.joints().len();
        entity
            .property(POSE)
            .and_then(Value::as_str)
            .map_or_else(|| body.standing(), |text| Self::from_text_for(text, count))
    }

    /// The pose mirrored left to right (Clip Studio Paint's Flip pose):
    /// each side takes the other's turns, swung and twisted the other
    /// way.
    #[must_use]
    pub fn flipped(&self) -> Self {
        let joints: Vec<Joint> = if self.0.len() == HAND.len() {
            HAND.to_vec()
        } else if self.0.len() == HEAD_JOINTS.len() {
            HEAD_JOINTS.to_vec()
        } else {
            JOINTS.to_vec()
        };
        let mut out = self.clone();
        let mirror =
            |at: usize, joint: &Joint| joint.mirror.filter(|m| *m < self.0.len()).unwrap_or(at);
        for (at, joint) in joints.iter().enumerate().take(self.0.len()) {
            let [x, y, z] = self.0[mirror(at, joint)];
            out.0[at] = [x, -y, -z];
        }
        out
    }
}

/// What people read for the pose presets, in order.
pub const PRESETS: [&str; 20] = [
    "Stand",
    "T-pose",
    "Walk",
    "Run",
    "Sit",
    "Arms up",
    "Think",
    "Contrapposto",
    "Hands on hips",
    "Arms crossed",
    "Wave",
    "Reach forward",
    "Crouch",
    "Kneel",
    "Jump",
    "Punch",
    "Kick",
    "Sit on the floor",
    "Bow",
    "Look back",
];

/// What people read for the hand's preset poses, in order.
pub const HAND_PRESETS: [&str; 16] = [
    "Open hand",
    "Relaxed",
    "Fist",
    "Pointing",
    "Peace sign",
    "Thumbs up",
    "Holding",
    "Spread",
    "Flat fold",
    "OK sign",
    "Pinch",
    "Claw",
    "Rock",
    "Three",
    "Call me",
    "Finger gun",
];

/// Hand preset pose `index` (see [`HAND_PRESETS`]).
#[must_use]
pub fn hand_preset(index: usize) -> Pose {
    let mut pose = Pose(vec![[0.0; 3]; HAND.len()]);
    // A finger (its first joint) curled by its three bends, spread by
    // `spread` degrees.
    let mut curl = |first: usize, bends: [f32; 3], spread: f32| {
        pose.0[first] = [bends[0], 0.0, spread];
        pose.0[first + 1] = [bends[1], 0.0, 0.0];
        pose.0[first + 2] = [bends[2], 0.0, 0.0];
    };
    let fist = [88.0, 100.0, 62.0];
    match index {
        1 => {
            for (first, spread) in [(1, 4.0), (4, 0.0), (7, -4.0), (10, -8.0)] {
                curl(first, [14.0, 20.0, 10.0], spread);
            }
            curl(13, [10.0, 8.0, 6.0], 6.0);
        }
        2 => {
            for first in [1, 4, 7, 10] {
                curl(first, fist, 0.0);
            }
            curl(13, [24.0, 34.0, 38.0], -42.0);
        }
        3 => {
            for first in [4, 7, 10] {
                curl(first, fist, 0.0);
            }
            curl(13, [24.0, 34.0, 38.0], -42.0);
        }
        4 => {
            curl(1, [0.0, 0.0, 0.0], 9.0);
            curl(4, [0.0, 0.0, 0.0], -7.0);
            for first in [7, 10] {
                curl(first, fist, 0.0);
            }
            curl(13, [26.0, 36.0, 40.0], -44.0);
        }
        5 => {
            for first in [1, 4, 7, 10] {
                curl(first, fist, 0.0);
            }
            curl(13, [-10.0, 0.0, 0.0], 18.0);
        }
        6 => {
            for (first, spread) in [(1, 3.0), (4, 0.0), (7, -3.0), (10, -6.0)] {
                curl(first, [52.0, 48.0, 30.0], spread);
            }
            curl(13, [26.0, 18.0, 18.0], -32.0);
        }
        _ => {}
    }
    pose
}

/// What people read for the head's preset poses, in order.
pub const HEAD_PRESETS: [&str; 7] = [
    "Front",
    "Three-quarter",
    "Look up",
    "Look down",
    "Turn left",
    "Tilt",
    "Open mouth",
];

/// Head preset pose `index` (see [`HEAD_PRESETS`]): the neck turns and
/// tilts the head; the jaw opens the mouth.
#[must_use]
pub fn head_preset(index: usize) -> Pose {
    let mut pose = Pose(vec![[0.0; 3]; HEAD_JOINTS.len()]);
    match index {
        1 => {
            pose.0[0] = [0.0, 30.0, 0.0];
            pose.0[1] = [4.0, 10.0, 0.0];
        }
        2 => {
            pose.0[0] = [-12.0, 0.0, 0.0];
            pose.0[1] = [-18.0, 0.0, 0.0];
        }
        3 => {
            pose.0[0] = [12.0, 0.0, 0.0];
            pose.0[1] = [20.0, 0.0, 0.0];
        }
        4 => {
            pose.0[0] = [0.0, 60.0, 0.0];
            pose.0[1] = [0.0, 15.0, 0.0];
        }
        5 => pose.0[1] = [0.0, 0.0, 16.0],
        6 => pose.0[2] = [26.0, 0.0, 0.0],
        _ => {}
    }
    pose
}

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
        // Arms up: the shoulders rise with them.
        5 => {
            set(17, [0.0, 0.0, 12.0]);
            set(18, [0.0, 0.0, -12.0]);
            set(5, [-8.0, 0.0, 152.0]);
            set(8, [-8.0, 0.0, -152.0]);
            set(6, [-10.0, 0.0, 0.0]);
            set(9, [-10.0, 0.0, 0.0]);
        }
        6 => {
            set(4, [12.0, 0.0, -6.0]);
            set(8, [-45.0, 0.0, -10.0]);
            set(9, [-135.0, 0.0, 0.0]);
            set(5, [10.0, 0.0, 15.0]);
            set(6, [-90.0, 0.0, 0.0]);
        }
        // Contrapposto: the weight on one leg, the hips and shoulders
        // tilted against each other.
        7 => {
            set(0, [0.0, 0.0, 6.0]);
            set(1, [0.0, 0.0, -5.0]);
            set(2, [0.0, 8.0, -6.0]);
            set(4, [0.0, -12.0, 4.0]);
            set(11, [-4.0, 0.0, 2.0]);
            set(14, [-14.0, 0.0, -10.0]);
            set(15, [24.0, 0.0, 0.0]);
            set(16, [-8.0, 0.0, 0.0]);
            set(5, [5.0, 0.0, 14.0]);
            set(6, [-18.0, 0.0, 0.0]);
            set(8, [-8.0, 0.0, -16.0]);
            set(9, [-30.0, 0.0, 0.0]);
        }
        // Hands on hips.
        8 => {
            set(5, [22.0, -70.0, 46.0]);
            set(6, [-98.0, 0.0, 0.0]);
            set(8, [22.0, 70.0, -46.0]);
            set(9, [-98.0, 0.0, 0.0]);
            set(11, [0.0, 0.0, 6.0]);
            set(14, [0.0, 0.0, -6.0]);
        }
        // Arms crossed.
        9 => {
            set(5, [-18.0, -60.0, 16.0]);
            set(6, [-95.0, 0.0, 0.0]);
            set(7, [0.0, 0.0, 0.0]);
            set(8, [-18.0, 60.0, -16.0]);
            set(9, [-95.0, 0.0, 0.0]);
            set(4, [4.0, 0.0, 0.0]);
        }
        // Wave.
        10 => {
            set(18, [0.0, 0.0, -6.0]);
            set(8, [0.0, -90.0, -90.0]);
            set(9, [-78.0, 0.0, 0.0]);
            set(5, [0.0, 0.0, 10.0]);
            set(6, [-10.0, 0.0, 0.0]);
            set(4, [0.0, -8.0, -6.0]);
            set(2, [0.0, 0.0, -4.0]);
        }
        // Reach forward.
        11 => {
            set(1, [16.0, 0.0, 0.0]);
            set(2, [10.0, 12.0, 0.0]);
            set(8, [-88.0, 0.0, -4.0]);
            set(9, [-8.0, 0.0, 0.0]);
            set(5, [22.0, 0.0, 12.0]);
            set(6, [-20.0, 0.0, 0.0]);
            set(11, [-24.0, 0.0, 3.0]);
            set(12, [18.0, 0.0, 0.0]);
            set(14, [16.0, 0.0, -3.0]);
            set(15, [22.0, 0.0, 0.0]);
        }
        // Crouch.
        12 => {
            set(1, [28.0, 0.0, 0.0]);
            set(2, [14.0, 0.0, 0.0]);
            set(4, [-24.0, 0.0, 0.0]);
            set(11, [-112.0, 0.0, 10.0]);
            set(12, [132.0, 0.0, 0.0]);
            set(13, [-22.0, 0.0, 0.0]);
            set(14, [-112.0, 0.0, -10.0]);
            set(15, [132.0, 0.0, 0.0]);
            set(16, [-22.0, 0.0, 0.0]);
            set(5, [-58.0, 0.0, 10.0]);
            set(6, [-42.0, 0.0, 0.0]);
            set(8, [-58.0, 0.0, -10.0]);
            set(9, [-42.0, 0.0, 0.0]);
        }
        // Kneel, on one knee.
        13 => {
            set(11, [-88.0, 0.0, 4.0]);
            set(12, [88.0, 0.0, 0.0]);
            set(14, [6.0, 0.0, -4.0]);
            set(15, [96.0, 0.0, 0.0]);
            set(16, [38.0, 0.0, 0.0]);
            set(5, [-36.0, 0.0, 8.0]);
            set(6, [-48.0, 0.0, 0.0]);
            set(8, [6.0, 0.0, -8.0]);
            set(9, [-12.0, 0.0, 0.0]);
        }
        // Jump.
        14 => {
            set(1, [-10.0, 0.0, 0.0]);
            set(11, [-72.0, 0.0, 6.0]);
            set(12, [102.0, 0.0, 0.0]);
            set(14, [22.0, 0.0, -6.0]);
            set(15, [64.0, 0.0, 0.0]);
            set(16, [30.0, 0.0, 0.0]);
            set(2, [-8.0, 0.0, 0.0]);
            set(17, [0.0, 0.0, 10.0]);
            set(18, [0.0, 0.0, -10.0]);
            set(5, [-14.0, 0.0, 130.0]);
            set(6, [-24.0, 0.0, 0.0]);
            set(8, [-14.0, 0.0, -140.0]);
            set(9, [-20.0, 0.0, 0.0]);
            set(13, [34.0, 0.0, 0.0]);
            set(4, [-12.0, 0.0, 0.0]);
        }
        // Punch.
        15 => {
            set(0, [0.0, 10.0, 0.0]);
            set(2, [6.0, 24.0, 0.0]);
            set(4, [0.0, -26.0, 0.0]);
            set(8, [-84.0, 0.0, -8.0]);
            set(9, [-6.0, 0.0, 0.0]);
            set(5, [14.0, -40.0, 24.0]);
            set(6, [-122.0, 0.0, 0.0]);
            set(11, [-28.0, 0.0, 8.0]);
            set(12, [28.0, 0.0, 0.0]);
            set(14, [22.0, 0.0, -8.0]);
            set(15, [18.0, 0.0, 0.0]);
        }
        // Kick.
        16 => {
            set(1, [-12.0, 0.0, 0.0]);
            set(14, [-86.0, 0.0, -4.0]);
            set(15, [10.0, 0.0, 0.0]);
            set(16, [28.0, 0.0, 0.0]);
            set(11, [6.0, 0.0, 4.0]);
            set(12, [8.0, 0.0, 0.0]);
            set(5, [-30.0, 0.0, 30.0]);
            set(6, [-62.0, 0.0, 0.0]);
            set(8, [36.0, 0.0, -26.0]);
            set(9, [-40.0, 0.0, 0.0]);
        }
        // Sit on the floor, knees up, leaning on the hands.
        17 => {
            set(0, [-16.0, 0.0, 0.0]);
            set(1, [10.0, 0.0, 0.0]);
            set(11, [-84.0, 0.0, 12.0]);
            set(12, [84.0, 0.0, 0.0]);
            set(13, [-6.0, 0.0, 0.0]);
            set(14, [-84.0, 0.0, -12.0]);
            set(15, [84.0, 0.0, 0.0]);
            set(16, [-6.0, 0.0, 0.0]);
            set(5, [36.0, 0.0, 16.0]);
            set(6, [-4.0, 0.0, 0.0]);
            set(8, [36.0, 0.0, -16.0]);
            set(9, [-4.0, 0.0, 0.0]);
        }
        // Bow.
        18 => {
            set(1, [36.0, 0.0, 0.0]);
            set(2, [24.0, 0.0, 0.0]);
            set(3, [10.0, 0.0, 0.0]);
            set(5, [-6.0, 0.0, 6.0]);
            set(8, [-6.0, 0.0, -6.0]);
        }
        // Look back over the shoulder.
        19 => {
            set(1, [0.0, 20.0, 0.0]);
            set(2, [0.0, 32.0, 0.0]);
            set(3, [0.0, 38.0, 0.0]);
            set(4, [4.0, 24.0, 0.0]);
            set(5, [10.0, 0.0, 12.0]);
            set(6, [-14.0, 0.0, 0.0]);
            set(8, [-14.0, 0.0, -14.0]);
            set(9, [-22.0, 0.0, 0.0]);
            set(14, [-10.0, 0.0, -4.0]);
            set(15, [16.0, 0.0, 0.0]);
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
    let shaped = human::shaped(body);
    let stand = body.place.matrix();
    let flip = human::is_mirrored(body);
    human::posed(pose, &shaped, body)
        .into_iter()
        .map(|(mut at, mut frame)| {
            // A right hand: the left hand's mirror image.
            if flip {
                at[0] = -at[0];
                for (row, line) in frame.iter_mut().enumerate() {
                    for (column, value) in line.iter_mut().enumerate() {
                        if (row == 0) != (column == 0) {
                            *value = -*value;
                        }
                    }
                }
            }
            (
                add(body.place.position, apply(&stand, at)),
                mul(&stand, &frame),
            )
        })
        .collect()
}

fn transpose(m: &Mat3) -> Mat3 {
    [0, 1, 2].map(|row| [0, 1, 2].map(|column| m[column][row]))
}

fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (length > 1e-6).then(|| v.map(|value| value / length))
}

/// The shortest turn that takes unit `from` onto unit `to`.
fn between(from: [f32; 3], to: [f32; 3]) -> Mat3 {
    let dot = (from[0] * to[0] + from[1] * to[1] + from[2] * to[2]).clamp(-1.0, 1.0);
    let cross = [
        from[1] * to[2] - from[2] * to[1],
        from[2] * to[0] - from[0] * to[2],
        from[0] * to[1] - from[1] * to[0],
    ];
    let axis = unit(cross).unwrap_or_else(|| {
        // Opposite: any axis across `from`.
        let other = if from[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        unit([
            from[1] * other[2] - from[2] * other[1],
            from[2] * other[0] - from[0] * other[2],
            from[0] * other[1] - from[1] * other[0],
        ])
        .unwrap_or([0.0, 0.0, 1.0])
    });
    let angle = dot.acos();
    let (sin, cos) = angle.sin_cos();
    let [x, y, z] = axis;
    let t = 1.0 - cos;
    [
        [t * x * x + cos, t * x * y - sin * z, t * x * z + sin * y],
        [t * x * y + sin * z, t * y * y + cos, t * y * z - sin * x],
        [t * x * z - sin * y, t * y * z + sin * x, t * z * z + cos],
    ]
}

/// The degrees about x, y and z of turn `m`, as [`euler`] composes them.
fn degrees(m: &Mat3) -> [f32; 3] {
    let y = (-m[2][0]).clamp(-1.0, 1.0).asin();
    let x = m[2][1].atan2(m[2][2]);
    let z = m[1][0].atan2(m[0][0]);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

/// A pose that points bones where `targets` say (Clip Studio Paint's Pose
/// Scanner and Hand Scanner): `root` turns the first joint whole (its
/// columns are where its x, y and z go), and each joint with a target
/// direction (in the figure's space) turns the least that lays its bone
/// along it. Joints without one keep `base`'s turn.
#[must_use]
pub fn aimed(
    joints: &[Joint],
    base: &Pose,
    root: Option<[[f32; 3]; 3]>,
    targets: &[Option<[f32; 3]>],
) -> Pose {
    let mut pose = Pose(
        (0..joints.len())
            .map(|at| base.0.get(at).copied().unwrap_or([0.0; 3]))
            .collect(),
    );
    let mut frames: Vec<Mat3> = vec![IDENTITY; joints.len()];
    // Parents first, whatever order the joints are listed in.
    let mut order: Vec<usize> = Vec::with_capacity(joints.len());
    let mut done = vec![false; joints.len()];
    while order.len() < joints.len() {
        let before = order.len();
        for (at, joint) in joints.iter().enumerate() {
            if !done[at]
                && joint
                    .parent
                    .is_none_or(|parent| parent >= joints.len() || done[parent])
            {
                done[at] = true;
                order.push(at);
            }
        }
        if order.len() == before {
            break;
        }
    }
    for at in order {
        let joint = &joints[at];
        let parent = joint
            .parent
            .filter(|parent| *parent < joints.len())
            .map_or(IDENTITY, |parent| frames[parent]);
        if at == 0
            && let Some(root) = root
        {
            pose.0[0] = degrees(&root);
        } else if let Some(target) = targets.get(at).copied().flatten().and_then(unit)
            && let Some(bone) = unit(joint.bone)
        {
            let local = apply(&transpose(&parent), target);
            pose.0[at] = degrees(&between(bone, local));
        }
        frames[at] = mul(&parent, &euler(pose.0[at]));
    }
    pose
}

pub(crate) const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Each joint's bone, start and end, for `pose`.
#[must_use]
pub fn bones(pose: &Pose, body: &Body) -> Vec<([f32; 3], [f32; 3])> {
    solve(pose, body)
        .iter()
        .zip(body.joints().iter())
        .map(|((start, frame), joint)| (*start, add(*start, apply(frame, joint.bone))))
        .collect()
}

/// The posed body as a surface to draw: its skin bent to the pose, with
/// its guide lines and the relief of its hands.
#[must_use]
pub fn surface(pose: &Pose, body: &Body) -> crate::three::Surface {
    human::surface(pose, body)
}

/// The posed body as `view` sees it on a `size` canvas.
#[must_use]
pub fn render(
    pose: &Pose,
    body: &Body,
    view: &crate::three::View3D,
    size: [u32; 2],
) -> crate::three::Buffers {
    crate::three::render_surface(&surface(pose, body), view, size, body.frame())
}

/// The posed body as one shape of triangles.
#[must_use]
pub fn geometry(pose: &Pose, body: &Body) -> Geometry {
    let surface = surface(pose, body);
    let mut geometry = Geometry::new();
    let ids: Vec<u32> = surface
        .positions
        .iter()
        .map(|p| geometry.add_point(p.map(f64::from)))
        .collect();
    for t in surface.indices.as_chunks::<3>().0 {
        let _ = geometry.add_prim(PrimKind::Polygon, &t.map(|i| ids[i as usize]));
    }
    geometry
}

/// The posed body as triangles.
#[must_use]
pub fn mesh(pose: &Pose, body: &Body) -> MeshBuffer3D {
    let surface = surface(pose, body);
    MeshBuffer3D::new(surface.positions, surface.indices).unwrap_or_default()
}

/// A new figure model, standing.
#[must_use]
pub fn new_model(name: &str) -> Entity {
    new_of(name, Kind::Body)
}

/// A new figure model of `kind`, in its first preset pose.
#[must_use]
pub fn new_of(name: &str, kind: Kind) -> Entity {
    let body = Body {
        kind,
        ..Body::default()
    };
    Entity::empty(name, CLASS)
        .with_property(KIND, Value::from(kind.key()))
        .with_property(GROUND, Value::from(kind == Kind::Body))
        .with_property(POSE, Value::from(body.standing().to_text()))
}

/// The three rings of the rotation manipulator round joint `joint` of
/// the posed body: for each of the joint's own x, y and z, a circle of
/// points square to it, in the model's space. Turning a ring turns the
/// joint about that axis ([`turned`]).
#[must_use]
pub fn rings(pose: &Pose, body: &Body, joint: usize) -> Option<[Vec<[f32; 3]>; 3]> {
    let placed = solve(pose, body);
    let (at, frame) = *placed.get(joint)?;
    let joints = body.joints();
    let bone = joints.get(joint)?.bone;
    let long = (bone[0] * bone[0] + bone[1] * bone[1] + bone[2] * bone[2]).sqrt();
    let size = body.frame().1 * body.place.scale;
    let radius = (long * 0.8).clamp(size * 0.035, size * 0.075);
    let axis = |k: usize| [frame[0][k], frame[1][k], frame[2][k]];
    Some([0, 1, 2].map(|k| {
        let (u, v) = (axis((k + 1) % 3), axis((k + 2) % 3));
        (0..=48)
            .map(|step| {
                let (sin, cos) = (step as f32 / 48.0 * std::f32::consts::TAU).sin_cos();
                [0, 1, 2].map(|i| at[i] + radius * (cos * u[i] + sin * v[i]))
            })
            .collect()
    }))
}

/// The three arrows of the translate manipulator at the root of the
/// posed body (its hips): for the world's x, y and z, where the arrow
/// starts and ends, just outside the rotation rings. Dragging one moves
/// the whole body along that axis ([`moved`]). `None` for a joint that
/// is not the root.
#[must_use]
pub fn arrows(pose: &Pose, body: &Body, joint: usize) -> Option<[[[f32; 3]; 2]; 3]> {
    let joints = body.joints();
    if joints.get(joint)?.parent.is_some() {
        return None;
    }
    let at = solve(pose, body).get(joint)?.0;
    let size = body.frame().1 * body.place.scale;
    let (from, to) = (size * 0.085, size * 0.16);
    Some([0, 1, 2].map(|axis| {
        let mut start = at;
        let mut end = at;
        start[axis] += from;
        end[axis] += to;
        [start, end]
    }))
}

/// The hands and feet pinned on figure model `entity`, by joint.
#[must_use]
pub fn pins_of(entity: &Entity) -> Vec<usize> {
    entity
        .property(PINS)
        .and_then(Value::as_str)
        .map(|text| {
            text.split(',')
                .filter_map(|joint| joint.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `pins` as [`PINS`] keeps them.
#[must_use]
pub fn pins_text(pins: &[usize]) -> String {
    pins.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// The edit that makes the figure of 3D layer `layer` another of Sue's
/// models (a body of a type, a hand of a side, or a head) in that
/// model's first pose, and draws the layer again, as one step named
/// `label`. `None` for an imported model, which is its own.
#[must_use]
pub fn set_model(
    arena: &Arena,
    layer: EntityId,
    kind: Kind,
    build: Build,
    side: Side,
    label: &str,
) -> Option<Edit> {
    let model = of_layer(arena, layer)?;
    let now = Body::of(arena.get_by_id(model).ok()?);
    if now.kind == Kind::Imported || kind == Kind::Imported {
        return None;
    }
    let body = Body {
        kind,
        build,
        side,
        ground: kind == Kind::Body,
        ..now
    };
    let pose = if kind == now.kind {
        Pose::of(arena.get_by_id(model).ok()?)
    } else {
        body.standing()
    };
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let mut view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
    let mut edit = Edit::new(label);
    if kind != now.kind {
        // Another kind of model is looked at its own way.
        let fresh = crate::three::figure_view(kind);
        view.yaw = fresh.yaw;
        view.pitch = fresh.pitch;
        view.distance = fresh.distance;
        view.pan = [0.0; 2];
        for set in view.edits(layer) {
            edit = edit.then(set);
        }
    }
    let drawn = crate::three::draw(&render(&pose, &body, &view, size), &view)?;
    Some(
        edit.then(SetProperty::new(model, KIND, kind.key()))
            .then(SetProperty::new(model, BUILD, build.key()))
            .then(SetProperty::new(model, SIDE, side.key()))
            .then(SetProperty::new(model, GROUND, body.ground))
            .then(SetProperty::new(model, POSE, pose.to_text()))
            .then(ReplaceComponent::new(layer, drawn, label)),
    )
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
    let buffers = render(pose, &body, &view, size);
    let drawn = crate::three::draw(&buffers, &view)?;
    Some(
        Edit::new(label)
            .then(SetProperty::new(model, POSE, pose.to_text()))
            .then(ReplaceComponent::new(layer, drawn, label)),
    )
}

/// [`set_pose`], also standing the figure at `position`, as one step:
/// what moving the whole body records.
#[must_use]
pub fn set_pose_placed(
    arena: &Arena,
    layer: EntityId,
    pose: &Pose,
    position: [f32; 3],
    label: &str,
) -> Option<Edit> {
    let model = of_layer(arena, layer)?;
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
    let mut body = Body::of(arena.get_by_id(model).ok()?);
    body.place.position = position;
    let buffers = render(pose, &body, &view, size);
    let drawn = crate::three::draw(&buffers, &view)?;
    let mut edit = Edit::new(label).then(SetProperty::new(model, POSE, pose.to_text()));
    for (key, value) in [
        (crate::three::OBJECT_X, position[0]),
        (crate::three::OBJECT_Y, position[1]),
        (crate::three::OBJECT_Z, position[2]),
    ] {
        edit = edit.then(SetProperty::new(model, key, f64::from(value)));
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
}

/// [`set_pose`], also turning the layer's camera to `view`, as one step.
#[must_use]
pub fn set_pose_seen(
    arena: &Arena,
    layer: EntityId,
    pose: &Pose,
    view: &crate::three::View3D,
    label: &str,
) -> Option<Edit> {
    let model = of_layer(arena, layer)?;
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let body = Body::of(arena.get_by_id(model).ok()?);
    let buffers = render(pose, &body, view, size);
    let drawn = crate::three::draw(&buffers, view)?;
    let mut edit = Edit::new(label).then(SetProperty::new(model, POSE, pose.to_text()));
    for set in view.edits(layer) {
        edit = edit.then(set);
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
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
    let buffers = render(&pose, body, &view, size);
    let drawn = crate::three::draw(&buffers, &view)?;
    let mut edit = Edit::new(label)
        .then(SetProperty::new(model, HEIGHT, f64::from(body.height)))
        .then(SetProperty::new(model, HEADS, f64::from(body.heads)))
        .then(SetProperty::new(model, PARTS_TEXT, body.parts_text()))
        .then(SetProperty::new(model, GUIDES, body.guides))
        .then(SetProperty::new(model, GROUND, body.ground));
    if body.kind == Kind::Hand {
        edit = edit.then(SetProperty::new(model, SIDE, body.side.key()));
    }
    if body.kind == Kind::Body {
        edit = edit
            .then(SetProperty::new(model, BUILD, body.build.key()))
            .then(SetProperty::new(model, SHAPE, body.shape.to_text()))
            .then(SetProperty::new(model, FOLLOW, body.follow));
    }
    for set in body.place.edits(model) {
        edit = edit.then(set);
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
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
                heads: 10.0,
                ..Body::default()
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

    #[test]
    fn hands_and_heads_pose_as_their_own_kinds() {
        // A new hand keeps its kind, and its pose has a turn for each of its
        // sixteen joints.
        let hand = new_of("Hand", Kind::Hand);
        let body = Body::of(&hand);
        assert_eq!(body.kind, Kind::Hand);
        assert_eq!(body.joints().len(), 16);
        assert_eq!(Pose::of(&hand).0.len(), 16);
        assert_eq!(body.presets().len(), HAND_PRESETS.len());
        // A fist curls the fingertips toward the palm (+z), below their
        // knuckles; an open hand's point up.
        let tip = |pose: &Pose, joint: usize| bones(pose, &body)[joint].1;
        let open = body.preset(0);
        let fist = body.preset(2);
        assert!(tip(&open, 6)[1] > 0.15 && tip(&open, 6)[2].abs() < 1e-3);
        assert!(
            tip(&fist, 6)[2] > 0.0 && tip(&fist, 5)[2] > 0.02,
            "curled toward the palm: {:?}",
            tip(&fist, 6)
        );
        assert!(tip(&fist, 6)[1] < tip(&open, 6)[1] - 0.05);
        // Pointing keeps the index finger straight.
        let pointing = body.preset(3);
        assert!((tip(&pointing, 3)[1] - tip(&open, 3)[1]).abs() < 1e-3);
        // A head: turning the neck takes the eyes round to the side.
        let head = new_of("Head", Kind::Head);
        let face = Body::of(&head);
        assert_eq!(face.kind, Kind::Head);
        let eye = |pose: &Pose| bones(pose, &face)[5].0;
        let front = eye(&face.preset(0));
        let turned = eye(&face.preset(4));
        assert!(front[2] > 0.06, "the eyes face forward: {front:?}");
        assert!(turned[2] < front[2] - 0.03, "turned away: {turned:?}");
        // Opening the mouth drops the chin.
        let chin = |pose: &Pose| bones(pose, &face)[2].1;
        assert!(chin(&face.preset(6))[1] < chin(&face.preset(0))[1] - 0.005);
        // Each kind frames itself, so the whole hand or head shows.
        assert!(body.frame().1 < FRAME.1 / 4.0);
        assert!(face.frame().1 < FRAME.1 / 3.0);
        // A body pose read for a hand stops at the hand's joints.
        let text = Pose(vec![[1.0, 2.0, 3.0]; 17]).to_text();
        assert_eq!(Pose::from_text_for(&text, 16).0.len(), 16);
        assert!(!geometry(&fist, &body).to_mesh().indices().is_empty());
    }

    #[test]
    fn body_types_are_built_their_own_way() {
        let male = Body::default();
        let female = Body {
            build: Build::Female,
            ..male
        };
        let rest = Pose::rest();
        let shoulder = |body: &Body| bones(&rest, body)[5].0[0];
        assert!(shoulder(&female) < shoulder(&male), "narrower shoulders");
        // The two are different meshes of the same points.
        let (a, b) = (surface(&rest, &male), surface(&rest, &female));
        assert_eq!(a.positions.len(), b.positions.len());
        assert_ne!(a.positions, b.positions);
        // Kept as a property; anything else reads as male.
        let model = new_model("Figure").with_property(BUILD, Value::from("female"));
        assert_eq!(Body::of(&model).build, Build::Female);
        assert_eq!(Build::from_key("nonsense"), Build::Male);
        for build in Build::ALL {
            assert_eq!(Build::from_key(build.key()), build);
        }
    }

    #[test]
    fn the_body_shape_chart_shapes_the_whole_body_and_each_part() {
        let plain = Body::default();
        let mut fat = plain;
        fat.shape.0[0] = [1.0, 0.0];
        let mut thin = plain;
        thin.shape.0[0] = [-1.0, 0.0];
        // The belly, at a little over half the height.
        let belly = |body: &Body| body.joints()[1].start[0];
        assert!(
            belly(&fat) > belly(&plain) * 1.1,
            "fat is wider: {} {}",
            belly(&fat),
            belly(&plain)
        );
        assert!(belly(&thin) < belly(&plain), "thin is narrower");
        // A part's own place on the chart shapes that part: stronger
        // arms are thicker arms, and the legs stay as they were.
        let mut strong_arms = plain;
        strong_arms.shape.0[3] = [0.0, 1.0];
        let arm = |body: &Body| body.joints()[5].start[0];
        let leg = |body: &Body| body.joints()[12].start[0];
        assert!(arm(&strong_arms) > arm(&plain) * 1.02, "an arm");
        assert!((leg(&strong_arms) - leg(&plain)).abs() < 1e-4, "not a leg");
        // As text, and read back clamped.
        let mut shape = Shape::default();
        shape.0[2] = [0.25, -0.5];
        assert_eq!(Shape::from_text(&shape.to_text()), shape);
        assert_eq!(Shape::from_text("v1;9,-9").0[0], [1.0, -1.0]);
        assert_eq!(Shape::from_text("nonsense"), Shape::default());
        assert_eq!(Shape::from_text("v1;NaN,x").0[0], [0.0, 0.0]);
        assert_eq!(PARTS.len(), 5);
        for joint in 0..17 {
            assert!(Shape::part_of(joint) > 0 && Shape::part_of(joint) < PARTS.len());
        }
    }

    #[test]
    fn every_slider_shapes_the_body_and_round_trips_as_text() {
        let plain = Body::default();
        let rest = Pose::rest();
        let still = surface(&rest, &plain).positions;
        for slider in &SLIDERS {
            let female = Body {
                build: Build::Female,
                ..plain
            };
            let base = if slider.key == "bust" { female } else { plain };
            let most = base.with_slider(slider.key, slider.range[1]);
            assert_eq!(most.slider(slider.key), slider.range[1]);
            let moved = surface(&rest, &most).positions;
            let from = if slider.key == "bust" {
                surface(&rest, &female).positions
            } else {
                still.clone()
            };
            let far = moved
                .iter()
                .zip(&from)
                .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max))
                .fold(0.0_f32, f32::max);
            // Veins are drawn in the light on a body (its hands are too
            // coarse to raise them), so they move no point there.
            assert!(
                far > 0.0005 || matches!(slider.key, "veins" | "vein_pattern"),
                "{} shapes the body: {far}",
                slider.key
            );
            // Past its range it stays at its most.
            assert_eq!(
                base.with_slider(slider.key, 99.0).slider(slider.key),
                slider.range[1]
            );
        }
        // Longer legs raise the hips of a body of the same height's
        // share; longer fingers reach further from the wrist.
        let long = plain.with_slider("finger_length", 1.0);
        let reach = |body: &Body| {
            let placed = bones(&rest, body);
            let (wrist, tip) = (placed[7].0, placed[26].1);
            (0..3)
                .map(|k| (tip[k] - wrist[k]).powi(2))
                .sum::<f32>()
                .sqrt()
        };
        assert!(reach(&long) > reach(&plain) * 1.03, "longer fingers");
        // As text on the model, and back; what is at rest is not written.
        let shaped = plain.with_slider("waist", -0.5).with_slider("veins", 1.0);
        assert_eq!(shaped.parts_text(), "waist=-0.5;veins=1");
        assert_eq!(Body::parts_from_text(&shaped.parts_text()), shaped.parts);
        assert_eq!(Body::parts_from_text("nonsense;waist=x"), plain.parts);
        let model = new_model("Figure").with_property(PARTS_TEXT, Value::from("hips=0.25"));
        assert_eq!(Body::of(&model).slider("hips"), 0.25);
        assert!(plain.parts_text().is_empty());
        // A body shape material carries them.
        let read = Body::default()
            .with_shape_text(&shaped.shape_text())
            .unwrap();
        assert_eq!(read.parts, shaped.parts);
    }

    #[test]
    fn faces_take_their_shape_expression_and_gaze() {
        let head = Body {
            kind: Kind::Head,
            ..Body::default()
        };
        let rest = head.standing();
        let plain = surface(&rest, &head);
        // A head has the sliders of the head and face, not the legs'.
        let own: Vec<&str> = SLIDERS
            .iter()
            .filter(|slider| slider.suits(Kind::Head))
            .map(|slider| slider.key)
            .collect();
        for key in ["head_width", "jaw", "eye_size", "nose_length", "smile"] {
            assert!(own.contains(&key), "{key}");
        }
        assert!(!own.contains(&"leg_length") && !own.contains(&"veins"));
        // Each of its sliders moves the head's own points.
        for slider in SLIDERS.iter().filter(|slider| slider.of_face()) {
            let shaped = surface(&rest, &head.with_slider(slider.key, slider.range[1]));
            let far = shaped
                .positions
                .iter()
                .zip(&plain.positions)
                // (Not the cut at the neck, whose points are not
                // numbered the same way each time.)
                .filter(|(a, b)| a[1] > 0.04 && b[1] > 0.04)
                .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max))
                .fold(0.0_f32, f32::max);
            assert!(far > 0.0003, "{} shapes the head: {far}", slider.key);
        }
        // An expression is worn whole and alone; the sliders mix them.
        assert_eq!(head.expression(), Some(0));
        let smiling = head.with_expression(1);
        assert_eq!(smiling.slider("smile"), 1.0);
        assert_eq!(smiling.expression(), Some(1));
        let sad = smiling.with_expression(4);
        assert_eq!((sad.slider("smile"), sad.slider("sad")), (0.0, 1.0));
        assert_eq!(sad.with_slider("smile", 0.5).expression(), None);
        assert_eq!(sad.with_expression(0).expression(), Some(0));
        // The face is kept with the body shape: as text on the model
        // and in a body shape material.
        let shaped = smiling.with_slider("jaw", -0.5);
        assert_eq!(Body::parts_from_text(&shaped.parts_text()), shaped.parts);
        let read = Body::default()
            .with_shape_text(&shaped.shape_text())
            .unwrap();
        assert_eq!((read.slider("jaw"), read.slider("smile")), (-0.5, 1.0));
        // The eyes have an iris and a pupil of their own color, and
        // turn where they are told to look, both together.
        assert_eq!(plain.colors.len(), plain.positions.len());
        let dark = plain.colors.iter().filter(|color| color[0] < 0.5).count();
        assert!(dark >= 80, "irises and pupils: {dark}");
        assert_eq!(gaze(&rest, &head), [0.0, 0.0]);
        let looking = with_gaze(&rest, &head, [20.0, 10.0]);
        assert_eq!(gaze(&looking, &head), [20.0, 10.0]);
        assert_eq!(looking.0[5], looking.0[6]);
        assert_eq!(
            with_gaze(&rest, &head, [90.0, -90.0]).0[5],
            [30.0, 35.0, 0.0]
        );
        let turned = surface(&looking, &head);
        let moved = turned
            .positions
            .iter()
            .zip(&plain.positions)
            .zip(&plain.colors)
            .filter(|((a, b), color)| color[0] < 0.5 && a != b)
            .count();
        assert_eq!(moved, dark, "the irises went with the eyes");
        // A body's eyes are its own joints; a hand has none.
        let body = Body::default();
        assert_eq!(body.eyes(), Some([52, 53]));
        let pose = with_gaze(&body.standing(), &body, [-15.0, 0.0]);
        assert_eq!(pose.0[52], [0.0, -15.0, 0.0]);
        assert!(
            Body {
                kind: Kind::Hand,
                ..body
            }
            .eyes()
            .is_none()
        );
    }

    #[test]
    fn veins_come_in_with_the_slider_and_have_patterns() {
        let hand = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        let pose = hand.standing();
        let bare = surface(&pose, &hand.with_slider("veins", 0.0)).positions;
        // How many points the veins raise, and how high the highest.
        let raised = |veins: f32, pattern: f32| {
            let body = hand
                .with_slider("veins", veins)
                .with_slider("vein_pattern", pattern);
            let lifts: Vec<f32> = surface(&pose, &body)
                .positions
                .iter()
                .zip(&bare)
                // (Not the cut at the forearm, whose points are not
                // numbered the same way each time.)
                .filter(|(a, b)| a[1] > -0.03 && b[1] > -0.03)
                .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt())
                .collect();
            (
                lifts.iter().filter(|lift| **lift > 0.0001).count(),
                lifts.iter().copied().fold(0.0_f32, f32::max),
            )
        };
        let (few, low) = raised(0.3, 0.0);
        let (some, _) = raised(0.65, 0.0);
        let (many, high) = raised(1.0, 0.0);
        // More veins show as the slider rises, rather than the same
        // ones standing ever higher: at most a millimeter.
        assert!(few > 50, "a few veins at rest: {few}");
        assert!(
            some > few * 5 / 4 && many > some * 11 / 10,
            "{few} {some} {many}"
        );
        assert!(low > 0.0002 && high < 0.0012, "{low} {high}");
        assert!(high < low * 3.5, "more veins, not cords: {low} {high}");
        // Another pattern is another hand's veins: other points rise.
        let veined = |pattern: f32| {
            surface(
                &pose,
                &hand
                    .with_slider("veins", 1.0)
                    .with_slider("vein_pattern", pattern),
            )
            .positions
        };
        let (first, other) = (veined(0.0), veined(0.6));
        let differ = first
            .iter()
            .zip(&other)
            .filter(|(a, b)| a[1] > -0.03 && b[1] > -0.03)
            .filter(|(a, b)| (0..3).any(|k| (a[k] - b[k]).abs() > 0.0001))
            .count();
        assert!(differ > 100, "the pattern moves the veins: {differ}");
        // A body's hands are too coarse to raise veins, so the light
        // shows them whole: its relief stands higher than the hand
        // figure's, which only sharpens veins its points already hold.
        let most = |body: &Body| {
            human::shaped(body).relief.as_ref().map_or(0.0, |relief| {
                relief.heights.iter().copied().fold(0.0, f32::max)
            })
        };
        let body = Body::default().with_slider("veins", 1.0);
        let figure = hand.with_slider("veins", 1.0);
        assert!(
            most(&body) > most(&figure) * 1.3,
            "{} {}",
            most(&body),
            most(&figure)
        );
        assert!(most(&body.with_slider("veins", 0.0)) < most(&body));
    }

    #[test]
    fn old_pose_texts_still_load_and_new_ones_round_trip() {
        // A pose kept before the body had fingers: seventeen joints.
        let old = "v1;0,0,0;0,0,0;0,0,0;0,0,0;12,0,-6;10,0,15;-90,0,0;0,0,0;-45,0,-10;\
                   -135,0,0;0,0,0;0,0,0;0,0,0;0,0,0;0,0,0;0,0,0;0,0,0";
        let model = new_model("Figure").with_property(POSE, Value::from(old));
        let pose = Pose::of(&model);
        assert_eq!(pose.0.len(), JOINTS.len(), "a turn for every joint now");
        assert_eq!(pose.0[..17], preset(6).0[..17], "the old turns, unchanged");
        assert!(pose.0[17..].iter().all(|turn| *turn == [0.0; 3]));
        // It poses the new body as it posed the old: the right hand
        // comes up to the chin.
        let body = Body::of(&model);
        let hand = bones(&pose, &body)[10].0;
        let rest = bones(&Pose::rest(), &body)[10].0;
        assert!(hand[1] > rest[1] + 0.3, "the hand is raised: {hand:?}");
        // The first seventeen joints keep their names and order.
        for (at, name) in ["Hips", "Spine", "Chest", "Neck", "Head", "Left upper arm"]
            .iter()
            .enumerate()
        {
            assert_eq!(JOINTS[at].name, *name);
        }
        assert_eq!(JOINTS[16].name, "Right foot");
        assert_eq!(JOINTS[21].name, "Left index finger base");
        assert_eq!(JOINTS[36].name, "Right index finger base");
        // An old hand pose: sixteen joints, as ever.
        let fist = hand_preset(2);
        assert_eq!(Pose::from_text_for(&fist.to_text(), HAND.len()), fist);
        // A whole new pose round trips, fingers and all.
        let full = Body::default().preset(15);
        assert_eq!(Pose::from_text(&full.to_text()), full);
        assert_eq!(full.flipped().flipped().0.len(), full.0.len());
        // Flipping takes the fist from the right hand to the left.
        let flipped = full.flipped();
        assert_eq!(flipped.0[21][0], full.0[36][0]);
    }

    #[test]
    fn the_skin_follows_its_joints_without_tearing() {
        let body = Body::default();
        let rest = surface(&Pose::rest(), &body);
        assert_eq!(rest.normals.len(), rest.positions.len());
        assert!(rest.indices.len() > 50_000, "a real mesh");
        // Bending the left elbow moves the forearm and hand, and leaves
        // the right arm and the legs exactly where they were.
        let mut bent = Pose::rest();
        bent.0[6] = [-90.0, 0.0, 0.0];
        let posed = surface(&bent, &body);
        let moved: Vec<usize> = (0..rest.positions.len())
            .filter(|at| {
                (0..3).any(|k| (rest.positions[*at][k] - posed.positions[*at][k]).abs() > 1e-5)
            })
            .collect();
        assert!(moved.len() > 500, "the forearm and hand moved");
        assert!(
            moved.iter().all(|at| rest.positions[*at][0] > 0.0),
            "only the left side"
        );
        // No triangle is pulled apart: every edge stays about as long as
        // it was, even at the elbow.
        let long = |surface: &crate::three::Surface, a: u32, b: u32| {
            let (a, b) = (surface.positions[a as usize], surface.positions[b as usize]);
            (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
        };
        for t in rest.indices.as_chunks::<3>().0 {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (was, now) = (long(&rest, a, b), long(&posed, a, b));
                assert!(now < was * 3.0 + 0.01, "an edge tore: {was} to {now}");
            }
        }
        // The hands carry relief and the body its guide lines; a body
        // told not to shows none.
        assert!(rest.relief.is_some() && rest.detail.iter().any(|d| d[2] > 0.5));
        assert!(rest.guides.len() > 100);
        let bare = Body {
            guides: false,
            ..body
        };
        assert!(surface(&Pose::rest(), &bare).guides.is_empty());
        // Veins and prominence change the hand's surface.
        let hand = Body {
            kind: Kind::Hand,
            ..body
        };
        let smooth = hand
            .with_slider("veins", 0.0)
            .with_slider("prominence", 0.0);
        let veined = hand
            .with_slider("veins", 1.0)
            .with_slider("prominence", 1.0);
        let open = hand.preset(0);
        let (a, b) = (surface(&open, &smooth), surface(&open, &veined));
        assert_eq!(a.positions.len(), b.positions.len());
        let raised = a
            .positions
            .iter()
            .zip(&b.positions)
            .filter(|(a, b)| (0..3).any(|k| (a[k] - b[k]).abs() > 0.0003))
            .count();
        assert!(raised > 200, "veins and knuckles stand out: {raised}");
        // A right hand is the left hand's mirror image.
        let right = Body {
            side: Side::Right,
            ..hand
        };
        let (l, r) = (surface(&open, &hand), surface(&open, &right));
        assert!((l.positions[100][0] + r.positions[100][0]).abs() < 1e-6);
        assert_eq!(l.positions[100][1], r.positions[100][1]);
    }

    #[test]
    fn the_head_to_body_ratio_can_follow_the_height() {
        assert!((Body::heads_for(Body::HEIGHT) - Body::HEADS).abs() < 1e-4);
        assert!((Body::heads_for(50.0) - 4.0).abs() < 1e-4);
        assert!(Body::heads_for(250.0) <= *Body::RATIOS.end());
        let free = Body::default().with_height(100.0);
        assert_eq!(free.heads, Body::HEADS, "free: the ratio stays");
        let following = Body {
            follow: true,
            ..Body::default()
        }
        .with_height(100.0);
        assert!(following.heads < 6.0, "a child: {}", following.heads);
        assert_eq!(Body::default().with_height(9999.0).height, 250.0);
        // Initialize keeps the kind and where it stands.
        let moved = Body {
            build: Build::Female,
            heads: 3.0,
            place: crate::three::Placement {
                position: [1.0, 0.0, 0.0],
                ..crate::three::Placement::default()
            },
            ..Body::default()
        };
        let fresh = moved.initialized();
        assert_eq!(fresh.build, Build::Male);
        assert_eq!(fresh.heads, Body::HEADS);
        assert_eq!(fresh.place, moved.place);
    }

    #[test]
    fn a_body_shape_round_trips_as_text() {
        let mut body = Body {
            build: Build::Female,
            height: 150.0,
            heads: 6.0,
            follow: true,
            ..Body::default()
        };
        body.shape.0[1] = [0.5, 0.25];
        let read = Body::default().with_shape_text(&body.shape_text()).unwrap();
        assert_eq!(read, body);
        assert!(Body::default().with_shape_text("nothing here").is_none());
        let partial = Body::default()
            .with_shape_text("height=999\nheads=x")
            .unwrap();
        assert_eq!(partial.height, 250.0);
        assert_eq!(partial.heads, Body::HEADS, "unreadable stays");
    }

    #[test]
    fn a_placed_figure_moves_turns_and_grows_whole() {
        let rest = Pose::rest();
        let still = Body::default();
        let moved = Body {
            place: crate::three::Placement {
                position: [0.5, 0.0, 0.0],
                turn: [90.0, 0.0, 0.0],
                scale: 2.0,
            },
            ..still
        };
        let a = bones(&rest, &still);
        let b = bones(&rest, &moved);
        // The hips stand twice as high and moved across.
        assert!((b[0].0[1] - a[0].0[1] * 2.0).abs() < 1e-4);
        assert!((b[0].0[0] - 0.5).abs() < 0.08);
        // Turned a quarter round: the left shoulder (+x) is now toward -z.
        assert!(b[5].0[2] < -0.2, "{:?}", b[5].0);
        // The framing stays, so a placed figure moves on the canvas.
        assert_eq!(moved.frame(), still.frame());
        // A posed joint still turns everything hanging from it.
        let mut raised = rest.clone();
        raised.0[5] = [0.0, 0.0, 90.0];
        assert_ne!(bones(&raised, &moved)[7], b[7]);
        assert_eq!(bones(&raised, &moved)[16], b[16]);
    }
}

#[cfg(test)]
mod preview {
    use super::*;
    use crate::three::View3D;
    use sue_core::{PixelFormat, RasterTile2D};

    /// Draws the figures to PNG files in the folder `SUE_FIGURE_PREVIEW`
    /// names, to look at while working on them. Run with `--ignored`.
    #[test]
    #[ignore = "writes pictures to look at"]
    fn figures_to_look_at() {
        let Ok(folder) = std::env::var("SUE_FIGURE_PREVIEW") else {
            return;
        };
        let save = |name: &str, body: &Body, pose: &Pose, view: &View3D, size: [u32; 2]| {
            let started = std::time::Instant::now();
            let buffers = render(pose, body, view, size);
            let took = started.elapsed();
            let tile = crate::three::draw(&buffers, view).unwrap();
            let mut pixels = tile.bytes().unwrap().to_vec();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                let alpha = f32::from(pixel[3]) / 255.0;
                for channel in pixel.iter_mut().take(3) {
                    *channel = (f32::from(*channel) * alpha + 250.0 * (1.0 - alpha)) as u8;
                }
                pixel[3] = 255;
            }
            let png = sue_io::encode_png(
                &RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels).unwrap(),
            )
            .unwrap();
            std::fs::write(format!("{folder}/{name}.png"), png).unwrap();
            println!("{name}: {took:?}");
        };
        let front = View3D {
            yaw: 25.0,
            pitch: 8.0,
            distance: 4.2,
            style: 0,
            floor: true,
            ..View3D::default()
        };
        let male = Body::default();
        let female = Body {
            build: Build::Female,
            ..male
        };
        save("male", &male, &male.standing(), &front, [700, 1000]);
        save("female", &female, &female.standing(), &front, [700, 1000]);
        save(
            "male-rest",
            &male,
            &Pose::rest(),
            &View3D { yaw: 0.0, ..front },
            [700, 1000],
        );
        for (index, name) in PRESETS.iter().enumerate().skip(1) {
            save(
                &format!("pose-{index:02}-{name}"),
                &male,
                &male.preset(index),
                &View3D {
                    floor: false,
                    ..front
                },
                [300, 420],
            );
        }
        let hand = Body {
            kind: Kind::Hand,
            ..male
        };
        let back = View3D {
            yaw: 160.0,
            pitch: 20.0,
            light: 200.0,
            distance: 2.6,
            style: 0,
            ..View3D::default()
        };
        save("hand-back", &hand, &hand.preset(0), &back, [700, 800]);
        let palm = View3D {
            yaw: 20.0,
            light: 45.0,
            ..back
        };
        for (index, name) in HAND_PRESETS.iter().enumerate() {
            save(
                &format!("hand-{index:02}-{name}"),
                &hand,
                &hand.preset(index),
                if index % 2 == 0 { &palm } else { &back },
                [300, 340],
            );
        }
        let head = Body {
            kind: Kind::Head,
            ..male
        };
        save("head", &head, &head.preset(1), &front, [700, 800]);
    }

    /// One picture of a sheet: the figure seen close, opaque on paper.
    fn tile(
        pose: &Pose,
        body: &Body,
        view: &View3D,
        frame: ([f32; 3], f32),
        size: [u32; 2],
    ) -> Vec<u8> {
        let buffers = crate::three::render_surface(&surface(pose, body), view, size, frame);
        let tile = crate::three::draw(&buffers, view).unwrap();
        let mut pixels = tile.bytes().unwrap().to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = f32::from(pixel[3]) / 255.0;
            for channel in pixel.iter_mut().take(3) {
                *channel = (f32::from(*channel) * alpha + 250.0 * (1.0 - alpha)) as u8;
            }
            pixel[3] = 255;
        }
        pixels
    }

    /// Tiles (each `size`) laid `across` a row and saved as one PNG.
    fn sheet(path: &str, tiles: &[Vec<u8>], size: [u32; 2], across: usize) {
        let rows = tiles.len().div_ceil(across);
        let (w, h) = (size[0] as usize * across, size[1] as usize * rows);
        let mut pixels = vec![250_u8; w * h * 4];
        for (index, tile) in tiles.iter().enumerate() {
            let (left, top) = (
                (index % across) * size[0] as usize,
                (index / across) * size[1] as usize,
            );
            for y in 0..size[1] as usize {
                let from = y * size[0] as usize * 4;
                let to = ((top + y) * w + left) * 4;
                pixels[to..to + size[0] as usize * 4]
                    .copy_from_slice(&tile[from..from + size[0] as usize * 4]);
            }
        }
        let png = sue_io::encode_png(
            &RasterTile2D::from_bytes(w as u32, h as u32, PixelFormat::Rgba8, pixels).unwrap(),
        )
        .unwrap();
        std::fs::write(path, png).unwrap();
    }

    /// What a joint sheet turns, which joint it looks at, from where
    /// (degrees round and above) and how close.
    type Case = (&'static [(usize, [f32; 3])], usize, f32, f32, f32);

    /// Every big joint at its limits, by part of the body.
    const CASES: [(&str, &[Case]); 4] = [
        (
            "arms",
            &[
                (&[(6, [-90.0, 0.0, 0.0])], 6, 90.0, 5.0, 0.4),
                (&[(6, [-150.0, 0.0, 0.0])], 6, 90.0, 5.0, 0.4),
                (&[(6, [-150.0, 0.0, 0.0])], 6, 20.0, 5.0, 0.4),
                (&[(6, [-150.0, 0.0, 0.0])], 6, 150.0, 5.0, 0.4),
                (&[(7, [85.0, 0.0, 0.0])], 7, 90.0, 5.0, 0.3),
                (&[(7, [-85.0, 0.0, 0.0])], 7, 90.0, 5.0, 0.3),
                (&[(7, [0.0, 0.0, 35.0])], 7, 90.0, 5.0, 0.3),
                (&[(7, [0.0, 25.0, 0.0])], 7, 60.0, 5.0, 0.3),
                (&[(6, [0.0, 90.0, 0.0])], 6, 60.0, 5.0, 0.5),
                (&[(6, [0.0, -90.0, 0.0])], 6, 60.0, 5.0, 0.5),
                (&[(5, [0.0, 90.0, 0.0])], 5, 60.0, 5.0, 0.6),
                (&[(5, [0.0, -90.0, 0.0])], 5, 60.0, 5.0, 0.6),
            ],
        ),
        (
            "shoulders",
            &[
                (&[(5, [0.0, 0.0, 90.0])], 5, 0.0, 10.0, 0.5),
                (&[(5, [0.0, 0.0, 90.0])], 5, 180.0, 10.0, 0.5),
                (&[(5, [0.0, 0.0, 170.0])], 5, 0.0, 10.0, 0.5),
                (&[(5, [0.0, 0.0, 170.0])], 5, 180.0, 10.0, 0.5),
                (&[(5, [-90.0, 0.0, 0.0])], 5, 45.0, 10.0, 0.5),
                (&[(5, [-90.0, 0.0, 0.0])], 5, 135.0, 10.0, 0.5),
                (&[(5, [-170.0, 0.0, 0.0])], 5, 60.0, 10.0, 0.5),
                (&[(5, [60.0, 0.0, 0.0])], 5, 150.0, 10.0, 0.5),
            ],
        ),
        (
            "legs",
            &[
                (&[(12, [90.0, 0.0, 0.0])], 12, 90.0, 5.0, 0.5),
                (&[(12, [150.0, 0.0, 0.0])], 12, 90.0, 5.0, 0.5),
                (&[(12, [150.0, 0.0, 0.0])], 12, 10.0, 5.0, 0.5),
                (&[(12, [150.0, 0.0, 0.0])], 12, 170.0, 5.0, 0.5),
                (&[(11, [-90.0, 0.0, 0.0])], 11, 90.0, 5.0, 0.6),
                (&[(11, [-130.0, 0.0, 0.0])], 11, 90.0, 5.0, 0.6),
                (&[(11, [-90.0, 0.0, 0.0])], 11, 180.0, 5.0, 0.6),
                (&[(11, [0.0, 0.0, 70.0])], 11, 0.0, 5.0, 0.6),
                (&[(11, [45.0, 0.0, 0.0])], 11, 110.0, 5.0, 0.6),
                (&[(11, [0.0, 50.0, 0.0])], 12, 20.0, 5.0, 0.9),
                (&[(13, [-50.0, 0.0, 0.0])], 13, 90.0, 5.0, 0.35),
                (&[(13, [40.0, 0.0, 0.0])], 13, 90.0, 5.0, 0.35),
            ],
        ),
        (
            "trunk",
            &[
                (
                    &[(1, [50.0, 0.0, 0.0]), (2, [40.0, 0.0, 0.0])],
                    1,
                    90.0,
                    5.0,
                    0.8,
                ),
                (
                    &[(1, [0.0, 35.0, 0.0]), (2, [0.0, 40.0, 0.0])],
                    1,
                    10.0,
                    5.0,
                    0.8,
                ),
                (
                    &[(3, [0.0, 60.0, 0.0]), (4, [0.0, 50.0, 0.0])],
                    3,
                    10.0,
                    5.0,
                    0.4,
                ),
                (
                    &[(3, [45.0, 0.0, 0.0]), (4, [40.0, 0.0, 0.0])],
                    3,
                    90.0,
                    5.0,
                    0.4,
                ),
            ],
        ),
    ];

    /// Contact sheets of every joint at its limits and of the preset
    /// poses, in the folder `SUE_FIGURE_PREVIEW` names. Run with
    /// `--ignored`.
    #[test]
    #[ignore = "writes pictures to look at"]
    fn joints_to_look_at() {
        let Ok(folder) = std::env::var("SUE_FIGURE_PREVIEW") else {
            return;
        };
        skin::PLAIN.with(|plain| plain.set(std::env::var("SUE_FIGURE_PLAIN").is_ok()));
        let body = Body {
            guides: std::env::var("SUE_FIGURE_GUIDES").is_ok(),
            ..Body::default()
        };
        let view = |yaw: f32, pitch: f32| View3D {
            yaw,
            pitch,
            distance: 4.2,
            style: 0,
            ..View3D::default()
        };
        let size = [480, 480];
        for (part, cases) in CASES {
            let mut tiles = Vec::new();
            for (turns, look, yaw, pitch, close) in cases {
                let mut pose = body.standing();
                for (joint, turn) in *turns {
                    pose.0[*joint] = *turn;
                }
                let at = solve(&pose, &body)[*look].0;
                tiles.push(tile(&pose, &body, &view(*yaw, *pitch), (at, *close), size));
            }
            sheet(
                &format!("{folder}/sheet-joints-{part}.png"),
                &tiles,
                size,
                4,
            );
        }
        // The fist, on the body and on the hand figure, from all round.
        let mut tiles = Vec::new();
        let mut pose = body.standing();
        pose.0[6] = [-90.0, 0.0, 0.0];
        for side in [Side::Left, Side::Right] {
            hand_onto_body(&body, &mut pose, side, &hand_pose(&body, 2));
        }
        let at = solve(&pose, &body)[24].0;
        for yaw in [0.0, 60.0, 120.0, 180.0, 240.0, 300.0] {
            tiles.push(tile(&pose, &body, &view(yaw, 15.0), (at, 0.16), size));
        }
        let hand = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        for index in [2, 6] {
            for yaw in [0.0, 60.0, 120.0, 180.0, 240.0, 300.0] {
                tiles.push(tile(
                    &hand.preset(index),
                    &hand,
                    &View3D {
                        light: yaw + 30.0,
                        ..view(yaw, 15.0)
                    },
                    ([0.0, 0.07, 0.0], 0.16),
                    size,
                ));
            }
        }
        sheet(&format!("{folder}/sheet-fist.png"), &tiles, size, 6);
        if let Ok(only) = std::env::var("SUE_FIGURE_ONLY") {
            let mut tiles = Vec::new();
            for index in only.split(',').filter_map(|n| n.parse::<usize>().ok()) {
                for yaw in [0.0, 60.0, 180.0, 300.0] {
                    let frame = ([0.0, 1.2, 0.0], 1.1);
                    tiles.push(tile(
                        &body.preset(index),
                        &body,
                        &view(yaw, 8.0),
                        frame,
                        [480, 480],
                    ));
                }
            }
            sheet(&format!("{folder}/sheet-only.png"), &tiles, [480, 480], 4);
            return;
        }
        let mut tiles = Vec::new();
        for index in 0..PRESETS.len() {
            tiles.push(tile(
                &body.preset(index),
                &body,
                &View3D {
                    distance: 3.2,
                    ..view(25.0, 8.0)
                },
                body.frame(),
                [300, 420],
            ));
        }
        sheet(
            &format!("{folder}/sheet-body-poses.png"),
            &tiles,
            [300, 420],
            7,
        );
        // The back of the hand across the veins slider, two patterns,
        // then lean hands, then a body's hand seen close.
        let mut tiles = Vec::new();
        let back = View3D {
            light: 215.0,
            distance: 2.6,
            ..view(165.0, 22.0)
        };
        for (pattern, prominence) in [(0.0, 0.35), (0.6, 0.35), (0.0, 1.0)] {
            for veins in [0.0, 0.3, 0.65, 1.0] {
                let shaped = hand
                    .with_slider("veins", veins)
                    .with_slider("vein_pattern", pattern)
                    .with_slider("prominence", prominence);
                tiles.push(tile(
                    &shaped.preset(0),
                    &shaped,
                    &back,
                    ([0.0, 0.06, 0.0], 0.24),
                    [480, 480],
                ));
            }
        }
        let veined = body.with_slider("veins", 1.0);
        let mut pose = veined.standing();
        pose.0[6] = [-90.0, 0.0, 0.0];
        let at = solve(&pose, &veined)[24].0;
        for (yaw, pitch) in [(60.0, 60.0), (100.0, 40.0), (20.0, 70.0), (140.0, 50.0)] {
            tiles.push(tile(
                &pose,
                &veined,
                &View3D {
                    light: 120.0,
                    ..view(yaw, pitch)
                },
                (at, 0.2),
                [480, 480],
            ));
        }
        sheet(&format!("{folder}/sheet-veins.png"), &tiles, [480, 480], 4);
        // The head: plain, its eyes turned, each expression, a few
        // shapes of the face.
        let head = Body {
            kind: Kind::Head,
            ..Body::default()
        };
        let looking = with_gaze(&head.standing(), &head, [25.0, 10.0]);
        let mut faces: Vec<(Body, Pose)> = vec![
            (head, head.standing()),
            (head, looking),
            (head.with_slider("eyes_closed", 1.0), head.standing()),
        ];
        for index in 1..EXPRESSIONS.len() {
            faces.push((head.with_expression(index), head.standing()));
        }
        for (key, value) in [
            ("anime", 1.0),
            ("age", 1.0),
            ("age", -1.0),
            ("face_age", 1.0),
            ("jaw", 1.0),
            ("chin", 1.0),
            ("cheeks", 1.0),
            ("eye_size", 1.0),
            ("eye_spacing", 1.0),
            ("nose_length", 1.0),
            ("nose_width", 1.0),
            ("mouth_width", 1.0),
            ("ear_size", 1.0),
        ] {
            faces.push((head.with_slider(key, value), head.standing()));
        }
        let tiles: Vec<Vec<u8>> = faces
            .iter()
            .map(|(body, pose)| {
                tile(
                    pose,
                    body,
                    &View3D {
                        distance: 3.0,
                        ..view(20.0, 5.0)
                    },
                    body.frame(),
                    [360, 400],
                )
            })
            .collect();
        sheet(&format!("{folder}/sheet-faces.png"), &tiles, [360, 400], 5);
        let mut tiles = Vec::new();
        for index in 0..HAND_PRESETS.len() {
            let yaw = if index % 2 == 0 { 20.0 } else { 160.0 };
            tiles.push(tile(
                &hand.preset(index),
                &hand,
                &View3D {
                    light: yaw + 30.0,
                    ..view(yaw, 20.0)
                },
                hand.frame(),
                [300, 340],
            ));
        }
        sheet(
            &format!("{folder}/sheet-hand-poses.png"),
            &tiles,
            [300, 340],
            8,
        );
    }
}
