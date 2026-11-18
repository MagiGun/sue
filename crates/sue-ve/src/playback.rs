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
//! A device that takes nothing for [`STALL`] (it is gone, asleep or
//! refuses the stream) never holds the playhead: the clock goes on by the
//! wall clock, without sound, until playing starts again. A device that
//! comes after playing started (the first play opens it) or starts its
//! count again (a new device) takes the sound from where the playhead is.
//!
//! The clock knows the loop and the in and out range, and stops (or
//! loops) at the end.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sue_core::{EntityId, Time};
use sue_model::Sequence;
use sue_platform::AudioSink;

use crate::media::sound::Sound;
use crate::mix::{self, Meter};

/// The speeds shuttling steps through.
pub const SHUTTLE: [f64; 4] = [1.0, 2.0, 4.0, 8.0];

/// How far ahead of the playhead sound is queued, in seconds.
const AHEAD: f64 = 0.12;

/// How long a device may take nothing that is queued for it before the
/// wall clock takes over.
pub const STALL: Duration = Duration::from_millis(150);

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
    /// The device took nothing for [`STALL`]: the wall clock runs.
    stalled: bool,
    /// The device's played frames when the clock last looked.
    seen_played: u64,
    /// When the device last took something (or had nothing to take).
    progressed_at: Instant,
    /// Frames shown while playing.
    pub shown: u64,
    /// Frames the monitor had to skip or repeat.
    pub dropped: u64,
    /// The meters.
    pub meter: Meter,
    /// The mixing desk (its track meters and effects' state).
    pub mixer: mix::Mixer,
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
            stalled: false,
            seen_played: 0,
            progressed_at: Instant::now(),
            shown: 0,
            dropped: 0,
            meter: Meter::default(),
            mixer: mix::Mixer::default(),
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
        self.audio_master = false;
        self.stalled = false;
        self.progressed_at = now;
        if let Some(sink) = sink {
            if (speed - 1.0).abs() < 1e-9 {
                self.start_sound(now, sink);
            } else {
                sink.clear();
                sink.set_playing(false);
            }
        }
    }

    /// Makes the sound the master from where the playhead is.
    fn start_sound(&mut self, now: Instant, sink: &mut (dyn AudioSink + 'static)) {
        self.anchor = self.position;
        self.started = now;
        self.pushed = 0;
        sink.clear();
        self.played_at_start = sink.played_frames();
        self.seen_played = self.played_at_start;
        self.progressed_at = now;
        self.audio_master = true;
        self.stalled = false;
        sink.set_playing(true);
    }

    /// `true` while the device took nothing and the wall clock runs.
    #[must_use]
    pub fn stalled(&self) -> bool {
        self.stalled
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
            self.mixer.decay(1.0 / 60.0);
            return false;
        }
        let (start, end) = range;
        let mut by_sound = false;
        if (self.speed - 1.0).abs() < 1e-9
            && !self.stalled
            && let Some(sink) = sink.as_deref_mut()
        {
            let seen = sink.played_frames();
            if !self.audio_master || seen < self.seen_played {
                // The device came after playing started, or a new one
                // counts from zero: the sound starts here.
                self.start_sound(now, sink);
            }
            if seen > self.seen_played || sink.queued_frames() == 0 {
                self.progressed_at = now;
            }
            self.seen_played = seen;
            if now.saturating_duration_since(self.progressed_at) >= STALL {
                // Nothing taken for a while: the wall clock goes on from
                // here, and the device is let be.
                // Counted from the last time it took something, so the
                // playhead catches up with the time the wait cost.
                self.stalled = true;
                self.anchor = self.position;
                self.started = self.progressed_at;
                sink.clear();
                sink.set_playing(false);
            } else {
                by_sound = true;
            }
        }
        if by_sound && let Some(sink) = sink.as_deref_mut() {
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
                self.mixer
                    .mix(sequence, first, rate, &mut self.block, sound);
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
    #[ignore = "needs an audio output device; run with --ignored --nocapture"]
    fn the_real_device_drives_the_clock() {
        let config = sue_platform::AudioConfig {
            device: None,
            sample_rate: 48_000,
            buffer_frames: 512,
            realtime: true,
            queue_ms: 500,
        };
        let mut sink = sue_platform::AudioOutput::open(&config).expect("an output device");
        eprintln!("{:?}", sink.info());
        let sequence = crate::model::empty_sequence(64, 36, Rate::FPS_30, 1, 1);
        let range = (Time::ZERO, Time::from_seconds(6.0));
        let mut clock = Clock::default();
        let start = Instant::now();
        clock.play(1.0, start, Some(&mut sink));
        let mut last = 0.0;
        while start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(8));
            let now = Instant::now();
            clock.advance(&sequence, range, now, Some(&mut sink), &|_| None);
            let at = clock.position.seconds();
            if at - last >= 0.1 || start.elapsed().as_millis() % 250 < 9 {
                eprintln!(
                    "wall {:5.3} played {:6} pushed {:6} queued {:5} free {:5} position {:5.3}",
                    now.duration_since(start).as_secs_f64(),
                    sink.played_frames(),
                    clock.pushed,
                    sink.queued_frames(),
                    sink.free_frames(),
                    at
                );
                last = at;
            }
        }
        clock.pause(Some(&mut sink));
        assert!(clock.position.seconds() > 1.5, "{:?}", clock.position);
    }

    /// Advances `clock` at `at` milliseconds after `start`.
    fn step(clock: &mut Clock, sink: &mut NullAudio, start: Instant, at: u64) -> f64 {
        let sequence = crate::model::empty_sequence(64, 36, Rate::FPS_25, 1, 1);
        let range = (Time::ZERO, Time::from_seconds(60.0));
        let now = start + Duration::from_millis(at);
        assert!(clock.advance(&sequence, range, now, Some(sink), &|_| None));
        clock.position.seconds()
    }

    #[test]
    fn a_device_that_takes_nothing_never_holds_the_playhead() {
        let mut sink = NullAudio::new(48_000, 2, 48_000);
        let mut clock = Clock::default();
        let start = Instant::now();
        clock.play(1.0, start, Some(&mut sink));
        // The device never plays what is queued for it.
        assert_eq!(step(&mut clock, &mut sink, start, 0), 0.0);
        assert!(sink.queued_frames() > 0, "sound was queued");
        assert_eq!(step(&mut clock, &mut sink, start, 100), 0.0);
        assert!(!clock.stalled());
        step(&mut clock, &mut sink, start, 200);
        assert!(clock.stalled(), "the wall clock took over");
        assert!(!sink.is_playing(), "the device is let be");
        let at = step(&mut clock, &mut sink, start, 1_000);
        assert!((at - 1.0).abs() < 1e-6, "at real time: {at}");
        // Playing again tries the device again.
        clock.pause(Some(&mut sink));
        clock.play(1.0, start, Some(&mut sink));
        assert!(!clock.stalled() && sink.is_playing());
    }

    #[test]
    fn a_device_that_stalls_while_playing_hands_over_to_the_wall_clock() {
        let mut sink = NullAudio::new(48_000, 2, 48_000);
        let mut clock = Clock::default();
        let start = Instant::now();
        clock.play(1.0, start, Some(&mut sink));
        for at in (0..=500).step_by(10) {
            step(&mut clock, &mut sink, start, at);
            sink.advance(480);
        }
        let before = step(&mut clock, &mut sink, start, 510);
        assert!((before - 0.51).abs() < 0.011, "{before}");
        // Now it takes nothing more.
        for at in (520..=800).step_by(10) {
            step(&mut clock, &mut sink, start, at);
        }
        assert!(clock.stalled());
        let at = step(&mut clock, &mut sink, start, 1_500);
        assert!((at - 1.5).abs() < 0.02, "caught up with real time: {at}");
    }

    #[test]
    fn a_new_device_or_one_that_comes_late_takes_the_sound_from_the_playhead() {
        let sequence = crate::model::empty_sequence(64, 36, Rate::FPS_25, 1, 1);
        let range = (Time::ZERO, Time::from_seconds(60.0));
        let mut clock = Clock::default();
        let start = Instant::now();
        // Played before the device was open: the wall clock runs.
        clock.play(1.0, start, None);
        clock.advance(
            &sequence,
            range,
            start + Duration::from_millis(200),
            None,
            &|_| None,
        );
        assert!((clock.position.seconds() - 0.2).abs() < 1e-6);
        // The device opens: the sound starts where the playhead is.
        let mut sink = NullAudio::new(48_000, 2, 48_000);
        sink.advance(96_000);
        assert_eq!(step(&mut clock, &mut sink, start, 210), 0.2);
        assert!(sink.is_playing(), "the late device plays");
        for at in (220..=500).step_by(10) {
            sink.advance(480);
            step(&mut clock, &mut sink, start, at);
        }
        let at = clock.position.seconds();
        assert!((at - 0.49).abs() < 0.011, "{at}");
        // Another device, counting from zero (the settings changed).
        let mut other = NullAudio::new(44_100, 2, 44_100);
        assert!((step(&mut clock, &mut other, start, 510) - at).abs() < 1e-6);
        assert!(other.is_playing() && !clock.stalled());
        for at in (520..=1_000).step_by(10) {
            other.advance(441);
            step(&mut clock, &mut other, start, at);
        }
        let later = clock.position.seconds();
        assert!((later - (at + 0.49)).abs() < 0.011, "{later}");
    }

    #[test]
    fn a_device_that_takes_sound_in_bursts_stays_the_master() {
        let mut sink = NullAudio::new(48_000, 2, 48_000);
        let mut clock = Clock::default();
        let start = Instant::now();
        clock.play(1.0, start, Some(&mut sink));
        // A tenth of a second at a time, ticks every ten milliseconds.
        for at in (0..=2_000_u64).step_by(10) {
            if at % 100 == 0 {
                sink.advance(4_800);
            }
            let position = step(&mut clock, &mut sink, start, at);
            assert!(!clock.stalled(), "at {at} ms");
            let played = sink.played_frames() as f64 / 48_000.0;
            assert!((position - played).abs() < 1e-6, "{position} {played}");
        }
        assert!((clock.position.seconds() - 2.0).abs() < 0.11);
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
