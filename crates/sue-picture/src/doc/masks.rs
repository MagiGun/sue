//! Layer masks: a raster beside its layer (or folder, paper, correction
//! or any other layer) that says how much of it shows.
//!
//! A mask is an entity of class [`MASK_CLASS`] related to its layer by
//! [`MASK`]. Its raster is `Gray8`, whose value is the cover: what Sue
//! makes and paints. Masks of 0.0.75 and of the first 0.0.8 builds are
//! `Rgba8`, whose alpha is the cover; they become `Gray8` when their
//! picture opens ([`migrate_masks`]), and every reader goes through
//! [`cover`], so both show the same until then.
//!
//! Where a mask sits: while it is linked to its layer ([`MASK_LINKED`],
//! the default) its own `layer.x` and `layer.y` are relative to the
//! layer's, so it moves with the layer; unlinked they are picture pixels,
//! so the layer moves under it. A mask without them sits where its layer
//! does, as every mask of 0.0.75 did.

use sue_core::txn::{DespawnEntity, SetProperty, SpawnEntity};
use sue_core::{Arena, Entity, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::picture::{self, LayerStyle};

use super::{Edit, raster, size_of};

/// The relation from a layer to its mask.
pub const MASK: &str = "sue.paint.mask";

/// A layer's mask is in use (`Bool`, on the layer).
pub const MASK_ON: &str = "layer.mask_on";

/// A layer's mask moves with it (`Bool`, on the layer; on when missing).
pub const MASK_LINKED: &str = "layer.mask_linked";

/// The class of a layer mask: a raster whose cover (the alpha of `Rgba8`,
/// the value of `Gray8`) is how much of its layer shows.
pub const MASK_CLASS: &str = "sue.paint.layer_mask";

/// The mask of `layer`, if it has one.
#[must_use]
pub fn mask_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    arena.relations().targets(layer, MASK).into_iter().next()
}

/// `true` if `layer`'s mask is in use.
#[must_use]
pub fn mask_on(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.property(MASK_ON))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// `true` if `layer`'s mask moves with it (Link Mask to Layer).
#[must_use]
pub fn mask_linked(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.property(MASK_LINKED))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// How much a mask raster shows at its pixel `(x, y)`, 0 to 1: 0 outside
/// it.
#[must_use]
pub fn cover(mask: &RasterTile2D, x: i64, y: i64) -> f32 {
    if x < 0 || y < 0 || x >= i64::from(mask.width()) || y >= i64::from(mask.height()) {
        return 0.0;
    }
    let index = y as usize * mask.width() as usize + x as usize;
    match mask.format() {
        PixelFormat::Gray8 => mask
            .bytes()
            .map_or(1.0, |bytes| f32::from(bytes[index]) / 255.0),
        PixelFormat::Rgba8 => mask
            .bytes()
            .map_or(1.0, |bytes| f32::from(bytes[index * 4 + 3]) / 255.0),
        PixelFormat::RgbaF32 => mask
            .f32_samples()
            .map_or(1.0, |samples| samples[index * 4 + 3].clamp(0.0, 1.0)),
    }
}

/// The picture pixel where the top left of `layer`'s mask sits, if it
/// has one.
#[must_use]
pub fn mask_offset(arena: &Arena, layer: EntityId) -> Option<[i64; 2]> {
    let mask = arena.get_by_id(mask_of(arena, layer)?).ok()?;
    let own = LayerStyle::of(mask).offset;
    if mask_linked(arena, layer) {
        let at = arena
            .get_by_id(layer)
            .map(|entity| LayerStyle::of(entity).offset)
            .unwrap_or_default();
        Some([at[0] + own[0], at[1] + own[1]])
    } else {
        Some(own)
    }
}

/// How much of `layer` its mask shows at picture pixel `(x, y)`: 1 with
/// no mask or with the mask off.
#[must_use]
pub fn shown_at(arena: &Arena, layer: EntityId, x: i64, y: i64) -> f32 {
    let Some(mask) = mask_of(arena, layer).filter(|_| mask_on(arena, layer)) else {
        return 1.0;
    };
    let (Some(tile), Some(at)) = (raster(arena, mask), mask_offset(arena, layer)) else {
        return 1.0;
    };
    cover(tile, x - at[0], y - at[1])
}

