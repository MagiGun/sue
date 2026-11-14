//! Layered picture files in the [`FormatRegistry`](sue_core::FormatRegistry):
//! Clip Studio Paint (`.clip`) and Photoshop (`.psd`, `.psb`).
//!
//! Importing gives a layered picture (`sue-model`'s picture: a
//! `sue.image` with its layers as ordered children) that every studio can
//! show, with the original file kept beside it as [`Encoded`]. A picture
//! keeps no folders, so each layer takes on what the folders around it do
//! to it (hidden with them, their opacity multiplied in); a Photoshop
//! layer mask that is on is laid into the layer's alpha. Exporting a
//! picture whose layers are still as they were imported gives the
//! original file back, byte for byte; once they changed, the file is
//! written from the layers.
//!
//! Sue Paint opens these formats through its own mapping, which keeps
//! folders, masks and the rest; this is the way for everything else.

use sue_core::{
    Arena, Bundle, CHILD, Color, ColorSpace, ComponentKind, Entity, EntityId, FormatError,
    FormatHandler, FormatInfo, OrderKey, PixelFormat, Properties, RasterTile2D, Relation, Value,
};
use sue_model::picture::{LayerKind, LayerStyle};
use sue_model::{BlendMode, Encoded, class, picture};

use crate::psd::{Psd, PsdKind, PsdLayer, blend_key, blend_name};
use crate::{NativeFormat, meta};

/// The property holding a fingerprint of a picture's layers as imported,
/// so export knows whether the original file still says the same.
pub const IMPORT_FINGERPRINT: &str = "import.fingerprint";

/// Which layered format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Clip,
    Psd,
}

/// `.clip` or `.psd` (and `.psb`) through the registry.
#[derive(Clone, Copy, Debug)]
pub struct LayeredFormat {
    kind: Kind,
}

impl LayeredFormat {
    /// Clip Studio Paint files.
    #[must_use]
    pub fn clip() -> Self {
        Self { kind: Kind::Clip }
    }

    /// Photoshop documents.
    #[must_use]
    pub fn psd() -> Self {
        Self { kind: Kind::Psd }
    }

    fn id(self) -> &'static str {
        match self.kind {
            Kind::Clip => "clip",
            Kind::Psd => "psd",
        }
    }
}

/// A folder of records or one record, bottom first.
enum Node<'a> {
    Layer(&'a PsdLayer),
    Folder(&'a PsdLayer, Vec<Node<'a>>),
}

fn tree(layers: &[PsdLayer]) -> Vec<Node<'_>> {
    let mut stack: Vec<Vec<Node<'_>>> = vec![Vec::new()];
    for layer in layers {
        match layer.kind {
            PsdKind::FolderEnd => stack.push(Vec::new()),
            PsdKind::FolderOpen { .. } => {
                let children = if stack.len() > 1 {
                    stack.pop().unwrap_or_default()
                } else {
                    Vec::new()
                };
                if let Some(parent) = stack.last_mut() {
                    parent.push(Node::Folder(layer, children));
                }
            }
            PsdKind::Pixels => {
                if let Some(parent) = stack.last_mut() {
                    parent.push(Node::Layer(layer));
                }
            }
        }
    }
    while stack.len() > 1 {
        let children = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.extend(children);
        }
    }
    stack.pop().unwrap_or_default()
}

/// `sue-model`'s blend mode for a Photoshop key (Normal for Through and
/// keys Sue has no mode for).
fn mode_of(key: [u8; 4]) -> BlendMode {
    let name = blend_name(key);
    BlendMode::ALL
        .into_iter()
        .find(|mode| camel(mode.name()) == name)
        .unwrap_or_default()
}

/// `color-burn` as `ColorBurn`.
fn camel(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// A record's pixels with its mask laid into the alpha when the mask is
/// on.
#[allow(clippy::cast_possible_truncation)]
fn masked(layer: &PsdLayer) -> Vec<u8> {
    let mut pixels = layer.pixels.clone();
    let Some(mask) = layer.mask.as_ref().filter(|mask| mask.enabled) else {
        return pixels;
    };
    let width = layer.width as usize;
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = layer.left as i64 + (index % width.max(1)) as i64 - i64::from(mask.left);
        let y = layer.top as i64 + (index / width.max(1)) as i64 - i64::from(mask.top);
        let inside = x >= 0 && y >= 0 && x < i64::from(mask.width) && y < i64::from(mask.height);
        let value = if inside {
            mask.pixels
                .get(y as usize * mask.width as usize + x as usize)
                .copied()
                .unwrap_or(mask.outside)
        } else {
            mask.outside
        };
        pixel[3] = ((u16::from(pixel[3]) * u16::from(value) + 127) / 255) as u8;
    }
    pixels
}

