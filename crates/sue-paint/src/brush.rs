//! The brush engine.
//!
//! A stroke goes through a few stages so lines come out clean without the
//! brush feeling slow:
//!
//! 1. **Stabilization** ([`Stabilizer`]): the brush follows the pointer
//!    through a time-based smoothing filter. With *adjust by speed* the
//!    filter loosens as the pen moves faster, so slow careful lines lose
//!    their tremor while quick flicks stay snappy.
//! 2. **Painting while drawing**: pen-like tips (no build-up) are painted as
//!    a continuous tapered capsule between consecutive samples, not as
//!    stamps, so there are no gaps, ridges or beads however fast the pen
//!    moves; every pixel's coverage comes from its exact distance to the
//!    path, so edges are sub-pixel smooth. Airbrushes and mixing brushes
//!    stamp dabs at a tight spacing instead, because their dabs add up.
//! 3. **Starting and ending**: the line grows from a point over its first
//!    pixels and, once the pen lifts, thins to a point over its last ones.
//! 4. **Finishing on lift** ([`Stroke::end`]): the hooks a pen leaves when it
//!    touches down or lifts are trimmed; *post correction* resamples the
//!    path and smooths it into a clean curve while keeping deliberate sharp
//!    corners; a quick *flick* carries the line on along the pen's momentum
//!    into a fine point. The stroke is then painted again from its
//!    corrected path.
//!
//! A stroke never stacks on itself: each touched 64-pixel tile keeps its
//! pixels from before the stroke and a coverage mask, and every pixel is
//! recomputed from those two. A pen at 50% opacity stays at 50% where the
//! stroke crosses itself, and painting the stroke again from its corrected
//! path starts from the untouched pixels. When the stroke ends, the kept
//! tiles and their new pixels become one [`RasterEdit`] for the undo
//! history (backbone rule 2).

use std::collections::BTreeMap;

use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{EntityId, RasterTile2D};
use sue_model::Response;

use crate::select::Selection;

/// Side of an undo tile, in pixels.
pub const TILE: u32 = 64;

/// How soft the edge of a hard brush is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAlias {
    /// Hard pixels.
    None,
    /// A thin soft edge.
    #[default]
    Weak,
    /// A medium soft edge.
    Medium,
    /// A wide soft edge.
    Strong,
}

impl AntiAlias {
    /// Every level, weakest first.
    pub const ALL: [Self; 4] = [Self::None, Self::Weak, Self::Medium, Self::Strong];

    /// Width of the soft edge, in pixels.
    #[must_use]
    pub const fn edge(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Weak => 0.9,
            Self::Medium => 1.5,
            Self::Strong => 2.5,
        }
    }

    /// What people read.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Weak => "Weak",
            Self::Medium => "Medium",
            Self::Strong => "Strong",
        }
    }
}

/// How the pen changes one setting along a stroke, as Clip Studio Paint's
/// dynamics do: each input that is on scales the setting between its
/// minimum and full through its curve, and the inputs multiply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dynamics {
    /// Pen pressure changes it.
    pub pressure: bool,
    /// The share left at the lightest pressure, 0 to 1.
    pub pressure_min: f32,
    /// How pressure becomes the amount.
    pub pressure_curve: Response,
    /// The pen's speed changes it.
    pub velocity: bool,
    /// The share left when the pen is still, 0 to 1.
    pub velocity_min: f32,
    /// How speed (still to fast) becomes the amount.
    pub velocity_curve: Response,
    /// Chance changes it along the stroke.
    pub random: bool,
    /// The least share chance leaves, 0 to 1.
    pub random_min: f32,
}

impl Default for Dynamics {
    fn default() -> Self {
        Self::NONE
    }
}

impl Dynamics {
    /// Nothing changes the setting.
    pub const NONE: Self = Self {
        pressure: false,
        pressure_min: 0.0,
        pressure_curve: Response::LINEAR,
        velocity: false,
        velocity_min: 0.0,
        velocity_curve: Response::LINEAR,
        random: false,
        random_min: 0.0,
    };

    /// Pen pressure changes the setting, down to `min` of it.
    #[must_use]
    pub const fn pressure(min: f32) -> Self {
        Self {
            pressure: true,
            pressure_min: min,
            ..Self::NONE
        }
    }

    /// `true` if any input is on.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.pressure || self.velocity || self.random
    }

    /// The share of the setting at `pressure`, `speed` and `chance` (each
    /// 0 to 1).
    #[must_use]
    pub fn factor(&self, pressure: f32, speed: f32, chance: f32) -> f32 {
        let scale = |on: bool, min: f32, amount: f32| {
            if on {
                let min = min.clamp(0.0, 1.0);
                min + (1.0 - min) * amount.clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        scale(
            self.pressure,
            self.pressure_min,
            self.pressure_curve.eval(pressure),
        ) * scale(
            self.velocity,
            self.velocity_min,
            self.velocity_curve.eval(speed),
        ) * scale(self.random, self.random_min, chance)
    }
}

/// How a stroke's starting and ending are measured (Clip Studio Paint's
/// How to specify).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaperMode {
    /// Starting and ending in pixels.
    #[default]
    Length,
    /// Starting and ending as percentages of the whole stroke.
    Percent,
    /// The stroke fades out by the ending length from where it began.
    Fade,
}

impl TaperMode {
    /// Every mode, as the menu lists them.
    pub const ALL: [Self; 3] = [Self::Length, Self::Percent, Self::Fade];

    /// What people read.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Length => "Specify length",
            Self::Percent => "By percentage",
            Self::Fade => "Fade",
        }
    }
}

/// Color jitter: how far the paint's color wanders (Clip Studio Paint's
/// Color Jitter). Amounts are 0 to 1: hue as a share of half the color
/// wheel, saturation and luminosity as shares of their range, and how far
/// toward the sub color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Jitter {
    /// Change brush tip color: every dab differs (brushes that stamp).
    pub tip: bool,
    /// Hue for each dab.
    pub hue: f32,
    /// Saturation for each dab.
    pub saturation: f32,
    /// Luminosity for each dab.
    pub value: f32,
    /// Blend with sub color for each dab.
    pub sub: f32,
    /// Randomize per stroke: the whole stroke differs.
    pub stroke: bool,
    /// Hue for each stroke.
    pub stroke_hue: f32,
    /// Saturation for each stroke.
    pub stroke_saturation: f32,
    /// Luminosity for each stroke.
    pub stroke_value: f32,
}

/// `color` moved by up to `amounts` (hue, saturation, luminosity, toward
/// `sub`) with the four chances in `chance` (0 to 1).
fn jittered(color: [f32; 3], sub: [f32; 3], amounts: [f32; 4], chance: [f32; 4]) -> [f32; 3] {
    let spread = |amount: f32, chance: f32| (chance * 2.0 - 1.0) * amount.clamp(0.0, 1.0);
    let mut hsv = crate::color::Hsv::from_rgb(color);
    hsv.h = (hsv.h + spread(amounts[0], chance[0]) * 180.0).rem_euclid(360.0);
    hsv.s = (hsv.s + spread(amounts[1], chance[1])).clamp(0.0, 1.0);
    hsv.v = (hsv.v + spread(amounts[2], chance[2])).clamp(0.0, 1.0);
    let rgb = hsv.to_rgb();
    let toward = chance[3] * amounts[3].clamp(0.0, 1.0);
    [0, 1, 2].map(|i| rgb[i] + (sub[i] - rgb[i]) * toward)
}

/// A steady chance from 0 to 1 for `seed` and `salt`.
fn noise(seed: u32, salt: u32) -> f32 {
    let mut h = seed ^ salt.wrapping_mul(0x9E37_79B1);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// The shape and behavior of a brush.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    /// Diameter in pixels.
    pub size: f32,
    /// 0 to 1.
    pub opacity: f32,
    /// 1 is a hard disc; lower values fade toward the rim.
    pub hardness: f32,
    /// Edge softness of hard tips.
    pub aa: AntiAlias,
    /// Distance between dabs of dabbed brushes, as a fraction of the size.
    pub spacing: f32,
    /// Dabs add up (airbrush) instead of reaching the opacity at most (pen).
    pub build_up: bool,
    /// Strength of one dab when building up, 0 to 1.
    pub flow: f32,
    /// Paper grain that breaks up the stroke (pencils), 0 to 1.
    pub grain: f32,
    /// What changes the size along a stroke (Brush Size Dynamics).
    pub size_by: Dynamics,
    /// What changes the opacity along a stroke (Opacity Dynamics).
    pub opacity_by: Dynamics,
    /// The size is measured on the screen, so the brush looks the same at
    /// any zoom (Specify by size on screen).
    pub screen_size: bool,
    /// The line never gets thinner than a pixel, even at the lightest
    /// pressure (At least 1 pixel).
    pub at_least_one: bool,
    /// Removes pixels instead of painting them.
    pub eraser: bool,
    /// Length over which a stroke grows from a point to full size, in
    /// pixels.
    pub taper_start: f32,
    /// Length over which a stroke thins to a point at its end, in pixels
    /// (a percentage of the stroke `By percentage`, and the length it fades
    /// out by with `Fade`).
    pub taper_end: f32,
    /// How the starting and ending are measured.
    pub taper_mode: TaperMode,
    /// The starting and ending thin the brush.
    pub taper_size: bool,
    /// The starting and ending fade the paint.
    pub taper_opacity: bool,
    /// Quicker strokes get longer starts and ends (Starting and ending by
    /// speed).
    pub taper_by_speed: bool,
    /// How far the color wanders.
    pub jitter: Jitter,
    /// How much of the color under the brush it picks up and drags along,
    /// 0 to 1 (painting brushes).
    pub mix: f32,
}

