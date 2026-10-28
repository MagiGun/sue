//! Picture thumbnails for the File Manager's cards. A background thread
//! reads and decodes each picture (PNG, JPEG, JPEG XL from its 1:8 preview)
//! and shrinks it to fit a small square, so the interface never waits for a
//! decode. Results are kept by path and modification time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use sue_core::PixelFormat;
use sue_io::{JxlPipeline, NativeJxlPipeline, ProgressiveTarget, decode_jpeg, decode_png};

/// The longest side of a thumbnail, in pixels.
pub const SIDE: u32 = 192;

/// Pictures larger than this are not read for a thumbnail.
const MAX_BYTES: u64 = 96 * 1024 * 1024;

/// A shrunk picture: RGBA8 rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumb {
    /// Width and height in pixels.
    pub size: [u32; 2],
    /// RGBA8 pixels.
    pub pixels: Vec<u8>,
    /// Changes when the file does, for the texture cache.
    pub version: u64,
}

#[derive(Debug)]
enum State {
    Pending,
    Ready(Thumb),
    Failed,
}

type Job = (PathBuf, u64);
type Done = (PathBuf, u64, Option<Thumb>);

/// The thumbnail cache and its worker.
#[derive(Debug, Default)]
pub struct Thumbs {
    jobs: Option<Sender<Job>>,
    done: Option<Receiver<Done>>,
    cache: HashMap<PathBuf, (u64, State)>,
}

/// `true` for files the worker can make a thumbnail of.
#[must_use]
pub fn is_picture(path: &Path) -> bool {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "jxl")
}

/// A version number for a modification time.
#[must_use]
pub fn version_of(modified: Option<SystemTime>, size: u64) -> u64 {
    let seconds = modified
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as u64);
    seconds ^ size.rotate_left(32)
}

impl Thumbs {
    /// The thumbnail of `path`, asking the worker for it the first time.
    pub fn get(&mut self, path: &Path, version: u64, size: u64) -> Option<&Thumb> {
        let stale = self
            .cache
            .get(path)
            .is_none_or(|(cached, _)| *cached != version);
        if stale {
            let state = if size > MAX_BYTES || !self.send((path.to_path_buf(), version)) {
                State::Failed
            } else {
                State::Pending
            };
            self.cache.insert(path.to_path_buf(), (version, state));
        }
        match self.cache.get(path) {
            Some((_, State::Ready(thumb))) => Some(thumb),
            _ => None,
        }
    }

    /// `true` while the worker still has pictures to finish.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.cache
            .values()
            .any(|(_, state)| matches!(state, State::Pending))
    }

    /// Takes in what the worker finished. Returns `true` if anything did.
    pub fn collect(&mut self) -> bool {
        let Some(done) = self.done.as_ref() else {
            return false;
        };
        let mut any = false;
        while let Ok((path, version, thumb)) = done.try_recv() {
            any = true;
            if let Some((cached, state)) = self.cache.get_mut(&path)
                && *cached == version
            {
                *state = thumb.map_or(State::Failed, State::Ready);
            }
        }
        any
    }

    fn send(&mut self, job: Job) -> bool {
        if self.jobs.is_none() {
            let (jobs, inbox) = channel::<Job>();
            let (outbox, done) = channel::<Done>();
            let started = std::thread::Builder::new()
                .name("sue-thumbnails".to_owned())
                .spawn(move || {
                    // Newest requests first: the folder just opened matters
                    // more than one scrolled past.
                    while let Ok(first) = inbox.recv() {
                        let mut queue = vec![first];
                        queue.extend(inbox.try_iter());
                        while let Some((path, version)) = queue.pop() {
                            let thumb = make(&path);
                            if outbox.send((path, version, thumb)).is_err() {
                                return;
                            }
                            queue.extend(inbox.try_iter());
                        }
                    }
                });
            if started.is_err() {
                return false;
            }
            self.jobs = Some(jobs);
            self.done = Some(done);
        }
        self.jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok())
    }
}

