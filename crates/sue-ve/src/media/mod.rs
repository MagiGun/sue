//! Media: bringing files in ([`import`]), decoded pictures ([`frames`]),
//! decoded sound and waveforms ([`sound`]), thumbnails and proxies
//! ([`thumbs`]), and the hub that holds them for the studio
//! ([`MediaHub`]).

pub mod frames;
pub mod import;
pub mod sound;
pub mod thumbs;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sue_core::{Arena, EntityId, Time};

use crate::model::{self, SourceKind, key};
use crate::render;
use frames::{FrameCache, Movie, SharedCache, Variant, VideoFrame, Want, Worker};
use sound::Sounds;
use thumbs::Thumbs;

/// How large monitors decode pictures, against the source's own size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Quality {
    /// The source's own size.
    Full,
    /// Half each way.
    #[default]
    Half,
    /// A quarter each way.
    Quarter,
}

impl Quality {
    /// Every quality, best first.
    pub const ALL: [Self; 3] = [Self::Full, Self::Half, Self::Quarter];

    /// The size class frames are cached under.
    #[must_use]
    pub fn class(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Half => 1,
            Self::Quarter => 2,
        }
    }

    /// `size` at this quality, even, at least 34 pixels a side.
    #[must_use]
    pub fn of(self, size: (u32, u32)) -> (u32, u32) {
        let divide = 1 << self.class();
        let even = |value: u32| ((value / divide).max(34) + 1) & !1;
        (even(size.0), even(size.1))
    }

    /// Its stable name, as settings keep it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Half => "half",
            Self::Quarter => "quarter",
        }
    }

    /// The quality of a stable name.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|quality| quality.id() == id)
    }
}

/// How a frame is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// The exact frame (paused, stepping).
    Exact,
    /// Any near frame now, the exact one when it can (scrubbing).
    Fast,
    /// The frame and the ones after it (playing).
    Ahead(u32),
}

/// A frame found for a monitor.
#[derive(Clone, Debug)]
pub struct Found {
    /// The pictures.
    pub frame: Arc<VideoFrame>,
    /// `false` while a nearby frame stands in for the one asked.
    pub exact: bool,
}

/// A source time in 100 nanosecond units.
#[must_use]
pub fn ticks(time: Time) -> i64 {
    time.flicks() / (sue_core::FLICKS_PER_SECOND / 10_000_000)
}

/// Everything the studio has decoded or is decoding.
pub struct MediaHub {
    /// Decoded movie frames.
    pub cache: SharedCache,
    workers: HashMap<(EntityId, Variant), Worker>,
    /// Decoded sound.
    pub sounds: Sounds,
    /// Thumbnail strips.
    pub thumbs: Thumbs,
    /// Pictures drawn from the arena (stills, pictures, titles, solids)
    /// and image sequence frames: by source, its revision, frame and size.
    generated: HashMap<EntityId, (u64, i64, (u32, u32), Arc<VideoFrame>)>,
    /// Decode on the graphics card.
    pub hardware: bool,
    /// Play proxies where a movie has one.
    pub proxies: bool,
}

impl std::fmt::Debug for MediaHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaHub")
            .field("workers", &self.workers.len())
            .field("hardware", &self.hardware)
            .finish_non_exhaustive()
    }
}

