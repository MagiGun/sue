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
/// A page's bleed beyond its finished size, in millimeters (`Float`).
pub const PAGE_BLEED: &str = "page.bleed";
/// A page's inner border inside its finished size, in millimeters
/// (`Float`).
pub const PAGE_INNER: &str = "page.inner";
/// A page that is two facing pages combined (`Bool`, Combine Pages).
pub const SPREAD: &str = "page.spread";
/// The story's author (`Text`, Change Story Information).
pub const AUTHOR: &str = "story.author";
/// The number the first page has (`Int`).
pub const FIRST_NUMBER: &str = "story.first_number";

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
    let mut edit = Edit::new(tr!("new", name = name)).then(SpawnEntity::new(story));
    let mut keys = OrderKey::sequence(None, count.max(1)).into_iter();
    let mut ids = Vec::new();
    for number in 1..=count.max(1) {
        let page = NewPicture {
            name: tr!("page", number = number),
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
        .map_or_else(|_| tr!("story"), |entity| entity.name().to_owned());
    let story = Entity::empty(name, STORY).with_property(BINDING, "left");
    let id = story.id();
    let edit = Edit::new(tr!("new-story"))
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
    let [width, height] = picture::size(entity).ok_or(tr!("that-is-not-a-page"))?;
    let paper = doc::layers(arena, like)
        .into_iter()
        .find(|info| info.kind == picture::LayerKind::Paper)
        .and_then(|info| arena.get_by_id(info.id).ok())
        .map(picture::paper_color);
    let spec = NewPicture {
        name: tr!("page-3", value = pages(arena, story).len() + 1),
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
    let edit = Edit::new(tr!("add-page")).then(spawn).then(Relate::new(
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
    copy.set_name(tr!("copy", source = source.name()));
    let id = copy.id();
    let index = pages(arena, story)
        .iter()
        .position(|known| *known == page)
        .map_or(usize::MAX, |at| at + 1);
    let mut edit = Edit::new(tr!("duplicate-page")).then(SpawnEntity::new(copy));
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

/// The edit that combines `left` and `right`, two pages of `story`, into
/// one spread (Combine Pages): `left` widens to hold both, `right`'s
/// layers move onto it beside its own (its paper is let go) and `right`
/// leaves the story.
///
/// # Errors
/// If either is not a page, or they are the same page.
pub fn combine(
    arena: &Arena,
    story: EntityId,
    left: EntityId,
    right: EntityId,
) -> Result<Edit, String> {
    if left == right {
        return Err(tr!("pick-two-pages-to-combine"));
    }
    let size = |page: EntityId| {
        arena
            .get_by_id(page)
            .ok()
            .and_then(picture::size)
            .ok_or(tr!("that-is-not-a-page"))
    };
    let ([lw, lh], [rw, rh]) = (size(left)?, size(right)?);
    let mut edit = doc::resize_canvas(arena, left, [lw + rw, lh.max(rh)], [0, 0])?;
    edit = Edit::new(tr!("combine-pages")).then(edit);
    let mut order = arena.relations().next_order(left, CHILD);
    for info in doc::layers(arena, right) {
        if info.kind == picture::LayerKind::Paper {
            edit = edit.then(DespawnEntity::new(info.id));
            continue;
        }
        edit = edit
            .then(sue_core::txn::Unrelate::new(right, CHILD, info.id))
            .then(Relate::new(
                Relation::new(left, CHILD, info.id).ordered(order.clone()),
            ))
            .then(sue_core::txn::SetProperty::new(
                info.id,
                picture::X,
                info.style.offset[0] + i64::from(lw),
            ));
        order = OrderKey::between(Some(&order), None);
    }
    Ok(edit
        .then(sue_core::txn::SetProperty::new(left, SPREAD, true))
        .then(sue_core::txn::Unrelate::new(story, CHILD, right))
        .then(DespawnEntity::new(right)))
}

/// The edit that separates spread `page` of `story` into two pages at its
/// middle (Separate Pages): its left half stays, its right half becomes a
/// new page right after it with the same layers, folders and paper.
/// Returns the new page.
///
/// # Errors
/// If `page` is not a page at least two pixels wide.
pub fn separate(
    arena: &Arena,
    story: EntityId,
    page: EntityId,
) -> Result<(Edit, EntityId), String> {
    let entity = arena.get_by_id(page).map_err(|error| error.to_string())?;
    let [width, height] = picture::size(entity).ok_or(tr!("that-is-not-a-page"))?;
    if width < 2 {
        return Err(tr!("page-too-narrow-separate"));
    }
    let half = width / 2;
    let mut fresh = Entity::empty(tr!("right", entity = entity.name()), entity.class());
    for (key, value) in entity.properties().iter() {
        fresh = fresh.with_property(key, value.clone());
    }
    let fresh = fresh
        .with_property(picture::WIDTH, i64::from(width - half))
        .with_property(SPREAD, false);
    let new_page = fresh.id();
    let index = pages(arena, story)
        .iter()
        .position(|known| *known == page)
        .map_or(usize::MAX, |at| at + 1);
    let mut edit = Edit::new(tr!("separate-pages"))
        .then(SpawnEntity::new(fresh))
        .then(Relate::new(
            Relation::new(story, CHILD, new_page).ordered(order_at(arena, story, index, None)),
        ))
        .then(sue_core::txn::SetProperty::new(
            page,
            picture::WIDTH,
            i64::from(half),
        ))
        .then(sue_core::txn::SetProperty::new(page, SPREAD, false));
    let mut copies: std::collections::HashMap<EntityId, EntityId> =
        std::collections::HashMap::new();
    let stack = doc::layers(arena, page);
    // The part of a layer's pixels between picture columns `from` and `to`.
    let cut = |info: &doc::LayerInfo, from: u32, to: u32| -> Option<sue_core::RasterTile2D> {
        let raster = doc::raster(arena, info.id)?;
        let bytes = raster.bytes()?;
        let w = to - from;
        let mut out = vec![0_u8; (w * height * 4) as usize];
        let [ox, oy] = info.style.offset;
        for y in 0..raster.height() {
            let ty = i64::from(y) + oy;
            if ty < 0 || ty >= i64::from(height) {
                continue;
            }
            for x in 0..raster.width() {
                let tx = i64::from(x) + ox;
                if tx < i64::from(from) || tx >= i64::from(to) {
                    continue;
                }
                let at = ((ty as u32 * w + (tx as u32 - from)) * 4) as usize;
                out[at..at + 4]
                    .copy_from_slice(&bytes[((y * raster.width() + x) * 4) as usize..][..4]);
            }
        }
        sue_core::RasterTile2D::from_bytes(w, height, sue_core::PixelFormat::Rgba8, out).ok()
    };
    for info in &stack {
        let layer = arena
            .get_by_id(info.id)
            .map_err(|error| error.to_string())?;
        let mut copy = match (info.kind, cut(info, half, width)) {
            (picture::LayerKind::Raster, Some(right_part)) => {
                let left_part = cut(info, 0, half).ok_or(tr!("a-layer-will-not-separate"))?;
                edit = edit.then(sue_core::txn::ReplaceComponent::new(
                    info.id,
                    left_part,
                    tr!("separate-pages"),
                ));
                if info.style.offset != [0, 0] {
                    edit = edit
                        .then(sue_core::txn::SetProperty::new(info.id, picture::X, 0_i64))
                        .then(sue_core::txn::SetProperty::new(info.id, picture::Y, 0_i64));
                }
                // The right half is plain pixels: vector lines and text
                // stay with the left page.
                let class = if layer.class() == crate::vector::CLASS
                    || layer.class() == crate::lettering::TEXT
                {
                    sue_model::class::LAYER
                } else {
                    layer.class()
                };
                Entity::new(layer.name(), right_part).with_class(class)
            }
            _ => Entity::empty(layer.name(), layer.class()),
        };
        for (key, value) in layer.properties().iter() {
            if key != picture::X && key != picture::Y {
                copy = copy.with_property(key, value.clone());
            }
        }
        let id = copy.id();
        copies.insert(info.id, id);
        edit = edit.then(
            SpawnEntity::new(copy)
                .with_relation(Relation::new(new_page, CHILD, id).ordered(info.order.clone())),
        );
    }
    for info in &stack {
        if let (Some(folder), Some(copy)) = (doc::folder_of(arena, info.id), copies.get(&info.id))
            && let Some(folder_copy) = copies.get(&folder)
        {
            edit = edit.then(Relate::new(Relation::new(*folder_copy, doc::MEMBER, *copy)));
        }
    }
    Ok((edit, new_page))
}

/// The edit that changes every page of `story` to `size` at `dpi` (each
/// page's pixels kept in the middle) with `bleed` and `inner` borders in
/// millimeters (Change Page Settings).
///
/// # Errors
/// If the size is out of range.
pub fn page_settings(
    arena: &Arena,
    story: EntityId,
    size: [u32; 2],
    dpi: f64,
    bleed: f64,
    inner: f64,
) -> Result<Edit, String> {
    let mut edit = Edit::new(tr!("change-page-settings"));
    for page in pages(arena, story) {
        let Some([w, h]) = arena.get_by_id(page).ok().and_then(picture::size) else {
            continue;
        };
        if [w, h] != size {
            let shift = [
                (i64::from(size[0]) - i64::from(w)) / 2,
                (i64::from(size[1]) - i64::from(h)) / 2,
            ];
            edit = edit.then(doc::resize_canvas(arena, page, size, shift)?);
        }
        edit = edit
            .then(sue_core::txn::SetProperty::new(page, picture::DPI, dpi))
            .then(sue_core::txn::SetProperty::new(page, PAGE_BLEED, bleed))
            .then(sue_core::txn::SetProperty::new(page, PAGE_INNER, inner));
    }
    Ok(edit)
}

/// Text to letter onto pages (Batch Import Text): blocks parted by blank
/// lines, each page's blocks parted from the next page's by a line of
/// `---` or a form feed.
#[must_use]
pub fn text_blocks(text: &str) -> Vec<Vec<String>> {
    let mut pages = vec![Vec::new()];
    let mut block: Vec<&str> = Vec::new();
    let finish = |block: &mut Vec<&str>, page: &mut Vec<String>| {
        if !block.is_empty() {
            page.push(block.join("\n"));
            block.clear();
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "\u{c}" {
            if let Some(page) = pages.last_mut() {
                finish(&mut block, page);
            }
            pages.push(Vec::new());
        } else if trimmed.is_empty() {
            if let Some(page) = pages.last_mut() {
                finish(&mut block, page);
            }
        } else {
            block.push(line.trim_end());
        }
    }
    if let Some(page) = pages.last_mut() {
        finish(&mut block, page);
    }
    while pages.last().is_some_and(Vec::is_empty) && pages.len() > 1 {
        pages.pop();
    }
    pages
}

/// The edit that takes `page` and its layers away (undo brings them back).
#[must_use]
pub fn delete_page(arena: &Arena, page: EntityId) -> Edit {
    let mut edit = Edit::new(tr!("delete-page"));
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
    Some(Edit::new(tr!("move-page")).then(Relate::new(
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
    fn two_pages_combine_into_a_spread_and_separate_again() {
        let mut arena = Arena::new();
        let (mut edit, story, ids) = create(&spec(), 2, "Spread", false).unwrap();
        edit.apply(&mut arena).unwrap();
        // A dot on each page's ink layer.
        let ink = |arena: &Arena, page: EntityId| {
            doc::layers(arena, page)
                .into_iter()
                .find(|info| info.kind == picture::LayerKind::Raster)
                .unwrap()
                .id
        };
        for (page, shade) in [(ids[0], 10_u8), (ids[1], 20_u8)] {
            let layer = ink(&arena, page);
            doc::raster_mut(&mut arena, layer)
                .unwrap()
                .bytes_mut()
                .unwrap()[..4]
                .copy_from_slice(&[shade, 0, 0, 255]);
        }
        let mut combined = combine(&arena, story, ids[0], ids[1]).unwrap();
        combined.apply(&mut arena).unwrap();
        assert_eq!(pages(&arena, story), vec![ids[0]]);
        assert_eq!(
            arena.get_by_id(ids[0]).ok().and_then(picture::size),
            Some([80, 60])
        );
        let flat =
            crate::composite::flatten(&arena, ids[0], crate::composite::RenderOptions::default())
                .unwrap();
        let bytes = flat.bytes().unwrap();
        assert_eq!(bytes[0], 10, "the left page's dot");
        assert_eq!(bytes[40 * 4], 20, "the right page's dot beside it");
        let (mut apart, right) = separate(&arena, story, ids[0]).unwrap();
        apart.apply(&mut arena).unwrap();
        assert_eq!(pages(&arena, story), vec![ids[0], right]);
        assert_eq!(
            arena.get_by_id(right).ok().and_then(picture::size),
            Some([40, 60])
        );
        let flat =
            crate::composite::flatten(&arena, right, crate::composite::RenderOptions::default())
                .unwrap();
        assert_eq!(
            flat.bytes().unwrap()[0],
            20,
            "the dot is back on its own page"
        );
        apart.revert(&mut arena).unwrap();
        combined.revert(&mut arena).unwrap();
        assert_eq!(pages(&arena, story), ids);
    }

    #[test]
    fn text_splits_into_blocks_and_pages() {
        let pages = text_blocks("Hello\nthere\n\nSecond\n---\nNext page\n\n\n");
        assert_eq!(
            pages,
            vec![
                vec!["Hello\nthere".to_owned(), "Second".to_owned()],
                vec!["Next page".to_owned()]
            ]
        );
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
