//! Photoshop documents in and out of a picture with their layers kept:
//! Export as `.psd` / `.psb` and File, Import, Image as Separate Layers.

use sue_core::txn::{Relate, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, OrderKey, PixelFormat, RasterTile2D, Relation};
use sue_io::psd::{Psd, PsdKind, PsdLayer, blend_key, blend_name};
use sue_model::BlendMode;
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

use crate::composite::{RenderOptions, flatten};
use crate::doc::{self, Edit, MEMBER};

/// `picture` as a Photoshop document: every layer and folder as it stacks
/// (drafts left out, masks laid into each layer's alpha, paper as a
/// filled layer), each cut to where it has pixels.
#[must_use]
pub fn to_psd(arena: &Arena, picture: EntityId) -> Option<Psd> {
    let [width, height] = picture::size(arena.get_by_id(picture).ok()?)?;
    let options = RenderOptions {
        paper: true,
        drafts: false,
    };
    let merged = flatten(arena, picture, options)?.bytes()?.to_vec();
    let stack = doc::layers(arena, picture);
    let mut layers = Vec::new();
    emit(arena, &stack, None, [width, height], &mut layers);
    Some(Psd {
        width,
        height,
        layers,
        merged,
    })
}

/// The records of the layers whose folder is `parent`, bottom first.
fn emit(
    arena: &Arena,
    stack: &[doc::LayerInfo],
    parent: Option<EntityId>,
    size: [u32; 2],
    out: &mut Vec<PsdLayer>,
) {
    for info in stack
        .iter()
        .filter(|info| doc::folder_of(arena, info.id) == parent)
    {
        if info.style.draft {
            continue;
        }
        if doc::is_folder(arena, info.id) {
            out.push(PsdLayer::folder("</Layer group>", PsdKind::FolderEnd));
            emit(arena, stack, Some(info.id), size, out);
            let mut folder = PsdLayer::folder(
                &info.name,
                PsdKind::FolderOpen {
                    open: doc::is_open(arena, info.id),
                },
            );
            folder.opacity = unit_byte(info.style.opacity);
            folder.visible = info.style.visible;
            out.push(folder);
            continue;
        }
        let Some(mut layer) = pixels_of(arena, info, size) else {
            continue;
        };
        layer.opacity = unit_byte(info.style.opacity);
        layer.visible = info.style.visible;
        layer.clip = info.style.clip;
        layer.blend = blend_key(&camel(info.style.blend));
        out.push(layer);
    }
}

