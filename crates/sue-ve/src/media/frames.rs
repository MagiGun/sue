//! Decoded pictures: a cache with a memory budget, and a worker thread
//! per movie that decodes what the monitors ask for.
//!
//! A worker owns one decoder (a decoder stays on the thread that opened
//! it). It takes the newest request only, so a scrub that asks for forty
//! frames in a second decodes the few it can and never falls behind:
//!
//! - **exact** (paused, stepping): it seeks to the key frame before the
//!   frame and reads on to it, or reads straight on when the decoder is a
//!   few frames short of it already;
//! - **fast** (scrubbing): it seeks and decodes the key frame only, which
//!   the monitor shows until the exact frame is there;
//! - **ahead** (playing): it reads on from the frame shown, filling the
//!   cache ahead of the playhead.
//!
//! Every frame decoded on the way goes into the cache, so stepping back
//! and forth costs nothing. The cache keeps frames by source, size and
//! index, and drops the least recently used beyond its budget.
//!
//! Adapted in spirit from FilmCraft's GOP caches with a shared frame
//! budget (`docs/performance.md`, MEM2 and MEM3, of
//! <https://github.com/storytold/filmcraft>, MIT OR Apache-2.0): one
//! budget for every source, least recently used out first.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

use sue_core::EntityId;

/// One decoded picture, shared by handle (rule 5): the cache, the
/// monitors and the export read the same bytes.
#[derive(Debug)]
pub struct VideoFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Opaque or straight alpha RGBA, row by row.
    pub rgba: Vec<u8>,
    /// Its index in its source (frames from the start).
    pub index: i64,
    /// Unique per frame made: textures upload again when it changes.
    pub serial: u64,
}

static SERIAL: AtomicU64 = AtomicU64::new(1);

impl VideoFrame {
    /// A frame of `rgba` pixels, `width` by `height`.
    #[must_use]
    pub fn new(width: u32, height: u32, rgba: Vec<u8>, index: i64) -> Self {
        Self {
            width,
            height,
            rgba,
            index,
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Bytes held.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// Which copy of a source a frame is from: its size class (0 full, 1
/// half, 2 quarter, 3 thumbnail) and whether it came from the proxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Variant {
    /// 0 full size, 1 half, 2 quarter, 3 a thumbnail.
    pub size: u8,
    /// From the proxy file.
    pub proxy: bool,
}

struct Entry {
    frame: Arc<VideoFrame>,
    used: u64,
}

/// Decoded frames under one memory budget.
#[derive(Default)]
pub struct FrameCache {
    frames: HashMap<(EntityId, Variant), BTreeMap<i64, Entry>>,
    bytes: usize,
    budget: usize,
    clock: u64,
    /// Frames asked for and found.
    pub hits: u64,
    /// Frames asked for and not found.
    pub misses: u64,
}

impl std::fmt::Debug for FrameCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCache")
            .field("bytes", &self.bytes)
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

impl FrameCache {
    /// A cache that holds at most `budget` bytes of pictures.
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    /// Bytes held now.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Frames held now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.values().map(BTreeMap::len).sum()
    }

    /// `true` when it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Changes the budget (dropping frames when it shrank).
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        self.trim();
    }

    /// `true` when frame `index` of `source` is held.
    #[must_use]
    pub fn has(&self, source: EntityId, variant: Variant, index: i64) -> bool {
        self.frames
            .get(&(source, variant))
            .is_some_and(|frames| frames.contains_key(&index))
    }

    /// Frame `index`, if held.
    pub fn get(
        &mut self,
        source: EntityId,
        variant: Variant,
        index: i64,
    ) -> Option<Arc<VideoFrame>> {
        self.clock += 1;
        let clock = self.clock;
        let found = self
            .frames
            .get_mut(&(source, variant))
            .and_then(|frames| frames.get_mut(&index))
            .map(|entry| {
                entry.used = clock;
                Arc::clone(&entry.frame)
            });
        if found.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        found
    }

    /// The held frame nearest `index` at or before it (else the nearest
    /// after it), for a monitor to show while the exact one decodes.
    pub fn nearest(
        &mut self,
        source: EntityId,
        variant: Variant,
        index: i64,
    ) -> Option<Arc<VideoFrame>> {
        let frames = self.frames.get(&(source, variant))?;
        let before = frames.range(..=index).next_back();
        let after = frames.range(index..).next();
        let entry = before.or(after)?;
        Some(Arc::clone(&entry.1.frame))
    }

    /// Keeps `frame`.
    pub fn put(&mut self, source: EntityId, variant: Variant, frame: Arc<VideoFrame>) {
        self.clock += 1;
        let bytes = frame.bytes();
        let index = frame.index;
        let old = self.frames.entry((source, variant)).or_default().insert(
            index,
            Entry {
                frame,
                used: self.clock,
            },
        );
        self.bytes += bytes;
        if let Some(old) = old {
            self.bytes -= old.frame.bytes();
        }
        self.trim();
    }

