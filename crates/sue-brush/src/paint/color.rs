//! Color jitter: the paint's hue, saturation and luminosity wander dab
//! by dab, or stroke by stroke, and toward the sub color.
//!
//! The amounts are the brush model's [`ColorJitter`]; the stroke points
//! add their own shifts ([`crate::input::StrokePoint::hue`] and the
//! rest), which a pen can drive. Colors are the layer's own sRGB values
//! from 0 to 1, as the rest of the engine mixes them.

use sue_model::brush::ColorJitter;

use super::math::{chance, unit};

/// The chance streams of color jitter.
const HUE: u64 = 0x10;
const SATURATION: u64 = 0x11;
const LUMINOSITY: u64 = 0x12;
const SUB: u64 = 0x13;
/// The stroke's own draw, made once from the seed.
const STROKE: u64 = 0x14;

/// A color as hue (0 to 1, a full turn), saturation and lightness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    /// The hue, 0 to 1.
    pub h: f32,
    /// The saturation, 0 to 1.
    pub s: f32,
    /// The lightness, 0 to 1.
    pub l: f32,
}

impl Hsl {
    /// The HSL of `rgb`.
    #[must_use]
    pub fn of(rgb: [f32; 3]) -> Self {
        let [r, g, b] = rgb.map(unit);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) * 0.5;
        let d = max - min;
        if d < 1.0e-6 {
            return Self { h: 0.0, s: 0.0, l };
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1.0e-6);
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        Self { h, s: unit(s), l }
    }

    /// The RGB of this color.
    #[must_use]
    pub fn rgb(self) -> [f32; 3] {
        let (h, s, l) = (self.h.rem_euclid(1.0) * 6.0, unit(self.s), unit(self.l));
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
        let m = l - c * 0.5;
        let [r, g, b] = match h as u32 {
            0 => [c, x, 0.0],
            1 => [x, c, 0.0],
            2 => [0.0, c, x],
            3 => [0.0, x, c],
            4 => [x, 0.0, c],
            _ => [c, 0.0, x],
        };
        [r + m, g + m, b + m].map(unit)
    }
}

/// `color` with its hue turned by `hue` degrees and its saturation and
/// luminosity moved by `saturation` and `luminosity` (-1 to 1).
#[must_use]
pub fn shifted(color: [f32; 3], hue: f32, saturation: f32, luminosity: f32) -> [f32; 3] {
    let clean = |v: f32| if v.is_finite() { v } else { 0.0 };
    let (hue, saturation, luminosity) = (clean(hue), clean(saturation), clean(luminosity));
    if hue == 0.0 && saturation == 0.0 && luminosity == 0.0 {
        return color;
    }
    let mut hsl = Hsl::of(color);
    hsl.h += hue / 360.0;
    hsl.s = unit(hsl.s + saturation);
    hsl.l = unit(hsl.l + luminosity);
    hsl.rgb()
}

/// The paint's color, wandering as a brush's [`ColorJitter`] says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jitter {
    jitter: ColorJitter,
    seed: u64,
    main: [f32; 3],
    sub: [f32; 3],
}

impl Jitter {
    /// The jitter of `jitter` for a stroke with `seed`, painting `main`
    /// with `sub` as the sub color. A stroke's own wander
    /// ([`ColorJitter::stroke`]) is drawn here, once.
    #[must_use]
    pub fn new(jitter: ColorJitter, seed: u64, main: [f32; 3], sub: [f32; 3]) -> Self {
        let main = if jitter.stroke {
            let draw = |stream: u64| chance(seed, 0, STROKE + stream) * 2.0 - 1.0;
            shifted(
                main,
                draw(0) * unit(jitter.stroke_hue) * 180.0,
                draw(1) * unit(jitter.stroke_saturation),
                draw(2) * unit(jitter.stroke_luminosity),
            )
        } else {
            main
        };
        Self {
            jitter,
            seed,
            main,
            sub,
        }
    }

    /// `true` when dabs differ in color, so a stroke must keep a color
    /// for each pixel rather than one for all.
    #[must_use]
    pub fn varies(&self) -> bool {
        let j = &self.jitter;
        j.tip && (j.hue > 0.0 || j.saturation > 0.0 || j.luminosity > 0.0 || j.sub > 0.0)
    }

