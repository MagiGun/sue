//! Layer folders: the layers in a folder by relation, folders inside
//! folders, and the edits that make, fill and remove them.

use sue_core::txn::{DespawnEntity, Relate, SpawnEntity, Unrelate};
use sue_core::{Arena, CHILD, Entity, EntityId, Relation, Value};
use sue_model::class;

use super::{Edit, key_above, layers};

/// The relation from a layer folder to each layer in it.
pub const MEMBER: &str = "sue.paint.member";

/// A folder shows its layers in the palette (`Bool`).
pub const OPEN: &str = "layer.open";

/// `true` for a layer folder: a `sue.folder` in a picture's stack.
#[must_use]
pub fn is_folder(arena: &Arena, id: EntityId) -> bool {
    arena
        .get_by_id(id)
        .is_ok_and(|entity| entity.class() == class::FOLDER)
}

/// The folder `layer` is in.
#[must_use]
pub fn folder_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    arena.relations().sources(layer, MEMBER).into_iter().next()
}

/// The folders around `layer`, innermost first.
#[must_use]
pub fn folders_around(arena: &Arena, layer: EntityId) -> Vec<EntityId> {
    let mut out = Vec::new();
    let mut at = folder_of(arena, layer);
    while let Some(folder) = at {
        if out.contains(&folder) || out.len() > 32 {
            break;
        }
        out.push(folder);
        at = folder_of(arena, folder);
    }
    out
}

/// The layers in `folder` (and in the folders in it), in stacking order.
#[must_use]
pub fn members(arena: &Arena, picture: EntityId, folder: EntityId) -> Vec<EntityId> {
    layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| folders_around(arena, *id).contains(&folder))
        .collect()
}

/// `true` if the folder shows its layers in the palette.
#[must_use]
pub fn is_open(arena: &Arena, folder: EntityId) -> bool {
    arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.property(OPEN))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// The edit that makes a folder right above `layer` (or on top), with
/// `layer` in it when `insert` is set, and the folder's id.
#[must_use]
pub fn new_folder(
    arena: &Arena,
    picture: EntityId,
    layer: Option<EntityId>,
    insert: bool,
) -> (Edit, EntityId) {
    let count = layers(arena, picture)
        .iter()
        .filter(|info| is_folder(arena, info.id))
        .count();
    let folder =
        Entity::empty(tr!("folder", value = count + 1), class::FOLDER).with_property(OPEN, true);
    let id = folder.id();
    let order = key_above(arena, picture, layer);
    let mut edit = Edit::new(if insert {
        tr!("create-folder-and-insert-layer")
    } else {
        tr!("new-layer-folder")
    })
    .then(SpawnEntity::new(folder).with_relation(Relation::new(picture, CHILD, id).ordered(order)));
    if let Some(layer) = layer.filter(|_| insert) {
        if let Some(outer) = folder_of(arena, layer) {
            // The new folder takes the layer's place in its folder.
            edit = edit
                .then(Unrelate::new(outer, MEMBER, layer))
                .then(Relate::new(Relation::new(outer, MEMBER, id)));
        }
        edit = edit.then(Relate::new(Relation::new(id, MEMBER, layer)));
    }
    (edit, id)
}

/// The edit that puts `layer` in `folder` (`None`: in no folder).
#[must_use]
pub fn set_folder(arena: &Arena, layer: EntityId, folder: Option<EntityId>) -> Option<Edit> {
    let now = folder_of(arena, layer);
    if now == folder || folder == Some(layer) {
        return None;
    }
    let mut edit = Edit::new(if folder.is_some() {
        tr!("into-folder")
    } else {
        tr!("out-of-folder")
    });
    if let Some(now) = now {
        edit = edit.then(Unrelate::new(now, MEMBER, layer));
    }
    if let Some(folder) = folder {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, layer)));
    }
    Some(edit)
}

/// The edit that removes `folder` and every layer in it.
#[must_use]
pub fn delete_folder(arena: &Arena, picture: EntityId, folder: EntityId) -> Edit {
    let mut edit = Edit::new(tr!("delete-folder"));
    for layer in members(arena, picture, folder) {
        edit = edit.then(DespawnEntity::new(layer));
    }
    edit.then(DespawnEntity::new(folder))
}