/// The layer entities of a tree, bottom first, each with what the folders
/// around it do to it.
fn flatten(nodes: &[Node<'_>], visible: bool, opacity: f64, out: &mut Vec<Entity>) {
    for node in nodes {
        match node {
            Node::Folder(folder, children) => flatten(
                children,
                visible && folder.visible,
                opacity * f64::from(folder.opacity) / 255.0,
                out,
            ),
            Node::Layer(layer) => {
                let (width, height) = (layer.width.max(1), layer.height.max(1));
                let pixels = if layer.width == 0 || layer.height == 0 {
                    vec![0; 4]
                } else {
                    masked(layer)
                };
                let Ok(tile) = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
                else {
                    continue;
                };
                out.push(
                    Entity::new(&layer.name, tile)
                        .with_class(class::LAYER)
                        .with_property(picture::VISIBLE, visible && layer.visible)
                        .with_property(picture::OPACITY, opacity * f64::from(layer.opacity) / 255.0)
                        .with_property(picture::BLEND, mode_of(layer.blend).name())
                        .with_property(picture::CLIP, layer.clip)
                        .with_property(picture::X, i64::from(layer.left))
                        .with_property(picture::Y, i64::from(layer.top)),
                );
            }
        }
    }
}

/// A fingerprint of the layers of a picture: their order, names, styles
/// and pixels.
#[allow(clippy::cast_possible_wrap)]
fn fingerprint(layers: &[&Entity]) -> i64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash = (hash ^ bytes.len() as u64).wrapping_mul(0x0000_0100_0000_01b3);
    };
    for layer in layers {
        eat(layer.name().as_bytes());
        eat(layer.class().as_bytes());
        let style = LayerStyle::of(layer);
        eat(format!("{style:?}").as_bytes());
        if let Some(raster) = raster(layer) {
            eat(&raster.width().to_le_bytes());
            eat(&raster.height().to_le_bytes());
            eat(raster.bytes().unwrap_or_default());
        }
        if let Some(color) = layer.property(picture::COLOR) {
            eat(format!("{color:?}").as_bytes());
        }
    }
    hash as i64
}

fn raster(entity: &Entity) -> Option<&RasterTile2D> {
    entity
        .component(ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
}

/// A layer's pixels as straight RGBA8, when it has pixels.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rgba8(tile: &RasterTile2D) -> Option<Vec<u8>> {
    match tile.format() {
        PixelFormat::Rgba8 => tile.bytes().map(<[u8]>::to_vec),
        PixelFormat::Gray8 => tile
            .bytes()
            .map(|gray| gray.iter().flat_map(|v| [*v, *v, *v, 255]).collect()),
        PixelFormat::RgbaF32 => Some(
            tile.f32_samples()?
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        ),
    }
}

/// The 8-bit sRGB of a color.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn srgb8(color: Color) -> [u8; 3] {
    color
        .to_space(ColorSpace::Srgb)
        .rgb
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The picture's layers as Photoshop records (draft layers left out),
/// and the picture merged.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn records(arena: &Arena, id: EntityId, size: [u32; 2]) -> Result<Psd, FormatError> {
    let [width, height] = size;
    let area = width as usize * height as usize;
    let mut layers = Vec::new();
    let mut merged = vec![[0.0_f32; 4]; area];
    // The alpha of the layer clipped layers clip to.
    let mut base: Vec<f32> = vec![0.0; area];
    for child in picture::layers(arena, id) {
        let entity = arena.get_by_id(child)?;
        let style = LayerStyle::of(entity);
        if style.draft {
            continue;
        }
        let (record, pixels) = match LayerKind::of(entity) {
            LayerKind::Raster => {
                let Some(tile) = raster(entity) else { continue };
                let Some(pixels) = rgba8(tile) else { continue };
                let [x, y] = style.offset.map(|v| i32::try_from(v).unwrap_or(0));
                (
                    PsdLayer::pixels(
                        entity.name(),
                        x,
                        y,
                        tile.width(),
                        tile.height(),
                        pixels.clone(),
                    ),
                    pixels,
                )
            }
            LayerKind::Paper => {
                let [r, g, b] = srgb8(picture::paper_color(entity));
                let pixels: Vec<u8> = std::iter::repeat_n([r, g, b, 255], area)
                    .flatten()
                    .collect();
                (
                    PsdLayer::pixels(entity.name(), 0, 0, width, height, pixels.clone()),
                    pixels,
                )
            }
            LayerKind::Other => continue,
        };
        let mut record = record;
        record.opacity = (style.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        record.visible = style.visible;
        record.clip = style.clip;
        record.blend = blend_key(&camel(style.blend.name()));
        if style.visible {
            let (left, top, w) = (record.left, record.top, record.width as usize);
            let clipped = style.clip;
            for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
                let (x, y) = (
                    i64::from(left) + (index % w.max(1)) as i64,
                    i64::from(top) + (index / w.max(1)) as i64,
                );
                if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                    continue;
                }
                let at = y as usize * width as usize + x as usize;
                let mut alpha = f32::from(pixel[3]) / 255.0 * style.opacity;
                if clipped {
                    alpha *= base[at];
                } else {
                    base[at] = f32::from(pixel[3]) / 255.0 * style.opacity;
                }
                let source = [
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                    alpha,
                ];
                merged[at] = style.blend.composite(merged[at], source);
            }
        }
        layers.push(record);
    }
    let merged = merged
        .iter()
        .flat_map(|pixel| pixel.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    Ok(Psd {
        width,
        height,
        layers,
        merged,
    })
}

