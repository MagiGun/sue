//! Edits of every picked layer at once, as Clip Studio Paint makes them:
//! a folder goes with what it holds. Duplicate, delete, show, hide, lock,
//! move, merge, rasterize and Create Folder and Insert Layer.

use std::collections::HashMap;

use sue_core::txn::{DespawnEntity, Relate, SetExtension, SetProperty, SpawnEntity, Unrelate};
use sue_core::{Arena, CHILD, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::picture::{self, LayerKind, LayerStyle};
use sue_model::{BlendMode, Sequence, class};

use super::{
    Edit, MASK, MEMBER, folder_of, folders_around, is_folder, keys_above, label, layers, mask_of,
    new_folder, size_of,
};
use crate::composite::{RenderOptions, render};

/// `ids` without the ones inside a folder that is picked too (the folder
/// speaks for them), in stacking order, bottom first.
#[must_use]
pub fn outermost(arena: &Arena, picture: EntityId, ids: &[EntityId]) -> Vec<EntityId> {
    layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| ids.contains(id))
        .filter(|id| {
            !folders_around(arena, *id)
                .iter()
                .any(|folder| ids.contains(folder))
        })
        .collect()
}

/// `ids` with everything in the picked folders, in stacking order.
#[must_use]
pub fn with_members(arena: &Arena, picture: EntityId, ids: &[EntityId]) -> Vec<EntityId> {
    layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| {
            ids.contains(id)
                || folders_around(arena, *id)
                    .iter()
                    .any(|folder| ids.contains(folder))
        })
        .collect()
}

/// The edit that sets `key` to `value` on every picked layer (show, hide,
/// lock, palette color...), named as for one; `None` when it changes
/// nothing.
#[must_use]
pub fn set_layers(
    arena: &Arena,
    ids: &[EntityId],
    key: &str,
    value: impl Into<Value>,
) -> Option<Edit> {
    let value = value.into();
    let mut edit = Edit::new(label(key, &value));
    let mut changed = false;
    for id in ids {
        let Ok(entity) = arena.get_by_id(*id) else {
            continue;
        };
        if entity.property(key) != Some(&value) {
            edit = edit.then(SetProperty::new(*id, key, value.clone()));
            changed = true;
        }
    }
    changed.then_some(edit)
}

/// The edit that removes every picked layer, a folder with what it holds
/// and a layer with its mask (undo brings them all back in place).
///
/// # Errors
/// If nothing would be left of the picture.
pub fn delete_layers(arena: &Arena, picture: EntityId, ids: &[EntityId]) -> Result<Edit, String> {
    let gone = with_members(arena, picture, ids);
    if gone.is_empty() {
        return Err(tr!("layer-not-picture"));
    }
    let left = layers(arena, picture)
        .iter()
        .filter(|info| !gone.contains(&info.id) && !is_folder(arena, info.id))
        .count();
    if left == 0 {
        return Err(tr!("picture-keeps-least-one-layer"));
    }
    let mut edit = Edit::new(if gone.len() == 1 {
        tr!("delete-layer")
    } else {
        tr!("delete-layers")
    });
    // The top first, so undo puts them back from the bottom.
    for id in gone.iter().rev() {
        for attached in attached(arena, *id) {
            edit = edit.then(DespawnEntity::new(attached));
        }
        edit = edit.then(DespawnEntity::new(*id));
    }
    Ok(edit)
}

