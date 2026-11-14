//! The dab plan: stroke points in, the dabs a stroke stamps out.
//!
//! A [`Planner`] walks the stroke's points by arc length and places a dab
//! every gap, read from the size where the last dab fell
//! ([`DabWalker`]). Each dab then gets what the behaviours give it: a
//! scatter off the line, a spray of particles in its place, which of the
//! tip's pictures it stamps and whether flipped, its turn, its strength
//! (with the density kept however close dabs fall), and its color after
//! the color jitter. Everything that is chance is a counter hash of the
//! stroke's seed and the dab's number, so a stroke replays the same
//! however its points are split into calls.

use sue_model::brush::{AntiAlias, Spacing};

use super::color::Jitter;
use super::math::{chance, cos_sin, direction, in_disc, pow_unit, unit};
use super::settings::{Flip, ParticleDirection, Spray, TipOrder, key};
use crate::input::StrokePoint;
use crate::raster::{DabWalker, Disc, Turn};

/// The chance streams of the dab plan.
const SCATTER: u64 = 0x20;
const SPRAY_PLACE: u64 = 0x21;
const SPRAY_TURN: u64 = 0x22;
const PICK: u64 = 0x23;
const FLIP_X: u64 = 0x24;
const FLIP_Y: u64 = 0x25;

/// The automatic gap, as a share of the diameter: close enough that no
/// dab shows on its own.
pub const AUTO_GAP: f32 = 0.05;

/// The automatic gap of a spraying brush, as a share of the diameter:
/// the particles, not the dabs, fill the stroke.
pub const AUTO_SPRAY_GAP: f32 = 0.25;

/// The gap at which a dab's density is the brush's own when the density
/// is kept by gap, as a share of the diameter.
pub const DENSITY_GAP: f32 = 0.1;

/// Clip Studio's four gap buttons, as shares of the diameter, by their
/// stored names. A name is never taken back.
pub const GAP_PRESETS: &[(&str, f32)] = &[
    ("narrow", 0.05),
    ("normal", 0.1),
    ("wide", 0.3),
    ("very-wide", 0.8),
];

/// The gap of the preset stored as `name`, as a share of the diameter.
#[must_use]
pub fn gap_preset(name: &str) -> Option<f32> {
    GAP_PRESETS.iter().find(|(n, _)| *n == name).map(|p| p.1)
}

/// The antialiasing width of a hard rim, in pixels.
#[must_use]
pub const fn edge_of(antialias: AntiAlias) -> f32 {
    match antialias {
        AntiAlias::None => 0.0,
        AntiAlias::Weak => 0.9,
        AntiAlias::Medium => 1.5,
        AntiAlias::Strong => 2.5,
    }
}

/// What the plan needs to know of the brush.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanSettings {
    /// What the spacing number means.
    pub spacing: Spacing,
    /// Dabs add up (airbrush); their strength is the flow.
    pub build_up: bool,
    /// How many pictures the tip has; 0 for a round tip.
    pub tips: usize,
    /// How the pictures are taken.
    pub order: TipOrder,
    /// Flip across, and up and down.
    pub flip: [Flip; 2],
    /// Keep the density however close dabs fall (build-up brushes).
    pub gap_density: bool,
    /// The spraying effect.
    pub spray: Option<Spray>,
    /// Continuous spraying: dabs a second while the pen holds still.
    pub continuous: Option<f32>,
    /// The density of paint, where no point carries its own.
    pub paint_density: f32,
    /// The stroke's own opacity, laid once on the whole stroke when it
    /// meets the layer. A point's opacity is a share of it, so dabs that
    /// add up (build-up) never pass it: Clip Studio's brush density, a
    /// dab's own strength, is apart from the opacity.
    pub opacity: f32,
}

impl Default for PlanSettings {
    fn default() -> Self {
        Self {
            spacing: Spacing::Auto,
            build_up: false,
            tips: 0,
            order: TipOrder::Repeat,
            flip: [Flip::Off; 2],
            gap_density: false,
            spray: None,
            continuous: None,
            paint_density: 1.0,
            opacity: 1.0,
        }
    }
}

