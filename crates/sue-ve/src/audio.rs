//! Sound processing for the Audio page, as a mixing desk does it: a
//! track's equalizer (five bands), noise gate and compressor, pan, and
//! loudness measured the way broadcasters do (ITU-R BS.1770: K weighted,
//! gated; LUFS), which the meters show and Normalize aims at.
//!
//! A track's effects live in its properties ([`crate::fx::key::AUDIO_FX`])
//! as a map, so every change is part of the sequence's undo step. The
//! processing keeps its filters' state from one block to the next
//! ([`TrackFx::process`]); the mixer starts it again when playing jumps.

use std::collections::BTreeMap;

use sue_core::Value;

/// One biquad filter (Robert Bristow-Johnson's cookbook), direct form 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [[f64; 2]; 2],
    y: [[f64; 2]; 2],
}

/// The shape of an equalizer band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandKind {
    /// Lifts or cuts everything below the frequency.
    LowShelf,
    /// A bell round the frequency.
    Peak,
    /// Lifts or cuts everything above the frequency.
    HighShelf,
    /// Takes away what is below the frequency.
    HighPass,
    /// Takes away what is above the frequency.
    LowPass,
}

impl BandKind {
    /// Every kind.
    pub const ALL: [Self; 5] = [
        Self::LowShelf,
        Self::Peak,
        Self::HighShelf,
        Self::HighPass,
        Self::LowPass,
    ];

    /// Its stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::LowShelf => "low-shelf",
            Self::Peak => "peak",
            Self::HighShelf => "high-shelf",
            Self::HighPass => "high-pass",
            Self::LowPass => "low-pass",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.id() == id)
            .unwrap_or(Self::Peak)
    }
}

impl Biquad {
    /// A filter of `kind` at `frequency` Hz with `gain` dB and quality
    /// `q`, for sound at `rate`.
    #[must_use]
    pub fn new(kind: BandKind, frequency: f64, gain: f64, q: f64, rate: f64) -> Self {
        let frequency = frequency.clamp(10.0, rate * 0.49);
        let w0 = std::f64::consts::TAU * frequency / rate;
        let (sin, cos) = w0.sin_cos();
        let q = q.max(0.05);
        let alpha = sin / (2.0 * q);
        let a = 10_f64.powf(gain / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            BandKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            BandKind::LowShelf => {
                let root = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + root),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - root),
                    (a + 1.0) + (a - 1.0) * cos + root,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - root,
                )
            }
            BandKind::HighShelf => {
                let root = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + root),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - root),
                    (a + 1.0) - (a - 1.0) * cos + root,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - root,
                )
            }
            BandKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            BandKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
        };
        Self {
            b: [b0 / a0, b1 / a0, b2 / a0],
            a: [a1 / a0, a2 / a0],
            x: [[0.0; 2]; 2],
            y: [[0.0; 2]; 2],
        }
    }

    /// A filter from raw coefficients (the K weighting's).
    #[must_use]
    pub fn raw(b: [f64; 3], a: [f64; 2]) -> Self {
        Self {
            b,
            a,
            x: [[0.0; 2]; 2],
            y: [[0.0; 2]; 2],
        }
    }

    /// Filters one sample of `channel` (0 or 1).
    pub fn tick(&mut self, channel: usize, input: f64) -> f64 {
        let [x1, x2] = self.x[channel];
        let [y1, y2] = self.y[channel];
        let output =
            self.b[0] * input + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
        self.x[channel] = [input, x1];
        self.y[channel] = [output, y1];
        output
    }

    /// Its gain at `frequency` Hz, in decibels, for sound at `rate`.
    #[must_use]
    pub fn response(&self, frequency: f64, rate: f64) -> f64 {
        let w = std::f64::consts::TAU * frequency / rate;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let num = (
            self.b[0] + self.b[1] * c1 + self.b[2] * c2,
            -(self.b[1] * s1 + self.b[2] * s2),
        );
        let den = (
            1.0 + self.a[0] * c1 + self.a[1] * c2,
            -(self.a[0] * s1 + self.a[1] * s2),
        );
        let magnitude = (num.0.hypot(num.1)) / den.0.hypot(den.1).max(1e-12);
        20.0 * magnitude.max(1e-12).log10()
    }
}