    /// Forgets every frame of `source`.
    pub fn forget(&mut self, source: EntityId) {
        let keys: Vec<(EntityId, Variant)> = self
            .frames
            .keys()
            .filter(|(id, _)| *id == source)
            .copied()
            .collect();
        for key in keys {
            if let Some(frames) = self.frames.remove(&key) {
                self.bytes -= frames
                    .values()
                    .map(|entry| entry.frame.bytes())
                    .sum::<usize>();
            }
        }
    }

    fn trim(&mut self) {
        while self.bytes > self.budget {
            let oldest = self
                .frames
                .iter()
                .flat_map(|(key, frames)| {
                    frames
                        .iter()
                        .map(move |(index, entry)| (entry.used, *key, *index))
                })
                .min_by_key(|(used, _, _)| *used);
            let Some((_, key, index)) = oldest else {
                break;
            };
            if let Some(entry) = self
                .frames
                .get_mut(&key)
                .and_then(|frames| frames.remove(&index))
            {
                self.bytes -= entry.frame.bytes();
            }
        }
    }
}

/// The cache, shared with the workers.
pub type SharedCache = Arc<Mutex<FrameCache>>;

/// Locks the cache; a worker that panicked holding it leaves it usable.
pub fn lock(cache: &SharedCache) -> std::sync::MutexGuard<'_, FrameCache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A movie a worker decodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Movie {
    /// The source entity.
    pub id: EntityId,
    /// The file to read (the original, or its proxy).
    pub path: PathBuf,
    /// Its frame rate as numerator and denominator.
    pub rate: (u32, u32),
    /// The size to decode at.
    pub size: (u32, u32),
    /// Which copy this is.
    pub variant: Variant,
    /// Decode on the graphics card when it can.
    pub hardware: bool,
}

impl Movie {
    /// One frame's length in 100 nanosecond units.
    #[must_use]
    pub fn frame_ticks(&self) -> f64 {
        let (numerator, denominator) = self.rate;
        if numerator == 0 {
            return 10_000_000.0 / 30.0;
        }
        10_000_000.0 * f64::from(denominator) / f64::from(numerator)
    }

    /// The index of the frame showing at `ticks` (100 ns).
    #[must_use]
    pub fn index_at(&self, ticks: i64) -> i64 {
        ((ticks as f64 + 1.0) / self.frame_ticks()).floor() as i64
    }

    /// When frame `index` starts.
    #[must_use]
    pub fn ticks_of(&self, index: i64) -> i64 {
        (index as f64 * self.frame_ticks()).round() as i64
    }
}

/// What a monitor wants of a worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    /// The frame.
    pub index: i64,
    /// The exact frame (else the key frame before it will do).
    pub exact: bool,
    /// Frames after it to decode too (playing).
    pub ahead: u32,
}

/// How a worker is doing, for the monitors and the measurements.
#[derive(Debug, Default)]
pub struct WorkerStats {
    /// Frames decoded.
    pub decoded: AtomicU64,
    /// Seeks made.
    pub seeks: AtomicU64,
    /// Decoding on the graphics card.
    pub hardware: AtomicBool,
    /// The last error, if decoding failed.
    pub error: Mutex<Option<String>>,
}

/// A worker thread decoding one movie.
#[derive(Debug)]
pub struct Worker {
    movie: Movie,
    to: Sender<Want>,
    last: Option<Want>,
    /// How it is doing.
    pub stats: Arc<WorkerStats>,
}

impl Worker {
    /// Starts a worker for `movie`, decoding into `cache`.
    #[must_use]
    pub fn start(movie: Movie, cache: SharedCache) -> Self {
        let (to, from) = std::sync::mpsc::channel();
        let stats = Arc::new(WorkerStats::default());
        let thread_stats = Arc::clone(&stats);
        let thread_movie = movie.clone();
        let spawned = std::thread::Builder::new()
            .name("sue-ve-decode".to_owned())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&thread_movie, &cache, &from, &thread_stats);
                }));
                if outcome.is_err() {
                    *thread_stats
                        .error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) =
                        Some("the decoder stopped".to_owned());
                }
            });
        if let Err(error) = spawned {
            *stats.error.lock().unwrap_or_else(PoisonError::into_inner) = Some(error.to_string());
        }
        Self {
            movie,
            to,
            last: None,
            stats,
        }
    }

    /// The movie it decodes.
    #[must_use]
    pub fn movie(&self) -> &Movie {
        &self.movie
    }

    /// Asks for a frame (nothing is sent when the same was asked last).
    pub fn want(&mut self, want: Want) {
        if self.last == Some(want) {
            return;
        }
        self.last = Some(want);
        let _ = self.to.send(want);
    }

    /// The last error, if decoding failed.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.stats
            .error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// How far ahead of the decoder a frame may be and still be read to
