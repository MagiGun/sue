//! # The input pipeline: pen samples in, stroke points out
//!
//! A stroke starts with a [`Kind::Down`] sample, carries on with
//! [`Kind::Move`]s and ends with a [`Kind::Up`]. [`Pipeline::push`] takes
//! them one by one, in the order the device sent them, and turns them
//! into [`StrokePoint`]s: where the brush is, how far along the stroke,
//! and every parameter of the brush ([`sue_model::brush::param`]) after
//! its dynamics and the tapers. The stages, each in its own file:
//!
//! 1. **Ingest** (`ingest.rs`): maps to layer space, drops a packet
//!    that comes twice, repairs time that does not go forward, fills a
//!    packet without pressure from the one before (never from a full
//!    press; packets before a pen's first pressure wait for it), and
//!    says where and when the pen lifted. A mouse is a device that
//!    declares one pressure for good ([`Device::Constant`]), not a
//!    fallback.
//! 2. **Pressure** (also `ingest.rs`): the person's pressure curve, a
//!    median of three against single-packet spikes, a limit on how fast
//!    pressure may rise, and a short smoothing against the tablet's
//!    steps. The speed of the pen is measured here, on device time.
//! 3. **Resampling** (`resample.rs`): records at a fixed [`RATE`] in
//!    device time, so the stabilizer does not depend on how often the
//!    device or the frames come.
//! 4. **Stabilizer** (`stabilize.rs`): a weighted average of whole
//!    records (place, pressure, tilt, twist and speed together) over a
//!    window of device time set by Clip Studio's 0 to 100 scale, which
//!    shortens as the pen speeds up when "adjust by speed" is on, and
//!    as far as the brush must to trail the pen by no more than
//!    `10 + 1.5 * level` screen points. On the lift it is drained along
//!    the records it still holds, without a kink, onto the pen's path,
//!    and then the line ramps to the lift's pressure (none, for a pen).
//!    At level 0 there is neither resampling nor stabilizer: the curve
//!    goes through the device's own samples.
//! 5. **Curve** (`curve.rs`): a centripetal Catmull-Rom curve through
//!    the records, built segment by segment with an arc-length table and
//!    sampled at even steps of arc length. Only the last two segments
//!    can still change.
//! 6. **Dynamics** (`dynamics.rs`): every binding of the brush, at every
//!    point.
//! 7. **Tapers** (`taper.rs`): the starting and the ending as functions
//!    of arc length. The ending is drawn while the pen is still down, on
//!    the provisional end, so what the artist sees before the lift is
//!    what stays after it.
//!
//! What comes out is in two parts. [`Pipeline::committed`] only grows:
//! a point there never changes again. [`Pipeline::tail`] is the rest of
//! the stroke as it stands now; it is worked out afresh on every call
//! and may change with the next sample. After the [`Kind::Up`] the whole
//! stroke is committed and the tail is empty.
//!
//! Three promises hold for every stroke, and the tests hold them:
//!
//! - Nothing after the last contact presses harder than the stroke did
//!   at that contact: a lift never draws a bigger end.
//! - Over the final taper window the size never grows.
//! - The same samples give the same points, however they are split into
//!   calls, on every thread count and platform: nothing here reads a
//!   clock, a frame or the system's math library.

mod curve;
mod dynamics;
mod ingest;
mod math;
mod resample;
mod stabilize;
mod taper;

use sue_model::brush::BrushRegistry;
use sue_model::{BrushPreset, Response};

use self::curve::Curve;
use self::dynamics::Dynamics;
use self::ingest::{Event, Ingest};
use self::resample::Resampler;
use self::stabilize::Stabilizer;
use self::taper::Shaper;

/// How many records a second the resampler makes, in device time.
pub const RATE: f64 = 400.0;

/// What a sample says happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The tip touched here: the stroke starts.
    Down,
    /// It moved here, or pressed or leaned differently, while touching.
    Move,
    /// It lifted here. A lift is where the line ends, never one more
    /// point pressed at some strength: its pressure is not read.
    Up,
}

/// One packet of a pointing device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Where, in the space [`Settings::to_layer`] maps from.
    pub pos: [f32; 2],
    /// When, in seconds on the device's clock (or a monotonic one).
    pub time: f64,
    /// How hard it pressed, 0 to 1. `None` when the packet did not say:
    /// the pressure before it is kept.
    pub pressure: Option<f32>,
    /// How far it leans, `[x, y]`, each -1 to 1, 0 upright.
    pub tilt: Option<[f32; 2]>,
    /// Its turn about its own axis, in degrees.
    pub twist: Option<f32>,
    /// What happened.
    pub kind: Kind,
}

impl Sample {
    /// A sample of `kind` at `pos` and `time`, saying nothing else.
    #[must_use]
    pub fn new(kind: Kind, pos: [f32; 2], time: f64) -> Self {
        Self {
            pos,
            time,
            pressure: None,
            tilt: None,
            twist: None,
            kind,
        }
    }

