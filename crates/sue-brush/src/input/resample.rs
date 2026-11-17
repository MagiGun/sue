//! Fixed-rate resampling: records at every tick of [`RATE`] in device
//! time, laid along a centripetal Catmull-Rom curve through the device's
//! own records, so the stabilizer sees the same stream however often the
//! device reports.
//!
//! A mouse, or a pen moved fast, reports places far apart (40 pixels and
//! more). Laid straight between them, the ticks would make a polyline
//! with a turn at every packet, which no stabilizer level rounds off;
//! along the curve through them the line is the arc the hand drew. The
//! piece between two records needs the record after them, so it is laid
//! when that record comes (one packet later); [`Resampler::provisional`]
//! lays the newest piece with the way it was going for the tail, and
//! [`Resampler::flush`] does it for good on the lift. Every value besides
//! the place goes straight between the two records by time.

use super::RATE;
use super::curve::Spline;
use super::ingest::Record;

/// One record of the resampled stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tick {
    /// The tick's time: the clock the stabilizer measures ages on.
    pub at: f64,
    /// The record there.
    pub record: Record,
}

/// The resampler. See the module.
#[derive(Clone, Debug, Default)]
pub(crate) struct Resampler {
    /// The first record's time: tick `n` is at `start + n / RATE`.
    start: Option<f64>,
    /// The next tick's number.
    next: u64,
    /// The newest records, oldest first (three at most): the piece
    /// between the last two waits for the record after it.
    knots: Vec<Record>,
    /// The piece up to the newest record has been laid (the lift).
    flushed: bool,
}

impl Resampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The time of tick `n`.
    fn time(start: f64, n: u64) -> f64 {
        start + n as f64 / RATE
    }

    /// Takes `record` and adds every tick the curve is known up to to
    /// `out`.
    pub fn push(&mut self, record: Record, out: &mut Vec<Tick>) {
        if self.start.is_none() {
            self.start = Some(record.time);
            self.next = 1;
            self.knots.push(record);
            out.push(Tick {
                at: record.time,
                record,
            });
            return;
        }
        self.flushed = false;
        self.knots.push(record);
        let n = self.knots.len();
        if n >= 3 {
            let before = (n >= 4).then(|| self.knots[n - 4]);
            let (from, to, after) = (self.knots[n - 3], self.knots[n - 2], self.knots[n - 1]);
            self.lay(before, from, to, Some(after), out);
        }
        if self.knots.len() > 3 {
            self.knots.remove(0);
        }
    }

    /// Lays the piece up to the newest record, with the way the curve
    /// was going there (the lift: no record comes after it).
    pub fn flush(&mut self, out: &mut Vec<Tick>) {
        if self.flushed {
            return;
        }
        self.flushed = true;
        let n = self.knots.len();
        if n >= 2 {
            let before = (n >= 3).then(|| self.knots[n - 3]);
            let (from, to) = (self.knots[n - 2], self.knots[n - 1]);
            self.lay(before, from, to, None, out);
        }
    }

    /// The ticks of the curve from where it is laid to the newest
    /// record, as it stands now, and that record as a tick of its own
    /// when it falls between two ticks: the stream's provisional end.
    pub fn provisional(&self) -> (Vec<Tick>, Option<Tick>) {
        let mut ahead = self.clone();
        let mut ticks = Vec::new();
        ahead.flush(&mut ticks);
        (ticks, ahead.partial())
    }

    /// Every tick from the next one up to `to`'s time, on the piece from
    /// `from` to `to` of the curve whose neighbours are `before` and
    /// `after` (mirrored where missing).
    fn lay(
        &mut self,
        before: Option<Record>,
        from: Record,
        to: Record,
        after: Option<Record>,
        out: &mut Vec<Tick>,
    ) {
        let Some(start) = self.start else {
            return;
        };
        let (p1, p2) = (from.pos, to.pos);
        let p0 = before.map_or([2.0 * p1[0] - p2[0], 2.0 * p1[1] - p2[1]], |r| r.pos);
        let p3 = after.map_or([2.0 * p2[0] - p1[0], 2.0 * p2[1] - p1[1]], |r| r.pos);
        let spline = Spline::new([p0, p1, p2, p3]);
        let span = to.time - from.time;
        loop {
            let at = Self::time(start, self.next);
            if at > to.time {
                break;
            }
            let t = if span > 0.0 {
                (((at - from.time) / span) as f32).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let mut tick = from.lerp(&to, t);
            tick.pos = spline.at(t);
            tick.time = at;
            out.push(Tick { at, record: tick });
            self.next += 1;
        }
    }

    /// The newest record as a tick of its own when it falls between two
    /// ticks and the curve is laid up to it: the stream's end.
    pub fn partial(&self) -> Option<Tick> {
        let (start, last) = (self.start?, *self.knots.last()?);
        let before = Self::time(start, self.next - 1);
        (last.time > before && (self.flushed || self.knots.len() == 1)).then_some(Tick {
            at: last.time,
            record: last,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(time: f64) -> Record {
        Record {
            pos: [time as f32 * 100.0, 0.0],
            time,
            pressure: 0.5,
            tilt: [0.0; 2],
            twist: 0.0,
            speed: 100.0,
        }
    }

    #[test]
    fn ticks_come_at_the_fixed_rate_whatever_the_device_rate() {
        let mut slow = Resampler::new();
        let mut fast = Resampler::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for i in 0..=10 {
            slow.push(record(i as f64 / 100.0), &mut a);
        }
        for i in 0..=30 {
            fast.push(record(i as f64 / 300.0), &mut b);
        }
        slow.flush(&mut a);
        fast.flush(&mut b);
        assert_eq!(a.len(), 41);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x.at - y.at).abs() < 1e-12);
            assert!((x.record.pos[0] - y.record.pos[0]).abs() < 1e-3);
        }
        assert!(slow.partial().is_none(), "the last record fell on a tick");
        slow.push(record(0.1013), &mut a);
        assert!(slow.partial().is_none(), "laid only on the lift");
        assert_eq!(slow.provisional().1.map(|t| t.at), Some(0.1013));
        slow.flush(&mut a);
        assert_eq!(slow.partial().map(|t| t.at), Some(0.1013));
    }

    #[test]
    fn far_apart_records_are_joined_by_an_arc_not_a_corner() {
        // Four records on a circle of radius 100, 45 degrees apart: the
        // ticks between the middle two stay on the circle's side of the
        // chord, as an arc does.
        let on = |angle: f32, time: f64| Record {
            pos: [100.0 * angle.cos(), 100.0 * angle.sin()],
            ..record(time)
        };
        let quarter = std::f32::consts::FRAC_PI_4;
        let mut resampler = Resampler::new();
        let mut ticks = Vec::new();
        for i in 0..4 {
            resampler.push(on(quarter * i as f32, 0.1 * f64::from(i)), &mut ticks);
        }
        let between: Vec<&Tick> = ticks
            .iter()
            .filter(|t| t.at > 0.1 + 1e-9 && t.at < 0.2 - 1e-9)
            .collect();
        assert!(!between.is_empty());
        for tick in between {
            let radius = tick.record.pos[0].hypot(tick.record.pos[1]);
            assert!(
                (radius - 100.0).abs() < 1.0,
                "on the arc, not the chord: {radius}"
            );
        }
    }
}
