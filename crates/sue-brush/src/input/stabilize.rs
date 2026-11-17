//! The stabilizer: a weighted average of whole records over a window of
//! device time.
//!
//! The level is Clip Studio's 0 to 100. A record of age `a` in a window
//! `W` weighs `(1 - a / W)^2`, so the newest weighs most and the weight
//! falls smoothly to nothing at the window's end. The mean age of that
//! window is `W / 4`, and `W` is 16 ms a level, so the brush trails the
//! pen by 4 ms a level, as Sue's earlier stabilizer did (0.4 s at 100).
//! With "adjust by speed" the window shortens as the pen speeds up, to a
//! tenth of it from 1,500 screen points a second on.
//!
//! The brush trails the pen by `10 + 1.5 * level` screen points at most.
//! Where it would trail further, the window is shortened in proportion
//! for that tick, so the brush is still an average and the pen's jitter
//! does not come back; only a jump the shorter window cannot follow
//! pulls it straight toward the pen. A shortened window grows back no
//! faster than half as fast as time passes, so no older record ever
//! gains weight over a newer one and the brush never slides back along
//! its path when the pen slows down. Before the stroke the pen is taken
//! to have rested at its first place, so the brush leaves it smoothly.
//!
//! Every field is averaged together, so the pressure the brush has at a
//! place is the pressure the pen had there. Because the window holds the
//! records themselves, the stabilizer can be drained on the lift along
//! the path the pen took, to where it lifted ([`Stabilizer::drain`]).

use std::collections::VecDeque;

use super::RATE;
use super::ingest::Record;
use super::resample::Tick;

/// Seconds of window a level.
const WINDOW_PER_LEVEL: f64 = 0.016;

/// Screen points a second where "adjust by speed" starts loosening.
const SLOW: f32 = 150.0;

/// Screen points a second where it has loosened all it does.
const FAST: f32 = 1500.0;

/// The narrowest the drain's centred average gets, in seconds each way
/// (never more than the stabilizer's own): the drain reaches the
/// smoothed path this long before the pen's last packet, still
/// averaging several packets, and runs straight on from there.
const END_HALF: f64 = 0.01;

/// How fast the window may grow back, in seconds of window a second.
const GROW_BACK: f64 = 0.5;

/// The window the pen's own place is smoothed over to measure the lag,
/// in seconds.
const PEN_WINDOW: f64 = 0.02;

/// The share of the farthest lag the window is shortened to keep under.
const SOFT: f32 = 0.9;

/// The share of the window "adjust by speed" takes away at most.
const LOOSEN: f32 = 0.9;

/// The stabilizer. See the module.
#[derive(Clone, Debug)]
pub(crate) struct Stabilizer {
    level: f32,
    by_speed: bool,
    /// Screen points per layer pixel.
    scale: f32,
    /// The longest window, in seconds.
    longest: f64,
    /// The records in the window, oldest first.
    held: VecDeque<Tick>,
    /// The last output.
    last: Option<Record>,
    /// The window the last tick was averaged over.
    used: f64,
}

impl Stabilizer {
    pub fn new(level: u32, by_speed: bool, scale: f32) -> Self {
        let level = level.min(100) as f32;
        Self {
            level,
            by_speed,
            scale,
            longest: f64::from(level) * WINDOW_PER_LEVEL,
            held: VecDeque::new(),
            last: None,
            used: 0.0,
        }
    }

    /// The window for a pen moving at `speed` layer pixels a second.
    fn window(&self, speed: f32) -> f64 {
        let mut level = self.level;
        if self.by_speed {
            let fast = ((speed * self.scale - SLOW) / (FAST - SLOW)).clamp(0.0, 1.0);
            level *= 1.0 - LOOSEN * fast;
        }
        f64::from(level) * WINDOW_PER_LEVEL
    }

    /// How far the brush may trail the pen, in layer pixels.
    fn most_behind(&self) -> f32 {
        (10.0 + 1.5 * self.level) / self.scale
    }