/// One band of the equalizer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// On.
    pub on: bool,
    /// Its shape.
    pub kind: BandKind,
    /// Its frequency, Hz.
    pub frequency: f64,
    /// Lift or cut, dB.
    pub gain: f64,
    /// How narrow.
    pub q: f64,
}

/// A track's effects: equalizer, gate, compressor.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackFx {
    /// The equalizer is on.
    pub eq_on: bool,
    /// Its five bands, low to high.
    pub bands: [Band; 5],
    /// The gate is on.
    pub gate_on: bool,
    /// Below this the gate closes, dBFS.
    pub gate_threshold: f64,
    /// How far it closes, dB.
    pub gate_range: f64,
    /// How fast it opens and closes, ms.
    pub gate_release: f64,
    /// The compressor is on.
    pub comp_on: bool,
    /// Above this it compresses, dBFS.
    pub threshold: f64,
    /// How much, n to 1.
    pub ratio: f64,
    /// How fast it reacts, ms.
    pub attack: f64,
    /// How fast it lets go, ms.
    pub release: f64,
    /// Gain after it, dB.
    pub makeup: f64,
}

impl Default for TrackFx {
    fn default() -> Self {
        let band = |kind, frequency| Band {
            on: true,
            kind,
            frequency,
            gain: 0.0,
            q: 0.71,
        };
        Self {
            eq_on: false,
            bands: [
                band(BandKind::LowShelf, 80.0),
                band(BandKind::Peak, 250.0),
                band(BandKind::Peak, 1000.0),
                band(BandKind::Peak, 4000.0),
                band(BandKind::HighShelf, 10_000.0),
            ],
            gate_on: false,
            gate_threshold: -50.0,
            gate_range: -40.0,
            gate_release: 80.0,
            comp_on: false,
            threshold: -18.0,
            ratio: 3.0,
            attack: 10.0,
            release: 120.0,
            makeup: 0.0,
        }
    }
}

impl TrackFx {
    /// `true` when it does nothing.
    #[must_use]
    pub fn is_bypassed(&self) -> bool {
        !(self.eq_on || self.gate_on || self.comp_on)
    }

    /// As it is stored.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = BTreeMap::new();
        let float = |v: f64| Value::Float(v);
        map.insert("eq".to_owned(), Value::Bool(self.eq_on));
        map.insert(
            "bands".to_owned(),
            Value::List(
                self.bands
                    .iter()
                    .map(|band| {
                        let mut b = BTreeMap::new();
                        b.insert("on".to_owned(), Value::Bool(band.on));
                        b.insert("kind".to_owned(), Value::Text(band.kind.id().to_owned()));
                        b.insert("frequency".to_owned(), float(band.frequency));
                        b.insert("gain".to_owned(), float(band.gain));
                        b.insert("q".to_owned(), float(band.q));
                        Value::Map(b)
                    })
                    .collect(),
            ),
        );
        map.insert("gate".to_owned(), Value::Bool(self.gate_on));
        map.insert("gate_threshold".to_owned(), float(self.gate_threshold));
        map.insert("gate_range".to_owned(), float(self.gate_range));
        map.insert("gate_release".to_owned(), float(self.gate_release));
        map.insert("comp".to_owned(), Value::Bool(self.comp_on));
        map.insert("threshold".to_owned(), float(self.threshold));
        map.insert("ratio".to_owned(), float(self.ratio));
        map.insert("attack".to_owned(), float(self.attack));
        map.insert("release".to_owned(), float(self.release));
        map.insert("makeup".to_owned(), float(self.makeup));
        Value::Map(map)
    }

    /// Read back from what is stored.
    #[must_use]
    pub fn from_value(value: &Value) -> Self {
        let mut out = Self::default();
        let Value::Map(map) = value else {
            return out;
        };
        let number =
            |key: &str, default: f64| map.get(key).and_then(Value::as_f64).unwrap_or(default);
        let flag = |key: &str| map.get(key).and_then(Value::as_bool).unwrap_or(false);
        out.eq_on = flag("eq");
        if let Some(Value::List(bands)) = map.get("bands") {
            for (slot, band) in out.bands.iter_mut().zip(bands) {
                if let Value::Map(b) = band {
                    slot.on = b.get("on").and_then(Value::as_bool).unwrap_or(true);
                    slot.kind =
                        BandKind::from_id(b.get("kind").and_then(Value::as_str).unwrap_or("peak"));
                    slot.frequency = b
                        .get("frequency")
                        .and_then(Value::as_f64)
                        .unwrap_or(slot.frequency);
                    slot.gain = b.get("gain").and_then(Value::as_f64).unwrap_or(0.0);
                    slot.q = b.get("q").and_then(Value::as_f64).unwrap_or(slot.q);
                }
            }
        }
        out.gate_on = flag("gate");
        out.gate_threshold = number("gate_threshold", out.gate_threshold);
        out.gate_range = number("gate_range", out.gate_range);
        out.gate_release = number("gate_release", out.gate_release);
        out.comp_on = flag("comp");
        out.threshold = number("threshold", out.threshold);
        out.ratio = number("ratio", out.ratio).max(1.0);
        out.attack = number("attack", out.attack);
        out.release = number("release", out.release);
        out.makeup = number("makeup", out.makeup);
        out
    }

    /// The filters of its bands, for sound at `rate`.
    #[must_use]
    pub fn filters(&self, rate: f64) -> Vec<Biquad> {
        self.bands
            .iter()
            .filter(|band| {
                band.on
                    && (band.gain.abs() > 1e-6
                        || matches!(band.kind, BandKind::HighPass | BandKind::LowPass))
            })
            .map(|band| Biquad::new(band.kind, band.frequency, band.gain, band.q, rate))
            .collect()
    }
}

