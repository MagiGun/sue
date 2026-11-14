//! Dynamics: every parameter of the brush at one point of the curve,
//! after every binding that drives it.
//!
//! The inputs a binding reads, each as a share from 0 to 1:
//!
//! | Input | Share |
//! | --- | --- |
//! | `pressure` | the conditioned pressure |
//! | `speed` | layer pixels a second over 2,000 |
//! | `tilt` | how far the pen leans, upright to flat |
//! | `tilt_direction` | the way it leans, over a full turn |
//! | `twist` | its turn, over a full turn |
//! | `direction` | the way the line runs, over a full turn |
//! | `distance` | arc length over 1,000 layer pixels |
//! | `time` | seconds since the stroke began, over 2 |
//! | `random` | new for every point, from the stroke's seed |
//! | `stroke_random` | one for the whole stroke |
//!
//! An input this build does not know reads 0. Speed and distance are in
//! layer pixels, so the same samples give the same stroke at any zoom.

use sue_model::BrushPreset;
use sue_model::brush::BrushRegistry;
use sue_model::brush::{Binding, input, param};

use super::StrokePoint;
use super::curve::Base;
use super::math;

/// Layer pixels a second that read as full speed.
const FULL_SPEED: f32 = 2000.0;

/// Layer pixels that read as the full distance.
const FULL_DISTANCE: f32 = 1000.0;

/// Seconds that read as the full time.
const FULL_TIME: f32 = 2.0;

/// The standard parameters, in the order of [`Values`].
const STANDARD: [&str; 13] = [
    param::SIZE,
    param::OPACITY,
    param::FLOW,
    param::DENSITY,
    param::HARDNESS,
    param::ASPECT,
    param::ANGLE,
    param::SPACING,
    param::SCATTER,
    param::TEXTURE_STRENGTH,
    param::HUE,
    param::SATURATION,
    param::LUMINOSITY,
];

/// One parameter: its value before the pen drives it, what drives it,
/// and its range.
#[derive(Clone, Debug)]
struct Param {
    base: f32,
    bindings: Vec<Binding>,
    range: Option<(f32, f32)>,
}

impl Param {
    fn new(preset: &BrushPreset, registry: &BrushRegistry, key: &str, scale: f32) -> Self {
        let mut base = registry.value(preset, key).unwrap_or(0.0);
        if key == param::SIZE && preset.size_on_screen {
            base /= scale;
        }
        Self {
            base,
            bindings: preset.bindings(key).filter(|b| b.on).cloned().collect(),
            range: registry.param(key).map(|def| (def.min, def.max)),
        }
    }

    fn eval(&self, inputs: &Inputs) -> f32 {
        let mut value = self.base;
        for binding in &self.bindings {
            value = binding.apply(value, inputs.share(&binding.input));
        }
        match self.range {
            Some((min, max)) => value.clamp(min, max),
            None => value,
        }
    }
}

/// The inputs at one point.
struct Inputs {
    pressure: f32,
    speed: f32,
    tilt: [f32; 2],
    twist: f32,
    direction: f32,
    distance: f32,
    time: f32,
    random: f32,
    stroke_random: f32,
}

impl Inputs {
    fn share(&self, key: &str) -> f32 {
        let turn = |degrees: f32| (degrees / 360.0).rem_euclid(1.0);
        match key {
            input::PRESSURE => self.pressure,
            input::SPEED => self.speed / FULL_SPEED,
            input::TILT => {
                let [x, y] = self.tilt;
                (x * x + y * y).sqrt()
            }
            input::TILT_DIRECTION => math::direction(self.tilt).map_or(0.0, turn),
            input::TWIST => turn(self.twist),
            input::DIRECTION => turn(self.direction),
            input::DISTANCE => self.distance / FULL_DISTANCE,
            input::TIME => self.time / FULL_TIME,
            input::RANDOM => self.random,
            input::STROKE_RANDOM => self.stroke_random,
            _ => 0.0,
        }
        .clamp(0.0, 1.0)
    }
}

/// The brush's parameters and what drives them. See the module.
#[derive(Clone, Debug)]
pub(crate) struct Dynamics {
    standard: Vec<Param>,
    extra: Vec<Param>,
    extra_keys: Vec<String>,
    seed: u64,
    stroke_random: f32,
    /// The time of the stroke's first point.
    start: Option<f64>,
}

impl Dynamics {
    pub fn new(preset: &BrushPreset, registry: &BrushRegistry, scale: f32, seed: u64) -> Self {
        let standard = STANDARD
            .iter()
            .map(|key| Param::new(preset, registry, key, scale))
            .collect();
        // Every other parameter the registry knows or the preset holds.
        let mut extra_keys: Vec<String> = registry
            .params()
            .map(|def| def.key.clone())
            .chain(preset.params.keys().cloned())
            .chain(preset.dynamics.iter().map(|b| b.param.clone()))
            .filter(|key| !STANDARD.contains(&key.as_str()))
            .collect();
        extra_keys.sort();
        extra_keys.dedup();
        let extra = extra_keys
            .iter()
            .map(|key| Param::new(preset, registry, key, scale))
            .collect();
        Self {
            standard,
            extra,
            extra_keys,
            seed,
            stroke_random: math::chance(seed, u64::MAX),
            start: None,
        }
    }

    /// The brush's size before the pen drives it, in layer pixels.
    pub fn base_size(&self) -> f32 {
        self.standard[0].base
    }

    pub fn extra_keys(&self) -> &[String] {
        &self.extra_keys
    }

    /// The point at `base`, without the tapers (`taper` is 1).
    pub fn eval(&self, base: &Base) -> StrokePoint {
        let r = &base.record;
        let start = self.start.unwrap_or(r.time);
        let inputs = Inputs {
            pressure: r.pressure,
            speed: r.speed,
            tilt: r.tilt,
            twist: r.twist,
            direction: base.direction,
            distance: base.arc,
            time: (r.time - start).max(0.0) as f32,
            random: math::chance(self.seed, base.index),
            stroke_random: self.stroke_random,
        };
        let v: Vec<f32> = self.standard.iter().map(|p| p.eval(&inputs)).collect();
        StrokePoint {
            pos: r.pos,
            arc: base.arc,
            time: r.time,
            pressure: r.pressure,
            speed: r.speed,
            tilt: r.tilt,
            twist: r.twist.rem_euclid(360.0),
            direction: base.direction,
            taper: 1.0,
            size: v[0],
            opacity: v[1],
            flow: v[2],
            density: v[3],
            hardness: v[4],
            aspect: v[5],
            angle: v[6],
            spacing: v[7],
            scatter: v[8],
            texture_strength: v[9],
            hue: v[10],
            saturation: v[11],
            luminosity: v[12],
            extra: self.extra.iter().map(|p| p.eval(&inputs)).collect(),
        }
    }

    /// Notes the stroke's first time, for the `time` input.
    pub fn start_at(&mut self, time: f64) {
        self.start.get_or_insert(time);
    }
}