/// The edit that gives `layer` (any kind: a raster, a folder, paper, a
/// correction) a mask showing it where `show` (picture pixels, 0 to 1)
/// says, and the mask's id: Mask Outside Selection is the selection, Mask
/// Selection its opposite. A raster layer's mask is its size and sits
/// where it does; any other layer's is the picture's.
///
/// # Errors
/// If the layer already has a mask, or neither it nor its picture has a
/// size.
pub fn new_mask(
    arena: &Arena,
    layer: EntityId,
    show: impl Fn(i64, i64) -> f32,
) -> Result<(Edit, EntityId), String> {
    if mask_of(arena, layer).is_some() {
        return Err(tr!("layer-has-mask-already"));
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let (width, height) = match raster(arena, layer) {
        Some(tile) => (tile.width(), tile.height()),
        None => {
            let picture =
                picture::picture_of(arena, layer).ok_or(tr!("only-raster-layer-takes-mask"))?;
            let [width, height] = size_of(arena, picture)?;
            (width, height)
        }
    };
    let offset = LayerStyle::of(entity).offset;
    let mut pixels = vec![255; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let cover = show(i64::from(x) + offset[0], i64::from(y) + offset[1]).clamp(0.0, 1.0);
            pixels[(y * width + x) as usize] = (cover * 255.0).round() as u8;
        }
    }
    let mask = Entity::new(
        tr!("mask"),
        RasterTile2D::from_bytes(width, height, PixelFormat::Gray8, pixels)
            .map_err(|error| error.to_string())?,
    )
    .with_class(MASK_CLASS);
    let id = mask.id();
    let edit = Edit::new(tr!("new-layer-mask"))
        .then(SpawnEntity::new(mask).with_relation(Relation::new(layer, MASK, id)))
        .then(SetProperty::new(layer, MASK_ON, true));
    Ok((edit, id))
}

/// The edit that links `layer`'s mask to it (`linked`: it moves with the
/// layer from now on) or unlinks it (the layer moves under it), with the
/// mask staying where it is on the canvas. `None` when there is no mask or
/// nothing changes.
#[must_use]
pub fn set_mask_linked(arena: &Arena, layer: EntityId, linked: bool) -> Option<Edit> {
    let mask = mask_of(arena, layer)?;
    if mask_linked(arena, layer) == linked {
        return None;
    }
    let at = mask_offset(arena, layer)?;
    let base = arena
        .get_by_id(layer)
        .map(|entity| LayerStyle::of(entity).offset)
        .unwrap_or_default();
    let own = if linked {
        [at[0] - base[0], at[1] - base[1]]
    } else {
        at
    };
    Some(
        Edit::new(if linked {
            tr!("link-mask-to-layer")
        } else {
            tr!("unlink-mask-from-layer")
        })
        .then(SetProperty::new(mask, picture::X, own[0]))
        .then(SetProperty::new(mask, picture::Y, own[1]))
        .then(SetProperty::new(layer, MASK_LINKED, linked)),
    )
}

/// Turns every `Rgba8` layer mask in `arena` (0.0.75's, and the first
/// 0.0.8 builds') into the `Gray8` mask Sue makes, its alpha becoming the
/// value, so each pixel covers as it did. A migration on load: it puts
/// nothing on the undo stack, and a picture whose masks it turned looks
/// and saves as before. Answers how many masks it turned; a second run
/// turns none.
pub fn migrate_masks(arena: &mut Arena) -> usize {
    let old: Vec<EntityId> = arena
        .iter()
        .filter(|(_, entity)| entity.class() == MASK_CLASS)
        .filter(|(_, entity)| {
            entity
                .component(sue_core::ComponentKind::RasterTile2D)
                .and_then(sue_core::ComponentData::as_raster)
                .is_some_and(|tile| tile.format() == PixelFormat::Rgba8 && tile.bytes().is_some())
        })
        .map(|(_, entity)| entity.id())
        .collect();
    let mut turned = 0;
    for id in old {
        let Some(gray) = raster(arena, id).and_then(gray_of) else {
            continue;
        };
        if let Ok(entity) = arena.get_by_id_mut(id)
            && let Some(data) = entity.component_mut(sue_core::ComponentKind::RasterTile2D)
        {
            *data = sue_core::ComponentData::RasterTile2D(gray);
            turned += 1;
        }
    }
    turned
}

/// The `Gray8` raster whose values are `tile`'s alpha (an `Rgba8` mask).
fn gray_of(tile: &RasterTile2D) -> Option<RasterTile2D> {
    let bytes = tile.bytes()?;
    let values = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect();
    RasterTile2D::from_bytes(tile.width(), tile.height(), PixelFormat::Gray8, values).ok()
}

/// The edit that takes `layer`'s mask away (undo brings it back).
#[must_use]
pub fn delete_mask(arena: &Arena, layer: EntityId) -> Option<Edit> {
    let mask = mask_of(arena, layer)?;
    Some(Edit::new(tr!("delete-mask")).then(DespawnEntity::new(mask)))
}

