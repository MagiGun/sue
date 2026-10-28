//! Pictures in the arena and the undoable edits Sue Paint makes to them.
//!
//! Every picture is the shared layered-picture shape of
//! [`sue_model::picture`]: an entity of class `sue.image`, its layers as
//! ordered children. Each function here builds a transaction from the
//! backbone's generic edits ([`SpawnEntity`], [`DespawnEntity`],
//! [`SetProperty`], [`Relate`], [`RasterEdit`]) by entity id; the studio
//! applies it through the global undo history.

use sue_core::txn::{
    Batch, DespawnEntity, RasterEdit, RasterPatch, Relate, SetProperty, SpawnEntity,
};
use sue_core::{
    Arena, CHILD, Color, ComponentData, ComponentKind, Entity, EntityId, OrderKey, PixelFormat,
    RasterTile2D, Relation, Value,
};
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

use crate::composite::{RenderOptions, render};

/// An edit of a picture: several arena steps undone as one.
pub type Edit = Batch<Arena>;

/// The largest side of a new picture, in pixels.
pub const MAX_SIDE: u32 = 10_000;

/// What a new picture looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPicture {
    /// Its name.
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Print resolution in pixels per inch.
    pub dpi: f64,
    /// The paper color, or `None` for a clear background.
    pub paper: Option<Color>,
}

impl Default for NewPicture {
    fn default() -> Self {
        Self {
            name: "Illustration".to_owned(),
            width: 1600,
            height: 1200,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
        }
    }
}

/// The edit that creates a picture with its paper and one empty layer, and
/// the picture's id.
///
/// # Errors
/// If the size is zero or larger than [`MAX_SIDE`].
pub fn create(spec: &NewPicture) -> Result<(Edit, EntityId), String> {
    if spec.width == 0 || spec.height == 0 || spec.width > MAX_SIDE || spec.height > MAX_SIDE {
        return Err(format!(
            "A picture must be 1 to {MAX_SIDE} pixels a side, not {} by {}",
            spec.width, spec.height
        ));
    }
    let image = Entity::empty(spec.name.clone(), class::IMAGE)
        .with_property(picture::WIDTH, i64::from(spec.width))
        .with_property(picture::HEIGHT, i64::from(spec.height))
        .with_property(picture::DPI, spec.dpi);
    let id = image.id();
    let mut edit = Edit::new(format!("New {}", spec.name)).then(SpawnEntity::new(image));
    let mut keys = OrderKey::sequence(None, 2).into_iter();
    if let Some(color) = spec.paper {
        let paper = Entity::empty("Paper", class::PAPER).with_property(picture::COLOR, color);
        let order = keys.next().unwrap_or_default();
        let paper_id = paper.id();
        edit = edit.then(
            SpawnEntity::new(paper)
                .with_relation(Relation::new(id, CHILD, paper_id).ordered(order)),
        );
    }
    let raster = RasterTile2D::new(spec.width, spec.height, PixelFormat::Rgba8)
        .map_err(|error| error.to_string())?;
    let layer = Entity::new("Layer 1", raster).with_class(class::LAYER);
    let layer_id = layer.id();
    let order = keys.next().unwrap_or_default();
    edit = edit.then(
        SpawnEntity::new(layer).with_relation(Relation::new(id, CHILD, layer_id).ordered(order)),
    );
    Ok((edit, id))
}

/// One layer as the panels show it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInfo {
    /// The layer entity.
    pub id: EntityId,
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: LayerKind,
    /// How it is laid over the layers below.
    pub style: LayerStyle,
    /// Locked against edits.
    pub locked: bool,
    /// Painting keeps its alpha.
    pub alpha_locked: bool,
    /// Its arena revision.
    pub revision: u64,
    /// Its place among its siblings.
    pub order: OrderKey,
}

