//! Reading movies and their sound through the system's own decoders, for
//! Sue VE.
//!
//! - Windows: Media Foundation's Source Reader (the audited `sue-windows`
//!   crate, module `decode`): H.264, and HEVC where the system has its
//!   extension, in MP4 and QuickTime files; AAC, MP3, WAV and FLAC sound.
//!   Pictures decode on the graphics card where it can, and come out as
//!   RGBA at the size asked for.
//! - Elsewhere there is no route yet: [`available`] says so and every
//!   call returns [`VideoError::NotAvailable`], so the interface can show
//!   a clear "not available yet" instead of failing later.
//!
//! A decoder stays on the thread that opened it (on Windows it holds a
//! COM apartment), so each one lives on a worker thread of its own.

use std::path::Path;

/// Why a movie could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoError {
    /// This system has no movie decoding in Sue yet.
    NotAvailable,
    /// The file has no pictures (or, for a sound decoder, no sound).
    NoStream,
    /// The system decoder refused, with its own words.
    Failed(String),
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAvailable => f.write_str("movie decoding is not available on this system yet"),
            Self::NoStream => f.write_str("the file has no stream of that kind"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for VideoError {}

/// What a movie or sound file holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaFacts {
    /// Its pictures' width and height (0 for sound only).
    pub width: u32,
    /// See `width`.
    pub height: u32,
    /// Frames a second as numerator and denominator.
    pub rate: (u32, u32),
    /// Its length in 100 nanosecond units.
    pub duration: i64,
    /// The pictures' codec (`H264`, `HEVC`), empty for sound only.
    pub video_codec: String,
    /// The sound's rate and channels, if it has sound.
    pub audio: Option<(u32, u16)>,
}

/// One picture decoded: when it shows and for how long, in 100
/// nanosecond units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedFrame {
    /// When it shows.
    pub time: i64,
    /// How long (0 when the file does not say).
    pub duration: i64,
}

/// `true` where this system can read movies now.
#[must_use]
pub fn available() -> bool {
    cfg!(target_os = "windows")
}