impl Default for MediaHub {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl MediaHub {
    /// A hub whose frame cache holds at most `budget_mb` megabytes.
    #[must_use]
    pub fn new(budget_mb: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(FrameCache::new(budget_mb.max(64) * 1024 * 1024))),
            workers: HashMap::new(),
            sounds: Sounds::default(),
            thumbs: Thumbs::default(),
            generated: HashMap::new(),
            hardware: true,
            proxies: false,
        }
    }

    /// The movie a worker decodes for source `id` at `quality`.
    #[must_use]
    pub fn movie(&self, arena: &Arena, id: EntityId, quality: Quality) -> Option<Movie> {
        let entity = arena.get_by_id(id).ok()?;
        if SourceKind::of(entity) != SourceKind::Movie {
            return None;
        }
        let size = (
            model::number(entity, key::WIDTH)? as u32,
            model::number(entity, key::HEIGHT)? as u32,
        );
        let rate = model::rate_of(entity)?;
        let proxy = self
            .proxies
            .then(|| model::text(entity, key::PROXY).map(std::path::PathBuf::from))
            .flatten()
            .filter(|path| path.is_file());
        let (path, source_size, from_proxy) = match proxy {
            Some(path) => (path, thumbs::proxy_size(size), true),
            None => (import::file_of(entity)?, size, false),
        };
        Some(Movie {
            id,
            path,
            rate: (rate.numerator(), rate.denominator()),
            size: quality.of(source_size),
            variant: Variant {
                size: quality.class(),
                proxy: from_proxy,
            },
            hardware: self.hardware,
        })
    }

    fn worker(&mut self, movie: Movie) -> &mut Worker {
        let key = (movie.id, movie.variant);
        let stale = self
            .workers
            .get(&key)
            .is_some_and(|worker| worker.movie() != &movie);
        if stale {
            self.workers.remove(&key);
            frames::lock(&self.cache).forget(movie.id);
        }
        let cache = Arc::clone(&self.cache);
        self.workers
            .entry(key)
            .or_insert_with(|| Worker::start(movie, cache))
    }

    /// The picture of source `id` (of `kind`) at `source_time`, for a
    /// frame of `frame` size at `quality`. Movies are asked of their
    /// worker as `need` says and come from the cache (a nearby frame
    /// standing in until the one asked for is decoded); the rest is drawn
    /// here and kept until the source changes.
    pub fn frame(
        &mut self,
        arena: &Arena,
        id: EntityId,
        kind: &SourceKind,
        source_time: Time,
        frame: (u32, u32),
        quality: Quality,
        need: Need,
    ) -> Option<Found> {
        match kind {
            SourceKind::Movie => {
                let movie = self.movie(arena, id, quality)?;
                let index = movie.index_at(ticks(source_time)).max(0);
                let variant = movie.variant;
                let want = match need {
                    Need::Exact => Want {
                        index,
                        exact: true,
                        ahead: 0,
                    },
                    Need::Fast => Want {
                        index,
                        exact: false,
                        ahead: 0,
                    },
                    Need::Ahead(count) => Want {
                        index,
                        exact: true,
                        ahead: count,
                    },
                };
                self.worker(movie).want(want);
                let mut cache = frames::lock(&self.cache);
                if let Some(frame) = cache.get(id, variant, index) {
                    return Some(Found { frame, exact: true });
                }
                cache.nearest(id, variant, index).map(|frame| Found {
                    frame,
                    exact: false,
                })
            }
            SourceKind::ImageSequence => {
                let entity = arena.get_by_id(id).ok()?;
                let rate = model::rate_of(entity)?;
                let index = source_time.frame(rate).max(0);
                let revision = entity.revision();
                if let Some((seen, at, _, picture)) = self.generated.get(&id)
                    && *seen == revision
                    && *at == index
                {
                    return Some(Found {
                        frame: Arc::clone(picture),
                        exact: true,
                    });
                }
                let path = import::sequence_frame(entity, index)?;
                let bytes = sue_platform::fs::read(&path).ok()?;
                let tile = import::read_picture(&bytes).ok()?;
                let picture = Arc::new(VideoFrame::new(
                    tile.width(),
                    tile.height(),
                    render::rgba_of(&tile)?,
                    index,
                ));
                self.generated
                    .insert(id, (revision, index, frame, Arc::clone(&picture)));
                Some(Found {
                    frame: picture,
                    exact: true,
                })
            }
            _ => {
                let entity = arena.get_by_id(id).ok()?;
                let revision = entity.revision();
                let index = match kind {
                    SourceKind::Picture => sue_picture::anim::timeline(arena, id)
                        .map_or(0, |sequence| source_time.frame(sequence.rate)),
                    _ => 0,
                };
                if let Some((seen, at, size, picture)) = self.generated.get(&id)
                    && *seen == revision
                    && *at == index
                    && *size == frame
                {
                    return Some(Found {
                        frame: Arc::clone(picture),
                        exact: true,
                    });
                }
                let picture = Arc::new(render::generated(arena, id, kind, source_time, frame)?);
                self.generated
                    .insert(id, (revision, index, frame, Arc::clone(&picture)));
                Some(Found {
                    frame: picture,
                    exact: true,
                })
            }
        }
    }

    /// Starts what a source needs in the background: its sound and its
    /// thumbnails.
    pub fn prepare(&mut self, arena: &Arena, id: EntityId) {
        let Ok(entity) = arena.get_by_id(id) else {
            return;
        };
        match SourceKind::of(entity) {
            SourceKind::Movie => {
                let Some(path) = import::file_of(entity) else {
                    return;
                };
                if model::has_sound(arena, id) {
                    self.sounds.request(id, path.clone(), true);
                }
                if let (Some(width), Some(height)) = (
                    model::number(entity, key::WIDTH),
                    model::number(entity, key::HEIGHT),
                ) {
                    let seconds = model::length_of(arena, id).map_or(1.0, Time::seconds);
                    self.thumbs
                        .request(id, &path, (width as u32, height as u32), seconds);
                }
            }
            SourceKind::Sound => {
                if let Some(path) = import::file_of(entity) {
                    self.sounds.request(id, path, false);
                }
            }
            _ => {}
        }
    }

    /// Takes in background work that finished. `true` when something did.
    pub fn poll(&mut self) -> bool {
        self.sounds.poll()
    }

    /// `true` while sound or thumbnails are being made.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.sounds.loading() || self.thumbs.busy()
    }

    /// Why decoding source `id` failed, if it did.
    #[must_use]
    pub fn error(&self, id: EntityId) -> Option<String> {
        self.workers
            .iter()
            .filter(|((source, _), _)| *source == id)
            .find_map(|(_, worker)| worker.error())
    }

    /// Whether any worker decodes on the graphics card, and how many
    /// frames and seeks all have done.
    #[must_use]
    pub fn stats(&self) -> (bool, u64, u64) {
        use std::sync::atomic::Ordering;
        let mut hardware = false;
        let (mut decoded, mut seeks) = (0, 0);
        for worker in self.workers.values() {
            hardware |= worker.stats.hardware.load(Ordering::Relaxed);
            decoded += worker.stats.decoded.load(Ordering::Relaxed);
            seeks += worker.stats.seeks.load(Ordering::Relaxed);
        }
        (hardware, decoded, seeks)
    }

    /// Stops every worker and drops what was decoded (the files changed,
    /// or decoding settings did).
    pub fn reset(&mut self) {
        self.workers.clear();
        let mut cache = frames::lock(&self.cache);
        let budget = cache.bytes().max(1);
        *cache = FrameCache::new(budget.max(64 * 1024 * 1024));
        drop(cache);
        self.generated.clear();
    }
}
