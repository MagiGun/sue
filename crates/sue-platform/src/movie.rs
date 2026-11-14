//! Movies through the system's own encoders. `sue-io` holds the neutral
//! part (formats, settings, NV12 and the [`MovieSink`] trait); this module
//! says which formats this system can write and opens a sink for one.
//!
//! - Windows: "MP4 (H.264)" through Media Foundation (the audited
//!   `sue-windows` crate), H.264 High and AAC-LC, encoded by Windows
//!   itself; where Windows has no encoder (Windows N without the Media
//!   Feature Pack) only the AVI is offered.
//! - Everywhere: "AVI (Motion JPEG)", Sue's own writer, without sound.
//! - Android (P7-3 of the v0.0.8 plan) and the other systems (Phase 8)
//!   have no MP4 route yet: they offer the AVI.

use sue_core::RasterTile2D;
use sue_io::{MjpegAvi, MovieError, MovieFormat, MovieSettings, MovieSink};

/// Whether this system can write "MP4 (H.264)" now.
#[must_use]
pub fn mp4_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        sue_windows::media::h264_encoder_present()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Whether an MP4 can carry sound here (an AAC encoder is present).
#[must_use]
pub fn mp4_sound_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        mp4_available() && sue_windows::media::aac_encoder_present()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// The movie formats this system writes, in the order a dialog offers
/// them: MP4 first where there is an encoder, then the AVI.
#[must_use]
pub fn formats() -> Vec<MovieFormat> {
    MovieFormat::ALL
        .into_iter()
        .filter(|format| *format != MovieFormat::Mp4H264 || mp4_available())
        .collect()
}

/// Acts, on this thread, as a system without an MP4 encoder (`true`) or
/// as it is (`false`), so tests can try the fallback on a machine that has
/// one.
pub fn pretend_no_mp4_encoder(pretend: bool) {
    #[cfg(target_os = "windows")]
    sue_windows::media::pretend_no_encoder(pretend);
    #[cfg(not(target_os = "windows"))]
    let _ = pretend;
}

/// Begins a movie in `format`. The sink stays on the thread that opened
/// it.
///
/// # Errors
/// [`MovieError::NoEncoder`] where this system cannot write the format;
/// settings the format refuses ([`MovieSettings::check`]); a system
/// encoder that will not start.
pub fn open(
    format: MovieFormat,
    settings: MovieSettings,
) -> Result<Box<dyn MovieSink>, MovieError> {
    settings.check(format)?;
    match format {
        MovieFormat::MjpegAvi => Ok(Box::new(MjpegAvi::sink(settings)?)),
        MovieFormat::Mp4H264 => open_mp4(settings),
    }
}

#[cfg(not(target_os = "windows"))]
fn open_mp4(_settings: MovieSettings) -> Result<Box<dyn MovieSink>, MovieError> {
    Err(MovieError::NoEncoder)
}

#[cfg(target_os = "windows")]
fn open_mp4(settings: MovieSettings) -> Result<Box<dyn MovieSink>, MovieError> {
    Ok(Box::new(windows_mp4::Mp4Sink::begin(settings)?))
}

/// A float sample in -1 to 1 as a 16-bit one, clamped and rounded.
fn to_pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16
}

#[cfg(target_os = "windows")]
mod windows_mp4 {
    use super::{MovieError, MovieFormat, MovieSettings, MovieSink, RasterTile2D, to_pcm16};
    use sue_core::PixelFormat;
    use sue_io::TICKS_PER_SECOND;
    use sue_windows::media::{AudioSpec, MediaError, Mp4Writer, VideoSpec};

    fn movie_error(error: MediaError) -> MovieError {
        match error {
            MediaError::NoEncoder => MovieError::NoEncoder,
            other => MovieError::Encoder(other.to_string()),
        }
    }

    /// The most sound frames handed over in one sample (a tenth of a
    /// second at 48000).
    const SOUND_PIECE: u64 = 4_800;

    /// An MP4 through Media Foundation.
    pub(super) struct Mp4Sink {
        writer: Mp4Writer,
        settings: MovieSettings,
        size: (u32, u32),
        frames: u64,
        /// The NV12 picture being converted, kept between frames.
        nv12: Vec<u8>,
        /// Sound pushed but not yet handed over, interleaved.
        sound: Vec<f32>,
        /// Sound frames handed over so far.
        sound_written: u64,
    }

    impl Mp4Sink {
        pub(super) fn begin(settings: MovieSettings) -> Result<Self, MovieError> {
            let size = settings.encoded_size(MovieFormat::Mp4H264);
            // A key frame every two seconds.
            let key_interval = (settings.fps() * 2.0).round().clamp(1.0, 600.0) as u32;
            let video = VideoSpec {
                width: size.0,
                height: size.1,
                rate: settings.rate,
                quality: settings.quality.h264_quality(),
                key_interval,
            };
            let audio = settings.sound.map(|sound| AudioSpec {
                rate: sound.rate,
                channels: sound.channels,
            });
            let writer = Mp4Writer::new(video, audio).map_err(movie_error)?;
            Ok(Self {
                writer,
                settings,
                size,
                frames: 0,
                nv12: Vec::new(),
                sound: Vec::new(),
                sound_written: 0,
            })
        }

