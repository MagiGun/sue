//! Decoded sound, kept by source, and its waveform.
//!
//! A source's sound is decoded once, in the background, to the mixing
//! rate in stereo: sound files through `sue-io` (Symphonia, pure Rust:
//! WAV, FLAC, MP3, AAC, Vorbis), a movie's sound through the system's
//! decoder (`sue_platform::video`), each falling back to the other. The
//! waveform (the loudest and softest sample of every hundredth of a
//! second) is made from it on the same thread.
//!
//! Decoded sound is derived data, like a thumbnail: it is not in the
//! arena and is not saved. Stage 1 keeps it in memory (about 23 MB a
//! minute); streaming it from disk is a known gap (`docs/sve-handover.md`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use sue_core::{EntityId, PCMFloat1D};

/// The mixing rate: every source is decoded to it.
pub const MIX_RATE: u32 = 48_000;
/// Sample frames per waveform point: a hundredth of a second.
pub const PEAK_STEP: usize = 480;

/// One source's sound, decoded.
#[derive(Debug)]
pub struct Sound {
    /// Interleaved stereo at [`MIX_RATE`].
    pub samples: Vec<f32>,
    /// The softest and loudest sample of every [`PEAK_STEP`] frames.
    pub peaks: Vec<[f32; 2]>,
}

impl Sound {
    /// Sound from interleaved stereo samples at [`MIX_RATE`].
    #[must_use]
    pub fn new(samples: Vec<f32>) -> Self {
        let peaks = samples
            .chunks(PEAK_STEP * 2)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold([0.0_f32, 0.0_f32], |[low, high], sample| {
                        [low.min(*sample), high.max(*sample)]
                    })
            })
            .collect();
        Self { samples, peaks }
    }

    /// Frames held.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// The stereo frame at `position` (frames, fractions between them
    /// read linearly); silence outside.
    #[must_use]
    pub fn at(&self, position: f64) -> [f32; 2] {
        if position < 0.0 {
            return [0.0, 0.0];
        }
        let index = position.floor() as usize;
        let t = (position - position.floor()) as f32;
        let get = |at: usize| -> [f32; 2] {
            self.samples
                .get(at * 2..at * 2 + 2)
                .map_or([0.0, 0.0], |pair| [pair[0], pair[1]])
        };
        let (a, b) = (get(index), get(index + 1));
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}

/// Any sound as interleaved stereo at [`MIX_RATE`].
fn to_mix(pcm: &PCMFloat1D) -> Result<Vec<f32>, String> {
    let options = sue_io::SoundOptions {
        sample_rate: Some(MIX_RATE),
        channels: Some(2),
        depth: sue_io::SampleDepth::Float32,
        ..sue_io::SoundOptions::default()
    };
    if pcm.sample_rate() == MIX_RATE && pcm.channels() == 2 {
        return Ok(pcm.samples().to_vec());
    }
    sue_io::transform_sound(pcm, &options).map(|pcm| pcm.samples().to_vec())
}

/// Files read whole for Symphonia at most: larger ones go to the
/// system's decoder, which streams them.
const READ_WHOLE: u64 = 512 * 1024 * 1024;

fn through_symphonia(path: &Path) -> Result<Vec<f32>, String> {
    let length = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len();
    if length > READ_WHOLE {
        return Err("too large to read at once".to_owned());
    }
    let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
    let extension = path.extension().and_then(|ext| ext.to_str());
    let (decoded, _) = sue_io::decode_audio(bytes, extension).map_err(|(why, _)| why)?;
    to_mix(&decoded.pcm)
}

fn through_the_system(path: &Path) -> Result<Vec<f32>, String> {
    let mut decoder = sue_platform::video::SoundDecoder::open(path, MIX_RATE, 2)
        .map_err(|error| error.to_string())?;
    let (rate, channels) = decoder.format();
    let mut samples = Vec::new();
    while decoder
        .next(&mut samples)
        .map_err(|error| error.to_string())?
        .is_some()
    {}
    if rate == MIX_RATE && channels == 2 {
        return Ok(samples);
    }
    let pcm = PCMFloat1D::new(rate, channels, samples).map_err(|error| error.to_string())?;
    to_mix(&pcm)
}

