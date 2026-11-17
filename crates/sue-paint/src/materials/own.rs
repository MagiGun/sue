//! Sue Paint's own materials: screentones, patterns, effect lines,
//! balloons, frame templates, poses and 3D backgrounds and objects.
//!
//! Every one is made here, by a formula or by building its shape: none
//! is drawn from or traced over another program's materials (rule 10 of
//! the 0.0.8 plan). Tones and patterns are ink a pixel, repeated over any
//! size; effect lines and balloons are drawn whole into a box, smooth at
//! every size, with their outline in the drawing color over white paper.

use sue_model::{Geometry, PrimKind};

use crate::figure::Kind;

/// How one of Sue Paint's own materials is made.
#[derive(Clone, Copy)]
pub(crate) enum Art {
    /// How much ink a pixel at `(x, y)` has, repeated over any size.
    Tiled(fn(f32, f32) -> f32),
    /// Drawn whole into a box `aspect` times as wide as it is high (a
    /// material that covers the canvas takes the canvas's shape). The
    /// function answers the ink and the white paper at `(x, y)`, where
    /// the box runs from -1 to 1 down and from `-aspect` to `aspect`
    /// across, and a pixel is `px` wide.
    Framed {
        aspect: f32,
        ink: fn(f32, f32, f32, f32) -> [f32; 2],
    },
    /// A preset pose of a drawing figure of this kind, by number.
    Pose(Kind, usize),
    /// A 3D model built of boxes and cylinders.
    Model(fn() -> Geometry),
    /// A frame border layer of this layout of [`crate::manga::LAYOUTS`].
    Frames(usize),
}

/// One of Sue Paint's own materials: its name (English, `localize` shows
/// it), its kind's id and how it is made.
pub(crate) type Own = (&'static str, &'static str, Art);

use super::{BACKGROUND, BALLOON, EFFECT, OBJECT, PATTERN, POSE, TEMPLATE, TONE};

