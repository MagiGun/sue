//! The worker's painter for every paint behaviour: dabs that build up,
//! picture tips, paper, spray, color jitter, mixing and the blend tools
//! ([`PaintStroke`]), driven the way replay drives them
//! ([`crate::replay`]), so a stroke drawn live and the same stroke
//! painted again give the same bytes.
//!
//! Such a stroke cannot be shown as coverage and one ink: its dabs
//! differ in color, it mixes with the picture, it is shaped as a whole
//! (a watercolor edge, a dual brush, a paper laid once). So after each
//! update the painter lays the tiles that changed onto a snapshot of
//! the layer (D21), exactly as the merge on lift will, and the live view
//! shows those pixels. Symmetry's copies are one stroke here, as in
//! replay: where they overlap they lay paint once, as the strongest of
//! them; a mixing stroke's copies mix one after another.

use std::sync::Arc;

use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{EntityId, JobPool, PixelFormat, RasterTile2D};

use crate::buffer::{MergeError, TILE, TileKey};
use crate::input::{Kind, Pipeline, Sample, Settings};
use crate::paint::finish::Field;
use crate::paint::lay::{Lay, lay_field};
use crate::paint::wet::{Backdrop, RasterBackdrop};
use crate::paint::{Behaviours, Colors, PaintStroke, Resources};
use crate::raster::PixelRect;
use crate::raster::kernel::Channel;
use crate::replay::{strongest, through};

/// What a stroke with paint behaviours reads of its layer and how it
/// lays on it, beyond the brush.
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    /// A snapshot of the raster the stroke is drawn on (a clone, which
    /// shares the pixels, D21): what a mixing stroke picks up, and what
    /// the live view lays the stroke on. Its pixel `(0, 0)` is the
    /// stroke's pixel `(0, 0)`. Without one the layer reads as clear.
    pub snapshot: Option<RasterTile2D>,
    /// The layer's transparent pixels stay transparent (lock
    /// transparent pixels).
    pub keep_alpha: bool,
    /// Symmetry's copies, each `[a, b, c, d, e, f]` mapping `(x, y)` to
    /// `(a x + b y + e, c x + d y + f)`; empty for a stroke drawn once.
    pub copies: Vec<[f32; 6]>,
    /// Paint even a plain round tip as replay paints it, so the stroke
    /// kept (a vector layer's line) paints again to the same bytes.
    pub replay: bool,
}

/// A snapshot raster as the backdrop a wet stroke reads.
#[derive(Debug)]
struct Snapshot(Option<RasterTile2D>);

impl Backdrop for Snapshot {
    fn pixel(&self, x: i32, y: i32) -> [f32; 4] {
        self.0.as_ref().map_or([0.0; 4], |raster| {
            RasterBackdrop {
                raster,
                origin: [0, 0],
            }
            .pixel(x, y)
        })
    }
}

/// One copy of the stroke: its own pipeline and paint.
#[derive(Clone, Debug)]
struct Copy {
    transform: Option<[f32; 6]>,
    pipeline: Pipeline,
    paint: PaintStroke,
}

/// A stroke with paint behaviours, painted on the worker.
#[derive(Debug)]
pub(crate) struct Painter {
    /// The stroke itself. Shared only while the tiles it changed are
    /// laid for the live view, with the jobs that lay them.
    core: Arc<Core>,
    /// The threads big work is shared with, when there are some.
    pool: Option<Arc<JobPool>>,
}

/// The stroke a [`Painter`] paints: what laying a tile reads.
#[derive(Clone, Debug)]
pub(crate) struct Core {
    copies: Vec<Copy>,
    lay: Lay,
    wet: bool,
    snapshot: Arc<Snapshot>,
    keep_alpha: bool,
    closed: bool,
}

/// Tiles laid for the live view: each with its pixels (straight RGBA8),
/// or `None` where it shows the layer as it was.
type LaidTiles = Vec<(TileKey, Option<Box<[u8]>>)>;

/// Fewer changed tiles than this are laid on one thread.
const WORTH_SHARING: usize = 12;

impl std::ops::Deref for Painter {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.core
    }
}