/// One dab of the plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planned {
    /// Where, how large and how strong (0 to 1, the most coverage it
    /// lays: the point's opacity and density, and the flow when dabs add
    /// up).
    pub disc: Disc,
    /// The tip's height over its width.
    pub aspect: f32,
    /// The tip's turn.
    pub turn: Turn,
    /// Which of the tip's pictures, or `None` for the round tip.
    pub tip: Option<usize>,
    /// Flipped across, and up and down.
    pub flip: [bool; 2],
    /// The round tip's hardness.
    pub hardness: f32,
    /// The paint's color here, after the color jitter.
    pub color: [f32; 3],
    /// How strongly the texture breaks this dab up.
    pub texture_strength: f32,
    /// What a wet brush mixes with here: the amount of paint and the
    /// color stretch as multipliers of the brush's own, and the density
    /// of paint.
    pub wet: [f32; 3],
    /// The dab's number in the stroke.
    pub index: u64,
}

/// What a point brings to the plan.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    pos: [f32; 2],
    time: f64,
    direction: f32,
    size: f32,
    strength: f32,
    hardness: f32,
    aspect: f32,
    angle: f32,
    spacing: f32,
    scatter: f32,
    texture_strength: f32,
    hue: f32,
    saturation: f32,
    luminosity: f32,
    particle_size: f32,
    particle_density: f32,
    deviation: f32,
    wet: [f32; 3],
}

impl Point {
    /// `self` and `b` mixed: `t` of the way to `b`.
    fn lerp(&self, b: &Self, t: f32) -> Self {
        let l = |x: f32, y: f32| x + (y - x) * t;
        // Angles the short way round.
        let turn = |x: f32, y: f32| {
            let d = (y - x + 540.0).rem_euclid(360.0) - 180.0;
            x + d * t
        };
        Self {
            pos: [l(self.pos[0], b.pos[0]), l(self.pos[1], b.pos[1])],
            time: self.time + (b.time - self.time) * f64::from(t),
            direction: turn(self.direction, b.direction),
            size: l(self.size, b.size),
            strength: l(self.strength, b.strength),
            hardness: l(self.hardness, b.hardness),
            aspect: l(self.aspect, b.aspect),
            angle: turn(self.angle, b.angle),
            spacing: l(self.spacing, b.spacing),
            scatter: l(self.scatter, b.scatter),
            texture_strength: l(self.texture_strength, b.texture_strength),
            hue: l(self.hue, b.hue),
            saturation: l(self.saturation, b.saturation),
            luminosity: l(self.luminosity, b.luminosity),
            particle_size: l(self.particle_size, b.particle_size),
            particle_density: l(self.particle_density, b.particle_density),
            deviation: l(self.deviation, b.deviation),
            wet: [
                l(self.wet[0], b.wet[0]),
                l(self.wet[1], b.wet[1]),
                l(self.wet[2], b.wet[2]),
            ],
        }
    }
}

/// Where in a point's `extra` each of the plan's own parameters is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Slots {
    particle_size: Option<usize>,
    particle_density: Option<usize>,
    deviation: Option<usize>,
    amount: Option<usize>,
    stretch: Option<usize>,
    density: Option<usize>,
}

/// Turns stroke points into dabs, segment by segment. Cloning a planner
/// keeps its place, so the provisional tail can be planned from a clone
/// and thrown away at the next update.
#[derive(Clone, Debug)]
pub struct Planner {
    settings: PlanSettings,
    seed: u64,
    jitter: Jitter,
    slots: Slots,
    walker: DabWalker,
    last: Option<Point>,
    /// Dabs placed so far (particles count one each).
    index: u64,
    /// Tip pictures taken so far.
    picks: u64,
    /// Device time continuous spraying counts from (the last point's),
    /// and the dabs it has sprayed since.
    held_from: f64,
    held: u64,
    /// Where the last dab was laid and its size: a pen pressing harder
    /// where it (all but) stands lays a larger one there.
    last_dab: Option<([f32; 2], f32)>,
}

