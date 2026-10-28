//! The Player's background work, so the interface never waits: one thread
//! decodes the song about to play (and the one after it), another reads
//! the tags and length of every file being added to the library.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use sue_core::{PCMFloat1D, Shared};

use super::library::{Song, audio_under, song_from};

/// Most files one import reads.
const IMPORT_LIMIT: usize = 20_000;

/// Loudness the Player evens songs out to (RMS of the samples).
const TARGET_RMS: f32 = 0.18;

/// What the background threads finished.
#[derive(Debug)]
pub enum Done {
    /// A song decoded, with the gain that evens out its loudness.
    Decoded {
        /// Its file.
        path: PathBuf,
        /// Its sound.
        pcm: Shared<PCMFloat1D>,
        /// Linear gain for loudness evening.
        gain: f32,
    },
    /// A song that would not decode.
    Failed {
        /// Its file.
        path: PathBuf,
        /// Why.
        error: String,
    },
    /// A file read for the library.
    Read(Song),
    /// A file being added that is not audio Sue can read.
    Skipped(PathBuf),
    /// Everything one call to [`Worker::read`] asked for is read.
    Finished,
}

/// The gain that brings `pcm` to the target loudness without clipping.
#[must_use]
pub fn evening_gain(pcm: &PCMFloat1D) -> f32 {
    let samples = pcm.samples();
    if samples.is_empty() {
        return 1.0;
    }
    let (mut sum, mut peak) = (0.0_f64, 0.0_f32);
    for &sample in samples {
        sum += f64::from(sample * sample);
        peak = peak.max(sample.abs());
    }
    let rms = (sum / samples.len() as f64).sqrt() as f32;
    if rms <= 1e-5 {
        return 1.0;
    }
    (TARGET_RMS / rms)
        .min(0.98 / peak.max(1e-5))
        .clamp(0.25, 4.0)
}

/// The two background threads and what they send back.
#[derive(Debug, Default)]
pub struct Worker {
    decode: Option<Sender<PathBuf>>,
    read: Option<Sender<Vec<PathBuf>>>,
    done: Option<Receiver<Done>>,
    outbox: Option<Sender<Done>>,
}

impl Worker {
    fn outbox(&mut self) -> Sender<Done> {
        if let Some(outbox) = &self.outbox {
            return outbox.clone();
        }
        let (outbox, done) = channel();
        self.done = Some(done);
        self.outbox = Some(outbox.clone());
        outbox
    }

    /// Decodes `path` in the background.
    pub fn decode(&mut self, path: PathBuf) {
        if self.decode.is_none() {
            let (jobs, inbox) = channel::<PathBuf>();
            let outbox = self.outbox();
            let started = std::thread::Builder::new()
                .name("sue-player-decode".to_owned())
                .spawn(move || {
                    while let Ok(path) = inbox.recv() {
                        let extension = path
                            .extension()
                            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
                        let done = match std::fs::read(&path) {
                            Ok(bytes) => match sue_io::decode_audio(bytes, extension.as_deref()) {
                                Ok((decoded, _)) => {
                                    let gain = evening_gain(&decoded.pcm);
                                    Done::Decoded {
                                        path,
                                        pcm: Shared::new(decoded.pcm),
                                        gain,
                                    }
                                }
                                Err((error, _)) => Done::Failed { path, error },
                            },
                            Err(error) => Done::Failed {
                                path,
                                error: error.to_string(),
                            },
                        };
                        if outbox.send(done).is_err() {
                            return;
                        }
                    }
                });
            if started.is_ok() {
                self.decode = Some(jobs);
            }
        }
        if let Some(jobs) = &self.decode {
            let _ = jobs.send(path);
        }
    }

    /// Reads the tags of `paths` (files, or every audio file under
    /// folders) in the background, for the library.
    pub fn read(&mut self, paths: Vec<PathBuf>) {
        if self.read.is_none() {
            let (jobs, inbox) = channel::<Vec<PathBuf>>();
            let outbox = self.outbox();
            let started = std::thread::Builder::new()
                .name("sue-player-read".to_owned())
                .spawn(move || {
                    while let Ok(roots) = inbox.recv() {
                        let paths = roots
                            .iter()
                            .flat_map(|root| audio_under(root, IMPORT_LIMIT))
                            .collect::<Vec<_>>();
                        for path in paths {
                            let added = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .map_or(0, |since| since.as_secs());
                            let done = match sue_io::probe_audio(&path) {
                                Ok(info) => Done::Read(song_from(&path, &info, added)),
                                Err(_) => Done::Skipped(path),
                            };
                            if outbox.send(done).is_err() {
                                return;
                            }
                        }
                        if outbox.send(Done::Finished).is_err() {
                            return;
                        }
                    }
                });
            if started.is_ok() {
                self.read = Some(jobs);
            }
        }
        if let Some(jobs) = &self.read {
            let _ = jobs.send(paths);
        }
    }

    /// Everything finished since the last call.
    pub fn collect(&mut self) -> Vec<Done> {
        self.done
            .as_ref()
            .map(|done| done.try_iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_songs_are_lifted_and_loud_ones_never_clip() {
        let quiet = PCMFloat1D::new(8_000, 1, vec![0.05; 800]).unwrap();
        let gain = evening_gain(&quiet);
        assert!((gain - 3.6).abs() < 0.01, "{gain}");
        let loud = PCMFloat1D::new(8_000, 1, vec![0.9; 800]).unwrap();
        assert!(evening_gain(&loud) * 0.9 <= 0.98);
        let silent = PCMFloat1D::new(8_000, 1, vec![0.0; 800]).unwrap();
        assert_eq!(evening_gain(&silent), 1.0);
    }

    #[test]
    fn songs_decode_and_files_are_read_in_the_background() {
        let dir = std::env::temp_dir().join(format!("sue-player-work-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        let pcm = PCMFloat1D::new(8_000, 1, vec![0.1; 8_000]).unwrap();
        std::fs::write(&wav, sue_io::encode_wav(&pcm)).unwrap();
        let text = dir.join("notes.wav");
        std::fs::write(&text, b"not audio").unwrap();
        let mut worker = Worker::default();
        worker.decode(wav.clone());
        // A folder: every audio file under it is read.
        worker.read(vec![dir.clone()]);
        let mut done = Vec::new();
        let start = std::time::Instant::now();
        while !done.iter().any(|done| matches!(done, Done::Finished))
            && start.elapsed().as_secs() < 20
        {
            done.extend(worker.collect());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(done.iter().any(|done| matches!(
            done,
            Done::Decoded { path, pcm, .. } if *path == wav && pcm.frames() == 8_000
        )));
        assert!(done.iter().any(|done| matches!(
            done,
            Done::Read(song) if song.path == wav && (song.seconds - 1.0).abs() < 1e-6
        )));
        assert!(
            done.iter()
                .any(|done| matches!(done, Done::Skipped(path) if *path == text))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