impl std::ops::DerefMut for Painter {
    /// The stroke, to change it. No job holds it between two calls of
    /// [`Painter::laid`], so nothing is copied.
    fn deref_mut(&mut self) -> &mut Core {
        Arc::make_mut(&mut self.core)
    }
}

impl Painter {
    /// A stroke of `preset` in `colors`, reading `resources`, drawn as
    /// `settings` say, on `canvas`.
    pub(crate) fn new(
        preset: &sue_model::BrushPreset,
        resources: &Resources,
        colors: Colors,
        settings: Settings,
        canvas: Canvas,
    ) -> Self {
        let registry = crate::paint::registry();
        let behaviours = Behaviours::from_preset(preset);
        let transforms: Vec<Option<[f32; 6]>> = if canvas.copies.is_empty() {
            vec![None]
        } else {
            canvas.copies.iter().copied().map(Some).collect()
        };
        let copies: Vec<Copy> = transforms
            .into_iter()
            .map(|transform| {
                let pipeline = Pipeline::new(preset, &registry, settings);
                let paint = PaintStroke::new(
                    preset,
                    &behaviours,
                    resources.clone(),
                    colors,
                    settings.seed,
                    pipeline.extra_keys(),
                );
                Copy {
                    transform,
                    pipeline,
                    paint,
                }
            })
            .collect();
        let lay = copies[0].paint.lay();
        let wet = copies[0].paint.is_wet();
        Self {
            core: Arc::new(Core {
                copies,
                lay,
                wet,
                snapshot: Arc::new(Snapshot(canvas.snapshot)),
                keep_alpha: canvas.keep_alpha,
                closed: false,
            }),
            pool: None,
        }
    }

    /// Lets the stroke share its big work (a big dab, many changed
    /// tiles to lay) with `pool`'s threads. The bytes stay the same.
    pub(crate) fn share(&mut self, pool: Arc<JobPool>) {
        let bands = pool.threads().clamp(1, super::MOST_BANDS);
        for copy in &mut self.copies {
            copy.paint.share(Arc::clone(&pool), bands);
        }
        self.pool = Some(pool);
    }

    /// What went wrong on the pool's threads, if anything did.
    pub(crate) fn failure(&self) -> Option<String> {
        self.copies
            .iter()
            .find_map(|copy| copy.paint.failure().map(str::to_owned))
    }

    /// Lets go of the snapshot of the layer (see
    /// [`super::Stroke::let_go_of_layer`]).
    pub(crate) fn let_go_of_layer(&mut self) {
        self.snapshot = Arc::new(Snapshot(None));
    }

    /// `true` once the pen has lifted and everything is laid.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    /// Takes one sample. Answers about how many pixels of work it was.
    pub(crate) fn push(&mut self, sample: Sample) -> u64 {
        let snapshot = Arc::clone(&self.snapshot);
        let backdrop: Option<&dyn Backdrop> =
            self.wet.then_some(snapshot.as_ref() as &dyn Backdrop);
        let mut work = 0_u64;
        let core: &mut Core = self;
        for copy in &mut core.copies {
            let sample = copy
                .transform
                .as_ref()
                .map_or(sample, |m| through(&sample, m));
            copy.pipeline.push(sample);
            // The points are the paint's from here: the pipeline keeps
            // none of them, however long the stroke gets.
            let fresh = copy.pipeline.take_committed();
            if !fresh.is_empty() {
                work += fresh
                    .iter()
                    .map(|p| {
                        let d = f64::from(p.size.max(1.0));
                        (d * d) as u64
                    })
                    .sum::<u64>();
                copy.paint.commit(&fresh, backdrop);
            }
            if sample.kind != Kind::Up {
                let tail = copy.pipeline.tail();
                copy.paint.hold(sample.time, tail.last(), backdrop);
            }
        }
        if core.copies.iter().all(|copy| copy.pipeline.is_done()) {
            for copy in &mut core.copies {
                copy.paint.finish(backdrop);
            }
            core.closed = true;
        }
        work
    }

    /// How many stroke points the stroke holds on to (its pipelines'):
    /// bounded, however long the stroke is.
    pub(crate) fn held(&self) -> usize {
        self.copies.iter().map(|copy| copy.pipeline.held()).sum()
    }

