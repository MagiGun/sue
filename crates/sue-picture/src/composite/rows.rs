//! Compositing pixels: one pixel through the layer stack, the fast path
//! that reads plain layers along a row, and rows shared out over the
//! cores.

use sue_core::{PixelFormat, RasterTile2D};

use super::gather::{Correction, Layer, Source};
use super::workers;
use crate::doc::cover;

/// The composited color of one pixel.
pub(super) fn pixel(layers: &[Layer<'_>], x: u32, y: u32) -> [f32; 4] {
    let mut state = Stack::EMPTY;
    fold(layers, x, y, &mut state);
    state.color
}

/// Where compositing a pixel stands after some layers: the color so far,
/// and the alpha and visibility of the layer the next clipped ones clip
/// to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stack {
    pub(super) color: [f32; 4],
    pub(super) base_alpha: f32,
    pub(super) base_visible: bool,
}

impl Stack {
    /// Before any layer.
    pub(super) const EMPTY: Self = Self {
        color: [0.0; 4],
        base_alpha: 1.0,
        base_visible: true,
    };
}

/// A correction layer over `color`: everything below, corrected and mixed
/// in by the layer's blend mode, as strongly as its opacity and masks say.
/// The alpha below stays as it is.
fn correct(color: &mut [f32; 4], correction: &Correction, layer: &Layer<'_>, strength: f32) {
    if !layer.style.visible || color[3] <= 0.0 || strength <= 0.0 {
        return;
    }
    let rgb = [color[0], color[1], color[2]];
    let fixed = layer.style.blend.mix(rgb, correction.apply(rgb));
    for channel in 0..3 {
        color[channel] += (fixed[channel] - color[channel]) * strength;
    }
}

/// How much the masks of the folders a layer passes through show at
/// `(x, y)`.
fn outer_cover(layer: &Layer<'_>, x: u32, y: u32) -> f32 {
    layer.outer.iter().fold(1.0, |shown, (mask, at)| {
        shown * cover(mask, i64::from(x) - at[0], i64::from(y) - at[1])
    })
}

/// How much a layer's own mask shows at `(x, y)` (seen through its
/// camera).
fn own_cover(layer: &Layer<'_>, x: u32, y: u32) -> f32 {
    layer.mask.map_or(1.0, |mask| {
        layer.seen(x, y).map_or(0.0, |(sx, sy)| {
            cover(
                mask,
                i64::from(sx) - layer.mask_offset[0],
                i64::from(sy) - layer.mask_offset[1],
            )
        })
    })
}

/// Lays `layers` over `state` at pixel `(x, y)`. A folder that is not
/// Through is composited on its own first, from nothing, and the result
/// laid over `state` as one layer.
pub(super) fn fold(layers: &[Layer<'_>], x: u32, y: u32, state: &mut Stack) {
    let Stack {
        mut color,
        mut base_alpha,
        mut base_visible,
    } = *state;
    for layer in layers {
        if let Source::Correction(correction) = &layer.source {
            let strength = layer.style.opacity * own_cover(layer, x, y) * outer_cover(layer, x, y);
            correct(&mut color, correction, layer, strength);
            continue;
        }
        if layer.empty_at(x, y) {
            // What a clear pixel does: the layers clipped to this one
            // show nothing over it.
            if !layer.style.clip {
                base_visible = layer.style.visible;
                base_alpha = 0.0;
            }
            continue;
        }
        if !layer.style.clip {
            base_visible = layer.style.visible;
        }
        if !layer.style.visible || (layer.style.clip && !base_visible) {
            if !layer.style.clip {
                base_alpha = 0.0;
            }
            continue;
        }
        let mut source = match &layer.source {
            Source::Group(members) => {
                let mut inner = Stack::EMPTY;
                fold(members, x, y, &mut inner);
                inner.color
            }
            _ => layer.sample(x, y),
        };
        source[3] *= own_cover(layer, x, y);
        if !layer.outer.is_empty() {
            source[3] *= outer_cover(layer, x, y);
        }
        if let Some((color, amount)) = layer.tint {
            for channel in 0..3 {
                source[channel] += (color[channel] - source[channel]) * amount;
            }
        }
        if let Some(lines) = &layer.lines {
            // Extract line: the layer as its lines, in black.
            source = [0.0, 0.0, 0.0, lines.at(x, y)];
        }
        if let (Some(rim), Some((_, darkness))) = (&layer.rim, layer.effects.watercolor)
            && source[3] > 0.0
        {
            // Watercolor edge: the paint darker where it stops.
            let near = rim.at(x, y);
            for channel in &mut source[..3] {
                *channel *= 1.0 - darkness * near * 0.85;
            }
        }
        if !layer.effects.is_plain() {
            source = layer.effects.apply(source, x, y);
        }
        if let (Some(edge), Some((_, rim))) = (&layer.edge, layer.effects.border) {
            // The border, under the layer.
            let cover = edge.at(x, y);
            let alpha = source[3] + cover * (1.0 - source[3]);
            if alpha > 0.0 {
                for channel in 0..3 {
                    source[channel] = (source[channel] * source[3]
                        + rim[channel] * cover * (1.0 - source[3]))
                        / alpha;
                }
            }
            source[3] = alpha;
        }
        source[3] *= layer.style.opacity;
        if layer.style.clip {
            source[3] *= base_alpha;
        } else {
            base_alpha = source[3];
        }
        color = layer.style.blend.composite(color, source);
    }
    *state = Stack {
        color,
        base_alpha,
        base_visible,
    };
}

/// Bytes to 0 to 1, as `f32::from(byte) / 255.0` gives them.
pub(super) fn unit(byte: u8) -> f32 {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| std::array::from_fn(|i| i as f32 / 255.0))[usize::from(byte)]
}

