//! Making, copying, merging and removing layers: raster, vector, paper,
//! text and correction layers, and reference layers.

use sue_core::txn::{DespawnEntity, RasterEdit, RasterPatch, SetProperty, SpawnEntity};
use sue_core::{
    Arena, CHILD, Color, Entity, EntityId, OrderKey, PixelFormat, RasterTile2D, Relation, Value,
};
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

use super::{
    Edit, LayerInfo, bytes, is_folder, key_above, keys_above, layers, raster, size_of, unit,
};
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
    let order = key_above(arena, picture, layer);
    let edit = Edit::new(tr!("new", name = name)).then(
        SpawnEntity::new(correction)
            .with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
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
    add_raster_layer_at(arena, picture, key_above(arena, picture, below), name, fill)
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
        None => RasterTile2D::new(width, height, PixelFormat::Rgba8),
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
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
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
    let keys = keys_above(arena, picture, below, layers_in.len());
    let (mut spawns, mut ids) = (Vec::new(), Vec::new());
    for ((name, raster), order) in layers_in.into_iter().zip(keys) {
        if raster.width() != width || raster.height() != height {
            return Err(tr!("layer-not-pictures-size"));
        }
        let layer = Entity::new(name, raster).with_class(class::LAYER);
        let id = layer.id();
        spawns.push(
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        );
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
    let raster =
        RasterTile2D::new(width, height, PixelFormat::Rgba8).map_err(|error| error.to_string())?;
    let layer = Entity::new(name, raster)
        .with_class(crate::vector::CLASS)
        .with_ext(sue_model::Geometry::new());
    let id = layer.id();
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// The edit that merges raster `ids` of `picture` into the lowest of them
/// (Merge Selected Layers): each laid over the one below as its blend
/// mode and opacity say, hidden ones left out. Returns the merged layer.
///
/// # Errors
/// If fewer than two of them are raster layers, or one is locked.
pub fn merge_layers(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Result<(Edit, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let chosen: Vec<LayerInfo> = layers(arena, picture)
        .into_iter()
        .filter(|info| {
            ids.contains(&info.id) && info.kind == LayerKind::Raster && !is_folder(arena, info.id)
        })
        .collect();
    if chosen.len() < 2 {
        return Err(tr!("pick-two-more-raster-layers-merge"));
    }
    if let Some(locked) = chosen.iter().find(|info| info.locked) {
        return Err(tr!("is-locked", name = locked.name));
    }
    let mut color = vec![[0.0_f32; 4]; (width * height) as usize];
    for info in chosen.iter().filter(|info| info.style.visible) {
        let Some(raster) = raster(arena, info.id) else {
            continue;
        };
        let (Some(bytes), PixelFormat::Rgba8) = (raster.bytes(), raster.format()) else {
            continue;
        };
        let [ox, oy] = info.style.offset;
        for y in 0..raster.height() {
            let ty = i64::from(y) + oy;
            if ty < 0 || ty >= i64::from(height) {
                continue;
            }
            for x in 0..raster.width() {
                let tx = i64::from(x) + ox;
                if tx < 0 || tx >= i64::from(width) {
                    continue;
                }
                let mut source = unit(&bytes[((y * raster.width() + x) * 4) as usize..][..4]);
                source[3] *= info.style.opacity;
                let at = (ty as u32 * width + tx as u32) as usize;
                color[at] = info.style.blend.composite(color[at], source);
            }
        }
    }
    let pixels: Vec<u8> = color.iter().flat_map(|pixel| self::bytes(*pixel)).collect();
    let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    let into = chosen[0].id;
    let mut edit = Edit::new(tr!("merge-selected-layers"))
        .then(sue_core::txn::ReplaceComponent::new(
            into,
            tile,
            tr!("merge-selected-layers"),
        ))
        .then(SetProperty::new(into, picture::X, 0_i64))
        .then(SetProperty::new(into, picture::Y, 0_i64))
        .then(SetProperty::new(into, picture::OPACITY, 1.0))
        .then(SetProperty::new(
            into,
            picture::BLEND,
            sue_model::BlendMode::Normal.name(),
        ))
        .then(SetProperty::new(into, picture::VISIBLE, true));
    for info in &chosen[1..] {
        edit = edit.then(delete(info.id));
    }
    Ok((edit, into))
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
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
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

/// A copy of `layer` right above it, under a new id.
///
/// # Errors
/// If the layer is gone.
pub fn duplicate(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(SpawnEntity, EntityId), String> {
    let source = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let mut copy = source.clone().with_id(EntityId::new());
    copy.set_name(tr!("copy", source = source.name()));
    let id = copy.id();
    let order = key_above(arena, picture, Some(layer));
    Ok((
        SpawnEntity::new(copy).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
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
    let below = index
        .checked_sub(1)
        .and_then(|at| stack.get(at))
        .filter(|info| info.kind == LayerKind::Raster)
        .ok_or(tr!("there-no-raster-layer-below-merge"))?;
    if below.locked {
        return Err(tr!("is-locked", name = below.name));
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
