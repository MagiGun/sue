//! Cels as Clip Studio Paint keeps them: a cel is any layer right in an
//! animation folder, a layer folder too (a folder cel shows and hides as
//! a whole); a new cel takes the folder structure of the cel before it,
//! a cel made between two is named like `1a`, and the cels can be named
//! again in timeline or layer order.

use std::collections::HashSet;

use sue_core::txn::{Relate, Rename, SetExtension, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, OrderKey, RasterTile2D, Relation};
use sue_model::Sequence;

use super::{Edit, assign, cel_at, cels, keys, length, timeline, track_of};
use crate::composite::{FrameView, Skin};
use crate::doc::MEMBER;

/// `view` with every folder cel it hides or shows faintly taken down to
/// the layers in it, so a folder cel shows and hides as a whole whatever
/// its blend mode (a Through folder lays its layers on one by one).
#[must_use]
pub fn whole_folders(arena: &Arena, picture: EntityId, mut view: FrameView) -> FrameView {
    let folders: Vec<EntityId> = view
        .hidden
        .iter()
        .copied()
        .filter(|id| crate::doc::is_folder(arena, *id))
        .collect();
    for folder in folders {
        let members = crate::doc::members(arena, picture, folder);
        match view.onion.iter().find(|skin| skin.cel == folder).copied() {
            Some(skin) => {
                // Shown faintly: its layers each take the skin, and the
                // folder itself stays so a folder of its own still shows.
                view.hidden.remove(&folder);
                for member in members {
                    view.hidden.insert(member);
                    if !view.onion.iter().any(|known| known.cel == member) {
                        view.onion.push(Skin {
                            cel: member,
                            ..skin
                        });
                    }
                }
            }
            None => view.hidden.extend(members),
        }
    }
    view
}

/// The cel `track` shows at or before `frame`, else the folder's top cel:
/// the one a new cel follows.
fn cel_before(
    arena: &Arena,
    picture: EntityId,
    folder: EntityId,
    sequence: &Sequence,
    track: usize,
    frame: u32,
) -> Option<EntityId> {
    let keyed = keys(&sequence.tracks[track], sequence.rate)
        .into_iter()
        .rev()
        .filter(|(at, _)| *at <= frame)
        .find_map(|(_, cel)| cel);
    keyed
        .filter(|cel| crate::doc::folder_of(arena, *cel) == Some(folder))
        .or_else(|| cels(arena, picture, folder).last().copied())
}

/// The cel `track` shows next after `frame`, if any.
fn cel_after(sequence: &Sequence, track: usize, frame: u32) -> Option<EntityId> {
    keys(&sequence.tracks[track], sequence.rate)
        .into_iter()
        .filter(|(at, _)| *at > frame)
        .find_map(|(_, cel)| cel)
}

/// The name of a cel made between `before` and the next one: `before`
/// with the first letter from `a` on that no cel in `taken` has (`1` gives
/// `1a`, `1a` gives `1b`).
#[must_use]
pub fn in_between_name(before: &str, taken: &HashSet<String>) -> String {
    // `1a` continues from its letter; any other name starts at `a`.
    let trimmed = before.strip_suffix(|ch: char| ch.is_ascii_lowercase());
    let (stem, from) = match trimmed {
        Some(stem) if stem.ends_with(|ch: char| ch.is_ascii_digit()) => {
            let last = before.chars().last().map_or('a', |last| {
                char::from_u32(u32::from(last) + 1).unwrap_or('{')
            });
            (stem, last)
        }
        _ => (before, 'a'),
    };
    let free = (u32::from(from)..=u32::from('z'))
        .filter_map(char::from_u32)
        .map(|letter| format!("{stem}{letter}"))
        .find(|name| !taken.contains(name));
    free.unwrap_or_else(|| {
        // Every letter is taken: a number after the name.
        (2..)
            .map(|number| format!("{before} {number}"))
            .find(|name| !taken.contains(name))
            .unwrap_or_default()
    })
}

/// `count` order keys between `low` and `high` in the stack, bottom first.
fn keys_between(low: Option<&OrderKey>, high: Option<&OrderKey>, count: usize) -> Vec<OrderKey> {
    let mut out = Vec::with_capacity(count);
    let mut low = low.cloned();
    for _ in 0..count {
        let key = match (&low, high) {
            (Some(below), Some(above)) if above > below => {
                OrderKey::between(Some(below), Some(above))
            }
            (None, Some(above)) => OrderKey::between(None, Some(above)),
            (below, _) => OrderKey::between(below.as_ref(), None),
        };
        out.push(key.clone());
        low = Some(key);
    }
    out
}

