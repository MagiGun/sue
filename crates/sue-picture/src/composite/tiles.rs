//! The screen image by 64 pixel tiles: which tiles a change of the
//! layers touches (each tile's [`tile_hashes`] before and after), a tile
//! composited from only the layers that show in it, tiles composited on
//! every core, and the composited tiles kept ([`TileCache`]) so a state
//! seen before (undo, a layer shown again, an animation's next round)
//! costs a copy.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use sue_core::{Arena, RasterTile2D};

use super::gather::{Layer, Occupancy, Source, gather_view, prepare};
use super::rows::{fold_picked, fold_plain_picked, pick_rows, picks, picks_plain, shown};
use super::{Compositor, RenderOptions, next_version, workers};

/// Side of a tile of the screen image (an [`Occupancy`] tile).
pub(super) const TILE: u32 = 64;

/// The memory the kept tiles may take unless told otherwise (see
/// [`Compositor::set_tile_memory`]).
const TILE_MEMORY: usize = 96 << 20;

/// How many waiting tiles are composited at once whatever time a frame
/// is given (about a megapixel).
pub(super) const SMALL_CHANGE: usize = 256;

/// `a` then `b` as one number, never 0 (0 stands for nothing shown).
fn mix(a: u64, b: u64) -> u64 {
    let mut x = (a ^ 0x9E37_79B9_7F4A_7C15).wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ b;
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    x.max(1)
}

/// A number that changes whenever what `layer` itself puts on the screen
/// may: its pixels, place, style, mask and effects; of a folder, its own
/// settings but not its members. Not its name, lock, palette row or
/// anything else the picture does not show.
fn own_key(arena: &Arena, layer: &Layer<'_>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let stamp = |raster: &RasterTile2D, hasher: &mut std::collections::hash_map::DefaultHasher| {
        (raster.width(), raster.height()).hash(hasher);
        raster.tile_revisions().hash(hasher);
    };
    layer.id.map(|id| id.to_u128()).hash(&mut hasher);
    format!(
        "{:?}{:?}{:?}{:?}{:?}",
        layer.style, layer.tint, layer.warp, layer.effects, layer.mask_offset
    )
    .hash(&mut hasher);
    layer.live.is_some().hash(&mut hasher);
    if let Some(mask) = layer.mask {
        stamp(mask, &mut hasher);
    }
    for (mask, offset) in &layer.outer {
        stamp(mask, &mut hasher);
        offset.hash(&mut hasher);
    }
    let revision = |hasher: &mut std::collections::hash_map::DefaultHasher| {
        layer
            .id
            .and_then(|id| arena.get_by_id(id).ok())
            .map(sue_core::Entity::revision)
            .hash(hasher);
    };
    match &layer.source {
        Source::Raster { raster, offset } => {
            stamp(raster, &mut hasher);
            offset.hash(&mut hasher);
        }
        Source::Paper(rgb) => rgb.map(f32::to_bits).hash(&mut hasher),
        Source::Material { image, .. } => {
            stamp(image, &mut hasher);
            revision(&mut hasher);
        }
        // A source whose content is not cheap to tell apart counts as
        // changed whenever its entity changed at all; so do a folder's
        // own settings (how it is composited).
        Source::Group(_) | Source::Correction(_) | Source::Solid(_) | Source::Gradient(_) => {
            revision(&mut hasher);
        }
    }
    hasher.finish().max(1)
}

/// Where a layer can show anything, and a number for what it shows there.
enum Shows {
    Nothing,
    /// Anywhere: paper, a fill, a layer with a border or seen through a
    /// camera, one whose tiles are not known.
    Everywhere(u64),
    /// In the tiles it has pixels in.
    Tiles(u64, Arc<Occupancy>),
    /// A folder composited on its own: a number a tile (0 where no member
    /// shows).
    Each(Vec<u64>),
}

impl Shows {
    fn of(arena: &Arena, layer: &Layer<'_>, count: usize) -> Self {
        if !layer.style.visible {
            return Self::Nothing;
        }
        let key = own_key(arena, layer);
        match &layer.source {
            Source::Group(members) => Self::Each(
                stack_hashes(arena, members, count)
                    .into_iter()
                    .map(|inner| if inner == 0 { 0 } else { mix(key, inner) })
                    .collect(),
            ),
            // As `Layer::empty_at` tells an empty tile.
            Source::Raster { .. }
                if layer.live.is_none() && layer.warp.is_none() && layer.effects.is_plain() =>
            {
                match &layer.occupancy {
                    Some(tiles) => Self::Tiles(key, tiles.clone()),
                    None => Self::Everywhere(key),
                }
            }
            _ => Self::Everywhere(key),
        }
    }

