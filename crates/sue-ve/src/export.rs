//! Export: a sequence to an MP4 (H.264 and AAC through the system's
//! encoder), a WAV of its sound, one frame as PNG or JPEG, or every
//! frame as a numbered run of PNG files.
//!
//! It runs off the interface's thread, on a snapshot of the arena (a
//! clone, which copies no pixels: rule 5), with decoders of its own that
//! read each movie straight through, and draws every frame with the same
//! layers the program monitor shows ([`crate::render`]). It reports its
//! progress and stops when cancelled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use sue_core::{Arena, EntityId, JobContext, PCMFloat1D, PixelFormat, RasterTile2D, Rate, Time};
use sue_model::Sequence;

use crate::compose::{Composer, Options};
use crate::media::frames::{Movie, Variant, VideoFrame};
use crate::media::sound::{MIX_RATE, Sound};
use crate::media::{Quality, import, sound, ticks};
use crate::model::{self, SourceKind};
use crate::render::{self, Layer};

/// What an export writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// An MP4 movie: H.264 pictures, AAC sound.
    Mp4,
    /// The sound only, as WAV.
    Wav,
    /// The frame at the start of the range, as PNG.
    Png,
    /// The frame at the start of the range, as JPEG.
    Jpeg,
    /// Every frame, as numbered PNG files.
    PngSequence,
}

impl Target {
    /// Every target, as the Deliver page offers them.
    pub const ALL: [Self; 5] = [
        Self::Mp4,
        Self::Wav,
        Self::Png,
        Self::Jpeg,
        Self::PngSequence,
    ];

    /// The file extension.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Wav => "wav",
            Self::Png | Self::PngSequence => "png",
            Self::Jpeg => "jpg",
        }
    }

    /// Its name as people read it.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Mp4 => tr!("target-mp4"),
            Self::Wav => tr!("target-wav"),
            Self::Png => tr!("target-png"),
            Self::Jpeg => tr!("target-jpeg"),
            Self::PngSequence => tr!("target-png-sequence"),
        }
    }
}

/// How an export is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// What is written.
    pub target: Target,
    /// The frame size.
    pub size: (u32, u32),
    /// Frames a second.
    pub rate: Rate,
    /// From where to where on the sequence.
    pub range: (Time, Time),
    /// The MP4's bit rate in bits a second (`None`: by quality).
    pub bitrate: Option<u32>,
    /// The MP4's quality when no bit rate is set.
    pub quality: sue_io::MovieQuality,
    /// The MP4 carries the sound.
    pub sound: bool,
}

impl Settings {
    /// An MP4 of the whole of `sequence` at its own size and rate.
    #[must_use]
    pub fn whole(sequence: &Sequence) -> Self {
        let (mark_in, mark_out) = model::marks(sequence);
        let end = sequence.duration();
        let range = match (mark_in, mark_out) {
            (Some(start), Some(stop)) if stop > start => (start, stop),
            (Some(start), None) if start < end => (start, end),
            _ => (Time::ZERO, end),
        };
        Self {
            target: Target::Mp4,
            size: model::frame_size(sequence),
            rate: sequence.rate,
            range,
            bitrate: None,
            quality: sue_io::MovieQuality::High,
            sound: true,
        }
    }

    /// How many frames it writes.
    #[must_use]
    pub fn frames(&self) -> u64 {
        match self.target {
            Target::Png | Target::Jpeg => 1,
            _ => {
                let seconds = (self.range.1 - self.range.0).seconds().max(0.0);
                (seconds * self.rate.as_f64()).round().max(1.0) as u64
            }
        }
    }

    /// When frame `index` shows on the sequence.
    #[must_use]
    pub fn time_of(&self, index: u64) -> Time {
        self.range.0 + Time::from_frames(index as i64, self.rate)
    }
}

/// What an export did.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Frames drawn.
    pub frames: u64,
    /// Seconds it took.
    pub seconds: f64,
    /// Bytes written.
    pub bytes: u64,
}

/// A movie read straight through for an export.
struct Reader {
    decoder: sue_platform::video::VideoDecoder,
    movie: Movie,
    last: Option<Arc<VideoFrame>>,
}

impl Reader {
    fn frame(&mut self, index: i64) -> Option<Arc<VideoFrame>> {
        if let Some(last) = &self.last
            && last.index == index
        {
            return Some(Arc::clone(last));
        }
        let close = self
            .last
            .as_ref()
            .is_some_and(|last| last.index < index && index - last.index <= 30);
        let mut rgba = Vec::new();
        if close {
            while let Ok(Some(frame)) = self.decoder.next_frame(&mut rgba) {
                let at = self.movie.index_at(frame.time);
                let (width, height) = self.decoder.size();
                let picture = Arc::new(VideoFrame::new(
                    width,
                    height,
                    std::mem::take(&mut rgba),
                    at,
                ));
                self.last = Some(Arc::clone(&picture));
                if at >= index {
                    return Some(picture);
                }
            }
            return self.last.clone();
        }
        let time = self.movie.ticks_of(index);
        self.decoder.seek(time).ok()?;
        let frame = self.decoder.frame_at(time, &mut rgba).ok()??;
        let (width, height) = self.decoder.size();
        let picture = Arc::new(VideoFrame::new(
            width,
            height,
            rgba,
            self.movie.index_at(frame.time),
        ));
        self.last = Some(Arc::clone(&picture));
        Some(picture)
    }
}

