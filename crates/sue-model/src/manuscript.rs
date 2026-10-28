//! Manuscripts: the shape every long piece of writing agrees on. A novel, a
//! thesis, a screenplay, a song book or a manual is an entity of class
//! [`crate::class::MANUSCRIPT`] whose ordered `sue.child` relations make a
//! binder: folders ([`crate::class::FOLDER`]) and rich text documents
//! ([`crate::class::DOCUMENT`], a `StringBuffer` with [`crate::TextMarks`]),
//! nested as deep as the work needs. Its top folders have a [`ROLE`]: the
//! draft that is published, research kept beside it (any entity: pictures,
//! sounds, web pages), and a trash. Every document carries its synopsis,
//! notes, status, label and word target as shared properties, so a writing
//! studio, an outliner, a publishing layout and an e-book exporter all read
//! the same thing.

use sue_core::{Arena, CHILD, Entity, EntityId, Value};

use crate::class;

/// A top folder's role (`Text`): one of [`role`].
pub const ROLE: &str = "manuscript.role";
/// A short summary, shown on index cards (`Text`).
pub const SYNOPSIS: &str = "doc.synopsis";
/// Free notes about a document (`Text`).
pub const NOTES: &str = "doc.notes";
/// Where a document stands, e.g. `First Draft` (`Text`).
pub const STATUS: &str = "doc.status";
/// A colored label, e.g. `Concept` (`Text`).
pub const LABEL: &str = "doc.label";
/// `false` leaves a document out of what is published (`Bool`).
pub const INCLUDE: &str = "doc.include";
/// The words a document (or, on the manuscript, the draft) aims for (`Int`).
pub const TARGET: &str = "doc.target_words";
/// Comma-separated keywords (`Text`).
pub const KEYWORDS: &str = "doc.keywords";
/// The title the work is published under (`Text`).
pub const TITLE: &str = "doc.title";
/// Who wrote it (`Text`).
pub const AUTHOR: &str = "doc.author";

/// The roles of a manuscript's top folders.
pub mod role {
    /// The work itself, in reading order.
    pub const DRAFT: &str = "draft";
    /// Material kept beside the work.
    pub const RESEARCH: &str = "research";
    /// What was thrown away, kept until emptied.
    pub const TRASH: &str = "trash";
}

/// `true` if `entity` is a manuscript.
#[must_use]
pub fn is_manuscript(entity: &Entity) -> bool {
    entity.class() == class::MANUSCRIPT
}

/// `true` if `entity` is a rich text document.
#[must_use]
pub fn is_document(entity: &Entity) -> bool {
    entity.class() == class::DOCUMENT && entity.string_buffer().is_some()
}

/// `true` if `entity` is a folder.
#[must_use]
pub fn is_folder(entity: &Entity) -> bool {
    entity.class() == class::FOLDER
}

/// The children of `id`, in order.
#[must_use]
pub fn children(arena: &Arena, id: EntityId) -> Vec<EntityId> {
    arena.relations().children(id)
}

/// The parent of `id` in the binder.
#[must_use]
pub fn parent(arena: &Arena, id: EntityId) -> Option<EntityId> {
    arena.relations().parent(id, CHILD)
}

/// The top folder of `manuscript` with role `role`.
#[must_use]
pub fn folder(arena: &Arena, manuscript: EntityId, role: &str) -> Option<EntityId> {
    children(arena, manuscript).into_iter().find(|child| {
        arena
            .get_by_id(*child)
            .ok()
            .and_then(|entity| entity.property(ROLE))
            .and_then(Value::as_str)
            == Some(role)
    })
}

/// The manuscript `id` belongs to (itself, if it is one).
#[must_use]
pub fn manuscript_of(arena: &Arena, id: EntityId) -> Option<EntityId> {
    let mut at = id;
    // A binder deeper than this is a cycle.
    for _ in 0..256 {
        if arena.get_by_id(at).is_ok_and(is_manuscript) {
            return Some(at);
        }
        at = parent(arena, at)?;
    }
    None
}