/// Every one of Sue Paint's own materials. A name is saved data (the
/// library keeps favorites by it), so it is never changed.
pub(crate) static OWN: &[Own] = &[
    // Screentones.
    ("Dot 10%", TONE, Art::Tiled(|x, y| dots(x, y, 0.1, 8.0))),
    ("Dot 20%", TONE, Art::Tiled(|x, y| dots(x, y, 0.2, 8.0))),
    ("Dot 30%", TONE, Art::Tiled(|x, y| dots(x, y, 0.3, 8.0))),
    ("Dot 50%", TONE, Art::Tiled(|x, y| dots(x, y, 0.5, 8.0))),
    ("Lines 20%", TONE, Art::Tiled(|_, y| bar(y, 6.0, 1.2))),
    ("Sand", TONE, Art::Tiled(|x, y| step(noise(x, y), 0.82))),
    ("Dot 5%", TONE, Art::Tiled(|x, y| dots(x, y, 0.05, 8.0))),
    ("Dot 40%", TONE, Art::Tiled(|x, y| dots(x, y, 0.4, 8.0))),
    ("Dot 60%", TONE, Art::Tiled(|x, y| dots(x, y, 0.6, 8.0))),
    ("Dot 70%", TONE, Art::Tiled(|x, y| dots(x, y, 0.7, 8.0))),
    (
        "Fine dot 20%",
        TONE,
        Art::Tiled(|x, y| dots(x, y, 0.2, 5.0)),
    ),
    (
        "Coarse dot 20%",
        TONE,
        Art::Tiled(|x, y| dots(x, y, 0.2, 14.0)),
    ),
    ("Lines 10%", TONE, Art::Tiled(|_, y| bar(y, 8.0, 0.8))),
    ("Lines 40%", TONE, Art::Tiled(|_, y| bar(y, 5.0, 2.0))),
    (
        "Diagonal lines 20%",
        TONE,
        Art::Tiled(|x, y| bar((x + y) * std::f32::consts::FRAC_1_SQRT_2, 6.0, 1.2)),
    ),
    (
        "Upright lines 20%",
        TONE,
        Art::Tiled(|x, _| bar(x, 6.0, 1.2)),
    ),
    ("Noise 30%", TONE, Art::Tiled(|x, y| step(noise(x, y), 0.7))),
    (
        "Dot gradation",
        TONE,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| {
                let _ = (aspect, px);
                // Full at the top, empty at the bottom.
                let share = ((1.0 - y) / 2.0).clamp(0.0, 1.0) * 0.85;
                [dots(x / px, y / px, share, 8.0), 0.0]
            },
        },
    ),
    (
        "Line gradation",
        TONE,
        Art::Framed {
            aspect: 1.0,
            ink: |_, y, _, px| {
                let width = ((1.0 - y) / 2.0).clamp(0.0, 1.0) * 5.0;
                [bar(y / px, 6.0, width), 0.0]
            },
        },
    ),
    // Patterns.
    ("Crosshatch", PATTERN, Art::Tiled(crosshatch)),
    (
        "Grid",
        PATTERN,
        Art::Tiled(|x, y| bar(x, 16.0, 1.0).max(bar(y, 16.0, 1.0))),
    ),
    ("Checker", PATTERN, Art::Tiled(checker)),
    ("Bricks", PATTERN, Art::Tiled(bricks)),
    ("Stripes", PATTERN, Art::Tiled(|x, y| bar(x + y, 12.0, 4.0))),
    ("Speckles", PATTERN, Art::Tiled(|x, y| noise(x, y) * 0.6)),
    ("Fine crosshatch", PATTERN, Art::Tiled(fine_crosshatch)),
    ("Waves", PATTERN, Art::Tiled(waves)),
    ("Scales", PATTERN, Art::Tiled(scales)),
    ("Herringbone", PATTERN, Art::Tiled(herringbone)),
    ("Diamonds", PATTERN, Art::Tiled(diamonds)),
    (
        "Polka dots",
        PATTERN,
        Art::Tiled(|x, y| dots(x, y, 0.25, 24.0)),
    ),
    ("Wood grain", PATTERN, Art::Tiled(wood)),
    ("Tiles", PATTERN, Art::Tiled(tiles)),
    ("Basket weave", PATTERN, Art::Tiled(basket)),
    // Effect lines: they cover the canvas.
    (
        "Focus lines",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [focus(x, y, aspect, px, 180, 0.45), 0.0],
        },
    ),
    (
        "Dense focus lines",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [focus(x, y, aspect, px, 420, 0.35), 0.0],
        },
    ),
    (
        "Speed lines",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [speed(x, y, aspect, px), 0.0],
        },
    ),
    (
        "Falling speed lines",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [speed(y, x, 1.0 / aspect.max(0.01), px), 0.0],
        },
    ),
    (
        "Flash",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [flash(x, y, aspect, px), 0.0],
        },
    ),
    (
        "Gloom lines",
        EFFECT,
        Art::Framed {
            aspect: 1.0,
            ink: |x, y, aspect, px| [gloom(x, y, aspect, px), 0.0],
        },
    ),
    // Balloons: an outline in the drawing color round white paper.
    (
        "Round balloon",
        BALLOON,
        Art::Framed {
            aspect: 1.4,
            ink: |x, y, _, px| round_balloon(x, y, px),
        },
    ),
    (
        "Thought balloon",
        BALLOON,
        Art::Framed {
            aspect: 1.4,
            ink: |x, y, _, px| thought_balloon(x, y, px),
        },
    ),
    (
        "Shout balloon",
        BALLOON,
        Art::Framed {
            aspect: 1.4,
            ink: |x, y, _, px| shout_balloon(x, y, px),
        },
    ),
    (
        "Whisper balloon",
        BALLOON,
        Art::Framed {
            aspect: 1.4,
            ink: |x, y, _, px| whisper_balloon(x, y, px),
        },
    ),
    (
        "Wavy balloon",
        BALLOON,
        Art::Framed {
            aspect: 1.4,
            ink: |x, y, _, px| wavy_balloon(x, y, px),
        },
    ),
    (
        "Caption box",
        BALLOON,
        Art::Framed {
            aspect: 1.8,
            ink: |x, y, _, px| caption_box(x, y, px),
        },
    ),
    // Frame templates: a frame border layer of each layout.
    ("One panel", TEMPLATE, Art::Frames(0)),
    ("Two rows", TEMPLATE, Art::Frames(1)),
    ("Three rows", TEMPLATE, Art::Frames(2)),
    ("Four rows (4-koma)", TEMPLATE, Art::Frames(3)),
    ("Two by two", TEMPLATE, Art::Frames(4)),
    ("Manga page (2, 1, 2)", TEMPLATE, Art::Frames(5)),
    ("Two by three", TEMPLATE, Art::Frames(6)),
    // Poses of the drawing figures, as the 3D Tool Property lists them.
    ("Stand", POSE, Art::Pose(Kind::Body, 0)),
    ("T-pose", POSE, Art::Pose(Kind::Body, 1)),
    ("Walk", POSE, Art::Pose(Kind::Body, 2)),
    ("Run", POSE, Art::Pose(Kind::Body, 3)),
    ("Sit", POSE, Art::Pose(Kind::Body, 4)),
    ("Arms up", POSE, Art::Pose(Kind::Body, 5)),
    ("Think", POSE, Art::Pose(Kind::Body, 6)),
    ("Contrapposto", POSE, Art::Pose(Kind::Body, 7)),
    ("Hands on hips", POSE, Art::Pose(Kind::Body, 8)),
    ("Arms crossed", POSE, Art::Pose(Kind::Body, 9)),
    ("Wave", POSE, Art::Pose(Kind::Body, 10)),
    ("Reach forward", POSE, Art::Pose(Kind::Body, 11)),
    ("Crouch", POSE, Art::Pose(Kind::Body, 12)),
    ("Kneel", POSE, Art::Pose(Kind::Body, 13)),
    ("Jump", POSE, Art::Pose(Kind::Body, 14)),
    ("Punch", POSE, Art::Pose(Kind::Body, 15)),
    ("Kick", POSE, Art::Pose(Kind::Body, 16)),
    ("Sit on the floor", POSE, Art::Pose(Kind::Body, 17)),
    ("Bow", POSE, Art::Pose(Kind::Body, 18)),
    ("Look back", POSE, Art::Pose(Kind::Body, 19)),
    ("Open hand", POSE, Art::Pose(Kind::Hand, 0)),
    ("Relaxed", POSE, Art::Pose(Kind::Hand, 1)),
    ("Fist", POSE, Art::Pose(Kind::Hand, 2)),
    ("Pointing", POSE, Art::Pose(Kind::Hand, 3)),
    ("Peace sign", POSE, Art::Pose(Kind::Hand, 4)),
    ("Thumbs up", POSE, Art::Pose(Kind::Hand, 5)),
    ("Holding", POSE, Art::Pose(Kind::Hand, 6)),
    ("Spread", POSE, Art::Pose(Kind::Hand, 7)),
    ("Flat fold", POSE, Art::Pose(Kind::Hand, 8)),
    ("OK sign", POSE, Art::Pose(Kind::Hand, 9)),
    ("Pinch", POSE, Art::Pose(Kind::Hand, 10)),
    ("Claw", POSE, Art::Pose(Kind::Hand, 11)),
    ("Rock", POSE, Art::Pose(Kind::Hand, 12)),
    ("Three", POSE, Art::Pose(Kind::Hand, 13)),
    ("Call me", POSE, Art::Pose(Kind::Hand, 14)),
    ("Finger gun", POSE, Art::Pose(Kind::Hand, 15)),
    ("Look up", POSE, Art::Pose(Kind::Head, 2)),
    ("Look down", POSE, Art::Pose(Kind::Head, 3)),
    ("Turn left", POSE, Art::Pose(Kind::Head, 4)),
    ("Tilt", POSE, Art::Pose(Kind::Head, 5)),
    ("Open mouth", POSE, Art::Pose(Kind::Head, 6)),
    // 3D backgrounds and small objects, built of boxes.
    ("Room", BACKGROUND, Art::Model(room)),
    ("Street", BACKGROUND, Art::Model(street)),
    ("Corridor", BACKGROUND, Art::Model(corridor)),
    ("Stairs", BACKGROUND, Art::Model(stairs)),
    ("Rooftop", BACKGROUND, Art::Model(rooftop)),
    ("Chair", OBJECT, Art::Model(chair)),
    ("Desk", OBJECT, Art::Model(desk)),
    ("Table", OBJECT, Art::Model(table)),
    ("Bed", OBJECT, Art::Model(bed)),
    ("Bookshelf", OBJECT, Art::Model(bookshelf)),
    ("Door", OBJECT, Art::Model(door)),
    ("Street lamp", OBJECT, Art::Model(street_lamp)),
    ("Crate", OBJECT, Art::Model(crate_box)),
];

