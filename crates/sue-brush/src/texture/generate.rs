//! Sue's own papers, made by rule: the same texels on every machine,
//! drawn from nothing but integer hashes and sums, so nothing of another
//! program's paper pictures is in them.
//!
//! The noise is white noise blurred (three box passes each way), which
//! looks the same in every direction: value noise, made on a grid of
//! cells, shows that grid as a lattice of dots at a fine scale, and a
//! paper must read as natural grain at 100 % and 200 %, never as a
//! grid.

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

/// White noise over a field of `side`, one hash a texel.
fn white(side: usize, salt: u64) -> Vec<f32> {
    (0..side * side)
        .map(|i| hash((i % side) as i64, (i / side) as i64, salt))
        .collect()
}

/// `field` (`side` by `side`, repeating) blurred by a box `2r + 1` wide
/// along one axis.
fn box_pass(field: &[f32], side: usize, r: usize, across: bool) -> Vec<f32> {
    let mut out = vec![0.0; field.len()];
    let width = (2 * r + 1) as f32;
    for line in 0..side {
        let at = |k: usize| {
            let k = k % side;
            if across {
                field[line * side + k]
            } else {
                field[k * side + line]
            }
        };
        // A running sum over the window, wrapping round the field.
        let mut sum: f32 = (0..=2 * r).map(|k| at(k + side - r)).sum();
        for k in 0..side {
            let i = if across {
                line * side + k
            } else {
                k * side + line
            };
            out[i] = sum / width;
            sum += at(k + r + 1) - at(k + side - r);
        }
    }
    out
}

/// Smooth noise with no grid in it: white noise blurred by three box
/// passes each way (close to a gaussian), so it is the same in every
/// direction and has no lattice of cells, as value noise has. Repeats
/// every `side` texels, and is stretched to 0 to 1.
fn smooth(side: usize, r: usize, salt: u64) -> Vec<f32> {
    let mut field = white(side, salt);
    if r > 0 {
        for _ in 0..3 {
            field = box_pass(&field, side, r, true);
            field = box_pass(&field, side, r, false);
        }
    }
    normalized(field)
}

/// Octaves of [`smooth`] noise from a blur of `r` down, each half as
/// wide and `falloff` times as strong as the one before.
fn octaves(side: usize, r: usize, count: usize, falloff: f32, salt: u64) -> Vec<f32> {
    let mut out = vec![0.0; side * side];
    let mut weight = 1.0;
    let mut r = r;
    for octave in 0..count {
        let layer = smooth(side, r, salt + octave as u64);
        for (o, v) in out.iter_mut().zip(layer) {
            *o += v * weight;
        }
        weight *= falloff;
        r /= 2;
    }
    normalized(out)
}

/// The smooth step a thread eases with.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
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

/// Fine tooth, like drawing paper under a pencil: small bumps of a few
/// sizes and a little pure chance, the same in every direction.
fn grain(side: usize) -> Vec<f32> {
    let tooth = octaves(side, 2, 3, 0.55, 11);
    let chance = white(side, 12);
    tooth
        .into_iter()
        .zip(chance)
        .map(|(t, c)| t * 0.8 + c * 0.2)
        .collect()
}

/// Noise like [`smooth`], blurred `rx` texels across and `ry` up and
/// down: streaks along the longer blur.
fn streaks(side: usize, rx: usize, ry: usize, salt: u64) -> Vec<f32> {
    let mut field = white(side, salt);
    for _ in 0..3 {
        if rx > 0 {
            field = box_pass(&field, side, rx, true);
        }
        if ry > 0 {
            field = box_pass(&field, side, ry, false);
        }
    }
    normalized(field)
}

/// Primed artist's canvas as it shows under a dry brush: threads across
/// and threads up and down, each a fibre that swells and thins along
/// its length, crossing over and under one another at no fixed pitch,
/// in broad patches where the cloth rises and sinks, with the gesso's
/// own tooth over all of it. Made of streaks of noise rather than a
/// ruled weave, so it reads as the grain of a cloth, never as a grid
/// of dots.
fn canvas(side: usize) -> Vec<f32> {
    let weft = streaks(side, 6, 1, 21);
    let warp = streaks(side, 1, 6, 22);
    // Which of the two lies on top, changing every few texels.
    let over = smooth(side, 2, 23);
    let height = smooth(side, 10, 24);
    let tooth = octaves(side, 1, 2, 0.6, 25);
    let mut out = Vec::with_capacity(side * side);
    for n in 0..side * side {
        let t = fade(over[n]);
        let threads = weft[n] * t + warp[n] * (1.0 - t);
        let ridges = threads.max(0.5 * (weft[n] + warp[n]));
        out.push(ridges * 0.55 + tooth[n] * 0.25 + height[n] * 0.2);
    }
    out
}

/// Cold pressed watercolor paper: soft rounded hills of a few sizes.
fn cold_press(side: usize) -> Vec<f32> {
    octaves(side, 6, 4, 0.55, 31)
        .into_iter()
        .map(|t| {
            // Rounded tops, wide valleys.
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

/// Rough paper: deep, uneven tooth with ridges.
fn rough(side: usize) -> Vec<f32> {
    let base = octaves(side, 12, 4, 0.5, 41);
    let ridges = octaves(side, 3, 2, 0.6, 43);
    base.into_iter()
        .zip(ridges)
        .map(|(b, r)| {
            let ridge = 1.0 - (r * 2.0 - 1.0).abs();
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

    /// How alike the paper is to itself moved by `(dx, dy)` texels, -1
    /// to 1 (1: the same pattern again, as a grid repeats).
    fn alike(paper: &Paper, dx: i32, dy: i32) -> f32 {
        let at = |x: i32, y: i32| paper.sample(x as f32 + 0.5, y as f32 + 0.5, 1.0);
        let mean = paper.mean();
        let (mut cross, mut var) = (0.0_f32, 0.0_f32);
        for y in 0..256 {
            for x in 0..256 {
                let a = at(x, y) - mean;
                cross += a * (at(x + dx, y + dy) - mean);
                var += a * a;
            }
        }
        cross / var.max(1e-9)
    }

    #[test]
    fn no_paper_repeats_as_a_grid() {
        // Along any way, natural grain grows less like itself the
        // further it is moved, and stays unlike. A grid of dots or a
        // ruled weave comes back: unlike half a pitch away, alike again
        // a whole pitch away. So the rebound, how much more alike than
        // at any shorter move along the same way, stays small.
        let mut report = String::new();
        let mut worst = 0.0_f32;
        for name in BUILTIN {
            let paper = Paper::builtin(name).expect("a paper");
            let mut most = 0.0_f32;
            for (wx, wy) in [(1, 0), (0, 1), (1, 1), (1, -1), (2, 1), (1, 2)] {
                let mut least = f32::INFINITY;
                for k in 1..=24 {
                    let r = alike(&paper, wx * k, wy * k);
                    most = most.max(r - least);
                    least = least.min(r);
                }
            }
            report += &format!(
                "{name}: rebound {most:.3}
"
            );
            worst = worst.max(most);
        }
        println!("{report}");
        assert!(
            worst < 0.15,
            "a paper repeats like a grid:
{report}"
        );
    }
}
