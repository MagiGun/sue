//! Compositing: a picture's layers, read from the arena, laid over each
//! other with the shared blend modes ([`sue_model::BlendMode`]).
//!
//! [`render`] composites any rectangle (exports, merges, the eyedropper).
//! A [`Compositor`] keeps the screen image of one picture: it recomposites
//! everything when the layers changed behind its back (undo, a property, a
//! new layer) and only the painted rectangle while a stroke is drawn.
//! Large areas are split into bands of rows composited on every core.

use std::collections::HashSet;

use sue_core::{Arena, ComponentKind, EntityId, PixelFormat, RasterTile2D};
use sue_model::picture::{self, LayerKind, LayerStyle};

/// What a render includes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Paper layers.
    pub paper: bool,
    /// Draft layers.
    pub drafts: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            paper: true,
            drafts: true,
        }
    }
}

/// How an animation frame shows the layers: the cels it hides, and the
/// ones shown faintly around it (onion skin, the light table).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameView {
    /// Cels not on this frame.
    pub hidden: HashSet<EntityId>,
    /// Cels shown anyway.
    pub onion: Vec<Skin>,
    /// Camera folders and what each canvas point sees through them (an
    /// affine map, `camera::Camera::map`).
    pub cameras: Vec<(EntityId, [f32; 6])>,
}

/// A hidden cel shown anyway: in a color, faintly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    /// The cel.
    pub cel: EntityId,
    /// Its skin color (sRGB, 0 to 1).
    pub color: [f32; 3],
    /// How much of its own color gives way to the skin color: 1 for
    /// Monochrome, a half for Half color, 0 for Color.
    pub tint: f32,
    /// How much shows, 0 to 1.
    pub opacity: f32,
}

/// A correction layer's change, ready for every pixel: a table a channel
/// for corrections that treat channels apart, else the filter itself.
enum Correction {
    Table(Box<[[u8; 256]; 3]>),
    Pixel(Box<crate::filters::Filter>),
}

impl Correction {
    fn of(filter: crate::filters::Filter) -> Self {
        if filter.per_channel() {
            let mut table = Box::new([[0_u8; 256]; 3]);
            for value in 0..=255_u8 {
                let out = filter.point([value, value, value, 255], 0, 0);
                for channel in 0..3 {
                    table[channel][usize::from(value)] = out[channel];
                }
            }
            Self::Table(table)
        } else {
            Self::Pixel(Box::new(filter))
        }
    }

    /// `rgb` (straight, 0 to 1) corrected.
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        match self {
            Self::Table(table) => [0, 1, 2]
                .map(|channel| f32::from(table[channel][usize::from(byte(rgb[channel]))]) / 255.0),
            Self::Pixel(filter) => {
                let out = filter.point([byte(rgb[0]), byte(rgb[1]), byte(rgb[2]), 255], 0, 0);
                [0, 1, 2].map(|channel| f32::from(out[channel]) / 255.0)
            }
        }
    }
}

enum Source<'a> {
    Paper([f32; 3]),
    /// A correction layer: it changes the picture below it.
    Correction(Correction),
    Raster {
        raster: &'a RasterTile2D,
        offset: [i64; 2],
    },
}

struct Layer<'a> {
    /// The layer's entity (none for a thumbnail's stand-in).
    id: Option<EntityId>,
    source: Source<'a>,
    style: LayerStyle,
    /// Drawn toward this color by this much (onion skin), keeping its
    /// alpha.
    tint: Option<([f32; 3], f32)>,
    /// The layer mask, the layer's size: its alpha is how much shows.
    mask: Option<&'a RasterTile2D>,
    /// Seen through a camera: where each picture pixel looks on the layer.
    warp: Option<[f32; 6]>,
    /// Its Layer Property effects.
    effects: crate::effects::Effects,
    /// Its border's cover over the rectangle being drawn.
    edge: Option<crate::effects::Edge>,
    /// Which tiles of the picture it has pixels in, when known for its
    /// current pixels.
    occupancy: Option<std::sync::Arc<Occupancy>>,
}

/// Side of an [`Occupancy`] tile.
const OCCUPANCY_TILE: u32 = 64;

/// Which 64 pixel tiles of the picture a layer has any pixel in (alpha
/// above 0), so compositing passes over its empty ones with a bit test.
#[derive(Debug)]
struct Occupancy {
    columns: u32,
    bits: Vec<u64>,
}

impl Occupancy {
    /// The tiles of a picture of `size` that `raster` (8-bit RGBA, placed
    /// at `offset`) covers with anything.
    fn of(raster: &RasterTile2D, offset: [i64; 2], size: [u32; 2]) -> Option<Self> {
        if raster.format() != PixelFormat::Rgba8 {
            return None;
        }
        let bytes = raster.bytes()?;
        let columns = size[0].div_ceil(OCCUPANCY_TILE);
        let rows = size[1].div_ceil(OCCUPANCY_TILE);
        let count = (columns * rows) as usize;
        let mut bits = vec![0_u64; count.div_ceil(64)];
        let (width, height) = (i64::from(raster.width()), i64::from(raster.height()));
        let stride = width as usize * 4;
        for ly in 0..height {
            let py = ly + offset[1];
            if py < 0 || py >= i64::from(size[1]) {
                continue;
            }
            let ty = py as u32 / OCCUPANCY_TILE;
            let row = &bytes[ly as usize * stride..(ly as usize + 1) * stride];
            // Picture x from the first column the layer reaches, tile by tile.
            let mut px = offset[0].max(0);
            let end = (offset[0] + width).min(i64::from(size[0]));
            while px < end {
                let tx = px as u32 / OCCUPANCY_TILE;
                let tile_end = (i64::from((tx + 1) * OCCUPANCY_TILE)).min(end);
                let index = (ty * columns + tx) as usize;
                if bits[index / 64] & (1 << (index % 64)) == 0 {
                    let from = (px - offset[0]) as usize * 4;
                    let to = (tile_end - offset[0]) as usize * 4;
                    if row[from..to].as_chunks::<4>().0.iter().any(|p| p[3] != 0) {
                        bits[index / 64] |= 1 << (index % 64);
                    }
                }
                px = tile_end;
            }
        }
        Some(Self { columns, bits })
    }

    /// `true` if the layer has any pixel in the tile of picture pixel
    /// `(x, y)`.
    fn has(&self, x: u32, y: u32) -> bool {
        let index = ((y / OCCUPANCY_TILE) * self.columns + x / OCCUPANCY_TILE) as usize;
        self.bits
            .get(index / 64)
            .is_none_or(|word| word & (1 << (index % 64)) != 0)
    }
}

/// Works out the borders of `layers` with a border effect over `rect`.
fn prepare(layers: &mut [Layer<'_>], rect: [u32; 4]) {
    for layer in layers.iter_mut() {
        let Some((width, _)) = layer.effects.border else {
            continue;
        };
        if width <= 0.0 || !layer.style.visible {
            continue;
        }
        let edge = crate::effects::Edge::build(
            |x, y| {
                if x < 0 || y < 0 {
                    0.0
                } else {
                    layer.sample(x as u32, y as u32)[3]
                }
            },
            rect,
            width,
        );
        layer.edge = Some(edge);
    }
}

impl Layer<'_> {
    /// `true` when the layer has nothing at picture pixel `(x, y)`, known
    /// without reading it: its tile there is empty, and nothing it does
    /// (a camera, an effect, a border) could show there anyway.
    fn empty_at(&self, x: u32, y: u32) -> bool {
        matches!(self.source, Source::Raster { .. })
            && self.warp.is_none()
            && self.effects.is_plain()
            && self
                .occupancy
                .as_ref()
                .is_some_and(|tiles| !tiles.has(x, y))
    }

    /// The layer's straight RGBA at picture pixel `(x, y)`, through its
    /// camera when it has one.
    fn sample(&self, x: u32, y: u32) -> [f32; 4] {
        let Some(m) = self.warp else {
            return self.texel(i64::from(x), i64::from(y));
        };
        if !matches!(self.source, Source::Raster { .. }) {
            return self.texel(i64::from(x), i64::from(y));
        }
        // Bilinear, premultiplied, at the point the camera sees.
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let (wx, wy) = (
            m[0] * px + m[1] * py + m[4] - 0.5,
            m[2] * px + m[3] * py + m[5] - 0.5,
        );
        let (x0, y0) = (wx.floor(), wy.floor());
        let (tx, ty) = (wx - x0, wy - y0);
        let mut sum = [0.0_f32; 4];
        for (dx, dy, weight) in [
            (0, 0, (1.0 - tx) * (1.0 - ty)),
            (1, 0, tx * (1.0 - ty)),
            (0, 1, (1.0 - tx) * ty),
            (1, 1, tx * ty),
        ] {
            let p = self.texel(x0 as i64 + dx, y0 as i64 + dy);
            for channel in 0..3 {
                sum[channel] += p[channel] * p[3] * weight;
            }
            sum[3] += p[3] * weight;
        }
        if sum[3] <= 0.0 {
            return [0.0; 4];
        }
        [
            sum[0] / sum[3],
            sum[1] / sum[3],
            sum[2] / sum[3],
            sum[3].min(1.0),
        ]
    }

    /// Where on the layer picture pixel `(x, y)` looks, to the nearest
    /// pixel (for its mask).
    fn seen(&self, x: u32, y: u32) -> Option<(u32, u32)> {
        let Some(m) = self.warp else {
            return Some((x, y));
        };
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let (wx, wy) = (m[0] * px + m[1] * py + m[4], m[2] * px + m[3] * py + m[5]);
        (wx >= 0.0 && wy >= 0.0).then_some((wx as u32, wy as u32))
    }

    /// The layer's straight RGBA at picture pixel `(x, y)`, as it is.
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        match &self.source {
            Source::Paper(rgb) => [rgb[0], rgb[1], rgb[2], 1.0],
            // A correction has no pixels of its own.
            Source::Correction(_) => [0.0; 4],
            Source::Raster { raster, offset } => {
                let lx = x - offset[0];
                let ly = y - offset[1];
                if lx < 0
                    || ly < 0
                    || lx >= i64::from(raster.width())
                    || ly >= i64::from(raster.height())
                {
                    return [0.0; 4];
                }
                let index = ly as usize * raster.width() as usize + lx as usize;
                match raster.format() {
                    PixelFormat::Rgba8 => raster.bytes().map_or([0.0; 4], |bytes| {
                        let p = &bytes[index * 4..index * 4 + 4];
                        [
                            f32::from(p[0]) / 255.0,
                            f32::from(p[1]) / 255.0,
                            f32::from(p[2]) / 255.0,
                            f32::from(p[3]) / 255.0,
                        ]
                    }),
                    PixelFormat::Gray8 => raster.bytes().map_or([0.0; 4], |bytes| {
                        let value = f32::from(bytes[index]) / 255.0;
                        [value, value, value, 1.0]
                    }),
                    PixelFormat::RgbaF32 => raster.f32_samples().map_or([0.0; 4], |samples| {
                        let p = &samples[index * 4..index * 4 + 4];
                        [p[0], p[1], p[2], p[3]].map(|value| value.clamp(0.0, 1.0))
                    }),
                }
            }
        }
    }
}

