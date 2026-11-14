//! One tile of a stroke buffer, and how coverage adds up.

use crate::raster::kernel::FULL;

/// The side of a tile, in pixels (the same as the legacy engine's and the
/// compositor's tiles).
pub const TILE: i32 = 64;

/// Pixels in a tile.
pub const AREA: usize = (TILE * TILE) as usize;

/// Which tile: its column and row, so it covers layer pixels
/// `x * TILE .. (x + 1) * TILE` by the same in `y`. Tiles sort by row,
/// then column, so every walk over a buffer goes the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    /// The row of tiles.
    pub y: i32,
    /// The column of tiles.
    pub x: i32,
}

impl TileKey {
    /// The tile holding layer pixel `(x, y)`.
    #[must_use]
    pub const fn of_pixel(x: i32, y: i32) -> Self {
        Self {
            x: x.div_euclid(TILE),
            y: y.div_euclid(TILE),
        }
    }

    /// The layer pixel at the tile's top left.
    #[must_use]
    pub const fn origin(&self) -> [i32; 2] {
        [self.x * TILE, self.y * TILE]
    }
}

/// How coverage that lands on coverage adds up: within a stroke, and the
/// tail onto the committed part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Combine {
    /// The most of the two: a stroke never darkens where it crosses
    /// itself, and a dab laid twice lays nothing more. Pens and pencils.
    #[default]
    Max,
    /// Each adds over the other (`a + b - a * b`): dabs build up where
    /// they overlap, up to full. Airbrushes and other build-up brushes.
    BuildUp,
}

impl Combine {
    /// `old` and `new` combined.
    #[must_use]
    pub const fn apply(self, old: u16, new: u16) -> u16 {
        match self {
            Self::Max => {
                if new > old {
                    new
                } else {
                    old
                }
            }
            Self::BuildUp => {
                let (a, b, full) = (old as u32, new as u32, FULL as u32);
                // a + b (1 - a), rounded up so that dabs repeated over
                // one place do reach full; never above full.
                (a + (b * (full - a)).div_ceil(full)) as u16
            }
        }
    }
}

/// A tile's coverage: the committed part, which only grows, and the
/// tail, which is cleared and drawn again at each update.
#[derive(Clone, Debug, Default)]
pub(super) struct Tile {
    pub(super) committed: Option<Box<[u16]>>,
    pub(super) tail: Option<Box<[u16]>>,
}

impl Tile {
    /// A part of the tile, made empty if it is not there yet.
    pub(super) fn part(slot: &mut Option<Box<[u16]>>) -> &mut [u16] {
        slot.get_or_insert_with(|| vec![0; AREA].into_boxed_slice())
    }

    /// The tile's coverage as the layer sees it, into `out`.
    pub(super) fn combined(&self, combine: Combine, out: &mut [u16]) {
        let out = &mut out[..AREA];
        match (&self.committed, &self.tail) {
            (Some(c), Some(t)) => {
                for ((o, &c), &t) in out.iter_mut().zip(c.iter()).zip(t.iter()) {
                    *o = combine.apply(c, t);
                }
            }
            (Some(only), None) | (None, Some(only)) => out.copy_from_slice(only),
            (None, None) => out.fill(0),
        }
    }

    /// `true` when the tile holds neither part.
    pub(super) fn is_empty(&self) -> bool {
        self.committed.is_none() && self.tail.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_cover_negative_pixels_too() {
        assert_eq!(TileKey::of_pixel(0, 63), TileKey { x: 0, y: 0 });
        assert_eq!(TileKey::of_pixel(-1, 64), TileKey { x: -1, y: 1 });
        assert_eq!(TileKey::of_pixel(-64, -65).origin(), [-64, -128]);
    }

    #[test]
    fn max_never_stacks_and_build_up_never_passes_full() {
        assert_eq!(Combine::Max.apply(30_000, 30_000), 30_000);
        assert_eq!(Combine::Max.apply(10, 9), 10);
        let half = FULL / 2 + 1;
        let twice = Combine::BuildUp.apply(half, half);
        assert!((49_150..=49_153).contains(&twice), "{twice}");
        assert_eq!(Combine::BuildUp.apply(FULL, FULL), FULL);
        assert_eq!(Combine::BuildUp.apply(0, 1234), 1234);
        assert_eq!(Combine::BuildUp.apply(1234, 0), 1234);
        let mut a = 0;
        for _ in 0..1000 {
            a = Combine::BuildUp.apply(a, 5000);
        }
        assert_eq!(a, FULL);
    }
}