// Ink a pixel.

/// 1 at or past `edge`, else 0.
fn step(value: f32, edge: f32) -> f32 {
    if value > edge { 1.0 } else { 0.0 }
}

/// A line `width` pixels thick every `period` pixels along `t`, its edges
/// smoothed over a pixel.
fn bar(t: f32, period: f32, width: f32) -> f32 {
    let at = t.rem_euclid(period);
    let away = at.min(period - at);
    (width / 2.0 - away + 0.5).clamp(0.0, 1.0)
}

/// Halftone dots on a 45 degree grid of `cell` pixel cells covering
/// `share`.
fn dots(x: f32, y: f32, share: f32, cell: f32) -> f32 {
    let (s, c) = std::f32::consts::FRAC_PI_4.sin_cos();
    let (u, v) = ((x * c + y * s) / cell, (-x * s + y * c) / cell);
    let (du, dv) = (u - u.round(), v - v.round());
    let away = du.hypot(dv) * cell;
    let radius = cell * (share / std::f32::consts::PI).sqrt();
    (radius - away + 0.5).clamp(0.0, 1.0)
}

/// A pixel's own random value, 0 to 1, the same every time.
fn noise(x: f32, y: f32) -> f32 {
    hash2(x as i64 as u32, y as i64 as u32)
}

/// A random value 0 to 1 for the pair `(a, b)`.
fn hash2(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFF) as f32 / 255.0
}

/// A random value 0 to 1 for `n`.
fn hash1(n: i64) -> f32 {
    hash2(n as u32, (n >> 32) as u32 ^ 0x5bd1_e995)
}

fn crosshatch(x: f32, y: f32) -> f32 {
    bar(x + y, 8.0 * std::f32::consts::SQRT_2, 1.0).max(bar(
        x - y,
        8.0 * std::f32::consts::SQRT_2,
        1.0,
    ))
}

fn fine_crosshatch(x: f32, y: f32) -> f32 {
    bar(x + y, 5.0 * std::f32::consts::SQRT_2, 0.8).max(bar(
        x - y,
        5.0 * std::f32::consts::SQRT_2,
        0.8,
    ))
}

fn checker(x: f32, y: f32) -> f32 {
    if ((x / 16.0).floor() + (y / 16.0).floor()).rem_euclid(2.0) < 1.0 {
        1.0
    } else {
        0.0
    }
}

fn bricks(x: f32, y: f32) -> f32 {
    let row = (y / 12.0).floor();
    let shift = if row.rem_euclid(2.0) < 1.0 { 0.0 } else { 12.0 };
    bar(y, 12.0, 1.5).max(bar(x + shift, 24.0, 1.5))
}