    /// Draws the provisional tail of every copy.
    pub(crate) fn tail(&mut self) {
        if self.closed {
            return;
        }
        for copy in &mut self.copies {
            let tail = copy.pipeline.tail();
            copy.paint.tail(&tail);
        }
    }

    /// The pen never said it lifted: the stroke ends as drawn.
    pub(crate) fn close(&mut self) {
        if self.closed {
            return;
        }
        let snapshot = Arc::clone(&self.snapshot);
        let backdrop: Option<&dyn Backdrop> =
            self.wet.then_some(snapshot.as_ref() as &dyn Backdrop);
        for copy in &mut self.copies {
            // What the pipeline still holds becomes final.
            let tail = copy.pipeline.tail();
            copy.paint.commit(&tail, backdrop);
            copy.paint.finish(backdrop);
        }
        self.closed = true;
    }

    /// The tiles whose laid pixels changed since the last call.
    pub(crate) fn take_dirty(&mut self) -> Vec<TileKey> {
        let mut keys: Vec<TileKey> = self
            .copies
            .iter_mut()
            .flat_map(|copy| copy.paint.take_dirty())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// The laid pixels of tiles `keys`, for the live view: many of them
    /// on the pool's threads at once, each tile on its own.
    ///
    /// # Errors
    /// When a job of the pool failed.
    pub(crate) fn laid(&self, keys: &[TileKey]) -> Result<LaidTiles, String> {
        let lay = |core: &Core, keys: &[TileKey]| -> LaidTiles {
            keys.iter().map(|&key| (key, core.laid_tile(key))).collect()
        };
        let shared = self
            .pool
            .as_ref()
            .filter(|pool| pool.threads() > 1 && keys.len() >= WORTH_SHARING);
        let Some(pool) = shared else {
            return Ok(lay(&self.core, keys));
        };
        let runs = pool.threads().clamp(1, super::MOST_BANDS);
        let per = keys.len().div_ceil(runs);
        let mut chunks = keys.chunks(per);
        let first = chunks.next().unwrap_or(&[]);
        let jobs: Vec<_> = chunks
            .map(|chunk| {
                let (core, chunk) = (Arc::clone(&self.core), chunk.to_vec());
                pool.spawn("brush tiles", move |_| {
                    let laid = lay(&core, &chunk);
                    // Let go before the answer goes back, so the stroke
                    // is the painter's alone again.
                    drop(core);
                    Ok(laid)
                })
            })
            .collect();
        let mut laid = lay(&self.core, first);
        let mut failed = None;
        for job in jobs {
            match job.wait() {
                Ok(part) => laid.extend(part),
                Err(error) => failed = Some(error.to_string()),
            }
        }
        failed.map_or(Ok(laid), Err)
    }
}

impl Core {
    /// The pixels the stroke spans so far.
    pub(crate) fn bounds(&self) -> Option<PixelRect> {
        self.copies
            .iter()
            .filter_map(|copy| copy.paint.bounds())
            .reduce(|a, b| PixelRect {
                x0: a.x0.min(b.x0),
                y0: a.y0.min(b.y0),
                x1: a.x1.max(b.x1),
                y1: a.y1.max(b.y1),
            })
    }

    /// The stroke's field over `rect`, its copies laid once.
    fn field(&self, rect: PixelRect, live: bool) -> Option<Field> {
        let fields: Vec<Field> = self
            .copies
            .iter()
            .filter_map(|copy| copy.paint.field(rect, live))
            .collect();
        if let [only] = fields.as_slice() {
            return Some(only.clone());
        }
        strongest(&fields)
    }