/// The sources of one renderer: decoders reading movies straight
/// through, and the pictures drawn from the arena.
struct Sources {
    readers: HashMap<EntityId, Option<Reader>>,
    generated: HashMap<(EntityId, i64), Arc<VideoFrame>>,
    hardware: bool,
}

impl Sources {
    fn reader(
        &mut self,
        arena: &Arena,
        sequence: &Sequence,
        size: (u32, u32),
        id: EntityId,
    ) -> Option<&mut Reader> {
        if !self.readers.contains_key(&id) {
            let hardware = self.hardware;
            let reader = (|| {
                let entity = arena.get_by_id(id).ok()?;
                let natural = model::size_of(arena, id)?;
                let rate = model::rate_of(entity)?;
                let path = import::file_of(entity)?;
                // Decoded no larger than the output needs, unless a clip of
                // it is scaled up.
                let frame = model::frame_size(sequence);
                let output = f64::from(size.0) / f64::from(frame.0.max(1));
                let fit = (f64::from(frame.0) / f64::from(natural.0))
                    .min(f64::from(frame.1) / f64::from(natural.1))
                    * output;
                let grown = sequence
                    .tracks
                    .iter()
                    .flat_map(|track| track.clips())
                    .filter(|clip| clip.source == id)
                    .any(|clip| clip.value_at(model::key::SCALE, clip.start, 1.0) > 1.0)
                    || clip_keys_scale(sequence, id);
                let scale = if grown { output.max(fit) } else { fit }.min(1.0);
                let even = |value: f64| ((value.round() as u32).max(34) + 1) & !1;
                let decode = (
                    even(f64::from(natural.0) * scale),
                    even(f64::from(natural.1) * scale),
                );
                let decoder =
                    sue_platform::video::VideoDecoder::open(&path, Some(decode), hardware).ok()?;
                let movie = Movie {
                    id,
                    path,
                    rate: (rate.numerator(), rate.denominator()),
                    size: decoder.size(),
                    variant: Variant {
                        size: 0,
                        proxy: false,
                    },
                    hardware,
                };
                Some(Reader {
                    decoder,
                    movie,
                    last: None,
                })
            })();
            self.readers.insert(id, reader);
        }
        self.readers.get_mut(&id).and_then(Option::as_mut)
    }

    /// The pixels of `layer`'s source.
    fn picture(
        &mut self,
        arena: &Arena,
        sequence: &Sequence,
        size: (u32, u32),
        layer: &Layer,
    ) -> Option<Arc<VideoFrame>> {
        match layer.kind {
            SourceKind::Movie => {
                self.reader(arena, sequence, size, layer.source)
                    .and_then(|reader| {
                        let index = reader.movie.index_at(ticks(layer.source_time)).max(0);
                        reader.frame(index)
                    })
            }
            _ => {
                let index = match layer.kind {
                    SourceKind::Picture => sue_picture::anim::timeline(arena, layer.source)
                        .map_or(0, |timeline| layer.source_time.frame(timeline.rate)),
                    SourceKind::ImageSequence => arena
                        .get_by_id(layer.source)
                        .ok()
                        .and_then(model::rate_of)
                        .map_or(0, |rate| layer.source_time.frame(rate)),
                    _ => 0,
                };
                let key = (layer.source, index);
                if !self.generated.contains_key(&key) {
                    let made = if layer.kind == SourceKind::ImageSequence {
                        arena
                            .get_by_id(layer.source)
                            .ok()
                            .and_then(|entity| import::sequence_frame(entity, index))
                            .and_then(|path| sue_platform::fs::read(path).ok())
                            .and_then(|bytes| import::read_picture(&bytes).ok())
                            .and_then(|tile| {
                                render::rgba_of(&tile).map(|rgba| {
                                    VideoFrame::new(tile.width(), tile.height(), rgba, index)
                                })
                            })
                    } else {
                        let unit = size.0 as f32 / model::frame_size(sequence).0.max(1) as f32;
                        render::generated(
                            arena,
                            layer.source,
                            &layer.kind,
                            layer.source_time,
                            size,
                            unit,
                        )
                    };
                    if let Some(made) = made {
                        // Stills and titles stay; frames of animations and
                        // image runs are kept a few at a time.
                        if self.generated.len() > 64 {
                            self.generated.clear();
                        }
                        self.generated.insert(key, Arc::new(made));
                    }
                }
                self.generated.get(&key).cloned()
            }
        }
    }
}