    /// Takes the next tick and answers where the brush is.
    pub fn push(&mut self, tick: Tick) -> Record {
        if self.held.is_empty() {
            // The pen rested at its first place before the stroke, so
            // the brush leaves it smoothly instead of copying the first
            // packets' jitter.
            let before = (self.longest * RATE).ceil() as u64;
            for step in (1..=before).rev() {
                self.held.push_back(Tick {
                    at: tick.at - step as f64 / RATE,
                    record: tick.record,
                });
            }
        }
        self.held.push_back(tick);
        while self
            .held
            .front()
            .is_some_and(|old| tick.at - old.at >= self.longest)
            && self.held.len() > 1
        {
            self.held.pop_front();
        }
        // The window may shrink at once but grows back no faster than
        // half as fast as time passes: then no older record ever gains
        // weight over a newer one, and the brush never slides back
        // along the path when the pen slows down.
        let grown = match self.held.iter().rev().nth(1) {
            Some(before) if self.used > 0.0 => self.used + GROW_BACK * (tick.at - before.at),
            _ => f64::INFINITY,
        };
        // The speed the brush has, not the pen's latest: smoother.
        let speed = self.last.map_or(tick.record.speed, |last| last.speed);
        let window = self.window(speed).min(grown);
        let most = self.most_behind();
        let mut out = self.average(&tick, window);
        // Too far behind: a window shorter in proportion, which trails
        // about that much less and is still an average, so the pen's
        // jitter does not come back. One step only, so the window, and
        // with it the brush, moves smoothly from tick to tick. The lag
        // is measured to the pen averaged over a few packets: measured
        // to the raw pen, its jitter would change the window and come
        // back through it.
        let pen = self.average(&tick, PEN_WINDOW.min(window));
        let behind = distance(out.pos, pen.pos);
        let mut window = window;
        if behind > SOFT * most {
            window *= f64::from(SOFT * most / behind);
            out = self.average(&tick, window);
        }
        self.used = window;
        let behind = distance(out.pos, tick.record.pos);
        if behind > most {
            // Still too far (a jump of the pen): pulled toward it, every
            // field with the place.
            out = tick.record.lerp(&out, most / behind);
        }
        self.last = Some(out);
        out
    }

    /// Where the brush would be after `tick` (the stream's provisional
    /// end), without taking it; with no tick, where it is.
    pub fn peek(&self, tick: Option<Tick>) -> Option<Record> {
        match tick {
            Some(tick) => Some(self.clone().push(tick)),
            None => self.last,
        }
    }

    /// The brush's way to the pen after the lift, whose record was the
    /// last tick taken. The brush follows the pen's own path, smoothed
    /// by a centred average that narrows toward the pen, from the place
    /// on it nearest the brush to just before the pen's last packet;
    /// how far the brush stood off that path (inside a curve, where an
    /// average cuts across) it lets go of as it goes, easing
    /// (`3u^2 - 2u^3`). So the line runs on from where it was without a
    /// kink, only ever forward along the pen's way (never back over
    /// what it drew, however far a quick pen had pulled it), and lands
    /// on the pen along the path the pen took. The pressure along the
    /// way is the pen's own at each place, not an average, so a pen
    /// that eased off before it lifted thins out as it did. From a few
    /// milliseconds before the pen's last packet the line runs straight
    /// on along the smoothed path as far as the pen got, so it ends
    /// where the pen lifted and that packet's jitter does not bend the
    /// end into a hook.
    pub fn drain(&mut self) -> Vec<Record> {
        let Some(newest) = self.held.back().copied() else {
            return Vec::new();
        };
        let path: Vec<Tick> = self.held.iter().copied().collect();
        let window = self.used;
        let half = window / 4.0;
        let smoothed = |at: f64| centred(&path, at, half.min(newest.at - at));
        // Where along the path, in time, the brush stands now: the place
        // of the path nearest it, from its own time on (a brush pulled
        // toward a quick pen stands further along than its time says).
        let mut from = self
            .last
            .map_or(newest.at, |r| r.time)
            .clamp(path[0].at, newest.at);
        if let Some(brush) = self.last {
            let mut best = distance(smoothed(from).pos, brush.pos);
            let mut at = from;
            while at < newest.at {
                at = (at + 0.25 / RATE).min(newest.at);
                let gap = distance(smoothed(at).pos, brush.pos);
                if gap < best {
                    best = gap;
                    from = at;
                }
            }
        }
        let to = (newest.at - END_HALF.min(half)).max(from);
        // Where the drain ends: the pen's path, smoothed, just before
        // its last packet.
        let target = smoothed(to);
        let start = smoothed(from);
        let offset = self.last.map_or([0.0; 2], |brush| {
            [brush.pos[0] - start.pos[0], brush.pos[1] - start.pos[1]]
        });
        let steps = ((to - from) * RATE).ceil().max(1.0) as u64;
        let mut out = Vec::with_capacity(steps as usize + 8);
        for step in 1..=steps {
            let u = step as f64 / steps as f64;
            let at = from + (to - from) * u;
            let mut record = smoothed(at);
            // The pen's own pressure there, not an average: a pen that
            // eased off before it lifted ends as thin as it did.
            record.pressure = centred(&path, at, 0.0).pressure;
            let keep = 1.0 - (u * u * (3.0 - 2.0 * u)) as f32;
            record.pos[0] += offset[0] * keep;
            record.pos[1] += offset[1] * keep;
            out.push(record);
        }
        // The last stretch, to the pen's last packet: straight on along
        // the smoothed path as far as the pen got, so the line reaches
        // where the pen lifted without bending to that packet's jitter.
        // Every other field narrows onto the pen's own last record.
        let span = newest.at - to;
        if span > 0.0 {
            let behind = centred(&path, (to - span).max(path[0].at), half.min(newest.at - to));
            let way = [target.pos[0] - behind.pos[0], target.pos[1] - behind.pos[1]];
            let length = (way[0] * way[0] + way[1] * way[1]).sqrt();
            let end = if length > 1e-6 {
                let way = [way[0] / length, way[1] / length];
                let reach = (newest.record.pos[0] - target.pos[0]) * way[0]
                    + (newest.record.pos[1] - target.pos[1]) * way[1];
                let reach = reach.max(0.0);
                [
                    target.pos[0] + way[0] * reach,
                    target.pos[1] + way[1] * reach,
                ]
            } else {
                target.pos
            };
            let count = (span * RATE).ceil().max(1.0) as u64;
            for k in 1..=count {
                let share = k as f64 / count as f64;
                let at = to + span * share;
                let mut record = centred(&path, at, half.min(newest.at - at));
                record.pressure = centred(&path, at, 0.0).pressure;
                let s = share as f32;
                record.pos = [
                    target.pos[0] + (end[0] - target.pos[0]) * s,
                    target.pos[1] + (end[1] - target.pos[1]) * s,
                ];
                out.push(record);
            }
        }
        self.last = out.last().copied();
        out
    }

