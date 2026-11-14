//! The Blend tool (Clip Studio Paint's J): brushes that rework the pixels
//! already there instead of adding paint. Blur softens, Finger Tip drags
//! color along the stroke, and Liquify moves the picture itself: Push
//! carries it along, Expand swells it, Pinch draws it in, Twirl turns it
//! either way, and Push left and Push right carry it sideways
//! ([`LiquifyMode`]).
//!
//! Each dab reads a copy of the pixels it covers from before the dab, so a
//! dab never reads its own result, and falls off smoothly to the rim.

/// What a Blend dab does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendKind {
    /// Softens what is under the brush.
    Blur,
    /// Drags color along the stroke (Finger Tip).
    Smudge,
    /// Moves the picture along the stroke (Liquify, Push).
    Push,
    /// Swells the picture from the middle (Liquify, Expand).
    Expand,
    /// Draws the picture into the middle (Liquify, Pinch).
    Pinch,
    /// Turns the picture around the middle counterclockwise on the
    /// screen (Liquify, Twirl).
    Twirl,
    /// Turns it clockwise.
    TwirlBack,
    /// Moves the picture to the left of the stroke's way (Liquify, Push
    /// left).
    PushLeft,
    /// Moves it to the right (Liquify, Push right).
    PushRight,
}

impl BlendKind {
    /// The kind a Blend sub tool's mode names.
    #[must_use]
    pub const fn of_mode(mode: u8) -> Self {
        match mode {
            0 => Self::Blur,
            1 => Self::Smudge,
            2 => Self::Push,
            3 => Self::Expand,
            4 => Self::Pinch,
            7 => Self::TwirlBack,
            8 => Self::PushLeft,
            9 => Self::PushRight,
            _ => Self::Twirl,
        }
    }
}

/// The Liquify sub tool's modes (Clip Studio Paint's seven, in its
/// order). A mode is the sub tool's `mode`, so [`BlendKind::of_mode`]
/// reads it like any Blend sub tool's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiquifyMode {
    /// Carries the picture along the stroke.
    Push,
    /// Swells it from the middle.
    Expand,
    /// Draws it into the middle.
    Pinch,
    /// Turns it clockwise.
    TwirlClockwise,
    /// Turns it counterclockwise.
    TwirlCounterclockwise,
    /// Carries it to the left of the stroke's way.
    PushLeft,
    /// Carries it to the right.
    PushRight,
}

impl LiquifyMode {
    /// Every mode, in Tool Property's order.
    pub const ALL: [Self; 7] = [
        Self::Push,
        Self::Expand,
        Self::Pinch,
        Self::TwirlClockwise,
        Self::TwirlCounterclockwise,
        Self::PushLeft,
        Self::PushRight,
    ];

    /// The sub tool mode that means it. The first four are the modes the
    /// four Liquify sub tools of Sue 0.0.75 had (its Twirl turned
    /// counterclockwise on the screen).
    #[must_use]
    pub const fn mode(self) -> u8 {
        match self {
            Self::Push => 2,
            Self::Expand => 3,
            Self::Pinch => 4,
            Self::TwirlClockwise => 7,
            Self::TwirlCounterclockwise => 5,
            Self::PushLeft => 8,
            Self::PushRight => 9,
        }
    }

    /// The mode sub tool mode `mode` means, if it is a Liquify one.
    #[must_use]
    pub fn of_mode(mode: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|found| found.mode() == mode)
    }

    /// Its stable name in saved settings.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Expand => "expand",
            Self::Pinch => "pinch",
            Self::TwirlClockwise => "twirl_clockwise",
            Self::TwirlCounterclockwise => "twirl_counterclockwise",
            Self::PushLeft => "push_left",
            Self::PushRight => "push_right",
        }
    }

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Push => tr!("liquify-push"),
            Self::Expand => tr!("liquify-expand"),
            Self::Pinch => tr!("liquify-pinch"),
            Self::TwirlClockwise => tr!("liquify-twirl-clockwise"),
            Self::TwirlCounterclockwise => tr!("liquify-twirl-counterclockwise"),
            Self::PushLeft => tr!("liquify-push-left"),
            Self::PushRight => tr!("liquify-push-right"),
        }
    }
}

/// `true` if a Blend sub tool of mode `mode` liquifies.
#[must_use]
pub fn is_liquify(mode: u8) -> bool {
    LiquifyMode::of_mode(mode).is_some()
}

