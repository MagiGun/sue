//! A stroke buffer that keeps a color for every pixel: for strokes whose
//! dabs differ in color (color jitter dab by dab, a color tip), where a
//! coverage buffer and one ink color cannot say what the stroke holds.
//!
//! Like [`crate::buffer::StrokeBuffer`] it has a committed part and a
//! tail, keeps its tiles in a sorted map and lays the stroke on the
//! layer once, on lift; its coverage adds up by the same [`Combine`], so
//! a stroke that crosses itself still never lays its paint twice. Where a
//! dab lands on paint of the same stroke, its color lies over it by its
//! coverage ([`TipBlend::Over`]) or the darker channels win
//! ([`TipBlend::Darken`]).

use std::collections::{BTreeMap, BTreeSet};

use super::plan::Planned;
use super::settings::TipBlend;
use super::stamp::{Stamp, Tips};
use crate::buffer::{AREA, Combine, TILE, TileKey};
use crate::raster::{PixelRect, Shape};
use crate::texture::Texture;

/// One part of a tile: straight color and coverage, four floats a pixel.
type Part = Box<[f32]>;

/// A tile's committed part and tail.
#[derive(Clone, Debug, Default)]
struct ColorTile {
    committed: Option<Part>,
    tail: Option<Part>,
}

/// The coverage of `a` and `b` combined by `combine`, as shares.
fn combine_share(combine: Combine, a: f32, b: f32) -> f32 {
    match combine {
        Combine::Max => a.max(b),
        Combine::BuildUp => a + b * (1.0 - a),
    }
}

/// The color of a pixel holding `old` (straight RGB, coverage `old_a`)
/// when `new` lands on it with coverage `c`.
fn lay(blend: TipBlend, old: [f32; 3], old_a: f32, new: [f32; 3], c: f32) -> [f32; 3] {
    if old_a <= 0.0 {
        return new;
    }
    match blend {
        TipBlend::Over => {
            // Over by the share the new paint takes of the pixel's paint.
            let t = (c / combine_share(Combine::BuildUp, old_a, c)).clamp(0.0, 1.0);
            [
                old[0] + (new[0] - old[0]) * t,
                old[1] + (new[1] - old[1]) * t,
                old[2] + (new[2] - old[2]) * t,
            ]
        }
        TipBlend::Darken => {
            let t = c.clamp(0.0, 1.0);
            [
                old[0] + (old[0].min(new[0]) - old[0]) * t,
                old[1] + (old[1].min(new[1]) - old[1]) * t,
                old[2] + (old[2].min(new[2]) - old[2]) * t,
            ]
        }
    }
}

/// The colored coverage of one stroke, in tiles of [`TILE`] pixels.
#[derive(Clone, Debug, Default)]
pub struct ColorBuffer {
    combine: Combine,
    blend: TipBlend,
    tiles: BTreeMap<TileKey, ColorTile>,
    tail: BTreeSet<TileKey>,
    dirty: BTreeSet<TileKey>,
}

impl ColorBuffer {
    /// An empty buffer whose coverage adds up by `combine` and whose
    /// colors meet by `blend`.
    #[must_use]
    pub fn new(combine: Combine, blend: TipBlend) -> Self {
        Self {
            combine,
            blend,
            ..Self::default()
        }
    }