    /// The weighted average over `window` of the held records, of which
    /// `newest` is the last.
    fn average(&self, newest: &Tick, window: f64) -> Record {
        let now = newest.at;
        if window <= 0.0 {
            return newest.record;
        }
        let mut sum = [0.0_f64; 8];
        let mut time = 0.0_f64;
        let mut total = 0.0_f64;
        for tick in self.held.iter().rev() {
            let age = now - tick.at;
            let share = (1.0 - age / window).max(0.0);
            let w = share * share;
            if w <= 0.0 {
                break;
            }
            let r = &tick.record;
            for (slot, value) in sum.iter_mut().zip([
                r.pos[0], r.pos[1], r.pressure, r.tilt[0], r.tilt[1], r.twist, r.speed, 0.0,
            ]) {
                *slot += w * f64::from(value);
            }
            time += w * r.time;
            total += w;
        }
        let at = |slot: usize| (sum[slot] / total) as f32;
        Record {
            pos: [at(0), at(1)],
            time: time / total,
            pressure: at(2).clamp(0.0, 1.0),
            tilt: [at(3), at(4)],
            twist: at(5),
            speed: at(6).max(0.0),
        }
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

/// The pen's record at time `at`, averaged over `half` seconds each way
/// with weights falling straight to nothing (a triangle), from `path`
/// (oldest first). With no width, the record at that time.
fn centred(path: &[Tick], at: f64, half: f64) -> Record {
    let after = path
        .partition_point(|tick| tick.at < at)
        .min(path.len() - 1);
    if half * RATE < 1.0 || path.len() == 1 {
        let next = path[after];
        if after == 0 || next.at <= at {
            return next.record;
        }
        let before = path[after - 1];
        let t = ((at - before.at) / (next.at - before.at)) as f32;
        return before.record.lerp(&next.record, t);
    }
    let mut sum = [0.0_f64; 7];
    let (mut time, mut total) = (0.0_f64, 0.0_f64);
    for tick in path {
        let w = 1.0 - (tick.at - at).abs() / half;
        if w <= 0.0 {
            continue;
        }
        let r = &tick.record;
        for (slot, value) in sum.iter_mut().zip([
            r.pos[0], r.pos[1], r.pressure, r.tilt[0], r.tilt[1], r.twist, r.speed,
        ]) {
            *slot += w * f64::from(value);
        }
        time += w * r.time;
        total += w;
    }
    if total <= 0.0 {
        return path[after].record;
    }
    let at = |slot: usize| (sum[slot] / total) as f32;
    Record {
        pos: [at(0), at(1)],
        time: time / total,
        pressure: at(2).clamp(0.0, 1.0),
        tilt: [at(3), at(4)],
        twist: at(5),
        speed: at(6).max(0.0),
    }
}
