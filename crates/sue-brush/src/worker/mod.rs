//! # The stroke worker
//!
//! While the pen is down the interface only queues the pen's samples
//! ([`StrokeWorker::push`]). The stroke itself, the input pipeline and
//! the stroke buffer together ([`Stroke`]), is an owned value that moves
//! into a job of the shared [`JobPool`] ([`shared_pool`]), lays what the
//! samples so far add up to, and comes back ([`StrokeWorker::poll`]).
//! What it laid is shown by the compositor through a [`LiveView`] before
//! it is in the layer; on lift the whole stroke is merged into the layer
//! once, as one [`RasterEdit`] ([`Stroke::commit`]), and a stroke given
//! up leaves nothing behind.
//!
//! The pool is the compositor's too (`sue_picture::composite::share_pool`),
//! so painting and showing take one set of cores between them. A big
//! update is laid on several of its threads at once, the buffer cut into
//! bands of tile rows that each job owns ([`bands`]); the pixels do not
//! depend on how many there are.
//!
//! Where there are no threads (a [`JobPool::inline`], as in the browser)
//! the same work runs on the interface thread in slices: each
//! [`StrokeWorker::poll`] lays samples until about [`SLICE_PIXELS`]
//! pixels of work are done and leaves the rest for the next frame.
//!
//! What the worker paints with today is the round tip ([`round`]): the
//! pens and the pencils. Paint behaviours (mixing, textures, dabs) come
//! through the same worker as their painters land.

pub mod bands;
mod live;
pub mod round;

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{EntityId, JobHandle, JobPool, RasterTile2D};
use sue_model::BrushPreset;
use sue_model::brush::BrushRegistry;

use crate::buffer::{Combine, Counters, MergeError, TILE, TileKey};
use crate::input::{Pipeline, Sample, Settings};
use crate::raster::PixelRect;
use crate::raster::kernel::Ink;

use self::bands::{Bands, MOST_BANDS, Work};
pub use self::live::LiveView;
use self::round::Round;

/// About how many pixels of work one slice lays on a pool without
/// threads, so a frame keeps its time (a few milliseconds).
pub const SLICE_PIXELS: u64 = 1 << 20;

/// The pool painting shares with compositing: made on first use, one
/// core left for the interface (at most eight threads), and kept for the
/// life of the program. Where no thread can start it has none, and work
/// runs on the caller's thread.
#[must_use]
pub fn shared_pool() -> Arc<JobPool> {
    static POOL: OnceLock<Arc<JobPool>> = OnceLock::new();
    Arc::clone(POOL.get_or_init(|| Arc::new(JobPool::for_this_machine())))
}

/// What a stroke paints with.
#[derive(Clone, Debug)]
pub struct Brush {
    /// The brush.
    pub preset: BrushPreset,
    /// The keys of its parameters and inputs.
    pub registry: BrushRegistry,
    /// The paint's color, straight sRGB from 0 to 1 (an eraser ignores
    /// it).
    pub color: [f32; 3],
}

impl Brush {
    /// `preset` in `color`, with the standard parameters.
    #[must_use]
    pub fn new(preset: BrushPreset, color: [f32; 3]) -> Self {
        Self {
            preset,
            registry: BrushRegistry::standard(),
            color,
        }
    }

    /// What its strokes lay on the layer.
    #[must_use]
    pub fn ink(&self) -> Ink {
        let opacity = Round::new(&self.preset, &self.registry).opacity();
        if self.preset.erase {
            Ink::Erase { opacity }
        } else {
            Ink::Paint {
                color: self.color,
                opacity,
            }
        }
    }
}

/// One stroke: its pipeline, its coverage and what is shown of it.
#[derive(Debug)]
pub struct Stroke {
    pipeline: Pipeline,
    round: Round,
    bands: Bands,
    live: Arc<LiveView>,
    /// Tiles laid since the interface last asked.
    dirty: Vec<TileKey>,
    /// The open run has been closed after the lift.
    closed: bool,
}

impl Stroke {
    /// A stroke of `brush` drawn as `settings` say, its coverage in
    /// `bands` bands (one per thread that may lay it; 1 to 8).
    #[must_use]
    pub fn new(brush: &Brush, settings: Settings, bands: usize) -> Self {
        let combine = if brush.preset.build_up {
            Combine::BuildUp
        } else {
            Combine::Max
        };
        Self {
            pipeline: Pipeline::new(&brush.preset, &brush.registry, settings),
            round: Round::new(&brush.preset, &brush.registry),
            bands: Bands::new(combine, bands),
            live: Arc::new(LiveView::new(brush.ink())),
            dirty: Vec::new(),
            closed: false,
        }
    }

