//! The brush library: brushes as ordinary entities, so every studio reads
//! them, every edit is a transaction on an undo stack, and the library is
//! saved as an ordinary `.sue` file.
//!
//! The library is a folder entity (the property [`LIBRARY`] marks it)
//! whose children are brush sets, folders of brushes ([`class::BRUSH`]
//! entities carrying a [`BrushPreset`]). A set made for one tool says so
//! in the property [`TOOL`]. Tip and paper pictures are entities of
//! [`PICTURE`] holding the encoded file ([`Encoded`]), named by the hex
//! of its SHA-256, children of the library; a brush refers to each
//! picture it uses (`sue.ref`).
//!
//! A brush or a set is shared as a `.sue` file of its own
//! ([`share_set`], [`share_brush`]), and installed from one ([`install`]).
//! `docs/brush-files.md` describes the files.

use std::collections::BTreeMap;

use sue_core::txn::{Batch, DespawnEntity, Relate, Rename, SetExtension, SpawnEntity, Unrelate};
use sue_core::{Arena, CHILD, Entity, EntityId, REF, Relation, SueFile};
use sue_model::brush::ContentHash;
use sue_model::{BrushPreset, Encoded, class};

use super::sut::{TipPicture, sha256};

/// The property that marks the library's own folder.
pub const LIBRARY: &str = "sue.brush.library";
/// The property of a set made for one tool: the tool's key
/// (`sue_brush::presets::TOOLS`).
pub const TOOL: &str = "sue.brush.tool";
/// The class of a tip or paper picture.
pub const PICTURE: &str = "sue.brush.picture";
/// The name of the library's folder.
pub const LIBRARY_NAME: &str = "Brushes";
/// The name of the sets that hold a person's own sub tools of 0.0.75.
pub const CUSTOM: &str = "Custom";
/// What an edit of the library is called in History.
pub const EDIT_BRUSHES: &str = "Edit brushes";

/// A file that is not a brush file Sue reads.
pub const NOT_BRUSHES: &str = "the file holds no brushes";

