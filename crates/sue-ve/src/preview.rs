//! The program monitor's processed frames and the render cache.
//!
//! A frame that holds effects, grades, masks, wipes, adjustment clips or
//! nested sequences is drawn on the processor by the very renderer the
//! export uses ([`crate::export::Renderer`], which calls
//! [`crate::compose::frame`]), on a thread of its own, at the monitor's
//! size. Frames drawn are kept in the render cache by their
//! [`crate::compose::frame_key`], so playing a stretch again, or after an
//! edit elsewhere, shows them at once. The timeline's ruler marks the
//! stretches that need the processor: red where they are not drawn yet,
//! green where they are, as Premiere Pro's render bar does.
//!
//! The worker draws what the monitor asks for first (the playhead and the
//! frames after it while playing), then, while nothing else is asked and
//! background rendering is on, what is asked to be rendered ahead
//! ([`Preview::render_ahead`]).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

use sue_core::{Arena, EntityId, Time};

use crate::compose::Options;
use crate::export::Renderer;
use crate::media::frames::VideoFrame;

/// Drawn frames by key, under a memory budget, least recently used out
/// first.
#[derive(Debug, Default)]
pub struct RenderCache {
    frames: HashMap<u64, (Arc<VideoFrame>, u64)>,
    bytes: usize,
    budget: usize,
    clock: u64,
    /// Changes whenever a frame comes in or goes (the ruler's bar redraws).
    pub generation: u64,
}

impl RenderCache {
    /// A cache of at most `budget` bytes.
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    /// The frame drawn for `key`.
    pub fn get(&mut self, key: u64) -> Option<Arc<VideoFrame>> {
        self.clock += 1;
        let clock = self.clock;
        self.frames.get_mut(&key).map(|(frame, used)| {
            *used = clock;
            Arc::clone(frame)
        })
    }

    /// `true` when `key` is drawn.
    #[must_use]
    pub fn has(&self, key: u64) -> bool {
        self.frames.contains_key(&key)
    }

    /// Keeps `frame` under `key`.
    pub fn put(&mut self, key: u64, frame: Arc<VideoFrame>) {
        self.clock += 1;
        self.bytes += frame.bytes();
        if let Some((old, _)) = self.frames.insert(key, (frame, self.clock)) {
            self.bytes -= old.bytes();
        }
        while self.bytes > self.budget {
            let Some(oldest) = self
                .frames
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some((frame, _)) = self.frames.remove(&oldest) {
                self.bytes -= frame.bytes();
            }
        }
        self.generation += 1;
    }

    /// Frames held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// `true` when it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Drops every frame.
    pub fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
        self.generation += 1;
    }
}

/// A frame to draw: when, and the key it is kept by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    /// On the sequence.
    pub time: Time,
    /// Its key.
    pub key: u64,
}

enum Message {
    /// Draw from this arena from now on.
    Snapshot {
        arena: Box<Arena>,
        sequence: EntityId,
        size: (u32, u32),
        hardware: bool,
    },
    /// Draw these first (replacing what was asked before).
    Now(Vec<Want>, Options),
    /// Draw these when nothing else is asked.
    Ahead(Vec<Want>, Options),
}

/// The worker that draws processed frames, and the cache it fills.
pub struct Preview {
    /// The render cache.
    pub cache: Arc<Mutex<RenderCache>>,
    to: Option<Sender<Message>>,
    /// The arena revision, sequence and size the worker draws from.
    synced: Option<(u64, EntityId, (u32, u32))>,
    /// Frames drawn by the worker.
    pub drawn: Arc<AtomicU64>,
    /// Milliseconds the last frame took.
    pub last_ms: Arc<AtomicU64>,
    busy: Arc<AtomicBool>,
    /// The last frame shown, while the one due is drawn.
    pub last: Option<Arc<VideoFrame>>,
    asked: Vec<Want>,
}

impl std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preview")
            .field("synced", &self.synced)
            .finish_non_exhaustive()
    }
}

impl Default for Preview {
    fn default() -> Self {
        Self::new(768)
    }
}

