//! The playback clock.
//!
//! While playing forward at normal speed with a sound device, the sound
//! is the master clock: the playhead is where the sound the device has
//! played is ([`AudioSink::played_frames`] counted from the start, less
//! nothing queued), so pictures follow the sound and never drift from it.
//! The pictures that are not decoded in time are dropped (the monitor
//! shows the last one it has), counted in [`Clock::dropped`]. Without a
//! device, and while shuttling (J and L pressed again run at 2, 4 and 8
//! times, backward with J), the clock is the wall clock.
//!
//! The clock knows the loop and the in and out range, and stops (or
//! loops) at the end.

use std::sync::Arc;
use std::time::Instant;

use sue_core::{EntityId, Time};
use sue_model::Sequence;
use sue_platform::AudioSink;

use crate::media::sound::Sound;
use crate::mix::{self, Meter};

/// The speeds shuttling steps through.
pub const SHUTTLE: [f64; 4] = [1.0, 2.0, 4.0, 8.0];

/// How far ahead of the playhead sound is queued, in seconds.
const AHEAD: f64 = 0.12;

/// The playback clock of one sequence.
#[derive(Debug)]
pub struct Clock {
    /// Where the playhead is.
    pub position: Time,
    /// Playing (else paused).
    pub playing: bool,
    /// Frames a second forward (negative: backward); 1 is normal.
    pub speed: f64,
    /// Play from the in mark to the out mark (or the whole) again and
    /// again.
    pub looping: bool,
    /// Where playing started, on the sequence.
    anchor: Time,
    /// When it started, on the wall clock.
    started: Instant,
    /// Sound frames queued since it started.
    pushed: u64,
    /// The device's played frames when it started.
    played_at_start: u64,
    /// Whether sound is the master this time.
    audio_master: bool,
    /// Frames shown while playing.
    pub shown: u64,
    /// Frames the monitor had to skip or repeat.
    pub dropped: u64,
    /// The meters.
    pub meter: Meter,
    /// The mix of the last block, kept to avoid allocating.
    block: Vec<f32>,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            position: Time::ZERO,
            playing: false,
            speed: 1.0,
            looping: false,
            anchor: Time::ZERO,
            started: Instant::now(),
            pushed: 0,
            played_at_start: 0,
            audio_master: false,
            shown: 0,
            dropped: 0,
            meter: Meter::default(),
            block: Vec::new(),
        }
    }
}

impl Clock {
    /// Starts playing at `speed` from where the playhead is, at `now`.
    pub fn play(&mut self, speed: f64, now: Instant, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.speed = speed;
        self.playing = true;
        self.anchor = self.position;
        self.started = now;
        self.pushed = 0;
        self.audio_master = (speed - 1.0).abs() < 1e-9 && sink.is_some();
        if let Some(sink) = sink {
            sink.clear();
            self.played_at_start = sink.played_frames();
            sink.set_playing(self.audio_master);
        }
    }

    /// Stops where it is.
    pub fn pause(&mut self, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.playing = false;
        self.speed = 1.0;
        if let Some(sink) = sink {
            sink.clear();
            sink.set_playing(false);
        }
    }

    /// Moves the playhead to `at` (playing goes on from there).
    pub fn seek(&mut self, at: Time, now: Instant, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.position = at.max(Time::ZERO);
        if self.playing {
            let speed = self.speed;
            self.play(speed, now, sink);
        }
    }