fn gather(arena: &Arena, picture: EntityId, options: RenderOptions) -> Vec<Layer<'_>> {
    gather_view(arena, picture, options, None)
}

fn gather_view<'a>(
    arena: &'a Arena,
    picture: EntityId,
    options: RenderOptions,
    view: Option<&FrameView>,
) -> Vec<Layer<'a>> {
    let dpi = arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(picture::DPI))
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(72.0);
    picture::layers(arena, picture)
        .into_iter()
        .filter_map(|id| arena.get_by_id(id).ok())
        .filter_map(|entity| {
            let mut style = LayerStyle::of(entity);
            // Folders pass their layers through: hidden with them, and
            // as see-through as each folder around them.
            for folder in crate::doc::folders_around(arena, entity.id()) {
                if let Ok(folder) = arena.get_by_id(folder) {
                    let outer = LayerStyle::of(folder);
                    style.visible &= outer.visible;
                    style.opacity *= outer.opacity;
                }
            }
            let mut tint = None;
            if let Some(view) = view
                && view.hidden.contains(&entity.id())
            {
                match view.onion.iter().find(|skin| skin.cel == entity.id()) {
                    Some(skin) => {
                        style.opacity *= skin.opacity;
                        tint = Some((skin.color, skin.tint));
                    }
                    None => style.visible = false,
                }
            }
            if style.draft && !options.drafts {
                style.visible = false;
            }
            let source = match LayerKind::of(entity) {
                LayerKind::Raster => Source::Raster {
                    raster: entity.component(ComponentKind::RasterTile2D)?.as_raster()?,
                    offset: style.offset,
                },
                LayerKind::Paper => {
                    if !options.paper {
                        style.visible = false;
                    }
                    let color = picture::paper_color(entity).to_space(sue_core::ColorSpace::Srgb);
                    Source::Paper(color.rgb)
                }
                LayerKind::Other if entity.class() == crate::doc::CORRECTION => {
                    let filter = entity
                        .property(crate::doc::CORRECTION_FILTER)
                        .and_then(sue_core::Value::as_str)
                        .and_then(crate::filters::Filter::from_text)?;
                    Source::Correction(Correction::of(filter))
                }
                LayerKind::Other => return None,
            };
            let mask = crate::doc::mask_of(arena, entity.id())
                .filter(|_| crate::doc::mask_on(arena, entity.id()))
                .and_then(|mask| arena.get_by_id(mask).ok())
                .and_then(|mask| mask.component(ComponentKind::RasterTile2D)?.as_raster());
            // Seen through the innermost camera folder around it.
            let warp = view.and_then(|view| {
                crate::doc::folders_around(arena, entity.id())
                    .into_iter()
                    .find_map(|folder| {
                        view.cameras
                            .iter()
                            .find(|(id, _)| *id == folder)
                            .map(|(_, map)| *map)
                    })
            });
            let effects = if matches!(source, Source::Raster { .. }) {
                crate::effects::Effects::of(entity, dpi)
            } else {
                crate::effects::Effects::default()
            };
            Some(Layer {
                id: Some(entity.id()),
                source,
                style,
                tint,
                mask,
                warp,
                effects,
                edge: None,
                occupancy: None,
            })
        })
        .collect()
}

/// How much of a masked layer shows at picture pixel `(x, y)`.
fn mask_at(mask: &RasterTile2D, offset: [i64; 2], x: u32, y: u32) -> f32 {
    let (lx, ly) = (i64::from(x) - offset[0], i64::from(y) - offset[1]);
    if lx < 0 || ly < 0 || lx >= i64::from(mask.width()) || ly >= i64::from(mask.height()) {
        return 0.0;
    }
    mask.bytes().map_or(1.0, |bytes| {
        f32::from(bytes[((ly as u32 * mask.width() + lx as u32) * 4 + 3) as usize]) / 255.0
    })
}

/// The composited color of one pixel.
fn pixel(layers: &[Layer<'_>], x: u32, y: u32) -> [f32; 4] {
    let mut state = Stack::EMPTY;
    fold(layers, x, y, &mut state);
    state.color
}

/// Where compositing a pixel stands after some layers: the color so far,
/// and the alpha and visibility of the layer the next clipped ones clip
/// to.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stack {
    color: [f32; 4],
    base_alpha: f32,
    base_visible: bool,
}

impl Stack {
    /// Before any layer.
    const EMPTY: Self = Self {
        color: [0.0; 4],
        base_alpha: 1.0,
        base_visible: true,
    };
}