    /// The number for tile `index`, 0 where it shows nothing.
    fn at(&self, index: usize) -> u64 {
        match self {
            Self::Nothing => 0,
            Self::Everywhere(key) => *key,
            Self::Tiles(key, tiles) => {
                if tiles.has_tile(index) {
                    *key
                } else {
                    0
                }
            }
            Self::Each(keys) => keys.get(index).copied().unwrap_or(0),
        }
    }

    /// Runs `add(tile, number)` for every tile it shows anything in.
    fn each(&self, count: usize, mut add: impl FnMut(usize, u64)) {
        match self {
            Self::Nothing => {}
            Self::Everywhere(key) => (0..count).for_each(|index| add(index, *key)),
            Self::Tiles(key, tiles) => tiles.each_tile(|index| {
                if index < count {
                    add(index, *key);
                }
            }),
            Self::Each(keys) => {
                for (index, key) in keys.iter().enumerate().take(count) {
                    if *key != 0 {
                        add(index, *key);
                    }
                }
            }
        }
    }
}

/// For each of the `count` tiles, a number for what the stack `layers`
/// composites to in it: the layers that show anything there, bottom
/// first, each with what it shows (and, for a clipped layer, what it is
/// clipped to). Two stacks with the same number for a tile composite to
/// the same pixels in it; 0 is a tile nothing shows in.
fn stack_hashes(arena: &Arena, layers: &[Layer<'_>], count: usize) -> Vec<u64> {
    let mut out = vec![0_u64; count];
    // What the next clipped layers clip to; none at the very bottom,
    // where they show as they are.
    let mut base: Option<Shows> = None;
    for layer in layers {
        if matches!(layer.source, Source::Correction(_)) {
            if layer.style.visible {
                let key = own_key(arena, layer);
                for slot in &mut out {
                    *slot = mix(*slot, key);
                }
            }
            continue;
        }
        let shows = Shows::of(arena, layer, count);
        if layer.style.clip {
            match &base {
                Some(base) => shows.each(count, |index, key| {
                    let under = base.at(index);
                    if under != 0 {
                        out[index] = mix(out[index], mix(key, under));
                    }
                }),
                None => shows.each(count, |index, key| {
                    out[index] = mix(out[index], mix(key, 1))
                }),
            }
        } else {
            shows.each(count, |index, key| out[index] = mix(out[index], key));
            base = Some(shows);
        }
    }
    out
}

/// [`stack_hashes`] for the whole picture of `size`, a number for each
/// 64 pixel tile in rows from the top left.
pub(super) fn tile_hashes(arena: &Arena, layers: &[Layer<'_>], size: [u32; 2]) -> Vec<u64> {
    let count = (size[0].div_ceil(TILE) * size[1].div_ceil(TILE)) as usize;
    stack_hashes(arena, layers, count)
}

/// A kept tile: when it was last used, and its pixels.
type Kept = (u64, Arc<[u8]>);

/// The composited tiles kept, by tile and the number of what it shows
/// (see [`tile_hashes`]): the least lately used go when they take more
/// than the memory allowed.
#[derive(Debug)]
pub(super) struct TileCache {
    tiles: HashMap<(u32, u64), Kept>,
    clock: u64,
    bytes: usize,
    limit: usize,
}

impl Default for TileCache {
    fn default() -> Self {
        Self {
            tiles: HashMap::new(),
            clock: 0,
            bytes: 0,
            limit: TILE_MEMORY,
        }
    }
}

impl TileCache {
    fn get(&mut self, key: (u32, u64)) -> Option<Arc<[u8]>> {
        self.clock += 1;
        let (used, pixels) = self.tiles.get_mut(&key)?;
        *used = self.clock;
        Some(pixels.clone())
    }

    fn put(&mut self, key: (u32, u64), pixels: Arc<[u8]>) {
        if pixels.len() > self.limit / 4 {
            return;
        }
        self.clock += 1;
        self.bytes += pixels.len();
        if let Some((_, old)) = self.tiles.insert(key, (self.clock, pixels)) {
            self.bytes -= old.len();
        }
        if self.bytes > self.limit {
            // The older half goes at once, so this is seldom done.
            let mut used: Vec<u64> = self.tiles.values().map(|(used, _)| *used).collect();
            used.sort_unstable();
            let cut = used[used.len() / 2];
            self.tiles.retain(|_, (used, _)| *used > cut);
            self.bytes = self.tiles.values().map(|(_, pixels)| pixels.len()).sum();
        }
    }

    pub(super) fn clear(&mut self) {
        self.tiles.clear();
        self.bytes = 0;
    }

    pub(super) fn set_limit(&mut self, bytes: usize) {
        self.limit = bytes;
        if self.bytes > bytes {
            self.clear();
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.tiles.len()
    }
}

/// The rectangle `rect` (within one tile) of the screen image composited
/// from `layers`: opaque RGBA8 rows over the checkerboard. Only the
/// layers that show in the tile are gone through (see [`picks`]), read
/// along their rows when they are plain; the result is what compositing
/// every layer at every pixel gives.
fn compose(layers: &[Layer<'_>], [x0, y0, x1, y1]: [u32; 4]) -> Vec<u8> {
    let kept = picks(layers, x0, y0);
    let plain = picks_plain(layers, &kept);
    let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize * 4);
    for y in y0..y1 {
        if plain {
            let rows = pick_rows(layers, &kept, y);
            for x in x0..x1 {
                let color = fold_plain_picked(layers, &kept, &rows, x);
                out.extend_from_slice(&shown(color, x, y));
            }
        } else {
            for x in x0..x1 {
                let color = fold_picked(layers, &kept, x, y);
                out.extend_from_slice(&shown(color, x, y));
            }
        }
    }
    out
}

/// [`compose`] at a glance, for a picture seen small: one sample each
/// `step` pixels a side, every block filled with its sample.
fn compose_coarse(layers: &[Layer<'_>], [x0, y0, x1, y1]: [u32; 4], step: u32) -> Vec<u8> {
    let kept = picks(layers, x0, y0);
    let width = (x1 - x0) as usize;
    let mut out = vec![0_u8; width * (y1 - y0) as usize * 4];
    for gy in (y0 / step * step..y1).step_by(step as usize) {
        let y = (gy + step / 2).clamp(y0, y1 - 1);
        for gx in (x0 / step * step..x1).step_by(step as usize) {
            let x = (gx + step / 2).clamp(x0, x1 - 1);
            let color = shown(fold_picked(layers, &kept, x, y), x, y);
            for by in gy.max(y0)..(gy + step).min(y1) {
                let row = (by - y0) as usize * width;
                for bx in gx.max(x0)..(gx + step).min(x1) {
                    let at = (row + (bx - x0) as usize) * 4;
                    out[at..at + 4].copy_from_slice(&color);
                }
            }
        }
    }
    out
}

/// Why waiting tiles are composited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// The layers just changed.
    Change,
    /// The rest of a change shared out over frames.
    Rest,
    /// What a stroke showed a step off is brought exact.
    Exact,
}

/// A frame of the interface the screen image is composited for (see
/// [`Compositor::sync_within`]).
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    /// The rectangle of the picture on screen.
    visible: [u32; 4],
    /// How many picture pixels a side one screen pixel covers (see
    /// `view_step`).
    step: u32,
    /// How long compositing may take.
    budget: Duration,
}

/// `work(index)` for every index below `count`, on as many threads as
/// big jobs take (see [`workers`]), each taking the next index not yet
/// begun so uneven work evens out; the results in the order of the
/// indices.
pub(super) fn each_index<T: Send>(count: usize, work: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = if count < 8 {
        1
    } else {
        workers().min(count / 4)
    };
    if threads <= 1 {
        return (0..count).map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, T)> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..threads)
            .map(|_| {
                let (next, work) = (&next, &work);
                scope.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= count {
                            break out;
                        }
                        out.push((index, work(index)));
                    }
                })
            })
            .collect();
        jobs.into_iter()
            .flat_map(|job| job.join().unwrap_or_default())
            .collect()
    });
    // A thread that could not be joined leaves its indices undone.
    done.sort_unstable_by_key(|(index, _)| *index);
    let mut out = Vec::with_capacity(count);
    for (index, value) in done {
        while out.len() < index {
            out.push(work(out.len()));
        }
        out.push(value);
    }
    while out.len() < count {
        out.push(work(out.len()));
    }
    out
}