/// A layer's pixels, cut to where it is not clear.
fn pixels_of(arena: &Arena, info: &doc::LayerInfo, size: [u32; 2]) -> Option<PsdLayer> {
    let entity = arena.get_by_id(info.id).ok()?;
    let (mut pixels, width, height, [x, y]) = match info.kind {
        LayerKind::Raster => {
            let raster = doc::raster(arena, info.id)?;
            if raster.format() != PixelFormat::Rgba8 {
                return None;
            }
            let mut pixels = raster.bytes()?.to_vec();
            if let Some(mask) = doc::mask_of(arena, info.id)
                .filter(|_| doc::mask_on(arena, info.id))
                .and_then(|mask| doc::raster(arena, mask))
                .and_then(RasterTile2D::bytes)
            {
                for (pixel, cover) in pixels.chunks_mut(4).zip(mask.chunks(4)) {
                    pixel[3] = ((u32::from(pixel[3]) * u32::from(cover[3])) / 255) as u8;
                }
            }
            (pixels, raster.width(), raster.height(), info.style.offset)
        }
        LayerKind::Paper => {
            let rgb = picture::paper_color(entity)
                .to_space(sue_core::ColorSpace::Srgb)
                .rgb;
            let [r, g, b] = rgb.map(unit_byte);
            let pixels = [r, g, b, 255].repeat((size[0] * size[1]) as usize);
            (pixels, size[0], size[1], [0, 0])
        }
        LayerKind::Other => return None,
    };
    // Cut to the pixels that show.
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    for row in 0..height {
        for column in 0..width {
            if pixels[((row * width + column) * 4 + 3) as usize] > 0 {
                x0 = x0.min(column);
                y0 = y0.min(row);
                x1 = x1.max(column + 1);
                y1 = y1.max(row + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return Some(PsdLayer::pixels(&info.name, 0, 0, 0, 0, Vec::new()));
    }
    if [x0, y0, x1, y1] != [0, 0, width, height] {
        let mut cut = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
        for row in y0..y1 {
            let start = ((row * width + x0) * 4) as usize;
            cut.extend_from_slice(&pixels[start..start + ((x1 - x0) * 4) as usize]);
        }
        pixels = cut;
    }
    Some(PsdLayer::pixels(
        &info.name,
        (x + i64::from(x0)) as i32,
        (y + i64::from(y0)) as i32,
        x1 - x0,
        y1 - y0,
        pixels,
    ))
}

/// The edit that adds the layers and folders of `psd` to `picture` right
/// above `below` (or on top), each layer the picture's size with its
/// pixels where the document put them.
///
/// # Errors
/// If the document has no layers or the picture has no size.
pub fn import(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    psd: &Psd,
) -> Result<Edit, String> {
    // A picture being made in the same edit takes the document's size.
    let [width, height] = arena
        .get_by_id(picture)
        .ok()
        .and_then(picture::size)
        .unwrap_or([psd.width, psd.height]);
    if width == 0 || height == 0 {
        return Err("The picture has no size".to_owned());
    }
    let mut records = psd.layers.clone();
    if records.is_empty() {
        // A document with only its merged picture.
        records.push(PsdLayer::pixels(
            "Background",
            0,
            0,
            psd.width,
            psd.height,
            psd.merged.clone(),
        ));
    }
    let stack = doc::layers(arena, picture);
    let at = below.and_then(|id| stack.iter().position(|layer| layer.id == id));
    let mut low = match at {
        Some(index) => Some(stack[index].order.clone()),
        None => stack.last().map(|layer| layer.order.clone()),
    };
    let high = at
        .and_then(|index| stack.get(index + 1))
        .map(|layer| layer.order.clone());
    let mut next_key = || {
        let key = match (&low, &high) {
            (Some(low), Some(high)) if high > low => OrderKey::between(Some(low), Some(high)),
            (low, _) => OrderKey::between(low.as_ref(), None),
        };
        low = Some(key.clone());
        key
    };
    let outer = below.and_then(|below| doc::folder_of(arena, below));
    let mut edit = Edit::new("Import Layers");
    // The folders open while walking up: their members so far.
    let mut open: Vec<Vec<EntityId>> = vec![Vec::new()];
    for record in &records {
        match record.kind {
            PsdKind::FolderEnd => open.push(Vec::new()),
            PsdKind::FolderOpen { open: shown } => {
                let members = if open.len() > 1 {
                    open.pop().unwrap_or_default()
                } else {
                    Vec::new()
                };
                let folder = Entity::empty(record.name.clone(), class::FOLDER)
                    .with_property(doc::OPEN, shown)
                    .with_property(picture::OPACITY, f64::from(record.opacity) / 255.0)
                    .with_property(picture::VISIBLE, record.visible);
                let id = folder.id();
                edit = edit.then(
                    SpawnEntity::new(folder)
                        .with_relation(Relation::new(picture, CHILD, id).ordered(next_key())),
                );
                for member in members {
                    edit = edit.then(Relate::new(Relation::new(id, MEMBER, member)));
                }
                if let Some(list) = open.last_mut() {
                    list.push(id);
                }
            }
            PsdKind::Pixels => {
                let mut pixels = vec![0_u8; (width * height * 4) as usize];
                for row in 0..record.height {
                    let ty = i64::from(record.top) + i64::from(row);
                    if ty < 0 || ty >= i64::from(height) {
                        continue;
                    }
                    for column in 0..record.width {
                        let tx = i64::from(record.left) + i64::from(column);
                        if tx < 0 || tx >= i64::from(width) {
                            continue;
                        }
                        let from = ((row * record.width + column) * 4) as usize;
                        let to = ((ty as u32 * width + tx as u32) * 4) as usize;
                        pixels[to..to + 4].copy_from_slice(&record.pixels[from..from + 4]);
                    }
                }
                let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
                    .map_err(|error| error.to_string())?;
                let name = if record.name.is_empty() {
                    "Layer"
                } else {
                    &record.name
                };
                let blend = from_camel(blend_name(record.blend));
                let mut layer = Entity::new(name, raster).with_class(class::LAYER);
                let style = LayerStyle::default();
                if record.opacity != 255 {
                    layer =
                        layer.with_property(picture::OPACITY, f64::from(record.opacity) / 255.0);
                }
                if record.visible != style.visible {
                    layer = layer.with_property(picture::VISIBLE, record.visible);
                }
                if record.clip {
                    layer = layer.with_property(picture::CLIP, true);
                }
                if blend != BlendMode::Normal {
                    layer = layer.with_property(picture::BLEND, blend.name());
                }
                let id = layer.id();
                edit = edit.then(
                    SpawnEntity::new(layer)
                        .with_relation(Relation::new(picture, CHILD, id).ordered(next_key())),
                );
                if let Some(list) = open.last_mut() {
                    list.push(id);
                }
            }
        }
    }
    // What is left at the top goes in the folder of `below`.
    if let Some(outer) = outer {
        for remaining in open.concat() {
            edit = edit.then(Relate::new(Relation::new(outer, MEMBER, remaining)));
        }
    }
    Ok(edit)
}

/// 0 to 1 as 0 to 255.
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A blend mode as `ColorDodge` (Photoshop keys are looked up this way).
fn camel(mode: BlendMode) -> String {
    mode.name()
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// `ColorDodge` as a blend mode (`Normal` for names Sue has no mode for).
fn from_camel(name: &str) -> BlendMode {
    let mut kebab = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && index > 0 {
            kebab.push('-');
        }
        kebab.push(c.to_ascii_lowercase());
    }
    BlendMode::from_name_or_normal(&kebab)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::ReversibleTransaction;
    use sue_core::txn::SetProperty;

    #[test]
    fn a_picture_goes_out_as_psd_and_back_with_its_layers_and_folders() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&doc::NewPicture {
            name: "Page".to_owned(),
            width: 8,
            height: 6,
            ..doc::NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        // An ink layer with a dot, multiplied, in a folder.
        let top = doc::layers(&arena, picture).last().map(|layer| layer.id);
        let mut pixels = vec![0_u8; 8 * 6 * 4];
        pixels[(2 * 8 + 3) * 4..][..4].copy_from_slice(&[10, 20, 30, 255]);
        let raster = RasterTile2D::from_bytes(8, 6, PixelFormat::Rgba8, pixels).unwrap();
        let (spawn, ink) =
            doc::add_layer_pixels(&arena, picture, top, "Ink".to_owned(), raster).unwrap();
        let mut edit = Edit::new("Ink")
            .then(spawn)
            .then(SetProperty::new(ink, picture::BLEND, "multiply"))
            .then(SetProperty::new(ink, picture::OPACITY, 0.5));
        edit.apply(&mut arena).unwrap();
        let (mut edit, _) = doc::new_folder(&arena, picture, Some(ink), true);
        edit.apply(&mut arena).unwrap();

        let psd = to_psd(&arena, picture).unwrap();
        let kinds: Vec<PsdKind> = psd.layers.iter().map(|layer| layer.kind).collect();
        assert!(kinds.contains(&PsdKind::FolderEnd));
        let dot = psd.layers.iter().find(|layer| layer.name == "Ink").unwrap();
        assert_eq!(
            (dot.left, dot.top, dot.width, dot.height),
            (3, 2, 1, 1),
            "cut to the dot"
        );
        assert_eq!(&dot.blend, b"mul ");
        assert_eq!(dot.opacity, 128);
        let bytes = sue_io::psd::encode_psd(&psd, false).unwrap();
        let back = sue_io::psd::decode_psd(&bytes).unwrap();

        // Into a fresh picture: the same layers, folder and style.
        let mut other = Arena::new();
        let (mut edit, fresh) = doc::create(&doc::NewPicture {
            name: "Fresh".to_owned(),
            width: 8,
            height: 6,
            ..doc::NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut other).unwrap();
        let before = doc::layers(&other, fresh).len();
        let mut edit = import(&other, fresh, None, &back).unwrap();
        edit.apply(&mut other).unwrap();
        let after = doc::layers(&other, fresh);
        let ink = after.iter().find(|layer| layer.name == "Ink").unwrap();
        assert_eq!(ink.style.blend, BlendMode::Multiply);
        assert!((ink.style.opacity - 128.0 / 255.0).abs() < 1e-3);
        let folder = doc::folder_of(&other, ink.id).expect("in its folder");
        assert!(doc::is_folder(&other, folder));
        let pixels = doc::raster(&other, ink.id).unwrap().bytes().unwrap();
        assert_eq!(&pixels[(2 * 8 + 3) * 4..][..4], &[10, 20, 30, 255]);
        assert!(after.len() > before);
        edit.revert(&mut other).unwrap();
        assert_eq!(
            doc::layers(&other, fresh).len(),
            before,
            "undo takes them away"
        );
    }

    #[test]
    fn blend_names_go_both_ways() {
        for mode in [
            BlendMode::Normal,
            BlendMode::ColorDodge,
            BlendMode::LinearLight,
            BlendMode::Luminosity,
        ] {
            assert_eq!(from_camel(&camel(mode)), mode);
        }
    }
}
