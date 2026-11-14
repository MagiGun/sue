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
//! the path the pen took ([`Stabilizer::drain`]).

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
/// (never more than the stabilizer's own): the drain ends this long
/// before the pen's last packet, still averaging several packets.
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
    /// last tick taken. Two ways are blended. One feeds the drain's end
    /// again and again until the window holds nothing else, as if the
    /// pen stood there: its first step moves the brush almost as the
    /// tick before did, so the line runs on without a kink, but it cuts
    /// across a curve. The other follows the pen's own path with a
    /// centred average that narrows toward the pen. The blend
    /// starts all on the first and ends all on the second, easing over
    /// (`3u^2 - 2u^3`), so the line leaves smoothly and lands on the pen
    /// along the path the pen took. It ends a few milliseconds of travel
    /// short of the pen's last packet, still smoothed, so that packet's
    /// jitter does not bend the end into a hook. The window stays the
    /// one the brush trailed with at the lift (a longer one would pull
    /// it back).
    pub fn drain(&mut self) -> Vec<Record> {
        let Some(newest) = self.held.back().copied() else {
            return Vec::new();
        };
        let path: Vec<Tick> = self.held.iter().copied().collect();
        let window = self.used;
        let steps = (window * RATE).ceil() as u64;
        // Where along the path, in time, the brush stands now.
        let from = self
            .last
            .map_or(newest.at, |r| r.time)
            .clamp(path[0].at, newest.at);
        let half = window / 4.0;
        let to = (newest.at - END_HALF.min(half)).max(from);
        // Where the drain ends: the pen's path, smoothed, just before
        // its last packet.
        let target = centred(&path, to, half.min(newest.at - to));
        let mut out = Vec::with_capacity(steps as usize + 1);
        for step in 1..=steps.max(1) {
            let tick = Tick {
                at: newest.at + step as f64 / RATE,
                record: target,
            };
            self.held.push_back(tick);
            while self
                .held
                .front()
                .is_some_and(|old| tick.at - old.at >= window)
            {
                self.held.pop_front();
            }
            let standing = self.average(&tick, window);
            let u = step as f64 / steps.max(1) as f64;
            // The centred window never reaches past the pen's last
            // packet, so it never turns one-sided.
            // Along the path as far, in time, as the standing brush.
            let at = standing.time.clamp(from, to);
            let following = centred(&path, at, half.min(newest.at - at));
            let ease = (u * u * (3.0 - 2.0 * u)) as f32;
            out.push(standing.lerp(&following, ease));
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