/// The tiles `tiles` (in a picture `columns` tiles wide, of `size`) as
/// few rectangles: each run of tiles in a row, runs one under the other
/// joined.
fn tile_rects(tiles: &[u32], columns: u32, size: [u32; 2]) -> Vec<[u32; 4]> {
    let mut sorted = tiles.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    // Runs as (row, first column, column after the last).
    let mut runs: Vec<[u32; 3]> = Vec::new();
    for tile in sorted {
        let (tx, ty) = (tile % columns, tile / columns);
        match runs.last_mut() {
            Some(run) if run[0] == ty && run[2] == tx => run[2] = tx + 1,
            _ => runs.push([ty, tx, tx + 1]),
        }
    }
    // Rectangles in tiles, the open ones by their columns.
    let mut open: HashMap<(u32, u32), usize> = HashMap::new();
    let mut rects: Vec<[u32; 4]> = Vec::new();
    for [ty, from, to] in runs {
        match open.get(&(from, to)) {
            Some(index) if rects[*index][3] == ty => rects[*index][3] = ty + 1,
            _ => {
                open.insert((from, to), rects.len());
                rects.push([from, ty, to, ty + 1]);
            }
        }
    }
    rects
        .into_iter()
        .map(|[x0, y0, x1, y1]| {
            [
                x0 * TILE,
                y0 * TILE,
                (x1 * TILE).min(size[0]),
                (y1 * TILE).min(size[1]),
            ]
        })
        .collect()
}