fn waves(x: f32, y: f32) -> f32 {
    let wave = (x / 24.0 * std::f32::consts::TAU).sin() * 3.0;
    bar(y + wave, 10.0, 1.4)
}

/// Rows of half circles, each row shifted half a scale.
fn scales(x: f32, y: f32) -> f32 {
    let size = 16.0;
    let row = (y / (size / 2.0)).floor();
    let shift = if row.rem_euclid(2.0) < 1.0 {
        0.0
    } else {
        size / 2.0
    };
    let cx = ((x + shift) / size).round() * size - shift;
    let cy = (row + 1.0) * (size / 2.0);
    let away = (x - cx).hypot(y - cy);
    (0.7 - (away - size / 2.0).abs()).clamp(0.0, 1.0)
}

fn herringbone(x: f32, y: f32) -> f32 {
    let column = (x / 12.0).floor();
    let slope = if column.rem_euclid(2.0) < 1.0 {
        1.0
    } else {
        -1.0
    };
    let line = bar(y + slope * x, 8.0, 1.2);
    line.max(bar(x, 12.0, 1.0))
}

fn diamonds(x: f32, y: f32) -> f32 {
    let (u, v) = (
        (x + y).rem_euclid(20.0) - 10.0,
        (x - y).rem_euclid(20.0) - 10.0,
    );
    if u.abs() + v.abs() < 7.0 { 1.0 } else { 0.0 }
}

/// Wavy rings round a far centre, like the grain of a plank.
fn wood(x: f32, y: f32) -> f32 {
    let wobble = (y / 37.0).sin() * 6.0 + (y / 11.0).sin() * 1.5;
    let ring = (x + wobble + (y * 0.02).sin() * 20.0) / 9.0;
    let at = ring - ring.floor();
    let width = 0.12 + 0.1 * hash1(ring.floor() as i64);
    if at < width { 1.0 } else { 0.0 }
}

fn tiles(x: f32, y: f32) -> f32 {
    let line = bar(x, 20.0, 1.2).max(bar(y, 20.0, 1.2));
    // A small square at every crossing.
    let (dx, dy) = (
        (x.rem_euclid(20.0) - 10.0).abs(),
        (y.rem_euclid(20.0) - 10.0).abs(),
    );
    line.max(if dx > 7.5 && dy > 7.5 { 1.0 } else { 0.0 })
}

/// Squares of three lines, upright and lying in turn.
fn basket(x: f32, y: f32) -> f32 {
    let size = 18.0;
    let (cx, cy) = ((x / size).floor(), (y / size).floor());
    let across = (cx + cy).rem_euclid(2.0) < 1.0;
    let t = if across { y } else { x };
    let border = bar(x, size, 0.9).max(bar(y, size, 0.9));
    bar(t, size / 3.0, 1.2).max(border)
}

// Drawn whole: shapes by their signed distance (inside below zero).

/// How much of a pixel `px` wide a shape covers at signed distance `d`.
fn cover(d: f32, px: f32) -> f32 {
    (0.5 - d / px.max(1e-4)).clamp(0.0, 1.0)
}

/// A line `width` thick along the edge of a shape at signed distance `d`.
fn outline(d: f32, width: f32, px: f32) -> f32 {
    cover(d.abs() - width / 2.0, px)
}

/// The signed distance to an ellipse of half axes `a` and `b` round the
/// middle (close to the true distance, enough for an outline).
fn ellipse(x: f32, y: f32, a: f32, b: f32) -> f32 {
    let k = (x / a).hypot(y / b);
    if k < 1e-6 {
        return -a.min(b);
    }
    // Scaled by how fast k grows there, so the line stays even round it.
    let grad = ((x / (a * a)).hypot(y / (b * b))) / k;
    (k - 1.0) / grad.max(1e-6)
}