/// The Blend tool's Copy stamp sub tool's mode.
pub const COPY_STAMP: u8 = 6;

/// One Copy stamp dab at `center` (layer pixels), `radius` wide, at
/// `strength`: paints in what `source` (the layer as it was when the
/// stroke began, as large as `pixels`) holds `offset` away. Returns the
/// rectangle it changed, `[x0, y0, x1, y1]`.
#[allow(clippy::too_many_arguments, reason = "a dab's whole description")]
pub fn stamp(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    source: &[u8],
    center: [f32; 2],
    radius: f32,
    strength: f32,
    offset: [f32; 2],
) -> Option<[u32; 4]> {
    let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
    let (x0, y0) = (
        clamp(center[0] - radius - 1.0, width),
        clamp(center[1] - radius - 1.0, height),
    );
    let (x1, y1) = (
        clamp(center[0] + radius + 2.0, width),
        clamp(center[1] + radius + 2.0, height),
    );
    if x1 <= x0 || y1 <= y0 || source.len() != pixels.len() {
        return None;
    }
    let strength = strength.clamp(0.0, 1.0);
    let reach_sq = radius.max(0.5) * radius.max(0.5);
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        for x in x0..x1 {
            let px = x as f32 + 0.5;
            let (dx, dy) = (px - center[0], py - center[1]);
            // Firmer than a blur: whole inside, softening at the rim.
            let t = ((dx * dx + dy * dy) / reach_sq).clamp(0.0, 1.0);
            let weight = (1.0 - t * t * t) * strength;
            if weight <= 0.0 {
                continue;
            }
            let (sx, sy) = (px + offset[0], py + offset[1]);
            if sx < 0.0 || sy < 0.0 || sx >= width as f32 || sy >= height as f32 {
                continue;
            }
            let from = crate::warp::sample(source, width, height, sx, sy);
            let at = ((y * width + x) * 4) as usize;
            let here = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            pixels[at..at + 4].copy_from_slice(&mix(here, from, weight));
        }
    }
    Some([x0, y0, x1, y1])
}

/// How much a dab acts at a squared distance from its middle over its
/// squared radius: 1 in the middle to 0 at the rim. With `hardness` (0 to
/// 1) it stays whole that far out, then falls off over what is left.
fn falloff_sq(t_sq: f32, hardness: f32) -> f32 {
    let t_sq = t_sq.clamp(0.0, 1.0);
    let hard = hardness.clamp(0.0, 0.99);
    if hard <= 0.0 {
        let u = 1.0 - t_sq;
        return u * u;
    }
    let t = t_sq.sqrt();
    if t <= hard {
        return 1.0;
    }
    let s = (t - hard) / (1.0 - hard);
    let u = 1.0 - s * s;
    u * u
}

/// One dab of `kind` at `center` (layer pixels), `radius` wide, at
/// `strength` (0 to 1), the pen having just moved by `motion`. Works in
/// place on RGBA8 `pixels` of `width` by `height`; returns the rectangle
/// it changed, `[x0, y0, x1, y1]`.
#[allow(
    clippy::too_many_arguments,
    reason = "a dab and the pixels it lands on"
)]
pub fn dab(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    kind: BlendKind,
    center: [f32; 2],
    radius: f32,
    strength: f32,
    motion: [f32; 2],
) -> Option<[u32; 4]> {
    dab_hard(
        pixels, width, height, kind, center, radius, strength, motion, 0.0,
    )
}