impl Compositor {
    /// How many tiles wide the screen image is.
    fn columns(&self) -> u32 {
        self.size[0].div_ceil(TILE)
    }

    /// Tile `tile`'s rectangle of the picture.
    fn tile_rect(&self, tile: u32) -> [u32; 4] {
        let columns = self.columns().max(1);
        let (tx, ty) = (tile % columns, tile / columns);
        [
            tx * TILE,
            ty * TILE,
            ((tx + 1) * TILE).min(self.size[0]),
            ((ty + 1) * TILE).min(self.size[1]),
        ]
    }

    /// Lets the composited tiles kept take up to `bytes` of memory (96 MB
    /// unless told; 0 keeps none).
    pub fn set_tile_memory(&mut self, bytes: usize) {
        self.tiles.set_limit(bytes);
    }

    /// Composites `pieces` (each within one tile) from `layers` on every
    /// core and writes them to the screen image. With `hashes` (each
    /// tile's number for the layers as they are, see [`tile_hashes`]) a
    /// whole tile kept from before is copied instead, and the ones made
    /// are kept. Answers the pieces whose pixels came out different.
    pub(super) fn paint_pieces(
        &mut self,
        layers: &[Layer<'_>],
        pieces: &[[u32; 4]],
        hashes: Option<&[u64]>,
    ) -> Vec<[u32; 4]> {
        let columns = self.columns().max(1);
        let stride = self.size[0] as usize * 4;
        // A whole tile, by the number of what it shows.
        let key = |piece: [u32; 4], this: &Self| {
            let tile = piece[1] / TILE * columns + piece[0] / TILE;
            let hash = hashes?.get(tile as usize)?;
            (this.tile_rect(tile) == piece).then_some((tile, *hash))
        };
        // Writes a piece; `true` if a pixel of it is new.
        let blit = |display: &mut [u8], [x0, y0, x1, y1]: [u32; 4], pixels: &[u8]| {
            let width = (x1 - x0) as usize * 4;
            let mut new = false;
            for (row, line) in (y0..y1).zip(pixels.chunks_exact(width)) {
                let at = row as usize * stride + x0 as usize * 4;
                let shown = &mut display[at..at + width];
                if shown != line {
                    shown.copy_from_slice(line);
                    new = true;
                }
            }
            new
        };
        let mut changed = Vec::new();
        let mut todo = Vec::new();
        for piece in pieces {
            match key(*piece, self).and_then(|key| self.tiles.get(key)) {
                Some(pixels) => {
                    if blit(&mut self.display, *piece, &pixels) {
                        changed.push(*piece);
                    }
                }
                None => todo.push(*piece),
            }
        }
        // In batches, so the tiles made and not yet written stay few.
        for batch in todo.chunks(1024) {
            let made = each_index(batch.len(), |index| compose(layers, batch[index]));
            for (piece, pixels) in batch.iter().zip(made) {
                if blit(&mut self.display, *piece, &pixels) {
                    changed.push(*piece);
                }
                if let Some(key) = key(*piece, self) {
                    self.tiles.put(key, pixels.into());
                }
            }
        }
        changed
    }

    /// Composites the rectangle `rect` of the picture again, as the
    /// arena has it now.
    pub(super) fn paint(&mut self, arena: &Arena, [x0, y0, x1, y1]: [u32; 4]) {
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, [x0, y0, x1, y1]);
        // A whole picture finds what each layer covers; a piece reuses it.
        let whole = [x0, y0, x1, y1] == [0, 0, self.size[0], self.size[1]];
        self.attach(arena, &mut layers, whole);
        let mut pieces = Vec::new();
        for ty in y0 / TILE..=(y1 - 1) / TILE {
            for tx in x0 / TILE..=(x1 - 1) / TILE {
                pieces.push([
                    (tx * TILE).max(x0),
                    (ty * TILE).max(y0),
                    ((tx + 1) * TILE).min(x1),
                    ((ty + 1) * TILE).min(y1),
                ]);
            }
        }
        self.paint_pieces(&layers, &pieces, None);
        if whole {
            self.waiting.clear();
            self.rough.clear();
        }
        self.record(vec![[x0, y0, x1, y1]]);
    }