    /// The stroke's color before any dab's own wander.
    #[must_use]
    pub const fn main(&self) -> [f32; 3] {
        self.main
    }

    /// The color of dab `index`, with the stroke point's own shifts
    /// (`hue` in degrees, `saturation` and `luminosity` -1 to 1).
    #[must_use]
    pub fn dab(&self, index: u64, hue: f32, saturation: f32, luminosity: f32) -> [f32; 3] {
        let j = &self.jitter;
        let mut color = self.main;
        if j.tip {
            let draw = |stream: u64| chance(self.seed, index, stream);
            if j.sub > 0.0 {
                let t = draw(SUB) * unit(j.sub);
                for (c, s) in color.iter_mut().zip(self.sub) {
                    *c += (s - *c) * t;
                }
            }
            color = shifted(
                color,
                (draw(HUE) * 2.0 - 1.0) * unit(j.hue) * 180.0 + hue,
                (draw(SATURATION) * 2.0 - 1.0) * unit(j.saturation) + saturation,
                (draw(LUMINOSITY) * 2.0 - 1.0) * unit(j.luminosity) + luminosity,
            );
        } else {
            color = shifted(color, hue, saturation, luminosity);
        }
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1.0e-4)
    }

    #[test]
    fn hsl_round_trips() {
        for rgb in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.9, 0.2, 0.1],
            [0.1, 0.8, 0.3],
            [0.2, 0.3, 0.95],
            [0.5, 0.5, 0.2],
        ] {
            assert!(close(Hsl::of(rgb).rgb(), rgb), "{rgb:?}");
        }
        // A third of a turn takes red to green.
        assert!(close(
            shifted([1.0, 0.0, 0.0], 120.0, 0.0, 0.0),
            [0.0, 1.0, 0.0]
        ));
    }

    #[test]
    fn dabs_wander_within_their_amounts_and_repeat_for_a_seed() {
        let jitter = ColorJitter {
            tip: true,
            hue: 0.1,
            ..ColorJitter::default()
        };
        let red = [0.9, 0.1, 0.1];
        let j = Jitter::new(jitter, 7, red, [0.0; 3]);
        assert!(j.varies());
        let hues: Vec<f32> = (0..200)
            .map(|i| Hsl::of(j.dab(i, 0.0, 0.0, 0.0)).h)
            .collect();
        // Within 18 degrees of red, either side.
        assert!(
            hues.iter()
                .all(|&h| h <= 0.05 + 1.0e-4 || h >= 0.95 - 1.0e-4)
        );
        assert!(hues.iter().any(|&h| h > 0.02 && h < 0.06));
        assert!(hues.iter().any(|&h| h > 0.94 && h < 0.98));
        assert_eq!(j.dab(5, 0.0, 0.0, 0.0), j.dab(5, 0.0, 0.0, 0.0));
        let still = Jitter::new(ColorJitter::default(), 7, red, [0.0; 3]);
        assert!(!still.varies());
        assert_eq!(still.dab(3, 0.0, 0.0, 0.0), red);
    }

    #[test]
    fn a_stroke_wander_moves_the_whole_stroke_alike() {
        let jitter = ColorJitter {
            stroke: true,
            stroke_luminosity: 0.5,
            ..ColorJitter::default()
        };
        let gray = [0.5, 0.5, 0.5];
        let a = Jitter::new(jitter, 1, gray, gray);
        let b = Jitter::new(jitter, 2, gray, gray);
        assert_ne!(a.main(), b.main());
        assert_eq!(a.dab(0, 0.0, 0.0, 0.0), a.dab(99, 0.0, 0.0, 0.0));
    }

    #[test]
    fn the_sub_color_pulls_dabs_toward_it() {
        let jitter = ColorJitter {
            tip: true,
            sub: 1.0,
            ..ColorJitter::default()
        };
        let j = Jitter::new(jitter, 3, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let lights: Vec<f32> = (0..100).map(|i| j.dab(i, 0.0, 0.0, 0.0)[0]).collect();
        assert!(lights.iter().any(|&l| l > 0.8) && lights.iter().any(|&l| l < 0.2));
    }
}