/// A track's effects of `track` (default when it has none).
#[must_use]
pub fn track_fx(track: &sue_model::Track) -> TrackFx {
    track
        .properties
        .get(crate::fx::key::AUDIO_FX)
        .map(TrackFx::from_value)
        .unwrap_or_default()
}

/// The running state of a track's effects.
#[derive(Clone, Debug, Default)]
pub struct FxState {
    filters: Vec<Biquad>,
    /// The settings the filters were made for.
    made_for: Option<TrackFx>,
    /// The compressor's level follower, linear.
    envelope: f64,
    /// The gate's gain, linear.
    gate: f64,
    /// The level the gate listens to (peaks held a moment).
    gate_level: f64,
    /// How much the compressor takes off now, dB (for its meter).
    pub reduction: f64,
}

impl FxState {
    /// Processes interleaved stereo `block` at `rate` through `fx`.
    pub fn process(&mut self, fx: &TrackFx, block: &mut [f32], rate: f64) {
        if fx.is_bypassed() {
            self.reduction = 0.0;
            return;
        }
        if self.made_for.as_ref() != Some(fx) {
            self.filters = fx.filters(rate);
            self.made_for = Some(fx.clone());
            self.gate = 1.0;
        }
        let coefficient = |ms: f64| (-1.0 / (ms.max(0.1) / 1000.0 * rate)).exp();
        let (attack, release) = (coefficient(fx.attack), coefficient(fx.release));
        let gate_speed = coefficient(fx.gate_release);
        let gate_hold = coefficient(20.0);
        let gate_floor = db_to_gain(fx.gate_range);
        let gate_open = db_to_gain(fx.gate_threshold);
        let makeup = db_to_gain(fx.makeup);
        let mut most = 0.0_f64;
        for pair in block.as_chunks_mut::<2>().0 {
            let mut sample = [f64::from(pair[0]), f64::from(pair[1])];
            if fx.eq_on {
                for filter in &mut self.filters {
                    for (channel, value) in sample.iter_mut().enumerate() {
                        *value = filter.tick(channel, *value);
                    }
                }
            }
            let level = sample[0].abs().max(sample[1].abs());
            if fx.gate_on {
                self.gate_level = level.max(self.gate_level * gate_hold);
                let target = if self.gate_level >= gate_open {
                    1.0
                } else {
                    gate_floor
                };
                self.gate = target + (self.gate - target) * gate_speed;
                for value in &mut sample {
                    *value *= self.gate;
                }
            }
            if fx.comp_on {
                let coefficient = if level > self.envelope {
                    attack
                } else {
                    release
                };
                self.envelope = level + (self.envelope - level) * coefficient;
                let over = gain_to_db(self.envelope) - fx.threshold;
                let cut = if over > 0.0 {
                    over * (1.0 - 1.0 / fx.ratio.max(1.0))
                } else {
                    0.0
                };
                most = most.max(cut);
                let gain = db_to_gain(-cut) * makeup;
                for value in &mut sample {
                    *value *= gain;
                }
            }
            pair[0] = sample[0] as f32;
            pair[1] = sample[1] as f32;
        }
        self.reduction = most;
    }