/// [`dab`] with a `hardness` (0 to 1): how far out from the middle the
/// dab acts in full before it falls off (Liquify's Hardness).
#[allow(
    clippy::too_many_arguments,
    reason = "a dab and the pixels it lands on"
)]
pub fn dab_hard(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    kind: BlendKind,
    center: [f32; 2],
    radius: f32,
    strength: f32,
    motion: [f32; 2],
    hardness: f32,
) -> Option<[u32; 4]> {
    let reach = radius + motion[0].abs().max(motion[1].abs()) + 3.0;
    let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
    let (x0, y0) = (
        clamp(center[0] - reach, width),
        clamp(center[1] - reach, height),
    );
    let (x1, y1) = (
        clamp(center[0] + reach + 1.0, width),
        clamp(center[1] + reach + 1.0, height),
    );
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // A copy of the patch from before this dab, to read from.
    let (pw, ph) = (x1 - x0, y1 - y0);
    let mut patch = vec![0; (pw * ph * 4) as usize];
    for y in 0..ph {
        let from = (((y0 + y) * width + x0) * 4) as usize;
        let to = (y * pw * 4) as usize;
        patch[to..to + (pw * 4) as usize].copy_from_slice(&pixels[from..from + (pw * 4) as usize]);
    }
    let read = |x: f32, y: f32| crate::warp::sample(&patch, pw, ph, x - x0 as f32, y - y0 as f32);
    let strength = strength.clamp(0.0, 1.0);
    // Only the disc changes: each row from where it meets the circle to
    // where it leaves, and the weight from the squared distance.
    let reach_sq = radius.max(0.5) * radius.max(0.5);
    let mut changed: Option<[u32; 4]> = None;
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        let dy = py - center[1];
        let half_sq = reach_sq - dy * dy;
        if half_sq <= 0.0 {
            continue;
        }
        let half = half_sq.sqrt();
        let from = clamp((center[0] - half - 0.5).floor(), width).max(x0);
        let to = clamp((center[0] + half + 0.5).ceil() + 1.0, width).min(x1);
        for x in from..to {
            let px = x as f32 + 0.5;
            let dx = px - center[0];
            let weight = falloff_sq((dx * dx + dy * dy) / reach_sq, hardness) * strength;
            if weight <= 0.0 {
                continue;
            }
            let at = ((y * width + x) * 4) as usize;
            let here = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            let new = match kind {
                BlendKind::Blur => {
                    // The average around, a little way out.
                    let spread = (radius * 0.15).max(1.0);
                    let mut sum = [0.0_f32; 4];
                    for (ox, oy) in [
                        (0.0, 0.0),
                        (-spread, 0.0),
                        (spread, 0.0),
                        (0.0, -spread),
                        (0.0, spread),
                    ] {
                        let sample = read(px + ox, py + oy);
                        let a = f32::from(sample[3]) / 255.0;
                        for channel in 0..3 {
                            sum[channel] += f32::from(sample[channel]) * a;
                        }
                        sum[3] += a;
                    }
                    let soft = if sum[3] > 0.0 {
                        [
                            (sum[0] / sum[3]) as u8,
                            (sum[1] / sum[3]) as u8,
                            (sum[2] / sum[3]) as u8,
                            (sum[3] / 5.0 * 255.0) as u8,
                        ]
                    } else {
                        [0; 4]
                    };
                    mix(here, soft, weight)
                }
                BlendKind::Smudge => mix(here, read(px - motion[0], py - motion[1]), weight),
                BlendKind::Push => read(px - motion[0] * weight, py - motion[1] * weight),
                BlendKind::Expand | BlendKind::Pinch => {
                    let pull = if kind == BlendKind::Expand {
                        -0.12
                    } else {
                        0.12
                    } * weight;
                    read(center[0] + dx * (1.0 + pull), center[1] + dy * (1.0 + pull))
                }
                BlendKind::PushLeft | BlendKind::PushRight => {
                    // Across the stroke's way: its left on the screen is
                    // (y, -x) of the way, as y runs down.
                    let side = if kind == BlendKind::PushLeft {
                        1.0
                    } else {
                        -1.0
                    };
                    let across = [motion[1] * side, -motion[0] * side];
                    read(px - across[0] * weight, py - across[1] * weight)
                }
                BlendKind::Twirl | BlendKind::TwirlBack => {
                    let turn = if kind == BlendKind::Twirl {
                        0.18
                    } else {
                        -0.18
                    };
                    let (sin, cos) = (turn * weight).sin_cos();
                    read(
                        center[0] + dx * cos - dy * sin,
                        center[1] + dx * sin + dy * cos,
                    )
                }
            };
            if new != here {
                pixels[at..at + 4].copy_from_slice(&new);
                changed = Some(changed.map_or([x, y, x + 1, y + 1], |[a, b, c, d]| {
                    [a.min(x), b.min(y), c.max(x + 1), d.max(y + 1)]
                }));
            }
        }
    }
    changed
}