/// The spawns of an empty copy of the folder cel `source` (its folders
/// and layers, same names and settings, nothing drawn) at `orders`, in
/// the animation folder `folder`, named `name`; and the copy's id.
fn empty_structure(
    arena: &Arena,
    picture: EntityId,
    folder: EntityId,
    source: EntityId,
    name: &str,
    orders: &mut dyn Iterator<Item = OrderKey>,
) -> Result<(Edit, EntityId), String> {
    let block = crate::doc::with_members(arena, picture, &[source]);
    let [width, height] = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .ok_or_else(|| tr!("the-picture-has-no-size"))?;
    let mut map = std::collections::HashMap::new();
    for id in &block {
        map.insert(*id, EntityId::new());
    }
    let mut edit = Edit::new(tr!("new-animation-cel"));
    for id in &block {
        let original = arena.get_by_id(*id).map_err(|error| error.to_string())?;
        let new = map[id];
        let mut copy = if crate::doc::is_folder(arena, *id) {
            Entity::empty(original.name(), original.class())
        } else {
            let blank = RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8)
                .map_err(|error| error.to_string())?;
            let class = if original.class() == crate::vector::CLASS {
                crate::vector::CLASS
            } else {
                sue_model::class::LAYER
            };
            let mut layer = Entity::new(original.name(), blank).with_class(class);
            if class == crate::vector::CLASS {
                layer = layer.with_ext(sue_model::Geometry::new());
            }
            layer
        }
        .with_id(new);
        for (key, value) in original.properties().iter() {
            // Where its pixels sit and its keyframes stay with the drawing.
            if ![
                sue_model::picture::X,
                sue_model::picture::Y,
                super::motion::KEYFRAMES,
            ]
            .contains(&key)
            {
                copy.properties_mut().set(key, value.clone());
            }
        }
        if *id == source {
            copy.set_name(name);
        }
        let order = orders.next().unwrap_or_default();
        edit = edit.then(
            SpawnEntity::new(copy).with_relation(Relation::new(picture, CHILD, new).ordered(order)),
        );
    }
    for id in &block {
        let into = match crate::doc::folder_of(arena, *id) {
            Some(around) if *id != source => map.get(&around).copied().unwrap_or(folder),
            _ => folder,
        };
        edit = edit.then(Relate::new(Relation::new(into, MEMBER, map[id])));
    }
    Ok((edit, map[&source]))
}

/// The edit that makes a new cel in the animation folder `folder` and
/// shows it from `frame` on the folder's track (New Animation Cel), as
/// Clip Studio Paint does: a folder cel when the cel before it is one
/// (its folders and layers, empty); between two cels on the timeline it
/// is named after the one before (`1a`) and sits right above it, else it
/// takes the next number and goes on top of the folder. Returns the cel.
///
/// # Errors
/// If the picture has no timeline, `folder` has no track, or the picture
/// has no size.
pub fn new_cel(
    arena: &Arena,
    picture: EntityId,
    folder: EntityId,
    frame: u32,
) -> Result<(Edit, EntityId), String> {
    let sequence = timeline(arena, picture)
        .ok_or_else(|| tr!("make-timeline-first-animation-timeline-new"))?;
    let track = track_of(sequence, folder).ok_or_else(|| tr!("not-an-animation-folder"))?;
    let existing = cels(arena, picture, folder);
    let name_of = |cel: EntityId| {
        arena
            .get_by_id(cel)
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default()
    };
    let taken: HashSet<String> = existing.iter().map(|cel| name_of(*cel)).collect();
    let before = cel_before(arena, picture, folder, sequence, track, frame);
    let between = before.filter(|_| cel_after(sequence, track, frame).is_some());
    let name = match between {
        Some(before) => in_between_name(&name_of(before), &taken),
        None => {
            let number = existing
                .iter()
                .filter_map(|cel| name_of(*cel).trim().parse::<u32>().ok())
                .max()
                .unwrap_or(0)
                .max(existing.len() as u32)
                + 1;
            number.to_string()
        }
    };
    // Right above the cel before (and its layers), or on top of the
    // folder's cels: below the folder's own row.
    let stack = crate::doc::layers(arena, picture);
    let position = |id: EntityId| stack.iter().position(|info| info.id == id);
    let (low, high) = match between.and_then(position) {
        Some(at) => (
            Some(&stack[at].order),
            stack.get(at + 1).map(|info| &info.order),
        ),
        None => {
            let at = position(folder).unwrap_or(stack.len());
            (
                at.checked_sub(1).map(|below| &stack[below].order),
                stack.get(at).map(|info| &info.order),
            )
        }
    };
    let length = length(arena, picture);
    let structure = before.filter(|cel| crate::doc::is_folder(arena, *cel));
    let (mut edit, cel) = match structure {
        Some(source) => {
            let count = crate::doc::with_members(arena, picture, &[source]).len();
            let mut orders = keys_between(low, high, count).into_iter();
            empty_structure(arena, picture, folder, source, &name, &mut orders)?
        }
        None => {
            let order = keys_between(low, high, 1).pop().unwrap_or_default();
            let (spawn, cel) = crate::doc::add_raster_layer_at(arena, picture, order, name, None)?;
            (
                Edit::new(tr!("new-animation-cel"))
                    .then(spawn)
                    .then(Relate::new(Relation::new(folder, MEMBER, cel))),
                cel,
            )
        }
    };
    let sequence = assign(sequence, track, frame, Some(cel), picture, length);
    edit = edit.then(SetExtension::set(picture, sequence));
    Ok((edit, cel))
}

