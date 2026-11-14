//! # The engine's own math
//!
//! Arithmetic that gives the same bits on every platform. IEEE 754 rounds
//! `+`, `-`, `*`, `/` and `sqrt` exactly, so they give the same result on
//! every machine; the system library's `sin`, `cos`, `exp`, `ln`, `powf`
//! and `atan2` do not, and their last bits differ between platforms and
//! even between versions of one platform's library. A stroke that
//! replays (`crate::replay`) must give the same pixels anywhere, so no
//! value that reaches a pixel goes through them: the engine uses the
//! functions here instead, built from the exact operations alone.
//!
//! A test (`tests/replay.rs`) reads the engine's sources and fails when
//! one of the system's functions appears outside a test.
//!
//! What is here, and how close each is to the true value:
//!
//! | Function | Domain | Within |
//! | --- | --- | --- |
//! | [`sin_cos`], [`sin`], [`cos`] | radians, any finite | 2e-7 (absolute) up to a few thousand turns |
//! | [`cos_sin`] | degrees, any finite | 2e-6 (absolute) |
//! | [`atan2`] | any | 2e-5 radians |
//! | [`exp`] | any | 3e-7 (relative) |
//! | [`ln`] | positive | 2e-7 (absolute, near 1) and 3e-7 (relative) |
//! | [`pow`] | positive base, or a whole exponent | 1e-6 (relative) for moderate results |
//! | [`pow_unit`] | base 0 to 1 | 1e-4 |
//!
//! [`chance`] is the engine's randomness: a counter hash of a stroke's
//! seed, a dab's number and a stream, so a stroke's chances depend on its
//! stored seed alone, never on where it lies or on what was drawn before.

use std::f32::consts::{FRAC_PI_2, PI};

/// A number from 0 (included) to 1 (not) that depends only on `seed`,
/// `index` and `stream`: SplitMix64 on a counter. Each use of chance has
/// its own stream, so adding one never moves another. Stream 0 is the
/// input pipeline's `random` and `stroke_random`.
#[must_use]
pub fn chance(seed: u64, index: u64, stream: u64) -> f32 {
    let mut z = seed
        ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ stream.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // The top 24 bits, which an f32 holds exactly.
    (z >> 40) as f32 / (1_u64 << 24) as f32
}

/// The sine and the cosine of `x` radians; `(0, 1)` for an angle that is
/// not a number.
#[must_use]
pub fn sin_cos(x: f32) -> (f32, f32) {
    if !x.is_finite() {
        return (0.0, 1.0);
    }
    // To the nearest quarter turn and what is left (-pi/4 to pi/4), with
    // the quarter turn split in two parts (Cody and Waite) so the rest
    // keeps its bits for angles of many turns.
    const QUARTER_HI: f32 = 1.570_312_5;
    const QUARTER_LO: f32 = 4.838_267_9e-4;
    let quarter = (x / FRAC_PI_2 + 0.5).floor();
    let rest = (x - quarter * QUARTER_HI) - quarter * QUARTER_LO;
    let (s, c) = sin_cos_small(rest);
    match (quarter.rem_euclid(4.0)) as i32 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// The sine and the cosine of `r` from -pi/4 to pi/4: Taylor polynomials,
/// exact there to the last bit or so.
fn sin_cos_small(r: f32) -> (f32, f32) {
    let r2 = r * r;
    let sin = r
        * (1.0
            + r2 * (-1.0 / 6.0
                + r2 * (1.0 / 120.0 + r2 * (-1.0 / 5_040.0 + r2 * (1.0 / 362_880.0)))));
    let cos = 1.0
        + r2 * (-0.5
            + r2 * (1.0 / 24.0
                + r2 * (-1.0 / 720.0 + r2 * (1.0 / 40_320.0 + r2 * (-1.0 / 3_628_800.0)))));
    (sin, cos)
}

/// The sine of `x` radians.
#[must_use]
pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

/// The cosine of `x` radians.
#[must_use]
pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// The cosine and sine of `degrees`, within about 1e-6; `(1, 0)` for an
/// angle that is not a number. Whole quarter turns are exact.
#[must_use]
pub fn cos_sin(degrees: f32) -> (f32, f32) {
    if !degrees.is_finite() {
        return (1.0, 0.0);
    }
    // Into 0 to 360, then to the nearest quarter turn and what is left
    // (-45 to 45 degrees), where the polynomials are exact enough.
    let turn = degrees - 360.0 * (degrees / 360.0).floor();
    let quarter = (turn / 90.0 + 0.5).floor();
    let rest = (turn - quarter * 90.0) * (PI / 180.0);
    let s2 = rest * rest;
    let sin = rest * (1.0 + s2 * (-1.0 / 6.0 + s2 * (1.0 / 120.0 + s2 * (-1.0 / 5040.0))));
    let cos = 1.0 + s2 * (-0.5 + s2 * (1.0 / 24.0 + s2 * (-1.0 / 720.0 + s2 / 40_320.0)));
    match (quarter as i32).rem_euclid(4) {
        0 => (cos, sin),
        1 => (-sin, cos),
        2 => (-cos, -sin),
        _ => (sin, -cos),
    }
}

/// `atan2(y, x)` in radians, -pi to pi, within about 1e-5; 0 for the
/// origin.
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (x.abs(), y.abs());
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    // atan on 0 to 1, then folded out to the other octants.
    let swap = ay > ax;
    let z = if swap { ax / ay } else { ay / ax };
    let mut angle = atan_unit(z);
    if swap {
        angle = FRAC_PI_2 - angle;
    }
    if x < 0.0 {
        angle = PI - angle;
    }
    if y < 0.0 { -angle } else { angle }
}