/// The layers of `picture`, bottom first.
#[must_use]
pub fn layers(arena: &Arena, picture: EntityId) -> Vec<LayerInfo> {
    picture::layers(arena, picture)
        .into_iter()
        .filter_map(|id| {
            let entity = arena.get_by_id(id).ok()?;
            Some(LayerInfo {
                id,
                name: entity.name().to_owned(),
                kind: LayerKind::of(entity),
                style: LayerStyle::of(entity),
                locked: picture::is_locked(entity),
                alpha_locked: picture::is_alpha_locked(entity),
                revision: entity.revision(),
                order: arena.relations().get(picture, CHILD, id)?.order,
            })
        })
        .collect()
}

/// The pixels of a raster layer.
#[must_use]
pub fn raster(arena: &Arena, layer: EntityId) -> Option<&RasterTile2D> {
    arena
        .get_by_id(layer)
        .ok()?
        .component(ComponentKind::RasterTile2D)?
        .as_raster()
}

/// The pixels of a raster layer, to paint in place (this marks it changed).
pub fn raster_mut(arena: &mut Arena, layer: EntityId) -> Option<&mut RasterTile2D> {
    match arena
        .get_by_id_mut(layer)
        .ok()?
        .component_mut(ComponentKind::RasterTile2D)?
    {
        ComponentData::RasterTile2D(raster) => Some(raster),
        _ => None,
    }
}