/// An id that is the same in every library for the same `what`, so a
/// built-in brush keeps its id from one run to the next.
#[must_use]
pub fn stable_id(what: &str) -> EntityId {
    let hash = sha256(format!("sue.brush.library:{what}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    // A version 4 layout, as every other id has.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    EntityId::from_bytes(bytes)
}

fn hex(hash: &ContentHash) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn set_entity(id: EntityId, name: &str, tool: Option<&str>) -> Entity {
    let set = Entity::empty(name, class::FOLDER).with_id(id);
    match tool {
        Some(tool) => set.with_property(TOOL, tool),
        None => set,
    }
}

fn brush_entity(id: EntityId, preset: BrushPreset) -> Entity {
    Entity::empty(preset.name.clone(), class::BRUSH)
        .with_id(id)
        .with_ext(preset)
}

fn picture_entity(id: EntityId, picture: &TipPicture) -> Entity {
    Entity::empty(hex(&picture.hash()), PICTURE)
        .with_id(id)
        .with_ext(Encoded::new("png", picture.png.clone()))
}

/// The library's folder in `arena`.
#[must_use]
pub fn root(arena: &Arena) -> Option<EntityId> {
    arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| entity.class() == class::FOLDER && entity.property(LIBRARY).is_some())
        .map(Entity::id)
}

/// The library's sets, in order.
#[must_use]
pub fn sets(arena: &Arena) -> Vec<EntityId> {
    let Some(root) = root(arena) else {
        return Vec::new();
    };
    arena
        .relations()
        .children(root)
        .into_iter()
        .filter(|id| {
            arena
                .get_by_id(*id)
                .is_ok_and(|entity| entity.class() == class::FOLDER)
        })
        .collect()
}

/// The brushes of `set`, in order.
#[must_use]
pub fn brushes(arena: &Arena, set: EntityId) -> Vec<EntityId> {
    arena
        .relations()
        .children(set)
        .into_iter()
        .filter(|id| preset(arena, *id).is_some())
        .collect()
}

/// The brush `id`.
#[must_use]
pub fn preset(arena: &Arena, id: EntityId) -> Option<&BrushPreset> {
    arena
        .get_by_id(id)
        .ok()
        .filter(|entity| entity.class() == class::BRUSH)
        .and_then(Entity::ext::<BrushPreset>)
}

/// The tool a set is for.
#[must_use]
pub fn tool(arena: &Arena, set: EntityId) -> Option<&str> {
    arena
        .get_by_id(set)
        .ok()?
        .property(TOOL)
        .and_then(sue_core::Value::as_str)
}

/// The picture entity whose file has `hash`.
#[must_use]
pub fn picture_id(arena: &Arena, hash: &ContentHash) -> Option<EntityId> {
    let name = hex(hash);
    arena
        .with_class(PICTURE)
        .filter_map(|handle| arena.get(handle).ok())
        .find(|entity| entity.name() == name)
        .map(Entity::id)
}

/// The encoded file of the picture with `hash`.
#[must_use]
pub fn picture(arena: &Arena, hash: &ContentHash) -> Option<Vec<u8>> {
    let id = picture_id(arena, hash)?;
    arena
        .get_by_id(id)
        .ok()?
        .ext::<Encoded>()
        .map(|encoded| encoded.bytes.to_vec())
}

/// Sue's own library: one set for each group of each brush tool, every
/// built-in brush in it, every id stable.
#[must_use]
pub fn standard() -> Arena {
    let mut arena = Arena::new();
    let root = stable_id("root");
    // An entity that is new keeps its id, so this cannot fail.
    let _ = arena.insert_keep_id(
        Entity::empty(LIBRARY_NAME, class::FOLDER)
            .with_id(root)
            .with_property(LIBRARY, true),
    );
    let mut made: BTreeMap<String, EntityId> = BTreeMap::new();
    for preset in sue_brush::presets::builtin() {
        let tool = sue_brush::presets::tool_of(&preset)
            .unwrap_or_default()
            .to_owned();
        let what = format!("set:{tool}/{}", preset.group);
        let set = *made.entry(what.clone()).or_insert_with(|| {
            let id = stable_id(&what);
            let _ = arena.insert_keep_id(set_entity(id, &preset.group, Some(&tool)));
            let order = arena.relations().next_order(root, CHILD);
            let _ = arena.relate(Relation::new(root, CHILD, id).ordered(order));
            id
        });
        let id = stable_id(&preset.id);
        let _ = arena.insert_keep_id(brush_entity(id, preset));
        let order = arena.relations().next_order(set, CHILD);
        let _ = arena.relate(Relation::new(set, CHILD, id).ordered(order));
    }
    arena
}

/// The library saved as a `.sue` file.
#[must_use]
pub fn to_file(arena: &Arena) -> Vec<u8> {
    let mut file = SueFile::new();
    file.set_arena(arena);
    file.to_bytes()
}

/// A library or a shared brush file read from a `.sue` file.
///
/// # Errors
/// When the bytes are not a `.sue` file, or it holds no library.
pub fn from_file(bytes: &[u8]) -> Result<Arena, String> {
    let arena = SueFile::from_bytes(bytes)
        .and_then(|file| file.arena(&sue_model::registry()))
        .map_err(|error| error.to_string())?;
    if root(&arena).is_none() {
        return Err(NOT_BRUSHES.to_owned());
    }
    Ok(arena)
}

/// The library a person has: the one saved in `saved`, or else Sue's own
/// with the custom sub tools of 0.0.75 (`item_bank`, the text of
/// `item_bank.txt`) moved in, and whether that move happened. The old
/// file is only read, so it stays as a backup; the move goes on no undo
/// stack.
///
/// # Errors
/// When `saved` is not a library.
pub fn open(saved: Option<&[u8]>, item_bank: Option<&str>) -> Result<(Arena, bool), String> {
    if let Some(bytes) = saved {
        return from_file(bytes).map(|arena| (arena, false));
    }
    let mut arena = standard();
    let moved = item_bank.is_some_and(|text| migrate(&mut arena, text) > 0);
    Ok((arena, moved))
}

/// The 0.0.75 tools by their English labels, and the key of each that
/// paints with a brush.
const OLD_TOOLS: [(&str, &str); 6] = [
    ("Pen", "pen"),
    ("Sketch", "pencil"),
    ("Brush", "brush"),
    ("Airbrush", "airbrush"),
    ("Decoration", "decoration"),
    ("Eraser", "eraser"),
];

/// A person's own sub tools of 0.0.75 (`item_bank.txt`: tool, name,
/// stabilization and brush text, tab separated, one a line) as brushes,
/// each with the tool label its line named. Lines that do not read are
/// left out.
#[must_use]
pub fn from_item_bank(text: &str) -> Vec<(String, BrushPreset)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let (tool, name, level, tip) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            let mut preset = BrushPreset::from_legacy_text(tip);
            preset.name = name.to_owned();
            preset.group = CUSTOM.to_owned();
            preset.input.stabilization = level.parse().ok()?;
            // As a 0.0.75 sub tool was.
            preset.input.by_speed = true;
            preset.input.smart_shape = true;
            Some((tool.to_owned(), preset))
        })
        .collect()
}