/// `atan(z)` for `z` from 0 to 1: a minimax polynomial in `z^2`.
fn atan_unit(z: f32) -> f32 {
    let s = z * z;
    z * (0.999_977_26
        + s * (-0.332_623_47
            + s * (0.193_543_46 + s * (-0.116_432_87 + s * (0.052_653_32 + s * -0.011_721_2)))))
}

/// `2^k` for a whole `k`, exactly, from its bits; 0 below the smallest
/// normal number and infinity above the largest.
fn two_to(k: i32) -> f32 {
    if k < -126 {
        0.0
    } else if k > 127 {
        f32::INFINITY
    } else {
        f32::from_bits(((k + 127) as u32) << 23)
    }
}

/// `e^x`. Infinity above about 88.7, 0 below about -87.3 (numbers too
/// small to be normal are taken as 0), not a number for not a number.
#[must_use]
pub fn exp(x: f32) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    if x > 88.72 {
        return f32::INFINITY;
    }
    if x < -87.33 {
        return 0.0;
    }
    // x = k ln 2 + r with r from -ln2/2 to ln2/2 (ln 2 in two parts), so
    // e^x = 2^k e^r.
    const LN2_HI: f32 = 0.693_145_75;
    const LN2_LO: f32 = 1.428_606_8e-6;
    let k = (x * std::f32::consts::LOG2_E + 0.5).floor();
    let r = (x - k * LN2_HI) - k * LN2_LO;
    let er = 1.0
        + r * (1.0
            + r * (1.0 / 2.0
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0 + r * (1.0 / 720.0 + r * (1.0 / 5_040.0)))))));
    // 2^k may leave the normal range at the edges while 2^k e^r does not.
    let k = k as i32;
    if k > 127 {
        er * two_to(127) * two_to(k - 127)
    } else if k < -126 {
        er * two_to(-126) * two_to(k + 126)
    } else {
        er * two_to(k)
    }
}

/// The natural logarithm of `x`: not a number below 0 and for not a
/// number, minus infinity at 0, infinity at infinity.
#[must_use]
pub fn ln(x: f32) -> f32 {
    if x.is_nan() || x < 0.0 {
        return f32::NAN;
    }
    if x == 0.0 {
        return f32::NEG_INFINITY;
    }
    if x.is_infinite() {
        return f32::INFINITY;
    }
    // Numbers too small to be normal are scaled up first.
    let (x, shift) = if x < f32::MIN_POSITIVE {
        (x * two_to(64), -64)
    } else {
        (x, 0)
    };
    // x = m 2^e with m from sqrt(1/2) to sqrt(2).
    let bits = x.to_bits();
    let mut e = ((bits >> 23) & 0xFF) as i32 - 127;
    let mut m = f32::from_bits((bits & 0x007F_FFFF) | (127 << 23));
    if m > std::f32::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    // ln m = 2 atanh s, s = (m - 1) / (m + 1), |s| under 0.172.
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let atanh = s
        * (1.0
            + s2 * (1.0 / 3.0
                + s2 * (1.0 / 5.0 + s2 * (1.0 / 7.0 + s2 * (1.0 / 9.0 + s2 * (1.0 / 11.0))))));
    let e = (e + shift) as f32;
    const LN2_HI: f32 = 0.693_145_75;
    const LN2_LO: f32 = 1.428_606_8e-6;
    e * LN2_HI + (2.0 * atanh + e * LN2_LO)
}