impl Planner {
    /// A plan for a stroke with `seed`, its color wandering by `jitter`,
    /// whose points carry their extra parameters in the order of
    /// `extra_keys` ([`crate::input::Pipeline::extra_keys`]).
    #[must_use]
    pub fn new(settings: PlanSettings, seed: u64, jitter: Jitter, extra_keys: &[String]) -> Self {
        let slot = |key: &str| extra_keys.iter().position(|k| k == key);
        Self {
            settings,
            seed,
            jitter,
            slots: Slots {
                particle_size: slot(key::PARTICLE_SIZE),
                particle_density: slot(key::PARTICLE_DENSITY),
                deviation: slot(key::SPRAY_DEVIATION),
                amount: slot(key::AMOUNT_OF_PAINT),
                stretch: slot(key::COLOR_STRETCH),
                density: slot(key::PAINT_DENSITY),
            },
            walker: DabWalker::new(),
            last: None,
            index: 0,
            picks: 0,
            held_from: f64::NEG_INFINITY,
            held: 0,
            last_dab: None,
        }
    }

    /// The settings.
    #[must_use]
    pub const fn settings(&self) -> &PlanSettings {
        &self.settings
    }

    /// Dabs placed so far.
    #[must_use]
    pub const fn dabs(&self) -> u64 {
        self.index
    }

    fn point(&self, p: &StrokePoint) -> Point {
        let extra = |slot: Option<usize>, default: f32| {
            slot.and_then(|i| p.extra.get(i).copied())
                .unwrap_or(default)
        };
        let spray = self.settings.spray.unwrap_or_default();
        let base = self.settings.opacity;
        let share = if base > 0.0 {
            unit(p.opacity / base)
        } else {
            0.0
        };
        let mut strength = share * unit(p.density);
        if self.settings.build_up {
            strength *= unit(p.flow);
        }
        Point {
            pos: p.pos,
            time: p.time,
            direction: p.direction,
            size: p.size.max(0.0),
            strength,
            hardness: p.hardness,
            aspect: p.aspect,
            angle: p.angle,
            spacing: p.spacing,
            scatter: p.scatter,
            texture_strength: p.texture_strength,
            hue: p.hue,
            saturation: p.saturation,
            luminosity: p.luminosity,
            particle_size: extra(self.slots.particle_size, spray.size),
            particle_density: extra(self.slots.particle_density, spray.density),
            deviation: extra(self.slots.deviation, spray.deviation),
            wet: [
                extra(self.slots.amount, 1.0),
                extra(self.slots.stretch, 1.0),
                extra(self.slots.density, self.settings.paint_density),
            ],
        }
    }

    /// The gap after a dab of `diameter` at a point whose spacing is
    /// `spacing`.
    fn gap(&self, diameter: f32, spacing: f32) -> f32 {
        let share = |s: f32| (s * diameter).max(DabWalker::SMALLEST_GAP);
        match self.settings.spacing {
            Spacing::Auto if self.settings.spray.is_some() => share(AUTO_SPRAY_GAP),
            Spacing::Auto => share(AUTO_GAP),
            Spacing::Ratio => share(spacing.max(0.0)),
            Spacing::Fixed => spacing.max(DabWalker::SMALLEST_GAP),
        }
    }

    /// Plans the dabs from the last point to `point`, into `out`.
    pub fn push(&mut self, point: &StrokePoint, out: &mut Vec<Planned>) {
        let b = self.point(point);
        let a = self.last.unwrap_or(b);
        let (ax, ay) = (a.pos[0], a.pos[1]);
        let hypot = |dx: f32, dy: f32| (dx * dx + dy * dy).sqrt();
        let length = hypot(b.pos[0] - ax, b.pos[1] - ay);
        let disc = |p: &Point| Disc::new(p.pos, p.size * 0.5, 1.0);
        let spacing = a.spacing;
        let mut stamped = Vec::new();
        let mut walker = self.walker;
        walker.walk(
            disc(&a),
            disc(&b),
            |radius| self.gap(radius * 2.0, spacing),
            |d| {
                let t = if length > 0.0 {
                    (hypot(d.at[0] - ax, d.at[1] - ay) / length).min(1.0)
                } else {
                    0.0
                };
                stamped.push(a.lerp(&b, t));
            },
        );
        self.walker = walker;
        // Dabs follow the distance the pen goes, so a pen that presses
        // harder where it stands (a tap that lands lightly) would keep
        // the first, smaller dab. A pen held still never holds exactly
        // still, so within a small reach of the last dab (the automatic
        // gap, as for the roundest brush) one grown by as much is laid
        // again there. A pen on its way lays by distance alone, so a
        // brush spaced wide gains no dabs when pressure rises.
        if stamped.is_empty()
            && let Some((at, size)) = self.last_dab
        {
            let reach = (AUTO_GAP * b.size).max(DabWalker::SMALLEST_GAP);
            let off = hypot(b.pos[0] - at[0], b.pos[1] - at[1]);
            if off <= reach && b.size - size >= reach {
                stamped.push(b);
            }
        }
        for p in stamped {
            self.last_dab = Some((p.pos, p.size));
            self.place(&p, out);
        }
        self.last = Some(b);
        if self.held_from < b.time {
            self.held_from = b.time;
            self.held = 0;
        }
    }