/// Decodes the sound of `path`. `movie` says whether it is a movie (the
/// system's decoder first) or a sound file (Symphonia first).
///
/// # Errors
/// When neither decoder reads it.
pub fn decode(path: &Path, movie: bool) -> Result<Sound, String> {
    let samples = if movie {
        through_the_system(path).or_else(|first| through_symphonia(path).map_err(|_| first))
    } else {
        through_symphonia(path).or_else(|first| through_the_system(path).map_err(|_| first))
    }?;
    Ok(Sound::new(samples))
}

enum State {
    Loading(Receiver<Result<Sound, String>>),
    Ready(Arc<Sound>),
    Failed(String),
}

/// Every source's sound: decoding in the background, then ready.
#[derive(Default)]
pub struct Sounds {
    states: HashMap<EntityId, State>,
}

impl std::fmt::Debug for Sounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sounds")
            .field("sources", &self.states.len())
            .finish()
    }
}

impl Sounds {
    /// Starts decoding `id` from `path` unless it is under way or done.
    pub fn request(&mut self, id: EntityId, path: PathBuf, movie: bool) {
        if self.states.contains_key(&id) {
            return;
        }
        let (to, from) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("sue-ve-sound".to_owned())
            .spawn(move || {
                let decoded = std::panic::catch_unwind(|| decode(&path, movie))
                    .unwrap_or_else(|_| Err("the sound decoder stopped".to_owned()));
                let _ = to.send(decoded);
            });
        let state = match spawned {
            Ok(_) => State::Loading(from),
            Err(error) => State::Failed(error.to_string()),
        };
        self.states.insert(id, state);
    }

    /// Puts sound in place directly (tests, generated sound).
    pub fn insert(&mut self, id: EntityId, sound: Sound) {
        self.states.insert(id, State::Ready(Arc::new(sound)));
    }

    /// Takes in what finished decoding. Returns `true` when something
    /// did.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for state in self.states.values_mut() {
            if let State::Loading(from) = state {
                match from.try_recv() {
                    Ok(Ok(sound)) => {
                        *state = State::Ready(Arc::new(sound));
                        changed = true;
                    }
                    Ok(Err(why)) => {
                        *state = State::Failed(why);
                        changed = true;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        *state = State::Failed("the sound decoder stopped".to_owned());
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// `true` while anything decodes.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.states
            .values()
            .any(|state| matches!(state, State::Loading(_)))
    }

    /// The sound of `id`, once decoded.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<Arc<Sound>> {
        match self.states.get(&id) {
            Some(State::Ready(sound)) => Some(Arc::clone(sound)),
            _ => None,
        }
    }

    /// Why `id`'s sound failed, if it did.
    #[must_use]
    pub fn error(&self, id: EntityId) -> Option<&str> {
        match self.states.get(&id) {
            Some(State::Failed(why)) => Some(why),
            _ => None,
        }
    }

    /// Forgets `id` (its file changed).
    pub fn forget(&mut self, id: EntityId) {
        self.states.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_and_reading_between_samples() {
        let samples: Vec<f32> = (0..PEAK_STEP * 3)
            .flat_map(|at| {
                let v = if at < PEAK_STEP { 0.5 } else { -0.25 };
                [v, v]
            })
            .collect();
        let sound = Sound::new(samples);
        assert_eq!(sound.frames(), PEAK_STEP * 3);
        assert_eq!(sound.peaks.len(), 3);
        assert_eq!(sound.peaks[0], [0.0, 0.5]);
        assert_eq!(sound.peaks[1], [-0.25, 0.0]);
        assert_eq!(sound.at(-1.0), [0.0, 0.0]);
        let between = sound.at(PEAK_STEP as f64 - 0.5);
        assert!((between[0] - 0.125).abs() < 1e-6);
        assert_eq!(sound.at(1e9), [0.0, 0.0]);
    }

    #[test]
    fn a_wav_file_decodes_to_the_mixing_rate() {
        let dir = std::env::temp_dir().join("sue-ve-sound-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("tone-{}.wav", std::process::id()));
        let tone: Vec<f32> = (0..22_050)
            .map(|at| (at as f32 * 440.0 * std::f32::consts::TAU / 22_050.0).sin() * 0.5)
            .collect();
        let pcm = PCMFloat1D::new(22_050, 1, tone).unwrap();
        std::fs::write(&path, sue_io::encode_wav(&pcm)).unwrap();
        let sound = decode(&path, false).unwrap();
        assert!(
            (sound.frames() as i64 - 48_000).abs() < 100,
            "{}",
            sound.frames()
        );
        let peak = sound.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.05, "{peak}");
        let _ = std::fs::remove_file(&path);
    }
}