/// Moves the custom sub tools of `item_bank` into `arena`: a "Custom"
/// set for each tool they belong to (named by the tool when it is not
/// one that paints), in their order. Returns how many came in.
pub fn migrate(arena: &mut Arena, item_bank: &str) -> usize {
    let Some(root) = root(arena) else {
        return 0;
    };
    let mut count = 0;
    let mut made: BTreeMap<String, EntityId> = BTreeMap::new();
    for (index, (label, preset)) in from_item_bank(item_bank).into_iter().enumerate() {
        let tool = OLD_TOOLS
            .iter()
            .find(|(old, _)| *old == label)
            .map(|(_, key)| *key);
        let set = *made.entry(label.clone()).or_insert_with(|| {
            let id = stable_id(&format!("custom:{label}"));
            let name = if tool.is_some() {
                CUSTOM.to_owned()
            } else {
                format!("{CUSTOM}: {label}")
            };
            let _ = arena.insert_keep_id(set_entity(id, &name, tool));
            let order = arena.relations().next_order(root, CHILD);
            let _ = arena.relate(Relation::new(root, CHILD, id).ordered(order));
            id
        });
        let id = stable_id(&format!("custom:{index}:{label}:{}", preset.name));
        if arena.insert_keep_id(brush_entity(id, preset)).is_ok() {
            let order = arena.relations().next_order(set, CHILD);
            let _ = arena.relate(Relation::new(set, CHILD, id).ordered(order));
            count += 1;
        }
    }
    count
}

// ----- Edits, each one undo step --------------------------------------------

/// Adds a new set named `name` (for `tool`, if any) at the end. Returns
/// the transaction and the new set's id.
#[must_use]
pub fn new_set(arena: &Arena, name: &str, tool: Option<&str>) -> Option<(Batch<Arena>, EntityId)> {
    let root = root(arena)?;
    let set = set_entity(EntityId::new(), name, tool);
    let id = set.id();
    let order = arena.relations().next_order(root, CHILD);
    let batch = Batch::new(EDIT_BRUSHES)
        .then(SpawnEntity::new(set).with_relation(Relation::new(root, CHILD, id).ordered(order)));
    Some((batch, id))
}

/// Steps that add the pictures of `pictures` the library does not hold
/// yet, and the ids of all of them by hash.
fn add_pictures(
    arena: &Arena,
    root: EntityId,
    pictures: &[TipPicture],
    batch: &mut Batch<Arena>,
) -> BTreeMap<ContentHash, EntityId> {
    let mut ids = BTreeMap::new();
    for picture in pictures {
        let hash = picture.hash();
        if ids.contains_key(&hash) {
            continue;
        }
        let id = match picture_id(arena, &hash) {
            Some(id) => id,
            None => {
                let entity = picture_entity(EntityId::new(), picture);
                let id = entity.id();
                batch.push(Box::new(
                    SpawnEntity::new(entity).with_relation(Relation::new(root, CHILD, id)),
                ));
                id
            }
        };
        ids.insert(hash, id);
    }
    ids
}

/// Adds `preset` at the end of `set`, with the pictures it names (those
/// the library lacks are added). Returns the transaction and the new
/// brush's id.
#[must_use]
pub fn add_brush(
    arena: &Arena,
    set: EntityId,
    preset: BrushPreset,
    pictures: &[TipPicture],
) -> Option<(Batch<Arena>, EntityId)> {
    let root = root(arena)?;
    arena.get_by_id(set).ok()?;
    let mut batch = Batch::new(EDIT_BRUSHES);
    let ids = add_pictures(arena, root, pictures, &mut batch);
    let named = super::sut::named_pictures(&preset);
    let brush = brush_entity(EntityId::new(), preset);
    let id = brush.id();
    let order = arena.relations().next_order(set, CHILD);
    let mut spawn =
        SpawnEntity::new(brush).with_relation(Relation::new(set, CHILD, id).ordered(order));
    for hash in named {
        if let Some(picture) = ids.get(&hash).copied().or_else(|| picture_id(arena, &hash)) {
            spawn = spawn.with_relation(Relation::new(id, REF, picture));
        }
    }
    batch.push(Box::new(spawn));
    Some((batch, id))
}