/// Draws the frames and mixes the sound of one sequence, on whatever
/// thread it runs on: the export's, and the program monitor's when the
/// frames need the processor (the same drawing, so the preview is the
/// export).
pub struct Renderer {
    arena: Arena,
    sequence: Sequence,
    size: (u32, u32),
    sources: Sources,
    composer: Composer,
    sounds: HashMap<EntityId, Option<Arc<Sound>>>,
    mixer: crate::mix::Mixer,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// A renderer of sequence `id` in `arena` (a snapshot) at `size`.
    ///
    /// # Errors
    /// When `id` holds no sequence.
    pub fn new(
        arena: Arena,
        id: EntityId,
        size: (u32, u32),
        hardware: bool,
    ) -> Result<Self, String> {
        let sequence = model::sequence(&arena, id)
            .ok_or_else(|| tr!("edit-no-sequence"))?
            .clone();
        Ok(Self {
            arena,
            sequence,
            size,
            sources: Sources {
                readers: HashMap::new(),
                generated: HashMap::new(),
                hardware,
            },
            composer: Composer::default(),
            sounds: HashMap::new(),
            mixer: crate::mix::Mixer::default(),
        })
    }

    /// The sequence it draws.
    #[must_use]
    pub fn sequence(&self) -> &Sequence {
        &self.sequence
    }

    /// The arena it reads.
    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// The picture at sequence time `time`, opaque RGBA at the size.
    pub fn frame(&mut self, time: Time) -> Vec<u8> {
        self.frame_with(time, Options::default())
    }

    /// The picture at sequence time `time` drawn with `options`.
    pub fn frame_with(&mut self, time: Time, options: Options) -> Vec<u8> {
        let Self {
            arena,
            sequence,
            size,
            sources,
            composer,
            ..
        } = self;
        let size = *size;
        let (arena, sequence) = (&*arena, &*sequence);
        crate::compose::frame(
            arena,
            sequence,
            time,
            size,
            options,
            &mut |layer: &Layer| sources.picture(arena, sequence, size, layer),
            composer,
        )
    }

    fn sound(&mut self, id: EntityId) -> Option<Arc<Sound>> {
        if !self.sounds.contains_key(&id) {
            let decoded = self.arena.get_by_id(id).ok().and_then(|entity| {
                let movie = SourceKind::of(entity) == SourceKind::Movie;
                let path = import::file_of(entity)?;
                sound::decode(&path, movie).ok().map(Arc::new)
            });
            self.sounds.insert(id, decoded);
        }
        self.sounds.get(&id).cloned().flatten()
    }

    /// Decodes the sound of every source the sequence plays.
    pub fn load_sounds(&mut self) {
        let sources: Vec<EntityId> = self
            .sequence
            .tracks
            .iter()
            .filter(|track| !model::is_video(track))
            .flat_map(|track| track.clips().iter().map(|clip| clip.source))
            .collect();
        for id in sources {
            let _ = self.sound(id);
        }
    }

    /// Mixes `frames` frames of sound from sequence frame `first` (at the
    /// mixing rate), with the tracks' effects carried on from the block
    /// before when it follows on.
    #[must_use]
    pub fn sound_block(&mut self, first: i64, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        let sounds = &self.sounds;
        self.mixer
            .mix(&self.sequence, first, MIX_RATE, &mut out, &|id| {
                sounds.get(&id).cloned().flatten()
            });
        out
    }
}

fn clip_keys_scale(sequence: &Sequence, id: EntityId) -> bool {
    sequence
        .tracks
        .iter()
        .flat_map(|track| track.clips())
        .filter(|clip| clip.source == id)
        .filter_map(|clip| clip.animation.channel(model::key::SCALE))
        .any(|curve| curve.keys().iter().any(|key| key.value > 1.0))
}

fn tile(size: (u32, u32), rgba: Vec<u8>) -> Result<RasterTile2D, String> {
    RasterTile2D::from_bytes(size.0, size.1, PixelFormat::Rgba8, rgba)
        .map_err(|error| error.to_string())
}