/// Everything under `root`, depth first in binder order, with its depth
/// (`root`'s children are at depth 0).
#[must_use]
pub fn walk(arena: &Arena, root: EntityId) -> Vec<(EntityId, usize)> {
    let mut out = Vec::new();
    let mut stack: Vec<(EntityId, usize)> = children(arena, root)
        .into_iter()
        .rev()
        .map(|child| (child, 0))
        .collect();
    while let Some((id, depth)) = stack.pop() {
        if out.len() > 1_000_000 {
            break;
        }
        out.push((id, depth));
        stack.extend(
            children(arena, id)
                .into_iter()
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
    out
}

/// `true` unless `entity` is left out of what is published.
#[must_use]
pub fn included(entity: &Entity) -> bool {
    entity.property(INCLUDE).and_then(Value::as_bool) != Some(false)
}

/// What is published from `root`, in reading order: each document and
/// folder that is included (with everything under an excluded folder left
/// out too), with its depth.
#[must_use]
pub fn publishable(arena: &Arena, root: EntityId) -> Vec<(EntityId, usize)> {
    let mut out = Vec::new();
    let mut skip_below: Option<usize> = None;
    for (id, depth) in walk(arena, root) {
        if skip_below.is_some_and(|limit| depth > limit) {
            continue;
        }
        skip_below = None;
        let Ok(entity) = arena.get_by_id(id) else {
            continue;
        };
        if !included(entity) {
            skip_below = Some(depth);
            continue;
        }
        if is_document(entity) || is_folder(entity) {
            out.push((id, depth));
        }
    }
    out
}

/// The words in document `id` (0 for anything else).
#[must_use]
pub fn words(arena: &Arena, id: EntityId) -> usize {
    arena
        .get_by_id(id)
        .ok()
        .and_then(Entity::string_buffer)
        .map_or(0, |text| crate::text::word_count(text.as_str()))
}

/// The words in every document under `root` (and `root` itself).
#[must_use]
pub fn total_words(arena: &Arena, root: EntityId) -> usize {
    words(arena, root)
        + walk(arena, root)
            .into_iter()
            .map(|(id, _)| words(arena, id))
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Relation, StringBuffer};

    fn add(arena: &mut Arena, parent: EntityId, entity: Entity) -> EntityId {
        let id = entity.id();
        arena.insert_keep_id(entity).unwrap();
        let order = arena.relations().next_order(parent, CHILD);
        arena
            .relate(Relation::new(parent, CHILD, id).ordered(order))
            .unwrap();
        id
    }

    fn text(name: &str, body: &str) -> Entity {
        Entity::new(name, StringBuffer::from(body)).with_class(class::DOCUMENT)
    }

    #[test]
    fn a_binder_walks_in_reading_order_and_publishes_what_is_included() {
        let mut arena = Arena::new();
        let book = Entity::empty("Novel", class::MANUSCRIPT);
        let book_id = book.id();
        arena.insert_keep_id(book).unwrap();
        let draft = add(
            &mut arena,
            book_id,
            Entity::empty("Manuscript", class::FOLDER).with_property(ROLE, role::DRAFT),
        );
        let research = add(
            &mut arena,
            book_id,
            Entity::empty("Research", class::FOLDER).with_property(ROLE, role::RESEARCH),
        );
        let one = add(
            &mut arena,
            draft,
            Entity::empty("Chapter One", class::FOLDER),
        );
        let scene = add(&mut arena, one, text("Scene", "It was a dark night."));
        let cut = add(
            &mut arena,
            draft,
            Entity::empty("Cut", class::FOLDER).with_property(INCLUDE, false),
        );
        add(&mut arena, cut, text("Old", "never seen"));
        let two = add(&mut arena, draft, text("Chapter Two", "The end."));
        add(&mut arena, research, text("Notes", "one two three"));

        assert_eq!(folder(&arena, book_id, role::DRAFT), Some(draft));
        assert_eq!(folder(&arena, book_id, role::TRASH), None);
        assert_eq!(manuscript_of(&arena, scene), Some(book_id));
        let order: Vec<_> = walk(&arena, draft).into_iter().map(|(id, _)| id).collect();
        assert_eq!(order[0], one);
        assert_eq!(order[1], scene);
        assert_eq!(walk(&arena, draft)[1].1, 1);
        let published: Vec<_> = publishable(&arena, draft)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(published, vec![one, scene, two]);
        assert_eq!(total_words(&arena, draft), 5 + 2 + 2);
        assert_eq!(words(&arena, two), 2);
    }
}
