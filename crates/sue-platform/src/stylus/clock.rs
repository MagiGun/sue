//! A pen device's own clock, read on Sue's.
//!
//! A tablet driver stamps every packet with its own time in milliseconds
//! (WinTab's `pkTime`, an X server's event time, a Wayland frame's time,
//! a browser's `timeStamp`). Those times say exactly how far apart two
//! packets were, which the time Sue happens to read them does not: a
//! frame's packets are all read at once. [`DeviceClock`] keeps the
//! device's steps and places them on the monotonic clock everything else
//! in the pen's stream uses ([`crate::time::since_start`], in seconds).

/// How far a device's clock may drift from Sue's before it is set again,
/// in seconds: more than a driver's delay, far less than a clock that
/// was reset or wrapped.
const DRIFT: f64 = 0.25;

/// A device's millisecond clock on Sue's monotonic one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DeviceClock {
    /// The last device time seen, as the device counts (it may wrap).
    last: Option<u32>,
    /// That time unwrapped, in milliseconds since the first one seen.
    run: f64,
    /// What to add to `run` (in seconds) for Sue's clock; `None` until the
    /// first [`DeviceClock::sync`].
    offset: Option<f64>,
}

impl DeviceClock {
    /// The device's time `ms` in seconds since the first time seen,
    /// unwrapped: a counter that wraps at 32 bits goes on counting, and a
    /// time a little before the last one (packets out of order) is a
    /// little earlier, not 49 days later.
    fn run(&mut self, ms: u32) -> f64 {
        if let Some(last) = self.last {
            // The step as a signed number: forward or a little back.
            let step = ms.wrapping_sub(last) as i32;
            self.run += f64::from(step);
        }
        self.last = Some(ms);
        self.run / 1000.0
    }

    /// Lines the clocks up for a batch of packets read at `now` (seconds
    /// on Sue's clock) whose newest has the device time `newest`: that
    /// packet happened about now. The offset is kept from batch to batch,
    /// so the steps between packets stay the device's own, and set again
    /// only when the two clocks have come apart (a driver restarted).
    pub(crate) fn sync(&mut self, newest: u32, now: f64) {
        if self.last.is_none() {
            // The first time seen is where the count begins; the packets
            // read with it, a little older, count back from it.
            self.last = Some(newest);
            self.run = 0.0;
        }
        let mut probe = *self;
        let at = probe.run(newest);
        let fits = self
            .offset
            .is_some_and(|offset| (at + offset - now).abs() <= DRIFT);
        if !fits {
            self.offset = Some(now - at);
        }
    }

    /// The device time `ms` in seconds on Sue's clock. Before any
    /// [`DeviceClock::sync`] the first time seen counts as 0.
    pub(crate) fn seconds(&mut self, ms: u32) -> f64 {
        self.run(ms) + self.offset.unwrap_or(0.0)
    }

    /// As [`DeviceClock::sync`] and [`DeviceClock::seconds`] for a clock
    /// that counts in fractions of a millisecond (a browser's): the whole
    /// milliseconds go through the wrapping counter, the rest is added.
    #[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
    pub(crate) fn seconds_of(&mut self, ms: f64, now: f64) -> f64 {
        if !ms.is_finite() {
            return now;
        }
        let whole = ms.floor();
        let counter = whole.rem_euclid(4_294_967_296.0) as u32;
        self.sync(counter, now);
        self.seconds(counter) + (ms - whole) / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_keep_the_devices_steps_on_sues_clock() {
        let mut clock = DeviceClock::default();
        // Three packets 5 ms apart, read together at 10 s.
        clock.sync(1_010, 10.0);
        let times = [1_000, 1_005, 1_010].map(|ms| clock.seconds(ms));
        assert!((times[2] - 10.0).abs() < 1e-9, "the newest is now");
        assert!((times[1] - 9.995).abs() < 1e-9);
        assert!((times[0] - 9.990).abs() < 1e-9);
        // The next batch, read 16.7 ms later though the device counted
        // 15: the steps stay the device's.
        clock.sync(1_025, 10.0167);
        let next = [1_015, 1_020, 1_025].map(|ms| clock.seconds(ms));
        assert!((next[0] - 9.995 - 0.010).abs() < 1e-9);
        assert!((next[2] - 10.015).abs() < 1e-9);
    }

    #[test]
    fn a_clock_that_jumps_is_lined_up_again() {
        let mut clock = DeviceClock::default();
        clock.sync(500, 3.0);
        assert!((clock.seconds(500) - 3.0).abs() < 1e-9);
        // The driver restarted: its clock begins again near 0.
        clock.sync(20, 8.0);
        assert!((clock.seconds(20) - 8.0).abs() < 1e-9);
        // The pen was away for a minute; both clocks ran on alike.
        clock.sync(60_020, 68.0);
        assert!((clock.seconds(60_020) - 68.0).abs() < 1e-9);
    }

    #[test]
    fn a_counter_that_wraps_goes_on_counting() {
        let mut clock = DeviceClock::default();
        clock.sync(u32::MAX - 4, 1.0);
        let before = clock.seconds(u32::MAX - 4);
        clock.sync(5, 1.010);
        let after = clock.seconds(5);
        assert!((after - before - 0.010).abs() < 1e-9, "10 ms later");
        // A packet out of order is a little earlier.
        let late = clock.seconds(3);
        assert!((after - late - 0.002).abs() < 1e-9);
    }

    #[test]
    fn a_browsers_clock_keeps_its_fractions() {
        let mut clock = DeviceClock::default();
        let first = clock.seconds_of(1_000.25, 2.0);
        let second = clock.seconds_of(1_008.75, 2.008);
        assert!((second - first - 0.0085).abs() < 1e-9);
        assert_eq!(clock.seconds_of(f64::NAN, 2.5), 2.5);
    }
}
