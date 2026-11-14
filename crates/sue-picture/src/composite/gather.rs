//! Gathering a picture's layers for compositing: what each one shows
//! (pixels, paper or a correction) and how (its style, mask, camera and
//! effects), and which tiles of the picture it has anything in.

use std::collections::HashSet;

use sue_core::{Arena, ComponentKind, EntityId, PixelFormat, RasterTile2D};
use sue_model::picture::{self, LayerKind, LayerStyle};

use super::{Compositor, FrameView, LiveStroke, RenderOptions, split};

/// A correction layer's change, ready for every pixel: a table a channel
/// for corrections that treat channels apart, else the filter itself.
pub(super) enum Correction {
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
    pub(super) fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
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

pub(super) enum Source<'a> {
    Paper([f32; 3]),
    /// A correction layer: it changes the picture below it.
    Correction(Correction),
    Raster {
        raster: &'a RasterTile2D,
        offset: [i64; 2],
    },
}

pub(super) struct Layer<'a> {
    /// The layer's entity (none for a thumbnail's stand-in).
    pub(super) id: Option<EntityId>,
    pub(super) source: Source<'a>,
    pub(super) style: LayerStyle,
    /// Drawn toward this color by this much (onion skin), keeping its
    /// alpha.
    pub(super) tint: Option<([f32; 3], f32)>,
    /// The layer mask, the layer's size: its alpha is how much shows.
    pub(super) mask: Option<&'a RasterTile2D>,
    /// Seen through a camera: where each picture pixel looks on the layer.
    pub(super) warp: Option<[f32; 6]>,
    /// Its Layer Property effects.
    pub(super) effects: crate::effects::Effects,
    /// Its border's cover over the rectangle being drawn.
    pub(super) edge: Option<crate::effects::Edge>,
    /// Its watercolor edge over the rectangle: how near the paint's edge.
    pub(super) rim: Option<crate::effects::Edge>,
    /// Its extracted lines over the rectangle.
    pub(super) lines: Option<crate::effects::Lines>,
    /// Which tiles of the picture it has pixels in, when known for its
    /// current pixels.
    pub(super) occupancy: Option<std::sync::Arc<Occupancy>>,
    /// The stroke being painted on it, not in its pixels yet.
    pub(super) live: Option<std::sync::Arc<dyn LiveStroke>>,
}

impl<'a> Layer<'a> {
    /// A layer showing `source` as it is, with `style`: no mask, camera,
    /// tint or effect.
    pub(super) fn new(source: Source<'a>, style: LayerStyle) -> Self {
        Self {
            id: None,
            source,
            style,
            tint: None,
            mask: None,
            warp: None,
            effects: crate::effects::Effects::default(),
            edge: None,
            rim: None,
            lines: None,
            occupancy: None,
            live: None,
        }
    }
}

/// Side of an [`Occupancy`] tile.
const OCCUPANCY_TILE: u32 = 64;