    /// What the compositor shows of it.
    #[must_use]
    pub fn live(&self) -> Arc<LiveView> {
        Arc::clone(&self.live)
    }

    /// `true` once the pen has lifted and everything is laid.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.closed
    }

    /// The work laid so far.
    #[must_use]
    pub fn counters(&self) -> Counters {
        self.bands.counters()
    }

    /// The coverage of layer pixel `(x, y)`, as the merge lays it.
    #[must_use]
    pub fn coverage(&self, x: i32, y: i32) -> u16 {
        self.bands.coverage(x, y)
    }

    /// The tiles laid since the last call, in order.
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        std::mem::take(&mut self.dirty)
    }

    /// Lays the samples at the front of `queue`, taking them out: all of
    /// them, or with `budget` until about that many pixels of work are
    /// done (at least one sample). Big work goes to `pool`'s threads.
    ///
    /// # Errors
    /// When a job of the pool failed: the stroke cannot be trusted.
    pub fn update(
        &mut self,
        queue: &mut VecDeque<Sample>,
        budget: Option<u64>,
        pool: Option<&JobPool>,
    ) -> Result<(), String> {
        if queue.is_empty() || self.closed {
            queue.clear();
            return Ok(());
        }
        let mut work = Work::default();
        let mut reach = 0;
        while let Some(sample) = queue.pop_front() {
            self.pipeline.push(sample);
            let before = work.committed.len();
            self.round
                .committed(self.pipeline.committed(), &mut work.committed);
            reach += work.committed[before..]
                .iter()
                .map(bands::reach)
                .sum::<u64>();
            if self.pipeline.is_done() {
                self.round.close(&mut work.committed);
                self.closed = true;
                queue.clear();
                break;
            }
            if budget.is_some_and(|budget| reach >= budget) {
                break;
            }
        }
        work.tail = Some(if self.closed {
            Vec::new()
        } else {
            self.round.tail(&self.pipeline.tail())
        });
        self.bands.lay(&Arc::new(work), pool)?;
        if self.closed {
            self.bands.settle();
        }
        let dirty = self.bands.take_dirty();
        self.live.show(&self.bands, &dirty);
        self.dirty.extend(dirty);
        self.dirty.sort_unstable();
        self.dirty.dedup();
        Ok(())
    }

    /// The layer pixels the stroke covers, tile by tile.
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        self.bands.bounds()
    }

    /// Merges the stroke into `raster` (layer `layer`, whose pixel
    /// `(0, 0)` is layer pixel `origin`) and answers the edit that did
    /// it, already applied: the bytes of each changed tile before and
    /// after, under `label`. A stroke that changed nothing answers an
    /// empty edit.
    ///
    /// # Errors
    /// For a raster without 8-bit color.
    pub fn commit(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        layer: EntityId,
        label: &str,
    ) -> Result<RasterEdit, MergeError> {
        let (width, height) = (raster.width() as i32, raster.height() as i32);
        let mut rects = Vec::new();
        for key in self.bands.keys() {
            let [tx, ty] = key.origin();
            let x0 = (tx - origin[0]).max(0);
            let x1 = (tx + TILE - origin[0]).min(width);
            let y0 = (ty - origin[1]).max(0);
            let y1 = (ty + TILE - origin[1]).min(height);
            if x0 < x1 && y0 < y1 {
                let rect = [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32];
                let Some(before) = RasterPatch::read(raster, rect[0], rect[1], rect[2], rect[3])
                else {
                    return Err(MergeError::Format(raster.format()));
                };
                rects.push((rect, before));
            }
        }
        self.bands.merge(raster, origin, self.live.ink())?;
        let patches = rects
            .into_iter()
            .filter_map(|(rect, before)| RasterPatch::capture(raster, rect, before))
            .filter(|patch| patch.before != patch.after)
            .collect();
        Ok(RasterEdit::new(layer, label, patches))
    }
}

/// Where the worker's stroke is.
#[derive(Debug)]
enum State {
    /// Here, waiting for samples.
    Idle(Box<Stroke>),
    /// In a job of the pool.
    Busy(JobHandle<Box<Stroke>>),
    /// A job failed; the stroke is lost.
    Failed,
}

/// A stroke being drawn: the interface queues samples and polls; the
/// stroke is laid on the shared pool (see the module).
#[derive(Debug)]
pub struct StrokeWorker {
    pool: Arc<JobPool>,
    queue: VecDeque<Sample>,
    state: State,
    live: Arc<LiveView>,
    dirty: Vec<TileKey>,
}