    /// Tile `key` of the layer with the stroke laid on it, straight
    /// RGBA8, as the merge lays it; `None` where it is the layer as it
    /// was (or lies outside it).
    fn laid_tile(&self, key: TileKey) -> Option<Box<[u8]>> {
        let [ox, oy] = key.origin();
        let snapshot = self.snapshot.0.as_ref();
        let (w, h) = snapshot.map_or((i32::MAX, i32::MAX), |r| {
            (r.width() as i32, r.height() as i32)
        });
        if ox >= w || oy >= h || ox + TILE <= 0 || oy + TILE <= 0 {
            return None;
        }
        // The layer's pixels of the tile, as they were.
        let mut tile = RasterTile2D::new(TILE as u32, TILE as u32, PixelFormat::Rgba8).ok()?;
        if let Some(raster) = snapshot
            && let (Some(from), Some(into)) = (raster.bytes(), tile.bytes_mut())
            && raster.format() == PixelFormat::Rgba8
        {
            for y in oy.max(0)..(oy + TILE).min(h) {
                let x0 = ox.max(0);
                let x1 = (ox + TILE).min(w);
                let src = ((y * w + x0) * 4) as usize;
                let dst = (((y - oy) * TILE + (x0 - ox)) * 4) as usize;
                let len = ((x1 - x0) * 4) as usize;
                into[dst..dst + len].copy_from_slice(&from[src..src + len]);
            }
        }
        let before = tile.bytes()?.to_vec();
        if self.wet {
            let backdrop = self.snapshot.as_ref();
            let bytes = tile.bytes_mut()?;
            // Each copy over the ones before it, where it mixed.
            for copy in &self.copies {
                copy.paint
                    .wet_tile(backdrop, key, self.keep_alpha, |at, pixel| {
                        for (b, v) in bytes[at * 4..at * 4 + 4].iter_mut().zip(pixel) {
                            *b = u8::from_unit(v);
                        }
                    });
            }
        } else {
            let rect = PixelRect {
                x0: ox,
                y0: oy,
                x1: ox + TILE,
                y1: oy + TILE,
            };
            let field = self.field(rect, !self.closed)?;
            lay_field(&field, &mut tile, [ox, oy], self.lay, self.keep_alpha).ok()?;
        }
        let bytes = tile.bytes()?;
        (bytes != before.as_slice()).then(|| bytes.to_vec().into_boxed_slice())
    }

    /// Merges the finished stroke into `raster` (its pixel `(0, 0)` is
    /// the stroke's pixel `origin`) and answers the edit that did it,
    /// already applied, under `label`.
    pub(crate) fn commit(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        layer: EntityId,
        label: &str,
    ) -> Result<RasterEdit, MergeError> {
        if !matches!(raster.format(), PixelFormat::Rgba8 | PixelFormat::RgbaF32) {
            return Err(MergeError::Format(raster.format()));
        }
        let (width, height) = (raster.width() as i32, raster.height() as i32);
        let Some(bounds) = self.bounds() else {
            return Ok(RasterEdit::new(layer, label, Vec::new()));
        };
        // Tile by tile, so an undo keeps only what the stroke touched.
        let mut rects = Vec::new();
        let (k0, k1) = (
            TileKey::of_pixel(bounds.x0 - origin[0], bounds.y0 - origin[1]),
            TileKey::of_pixel(bounds.x1 - 1 - origin[0], bounds.y1 - 1 - origin[1]),
        );
        for ty in k0.y..=k1.y {
            for tx in k0.x..=k1.x {
                let x0 = (tx * TILE).max(0);
                let y0 = (ty * TILE).max(0);
                let x1 = ((tx + 1) * TILE).min(width);
                let y1 = ((ty + 1) * TILE).min(height);
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                let rect = [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32];
                let Some(before) = RasterPatch::read(raster, rect[0], rect[1], rect[2], rect[3])
                else {
                    return Err(MergeError::Format(raster.format()));
                };
                rects.push((rect, before));
            }
        }
        self.merge(raster, origin)?;
        let patches = rects
            .into_iter()
            .filter_map(|(rect, before)| RasterPatch::capture(raster, rect, before))
            .filter(|patch| patch.before != patch.after)
            .collect();
        Ok(RasterEdit::new(layer, label, patches))
    }

    /// Lays the finished stroke on `raster`.
    fn merge(&self, raster: &mut RasterTile2D, origin: [i32; 2]) -> Result<(), MergeError> {
        if self.wet {
            for copy in &self.copies {
                copy.paint.merge(raster, origin, self.keep_alpha)?;
            }
            return Ok(());
        }
        if let [only] = self.copies.as_slice() {
            only.paint.merge(raster, origin, self.keep_alpha)?;
            return Ok(());
        }
        let Some(rect) = self.bounds() else {
            return Ok(());
        };
        if let Some(field) = self.field(rect, false) {
            lay_field(&field, raster, origin, self.lay, self.keep_alpha)?;
        }
        Ok(())
    }
}