/// The edit that applies `layer`'s mask to its pixels, then deletes it.
///
/// # Errors
/// If the layer has no 8-bit pixels or no mask.
pub fn apply_mask(arena: &Arena, layer: EntityId) -> Result<Edit, String> {
    let mask = mask_of(arena, layer).ok_or(tr!("this-layer-has-no-mask"))?;
    let tile = raster(arena, layer).ok_or(tr!("the-layer-has-no-pixels"))?;
    let shape = raster(arena, mask).ok_or(tr!("the-mask-has-no-pixels"))?;
    if tile.format() != PixelFormat::Rgba8 {
        return Err(tr!("the-layer-has-no-pixels"));
    }
    let mut pixels = tile.bytes().ok_or(tr!("the-layer-has-no-pixels"))?.to_vec();
    let base = arena
        .get_by_id(layer)
        .map(|entity| LayerStyle::of(entity).offset)
        .unwrap_or_default();
    let at = mask_offset(arena, layer).unwrap_or(base);
    let shift = [base[0] - at[0], base[1] - at[1]];
    let width = tile.width() as usize;
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((index % width) as i64, (index / width) as i64);
        let shown = cover(shape, x + shift[0], y + shift[1]);
        let shown = (shown * 255.0).round() as u16;
        pixel[3] = ((u16::from(pixel[3]) * shown + 127) / 255) as u8;
    }
    let applied = RasterTile2D::from_bytes(tile.width(), tile.height(), PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    Ok(Edit::new(tr!("apply-mask-to-layer"))
        .then(sue_core::txn::ReplaceComponent::new(
            layer,
            applied,
            tr!("apply-mask-to-layer"),
        ))
        .then(DespawnEntity::new(mask)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::{RenderOptions, render};
    use crate::doc::{NewPicture, create, layers, raster_mut};

    /// A picture 4 by 1 with a white layer, and that layer.
    fn white_picture() -> (Arena, sue_core::UndoStack<Arena>, EntityId, EntityId) {
        let mut arena = Arena::new();
        let mut history: sue_core::UndoStack<Arena> = sue_core::UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 4,
            height: 1,
            paper: None,
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture)[0].id;
        if let Some(bytes) = raster_mut(&mut arena, layer).and_then(RasterTile2D::bytes_mut) {
            bytes.fill(255);
        }
        (arena, history, picture, layer)
    }

    fn shown(arena: &Arena, picture: EntityId) -> Vec<u8> {
        render(arena, picture, [0, 0, 4, 1], RenderOptions::default())
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect()
    }

    #[test]
    fn a_new_mask_is_gray() {
        let (mut arena, mut history, picture, layer) = white_picture();
        let (edit, mask) =
            new_mask(&arena, layer, |x, _| [1.0, 0.5, 0.0, 1.0][x as usize]).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let tile = raster(&arena, mask).unwrap();
        assert_eq!(tile.format(), PixelFormat::Gray8);
        assert_eq!(tile.bytes().unwrap(), &[255, 128, 0, 255]);
        assert_eq!(shown(&arena, picture), vec![255, 128, 0, 255]);
        history.undo(&mut arena).unwrap();
        assert_eq!(mask_of(&arena, layer), None, "undo takes it away");
    }

    /// An `Rgba8` mask as 0.0.75 wrote it (color white, alpha the cover)
    /// becomes `Gray8` and shows the same; the undo stack is untouched and
    /// a second run changes nothing.
    #[test]
    fn old_rgba_masks_turn_gray_and_show_the_same() {
        let (mut arena, history, picture, layer) = white_picture();
        let covers = [255_u8, 200, 77, 0];
        let pixels = covers.iter().flat_map(|&a| [255, 255, 255, a]).collect();
        let old = Entity::new(
            "Mask",
            RasterTile2D::from_bytes(4, 1, PixelFormat::Rgba8, pixels).unwrap(),
        )
        .with_class(MASK_CLASS);
        let mask = old.id();
        arena.insert(old);
        arena.relate(Relation::new(layer, MASK, mask)).unwrap();
        let before = shown(&arena, picture);
        let steps = history.len();
        assert_eq!(migrate_masks(&mut arena), 1);
        let tile = raster(&arena, mask).unwrap();
        assert_eq!(tile.format(), PixelFormat::Gray8);
        assert_eq!(tile.bytes().unwrap(), &covers);
        assert_eq!(shown(&arena, picture), before, "it shows as it did");
        assert_eq!(history.len(), steps, "nothing on the undo stack");
        assert_eq!(mask_of(&arena, layer), Some(mask), "the same entity");
        assert_eq!(migrate_masks(&mut arena), 0, "a second run turns none");
    }
}