/// The edit that copies every picked layer right above itself, a folder
/// with all it holds (and an animation folder with its track), each copy
/// in the folder its original is in and with a copy of its mask. Answers
/// the copies of the picked layers, bottom first.
///
/// # Errors
/// If a picked layer is not in the picture.
pub fn duplicate_layers(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Result<(Edit, Vec<EntityId>), String> {
    let picked = outermost(arena, picture, ids);
    if picked.is_empty() {
        return Err(tr!("layer-not-picture"));
    }
    let mut edit = Edit::new(tr!("duplicate-layer"));
    let mut copies = Vec::new();
    let mut timeline = crate::anim::timeline(arena, picture).cloned();
    for top in picked {
        let block = with_members(arena, picture, &[top]);
        let keys = keys_above(arena, picture, Some(top), block.len());
        let mut map: HashMap<EntityId, EntityId> = HashMap::new();
        for id in &block {
            map.insert(*id, EntityId::new());
            for attached in attached(arena, *id) {
                map.insert(attached, EntityId::new());
            }
        }
        for (id, order) in block.iter().zip(keys) {
            let source = arena.get_by_id(*id).map_err(|error| error.to_string())?;
            let new = map[id];
            let mut copy = source.clone().with_id(new);
            if *id == top {
                copy.set_name(tr!("copy", source = source.name()));
            }
            edit = edit.then(
                SpawnEntity::new(copy)
                    .with_relation(Relation::new(picture, CHILD, new).ordered(order)),
            );
            for (kind, attached) in attachments(arena, *id) {
                let source = arena
                    .get_by_id(attached)
                    .map_err(|error| error.to_string())?;
                edit = edit.then(
                    SpawnEntity::new(source.clone().with_id(map[&attached]))
                        .with_relation(Relation::new(new, kind, map[&attached])),
                );
            }
        }
        // Once every copy is there: each in the folder its original is in
        // (the copied folder inside the block, the original's own outside
        // it), with the other ties of its original (a light table, a
        // reference).
        for id in &block {
            let new = map[id];
            if let Some(folder) = folder_of(arena, *id) {
                edit = edit.then(Relate::new(Relation::new(
                    map.get(&folder).copied().unwrap_or(folder),
                    MEMBER,
                    new,
                )));
            }
            for relation in arena.relations().involving(*id) {
                if relation.from != *id
                    || [CHILD, MEMBER, MASK, super::MATERIAL_IMAGE]
                        .contains(&relation.kind.as_str())
                {
                    continue;
                }
                let mut tie = relation.clone();
                tie.from = new;
                tie.to = map.get(&relation.to).copied().unwrap_or(relation.to);
                edit = edit.then(Relate::new(tie));
            }
        }
        if let Some(sequence) = &mut timeline {
            copy_tracks(sequence, &block, &map);
        }
        copies.push(map[&top]);
    }
    if let Some(sequence) = timeline
        && crate::anim::timeline(arena, picture) != Some(&sequence)
    {
        edit = edit.then(SetExtension::set(picture, sequence));
    }
    Ok((edit, copies))
}

/// Adds to `sequence`, right after each, a copy of the track of every
/// animation folder of `block`, standing for that folder's copy and
/// showing the copies of its cels (`map` takes each original to its copy).
fn copy_tracks(sequence: &mut Sequence, block: &[EntityId], map: &HashMap<EntityId, EntityId>) {
    for folder in block {
        let Some(index) = crate::anim::track_of(sequence, *folder) else {
            continue;
        };
        let mut track = sequence.tracks[index].clone();
        track
            .properties
            .set(crate::anim::TRACK_FOLDER, Value::Entity(map[folder]));
        let clips: Vec<sue_model::Clip> = track.clips().to_vec();
        while track.remove(0).is_some() {}
        for mut clip in clips {
            clip.source = map.get(&clip.source).copied().unwrap_or(clip.source);
            track.insert(clip);
        }
        sequence.tracks.insert(index + 1, track);
    }
}

/// The edit that moves every picked layer by `by` picture pixels (Move
/// Layer), the layers in a picked folder too; a mask unlinked from its
/// layer stays. `None` when nothing moves.
#[must_use]
pub fn move_layers(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
    by: [i64; 2],
) -> Option<Edit> {
    if by == [0, 0] {
        return None;
    }
    let mut edit = Edit::new(tr!("move-layer"));
    let mut any = false;
    for id in with_members(arena, picture, ids) {
        let Ok(entity) = arena.get_by_id(id) else {
            continue;
        };
        if is_folder(arena, id) || LayerKind::of(entity) != LayerKind::Raster {
            continue;
        }
        let at = LayerStyle::of(entity).offset;
        edit = edit
            .then(SetProperty::new(id, picture::X, at[0] + by[0]))
            .then(SetProperty::new(id, picture::Y, at[1] + by[1]));
        any = true;
    }
    any.then_some(edit)
}

/// The edit that makes a folder right above the topmost picked layer, in
/// the folder that one is in, and moves every picked layer into it in
/// their order (Create Folder and Insert Layer); a picked folder goes in
/// with what it holds. Answers the new folder.
#[must_use]
pub fn folder_insert(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Option<(Edit, EntityId)> {
    let picked = outermost(arena, picture, ids);
    let top = *picked.last()?;
    let block = with_members(arena, picture, &picked);
    // The folder right above the top one, in its folder (where New Layer
    // Folder puts it with that one picked).
    let stack = layers(arena, picture);
    let folder_key = keys_above(arena, picture, Some(top), 1).pop()?;
    let (mut edit, folder) = new_folder(arena, picture, Some(top), false);
    // The picked layers right under it, in their order, above the last
    // layer under the top one that is not picked.
    let at = stack.iter().position(|info| info.id == top)?;
    let mut low = stack[..at]
        .iter()
        .rev()
        .find(|info| !block.contains(&info.id))
        .map(|info| info.order.clone());
    for info in stack.iter().filter(|info| block.contains(&info.id)) {
        let key = sue_core::OrderKey::between(low.as_ref(), Some(&folder_key));
        edit = edit.then(Relate::new(
            Relation::new(picture, CHILD, info.id).ordered(key.clone()),
        ));
        low = Some(key);
    }
    for id in picked {
        if let Some(outer) = folder_of(arena, id) {
            edit = edit.then(Unrelate::new(outer, MEMBER, id));
        }
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
    }
    Some((
        Edit::new(tr!("create-folder-and-insert-layer")).then(edit),
        folder,
    ))
}

/// `true` if `layer` is clipped to a layer below it that is not among
/// `merged`: the nearest unclipped layer under it in its folder.
fn clipped_outside(arena: &Arena, picture: EntityId, layer: EntityId, merged: &[EntityId]) -> bool {
    let stack = layers(arena, picture);
    let Some(at) = stack.iter().position(|info| info.id == layer) else {
        return false;
    };
    if !stack[at].style.clip {
        return false;
    }
    let folder = folder_of(arena, layer);
    stack[..at]
        .iter()
        .rev()
        .filter(|info| folder_of(arena, info.id) == folder)
        .find(|info| !info.style.clip)
        .is_some_and(|base| !merged.contains(&base.id))
}

/// What the picked layers show together, as the picture shows them
/// without the folders around them: a full-size raster, every effect,
/// mask, mode and folder in it baked.
fn composite_of(arena: &Arena, picture: EntityId, ids: &[EntityId]) -> Option<RasterTile2D> {
    let [width, height] = size_of(arena, picture).ok()?;
    let shown = with_members(arena, picture, ids);
    let picked = outermost(arena, picture, ids);
    // A copy of the picture costs no pixels (they are shared): in it,
    // everything else hides and the picked layers leave their folders.
    let mut alone = arena.clone();
    for info in layers(arena, picture) {
        if !shown.contains(&info.id) {
            let mut hide = SetProperty::new(info.id, picture::VISIBLE, false);
            sue_core::ReversibleTransaction::apply(&mut hide, &mut alone).ok()?;
        }
    }
    for id in &picked {
        if let Some(outer) = folder_of(arena, *id) {
            alone.unrelate(outer, MEMBER, *id);
        }
        // Clipped to a layer that is not merged: its own pixels go in
        // whole, and the merged layer keeps the clip (`merge_picked`).
        if clipped_outside(arena, picture, *id, &shown) {
            let mut unclip = SetProperty::new(*id, picture::CLIP, false);
            sue_core::ReversibleTransaction::apply(&mut unclip, &mut alone).ok()?;
        }
    }
    let pixels = render(
        &alone,
        picture,
        [0, 0, width, height],
        RenderOptions::default(),
    );
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()
}

/// The properties a raster layer made from others keeps of the one it
/// stands for: its name, flags and color, not how it was laid on (the
/// pixels have that baked in) nor its effects.
fn kept_properties(source: &sue_core::Entity) -> Vec<(String, Value)> {
    source
        .properties()
        .iter()
        .filter(|(key, _)| {
            !key.starts_with("effect.")
                && ![
                    picture::OPACITY,
                    picture::BLEND,
                    picture::X,
                    picture::Y,
                    picture::VISIBLE,
                    picture::CLIP,
                    super::OPEN,
                    super::MASK_ON,
                ]
                .contains(key)
        })
        .map(|(key, value)| (key.to_owned(), value.clone()))
        .collect()
}

/// The edit that replaces the picked layers with one raster layer of what
/// they show together (Merge Selected Layers, and Merge on a folder):
/// effects, masks, modes and folders baked. With a folder picked, the
/// result takes the topmost picked one's name and place; else it is the
/// lowest picked layer, as before. Answers the merged layer.
///
/// # Errors
/// If fewer than two layers are picked and none is a folder, or one is
/// locked.
pub fn merge_picked(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Result<(Edit, EntityId), String> {
    let picked = outermost(arena, picture, ids);
    let folders = picked.iter().any(|id| is_folder(arena, *id));
    if picked.len() < 2 && !folders {
        return Err(tr!("pick-two-more-raster-layers-merge"));
    }
    let all = with_members(arena, picture, &picked);
    if let Some(locked) = all
        .iter()
        .filter_map(|id| arena.get_by_id(*id).ok())
        .find(|entity| picture::is_locked(entity))
    {
        return Err(tr!("is-locked", name = locked.name()));
    }
    let tile = composite_of(arena, picture, &picked).ok_or(tr!("the-picture-has-no-size"))?;
    // The lowest picked layer clipped to one that is not merged: the
    // merged layer stays clipped to it, as the lowest one was.
    let keep_clip = picked
        .first()
        .is_some_and(|lowest| clipped_outside(arena, picture, *lowest, &all));
    let label = tr!("merge-selected-layers");
    // A plain raster layer at the bottom takes the pixels in; else a new
    // layer does, in the topmost picked one's place.
    let lowest = picked.first().copied().filter(|id| {
        !folders
            && arena
                .get_by_id(*id)
                .is_ok_and(|entity| entity.class() == class::LAYER)
            && super::raster(arena, *id).is_some()
    });
    let (mut edit, into) = match lowest {
        Some(into) => {
            let edit = Edit::new(&label)
                .then(sue_core::txn::ReplaceComponent::new(into, tile, &label))
                .then(SetProperty::new(into, picture::X, 0_i64))
                .then(SetProperty::new(into, picture::Y, 0_i64))
                .then(SetProperty::new(into, picture::OPACITY, 1.0))
                .then(SetProperty::new(
                    into,
                    picture::BLEND,
                    BlendMode::Normal.name(),
                ))
                .then(SetProperty::new(into, picture::VISIBLE, true))
                .then(SetProperty::new(into, picture::CLIP, keep_clip));
            // Its effects are in the pixels now.
            let effects: Vec<String> = arena
                .get_by_id(into)
                .map(|entity| {
                    entity
                        .properties()
                        .iter()
                        .filter(|(key, _)| key.starts_with("effect."))
                        .map(|(key, _)| key.to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let edit = effects
                .into_iter()
                .fold(edit, |edit, key| edit.then(SetProperty::remove(into, key)));
            (edit, into)
        }
        None => {
            let top = *picked.last().ok_or(tr!("layer-not-picture"))?;
            let (mut edit, id) = raster_in_place_of(arena, picture, top, tile, &label)?;
            if keep_clip {
                edit = edit.then(SetProperty::new(id, picture::CLIP, true));
            }
            (edit, id)
        }
    };
    for id in all.iter().rev() {
        if *id == into {
            continue;
        }
        for attached in attached(arena, *id) {
            edit = edit.then(DespawnEntity::new(attached));
        }
        edit = edit.then(DespawnEntity::new(*id));
    }
    if let Some(mask) = mask_of(arena, into) {
        // Its mask is in the pixels now.
        edit = edit.then(DespawnEntity::new(mask));
    }
    Ok((edit, into))
}

/// The edit that spawns a raster layer holding `tile` right above `layer`,
/// in its folder, with the properties `layer` keeps (see
/// [`kept_properties`]); the caller removes `layer`.
fn raster_in_place_of(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    tile: RasterTile2D,
    label: &str,
) -> Result<(Edit, EntityId), String> {
    let source = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let mut fresh = sue_core::Entity::new(source.name(), tile).with_class(class::LAYER);
    for (key, value) in kept_properties(source) {
        fresh = fresh.with_property(&key, value);
    }
    let id = fresh.id();
    let order = super::key_above(arena, picture, Some(layer));
    let mut spawn =
        SpawnEntity::new(fresh).with_relation(Relation::new(picture, CHILD, id).ordered(order));
    if let Some(folder) = folder_of(arena, layer) {
        spawn = spawn.with_relation(Relation::new(folder, MEMBER, id));
    }
    Ok((Edit::new(label).then(spawn), id))
}

/// `true` if Rasterize makes a difference to `layer`: anything but a
/// plain raster layer with no effect on (a folder, a vector, text, frame,
/// 3D or paper layer, or a raster layer whose effects it bakes).
#[must_use]
pub fn rasterizes(arena: &Arena, layer: EntityId) -> bool {
    let Ok(entity) = arena.get_by_id(layer) else {
        return false;
    };
    if entity.class() == super::CORRECTION {
        return false;
    }
    is_folder(arena, layer)
        || entity.class() != class::LAYER
        || !crate::effects::Effects::of(entity, 72.0).is_plain()
}

/// The edit that turns `layer` into a plain raster layer in its place
/// (Rasterize): any kind of layer, its effects baked into its pixels; a
/// folder becomes one layer of what it shows. Its name, flags, opacity,
/// mode, folder and mask stay. Returns the new layer.
///
/// # Errors
/// If there is nothing to rasterize (see [`rasterizes`]) or the layer is
/// locked.
pub fn rasterize_any(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    if !rasterizes(arena, layer) {
        return Err(tr!("only-vector-layers-rasterize"));
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    if picture::is_locked(entity) {
        return Err(tr!("is-locked", name = entity.name()));
    }
    let folder = is_folder(arena, layer);
    // What the layer itself shows, laid on plainly: its opacity and mode
    // stay properties, its mask stays a mask.
    let mut alone = arena.clone();
    for (key, value) in [
        (picture::OPACITY, Value::from(1.0)),
        (picture::BLEND, Value::from(BlendMode::Normal.name())),
        (picture::VISIBLE, Value::from(true)),
        (picture::CLIP, Value::from(false)),
        (super::MASK_ON, Value::from(false)),
    ] {
        if folder && key == picture::BLEND {
            continue;
        }
        let mut step = SetProperty::new(layer, key, value);
        sue_core::ReversibleTransaction::apply(&mut step, &mut alone)
            .map_err(|error| error.to_string())?;
    }
    if folder && super::folder_blend(arena, layer) != BlendMode::Through {
        // The folder's own mode is how its composite is laid on: kept as
        // the new layer's, and left out of what it shows.
        let mut step = SetProperty::new(layer, picture::BLEND, BlendMode::Normal.name());
        sue_core::ReversibleTransaction::apply(&mut step, &mut alone)
            .map_err(|error| error.to_string())?;
    }
    let tile = composite_of(&alone, picture, &[layer]).ok_or(tr!("the-picture-has-no-size"))?;
    let label = tr!("rasterize");
    let (mut edit, id) = raster_in_place_of(arena, picture, layer, tile, &label)?;
    // How it was laid on, kept.
    let style = LayerStyle::of(entity);
    let blend = if folder {
        super::folder_blend(arena, layer)
    } else {
        style.blend
    };
    let blend = if blend == BlendMode::Through {
        BlendMode::Normal
    } else {
        blend
    };
    edit = edit
        .then(SetProperty::new(
            id,
            picture::OPACITY,
            f64::from(style.opacity),
        ))
        .then(SetProperty::new(id, picture::BLEND, blend.name()))
        .then(SetProperty::new(id, picture::VISIBLE, style.visible))
        .then(SetProperty::new(id, picture::CLIP, style.clip));
    // Its mask goes over to the new layer as it is.
    if let Some(mask) = mask_of(arena, layer) {
        edit = edit
            .then(Unrelate::new(layer, MASK, mask))
            .then(Relate::new(Relation::new(id, MASK, mask)))
            .then(SetProperty::new(
                id,
                super::MASK_ON,
                super::mask_on(arena, layer),
            ));
        // The new layer sits at the picture's corner: the mask keeps its
        // place on the canvas, remade over the canvas like the layer.
        edit = mask_over_canvas(arena, picture, layer, edit)?;
    }
    for id in with_members(arena, picture, &[layer]).iter().rev() {
        // The layer's own mask went over to the new one.
        for attached in attached(arena, *id) {
            if *id != layer || Some(attached) != mask_of(arena, layer) {
                edit = edit.then(DespawnEntity::new(attached));
            }
        }
        edit = edit.then(DespawnEntity::new(*id));
    }
    Ok((edit, id))
}

/// `true` when Merge Down can lay `layer` on the layer below as plain
/// pixels: a raster layer of 8-bit color with no mask and no effect.
pub(super) fn merges_plainly(arena: &Arena, layer: EntityId) -> bool {
    arena.get_by_id(layer).is_ok_and(|entity| {
        !is_folder(arena, layer)
            && mask_of(arena, layer).is_none()
            && crate::effects::Effects::of(entity, 72.0).is_plain()
            && super::raster(arena, layer).is_some_and(|tile| tile.format() == PixelFormat::Rgba8)
    })
}

/// The edit that merges `layer` (a folder with what it holds, or a layer
/// with a mask or effects) into the raster layer `below` it (Merge Down):
/// what `layer` shows, its mask, effects, mode and folders baked, laid
/// over `below`'s own pixels and effects. `below` keeps its id, opacity,
/// mode and mask.
///
/// # Errors
/// If `below` is locked or the picture has no size.
pub(super) fn merge_down_baked(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    below: EntityId,
) -> Result<Edit, String> {
    let entity = arena.get_by_id(below).map_err(|error| error.to_string())?;
    if picture::is_locked(entity) {
        return Err(tr!("is-locked", name = entity.name()));
    }
    let mut alone = arena.clone();
    for (key, value) in [
        (picture::OPACITY, Value::from(1.0)),
        (picture::BLEND, Value::from(BlendMode::Normal.name())),
        (picture::VISIBLE, Value::from(true)),
        (picture::CLIP, Value::from(false)),
        (super::MASK_ON, Value::from(false)),
    ] {
        let mut step = SetProperty::new(below, key, value);
        sue_core::ReversibleTransaction::apply(&mut step, &mut alone)
            .map_err(|error| error.to_string())?;
    }
    let tile =
        composite_of(&alone, picture, &[below, layer]).ok_or(tr!("the-picture-has-no-size"))?;
    let name = arena
        .get_by_id(layer)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    let mut edit = Edit::new(tr!("merge", name = name))
        .then(sue_core::txn::ReplaceComponent::new(
            below,
            tile,
            tr!("merge-2"),
        ))
        .then(SetProperty::new(below, picture::X, 0_i64))
        .then(SetProperty::new(below, picture::Y, 0_i64));
    // The layer moves to the picture's corner and takes its size; its
    // mask stays where it is on the canvas, remade over the canvas so it
    // lies over the layer's pixels as painting a mask expects.
    edit = mask_over_canvas(arena, picture, below, edit)?;
    for (key, _) in entity.properties().iter() {
        if key.starts_with("effect.") {
            edit = edit.then(SetProperty::remove(below, key));
        }
    }
    for id in with_members(arena, picture, &[layer]).iter().rev() {
        for attached in attached(arena, *id) {
            edit = edit.then(DespawnEntity::new(attached));
        }
        edit = edit.then(DespawnEntity::new(*id));
    }
    Ok(edit)
}

/// `edit` followed by remaking `layer`'s mask, if it has one, over the
/// whole canvas at its corner, where a layer made of what the picture
/// shows sits: the mask shows the same on the canvas as before.
fn mask_over_canvas(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    edit: Edit,
) -> Result<Edit, String> {
    let size = size_of(arena, picture)?;
    super::convert::mask_resized(arena, layer, edit, [0, 0], [0, 0], |mask, at| {
        Some((super::convert::placed_cover(mask, at, size), size))
    })
}

/// The entities that belong to `layer` alone and go where it goes (its
/// mask, an image material's image), with the relation that ties each.
fn attachments(arena: &Arena, layer: EntityId) -> Vec<(&'static str, EntityId)> {
    let mut out: Vec<(&'static str, EntityId)> = mask_of(arena, layer)
        .map(|mask| (MASK, mask))
        .into_iter()
        .collect();
    for image in arena.relations().targets(layer, super::MATERIAL_IMAGE) {
        out.push((super::MATERIAL_IMAGE, image));
    }
    out
}

/// The entities of [`attachments`].
fn attached(arena: &Arena, layer: EntityId) -> Vec<EntityId> {
    attachments(arena, layer)
        .into_iter()
        .map(|(_, id)| id)
        .collect()
}
