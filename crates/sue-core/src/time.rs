//! Time. Video, audio, animation, slideshows, subtitles and screen recording
//! all share one clock: [`Time`] counts flicks (1/705 600 000 s), which
//! divide evenly by every common frame and sample rate, so frame and sample
//! positions convert without drift. [`Rate`] is an exact rational rate
//! (23.976 is 24000/1001, not a float) and [`Timecode`] reads and writes
//! SMPTE timecode, drop-frame included.

use std::fmt;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use crate::FLICKS_PER_SECOND;

/// A point or a duration on the global clock, in flicks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(pub i64);

impl Time {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// One second.
    pub const SECOND: Self = Self(FLICKS_PER_SECOND);

    /// From seconds, rounded to the nearest flick.
    #[must_use]
    pub fn from_seconds(seconds: f64) -> Self {
        Self((seconds * FLICKS_PER_SECOND as f64).round() as i64)
    }

    /// In seconds.
    #[must_use]
    pub fn seconds(self) -> f64 {
        self.0 as f64 / FLICKS_PER_SECOND as f64
    }

    /// The flick count.
    #[must_use]
    pub const fn flicks(self) -> i64 {
        self.0
    }

    /// The start of frame `frame` at `rate`.
    #[must_use]
    pub fn from_frames(frame: i64, rate: Rate) -> Self {
        Self(rate.flicks_for(frame))
    }

    /// The frame that contains this time at `rate` (floor).
    #[must_use]
    pub fn frame(self, rate: Rate) -> i64 {
        let numerator = i128::from(self.0) * i128::from(rate.numerator);
        let denominator = i128::from(FLICKS_PER_SECOND) * i128::from(rate.denominator);
        numerator.div_euclid(denominator) as i64
    }

    /// The start of sample `sample` at `sample_rate` Hz.
    #[must_use]
    pub fn from_samples(sample: i64, sample_rate: u32) -> Self {
        Self::from_frames(sample, Rate::new(sample_rate, 1))
    }

    /// The sample that contains this time at `sample_rate` Hz (floor).
    #[must_use]
    pub fn sample(self, sample_rate: u32) -> i64 {
        self.frame(Rate::new(sample_rate, 1))
    }

    /// Snapped down to the start of its frame at `rate`.
    #[must_use]
    pub fn snap(self, rate: Rate) -> Self {
        Self::from_frames(self.frame(rate), rate)
    }
}

impl Add for Time {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl Sub for Time {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl Neg for Time {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

/// An exact rational rate in events per second (frames, samples, ticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rate {
    numerator: u32,
    denominator: u32,
}

impl Rate {
    /// 23.976 fps (NTSC film).
    pub const FPS_23_976: Self = Self::new(24_000, 1001);
    /// 24 fps.
    pub const FPS_24: Self = Self::new(24, 1);
    /// 25 fps (PAL).
    pub const FPS_25: Self = Self::new(25, 1);
    /// 29.97 fps (NTSC).
    pub const FPS_29_97: Self = Self::new(30_000, 1001);
    /// 30 fps.
    pub const FPS_30: Self = Self::new(30, 1);
    /// 50 fps.
    pub const FPS_50: Self = Self::new(50, 1);
    /// 59.94 fps.
    pub const FPS_59_94: Self = Self::new(60_000, 1001);
    /// 60 fps.
    pub const FPS_60: Self = Self::new(60, 1);

    /// `numerator / denominator` per second, reduced.
    ///
    /// # Panics
    /// If either part is zero.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        assert!(numerator > 0 && denominator > 0, "a rate must be positive");
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        Self {
            numerator: numerator / a,
            denominator: denominator / a,
        }
    }

    /// Numerator.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Denominator.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// As a float, for display.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    /// The flick position of event `index`, rounded to the nearest flick
    /// (exact for every common rate).
    #[must_use]
    pub fn flicks_for(self, index: i64) -> i64 {
        let numerator =
            i128::from(index) * i128::from(FLICKS_PER_SECOND) * i128::from(self.denominator);
        let denominator = i128::from(self.numerator);
        let half = denominator / 2;
        let rounded = if numerator >= 0 {
            (numerator + half) / denominator
        } else {
            (numerator - half) / denominator
        };
        rounded as i64
    }

    /// The nominal whole frame count per second used by timecode (30 for 29.97).
    #[must_use]
    pub fn timecode_base(self) -> u32 {
        self.numerator.div_ceil(self.denominator)
    }

    /// `true` for the NTSC rates whose timecode is normally drop-frame.
    #[must_use]
    pub fn is_ntsc(self) -> bool {
        self.denominator == 1001 && self.numerator.is_multiple_of(30_000)
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(f, "{}", self.numerator)
        } else {
            write!(f, "{}/{}", self.numerator, self.denominator)
        }
    }
}

