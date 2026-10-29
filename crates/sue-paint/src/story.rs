//! Stories: a comic, a manga or any work of many pages (Clip Studio Paint's
//! Story menu). A story is an entity of class [`STORY`] whose pages are
//! pictures, its ordered `sue.child` relations, first page first. Each page
//! is an ordinary picture, so it paints, saves and sits on the canvas like
//! any other, and every other studio reaches it by its id.

use sue_core::txn::{Batch, DespawnEntity, Relate, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, OrderKey, Relation, Value};
use sue_model::picture;

use crate::doc::{self, NewPicture};

/// The class of a story.
pub const STORY: &str = "sue.paint.story";
/// Which edge the pages are bound on: `right` (manga, read right to left)
/// or `left` (`Text`).
pub const BINDING: &str = "story.binding";

type Edit = Batch<sue_core::Arena>;

/// The story `picture` is a page of.
#[must_use]
pub fn story_of(arena: &Arena, picture: EntityId) -> Option<EntityId> {
    let parent = arena.relations().parent(picture, CHILD)?;
    arena
        .get_by_id(parent)
        .is_ok_and(|entity| entity.class() == STORY)
        .then_some(parent)
}

/// The pages of `story`, first first.
#[must_use]
pub fn pages(arena: &Arena, story: EntityId) -> Vec<EntityId> {
    arena
        .relations()
        .children(story)
        .into_iter()
        .filter(|id| arena.get_by_id(*id).is_ok_and(picture::is_picture))
        .collect()
}

/// `true` if the story is bound on the right (read right to left).
#[must_use]
pub fn bound_right(arena: &Arena, story: EntityId) -> bool {
    arena
        .get_by_id(story)
        .ok()
        .and_then(|entity| entity.property(BINDING))
        .and_then(Value::as_str)
        == Some("right")
}

/// The pages two by two as they face each other, by index: `[left, right]`.
/// The first page faces nothing, on the right of a left-bound book and on
/// the left of a right-bound one.
#[must_use]
pub fn spreads(count: usize, right_bound: bool) -> Vec<[Option<usize>; 2]> {
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    out.push(if right_bound {
        [Some(0), None]
    } else {
        [None, Some(0)]
    });
    let mut index = 1;
    while index < count {
        let (first, second) = (Some(index), (index + 1 < count).then_some(index + 1));
        // The earlier page of a spread sits on the bound side.
        out.push(if right_bound {
            [second, first]
        } else {
            [first, second]
        });
        index += 2;
    }
    out
}

/// The order key that puts a page at `index` among `story`'s pages,
/// leaving `moving` out.
fn order_at(arena: &Arena, story: EntityId, index: usize, moving: Option<EntityId>) -> OrderKey {
    let keys: Vec<OrderKey> = pages(arena, story)
        .into_iter()
        .filter(|id| Some(*id) != moving)
        .filter_map(|id| {
            arena
                .relations()
                .get(story, CHILD, id)
                .map(|relation| relation.order)
        })
        .collect();
    let index = index.min(keys.len());
    let low = index.checked_sub(1).and_then(|at| keys.get(at));
    OrderKey::between(low, keys.get(index))
}

/// The edit that makes a story `name` of `count` new pages like `spec`,
/// the story's id and its pages' ids.
///
/// # Errors
/// If the page size is out of range.
pub fn create(
    spec: &NewPicture,
    count: usize,
    name: &str,
    right_bound: bool,
) -> Result<(Edit, EntityId, Vec<EntityId>), String> {
    let story = Entity::empty(name, STORY)
        .with_property(BINDING, if right_bound { "right" } else { "left" });
    let story_id = story.id();
    let mut edit = Edit::new(format!("New {name}")).then(SpawnEntity::new(story));
    let mut keys = OrderKey::sequence(None, count.max(1)).into_iter();
    let mut ids = Vec::new();
    for number in 1..=count.max(1) {
        let page = NewPicture {
            name: format!("Page {number}"),
            ..spec.clone()
        };
        let (spawn, id) = doc::create(&page)?;
        edit = edit.then(spawn).then(Relate::new(
            Relation::new(story_id, CHILD, id).ordered(keys.next().unwrap_or_default()),
        ));
        ids.push(id);
    }
    Ok((edit, story_id, ids))
}

/// The edit that makes `picture` the first page of a new story, and the
/// story's id.
#[must_use]
pub fn begin(arena: &Arena, picture: EntityId) -> (Edit, EntityId) {
    let name = arena
        .get_by_id(picture)
        .map_or_else(|_| "Story".to_owned(), |entity| entity.name().to_owned());
    let story = Entity::empty(name, STORY).with_property(BINDING, "left");
    let id = story.id();
    let edit = Edit::new("New Story")
        .then(SpawnEntity::new(story))
        .then(Relate::new(
            Relation::new(id, CHILD, picture).ordered(OrderKey::between(None, None)),
        ));
    (edit, id)
}

/// The edit that adds a blank page to `story` at `index`, the size,
/// resolution and paper of `like`, and the page's id.
///
/// # Errors
/// If `like` is not a picture.
pub fn add_page(
    arena: &Arena,
    story: EntityId,
    like: EntityId,
    index: usize,
) -> Result<(Edit, EntityId), String> {
    let entity = arena.get_by_id(like).map_err(|error| error.to_string())?;
    let [width, height] = picture::size(entity).ok_or("That is not a page")?;
    let paper = doc::layers(arena, like)
        .into_iter()
        .find(|info| info.kind == picture::LayerKind::Paper)
        .and_then(|info| arena.get_by_id(info.id).ok())
        .map(picture::paper_color);
    let spec = NewPicture {
        name: format!("Page {}", pages(arena, story).len() + 1),
        width,
        height,
        dpi: entity
            .property(picture::DPI)
            .and_then(Value::as_f64)
            .unwrap_or(72.0),
        paper,
        timelapse: false,
    };
    let (spawn, id) = doc::create(&spec)?;
    let edit = Edit::new("Add Page").then(spawn).then(Relate::new(
        Relation::new(story, CHILD, id).ordered(order_at(arena, story, index, None)),
    ));
    Ok((edit, id))
}

