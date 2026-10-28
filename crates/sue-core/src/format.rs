//! File formats and bundles. Every importer turns bytes into a [`Bundle`]
//! (entities plus the relations between them) and every exporter turns
//! entities back into bytes, through one [`FormatRegistry`]. So any format
//! a feature teaches Sue opens everywhere: drop a `.csv` on the canvas, put
//! it in a slideshow or feed it to a node graph, it is the same entity.
//!
//! Bundles are also the clipboard and the "insert from another document"
//! unit: [`Bundle::copy_from`] lifts entities out of an arena,
//! [`Bundle::renew_ids`] gives the copies fresh ids (rewriting the links
//! between them), and [`InsertBundle`] puts them in as one undo step.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::codec::{CodecError, Reader, Writer};
use crate::{
    Arena, AssetHandle, CHILD, ComponentRegistry, CoreError, Entity, EntityId, Properties,
    PropertyDef, Relation, ReversibleTransaction, TxnError,
};

/// Entities and the relations between them, outside any arena.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bundle {
    /// The entities.
    pub entities: Vec<Entity>,
    /// Relations among them (or to entities the target arena already has).
    pub relations: Vec<Relation>,
}

impl Bundle {
    /// An empty bundle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A bundle of one entity.
    #[must_use]
    pub fn single(entity: Entity) -> Self {
        Self {
            entities: vec![entity],
            relations: Vec::new(),
        }
    }

    /// Adds an entity, returning its id.
    pub fn add(&mut self, entity: Entity) -> EntityId {
        let id = entity.id();
        self.entities.push(entity);
        id
    }

    /// Adds `child` under `parent` (both in the bundle), after its siblings.
    pub fn add_child(&mut self, parent: EntityId, child: Entity) -> EntityId {
        let order = {
            let siblings = self
                .relations
                .iter()
                .filter(|relation| relation.from == parent && relation.kind == CHILD)
                .map(|relation| relation.order.clone())
                .max();
            crate::OrderKey::between(siblings.as_ref(), None)
        };
        let id = self.add(child);
        self.relations
            .push(Relation::new(parent, CHILD, id).ordered(order));
        id
    }

    /// Entities that are no other bundle entity's child, in order.
    #[must_use]
    pub fn roots(&self) -> Vec<EntityId> {
        let children: BTreeSet<EntityId> = self
            .relations
            .iter()
            .filter(|relation| relation.kind == CHILD)
            .map(|relation| relation.to)
            .collect();
        self.entities
            .iter()
            .map(Entity::id)
            .filter(|id| !children.contains(id))
            .collect()
    }

    /// Copies `ids` (and, with `descendants`, everything under them) out of
    /// `arena`, with the relations among the copied entities.
    ///
    /// # Errors
    /// [`CoreError::UnknownId`] for an id that is not live.
    pub fn copy_from(
        arena: &Arena,
        ids: &[EntityId],
        descendants: bool,
    ) -> Result<Self, CoreError> {
        let mut chosen = Vec::new();
        let mut seen = BTreeSet::new();
        let mut queue: Vec<EntityId> = ids.iter().rev().copied().collect();
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            chosen.push(arena.get_by_id(id)?.clone());
            if descendants {
                queue.extend(arena.relations().children(id).into_iter().rev());
            }
        }
        let relations = arena
            .relations()
            .iter()
            .filter(|relation| seen.contains(&relation.from) && seen.contains(&relation.to))
            .collect();
        Ok(Self {
            entities: chosen,
            relations,
        })
    }

    /// Gives every entity a fresh id and rewrites every link that points
    /// inside the bundle (relations, properties, components). Links to
    /// entities outside the bundle are kept. Returns old id to new id.
    pub fn renew_ids(&mut self) -> BTreeMap<EntityId, EntityId> {
        let map: BTreeMap<EntityId, EntityId> = self
            .entities
            .iter()
            .map(|entity| (entity.id(), EntityId::new()))
            .collect();
        let mut remap = |id: EntityId| map.get(&id).copied().unwrap_or(id);
        for entity in &mut self.entities {
            let new = remap(entity.id());
            entity.set_id(new);
            entity.remap_entities(&mut remap);
        }
        for relation in &mut self.relations {
            relation.from = remap(relation.from);
            relation.to = remap(relation.to);
            relation
                .properties
                .values_mut()
                .for_each(|value| value.remap_entities(&mut remap));
        }
        map
    }

    /// Writes the bundle (the clipboard format).
    pub fn encode(&self, out: &mut Writer) {
        out.len(self.entities.len());
        for entity in &self.entities {
            out.nested(|out| entity.encode(out));
        }
        out.len(self.relations.len());
        for relation in &self.relations {
            relation.encode(out);
        }
    }

    /// Reads a bundle.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(
        input: &mut Reader<'_>,
        registry: &ComponentRegistry,
    ) -> Result<Self, CodecError> {
        let count = input.len(1)?;
        let mut entities = Vec::with_capacity(count);
        for _ in 0..count {
            entities.push(Entity::decode(&mut input.nested()?, registry)?);
        }
        let count = input.len(34)?;
        let relations = (0..count)
            .map(|_| Relation::decode(input))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            entities,
            relations,
        })
    }
}