/// The signed distance to the polygon of `points` (any shape, in order).
fn polygon(x: f32, y: f32, points: &[[f32; 2]]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    let n = points.len();
    for i in 0..n {
        let a = points[i];
        let b = points[(i + n - 1) % n];
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let (wx, wy) = (x - a[0], y - a[1]);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(0.0, 1.0);
        d = d.min((wx - ex * t).hypot(wy - ey * t));
        let crosses = (a[1] > y) != (b[1] > y);
        if crosses && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    if inside { -d } else { d }
}

/// The ink and paper of a shape at signed distance `d`, outlined `width`
/// thick.
fn outlined(d: f32, width: f32, px: f32) -> [f32; 2] {
    [outline(d, width, px), cover(d, px)]
}

/// The usual line of a balloon.
const LINE: f32 = 0.035;

/// An oval with a tail down to the lower left.
fn round_balloon(x: f32, y: f32, px: f32) -> [f32; 2] {
    let body = ellipse(x, y + 0.12, 1.25, 0.78);
    let tail = polygon(x, y, &[[-0.55, 0.35], [-0.2, 0.45], [-0.85, 0.98]]);
    outlined(body.min(tail), LINE, px)
}

/// Bumps round an oval, and three bubbles trailing off.
fn thought_balloon(x: f32, y: f32, px: f32) -> [f32; 2] {
    let (a, b) = (1.05, 0.6);
    let mut d = ellipse(x, y + 0.15, a, b);
    let bumps = 14;
    for i in 0..bumps {
        let angle = i as f32 / bumps as f32 * std::f32::consts::TAU;
        let (cx, cy) = (angle.cos() * a, angle.sin() * b - 0.15);
        d = d.min((x - cx).hypot(y - cy) - 0.2);
    }
    let mut ink = outline(d, LINE, px);
    let mut paper = cover(d, px);
    for (cx, cy, r) in [(-0.75, 0.62, 0.11), (-0.95, 0.8, 0.075), (-1.1, 0.93, 0.05)] {
        let bubble = (x - cx).hypot(y - cy) - r;
        ink = ink.max(outline(bubble, LINE * 0.8, px));
        paper = paper.max(cover(bubble, px));
    }
    [ink, paper]
}

/// A star of uneven spikes round an oval: a shout.
fn shout_balloon(x: f32, y: f32, px: f32) -> [f32; 2] {
    let spikes = 18;
    let mut points = Vec::with_capacity(spikes * 2);
    for i in 0..spikes * 2 {
        let angle = i as f32 / (spikes * 2) as f32 * std::f32::consts::TAU;
        let out = if i % 2 == 0 {
            0.98 + 0.25 * hash1(i as i64 * 7 + 3)
        } else {
            0.72
        };
        points.push([angle.cos() * out * 1.22, angle.sin() * out * 0.8]);
    }
    outlined(polygon(x, y, &points), LINE, px)
}

/// An oval whose outline is a dashed line: a whisper.
fn whisper_balloon(x: f32, y: f32, px: f32) -> [f32; 2] {
    let d = ellipse(x, y + 0.05, 1.25, 0.85);
    let angle = (y + 0.05).atan2(x / 1.25 * 0.85);
    let dash = (angle / std::f32::consts::TAU * 36.0).rem_euclid(1.0) < 0.55;
    let ink = if dash {
        outline(d, LINE * 0.8, px)
    } else {
        0.0
    };
    [ink, cover(d, px)]
}

/// An oval whose edge waves: a shaky voice.
fn wavy_balloon(x: f32, y: f32, px: f32) -> [f32; 2] {
    let angle = y.atan2(x);
    let wave = 1.0 + 0.045 * (angle * 16.0).sin();
    let d = ellipse(x / wave, y / wave, 1.2, 0.82);
    outlined(d, LINE, px)
}

/// A box for narration.
fn caption_box(x: f32, y: f32, px: f32) -> [f32; 2] {
    let d = polygon(x, y, &[[-1.6, -0.7], [1.6, -0.7], [1.6, 0.7], [-1.6, 0.7]]);
    outlined(d, LINE * 0.8, px)
}

/// Lines converging on the middle: `count` of them round, none inside
/// an oval `clear` of the box's height across. Each starts outside the
/// box and runs in to a point.
fn focus(x: f32, y: f32, aspect: f32, px: f32, count: i64, clear: f32) -> f32 {
    let r = x.hypot(y);
    let angle = y.atan2(x);
    let slice = std::f32::consts::TAU / count as f32;
    let index = (angle / slice).round() as i64;
    let mut ink = 0.0_f32;
    // This line and its two neighbours, whose taper may reach here.
    for k in index - 1..=index + 1 {
        let wrapped = k.rem_euclid(count);
        let jitter = (hash1(wrapped * 31 + 7) - 0.5) * slice * 0.8;
        let line_angle = k as f32 * slice + jitter;
        // Where it ends inside: the clear oval, pushed out a little more
        // for some lines.
        let (c, s) = (line_angle.cos(), line_angle.sin());
        let oval = 1.0 / ((c / (clear * aspect)).hypot(s / clear)).max(1e-3);
        let inner = oval * (1.0 + 0.5 * hash1(wrapped * 17 + 1));
        let outer = aspect.hypot(1.0) * 1.05;
        if r <= inner {
            continue;
        }
        let t = ((r - inner) / (outer - inner)).clamp(0.0, 1.0);
        let width = (0.004 + 0.012 * hash1(wrapped * 13 + 5)) * t.sqrt() * 2.0;
        let off = (angle - line_angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let distance = r * off.abs();
        ink = ink.max(cover(distance - width / 2.0, px));
    }
    ink
}

/// Streaks across the box, each tapered at both ends, as speed shows.
fn speed(x: f32, y: f32, aspect: f32, px: f32) -> f32 {
    let spacing = 0.018;
    let row = (y / spacing).floor() as i64;
    let mut ink = 0.0_f32;
    for k in row - 1..=row + 1 {
        if hash1(k * 5 + 11) < 0.35 {
            continue;
        }
        let centre = (k as f32 + 0.5) * spacing + (hash1(k * 7 + 3) - 0.5) * spacing * 0.6;
        let length = (0.6 + 1.2 * hash1(k * 11 + 2)) * aspect;
        let start = -aspect - 0.1 + hash1(k * 13 + 9) * (2.0 * aspect + 0.2 - length * 0.5);
        let t = ((x - start) / length).clamp(0.0, 1.0);
        if x < start || x > start + length {
            continue;
        }
        let taper = (t * (1.0 - t) * 4.0).min(1.0);
        let width = (0.002 + 0.006 * hash1(k * 17 + 4)) * taper;
        ink = ink.max(cover((y - centre).abs() - width / 2.0, px));
    }
    ink
}

/// Black round the edges with white spikes bursting out of the middle.
fn flash(x: f32, y: f32, aspect: f32, px: f32) -> f32 {
    let r = x.hypot(y);
    let angle = y.atan2(x);
    let spikes = 64;
    let slice = std::f32::consts::TAU / spikes as f32;
    let index = (angle / slice).floor() as i64;
    let within = (angle / slice) - index as f32;
    let reach = |k: i64| 0.45 + 0.75 * hash1(k.rem_euclid(spikes) * 19 + 3);
    // Each spike rises to its tip in its middle.
    let tip = reach(index);
    let edge = 0.38 + (tip - 0.38) * (1.0 - (within - 0.5).abs() * 2.0);
    // The spikes keep to the box's shorter side.
    cover(edge * aspect.min(1.0) - r, px * 1.5)
}

/// Upright lines hanging from the top, each fading out lower down: gloom.
fn gloom(x: f32, y: f32, aspect: f32, px: f32) -> f32 {
    let _ = aspect;
    let spacing = 0.03;
    let column = (x / spacing).floor() as i64;
    let mut ink = 0.0_f32;
    for k in column - 1..=column + 1 {
        let centre = (k as f32 + 0.5) * spacing + (hash1(k * 3 + 1) - 0.5) * spacing * 0.5;
        let reach = -1.0 + 0.5 + 0.9 * hash1(k * 23 + 6);
        if y > reach {
            continue;
        }
        let t = ((reach - y) / (reach + 1.0)).clamp(0.0, 1.0);
        let width = (0.003 + 0.004 * hash1(k * 29 + 8)) * t;
        ink = ink.max(cover((x - centre).abs() - width / 2.0, px));
    }
    ink
}

// 3D models of boxes.

/// How a person looks into a 3D background: standing at its open end
/// (where `z` is greatest), eyes 1.6 m above the ground, looking straight
/// along it through a lens `fov` degrees high, so the canvas shows the
/// place from inside, in one point perspective, as a background is drawn.
pub(crate) fn eye_view(geometry: &Geometry, fov: f32) -> crate::three::View3D {
    let mesh = geometry.to_mesh();
    let (middle, extent) = crate::three::frame_of(&mesh);
    let scale = 2.0 / extent.max(1e-6);
    let front = mesh
        .positions()
        .iter()
        .map(|p| p[2])
        .fold(f32::NEG_INFINITY, f32::max);
    // Just in front of the nearest wall's edge, so nothing of the place
    // is behind the eye.
    let distance = (front - middle[2]) * scale + 0.08;
    crate::three::View3D {
        yaw: 0.0,
        pitch: 0.0,
        distance,
        fov,
        pan: [0.0, (1.6 - middle[1]) * scale],
        ..crate::three::View3D::default()
    }
}

/// Adds a box from `min` to `max`, its faces wound outwards.
fn cuboid(geometry: &mut Geometry, min: [f64; 3], max: [f64; 3]) {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let faces = [
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
        [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
    ];
    for face in faces {
        let ids: Vec<u32> = face
            .iter()
            .map(|point| geometry.add_point(*point))
            .collect();
        let _ = geometry.add_prim(PrimKind::Polygon, &ids);
    }
}

/// Adds an upright cylinder of `radius` round `(x, z)` from `y0` to `y1`.
fn post(geometry: &mut Geometry, x: f64, z: f64, radius: f64, y0: f64, y1: f64) {
    let sides = 16;
    let at = |i: usize| {
        let theta = i as f64 / sides as f64 * std::f64::consts::TAU;
        (x + theta.cos() * radius, z + theta.sin() * radius)
    };
    for i in 0..sides {
        let (ax, az) = at(i);
        let (bx, bz) = at(i + 1);
        let ids: Vec<u32> = [[ax, y0, az], [bx, y0, bz], [bx, y1, bz], [ax, y1, az]]
            .iter()
            .map(|point| geometry.add_point(*point))
            .collect();
        let _ = geometry.add_prim(PrimKind::Polygon, &ids);
    }
    let cap: Vec<u32> = (0..sides)
        .map(|i| {
            let (px, pz) = at(i);
            geometry.add_point([px, y1, pz])
        })
        .collect();
    let _ = geometry.add_prim(PrimKind::Polygon, &cap);
}

/// A room seen from where its fourth wall would be: floor, three walls,
/// a ceiling, a window and a door.
fn room() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-4.0, -0.1, -4.0], [4.0, 0.0, 4.0]);
    cuboid(&mut g, [-4.0, 0.0, -4.1], [4.0, 3.0, -4.0]);
    cuboid(&mut g, [-4.1, 0.0, -4.0], [-4.0, 3.0, 4.0]);
    cuboid(&mut g, [4.0, 0.0, -4.0], [4.1, 3.0, 4.0]);
    // A window in the back wall: its frame and its cross.
    cuboid(&mut g, [-1.6, 1.0, -3.98], [1.6, 1.08, -3.9]);
    cuboid(&mut g, [-1.6, 2.4, -3.98], [1.6, 2.48, -3.9]);
    cuboid(&mut g, [-1.6, 1.0, -3.98], [-1.52, 2.48, -3.9]);
    cuboid(&mut g, [1.52, 1.0, -3.98], [1.6, 2.48, -3.9]);
    cuboid(&mut g, [-0.04, 1.0, -3.98], [0.04, 2.48, -3.9]);
    // A door in the left wall.
    cuboid(&mut g, [-3.98, 0.0, 1.0], [-3.9, 2.2, 2.0]);
    // The ceiling.
    cuboid(&mut g, [-4.1, 3.0, -4.1], [4.1, 3.1, 4.0]);
    g
}

