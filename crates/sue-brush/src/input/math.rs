//! Arithmetic that gives the same bits on every platform: only `+`, `-`,
//! `*`, `/` and `sqrt`, which IEEE 754 rounds exactly, never the system
//! library's `atan2`, `exp` or `powf`, whose last bits differ between
//! platforms. A stroke that replays must give the same pixels anywhere.

/// The angle of `v` in degrees, 0 to 360 (0 along x, 90 along y);
/// `None` for a vector too short to have one.
#[must_use]
pub(crate) fn direction(v: [f32; 2]) -> Option<f32> {
    let [x, y] = v;
    if x * x + y * y < 1e-12 {
        return None;
    }
    let degrees = atan2(y, x).to_degrees();
    Some(if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    })
}

/// `atan2(y, x)` in radians, -pi to pi, within about 1e-5.
#[must_use]
pub(crate) fn atan2(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
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

/// A number from 0 (included) to 1 (not) that depends only on `seed`
/// and `index`: SplitMix64.
#[must_use]
pub(crate) fn chance(seed: u64, index: u64) -> f32 {
    let mut z = seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // The top 24 bits, which an f32 holds exactly.
    (z >> 40) as f32 / (1_u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(direction([1.0, 0.0]), Some(0.0));
        assert!((direction([0.0, 1.0]).unwrap() - 90.0).abs() < 1e-3);
        assert!((direction([0.0, -1.0]).unwrap() - 270.0).abs() < 1e-3);
        assert_eq!(direction([0.0, 0.0]), None);
    }

    #[test]
    fn chance_is_spread_and_repeatable() {
        let values: Vec<f32> = (0..10_000).map(|i| chance(7, i)).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
        assert_eq!(chance(7, 3), values[3]);
        assert_ne!(chance(8, 3), values[3]);
    }
}
