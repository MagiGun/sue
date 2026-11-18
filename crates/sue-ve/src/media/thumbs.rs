//! Thumbnails of movies (a strip of small pictures spread over the
//! movie, for the pool and the timeline's clips), made in the background,
//! and proxies (a smaller copy of a movie to edit with), made as a
//! background job with progress and Cancel.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use sue_core::{EntityId, JobContext, PixelFormat, RasterTile2D};

use super::frames::VideoFrame;

/// The height of a thumbnail, in pixels.
pub const THUMB_HEIGHT: u32 = 72;
/// The most thumbnails in a strip.
pub const MOST: usize = 40;

/// A movie's thumbnails, filled in as they decode.
#[derive(Debug, Default)]
pub struct Strip {
    /// Where each was taken, in seconds of the source.
    pub times: Vec<f64>,
    /// The pictures, in order (as many as decoded so far).
    pub frames: Vec<Arc<VideoFrame>>,
    /// Done (or given up).
    pub done: bool,
}

impl Strip {
    /// The thumbnail nearest `seconds` into the source.
    #[must_use]
    pub fn at(&self, seconds: f64) -> Option<&Arc<VideoFrame>> {
        let index = self
            .times
            .iter()
            .take(self.frames.len())
            .enumerate()
            .min_by(|a, b| {
                (a.1 - seconds)
                    .abs()
                    .partial_cmp(&(b.1 - seconds).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)?;
        self.frames.get(index)
    }
}

/// The thumbnails of every movie.
#[derive(Debug, Default)]
pub struct Thumbs {
    strips: HashMap<EntityId, Arc<Mutex<Strip>>>,
}

/// An even size `height` pixels high with the aspect of `size`.
#[must_use]
pub fn fit_height(size: (u32, u32), height: u32) -> (u32, u32) {
    let (width, tall) = (size.0.max(1), size.1.max(1));
    let across = (u64::from(width) * u64::from(height) / u64::from(tall)) as u32;
    ((across.max(2) + 1) & !1, height & !1)
}

impl Thumbs {
    /// Starts the strip of movie `id` (at `path`, `size`, `seconds`
    /// long) unless there is one.
    pub fn request(&mut self, id: EntityId, path: &Path, size: (u32, u32), seconds: f64) {
        if self.strips.contains_key(&id) {
            return;
        }
        let count = (seconds.ceil() as usize).clamp(1, MOST);
        let strip = Arc::new(Mutex::new(Strip {
            times: (0..count)
                .map(|at| seconds * (at as f64 + 0.5) / count as f64)
                .collect(),
            ..Strip::default()
        }));
        self.strips.insert(id, Arc::clone(&strip));
        let path = path.to_path_buf();
        let small = fit_height(size, THUMB_HEIGHT);
        let _ = std::thread::Builder::new()
            .name("sue-ve-thumbs".to_owned())
            .spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fill(&path, small, &strip);
                }));
                strip.lock().unwrap_or_else(PoisonError::into_inner).done = true;
            });
    }

    /// The strip of `id`, as far as it is.
    #[must_use]
    pub fn strip(&self, id: EntityId) -> Option<Arc<Mutex<Strip>>> {
        self.strips.get(&id).cloned()
    }

    /// `true` while a strip is still filling.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.strips
            .values()
            .any(|strip| !strip.lock().unwrap_or_else(PoisonError::into_inner).done)
    }
}

fn fill(path: &Path, size: (u32, u32), strip: &Mutex<Strip>) {
    let Ok(mut decoder) = sue_platform::video::VideoDecoder::open(path, Some(size), true) else {
        return;
    };
    let times = strip
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .times
        .clone();
    let mut rgba = Vec::new();
    for (index, seconds) in times.iter().enumerate() {
        let ticks = (seconds * 10_000_000.0) as i64;
        if decoder.seek(ticks).is_err() {
            break;
        }
        match decoder.next_frame(&mut rgba) {
            Ok(Some(_)) => {
                let (width, height) = decoder.size();
                let frame = Arc::new(VideoFrame::new(
                    width,
                    height,
                    std::mem::take(&mut rgba),
                    index as i64,
                ));
                strip
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .frames
                    .push(frame);
            }
            _ => break,
        }
    }
}

/// The size a proxy is made at: half the movie, at most 960 pixels
/// across, even.
#[must_use]
pub fn proxy_size(size: (u32, u32)) -> (u32, u32) {
    let (width, height) = (size.0.max(2), size.1.max(2));
    let scale = (960.0 / f64::from(width)).min(0.5);
    let even = |value: f64| ((value.round() as u32).max(34) + 1) & !1;
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// Makes a proxy of the movie at `from` (`size`, at `rate`) in `to`: H.264
/// at [`proxy_size`], without sound (the original's sound plays). Reports
/// progress through `job` and stops when it is cancelled.
///
/// # Errors
/// When the movie cannot be read or the proxy written, or on Cancel.
pub fn make_proxy(
    from: &Path,
    to: &Path,
    size: (u32, u32),
    rate: (u32, u32),
    seconds: f64,
    job: Option<&JobContext>,
) -> Result<(), String> {
    let small = proxy_size(size);
    let mut decoder = sue_platform::video::VideoDecoder::open(from, Some(small), true)
        .map_err(|error| error.to_string())?;
    let (width, height) = decoder.size();
    let mut settings = sue_io::MovieSettings::new(width, height, 30);
    settings.rate = rate;
    let mut sink = sue_platform::movie::open(sue_io::MovieFormat::Mp4H264, settings)
        .map_err(|error| error.text().to_owned())?;
    let mut rgba = Vec::new();
    let total = (seconds * f64::from(rate.0) / f64::from(rate.1.max(1))).max(1.0);
    let mut count = 0_u64;
    while let Some(_frame) = decoder
        .next_frame(&mut rgba)
        .map_err(|error| error.to_string())?
    {
        if job.is_some_and(JobContext::is_cancelled) {
            return Err(tr!("cancelled"));
        }
        let tile =
            RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, std::mem::take(&mut rgba))
                .map_err(|error| error.to_string())?;
        sink.push_frame(&tile)
            .map_err(|error| error.text().to_owned())?;
        count += 1;
        if let Some(job) = job {
            job.set_progress((count as f64 / total).min(0.99) as f32);
        }
    }
    let bytes = sink.finish().map_err(|error| error.text().to_owned())?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(to, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_keep_the_aspect_and_stay_even() {
        assert_eq!(fit_height((1920, 1080), 72), (128, 72));
        assert_eq!(fit_height((1080, 1920), 72), (40, 72));
        assert_eq!(proxy_size((1920, 1080)), (960, 540));
        assert_eq!(proxy_size((3840, 2160)), (960, 540));
        assert_eq!(proxy_size((640, 480)), (320, 240));
    }
}
