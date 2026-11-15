//! Arithmetic that gives the same bits on every platform: `+`, `-`, `*`,
//! `/` and `sqrt` only, which IEEE 754 rounds exactly. The system's
//! `sin`, `cos` and `powf` differ in their last bits between platforms,
//! and a stroke that replays must give the same pixels anywhere. The
//! functions shared with the rest of the engine live in [`crate::math`]
//! and are named here too.

pub use crate::math::{chance, cos_sin, pow_unit, unit};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