/// A road between pavements, with buildings of different heights on
/// each side.
fn street() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-3.0, -0.1, -20.0], [3.0, 0.0, 4.0]);
    for side in [-1.0, 1.0] {
        cuboid(
            &mut g,
            [side * 3.0 - if side < 0.0 { 2.0 } else { 0.0 }, 0.0, -20.0],
            [side * 3.0 + if side > 0.0 { 2.0 } else { 0.0 }, 0.15, 4.0],
        );
        let mut z = 4.0;
        let mut n = 0;
        while z > -20.0 {
            let depth = 3.0 + 2.0 * f64::from(hash1(n * 3 + if side < 0.0 { 1 } else { 2 }));
            let height = 4.0 + 8.0 * f64::from(hash1(n * 5 + if side < 0.0 { 7 } else { 9 }));
            let (x0, x1) = if side < 0.0 {
                (-11.0, -5.0)
            } else {
                (5.0, 11.0)
            };
            cuboid(&mut g, [x0, 0.0, z - depth + 0.3], [x1, height, z]);
            z -= depth;
            n += 1;
        }
    }
    g
}

/// A long hall with doors on both sides.
fn corridor() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-1.5, -0.1, -16.0], [1.5, 0.0, 2.0]);
    cuboid(&mut g, [-1.6, 0.0, -16.0], [-1.5, 2.8, 2.0]);
    cuboid(&mut g, [1.5, 0.0, -16.0], [1.6, 2.8, 2.0]);
    cuboid(&mut g, [-1.5, 0.0, -16.1], [1.5, 2.8, -16.0]);
    // The ceiling.
    cuboid(&mut g, [-1.6, 2.8, -16.1], [1.6, 2.9, 2.0]);
    let mut z = 0.0;
    while z > -15.0 {
        cuboid(&mut g, [-1.5, 0.0, z - 1.0], [-1.44, 2.1, z]);
        cuboid(&mut g, [1.44, 0.0, z - 3.0], [1.5, 2.1, z - 2.0]);
        z -= 4.0;
    }
    g
}