    /// Plans every point of `points`, in order.
    pub fn push_all(&mut self, points: &[StrokePoint], out: &mut Vec<Planned>) {
        for p in points {
            self.push(p, out);
        }
    }

    /// Continuous spraying: the pen has held still at the last point up
    /// to device time `now`, so dabs keep coming there, as many as the
    /// rate says for the time held. Nothing without continuous spraying
    /// or before the first point.
    pub fn hold(&mut self, now: f64, out: &mut Vec<Planned>) {
        let (Some(rate), Some(last)) = (self.settings.continuous, self.last) else {
            return;
        };
        if rate.is_nan() || rate <= 0.0 || !now.is_finite() {
            return;
        }
        // Counted from the last point, not stepped, so no rounding adds
        // up over a long hold.
        let due = ((now - self.held_from) * f64::from(rate) + 1.0e-9).floor();
        let due = if due > 0.0 { due.min(1.0e6) as u64 } else { 0 };
        while self.held < due {
            self.held += 1;
            self.place(&last, out);
        }
    }

    /// The gap-kept strength of a dab of `strength` at `gap` pixels from
    /// the next, for a dab of `diameter`.
    fn kept(&self, strength: f32, gap: f32, diameter: f32) -> f32 {
        if !self.settings.gap_density || !self.settings.build_up || diameter <= 0.0 {
            return strength;
        }
        // n dabs at the reference gap lay 1 - (1 - s)^n; the same reach
        // at this gap holds n * reference / gap dabs.
        let reference = (DENSITY_GAP * diameter).max(DabWalker::SMALLEST_GAP);
        1.0 - pow_unit(1.0 - unit(strength), gap / reference)
    }

    /// The tip picture dab number `pick` takes.
    fn pick(&self, pick: u64) -> Option<usize> {
        let n = self.settings.tips as u64;
        if n == 0 {
            return None;
        }
        Some(match self.settings.order {
            TipOrder::Repeat => pick % n,
            TipOrder::Clamp => pick.min(n - 1),
            TipOrder::BackAndForth => {
                if n == 1 {
                    0
                } else {
                    let k = pick % (2 * n - 2);
                    if k < n { k } else { 2 * n - 2 - k }
                }
            }
            TipOrder::Random => ((chance(self.seed, pick, PICK) * n as f32) as u64).min(n - 1),
        } as usize)
    }

    /// Whether dab `index` is flipped along `axis`.
    fn flipped(&self, axis: usize, index: u64) -> bool {
        match self.settings.flip[axis] {
            Flip::Off => false,
            Flip::On => true,
            Flip::Random => chance(self.seed, index, if axis == 0 { FLIP_X } else { FLIP_Y }) < 0.5,
        }
    }

