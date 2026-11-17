//! Making, copying, merging and removing layers: raster, vector, paper,
//! text and correction layers, and reference layers.

use sue_core::txn::{DespawnEntity, RasterEdit, RasterPatch, SpawnEntity};
use sue_core::{
    Arena, CHILD, Color, Entity, EntityId, OrderKey, PixelFormat, RasterTile2D, Relation, Value,
};
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

use super::{Edit, MEMBER, bytes, is_folder, landing, layers, raster, size_of, unit};
use crate::composite::{RenderOptions, render};

// ----- Text layers -------------------------------------------------------------------

/// The class of a text layer.
pub const TEXT: &str = "sue.paint.text";

// ----- Correction layers -------------------------------------------------------------

/// The class of a correction layer: it changes the picture below it.
pub const CORRECTION: &str = "sue.paint.correction";

/// A correction layer's correction (`Text`, `Filter::to_text`).
pub const CORRECTION_FILTER: &str = "correction.filter";

/// The edit that puts a correction layer named `name` doing `text` right
/// above `layer` (or on top), and its id.
#[must_use]
pub fn new_correction(
    arena: &Arena,
    picture: EntityId,
    layer: Option<EntityId>,
    name: &str,
    text: &str,
) -> (Edit, EntityId) {
    let correction = Entity::empty(name, CORRECTION).with_property(CORRECTION_FILTER, text);
    let id = correction.id();
    let edit = Edit::new(tr!("new", name = name)).then(landed(arena, picture, layer, correction));
    (edit, id)
}

/// A new raster layer right above `below` (or at the top), clear or filled
/// with `fill` (straight RGBA).
///
/// # Errors
/// If the picture has no size.
pub fn add_raster_layer(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    fill: Option<[u8; 4]>,
) -> Result<(SpawnEntity, EntityId), String> {
    let (keys, folder) = landing(arena, picture, below, 1);
    let (spawn, id) = add_raster_layer_at(
        arena,
        picture,
        keys.into_iter().next().unwrap_or_default(),
        name,
        fill,
    )?;
    Ok((joined(spawn, folder, id), id))
}

/// `spawn` (of layer `id`) also putting the layer in `folder`, if any.
fn joined(spawn: SpawnEntity, folder: Option<EntityId>, id: EntityId) -> SpawnEntity {
    match folder {
        Some(folder) => spawn.with_relation(Relation::new(folder, MEMBER, id)),
        None => spawn,
    }
}

/// The spawn of `layer` where a new layer goes with `current` picked (see
/// [`landing`]): in its place in the stack and in its folder. Every maker
/// of a new layer spawns it through this.
#[must_use]
pub fn landed(
    arena: &Arena,
    picture: EntityId,
    current: Option<EntityId>,
    layer: Entity,
) -> SpawnEntity {
    let (keys, folder) = landing(arena, picture, current, 1);
    let id = layer.id();
    let order = keys.into_iter().next().unwrap_or_default();
    joined(
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        folder,
        id,
    )
}

/// A new raster layer at `order` in `picture`'s stack, clear or filled
/// with `fill` (straight RGBA).
///
/// # Errors
/// If the picture has no size.
pub fn add_raster_layer_at(
    arena: &Arena,
    picture: EntityId,
    order: OrderKey,
    name: String,
    fill: Option<[u8; 4]>,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster = match fill {
        Some(rgba) => RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            rgba.repeat(width as usize * height as usize),
        ),
        None => RasterTile2D::blank(width, height, PixelFormat::Rgba8),
    }
    .map_err(|error| error.to_string())?;
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// A new raster layer named `name` right above `below` (or at the top),
/// holding `raster` (the picture's size).
///
/// # Errors
/// If `raster` is not the picture's size.
pub fn add_layer_pixels(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    raster: RasterTile2D,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    if raster.width() != width || raster.height() != height {
        return Err(tr!("layer-not-pictures-size"));
    }
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    Ok((landed(arena, picture, below, layer), id))
}

/// New raster layers right above `below` (or at the top), in the order
/// given (the first lowest), each named and holding its raster (the
/// picture's size); their spawns and ids.
///
/// # Errors
/// If a raster is not the picture's size.
pub fn add_layers_pixels(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    layers_in: Vec<(String, RasterTile2D)>,
) -> Result<(Vec<SpawnEntity>, Vec<EntityId>), String> {
    let [width, height] = size_of(arena, picture)?;
    let (keys, folder) = landing(arena, picture, below, layers_in.len());
    let (mut spawns, mut ids) = (Vec::new(), Vec::new());
    for ((name, raster), order) in layers_in.into_iter().zip(keys) {
        if raster.width() != width || raster.height() != height {
            return Err(tr!("layer-not-pictures-size"));
        }
        let layer = Entity::new(name, raster).with_class(class::LAYER);
        let id = layer.id();
        spawns.push(joined(
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
            folder,
            id,
        ));
        ids.push(id);
    }
    Ok((spawns, ids))
}

/// A layer the fill and auto select can refer to (Set as Reference Layer,
/// `Bool`).
pub const REFERENCE: &str = "layer.reference";

