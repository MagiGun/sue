//! # The stroke buffer
//!
//! A stroke does not paint on its layer while the pen is down. It lays
//! coverage into its own tiled buffer, which the compositor shows over
//! the layer, and which is merged into the layer once, on lift
//! ([`StrokeBuffer::merge`]). Because coverage is combined in the buffer
//! first ([`Combine`]), a stroke that crosses itself never lays its paint
//! twice: the opacity of a stroke is the opacity of the stroke, not of
//! each dab. This generalises the legacy engine's per-tile `before` bytes
//! and coverage mask (`TileState` in `paint/brush.rs`) and replaces its
//! restore-and-repaint on lift.
//!
//! The buffer has two parts, after the input pipeline's two outputs
//! (D2): the **committed** part, whose points never change, only ever
//! gains coverage; the **tail**, which may still change (the stabilizer
//! catching up, the end taper, a flick), is cleared and drawn again at
//! every update. Only tiles the tail touched are cleared, so an update
//! costs what the tail costs, however long the stroke already is
//! ([`Counters`] measures it). On lift the last tail is folded into the
//! committed part ([`StrokeBuffer::settle`]).
//!
//! Tiles are owned values in a sorted map, so the buffer can move to a
//! worker and back (P3-6) and every walk over it goes the same way.

mod tile;

use std::collections::BTreeMap;

use sue_core::{PixelFormat, RasterTile2D};

use crate::raster::kernel::{Channel, FULL, Ink, paint_row};
use crate::raster::{Capsule, Dab, Disc, PixelRect, Ribbon, RoundTip, Shape};
use tile::Tile;
pub use tile::{AREA, Combine, TILE, TileKey};

/// Which part of the buffer a painter lays coverage in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The part that never changes again.
    Committed,
    /// The part that is drawn again at every update.
    Tail,
}

/// What the buffer has done since it was made or its counters were
/// reset: the work an update costs, so a test can hold it to a budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Pixels whose coverage a primitive worked out.
    pub pixels: u64,
    /// Pixels worked out for the tail alone.
    pub tail_pixels: u64,
    /// Tail tiles cleared to draw the tail again.
    pub tail_tiles_cleared: u64,
}

/// Why a stroke could not be merged into a raster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeError {
    /// The raster holds no color to paint (a gray mask); masks get their
    /// own merge with the selection and mask work.
    Format(PixelFormat),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(format) => write!(f, "a stroke cannot be merged into a {format:?} raster"),
        }
    }
}

impl std::error::Error for MergeError {}

/// The coverage of one stroke, in tiles of [`TILE`] pixels, in layer
/// coordinates.
#[derive(Clone, Debug, Default)]
pub struct StrokeBuffer {
    combine: Combine,
    tiles: BTreeMap<TileKey, Tile>,
    /// The tiles the tail lies on, each with the pixels of it the tail
    /// may cover.
    tail: BTreeMap<TileKey, PixelRect>,
    /// The tiles changed since [`Self::take_dirty`] was asked, each with
    /// the pixels of it that may have changed.
    dirty: BTreeMap<TileKey, PixelRect>,
    counters: Counters,
}

impl StrokeBuffer {
    /// An empty buffer whose coverage adds up by `combine`.
    #[must_use]
    pub fn new(combine: Combine) -> Self {
        Self {
            combine,
            ..Self::default()
        }
    }

    /// How coverage adds up in this buffer.
    #[must_use]
    pub const fn combine(&self) -> Combine {
        self.combine
    }