/// Reads, decodes and shrinks one picture.
fn make(path: &Path) -> Option<Thumb> {
    let bytes = std::fs::read(path).ok()?;
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let tile = match extension.as_str() {
        "png" => decode_png(&bytes).ok()?.tile,
        "jpg" | "jpeg" => decode_jpeg(&bytes).ok()?.tile,
        "jxl" => {
            NativeJxlPipeline
                .decode(&bytes, ProgressiveTarget::DcPreview)
                .ok()?
                .tile
        }
        _ => return None,
    };
    let (width, height) = (tile.width(), tile.height());
    let rgba: Vec<u8> = match tile.format() {
        PixelFormat::Rgba8 => tile.bytes()?.to_vec(),
        PixelFormat::Gray8 => tile
            .bytes()?
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        PixelFormat::RgbaF32 => return None,
    };
    Some(shrink(&rgba, [width, height], SIDE))
}

/// Box-filters RGBA8 `pixels` of `size` to fit in `side` by `side`.
#[must_use]
pub fn shrink(pixels: &[u8], [width, height]: [u32; 2], side: u32) -> Thumb {
    let scale = (side as f32 / width.max(height).max(1) as f32).min(1.0);
    let out = [
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    ];
    let mut result = Vec::with_capacity(out[0] as usize * out[1] as usize * 4);
    for y in 0..out[1] {
        let (y0, y1) = span(y, out[1], height);
        for x in 0..out[0] {
            let (x0, x1) = span(x, out[0], width);
            let mut sum = [0_u64; 4];
            for row in y0..y1 {
                for column in x0..x1 {
                    let at = (row as usize * width as usize + column as usize) * 4;
                    for (total, &value) in sum.iter_mut().zip(&pixels[at..at + 4]) {
                        *total += u64::from(value);
                    }
                }
            }
            let count = u64::from((y1 - y0) * (x1 - x0)).max(1);
            result.extend(sum.map(|total| (total / count) as u8));
        }
    }
    Thumb {
        size: out,
        pixels: result,
        version: 0,
    }
}

/// The source pixels `index` of `count` output pixels covers.
fn span(index: u32, count: u32, source: u32) -> (u32, u32) {
    let start = u64::from(index) * u64::from(source) / u64::from(count);
    let end = (u64::from(index + 1) * u64::from(source) / u64::from(count)).max(start + 1);
    (start as u32, (end as u32).min(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_shrink_to_fit_and_average_their_pixels() {
        // A 4 by 2 picture: left half black, right half white.
        let mut pixels = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let value = if x < 2 { 0 } else { 255 };
                pixels.extend([value, value, value, 255]);
            }
        }
        let thumb = shrink(&pixels, [4, 2], 2);
        assert_eq!(thumb.size, [2, 1]);
        assert_eq!(thumb.pixels, [0, 0, 0, 255, 255, 255, 255, 255]);
        // Small pictures are never enlarged.
        assert_eq!(shrink(&pixels, [4, 2], 64).size, [4, 2]);
    }

    #[test]
    fn the_worker_makes_thumbnails_in_the_background() {
        let dir = std::env::temp_dir().join(format!("sue-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("card.png");
        let pixels = [200_u8, 10, 10, 255].repeat(300 * 100);
        std::fs::write(&path, sue_io::encode_png_rgba(300, 100, &pixels).unwrap()).unwrap();
        assert!(is_picture(&path));
        let mut thumbs = Thumbs::default();
        assert!(thumbs.get(&path, 1, 10).is_none(), "not made yet");
        assert!(thumbs.pending());
        let start = std::time::Instant::now();
        while thumbs.pending() && start.elapsed().as_secs() < 20 {
            thumbs.collect();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let thumb = thumbs.get(&path, 1, 10).expect("made");
        assert_eq!(thumb.size, [SIDE, 64]);
        assert_eq!(&thumb.pixels[..4], &[200, 10, 10, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