/// Puts a bundle into the arena as one undo step, optionally under a
/// parent. Undo takes it all out again; redo brings back the same buffers.
#[derive(Debug)]
pub struct InsertBundle {
    label: String,
    pending: Vec<(Option<AssetHandle>, Entity)>,
    relations: Vec<Relation>,
    ids: Vec<EntityId>,
}

impl InsertBundle {
    /// Inserts `bundle`.
    pub fn new(label: impl Into<String>, bundle: Bundle) -> Self {
        Self {
            label: label.into(),
            ids: bundle.entities.iter().map(Entity::id).collect(),
            pending: bundle
                .entities
                .into_iter()
                .map(|entity| (None, entity))
                .collect(),
            relations: bundle.relations,
        }
    }

    /// Also puts the bundle's roots under `parent`, after its children.
    #[must_use]
    pub fn under(mut self, arena: &Arena, parent: EntityId) -> Self {
        let roots = {
            let children: BTreeSet<EntityId> = self
                .relations
                .iter()
                .filter(|relation| relation.kind == CHILD)
                .map(|relation| relation.to)
                .collect();
            self.ids
                .iter()
                .copied()
                .filter(|id| !children.contains(id))
                .collect::<Vec<_>>()
        };
        let last = arena
            .relations()
            .children(parent)
            .last()
            .and_then(|child| arena.relations().get(parent, CHILD, *child))
            .map(|relation| relation.order);
        let orders = crate::OrderKey::sequence(last.as_ref(), roots.len());
        for (root, order) in roots.into_iter().zip(orders) {
            self.relations
                .push(Relation::new(parent, CHILD, root).ordered(order));
        }
        self
    }

    /// The ids of the inserted entities.
    #[must_use]
    pub fn ids(&self) -> &[EntityId] {
        &self.ids
    }
}

impl ReversibleTransaction<Arena> for InsertBundle {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        for (_, entity) in &self.pending {
            if entity.id().is_nil() || arena.handle_of(entity.id()).is_some() {
                return Err(CoreError::DuplicateId(entity.id()).into());
            }
        }
        let mut placed = Vec::with_capacity(self.pending.len());
        for (handle, entity) in self.pending.drain(..) {
            let handle = match handle {
                Some(handle) => arena.restore(handle, entity),
                None => arena.insert_keep_id(entity)?,
            };
            placed.push(handle);
        }
        for (done, relation) in self.relations.iter().enumerate() {
            if let Err(error) = arena.relate(relation.clone()) {
                for relation in &self.relations[..done] {
                    arena.unrelate(relation.from, &relation.kind, relation.to);
                }
                for handle in placed.into_iter().rev() {
                    if let Ok(entity) = arena.despawn(handle) {
                        self.pending.insert(0, (Some(handle), entity));
                    }
                }
                return Err(error.into());
            }
        }
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let mut relations: Vec<Relation> = Vec::new();
        let mut taken = Vec::with_capacity(self.ids.len());
        for id in self.ids.iter().rev() {
            let handle = arena.handle_of(*id).ok_or(CoreError::UnknownId(*id))?;
            let (entity, removed) = arena.despawn_with_relations(handle)?;
            for relation in removed {
                let duplicate = relations.iter().any(|known| {
                    known.from == relation.from
                        && known.kind == relation.kind
                        && known.to == relation.to
                });
                if !duplicate {
                    relations.push(relation);
                }
            }
            taken.push((Some(handle), entity));
        }
        taken.reverse();
        self.pending = taken;
        self.relations = relations;
        Ok(())
    }
}

/// Why a file could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError {
    /// A message for the user.
    pub message: String,
}