    /// A painter that lays coverage in the committed part.
    pub fn committed(&mut self) -> Painter<'_> {
        Painter {
            buffer: self,
            part: Part::Committed,
        }
    }

    /// Clears the tail and hands a painter that draws the new one.
    pub fn tail(&mut self) -> Painter<'_> {
        self.clear_tail();
        Painter {
            buffer: self,
            part: Part::Tail,
        }
    }

    /// Clears the tail: the tiles it touched lose their tail coverage and
    /// are marked for showing again.
    pub fn clear_tail(&mut self) {
        for (key, rect) in std::mem::take(&mut self.tail) {
            if let Some(tile) = self.tiles.get_mut(&key) {
                tile.tail = None;
                self.counters.tail_tiles_cleared += 1;
                if tile.is_empty() {
                    self.tiles.remove(&key);
                }
            }
            mark(&mut self.dirty, key, rect);
        }
    }

    /// Folds the tail into the committed part (on lift, when the tail is
    /// final), leaving no tail.
    pub fn settle(&mut self) {
        let combine = self.combine;
        for (key, _) in std::mem::take(&mut self.tail) {
            let Some(tile) = self.tiles.get_mut(&key) else {
                continue;
            };
            let Some(tail) = tile.tail.take() else {
                continue;
            };
            match &mut tile.committed {
                Some(committed) => {
                    for (c, &t) in committed.iter_mut().zip(tail.iter()) {
                        *c = combine.apply(*c, t);
                    }
                }
                none => *none = Some(tail),
            }
        }
    }

    /// `true` when nothing has been painted (or all of it was tail that
    /// was cleared).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The tiles that hold coverage, in order.
    pub fn keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.tiles.keys().copied()
    }

    /// The tiles that hold tail coverage, in order.
    pub fn tail_keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.tail.keys().copied()
    }

    /// The tiles that changed since the last call, in order: what the
    /// compositor shows again.
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        std::mem::take(&mut self.dirty).into_keys().collect()
    }

    /// [`Self::take_dirty`] with, for each tile, the pixels of it that
    /// may have changed (a rectangle around them): what a view that
    /// reads around each pixel (a watercolor edge) needs to know how far
    /// the change reaches, so it shows no more tiles again than it must.
    pub fn take_dirty_rects(&mut self) -> Vec<(TileKey, PixelRect)> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// The work done so far.
    #[must_use]
    pub const fn counters(&self) -> Counters {
        self.counters
    }

    /// Starts the counters again from zero.
    pub fn reset_counters(&mut self) {
        self.counters = Counters::default();
    }

    /// The coverage of layer pixel `(x, y)`, committed and tail combined.
    #[must_use]
    pub fn coverage(&self, x: i32, y: i32) -> u16 {
        let key = TileKey::of_pixel(x, y);
        let Some(tile) = self.tiles.get(&key) else {
            return 0;
        };
        let [ox, oy] = key.origin();
        let index = ((y - oy) * TILE + (x - ox)) as usize;
        let committed = tile.committed.as_ref().map_or(0, |c| c[index]);
        let tail = tile.tail.as_ref().map_or(0, |t| t[index]);
        self.combine.apply(committed, tail)
    }

    /// The coverage of tile `key` as the layer sees it, row by row, into
    /// `out` (at least [`AREA`] long); `false`, and `out` cleared, when
    /// the tile holds nothing.
    ///
    /// # Panics
    /// When `out` is shorter than [`AREA`].
    pub fn tile_coverage(&self, key: TileKey, out: &mut [u16]) -> bool {
        match self.tiles.get(&key) {
            Some(tile) => {
                tile.combined(self.combine, out);
                true
            }
            None => {
                out[..AREA].fill(0);
                false
            }
        }
    }

    /// The pixels the buffer's tiles span, or `None` when it is empty.
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        let mut keys = self.tiles.keys();
        let first = keys.next()?;
        let mut rect = PixelRect {
            x0: first.x,
            y0: first.y,
            x1: first.x,
            y1: first.y,
        };
        for key in self.tiles.keys() {
            rect.x0 = rect.x0.min(key.x);
            rect.y0 = rect.y0.min(key.y);
            rect.x1 = rect.x1.max(key.x);
            rect.y1 = rect.y1.max(key.y);
        }
        Some(PixelRect {
            x0: rect.x0 * TILE,
            y0: rect.y0 * TILE,
            x1: (rect.x1 + 1) * TILE,
            y1: (rect.y1 + 1) * TILE,
        })
    }

    /// Lays the stroke on `raster`, whose pixel `(0, 0)` is layer pixel
    /// `origin`, with `ink`: each pixel once, by its combined coverage.
    /// Returns the rectangles of layer pixels it changed, one per tile,
    /// in order (what an undo record keeps). The buffer is unchanged, so
    /// the compositor can show the same coverage over a layer view.
    ///
    /// # Errors
    /// [`MergeError::Format`] for a raster with no color (`Gray8`).
    pub fn merge(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        ink: Ink,
    ) -> Result<Vec<PixelRect>, MergeError> {
        let (width, height) = (raster.width() as i32, raster.height() as i32);
        match raster.format() {
            PixelFormat::Rgba8 => {
                let Some(bytes) = raster.bytes_mut() else {
                    return Err(MergeError::Format(PixelFormat::Rgba8));
                };
                Ok(self.merge_samples(bytes, width, height, origin, ink))
            }
            PixelFormat::RgbaF32 => {
                let Some(samples) = raster.f32_samples_mut() else {
                    return Err(MergeError::Format(PixelFormat::RgbaF32));
                };
                Ok(self.merge_samples(samples, width, height, origin, ink))
            }
            other => Err(MergeError::Format(other)),
        }
    }

    /// [`Self::merge`] over RGBA samples of any type.
    fn merge_samples<C: Channel>(
        &self,
        samples: &mut [C],
        width: i32,
        height: i32,
        origin: [i32; 2],
        ink: Ink,
    ) -> Vec<PixelRect> {
        let mut changed = Vec::new();
        let mut coverage = vec![0_u16; AREA];
        let stride = width as usize * 4;
        for (&key, tile) in &self.tiles {
            let [tx, ty] = key.origin();
            // The tile in raster pixels, cut to the raster.
            let x0 = (tx - origin[0]).max(0);
            let x1 = (tx + TILE - origin[0]).min(width);
            let y0 = (ty - origin[1]).max(0);
            let y1 = (ty + TILE - origin[1]).min(height);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            tile.combined(self.combine, &mut coverage);
            for ry in y0..y1 {
                let row_in_tile = (ry + origin[1] - ty) as usize;
                let from = (x0 + origin[0] - tx) as usize;
                let to = (x1 + origin[0] - tx) as usize;
                let cover =
                    &coverage[row_in_tile * TILE as usize + from..row_in_tile * TILE as usize + to];
                let start = ry as usize * stride + x0 as usize * 4;
                let end = ry as usize * stride + x1 as usize * 4;
                paint_row(&mut samples[start..end], cover, ink);
            }
            changed.push(PixelRect {
                x0: x0 + origin[0],
                y0: y0 + origin[1],
                x1: x1 + origin[0],
                y1: y1 + origin[1],
            });
        }
        changed
    }
}