impl Default for Tip {
    fn default() -> Self {
        Self {
            size: 8.0,
            opacity: 1.0,
            hardness: 1.0,
            aa: AntiAlias::Weak,
            spacing: 0.06,
            build_up: false,
            flow: 1.0,
            grain: 0.0,
            size_by: Dynamics::pressure(0.15),
            opacity_by: Dynamics::NONE,
            screen_size: false,
            at_least_one: false,
            eraser: false,
            taper_start: 0.0,
            taper_end: 0.0,
            taper_mode: TaperMode::Length,
            taper_size: true,
            taper_opacity: false,
            taper_by_speed: false,
            jitter: Jitter::default(),
            mix: 0.0,
        }
    }
}

impl Tip {
    /// Radius and strength of the brush at `pressure` (0 to 1), the pen
    /// still and chance at its fullest.
    #[must_use]
    pub fn dab(&self, pressure: f32) -> (f32, f32) {
        self.dab_at(pressure, 0.0, 1.0)
    }

    /// Radius and strength of the brush at `pressure`, `speed` and
    /// `chance` (each 0 to 1), through its dynamics.
    #[must_use]
    pub fn dab_at(&self, pressure: f32, speed: f32, chance: f32) -> (f32, f32) {
        let scale = self.size_by.factor(pressure, speed, chance);
        let strength = self.opacity * self.opacity_by.factor(pressure, speed, chance);
        ((self.size * 0.5 * scale).max(0.0), strength.clamp(0.0, 1.0))
    }

    /// How much of a disc of `radius` covers a pixel whose center is
    /// `distance` from its center, 0 to 1.
    #[must_use]
    pub fn coverage(&self, distance: f32, radius: f32) -> f32 {
        if self.hardness >= 0.99 {
            let edge = self.aa.edge();
            if edge <= 0.0 {
                if distance <= radius { 1.0 } else { 0.0 }
            } else {
                ((radius - distance) / edge + 0.5).clamp(0.0, 1.0)
            }
        } else {
            let t = distance / radius.max(1e-3);
            if t >= 1.0 {
                return 0.0;
            }
            let inner = self.hardness.clamp(0.0, 0.98);
            if t <= inner {
                1.0
            } else {
                let u = (t - inner) / (1.0 - inner);
                1.0 - u * u * (3.0 - 2.0 * u)
            }
        }
    }

    /// `true` if the brush stamps dabs that add up (airbrushes, mixing
    /// brushes) rather than painting one continuous line.
    #[must_use]
    pub fn dabbed(&self) -> bool {
        self.build_up || self.mix > 0.0
    }
}

/// Deterministic paper grain at a pixel, 0 to 1.
fn grain_at(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// One point of a stroke: where (layer pixels) and how hard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Position in the layer's pixels.
    pub at: [f32; 2],
    /// Pen pressure, 0 to 1.
    pub pressure: f32,
    /// How fast the pen moved there, 0 (still) to 1 (fast).
    pub speed: f32,
}

/// How a stroke is finished when the pen lifts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Finish {
    /// Post correction, 0 (none) to 10: the path is smoothed into a clean
    /// curve, keeping sharp corners.
    pub correction: u32,
    /// The pen's velocity as it lifted (layer pixels per second), to carry
    /// a quick flick on into a fine point. `None` ends where the pen lifted.
    pub flick: Option<[f32; 2]>,
}

/// A tile's pixels before the stroke and how much the stroke covers them.
#[derive(Clone, Debug)]
struct TileState {
    rect: [u32; 4],
    before: Vec<u8>,
    mask: Vec<f32>,
}

/// How painted coverage turns into pixels.
#[derive(Clone, Copy, Debug)]
enum Mode {
    /// The highest coverage wins (pens): the stroke never stacks on itself.
    Max,
    /// Coverage adds up (airbrushes).
    Accumulate,
    /// Each dab is laid straight over the pixels in its own color (mixing
    /// brushes, whose color changes along the stroke).
    Direct([f32; 3]),
}

/// A stroke in progress on one layer.
#[derive(Clone, Debug)]
pub struct Stroke {
    layer: EntityId,
    tip: Tip,
    color: [f32; 3],
    alpha_lock: bool,
    tiles: BTreeMap<(u32, u32), TileState>,
    samples: Vec<Sample>,
    travelled: f32,
    since_dab: f32,
    carried: [f32; 3],
    last_dab: Option<[f32; 2]>,
    total: Option<f32>,
    flicked: f32,
    dirty: Option<[u32; 4]>,
    speed: f32,
    seed: u32,
    sub: [f32; 3],
    end_speeds: Option<(f32, f32)>,
}

impl Stroke {
    /// A stroke with `tip` in `color` (sRGB, 0 to 1) on `layer`.
    /// `alpha_lock` keeps the layer's alpha (painting only over pixels).
    #[must_use]
    pub fn new(layer: EntityId, tip: Tip, color: [f32; 3], alpha_lock: bool) -> Self {
        Self {
            layer,
            tip,
            color,
            alpha_lock,
            tiles: BTreeMap::new(),
            samples: Vec::new(),
            travelled: 0.0,
            since_dab: 0.0,
            carried: color,
            last_dab: None,
            total: None,
            flicked: 0.0,
            dirty: None,
            speed: 0.0,
            seed: 0,
            sub: [1.0; 3],
            end_speeds: None,
        }
    }

    /// The sub color, for color jitter that blends toward it.
    pub fn set_sub_color(&mut self, sub: [f32; 3]) {
        self.sub = sub;
    }