impl FormatError {
    /// An error with `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FormatError {}

impl From<CodecError> for FormatError {
    fn from(error: CodecError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<CoreError> for FormatError {
    fn from(error: CoreError) -> Self {
        Self::new(error.to_string())
    }
}

/// What a format is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatInfo {
    /// Unique id, e.g. `png`; export presets target `export.<id>`.
    pub id: &'static str,
    /// Name for the UI, e.g. `PNG image`.
    pub name: &'static str,
    /// File extensions, lowercase, without the dot.
    pub extensions: &'static [&'static str],
    /// MIME type.
    pub mime: &'static str,
    /// Group for pickers, e.g. `Image`.
    pub category: &'static str,
}

/// Reads and writes one file format.
pub trait FormatHandler: Send + Sync {
    /// What it is.
    fn info(&self) -> FormatInfo;

    /// `true` if `head` (the first bytes of a file) looks like this format.
    fn sniff(&self, _head: &[u8]) -> bool {
        false
    }

    /// `true` if it can read files.
    fn can_import(&self) -> bool {
        false
    }

    /// Reads a file named `name`.
    ///
    /// # Errors
    /// Why the bytes could not be read.
    fn import(
        &self,
        _bytes: &[u8],
        _name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        Err(FormatError::new(format!(
            "{} files cannot be opened yet",
            self.info().name
        )))
    }

    /// `true` if it can write `entity`.
    fn can_export(&self, _entity: &Entity) -> bool {
        false
    }

    /// Writes the entities `ids`.
    ///
    /// # Errors
    /// Why the entities could not be written.
    fn export(
        &self,
        _arena: &Arena,
        _ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        Err(FormatError::new(format!(
            "{} files cannot be written yet",
            self.info().name
        )))
    }

    /// Import and export options (quality, sample rate, page size), so
    /// presets and dialogs work for every format alike.
    fn options(&self) -> Vec<PropertyDef> {
        Vec::new()
    }
}

/// Every format this build reads or writes.
#[derive(Default)]
pub struct FormatRegistry {
    handlers: Vec<Box<dyn FormatHandler>>,
}

impl fmt::Debug for FormatRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.handlers.iter().map(|handler| handler.info().id))
            .finish()
    }
}

fn extension_of(name: &str) -> Option<String> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (_, extension) = file.rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