/// Lays primitives in one part of a [`StrokeBuffer`].
#[derive(Debug)]
pub struct Painter<'a> {
    buffer: &'a mut StrokeBuffer,
    part: Part,
}

impl Painter<'_> {
    /// The part this painter lays coverage in.
    #[must_use]
    pub const fn part(&self) -> Part {
        self.part
    }

    /// The round tip swept from `a` to `b` (an uneven capsule).
    pub fn capsule(&mut self, a: Disc, b: Disc, tip: RoundTip) {
        self.fill(&Capsule::new(a.drawn(false), b.drawn(false), tip));
    }

    /// The round tip swept along `discs`, one capsule a segment; a single
    /// disc is a dot (a tap).
    pub fn polyline(&mut self, discs: &[Disc], tip: RoundTip) {
        match discs {
            [] => {}
            [only] => self.capsule(*only, *only, tip),
            _ => {
                for pair in discs.windows(2) {
                    self.capsule(pair[0], pair[1], tip);
                }
            }
        }
    }

    /// One dab.
    pub fn dab(&mut self, dab: &Dab<'_>) {
        self.fill(dab);
    }

    /// A ribbon.
    pub fn ribbon(&mut self, ribbon: &Ribbon<'_>) {
        self.fill(ribbon);
    }

    /// Lays `shape`'s coverage, pixel by pixel within its bounds and row
    /// spans, combining it with what the part holds. The work is row by
    /// row over each row's span only, so a long thin shape at a slant
    /// costs its pixels, not the tiles of its bounding box.
    pub fn fill(&mut self, shape: &impl Shape) {
        let rect = shape.bounds();
        if rect.is_empty() {
            return;
        }
        let combine = self.buffer.combine;
        let mut values = [0_u16; TILE as usize];
        let mut worked = 0_u64;
        for y in rect.y0..rect.y1 {
            let cy = y as f32 + 0.5;
            let Some((lo, hi)) = shape.span(cy) else {
                continue;
            };
            // Pixels whose centers lie within the span.
            let from = rect.x0.max(clamp_to_i32((lo - 0.5).ceil()));
            let to = rect
                .x1
                .min(clamp_to_i32((hi - 0.5).floor()).saturating_add(1));
            if from >= to {
                continue;
            }
            worked += (to - from) as u64;
            // The span cut at tile borders.
            let mut x0 = from;
            while x0 < to {
                let key = TileKey::of_pixel(x0, y);
                let [ox, oy] = key.origin();
                let x1 = to.min(ox + TILE);
                let mut any = false;
                for x in x0..x1 {
                    let q = quantize(shape.coverage([x as f32 + 0.5, cy]));
                    values[(x - x0) as usize] = q;
                    any |= q > 0;
                }
                if any {
                    let tile = self.buffer.tiles.entry(key).or_default();
                    let slot = match self.part {
                        Part::Committed => &mut tile.committed,
                        Part::Tail => &mut tile.tail,
                    };
                    let part = Tile::part(slot);
                    let row = ((y - oy) * TILE) as usize;
                    for (x, &q) in (x0..x1).zip(&values) {
                        let i = row + (x - ox) as usize;
                        part[i] = combine.apply(part[i], q);
                    }
                    let run = PixelRect {
                        x0,
                        y0: y,
                        x1,
                        y1: y + 1,
                    };
                    if self.part == Part::Tail {
                        mark(&mut self.buffer.tail, key, run);
                    }
                    mark(&mut self.buffer.dirty, key, run);
                }
                x0 = x1;
            }
        }
        self.buffer.counters.pixels += worked;
        if self.part == Part::Tail {
            self.buffer.counters.tail_pixels += worked;
        }
    }
}

