//! Ingest and pressure conditioning: device samples become records in
//! layer space with a pressure that is never made up.

use std::collections::VecDeque;

use sue_model::Response;

use super::{Device, Kind, Sample};

/// The smallest step of time between two records, in seconds. A sample
/// whose time does not go forward (several events stamped alike) is put
/// this far after the one before.
const MIN_STEP: f64 = 0.001;

/// The longest gap between two records, in seconds. A longer one (the
/// device slept, the clock jumped) is shortened to this, so nothing
/// downstream works through it tick by tick.
const MAX_GAP: f64 = 0.25;

/// How fast pressure may rise, in full presses a second: a full press
/// takes at least 10 ms. A real landing is slower than that over more
/// than one packet; a spike is not.
const RISE: f32 = 100.0;

/// The time the first packet is taken to cover, for the rise limit.
const FIRST_STEP: f32 = 0.005;

/// The weights of the smoothing against the tablet's pressure steps,
/// newest first.
const SMOOTH: [f32; 3] = [0.5, 0.3, 0.2];

/// The span of device time the pen's speed is measured over, in seconds.
const SPEED_SPAN: f64 = 0.03;

/// One moment of the pen in layer space, as every later stage carries
/// it: all fields travel and are smoothed together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Record {
    /// Where, in layer pixels.
    pub pos: [f32; 2],
    /// The device time it stands for.
    pub time: f64,
    /// The conditioned pressure, 0 to 1.
    pub pressure: f32,
    /// The lean.
    pub tilt: [f32; 2],
    /// The twist in degrees, unwrapped: it runs on past 360 and below 0
    /// so that averages go the short way round.
    pub twist: f32,
    /// The pen's speed, layer pixels a second.
    pub speed: f32,
}

impl Record {
    /// The record `t` of the way from `self` to `other` (0 is `self`).
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            pos: [
                mix(self.pos[0], other.pos[0]),
                mix(self.pos[1], other.pos[1]),
            ],
            time: self.time + (other.time - self.time) * f64::from(t),
            pressure: mix(self.pressure, other.pressure),
            tilt: [
                mix(self.tilt[0], other.tilt[0]),
                mix(self.tilt[1], other.tilt[1]),
            ],
            twist: mix(self.twist, other.twist),
            speed: mix(self.speed, other.speed),
        }
    }
}

/// What the ingest stage hands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    /// A record while the pen touches.
    Contact(Record),
    /// The pen lifted: where and when, and the pressure the line ends
    /// at (0 for a pen, its constant for a constant device).
    Lift(Record),
}

/// A sample in layer space, before its pressure is known.
#[derive(Clone, Copy, Debug)]
struct Raw {
    pos: [f32; 2],
    time: f64,
    pressure: Option<f32>,
    tilt: [f32; 2],
    twist: f32,
}

/// The ingest and pressure stages. See the module.
#[derive(Clone, Debug)]
pub(crate) struct Ingest {
    device: Device,
    curve: Response,
    to_layer: [[f32; 3]; 2],
    started: bool,
    ended: bool,
    /// The last sample taken, in layer space with repaired time.
    last: Option<Raw>,
    /// The last sample as it came.
    previous: Option<Sample>,
    /// Device time taken out of long gaps so far.
    offset: f64,
    /// Samples of a pen held back until it says its first pressure.
    held: Vec<Raw>,
    /// The pen has said a pressure in this stroke.
    pressed: bool,
    /// The last pressure the pen said, after the curve.
    said: f32,
    /// The pressures after the curve, newest last (for the median).
    curved: VecDeque<f32>,
    /// The pressures after the rise limit, newest last.
    limited: VecDeque<f32>,
    /// The time of the last record the rise limit saw.
    limit_time: Option<f64>,
    /// Recent places and their times, for the speed.
    path: VecDeque<(f64, [f32; 2])>,
}