    /// Forgets what came before (playing jumped).
    pub fn reset(&mut self) {
        self.made_for = None;
        self.envelope = 0.0;
        self.gate = 1.0;
        self.gate_level = 0.0;
    }
}

/// Decibels as a gain.
#[must_use]
pub fn db_to_gain(db: f64) -> f64 {
    10_f64.powf(db / 20.0)
}

/// A gain in decibels.
#[must_use]
pub fn gain_to_db(gain: f64) -> f64 {
    20.0 * gain.max(1e-9).log10()
}

/// The left and right gains of a pan (or balance) of `pan`, -1 to 1:
/// the far side fades out, the near side stays.
#[must_use]
pub fn pan_gains(pan: f64) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    [(1.0 - pan.max(0.0)) as f32, (1.0 + pan.min(0.0)) as f32]
}

/// Loudness as ITU-R BS.1770-4 measures it: K weighted, in 400 ms blocks
/// that overlap by three quarters, gated (absolute -70 LUFS, then 10 LU
/// under the mean) for the integrated value.
#[derive(Clone, Debug)]
pub struct Loudness {
    rate: f64,
    shelf: Biquad,
    high_pass: Biquad,
    /// Squared K weighted samples summed per 100 ms step.
    steps: Vec<f64>,
    current: f64,
    in_step: usize,
    step_len: usize,
    /// The mean squares of every 400 ms block so far.
    blocks: Vec<f64>,
}