/// One row of a mask as the fast path reads it: its bytes, the picture x
/// of its first pixel, and the bytes a pixel takes (4 for `Rgba8`, whose
/// last byte is the cover; 1 for `Gray8`).
#[derive(Clone, Copy)]
pub(super) struct MaskRow<'a> {
    bytes: &'a [u8],
    left: i64,
    step: usize,
}

impl MaskRow<'_> {
    /// How much shows at picture x `x`.
    fn at(&self, x: u32) -> f32 {
        let mx = i64::from(x) - self.left;
        if mx >= 0 && (mx as usize) * self.step < self.bytes.len() {
            unit(self.bytes[mx as usize * self.step + self.step - 1])
        } else {
            0.0
        }
    }
}

/// A layer as the fast path reads it along one row: the row's pixels and
/// mask, found once instead of per pixel.
pub(super) enum RowSource<'a> {
    Paper([f32; 3]),
    Correction(&'a Correction),
    Pixels {
        /// The layer's row (empty when the row misses the layer).
        bytes: &'a [u8],
        /// The picture x of the row's first pixel.
        left: i64,
        mask: Option<MaskRow<'a>>,
    },
    /// A folder composited on its own: its members' rows.
    Group(Vec<RowSource<'a>>),
}

impl Layer<'_> {
    /// `true` for what the fast path handles: plain 8-bit pixels, paper, a
    /// correction or a folder of such layers, with no camera, tint,
    /// effect, border or folder mask, and a mask only on pixels.
    pub(super) fn plain(&self) -> bool {
        let source = match &self.source {
            Source::Paper(_) | Source::Correction(_) => self.mask.is_none(),
            Source::Raster { raster, .. } => {
                raster.format() == PixelFormat::Rgba8
                    && raster.bytes().is_some()
                    && self.mask.is_none_or(|mask| {
                        matches!(mask.format(), PixelFormat::Rgba8 | PixelFormat::Gray8)
                            && mask.bytes().is_some()
                    })
            }
            Source::Group(members) => self.mask.is_none() && members.iter().all(Layer::plain),
        };
        source
            && self.outer.is_empty()
            && self.live.is_none()
            && self.warp.is_none()
            && self.tint.is_none()
            && self.effects.is_plain()
            && self.edge.is_none()
            && self.rim.is_none()
            && self.lines.is_none()
    }

    /// The layer along picture row `y`.
    pub(super) fn row(&self, y: u32) -> RowSource<'_> {
        match &self.source {
            Source::Paper(rgb) => RowSource::Paper(*rgb),
            Source::Correction(correction) => RowSource::Correction(correction),
            Source::Raster { raster, offset } => {
                let (bytes, left) = raster_row(raster, *offset, y);
                let mask = self.mask.map(|mask| {
                    let (bytes, left) = raster_row(mask, self.mask_offset, y);
                    MaskRow {
                        bytes,
                        left,
                        step: if mask.format() == PixelFormat::Gray8 {
                            1
                        } else {
                            4
                        },
                    }
                });
                RowSource::Pixels { bytes, left, mask }
            }
            Source::Group(members) => {
                RowSource::Group(members.iter().map(|member| member.row(y)).collect())
            }
        }
    }
}

/// Row `y` of the picture in `raster` placed at `offset`: its pixels (empty
/// when the row misses it) and the picture x of the first.
fn raster_row(raster: &RasterTile2D, offset: [i64; 2], y: u32) -> (&[u8], i64) {
    let ly = i64::from(y) - offset[1];
    let bytes = raster.bytes().unwrap_or_default();
    if ly < 0 || ly >= i64::from(raster.height()) {
        return (&[], offset[0]);
    }
    let stride = raster.width() as usize * raster.format().bytes_per_pixel();
    let start = ly as usize * stride;
    (
        bytes.get(start..start + stride).unwrap_or_default(),
        offset[0],
    )
}