/// The edit that names the cels of `folder` 1, 2, 3 in the order the
/// timeline first shows them (Rename in timeline order); the cels it
/// never shows follow in layer order. `None` when no name changes.
#[must_use]
pub fn rename_in_timeline_order(
    arena: &Arena,
    picture: EntityId,
    folder: EntityId,
) -> Option<Edit> {
    let sequence = timeline(arena, picture)?;
    let track = track_of(sequence, folder)?;
    let all = cels(arena, picture, folder);
    let mut order: Vec<EntityId> = Vec::new();
    for (_, cel) in keys(&sequence.tracks[track], sequence.rate) {
        if let Some(cel) = cel
            && all.contains(&cel)
            && !order.contains(&cel)
        {
            order.push(cel);
        }
    }
    order.extend(
        all.iter()
            .filter(|cel| !order.contains(cel))
            .copied()
            .collect::<Vec<_>>(),
    );
    renamed(arena, &order, tr!("renamed-in-timeline-order"))
}

/// The edit that names the cels of `folder` 1, 2, 3 from the bottom of
/// the folder up (Rename in layer order). `None` when no name changes.
#[must_use]
pub fn rename_in_layer_order(arena: &Arena, picture: EntityId, folder: EntityId) -> Option<Edit> {
    renamed(
        arena,
        &cels(arena, picture, folder),
        tr!("renamed-in-layer-order"),
    )
}

/// The edit that names `cels` 1, 2, 3 in order.
fn renamed(arena: &Arena, cels: &[EntityId], label: String) -> Option<Edit> {
    let mut edit = Edit::new(label);
    let mut any = false;
    for (index, cel) in cels.iter().enumerate() {
        let name = (index + 1).to_string();
        if arena
            .get_by_id(*cel)
            .is_ok_and(|entity| entity.name() != name)
        {
            edit = edit.then(Rename::new(*cel, name));
            any = true;
        }
    }
    any.then_some(edit)
}

/// The frame where `track` first shows `cel` (a click on a cel in the
/// Layer palette moves the playhead there).
#[must_use]
pub fn first_frame_of(sequence: &Sequence, track: usize, cel: EntityId) -> Option<u32> {
    keys(sequence.tracks.get(track)?, sequence.rate)
        .into_iter()
        .find(|(_, shown)| *shown == Some(cel))
        .map(|(frame, _)| frame)
}

/// `true` if the cel `cel` of `folder` shows at `frame`.
#[must_use]
pub fn shows_at(sequence: &Sequence, folder: EntityId, cel: EntityId, frame: u32) -> bool {
    track_of(sequence, folder)
        .is_some_and(|track| cel_at(&sequence.tracks[track], sequence.rate, frame) == Some(cel))
}