/// Lays `layers` over `state` at pixel `(x, y)`.
fn fold(layers: &[Layer<'_>], x: u32, y: u32, state: &mut Stack) {
    let Stack {
        mut color,
        mut base_alpha,
        mut base_visible,
    } = *state;
    for layer in layers {
        if let Source::Correction(correction) = &layer.source {
            // Everything below, corrected, by the layer's opacity.
            if layer.style.visible && color[3] > 0.0 {
                let fixed = correction.apply([color[0], color[1], color[2]]);
                for channel in 0..3 {
                    color[channel] += (fixed[channel] - color[channel]) * layer.style.opacity;
                }
            }
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
        let mut source = layer.sample(x, y);
        if let Some(mask) = layer.mask {
            source[3] *= layer
                .seen(x, y)
                .map_or(0.0, |(sx, sy)| mask_at(mask, layer.style.offset, sx, sy));
        }
        if let Some((color, amount)) = layer.tint {
            for channel in 0..3 {
                source[channel] += (color[channel] - source[channel]) * amount;
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
fn unit(byte: u8) -> f32 {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| std::array::from_fn(|i| i as f32 / 255.0))[usize::from(byte)]
}

/// A layer as the fast path reads it along one row: the row's pixels and
/// mask, found once instead of per pixel.
enum RowSource<'a> {
    Paper([f32; 3]),
    Correction(&'a Correction),
    Pixels {
        /// The layer's row (empty when the row misses the layer).
        bytes: &'a [u8],
        /// The picture x of the row's first pixel.
        left: i64,
        mask: Option<(&'a [u8], i64)>,
    },
}

impl Layer<'_> {
    /// `true` for what the fast path handles: plain 8-bit pixels, paper or
    /// a correction, with no camera, tint, effect or border.
    fn plain(&self) -> bool {
        let source = match &self.source {
            Source::Paper(_) | Source::Correction(_) => true,
            Source::Raster { raster, .. } => {
                raster.format() == PixelFormat::Rgba8 && raster.bytes().is_some()
            }
        };
        source
            && self.warp.is_none()
            && self.tint.is_none()
            && self.effects.is_plain()
            && self.edge.is_none()
            && self
                .mask
                .is_none_or(|mask| mask.format() == PixelFormat::Rgba8 && mask.bytes().is_some())
    }

    /// The layer along picture row `y`.
    fn row(&self, y: u32) -> RowSource<'_> {
        match &self.source {
            Source::Paper(rgb) => RowSource::Paper(*rgb),
            Source::Correction(correction) => RowSource::Correction(correction),
            Source::Raster { raster, offset } => {
                let (bytes, left) = raster_row(raster, *offset, y);
                let mask = self.mask.map(|mask| raster_row(mask, self.style.offset, y));
                RowSource::Pixels { bytes, left, mask }
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
    let stride = raster.width() as usize * 4;
    let start = ly as usize * stride;
    (
        bytes.get(start..start + stride).unwrap_or_default(),
        offset[0],
    )
}

/// [`pixel`] for plain layers (see [`Layer::plain`]), reading each from
/// its row: the same steps and the same arithmetic, without looking up the
/// layer's format, size and mask for every pixel.
fn pixel_plain(layers: &[Layer<'_>], rows: &[RowSource<'_>], x: u32, y: u32) -> [f32; 4] {
    let mut color = [0.0; 4];
    let mut base_alpha = 1.0;
    let mut base_visible = true;
    for (layer, row) in layers.iter().zip(rows) {
        if let RowSource::Correction(correction) = row {
            if layer.style.visible && color[3] > 0.0 {
                let fixed = correction.apply([color[0], color[1], color[2]]);
                for channel in 0..3 {
                    color[channel] += (fixed[channel] - color[channel]) * layer.style.opacity;
                }
            }
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
                if let Some((mask, mask_left)) = mask {
                    let mx = i64::from(x) - mask_left;
                    source[3] *= if mx >= 0 && (mx as usize) * 4 < mask.len() {
                        unit(mask[mx as usize * 4 + 3])
                    } else {
                        0.0
                    };
                }
                source
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
    color
}

/// The [`Focus`] tiles `wanted`, made on every core: each pixel's stack
/// under layer `index`, and the layers over it flattened when `flat`.
#[allow(clippy::type_complexity)]
fn focus_tiles(
    layers: &[Layer<'_>],
    index: usize,
    flat: bool,
    wanted: &[(u32, u32)],
    size: [u32; 2],
) -> Vec<((u32, u32), Box<[[u8; 6]]>, Option<Box<[[u8; 4]]>>)> {
    let make = |&(tx, ty): &(u32, u32)| {
        let area = (FOCUS_TILE * FOCUS_TILE) as usize;
        let mut below = vec![[0_u8; 6]; area];
        let mut above = flat.then(|| vec![[0_u8; 4]; area]);
        for y in ty * FOCUS_TILE..((ty + 1) * FOCUS_TILE).min(size[1]) {
            for x in tx * FOCUS_TILE..((tx + 1) * FOCUS_TILE).min(size[0]) {
                let at = ((y % FOCUS_TILE) * FOCUS_TILE + x % FOCUS_TILE) as usize;
                let mut stack = Stack::EMPTY;
                fold(&layers[..index], x, y, &mut stack);
                let [r, g, b, a] = stack.color.map(to_byte);
                below[at] = [
                    r,
                    g,
                    b,
                    a,
                    to_byte(stack.base_alpha),
                    u8::from(stack.base_visible),
                ];
                if let Some(above) = &mut above {
                    let mut over = Stack::EMPTY;
                    fold(&layers[index + 1..], x, y, &mut over);
                    above[at] = over.color.map(to_byte);
                }
            }
        }
        (
            (tx, ty),
            below.into_boxed_slice(),
            above.map(Vec::into_boxed_slice),
        )
    };
    let threads = if wanted.len() < 4 {
        1
    } else {
        workers().min(wanted.len())
    };
    if threads <= 1 {
        return wanted.iter().map(make).collect();
    }
    let per = wanted.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = wanted
            .chunks(per)
            .map(|chunk| scope.spawn(move || chunk.iter().map(make).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

/// The color at `(x, y)` through a [`Focus`]: the kept stack under layer
/// `index` (`below`, the pixel's tile; its layers when not kept), the layer
/// itself, then the kept flattened stack over it (`above`; the layers over
/// it when that is not kept).
fn focused_color(
    layers: &[Layer<'_>],
    below: Option<&[[u8; 6]]>,
    above: Option<&[[u8; 4]]>,
    index: usize,
    x: u32,
    y: u32,
) -> [f32; 4] {
    let unit = |byte: u8| f32::from(byte) / 255.0;
    let at = ((y % FOCUS_TILE) * FOCUS_TILE + x % FOCUS_TILE) as usize;
    let mut stack = match below {
        Some(below) => {
            let [r, g, b, a, base, shows] = below[at];
            Stack {
                color: [unit(r), unit(g), unit(b), unit(a)],
                base_alpha: unit(base),
                base_visible: shows != 0,
            }
        }
        None => {
            let mut stack = Stack::EMPTY;
            fold(&layers[..index], x, y, &mut stack);
            stack
        }
    };
    fold(&layers[index..=index], x, y, &mut stack);
    match above {
        Some(above) => {
            let over = above[at].map(unit);
            sue_model::BlendMode::Normal.composite(stack.color, over)
        }
        None => {
            fold(&layers[index + 1..], x, y, &mut stack);
            stack.color
        }
    }
}

/// `color` (straight RGBA) as the screen shows it at `(x, y)`: over the
/// checkerboard, opaque.
fn shown(color: [f32; 4], x: u32, y: u32) -> [u8; 4] {
    let under = checker(x, y);
    let alpha = color[3];
    let mut out = [255; 4];
    for (slot, value) in out.iter_mut().zip(color).take(3) {
        let value = value * alpha + under * (1.0 - alpha);
        *slot = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
}

/// [`composite_span`] through a [`Focus`] (see [`focused_color`]).
#[allow(clippy::too_many_arguments)]
fn focused_span(
    layers: &[Layer<'_>],
    focus: &Focus,
    index: usize,
    flat: bool,
    y: u32,
    x0: u32,
    x1: u32,
    row: &mut [u8],
) {
    let ty = y / FOCUS_TILE;
    let mut x = x0;
    while x < x1 {
        let tx = x / FOCUS_TILE;
        let end = x1.min((tx + 1) * FOCUS_TILE);
        let below = focus.below.get(&(tx, ty)).map(AsRef::as_ref);
        let above = flat
            .then(|| focus.above.get(&(tx, ty)))
            .flatten()
            .map(AsRef::as_ref);
        for x in x..end {
            let color = focused_color(layers, below, above, index, x, y);
            row[x as usize * 4..x as usize * 4 + 4].copy_from_slice(&shown(color, x, y));
        }
        x = end;
    }
}

/// Composites pixels `x0..x1` of row `y` into `row` (the whole row of the
/// screen image), over the checkerboard.
fn composite_span(layers: &[Layer<'_>], y: u32, x0: u32, x1: u32, row: &mut [u8]) {
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
            let shown = value * alpha + under * (1.0 - alpha);
            *channel = (shown.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        slot[3] = 255;
    }
}

/// How many threads big jobs split into: the cores, or `SUE_THREADS` when
/// set (1 measures what the browser, which has no threads, goes through).
#[must_use]
pub fn workers() -> usize {
    static COUNT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *COUNT.get_or_init(|| {
        std::env::var("SUE_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| *count > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
            })
    })
}

/// Runs `work(y, row)` for each `stride`-byte row of `buffer` (row `y0`
/// first), spread over the cores when the area is large.
fn rows(
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

/// Composites the rectangle `[x0, y0, x1, y1]` of `picture` as straight
/// RGBA8, row by row.
#[must_use]
pub fn render(arena: &Arena, picture: EntityId, rect: [u32; 4], options: RenderOptions) -> Vec<u8> {
    render_view(arena, picture, rect, options, None)
}

/// As [`render`], for one animation frame's `view` of the layers.
#[must_use]
pub fn render_view(
    arena: &Arena,
    picture: EntityId,
    [x0, y0, x1, y1]: [u32; 4],
    options: RenderOptions,
    view: Option<&FrameView>,
) -> Vec<u8> {
    let mut layers = gather_view(arena, picture, options, view);
    prepare(&mut layers, [x0, y0, x1, y1]);
    let (width, height) = (
        x1.saturating_sub(x0) as usize,
        y1.saturating_sub(y0) as usize,
    );
    let mut out = vec![0_u8; width * height * 4];
    rows(&mut out, width * 4, y0, width * height, |y, row| {
        for (x, slot) in (x0..x1).zip(row.as_chunks_mut::<4>().0) {
            *slot = pixel(&layers, x, y).map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    });
    out
}

/// The whole picture flattened into one straight RGBA8 raster.
#[must_use]
pub fn flatten(arena: &Arena, picture: EntityId, options: RenderOptions) -> Option<RasterTile2D> {
    flatten_view(arena, picture, options, None)
}

/// As [`flatten`], for one animation frame's `view` of the layers.
#[must_use]
pub fn flatten_view(
    arena: &Arena,
    picture: EntityId,
    options: RenderOptions,
    view: Option<&FrameView>,
) -> Option<RasterTile2D> {
    let [width, height] = picture::size(arena.get_by_id(picture).ok()?)?;
    let pixels = render_view(arena, picture, [0, 0, width, height], options, view);
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()
}

/// The picture's and its layers' revisions, in stacking order: when this
/// changes, the picture looks different.
#[must_use]
pub fn signature(arena: &Arena, picture: EntityId) -> Vec<(EntityId, u64)> {
    let revision = |id| {
        arena
            .get_by_id(id)
            .map_or(u64::MAX, sue_core::Entity::revision)
    };
    let mut seen = vec![(picture, revision(picture))];
    for id in picture::layers(arena, picture) {
        seen.push((id, revision(id)));
        // A mask's edits change how its layer looks.
        if let Some(mask) = crate::doc::mask_of(arena, id) {
            seen.push((mask, revision(mask)));
        }
    }
    seen
}

/// The whole picture at most `max` pixels a side (sampled, not filtered),
/// over the checkerboard: opaque RGBA8 pixels and their size.
#[must_use]
pub fn preview(arena: &Arena, picture: EntityId, max: [u32; 2]) -> Option<(Vec<u8>, [u32; 2])> {
    let size = picture::size(arena.get_by_id(picture).ok()?)?;
    let scale = (max[0] as f32 / size[0] as f32)
        .min(max[1] as f32 / size[1] as f32)
        .min(1.0);
    let out = [
        ((size[0] as f32 * scale).round() as u32).max(1),
        ((size[1] as f32 * scale).round() as u32).max(1),
    ];
    let layers = gather(arena, picture, RenderOptions::default());
    let mut pixels = vec![0_u8; out[0] as usize * out[1] as usize * 4];
    rows(
        &mut pixels,
        out[0] as usize * 4,
        0,
        (out[0] * out[1]) as usize,
        |ty, row| {
            for (tx, slot) in (0..out[0]).zip(row.as_chunks_mut::<4>().0) {
                let x = (((tx as f32 + 0.5) / scale) as u32).min(size[0] - 1);
                let y = (((ty as f32 + 0.5) / scale) as u32).min(size[1] - 1);
                let color = pixel(&layers, x, y);
                let under = checker(tx, ty);
                for (channel, value) in slot.iter_mut().zip(color) {
                    let shown = value * color[3] + under * (1.0 - color[3]);
                    *channel = (shown.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                slot[3] = 255;
            }
        },
    );
    Some((pixels, out))
}

/// The gray of the transparency checkerboard at a pixel.
fn checker(x: u32, y: u32) -> f32 {
    if ((x / 8) + (y / 8)).is_multiple_of(2) {
        1.0
    } else {
        0.8
    }
}

/// Side of a [`Focus`] tile.
const FOCUS_TILE: u32 = 64;
/// The most tiles a [`Focus`] keeps (about 70 MB).
const FOCUS_TILES: usize = 2048;

/// While a stroke paints one layer, everything under it and over it stays
/// as it is: the stack under the layer is kept per tile, and so is the
/// stack over it, flattened, when its layers are plain Normal ones. A
/// pixel of the screen is then three layers' work, not every layer's, as
/// painting programs do with their layer caches.
#[derive(Debug)]
struct Focus {
    layer: EntityId,
    /// The picture's signature apart from the layer (and its mask): the
    /// cache holds while it does.
    others: Vec<(EntityId, u64)>,
    /// Per tile, each pixel's [`Stack`] under the layer: color and base
    /// alpha as bytes, and whether the base shows.
    below: std::collections::HashMap<(u32, u32), Box<[[u8; 6]]>>,
    /// Per tile, the stack over the layer flattened (straight RGBA), when
    /// that stack is plain.
    above: std::collections::HashMap<(u32, u32), Box<[[u8; 4]]>>,
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `true` when the layers over the focused one can be flattened ahead:
/// Normal blends only, no correction, no border, and the first of them
/// not clipped onto the focused layer.
fn plain_above(layers: &[Layer<'_>]) -> bool {
    layers.first().is_none_or(|layer| !layer.style.clip)
        && layers.iter().all(|layer| {
            !matches!(layer.source, Source::Correction(_))
                && layer.style.blend == sue_model::BlendMode::Normal
                && layer.effects.border.is_none()
        })
}

/// The screen image of one picture, kept in step with the arena.
#[derive(Debug)]
pub struct Compositor {
    picture: EntityId,
    size: [u32; 2],
    display: Vec<u8>,
    seen: Vec<(EntityId, u64)>,
    version: u64,
    view: Option<FrameView>,
    /// The rectangles repainted lately, by the version each made, so a
    /// texture can take just what changed (see [`Self::changed_since`]).
    changes: std::collections::VecDeque<(u64, Vec<[u32; 4]>)>,
    /// The newest version no longer on record: what changed up to it is
    /// unknown.
    forgotten: u64,
    /// The layer being painted and the stacks around it.
    focus: Option<Focus>,
    /// Each layer's [`Occupancy`], for the revision and offset it was
    /// found at.
    occupancy: std::collections::HashMap<EntityId, (u64, [i64; 2], std::sync::Arc<Occupancy>)>,
    /// The 64 pixel tiles shown coarse, or not redrawn at all because they
    /// were off screen, waiting for full detail (see [`Self::refine`]),
    /// by (column, row).
    pending: std::collections::BTreeSet<(u32, u32)>,
    /// The layer the waiting tiles were painted on.
    pending_layer: Option<EntityId>,
}

/// Side of a [`Compositor::refine`] tile.
const PENDING_TILE: u32 = 64;

/// `rect` cut to `clip`, or `None` when they do not meet.
fn meet(rect: [u32; 4], clip: [u32; 4]) -> Option<[u32; 4]> {
    let out = [
        rect[0].max(clip[0]),
        rect[1].max(clip[1]),
        rect[2].min(clip[2]),
        rect[3].min(clip[3]),
    ];
    (out[0] < out[2] && out[1] < out[3]).then_some(out)
}

/// How many picture pixels a side one sample covers while painting at a
/// view of `scale` screen pixels per picture pixel: 1 from half size up,
/// else the largest power of two the screen cannot tell apart (8 at most).
#[must_use]
pub fn view_step(scale: f32) -> u32 {
    let mut step = 1;
    while step < 8 && (step * 2) as f32 * scale <= 1.0 {
        step *= 2;
    }
    step
}

/// Versions come from one count for every compositor, so a texture made
/// from another one (a picture closed and opened again) never passes for
/// this one's.
fn next_version() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Compositor {
    /// A compositor for `picture`; it renders on the first [`Self::sync`].
    #[must_use]
    pub fn new(picture: EntityId) -> Self {
        let version = next_version();
        Self {
            picture,
            size: [0, 0],
            display: Vec::new(),
            seen: Vec::new(),
            version,
            view: None,
            changes: std::collections::VecDeque::new(),
            forgotten: version,
            focus: None,
            occupancy: std::collections::HashMap::new(),
            pending: std::collections::BTreeSet::new(),
            pending_layer: None,
        }
    }

    /// Shows the layers as animation frame `view` has them (`None`: all
    /// of them); a change is composited on the next [`Self::sync`].
    pub fn set_view(&mut self, view: Option<FrameView>) {
        if self.view != view {
            self.view = view;
            self.seen.clear();
            self.focus = None;
        }
    }

    /// The picture it shows.
    #[must_use]
    pub fn picture(&self) -> EntityId {
        self.picture
    }

    /// Opaque RGBA8 pixels for the screen (a checkerboard shows through
    /// transparent areas), their size and a version that changes with them.
    #[must_use]
    pub fn display(&self) -> (&[u8], [u32; 2], u64) {
        (&self.display, self.size, self.version)
    }

    /// The screen color at a picture pixel.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size[0] || y >= self.size[1] {
            return None;
        }
        let at = (y as usize * self.size[0] as usize + x as usize) * 4;
        self.display.get(at..at + 4)?.try_into().ok()
    }

    /// The [`Focus`] around `painted` in `layers` (with its index and
    /// whether the stack over it is flattened): the kept one while nothing
    /// else changed (`signature` is the picture's now), else a new one. The
    /// tiles in `wanted` it lacks are made.
    #[allow(clippy::type_complexity)]
    fn kept_focus(
        &mut self,
        arena: &Arena,
        layers: &[Layer<'_>],
        painted: Option<EntityId>,
        signature: &[(EntityId, u64)],
        wanted: &[(u32, u32)],
    ) -> Option<(Focus, usize, bool)> {
        let painted = painted?;
        let index = layers.iter().position(|layer| layer.id == Some(painted))?;
        // A border is drawn for a rectangle at a time: no cache.
        if layers.iter().any(|layer| layer.effects.border.is_some()) {
            return None;
        }
        let mask = crate::doc::mask_of(arena, painted);
        let others: Vec<(EntityId, u64)> = signature
            .iter()
            .filter(|(id, _)| *id != painted && Some(*id) != mask)
            .copied()
            .collect();
        let keep = self.focus.take().filter(|focus| {
            focus.layer == painted && focus.others == others && focus.below.len() <= FOCUS_TILES
        });
        let mut focus = keep.unwrap_or(Focus {
            layer: painted,
            others,
            below: std::collections::HashMap::new(),
            above: std::collections::HashMap::new(),
        });
        let flat = plain_above(&layers[index + 1..]);
        let missing: Vec<(u32, u32)> = wanted
            .iter()
            .copied()
            .filter(|tile| !focus.below.contains_key(tile))
            .collect();
        for (key, below, above) in focus_tiles(layers, index, flat, &missing, self.size) {
            focus.below.insert(key, below);
            if let Some(above) = above {
                focus.above.insert(key, above);
            }
        }
        Some((focus, index, flat))
    }

    /// Gives `layers` the occupancy kept for their current pixels. With
    /// `find`, layers without one get it worked out (on every core), as a
    /// full composite does; a stroke's refresh does not, so the layer being
    /// painted is read everywhere.
    fn attach(&mut self, arena: &Arena, layers: &mut [Layer<'_>], find: bool) {
        let size = self.size;
        let mut missing = Vec::new();
        for (index, layer) in layers.iter_mut().enumerate() {
            let (Some(id), Source::Raster { offset, .. }) = (layer.id, &layer.source) else {
                continue;
            };
            let Ok(entity) = arena.get_by_id(id) else {
                continue;
            };
            let revision = entity.revision();
            match self.occupancy.get(&id) {
                Some((seen, at, tiles)) if *seen == revision && at == offset => {
                    layer.occupancy = Some(tiles.clone());
                }
                _ if find => missing.push((index, id, revision, *offset)),
                _ => {}
            }
        }
        if missing.is_empty() {
            return;
        }
        let rasters: Vec<(&RasterTile2D, [i64; 2])> = missing
            .iter()
            .filter_map(|(index, _, _, offset)| match &layers[*index].source {
                Source::Raster { raster, .. } => Some((*raster, *offset)),
                _ => None,
            })
            .collect();
        let found: Vec<Option<Occupancy>> = if rasters.len() > 1 && workers() > 1 {
            std::thread::scope(|scope| {
                let jobs: Vec<_> = rasters
                    .iter()
                    .map(|(raster, offset)| {
                        scope.spawn(move || Occupancy::of(raster, *offset, size))
                    })
                    .collect();
                jobs.into_iter()
                    .map(|job| job.join().ok().flatten())
                    .collect()
            })
        } else {
            rasters
                .iter()
                .map(|(raster, offset)| Occupancy::of(raster, *offset, size))
                .collect()
        };
        for ((index, id, revision, offset), tiles) in missing.into_iter().zip(found) {
            if let Some(tiles) = tiles {
                let tiles = std::sync::Arc::new(tiles);
                layers[index].occupancy = Some(tiles.clone());
                self.occupancy.insert(id, (revision, offset, tiles));
            }
        }
        // Layers that are gone keep nothing.
        let present: HashSet<EntityId> = layers.iter().filter_map(|layer| layer.id).collect();
        self.occupancy.retain(|id, _| present.contains(id));
    }

    /// The rectangles `[x0, y0, x1, y1]` holding every pixel that changed
    /// since `version` (none when nothing did), or `None` when that is too
    /// long ago to know.
    #[must_use]
    pub fn changed_since(&self, version: u64) -> Option<Vec<[u32; 4]>> {
        if version == self.version {
            return Some(Vec::new());
        }
        // Every change after `version` must still be on record.
        if version < self.forgotten || version > self.version {
            return None;
        }
        let mut rects: Vec<[u32; 4]> = self
            .changes
            .iter()
            .filter(|(made, _)| *made > version)
            .flat_map(|(_, rects)| rects.iter().copied())
            .collect();
        // Many small pieces cost more to send one by one than together.
        if rects.len() > 256 {
            let all = rects
                .iter()
                .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                    [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
                });
            rects = vec![all];
        }
        Some(rects)
    }

    fn signature(&self, arena: &Arena) -> Vec<(EntityId, u64)> {
        signature(arena, self.picture)
    }

    /// Recomposites everything if the layers changed since the last look.
    /// Returns `true` if the image changed.
    pub fn sync(&mut self, arena: &Arena) -> bool {
        let Some(size) = arena.get_by_id(self.picture).ok().and_then(picture::size) else {
            return false;
        };
        let seen = self.signature(arena);
        if size == self.size && seen == self.seen {
            return false;
        }
        self.size = size;
        self.display = vec![0; size[0] as usize * size[1] as usize * 4];
        self.paint(arena, [0, 0, size[0], size[1]]);
        self.seen = seen;
        self.pending.clear();
        self.pending_layer = None;
        true
    }

    /// [`Self::refresh_rects`] for a stroke seen at some zoom: of `rects`
    /// only what lies in `visible` is composited now, one sample each
    /// `step` pixels a side (see [`view_step`]), as Krita's instant preview
    /// does for big brushes. The rest, and every coarse tile, waits for
    /// [`Self::refine`] to bring it to full detail.
    pub fn refresh_view(
        &mut self,
        arena: &Arena,
        rects: &[[u32; 4]],
        painted: Option<EntityId>,
        visible: [u32; 4],
        step: u32,
    ) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let step = step.max(1);
        let whole = [0, 0, self.size[0], self.size[1]];
        let mut now = Vec::new();
        for rect in rects {
            let Some(rect) = meet(*rect, whole) else {
                continue;
            };
            for ty in rect[1] / PENDING_TILE..=(rect[3] - 1) / PENDING_TILE {
                for tx in rect[0] / PENDING_TILE..=(rect[2] - 1) / PENDING_TILE {
                    let tile = [
                        tx * PENDING_TILE,
                        ty * PENDING_TILE,
                        (tx + 1) * PENDING_TILE,
                        (ty + 1) * PENDING_TILE,
                    ];
                    let Some(piece) = meet(rect, tile) else {
                        continue;
                    };
                    let seen = meet(piece, visible);
                    if step > 1 || seen != Some(piece) {
                        self.pending.insert((tx, ty));
                        self.pending_layer = painted;
                    }
                }
            }
            if let Some(seen) = meet(rect, visible) {
                now.push(seen);
            }
        }
        if step == 1 {
            if now.is_empty() {
                self.seen = self.signature(arena);
            } else {
                self.refresh_rects(arena, &now, painted);
            }
            return;
        }
        self.coarse(arena, &now, painted, step);
    }

    /// Composites `rects` one sample a `step` by `step` block, each block
    /// filled with its sample.
    fn coarse(&mut self, arena: &Arena, rects: &[[u32; 4]], painted: Option<EntityId>, step: u32) {
        let signature = self.signature(arena);
        if rects.is_empty() {
            self.seen = signature;
            return;
        }
        let bounds = rects
            .iter()
            .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
            });
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, bounds);
        self.attach(arena, &mut layers, false);
        let focus = self.kept_focus(arena, &layers, painted, &signature, &[]);
        // One sample per block, the blocks' rows shared out over the cores.
        let jobs: Vec<([u32; 4], u32)> = rects
            .iter()
            .flat_map(|rect| {
                let first = rect[1] / step * step;
                (first..rect[3])
                    .step_by(step as usize)
                    .map(move |gy| (*rect, gy))
            })
            .collect();
        let sample_row = |&(rect, gy): &([u32; 4], u32)| -> Vec<[u8; 4]> {
            let first = rect[0] / step * step;
            let y = (gy + step / 2).clamp(rect[1], rect[3] - 1);
            (first..rect[2])
                .step_by(step as usize)
                .map(|gx| {
                    let x = (gx + step / 2).clamp(rect[0], rect[2] - 1);
                    let color = match &focus {
                        Some((focus, index, flat)) => {
                            let tile = (x / FOCUS_TILE, y / FOCUS_TILE);
                            let below = focus.below.get(&tile).map(AsRef::as_ref);
                            let above = flat
                                .then(|| focus.above.get(&tile))
                                .flatten()
                                .map(AsRef::as_ref);
                            focused_color(&layers, below, above, *index, x, y)
                        }
                        None => pixel(&layers, x, y),
                    };
                    shown(color, x, y)
                })
                .collect()
        };
        let threads = workers().min(jobs.len()).max(1);
        let samples: Vec<Vec<[u8; 4]>> = if threads <= 1 {
            jobs.iter().map(sample_row).collect()
        } else {
            let per = jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let workers: Vec<_> = jobs
                    .chunks(per)
                    .map(|chunk| {
                        let sample_row = &sample_row;
                        scope.spawn(move || chunk.iter().map(sample_row).collect::<Vec<_>>())
                    })
                    .collect();
                workers
                    .into_iter()
                    .flat_map(|worker| worker.join().unwrap_or_default())
                    .collect()
            })
        };
        let stride = self.size[0] as usize * 4;
        for ((rect, gy), row) in jobs.iter().zip(samples) {
            let first = rect[0] / step * step;
            for y in (*gy).max(rect[1])..(gy + step).min(rect[3]) {
                let line = &mut self.display[y as usize * stride..(y as usize + 1) * stride];
                for (block, color) in row.iter().enumerate() {
                    let gx = first + block as u32 * step;
                    for x in gx.max(rect[0])..(gx + step).min(rect[2]) {
                        line[x as usize * 4..x as usize * 4 + 4].copy_from_slice(color);
                    }
                }
            }
        }
        self.focus = focus.map(|(focus, _, _)| focus);
        self.version = next_version();
        self.changes.push_back((self.version, rects.to_vec()));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
        self.seen = signature;
    }

    /// Brings up to `tiles` of the waiting tiles (the ones in `visible`
    /// first) to full detail. Returns `true` while some still wait, so the
    /// caller draws another frame.
    pub fn refine(&mut self, arena: &Arena, visible: [u32; 4], tiles: usize) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        let tile_rect = |(tx, ty): (u32, u32)| {
            [
                tx * PENDING_TILE,
                ty * PENDING_TILE,
                ((tx + 1) * PENDING_TILE).min(self.size[0]),
                ((ty + 1) * PENDING_TILE).min(self.size[1]),
            ]
        };
        let mut chosen: Vec<(u32, u32)> = self
            .pending
            .iter()
            .copied()
            .filter(|tile| meet(tile_rect(*tile), visible).is_some())
            .take(tiles)
            .collect();
        if chosen.len() < tiles {
            let more: Vec<(u32, u32)> = self
                .pending
                .iter()
                .copied()
                .filter(|tile| !chosen.contains(tile))
                .take(tiles - chosen.len())
                .collect();
            chosen.extend(more);
        }
        let rects: Vec<[u32; 4]> = chosen
            .iter()
            .map(|tile| tile_rect(*tile))
            .filter(|rect| rect[0] < rect[2] && rect[1] < rect[3])
            .collect();
        for tile in &chosen {
            self.pending.remove(tile);
        }
        let painted = self.pending_layer;
        if self.pending.is_empty() {
            self.pending_layer = None;
        }
        self.refresh_rects(arena, &rects, painted);
        !self.pending.is_empty()
    }

    /// Takes the picture as it is now as seen when the only change since
    /// the last look is to `painted` (a stroke's layer, written to
    /// although its pen moved too little to paint): no recomposite for
    /// that. Any other change is left for [`Self::sync`].
    pub fn accept(&mut self, arena: &Arena, painted: EntityId) {
        let signature = self.signature(arena);
        let mask = crate::doc::mask_of(arena, painted);
        let only = signature.len() == self.seen.len()
            && signature.iter().zip(&self.seen).all(|(now, seen)| {
                now == seen || (now.0 == seen.0 && (now.0 == painted || Some(now.0) == mask))
            });
        if only {
            self.seen = signature;
        }
    }

    /// `true` while some tiles wait for full detail.
    #[must_use]
    pub fn refining(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Recomposites the rectangle `[x0, y0, x1, y1]` after an edit made in
    /// place, and takes the edit as seen.
    pub fn refresh(&mut self, arena: &Arena, rect: [u32; 4]) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let rect = [
            rect[0].min(self.size[0]),
            rect[1].min(self.size[1]),
            rect[2].min(self.size[0]),
            rect[3].min(self.size[1]),
        ];
        self.paint(arena, rect);
        self.seen = self.signature(arena);
    }

    /// Recomposites the rectangles `[x0, y0, x1, y1]` after an edit made in
    /// place (the tiles a stroke changed), gathering the layers once, and
    /// takes the edit as seen. `painted` is the layer the edit changed:
    /// the stacks under and over it are kept for the next refresh (see
    /// [`Focus`]).
    pub fn refresh_rects(&mut self, arena: &Arena, rects: &[[u32; 4]], painted: Option<EntityId>) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let [width, height] = self.size;
        let rects: Vec<[u32; 4]> = rects
            .iter()
            .map(|r| {
                [
                    r[0].min(width),
                    r[1].min(height),
                    r[2].min(width),
                    r[3].min(height),
                ]
            })
            .filter(|r| r[0] < r[2] && r[1] < r[3])
            .collect();
        if rects.is_empty() {
            return;
        }
        let bounds = rects
            .iter()
            .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
            });
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, bounds);
        self.attach(arena, &mut layers, false);
        let stride = width as usize * 4;
        let signature = self.signature(arena);
        // The stacks of the tiles this refresh needs.
        let mut wanted = Vec::new();
        for [x0, y0, x1, y1] in &rects {
            for ty in y0 / FOCUS_TILE..=(y1 - 1) / FOCUS_TILE {
                for tx in x0 / FOCUS_TILE..=(x1 - 1) / FOCUS_TILE {
                    if !wanted.contains(&(tx, ty)) {
                        wanted.push((tx, ty));
                    }
                }
            }
        }
        let focus = self.kept_focus(arena, &layers, painted, &signature, &wanted);
        // Each row's spans, then the rows painted on every core.
        let mut spans: Vec<Vec<(u32, u32)>> = vec![Vec::new(); (bounds[3] - bounds[1]) as usize];
        for [x0, y0, x1, y1] in &rects {
            for y in *y0..*y1 {
                spans[(y - bounds[1]) as usize].push((*x0, *x1));
            }
        }
        let area: usize = rects
            .iter()
            .map(|[x0, y0, x1, y1]| ((x1 - x0) * (y1 - y0)) as usize)
            .sum();
        let band = &mut self.display[bounds[1] as usize * stride..bounds[3] as usize * stride];
        rows(band, stride, bounds[1], area, |y, row| {
            for &(x0, x1) in &spans[(y - bounds[1]) as usize] {
                match &focus {
                    Some((focus, index, flat)) => {
                        focused_span(&layers, focus, *index, *flat, y, x0, x1, row);
                    }
                    None => composite_span(&layers, y, x0, x1, row),
                }
            }
        });
        self.focus = focus.map(|(focus, _, _)| focus);
        self.version = next_version();
        self.changes.push_back((self.version, rects));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
        self.seen = signature;
    }

    fn paint(&mut self, arena: &Arena, [x0, y0, x1, y1]: [u32; 4]) {
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
        let stride = self.size[0] as usize * 4;
        let band = &mut self.display[y0 as usize * stride..y1 as usize * stride];
        let area = (x1.saturating_sub(x0) * y1.saturating_sub(y0)) as usize;
        rows(band, stride, y0, area, |y, row| {
            composite_span(&layers, y, x0, x1, row);
        });
        self.version = next_version();
        self.changes
            .push_back((self.version, vec![[x0, y0, x1, y1]]));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
    }
}

/// A small preview of one layer, `max` pixels at most a side, over the
/// checkerboard: RGBA8 pixels and their size.
#[must_use]
pub fn thumbnail(arena: &Arena, layer: EntityId, max: [u32; 2]) -> Option<(Vec<u8>, [u32; 2])> {
    let entity = arena.get_by_id(layer).ok()?;
    let picture_size = picture::picture_of(arena, layer)
        .and_then(|picture| arena.get_by_id(picture).ok())
        .and_then(picture::size);
    let (source, size) = match LayerKind::of(entity) {
        LayerKind::Raster => {
            let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
            let size = picture_size.unwrap_or([raster.width(), raster.height()]);
            (
                Source::Raster {
                    raster,
                    offset: LayerStyle::of(entity).offset,
                },
                size,
            )
        }
        LayerKind::Paper => (
            Source::Paper(
                picture::paper_color(entity)
                    .to_space(sue_core::ColorSpace::Srgb)
                    .rgb,
            ),
            picture_size.unwrap_or([4, 3]),
        ),
        LayerKind::Other => return None,
    };
    let layer = Layer {
        id: None,
        source,
        style: LayerStyle::default(),
        tint: None,
        mask: None,
        warp: None,
        effects: crate::effects::Effects::default(),
        edge: None,
        occupancy: None,
    };
    let scale = (max[0] as f32 / size[0].max(1) as f32).min(max[1] as f32 / size[1].max(1) as f32);
    let out = [
        ((size[0] as f32 * scale).round() as u32).max(1),
        ((size[1] as f32 * scale).round() as u32).max(1),
    ];
    let mut pixels = Vec::with_capacity(out[0] as usize * out[1] as usize * 4);
    for ty in 0..out[1] {
        for tx in 0..out[0] {
            let x = ((tx as f32 + 0.5) / scale) as u32;
            let y = ((ty as f32 + 0.5) / scale) as u32;
            let color = layer.sample(x.min(size[0] - 1), y.min(size[1] - 1));
            let under = if ((tx / 4) + (ty / 4)).is_multiple_of(2) {
                1.0
            } else {
                0.8
            };
            for value in &color[..3] {
                let shown = value * color[3] + under * (1.0 - color[3]);
                pixels.push((shown.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            pixels.push(255);
        }
    }
    Some((pixels, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{self, NewPicture};
    use sue_core::txn::SetProperty;
    use sue_core::{Color, ReversibleTransaction};

    fn picture() -> (Arena, EntityId, EntityId) {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Test".to_owned(),
            width: 8,
            height: 4,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let layer = doc::layers(&arena, picture)
            .last()
            .map(|info| info.id)
            .unwrap();
        (arena, picture, layer)
    }

    fn paint(arena: &mut Arena, layer: EntityId, x: u32, y: u32, rgba: [u8; 4]) {
        let raster = doc::raster_mut(arena, layer).unwrap();
        let at = (y * raster.width() + x) as usize * 4;
        raster.bytes_mut().unwrap()[at..at + 4].copy_from_slice(&rgba);
    }

    #[test]
    fn layers_stack_over_the_paper() {
        let (mut arena, picture, layer) = picture();
        paint(&mut arena, layer, 1, 1, [0, 0, 0, 128]);
        let all = render(&arena, picture, [0, 0, 8, 4], RenderOptions::default());
        assert_eq!(&all[..4], &[255, 255, 255, 255], "paper");
        let at = (8 + 1) * 4;
        assert_eq!(
            &all[at..at + 4],
            &[127, 127, 127, 255],
            "half black on white"
        );
        let clear = render(
            &arena,
            picture,
            [0, 0, 8, 4],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        );
        assert_eq!(&clear[..4], &[0, 0, 0, 0]);
        assert_eq!(&clear[at..at + 4], &[0, 0, 0, 128]);
        // Multiply with white leaves the paper as it was.
        let mut mode = SetProperty::new(layer, picture::BLEND, "multiply");
        mode.apply(&mut arena).unwrap();
        paint(&mut arena, layer, 2, 2, [255, 255, 255, 255]);
        let multiplied = render(&arena, picture, [2, 2, 3, 3], RenderOptions::default());
        assert_eq!(multiplied, [255, 255, 255, 255]);
        let flat = flatten(&arena, picture, RenderOptions::default()).unwrap();
        assert_eq!((flat.width(), flat.height()), (8, 4));
    }

    #[test]
    fn clipped_layers_show_only_over_the_layer_below() {
        let (mut arena, picture, base) = picture();
        let (mut spawn, top) =
            doc::add_raster_layer(&arena, picture, Some(base), "Shade".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        let mut clip = SetProperty::new(top, picture::CLIP, true);
        clip.apply(&mut arena).unwrap();
        paint(&mut arena, base, 0, 0, [0, 0, 255, 255]);
        paint(&mut arena, top, 0, 0, [255, 0, 0, 255]);
        paint(&mut arena, top, 1, 0, [255, 0, 0, 255]);
        let row = render(
            &arena,
            picture,
            [0, 0, 2, 1],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        );
        assert_eq!(&row[..4], &[255, 0, 0, 255], "red over the blue base");
        assert_eq!(&row[4..], &[0, 0, 0, 0], "nothing where the base is clear");
    }

    #[test]
    fn the_fast_path_composites_exactly_as_the_full_one() {
        let (mut arena, picture, base) = picture();
        let (mut spawn, top) =
            doc::add_raster_layer(&arena, picture, Some(base), "Top".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        let (mut spawn, clipped) =
            doc::add_raster_layer(&arena, picture, Some(top), "Clipped".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        SetProperty::new(clipped, picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        SetProperty::new(top, picture::OPACITY, 0.6)
            .apply(&mut arena)
            .unwrap();
        for (i, layer) in [base, top, clipped].into_iter().enumerate() {
            for x in 0..8 {
                for y in 0..4 {
                    if !(x + y + i as u32).is_multiple_of(3) {
                        let v = (x * 31 + y * 17 + i as u32 * 50) as u8;
                        paint(&mut arena, layer, x, y, [v, 255 - v, 90, 60 + v / 2]);
                    }
                }
            }
        }
        let layers = gather(&arena, picture, RenderOptions::default());
        assert!(layers.iter().all(Layer::plain));
        for y in 0..4 {
            let rows: Vec<RowSource<'_>> = layers.iter().map(|layer| layer.row(y)).collect();
            for x in 0..8 {
                assert_eq!(
                    pixel_plain(&layers, &rows, x, y),
                    pixel(&layers, x, y),
                    "{x},{y}"
                );
            }
        }
    }

    #[test]
    fn painting_through_the_layer_cache_shows_what_a_full_composite_does() {
        let (mut arena, picture, base) = picture();
        let mut below = Some(base);
        let mut ids = vec![base];
        for name in ["Middle", "Top", "Clipped"] {
            let (mut spawn, id) =
                doc::add_raster_layer(&arena, picture, below, name.to_owned(), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            ids.push(id);
            below = Some(id);
        }
        SetProperty::new(ids[3], picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        SetProperty::new(ids[2], picture::OPACITY, 0.7)
            .apply(&mut arena)
            .unwrap();
        for (i, layer) in ids.iter().enumerate() {
            for x in 0..8 {
                for y in 0..4 {
                    if !(x * 3 + y + i as u32).is_multiple_of(4) {
                        let v = (x * 29 + y * 13 + i as u32 * 61) as u8;
                        paint(&mut arena, *layer, x, y, [v, 200 - v / 2, 70, 90 + v / 3]);
                    }
                }
            }
        }
        // What the screen should show: the full composite over the
        // checkerboard, give or take a step for the kept bytes.
        let check = |compositor: &Compositor, arena: &Arena| {
            let full = render(arena, picture, [0, 0, 8, 4], RenderOptions::default());
            for y in 0..4 {
                for x in 0..8 {
                    let at = ((y * 8 + x) * 4) as usize;
                    let want = &full[at..at + 4];
                    let alpha = f32::from(want[3]) / 255.0;
                    let under = checker(x, y);
                    let got = compositor.pixel(x, y).unwrap();
                    for channel in 0..3 {
                        let shown = (f32::from(want[channel]) / 255.0 * alpha
                            + under * (1.0 - alpha))
                            * 255.0;
                        let diff = (f32::from(got[channel]) - shown.round()).abs();
                        assert!(
                            diff <= 2.0,
                            "{x},{y} channel {channel}: {got:?} vs {want:?}"
                        );
                    }
                }
            }
        };
        // Painting the middle layer (over it a clipped pair: not plain),
        // the top one (plain over it) and the base.
        for painted in [ids[1], ids[2], ids[0]] {
            let mut compositor = Compositor::new(picture);
            compositor.sync(&arena);
            for round in 0..3 {
                paint(&mut arena, painted, round * 2, 1, [10, 250, 10, 255]);
                compositor.refresh_rects(&arena, &[[0, 0, 8, 4]], Some(painted));
                assert!(compositor.focus.is_some(), "the stacks are kept");
                check(&compositor, &arena);
            }
            // Anything else changing drops the kept stacks, so the screen
            // follows it.
            let other = if painted == ids[0] { ids[1] } else { ids[0] };
            paint(&mut arena, other, 7, 3, [0, 0, 0, 255]);
            compositor.refresh_rects(&arena, &[[0, 0, 8, 4]], Some(painted));
            check(&compositor, &arena);
        }
    }

    #[test]
    fn empty_tiles_are_passed_over_exactly_as_clear_pixels() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Tiles".to_owned(),
            width: 200,
            height: 150,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let base = doc::layers(&arena, picture).last().unwrap().id;
        let mut ids = vec![base];
        let mut below = Some(base);
        for name in ["Empty base", "Clipped", "Corner"] {
            let (mut spawn, id) =
                doc::add_raster_layer(&arena, picture, below, name.to_owned(), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            ids.push(id);
            below = Some(id);
        }
        // A clipped layer over a base that is empty in most tiles: there it
        // must show nothing.
        SetProperty::new(ids[2], picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        for x in 0..200 {
            for y in 0..150 {
                paint(&mut arena, ids[2], x, y, [200, 30, 30, 255]);
                if (x + y) % 5 == 0 {
                    paint(&mut arena, base, x, y, [10, 10, 200, 120]);
                }
            }
        }
        for x in 70..90 {
            for y in 70..80 {
                paint(&mut arena, ids[1], x, y, [0, 200, 0, 255]);
            }
        }
        for x in 150..200 {
            for y in 0..20 {
                paint(&mut arena, ids[3], x, y, [0, 0, 0, 90]);
            }
        }
        let mut compositor = Compositor::new(picture);
        compositor.sync(&arena);
        assert!(
            compositor.occupancy.len() >= 4,
            "the layers' tiles are known"
        );
        let full = render(&arena, picture, [0, 0, 200, 150], RenderOptions::default());
        for y in 0..150 {
            for x in 0..200 {
                let at = ((y * 200 + x) * 4) as usize;
                let want = &full[at..at + 4];
                let alpha = f32::from(want[3]) / 255.0;
                let under = checker(x, y);
                let shown: Vec<u8> = (0..3)
                    .map(|c| {
                        ((f32::from(want[c]) / 255.0 * alpha + under * (1.0 - alpha))
                            .clamp(0.0, 1.0)
                            * 255.0)
                            .round() as u8
                    })
                    .collect();
                assert_eq!(&compositor.pixel(x, y).unwrap()[..3], &shown[..], "{x},{y}");
            }
        }
    }

    #[test]
    fn coarse_and_off_screen_pieces_come_to_full_detail() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "View".to_owned(),
            width: 300,
            height: 200,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let layer = doc::layers(&arena, picture).last().unwrap().id;
        let mut compositor = Compositor::new(picture);
        compositor.sync(&arena);
        for x in 10..290 {
            for y in 20..180 {
                if (x / 3 + y / 7) % 2 == 0 {
                    paint(
                        &mut arena,
                        layer,
                        x,
                        y,
                        [(x % 256) as u8, (y % 256) as u8, 90, 200],
                    );
                }
            }
        }
        // Zoomed out to a quarter, with only the left half on screen.
        assert_eq!(view_step(0.25), 4);
        assert_eq!(view_step(0.6), 1);
        assert_eq!(view_step(0.01), 8);
        compositor.refresh_view(
            &arena,
            &[[0, 0, 300, 200]],
            Some(layer),
            [0, 0, 150, 200],
            4,
        );
        assert!(compositor.refining());
        // Off screen: not drawn yet.
        assert_eq!(compositor.pixel(250, 100), Some([255, 255, 255, 255]));
        let mut rounds = 0;
        while compositor.refine(&arena, [0, 0, 150, 200], 5) {
            rounds += 1;
            assert!(rounds < 100);
        }
        assert!(!compositor.refining());
        let full = render(&arena, picture, [0, 0, 300, 200], RenderOptions::default());
        for y in 0..200 {
            for x in 0..300 {
                let at = ((y * 300 + x) * 4) as usize;
                let want = &full[at..at + 4];
                let color = [0, 1, 2, 3].map(|c| f32::from(want[c]) / 255.0);
                let got = compositor.pixel(x, y).unwrap();
                let expect = shown(color, x, y);
                for c in 0..3 {
                    assert!(
                        got[c].abs_diff(expect[c]) <= 2,
                        "{x},{y}: {got:?} vs {expect:?}"
                    );
                }
            }
        }
        // Writing to the painted layer without changing it is no change;
        // anything else still is.
        let before = compositor.display().2;
        let _ = doc::raster_mut(&mut arena, layer);
        compositor.accept(&arena, layer);
        assert!(!compositor.sync(&arena), "nothing to recomposite");
        assert_eq!(compositor.display().2, before);
        let (mut spawn, _) =
            doc::add_raster_layer(&arena, picture, Some(layer), "New".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        compositor.accept(&arena, layer);
        assert!(compositor.sync(&arena), "a new layer is a change");
        // A full composite drops whatever still waited.
        compositor.refresh_view(&arena, &[[0, 0, 300, 200]], Some(layer), [0, 0, 10, 10], 8);
        paint(&mut arena, layer, 0, 0, [0, 0, 0, 255]);
        compositor.sync(&arena);
        assert!(!compositor.refining());
    }

    #[test]
    fn the_compositor_follows_the_arena() {
        let (mut arena, picture, layer) = picture();
        let mut compositor = Compositor::new(picture);
        assert!(compositor.sync(&arena));
        assert!(!compositor.sync(&arena), "nothing changed");
        let (_, size, first) = compositor.display();
        assert_eq!(size, [8, 4]);
        paint(&mut arena, layer, 3, 2, [0, 0, 0, 255]);
        compositor.refresh(&arena, [3, 2, 4, 3]);
        assert_eq!(compositor.pixel(3, 2), Some([0, 0, 0, 255]));
        assert!(!compositor.sync(&arena), "the refresh saw the edit");
        // A texture at the first version takes just the refreshed pixel.
        assert_eq!(compositor.changed_since(first), Some(vec![[3, 2, 4, 3]]));
        assert_eq!(
            compositor.changed_since(compositor.display().2),
            Some(Vec::new())
        );
        compositor.refresh(&arena, [0, 0, 1, 1]);
        assert_eq!(
            compositor.changed_since(first),
            Some(vec![[3, 2, 4, 3], [0, 0, 1, 1]])
        );
        // Several pieces at once: each is composited, and only they.
        paint(&mut arena, layer, 6, 0, [0, 0, 0, 255]);
        paint(&mut arena, layer, 1, 3, [0, 0, 0, 255]);
        let before = compositor.display().2;
        compositor.refresh_rects(&arena, &[[6, 0, 7, 1], [1, 3, 2, 4]], None);
        assert_eq!(compositor.pixel(6, 0), Some([0, 0, 0, 255]));
        assert_eq!(compositor.pixel(1, 3), Some([0, 0, 0, 255]));
        assert_eq!(
            compositor.changed_since(before),
            Some(vec![[6, 0, 7, 1], [1, 3, 2, 4]])
        );
        for _ in 0..70 {
            compositor.refresh(&arena, [0, 0, 1, 1]);
        }
        assert_eq!(compositor.changed_since(first), None, "too long ago");
        // Another compositor's versions never pass for this one's.
        let other = Compositor::new(picture);
        assert_eq!(compositor.changed_since(other.display().2), None);
        assert_eq!(other.changed_since(compositor.display().2), None);
        let mut hide = SetProperty::new(layer, picture::VISIBLE, false);
        hide.apply(&mut arena).unwrap();
        assert!(compositor.sync(&arena), "a hidden layer recomposites");
        assert_eq!(compositor.pixel(3, 2), Some([255, 255, 255, 255]));
        assert!(compositor.display().2 > first);
        let (thumb, thumb_size) = thumbnail(&arena, layer, [16, 16]).unwrap();
        assert_eq!(thumb_size, [16, 8]);
        assert_eq!(thumb.len(), 16 * 8 * 4);
        let (small, small_size) = preview(&arena, picture, [4, 4]).unwrap();
        assert_eq!(small_size, [4, 2]);
        assert_eq!(&small[..4], &[255, 255, 255, 255], "the paper, opaque");
        let before = signature(&arena, picture);
        paint(&mut arena, layer, 0, 0, [1, 1, 1, 255]);
        assert_ne!(signature(&arena, picture), before);
    }
}

/// Timings of compositing big areas, for tuning (`cargo test --release -p
/// sue-paint composite_speed -- --ignored --nocapture`).
#[cfg(test)]
mod speed {
    use super::*;
    use crate::doc::{self, NewPicture};
    use std::time::Instant;
    use sue_core::{Color, ReversibleTransaction};

    #[test]
    #[ignore = "a benchmark"]
    fn composite_speed() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Big".to_owned(),
            width: 4000,
            height: 3000,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let mut below = doc::layers(&arena, picture).last().map(|info| info.id);
        for i in 0..6 {
            let (mut spawn, layer) =
                doc::add_raster_layer(&arena, picture, below, format!("L{i}"), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            if let Some(bytes) = doc::raster_mut(&mut arena, layer).and_then(|r| r.bytes_mut()) {
                for (n, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    if (n / 4000 + n % 4000 + i * 300) % 900 < 400 {
                        pixel.copy_from_slice(&[(i * 40) as u8, 90, 200, 180]);
                    }
                }
            }
            below = Some(layer);
        }
        let mut compositor = Compositor::new(picture);
        let start = Instant::now();
        compositor.sync(&arena);
        println!(
            "full composite 4000x3000, 8 layers: {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        // Frames of a big stroke on the top layer: the brush, then the
        // screen, the old way (the square around what changed) and the new
        // (just the changed tiles).
        let top = below.unwrap();
        for (soft, size) in [(false, 1500.0), (true, 1500.0), (false, 300.0)] {
            for tiles in [false, true] {
                let tip = crate::brush::Tip {
                    size,
                    hardness: if soft { 0.3 } else { 1.0 },
                    eraser: soft,
                    size_by: crate::brush::Dynamics::NONE,
                    ..crate::brush::Tip::default()
                };
                let mut stroke = crate::brush::Stroke::new(top, tip, [0.1, 0.1, 0.1], false);
                let frames = 60;
                let start = Instant::now();
                for frame in 0..frames {
                    let first = Instant::now();
                    for step in 0..2 {
                        let i = (frame * 2 + step) as f32;
                        let raster = doc::raster_mut(&mut arena, top).unwrap();
                        stroke.to(raster, [900.0 + i * 3.0, 1500.0], 1.0, None);
                    }
                    let painted = first.elapsed();
                    if tiles {
                        let rects = stroke.take_dirty_tiles();
                        let listed = first.elapsed();
                        compositor.refresh_rects(&arena, &rects, Some(top));
                        if frame == 0 {
                            println!(
                                "  first frame: brush {:.1} ms, tiles listed {:.1} ms, screen {:.1} ms ({} rects)",
                                painted.as_secs_f64() * 1000.0,
                                (listed - painted).as_secs_f64() * 1000.0,
                                (first.elapsed() - listed).as_secs_f64() * 1000.0,
                                rects.len()
                            );
                        }
                    } else if let Some(rect) = stroke.take_dirty() {
                        compositor.refresh(&arena, rect);
                    }
                }
                println!(
                    "{} {size} stroke frame, {}: {:.2} ms",
                    if soft { "soft eraser" } else { "hard pen" },
                    if tiles {
                        "changed tiles"
                    } else {
                        "whole square"
                    },
                    start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
                );
            }
        }
        for side in [100_u32, 500, 1500] {
            let start = Instant::now();
            let runs = 10;
            for run in 0..runs {
                let x = 500 + run * 7;
                compositor.refresh(&arena, [x, 500, x + side, 500 + side]);
            }
            println!(
                "refresh {side}x{side}: {:.2} ms",
                start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
            );
        }
    }
}