impl Loudness {
    /// A meter for sound at `rate`.
    #[must_use]
    pub fn new(rate: u32) -> Self {
        let fs = f64::from(rate.max(8000));
        // The K weighting's two stages (BS.1770-4's, worked for any rate).
        let shelf = {
            let f0 = 1_681.974_450_955_533;
            let g = 3.999_843_853_973_347;
            let q = 0.707_175_236_955_419_6;
            let k = (std::f64::consts::PI * f0 / fs).tan();
            let vh = 10_f64.powf(g / 20.0);
            let vb = vh.powf(0.499_666_774_154_541_6);
            let a0 = 1.0 + k / q + k * k;
            Biquad::raw(
                [
                    (vh + vb * k / q + k * k) / a0,
                    2.0 * (k * k - vh) / a0,
                    (vh - vb * k / q + k * k) / a0,
                ],
                [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            )
        };
        let high_pass = {
            let f0 = 38.135_470_876_024_44;
            let q = 0.500_327_037_323_877_3;
            let k = (std::f64::consts::PI * f0 / fs).tan();
            let a0 = 1.0 + k / q + k * k;
            Biquad::raw(
                [1.0, -2.0, 1.0],
                [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            )
        };
        Self {
            rate: fs,
            shelf,
            high_pass,
            steps: Vec::new(),
            current: 0.0,
            in_step: 0,
            step_len: (fs / 10.0).round() as usize,
            blocks: Vec::new(),
        }
    }

    /// Takes in interleaved stereo.
    pub fn feed(&mut self, block: &[f32]) {
        for pair in block.as_chunks::<2>().0 {
            let mut power = 0.0;
            for (channel, value) in pair.iter().enumerate() {
                let k = self
                    .high_pass
                    .tick(channel, self.shelf.tick(channel, f64::from(*value)));
                power += k * k;
            }
            self.current += power;
            self.in_step += 1;
            if self.in_step >= self.step_len {
                self.steps.push(self.current / self.step_len as f64);
                self.current = 0.0;
                self.in_step = 0;
                if self.steps.len() >= 4 {
                    let last = &self.steps[self.steps.len() - 4..];
                    self.blocks.push(last.iter().sum::<f64>() / 4.0);
                }
                // Keep three seconds of steps for the short term.
                if self.steps.len() > 300 {
                    self.steps.drain(..self.steps.len() - 30);
                }
            }
        }
    }

    fn lufs(mean_square: f64) -> f64 {
        -0.691 + 10.0 * mean_square.max(1e-12).log10()
    }

    /// The last 400 ms, LUFS.
    #[must_use]
    pub fn momentary(&self) -> f64 {
        self.blocks.last().map_or(-120.0, |m| Self::lufs(*m))
    }

    /// The last 3 seconds, LUFS.
    #[must_use]
    pub fn short_term(&self) -> f64 {
        let take = self.steps.len().min(30);
        if take == 0 {
            return -120.0;
        }
        let mean = self.steps[self.steps.len() - take..].iter().sum::<f64>() / take as f64;
        Self::lufs(mean)
    }

    /// Everything so far, gated, LUFS.
    #[must_use]
    pub fn integrated(&self) -> f64 {
        let loud: Vec<f64> = self
            .blocks
            .iter()
            .copied()
            .filter(|m| Self::lufs(*m) > -70.0)
            .collect();
        if loud.is_empty() {
            return -120.0;
        }
        let mean = loud.iter().sum::<f64>() / loud.len() as f64;
        let gate = Self::lufs(mean) - 10.0;
        let kept: Vec<f64> = loud.into_iter().filter(|m| Self::lufs(*m) > gate).collect();
        if kept.is_empty() {
            return -120.0;
        }
        Self::lufs(kept.iter().sum::<f64>() / kept.len() as f64)
    }

    /// Starts again.
    pub fn reset(&mut self) {
        *self = Self::new(self.rate as u32);
    }
}

/// The integrated loudness of interleaved stereo `samples` at `rate`.
#[must_use]
pub fn loudness_of(samples: &[f32], rate: u32) -> f64 {
    let mut meter = Loudness::new(rate);
    meter.feed(samples);
    meter.integrated()
}

/// The gain in decibels that brings sound of `measured` LUFS to
/// `target`, held so its peak stays under -1 dBFS.
#[must_use]
pub fn normalize_gain(measured: f64, target: f64, peak: f32) -> f64 {
    if measured <= -100.0 {
        return 0.0;
    }
    let wanted = target - measured;
    let headroom = -1.0 - gain_to_db(f64::from(peak.max(1e-9)));
    wanted.min(headroom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency: f64, amplitude: f64, seconds: f64, rate: u32) -> Vec<f32> {
        let count = (seconds * f64::from(rate)) as usize;
        (0..count)
            .flat_map(|n| {
                let v = (amplitude
                    * (std::f64::consts::TAU * frequency * n as f64 / f64::from(rate)).sin())
                    as f32;
                [v, v]
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len().max(1) as f64)
            .sqrt()
    }

    #[test]
    fn the_equalizer_bands_lift_and_cut_where_they_should() {
        let rate = 48_000.0;
        let peak = Biquad::new(BandKind::Peak, 1000.0, 6.0, 1.0, rate);
        assert!((peak.response(1000.0, rate) - 6.0).abs() < 0.05);
        assert!(peak.response(100.0, rate).abs() < 0.5);
        let low = Biquad::new(BandKind::LowShelf, 100.0, -9.0, 0.71, rate);
        assert!((low.response(20.0, rate) + 9.0).abs() < 0.5);
        assert!(low.response(5000.0, rate).abs() < 0.2);
        let high = Biquad::new(BandKind::HighPass, 200.0, 0.0, 0.71, rate);
        assert!(high.response(50.0, rate) < -20.0);
        // Processing a tone through a cut band lowers it by that much.
        let mut fx = TrackFx {
            eq_on: true,
            ..TrackFx::default()
        };
        fx.bands[2].gain = -12.0;
        let mut tone = sine(1000.0, 0.5, 0.5, 48_000);
        let before = rms(&tone[24_000..]);
        FxState::default().process(&fx, &mut tone, rate);
        let after = rms(&tone[24_000..]);
        assert!(
            (gain_to_db(after / before) + 12.0).abs() < 0.5,
            "{}",
            gain_to_db(after / before)
        );
    }

    #[test]
    fn the_compressor_holds_loud_sound_down_by_its_ratio() {
        let fx = TrackFx {
            comp_on: true,
            threshold: -20.0,
            ratio: 4.0,
            attack: 1.0,
            release: 50.0,
            ..TrackFx::default()
        };
        // A tone peaking at -6 dBFS: 14 dB over, so 10.5 dB taken off.
        let mut tone = sine(500.0, db_to_gain(-6.0), 1.0, 48_000);
        let mut state = FxState::default();
        state.process(&fx, &mut tone, 48_000.0);
        let peak = tone[48_000..].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let peak_db = gain_to_db(f64::from(peak));
        assert!((peak_db + 16.5).abs() < 1.5, "{peak_db}");
        assert!(state.reduction > 9.0);
        // Quiet sound passes.
        let mut quiet = sine(500.0, db_to_gain(-40.0), 0.5, 48_000);
        let before = rms(&quiet);
        FxState::default().process(&fx, &mut quiet, 48_000.0);
        assert!((rms(&quiet) / before - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_gate_closes_on_quiet_sound() {
        let fx = TrackFx {
            gate_on: true,
            gate_threshold: -40.0,
            gate_range: -60.0,
            gate_release: 5.0,
            ..TrackFx::default()
        };
        let mut hiss = sine(3000.0, db_to_gain(-55.0), 0.5, 48_000);
        let before = rms(&hiss[12_000..]);
        FxState::default().process(&fx, &mut hiss, 48_000.0);
        assert!(rms(&hiss[12_000..]) < before * 0.01);
        let mut voice = sine(300.0, db_to_gain(-12.0), 0.5, 48_000);
        let before = rms(&voice);
        FxState::default().process(&fx, &mut voice, 48_000.0);
        assert!((rms(&voice) / before - 1.0).abs() < 0.02);
    }

    #[test]
    fn loudness_reads_a_full_scale_tone_as_the_standard_says() {
        // BS.1770's check: a 997 Hz sine at -18 dBFS in both channels reads
        // about -18 LUFS... per channel -3.01: two channels sum to -18.
        let tone = sine(
            997.0,
            db_to_gain(-18.0) * std::f64::consts::SQRT_2 / 2.0_f64.sqrt(),
            5.0,
            48_000,
        );
        let lufs = loudness_of(&tone, 48_000);
        assert!((lufs - (-18.0 + 0.0)).abs() < 1.0, "{lufs}");
        // Gating ignores silence.
        let mut with_silence = tone.clone();
        with_silence.extend(vec![0.0; 48_000 * 2 * 5]);
        let gated = loudness_of(&with_silence, 48_000);
        assert!((gated - lufs).abs() < 0.5, "{gated} {lufs}");
        assert!(loudness_of(&[0.0; 1000], 48_000) <= -100.0);
    }

    #[test]
    fn normalizing_aims_at_the_target_within_the_headroom() {
        assert!((normalize_gain(-30.0, -16.0, 0.1) - 14.0).abs() < 1e-9);
        // A peak at -3 dBFS leaves only 2 dB.
        let held = normalize_gain(-30.0, -16.0, db_to_gain(-3.0) as f32);
        assert!((held - 2.0).abs() < 1e-6, "{held}");
        assert_eq!(normalize_gain(-120.0, -16.0, 0.5), 0.0);
    }

    #[test]
    fn pan_and_settings_round_trip() {
        assert_eq!(pan_gains(0.0), [1.0, 1.0]);
        assert_eq!(pan_gains(1.0), [0.0, 1.0]);
        assert_eq!(pan_gains(-0.5), [1.0, 0.5]);
        let mut fx = TrackFx {
            eq_on: true,
            comp_on: true,
            ratio: 6.0,
            ..TrackFx::default()
        };
        fx.bands[0].kind = BandKind::HighPass;
        fx.bands[3].gain = 3.5;
        assert_eq!(TrackFx::from_value(&fx.to_value()), fx);
    }
}
