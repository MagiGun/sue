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
                source,
                style,
                tint,
                mask,
                warp,
                effects,
                edge: None,
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
    let mut color = [0.0; 4];
    let mut base_alpha = 1.0;
    let mut base_visible = true;
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
    color
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
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .clamp(1, 32)
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

/// The screen image of one picture, kept in step with the arena.
#[derive(Debug)]
pub struct Compositor {
    picture: EntityId,
    size: [u32; 2],
    display: Vec<u8>,
    seen: Vec<(EntityId, u64)>,
    version: u64,
    view: Option<FrameView>,
}

impl Compositor {
    /// A compositor for `picture`; it renders on the first [`Self::sync`].
    #[must_use]
    pub fn new(picture: EntityId) -> Self {
        Self {
            picture,
            size: [0, 0],
            display: Vec::new(),
            seen: Vec::new(),
            version: 0,
            view: None,
        }
    }

    /// Shows the layers as animation frame `view` has them (`None`: all
    /// of them); a change is composited on the next [`Self::sync`].
    pub fn set_view(&mut self, view: Option<FrameView>) {
        if self.view != view {
            self.view = view;
            self.seen.clear();
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
        true
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

    fn paint(&mut self, arena: &Arena, [x0, y0, x1, y1]: [u32; 4]) {
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, [x0, y0, x1, y1]);
        let stride = self.size[0] as usize * 4;
        let band = &mut self.display[y0 as usize * stride..y1 as usize * stride];
        let area = (x1.saturating_sub(x0) * y1.saturating_sub(y0)) as usize;
        rows(band, stride, y0, area, |y, row| {
            let span = &mut row[x0 as usize * 4..x1 as usize * 4];
            for (x, slot) in (x0..x1).zip(span.as_chunks_mut::<4>().0) {
                let color = pixel(&layers, x, y);
                let under = checker(x, y);
                let alpha = color[3];
                for (channel, value) in slot.iter_mut().zip(color) {
                    let shown = value * alpha + under * (1.0 - alpha);
                    *channel = (shown.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                slot[3] = 255;
            }
        });
        self.version += 1;
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
        source,
        style: LayerStyle::default(),
        tint: None,
        mask: None,
        warp: None,
        effects: crate::effects::Effects::default(),
        edge: None,
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