/// A name like `Layer 3` that no layer of `picture` has yet.
#[must_use]
pub fn new_layer_name(arena: &Arena, picture: EntityId) -> String {
    let highest = layers(arena, picture)
        .iter()
        .filter_map(|layer| layer.name.strip_prefix("Layer ")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("Layer {}", highest + 1)
}

/// The order key for a new layer right above `below` (or at the top).
fn key_above(arena: &Arena, picture: EntityId, below: Option<EntityId>) -> OrderKey {
    let stack = layers(arena, picture);
    let Some(index) = below.and_then(|id| stack.iter().position(|layer| layer.id == id)) else {
        return arena.relations().next_order(picture, CHILD);
    };
    let low = &stack[index].order;
    match stack.get(index + 1) {
        Some(next) if next.order > *low => OrderKey::between(Some(low), Some(&next.order)),
        _ => OrderKey::between(Some(low), None),
    }
}

/// The size of a picture.
fn size_of(arena: &Arena, picture: EntityId) -> Result<[u32; 2], String> {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(picture::size)
        .ok_or_else(|| "The picture has no size".to_owned())
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
    let paper = Entity::empty("Paper", class::PAPER).with_property(picture::COLOR, color);
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
    copy.set_name(format!("{} copy", source.name()));
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

/// Moves `layer` to position `index` of the stack (0 is the bottom).
#[must_use]
pub fn reorder(arena: &Arena, picture: EntityId, layer: EntityId, index: usize) -> Option<Relate> {
    let stack: Vec<LayerInfo> = layers(arena, picture);
    let from = stack.iter().position(|info| info.id == layer)?;
    let others: Vec<&LayerInfo> = stack.iter().filter(|info| info.id != layer).collect();
    let index = index.min(others.len());
    if index == from {
        return None;
    }
    let low = index
        .checked_sub(1)
        .and_then(|at| others.get(at))
        .map(|info| &info.order);
    let high = others.get(index).map(|info| &info.order);
    let order = match (low, high) {
        (Some(low), Some(high)) if low >= high => return None,
        _ => OrderKey::between(low, high),
    };
    Some(Relate::new(
        Relation::new(picture, CHILD, layer).ordered(order),
    ))
}

/// Sets one property of a layer or picture.
pub fn set(id: EntityId, key: &str, value: impl Into<Value>) -> SetProperty {
    SetProperty::new(id, key, value)
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
        .ok_or("The layer is not in this picture")?;
    let below = index
        .checked_sub(1)
        .and_then(|at| stack.get(at))
        .filter(|info| info.kind == LayerKind::Raster)
        .ok_or("There is no raster layer below to merge into")?;
    if below.locked {
        return Err(format!("{} is locked", below.name));
    }
    let upper = raster(arena, layer).ok_or("Only raster layers merge")?;
    let lower = raster(arena, below.id).ok_or("Only raster layers merge")?;
    if upper.format() != PixelFormat::Rgba8 || lower.format() != PixelFormat::Rgba8 {
        return Err("Only RGBA layers merge".to_owned());
    }
    let style = stack[index].style;
    let mut edit = Edit::new(format!("Merge {}", stack[index].name));
    if style.visible
        && let Some(patch) = merged_patch(upper, style, lower, below.style.offset)
    {
        edit = edit.then(RasterEdit::new(below.id, "Merge", vec![patch]));
    }
    Ok((edit.then(DespawnEntity::new(layer)), below.id))
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

/// Straight RGBA8 as 0 to 1.
#[must_use]
pub fn unit(pixel: &[u8]) -> [f32; 4] {
    [
        f32::from(pixel[0]) / 255.0,
        f32::from(pixel[1]) / 255.0,
        f32::from(pixel[2]) / 255.0,
        f32::from(pixel[3]) / 255.0,
    ]
}

/// 0 to 1 as straight RGBA8.
#[must_use]
pub fn bytes(color: [f32; 4]) -> [u8; 4] {
    color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
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
        "Layer 1".to_owned()
    } else {
        "Merged".to_owned()
    };
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    let mut edit = Edit::new(if flatten {
        "Flatten Image"
    } else {
        "Merge Visible to New Layer"
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

/// A whole-layer edit: `change` rewrites a copy of the layer's pixels (as
/// straight RGBA8, with the layer's width) and the result becomes one
/// patch over the rectangle that changed.
///
/// # Errors
/// If the layer is not an 8-bit RGBA raster.
pub fn rewrite(
    arena: &Arena,
    layer: EntityId,
    label: &str,
    change: impl FnOnce(&mut [u8], u32, u32),
) -> Result<Option<RasterEdit>, String> {
    let source = raster(arena, layer).ok_or("Only raster layers can change")?;
    if source.format() != PixelFormat::Rgba8 {
        return Err("Only RGBA layers can change".to_owned());
    }
    let (width, height) = (source.width(), source.height());
    let before = source.bytes().ok_or("The layer is not 8-bit")?;
    let mut after = before.to_vec();
    change(&mut after, width, height);
    Ok(diff_patch(before, &after, width, height)
        .map(|patch| RasterEdit::new(layer, label, vec![patch])))
}

/// The smallest patch that turns `before` into `after` (both full layers).
#[must_use]
pub fn diff_patch(before: &[u8], after: &[u8], width: u32, height: u32) -> Option<RasterPatch> {
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..height {
        let row = (y * width * 4) as usize..((y + 1) * width * 4) as usize;
        let (a, b) = (&before[row.clone()], &after[row]);
        if a == b {
            continue;
        }
        let first = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(0) as u32 / 4;
        let last = a.iter().zip(b).rposition(|(x, y)| x != y).unwrap_or(0) as u32 / 4 + 1;
        bounds = Some(match bounds {
            Some([x0, y0, x1, _]) => [x0.min(first), y0, x1.max(last), y + 1],
            None => [first, y, last, y + 1],
        });
    }
    let [x0, y0, x1, y1] = bounds?;
    let cut = |pixels: &[u8]| {
        let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
        for y in y0..y1 {
            let at = ((y * width + x0) * 4) as usize;
            out.extend_from_slice(&pixels[at..at + ((x1 - x0) * 4) as usize]);
        }
        out
    };
    Some(RasterPatch {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
        before: cut(before),
        after: cut(after),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{ReversibleTransaction, UndoStack};

    fn fresh() -> (Arena, EntityId) {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 16,
            height: 8,
            ..NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        (arena, picture)
    }

    #[test]
    fn a_new_picture_has_paper_and_one_layer_and_undoes_whole() {
        let mut arena = Arena::new();
        let (edit, picture) = create(&NewPicture::default()).unwrap();
        let mut history = UndoStack::new();
        history.apply(&mut arena, edit).unwrap();
        let stack = layers(&arena, picture);
        assert_eq!(
            stack
                .iter()
                .map(|layer| layer.name.as_str())
                .collect::<Vec<_>>(),
            ["Paper", "Layer 1"]
        );
        assert_eq!(stack[0].kind, LayerKind::Paper);
        assert!(picture::is_picture(arena.get_by_id(picture).unwrap()));
        history.undo(&mut arena).unwrap();
        assert!(arena.is_empty());
        history.redo(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
        assert!(
            create(&NewPicture {
                width: 0,
                ..NewPicture::default()
            })
            .is_err()
        );
    }

    #[test]
    fn layers_are_added_duplicated_reordered_and_deleted() {
        let (mut arena, picture) = fresh();
        let first = layers(&arena, picture)[1].id;
        assert_eq!(new_layer_name(&arena, picture), "Layer 2");
        let (mut spawn, second) =
            add_raster_layer(&arena, picture, Some(first), "Layer 2".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        // A layer added above the first lands between it and the top.
        let (mut middle, between) =
            add_raster_layer(&arena, picture, Some(first), "Between".to_owned(), None).unwrap();
        middle.apply(&mut arena).unwrap();
        let names = |arena: &Arena| {
            layers(arena, picture)
                .into_iter()
                .map(|layer| layer.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&arena), ["Paper", "Layer 1", "Between", "Layer 2"]);
        let (mut copy, _) = duplicate(&arena, picture, second).unwrap();
        copy.apply(&mut arena).unwrap();
        assert_eq!(names(&arena)[4], "Layer 2 copy");
        let mut down = reorder(&arena, picture, second, 1).unwrap();
        down.apply(&mut arena).unwrap();
        assert_eq!(
            names(&arena),
            ["Paper", "Layer 2", "Layer 1", "Between", "Layer 2 copy"]
        );
        down.revert(&mut arena).unwrap();
        assert_eq!(names(&arena)[3], "Layer 2");
        assert!(
            reorder(&arena, picture, second, 3).is_none(),
            "already there"
        );
        let mut gone = delete(between);
        gone.apply(&mut arena).unwrap();
        assert_eq!(names(&arena).len(), 4);
        gone.revert(&mut arena).unwrap();
        assert_eq!(names(&arena)[2], "Between");
    }

    #[test]
    fn merging_down_lays_the_layer_over_the_one_below() {
        let (mut arena, picture) = fresh();
        let base = layers(&arena, picture)[1].id;
        let (mut spawn, top) =
            add_raster_layer(&arena, picture, Some(base), "Top".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        raster_mut(&mut arena, base).unwrap().bytes_mut().unwrap()[..4]
            .copy_from_slice(&[0, 0, 255, 255]);
        raster_mut(&mut arena, top).unwrap().bytes_mut().unwrap()[..8]
            .copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 128]);
        let (edit, into) = merge_down(&arena, picture, top).unwrap();
        assert_eq!(into, base);
        let mut history = UndoStack::new();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
        let pixels = raster(&arena, base).unwrap().bytes().unwrap();
        assert_eq!(&pixels[..8], &[255, 0, 0, 255, 255, 0, 0, 128]);
        history.undo(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 3);
        assert_eq!(
            &raster(&arena, base).unwrap().bytes().unwrap()[..4],
            &[0, 0, 255, 255]
        );
        // The paper takes no merge.
        assert!(merge_down(&arena, picture, base).is_err());
    }

    #[test]
    fn flattening_and_rewrites_keep_only_what_changed() {
        let (mut arena, picture) = fresh();
        let layer = layers(&arena, picture)[1].id;
        let edit = rewrite(&arena, layer, "Fill", |pixels, width, _| {
            let at = ((2 * width + 3) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[1, 2, 3, 255]);
        })
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                edit.patches()[0].x,
                edit.patches()[0].y,
                edit.patches()[0].width
            ),
            (3, 2, 1)
        );
        assert!(
            rewrite(&arena, layer, "Nothing", |_, _, _| {})
                .unwrap()
                .is_none()
        );
        let (mut flat, merged) = merge_visible(&arena, picture, true).unwrap();
        flat.apply(&mut arena).unwrap();
        let stack = layers(&arena, picture);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].id, merged);
        assert_eq!(
            &raster(&arena, merged).unwrap().bytes().unwrap()[..4],
            &[255, 255, 255, 255],
            "the paper is in the flattened layer"
        );
        flat.revert(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
    }
}