/// `a` moved `t` (0 to 1) of the way to `b`, over premultiplied alpha.
fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let (aa, ba) = (f32::from(a[3]) / 255.0, f32::from(b[3]) / 255.0);
    let alpha = aa + (ba - aa) * t;
    if alpha <= 0.0 {
        return [0; 4];
    }
    let channel = |index: usize| {
        let value =
            f32::from(a[index]) * aa + (f32::from(b[index]) * ba - f32::from(a[index]) * aa) * t;
        (value / alpha + 0.5).clamp(0.0, 255.0) as u8
    };
    [
        channel(0),
        channel(1),
        channel(2),
        (alpha * 255.0 + 0.5) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white picture with a black square in its middle.
    fn square() -> Vec<u8> {
        let mut pixels = vec![255; 60 * 60 * 4];
        for y in 25..35 {
            for x in 25..35 {
                pixels[(y * 60 + x) * 4..][..3].copy_from_slice(&[0, 0, 0]);
            }
        }
        pixels
    }

    fn gray(pixels: &[u8], x: usize, y: usize) -> u8 {
        pixels[(y * 60 + x) * 4]
    }

    #[test]
    fn copy_stamp_paints_what_lies_at_the_source() {
        let source = square();
        let mut pixels = square();
        // The square's middle (30, 30) copied to (10, 10).
        stamp(
            &mut pixels,
            60,
            60,
            &source,
            [10.0, 10.0],
            4.0,
            1.0,
            [20.0, 20.0],
        )
        .unwrap();
        assert_eq!(gray(&pixels, 10, 10), 0);
        assert_eq!(gray(&pixels, 30, 30), 0);
        // Outside the dab nothing changes.
        assert_eq!(gray(&pixels, 20, 10), 255);
        // A source off the canvas leaves the pixel as it was.
        let mut edge = square();
        stamp(
            &mut edge,
            60,
            60,
            &source,
            [10.0, 10.0],
            4.0,
            1.0,
            [-40.0, 0.0],
        );
        assert_eq!(gray(&edge, 10, 10), 255);
    }

    #[test]
    fn blend_dabs_blur_smudge_and_liquify() {
        // Blur softens the square's edge.
        let mut blur = square();
        for _ in 0..6 {
            dab(
                &mut blur,
                60,
                60,
                BlendKind::Blur,
                [25.0, 30.0],
                8.0,
                1.0,
                [0.0, 0.0],
            );
        }
        assert!((1..255).contains(&gray(&blur, 24, 30)) && (1..255).contains(&gray(&blur, 25, 30)));
        // Push moves black to the right of the square.
        let mut push = square();
        for step in 0..8 {
            let x = 30.0 + step as f32 * 2.0;
            dab(
                &mut push,
                60,
                60,
                BlendKind::Push,
                [x, 30.0],
                10.0,
                1.0,
                [2.0, 0.0],
            );
        }
        assert!(gray(&push, 37, 30) < 128, "dark pushed past the edge");
        // Expand swells the square; pinch shrinks it.
        let mut expand = square();
        let mut pinch = square();
        for _ in 0..5 {
            dab(
                &mut expand,
                60,
                60,
                BlendKind::Expand,
                [30.0, 30.0],
                14.0,
                1.0,
                [0.0, 0.0],
            );
            dab(
                &mut pinch,
                60,
                60,
                BlendKind::Pinch,
                [30.0, 30.0],
                14.0,
                1.0,
                [0.0, 0.0],
            );
        }
        let dark = |pixels: &[u8]| (0..60 * 60).filter(|index| pixels[index * 4] < 128).count();
        assert!(dark(&expand) > dark(&square()));
        assert!(dark(&pinch) < dark(&square()));
        // Twirl turns it: the square's corner moves.
        let mut twirl = square();
        for _ in 0..5 {
            dab(
                &mut twirl,
                60,
                60,
                BlendKind::Twirl,
                [30.0, 30.0],
                14.0,
                1.0,
                [0.0, 0.0],
            );
        }
        assert_ne!(twirl, square());
        // A dab off the picture changes nothing.
        let mut same = square();
        assert!(
            dab(
                &mut same,
                60,
                60,
                BlendKind::Blur,
                [-100.0, -100.0],
                5.0,
                1.0,
                [0.0, 0.0]
            )
            .is_none()
        );
    }

    /// A white picture with a black dot of 3 by 3 around `(x, y)`.
    fn dot(x: usize, y: usize) -> Vec<u8> {
        let mut pixels = vec![255; 60 * 60 * 4];
        for dy in 0..3 {
            for dx in 0..3 {
                pixels[((y + dy - 1) * 60 + x + dx - 1) * 4..][..3].copy_from_slice(&[0, 0, 0]);
            }
        }
        pixels
    }

    /// The middle of the dark pixels.
    fn dark_middle(pixels: &[u8]) -> [f32; 2] {
        let (mut sum, mut count) = ([0.0_f32; 2], 0.0_f32);
        for y in 0..60 {
            for x in 0..60 {
                if gray(pixels, x, y) < 128 {
                    sum[0] += x as f32;
                    sum[1] += y as f32;
                    count += 1.0;
                }
            }
        }
        [sum[0] / count, sum[1] / count]
    }

    fn liquify(pixels: &mut [u8], mode: LiquifyMode, motion: [f32; 2]) {
        for _ in 0..6 {
            dab(
                pixels,
                60,
                60,
                BlendKind::of_mode(mode.mode()),
                [30.0, 30.0],
                16.0,
                1.0,
                motion,
            );
        }
    }

    #[test]
    fn liquify_has_seven_modes_each_moving_the_picture_its_own_way() {
        // Every mode a mode of its own, by number and by name, and none
        // the Copy stamp's.
        let mut modes: Vec<u8> = LiquifyMode::ALL.iter().map(|mode| mode.mode()).collect();
        modes.sort_unstable();
        modes.dedup();
        assert_eq!(modes.len(), 7);
        assert!(!is_liquify(COPY_STAMP) && !is_liquify(0) && !is_liquify(1));
        for mode in LiquifyMode::ALL {
            assert!(is_liquify(mode.mode()));
            assert_eq!(LiquifyMode::of_mode(mode.mode()), Some(mode));
            assert!(!mode.label().is_empty());
        }
        // A dot right of the middle: the twirls carry it up or down.
        let start = dark_middle(&dot(38, 30));
        let mut counter = dot(38, 30);
        liquify(&mut counter, LiquifyMode::TwirlCounterclockwise, [0.0, 0.0]);
        let mut clockwise = dot(38, 30);
        liquify(&mut clockwise, LiquifyMode::TwirlClockwise, [0.0, 0.0]);
        assert!(dark_middle(&counter)[1] < start[1] - 0.5, "up the screen");
        assert!(
            dark_middle(&clockwise)[1] > start[1] + 0.5,
            "down the screen"
        );
        // A stroke going right: Push left carries the dot up, Push right
        // down, Push along to the right.
        let start = dark_middle(&dot(30, 30));
        let mut left = dot(30, 30);
        liquify(&mut left, LiquifyMode::PushLeft, [2.0, 0.0]);
        let mut right = dot(30, 30);
        liquify(&mut right, LiquifyMode::PushRight, [2.0, 0.0]);
        let mut along = dot(30, 30);
        liquify(&mut along, LiquifyMode::Push, [2.0, 0.0]);
        assert!(dark_middle(&left)[1] < start[1] - 0.5);
        assert!(dark_middle(&right)[1] > start[1] + 0.5);
        assert!(dark_middle(&along)[0] > start[0] + 0.5);
        assert!((dark_middle(&along)[1] - start[1]).abs() < 0.5);
    }

    #[test]
    fn hardness_keeps_a_dab_whole_further_out() {
        let soft_at = |hardness: f32| {
            let mut pixels = square();
            dab_hard(
                &mut pixels,
                60,
                60,
                BlendKind::Blur,
                [30.0, 30.0],
                12.0,
                1.0,
                [0.0, 0.0],
                hardness,
            );
            pixels
        };
        // No hardness is the plain dab.
        let mut plain = square();
        dab(
            &mut plain,
            60,
            60,
            BlendKind::Blur,
            [30.0, 30.0],
            12.0,
            1.0,
            [0.0, 0.0],
        );
        assert_eq!(soft_at(0.0), plain);
        // Halfway out (the square's edge) a hard dab blurs more.
        let change =
            |pixels: &[u8]| i32::from(gray(pixels, 25, 30)) - i32::from(gray(&square(), 25, 30));
        assert!(change(&soft_at(0.8)).abs() > change(&plain).abs());
        assert_eq!(falloff_sq(0.25, 0.6), 1.0);
        assert!(falloff_sq(0.25, 0.0) < 1.0);
        assert_eq!(falloff_sq(1.0, 0.9), 0.0);
    }
}