/// [`pixel`] for plain layers (see [`Layer::plain`]), reading each from
/// its row: the same steps and the same arithmetic, without looking up the
/// layer's format, size and mask for every pixel.
pub(super) fn pixel_plain(
    layers: &[Layer<'_>],
    rows: &[RowSource<'_>],
    x: u32,
    y: u32,
) -> [f32; 4] {
    let mut stack = Stack::EMPTY;
    fold_plain(layers, rows, x, y, &mut stack);
    stack.color
}

/// [`fold`] for plain layers (see [`Layer::plain`]), reading each from its
/// row, as [`pixel_plain`] does.
pub(super) fn fold_plain(
    layers: &[Layer<'_>],
    rows: &[RowSource<'_>],
    x: u32,
    y: u32,
    state: &mut Stack,
) {
    let Stack {
        mut color,
        mut base_alpha,
        mut base_visible,
    } = *state;
    for (layer, row) in layers.iter().zip(rows) {
        if let RowSource::Correction(correction) = row {
            correct(&mut color, correction, layer, layer.style.opacity);
            continue;
        }
        if layer.empty_at(x, y) {
            // What a clear pixel does: the layers clipped to this one
            // show nothing over it.
            if !layer.style.clip {
                base_visible = layer.style.visible;
                base_alpha = 0.0;
            }
            continue;
        }
        if !layer.style.clip {
            base_visible = layer.style.visible;
        }
        if !layer.style.visible || (layer.style.clip && !base_visible) {
            if !layer.style.clip {
                base_alpha = 0.0;
            }
            continue;
        }
        let mut source = match row {
            RowSource::Paper(rgb) => [rgb[0], rgb[1], rgb[2], 1.0],
            RowSource::Correction(_) => [0.0; 4],
            RowSource::Pixels { bytes, left, mask } => {
                let lx = i64::from(x) - left;
                let mut source = if lx >= 0 && (lx as usize) * 4 < bytes.len() {
                    let p = &bytes[lx as usize * 4..lx as usize * 4 + 4];
                    [unit(p[0]), unit(p[1]), unit(p[2]), unit(p[3])]
                } else {
                    [0.0; 4]
                };
                if let Some(mask) = mask {
                    source[3] *= mask.at(x);
                }
                source
            }
            RowSource::Group(rows) => {
                let Source::Group(members) = &layer.source else {
                    continue;
                };
                let mut inner = Stack::EMPTY;
                fold_plain(members, rows, x, y, &mut inner);
                inner.color
            }
        };
        source[3] *= layer.style.opacity;
        if layer.style.clip {
            source[3] *= base_alpha;
        } else {
            base_alpha = source[3];
        }
        color = layer.style.blend.composite(color, source);
    }
    *state = Stack {
        color,
        base_alpha,
        base_visible,
    };
}

/// `color` (straight RGBA) as the screen shows it at `(x, y)`: over the
/// checkerboard, opaque.
pub(super) fn shown(color: [f32; 4], x: u32, y: u32) -> [u8; 4] {
    let under = checker(x, y);
    let alpha = color[3];
    let mut out = [255; 4];
    for (slot, value) in out.iter_mut().zip(color).take(3) {
        *slot = to_byte(value * alpha + under * (1.0 - alpha));
    }
    out
}

/// Composites pixels `x0..x1` of row `y` into `row` (the whole row of the
/// screen image), over the checkerboard.
pub(super) fn composite_span(layers: &[Layer<'_>], y: u32, x0: u32, x1: u32, row: &mut [u8]) {
    let plain = layers.iter().all(Layer::plain);
    let rows: Vec<RowSource<'_>> = if plain {
        layers.iter().map(|layer| layer.row(y)).collect()
    } else {
        Vec::new()
    };
    let span = &mut row[x0 as usize * 4..x1 as usize * 4];
    for (x, slot) in (x0..x1).zip(span.as_chunks_mut::<4>().0) {
        let color = if plain {
            pixel_plain(layers, &rows, x, y)
        } else {
            pixel(layers, x, y)
        };
        let under = checker(x, y);
        let alpha = color[3];
        for (channel, value) in slot.iter_mut().zip(color) {
            *channel = to_byte(value * alpha + under * (1.0 - alpha));
        }
        slot[3] = 255;
    }
}

/// Runs `work(y, row)` for each `stride`-byte row of `buffer` (row `y0`
/// first), spread over the cores when the area is large.
pub(super) fn rows(
    buffer: &mut [u8],
    stride: usize,
    y0: u32,
    pixels: usize,
    work: impl Fn(u32, &mut [u8]) + Sync,
) {
    if stride == 0 {
        return;
    }
    let count = buffer.len() / stride;
    let threads = if pixels < 1 << 16 {
        1
    } else {
        workers().clamp(1, 32)
    };
    if threads <= 1 || count < 2 {
        for (index, row) in buffer.chunks_mut(stride).enumerate() {
            work(y0 + index as u32, row);
        }
        return;
    }
    let per = count.div_ceil(threads);
    std::thread::scope(|scope| {
        for (band, chunk) in buffer.chunks_mut(per * stride).enumerate() {
            let work = &work;
            scope.spawn(move || {
                for (index, row) in chunk.chunks_mut(stride).enumerate() {
                    work(y0 + (band * per + index) as u32, row);
                }
            });
        }
    });
}

/// The gray of the transparency checkerboard at a pixel.
pub(super) fn checker(x: u32, y: u32) -> f32 {
    if ((x / 8) + (y / 8)).is_multiple_of(2) {
        1.0
    } else {
        0.8
    }
}

/// 0 to 1 to a byte, rounded to the nearest (without the library rounding,
/// slow in a pixel loop).
pub(super) fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
