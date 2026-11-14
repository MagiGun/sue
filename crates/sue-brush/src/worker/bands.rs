//! The stroke buffer cut into bands of tile rows, so several threads can
//! lay one stroke's coverage at once and every pixel still comes out the
//! same as on one thread.
//!
//! Band `k` of `n` owns the tile rows `y` with `y mod n == k`: rows
//! interleaved, so a brush of any size spreads over the bands. Each band
//! is a [`StrokeBuffer`] of its own, an owned value that moves into a job
//! and comes back (D2: jobs own their tiles). Every band lays the same
//! shapes in the same order, each only on its own rows, so the coverage
//! of a pixel is the same sequence of combines whatever `n` is: the
//! pixels do not depend on the thread count.

use std::sync::Arc;

use sue_core::{JobPool, RasterTile2D};

use crate::buffer::{AREA, Combine, Counters, MergeError, StrokeBuffer, TILE, TileKey};
use crate::raster::kernel::Ink;
use crate::raster::{PixelRect, Shape};

use super::round::Swept;

/// The most bands a stroke is cut into.
pub const MOST_BANDS: usize = 8;

/// Below this many pixels of shape bounds an update is laid on the
/// calling thread: handing it out would cost more than it saves.
const WORTH_SHARING: u64 = 48 * 1024;

/// A shape laid only on the rows of one band.
#[derive(Clone, Copy, Debug)]
struct InBand<'a, S> {
    shape: &'a S,
    band: i32,
    bands: i32,
}

impl<S: Shape> Shape for InBand<'_, S> {
    fn bounds(&self) -> PixelRect {
        self.shape.bounds()
    }

    fn span(&self, y: f32) -> Option<(f32, f32)> {
        let row = (y.floor() as i32).div_euclid(TILE);
        (row.rem_euclid(self.bands) == self.band)
            .then(|| self.shape.span(y))
            .flatten()
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        self.shape.coverage(p)
    }
}

/// The pixels `shape`'s bounds hold.
#[must_use]
pub fn reach(shape: &Swept) -> u64 {
    let r = shape.bounds();
    if r.is_empty() {
        0
    } else {
        (i64::from(r.x1) - i64::from(r.x0)) as u64 * (i64::from(r.y1) - i64::from(r.y0)) as u64
    }
}

/// What one update lays: shapes for the committed part, and the whole
/// new tail (`None`: the tail stays as it is).
#[derive(Clone, Debug, Default)]
pub struct Work {
    /// Shapes added to the committed part, in order.
    pub committed: Vec<Swept>,
    /// The tail drawn afresh, in order; `Some(empty)` clears it.
    pub tail: Option<Vec<Swept>>,
}

impl Work {
    /// The pixels the shapes' bounds hold: what the update may cost.
    #[must_use]
    pub fn reach(&self) -> u64 {
        self.committed
            .iter()
            .chain(self.tail.iter().flatten())
            .map(reach)
            .sum()
    }

    /// `true` when there is nothing to lay.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.committed.is_empty() && self.tail.is_none()
    }
}

/// Lays `work` in `buffer`, on the rows of band `band` of `bands`.
fn lay(buffer: &mut StrokeBuffer, work: &Work, band: usize, bands: usize) {
    let (band, bands) = (band as i32, bands as i32);
    if !work.committed.is_empty() {
        let mut painter = buffer.committed();
        for shape in &work.committed {
            painter.fill(&InBand { shape, band, bands });
        }
    }
    if let Some(tail) = &work.tail {
        let mut painter = buffer.tail();
        for shape in tail {
            painter.fill(&InBand { shape, band, bands });
        }
    }
}

/// One stroke's coverage, in bands of tile rows.
#[derive(Clone, Debug)]
pub struct Bands {
    parts: Vec<StrokeBuffer>,
}

impl Bands {
    /// Empty coverage in `count` bands (1 to [`MOST_BANDS`]) that adds up
    /// by `combine`.
    #[must_use]
    pub fn new(combine: Combine, count: usize) -> Self {
        Self {
            parts: vec![StrokeBuffer::new(combine); count.clamp(1, MOST_BANDS)],
        }
    }

    /// How many bands.
    #[must_use]
    pub fn count(&self) -> usize {
        self.parts.len()
    }

    /// The band holding tile `key`.
    fn part(&self, key: TileKey) -> &StrokeBuffer {
        &self.parts[key.y.rem_euclid(self.parts.len() as i32) as usize]
    }