impl Ingest {
    pub fn new(device: Device, curve: Response, to_layer: [[f32; 3]; 2]) -> Self {
        Self {
            device,
            curve,
            to_layer,
            started: false,
            ended: false,
            last: None,
            previous: None,
            offset: 0.0,
            held: Vec::new(),
            pressed: false,
            said: 0.0,
            curved: VecDeque::new(),
            limited: VecDeque::new(),
            limit_time: None,
            path: VecDeque::new(),
        }
    }

    /// Takes one device sample and adds what it gives to `out`.
    pub fn push(&mut self, sample: Sample, out: &mut Vec<Event>) {
        if self.ended {
            return;
        }
        match sample.kind {
            Kind::Down if !self.started => self.started = true,
            Kind::Down | Kind::Move | Kind::Up if self.started => {}
            // Hovering before the stroke starts, or a lift without one.
            _ => return,
        }
        // The same packet twice (a source that reports it once as a
        // pointer event and once as a pen packet) is taken once.
        if self.previous == Some(sample) && sample.kind == Kind::Move {
            return;
        }
        self.previous = Some(sample);
        let Some(raw) = self.raw(&sample) else {
            return;
        };
        if sample.kind == Kind::Up {
            self.ended = true;
            self.lift(raw, out);
            return;
        }
        self.last = Some(raw);
        match self.device {
            Device::Constant(pressure) => {
                let record = self.record(raw, pressure.clamp(0.0, 1.0));
                out.push(Event::Contact(record));
            }
            Device::Pen => {
                if let Some(said) = raw.pressure {
                    self.said = self.curve.eval(said);
                    if !self.pressed {
                        // The packets before the first pressure take it.
                        self.pressed = true;
                        for early in std::mem::take(&mut self.held) {
                            let pressure = self.condition(early.time, self.said);
                            out.push(Event::Contact(self.record(early, pressure)));
                        }
                    }
                } else if !self.pressed {
                    self.held.push(raw);
                    return;
                }
                let pressure = self.condition(raw.time, self.said);
                out.push(Event::Contact(self.record(raw, pressure)));
            }
        }
    }

    /// The lift at `raw`.
    fn lift(&mut self, raw: Raw, out: &mut Vec<Event>) {
        let end = match self.device {
            Device::Constant(pressure) => pressure.clamp(0.0, 1.0),
            Device::Pen if self.pressed => 0.0,
            Device::Pen => {
                // A pen that never said a pressure in the whole stroke
                // is a device without one: it presses the same all
                // along, so it cannot end bigger than it was.
                for early in std::mem::take(&mut self.held) {
                    let record = self.record(early, 1.0);
                    out.push(Event::Contact(record));
                }
                1.0
            }
        };
        let raw = Raw {
            pressure: None,
            tilt: self.last.map_or(raw.tilt, |last| last.tilt),
            twist: self.last.map_or(raw.twist, |last| last.twist),
            ..raw
        };
        let record = self.record(raw, end);
        out.push(Event::Lift(record));
    }

    /// `sample` mapped to layer space, with its time repaired and its
    /// lean and twist filled from the last sample. `None` for a sample
    /// whose place is not a number.
    fn raw(&mut self, sample: &Sample) -> Option<Raw> {
        let [x, y] = sample.pos;
        let m = &self.to_layer;
        let pos = [
            m[0][0] * x + m[0][1] * y + m[0][2],
            m[1][0] * x + m[1][1] * y + m[1][2],
        ];
        if !pos[0].is_finite() || !pos[1].is_finite() {
            return None;
        }
        let mut time = sample.time - self.offset;
        if let Some(last) = self.last {
            if !time.is_finite() || time <= last.time {
                time = last.time + MIN_STEP;
            } else if time - last.time > MAX_GAP {
                self.offset += time - last.time - MAX_GAP;
                time = last.time + MAX_GAP;
            }
        } else if !time.is_finite() {
            time = 0.0;
        }
        let pressure = sample
            .pressure
            .filter(|pressure| pressure.is_finite())
            .map(|pressure| pressure.clamp(0.0, 1.0));
        let tilt = sample
            .tilt
            .filter(|tilt| tilt[0].is_finite() && tilt[1].is_finite())
            .map(|tilt| [tilt[0].clamp(-1.0, 1.0), tilt[1].clamp(-1.0, 1.0)])
            .or_else(|| self.last.map(|last| last.tilt))
            .unwrap_or([0.0; 2]);
        let twist = match (sample.twist.filter(|twist| twist.is_finite()), self.last) {
            (Some(twist), Some(last)) => {
                // The short way round from the last twist.
                let turn = (twist - last.twist).rem_euclid(360.0);
                last.twist + if turn > 180.0 { turn - 360.0 } else { turn }
            }
            (Some(twist), None) => twist.rem_euclid(360.0),
            (None, Some(last)) => last.twist,
            (None, None) => 0.0,
        };
        Some(Raw {
            pos,
            time,
            pressure,
            tilt,
            twist,
        })
    }