/// A flight of stairs with a wall at one side.
fn stairs() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-2.0, -0.1, -1.0], [2.0, 0.0, 3.0]);
    for step in 0..12 {
        let s = f64::from(step);
        cuboid(
            &mut g,
            [-1.0, 0.0, -1.0 - s * 0.3],
            [1.0, 0.18 * (s + 1.0), -1.3 - s * 0.3],
        );
    }
    cuboid(&mut g, [-1.1, 0.0, -5.0], [-1.0, 3.4, 3.0]);
    g
}

/// A flat roof with a fence of posts and rails round three sides.
fn rooftop() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-5.0, -0.2, -5.0], [5.0, 0.0, 5.0]);
    let mut at = -5.0;
    while at <= 5.0 {
        post(&mut g, at, -5.0, 0.04, 0.0, 1.2);
        post(&mut g, -5.0, at, 0.04, 0.0, 1.2);
        post(&mut g, 5.0, at, 0.04, 0.0, 1.2);
        at += 1.0;
    }
    for height in [0.6, 1.2] {
        cuboid(&mut g, [-5.0, height, -5.03], [5.0, height + 0.05, -4.97]);
        cuboid(&mut g, [-5.03, height, -5.0], [-4.97, height + 0.05, 5.0]);
        cuboid(&mut g, [4.97, height, -5.0], [5.03, height + 0.05, 5.0]);
    }
    // A stair house in a corner.
    cuboid(&mut g, [2.5, 0.0, -4.5], [4.5, 2.6, -2.0]);
    g
}

fn chair() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.22, 0.42, -0.22], [0.22, 0.46, 0.22]);
    for (x, z) in [(-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)] {
        cuboid(
            &mut g,
            [x - 0.02, 0.0, z - 0.02],
            [x + 0.02, 0.42, z + 0.02],
        );
    }
    cuboid(&mut g, [-0.22, 0.46, -0.22], [0.22, 0.9, -0.19]);
    g
}

fn desk() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.6, 0.7, -0.3], [0.6, 0.74, 0.3]);
    for x in [-0.57, 0.57] {
        cuboid(&mut g, [x - 0.02, 0.0, -0.28], [x + 0.02, 0.7, 0.28]);
    }
    cuboid(&mut g, [0.1, 0.5, -0.28], [0.55, 0.7, 0.28]);
    g
}

fn table() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.9, 0.72, -0.5], [0.9, 0.76, 0.5]);
    for (x, z) in [(-0.85, -0.45), (0.85, -0.45), (-0.85, 0.45), (0.85, 0.45)] {
        cuboid(
            &mut g,
            [x - 0.03, 0.0, z - 0.03],
            [x + 0.03, 0.72, z + 0.03],
        );
    }
    g
}