impl Preview {
    /// A preview whose cache holds at most `budget_mb` megabytes.
    #[must_use]
    pub fn new(budget_mb: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(RenderCache::new(
                budget_mb.max(32) * 1024 * 1024,
            ))),
            to: None,
            synced: None,
            drawn: Arc::new(AtomicU64::new(0)),
            last_ms: Arc::new(AtomicU64::new(0)),
            busy: Arc::new(AtomicBool::new(false)),
            last: None,
            asked: Vec::new(),
        }
    }

    fn sender(&mut self) -> Option<&Sender<Message>> {
        if self.to.is_none() {
            let (to, from) = std::sync::mpsc::channel();
            let cache = Arc::clone(&self.cache);
            let drawn = Arc::clone(&self.drawn);
            let last_ms = Arc::clone(&self.last_ms);
            let busy = Arc::clone(&self.busy);
            let started = std::thread::Builder::new()
                .name("sue-ve-preview".to_owned())
                .spawn(move || {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(&from, &cache, &drawn, &last_ms, &busy);
                    }));
                });
            if started.is_err() {
                return None;
            }
            self.to = Some(to);
        }
        self.to.as_ref()
    }

    /// Makes the worker draw from `arena` when it changed since it last
    /// did (a clone, which copies no pixels).
    pub fn sync(&mut self, arena: &Arena, sequence: EntityId, size: (u32, u32), hardware: bool) {
        let now = (arena.revision(), sequence, size);
        if self.synced == Some(now) {
            return;
        }
        self.synced = Some(now);
        self.asked.clear();
        if let Some(to) = self.sender() {
            let _ = to.send(Message::Snapshot {
                arena: Box::new(arena.clone()),
                sequence,
                size,
                hardware,
            });
        }
    }

    /// The frame drawn for `key`.
    #[must_use]
    pub fn get(&self, key: u64) -> Option<Arc<VideoFrame>> {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
    }

    /// `true` when `key` is drawn.
    #[must_use]
    pub fn has(&self, key: u64) -> bool {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .has(key)
    }

    /// Asks for `wants` first (nothing is sent when the same was asked
    /// last).
    pub fn want(&mut self, wants: Vec<Want>, options: Options) {
        if wants == self.asked {
            return;
        }
        self.asked.clone_from(&wants);
        if let Some(to) = self.sender() {
            let _ = to.send(Message::Now(wants, options));
        }
    }

    /// Asks for `wants` to be drawn when nothing else is asked.
    pub fn render_ahead(&mut self, wants: Vec<Want>, options: Options) {
        if let Some(to) = self.sender() {
            let _ = to.send(Message::Ahead(wants, options));
        }
    }

    /// `true` while the worker draws.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    /// The cache's generation (it changes when frames come in).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .generation
    }

    /// Drops every drawn frame.
    pub fn clear(&mut self) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.last = None;
    }
}

fn run(
    from: &Receiver<Message>,
    cache: &Mutex<RenderCache>,
    drawn: &AtomicU64,
    last_ms: &AtomicU64,
    busy: &AtomicBool,
) {
    let mut renderer: Option<Renderer> = None;
    let mut now: Vec<Want> = Vec::new();
    let mut ahead: Vec<Want> = Vec::new();
    let mut options = Options::default();
    let mut ahead_options = Options::default();
    let take = |message: Message,
                renderer: &mut Option<Renderer>,
                now: &mut Vec<Want>,
                ahead: &mut Vec<Want>,
                options: &mut Options,
                ahead_options: &mut Options| match message {
        Message::Snapshot {
            arena,
            sequence,
            size,
            hardware,
        } => {
            *renderer = Renderer::new(*arena, sequence, size, hardware).ok();
            now.clear();
            ahead.clear();
        }
        Message::Now(wants, with) => {
            *now = wants;
            *options = with;
        }
        Message::Ahead(wants, with) => {
            *ahead = wants;
            *ahead_options = with;
        }
    };
    loop {
        // Wait when there is nothing to draw.
        if now.is_empty() && ahead.is_empty() {
            busy.store(false, Ordering::Relaxed);
            match from.recv() {
                Ok(message) => take(
                    message,
                    &mut renderer,
                    &mut now,
                    &mut ahead,
                    &mut options,
                    &mut ahead_options,
                ),
                Err(_) => return,
            }
        }
        loop {
            match from.try_recv() {
                Ok(message) => take(
                    message,
                    &mut renderer,
                    &mut now,
                    &mut ahead,
                    &mut options,
                    &mut ahead_options,
                ),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let (want, with) = if now.is_empty() {
            if ahead.is_empty() {
                continue;
            }
            (ahead.remove(0), ahead_options)
        } else {
            (now.remove(0), options)
        };
        let held = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .has(want.key);
        if held {
            continue;
        }
        let Some(drawer) = renderer.as_mut() else {
            now.clear();
            ahead.clear();
            continue;
        };
        busy.store(true, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let size = drawer.size();
        let rgba = drawer.frame_with(want.time, with);
        last_ms.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        drawn.fetch_add(1, Ordering::Relaxed);
        cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(want.key, Arc::new(VideoFrame::new(size.0, size.1, rgba, 0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_keeps_its_budget() {
        let mut cache = RenderCache::new(100);
        for key in 0..5 {
            cache.put(key, Arc::new(VideoFrame::new(1, 1, vec![0; 40], 0)));
        }
        assert_eq!(cache.len(), 2);
        assert!(cache.has(4) && cache.has(3) && !cache.has(0));
        assert!(cache.get(3).is_some());
        cache.put(9, Arc::new(VideoFrame::new(1, 1, vec![0; 40], 0)));
        assert!(cache.has(3) && !cache.has(4));
        let generation = cache.generation;
        cache.clear();
        assert!(cache.is_empty() && cache.generation > generation);
    }
}
