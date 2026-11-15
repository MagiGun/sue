//! # Paper textures
//!
//! A texture breaks a stroke up the way paper does: the paint lands on
//! the paper's high spots first and fills its valleys only as the brush
//! presses harder. A [`Paper`] is the height field (Sue's own, made by
//! rule, [`BUILTIN`], or a person's gray picture); [`TextureSettings`]
//! say how it is laid on the layer (scale, turn, brightness, contrast,
//! invert) and how it meets the stroke ([`TextureMode`], strength,
//! emphasized density, by each dab or by the whole stroke). A
//! [`Texture`] is the two together, ready to read at any layer pixel.
//!
//! The paper is fixed to the layer, not to the stroke: two strokes over
//! the same place show the same tooth, as on real paper.
//!
//! Nothing here reads the platform's `sin`, `cos` or `powf`
//! (`paint::math`), so a textured stroke replays to the same pixels.

mod generate;
mod paper;

pub use generate::BUILTIN;
pub use paper::{LARGEST, Paper};

use crate::paint::math::{cos_sin, smoothstep, unit};

/// How a texture meets the stroke's coverage. The stored names are in
/// [`TextureMode::name`]; a name is never taken back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureMode {
    /// The paper's valleys hold paint back in proportion: an even,
    /// gentle grain at every pressure.
    #[default]
    Multiply,
    /// The valleys are taken away from the coverage: a light stroke
    /// catches only the high spots, a heavy one fills most of the paper.
    Subtract,
    /// The stroke fills the paper from its high spots down, as deep as
    /// it is strong: crisp grain, like graphite or chalk on tooth.
    Height,
    /// The darker of the stroke and the paper.
    Darken,
    /// The paper's own light and shade, carried by the stroke: high
    /// spots take the paint, valleys only part of it.
    Overlay,
}

impl TextureMode {
    /// Every mode, in menu order.
    pub const ALL: &'static [Self] = &[
        Self::Multiply,
        Self::Subtract,
        Self::Height,
        Self::Darken,
        Self::Overlay,
    ];

    /// The stored name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Multiply => "multiply",
            Self::Subtract => "subtract",
            Self::Height => "height",
            Self::Darken => "darken",
            Self::Overlay => "overlay",
        }
    }

    /// The mode stored as `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|mode| mode.name() == name)
    }
}

/// How a paper is laid on the layer and how it meets the stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSettings {
    /// The paper's size: 1 is one texel a pixel, 2 twice as large.
    pub scale: f32,
    /// The paper's turn, in degrees.
    pub rotation: f32,
    /// Added to the paper's heights, -1 to 1: brighter paper catches
    /// more paint.
    pub brightness: f32,
    /// -1 (flat) to 1 (every height either a valley or a high spot).
    pub contrast: f32,
    /// High spots become valleys.
    pub invert: bool,
    /// How strongly the paper breaks the stroke up, 0 to 1 (Clip
    /// Studio's "texture density"; a pen can drive it).
    pub strength: f32,
    /// How the paper meets the stroke.
    pub mode: TextureMode,
    /// The lighter the stroke, the more of the paper it shows; a full
    /// press shows half as much.
    pub emphasize: bool,
    /// Laid on every dab before dabs add up, rather than once on the
    /// whole stroke: a build-up brush then fills the paper gradually.
    pub per_dab: bool,
}

impl Default for TextureSettings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            invert: false,
            strength: 1.0,
            mode: TextureMode::Multiply,
            emphasize: false,
            per_dab: false,
        }
    }
}

/// A paper laid on the layer: read it at any layer pixel.
#[derive(Clone, Copy, Debug)]
pub struct Texture<'a> {
    paper: &'a Paper,
    settings: TextureSettings,
    /// The turn back into the paper's frame, and one over the scale.
    cos: f32,
    sin: f32,
    inverse_scale: f32,
    gain: f32,
}

