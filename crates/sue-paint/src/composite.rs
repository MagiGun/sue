//! Compositing: a picture's layers, read from the arena, laid over each
//! other with the shared blend modes ([`sue_model::BlendMode`]).
//!
//! [`render`] composites any rectangle (exports, merges, the eyedropper).
//! A [`Compositor`] keeps the screen image of one picture: it recomposites
//! everything when the layers changed behind its back (undo, a property, a
//! new layer) and only the painted rectangle while a stroke is drawn.

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

enum Source<'a> {
    Paper([f32; 3]),
    Raster {
        raster: &'a RasterTile2D,
        offset: [i64; 2],
    },
}

struct Layer<'a> {
    source: Source<'a>,
    style: LayerStyle,
}

impl Layer<'_> {
    /// The layer's straight RGBA at picture pixel `(x, y)`.
    fn sample(&self, x: u32, y: u32) -> [f32; 4] {
        match &self.source {
            Source::Paper(rgb) => [rgb[0], rgb[1], rgb[2], 1.0],
            Source::Raster { raster, offset } => {
                let lx = i64::from(x) - offset[0];
                let ly = i64::from(y) - offset[1];
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
    picture::layers(arena, picture)
        .into_iter()
        .filter_map(|id| arena.get_by_id(id).ok())
        .filter_map(|entity| {
            let mut style = LayerStyle::of(entity);
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
                LayerKind::Other => return None,
            };
            Some(Layer { source, style })
        })
        .collect()
}

/// The composited color of one pixel.
fn pixel(layers: &[Layer<'_>], x: u32, y: u32) -> [f32; 4] {
    let mut color = [0.0; 4];
    let mut base_alpha = 1.0;
    let mut base_visible = true;
    for layer in layers {
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

/// Composites the rectangle `[x0, y0, x1, y1]` of `picture` as straight
/// RGBA8, row by row.
#[must_use]
pub fn render(
    arena: &Arena,
    picture: EntityId,
    [x0, y0, x1, y1]: [u32; 4],
    options: RenderOptions,
) -> Vec<u8> {
    let layers = gather(arena, picture, options);
    let mut out = Vec::with_capacity((x1.saturating_sub(x0) * y1.saturating_sub(y0)) as usize * 4);
    for y in y0..y1 {
        for x in x0..x1 {
            let color = pixel(&layers, x, y);
            out.extend(color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

/// The whole picture flattened into one straight RGBA8 raster.
#[must_use]
pub fn flatten(arena: &Arena, picture: EntityId, options: RenderOptions) -> Option<RasterTile2D> {
    let [width, height] = picture::size(arena.get_by_id(picture).ok()?)?;
    let pixels = render(arena, picture, [0, 0, width, height], options);
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()
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
        let mut seen = vec![(
            self.picture,
            arena
                .get_by_id(self.picture)
                .map_or(u64::MAX, sue_core::Entity::revision),
        )];
        seen.extend(picture::layers(arena, self.picture).into_iter().map(|id| {
            (
                id,
                arena
                    .get_by_id(id)
                    .map_or(u64::MAX, sue_core::Entity::revision),
            )
        }));
        seen
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
        let layers = gather(arena, self.picture, RenderOptions::default());
        let width = self.size[0] as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let color = pixel(&layers, x, y);
                let under = checker(x, y);
                let alpha = color[3];
                let at = (y as usize * width + x as usize) * 4;
                for (slot, value) in self.display[at..at + 3].iter_mut().zip(color) {
                    let shown = value * alpha + under * (1.0 - alpha);
                    *slot = (shown.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                self.display[at + 3] = 255;
            }
        }
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
    }
}