/// rather than seeked to.
const READ_ON: i64 = 24;

fn run(movie: &Movie, cache: &SharedCache, from: &Receiver<Want>, stats: &WorkerStats) {
    let mut decoder: Option<sue_platform::video::VideoDecoder> = None;
    // The index of the last frame read: the decoder sits just after it.
    let mut position: Option<i64> = None;
    let mut job: Option<Want> = None;
    let mut rgba = Vec::new();
    let mut failed = false;
    // The frame the last seek was for: after it the decoder reads on to
    // it from the key frame, however far that is.
    let mut seeked_for: Option<i64> = None;
    loop {
        if job.is_none() {
            match from.recv() {
                Ok(want) => job = Some(want),
                Err(_) => return,
            }
        }
        loop {
            match from.try_recv() {
                Ok(want) => job = Some(want),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let Some(want) = job else { continue };
        if failed {
            job = None;
            continue;
        }
        if decoder.is_none() {
            match sue_platform::video::VideoDecoder::open(
                &movie.path,
                Some(movie.size),
                movie.hardware,
            ) {
                Ok(opened) => {
                    stats.hardware.store(opened.hardware(), Ordering::Relaxed);
                    decoder = Some(opened);
                }
                Err(error) => {
                    *stats.error.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some(error.to_string());
                    failed = true;
                    continue;
                }
            }
        }
        let Some(reader) = decoder.as_mut() else {
            continue;
        };
        // The first frame of the request not held yet.
        let missing = {
            let cache = lock(cache);
            let last = want.index + i64::from(want.ahead);
            (want.index..=last).find(|index| !cache.has(movie.id, movie.variant, *index))
        };
        let Some(target) = missing else {
            job = None;
            continue;
        };
        let reading_on = position.is_some_and(|at| {
            at < target && (target - at <= READ_ON || seeked_for == Some(target))
        });
        if !reading_on {
            if reader.seek(movie.ticks_of(target)).is_err() {
                job = None;
                continue;
            }
            stats.seeks.fetch_add(1, Ordering::Relaxed);
            seeked_for = Some(target);
        }
        // Decode one frame, keep it, and look at the requests again.
        match reader.next_frame(&mut rgba) {
            Ok(Some(frame)) => {
                stats.decoded.fetch_add(1, Ordering::Relaxed);
                let index = movie.index_at(frame.time).max(0);
                let (width, height) = reader.size();
                position = Some(index);
                lock(cache).put(
                    movie.id,
                    movie.variant,
                    Arc::new(VideoFrame::new(
                        width,
                        height,
                        std::mem::take(&mut rgba),
                        index,
                    )),
                );
                // A fast request is answered by the key frame itself.
                if !want.exact && !reading_on && index <= target {
                    job = None;
                }
                // Past the frame (a file with gaps): stop asking for it.
                if index > target && want.ahead == 0 {
                    job = None;
                }
            }
            Ok(None) => {
                // The end: nothing more to decode this way.
                position = None;
                job = None;
            }
            Err(error) => {
                *stats.error.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(error.to_string());
                position = None;
                job = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(index: i64, bytes: usize) -> Arc<VideoFrame> {
        Arc::new(VideoFrame::new(1, 1, vec![0; bytes], index))
    }

    #[test]
    fn the_cache_keeps_its_budget_and_drops_the_least_used_first() {
        let id = EntityId::new();
        let full = Variant {
            size: 0,
            proxy: false,
        };
        let mut cache = FrameCache::new(100);
        for index in 0..5 {
            cache.put(id, full, frame(index, 30));
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.bytes() <= 100);
        assert!(!cache.has(id, full, 0) && !cache.has(id, full, 1));
        // Use 2: then 3 is the oldest and goes next.
        assert!(cache.get(id, full, 2).is_some());
        cache.put(id, full, frame(9, 30));
        assert!(cache.has(id, full, 2) && !cache.has(id, full, 3));
        // The nearest earlier frame stands in for one not decoded.
        assert_eq!(cache.nearest(id, full, 8).unwrap().index, 4);
        assert_eq!(cache.nearest(id, full, 1).unwrap().index, 2);
        cache.forget(id);
        assert!(cache.is_empty() && cache.bytes() == 0);
    }

    #[test]
    fn frame_times_and_indices_agree_at_ntsc_rates() {
        let movie = Movie {
            id: EntityId::new(),
            path: PathBuf::new(),
            rate: (30_000, 1001),
            size: (2, 2),
            variant: Variant {
                size: 0,
                proxy: false,
            },
            hardware: false,
        };
        for index in [0, 1, 29, 30, 1799, 107_892] {
            assert_eq!(movie.index_at(movie.ticks_of(index)), index);
            assert_eq!(movie.index_at(movie.ticks_of(index) + 3), index);
        }
    }
}
