//! Sue's own papers, made by rule: the same texels on every machine,
//! drawn from nothing but integer hashes, so nothing of another
//! program's paper pictures is in them.

use super::Paper;

/// The names of the papers Sue makes by rule, as a brush stores them.
/// A name, once given, is never taken back (golden files hold them).
pub const BUILTIN: &[&str] = &["grain", "canvas", "cold-press", "rough"];

impl Paper {
    /// One of Sue's own papers, by its stored name ([`BUILTIN`]); `None`
    /// for a name this build does not make.
    #[must_use]
    pub fn builtin(name: &str) -> Option<Self> {
        let (side, texels) = match name {
            "grain" => (256, grain(256)),
            "canvas" => (256, canvas(256)),
            "cold-press" => (256, cold_press(256)),
            "rough" => (256, rough(256)),
            _ => return None,
        };
        // Most of a paper's face is high and its valleys are few: lifted
        // so its mean is near two thirds, a full stroke shows the tooth
        // as specks rather than as a gray screen.
        let texels: Vec<f32> = normalized(texels).into_iter().map(f32::sqrt).collect();
        Self::from_texels(side, side, &texels)
    }
}

/// A number from 0 to 1 that depends only on its three integers.
fn hash(x: i64, y: i64, salt: u64) -> f32 {
    let mut z = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1_u64 << 24) as f32
}

/// Smooth value noise with `cells` cells across a field of `side`,
/// repeating every `side` texels.
fn value_noise(side: usize, cells: usize, salt: u64) -> Vec<f32> {
    let cells = cells.max(1);
    let mut out = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let fx = (x as f32 + 0.5) * cells as f32 / side as f32;
            let fy = (y as f32 + 0.5) * cells as f32 / side as f32;
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fade(fx - x0), fade(fy - y0));
            let (x0, y0) = (x0 as i64, y0 as i64);
            let c = cells as i64;
            let at = |i: i64, j: i64| hash(i.rem_euclid(c), j.rem_euclid(c), salt);
            let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
            let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
            out.push(top + (bottom - top) * ty);
        }
    }
    out
}

/// The smooth step a value noise eases with.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Octaves of value noise from `cells` cells up, each half as strong.
fn fbm(side: usize, cells: usize, octaves: usize, salt: u64) -> Vec<f32> {
    let mut out = vec![0.0; side * side];
    let mut weight = 1.0;
    let mut cells = cells;
    for octave in 0..octaves {
        let layer = value_noise(side, cells, salt + octave as u64);
        for (o, v) in out.iter_mut().zip(layer) {
            *o += v * weight;
        }
        weight *= 0.5;
        cells *= 2;
    }
    out
}

/// Stretched to 0 to 1 by its own lowest and highest values.
fn normalized(mut texels: Vec<f32>) -> Vec<f32> {
    let lo = texels.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = texels.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (hi - lo).max(1.0e-6);
    for t in &mut texels {
        *t = (*t - lo) / span;
    }
    texels
}

/// Fine tooth, like drawing paper under a pencil: small bumps and a
/// little pure chance.
fn grain(side: usize) -> Vec<f32> {
    let tooth = fbm(side, 64, 2, 11);
    let mut out = Vec::with_capacity(side * side);
    for (i, t) in tooth.into_iter().enumerate() {
        let (x, y) = ((i % side) as i64, (i / side) as i64);
        out.push(t * 0.75 + hash(x, y, 12) * 0.25);
    }
    out
}

/// A woven canvas: threads across and along, each passing over and
/// under the other in turn, with a little noise in the threads.
fn canvas(side: usize) -> Vec<f32> {
    // A thread is 8 texels wide; a field of 256 holds 32 of them.
    const THREAD: usize = 8;
    let slub = value_noise(side, 16, 21);
    let mut out = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let (cx, cy) = (x / THREAD, y / THREAD);
            // How far across its thread a texel is, 0 at the middle.
            let across = |n: usize| {
                let u = ((n % THREAD) as f32 + 0.5) / THREAD as f32 * 2.0 - 1.0;
                1.0 - u * u
            };
            let weft_on_top = (cx + cy) % 2 == 0;
            let (top, under) = if weft_on_top {
                (across(y), across(x))
            } else {
                (across(x), across(y))
            };
            let thread = top.max(under * 0.55);
            out.push(thread * (0.85 + 0.15 * slub[y * side + x]));
        }
    }
    out
}

/// Cold pressed watercolor paper: soft rounded hills of a few sizes.
fn cold_press(side: usize) -> Vec<f32> {
    let hills = fbm(side, 32, 3, 31);
    hills
        .into_iter()
        .map(|h| {
            // Rounded tops, wide valleys.
            let t = (h / 1.875).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

/// Rough paper: deep, uneven tooth with ridges.
fn rough(side: usize) -> Vec<f32> {
    let base = fbm(side, 16, 4, 41);
    let ridges = fbm(side, 64, 2, 43);
    base.into_iter()
        .zip(ridges)
        .map(|(b, r)| {
            let ridge = 1.0 - (r / 1.5 * 2.0 - 1.0).abs();
            b * 0.6 + ridge * 0.4
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_paper_is_made_the_same_each_time() {
        for name in BUILTIN {
            let a = Paper::builtin(name).expect("a paper");
            let b = Paper::builtin(name).expect("a paper");
            assert_eq!(a, b, "{name}");
            let mean = a.mean();
            assert!((0.45..0.85).contains(&mean), "{name}: mean {mean}");
            // Not flat.
            let spread: f32 = (0..64)
                .map(|i| (a.sample(i as f32 * 3.7, i as f32 * 1.3, 1.0) - mean).abs())
                .sum::<f32>()
                / 64.0;
            assert!(spread > 0.05, "{name}: spread {spread}");
        }
        assert!(Paper::builtin("no-such-paper").is_none());
    }
}
