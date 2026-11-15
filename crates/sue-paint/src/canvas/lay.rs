//! A stroke of the brush engine laid straight into its rasters while the
//! pen is down, for the strokes the compositor's live view cannot show:
//! on a layer mask (its own `Gray8` pixels), inside a selection, on a
//! layer whose transparent pixels are locked, and with symmetry's
//! copies.
//!
//! The worker still lays the stroke's coverage on the pool ([`LiveView`]);
//! each frame the tiles it laid are painted here from the pixels they had
//! before the stroke, through the same row kernel the merge on lift uses
//! (`sue_brush::raster::kernel::paint_row`), with the selection's cover
//! and the copies' coverage taken in. So what a person sees is what stays,
//! and the lift records exactly what changed, as one undo step.

use std::collections::HashMap;
use std::sync::Arc;

use sue_brush::buffer::{AREA, TILE, TileKey};
use sue_brush::raster::kernel::{Ink, paint_row};
use sue_brush::worker::LiveView;
use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D};

use crate::select::Selection;

/// One raster a stroke lays into.
#[derive(Debug)]
pub(crate) struct Target {
    /// The raster's entity (a layer, or a layer's mask).
    pub(crate) id: EntityId,
    /// Where the raster sits from the stroke's pixels: raster pixel `p`
    /// is stroke pixel `p + delta`.
    pub(crate) delta: [i64; 2],
    /// Its transparent pixels stay transparent, and its alpha as it is.
    pub(crate) lock: bool,
    /// Each tile's rectangle (raster pixels) and its bytes before the
    /// stroke, by the stroke tile it lies under.
    before: HashMap<TileKey, ([u32; 4], Vec<u8>)>,
}

impl Target {
    /// `id`, at `delta` from the stroke's pixels.
    pub(crate) fn new(id: EntityId, delta: [i64; 2], lock: bool) -> Self {
        Self {
            id,
            delta,
            lock,
            before: HashMap::new(),
        }
    }
}

/// How a stroke is laid straight into its rasters (see the module).
#[derive(Debug)]
pub(crate) struct Lay {
    /// What the stroke lays.
    ink: Ink,
    /// The selection the stroke stays inside, in picture pixels, and
    /// where the stroke's pixel `(0, 0)` is in the picture.
    clip: Option<(Selection, [i64; 2])>,
    /// The rasters, the first the one the stroke is drawn on.
    pub(crate) targets: Vec<Target>,
}

impl Lay {
    /// A stroke laying `ink` into `targets`, inside `clip` when there is
    /// one (picture pixels, and the stroke's place in the picture).
    pub(crate) fn new(ink: Ink, clip: Option<(Selection, [i64; 2])>, targets: Vec<Target>) -> Self {
        Self { ink, clip, targets }
    }

    /// The coverage of stroke tile `key`, the most of every copy's, with
    /// the selection's cover taken in; `None` where nothing covers it.
    fn coverage(&self, key: TileKey, lives: &[Arc<LiveView>]) -> Option<Vec<u16>> {
        let mut out = vec![0_u16; AREA];
        let mut one = vec![0_u16; AREA];
        let mut any = false;
        for live in lives {
            if live.tile_coverage(key, &mut one) {
                any = true;
                for (most, &c) in out.iter_mut().zip(&one) {
                    *most = (*most).max(c);
                }
            }
        }
        if let Some((selection, place)) = &self.clip {
            let [ox, oy] = key.origin();
            for y in 0..TILE {
                for x in 0..TILE {
                    let at = (y * TILE + x) as usize;
                    if out[at] == 0 {
                        continue;
                    }
                    let px = i64::from(ox + x) + place[0];
                    let py = i64::from(oy + y) + place[1];
                    let cover = match (u32::try_from(px), u32::try_from(py)) {
                        (Ok(px), Ok(py)) => selection.at(px, py),
                        _ => 0.0,
                    };
                    out[at] = (f32::from(out[at]) * cover).round() as u16;
                }
            }
        }
        any.then_some(out)
    }

    /// Paints stroke tiles `keys` into the targets' rasters in `arena`
    /// from what they held before the stroke, with the coverage `lives`
    /// show now. Answers the picture rectangles that changed, `[x0, y0,
    /// x1, y1]`, where the stroke's pixel `(0, 0)` is picture pixel
    /// `place`.
    pub(crate) fn lay(
        &mut self,
        arena: &mut Arena,
        keys: &[TileKey],
        lives: &[Arc<LiveView>],
        place: [i64; 2],
    ) -> Vec<[u32; 4]> {
        let covers: Vec<(TileKey, Option<Vec<u16>>)> = keys
            .iter()
            .map(|&key| (key, self.coverage(key, lives)))
            .collect();
        let ink = self.ink;
        let mut rects = Vec::new();
        for target in &mut self.targets {
            let Some(raster) = crate::doc::raster_mut(arena, target.id) else {
                continue;
            };
            for (key, cover) in &covers {
                if let Some(rect) = lay_tile(target, raster, *key, cover.as_deref(), ink) {
                    rects.push(picture_rect(rect, target.delta, place));
                }
            }
        }
        rects
    }

    /// Puts every pixel the stroke changed back as it was (a stroke given
    /// up, or one drawn again from its samples). Answers the picture
    /// rectangles, as [`Self::lay`].
    pub(crate) fn restore(&mut self, arena: &mut Arena, place: [i64; 2]) -> Vec<[u32; 4]> {
        let mut rects = Vec::new();
        for target in &mut self.targets {
            let Some(raster) = crate::doc::raster_mut(arena, target.id) else {
                continue;
            };
            let keys: Vec<TileKey> = target.before.keys().copied().collect();
            for key in keys {
                if let Some(rect) = lay_tile(target, raster, key, None, self.ink) {
                    rects.push(picture_rect(rect, target.delta, place));
                }
            }
        }
        rects
    }