/// `true` where pictures can be decoded on the graphics card.
#[must_use]
pub fn hardware_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        sue_windows::decode::hardware_present()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// What `path` holds.
///
/// # Errors
/// [`VideoError::NotAvailable`] off Windows; the system's refusal.
pub fn probe(path: &Path) -> Result<MediaFacts, VideoError> {
    #[cfg(target_os = "windows")]
    {
        sue_windows::decode::probe(path)
            .map(|info| facts(&info))
            .map_err(|error| VideoError::Failed(error.to_string()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err(VideoError::NotAvailable)
    }
}

#[cfg(target_os = "windows")]
fn facts(info: &sue_windows::decode::MediaInfo) -> MediaFacts {
    MediaFacts {
        width: info.width,
        height: info.height,
        rate: info.rate,
        duration: info.duration,
        video_codec: info.video_codec.clone(),
        audio: info.audio,
    }
}

#[cfg(target_os = "windows")]
fn failed(error: sue_windows::media::MediaError) -> VideoError {
    match error {
        sue_windows::media::MediaError::BadInput(
            "the file has no pictures" | "the file has no sound",
        ) => VideoError::NoStream,
        other => VideoError::Failed(other.to_string()),
    }
}

/// The pictures of a movie, decoded one after another.
#[derive(Debug)]
pub struct VideoDecoder {
    #[cfg(target_os = "windows")]
    reader: sue_windows::decode::MediaReader,
}

impl VideoDecoder {
    /// Opens `path` for its pictures at `size` (else its own), on the
    /// graphics card when `hardware` and the card takes it.
    ///
    /// # Errors
    /// [`VideoError::NotAvailable`] off Windows; a file without pictures;
    /// the system's refusal.
    pub fn open(path: &Path, size: Option<(u32, u32)>, hardware: bool) -> Result<Self, VideoError> {
        #[cfg(target_os = "windows")]
        {
            let options = sue_windows::decode::ReaderOptions::video(size, hardware);
            sue_windows::decode::MediaReader::open(path, options)
                .map(|reader| Self { reader })
                .map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (path, size, hardware);
            Err(VideoError::NotAvailable)
        }
    }

    /// What the file holds.
    #[must_use]
    pub fn facts(&self) -> MediaFacts {
        #[cfg(target_os = "windows")]
        {
            facts(self.reader.info())
        }
        #[cfg(not(target_os = "windows"))]
        {
            MediaFacts::default()
        }
    }

    /// The size pictures come out at.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        #[cfg(target_os = "windows")]
        {
            self.reader.size()
        }
        #[cfg(not(target_os = "windows"))]
        {
            (0, 0)
        }
    }

    /// `true` when it decodes on the graphics card.
    #[must_use]
    pub fn hardware(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.reader.hardware()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// One frame's length in 100 nanosecond units.
    #[must_use]
    pub fn frame_ticks(&self) -> i64 {
        #[cfg(target_os = "windows")]
        {
            self.reader.frame_ticks()
        }
        #[cfg(not(target_os = "windows"))]
        {
            333_333
        }
    }

    /// Goes to the key frame at or before `time`.
    ///
    /// # Errors
    /// The system's refusal.
    pub fn seek(&mut self, time: i64) -> Result<(), VideoError> {
        #[cfg(target_os = "windows")]
        {
            self.reader.seek(time).map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = time;
            Err(VideoError::NotAvailable)
        }
    }

    /// The next picture, as opaque RGBA in `rgba`. `None` at the end.
    ///
    /// # Errors
    /// The system's refusal.
    pub fn next_frame(&mut self, rgba: &mut Vec<u8>) -> Result<Option<DecodedFrame>, VideoError> {
        #[cfg(target_os = "windows")]
        {
            self.reader
                .next_frame(rgba)
                .map(|frame| {
                    frame.map(|frame| DecodedFrame {
                        time: frame.time,
                        duration: frame.duration,
                    })
                })
                .map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = rgba;
            Err(VideoError::NotAvailable)
        }
    }

    /// Reads on to the picture showing at `time` (after a seek to before
    /// it): the exact frame.
    ///
    /// # Errors
    /// The system's refusal.
    pub fn frame_at(
        &mut self,
        time: i64,
        rgba: &mut Vec<u8>,
    ) -> Result<Option<DecodedFrame>, VideoError> {
        #[cfg(target_os = "windows")]
        {
            self.reader
                .frame_at(time, rgba)
                .map(|frame| {
                    frame.map(|frame| DecodedFrame {
                        time: frame.time,
                        duration: frame.duration,
                    })
                })
                .map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (time, rgba);
            Err(VideoError::NotAvailable)
        }
    }
}

/// The sound of a movie or sound file, decoded piece by piece.
#[derive(Debug)]
pub struct SoundDecoder {
    #[cfg(target_os = "windows")]
    reader: sue_windows::decode::MediaReader,
}

impl SoundDecoder {
    /// Opens `path` for its sound, resampled to `rate` in `channels`.
    ///
    /// # Errors
    /// [`VideoError::NotAvailable`] off Windows; a file without sound;
    /// the system's refusal.
    pub fn open(path: &Path, rate: u32, channels: u16) -> Result<Self, VideoError> {
        #[cfg(target_os = "windows")]
        {
            let options = sue_windows::decode::ReaderOptions::sound(rate, channels);
            sue_windows::decode::MediaReader::open(path, options)
                .map(|reader| Self { reader })
                .map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (path, rate, channels);
            Err(VideoError::NotAvailable)
        }
    }

    /// The rate and channels the sound comes out at.
    #[must_use]
    pub fn format(&self) -> (u32, u16) {
        #[cfg(target_os = "windows")]
        {
            self.reader.sound_format()
        }
        #[cfg(not(target_os = "windows"))]
        {
            (0, 0)
        }
    }

    /// Goes to `time` (100 nanosecond units).
    ///
    /// # Errors
    /// The system's refusal.
    pub fn seek(&mut self, time: i64) -> Result<(), VideoError> {
        #[cfg(target_os = "windows")]
        {
            self.reader.seek(time).map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = time;
            Err(VideoError::NotAvailable)
        }
    }

    /// Appends the next piece of interleaved float sound to `samples`
    /// and says when it starts. `None` at the end.
    ///
    /// # Errors
    /// The system's refusal.
    pub fn next(&mut self, samples: &mut Vec<f32>) -> Result<Option<i64>, VideoError> {
        #[cfg(target_os = "windows")]
        {
            self.reader.next_sound(samples).map_err(failed)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = samples;
            Err(VideoError::NotAvailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_everywhere() {
        let missing = Path::new("no-such-movie-anywhere.mp4");
        assert!(probe(missing).is_err());
        assert!(VideoDecoder::open(missing, None, false).is_err());
        assert!(SoundDecoder::open(missing, 48_000, 2).is_err());
        assert_eq!(available(), cfg!(target_os = "windows"));
    }
}