/// Changes brush `id` to `preset` (its name too).
#[must_use]
pub fn edit_brush(arena: &Arena, id: EntityId, preset: BrushPreset) -> Option<Batch<Arena>> {
    let entity = arena.get_by_id(id).ok()?;
    let mut batch = Batch::new(EDIT_BRUSHES);
    if entity.name() != preset.name {
        batch.push(Box::new(Rename::new(id, preset.name.clone())));
    }
    batch.push(Box::new(SetExtension::set(id, preset)));
    Some(batch)
}

/// Renames brush `id`.
#[must_use]
pub fn rename_brush(arena: &Arena, id: EntityId, name: &str) -> Option<Batch<Arena>> {
    let mut preset = preset(arena, id)?.clone();
    preset.name = name.to_owned();
    edit_brush(arena, id, preset)
}

/// Copies brush `id` right after itself, named `name`. Returns the
/// transaction and the copy's id.
#[must_use]
pub fn duplicate_brush(
    arena: &Arena,
    id: EntityId,
    name: &str,
) -> Option<(Batch<Arena>, EntityId)> {
    let mut copy = preset(arena, id)?.clone();
    copy.name = name.to_owned();
    // A copy is a person's own brush: it has no built-in id.
    copy.id.clear();
    let set = arena.relations().parent(id, CHILD)?;
    let at = arena.relations().get(set, CHILD, id)?.order;
    let siblings = arena.relations().children(set);
    let next = siblings
        .iter()
        .position(|sibling| *sibling == id)
        .and_then(|index| siblings.get(index + 1))
        .and_then(|after| arena.relations().get(set, CHILD, *after))
        .map(|relation| relation.order);
    let order = sue_core::OrderKey::between(Some(&at), next.as_ref());
    let brush = brush_entity(EntityId::new(), copy);
    let new = brush.id();
    let mut spawn =
        SpawnEntity::new(brush).with_relation(Relation::new(set, CHILD, new).ordered(order));
    for picture in arena.relations().targets(id, REF) {
        spawn = spawn.with_relation(Relation::new(new, REF, picture));
    }
    Some((Batch::new(EDIT_BRUSHES).then(spawn), new))
}

/// Moves brush `id` to the end of `set`.
#[must_use]
pub fn move_brush(arena: &Arena, id: EntityId, set: EntityId) -> Option<Batch<Arena>> {
    let from = arena.relations().parent(id, CHILD)?;
    arena.get_by_id(set).ok()?;
    let order = arena.relations().next_order(set, CHILD);
    Some(
        Batch::new(EDIT_BRUSHES)
            .then(Unrelate::new(from, CHILD, id))
            .then(Relate::new(Relation::new(set, CHILD, id).ordered(order))),
    )
}

/// Deletes brush `id`. Its pictures stay for the brushes that share them.
#[must_use]
pub fn delete_brush(arena: &Arena, id: EntityId) -> Option<Batch<Arena>> {
    preset(arena, id)?;
    Some(Batch::new(EDIT_BRUSHES).then(DespawnEntity::new(id)))
}

/// Deletes `set` and its brushes.
#[must_use]
pub fn delete_set(arena: &Arena, set: EntityId) -> Option<Batch<Arena>> {
    arena.get_by_id(set).ok()?;
    let mut batch = Batch::new(EDIT_BRUSHES);
    for id in arena.relations().children(set) {
        batch.push(Box::new(DespawnEntity::new(id)));
    }
    batch.push(Box::new(DespawnEntity::new(set)));
    Some(batch)
}

// ----- Sharing ----------------------------------------------------------------

