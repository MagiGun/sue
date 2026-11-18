//! Mixing a sequence's sound.
//!
//! Every audio track that sounds (not muted; only the soloed ones when any
//! is soloed) adds its clips: each source's decoded sound read at the
//! clip's source time (between samples linearly, so speed changes and any
//! output rate work), times the clip's gain (keyframable, in decibels),
//! its fades, the crossfade of a dissolve between two clips, and the
//! track's gain. The result is stereo; [`Meter`] follows its peaks for the
//! audio meters.

use std::sync::Arc;

use sue_core::{EntityId, Time, Value};
use sue_model::{Clip, Sequence, Track};

use crate::media::sound::{MIX_RATE, Sound};
use crate::model::{self, key};

/// Decibels as a gain.
#[must_use]
pub fn gain(decibels: f64) -> f32 {
    if decibels <= -96.0 {
        0.0
    } else {
        10_f64.powf(decibels / 20.0) as f32
    }
}

/// How much of `clip` sounds at `time` because of its fades and the
/// transitions on its track (1 in the middle).
fn envelope(track: &Track, clip: &Clip, time: Time) -> f32 {
    let mut level = 1.0_f32;
    let seconds = |name: &str| {
        clip.properties
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let (fade_in, fade_out) = (seconds(key::FADE_IN), seconds(key::FADE_OUT));
    let into = (time - clip.start).seconds();
    let left = (clip.end() - time).seconds();
    if fade_in > 0.0 && into < fade_in {
        level *= (into / fade_in).clamp(0.0, 1.0) as f32;
    }
    if fade_out > 0.0 && left < fade_out {
        level *= (left / fade_out).clamp(0.0, 1.0) as f32;
    }
    for transition in &track.transitions {
        if time < transition.start || time >= transition.end() {
            continue;
        }
        let progress = transition.progress(time) as f32;
        if transition.from == clip.id {
            level *= 1.0 - progress;
        } else if transition.to == clip.id {
            level *= progress;
        }
    }
    level
}

/// The stretch a clip sounds over: itself, and the handles a transition
/// lends it on either side.
fn reach(track: &Track, clip: &Clip) -> (Time, Time) {
    let mut start = clip.start;
    let mut end = clip.end();
    for transition in &track.transitions {
        if transition.to == clip.id {
            start = start.min(transition.start);
        }
        if transition.from == clip.id {
            end = end.max(transition.end());
        }
    }
    (start, end)
}

/// Mixes `frames` stereo frames of `sequence` from frame `first` (at
/// `rate` frames a second) into `out` (interleaved, cleared first).
/// `sound` gives each source's decoded sound, when it is ready.
pub fn mix(
    sequence: &Sequence,
    first: i64,
    rate: u32,
    out: &mut [f32],
    sound: &dyn Fn(EntityId) -> Option<Arc<Sound>>,
) {
    out.fill(0.0);
    let frames = out.len() / 2;
    if frames == 0 || rate == 0 {
        return;
    }
    let rate_f = f64::from(rate);
    let at = |frame: i64| Time::from_seconds(frame as f64 / rate_f);
    let (from, to) = (at(first), at(first + frames as i64));
    let soloed = sequence
        .tracks
        .iter()
        .any(|track| !model::is_video(track) && model::flag(track, key::SOLO));
    for track in &sequence.tracks {
        if model::is_video(track) || track.muted || (soloed && !model::flag(track, key::SOLO)) {
            continue;
        }
        let track_gain = gain(
            track
                .properties
                .get(key::VOLUME)
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        for clip in track.clips() {
            let (start, end) = reach(track, clip);
            if !clip.enabled || end <= from || start >= to {
                continue;
            }
            if clip
                .properties
                .get(key::FREEZE)
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let Some(source) = sound(clip.source) else {
                continue;
            };
            let first_frame = ((start.seconds() * rate_f).ceil() as i64).max(first);
            let last_frame = ((end.seconds() * rate_f).ceil() as i64).min(first + frames as i64);
            let source_rate = f64::from(MIX_RATE);
            // Gain and envelope change slowly: worked out every 64 frames.
            let mut level = 0.0_f32;
            for frame in first_frame..last_frame {
                let time = at(frame);
                if (frame - first_frame) % 64 == 0 {
                    let decibels = clip.value_at(key::VOLUME, time, 0.0);
                    level = gain(decibels) * envelope(track, clip, time) * track_gain;
                }
                if level == 0.0 {
                    continue;
                }
                let position = (clip.source_in.seconds()
                    + (time - clip.start).seconds() * clip.speed)
                    * source_rate;
                let [left, right] = source.at(position);
                let index = (frame - first) as usize * 2;
                if let Some(pair) = out.get_mut(index..index + 2) {
                    pair[0] += left * level;
                    pair[1] += right * level;
                }
            }
        }
    }
}

/// The audio meters: the peak of each channel, falling back slowly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meter {
    /// The level of each channel, 0 to 1 (and above when it clips).
    pub level: [f32; 2],
    /// The highest level held for a moment.
    pub hold: [f32; 2],
}

impl Meter {
    /// Takes in a block of interleaved stereo that lasted `seconds`.
    pub fn feed(&mut self, block: &[f32], seconds: f32) {
        let mut peak = [0.0_f32; 2];
        for pair in block.chunks_exact(2) {
            peak[0] = peak[0].max(pair[0].abs());
            peak[1] = peak[1].max(pair[1].abs());
        }
        self.decay(seconds);
        for channel in 0..2 {
            self.level[channel] = self.level[channel].max(peak[channel]);
            self.hold[channel] = self.hold[channel].max(peak[channel]);
        }
    }

    /// Lets the levels fall for `seconds` (about 20 dB a second).
    pub fn decay(&mut self, seconds: f32) {
        let fall = 10_f32.powf(-seconds.max(0.0));
        for channel in 0..2 {
            self.level[channel] *= fall;
            self.hold[channel] *= fall.sqrt();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::Rate;
    use sue_model::Transition;

    fn tone(value: f32, seconds: f64) -> Arc<Sound> {
        let frames = (seconds * f64::from(MIX_RATE)) as usize;
        Arc::new(Sound::new(vec![value; frames * 2]))
    }

    #[test]
    fn clips_sum_with_gain_fades_and_crossfades() {
        let (a, b) = (EntityId::new(), EntityId::new());
        let mut sequence = model::empty_sequence(64, 36, Rate::FPS_25, 1, 2);
        let mut one = Clip::new(a, Time::ZERO, Time::SECOND);
        one.id = 1;
        one.properties.set(key::FADE_IN, 0.5);
        let mut two = Clip::new(b, Time::SECOND, Time::SECOND);
        two.id = 2;
        two.properties.set(key::VOLUME, -6.0);
        sequence.tracks[1].insert(one);
        sequence.tracks[1].insert(two);
        let sounds = |id: EntityId| {
            Some(if id == a {
                tone(0.5, 3.0)
            } else {
                tone(0.25, 3.0)
            })
        };
        let mut out = vec![0.0; 48_000 * 2 * 2];
        mix(&sequence, 0, 48_000, &mut out, &sounds);
        let at = |seconds: f64| out[(seconds * 48_000.0) as usize * 2];
        assert!(
            (at(0.25) - 0.25).abs() < 0.01,
            "half way up the fade: {}",
            at(0.25)
        );
        assert!((at(0.75) - 0.5).abs() < 1e-3);
        assert!((at(1.5) - 0.25 * 0.501).abs() < 0.002, "-6 dB: {}", at(1.5));
        // A crossfade across the cut: both sound, one going, one coming.
        sequence.tracks[1].transitions.push(Transition::new(
            "dissolve",
            Time::from_seconds(0.8),
            Time::from_seconds(0.4),
            1,
            2,
        ));
        mix(&sequence, 0, 48_000, &mut out, &sounds);
        let at = |seconds: f64| out[(seconds * 48_000.0) as usize * 2];
        let middle = 0.5 * 0.5 + 0.25 * 0.501 * 0.5;
        assert!((at(1.0) - middle).abs() < 0.01, "{} for {middle}", at(1.0));
        // Muted: silence; another track soloed: silence here too.
        sequence.tracks[1].muted = true;
        mix(&sequence, 0, 48_000, &mut out, &sounds);
        assert!(out.iter().all(|sample| *sample == 0.0));
        sequence.tracks[1].muted = false;
        sequence.tracks[2].properties.set(key::SOLO, true);
        mix(&sequence, 0, 48_000, &mut out, &sounds);
        assert!(out.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn speed_and_output_rate_read_the_source_at_the_right_place() {
        let a = EntityId::new();
        let mut sequence = model::empty_sequence(64, 36, Rate::FPS_25, 1, 1);
        let mut clip = Clip::new(a, Time::ZERO, Time::SECOND);
        clip.id = 1;
        clip.speed = 2.0;
        sequence.tracks[1].insert(clip);
        // A ramp: sample n is n / 48000.
        let ramp: Vec<f32> = (0..96_000).flat_map(|n| [n as f32 / 48_000.0; 2]).collect();
        let sound = Arc::new(Sound::new(ramp));
        let mut out = vec![0.0; 44_100 * 2];
        mix(&sequence, 0, 44_100, &mut out, &|_| {
            Some(Arc::clone(&sound))
        });
        // Half a second in at twice the speed: one second of source.
        let half = out[22_050 * 2];
        assert!((half - 1.0).abs() < 1e-3, "{half}");
    }

    #[test]
    fn meters_hold_peaks_and_fall() {
        let mut meter = Meter::default();
        meter.feed(&[0.5, -0.8, 0.1, 0.2], 0.01);
        assert_eq!(meter.level, [0.5, 0.8]);
        meter.decay(1.0);
        assert!(meter.level[1] < 0.1 && meter.hold[1] > meter.level[1]);
    }
}