/// The edit that copies `page` (every layer, under new ids) into `story`
/// right after it, and the copy's id.
///
/// # Errors
/// If `page` is gone.
pub fn duplicate_page(
    arena: &Arena,
    story: EntityId,
    page: EntityId,
) -> Result<(Edit, EntityId), String> {
    let source = arena.get_by_id(page).map_err(|error| error.to_string())?;
    let mut copy = source.clone().with_id(EntityId::new());
    copy.set_name(format!("{} copy", source.name()));
    let id = copy.id();
    let index = pages(arena, story)
        .iter()
        .position(|known| *known == page)
        .map_or(usize::MAX, |at| at + 1);
    let mut edit = Edit::new("Duplicate Page").then(SpawnEntity::new(copy));
    for info in doc::layers(arena, page) {
        let layer = arena
            .get_by_id(info.id)
            .map_err(|error| error.to_string())?;
        let layer = layer.clone().with_id(EntityId::new());
        let layer_id = layer.id();
        edit = edit.then(
            SpawnEntity::new(layer)
                .with_relation(Relation::new(id, CHILD, layer_id).ordered(info.order.clone())),
        );
    }
    let edit = edit.then(Relate::new(
        Relation::new(story, CHILD, id).ordered(order_at(arena, story, index, None)),
    ));
    Ok((edit, id))
}

/// The edit that takes `page` and its layers away (undo brings them back).
#[must_use]
pub fn delete_page(arena: &Arena, page: EntityId) -> Edit {
    let mut edit = Edit::new("Delete Page");
    for info in doc::layers(arena, page) {
        edit = edit.then(DespawnEntity::new(info.id));
    }
    edit.then(DespawnEntity::new(page))
}

/// The edit that moves `page` to position `index` of `story`.
#[must_use]
pub fn move_page(arena: &Arena, story: EntityId, page: EntityId, index: usize) -> Option<Edit> {
    let now = pages(arena, story)
        .iter()
        .position(|known| *known == page)?;
    if now == index {
        return None;
    }
    Some(Edit::new("Move Page").then(Relate::new(
        Relation::new(story, CHILD, page).ordered(order_at(arena, story, index, Some(page))),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::ReversibleTransaction;
    use sue_core::UndoStack;

    fn spec() -> NewPicture {
        NewPicture {
            width: 40,
            height: 60,
            ..NewPicture::default()
        }
    }

    #[test]
    fn a_story_keeps_its_pages_in_order_through_every_edit() {
        let mut arena = Arena::new();
        let mut history: UndoStack<Arena> = UndoStack::new();
        let (edit, story, ids) = create(&spec(), 3, "Chapter 1", true).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(pages(&arena, story), ids);
        assert!(bound_right(&arena, story));
        assert_eq!(story_of(&arena, ids[1]), Some(story));
        assert_eq!(doc::layers(&arena, ids[0]).len(), 2, "paper and a layer");

        // Add a page second, duplicate the first, move the last to the front.
        let (edit, added) = add_page(&arena, story, ids[0], 1).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(pages(&arena, story)[1], added);
        let (edit, copy) = duplicate_page(&arena, story, ids[0]).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(pages(&arena, story)[1], copy, "right after its page");
        assert_eq!(doc::layers(&arena, copy).len(), 2);
        let last = *pages(&arena, story).last().unwrap();
        let edit = move_page(&arena, story, last, 0).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(pages(&arena, story)[0], last);
        assert_eq!(pages(&arena, story).len(), 5);

        // Deleting a page takes its layers too; undo brings it all back.
        let layers = doc::layers(&arena, copy).len();
        let entities = arena.len();
        let edit = delete_page(&arena, copy);
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(arena.len(), entities - 1 - layers);
        assert!(!pages(&arena, story).contains(&copy));
        history.undo(&mut arena).unwrap();
        assert!(pages(&arena, story).contains(&copy));
        assert_eq!(doc::layers(&arena, copy).len(), layers);
        // Undo all the way: nothing left.
        while history.undo(&mut arena).unwrap() {}
        assert!(arena.get_by_id(story).is_err());
    }

    #[test]
    fn a_picture_becomes_the_first_page_of_a_new_story() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&spec()).unwrap();
        edit.apply(&mut arena).unwrap();
        assert_eq!(story_of(&arena, picture), None);
        let (mut edit, story) = begin(&arena, picture);
        edit.apply(&mut arena).unwrap();
        assert_eq!(pages(&arena, story), vec![picture]);
        assert!(!bound_right(&arena, story));
    }

    #[test]
    fn spreads_face_each_other_by_binding() {
        // A left-bound book: page 1 alone on the right, then 2 | 3, 4 | 5.
        assert_eq!(
            spreads(5, false),
            vec![[None, Some(0)], [Some(1), Some(2)], [Some(3), Some(4)]]
        );
        // A right-bound manga: page 1 alone on the left, then 3 | 2.
        assert_eq!(
            spreads(4, true),
            vec![[Some(0), None], [Some(2), Some(1)], [None, Some(3)]]
        );
        assert!(spreads(0, true).is_empty());
    }
}