        /// Hands over the pushed sound that falls before `until` (in
        /// ticks), in pieces.
        fn flush_sound(&mut self, until: u64) -> Result<(), MovieError> {
            let Some(sound) = self.settings.sound else {
                return Ok(());
            };
            let (rate, channels) = (u64::from(sound.rate), usize::from(sound.channels));
            let due = (u128::from(until) * u128::from(rate) / u128::from(TICKS_PER_SECOND)) as u64;
            while self.sound_written < due && !self.sound.is_empty() {
                let pending = (self.sound.len() / channels) as u64;
                let take = (due - self.sound_written).min(pending).min(SOUND_PIECE);
                if take == 0 {
                    break;
                }
                let samples = take as usize * channels;
                let pcm: Vec<i16> = self.sound.drain(..samples).map(to_pcm16).collect();
                let time = self.sound_written * TICKS_PER_SECOND / rate;
                let end = (self.sound_written + take) * TICKS_PER_SECOND / rate;
                self.writer
                    .write_audio(&pcm, time as i64, (end - time) as i64)
                    .map_err(movie_error)?;
                self.sound_written += take;
            }
            Ok(())
        }
    }

    impl MovieSink for Mp4Sink {
        fn format(&self) -> MovieFormat {
            MovieFormat::Mp4H264
        }

        fn settings(&self) -> &MovieSettings {
            &self.settings
        }

        fn push_frame(&mut self, frame: &RasterTile2D) -> Result<(), MovieError> {
            if (frame.width(), frame.height()) != (self.settings.width, self.settings.height) {
                return Err(MovieError::WrongSize);
            }
            if frame.format() != PixelFormat::Rgba8 {
                return Err(MovieError::NoPixels);
            }
            let rgba = frame.bytes().ok_or(MovieError::NoPixels)?;
            sue_io::rgba_to_nv12_into(
                rgba,
                frame.width(),
                frame.height(),
                self.size.0,
                self.size.1,
                &mut self.nv12,
            )?;
            let time = self.settings.frame_time(self.frames);
            let end = self.settings.frame_time(self.frames + 1);
            self.writer
                .write_video(&self.nv12, time as i64, (end - time) as i64)
                .map_err(movie_error)?;
            self.frames += 1;
            self.flush_sound(end)
        }

        fn push_sound(&mut self, samples: &[f32]) -> Result<(), MovieError> {
            let Some(sound) = self.settings.sound else {
                return Err(MovieError::NoSoundTrack);
            };
            if !samples.len().is_multiple_of(usize::from(sound.channels)) {
                return Err(MovieError::BadSound);
            }
            self.sound.extend_from_slice(samples);
            self.flush_sound(self.settings.frame_time(self.frames))
        }

        fn frames(&self) -> usize {
            self.frames as usize
        }