impl<'a> Texture<'a> {
    /// `paper` laid as `settings` say. A scale that is not a positive
    /// number is 1.
    #[must_use]
    pub fn new(paper: &'a Paper, settings: TextureSettings) -> Self {
        let scale = if settings.scale.is_finite() && settings.scale > 1.0e-3 {
            settings.scale
        } else {
            1.0
        };
        let (cos, sin) = cos_sin(settings.rotation);
        let contrast = if settings.contrast.is_finite() {
            settings.contrast.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        // Contrast 1 is a gain of 20 (all but a step), -1 is flat.
        let gain = if contrast >= 0.0 {
            1.0 / (1.0 - contrast * 0.95)
        } else {
            1.0 + contrast
        };
        Self {
            paper,
            settings,
            cos,
            sin,
            inverse_scale: 1.0 / scale,
            gain,
        }
    }

    /// The settings.
    #[must_use]
    pub const fn settings(&self) -> &TextureSettings {
        &self.settings
    }

    /// The paper's height under the pixel center `p` (layer pixels), 0
    /// (a valley) to 1 (a high spot), after the turn, scale, inversion,
    /// brightness and contrast.
    #[must_use]
    pub fn height(&self, p: [f32; 2]) -> f32 {
        let (x, y) = (p[0], p[1]);
        let u = (self.cos * x + self.sin * y) * self.inverse_scale;
        let v = (-self.sin * x + self.cos * y) * self.inverse_scale;
        let mut t = self.paper.sample(u, v, self.inverse_scale);
        if self.settings.invert {
            t = 1.0 - t;
        }
        let brightness = if self.settings.brightness.is_finite() {
            self.settings.brightness.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        unit((t - 0.5) * self.gain + 0.5 + brightness)
    }

    /// `coverage` (0 to 1) at pixel center `p` broken up by the paper,
    /// with the settings' strength.
    #[must_use]
    pub fn apply(&self, coverage: f32, p: [f32; 2]) -> f32 {
        self.apply_with(coverage, p, self.settings.strength)
    }

    /// [`Self::apply`] with `strength` (0 to 1) in place of the
    /// settings', for a strength a pen drives.
    #[must_use]
    pub fn apply_with(&self, coverage: f32, p: [f32; 2], strength: f32) -> f32 {
        let c = unit(coverage);
        if c == 0.0 {
            return 0.0;
        }
        let mut s = unit(strength);
        if self.settings.emphasize {
            s = unit(s * (2.0 - c));
        }
        if s == 0.0 {
            return c;
        }
        let t = self.height(p);
        let textured = match self.settings.mode {
            TextureMode::Multiply => c * (1.0 - s * (1.0 - t)),
            TextureMode::Subtract => (c - s * (1.0 - t)).max(0.0),
            TextureMode::Height => {
                // The paint sinks into the paper as deep as the stroke
                // is strong: from the high spots down to `1 - depth`. The
                // texture's strength holds the deepest reach back, so a
                // full stroke still leaves the deepest valleys open (a
                // twelfth of the range is the soft edge of each bump).
                // Where it reaches, it lays darker than the stroke's mean
                // (a grain of graphite is dark).
                let depth = c * (1.0 - 0.45 * s);
                let reached = smoothstep(-0.12, 0.0, t - (1.0 - depth));
                let crisp = reached * (0.5 + 0.5 * c);
                c + (crisp - c) * s
            }
            TextureMode::Darken => c + (c.min(t) - c) * s,
            TextureMode::Overlay => {
                // The paper overlaid by the stroke: a full stroke keeps
                // the high spots full and lightens the valleys toward
                // the paper, a half stroke is the paper's own shade. It
                // never lays more than the stroke.
                let o = if t < 0.5 {
                    2.0 * t * c
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - c)
                };
                c + (o.min(c) - c) * s
            }
        };
        unit(textured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripes() -> Paper {
        // Columns alternate between a high spot and a valley.
        let texels: Vec<f32> = (0..16 * 16)
            .map(|i| if (i % 16) % 2 == 0 { 1.0 } else { 0.0 })
            .collect();
        Paper::from_texels(16, 16, &texels).expect("a paper")
    }

    #[test]
    fn every_mode_has_a_name_that_reads_back() {
        for &mode in TextureMode::ALL {
            assert_eq!(TextureMode::from_name(mode.name()), Some(mode));
        }
        assert_eq!(TextureMode::from_name("nope"), None);
    }

    #[test]
    fn high_spots_take_paint_and_valleys_hold_it_back() {
        let paper = stripes();
        for &mode in TextureMode::ALL {
            let texture = Texture::new(
                &paper,
                TextureSettings {
                    mode,
                    ..TextureSettings::default()
                },
            );
            let high = texture.apply(0.6, [0.5, 0.5]);
            let low = texture.apply(0.6, [1.5, 0.5]);
            assert!(high > low, "{mode:?}: {high} against {low}");
            assert_eq!(texture.apply(0.0, [0.5, 0.5]), 0.0, "{mode:?}");
        }
    }

    #[test]
    fn height_mode_fills_deeper_as_the_brush_presses_harder() {
        let ramp: Vec<f32> = (0..16 * 16).map(|i| (i % 16) as f32 / 15.0).collect();
        let paper = Paper::from_texels(16, 16, &ramp).expect("a paper");
        let texture = Texture::new(
            &paper,
            TextureSettings {
                mode: TextureMode::Height,
                ..TextureSettings::default()
            },
        );
        let filled = |c: f32| {
            (0..16)
                .filter(|&x| texture.apply(c, [x as f32 + 0.5, 0.5]) > 0.5 * c)
                .count()
        };
        assert!(filled(0.3) < filled(0.6));
        assert!(filled(0.6) < filled(1.0));
        // A full stroke fills about half the paper at full strength: the
        // deepest valleys stay open, as on real tooth.
        assert!((7..=11).contains(&filled(1.0)), "{}", filled(1.0));
    }

    #[test]
    fn invert_brightness_contrast_and_turn_change_the_paper() {
        let paper = stripes();
        let plain = Texture::new(&paper, TextureSettings::default());
        let inverted = Texture::new(
            &paper,
            TextureSettings {
                invert: true,
                ..TextureSettings::default()
            },
        );
        assert!((plain.height([0.5, 0.5]) - (1.0 - inverted.height([0.5, 0.5]))).abs() < 1.0e-5);
        let bright = Texture::new(
            &paper,
            TextureSettings {
                brightness: 0.5,
                ..TextureSettings::default()
            },
        );
        assert!(bright.height([1.5, 0.5]) > plain.height([1.5, 0.5]));
        let flat = Texture::new(
            &paper,
            TextureSettings {
                contrast: -1.0,
                ..TextureSettings::default()
            },
        );
        assert!((flat.height([0.5, 0.5]) - 0.5).abs() < 1.0e-5);
        // A quarter turn makes the columns rows.
        let turned = Texture::new(
            &paper,
            TextureSettings {
                rotation: 90.0,
                ..TextureSettings::default()
            },
        );
        assert!((turned.height([0.5, 3.5]) - turned.height([7.5, 3.5])).abs() < 1.0e-4);
        assert!((turned.height([0.5, 0.5]) - turned.height([0.5, 1.5])).abs() > 0.9);
        // Twice the scale: each stripe two pixels wide.
        let large = Texture::new(
            &paper,
            TextureSettings {
                scale: 2.0,
                ..TextureSettings::default()
            },
        );
        assert!(large.height([1.0, 0.5]) > 0.9 && large.height([3.0, 0.5]) < 0.1);
    }

    #[test]
    fn emphasized_density_shows_more_paper_in_light_strokes() {
        let paper = stripes();
        let settings = TextureSettings {
            strength: 0.5,
            ..TextureSettings::default()
        };
        let plain = Texture::new(&paper, settings);
        let emphasized = Texture::new(
            &paper,
            TextureSettings {
                emphasize: true,
                ..settings
            },
        );
        let valley = [1.5, 0.5];
        assert!(emphasized.apply(0.3, valley) < plain.apply(0.3, valley));
        assert!((emphasized.apply(1.0, valley) - plain.apply(1.0, valley)).abs() < 1.0e-5);
    }
}