impl StrokeWorker {
    /// Starts a stroke of `brush`, drawn as `settings` say, laid on
    /// `pool`.
    #[must_use]
    pub fn start(pool: Arc<JobPool>, brush: &Brush, settings: Settings) -> Self {
        let bands = pool.threads().clamp(1, MOST_BANDS);
        let stroke = Stroke::new(brush, settings, bands);
        Self {
            live: stroke.live(),
            pool,
            queue: VecDeque::new(),
            state: State::Idle(Box::new(stroke)),
            dirty: Vec::new(),
        }
    }

    /// What the compositor shows of the stroke.
    #[must_use]
    pub fn live(&self) -> Arc<LiveView> {
        Arc::clone(&self.live)
    }

    /// Queues the device's next sample. Nothing is laid here.
    pub fn push(&mut self, sample: Sample) {
        self.queue.push_back(sample);
    }

    /// Queues samples, as a frame hands them over.
    pub fn push_all(&mut self, samples: &[Sample]) {
        self.queue.extend(samples.iter().copied());
    }

    /// `true` while a job lays the stroke.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(self.state, State::Busy(_))
    }

    /// `true` when a job failed and the stroke is lost.
    #[must_use]
    pub fn has_failed(&self) -> bool {
        matches!(self.state, State::Failed)
    }

    /// Samples queued and not laid yet.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.queue.len()
    }

    /// Takes back a finished job, and hands the queued samples to a new
    /// one (or, on a pool without threads, lays one slice of them here).
    /// Answers the tiles laid since the last poll, in order: what the
    /// compositor shows again.
    pub fn poll(&mut self) -> Vec<TileKey> {
        self.harvest(false);
        if let State::Idle(_) = self.state
            && !self.queue.is_empty()
        {
            self.kick();
        }
        // An inline pool finished its job as it was spawned.
        self.harvest(false);
        let mut dirty = std::mem::take(&mut self.dirty);
        dirty.sort_unstable();
        dirty.dedup();
        dirty
    }

    /// Takes a finished job's stroke back (`wait`: waits for it).
    fn harvest(&mut self, wait: bool) {
        let State::Busy(job) = &mut self.state else {
            return;
        };
        let result = if wait {
            let State::Busy(job) = std::mem::replace(&mut self.state, State::Failed) else {
                return;
            };
            job.wait()
        } else {
            match job.try_take() {
                Some(result) => result,
                None => return,
            }
        };
        self.state = match result {
            Ok(mut stroke) => {
                self.dirty.extend(stroke.take_dirty());
                State::Idle(stroke)
            }
            Err(_) => State::Failed,
        };
    }

    /// Sends the queued samples to be laid.
    fn kick(&mut self) {
        let State::Idle(mut stroke) = std::mem::replace(&mut self.state, State::Failed) else {
            return;
        };
        if self.pool.threads() == 0 {
            // No threads: one slice here, the rest on the next frames.
            self.state = match stroke.update(&mut self.queue, Some(SLICE_PIXELS), None) {
                Ok(()) => {
                    self.dirty.extend(stroke.take_dirty());
                    State::Idle(stroke)
                }
                Err(_) => State::Failed,
            };
            return;
        }
        let mut queue = std::mem::take(&mut self.queue);
        let pool = Arc::clone(&self.pool);
        self.state = State::Busy(self.pool.spawn("brush stroke", move |_| {
            stroke.update(&mut queue, None, Some(&pool))?;
            Ok(stroke)
        }));
    }

    /// Ends the stroke: waits for the job under way and lays every sample
    /// still queued (push the lift first). Answers the finished stroke to
    /// [commit](Stroke::commit), or `None` when a job failed. What is
    /// shown keeps showing until the caller [ends](LiveView::end) it,
    /// once the stroke is in its layer.
    #[must_use]
    pub fn finish(mut self) -> Option<Stroke> {
        self.harvest(true);
        let State::Idle(mut stroke) = std::mem::replace(&mut self.state, State::Failed) else {
            return None;
        };
        let pool = (self.pool.threads() > 0).then_some(self.pool.as_ref());
        stroke.update(&mut self.queue, None, pool).ok()?;
        if !stroke.closed {
            // The pen never said it lifted: the stroke ends as drawn.
            let work = Work {
                committed: stroke.round.tail(&stroke.pipeline.tail()),
                tail: Some(Vec::new()),
            };
            stroke.bands.lay(&Arc::new(work), pool).ok()?;
            stroke.bands.settle();
            stroke.closed = true;
            let dirty = stroke.bands.take_dirty();
            stroke.live.show(&stroke.bands, &dirty);
            stroke.dirty.extend(dirty);
        }
        Some(*stroke)
    }

    /// Gives the stroke up: nothing of it is shown, and nothing reaches a
    /// layer. A job under way finishes on its own and is dropped.
    pub fn abandon(self) {
        self.live.end();
    }
}
