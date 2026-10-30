//! The Blend tool (Clip Studio Paint's J): brushes that rework the pixels
//! already there instead of adding paint. Blur softens, Finger Tip drags
//! color along the stroke, and Liquify moves the picture itself: Push
//! carries it along, Expand swells it, Pinch draws it in, Twirl turns it.
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
    /// Turns the picture around the middle (Liquify, Twirl).
    Twirl,
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
            _ => Self::Twirl,
        }
    }
}

/// How much a dab acts at a squared distance from its middle over its
/// squared radius: 1 in the middle to 0 at the rim.
fn falloff_sq(t_sq: f32) -> f32 {
    let u = 1.0 - t_sq.clamp(0.0, 1.0);
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
            let weight = falloff_sq((dx * dx + dy * dy) / reach_sq) * strength;
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
                BlendKind::Twirl => {
                    let (sin, cos) = (0.18 * weight).sin_cos();
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
}