/// A library of its own holding `sets` (each with its brushes) and the
/// pictures they use, as a `.sue` file.
fn share(arena: &Arena, sets: &[(EntityId, Vec<EntityId>)]) -> Option<Vec<u8>> {
    let source = root(arena)?;
    let mut out = Arena::new();
    let top = arena.get_by_id(source).ok()?.clone();
    out.insert_keep_id(top).ok()?;
    for (set, brushes) in sets {
        out.insert_keep_id(arena.get_by_id(*set).ok()?.clone())
            .ok()?;
        let order = out.relations().next_order(source, CHILD);
        out.relate(Relation::new(source, CHILD, *set).ordered(order))
            .ok()?;
        for brush in brushes {
            out.insert_keep_id(arena.get_by_id(*brush).ok()?.clone())
                .ok()?;
            let order = out.relations().next_order(*set, CHILD);
            out.relate(Relation::new(*set, CHILD, *brush).ordered(order))
                .ok()?;
            for picture in arena.relations().targets(*brush, REF) {
                if out.get_by_id(picture).is_err() {
                    out.insert_keep_id(arena.get_by_id(picture).ok()?.clone())
                        .ok()?;
                    out.relate(Relation::new(source, CHILD, picture)).ok()?;
                }
                out.relate(Relation::new(*brush, REF, picture)).ok()?;
            }
        }
    }
    Some(to_file(&out))
}

/// `set` and its brushes as a file a person can share.
#[must_use]
pub fn share_set(arena: &Arena, set: EntityId) -> Option<Vec<u8>> {
    share(arena, &[(set, brushes(arena, set))])
}

/// Brush `id` alone, in a set named as its own, as a file a person can
/// share.
#[must_use]
pub fn share_brush(arena: &Arena, id: EntityId) -> Option<Vec<u8>> {
    let set = arena.relations().parent(id, CHILD)?;
    share(arena, &[(set, vec![id])])
}

/// Installs the sets of a shared brush file (`bytes`) at the end of the
/// library: new sets, new brushes, and the pictures the library lacks,
/// all with ids of their own, as one undo step. Returns the step and the
/// new sets' ids.
///
/// # Errors
/// When the file is not a brush file, or the library has no folder.
pub fn install(arena: &Arena, bytes: &[u8]) -> Result<(Batch<Arena>, Vec<EntityId>), String> {
    let shared = from_file(bytes)?;
    let root = root(arena).ok_or(NOT_BRUSHES)?;
    let mut batch = Batch::new(EDIT_BRUSHES);
    let mut order = arena.relations().next_order(root, CHILD);
    // The pictures, by the ids they had in the file.
    let mut pictures: BTreeMap<EntityId, EntityId> = BTreeMap::new();
    for handle in shared.with_class(PICTURE) {
        let Ok(entity) = shared.get(handle) else {
            continue;
        };
        let Some(encoded) = entity.ext::<Encoded>() else {
            continue;
        };
        let picture = TipPicture {
            png: encoded.bytes.to_vec(),
        };
        let mut ids = add_pictures(arena, root, std::slice::from_ref(&picture), &mut batch);
        if let Some(id) = ids.remove(&picture.hash()) {
            pictures.insert(entity.id(), id);
        }
    }
    let mut new_sets = Vec::new();
    for set in sets(&shared) {
        let Ok(entity) = shared.get_by_id(set) else {
            continue;
        };
        let tool = tool(&shared, set).map(str::to_owned);
        let made = set_entity(EntityId::new(), entity.name(), tool.as_deref());
        let made_id = made.id();
        batch.push(Box::new(SpawnEntity::new(made).with_relation(
            Relation::new(root, CHILD, made_id).ordered(order.clone()),
        )));
        order = sue_core::OrderKey::between(Some(&order), None);
        let mut inner = sue_core::OrderKey::between(None, None);
        for brush in brushes(&shared, set) {
            let Some(preset) = preset(&shared, brush) else {
                continue;
            };
            let made = brush_entity(EntityId::new(), preset.clone());
            let id = made.id();
            let mut spawn = SpawnEntity::new(made)
                .with_relation(Relation::new(made_id, CHILD, id).ordered(inner.clone()));
            inner = sue_core::OrderKey::between(Some(&inner), None);
            for picture in shared.relations().targets(brush, REF) {
                if let Some(target) = pictures.get(&picture) {
                    spawn = spawn.with_relation(Relation::new(id, REF, *target));
                }
            }
            batch.push(Box::new(spawn));
        }
        new_sets.push(made_id);
    }
    if new_sets.is_empty() {
        return Err(NOT_BRUSHES.to_owned());
    }
    Ok((batch, new_sets))
}

/// The pictures of `preset`'s brush entity, as the `.sut` exporter wants
/// them: the encoded file of each hash.
pub fn pictures_of(arena: &Arena) -> impl Fn(&ContentHash) -> Option<Vec<u8>> + '_ {
    move |hash| picture(arena, hash)
}