/// Coverage 0 to 1 as 16 bits, rounded to nearest; not a number is none.
/// Notes that the pixels of `rect` in tile `key` changed.
fn mark(tiles: &mut BTreeMap<TileKey, PixelRect>, key: TileKey, rect: PixelRect) {
    tiles
        .entry(key)
        .and_modify(|all| {
            all.x0 = all.x0.min(rect.x0);
            all.y0 = all.y0.min(rect.y0);
            all.x1 = all.x1.max(rect.x1);
            all.y1 = all.y1.max(rect.y1);
        })
        .or_insert(rect);
}

fn quantize(c: f32) -> u16 {
    if c.is_nan() {
        return 0;
    }
    (c.clamp(0.0, 1.0) * f32::from(FULL) + 0.5) as u16
}

/// A float pixel coordinate as an `i32`, held to its range.
fn clamp_to_i32(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        v.clamp(i32::MIN as f32, i32::MAX as f32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(buffer: &mut StrokeBuffer, x0: f32, x1: f32) {
        buffer.committed().capsule(
            Disc::new([x0, 10.0], 4.0, 1.0),
            Disc::new([x1, 10.0], 4.0, 1.0),
            RoundTip::default(),
        );
    }

    #[test]
    fn the_tail_is_cleared_at_each_update_and_settles_on_lift() {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        line(&mut buffer, 10.0, 20.0);
        buffer.tail().capsule(
            Disc::new([20.0, 10.0], 4.0, 1.0),
            Disc::new([90.0, 10.0], 4.0, 1.0),
            RoundTip::default(),
        );
        assert_eq!(buffer.coverage(80, 10), FULL);
        assert_eq!(buffer.tail_keys().count(), 2);
        // The next update has a shorter tail: the old one is gone.
        buffer.tail().capsule(
            Disc::new([20.0, 10.0], 4.0, 1.0),
            Disc::new([40.0, 10.0], 4.0, 1.0),
            RoundTip::default(),
        );
        assert_eq!(buffer.coverage(80, 10), 0);
        assert_eq!(buffer.keys().count(), 1, "the emptied tile is dropped");
        assert!(buffer.take_dirty().contains(&TileKey { x: 1, y: 0 }));
        buffer.settle();
        assert_eq!(buffer.tail_keys().count(), 0);
        assert_eq!(buffer.coverage(30, 10), FULL);
        buffer.clear_tail();
        assert_eq!(
            buffer.coverage(30, 10),
            FULL,
            "settled coverage is committed"
        );
    }

    #[test]
    fn a_broken_sample_paints_nothing_and_walks_nothing() {
        // Before the guard, one infinite sample walked a hundred million
        // pixels of rows or columns looking for coverage.
        let mut buffer = StrokeBuffer::new(Combine::Max);
        let good = Disc::new([10.0, 10.0], 3.0, 1.0);
        for broken in [
            [f32::INFINITY, 5.0],
            [5.0, f32::NEG_INFINITY],
            [f32::NAN, 5.0],
        ] {
            let bad = Disc::new(broken, 3.0, 1.0);
            buffer.committed().capsule(good, bad, RoundTip::default());
            buffer
                .committed()
                .dab(&Dab::round(bad, RoundTip::new(0.5, 1.0)));
        }
        assert!(buffer.is_empty());
        assert_eq!(buffer.counters().pixels, 0);
    }

    #[test]
    fn coverage_lands_in_every_tile_it_crosses() {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        buffer.committed().capsule(
            Disc::new([-10.0, -10.0], 6.0, 1.0),
            Disc::new([140.0, 70.0], 6.0, 1.0),
            RoundTip::default(),
        );
        let keys: Vec<TileKey> = buffer.keys().collect();
        assert!(keys.contains(&TileKey { x: -1, y: -1 }));
        assert!(keys.contains(&TileKey { x: 2, y: 1 }));
        assert_eq!(buffer.coverage(65, 31), FULL);
        let bounds = buffer.bounds().expect("bounds");
        assert_eq!((bounds.x0, bounds.y0), (-64, -64));
    }

    #[test]
    fn a_merge_lays_the_stroke_once_on_any_raster_offset() {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        line(&mut buffer, 10.0, 100.0);
        line(&mut buffer, 30.0, 60.0);
        let mut raster = RasterTile2D::new(50, 20, PixelFormat::Rgba8).expect("a raster");
        let changed = buffer
            .merge(
                &mut raster,
                [40, 0],
                Ink::Paint {
                    color: [1.0, 0.0, 0.0],
                    opacity: 0.5,
                },
            )
            .expect("merged");
        assert_eq!(changed.len(), 2);
        let bytes = raster.bytes().expect("bytes");
        // Layer pixel (60, 10) is raster pixel (20, 10).
        let i = (10 * 50 + 20) * 4;
        assert_eq!(bytes[i..i + 4], [255, 0, 0, 128]);
        let mut gray = RasterTile2D::new(4, 4, PixelFormat::Gray8).expect("a mask");
        assert_eq!(
            buffer.merge(&mut gray, [0, 0], Ink::Erase { opacity: 1.0 }),
            Err(MergeError::Format(PixelFormat::Gray8))
        );
    }

    #[test]
    fn a_float_raster_takes_the_same_stroke() {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        line(&mut buffer, 0.0, 30.0);
        let mut raster = RasterTile2D::new(32, 20, PixelFormat::RgbaF32).expect("a raster");
        buffer
            .merge(
                &mut raster,
                [0, 0],
                Ink::Paint {
                    color: [0.25, 0.5, 1.0],
                    opacity: 1.0,
                },
            )
            .expect("merged");
        let samples = raster.f32_samples().expect("floats");
        let i = (10 * 32 + 15) * 4;
        assert_eq!(samples[i..i + 4], [0.25, 0.5, 1.0, 1.0]);
    }
}