/// Which 64 pixel tiles of the picture a layer has any pixel in (alpha
/// above 0), so compositing passes over its empty ones with a bit test.
#[derive(Debug)]
pub(super) struct Occupancy {
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

/// Works out the effects of `layers` that look at neighbors (border,
/// watercolor edge, extracted lines) over `rect`.
pub(super) fn prepare(layers: &mut [Layer<'_>], rect: [u32; 4]) {
    for layer in layers.iter_mut() {
        if !layer.style.visible {
            continue;
        }
        let alpha = |layer: &Layer<'_>, x: i64, y: i64| {
            if x < 0 || y < 0 {
                0.0
            } else {
                layer.sample(x as u32, y as u32)[3]
            }
        };
        if let Some((width, _)) = layer.effects.watercolor {
            // The watercolor edge: how near each pixel is to where the
            // paint stops, from inside.
            let rim = crate::effects::Edge::fading(|x, y| 1.0 - alpha(layer, x, y), rect, width);
            layer.rim = Some(rim);
        }
        if let Some((threshold, width)) = layer.effects.extract {
            let lines = crate::effects::Lines::build(
                |x, y| {
                    if x < 0 || y < 0 {
                        [0.0; 4]
                    } else {
                        layer.sample(x as u32, y as u32)
                    }
                },
                rect,
                threshold,
                width,
            );
            layer.lines = Some(lines);
        }
        let Some((width, _)) = layer.effects.border else {
            continue;
        };
        if width <= 0.0 {
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
    pub(super) fn empty_at(&self, x: u32, y: u32) -> bool {
        matches!(self.source, Source::Raster { .. })
            && self.live.is_none()
            && self.warp.is_none()
            && self.effects.is_plain()
            && self
                .occupancy
                .as_ref()
                .is_some_and(|tiles| !tiles.has(x, y))
    }

    /// The layer's straight RGBA at picture pixel `(x, y)`, through its
    /// camera when it has one.
    pub(super) fn sample(&self, x: u32, y: u32) -> [f32; 4] {
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
    pub(super) fn seen(&self, x: u32, y: u32) -> Option<(u32, u32)> {
        let Some(m) = self.warp else {
            return Some((x, y));
        };
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let (wx, wy) = (m[0] * px + m[1] * py + m[4], m[2] * px + m[3] * py + m[5]);
        (wx >= 0.0 && wy >= 0.0).then_some((wx as u32, wy as u32))
    }

    /// The layer's straight RGBA at picture pixel `(x, y)`, as it is, with
    /// the stroke being painted on it.
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let own = self.own_texel(x, y);
        match (&self.live, &self.source) {
            (Some(live), Source::Raster { offset, .. }) => {
                live.over(x - offset[0], y - offset[1], own)
            }
            _ => own,
        }
    }

    /// The layer's straight RGBA at picture pixel `(x, y)`, from its own
    /// pixels.
    fn own_texel(&self, x: i64, y: i64) -> [f32; 4] {
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

pub(super) fn gather(arena: &Arena, picture: EntityId, options: RenderOptions) -> Vec<Layer<'_>> {
    gather_view(arena, picture, options, None)
}

pub(super) fn gather_view<'a>(
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
            let mut placed = None;
            if let Some(view) = view
                && view.hidden.contains(&entity.id())
            {
                match view.onion.iter().find(|skin| skin.cel == entity.id()) {
                    Some(skin) => {
                        style.opacity *= skin.opacity;
                        tint = Some((skin.color, skin.tint));
                        placed = skin.place;
                        if skin.force {
                            style.visible = true;
                        }
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
            let warp = placed.or_else(|| {
                let view = view?;
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
                tint,
                mask,
                warp,
                effects,
                ..Layer::new(source, style)
            })
        })
        .collect()
}

/// How much of a masked layer shows at picture pixel `(x, y)`.
pub(super) fn mask_at(mask: &RasterTile2D, offset: [i64; 2], x: u32, y: u32) -> f32 {
    let (lx, ly) = (i64::from(x) - offset[0], i64::from(y) - offset[1]);
    if lx < 0 || ly < 0 || lx >= i64::from(mask.width()) || ly >= i64::from(mask.height()) {
        return 0.0;
    }
    mask.bytes().map_or(1.0, |bytes| {
        f32::from(bytes[((ly as u32 * mask.width() + lx as u32) * 4 + 3) as usize]) / 255.0
    })
}

impl Compositor {
    /// Gives `layers` the occupancy kept for their current pixels. With
    /// `find`, layers without one get it worked out (on every core), as a
    /// full composite does; a stroke's refresh does not, so the layer being
    /// painted is read everywhere.
    pub(super) fn attach(&mut self, arena: &Arena, layers: &mut [Layer<'_>], find: bool) {
        if let Some(live) = &self.live {
            for layer in layers.iter_mut() {
                if layer.id == Some(live.layer()) {
                    layer.live = Some(live.clone());
                }
            }
        }
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
        let found: Vec<Option<Occupancy>> = split(&rasters, |(raster, offset)| {
            Occupancy::of(raster, *offset, size)
        });
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
}