    /// One dab of the stroke at `p`: scattered, sprayed, picked.
    fn place(&mut self, p: &Point, out: &mut Vec<Planned>) {
        let diameter = p.size;
        let gap = self.gap(diameter, p.spacing);
        let strength = self.kept(p.strength, gap, diameter);
        let (c, s) = cos_sin(p.angle);
        let brush_turn = Turn::new(c, s);
        let mut at = p.pos;
        if p.scatter > 0.0 {
            let [dx, dy] = in_disc(self.seed, self.index, SCATTER);
            at = [
                at[0] + dx * p.scatter * diameter,
                at[1] + dy * p.scatter * diameter,
            ];
        }
        let color = self
            .jitter
            .dab(self.index, p.hue, p.saturation, p.luminosity);
        let Some(spray) = self.settings.spray else {
            let index = self.index;
            out.push(Planned {
                disc: Disc::new(at, diameter * 0.5, strength),
                aspect: p.aspect,
                turn: brush_turn,
                tip: self.pick(self.picks),
                flip: [self.flipped(0, index), self.flipped(1, index)],
                hardness: p.hardness,
                color,
                texture_strength: p.texture_strength,
                wet: p.wet,
                index,
            });
            self.index += 1;
            self.picks += 1;
            return;
        };
        let spot = self.index;
        let count = p.particle_density.round().clamp(1.0, 256.0) as u64;
        let particle = if spray.linked {
            p.particle_size.max(0.0) * diameter
        } else {
            p.particle_size.max(0.0)
        };
        let reach = (diameter - particle).max(0.0) * 0.5;
        // Gathering pulls a particle's distance from the middle down by
        // a power of itself, 1 (even) to 4 (close in).
        let gather = 1.0 + 3.0 * unit(p.deviation);
        let (lc, ls) = cos_sin(p.direction);
        for k in 0..count {
            let index = self.index;
            let [dx, dy] = in_disc(self.seed, spot * 257 + k, SPRAY_PLACE);
            let r2 = dx * dx + dy * dy;
            let pull = if r2 > 0.0 {
                let r = r2.sqrt();
                pow_unit(r, gather) / r
            } else {
                0.0
            };
            let turn = match spray.direction {
                ParticleDirection::Brush => brush_turn,
                ParticleDirection::Line => Turn::new(lc, ls),
                ParticleDirection::Random => {
                    let [x, y] = direction(self.seed, index, SPRAY_TURN);
                    Turn::new(x, y)
                }
            };
            out.push(Planned {
                disc: Disc::new(
                    [at[0] + dx * pull * reach, at[1] + dy * pull * reach],
                    particle * 0.5,
                    strength,
                ),
                aspect: p.aspect,
                turn,
                tip: self.pick(self.picks),
                flip: [self.flipped(0, index), self.flipped(1, index)],
                hardness: p.hardness,
                color,
                texture_strength: p.texture_strength,
                wet: p.wet,
                index,
            });
            self.index += 1;
            self.picks += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_model::brush::ColorJitter;

    fn point(x: f32, size: f32) -> StrokePoint {
        StrokePoint {
            pos: [x, 10.0],
            arc: x,
            time: f64::from(x) / 100.0,
            pressure: 1.0,
            speed: 0.0,
            tilt: [0.0; 2],
            twist: 0.0,
            direction: 0.0,
            taper: 1.0,
            size,
            opacity: 1.0,
            flow: 1.0,
            density: 1.0,
            hardness: 1.0,
            aspect: 1.0,
            angle: 0.0,
            spacing: 0.5,
            scatter: 0.0,
            texture_strength: 1.0,
            hue: 0.0,
            saturation: 0.0,
            luminosity: 0.0,
            extra: Vec::new(),
        }
    }

    fn planner(settings: PlanSettings) -> Planner {
        Planner::new(
            settings,
            9,
            Jitter::new(ColorJitter::default(), 9, [0.0; 3], [1.0; 3]),
            &[],
        )
    }

    fn plan(settings: PlanSettings, xs: &[f32]) -> Vec<Planned> {
        let mut p = planner(settings);
        let mut out = Vec::new();
        for &x in xs {
            p.push(&point(x, 10.0), &mut out);
        }
        out
    }

    #[test]
    fn a_tap_that_presses_harder_where_it_stands_grows() {
        // A pen lands lightly and presses on without moving: the dot is
        // as large as the hardest press, not the first.
        let mut p = planner(PlanSettings::default());
        let mut out = Vec::new();
        for size in [4.0, 7.0, 6.0, 9.0, 9.0] {
            p.push(&point(0.0, size), &mut out);
        }
        let sizes: Vec<f32> = out.iter().map(|d| d.disc.radius * 2.0).collect();
        assert_eq!(sizes, [4.0, 7.0, 9.0], "a dab each time it grew");
        // A pen never stands exactly still: a hand's tremble of a tenth
        // of a pixel grows the dot all the same.
        let mut p = planner(PlanSettings::default());
        let mut out = Vec::new();
        for (x, size) in [(0.0, 4.0), (0.1, 7.0), (0.0, 6.0), (-0.1, 9.0), (0.05, 9.0)] {
            p.push(&point(x, size), &mut out);
        }
        // (The tremble adds up to a gap once, so one dab falls by
        // distance on the way.)
        let sizes: Vec<f32> = out.iter().map(|d| d.disc.radius * 2.0).collect();
        assert_eq!(sizes.first(), Some(&4.0));
        assert_eq!(
            sizes.last(),
            Some(&9.0),
            "a trembling tap grows too: {sizes:?}"
        );
        assert!(sizes.len() <= 4, "{sizes:?}");
        // A brush spaced wide (stars a size and a half apart) gains no
        // dab when pressure rises on the way: dabs follow the distance.
        let wide = PlanSettings {
            spacing: Spacing::Ratio,
            ..PlanSettings::default()
        };
        let mut p = planner(wide);
        let mut out = Vec::new();
        for k in 0..=20 {
            let x = k as f32;
            let mut q = point(x, 10.0 + x);
            q.spacing = 1.5;
            p.push(&q, &mut out);
        }
        let xs: Vec<f32> = out.iter().map(|d| d.disc.at[0]).collect();
        assert_eq!(xs.len(), 2, "one at the landing, one a gap on: {xs:?}");
    }

    #[test]
    fn ratio_spacing_places_dabs_by_size_however_points_are_split() {
        let settings = PlanSettings {
            spacing: Spacing::Ratio,
            ..PlanSettings::default()
        };
        let whole = plan(settings, &[0.0, 100.0]);
        let cut = plan(settings, &[0.0, 13.0, 50.5, 77.0, 100.0]);
        assert_eq!(whole.len(), 21);
        assert_eq!(whole.len(), cut.len());
        for (w, c) in whole.iter().zip(&cut) {
            assert!((w.disc.at[0] - c.disc.at[0]).abs() < 1.0e-3);
        }
    }

    #[test]
    fn the_repeat_methods_take_the_pictures_in_their_order() {
        let order = |order: TipOrder| -> Vec<usize> {
            let settings = PlanSettings {
                spacing: Spacing::Ratio,
                tips: 3,
                order,
                ..PlanSettings::default()
            };
            plan(settings, &[0.0, 45.0])
                .iter()
                .map(|d| d.tip.expect("a picture"))
                .collect()
        };
        assert_eq!(order(TipOrder::Repeat), [0, 1, 2, 0, 1, 2, 0, 1, 2, 0]);
        assert_eq!(
            order(TipOrder::BackAndForth),
            [0, 1, 2, 1, 0, 1, 2, 1, 0, 1]
        );
        assert_eq!(order(TipOrder::Clamp), [0, 1, 2, 2, 2, 2, 2, 2, 2, 2]);
        let random = order(TipOrder::Random);
        assert!(random.contains(&0) && random.contains(&1) && random.contains(&2));
        assert_eq!(
            random,
            order(TipOrder::Random),
            "the same seed, the same picks"
        );
    }

    #[test]
    fn flips_are_off_on_or_by_chance() {
        let flips = |flip: [Flip; 2]| -> Vec<[bool; 2]> {
            let settings = PlanSettings {
                spacing: Spacing::Ratio,
                flip,
                ..PlanSettings::default()
            };
            plan(settings, &[0.0, 200.0])
                .iter()
                .map(|d| d.flip)
                .collect()
        };
        assert!(
            flips([Flip::Off, Flip::On])
                .iter()
                .all(|f| *f == [false, true])
        );
        let random = flips([Flip::Random, Flip::Off]);
        let across = random.iter().filter(|f| f[0]).count();
        assert!(across > 10 && across < 31, "{across} of {}", random.len());
    }

    #[test]
    fn a_spray_fills_the_brush_with_particles_gathered_by_its_deviation() {
        let spread = |deviation: f32| -> (usize, f32) {
            let settings = PlanSettings {
                spacing: Spacing::Ratio,
                spray: Some(Spray {
                    size: 0.1,
                    linked: true,
                    density: 40.0,
                    deviation,
                    direction: ParticleDirection::Random,
                }),
                ..PlanSettings::default()
            };
            let mut p = planner(settings);
            let mut out = Vec::new();
            p.push(&point(0.0, 40.0), &mut out);
            let mean = out
                .iter()
                .map(|d| ((d.disc.at[0]).powi(2) + (d.disc.at[1] - 10.0).powi(2)).sqrt())
                .sum::<f32>()
                / out.len() as f32;
            assert!(out.iter().all(|d| (d.disc.radius - 2.0).abs() < 1.0e-4));
            assert!(out.iter().all(|d| d.disc.at[0].abs() <= 18.0 + 1.0e-3));
            (out.len(), mean)
        };
        let (count, even) = spread(0.0);
        let (_, gathered) = spread(1.0);
        assert_eq!(count, 40);
        // Even over a disc of 18: the mean distance is two thirds of it.
        assert!((even - 12.0).abs() < 2.0, "{even}");
        assert!(gathered < even * 0.6, "{gathered} against {even}");
    }

    #[test]
    fn particle_size_in_pixels_does_not_follow_the_brush() {
        let settings = PlanSettings {
            spray: Some(Spray {
                size: 3.0,
                linked: false,
                density: 5.0,
                ..Spray::default()
            }),
            ..PlanSettings::default()
        };
        let mut p = planner(settings);
        let mut out = Vec::new();
        p.push(&point(0.0, 80.0), &mut out);
        assert!(out.iter().all(|d| (d.disc.radius - 1.5).abs() < 1.0e-5));
    }

    #[test]
    fn density_kept_by_gap_lays_the_same_whatever_the_gap() {
        // Build-up dabs at two gaps: what piles up over one diameter is
        // the same with the density kept, and not without.
        let piled = |gap_density: bool, spacing: f32| -> f32 {
            let settings = PlanSettings {
                spacing: Spacing::Ratio,
                build_up: true,
                gap_density,
                ..PlanSettings::default()
            };
            let mut p = planner(settings);
            let mut out = Vec::new();
            let mut a = point(0.0, 10.0);
            let mut b = point(10.0, 10.0);
            a.spacing = spacing;
            b.spacing = spacing;
            a.flow = 0.2;
            b.flow = 0.2;
            p.push(&a, &mut out);
            p.push(&b, &mut out);
            // Dabs over the first diameter, piled up as build-up does.
            out.iter()
                .filter(|d| d.disc.at[0] < 10.0 - 1.0e-3)
                .fold(0.0, |acc, d| acc + d.disc.strength * (1.0 - acc))
        };
        let (close, far) = (piled(true, 0.05), piled(true, 0.2));
        assert!((close - far).abs() < 0.03, "{close} against {far}");
        let (close, far) = (piled(false, 0.05), piled(false, 0.2));
        assert!(close - far > 0.2, "{close} against {far}");
    }

    #[test]
    fn continuous_spraying_keeps_dabbing_while_the_pen_holds_still() {
        let settings = PlanSettings {
            continuous: Some(50.0),
            ..PlanSettings::default()
        };
        let mut p = planner(settings);
        let mut out = Vec::new();
        p.push(&point(0.0, 10.0), &mut out);
        let before = out.len();
        p.hold(0.5, &mut out);
        assert_eq!(out.len() - before, 25);
        // Holding again to the same time adds nothing.
        p.hold(0.5, &mut out);
        assert_eq!(out.len() - before, 25);
        let mut still = planner(PlanSettings::default());
        let mut none = Vec::new();
        still.push(&point(0.0, 10.0), &mut none);
        still.hold(5.0, &mut none);
        assert_eq!(none.len(), 1);
    }

    #[test]
    fn scatter_moves_dabs_off_the_line_within_its_reach() {
        let mut p = planner(PlanSettings {
            spacing: Spacing::Ratio,
            ..PlanSettings::default()
        });
        let mut out = Vec::new();
        let mut a = point(0.0, 10.0);
        let mut b = point(100.0, 10.0);
        a.scatter = 1.0;
        b.scatter = 1.0;
        p.push(&a, &mut out);
        p.push(&b, &mut out);
        let off: Vec<f32> = out.iter().map(|d| (d.disc.at[1] - 10.0).abs()).collect();
        assert!(off.iter().all(|&o| o <= 10.0 + 1.0e-4));
        assert!(off.iter().any(|&o| o > 4.0));
        assert_eq!(gap_preset("wide"), Some(0.3));
        assert_eq!(gap_preset("nope"), None);
    }
}
