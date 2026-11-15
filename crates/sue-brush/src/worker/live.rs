//! What the compositor shows of a stroke that is not in its layer yet.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{PoisonError, RwLock};

use crate::buffer::{AREA, TILE, TileKey};
use crate::raster::PixelRect;
use crate::raster::kernel::{Ink, paint_row};

use super::bands::Bands;

/// A hasher for tile keys: two small integers mixed by one multiply, as
/// a lookup happens for every pixel the compositor shows.
#[derive(Clone, Copy, Debug, Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 =
                (self.0.rotate_left(5) ^ u64::from(byte)).wrapping_mul(0x51_7C_C1_B7_27_22_0A_95);
        }
    }

    fn write_i32(&mut self, value: i32) {
        self.0 =
            (self.0.rotate_left(29) ^ u64::from(value as u32)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type Tiles = HashMap<TileKey, Box<[u16]>, BuildHasherDefault<KeyHasher>>;

#[derive(Debug, Default)]
struct Shown {
    tiles: Tiles,
    bounds: Option<PixelRect>,
    ended: bool,
}

/// A stroke's coverage as the compositor reads it while the pen is down:
/// the tiles the worker has laid, combined, and the ink they lay. The
/// worker updates the tiles that changed after each update; the
/// compositor reads any pixel at any time. Once the stroke is merged into
/// its layer (or given up) it is [ended](Self::end) and shows nothing.
#[derive(Debug)]
pub struct LiveView {
    ink: Ink,
    shown: RwLock<Shown>,
}

impl LiveView {
    /// A view of a stroke that lays `ink`, showing nothing yet.
    #[must_use]
    pub fn new(ink: Ink) -> Self {
        Self {
            ink,
            shown: RwLock::new(Shown::default()),
        }
    }

    /// The ink the stroke lays.
    #[must_use]
    pub const fn ink(&self) -> Ink {
        self.ink
    }

    /// Takes the coverage of tiles `keys` from `bands`.
    pub(crate) fn show(&self, bands: &Bands, keys: &[TileKey]) {
        let mut fresh = Vec::with_capacity(keys.len());
        for &key in keys {
            let mut coverage = vec![0_u16; AREA];
            let any = bands.tile_coverage(key, &mut coverage) && coverage.iter().any(|&c| c > 0);
            fresh.push((key, any.then(|| coverage.into_boxed_slice())));
        }
        let bounds = bands.bounds();
        let mut shown = self.shown.write().unwrap_or_else(PoisonError::into_inner);
        if shown.ended {
            return;
        }
        for (key, coverage) in fresh {
            match coverage {
                Some(coverage) => {
                    shown.tiles.insert(key, coverage);
                }
                None => {
                    shown.tiles.remove(&key);
                }
            }
        }
        shown.bounds = bounds;
    }

    /// Stops showing the stroke: from now on it covers nothing.
    pub fn end(&self) {
        let mut shown = self.shown.write().unwrap_or_else(PoisonError::into_inner);
        shown.ended = true;
        shown.tiles.clear();
        shown.bounds = None;
    }

    /// `true` once [`Self::end`] was called.
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.shown
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .ended
    }

    /// The layer pixels the stroke may cover, or `None` while it covers
    /// nothing (and once it ended).
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        self.shown
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .bounds
    }

    /// The coverage shown at layer pixel `(x, y)`.
    #[must_use]
    pub fn coverage(&self, x: i64, y: i64) -> u16 {
        let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
            return 0;
        };
        let key = TileKey::of_pixel(x, y);
        let shown = self.shown.read().unwrap_or_else(PoisonError::into_inner);
        shown.tiles.get(&key).map_or(0, |tile| {
            let [ox, oy] = key.origin();
            tile[((y - oy) * TILE + (x - ox)) as usize]
        })
    }

    /// Layer pixel `(x, y)` with the stroke on it: `under` (straight RGBA
    /// from 0 to 1) with the ink laid by the coverage there, through the
    /// same row kernel the merge uses, so what is shown is what the merge
    /// lays (but for the merge's rounding to the layer's bytes).
    #[must_use]
    pub fn over(&self, x: i64, y: i64, under: [f32; 4]) -> [f32; 4] {
        let coverage = self.coverage(x, y);
        if coverage == 0 {
            return under;
        }
        let mut pixel = under;
        paint_row(&mut pixel, &[coverage], self.ink);
        pixel
    }
}