/// `true` if `layer` is a reference layer.
#[must_use]
pub fn is_reference(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.property(REFERENCE))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A new vector layer right above `below` (or at the top): clear pixels
/// and no lines yet.
///
/// # Errors
/// If the picture has no size.
pub fn add_vector_layer(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster = RasterTile2D::blank(width, height, PixelFormat::Rgba8)
        .map_err(|error| error.to_string())?;
    let layer = Entity::new(name, raster)
        .with_class(crate::vector::CLASS)
        .with_ext(sue_model::Geometry::new());
    let id = layer.id();
    Ok((landed(arena, picture, below, layer), id))
}

/// The edit that merges the picked `ids` of `picture` (Merge Selected
/// Layers): what they show together, effects, masks, modes and folders
/// baked, in the lowest picked raster layer, or with a folder picked in a
/// new layer in the topmost one's place (see [`super::merge_picked`]).
/// Hidden layers are left out. Returns the merged layer.
///
/// # Errors
/// If fewer than two layers and no folder are picked, or one is locked.
pub fn merge_layers(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Result<(Edit, EntityId), String> {
    super::merge_picked(arena, picture, ids)
}

/// A new vector layer right above `below` (or at the top) holding
/// `lines`, drawn (File, Import, Vector).
///
/// # Errors
/// If the picture has no size.
pub fn add_vector_lines(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    lines: &[crate::vector::Line],
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster =
        crate::vector::render(lines, width, height).ok_or(tr!("the-lines-cannot-be-drawn"))?;
    let layer = Entity::new(name, raster)
        .with_class(crate::vector::CLASS)
        .with_ext(crate::vector::to_geometry(lines));
    let id = layer.id();
    Ok((landed(arena, picture, below, layer), id))
}

/// A paper layer under every other layer.
#[must_use]
pub fn add_paper(arena: &Arena, picture: EntityId, color: Color) -> (SpawnEntity, EntityId) {
    let first = layers(arena, picture)
        .first()
        .map(|layer| layer.order.clone());
    let order = OrderKey::between(None, first.as_ref());
    let paper = Entity::empty(tr!("paper-2"), class::PAPER).with_property(picture::COLOR, color);
    let id = paper.id();
    (
        SpawnEntity::new(paper).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    )
}

/// A copy of `layer` right above it, under a new id, in the folder it is
/// in and with a copy of its mask; a folder's copy holds copies of all it
/// holds (see [`super::duplicate_layers`]).
///
/// # Errors
/// If the layer is gone.
pub fn duplicate(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    let (edit, copies) = super::duplicate_layers(arena, picture, &[layer])?;
    let id = copies.first().copied().ok_or(tr!("layer-not-picture"))?;
    Ok((edit, id))
}

/// Removes a layer (undo brings it back in its place).
#[must_use]
pub fn delete(layer: EntityId) -> DespawnEntity {
    DespawnEntity::new(layer)
}

/// Merges `layer` into the raster layer below it, with its blend mode and
/// opacity, and removes it. Returns the edit and the layer merged into.
///
/// # Errors
/// If there is no raster layer right below, or either is not 8-bit RGBA.
pub fn merge_down(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    let stack = layers(arena, picture);
    let index = stack
        .iter()
        .position(|info| info.id == layer)
        .ok_or(tr!("layer-not-picture"))?;
    // Under a folder is what lies under all it holds.
    let block = super::with_members(arena, picture, &[layer]);
    let lowest = stack
        .iter()
        .position(|info| block.first() == Some(&info.id))
        .unwrap_or(index);
    let below = lowest
        .checked_sub(1)
        .and_then(|at| stack.get(at))
        .filter(|info| info.kind == LayerKind::Raster && !is_folder(arena, info.id))
        .ok_or(tr!("there-no-raster-layer-below-merge"))?;
    if below.locked {
        return Err(tr!("is-locked", name = below.name));
    }
    if !super::picked::merges_plainly(arena, layer) {
        // A folder, or a layer with a mask or effects: baked.
        let edit = super::picked::merge_down_baked(arena, picture, layer, below.id)?;
        return Ok((edit, below.id));
    }
    let upper = raster(arena, layer).ok_or(tr!("only-raster-layers-merge"))?;
    let lower = raster(arena, below.id).ok_or(tr!("only-raster-layers-merge"))?;
    if upper.format() != PixelFormat::Rgba8 || lower.format() != PixelFormat::Rgba8 {
        return Err(tr!("only-rgba-layers-merge"));
    }
    let style = stack[index].style;
    let mut edit = Edit::new(tr!("merge", name = stack[index].name));
    if style.visible
        && let Some(patch) = merged_patch(upper, style, lower, below.style.offset)
    {
        edit = edit.then(RasterEdit::new(below.id, tr!("merge-2"), vec![patch]));
    }
    Ok((edit.then(DespawnEntity::new(layer)), below.id))
}

/// Transfer To Lower Layer: `layer`'s pixels are laid onto the raster
/// layer below it, as Merge Down does, and `layer` stays, emptied. Answers
/// the edit and the layer below.
///
/// # Errors
/// As [`merge_down`].
pub fn transfer_down(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    let stack = layers(arena, picture);
    let index = stack
        .iter()
        .position(|info| info.id == layer)
        .ok_or(tr!("layer-not-picture"))?;
    let below = index
        .checked_sub(1)
        .and_then(|at| stack.get(at))
        .filter(|info| info.kind == LayerKind::Raster)
        .ok_or(tr!("there-no-raster-layer-below-merge"))?;
    if below.locked || stack[index].locked {
        return Err(tr!("is-locked", name = below.name));
    }
    let upper = raster(arena, layer).ok_or(tr!("only-raster-layers-merge"))?;
    let lower = raster(arena, below.id).ok_or(tr!("only-raster-layers-merge"))?;
    if upper.format() != PixelFormat::Rgba8 || lower.format() != PixelFormat::Rgba8 {
        return Err(tr!("only-rgba-layers-merge"));
    }
    let style = stack[index].style;
    let mut edit = Edit::new(tr!("transfer-to-lower-layer"));
    if let Some(patch) = merged_patch(upper, style, lower, below.style.offset) {
        edit = edit.then(RasterEdit::new(below.id, tr!("merge-2"), vec![patch]));
    }
    // The upper layer, emptied.
    let (width, height) = (upper.width(), upper.height());
    if let Some(before) = RasterPatch::read(upper, 0, 0, width, height)
        && before.iter().skip(3).step_by(4).any(|alpha| *alpha != 0)
    {
        let patch = RasterPatch {
            x: 0,
            y: 0,
            width,
            height,
            after: vec![0; before.len()],
            before,
        };
        edit = edit.then(RasterEdit::new(layer, tr!("clear"), vec![patch]));
    }
    Ok((edit, below.id))
}

/// The lower layer's pixels with the upper one laid over them, as a patch
/// covering the upper layer's pixels.
fn merged_patch(
    upper: &RasterTile2D,
    style: LayerStyle,
    lower: &RasterTile2D,
    lower_offset: [i64; 2],
) -> Option<RasterPatch> {
    let ub = upper.bytes()?;
    // Where the upper layer's pixels land in the lower layer.
    let shift = [
        style.offset[0] - lower_offset[0],
        style.offset[1] - lower_offset[1],
    ];
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..upper.height() {
        for x in 0..upper.width() {
            if ub[((y * upper.width() + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            let (lx, ly) = (i64::from(x) + shift[0], i64::from(y) + shift[1]);
            if lx < 0 || ly < 0 || lx >= i64::from(lower.width()) || ly >= i64::from(lower.height())
            {
                continue;
            }
            let (lx, ly) = (lx as u32, ly as u32);
            bounds = Some(match bounds {
                Some([a, b, c, d]) => [a.min(lx), b.min(ly), c.max(lx + 1), d.max(ly + 1)],
                None => [lx, ly, lx + 1, ly + 1],
            });
        }
    }
    let [x0, y0, x1, y1] = bounds?;
    let (width, height) = (x1 - x0, y1 - y0);
    let before = RasterPatch::read(lower, x0, y0, width, height)?;
    let mut after = before.clone();
    for y in 0..height {
        for x in 0..width {
            let (lx, ly) = (i64::from(x0 + x), i64::from(y0 + y));
            let (ux, uy) = (lx - shift[0], ly - shift[1]);
            if ux < 0 || uy < 0 || ux >= i64::from(upper.width()) || uy >= i64::from(upper.height())
            {
                continue;
            }
            let at = ((uy as u32 * upper.width() + ux as u32) * 4) as usize;
            let mut source = unit(&ub[at..at + 4]);
            source[3] *= style.opacity;
            let here = ((y * width + x) * 4) as usize;
            let out = style.blend.composite(unit(&after[here..here + 4]), source);
            after[here..here + 4].copy_from_slice(&bytes(out));
        }
    }
    Some(RasterPatch {
        x: x0,
        y: y0,
        width,
        height,
        before,
        after,
    })
}

/// Composites the visible layers into a new raster layer on top (`flatten`
/// false), or replaces every layer with that one layer (`flatten` true).
///
/// # Errors
/// If the picture has no size.
pub fn merge_visible(
    arena: &Arena,
    picture: EntityId,
    flatten: bool,
) -> Result<(Edit, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let pixels = render(
        arena,
        picture,
        [0, 0, width, height],
        RenderOptions::default(),
    );
    let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    let stack = layers(arena, picture);
    let name = if flatten {
        tr!("layer-number", number = 1)
    } else {
        tr!("merged")
    };
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    let mut edit = Edit::new(if flatten {
        tr!("flatten-image")
    } else {
        tr!("merge-visible-to-new-layer")
    });
    if flatten {
        for info in &stack {
            edit = edit.then(DespawnEntity::new(info.id));
        }
        edit = edit.then(
            SpawnEntity::new(layer)
                .with_relation(Relation::new(picture, CHILD, id).ordered(OrderKey::default())),
        );
    } else {
        let order = arena.relations().next_order(picture, CHILD);
        edit = edit.then(
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        );
    }
    Ok((edit, id))
}