/// `base` raised to `exponent`. A positive base takes any exponent; a
/// negative base only a whole one (else not a number); a base of 0 gives
/// 0 for a positive exponent, 1 for 0 and infinity for a negative one.
#[must_use]
pub fn pow(base: f32, exponent: f32) -> f32 {
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() || exponent.is_nan() {
        return f32::NAN;
    }
    if base == 0.0 {
        return if exponent > 0.0 { 0.0 } else { f32::INFINITY };
    }
    if base < 0.0 {
        if exponent.fract() != 0.0 {
            return f32::NAN;
        }
        let odd = (exponent * 0.5).fract() != 0.0;
        let magnitude = pow(-base, exponent);
        return if odd { -magnitude } else { magnitude };
    }
    if exponent < 0.0 {
        return 1.0 / pow(base, -exponent);
    }
    // The whole part by squaring, the fraction through the logarithm, so
    // a large exponent does not magnify the logarithm's last bits.
    let whole = exponent.floor();
    let fraction = exponent - whole;
    let mut result = if fraction > 0.0 {
        exp(fraction * ln(base))
    } else {
        1.0
    };
    if whole >= 256.0 {
        // Far past what an f32 holds for any base but one so close to 1.
        return result * exp(whole * ln(base));
    }
    let mut square = base;
    let mut n = whole as u32;
    while n > 0 {
        if n & 1 == 1 {
            result *= square;
        }
        square *= square;
        n >>= 1;
    }
    result
}

/// `x` held to 0 to 1; not a number is 0.
#[must_use]
pub fn unit(x: f32) -> f32 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