    /// The pen's pressure `said` (after the curve) at `time`, through
    /// the median, the rise limit and the smoothing.
    fn condition(&mut self, time: f64, said: f32) -> f32 {
        push_kept(&mut self.curved, said, 3);
        let median = match self.curved.len() {
            1 => said,
            // Of two, the lower: a spike in the first packets is not
            // believed yet.
            2 => self.curved[0].min(self.curved[1]),
            _ => {
                let [a, b, c] = [self.curved[0], self.curved[1], self.curved[2]];
                a.max(b).min(a.min(b).max(c))
            }
        };
        let step = match self.limit_time {
            Some(before) => (time - before) as f32,
            None => FIRST_STEP,
        };
        self.limit_time = Some(time);
        let before = self.limited.back().copied().unwrap_or(0.0);
        let limited = median.min(before + RISE * step);
        push_kept(&mut self.limited, limited, 3);
        let (mut sum, mut weight) = (0.0, 0.0);
        for (value, w) in self.limited.iter().rev().zip(SMOOTH) {
            sum += value * w;
            weight += w;
        }
        (sum / weight).clamp(0.0, 1.0)
    }

    /// The record of `raw` pressing `pressure`, with the pen's speed.
    fn record(&mut self, raw: Raw, pressure: f32) -> Record {
        let speed = self.speed(raw.time, raw.pos);
        Record {
            pos: raw.pos,
            time: raw.time,
            pressure,
            tilt: raw.tilt,
            twist: raw.twist,
            speed,
        }
    }

    /// The pen's speed on reaching `pos` at `time`: how far it got over
    /// the last [`SPEED_SPAN`] of device time, over that time. The
    /// straight distance, not the path: a path adds up the jitter of
    /// every packet.
    fn speed(&mut self, time: f64, pos: [f32; 2]) -> f32 {
        self.path.push_back((time, pos));
        // Keep one entry at or before the span's start.
        while self.path.len() > 2 && self.path[1].0 <= time - SPEED_SPAN {
            self.path.pop_front();
        }
        let (since, from) = self.path[0];
        let span = time - since;
        if span <= 0.0 {
            0.0
        } else {
            let (dx, dy) = (pos[0] - from[0], pos[1] - from[1]);
            ((dx * dx + dy * dy).sqrt() / span as f32).max(0.0)
        }
    }
}