/// SMPTE timecode: hours, minutes, seconds and frames at a rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timecode {
    /// Hours.
    pub hours: u32,
    /// Minutes.
    pub minutes: u32,
    /// Seconds.
    pub seconds: u32,
    /// Frames.
    pub frames: u32,
    /// Drop-frame counting (written with `;` before the frames).
    pub drop_frame: bool,
}

impl Timecode {
    /// The timecode of frame `frame` at `rate`. Drop-frame applies only to
    /// NTSC rates.
    #[must_use]
    pub fn from_frame(frame: i64, rate: Rate, drop_frame: bool) -> Self {
        let base = i64::from(rate.timecode_base());
        let drop_frame = drop_frame && rate.is_ntsc();
        let mut frame = frame.max(0);
        if drop_frame {
            // Two frame numbers (four at 59.94) are skipped every minute
            // except every tenth minute.
            let dropped = base / 15;
            let per_ten_minutes = base * 600 - dropped * 9;
            let per_minute = base * 60 - dropped;
            let tens = frame / per_ten_minutes;
            let rest = frame % per_ten_minutes;
            let extra = if rest < dropped {
                0
            } else {
                dropped * ((rest - dropped) / per_minute)
            };
            frame += dropped * 9 * tens + extra;
        }
        Self {
            hours: (frame / (base * 3600)) as u32,
            minutes: (frame / (base * 60) % 60) as u32,
            seconds: (frame / base % 60) as u32,
            frames: (frame % base) as u32,
            drop_frame,
        }
    }

    /// The frame number this timecode names at `rate`.
    #[must_use]
    pub fn to_frame(&self, rate: Rate) -> i64 {
        let base = i64::from(rate.timecode_base());
        let minutes = i64::from(self.hours) * 60 + i64::from(self.minutes);
        let mut frame = (minutes * 60 + i64::from(self.seconds)) * base + i64::from(self.frames);
        if self.drop_frame && rate.is_ntsc() {
            frame -= base / 15 * (minutes - minutes / 10);
        }
        frame
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let separator = if self.drop_frame { ';' } else { ':' };
        write!(
            f,
            "{:02}:{:02}:{:02}{separator}{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

impl FromStr for Timecode {
    type Err = String;

    /// Parses `HH:MM:SS:FF` or drop-frame `HH:MM:SS;FF`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let drop_frame = text.contains(';');
        let parts: Vec<u32> = text
            .split([':', ';'])
            .map(|part| part.trim().parse::<u32>())
            .collect::<Result<_, _>>()
            .map_err(|_| format!("`{text}` is not a timecode"))?;
        let [hours, minutes, seconds, frames] = parts[..] else {
            return Err(format!("`{text}` needs four fields"));
        };
        Ok(Self {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_samples_land_on_exact_flicks() {
        for rate in [
            Rate::FPS_23_976,
            Rate::FPS_24,
            Rate::FPS_25,
            Rate::FPS_29_97,
            Rate::FPS_59_94,
            Rate::FPS_60,
        ] {
            for frame in [0, 1, 1000, 86_400 * 60] {
                let time = Time::from_frames(frame, rate);
                assert_eq!(time.frame(rate), frame, "{rate}");
                assert_eq!(
                    i128::from(time.0) * i128::from(rate.numerator()),
                    i128::from(frame)
                        * i128::from(FLICKS_PER_SECOND)
                        * i128::from(rate.denominator()),
                    "{rate} has no rounding"
                );
            }
        }
        assert_eq!(Time::from_samples(48_000, 48_000), Time::SECOND);
        assert_eq!(Time::from_seconds(1.5).sample(44_100), 66_150);
        assert_eq!(Time::from_frames(-1, Rate::FPS_24).frame(Rate::FPS_24), -1);
        assert_eq!(Rate::new(50, 2), Rate::FPS_25);
    }

    #[test]
    fn timecode_round_trips_including_drop_frame() {
        let tc = Timecode::from_frame(90_000, Rate::FPS_25, false);
        assert_eq!(tc.to_string(), "01:00:00:00");
        // 29.97 drop-frame: frame 1800 is 00:01:00;02 (00 and 01 are skipped).
        let tc = Timecode::from_frame(1800, Rate::FPS_29_97, true);
        assert_eq!(tc.to_string(), "00:01:00;02");
        // Every tenth minute keeps its first two frame numbers.
        let tc = Timecode::from_frame(17_982, Rate::FPS_29_97, true);
        assert_eq!(tc.to_string(), "00:10:00;00");
        for frame in [0, 1799, 1800, 17_981, 17_982, 107_892, 1_000_000] {
            for rate in [Rate::FPS_29_97, Rate::FPS_59_94] {
                let tc = Timecode::from_frame(frame, rate, true);
                assert_eq!(tc.to_frame(rate), frame, "{tc}");
                assert_eq!(tc.to_string().parse::<Timecode>().unwrap(), tc);
            }
        }
        assert!("1:2:3".parse::<Timecode>().is_err());
    }
}