impl FormatRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a handler, replacing one with the same id.
    pub fn register(&mut self, handler: impl FormatHandler + 'static) {
        let id = handler.info().id;
        self.handlers.retain(|old| old.info().id != id);
        self.handlers.push(Box::new(handler));
    }

    /// The handler with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&dyn FormatHandler> {
        self.iter().find(|handler| handler.info().id == id)
    }

    /// Every handler, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn FormatHandler> {
        self.handlers.iter().map(AsRef::as_ref)
    }

    /// The first handler for the extension of `name`.
    #[must_use]
    pub fn for_name(&self, name: &str) -> Option<&dyn FormatHandler> {
        let extension = extension_of(name)?;
        self.iter()
            .find(|handler| handler.info().extensions.contains(&extension.as_str()))
    }

    /// The handler that should read `bytes` named `name`: content first
    /// (magic bytes), then the extension.
    #[must_use]
    pub fn importer_for(&self, bytes: &[u8], name: &str) -> Option<&dyn FormatHandler> {
        let head = &bytes[..bytes.len().min(4096)];
        self.iter()
            .filter(|handler| handler.can_import())
            .find(|handler| handler.sniff(head))
            .or_else(|| self.for_name(name).filter(|handler| handler.can_import()))
    }

    /// Reads `bytes` named `name` with the right handler.
    ///
    /// # Errors
    /// No handler, or the handler's own error.
    pub fn import(
        &self,
        bytes: &[u8],
        name: &str,
        options: &Properties,
    ) -> Result<Bundle, FormatError> {
        self.importer_for(bytes, name)
            .ok_or_else(|| FormatError::new(format!("Sue cannot open {name} yet")))?
            .import(bytes, name, options)
    }

    /// Handlers that can write `entity`.
    pub fn exporters_for<'a>(
        &'a self,
        entity: &'a Entity,
    ) -> impl Iterator<Item = &'a dyn FormatHandler> + 'a {
        self.iter()
            .filter(move |handler| handler.can_export(entity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StringBuffer, UndoStack, Value};

    struct Plain;

    impl FormatHandler for Plain {
        fn info(&self) -> FormatInfo {
            FormatInfo {
                id: "txt",
                name: "Plain text",
                extensions: &["txt", "text"],
                mime: "text/plain",
                category: "Document",
            }
        }

        fn sniff(&self, head: &[u8]) -> bool {
            head.starts_with(b"TXT:")
        }

        fn can_import(&self) -> bool {
            true
        }

        fn import(
            &self,
            bytes: &[u8],
            name: &str,
            _options: &Properties,
        ) -> Result<Bundle, FormatError> {
            let text = std::str::from_utf8(bytes).map_err(|_| FormatError::new("not UTF-8"))?;
            Ok(Bundle::single(Entity::new(name, StringBuffer::from(text))))
        }

        fn can_export(&self, entity: &Entity) -> bool {
            entity.string_buffer().is_some()
        }

        fn export(
            &self,
            arena: &Arena,
            ids: &[EntityId],
            _options: &Properties,
        ) -> Result<Vec<u8>, FormatError> {
            let mut out = String::new();
            for id in ids {
                if let Some(text) = arena.get_by_id(*id)?.string_buffer() {
                    out.push_str(text.as_str());
                }
            }
            Ok(out.into_bytes())
        }
    }

    #[test]
    fn registry_picks_by_content_then_extension() {
        let mut formats = FormatRegistry::new();
        formats.register(Plain);
        assert!(formats.importer_for(b"TXT:hi", "mystery.bin").is_some());
        assert!(formats.importer_for(b"hi", "C:/notes/A.TXT").is_some());
        assert!(formats.importer_for(b"hi", "a.png").is_none());
        let bundle = formats
            .import(b"hello", "a.txt", &Properties::new())
            .unwrap();
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let insert = InsertBundle::new("Import a.txt", bundle);
        let id = insert.ids()[0];
        history.apply(&mut arena, insert).unwrap();
        let entity = arena.get_by_id(id).unwrap();
        assert_eq!(formats.exporters_for(entity).count(), 1);
        let bytes = formats
            .get("txt")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        assert_eq!(bytes, b"hello");
        assert!(formats.import(b"x", "a.mov", &Properties::new()).is_err());
    }

    #[test]
    fn copies_get_fresh_ids_and_internal_links_follow() {
        let mut bundle = Bundle::new();
        let book = bundle.add(Entity::empty("Book", "book.book"));
        let one = bundle.add_child(book, Entity::empty("One", "book.chapter"));
        let two = bundle.add_child(
            book,
            Entity::empty("Two", "book.chapter").with_property("book.previous", one),
        );
        assert_eq!(bundle.roots(), vec![book]);

        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        history
            .apply(&mut arena, InsertBundle::new("Insert", bundle))
            .unwrap();
        assert_eq!(arena.relations().children(book), vec![one, two]);

        // Copy and paste the whole book: new ids, same shape.
        let mut copy = Bundle::copy_from(&arena, &[book], true).unwrap();
        assert_eq!(copy.entities.len(), 3);
        let map = copy.renew_ids();
        let (new_book, new_one, new_two) = (map[&book], map[&one], map[&two]);
        history
            .apply(&mut arena, InsertBundle::new("Paste", copy))
            .unwrap();
        assert_eq!(arena.len(), 6);
        assert_eq!(arena.relations().children(new_book), vec![new_one, new_two]);
        assert_eq!(
            arena.get_by_id(new_two).unwrap().property("book.previous"),
            Some(&Value::Entity(new_one))
        );

        // Undo the paste, redo it, and the same entities come back.
        history.undo(&mut arena).unwrap();
        assert_eq!(arena.len(), 3);
        history.redo(&mut arena).unwrap();
        assert_eq!(arena.relations().children(new_book), vec![new_one, new_two]);

        // Pasting the same bundle twice without new ids is refused whole.
        let again = Bundle::copy_from(&arena, &[book], true).unwrap();
        assert!(
            history
                .apply(&mut arena, InsertBundle::new("Dup", again))
                .is_err()
        );
        assert_eq!(arena.len(), 6);

        // Import under a parent and through the clipboard encoding.
        let folder = arena.insert(Entity::empty("Shelf", "sue.folder"));
        let folder = arena.get(folder).unwrap().id();
        let mut copy = Bundle::copy_from(&arena, &[book], true).unwrap();
        copy.renew_ids();
        let mut out = Writer::new();
        copy.encode(&mut out);
        let bytes = out.into_bytes();
        let pasted = Bundle::decode(&mut Reader::new(&bytes), &ComponentRegistry::new()).unwrap();
        assert_eq!(pasted, copy);
        let insert = InsertBundle::new("Paste", pasted).under(&arena, folder);
        history.apply(&mut arena, insert).unwrap();
        assert_eq!(arena.relations().children(folder).len(), 1);
    }
}
