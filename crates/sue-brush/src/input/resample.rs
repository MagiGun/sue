//! Fixed-rate resampling: records at every tick of [`RATE`] in device
//! time, laid straight between the device's own records, so the
//! stabilizer sees the same stream however often the device reports.

use super::RATE;
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
    /// The last record taken.
    last: Option<Record>,
}

impl Resampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The time of tick `n`.
    fn time(start: f64, n: u64) -> f64 {
        start + n as f64 / RATE
    }

    /// Takes `record` and adds every tick up to it to `out`.
    pub fn push(&mut self, record: Record, out: &mut Vec<Tick>) {
        let Some(start) = self.start else {
            self.start = Some(record.time);
            self.next = 1;
            self.last = Some(record);
            out.push(Tick {
                at: record.time,
                record,
            });
            return;
        };
        let last = self.last.unwrap_or(record);
        let span = record.time - last.time;
        loop {
            let at = Self::time(start, self.next);
            if at > record.time {
                break;
            }
            let t = if span > 0.0 {
                ((at - last.time) / span) as f32
            } else {
                1.0
            };
            let mut tick = last.lerp(&record, t.clamp(0.0, 1.0));
            tick.time = at;
            out.push(Tick { at, record: tick });
            self.next += 1;
        }
        self.last = Some(record);
    }

    /// The last record as a tick of its own when it falls between two
    /// ticks: the stream's provisional end.
    pub fn partial(&self) -> Option<Tick> {
        let (start, last) = (self.start?, self.last?);
        let before = Self::time(start, self.next - 1);
        (last.time > before).then_some(Tick {
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
        assert_eq!(a.len(), 41);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x.at - y.at).abs() < 1e-12);
            assert!((x.record.pos[0] - y.record.pos[0]).abs() < 1e-3);
        }
        assert!(slow.partial().is_none(), "the last record fell on a tick");
        slow.push(record(0.1013), &mut a);
        assert_eq!(slow.partial().map(|t| t.at), Some(0.1013));
    }
}