    /// Takes the tiles `rects` touch as painted a step off at most (see
    /// `rough`).
    pub(super) fn roughly(&mut self, rects: &[[u32; 4]]) {
        let columns = self.columns().max(1);
        for [x0, y0, x1, y1] in rects {
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            for ty in y0 / TILE..=(y1 - 1) / TILE {
                for tx in x0 / TILE..=(x1 - 1) / TILE {
                    self.rough.insert(ty * columns + tx);
                }
            }
        }
    }

    /// Takes `rects` as repainted: a new version, with the rectangles on
    /// record for [`Self::changed_since`].
    pub(super) fn record(&mut self, rects: Vec<[u32; 4]>) {
        self.record_pixels(rects);
        self.content = self.version;
    }

    /// The version that changes with what the picture shows: as
    /// [`Self::display`]'s, but not for pixels only brought exact after a
    /// stroke (a step off at most before). What keeps one image a change
    /// (a timelapse) goes by this one.
    #[must_use]
    pub fn content_version(&self) -> u64 {
        self.content
    }

    /// [`Self::record`] for pixels that show the same picture as before.
    fn record_pixels(&mut self, rects: Vec<[u32; 4]>) {
        self.version = next_version();
        self.changes.push_back((self.version, rects));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
    }

    /// Follows the arena: finds the tiles that show something else than
    /// the screen image has (all of them when that is not known) and
    /// composites them, within `limit` (what is on screen, and how long
    /// to take) when there is one. Returns `true` if the image changed.
    pub(super) fn follow(&mut self, arena: &Arena, limit: Option<Frame>) -> bool {
        let Some(size) = arena
            .get_by_id(self.picture)
            .ok()
            .and_then(sue_model::picture::size)
        else {
            return false;
        };
        let seen = self.signature(arena);
        if size == self.size && seen == self.seen && !self.view_changed {
            if !self.waiting.is_empty() {
                let mut layers = gather_view(
                    arena,
                    self.picture,
                    RenderOptions::default(),
                    self.view.as_ref(),
                );
                self.attach(arena, &mut layers, false);
                return self.settle(layers, limit, Why::Rest);
            }
            // The image matches the arena. Once a stroke is over (it
            // repaints by itself and leaves the looks behind), what each
            // tile shows is found again, so the next change of a layer
            // redraws only its part.
            let stale = self
                .looks
                .as_ref()
                .is_none_or(|looks| looks.signature != self.seen);
            if stale && self.live.is_none() && self.pending.is_empty() && size != [0, 0] {
                self.calm = self.calm.saturating_add(1);
                if self.calm >= 2 {
                    let mut layers = gather_view(
                        arena,
                        self.picture,
                        RenderOptions::default(),
                        self.view.as_ref(),
                    );
                    self.attach(arena, &mut layers, true);
                    self.looks = Some(super::Looks {
                        hashes: tile_hashes(arena, &layers, size),
                        signature: seen,
                    });
                    // What the stroke's refreshes showed a step off comes
                    // exact.
                    if !self.rough.is_empty() {
                        self.waiting.append(&mut self.rough);
                        return self.settle(layers, limit, Why::Exact);
                    }
                }
            }
            return false;
        }
        self.calm = 0;
        let resized = size != self.size;
        // Asked for in a way that can fail: a canvas there is no memory
        // to show is told to the person (`take_starved`), where `vec!`
        // would end the program.
        let len = size[0] as usize * size[1] as usize * 4;
        if self.display.len() != len {
            let mut display: Vec<u8> = Vec::new();
            if display.try_reserve_exact(len).is_err() {
                self.size = [0, 0];
                self.display = Vec::new();
                self.seen = seen;
                self.starved = true;
                self.looks = None;
                self.waiting.clear();
                return false;
            }
            drop(display);
            self.display = vec![0; len];
        }
        if resized {
            // Tiles are counted from the picture's corner: none of what
            // was known of them holds.
            self.occupancy.clear();
            self.tiles.clear();
        }
        self.size = size;
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        self.attach(arena, &mut layers, true);
        let hashes = tile_hashes(arena, &layers, size);
        // A change of the layers (not of the canvas's size): only the
        // tiles that show something else are composited again, and the
        // ones a stroke left coarse or a step off. A name, a lock or a
        // folder closed in the palette shows nothing new and costs
        // nothing.
        let before = self.looks.take().filter(|looks| {
            !resized && looks.signature == self.seen && looks.hashes.len() == hashes.len()
        });
        match before {
            Some(before) => {
                for (tile, (old, new)) in before.hashes.iter().zip(&hashes).enumerate() {
                    if old != new {
                        self.waiting.insert(tile as u32);
                    }
                }
                let columns = self.columns().max(1);
                let coarse = std::mem::take(&mut self.pending);
                self.waiting
                    .extend(coarse.into_iter().map(|(tx, ty)| ty * columns + tx));
                self.pending_layer = None;
                self.waiting.append(&mut self.rough);
            }
            None => {
                self.waiting = (0..hashes.len() as u32).collect();
                self.rough.clear();
                self.pending.clear();
                self.pending_layer = None;
            }
        }
        self.looks = Some(super::Looks {
            hashes,
            signature: seen.clone(),
        });
        self.seen = seen;
        self.view_changed = false;
        if self.waiting.is_empty() {
            return false;
        }
        self.settle(layers, limit, Why::Change)
    }