    /// The same sample pressing `pressure`.
    #[must_use]
    pub fn pressing(mut self, pressure: f32) -> Self {
        self.pressure = Some(pressure);
        self
    }
}

/// What kind of device draws the stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Device {
    /// A pen: pressure comes with its packets.
    Pen,
    /// A device that presses the same all the time: a mouse (1), a
    /// finger on a screen that does not feel force. Pressure in its
    /// samples is not read.
    Constant(f32),
}

impl Device {
    /// A mouse: always fully pressed.
    pub const MOUSE: Self = Self::Constant(1.0);
}

/// What a stroke is drawn with besides the brush. Everything here is
/// stored with a stroke that replays, so a replay gives the same points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The device.
    pub device: Device,
    /// The person's own pressure curve, before any brush's.
    pub curve: Response,
    /// Maps a sample's place to the layer's pixels: `x' = m[0][0] x +
    /// m[0][1] y + m[0][2]` and `y' = m[1][0] x + m[1][1] y + m[1][2]`.
    pub to_layer: [[f32; 3]; 2],
    /// Screen points per layer pixel when the stroke was drawn (the
    /// zoom). The stabilizer's farthest lag, the speed it loosens at, the
    /// spacing of the curve's knots and a size measured on the screen
    /// are in screen points.
    pub scale: f32,
    /// The seed of the stroke's chance (the `random` and
    /// `stroke_random` inputs).
    pub seed: u64,
    /// The arc length between two points, in layer pixels. `None`: a
    /// twentieth of the brush's size, from a quarter of a pixel to two.
    pub step: Option<f32>,
}

impl Settings {
    /// For `device`, with a straight pressure curve, samples already in
    /// layer pixels at 100 % zoom, and seed 0.
    #[must_use]
    pub fn new(device: Device) -> Self {
        Self {
            device,
            curve: Response::LINEAR,
            to_layer: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            scale: 1.0,
            seed: 0,
            step: None,
        }
    }
}

/// One point of a stroke: where the brush is and everything about it
/// there. Sizes and lengths are in layer pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokePoint {
    /// Where.
    pub pos: [f32; 2],
    /// How far along the stroke.
    pub arc: f32,
    /// The device time the point stands for.
    pub time: f64,
    /// The pressure, 0 to 1, after the curve and the stabilizer.
    pub pressure: f32,
    /// How fast the pen moved, in layer pixels a second.
    pub speed: f32,
    /// How the pen leaned.
    pub tilt: [f32; 2],
    /// How the pen was turned, degrees from 0 to 360.
    pub twist: f32,
    /// The way the line runs, degrees from 0 to 360 (0 to the right,
    /// 90 down).
    pub direction: f32,
    /// How much of the starting and ending is applied: 1 is none, 0 is
    /// gone. Already in `size` and `opacity`, as the brush says.
    pub taper: f32,
    /// The diameter.
    pub size: f32,
    /// The stroke's opacity here.
    pub opacity: f32,
    /// One dab's strength.
    pub flow: f32,
    /// The tip's own density.
    pub density: f32,
    /// The tip's hardness.
    pub hardness: f32,
    /// The tip's height over its width.
    pub aspect: f32,
    /// The tip's turn, in degrees.
    pub angle: f32,
    /// The distance between dabs.
    pub spacing: f32,
    /// How far dabs fall from the line, in sizes.
    pub scatter: f32,
    /// How strongly the texture breaks the stroke up.
    pub texture_strength: f32,
    /// The paint's hue turn, in degrees.
    pub hue: f32,
    /// The paint's saturation change.
    pub saturation: f32,
    /// The paint's luminosity change.
    pub luminosity: f32,
    /// The parameters beyond the standard ones, in the order of
    /// [`Pipeline::extra_keys`].
    pub extra: Vec<f32>,
}

/// Where the stroke stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// No [`Kind::Down`] yet.
    Waiting,
    /// The pen is down.
    Drawing,
    /// The pen has lifted; everything is committed.
    Done,
}

/// One stroke's way from pen samples to stroke points. See the module.
#[derive(Clone, Debug)]
pub struct Pipeline {
    phase: Phase,
    ingest: Ingest,
    /// The resampler and the stabilizer; none at stabilization 0, where
    /// the curve goes through the device's own samples.
    steady: Option<(Resampler, Stabilizer)>,
    curve: Curve,
    dynamics: Dynamics,
    shaper: Shaper,
}

impl Pipeline {
    /// A pipeline for one stroke of `preset`, whose parameters `registry`
    /// knows, drawn as `settings` say.
    #[must_use]
    pub fn new(preset: &BrushPreset, registry: &BrushRegistry, settings: Settings) -> Self {
        let scale = if settings.scale.is_finite() && settings.scale > 0.0 {
            settings.scale
        } else {
            1.0
        };
        let dynamics = Dynamics::new(preset, registry, scale, settings.seed);
        let step = settings
            .step
            .filter(|step| step.is_finite() && *step > 0.0)
            .unwrap_or_else(|| (dynamics.base_size() * 0.05).clamp(0.25, 2.0));
        let level = preset.input.stabilization.min(100);
        let steady = (level > 0).then(|| {
            (
                Resampler::new(),
                Stabilizer::new(level, preset.input.by_speed, scale),
            )
        });
        Self {
            phase: Phase::Waiting,
            ingest: Ingest::new(settings.device, settings.curve, settings.to_layer),
            steady,
            curve: Curve::new(step, curve::knot_spacing(scale)),
            dynamics,
            shaper: Shaper::new(preset.taper, preset.min_width),
        }
    }