    /// The J and L keys: each press runs faster the same way (1, 2, 4, 8
    /// times), the other way starts at normal speed.
    pub fn shuttle(
        &mut self,
        forward: bool,
        now: Instant,
        sink: Option<&mut (dyn AudioSink + 'static)>,
    ) {
        let same_way = self.playing && (self.speed > 0.0) == forward;
        let magnitude = if same_way {
            let at = SHUTTLE
                .iter()
                .position(|speed| (*speed - self.speed.abs()).abs() < 1e-9)
                .unwrap_or(0);
            SHUTTLE[(at + 1).min(SHUTTLE.len() - 1)]
        } else {
            1.0
        };
        let speed = if forward { magnitude } else { -magnitude };
        self.play(speed, now, sink);
    }

    /// Moves the clock on to `now`: queues the next sound (the mix of
    /// `sequence`, its sources from `sound`) and works out where the
    /// playhead is. `range` is where playing stops or loops. Returns
    /// `true` while playing.
    pub fn advance(
        &mut self,
        sequence: &Sequence,
        range: (Time, Time),
        now: Instant,
        mut sink: Option<&mut (dyn AudioSink + 'static)>,
        sound: &dyn Fn(EntityId) -> Option<Arc<Sound>>,
    ) -> bool {
        if !self.playing {
            self.meter.decay(1.0 / 60.0);
            return false;
        }
        let (start, end) = range;
        if self.audio_master
            && let Some(sink) = sink.as_deref_mut()
        {
            let rate = sink.info().sample_rate.max(1);
            let channels = usize::from(sink.info().channels.max(1));
            let played = sink.played_frames().saturating_sub(self.played_at_start);
            // Never ahead of what was queued: a device that has not
            // started yet holds the clock at the start.
            let played = played.min(self.pushed);
            self.position = self.anchor + Time::from_seconds(played as f64 / f64::from(rate));
            // Queue up to AHEAD seconds past the playhead.
            let wanted = (AHEAD * f64::from(rate)) as u64;
            let queued = self.pushed - played;
            let room = sink.free_frames() as u64;
            let more = wanted.saturating_sub(queued).min(room);
            if more > 0 {
                let first =
                    (self.anchor.seconds() * f64::from(rate)).round() as i64 + self.pushed as i64;
                self.block.resize(more as usize * 2, 0.0);
                mix::mix(sequence, first, rate, &mut self.block, sound);
                let seconds = more as f32 / rate as f32;
                self.meter.feed(&self.block, seconds);
                let interleaved: Vec<f32> = if channels == 2 {
                    self.block.iter().map(|s| s.clamp(-1.0, 1.0)).collect()
                } else {
                    self.block
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .flat_map(|pair| {
                            let mono = ((pair[0] + pair[1]) * 0.5).clamp(-1.0, 1.0);
                            std::iter::repeat_n(mono, channels)
                        })
                        .collect()
                };
                let taken = sink.push(&interleaved) as u64;
                self.pushed += taken;
            }
        } else {
            let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
            self.position = self.anchor + Time::from_seconds(elapsed * self.speed);
            self.meter.decay(1.0 / 60.0);
        }
        let past_end = self.speed > 0.0 && self.position >= end;
        let before_start = self.speed < 0.0 && self.position <= start;
        if past_end || before_start {
            if self.looping && self.speed > 0.0 {
                self.position = start;
                let speed = self.speed;
                self.play(speed, now, sink);
            } else {
                self.position = if past_end { end } else { start };
                self.pause(sink);
                return false;
            }
        }
        true
    }

    /// Counts a frame shown while playing, and whether it was the one due.
    pub fn count(&mut self, due: bool) {
        self.shown += 1;
        if !due {
            self.dropped += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use sue_core::Rate;
    use sue_platform::NullAudio;

    #[test]
    fn sound_is_the_master_clock() {
        let sequence = crate::model::empty_sequence(64, 36, Rate::FPS_25, 1, 1);
        let mut sink = NullAudio::new(48_000, 2, 48_000);
        let mut clock = Clock::default();
        let now = Instant::now();
        clock.play(1.0, now, Some(&mut sink));
        let range = (Time::ZERO, Time::from_seconds(10.0));
        assert!(clock.advance(&sequence, range, now, Some(&mut sink), &|_| None));
        assert_eq!(clock.position, Time::ZERO, "nothing played yet");
        // The device plays half a second while the wall clock says two.
        for _ in 0..50 {
            sink.advance(480);
            clock.advance(
                &sequence,
                range,
                now + Duration::from_secs(2),
                Some(&mut sink),
                &|_| None,
            );
        }
        assert!(
            (clock.position.seconds() - 0.5).abs() < 1e-6,
            "{:?}",
            clock.position
        );
    }

    #[test]
    fn shuttle_runs_faster_and_turns_around() {
        let mut clock = Clock::default();
        let now = Instant::now();
        for want in [1.0, 2.0, 4.0, 8.0, 8.0] {
            clock.shuttle(true, now, None);
            assert_eq!(clock.speed, want);
        }
        clock.shuttle(false, now, None);
        assert_eq!(clock.speed, -1.0);
        clock.shuttle(false, now, None);
        assert_eq!(clock.speed, -2.0);
    }

    #[test]
    fn the_wall_clock_runs_without_a_device_and_stops_or_loops_at_the_end() {
        let sequence = crate::model::empty_sequence(64, 36, Rate::FPS_25, 1, 1);
        let mut clock = Clock::default();
        let now = Instant::now();
        clock.position = Time::from_seconds(1.0);
        clock.play(2.0, now, None);
        let range = (Time::from_seconds(1.0), Time::from_seconds(3.0));
        assert!(clock.advance(
            &sequence,
            range,
            now + Duration::from_millis(500),
            None,
            &|_| None
        ));
        assert!((clock.position.seconds() - 2.0).abs() < 1e-6);
        assert!(!clock.advance(
            &sequence,
            range,
            now + Duration::from_secs(2),
            None,
            &|_| None
        ));
        assert_eq!(clock.position, Time::from_seconds(3.0));
        assert!(!clock.playing);
        clock.position = Time::from_seconds(1.0);
        clock.looping = true;
        clock.play(1.0, now, None);
        assert!(clock.advance(
            &sequence,
            range,
            now + Duration::from_secs(5),
            None,
            &|_| None
        ));
        assert_eq!(
            clock.position,
            Time::from_seconds(1.0),
            "back to the in mark"
        );
    }
}