    /// Composites the tiles that wait, all of them or, with a `limit`,
    /// the ones on screen first for about as long as it allows; the rest
    /// keeps waiting (see [`Self::settling`]). A picture seen small that
    /// just `changed` shows at a glance first: every tile on screen one
    /// sample a screen pixel, the detail on the frames after. Returns
    /// `true` if the image changed.
    fn settle(&mut self, mut layers: Vec<Layer<'_>>, limit: Option<Frame>, why: Why) -> bool {
        let columns = self.columns().max(1);
        // A few tiles take less than sharing them out over frames.
        let limit = limit.filter(|_| self.waiting.len() > self.small_change);
        let mut order: Vec<u32> = self.waiting.iter().copied().collect();
        let mut on_screen = order.len();
        if let Some(Frame {
            visible: [x0, y0, x1, y1],
            ..
        }) = limit
        {
            let shows = |tile: &u32| {
                let (tx, ty) = (tile % columns, tile / columns);
                tx * TILE < x1 && (tx + 1) * TILE > x0 && ty * TILE < y1 && (ty + 1) * TILE > y0
            };
            order.sort_by_key(|tile| !shows(tile));
            on_screen = order.iter().filter(|tile| shows(tile)).count();
        }
        let Some(bounds) = order
            .iter()
            .map(|tile| self.tile_rect(*tile))
            .reduce(|[a, b, c, d], [x0, y0, x1, y1]| [a.min(x0), b.min(y0), c.max(x1), d.max(y1)])
        else {
            return false;
        };
        prepare(&mut layers, bounds);
        if let Some(Frame { step, .. }) = limit
            && step > 1
            && why == Why::Change
        {
            let seen = &order[..on_screen];
            if seen.is_empty() {
                return false;
            }
            let pieces: Vec<[u32; 4]> = seen.iter().map(|tile| self.tile_rect(*tile)).collect();
            let made = each_index(pieces.len(), |index| {
                compose_coarse(&layers, pieces[index], step)
            });
            let stride = self.size[0] as usize * 4;
            for ([x0, y0, x1, y1], pixels) in pieces.iter().zip(made) {
                let width = (x1 - x0) as usize * 4;
                for (row, line) in (*y0..*y1).zip(pixels.chunks_exact(width)) {
                    let at = row as usize * stride + *x0 as usize * 4;
                    self.display[at..at + width].copy_from_slice(line);
                }
            }
            let rects = tile_rects(seen, columns, self.size);
            self.record(rects);
            return true;
        }
        // The kept tiles are for what the arena holds: not for a stroke
        // still being painted over it.
        let looks = self.looks.take();
        let hashes = looks
            .as_ref()
            .filter(|looks| looks.signature == self.seen && self.live.is_none())
            .map(|looks| looks.hashes.as_slice());
        let started = Instant::now();
        let mut at = 0;
        // The tiles that came out different from what was shown.
        let mut new: Vec<[u32; 4]> = Vec::new();
        let mut batch = match limit {
            None => order.len(),
            Some(Frame { budget, .. }) => {
                ((self.rate * budget.as_secs_f64() * 0.5) as usize).clamp(workers() * 2, 4096)
            }
        };
        while at < order.len() {
            let chunk = &order[at..(at + batch.max(1)).min(order.len())];
            let pieces: Vec<[u32; 4]> = chunk.iter().map(|tile| self.tile_rect(*tile)).collect();
            let begun = Instant::now();
            new.extend(self.paint_pieces(&layers, &pieces, hashes));
            at += chunk.len();
            let Some(Frame { budget, .. }) = limit else {
                continue;
            };
            let took = begun.elapsed().as_secs_f64().max(1e-6);
            self.rate = chunk.len() as f64 / took;
            let left = budget.saturating_sub(started.elapsed()).as_secs_f64();
            // What surely fits in the time left; nothing when even a
            // tile a thread may not.
            batch = (self.rate * left * 0.8) as usize;
            if batch < workers() {
                break;
            }
        }
        self.looks = looks;
        for tile in &order[..at] {
            self.waiting.remove(tile);
        }
        if why != Why::Exact {
            let rects = tile_rects(&order[..at], columns, self.size);
            self.record(rects);
            return true;
        }
        // Brought exact: only the tiles that came out different are
        // sent on, and the picture counts as the same.
        if new.is_empty() {
            return false;
        }
        let tiles: Vec<u32> = new
            .iter()
            .map(|piece| piece[1] / TILE * columns + piece[0] / TILE)
            .collect();
        let rects = tile_rects(&tiles, columns, self.size);
        self.record_pixels(rects);
        true
    }

    /// [`Self::sync`] for a frame of the interface: the tiles a change
    /// touches are composited for about `budget`, the ones in `visible`
    /// (`[x0, y0, x1, y1]` of the picture) first, and the rest on the
    /// next calls, so a slider dragged over a big picture never holds a
    /// frame up. Seen small (`step` picture pixels a side to a screen
    /// pixel, see `view_step`), a change shows at once all over what is
    /// on screen, one sample a screen pixel, and in full detail on the
    /// calls after. The newest state of the layers always wins: a tile
    /// is composited from the arena as it is when its turn comes. While
    /// [`Self::settling`] the caller draws another frame; once it is
    /// not, the image is exactly what [`Self::sync`] gives.
    pub fn sync_within(
        &mut self,
        arena: &Arena,
        visible: [u32; 4],
        step: u32,
        budget: Duration,
    ) -> bool {
        self.follow(
            arena,
            Some(Frame {
                visible,
                step: step.clamp(1, 8),
                budget,
            }),
        )
    }

    /// `true` while tiles a change touched still wait to be composited
    /// (see [`Self::sync_within`]).
    #[must_use]
    pub fn settling(&self) -> bool {
        !self.waiting.is_empty()
    }
}