    /// How fast the pen moved as the stroke began and as it ended (the
    /// ending is known once the pen lifts).
    fn speeds(&self) -> (f32, f32) {
        if let Some(speeds) = self.end_speeds {
            return speeds;
        }
        let mean = |samples: &[Sample]| {
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().map(|sample| sample.speed).sum::<f32>() / samples.len() as f32
            }
        };
        let head = &self.samples[..self.samples.len().min(6)];
        let start = mean(head);
        (start, start)
    }

    /// How fast the pen moves now, 0 (still) to 1 (fast), for the samples
    /// that follow (velocity dynamics).
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.0, 1.0);
    }

    /// A steady chance from 0 to 1 for the point `s` along the stroke: the
    /// same each time the stroke is painted again (random dynamics).
    fn chance(&self, s: f32) -> f32 {
        let step = (s * 0.5).floor() as i32 as u32;
        let mut h = step.wrapping_mul(0x9E37_79B1) ^ self.seed;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        (h & 0xFFFF) as f32 / 65_535.0
    }

    /// The layer it paints on.
    #[must_use]
    pub fn layer(&self) -> EntityId {
        self.layer
    }

    /// The tip it paints with.
    #[must_use]
    pub fn tip(&self) -> &Tip {
        &self.tip
    }

    /// The path painted so far.
    #[must_use]
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// Continues the stroke to `at` (layer pixels) at `pressure`.
    pub fn to(
        &mut self,
        raster: &mut RasterTile2D,
        at: [f32; 2],
        pressure: f32,
        selection: Option<&Selection>,
    ) {
        let sample = Sample {
            at,
            pressure,
            speed: self.speed,
        };
        if self.seed == 0 {
            self.seed = (at[0].to_bits() ^ at[1].to_bits().rotate_left(16)) | 1;
            let jitter = self.tip.jitter;
            if jitter.stroke && !self.tip.eraser {
                // Randomize per stroke: one color for the whole stroke.
                let chance = [1, 2, 3, 4].map(|salt| noise(self.seed, salt));
                self.color = jittered(
                    self.color,
                    self.sub,
                    [
                        jitter.stroke_hue,
                        jitter.stroke_saturation,
                        jitter.stroke_value,
                        0.0,
                    ],
                    chance,
                );
                self.carried = self.color;
            }
        }
        match self.samples.last().copied() {
            None => {
                if self.tip.dabbed() {
                    self.dab(raster, sample, 0.0, selection);
                } else {
                    self.capsule(raster, sample, sample, 0.0, 0.0, selection);
                }
            }
            Some(last) => {
                let length = distance(last.at, at);
                if length < 1e-3 {
                    return;
                }
                let start = self.travelled;
                if self.tip.dabbed() {
                    self.dabs(raster, last, sample, start, length, selection);
                } else {
                    self.capsule(raster, last, sample, start, start + length, selection);
                }
                self.travelled += length;
            }
        }
        self.samples.push(sample);
    }

    /// Finishes the stroke as the pen lifts: trims the hooks of touching
    /// down and lifting, applies post correction and a flick, and paints the
    /// stroke again from that path with its end tapered.
    pub fn end(
        &mut self,
        raster: &mut RasterTile2D,
        finish: Finish,
        selection: Option<&Selection>,
    ) {
        let finishing = finish.correction > 0
            || finish.flick.is_some()
            || self.tip.taper_end > 0.0
            || (self.tip.taper_start > 0.0
                && (self.tip.taper_mode == TaperMode::Percent || self.tip.taper_by_speed));
        if self.samples.len() < 2 || !finishing {
            return;
        }
        // A smooth curve through the samples, so even a fast mouse that
        // reported only a few points ends as a curve, not a polygon.
        let step = (self.tip.size * 0.05).clamp(0.75, 8.0);
        let mut path = spline(&trim_hooks(&self.samples, self.tip.size), step);
        if finish.correction > 0 {
            path = correct(&path, finish.correction, self.tip.size);
        }
        let before = length(&path);
        if let Some(velocity) = finish.flick {
            flick(&mut path, velocity, self.tip.size);
        }
        if path.is_empty() {
            return;
        }
        // The pen's speed at both ends, from the samples as drawn.
        let mean = |samples: &[Sample]| {
            samples.iter().map(|sample| sample.speed).sum::<f32>() / samples.len().max(1) as f32
        };
        let count = self.samples.len();
        self.end_speeds = Some((
            mean(&self.samples[..count.min(6)]),
            mean(&self.samples[count.saturating_sub(6)..]),
        ));
        self.restart(raster);
        self.total = Some(length(&path));
        self.flicked = self.total.unwrap_or(0.0) - before;
        for sample in path {
            self.speed = sample.speed;
            self.to(raster, sample.at, sample.pressure, selection);
        }
        self.total = None;
        self.flicked = 0.0;
        self.end_speeds = None;
    }

    /// Puts back the pixels from before the stroke, to paint it again.
    fn restart(&mut self, raster: &mut RasterTile2D) {
        let stride = raster.width() as usize * 4;
        if let Some(bytes) = raster.bytes_mut() {
            for tile in self.tiles.values_mut() {
                let [x, y, w, h] = tile.rect;
                let row = w as usize * 4;
                for line in 0..h as usize {
                    let at = (y as usize + line) * stride + x as usize * 4;
                    bytes[at..at + row].copy_from_slice(&tile.before[line * row..(line + 1) * row]);
                }
                tile.mask.fill(0.0);
                self.dirty = Some(union(self.dirty, [x, y, x + w, y + h]));
            }
        }
        self.samples.clear();
        self.travelled = 0.0;
        self.since_dab = 0.0;
        self.carried = self.color;
        self.last_dab = None;
    }

    /// How much of its size the stroke has at arc length `s`: it grows over
    /// the start taper and, once the length is known, thins over the end
    /// taper. Short strokes scale both tapers to fit.
    fn taper(&self, s: f32) -> f32 {
        let ease = |x: f32| {
            let x = x.clamp(0.0, 1.0);
            1.0 - (1.0 - x) * (1.0 - x)
        };
        let tip = &self.tip;
        let (mut start, mut end) = match (tip.taper_mode, self.total) {
            (TaperMode::Percent, Some(total)) => (
                tip.taper_start / 100.0 * total,
                tip.taper_end / 100.0 * total,
            ),
            // While drawing the length is not known yet: start as if the
            // stroke were 200 pixels long, and end once it is known.
            (TaperMode::Percent, None) => (tip.taper_start / 100.0 * 200.0, 0.0),
            _ => (tip.taper_start, tip.taper_end),
        };
        start = start.max(0.0);
        end = end.max(0.0);
        if tip.taper_by_speed {
            let (first, last) = self.speeds();
            start *= 0.3 + 1.4 * first;
            end *= 0.3 + 1.4 * last;
        }
        if tip.taper_mode == TaperMode::Fade {
            // Fade: gone by the ending length, however long the stroke.
            let up = if start > 0.0 { ease(s / start) } else { 1.0 };
            let down = if end > 0.0 { ease(1.0 - s / end) } else { 1.0 };
            return up.min(down);
        }
        // A flick thins all the way along the stretch it added.
        end = end.max(self.flicked * 1.2);
        match self.total {
            Some(total) => {
                if total < 1.0 {
                    return 1.0;
                }
                let fit = (start + end) / (0.8 * total);
                if fit > 1.0 {
                    start /= fit;
                    end /= fit;
                }
            }
            None => end = 0.0,
        }
        let up = if start > 0.0 { ease(s / start) } else { 1.0 };
        let down = match self.total {
            Some(total) if end > 0.0 => ease((total - s) / end),
            _ => 1.0,
        };
        up.min(down)
    }

    /// Paints the tapered capsule from `a` (arc length `s0`) to `b` (`s1`).
    fn capsule(
        &mut self,
        raster: &mut RasterTile2D,
        a: Sample,
        b: Sample,
        s0: f32,
        s1: f32,
        selection: Option<&Selection>,
    ) {
        let (ra, sa) = self.tip.dab_at(a.pressure, a.speed, self.chance(s0));
        let (rb, sb) = self.tip.dab_at(b.pressure, b.speed, self.chance(s1));
        let (ta, tb) = (self.taper(s0), self.taper(s1));
        let (ra, rb) = if self.tip.taper_size {
            (ra * ta, rb * tb)
        } else {
            (ra, rb)
        };
        let (sa, sb) = if self.tip.taper_opacity {
            (sa * ta, sb * tb)
        } else {
            (sa, sb)
        };
        let reach = ra.max(rb).max(0.7) + self.tip.aa.edge() + 1.0;
        let rect = [
            a.at[0].min(b.at[0]) - reach,
            a.at[1].min(b.at[1]) - reach,
            a.at[0].max(b.at[0]) + reach,
            a.at[1].max(b.at[1]) + reach,
        ];
        let tip = self.tip;
        let (ax, ay) = (a.at[0], a.at[1]);
        let (dx, dy) = (b.at[0] - ax, b.at[1] - ay);
        let length_sq = dx * dx + dy * dy;
        self.apply(raster, rect, selection, Mode::Max, |px, py| {
            let t = if length_sq > 0.0 {
                (((px - ax) * dx + (py - ay) * dy) / length_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (cx, cy) = (ax + dx * t, ay + dy * t);
            let d = ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt();
            let mut radius = ra + (rb - ra) * t;
            let mut strength = sa + (sb - sa) * t;
            // A line thinner than a pixel keeps its weight as lighter ink,
            // unless it is never to get thinner than a pixel.
            if tip.at_least_one {
                radius = radius.max(0.5);
            } else if radius < 0.7 {
                strength *= radius / 0.7;
                radius = 0.7;
            }
            tip.coverage(d, radius) * strength
        });
    }

    /// Stamps dabs every `spacing` from `a` (arc length `start`) to `b`.
    fn dabs(
        &mut self,
        raster: &mut RasterTile2D,
        a: Sample,
        b: Sample,
        start: f32,
        length: f32,
        selection: Option<&Selection>,
    ) {
        let (radius, _) = self
            .tip
            .dab_at(a.pressure.max(b.pressure), a.speed.max(b.speed), 1.0);
        let step = (radius * 2.0 * self.tip.spacing).max(0.25);
        let mut along = step - self.since_dab;
        while along <= length {
            let t = along / length;
            let sample = Sample {
                at: [
                    a.at[0] + (b.at[0] - a.at[0]) * t,
                    a.at[1] + (b.at[1] - a.at[1]) * t,
                ],
                pressure: a.pressure + (b.pressure - a.pressure) * t,
                speed: a.speed + (b.speed - a.speed) * t,
            };
            self.dab(raster, sample, start + along, selection);
            along += step;
        }
        self.since_dab = length - (along - step);
    }

    /// Stamps one dab at `sample`, `s` along the stroke.
    fn dab(
        &mut self,
        raster: &mut RasterTile2D,
        sample: Sample,
        s: f32,
        selection: Option<&Selection>,
    ) {
        let (mut radius, mut strength) =
            self.tip
                .dab_at(sample.pressure, sample.speed, self.chance(s));
        let taper = self.taper(s);
        if self.tip.taper_size {
            radius *= taper;
        }
        if self.tip.taper_opacity {
            strength *= taper;
        }
        // Tiny dabs keep their area instead of vanishing between pixels.
        if self.tip.at_least_one {
            radius = radius.max(0.5);
        } else if radius < 0.7 {
            strength *= (radius / 0.7).powi(2);
            radius = 0.7;
        }
        strength *= self.tip.flow;
        let [x, y] = sample.at;
        let mode = if self.tip.mix > 0.0 && !self.tip.eraser {
            // The brush picks up the color under it and is reloaded with its
            // own paint, both per distance travelled (not per dab), so the
            // stroke settles at `mix` of the canvas color.
            let moved = self
                .last_dab
                .map_or(radius, |last| distance(last, sample.at));
            self.last_dab = Some(sample.at);
            if let Some((under, alpha)) = average(raster, sample.at, radius) {
                let pick = self.tip.mix * alpha;
                let rate = 1.0 - (-2.0 * moved / (radius * 2.0).max(1.0)).exp();
                for ((carried, under), own) in self.carried.iter_mut().zip(under).zip(self.color) {
                    let target = own + (under - own) * pick;
                    *carried += (target - *carried) * rate;
                }
            }
            Mode::Direct(self.tip_color(self.carried, s))
        } else if self.tip.jitter.tip && !self.tip.eraser {
            Mode::Direct(self.tip_color(self.color, s))
        } else if self.tip.build_up {
            Mode::Accumulate
        } else {
            Mode::Max
        };
        let reach = radius + self.tip.aa.edge() + 1.0;
        let tip = self.tip;
        self.apply(
            raster,
            [x - reach, y - reach, x + reach, y + reach],
            selection,
            mode,
            |px, py| {
                let d = ((px - x) * (px - x) + (py - y) * (py - y)).sqrt();
                tip.coverage(d, radius) * strength
            },
        );
    }

    /// `color` with the tip's color jitter at `s` along the stroke.
    fn tip_color(&self, color: [f32; 3], s: f32) -> [f32; 3] {
        let jitter = self.tip.jitter;
        if !jitter.tip {
            return color;
        }
        let step = (s * 4.0) as u32;
        let chance =
            [11, 12, 13, 14].map(|salt| noise(self.seed ^ step.wrapping_mul(0x27D4_EB2F), salt));
        jittered(
            color,
            self.sub,
            [jitter.hue, jitter.saturation, jitter.value, jitter.sub],
            chance,
        )
    }

    /// Lays coverage from `amount` (per pixel center, already including
    /// strength) over the pixels of `rect`, keeping every touched tile's
    /// old pixels first.
    fn apply(
        &mut self,
        raster: &mut RasterTile2D,
        rect: [f32; 4],
        selection: Option<&Selection>,
        mode: Mode,
        amount: impl Fn(f32, f32) -> f32,
    ) {
        let (width, height) = (raster.width(), raster.height());
        let x0 = rect[0].floor().max(0.0) as u32;
        let y0 = rect[1].floor().max(0.0) as u32;
        let x1 = (rect[2].ceil().max(0.0) as u32).min(width);
        let y1 = (rect[3].ceil().max(0.0) as u32).min(height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = width as usize * 4;
        let (tip, color, alpha_lock) = (self.tip, self.color, self.alpha_lock);
        let Some(bytes) = raster.bytes_mut() else {
            return;
        };
        for ty in (y0 / TILE)..=((y1 - 1) / TILE) {
            for tx in (x0 / TILE)..=((x1 - 1) / TILE) {
                let state = self.tiles.entry((tx, ty)).or_insert_with(|| {
                    let rect = [
                        tx * TILE,
                        ty * TILE,
                        TILE.min(width - tx * TILE),
                        TILE.min(height - ty * TILE),
                    ];
                    let mut before = Vec::with_capacity(rect[2] as usize * rect[3] as usize * 4);
                    for line in 0..rect[3] as usize {
                        let at = (rect[1] as usize + line) * stride + rect[0] as usize * 4;
                        before.extend_from_slice(&bytes[at..at + rect[2] as usize * 4]);
                    }
                    TileState {
                        rect,
                        before,
                        mask: vec![0.0; rect[2] as usize * rect[3] as usize],
                    }
                });
                let [rx, ry, rw, rh] = state.rect;
                for y in y0.max(ry)..y1.min(ry + rh) {
                    for x in x0.max(rx)..x1.min(rx + rw) {
                        let mut cover = amount(x as f32 + 0.5, y as f32 + 0.5);
                        if cover <= 0.0 {
                            continue;
                        }
                        if tip.grain > 0.0 {
                            cover *= 1.0 - tip.grain * grain_at(x, y);
                        }
                        if let Some(selection) = selection {
                            cover *= selection.at(x, y);
                        }
                        let local = ((y - ry) * rw + (x - rx)) as usize;
                        let at = y as usize * stride + x as usize * 4;
                        let mask = &mut state.mask[local];
                        let old = *mask;
                        let pixel = match mode {
                            Mode::Max | Mode::Accumulate => {
                                *mask = match mode {
                                    Mode::Max => old.max(cover),
                                    _ => old + (1.0 - old) * cover,
                                };
                                if (*mask - old).abs() < 1e-6 {
                                    continue;
                                }
                                let before = &state.before[local * 4..local * 4 + 4];
                                paint_pixel(before, *mask, color, tip.eraser, alpha_lock)
                            }
                            Mode::Direct(rgb) => {
                                *mask = old + (1.0 - old) * cover;
                                let now = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
                                paint_pixel(&now, cover, rgb, tip.eraser, alpha_lock)
                            }
                        };
                        bytes[at..at + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
        self.dirty = Some(union(self.dirty, [x0, y0, x1, y1]));
    }

    /// The rectangle `[x0, y0, x1, y1]` (layer pixels) painted since the
    /// last call.
    pub fn take_dirty(&mut self) -> Option<[u32; 4]> {
        self.dirty.take()
    }

    /// Ends the stroke: the undo step holding the tiles it changed.
    #[must_use]
    pub fn finish(self, raster: &RasterTile2D, label: &str) -> RasterEdit {
        let patches = self
            .tiles
            .into_values()
            .filter_map(|tile| RasterPatch::capture(raster, tile.rect, tile.before))
            .filter(|patch| patch.before != patch.after)
            .collect();
        RasterEdit::new(self.layer, label, patches)
    }
}

fn union(rect: Option<[u32; 4]>, [x0, y0, x1, y1]: [u32; 4]) -> [u32; 4] {
    match rect {
        Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
        None => [x0, y0, x1, y1],
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn length(path: &[Sample]) -> f32 {
    path.windows(2)
        .map(|pair| distance(pair[0].at, pair[1].at))
        .sum()
}

/// The average color (straight sRGB) and alpha of the pixels under a disc.
fn average(raster: &RasterTile2D, center: [f32; 2], radius: f32) -> Option<([f32; 3], f32)> {
    let bytes = raster.bytes()?;
    let (width, height) = (raster.width() as i64, raster.height() as i64);
    let step = ((radius / 4.0).floor() as i64).max(1);
    let reach = radius.ceil() as i64;
    let (cx, cy) = (center[0] as i64, center[1] as i64);
    let mut sum = [0.0_f32; 4];
    let mut count = 0.0;
    let mut y = cy - reach;
    while y <= cy + reach {
        let mut x = cx - reach;
        while x <= cx + reach {
            if x >= 0 && y >= 0 && x < width && y < height {
                let at = ((y * width + x) * 4) as usize;
                let alpha = f32::from(bytes[at + 3]) / 255.0;
                for channel in 0..3 {
                    sum[channel] += f32::from(bytes[at + channel]) / 255.0 * alpha;
                }
                sum[3] += alpha;
                count += 1.0;
            }
            x += step;
        }
        y += step;
    }
    if count <= 0.0 || sum[3] <= 1e-4 {
        return Some(([0.0; 3], 0.0));
    }
    Some((
        [sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3]],
        sum[3] / count,
    ))
}

/// The pixel `before` becomes under a stroke covering it by `amount`.
fn paint_pixel(
    before: &[u8],
    amount: f32,
    color: [f32; 3],
    eraser: bool,
    alpha_lock: bool,
) -> [u8; 4] {
    let to_byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let old = [
        f32::from(before[0]) / 255.0,
        f32::from(before[1]) / 255.0,
        f32::from(before[2]) / 255.0,
        f32::from(before[3]) / 255.0,
    ];
    if eraser {
        return [
            before[0],
            before[1],
            before[2],
            to_byte(old[3] * (1.0 - amount)),
        ];
    }
    if alpha_lock {
        let mix = |channel: usize| old[channel] + (color[channel] - old[channel]) * amount;
        return [to_byte(mix(0)), to_byte(mix(1)), to_byte(mix(2)), before[3]];
    }
    let out = sue_model::BlendMode::Normal.composite(old, [color[0], color[1], color[2], amount]);
    [
        to_byte(out[0]),
        to_byte(out[1]),
        to_byte(out[2]),
        to_byte(out[3]),
    ]
}

// ----- Finishing a path ---------------------------------------------------------

/// The path without the little hooks a pen leaves when it touches down or
/// lifts: a last (or first) stretch shorter than the brush that turns back
/// on the line.
fn trim_hooks(samples: &[Sample], size: f32) -> Vec<Sample> {
    let mut path: Vec<Sample> = Vec::with_capacity(samples.len());
    for sample in samples {
        match path.last_mut() {
            Some(last) if distance(last.at, sample.at) < 0.35 => {
                last.pressure = sample.pressure;
                last.speed = sample.speed;
            }
            _ => path.push(*sample),
        }
    }
    if path.len() < 5 {
        return path;
    }
    let hook = (size * 0.75).max(3.0);
    path = cut_hook(path, hook);
    path.reverse();
    path = cut_hook(path, hook);
    path.reverse();
    path
}

/// Cuts a hook off the end of `path`.
fn cut_hook(mut path: Vec<Sample>, hook: f32) -> Vec<Sample> {
    let last = path.len() - 1;
    let mut tail = 0.0;
    let mut i = last;
    while i > 0 && tail < hook {
        tail += distance(path[i].at, path[i - 1].at);
        i -= 1;
    }
    let mut j = i;
    let mut before = 0.0;
    while j > 0 && before < hook * 2.0 {
        before += distance(path[j].at, path[j - 1].at);
        j -= 1;
    }
    if i == 0 || j == i || before < hook {
        return path;
    }
    let way = [path[i].at[0] - path[j].at[0], path[i].at[1] - path[j].at[1]];
    let end = [
        path[last].at[0] - path[i].at[0],
        path[last].at[1] - path[i].at[1],
    ];
    let (a, b) = (way[0].hypot(way[1]), end[0].hypot(end[1]));
    if a > 0.5 && b > 0.5 && (way[0] * end[0] + way[1] * end[1]) / (a * b) < -0.2 {
        path.truncate(i + 1);
    }
    path
}

/// The path resampled every `step` pixels along its length.
fn resample(path: &[Sample], step: f32) -> Vec<Sample> {
    let Some(first) = path.first() else {
        return Vec::new();
    };
    let mut out = vec![*first];
    let mut since = 0.0;
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = distance(a.at, b.at);
        if length <= 0.0 {
            continue;
        }
        let mut along = step - since;
        while along <= length {
            let t = along / length;
            out.push(Sample {
                at: [
                    a.at[0] + (b.at[0] - a.at[0]) * t,
                    a.at[1] + (b.at[1] - a.at[1]) * t,
                ],
                pressure: a.pressure + (b.pressure - a.pressure) * t,
                speed: a.speed + (b.speed - a.speed) * t,
            });
            along += step;
        }
        since = length - (along - step);
    }
    if let Some(last) = path.last()
        && out
            .last()
            .is_some_and(|end| distance(end.at, last.at) > 1e-3)
    {
        out.push(*last);
    }
    out
}

/// Post correction: the path resampled evenly, its sharp corners found,
/// and everything between them smoothed along its length by `level` (1 to
/// 10): a gaussian a few pixels wide that grows with the level and the
/// brush. Ends and corners stay where they were.
fn correct(path: &[Sample], level: u32, size: f32) -> Vec<Sample> {
    let step = (size * 0.05).clamp(0.75, 8.0);
    let points = resample(path, step);
    let count = points.len();
    if count < 5 {
        return points;
    }
    // Corners: a turn of more than about 65 degrees between arms at least
    // a brush wide, the sharpest turn around it, found on a lightly smoothed
    // copy so that tremor does not count as corners.
    let calm = gaussian(&points, &vec![false; count], (size * 0.2).max(1.5) / step);
    let reach = ((size.max(6.0) / step).ceil() as usize).clamp(3, count / 3 + 1);
    let turn = |i: usize| {
        let (a, b, c) = (calm[i - reach].at, calm[i].at, calm[i + reach].at);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let (lu, lv) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
        if lu < 1e-3 || lv < 1e-3 {
            0.0
        } else {
            ((u[0] * v[0] + u[1] * v[1]) / (lu * lv))
                .clamp(-1.0, 1.0)
                .acos()
        }
    };
    let mut fixed = vec![false; count];
    if count > reach * 2 {
        let turns: Vec<f32> = (0..count)
            .map(|i| {
                if i >= reach && i + reach < count {
                    turn(i)
                } else {
                    0.0
                }
            })
            .collect();
        for i in reach..count - reach {
            let local = turns[i - reach..=i + reach]
                .iter()
                .copied()
                .fold(0.0, f32::max);
            if turns[i] > 1.13 && turns[i] >= local {
                fixed[i] = true;
            }
        }
    }
    let sigma = level.min(10) as f32 * 0.5 * (size * 0.12).max(1.0);
    gaussian(&points, &fixed, sigma / step)
}

/// Gaussian smoothing of evenly spaced points, `sigma` measured in points.
/// The ends and `fixed` points stay put; each run between them is mirrored
/// through its ends, so the line is not pulled toward them.
fn gaussian(points: &[Sample], fixed: &[bool], sigma: f32) -> Vec<Sample> {
    let count = points.len();
    if sigma <= 0.0 || count < 3 {
        return points.to_vec();
    }
    let reach = (sigma * 3.0).ceil() as isize;
    let weights: Vec<f32> = (-reach..=reach)
        .map(|k| (-((k * k) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let anchors: Vec<usize> = (0..count)
        .filter(|i| *i == 0 || *i == count - 1 || fixed[*i])
        .collect();
    let mut out = points.to_vec();
    for run in anchors.windows(2) {
        let (a, b) = (run[0] as isize, run[1] as isize);
        let mirrored = |anchor: isize, other: isize| {
            let (p, q) = (
                points[anchor as usize].at,
                points[other.clamp(a, b) as usize].at,
            );
            [2.0 * p[0] - q[0], 2.0 * p[1] - q[1]]
        };
        for i in a + 1..b {
            let mut sum = [0.0_f32; 2];
            let mut total = 0.0;
            for (offset, weight) in (-reach..=reach).zip(&weights) {
                let j = i + offset;
                let at = if j < a {
                    mirrored(a, 2 * a - j)
                } else if j > b {
                    mirrored(b, 2 * b - j)
                } else {
                    points[j as usize].at
                };
                sum[0] += at[0] * weight;
                sum[1] += at[1] * weight;
                total += weight;
            }
            out[i as usize].at = [sum[0] / total, sum[1] / total];
        }
    }
    out
}

/// Carries a quick stroke on along `velocity` (layer pixels per second),
/// so a flick runs out into a fine point (the end taper thins it).
fn flick(path: &mut Vec<Sample>, velocity: [f32; 2], size: f32) {
    let Some(last) = path.last().copied() else {
        return;
    };
    let speed = velocity[0].hypot(velocity[1]);
    if speed < 1.0 {
        return;
    }
    let reach = (speed * 0.02).min(size * 4.0 + 16.0).min(120.0);
    let direction = [velocity[0] / speed, velocity[1] / speed];
    let steps = reach.ceil().max(1.0) as usize;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        path.push(Sample {
            at: [
                last.at[0] + direction[0] * reach * t,
                last.at[1] + direction[1] * reach * t,
            ],
            pressure: last.pressure * (1.0 - 0.5 * t),
            speed: last.speed,
        });
    }
}

/// A smooth curve through the samples (a centripetal Catmull-Rom spline,
/// which never loops or overshoots), sampled about every `step` pixels.
/// Sharp turns between longer stretches stay sharp.
fn spline(path: &[Sample], step: f32) -> Vec<Sample> {
    let count = path.len();
    if count < 3 {
        return path.to_vec();
    }
    let corner = |i: usize| {
        if i == 0 || i + 1 >= count {
            return true;
        }
        let (a, b, c) = (path[i - 1].at, path[i].at, path[i + 1].at);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let (lu, lv) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
        lu >= 3.0 && lv >= 3.0 && (u[0] * v[0] + u[1] * v[1]) / (lu * lv) < -0.17
    };
    let mirror = |p: [f32; 2], q: [f32; 2]| [2.0 * p[0] - q[0], 2.0 * p[1] - q[1]];
    let mut out = vec![path[0]];
    for i in 0..count - 1 {
        let (p1, p2) = (path[i].at, path[i + 1].at);
        let p0 = if corner(i) {
            mirror(p1, p2)
        } else {
            path[i - 1].at
        };
        let p3 = if corner(i + 1) {
            mirror(p2, p1)
        } else {
            path[i + 2].at
        };
        let steps = (distance(p1, p2) / step.max(0.25)).ceil().max(1.0) as usize;
        for step in 1..=steps {
            let u = step as f32 / steps as f32;
            out.push(Sample {
                at: catmull_rom([p0, p1, p2, p3], u),
                pressure: path[i].pressure + (path[i + 1].pressure - path[i].pressure) * u,
                speed: path[i].speed + (path[i + 1].speed - path[i].speed) * u,
            });
        }
    }
    out
}

/// A point `u` (0 to 1) of the way from `p1` to `p2` on a centripetal
/// Catmull-Rom spline.
fn catmull_rom([p0, p1, p2, p3]: [[f32; 2]; 4], u: f32) -> [f32; 2] {
    let knot = |a: [f32; 2], b: [f32; 2]| distance(a, b).sqrt().max(1e-4);
    let t1 = knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + (t2 - t1) * u;
    let lerp = |a: [f32; 2], b: [f32; 2], ta: f32, tb: f32| {
        let w = (t - ta) / (tb - ta);
        [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w]
    };
    let a1 = lerp(p0, p1, 0.0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(a1, a2, 0.0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

// ----- Stabilization ------------------------------------------------------------------

/// Smooths the pointer: the brush follows it through a time-based filter
/// (`level` 0 follows exactly, 100 trails far behind). With adjust by speed,
/// the filter loosens as the pen speeds up, so quick lines stay snappy while
/// slow ones lose their tremor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stabilizer {
    level: u32,
    by_speed: bool,
    at: Option<[f32; 2]>,
    raw: Option<[f32; 2]>,
    speed: f32,
}

impl Stabilizer {
    /// A stabilizer of `level` 0 to 100.
    #[must_use]
    pub fn new(level: u32, by_speed: bool) -> Self {
        Self {
            level: level.min(100),
            by_speed,
            at: None,
            raw: None,
            speed: 0.0,
        }
    }

    /// The pointer's recent speed, in screen points per second.
    #[must_use]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// The smoothed position for a raw sample (layer pixels). `scale` is
    /// screen points per layer pixel (the zoom) and `dt` the seconds since
    /// the previous sample.
    pub fn sample(&mut self, raw: [f32; 2], scale: f32, dt: f32) -> [f32; 2] {
        let dt = dt.clamp(1e-4, 0.1);
        if let Some(previous) = self.raw {
            let now = distance(previous, raw) * scale.max(1e-3) / dt;
            self.speed = self.speed * 0.6 + now * 0.4;
        }
        self.raw = Some(raw);
        let mut level = self.level as f32;
        if self.by_speed {
            let fast = ((self.speed - 150.0) / 1350.0).clamp(0.0, 1.0);
            level *= 1.0 - 0.9 * fast;
        }
        let lag = level * 0.004;
        let follow = if lag <= 0.0 {
            1.0
        } else {
            1.0 - (-dt / lag).exp()
        };
        let next = match self.at {
            Some(at) => [
                at[0] + (raw[0] - at[0]) * follow,
                at[1] + (raw[1] - at[1]) * follow,
            ],
            None => raw,
        };
        self.at = Some(next);
        next
    }

    /// Points that close the gap to `raw` when the pen lifts.
    pub fn catch_up(&mut self, raw: [f32; 2]) -> Vec<[f32; 2]> {
        let Some(at) = self.at else {
            return vec![raw];
        };
        let steps = ((distance(at, raw) / 2.0).ceil() as usize).clamp(1, 48);
        let points = (1..=steps)
            .map(|step| {
                let t = step as f32 / steps as f32;
                [at[0] + (raw[0] - at[0]) * t, at[1] + (raw[1] - at[1]) * t]
            })
            .collect();
        self.at = Some(raw);
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Arena, ComponentData, ComponentKind, Entity, PixelFormat, UndoStack};

    fn layer(width: u32, height: u32) -> RasterTile2D {
        RasterTile2D::new(width, height, PixelFormat::Rgba8).unwrap()
    }

    fn alpha(raster: &RasterTile2D, x: u32, y: u32) -> u8 {
        raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize]
    }

    fn pixel(raster: &RasterTile2D, x: u32, y: u32) -> [u8; 4] {
        let at = (y * raster.width() + x) as usize * 4;
        raster.bytes().unwrap()[at..at + 4].try_into().unwrap()
    }

    fn flat_pen(size: f32) -> Tip {
        Tip {
            size,
            size_by: Dynamics::NONE,
            ..Tip::default()
        }
    }

    /// Ink painted across row `y` from `x0` to `x1`, as total alpha per
    /// column: a line's width profile.
    fn widths(raster: &RasterTile2D, columns: std::ops::Range<u32>) -> Vec<u32> {
        columns
            .map(|x| {
                (0..raster.height())
                    .map(|y| u32::from(alpha(raster, x, y)))
                    .sum()
            })
            .collect()
    }

    #[test]
    fn a_pen_reaches_its_opacity_once_however_often_it_crosses() {
        let mut raster = layer(64, 64);
        let tip = Tip {
            opacity: 0.5,
            ..flat_pen(10.0)
        };
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0, 0.0, 0.0], false);
        for point in [[10.0, 32.0], [54.0, 32.0], [10.0, 32.0], [54.0, 32.0]] {
            stroke.to(&mut raster, point, 1.0, None);
        }
        assert_eq!(alpha(&raster, 32, 32), 128, "half opacity, not built up");
        assert_eq!(alpha(&raster, 32, 2), 0, "outside the tip");
        assert!(stroke.take_dirty().is_some());
        assert!(stroke.take_dirty().is_none());
    }

    #[test]
    fn lines_are_gapless_and_even_however_far_apart_the_samples() {
        // Two samples 80 pixels apart: a fast flick of the mouse.
        let mut raster = layer(100, 40);
        let mut stroke = Stroke::new(EntityId::new(), flat_pen(6.0), [0.0; 3], false);
        stroke.to(&mut raster, [10.0, 20.3], 1.0, None);
        stroke.to(&mut raster, [90.0, 20.3], 1.0, None);
        let profile = widths(&raster, 20..80);
        let (low, high) = (
            *profile.iter().min().unwrap(),
            *profile.iter().max().unwrap(),
        );
        assert!(
            high - low <= 2,
            "the line's weight never ripples: {low} to {high}"
        );
        assert!(low > 255 * 5, "about six pixels of ink in every column");
        // A hairline thinner than a pixel stays one unbroken light line.
        let mut thin = layer(100, 10);
        let mut hair = Stroke::new(EntityId::new(), flat_pen(0.6), [0.0; 3], false);
        hair.to(&mut thin, [5.0, 5.2], 1.0, None);
        hair.to(&mut thin, [95.0, 6.1], 1.0, None);
        assert!(widths(&thin, 10..90).iter().all(|ink| *ink > 60));
    }

    #[test]
    fn tapers_grow_from_a_point_and_thin_to_one() {
        let mut raster = layer(140, 40);
        let tip = Tip {
            taper_start: 30.0,
            taper_end: 30.0,
            ..flat_pen(10.0)
        };
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        for x in (10..=130).step_by(4) {
            stroke.to(&mut raster, [x as f32, 20.0], 1.0, None);
        }
        let live = widths(&raster, 0..140);
        assert!(live[14] < live[70] / 2, "the start grows while drawing");
        assert!(
            live[126] > live[70] * 9 / 10,
            "the end is not tapered before lifting"
        );
        stroke.end(&mut raster, Finish::default(), None);
        let done = widths(&raster, 0..140);
        assert!(done[126] < done[70] / 2, "the end thins once the pen lifts");
        assert!(done[70] >= live[70] - 2, "the middle keeps its full weight");
        assert!(done[14] < done[70] / 2);
        // A tap stays a dot instead of tapering away.
        let mut dot = layer(20, 20);
        let mut tap = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        tap.to(&mut dot, [10.0, 10.0], 1.0, None);
        tap.to(&mut dot, [10.3, 10.0], 1.0, None);
        tap.end(&mut dot, Finish::default(), None);
        assert!(alpha(&dot, 10, 10) > 0);
    }

    #[test]
    fn post_correction_smooths_tremor_and_keeps_corners() {
        // A shaky horizontal line.
        let shaky: Vec<Sample> = (0..=120)
            .map(|x| Sample {
                at: [x as f32, 50.0 + if x % 2 == 0 { 1.5 } else { -1.5 }],
                pressure: 1.0,
                speed: 0.0,
            })
            .collect();
        let calm = correct(&shaky, 5, 6.0);
        let deviation = |path: &[Sample]| {
            path[10..path.len() - 10]
                .iter()
                .map(|sample| (sample.at[1] - 50.0).abs())
                .fold(0.0, f32::max)
        };
        assert!(deviation(&calm) < 0.4, "{}", deviation(&calm));
        // An L: down, then right. The corner stays where it was.
        let mut l: Vec<Sample> = (0..=60)
            .map(|y| Sample {
                at: [20.0, y as f32],
                pressure: 1.0,
                speed: 0.0,
            })
            .collect();
        l.extend((1..=60).map(|x| Sample {
            at: [20.0 + x as f32, 60.0],
            pressure: 1.0,
            speed: 0.0,
        }));
        let kept = correct(&l, 8, 6.0);
        let nearest = kept
            .iter()
            .map(|sample| distance(sample.at, [20.0, 60.0]))
            .fold(f32::INFINITY, f32::min);
        assert!(nearest < 1.0, "the corner moved {nearest} px");
        assert_eq!(kept.first().unwrap().at, [20.0, 0.0], "ends stay put");
    }

    #[test]
    fn splines_pass_through_sparse_samples_smoothly_and_keep_corners() {
        // Four samples of a fast mouse stroke over a hill.
        let sparse: Vec<Sample> = [[0.0, 50.0], [40.0, 10.0], [80.0, 10.0], [120.0, 50.0]]
            .map(|at| Sample {
                at,
                pressure: 1.0,
                speed: 0.0,
            })
            .to_vec();
        let curve = spline(&sparse, 0.75);
        assert!(curve.len() > 150, "densely sampled: {}", curve.len());
        for sample in &sparse {
            assert!(
                curve
                    .iter()
                    .any(|point| distance(point.at, sample.at) < 1e-3)
            );
        }
        // Between the two top samples the curve bulges above the straight line.
        let top = curve
            .iter()
            .filter(|point| (point.at[0] - 60.0).abs() < 1.0)
            .map(|point| point.at[1])
            .fold(f32::INFINITY, f32::min);
        assert!(top < 10.0, "{top}");
        // A V stays a V.
        let v: Vec<Sample> = [[0.0, 0.0], [20.0, 40.0], [40.0, 0.0]]
            .map(|at| Sample {
                at,
                pressure: 1.0,
                speed: 0.0,
            })
            .to_vec();
        let sharp = spline(&v, 0.75);
        let lowest = sharp.iter().map(|point| point.at[1]).fold(0.0, f32::max);
        assert!((lowest - 40.0).abs() < 1e-3);
        assert!(
            sharp.iter().all(|point| point.at[1] <= 40.0 + 1e-3),
            "no overshoot"
        );
    }

    #[test]
    fn hooks_are_trimmed_and_flicks_end_in_a_point() {
        let mut path: Vec<Sample> = (0..=50)
            .map(|x| Sample {
                at: [x as f32, 10.0],
                pressure: 1.0,
                speed: 0.0,
            })
            .collect();
        // The pen slips back as it lifts.
        path.push(Sample {
            at: [48.0, 11.5],
            pressure: 0.4,
            speed: 0.0,
        });
        path.push(Sample {
            at: [47.0, 12.0],
            pressure: 0.2,
            speed: 0.0,
        });
        let trimmed = trim_hooks(&path, 4.0);
        assert!(
            trimmed.last().unwrap().at[0] >= 49.0,
            "{:?}",
            trimmed.last()
        );
        let mut flicked = trimmed.clone();
        flick(&mut flicked, [1200.0, 0.0], 4.0);
        let end = flicked.last().unwrap();
        assert!(end.at[0] > 60.0, "carried on to {:?}", end.at);
        // Painted, the flick thins to a point.
        let mut raster = layer(120, 30);
        let tip = Tip {
            taper_end: 10.0,
            ..flat_pen(6.0)
        };
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        for x in 10..=60 {
            stroke.to(&mut raster, [x as f32, 15.0], 1.0, None);
        }
        stroke.end(
            &mut raster,
            Finish {
                correction: 0,
                flick: Some([1500.0, 0.0]),
            },
            None,
        );
        let ink = widths(&raster, 0..120);
        let tail = ink.iter().rposition(|ink| *ink > 0).unwrap();
        assert!(tail > 75, "the flick ran on to x = {tail}");
        assert!(ink[tail - 3] < ink[40] / 3, "and thinned to a point");
        let mut slow = trimmed.clone();
        flick(&mut slow, [0.5, 0.0], 4.0);
        assert_eq!(slow.len(), trimmed.len(), "a resting pen does not flick");
    }

    #[test]
    fn painting_again_from_the_corrected_path_starts_from_the_old_pixels() {
        let mut arena = Arena::new();
        let mut base = layer(120, 60);
        base.bytes_mut().unwrap()[..4].copy_from_slice(&[9, 9, 9, 255]);
        let handle = arena.insert(Entity::new("Ink", base));
        let id = arena.get(handle).unwrap().id();
        let tip = Tip {
            taper_end: 20.0,
            ..flat_pen(6.0)
        };
        let mut stroke = Stroke::new(id, tip, [1.0, 0.0, 0.0], false);
        let edit = {
            let Some(ComponentData::RasterTile2D(raster)) = arena
                .get_mut(handle)
                .unwrap()
                .component_mut(ComponentKind::RasterTile2D)
            else {
                panic!("no raster");
            };
            for x in 10..=110 {
                let wobble = if x % 3 == 0 { 1.0 } else { 0.0 };
                stroke.to(raster, [x as f32, 30.0 + wobble], 1.0, None);
            }
            stroke.end(
                raster,
                Finish {
                    correction: 4,
                    flick: None,
                },
                None,
            );
            stroke.finish(raster, "Pen")
        };
        let raster = |arena: &Arena| {
            arena
                .get(handle)
                .unwrap()
                .component(ComponentKind::RasterTile2D)
                .unwrap()
                .as_raster()
                .unwrap()
                .clone()
        };
        assert_eq!(alpha(&raster(&arena), 60, 30), 255);
        assert_eq!(pixel(&raster(&arena), 0, 0), [9, 9, 9, 255], "untouched");
        let mut history = UndoStack::new();
        history.record(edit);
        history.undo(&mut arena).unwrap();
        let undone = raster(&arena);
        assert!((0..120).all(|x| (0..60).all(|y| alpha(&undone, x, y) == 0 || (x, y) == (0, 0))));
        history.redo(&mut arena).unwrap();
        assert_eq!(alpha(&raster(&arena), 60, 30), 255);
    }

    #[test]
    fn airbrushes_build_up_mixing_brushes_pick_up_color_and_erasers_remove() {
        let mut raster = layer(32, 32);
        let tip = Tip {
            size: 12.0,
            hardness: 0.0,
            build_up: true,
            flow: 0.2,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let mut once = Stroke::new(EntityId::new(), tip, [1.0, 0.0, 0.0], false);
        once.to(&mut raster, [16.0, 16.0], 1.0, None);
        let light = alpha(&raster, 16, 16);
        for step in 0..10 {
            once.to(&mut raster, [16.0 + (step % 2) as f32, 16.0], 1.0, None);
        }
        let heavy = alpha(&raster, 16, 16);
        assert!(light > 0 && heavy > light, "{light} then {heavy}");

        // Red dragged through blue comes out between the two.
        let mut canvas = layer(60, 20);
        for pixel in canvas.bytes_mut().unwrap().as_chunks_mut::<4>().0 {
            *pixel = [0, 0, 255, 255];
        }
        let mixing = Tip {
            size: 8.0,
            mix: 0.6,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let mut drag = Stroke::new(EntityId::new(), mixing, [1.0, 0.0, 0.0], false);
        for x in 5..55 {
            drag.to(&mut canvas, [x as f32, 10.0], 1.0, None);
        }
        let [r, _, b, _] = pixel(&canvas, 40, 10);
        assert!(r > 40 && b > 40, "a blend of red and blue: {r}, {b}");

        let eraser = Tip {
            size: 40.0,
            eraser: true,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let mut rub = Stroke::new(EntityId::new(), eraser, [0.0; 3], false);
        rub.to(&mut raster, [16.0, 16.0], 1.0, None);
        assert_eq!(alpha(&raster, 16, 16), 0);
    }

    #[test]
    fn locked_alpha_paints_only_over_pixels() {
        let mut raster = layer(16, 16);
        raster.bytes_mut().unwrap()[..4].copy_from_slice(&[0, 0, 255, 200]);
        let mut stroke = Stroke::new(EntityId::new(), flat_pen(64.0), [1.0, 1.0, 1.0], true);
        stroke.to(&mut raster, [8.0, 8.0], 1.0, None);
        assert_eq!(pixel(&raster, 0, 0), [255, 255, 255, 200]);
        assert_eq!(alpha(&raster, 5, 5), 0, "clear pixels stay clear");
    }

    #[test]
    fn dynamics_follow_their_curves_speed_and_chance() {
        // A soft pressure curve keeps light presses thin.
        let soft = Response::new(&[[0.0, 0.0], [0.8, 0.3], [1.0, 1.0]]);
        let tip = Tip {
            size: 20.0,
            size_by: Dynamics {
                pressure_curve: soft,
                ..Dynamics::pressure(0.0)
            },
            ..Tip::default()
        };
        let linear = Tip {
            size_by: Dynamics::pressure(0.0),
            ..tip
        };
        assert!(tip.dab(0.5).0 < linear.dab(0.5).0);
        assert!((tip.dab(1.0).0 - 10.0).abs() < 1e-3);
        // Velocity: a still pen keeps its minimum, a fast one the full size.
        let by_speed = Tip {
            size_by: Dynamics {
                velocity: true,
                velocity_min: 0.4,
                ..Dynamics::NONE
            },
            ..tip
        };
        assert!((by_speed.dab_at(1.0, 0.0, 1.0).0 - 4.0).abs() < 1e-3);
        assert!((by_speed.dab_at(1.0, 1.0, 1.0).0 - 10.0).abs() < 1e-3);
        // Opacity dynamics and inputs that multiply.
        let both = Tip {
            opacity_by: Dynamics::pressure(0.5),
            ..by_speed
        };
        assert!((both.dab_at(0.0, 0.0, 1.0).1 - 0.5).abs() < 1e-3);
        // Chance is steady along a stroke, so painting it again from its
        // corrected path gives the same sizes.
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        stroke.seed = 7;
        assert_eq!(stroke.chance(12.0), stroke.chance(12.4));
        let spread: Vec<f32> = (0..32)
            .map(|step| stroke.chance(step as f32 * 2.0))
            .collect();
        assert!(spread.iter().any(|value| *value < 0.3) && spread.iter().any(|value| *value > 0.7));
    }

    /// Paints a straight stroke from x 10 to 190 at y 50 and lifts.
    fn straight(tip: Tip) -> RasterTile2D {
        let mut raster = layer(200, 100);
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        for step in 0..=90 {
            stroke.to(&mut raster, [10.0 + step as f32 * 2.0, 50.0], 1.0, None);
        }
        stroke.end(&mut raster, Finish::default(), None);
        raster
    }

    /// How many pixels of column `x` the stroke covers.
    fn width_at(raster: &RasterTile2D, x: u32) -> usize {
        (0..raster.height())
            .filter(|&y| alpha(raster, x, y) > 60)
            .count()
    }

    #[test]
    fn starting_and_ending_by_percentage_fade_and_opacity() {
        let pen = Tip {
            size: 12.0,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        // By percentage: the first and last fifth of the stroke thin out.
        let percent = straight(Tip {
            taper_mode: TaperMode::Percent,
            taper_start: 20.0,
            taper_end: 20.0,
            ..pen
        });
        assert!(width_at(&percent, 16) < width_at(&percent, 100));
        assert!(width_at(&percent, 186) < width_at(&percent, 100));
        assert!(width_at(&percent, 100) >= 11);
        // Fade: gone 80 pixels in, however long the stroke.
        let fade = straight(Tip {
            taper_mode: TaperMode::Fade,
            taper_end: 80.0,
            ..pen
        });
        assert!(width_at(&fade, 20) > 0);
        assert_eq!(width_at(&fade, 120), 0, "faded out");
        // Opacity only: the line keeps its width and gets lighter.
        let light = straight(Tip {
            taper_end: 60.0,
            taper_size: false,
            taper_opacity: true,
            ..pen
        });
        let column = |x| (0..100).map(|y| alpha(&light, x, y)).max().unwrap();
        assert!(column(186) < column(100));
        assert_eq!(
            (0..100).filter(|&y| alpha(&light, 186, y) > 0).count(),
            (0..100).filter(|&y| alpha(&light, 100, y) > 0).count()
        );
    }

    #[test]
    fn color_jitter_varies_strokes_and_dabs_steadily() {
        let jitter = Jitter {
            stroke: true,
            stroke_hue: 0.5,
            stroke_value: 0.3,
            ..Jitter::default()
        };
        let tip = Tip {
            jitter,
            ..Tip::default()
        };
        let color_of = |start: f32| {
            let mut raster = layer(60, 60);
            let mut stroke = Stroke::new(EntityId::new(), tip, [0.9, 0.2, 0.2], false);
            stroke.to(&mut raster, [start, 30.0], 1.0, None);
            stroke.color
        };
        assert_ne!(color_of(10.0), [0.9, 0.2, 0.2], "the stroke wandered");
        assert_eq!(
            color_of(10.0),
            color_of(10.0),
            "the same stroke, the same color"
        );
        assert_ne!(color_of(10.0), color_of(30.0));
        // A stamping brush changes color dab by dab.
        let airbrush = Tip {
            size: 6.0,
            build_up: true,
            flow: 1.0,
            hardness: 1.0,
            size_by: Dynamics::NONE,
            jitter: Jitter {
                tip: true,
                hue: 1.0,
                ..Jitter::default()
            },
            ..Tip::default()
        };
        let mut raster = layer(200, 20);
        let mut stroke = Stroke::new(EntityId::new(), airbrush, [0.9, 0.2, 0.2], false);
        stroke.to(&mut raster, [5.0, 10.0], 1.0, None);
        stroke.to(&mut raster, [195.0, 10.0], 1.0, None);
        let bytes = raster.bytes().unwrap();
        let reds: std::collections::HashSet<u8> = (10..190)
            .map(|x| bytes[((10 * 200 + x) * 4) as usize])
            .collect();
        assert!(reds.len() > 4, "dabs in several colors");
    }

    #[test]
    fn a_line_set_to_at_least_one_pixel_keeps_full_ink() {
        let thin = Tip {
            size: 0.4,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let solid = Tip {
            at_least_one: true,
            ..thin
        };
        let ink = |tip: Tip| {
            let mut raster = layer(20, 20);
            let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
            stroke.to(&mut raster, [2.5, 10.5], 1.0, None);
            stroke.to(&mut raster, [17.5, 10.5], 1.0, None);
            alpha(&raster, 10, 10)
        };
        assert!(ink(solid) > ink(thin), "{} > {}", ink(solid), ink(thin));
        assert!(ink(solid) > 200);
    }

    #[test]
    fn pressure_scales_size_and_selections_clip() {
        let tip = Tip {
            size: 20.0,
            size_by: Dynamics::pressure(0.5),
            ..Tip::default()
        };
        assert_eq!(tip.dab(1.0).0, 10.0);
        assert_eq!(tip.dab(0.0).0, 5.0);
        let mut raster = layer(40, 40);
        let mut selection = Selection::new(40, 40);
        selection.rect([0, 0, 20, 40], crate::select::Op::Replace);
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        stroke.to(&mut raster, [20.0, 20.0], 1.0, Some(&selection));
        assert_eq!(alpha(&raster, 15, 20), 255);
        assert_eq!(alpha(&raster, 25, 20), 0, "outside the selection");
    }

    #[test]
    fn a_finished_stroke_undoes_through_its_tiles() {
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("Ink", layer(200, 100)));
        let id = arena.get(handle).unwrap().id();
        let mut stroke = Stroke::new(id, Tip::default(), [0.0; 3], false);
        let edit = {
            let Some(ComponentData::RasterTile2D(raster)) = arena
                .get_mut(handle)
                .unwrap()
                .component_mut(ComponentKind::RasterTile2D)
            else {
                panic!("no raster");
            };
            stroke.to(raster, [10.0, 10.0], 1.0, None);
            stroke.to(raster, [150.0, 20.0], 1.0, None);
            stroke.finish(raster, "Pen")
        };
        assert_eq!(edit.patches().len(), 3, "one row of three tiles");
        let ink = |arena: &Arena| {
            alpha(
                arena
                    .get(handle)
                    .unwrap()
                    .component(ComponentKind::RasterTile2D)
                    .unwrap()
                    .as_raster()
                    .unwrap(),
                80,
                15,
            )
        };
        assert_eq!(ink(&arena), 255);
        let mut history = UndoStack::new();
        history.record(edit);
        history.undo(&mut arena).unwrap();
        assert_eq!(ink(&arena), 0);
        history.redo(&mut arena).unwrap();
        assert_eq!(ink(&arena), 255);
    }

    #[test]
    fn the_stabilizer_lags_when_slow_and_keeps_up_when_fast() {
        let mut free = Stabilizer::new(0, false);
        assert_eq!(free.sample([10.0, 0.0], 1.0, 0.008), [10.0, 0.0]);
        assert_eq!(free.sample([20.0, 0.0], 1.0, 0.008), [20.0, 0.0]);
        // The same level, moving slowly and moving fast.
        let lag = |by_speed: bool, step: f32| {
            let mut steady = Stabilizer::new(30, by_speed);
            let mut at = [0.0, 0.0];
            let mut x = 0.0;
            for _ in 0..40 {
                x += step;
                at = steady.sample([x, 0.0], 1.0, 0.005);
            }
            x - at[0]
        };
        let slow = lag(true, 0.5) / 0.5;
        let fast = lag(true, 20.0) / 20.0;
        assert!(
            fast < slow / 3.0,
            "fast strokes trail fewer samples: {fast} vs {slow}"
        );
        assert!(
            lag(false, 20.0) / 20.0 > fast * 2.0,
            "without adjust by speed it lags"
        );
        let mut steady = Stabilizer::new(30, true);
        steady.sample([0.0, 0.0], 1.0, 0.005);
        steady.sample([100.0, 0.0], 1.0, 0.005);
        let rest = steady.catch_up([100.0, 0.0]);
        assert_eq!(rest.last(), Some(&[100.0, 0.0]));
    }
}

#[cfg(test)]
mod sheet {
    use super::*;
    use sue_core::PixelFormat;

    /// Simulated pen input: points along `curve(t)` with tremor, `count`
    /// samples, pressure rising then falling.
    fn draw(
        raster: &mut RasterTile2D,
        tip: Tip,
        level: u32,
        finish: Finish,
        tremor: f32,
        count: usize,
        curve: impl Fn(f32) -> [f32; 2],
    ) {
        let mut stroke = Stroke::new(EntityId::NIL, tip, [0.05, 0.05, 0.08], false);
        let mut stabilizer = Stabilizer::new(level, true);
        let mut seed = 7_u32;
        let mut raw = [0.0, 0.0];
        for i in 0..=count {
            let t = i as f32 / count as f32;
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let jx = ((seed >> 8) % 1000) as f32 / 1000.0 - 0.5;
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let jy = ((seed >> 8) % 1000) as f32 / 1000.0 - 0.5;
            let p = curve(t);
            raw = [p[0] + jx * tremor, p[1] + jy * tremor];
            let pressure = (t * std::f32::consts::PI).sin().powf(0.6).max(0.05);
            let at = stabilizer.sample(raw, 1.0, 0.006);
            stroke.to(raster, at, pressure, None);
        }
        for at in stabilizer.catch_up(raw) {
            stroke.to(raster, at, 0.1, None);
        }
        stroke.end(raster, finish, None);
    }

    #[test]
    #[ignore = "writes a picture of sample strokes for a person to look at"]
    fn stroke_sheet() {
        let (width, height) = (900_u32, 620_u32);
        let mut raster = RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            [255, 255, 255, 255].repeat((width * height) as usize),
        )
        .unwrap();
        let g_pen = Tip {
            size: 7.0,
            taper_start: 14.0,
            taper_end: 26.0,
            ..Tip::default()
        };
        let raw_pen = Tip {
            size: 7.0,
            ..Tip::default()
        };
        let wave = |y: f32| {
            move |t: f32| {
                [
                    40.0 + t * 380.0,
                    y + 30.0 * (t * std::f32::consts::TAU * 1.5).sin(),
                ]
            }
        };
        // Left column: no help; right column: stabilizer, tapers, correction.
        draw(
            &mut raster,
            raw_pen,
            0,
            Finish::default(),
            2.5,
            60,
            wave(60.0),
        );
        draw(
            &mut raster,
            g_pen,
            6,
            Finish {
                correction: 3,
                flick: None,
            },
            2.5,
            60,
            wave(140.0),
        );
        // Fast mouse: few samples far apart.
        draw(
            &mut raster,
            raw_pen,
            0,
            Finish::default(),
            0.0,
            8,
            wave(220.0),
        );
        draw(
            &mut raster,
            g_pen,
            6,
            Finish {
                correction: 2,
                flick: Some([2500.0, 0.0]),
            },
            0.0,
            8,
            wave(300.0),
        );
        // Hatching flicks.
        for k in 0..12 {
            let x = 470.0 + k as f32 * 34.0;
            draw(
                &mut raster,
                Tip {
                    size: 5.0,
                    taper_start: 10.0,
                    taper_end: 30.0,
                    ..Tip::default()
                },
                4,
                Finish {
                    correction: 2,
                    flick: Some([600.0, 1800.0]),
                },
                1.0,
                12,
                move |t| [x + t * 12.0, 30.0 + t * 90.0],
            );
        }
        // A corner kept sharp by post correction.
        draw(
            &mut raster,
            g_pen,
            6,
            Finish {
                correction: 8,
                flick: None,
            },
            2.0,
            80,
            |t| {
                if t < 0.5 {
                    [480.0, 160.0 + t * 2.0 * 140.0]
                } else {
                    [480.0 + (t - 0.5) * 2.0 * 160.0, 300.0]
                }
            },
        );
        // Ink brush, hairline, mixing watercolor over a color.
        draw(
            &mut raster,
            Tip {
                size: 22.0,
                size_by: Dynamics::pressure(0.02),
                taper_start: 20.0,
                taper_end: 40.0,
                aa: AntiAlias::Medium,
                ..Tip::default()
            },
            6,
            Finish {
                correction: 2,
                flick: None,
            },
            1.5,
            80,
            |t| [680.0 + 160.0 * t, 170.0 + 120.0 * (t * 3.1).sin()],
        );
        draw(
            &mut raster,
            Tip {
                size: 1.2,
                ..Tip::default()
            },
            4,
            Finish {
                correction: 2,
                flick: None,
            },
            0.6,
            120,
            |t| [40.0 + t * 380.0, 400.0 + 40.0 * (t * 9.0).sin()],
        );
        let mut red = Stroke::new(
            EntityId::NIL,
            Tip {
                size: 40.0,
                size_by: Dynamics::NONE,
                ..Tip::default()
            },
            [0.9, 0.2, 0.2],
            false,
        );
        red.to(&mut raster, [480.0, 420.0], 1.0, None);
        red.to(&mut raster, [860.0, 420.0], 1.0, None);
        let mut blue = Stroke::new(
            EntityId::NIL,
            Tip {
                size: 40.0,
                size_by: Dynamics::NONE,
                ..Tip::default()
            },
            [0.2, 0.3, 0.9],
            false,
        );
        blue.to(&mut raster, [480.0, 470.0], 1.0, None);
        blue.to(&mut raster, [860.0, 470.0], 1.0, None);
        let water = Tip {
            size: 36.0,
            hardness: 0.3,
            build_up: true,
            flow: 0.4,
            mix: 0.5,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let mut drag = Stroke::new(EntityId::NIL, water, [1.0, 0.95, 0.3], false);
        for i in 0..=200 {
            let t = i as f32 / 200.0;
            drag.to(
                &mut raster,
                [500.0 + t * 340.0, 445.0 + 60.0 * (t * 6.0).sin()],
                1.0,
                None,
            );
        }
        let png = sue_io::encode_png(&raster).unwrap();
        let path = std::env::var("SUE_SHEET").map_or_else(
            |_| std::env::temp_dir().join("sue-stroke-sheet.png"),
            std::path::PathBuf::from,
        );
        std::fs::write(path, png).unwrap();
    }
}