impl FormatHandler for LayeredFormat {
    fn info(&self) -> FormatInfo {
        match self.kind {
            Kind::Clip => FormatInfo {
                id: "clip",
                name: "Clip Studio Paint file",
                extensions: &["clip"],
                mime: "application/x-clip-studio-paint",
                category: "Image",
            },
            Kind::Psd => FormatInfo {
                id: "psd",
                name: "Photoshop document",
                extensions: &["psd", "psb"],
                mime: "image/vnd.adobe.photoshop",
                category: "Image",
            },
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        let want = match self.kind {
            Kind::Clip => NativeFormat::Clip,
            Kind::Psd => NativeFormat::Psd,
        };
        matches!(crate::sniff_stream(head), crate::IngestRoute::Native(format) if format == want)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let (psd, dpi) = match self.kind {
            Kind::Clip => {
                let clip = crate::clip::decode_clip(bytes).map_err(FormatError::new)?;
                (clip.psd, Some(clip.dpi))
            }
            Kind::Psd => {
                let psd = crate::psd::decode_psd(bytes).map_err(FormatError::new)?;
                let dpi = crate::psd::psd_info(bytes).ok().and_then(|info| info.dpi);
                (psd, dpi)
            }
        };
        let mut layers = Vec::new();
        flatten(&tree(&psd.layers), true, 1.0, &mut layers);
        let print = fingerprint(&layers.iter().collect::<Vec<_>>());
        let info = self.info();
        let mut image = Entity::empty(name, class::IMAGE)
            .with_property(picture::WIDTH, i64::from(psd.width))
            .with_property(picture::HEIGHT, i64::from(psd.height))
            .with_property(IMPORT_FINGERPRINT, print)
            .with_ext(Encoded::new(info.id, bytes.to_vec()))
            .with_meta(meta::FORMAT, info.name)
            .with_meta(meta::BYTES, bytes.len().to_string())
            .with_meta(meta::WIDTH, psd.width.to_string())
            .with_meta(meta::HEIGHT, psd.height.to_string());
        if let Some(dpi) = dpi.filter(|dpi| *dpi > 0.0) {
            image = image.with_property(picture::DPI, dpi);
        }
        let orders = OrderKey::sequence(None, layers.len());
        let relations = layers
            .iter()
            .zip(orders)
            .map(|(layer, order)| Relation::new(image.id(), CHILD, layer.id()).ordered(order))
            .collect();
        let mut entities = vec![image];
        entities.extend(layers);
        Ok(Bundle {
            entities,
            relations,
        })
    }

    fn can_export(&self, entity: &Entity) -> bool {
        picture::is_picture(entity) && picture::size(entity).is_some()
            || entity
                .ext::<Encoded>()
                .is_some_and(|encoded| encoded.format == self.id())
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let id = *ids
            .first()
            .ok_or_else(|| FormatError::new("nothing to export"))?;
        let entity = arena.get_by_id(id)?;
        let original = entity
            .ext::<Encoded>()
            .filter(|encoded| encoded.format == self.id());
        let Some(size) = picture::size(entity).filter(|_| picture::is_picture(entity)) else {
            // A file kept by an older Sue, with no layers of its own.
            return original
                .map(|encoded| encoded.bytes.to_vec())
                .ok_or_else(|| FormatError::new("only layered pictures export to this format"));
        };
        let children = picture::layers(arena, id);
        let layers: Vec<&Entity> = children
            .iter()
            .filter_map(|child| arena.get_by_id(*child).ok())
            .collect();
        let unchanged = entity
            .property(IMPORT_FINGERPRINT)
            .and_then(Value::as_i64)
            .is_some_and(|print| print == fingerprint(&layers));
        if let Some(encoded) = original.filter(|_| unchanged) {
            return Ok(encoded.bytes.to_vec());
        }
        let dpi = entity
            .property(picture::DPI)
            .and_then(Value::as_f64)
            .filter(|dpi| *dpi > 0.0);
        let psd = records(arena, id, size)?;
        match self.kind {
            Kind::Clip => {
                crate::clip::encode_clip(&psd, dpi.unwrap_or(72.0), None).map_err(FormatError::new)
            }
            Kind::Psd => {
                let big = options
                    .get("psd.big")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || size[0].max(size[1]) > crate::psd::PSD_MAX_SIDE;
                crate::psd::encode_psd_at(&psd, big, dpi).map_err(FormatError::new)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psd::PsdMask;
    use sue_core::{InsertBundle, UndoStack};

    fn insert(bundle: Bundle) -> (Arena, EntityId) {
        let mut arena = Arena::new();
        let insert = InsertBundle::new("Import", bundle);
        let id = insert.ids()[0];
        UndoStack::new().apply(&mut arena, insert).unwrap();
        (arena, id)
    }

    fn square(name: &str, left: i32, size: u32, rgba: [u8; 4]) -> PsdLayer {
        let pixels = std::iter::repeat_n(rgba, (size * size) as usize)
            .flatten()
            .collect();
        PsdLayer::pixels(name, left, 1, size, size, pixels)
    }

    fn document() -> Psd {
        let mut ink = square("Ink", 2, 3, [200, 10, 10, 255]);
        ink.blend = blend_key("Multiply");
        ink.mask = Some(PsdMask {
            left: 2,
            top: 1,
            width: 1,
            height: 1,
            pixels: vec![0],
            outside: 255,
            enabled: true,
            linked: true,
        });
        let mut folder = PsdLayer::folder("Hidden folder", PsdKind::FolderOpen { open: true });
        folder.visible = false;
        folder.opacity = 128;
        Psd {
            width: 8,
            height: 6,
            layers: vec![
                square("Back", 0, 6, [255, 255, 255, 255]),
                PsdLayer::folder("</Layer group>", PsdKind::FolderEnd),
                square("Inside", 0, 2, [0, 0, 255, 255]),
                folder,
                ink,
            ],
            merged: vec![255; 8 * 6 * 4],
        }
    }

    fn layer_named<'a>(arena: &'a Arena, picture_id: EntityId, name: &str) -> &'a Entity {
        picture::layers(arena, picture_id)
            .into_iter()
            .map(|id| arena.get_by_id(id).unwrap())
            .find(|entity| entity.name() == name)
            .unwrap()
    }

    #[test]
    fn a_psd_opens_as_a_layered_picture_and_saves_back_unchanged() {
        let bytes = crate::psd::encode_psd_at(&document(), false, Some(350.0)).unwrap();
        let format = LayeredFormat::psd();
        assert!(format.sniff(&bytes));
        let (arena, id) = insert(format.import(&bytes, "art", &Properties::new()).unwrap());
        let image = arena.get_by_id(id).unwrap();
        assert!(picture::is_picture(image));
        assert_eq!(picture::size(image), Some([8, 6]));
        assert_eq!(
            image.property(picture::DPI).and_then(Value::as_f64),
            Some(350.0)
        );
        let names: Vec<&str> = picture::layers(&arena, id)
            .into_iter()
            .map(|layer| arena.get_by_id(layer).unwrap().name())
            .collect();
        assert_eq!(names, ["Back", "Inside", "Ink"]);
        // The folder hides what it holds and halves its opacity.
        let inside = LayerStyle::of(layer_named(&arena, id, "Inside"));
        assert!(!inside.visible);
        assert!((inside.opacity - 128.0 / 255.0).abs() < 1e-6);
        let ink = layer_named(&arena, id, "Ink");
        let style = LayerStyle::of(ink);
        assert_eq!((style.blend, style.offset), (BlendMode::Multiply, [2, 1]));
        // The mask that is on is in the alpha: its one hidden pixel.
        let pixels = raster(ink).unwrap().bytes().unwrap();
        assert_eq!((pixels[3], pixels[7]), (0, 255));
        // Untouched, the original comes back byte for byte.
        let again = format.export(&arena, &[id], &Properties::new()).unwrap();
        assert_eq!(again, bytes);
    }

    #[test]
    fn a_changed_picture_is_written_from_its_layers() {
        let bytes = crate::psd::encode_psd_at(&document(), false, Some(300.0)).unwrap();
        let format = LayeredFormat::psd();
        let (mut arena, id) = insert(format.import(&bytes, "art", &Properties::new()).unwrap());
        let back = picture::layers(&arena, id)[0];
        let handle = arena.handle_of(back).unwrap();
        arena
            .get_mut(handle)
            .unwrap()
            .properties_mut()
            .set(picture::OPACITY, 0.5);
        let written = format.export(&arena, &[id], &Properties::new()).unwrap();
        assert_ne!(written, bytes);
        let psd = crate::psd::decode_psd(&written).unwrap();
        let names: Vec<&str> = psd.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Back", "Inside", "Ink"]);
        assert_eq!(psd.layers[0].opacity, 128);
        assert!(!psd.layers[1].visible);
        assert_eq!(psd.layers[2].blend, *b"mul ");
        assert_eq!(crate::psd::psd_info(&written).unwrap().dpi, Some(300.0));
        // The merged picture: nothing above the back layer, half white
        // on its first row, and the ink multiplied where it shows.
        assert_eq!(&psd.merged[..4], &[0, 0, 0, 0]);
        let at = |x: usize, y: usize| &psd.merged[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
        assert_eq!(at(0, 1), &[255, 255, 255, 128]);
        assert_eq!(at(2, 1), &[255, 255, 255, 128], "the masked pixel");
        // Multiply of red over white keeps the red; the ink is opaque.
        assert_eq!(at(3, 1), &[200, 10, 10, 255]);
        // As a .clip too, with the picture's resolution.
        let clip = LayeredFormat::clip()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let read = crate::clip::read(&clip).unwrap();
        assert_eq!((read.width, read.height, read.dpi), (8, 6, 300.0));
        assert_eq!(read.layers().len(), 3);
    }

    #[test]
    fn a_clip_opens_as_a_layered_picture_and_saves_back_unchanged() {
        let doc = crate::clip::testkit::raster_subset(40, 30, 7);
        let bytes = crate::clip::write(&doc).unwrap();
        let format = LayeredFormat::clip();
        assert!(format.sniff(&bytes) && !LayeredFormat::psd().sniff(&bytes));
        let (arena, id) = insert(format.import(&bytes, "comic", &Properties::new()).unwrap());
        let image = arena.get_by_id(id).unwrap();
        assert_eq!(picture::size(image), Some([40, 30]));
        assert_eq!(
            image.property(picture::DPI).and_then(Value::as_f64),
            Some(350.0)
        );
        assert!(picture::layers(&arena, id).len() > 3);
        assert_eq!(
            format.export(&arena, &[id], &Properties::new()).unwrap(),
            bytes
        );
    }

    #[test]
    fn the_registry_uses_the_real_handlers() {
        let formats = crate::formats(sue_model::registry());
        let psd = crate::psd::encode_psd(&document(), false).unwrap();
        let clip = crate::clip::write(&crate::clip::testkit::raster_subset(9, 9, 1)).unwrap();
        assert_eq!(
            formats.importer_for(&psd, "x.bin").unwrap().info().id,
            "psd"
        );
        assert_eq!(
            formats.importer_for(&clip, "x.bin").unwrap().info().id,
            "clip"
        );
        assert_eq!(formats.for_name("big.psb").unwrap().info().id, "psd");
        // A file an older Sue kept as bytes still exports as it was.
        let kept = Entity::empty("old", class::IMAGE).with_ext(Encoded::new("clip", clip.clone()));
        let mut arena = Arena::new();
        let kept_id = kept.id();
        arena.insert_keep_id(kept).unwrap();
        let handler = formats.get("clip").unwrap();
        assert!(handler.can_export(arena.get_by_id(kept_id).unwrap()));
        assert_eq!(
            handler
                .export(&arena, &[kept_id], &Properties::new())
                .unwrap(),
            clip
        );
        // Damaged files say so.
        assert!(
            LayeredFormat::psd()
                .import(&psd[..psd.len() / 2], "cut", &Properties::new())
                .is_err()
        );
        assert!(
            LayeredFormat::clip()
                .import(&clip[..clip.len() / 2], "cut", &Properties::new())
                .is_err()
        );
    }

    #[test]
    fn blend_names_map_to_photoshop_keys_and_back() {
        for mode in BlendMode::ALL {
            let key = blend_key(&camel(mode.name()));
            assert_eq!(mode_of(key), mode, "{}", mode.name());
        }
        assert_eq!(mode_of(*b"pass"), BlendMode::Normal);
    }
}