fn bed() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.5, 0.2, -1.0], [0.5, 0.45, 1.0]);
    cuboid(&mut g, [-0.5, 0.0, -1.05], [0.5, 0.9, -1.0]);
    cuboid(&mut g, [-0.35, 0.45, -0.95], [0.35, 0.55, -0.65]);
    for (x, z) in [(-0.48, 0.98), (0.48, 0.98)] {
        cuboid(&mut g, [x - 0.02, 0.0, z - 0.02], [x + 0.02, 0.2, z + 0.02]);
    }
    g
}

fn bookshelf() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.5, 0.0, -0.18], [-0.47, 1.8, 0.18]);
    cuboid(&mut g, [0.47, 0.0, -0.18], [0.5, 1.8, 0.18]);
    cuboid(&mut g, [-0.5, 0.0, -0.18], [0.5, 1.8, -0.16]);
    for shelf in 0..5 {
        let y = f64::from(shelf) * 0.44;
        cuboid(&mut g, [-0.47, y, -0.16], [0.47, y + 0.03, 0.18]);
        // Books of different widths and heights.
        let mut x = -0.45;
        let mut n = shelf * 17;
        while x < 0.4 && shelf < 4 {
            let width = 0.03 + 0.03 * f64::from(hash1(i64::from(n)));
            let height = 0.25 + 0.12 * f64::from(hash1(i64::from(n) + 99));
            cuboid(
                &mut g,
                [x, y + 0.03, -0.14],
                [x + width, y + 0.03 + height, 0.12],
            );
            x += width + 0.005;
            n += 1;
        }
    }
    g
}

fn door() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.45, 0.0, -0.02], [0.45, 2.05, 0.02]);
    cuboid(&mut g, [-0.5, 0.0, -0.05], [-0.45, 2.1, 0.05]);
    cuboid(&mut g, [0.45, 0.0, -0.05], [0.5, 2.1, 0.05]);
    cuboid(&mut g, [-0.5, 2.05, -0.05], [0.5, 2.1, 0.05]);
    cuboid(&mut g, [0.3, 1.0, 0.02], [0.38, 1.04, 0.06]);
    g
}

fn street_lamp() -> Geometry {
    let mut g = Geometry::new();
    post(&mut g, 0.0, 0.0, 0.12, 0.0, 0.3);
    post(&mut g, 0.0, 0.0, 0.05, 0.3, 3.8);
    cuboid(&mut g, [-0.03, 3.75, -0.03], [0.6, 3.8, 0.03]);
    cuboid(&mut g, [0.45, 3.6, -0.12], [0.75, 3.75, 0.12]);
    g
}

fn crate_box() -> Geometry {
    let mut g = Geometry::new();
    cuboid(&mut g, [-0.4, 0.0, -0.4], [0.4, 0.8, 0.4]);
    // Planks round its faces.
    for y in [0.05, 0.72] {
        cuboid(&mut g, [-0.42, y, -0.42], [0.42, y + 0.06, 0.42]);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_every_kind_has_some() {
        let mut names: Vec<&str> = OWN.iter().map(|(name, ..)| *name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "no two share a name");
        for kind in [
            TONE, PATTERN, EFFECT, BALLOON, TEMPLATE, POSE, BACKGROUND, OBJECT,
        ] {
            assert!(
                OWN.iter().filter(|(_, of, _)| *of == kind).count() >= 5,
                "{kind} has a library"
            );
        }
    }

    #[test]
    fn a_balloon_is_white_inside_and_outlined() {
        let px = 0.01;
        // The middle is paper with no ink; the edge carries the line.
        let [ink, paper] = round_balloon(0.0, -0.1, px);
        assert!(ink < 0.01 && paper > 0.99);
        let [ink, _] = round_balloon(1.25, -0.12, px);
        assert!(ink > 0.5, "{ink}");
        // Far outside there is nothing.
        assert_eq!(round_balloon(1.39, -0.99, px), [0.0, 0.0]);
    }

    #[test]
    fn focus_lines_leave_the_middle_clear_and_fill_the_edge() {
        let px = 0.004;
        let mut middle = 0.0;
        let mut edge = 0.0;
        for i in 0..400 {
            let angle = i as f32 / 400.0 * std::f32::consts::TAU;
            middle += focus(angle.cos() * 0.2, angle.sin() * 0.2, 1.0, px, 180, 0.45);
            edge += focus(angle.cos() * 1.3, angle.sin() * 1.3, 1.0, px, 180, 0.45);
        }
        assert!(middle < 1.0, "{middle}");
        assert!(edge > 40.0, "{edge}");
    }

    #[test]
    fn models_have_faces_and_poses_are_the_presets() {
        for (name, _, art) in OWN {
            match art {
                Art::Model(build) => assert!(!build().to_mesh().indices().is_empty(), "{name}"),
                Art::Pose(kind, index) => {
                    let presets = crate::figure::Body {
                        kind: *kind,
                        ..crate::figure::Body::default()
                    }
                    .presets();
                    assert_eq!(presets[*index], *name);
                }
                Art::Frames(index) => assert_eq!(crate::manga::LAYOUTS[*index].0, *name),
                _ => {}
            }
        }
    }
}