    /// `true` when nothing is painted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The tiles that hold paint, in order.
    pub fn keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.tiles.keys().copied()
    }

    /// The tiles that changed since the last call, in order.
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// Clears the tail.
    pub fn clear_tail(&mut self) {
        for key in std::mem::take(&mut self.tail) {
            if let Some(tile) = self.tiles.get_mut(&key) {
                tile.tail = None;
                if tile.committed.is_none() {
                    self.tiles.remove(&key);
                }
            }
            self.dirty.insert(key);
        }
    }

    /// Folds the tail into the committed part, as if its dabs had been
    /// laid there.
    pub fn settle(&mut self) {
        let (combine, blend) = (self.combine, self.blend);
        for key in std::mem::take(&mut self.tail) {
            let Some(tile) = self.tiles.get_mut(&key) else {
                continue;
            };
            let Some(tail) = tile.tail.take() else {
                continue;
            };
            match &mut tile.committed {
                Some(committed) => {
                    for (c, t) in committed.chunks_mut(4).zip(tail.chunks(4)) {
                        if t[3] > 0.0 {
                            let color =
                                lay(blend, [c[0], c[1], c[2]], c[3], [t[0], t[1], t[2]], t[3]);
                            c[..3].copy_from_slice(&color);
                            c[3] = combine_share(combine, c[3], t[3]);
                        }
                    }
                }
                none => *none = Some(tail),
            }
        }
    }

    /// Stamps `dabs`, each in its planned color, into the committed part
    /// (`tail` false) or the tail (`tail` true; clear it first with
    /// [`Self::clear_tail`]).
    pub fn stamp(
        &mut self,
        tail: bool,
        dabs: &[Planned],
        tips: &Tips<'_>,
        texture: Option<&Texture<'_>>,
    ) {
        for planned in dabs {
            self.fill(tail, &Stamp::new(planned, tips, texture), planned.color);
        }
    }

    /// Lays `shape`'s coverage in `color`.
    pub fn fill(&mut self, tail: bool, shape: &impl Shape, color: [f32; 3]) {
        let rect = shape.bounds();
        if rect.is_empty() {
            return;
        }
        let (combine, blend) = (self.combine, self.blend);
        for y in rect.y0..rect.y1 {
            let cy = y as f32 + 0.5;
            let Some((lo, hi)) = shape.span(cy) else {
                continue;
            };
            let from = rect.x0.max(clamp_i32((lo - 0.5).ceil()));
            let to = rect.x1.min(clamp_i32((hi - 0.5).floor()).saturating_add(1));
            let mut x = from;
            while x < to {
                let key = TileKey::of_pixel(x, y);
                let [ox, oy] = key.origin();
                let end = to.min(ox + TILE);
                for px in x..end {
                    let c = shape.coverage([px as f32 + 0.5, cy]);
                    if c.is_nan() || c <= 0.0 {
                        continue;
                    }
                    let c = c.min(1.0);
                    let tile = self.tiles.entry(key).or_default();
                    let slot = if tail {
                        &mut tile.tail
                    } else {
                        &mut tile.committed
                    };
                    let part = slot.get_or_insert_with(|| vec![0.0; AREA * 4].into_boxed_slice());
                    let i = (((y - oy) * TILE + (px - ox)) * 4) as usize;
                    let old = [part[i], part[i + 1], part[i + 2]];
                    let color = lay(blend, old, part[i + 3], color, c);
                    part[i..i + 3].copy_from_slice(&color);
                    part[i + 3] = combine_share(combine, part[i + 3], c);
                    if tail {
                        self.tail.insert(key);
                    }
                    self.dirty.insert(key);
                }
                x = end;
            }
        }
    }

    /// The straight color and coverage of layer pixel `(x, y)`, the tail
    /// over the committed part.
    #[must_use]
    pub fn pixel(&self, x: i32, y: i32) -> ([f32; 3], f32) {
        let key = TileKey::of_pixel(x, y);
        let Some(tile) = self.tiles.get(&key) else {
            return ([0.0; 3], 0.0);
        };
        let [ox, oy] = key.origin();
        let i = (((y - oy) * TILE + (x - ox)) * 4) as usize;
        let read = |part: &Option<Part>| {
            part.as_ref()
                .map_or(([0.0; 3], 0.0), |p| ([p[i], p[i + 1], p[i + 2]], p[i + 3]))
        };
        let (cc, ca) = read(&tile.committed);
        let (tc, ta) = read(&tile.tail);
        if ta <= 0.0 {
            return (cc, ca);
        }
        (
            lay(self.blend, cc, ca, tc, ta),
            combine_share(self.combine, ca, ta),
        )
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
        for key in keys {
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
}

/// A float pixel coordinate as an `i32`, held to its range.
fn clamp_i32(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        v.clamp(i32::MIN as f32, i32::MAX as f32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Disc, Turn};

    fn dab(x: f32, color: [f32; 3]) -> Planned {
        Planned {
            disc: Disc::new([x, 10.0], 5.0, 1.0),
            aspect: 1.0,
            turn: Turn::NONE,
            tip: None,
            flip: [false; 2],
            hardness: 1.0,
            color,
            texture_strength: 1.0,
            wet: [1.0; 3],
            index: 0,
        }
    }

    #[test]
    fn each_dab_keeps_its_own_color_and_coverage_never_stacks() {
        let mut buffer = ColorBuffer::new(Combine::Max, TipBlend::Over);
        let red = [1.0, 0.0, 0.0];
        let blue = [0.0, 0.0, 1.0];
        buffer.stamp(
            false,
            &[dab(10.0, red), dab(14.0, blue)],
            &Tips::round(1.0),
            None,
        );
        let (left, a) = buffer.pixel(6, 9);
        assert_eq!(left, red);
        assert_eq!(a, 1.0);
        let (overlap, a) = buffer.pixel(12, 9);
        assert_eq!(overlap, blue, "the later dab lies over");
        assert_eq!(a, 1.0, "coverage is the most of the two, not more");
    }

    #[test]
    fn darken_keeps_the_darker_channels() {
        let mut buffer = ColorBuffer::new(Combine::Max, TipBlend::Darken);
        buffer.stamp(
            false,
            &[dab(10.0, [1.0, 0.2, 0.9]), dab(10.0, [0.3, 0.8, 0.9])],
            &Tips::round(1.0),
            None,
        );
        let (color, _) = buffer.pixel(10, 9);
        assert!((color[0] - 0.3).abs() < 1.0e-6 && (color[1] - 0.2).abs() < 1.0e-6);
    }

    #[test]
    fn the_tail_is_drawn_again_and_settles_as_if_laid() {
        let mut buffer = ColorBuffer::new(Combine::Max, TipBlend::Over);
        let tips = Tips::round(1.0);
        buffer.stamp(false, &[dab(10.0, [1.0, 0.0, 0.0])], &tips, None);
        buffer.stamp(true, &[dab(40.0, [0.0, 1.0, 0.0])], &tips, None);
        assert_eq!(buffer.pixel(40, 9).1, 1.0);
        buffer.clear_tail();
        assert_eq!(buffer.pixel(40, 9).1, 0.0);
        buffer.stamp(true, &[dab(12.0, [0.0, 0.0, 1.0])], &tips, None);
        let live = buffer.pixel(12, 9);
        buffer.settle();
        assert_eq!(buffer.pixel(12, 9), live);
        buffer.clear_tail();
        assert_eq!(buffer.pixel(12, 9), live);
    }
}