        fn finish(mut self: Box<Self>) -> Result<Vec<u8>, MovieError> {
            if self.frames == 0 {
                return Err(MovieError::NoFrames);
            }
            // The frames make the movie's length: sound past it is cut.
            self.flush_sound(self.settings.frame_time(self.frames))?;
            let bytes = self.writer.finish().map_err(movie_error)?;
            // Media Foundation puts the index last; a file whose index
            // cannot be moved is still a valid MP4, so it is kept as is.
            Ok(sue_io::mp4_index_first(&bytes).unwrap_or(bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::PixelFormat;
    use sue_io::{MovieQuality, MovieSound, rgba_to_nv12};

    fn frame(width: u32, height: u32, rgba: [u8; 4]) -> RasterTile2D {
        RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            rgba.repeat((width * height) as usize),
        )
        .unwrap()
    }

    #[test]
    fn to_pcm16_clamps_and_rounds() {
        assert_eq!(to_pcm16(0.0), 0);
        assert_eq!(to_pcm16(1.0), 32_767);
        assert_eq!(to_pcm16(-1.0), -32_767);
        assert_eq!(to_pcm16(4.0), 32_767);
        assert_eq!(to_pcm16(0.5), 16_384);
    }

    #[test]
    fn the_avi_is_always_offered_and_last() {
        let formats = formats();
        assert_eq!(formats.last(), Some(&MovieFormat::MjpegAvi));
        pretend_no_mp4_encoder(true);
        assert_eq!(super::formats(), [MovieFormat::MjpegAvi]);
        assert!(!mp4_available());
        let refused = open(MovieFormat::Mp4H264, MovieSettings::new(64, 48, 24)).err();
        pretend_no_mp4_encoder(false);
        assert_eq!(refused, Some(MovieError::NoEncoder), "a clean refusal");
        // The fallback writes.
        let mut avi = open(MovieFormat::MjpegAvi, MovieSettings::new(8, 8, 12)).unwrap();
        avi.push_frame(&frame(8, 8, [0, 0, 0, 255])).unwrap();
        assert!(avi.finish().unwrap().starts_with(b"RIFF"));
    }

    #[cfg(target_os = "windows")]
    mod on_windows {
        use super::*;
        use sue_windows::media::read_mp4;

        /// A short known animation: a black square moving right over
        /// white, 65 by 49 (odd, so padded to 66 by 50), 12 frames at 12
        /// a second, with a second of sound.
        fn animation() -> Vec<RasterTile2D> {
            (0..12)
                .map(|step| {
                    let mut pixels = [255_u8; 65 * 49 * 4].to_vec();
                    for y in 16..32 {
                        for x in step * 4..step * 4 + 16 {
                            let at = (y * 65 + x) * 4;
                            pixels[at..at + 3].copy_from_slice(&[0, 0, 0]);
                        }
                    }
                    RasterTile2D::from_bytes(65, 49, PixelFormat::Rgba8, pixels).unwrap()
                })
                .collect()
        }

        #[test]
        fn mp4_is_offered_first_here() {
            // The development machine runs Windows 11 Home, with encoders.
            assert!(mp4_available() && mp4_sound_available());
            assert_eq!(formats(), [MovieFormat::Mp4H264, MovieFormat::MjpegAvi]);
        }

        #[test]
        fn a_short_animation_encodes_and_reads_back() {
            let mut settings = MovieSettings::new(65, 49, 12);
            settings.quality = MovieQuality::High;
            settings.sound = Some(MovieSound {
                rate: 48_000,
                channels: 2,
            });
            // Below the size floor and odd sizes are refused or padded.
            assert_eq!(
                open(MovieFormat::Mp4H264, MovieSettings::new(30, 30, 12)).err(),
                Some(MovieError::BadSize)
            );
            let frames = animation();
            let mut sink = open(MovieFormat::Mp4H264, settings).unwrap();
            assert_eq!(sink.format(), MovieFormat::Mp4H264);
            assert_eq!(
                sink.push_frame(&frame(8, 8, [0, 0, 0, 255])),
                Err(MovieError::WrongSize)
            );
            // Sound first, the whole second (and a half more, to be cut).
            let tone: Vec<f32> = (0..72_000)
                .flat_map(|at| {
                    let value = (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                    [value, value]
                })
                .collect();
            sink.push_sound(&tone).unwrap();
            assert_eq!(sink.push_sound(&[0.0]), Err(MovieError::BadSound));
            for frame in &frames {
                sink.push_frame(frame).unwrap();
            }
            assert_eq!(sink.frames(), 12);
            let bytes = sink.finish().unwrap();
            // ftyp, then the index before the data.
            let at = |tag: &[u8]| bytes.windows(4).position(|window| window == tag);
            assert_eq!(at(b"ftyp"), Some(4));
            assert!(at(b"moov").unwrap() < at(b"mdat").unwrap(), "index first");
            let read = read_mp4(&bytes, 12).unwrap();
            assert_eq!(read.frames, 12, "every frame is there");
            assert_eq!((read.width, read.height), (66, 50), "padded to even");
            assert_eq!(read.profile, 100, "High profile");
            assert_eq!(read.sound, Some((48_000, 2)));
            assert!(
                (46_000..=51_000).contains(&read.sound_frames),
                "a second of sound, the rest cut: {}",
                read.sound_frames
            );
            // Each decoded frame matches its source within a small error.
            for (index, (decoded, source)) in read.pictures.iter().zip(&frames).enumerate() {
                let want = rgba_to_nv12(source).unwrap();
                assert_eq!(decoded.len(), want.bytes.len());
                let luma = 66 * 50;
                let error = decoded[..luma]
                    .iter()
                    .zip(&want.bytes[..luma])
                    .map(|(a, b)| f64::from(a.abs_diff(*b)))
                    .sum::<f64>()
                    / luma as f64;
                assert!(error < 4.0, "frame {index}: mean luma error {error:.2}");
            }
        }

        #[test]
        fn a_silent_movie_has_no_sound_track() {
            let mut sink = open(MovieFormat::Mp4H264, MovieSettings::new(64, 64, 24)).unwrap();
            assert_eq!(sink.push_sound(&[0.0, 0.0]), Err(MovieError::NoSoundTrack));
            for _ in 0..3 {
                sink.push_frame(&frame(64, 64, [200, 30, 30, 255])).unwrap();
            }
            let read = read_mp4(&sink.finish().unwrap(), 0).unwrap();
            assert_eq!((read.frames, read.sound), (3, None));
            let empty = open(MovieFormat::Mp4H264, MovieSettings::new(64, 64, 24)).unwrap();
            assert_eq!(empty.finish(), Err(MovieError::NoFrames));
        }

        /// A 1280 by 720 clip of `frames` frames: "flat" is flat colour
        /// shapes and black lines moving over paper, as cel animation
        /// is; "painted" is soft gradients with fine grain, as a painted
        /// picture is.
        fn clip(painted: bool, frames: u32) -> Vec<RasterTile2D> {
            let (width, height) = (1280_u32, 720_u32);
            (0..frames)
                .map(|step| {
                    let mut pixels = vec![0_u8; (width * height * 4) as usize];
                    for y in 0..height {
                        for x in 0..width {
                            let at = ((y * width + x) * 4) as usize;
                            let (cx, cy) = (200.0 + step as f32 * 12.0, 360.0);
                            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                            let inside = dx * dx + dy * dy < 150.0 * 150.0;
                            let edge = (dx * dx + dy * dy).sqrt();
                            let rgb = if painted {
                                // A pseudo random grain, the same each run.
                                let grain =
                                    ((x * 7919 + y * 104_729 + step * 31) % 23) as f32 - 11.0;
                                let r = 120.0 + 100.0 * (x as f32 / 1280.0) + grain;
                                let g = 80.0 + 120.0 * (y as f32 / 720.0) + grain;
                                let b = if inside { 220.0 - edge * 0.5 } else { 140.0 } + grain;
                                [r, g, b].map(|value| value.clamp(0.0, 255.0) as u8)
                            } else if (edge - 150.0).abs() < 3.0 || (x + step * 6) % 160 < 3 {
                                [20, 20, 20]
                            } else if inside {
                                [230, 120, 60]
                            } else {
                                [250, 248, 240]
                            };
                            pixels[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                        }
                    }
                    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap()
                })
                .collect()
        }

        /// Measures the three qualities on both clips and writes the
        /// movies to `SUE_MOVIE_OUT` when it is set:
        /// `cargo test -p sue-platform --lib measure_movie_qualities -- --ignored --nocapture`.
        #[test]
        #[ignore = "a measurement, it prints a table"]
        fn measure_movie_qualities() {
            let out = std::env::var_os("SUE_MOVIE_OUT").map(std::path::PathBuf::from);
            println!("| clip | quality | KiB | luma PSNR dB | worst frame dB |");
            println!("| --- | --- | --- | --- | --- |");
            for painted in [false, true] {
                let frames = clip(painted, 48);
                let name = if painted { "painted" } else { "flat" };
                for quality in MovieQuality::ALL {
                    let mut settings = MovieSettings::new(1280, 720, 24);
                    settings.quality = quality;
                    settings.sound = Some(MovieSound {
                        rate: 48_000,
                        channels: 2,
                    });
                    let mut sink = open(MovieFormat::Mp4H264, settings).unwrap();
                    let tone: Vec<f32> = (0..96_000)
                        .flat_map(|at| {
                            let value =
                                (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.3;
                            [value, value]
                        })
                        .collect();
                    sink.push_sound(&tone).unwrap();
                    for frame in &frames {
                        sink.push_frame(frame).unwrap();
                    }
                    let bytes = sink.finish().unwrap();
                    let read = read_mp4(&bytes, frames.len()).unwrap();
                    assert_eq!(read.frames, frames.len());
                    let luma = 1280 * 720;
                    let mut total = 0.0;
                    let mut worst = f64::MAX;
                    for (decoded, source) in read.pictures.iter().zip(&frames) {
                        let want = rgba_to_nv12(source).unwrap();
                        let mse = decoded[..luma]
                            .iter()
                            .zip(&want.bytes[..luma])
                            .map(|(a, b)| f64::from(a.abs_diff(*b)).powi(2))
                            .sum::<f64>()
                            / luma as f64;
                        let psnr = 10.0 * (255.0 * 255.0 / mse.max(1e-9)).log10();
                        total += psnr;
                        worst = worst.min(psnr);
                    }
                    println!(
                        "| {name} | {} | {} | {:.1} | {:.1} |",
                        quality.id(),
                        bytes.len() / 1024,
                        total / frames.len() as f64,
                        worst
                    );
                    if let Some(dir) = &out {
                        std::fs::create_dir_all(dir).unwrap();
                        std::fs::write(dir.join(format!("{name}-{}.mp4", quality.id())), &bytes)
                            .unwrap();
                        if quality == MovieQuality::Normal {
                            let png = sue_io::encode_png(&frames[24]).unwrap();
                            std::fs::write(dir.join(format!("{name}-source-24.png")), png).unwrap();
                        }
                    }
                }
            }
        }
    }
}