/// The file frame `index` of a numbered run goes to, beside `path`.
#[must_use]
pub fn numbered(path: &Path, index: u64) -> PathBuf {
    let stem = path.file_stem().map_or_else(
        || "frame".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    path.with_file_name(format!("{stem}_{:05}.png", index + 1))
}

/// Exports sequence `id` of `arena` (a snapshot) to `path` as `settings`
/// say, reporting to `job` (progress, Cancel) when there is one.
///
/// # Errors
/// What the encoder or the file system said, or that it was cancelled.
pub fn run(
    arena: Arena,
    id: EntityId,
    settings: Settings,
    path: &Path,
    hardware: bool,
    job: Option<&JobContext>,
) -> Result<Report, String> {
    let started = Instant::now();
    let mut renderer = Renderer::new(arena, id, settings.size, hardware)?;
    let cancelled = || job.is_some_and(JobContext::is_cancelled);
    let progress = |done: f64| {
        if let Some(job) = job {
            job.set_progress(done.clamp(0.0, 0.99) as f32);
        }
    };
    let frames = settings.frames();
    let mut bytes = 0_u64;
    match settings.target {
        Target::Mp4 => {
            let mut movie = sue_io::MovieSettings::new(settings.size.0, settings.size.1, 30);
            movie.rate = (settings.rate.numerator(), settings.rate.denominator());
            movie.quality = settings.quality;
            movie.bitrate = settings.bitrate;
            if settings.sound {
                renderer.load_sounds();
                movie.sound = Some(sue_io::MovieSound {
                    rate: MIX_RATE,
                    channels: 2,
                });
            }
            let mut sink = sue_platform::movie::open(sue_io::MovieFormat::Mp4H264, movie)
                .map_err(|error| error.text().to_owned())?;
            let first_sample = (settings.range.0.seconds() * f64::from(MIX_RATE)).round() as i64;
            let mut sound_done = 0_i64;
            for index in 0..frames {
                if cancelled() {
                    return Err(tr!("cancelled"));
                }
                let time = settings.time_of(index);
                let picture = renderer.frame(time);
                sink.push_frame(&tile(settings.size, picture)?)
                    .map_err(|error| error.text().to_owned())?;
                if settings.sound {
                    let until = (Time::from_frames(index as i64 + 1, settings.rate).seconds()
                        * f64::from(MIX_RATE))
                    .round() as i64;
                    let block = renderer.sound_block(
                        first_sample + sound_done,
                        (until - sound_done).max(0) as usize,
                    );
                    let clamped: Vec<f32> = block.iter().map(|s| s.clamp(-1.0, 1.0)).collect();
                    sink.push_sound(&clamped)
                        .map_err(|error| error.text().to_owned())?;
                    sound_done = until;
                }
                progress((index + 1) as f64 / frames as f64);
            }
            let written = sink.finish().map_err(|error| error.text().to_owned())?;
            bytes = written.len() as u64;
            sue_platform::fs::write(path, written).map_err(|error| error.to_string())?;
        }
        Target::Wav => {
            renderer.load_sounds();
            let first = (settings.range.0.seconds() * f64::from(MIX_RATE)).round() as i64;
            let count = ((settings.range.1 - settings.range.0).seconds() * f64::from(MIX_RATE))
                .round()
                .max(0.0) as usize;
            let mut samples = Vec::with_capacity(count * 2);
            let step = MIX_RATE as usize;
            let mut done = 0;
            while done < count {
                if cancelled() {
                    return Err(tr!("cancelled"));
                }
                let take = step.min(count - done);
                samples.extend(renderer.sound_block(first + done as i64, take));
                done += take;
                progress(done as f64 / count.max(1) as f64);
            }
            let pcm = PCMFloat1D::new(MIX_RATE, 2, samples).map_err(|error| error.to_string())?;
            let written = sue_io::encode_wav_as(&pcm, sue_io::SampleDepth::Int24);
            bytes = written.len() as u64;
            sue_platform::fs::write(path, written).map_err(|error| error.to_string())?;
        }
        Target::Png | Target::Jpeg => {
            let picture = tile(settings.size, renderer.frame(settings.range.0))?;
            let written = if settings.target == Target::Png {
                sue_io::encode_png(&picture)?
            } else {
                sue_io::encode_jpeg(&picture, 92)?
            };
            bytes = written.len() as u64;
            sue_platform::fs::write(path, written).map_err(|error| error.to_string())?;
        }
        Target::PngSequence => {
            for index in 0..frames {
                if cancelled() {
                    return Err(tr!("cancelled"));
                }
                let picture = tile(settings.size, renderer.frame(settings.time_of(index)))?;
                let written = sue_io::encode_png(&picture)?;
                bytes += written.len() as u64;
                sue_platform::fs::write(numbered(path, index), written)
                    .map_err(|error| error.to_string())?;
                progress((index + 1) as f64 / frames as f64);
            }
        }
    }
    Ok(Report {
        frames,
        seconds: started.elapsed().as_secs_f64(),
        bytes,
    })
}

/// The quality the monitors decode at for a sequence of `size` by
/// default: full up to 1080p, half above.
#[must_use]
pub fn default_quality(size: (u32, u32)) -> Quality {
    if size.0.max(size.1) > 1920 {
        Quality::Half
    } else {
        Quality::Full
    }
}

#[cfg(test)]
mod tests;
