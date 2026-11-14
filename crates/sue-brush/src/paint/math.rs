//! Arithmetic that gives the same bits on every platform: `+`, `-`, `*`,
//! `/` and `sqrt` only, which IEEE 754 rounds exactly. The system's
//! `sin`, `cos` and `powf` differ in their last bits between platforms,
//! and a stroke that replays (P3-8) must give the same pixels anywhere.

/// The cosine and sine of `degrees`, within about 1e-6; `(1, 0)` for an
/// angle that is not a number.
#[must_use]
pub fn cos_sin(degrees: f32) -> (f32, f32) {
    if !degrees.is_finite() {
        return (1.0, 0.0);
    }
    // Into 0 to 360, then to the nearest quarter turn and what is left
    // (-45 to 45 degrees), where the polynomials are exact enough.
    let turn = degrees - 360.0 * (degrees / 360.0).floor();
    let quarter = (turn / 90.0 + 0.5).floor();
    let rest = (turn - quarter * 90.0) * (std::f32::consts::PI / 180.0);
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

/// A number from 0 (included) to 1 (not) that depends only on `seed`,
/// `index` and `stream`: SplitMix64. Each use of chance in the paint
/// behaviours has its own stream, so adding one never moves another.
#[must_use]
pub fn chance(seed: u64, index: u64, stream: u64) -> f32 {
    let mut z = seed
        ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ stream.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1_u64 << 24) as f32
}

/// A point in the unit disc, spread evenly over its area, from `seed`,
/// `index` and `stream`: points of the square are drawn until one lies
/// in the disc, so no angle and no trigonometry is needed.
#[must_use]
pub fn in_disc(seed: u64, index: u64, stream: u64) -> [f32; 2] {
    for attempt in 0..32_u64 {
        let k = index.wrapping_mul(64).wrapping_add(attempt * 2);
        let x = chance(seed, k, stream) * 2.0 - 1.0;
        let y = chance(seed, k + 1, stream) * 2.0 - 1.0;
        if x * x + y * y <= 1.0 {
            return [x, y];
        }
    }
    [0.0, 0.0]
}

/// A direction (a unit vector) from `seed`, `index` and `stream`.
#[must_use]
pub fn direction(seed: u64, index: u64, stream: u64) -> [f32; 2] {
    for attempt in 0..32_u64 {
        let [x, y] = in_disc(seed, index.wrapping_mul(33).wrapping_add(attempt), stream);
        let length = (x * x + y * y).sqrt();
        if length > 0.05 {
            return [x / length, y / length];
        }
    }
    [1.0, 0.0]
}

/// 0 below `lo`, 1 above `hi`, smooth between.
#[must_use]
pub fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `x` held to 0 to 1; not a number is 0.
#[must_use]
pub fn unit(x: f32) -> f32 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

/// `base` raised to `exponent` for `base` from 0 to 1, by halving and
/// square roots on the exponent's bits: the same on every platform, to
/// within 1e-4.
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
    }

    #[test]
    fn points_in_the_disc_fill_it_evenly() {
        let points: Vec<[f32; 2]> = (0..20_000).map(|i| in_disc(3, i, 1)).collect();
        assert!(points.iter().all(|p| p[0] * p[0] + p[1] * p[1] <= 1.0));
        // Half the area lies within 1/sqrt(2) of the middle.
        let inner = points
            .iter()
            .filter(|p| p[0] * p[0] + p[1] * p[1] <= 0.5)
            .count();
        let share = inner as f32 / points.len() as f32;
        assert!((share - 0.5).abs() < 0.02, "{share}");
        let d = direction(3, 9, 2);
        assert!((d[0] * d[0] + d[1] * d[1] - 1.0).abs() < 1.0e-5);
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
}