    /// Takes the device's next sample.
    pub fn push(&mut self, sample: Sample) {
        if self.phase == Phase::Done {
            return;
        }
        let mut events = Vec::new();
        self.ingest.push(sample, &mut events);
        for event in events {
            match event {
                Event::Contact(record) => {
                    self.phase = Phase::Drawing;
                    self.dynamics.start_at(record.time);
                    self.feed(record);
                }
                Event::Lift(record) => {
                    self.finish(record);
                    self.phase = Phase::Done;
                }
            }
        }
    }

    /// Takes several samples, as a frame hands them over. The same as
    /// pushing each in turn.
    pub fn push_all(&mut self, samples: &[Sample]) {
        for sample in samples {
            self.push(*sample);
        }
    }

    /// The points that never change again, from the stroke's start.
    #[must_use]
    pub fn committed(&self) -> &[StrokePoint] {
        self.shaper.committed()
    }

    /// The rest of the stroke as it stands now, after the committed
    /// points. Empty before the first contact and after the lift.
    #[must_use]
    pub fn tail(&self) -> Vec<StrokePoint> {
        if self.phase != Phase::Drawing {
            return Vec::new();
        }
        let mut curve = self.curve.clone();
        let mut bases = Vec::new();
        if let Some((resampler, stabilizer)) = &self.steady
            && let Some(record) = stabilizer.peek(resampler.partial())
        {
            curve.push(record, &mut bases);
        }
        curve.finish(&mut bases);
        let points: Vec<StrokePoint> = bases.iter().map(|base| self.dynamics.eval(base)).collect();
        self.shaper.tail(&points)
    }

    /// The whole stroke as it stands now: the committed points, then the
    /// tail.
    #[must_use]
    pub fn points(&self) -> Vec<StrokePoint> {
        let mut points = self.committed().to_vec();
        points.extend(self.tail());
        points
    }

    /// `true` once the pen has lifted.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// The keys of [`StrokePoint::extra`], in order.
    #[must_use]
    pub fn extra_keys(&self) -> &[String] {
        self.dynamics.extra_keys()
    }

    /// One record of a touching pen, through the stages.
    fn feed(&mut self, record: ingest::Record) {
        let mut bases = Vec::new();
        match &mut self.steady {
            Some((resampler, stabilizer)) => {
                let mut ticks = Vec::new();
                resampler.push(record, &mut ticks);
                for tick in ticks {
                    let steady = stabilizer.push(tick);
                    self.curve.push(steady, &mut bases);
                }
            }
            None => self.curve.push(record, &mut bases),
        }
        self.shape(&bases);
    }

    /// The lift at `up`: the last records, the stabilizer drained, the
    /// curve finished, everything committed.
    fn finish(&mut self, up: ingest::Record) {
        let mut bases = Vec::new();
        match &mut self.steady {
            Some((resampler, stabilizer)) => {
                // The last contact as a tick of its own, then the drain
                // to it. Nothing after the brush's place at the last
                // contact presses harder than the brush did there.
                let mut cap = f32::INFINITY;
                if let Some(tick) = resampler.partial() {
                    let steady = stabilizer.push(tick);
                    cap = steady.pressure;
                    self.curve.push(steady, &mut bases);
                } else if let Some(steady) = stabilizer.peek(None) {
                    cap = steady.pressure;
                }
                self.curve.settle(&mut bases);
                for mut steady in stabilizer.drain() {
                    steady.pressure = steady.pressure.min(cap);
                    cap = steady.pressure;
                    self.curve.push(steady, &mut bases);
                }
                // Then the lift itself, where the drain ended: the ramp
                // to its pressure.
                let mut up = up;
                up.pressure = up.pressure.min(cap);
                if let Some(end) = stabilizer.peek(None) {
                    up.pos = end.pos;
                }
                self.curve.push(up, &mut bases);
            }
            None => {
                // The last contact stays on the curve; the ramp runs from
                // it to the lift.
                self.curve.settle(&mut bases);
                self.curve.push(up, &mut bases);
            }
        }
        self.curve.finish(&mut bases);
        let points: Vec<StrokePoint> = bases.iter().map(|base| self.dynamics.eval(base)).collect();
        self.shaper.finish(&points);
    }

    /// Points of final curve segments, into the shaper.
    fn shape(&mut self, bases: &[curve::Base]) {
        for base in bases {
            let point = self.dynamics.eval(base);
            self.shaper.push(point);
        }
    }
}
