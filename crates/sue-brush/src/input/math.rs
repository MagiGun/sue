//! Arithmetic that gives the same bits on every platform: only `+`, `-`,
//! `*`, `/` and `sqrt`, which IEEE 754 rounds exactly, never the system
//! library's `atan2`, `exp` or `powf`, whose last bits differ between
//! platforms. A stroke that replays must give the same pixels anywhere.
//! The functions shared with the rest of the engine live in
//! [`crate::math`].

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

pub(crate) use crate::math::atan2;

/// A number from 0 (included) to 1 (not) that depends only on `seed`
/// and `index`: SplitMix64.
#[must_use]
pub(crate) fn chance(seed: u64, index: u64) -> f32 {
    // Stream 0 of the engine's chance: the same bits as always.
    crate::math::chance(seed, index, 0)
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