    /// Lays `work`: each band on a job of `pool` when the work is big
    /// enough and the pool has threads, else all on this thread. The
    /// result is the same either way.
    ///
    /// # Errors
    /// When a band's job failed (it panicked): its band is lost and the
    /// stroke cannot be trusted any more.
    pub fn lay(&mut self, work: &Arc<Work>, pool: Option<&JobPool>) -> Result<(), String> {
        let bands = self.parts.len();
        let shared =
            pool.filter(|pool| pool.threads() > 1 && bands > 1 && work.reach() >= WORTH_SHARING);
        let Some(pool) = shared else {
            for (band, buffer) in self.parts.iter_mut().enumerate() {
                lay(buffer, work, band, bands);
            }
            return Ok(());
        };
        // Bands 1.. go to jobs that own them; band 0 is laid here
        // meanwhile.
        let jobs: Vec<_> = self.parts[1..]
            .iter_mut()
            .enumerate()
            .map(|(index, buffer)| {
                let mut owned = std::mem::take(buffer);
                let work = Arc::clone(work);
                pool.spawn("brush band", move |_| {
                    lay(&mut owned, &work, index + 1, bands);
                    Ok(owned)
                })
            })
            .collect();
        lay(&mut self.parts[0], work, 0, bands);
        let mut failed = None;
        for (slot, job) in self.parts[1..].iter_mut().zip(jobs) {
            match job.wait() {
                Ok(buffer) => *slot = buffer,
                Err(error) => failed = Some(error.to_string()),
            }
        }
        failed.map_or(Ok(()), Err)
    }

    /// Folds every band's tail into its committed part (on lift).
    pub fn settle(&mut self) {
        for part in &mut self.parts {
            part.settle();
        }
    }

    /// The tiles that changed since the last call, in order.
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        let mut keys: Vec<TileKey> = self
            .parts
            .iter_mut()
            .flat_map(StrokeBuffer::take_dirty)
            .collect();
        keys.sort_unstable();
        keys
    }

    /// The tiles holding coverage, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<TileKey> {
        let mut keys: Vec<TileKey> = self.parts.iter().flat_map(StrokeBuffer::keys).collect();
        keys.sort_unstable();
        keys
    }

    /// `true` when nothing is painted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(StrokeBuffer::is_empty)
    }

    /// The coverage of layer pixel `(x, y)`.
    #[must_use]
    pub fn coverage(&self, x: i32, y: i32) -> u16 {
        self.part(TileKey::of_pixel(x, y)).coverage(x, y)
    }

    /// Tile `key`'s coverage into `out` (see
    /// [`StrokeBuffer::tile_coverage`]).
    ///
    /// # Panics
    /// When `out` is shorter than [`AREA`].
    pub fn tile_coverage(&self, key: TileKey, out: &mut [u16]) -> bool {
        assert!(out.len() >= AREA, "a tile's coverage needs AREA values");
        self.part(key).tile_coverage(key, out)
    }

    /// The pixels the bands' tiles span.
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        self.parts
            .iter()
            .filter_map(StrokeBuffer::bounds)
            .reduce(|a, b| PixelRect {
                x0: a.x0.min(b.x0),
                y0: a.y0.min(b.y0),
                x1: a.x1.max(b.x1),
                y1: a.y1.max(b.y1),
            })
    }

    /// The work every band has done, added up.
    #[must_use]
    pub fn counters(&self) -> Counters {
        self.parts
            .iter()
            .map(StrokeBuffer::counters)
            .fold(Counters::default(), |sum, c| Counters {
                pixels: sum.pixels + c.pixels,
                tail_pixels: sum.tail_pixels + c.tail_pixels,
                tail_tiles_cleared: sum.tail_tiles_cleared + c.tail_tiles_cleared,
            })
    }

    /// Lays the stroke on `raster` (pixel `(0, 0)` is layer pixel
    /// `origin`) with `ink`. The rectangles it changed, in tile order.
    ///
    /// # Errors
    /// As [`StrokeBuffer::merge`].
    pub fn merge(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        ink: Ink,
    ) -> Result<Vec<PixelRect>, MergeError> {
        let mut changed = Vec::new();
        for part in &self.parts {
            changed.extend(part.merge(raster, origin, ink)?);
        }
        changed.sort_unstable_by_key(|r| (r.y0, r.x0));
        Ok(changed)
    }
}