    /// What the stroke changed in each target, already applied: the bytes
    /// of every tile it painted before and after, under `label`. Empty
    /// edits are left out.
    pub(crate) fn edits(&self, arena: &Arena, label: &str) -> Vec<RasterEdit> {
        let mut edits = Vec::new();
        for target in &self.targets {
            let Some(raster) = crate::doc::raster(arena, target.id) else {
                continue;
            };
            let mut keys: Vec<&TileKey> = target.before.keys().collect();
            keys.sort_unstable();
            let patches: Vec<RasterPatch> = keys
                .into_iter()
                .filter_map(|key| {
                    let ([x, y, width, height], before) = target.before.get(key)?;
                    let after = RasterPatch::read(raster, *x, *y, *width, *height)?;
                    (after != *before).then(|| RasterPatch {
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                        before: before.clone(),
                        after,
                    })
                })
                .collect();
            if !patches.is_empty() {
                edits.push(RasterEdit::new(target.id, label, patches));
            }
        }
        edits
    }
}

/// Raster rectangle `[x, y, width, height]` of a target at `delta` from a
/// stroke at `place`, as picture pixels `[x0, y0, x1, y1]` (cut to the
/// picture's positive side).
fn picture_rect([x, y, width, height]: [u32; 4], delta: [i64; 2], place: [i64; 2]) -> [u32; 4] {
    let clamp = |v: i64| v.clamp(0, i64::from(u32::MAX)) as u32;
    let x0 = i64::from(x) + delta[0] + place[0];
    let y0 = i64::from(y) + delta[1] + place[1];
    [
        clamp(x0),
        clamp(y0),
        clamp(x0 + i64::from(width)),
        clamp(y0 + i64::from(height)),
    ]
}

/// The rectangle of stroke tile `key` in a raster `size` at `delta` from
/// the stroke, cut to the raster: `[x, y, width, height]`.
fn tile_rect(key: TileKey, delta: [i64; 2], size: [u32; 2]) -> Option<[u32; 4]> {
    let [ox, oy] = key.origin();
    let x0 = (i64::from(ox) - delta[0]).max(0);
    let y0 = (i64::from(oy) - delta[1]).max(0);
    let x1 = (i64::from(ox) + i64::from(TILE) - delta[0]).min(i64::from(size[0]));
    let y1 = (i64::from(oy) + i64::from(TILE) - delta[1]).min(i64::from(size[1]));
    (x0 < x1 && y0 < y1).then(|| [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

/// Bytes a pixel takes in `raster`.
fn pixel_bytes(raster: &RasterTile2D) -> Option<usize> {
    match raster.format() {
        PixelFormat::Rgba8 => Some(4),
        PixelFormat::Gray8 => Some(1),
        PixelFormat::RgbaF32 => None,
    }
}

/// Paints one stroke tile into one raster (see [`Lay::lay`]): its bytes
/// before are kept the first time, then the tile is painted from them.
/// Answers the raster rectangle `[x, y, width, height]` it painted.
fn lay_tile(
    target: &mut Target,
    raster: &mut RasterTile2D,
    key: TileKey,
    coverage: Option<&[u16]>,
    ink: Ink,
) -> Option<[u32; 4]> {
    let size = [raster.width(), raster.height()];
    let rect = tile_rect(key, target.delta, size)?;
    let per = pixel_bytes(raster)?;
    let before = match target.before.entry(key) {
        std::collections::hash_map::Entry::Occupied(kept) => &kept.into_mut().1,
        std::collections::hash_map::Entry::Vacant(free) => {
            // Never covered, never changed: nothing to keep or paint.
            coverage?;
            let bytes = RasterPatch::read(raster, rect[0], rect[1], rect[2], rect[3])?;
            &free.insert((rect, bytes)).1
        }
    };
    let [x, y, width, height] = rect;
    let stride = size[0] as usize * per;
    let row_bytes = width as usize * per;
    let [ox, oy] = key.origin();
    // Where the rectangle starts inside the stroke tile.
    let tx = (i64::from(x) + target.delta[0] - i64::from(ox)) as usize;
    let ty = (i64::from(y) + target.delta[1] - i64::from(oy)) as usize;
    let lock = target.lock;
    let bytes = raster.bytes_mut()?;
    let mut row = vec![0_u8; width as usize * 4];
    for r in 0..height as usize {
        let old = &before[r * row_bytes..(r + 1) * row_bytes];
        let start = (y as usize + r) * stride + x as usize * per;
        let out = &mut bytes[start..start + row_bytes];
        let Some(coverage) = coverage else {
            // Nothing covers it any more: as it was.
            out.copy_from_slice(old);
            continue;
        };
        let cover = &coverage
            [(ty + r) * TILE as usize + tx..(ty + r) * TILE as usize + tx + width as usize];
        if per == 4 {
            row.copy_from_slice(old);
        } else {
            for (pixel, &value) in row.as_chunks_mut::<4>().0.iter_mut().zip(old) {
                *pixel = [255, 255, 255, value];
            }
        }
        paint_row(&mut row, cover, ink);
        if per == 4 {
            if lock {
                for (pixel, was) in row
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(old.as_chunks::<4>().0)
                {
                    if was[3] == 0 {
                        *pixel = *was;
                    } else {
                        pixel[3] = was[3];
                    }
                }
            }
            out.copy_from_slice(&row);
        } else {
            for (value, pixel) in out.iter_mut().zip(row.as_chunks::<4>().0) {
                *value = pixel[3];
            }
        }
    }
    Some(rect)
}
