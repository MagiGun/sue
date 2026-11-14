//! Layer masks: a raster beside its layer whose alpha says how much of
//! the layer shows.

use sue_core::txn::{DespawnEntity, SetProperty, SpawnEntity};
use sue_core::{Arena, Entity, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::picture::LayerStyle;

use super::{Edit, raster};

/// The relation from a layer to its mask.
pub const MASK: &str = "sue.paint.mask";

/// A layer's mask is in use (`Bool`, on the layer).
pub const MASK_ON: &str = "layer.mask_on";

/// The class of a layer mask: an RGBA raster whose alpha is how much of
/// its layer shows.
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

/// The edit that gives `layer` a mask showing it where `show` (picture
/// pixels, 0 to 1) says, and the mask's id: Mask Outside Selection is the
/// selection, Mask Selection its opposite.
///
/// # Errors
/// If the layer has no pixels or already has a mask.
pub fn new_mask(
    arena: &Arena,
    layer: EntityId,
    show: impl Fn(i64, i64) -> f32,
) -> Result<(Edit, EntityId), String> {
    if mask_of(arena, layer).is_some() {
        return Err(tr!("layer-has-mask-already"));
    }
    let tile = raster(arena, layer).ok_or(tr!("only-raster-layer-takes-mask"))?;
    let (width, height) = (tile.width(), tile.height());
    let offset = arena
        .get_by_id(layer)
        .map(LayerStyle::of)
        .map(|style| style.offset)
        .unwrap_or_default();
    let mut pixels = vec![255; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let cover = show(i64::from(x) + offset[0], i64::from(y) + offset[1]).clamp(0.0, 1.0);
            pixels[((y * width + x) * 4 + 3) as usize] = (cover * 255.0).round() as u8;
        }
    }
    let mask = Entity::new(
        tr!("mask"),
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
            .map_err(|error| error.to_string())?,
    )
    .with_class(MASK_CLASS);
    let id = mask.id();
    let edit = Edit::new(tr!("new-layer-mask"))
        .then(SpawnEntity::new(mask).with_relation(Relation::new(layer, MASK, id)))
        .then(SetProperty::new(layer, MASK_ON, true));
    Ok((edit, id))
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
/// If the layer or its mask has no pixels, or they differ in size.
pub fn apply_mask(arena: &Arena, layer: EntityId) -> Result<Edit, String> {
    let mask = mask_of(arena, layer).ok_or(tr!("this-layer-has-no-mask"))?;
    let (tile, shape) = (
        raster(arena, layer).ok_or(tr!("the-layer-has-no-pixels"))?,
        raster(arena, mask).ok_or(tr!("the-mask-has-no-pixels"))?,
    );
    if [tile.width(), tile.height()] != [shape.width(), shape.height()] {
        return Err(tr!("mask-layer-differ-size"));
    }
    let mut pixels = tile.bytes().ok_or(tr!("the-layer-has-no-pixels"))?.to_vec();
    let alpha = shape.bytes().ok_or(tr!("the-mask-has-no-pixels"))?;
    for (pixel, cover) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(alpha.as_chunks::<4>().0)
    {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(cover[3]) + 127) / 255) as u8;
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