/// `base` raised to `exponent` for `base` from 0 to 1, by halving and
/// square roots on the exponent's bits: the same on every platform, to
/// within 1e-4. A base outside 0 to 1 is held there; an exponent that is
/// not a positive number gives 1.
#[must_use]
pub fn pow_unit(base: f32, exponent: f32) -> f32 {
    let base = unit(base);
    if !exponent.is_finite() || exponent <= 0.0 {
        return 1.0;
    }
    if base == 0.0 {
        return 0.0;
    }
    let whole = exponent.floor();
    let mut result = 1.0_f32;
    let mut square = base;
    let mut n = whole as u32;
    while n > 0 {
        if n & 1 == 1 {
            result *= square;
        }
        square *= square;
        n >>= 1;
    }
    // The fraction, one binary digit at a time.
    let mut fraction = exponent - whole;
    let mut root = base;
    for _ in 0..16 {
        root = root.sqrt();
        fraction *= 2.0;
        if fraction >= 1.0 {
            result *= root;
            fraction -= 1.0;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(ours: f32, theirs: f64, absolute: f64, relative: f64, what: &str) {
        let gap = (f64::from(ours) - theirs).abs();
        assert!(
            gap <= absolute.max(relative * theirs.abs()),
            "{what}: {ours} against {theirs} (off by {gap})"
        );
    }

    #[test]
    fn sine_and_cosine_are_close_everywhere() {
        for step in -40_000..40_000 {
            let x = step as f32 * 0.003_7;
            let (s, c) = sin_cos(x);
            close(s, f64::from(x).sin(), 2.0e-7, 0.0, &format!("sin {x}"));
            close(c, f64::from(x).cos(), 2.0e-7, 0.0, &format!("cos {x}"));
            assert_eq!(sin(x), s);
            assert_eq!(cos(x), c);
        }
        // Whole quarter turns land where they should.
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
        assert!(sin(PI).abs() < 1.0e-7 && (cos(PI) + 1.0).abs() < 1.0e-7);
        assert!((sin(FRAC_PI_2) - 1.0).abs() < 1.0e-7);
        // Many turns out, the rest keeps its bits.
        let far = 10_000.0_f32;
        close(sin(far), f64::from(far).sin(), 3.0e-6, 0.0, "sin far");
        assert_eq!(sin_cos(f32::NAN), (0.0, 1.0));
        assert_eq!(sin_cos(f32::INFINITY), (0.0, 1.0));
    }

    #[test]
    fn cos_sin_is_close_everywhere() {
        for step in -7200..7200 {
            let degrees = step as f32 * 0.137;
            let (c, s) = cos_sin(degrees);
            let r = f64::from(degrees).to_radians();
            assert!((f64::from(c) - r.cos()).abs() < 2.0e-6, "{degrees}");
            assert!((f64::from(s) - r.sin()).abs() < 2.0e-6, "{degrees}");
        }
        assert_eq!(cos_sin(f32::NAN), (1.0, 0.0));
        assert_eq!(cos_sin(90.0), (0.0, 1.0));
        assert_eq!(cos_sin(-180.0), (-1.0, 0.0));
    }

    #[test]
    fn atan2_is_close_everywhere() {
        for step in 0..3600 {
            let angle = step as f64 / 3600.0 * std::f64::consts::TAU - std::f64::consts::PI;
            let (y, x) = (angle.sin() as f32 * 3.0, angle.cos() as f32 * 3.0);
            let ours = f64::from(atan2(y, x));
            let theirs = f64::from(y).atan2(f64::from(x));
            let mut gap = (ours - theirs).abs();
            if gap > std::f64::consts::PI {
                gap = std::f64::consts::TAU - gap;
            }
            assert!(gap < 2e-5, "{angle}: {ours} against {theirs}");
        }
        assert_eq!(atan2(0.0, 0.0), 0.0);
    }

    #[test]
    fn exp_is_close_everywhere() {
        for step in -8_700..8_870 {
            let x = step as f32 * 0.01;
            close(exp(x), f64::from(x).exp(), 0.0, 3.0e-7, &format!("exp {x}"));
        }
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(exp(100.0), f32::INFINITY);
        assert_eq!(exp(-100.0), 0.0);
        assert!(exp(f32::NAN).is_nan());
        assert_eq!(exp(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn ln_is_close_everywhere() {
        let mut x = 1.0e-30_f32;
        while x < 1.0e30 {
            close(ln(x), f64::from(x).ln(), 2.0e-7, 3.0e-7, &format!("ln {x}"));
            x *= 1.013;
        }
        for step in 1..2_000 {
            let x = step as f32 * 0.001 + 0.5;
            close(ln(x), f64::from(x).ln(), 2.0e-7, 3.0e-7, &format!("ln {x}"));
        }
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(0.0), f32::NEG_INFINITY);
        assert!(ln(-1.0).is_nan());
        assert_eq!(ln(f32::INFINITY), f32::INFINITY);
        // A number too small to be normal still has its logarithm.
        close(ln(1.0e-40), (1.0e-40_f64).ln(), 0.0, 1.0e-5, "ln tiny");
    }

    #[test]
    fn pow_is_close_where_it_is_used() {
        for &b in &[0.001_f32, 0.2, 0.5, 0.9, 1.0, 1.5, 2.0, 10.0, 300.0] {
            for &e in &[-3.0_f32, -0.5, 0.1, 0.5, 1.0, 1.7, 2.0, 3.25, 10.0] {
                let theirs = f64::from(b).powf(f64::from(e));
                close(pow(b, e), theirs, 0.0, 1.0e-6, &format!("{b}^{e}"));
            }
        }
        assert_eq!(pow(0.0, 2.0), 0.0);
        assert_eq!(pow(0.0, -1.0), f32::INFINITY);
        assert_eq!(pow(7.0, 0.0), 1.0);
        close(pow(-2.0, 3.0), -8.0, 0.0, 1.0e-6, "-2^3");
        close(pow(-2.0, 2.0), 4.0, 0.0, 1.0e-6, "-2^2");
        assert!(pow(-2.0, 0.5).is_nan());
    }

    #[test]
    fn pow_unit_matches_powf() {
        for &b in &[0.0_f32, 0.01, 0.3, 0.5, 0.77, 1.0] {
            for &e in &[0.1_f32, 0.5, 1.0, 1.7, 3.25, 10.0] {
                let ours = pow_unit(b, e);
                let theirs = b.powf(e);
                assert!((ours - theirs).abs() < 1.0e-4, "{b}^{e}: {ours} {theirs}");
            }
        }
        assert_eq!(pow_unit(0.4, 0.0), 1.0);
    }

    #[test]
    fn chance_is_spread_and_repeatable() {
        let values: Vec<f32> = (0..10_000).map(|i| chance(7, i, 0)).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
        assert_eq!(chance(7, 3, 0), values[3]);
        assert_ne!(chance(8, 3, 0), values[3]);
        assert_ne!(chance(7, 3, 1), values[3]);
    }

    #[test]
    fn the_same_inputs_give_the_same_bits() {
        // Recorded on Windows: these bits are part of the engine version.
        // A change here changes every stroke that uses them.
        assert_eq!(sin(1.0).to_bits(), 0x3f57_6aa4);
        assert_eq!(cos(1.0).to_bits(), 0x3f0a_5140);
        assert_eq!(exp(1.0).to_bits(), 0x402d_f854);
        assert_eq!(ln(10.0).to_bits(), 0x4013_5d8e);
        assert_eq!(pow(2.0, 0.5).to_bits(), 0x3fb5_04f3);
        assert_eq!(chance(1, 2, 3).to_bits(), chance(1, 2, 3).to_bits());
    }
}
