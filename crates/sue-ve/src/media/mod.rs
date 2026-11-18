//! Media: bringing files in ([`import`]), decoded pictures ([`frames`]),
//! decoded sound and waveforms ([`sound`]), thumbnails and proxies
//! ([`thumbs`]), and the hub that holds them for the studio
//! ([`MediaHub`]).

pub mod frames;
pub mod import;
pub mod sound;
pub mod stills;
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

/// A picture drawn from the arena: the source's revision, the frame, the
/// size it was drawn for, and the pixels.
type Generated = (u64, i64, (u32, u32), Arc<VideoFrame>);

/// Everything the studio has decoded or is decoding.
pub struct MediaHub {
    /// Decoded movie frames.
    pub cache: SharedCache,
    workers: HashMap<(EntityId, Variant), Worker>,
    /// Decoded sound.
    pub sounds: Sounds,
    /// Thumbnail strips.
    pub thumbs: Thumbs,
    /// Pictures drawn from the arena on this thread (stills, titles,
    /// solids): by source, its revision, frame and size.
    generated: HashMap<EntityId, Generated>,
    /// Frames of image sequences and Sue Paint pictures, drawn by
    /// workers into [`Self::cache`].
    pub stills: stills::Stills,
    /// The revision and size each drawn source's cached frames are of.
    drawn_as: HashMap<EntityId, (u64, (u32, u32))>,
    /// The arena as a picture was when its frames were asked for: what
    /// the workers composite it from (a clone copies no pixels).
    snapshots: HashMap<EntityId, (u64, Arc<Arena>)>,
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
            stills: stills::Stills::default(),
            drawn_as: HashMap::new(),
            snapshots: HashMap::new(),
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
    #[expect(
        clippy::too_many_arguments,
        reason = "what a monitor asks for: a source, its kind and time, the frame, the quality and the need"
    )]
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
            SourceKind::ImageSequence | SourceKind::Picture => {
                self.drawn(arena, id, kind, source_time, frame, need)
            }
            _ => {
                let entity = arena.get_by_id(id).ok()?;
                let revision = entity.revision();
                let index = 0;
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
                let picture =
                    Arc::new(render::generated(arena, id, kind, source_time, frame, 1.0)?);
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

    /// The frame of an image sequence or a Sue Paint picture (or its
    /// animation) at `source_time`: from the cache when a worker drew it,
    /// else the nearest frame drawn while the workers draw it (with the
    /// frames after it when playing).
    fn drawn(
        &mut self,
        arena: &Arena,
        id: EntityId,
        kind: &SourceKind,
        source_time: Time,
        frame: (u32, u32),
        need: Need,
    ) -> Option<Found> {
        let entity = arena.get_by_id(id).ok()?;
        let revision = entity.revision();
        let (index, last) = match kind {
            SourceKind::ImageSequence => {
                let rate = model::rate_of(entity)?;
                let count = model::number(entity, key::COUNT).unwrap_or(1.0).max(1.0) as i64;
                (source_time.frame(rate).clamp(0, count - 1), count - 1)
            }
            _ => match sue_picture::anim::timeline(arena, id) {
                Some(sequence) if sue_picture::anim::length(arena, id) > 0 => {
                    let length = i64::from(sue_picture::anim::length(arena, id));
                    (
                        source_time.frame(sequence.rate).clamp(0, length - 1),
                        length - 1,
                    )
                }
                _ => (0, 0),
            },
        };
        let size = if *kind == SourceKind::Picture {
            frame
        } else {
            (0, 0)
        };
        if self.drawn_as.get(&id) != Some(&(revision, size)) {
            frames::lock(&self.cache).forget(id);
            self.drawn_as.insert(id, (revision, size));
        }
        let ahead = match need {
            Need::Ahead(count) => i64::from(count),
            _ => 0,
        };
        let wanted: Vec<i64> = {
            let cache = frames::lock(&self.cache);
            (index..=(index + ahead).min(last))
                .filter(|at| !cache.has(id, DRAWN, *at))
                .collect()
        };
        if !wanted.is_empty() {
            let jobs = match kind {
                SourceKind::ImageSequence => wanted
                    .iter()
                    .filter_map(|at| {
                        let path = import::sequence_frame(entity, *at)?;
                        let at = *at;
                        Some(stills::Job {
                            source: id,
                            revision,
                            index: at,
                            make: Box::new(move || sequence_frame(&path, at)),
                        })
                    })
                    .collect(),
                _ => {
                    let snapshot = match self.snapshots.get(&id) {
                        Some((seen, snapshot)) if *seen == revision => Arc::clone(snapshot),
                        _ => {
                            let snapshot = Arc::new(arena.clone());
                            self.snapshots.insert(id, (revision, Arc::clone(&snapshot)));
                            snapshot
                        }
                    };
                    let rate = sue_picture::anim::timeline(arena, id)
                        .map_or(sue_core::Rate::FPS_24, |sequence| sequence.rate);
                    let max = frame.0.max(frame.1).max(256);
                    wanted
                        .iter()
                        .map(|at| {
                            let snapshot = Arc::clone(&snapshot);
                            let at = *at;
                            stills::Job {
                                source: id,
                                revision,
                                index: at,
                                make: Box::new(move || {
                                    let time = Time::from_frames(at, rate);
                                    render::picture(&snapshot, id, time, max).map(|mut picture| {
                                        picture.index = at;
                                        picture
                                    })
                                }),
                            }
                        })
                        .collect()
                }
            };
            self.stills.want(id, jobs);
        }
        let mut cache = frames::lock(&self.cache);
        if let Some(frame) = cache.get(id, DRAWN, index) {
            return Some(Found { frame, exact: true });
        }
        cache.nearest(id, DRAWN, index).map(|frame| Found {
            frame,
            exact: false,
        })
    }

    /// Takes in background work that finished. `true` when something did.
    pub fn poll(&mut self) -> bool {
        let mut changed = self.sounds.poll();
        let drawn = self.stills.take();
        if !drawn.is_empty() {
            let mut cache = frames::lock(&self.cache);
            for (source, revision, _, picture) in drawn {
                let current = self
                    .drawn_as
                    .get(&source)
                    .is_some_and(|(seen, _)| *seen == revision);
                if let (true, Some(picture)) = (current, picture) {
                    cache.put(source, DRAWN, picture);
                    changed = true;
                }
            }
        }
        changed
    }

    /// `true` while sound, thumbnails or drawn frames are being made.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.sounds.loading() || self.thumbs.busy() || self.stills.busy()
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
        self.stills.clear();
        self.drawn_as.clear();
        self.snapshots.clear();
    }
}

/// The cache's variant for frames the workers draw (image sequences,
/// Sue Paint pictures).
const DRAWN: Variant = Variant {
    size: 4,
    proxy: false,
};

/// Reads and decodes frame `index` of an image sequence from `path`.
fn sequence_frame(path: &std::path::Path, index: i64) -> Option<VideoFrame> {
    let bytes = sue_platform::fs::read(path).ok()?;
    let tile = import::read_picture(&bytes).ok()?;
    Some(VideoFrame::new(
        tile.width(),
        tile.height(),
        render::rgba_of(&tile)?,
        index,
    ))
}