/// Pushes `value` and keeps the newest `keep`.
fn push_kept(values: &mut VecDeque<f32>, value: f32, keep: usize) {
    values.push_back(value);
    while values.len() > keep {
        values.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ingest(device: Device) -> Ingest {
        Ingest::new(device, Response::LINEAR, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
    }

    fn take(ingest: &mut Ingest, samples: &[Sample]) -> Vec<Event> {
        let mut out = Vec::new();
        for sample in samples {
            ingest.push(*sample, &mut out);
        }
        out
    }

    fn contacts(events: &[Event]) -> Vec<Record> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Contact(record) => Some(*record),
                Event::Lift(_) => None,
            })
            .collect()
    }

    #[test]
    fn time_always_goes_forward_and_long_gaps_shrink() {
        let mut stage = ingest(Device::MOUSE);
        let events = take(
            &mut stage,
            &[
                Sample::new(Kind::Down, [0.0, 0.0], 5.0),
                Sample::new(Kind::Move, [1.0, 0.0], 5.0),
                Sample::new(Kind::Move, [2.0, 0.0], 4.0),
                Sample::new(Kind::Move, [3.0, 0.0], 60.0),
                Sample::new(Kind::Move, [4.0, 0.0], 60.01),
            ],
        );
        let times: Vec<f64> = contacts(&events).iter().map(|r| r.time).collect();
        assert!(times.windows(2).all(|w| w[1] > w[0]), "{times:?}");
        assert!((times[3] - times[2] - MAX_GAP).abs() < 1e-9);
        assert!((times[4] - times[3] - 0.01).abs() < 1e-6);
    }

    #[test]
    fn hovering_before_the_stroke_and_samples_after_it_are_ignored() {
        let mut stage = ingest(Device::Pen);
        let events = take(
            &mut stage,
            &[
                Sample::new(Kind::Move, [9.0, 9.0], 0.0),
                Sample::new(Kind::Up, [9.0, 9.0], 0.001),
                Sample::new(Kind::Down, [0.0, 0.0], 0.01).pressing(0.4),
                Sample::new(Kind::Up, [1.0, 0.0], 0.02),
                Sample::new(Kind::Down, [5.0, 5.0], 0.03).pressing(0.9),
            ],
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], Event::Contact(r) if r.pos == [0.0, 0.0]));
        assert!(matches!(events[1], Event::Lift(r) if r.pos == [1.0, 0.0] && r.pressure == 0.0));
    }

    #[test]
    fn a_single_spike_is_not_believed() {
        let mut stage = ingest(Device::Pen);
        let mut samples = vec![Sample::new(Kind::Down, [0.0, 0.0], 0.0).pressing(0.3)];
        for i in 1..12 {
            let pressure = if i == 6 { 1.0 } else { 0.3 };
            samples.push(
                Sample::new(Kind::Move, [i as f32, 0.0], i as f64 / 200.0).pressing(pressure),
            );
        }
        let records = contacts(&take(&mut stage, &samples));
        assert!(
            records.iter().all(|r| r.pressure <= 0.3 + 1e-6),
            "{records:?}"
        );
    }

    #[test]
    fn twist_goes_the_short_way_round_and_a_place_maps_to_the_layer() {
        let mut stage = Ingest::new(
            Device::MOUSE,
            Response::LINEAR,
            [[2.0, 0.0, 10.0], [0.0, 2.0, -4.0]],
        );
        let mut samples = Vec::new();
        for (i, twist) in [350.0, 355.0, 2.0, 8.0].into_iter().enumerate() {
            let kind = if i == 0 { Kind::Down } else { Kind::Move };
            let mut sample = Sample::new(kind, [i as f32, 1.0], i as f64 * 0.005);
            sample.twist = Some(twist);
            samples.push(sample);
        }
        let records = contacts(&take(&mut stage, &samples));
        let twists: Vec<f32> = records.iter().map(|r| r.twist).collect();
        assert_eq!(twists, [350.0, 355.0, 362.0, 368.0]);
        assert_eq!(records[1].pos, [12.0, -2.0]);
    }

    #[test]
    fn speed_is_measured_on_device_time() {
        let mut stage = ingest(Device::MOUSE);
        let samples: Vec<Sample> = (0..20)
            .map(|i| {
                let kind = if i == 0 { Kind::Down } else { Kind::Move };
                Sample::new(kind, [i as f32 * 2.0, 0.0], i as f64 * 0.005)
            })
            .collect();
        let records = contacts(&take(&mut stage, &samples));
        let last = records.last().unwrap();
        assert!((last.speed - 400.0).abs() < 1.0, "{}", last.speed);
    }
}
