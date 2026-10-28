//! The generational entity arena.

use std::collections::{BTreeMap, HashMap};

use crate::codec::{CodecError, Reader, Writer};
use crate::registry::components_equal;
use crate::{
    AssetHandle, Component, ComponentData, ComponentKind, ComponentRegistry, ComponentType,
    CoreError, EntityId, Projection, ProjectionMask, Properties, Relation, RelationStore,
    StringBuffer, Value,
};

/// An asset: a stable id, a class that says what it is, an optional base
/// buffer plus projection buffers (at most one per kind), extension
/// components (at most one per type), properties and string metadata.
#[derive(Clone, Debug)]
pub struct Entity {
    id: EntityId,
    name: String,
    class: String,
    base: Option<ComponentKind>,
    projections: ProjectionMask,
    components: BTreeMap<ComponentKind, ComponentData>,
    metadata: BTreeMap<String, String>,
    properties: Properties,
    ext: BTreeMap<String, Box<dyn Component>>,
    revision: u64,
}

impl PartialEq for Entity {
    /// Equal content; the revision counter is ignored.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.class == other.class
            && self.base == other.base
            && self.projections == other.projections
            && self.components == other.components
            && self.metadata == other.metadata
            && self.properties == other.properties
            && self.ext.len() == other.ext.len()
            && self
                .ext
                .iter()
                .zip(&other.ext)
                .all(|((a_name, a), (b_name, b))| {
                    a_name == b_name && components_equal(a.as_ref(), b.as_ref())
                })
    }
}

impl Entity {
    /// An entity named `name` with a fresh id, whose base is `base`. Its
    /// class defaults to the base kind's ([`ComponentKind::default_class`]).
    pub fn new(name: impl Into<String>, base: impl Into<ComponentData>) -> Self {
        let base = base.into();
        let kind = base.kind();
        Self {
            class: kind.default_class().to_owned(),
            base: Some(kind),
            projections: ProjectionMask::EMPTY.with(Projection::of_kind(kind)),
            components: BTreeMap::from([(kind, base)]),
            ..Self::empty(name, "sue.entity")
        }
    }

    /// An entity with no base buffer: a folder, a group, a scene, a table
    /// or anything made only of extension components and relations.
    pub fn empty(name: impl Into<String>, class: impl Into<String>) -> Self {
        Self {
            id: EntityId::new(),
            name: name.into(),
            class: class.into(),
            base: None,
            projections: ProjectionMask::EMPTY,
            components: BTreeMap::new(),
            metadata: BTreeMap::new(),
            properties: Properties::new(),
            ext: BTreeMap::new(),
            revision: 0,
        }
    }

    /// With a given id (loading, or redoing a creation).
    #[must_use]
    pub fn with_id(mut self, id: EntityId) -> Self {
        self.id = id;
        self
    }

    /// With a class, e.g. `sue.document` or `noelle.scene`.
    #[must_use]
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = class.into();
        self
    }

    /// With a metadata entry.
    #[must_use]
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_meta(key, value);
        self
    }

    /// With a property.
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.properties.set(key, value);
        self
    }

    /// With an extension component.
    #[must_use]
    pub fn with_ext(mut self, component: impl Component) -> Self {
        self.set_ext(Box::new(component));
        self
    }

    /// With a projection buffer next to the base.
    ///
    /// # Errors
    /// [`CoreError::AlreadyProjected`] if that kind is already present.
    pub fn with_component(
        mut self,
        component: impl Into<ComponentData>,
    ) -> Result<Self, CoreError> {
        let component = component.into();
        let projection = Projection::of_kind(component.kind());
        if self.components.contains_key(&component.kind()) {
            return Err(CoreError::AlreadyProjected {
                handle: AssetHandle::NULL,
                projection,
            });
        }
        if self.base.is_none() {
            self.base = Some(component.kind());
        }
        self.projections.insert(projection);
        self.components.insert(component.kind(), component);
        Ok(self)
    }

    /// The stable id.
    #[must_use]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Gives a detached entity a new id (copies, see `Bundle::renew_ids`).
    pub(crate) fn set_id(&mut self, id: EntityId) {
        self.id = id;
    }

    /// Human-readable name, e.g. a file name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renames the entity, returning the old name.
    pub fn set_name(&mut self, name: impl Into<String>) -> String {
        std::mem::replace(&mut self.name, name.into())
    }

    /// What the entity is, e.g. `sue.image` or `cad.part`. Features decide
    /// whether they can open an entity by its class first.
    #[must_use]
    pub fn class(&self) -> &str {
        &self.class
    }

    /// Changes the class, returning the old one.
    pub fn set_class(&mut self, class: impl Into<String>) -> String {
        std::mem::replace(&mut self.class, class.into())
    }

    /// The arena revision of the last mutable access: compare with a value
    /// saved earlier to know whether caches (thumbnails, renders) are stale.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The metadata value for `key`.
    #[must_use]
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    /// Sets a metadata value, returning the previous one. Metadata describes an
    /// asset (where it came from, how it was decoded); it is not edit state.
    pub fn set_meta(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String> {
        self.metadata.insert(key.into(), value.into())
    }

    /// Removes a metadata value.
    pub fn remove_meta(&mut self, key: &str) -> Option<String> {
        self.metadata.remove(key)
    }

    /// All metadata, ordered by key.
    pub fn metadata(&self) -> impl Iterator<Item = (&str, &str)> {
        self.metadata
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    /// The properties (edit state any feature can read).
    #[must_use]
    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    /// The properties, mutably. Mutations must come from a transaction.
    pub fn properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    /// One property.
    #[must_use]
    pub fn property(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    /// Kind of the base buffer, if the entity has one.
    #[must_use]
    pub fn base_kind(&self) -> Option<ComponentKind> {
        self.base
    }

    /// Projections currently attached.
    #[must_use]
    pub fn projections(&self) -> ProjectionMask {
        self.projections
    }

    /// The component of `kind`, if attached.
    #[must_use]
    pub fn component(&self, kind: ComponentKind) -> Option<&ComponentData> {
        self.components.get(&kind)
    }

    /// The component of `kind`, mutably. Mutations must come from a
    /// [`ReversibleTransaction`](crate::ReversibleTransaction).
    pub fn component_mut(&mut self, kind: ComponentKind) -> Option<&mut ComponentData> {
        self.components.get_mut(&kind)
    }

    /// All attached components, ordered by kind.
    pub fn components(&self) -> impl Iterator<Item = &ComponentData> {
        self.components.values()
    }

    /// The `StringBuffer` component, if attached.
    #[must_use]
    pub fn string_buffer(&self) -> Option<&StringBuffer> {
        self.component(ComponentKind::StringBuffer)?
            .as_string_buffer()
    }

    /// The `StringBuffer` component, mutably.
    pub fn string_buffer_mut(&mut self) -> Option<&mut StringBuffer> {
        self.component_mut(ComponentKind::StringBuffer)?
            .as_string_buffer_mut()
    }

    /// The extension component of type `T`.
    #[must_use]
    pub fn ext<T: ComponentType>(&self) -> Option<&T> {
        self.ext.get(T::NAME)?.as_any().downcast_ref()
    }

    /// The extension component of type `T`, mutably.
    pub fn ext_mut<T: ComponentType>(&mut self) -> Option<&mut T> {
        self.ext.get_mut(T::NAME)?.as_any_mut().downcast_mut()
    }

    /// The extension component named `name`, whatever its type.
    #[must_use]
    pub fn ext_named(&self, name: &str) -> Option<&dyn Component> {
        self.ext.get(name).map(AsRef::as_ref)
    }

    /// Adds or replaces an extension component, returning the old one.
    pub fn set_ext(&mut self, component: Box<dyn Component>) -> Option<Box<dyn Component>> {
        self.ext.insert(component.type_name().to_owned(), component)
    }

    /// Removes the extension component named `name`.
    pub fn remove_ext(&mut self, name: &str) -> Option<Box<dyn Component>> {
        self.ext.remove(name)
    }

    /// Every extension component, ordered by type name.
    pub fn exts(&self) -> impl Iterator<Item = &dyn Component> {
        self.ext.values().map(AsRef::as_ref)
    }

    /// Points every entity link (properties, AST children, extension
    /// components) through `map`.
    pub fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        self.properties
            .values_mut()
            .for_each(|value| value.remap_entities(map));
        if let Some(ComponentData::ASTNode(node)) = self.components.get_mut(&ComponentKind::ASTNode)
        {
            node.remap_children(map);
        }
        for component in self.ext.values_mut() {
            component.remap_entities(map);
        }
    }

    /// Approximate heap footprint of every component, in bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.components
            .values()
            .map(ComponentData::heap_bytes)
            .sum::<usize>()
            + self.ext.values().map(|ext| ext.heap_bytes()).sum::<usize>()
    }

    /// Writes the entity (everything but its revision).
    pub fn encode(&self, out: &mut Writer) {
        out.raw(&self.id.to_bytes());
        out.str(&self.name);
        out.str(&self.class);
        out.str(self.base.map_or("", ComponentKind::name));
        out.len(self.components.len());
        for component in self.components.values() {
            out.str(component.kind().name());
            out.nested(|out| component.encode(out));
        }
        out.len(self.metadata.len());
        for (key, value) in &self.metadata {
            out.str(key);
            out.str(value);
        }
        self.properties.encode(out);
        out.len(self.ext.len());
        for component in self.ext.values() {
            out.str(component.type_name());
            out.varint(u64::from(component.version()));
            out.nested(|out| component.encode(out));
        }
    }

    /// Reads an entity. Extension components decode through `registry`;
    /// unknown ones are kept opaque.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(
        input: &mut Reader<'_>,
        registry: &ComponentRegistry,
    ) -> Result<Self, CodecError> {
        let mut id = [0; 16];
        id.copy_from_slice(input.raw(16)?);
        let mut entity = Self::empty(input.str()?, input.str()?).with_id(EntityId::from_bytes(id));
        let base = input.str()?;
        for _ in 0..input.len(2)? {
            let name = input.str()?;
            let kind = ComponentKind::from_name(&name)
                .ok_or_else(|| input.error(format!("unknown built-in component {name}")))?;
            let mut body = input.nested()?;
            let component = ComponentData::decode(kind, &mut body)?;
            entity.projections.insert(Projection::of_kind(kind));
            entity.components.insert(kind, component);
        }
        if !base.is_empty() {
            let kind = ComponentKind::from_name(&base)
                .filter(|kind| entity.components.contains_key(kind))
                .ok_or_else(|| input.error(format!("missing base component {base}")))?;
            entity.base = Some(kind);
        }
        for _ in 0..input.len(2)? {
            let key = input.str()?;
            entity.metadata.insert(key, input.str()?);
        }
        entity.properties = Properties::decode(input)?;
        for _ in 0..input.len(3)? {
            let name = input.str()?;
            let version =
                u32::try_from(input.varint()?).map_err(|_| input.error("version overflows"))?;
            let bytes = input.bytes()?;
            let component = registry.decode(&name, version, bytes)?;
            entity.ext.insert(name, component);
        }
        Ok(entity)
    }
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    entity: Option<Entity>,
}

/// Generational slot arena of [`Entity`]s, addressed by [`AssetHandle`] at
/// runtime and by [`EntityId`] for good, plus the [`RelationStore`] that
/// links them.
///
/// `spawn` and `despawn` are O(1). Despawning bumps the slot generation, so old
/// handles go stale instead of aliasing the next occupant. Mutation needs
/// `&mut Arena`. The arena is `Send + Sync`, so any number of threads may read
/// it concurrently.
#[derive(Clone, Debug, Default)]
pub struct Arena {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
    ids: HashMap<EntityId, AssetHandle>,
    relations: RelationStore,
    revision: u64,
}

impl Arena {
    /// An empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty arena with room for `capacity` entities.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    /// Number of live entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// `true` if no entity is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Increases on every change (spawn, despawn, mutable access, relation
    /// edit), so "is the document modified" and "is this cache stale" are
    /// one comparison.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn bump(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Creates an entity named `name` whose base component is `base`.
    ///
    /// # Panics
    /// If more than `u32::MAX` slots would be needed.
    pub fn spawn(
        &mut self,
        name: impl Into<String>,
        base: impl Into<ComponentData>,
    ) -> AssetHandle {
        self.insert(Entity::new(name, base))
    }

    /// Puts an entity into the arena. If its id is nil or already taken, it
    /// gets a fresh id (see [`Self::insert_keep_id`] to refuse instead).
    ///
    /// # Panics
    /// If more than `u32::MAX` slots would be needed.
    pub fn insert(&mut self, mut entity: Entity) -> AssetHandle {
        if entity.id.is_nil() || self.ids.contains_key(&entity.id) {
            entity.id = EntityId::new();
        }
        self.place(entity)
    }

    /// Puts an entity into the arena under its own id.
    ///
    /// # Errors
    /// [`CoreError::DuplicateId`] if the id is nil or taken.
    pub fn insert_keep_id(&mut self, entity: Entity) -> Result<AssetHandle, CoreError> {
        if entity.id.is_nil() || self.ids.contains_key(&entity.id) {
            return Err(CoreError::DuplicateId(entity.id));
        }
        Ok(self.place(entity))
    }

    fn place(&mut self, mut entity: Entity) -> AssetHandle {
        entity.revision = self.bump();
        let id = entity.id;
        self.live += 1;
        let handle = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.entity = Some(entity);
            AssetHandle::new(index, slot.generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("arena exceeded u32::MAX slots");
            self.slots.push(Slot {
                generation: 1,
                entity: Some(entity),
            });
            AssetHandle::new(index, 1)
        };
        self.ids.insert(id, handle);
        handle
    }

    /// Puts back an entity that [`despawn`](Self::despawn) removed from
    /// `handle`, under the same handle when its slot was not reused since,
    /// so later edits that name the handle still apply (redo). Otherwise
    /// the entity gets a new handle.
    pub fn restore(&mut self, handle: AssetHandle, mut entity: Entity) -> AssetHandle {
        let index = handle.index();
        let reusable = self.slots.get(index as usize).is_some_and(|slot| {
            slot.entity.is_none() && slot.generation == handle.generation().wrapping_add(1)
        }) && self.free.contains(&index)
            && !self.ids.contains_key(&entity.id);
        if !reusable {
            return self.insert(entity);
        }
        entity.revision = self.bump();
        self.ids.insert(entity.id, handle);
        self.free.retain(|free| *free != index);
        let slot = &mut self.slots[index as usize];
        slot.generation = handle.generation();
        slot.entity = Some(entity);
        self.live += 1;
        handle
    }

    /// Removes an entity and returns it. Every handle to it becomes stale,
    /// and every relation to or from it is dropped (use
    /// [`Self::despawn_with_relations`] to keep them for undo).
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn despawn(&mut self, handle: AssetHandle) -> Result<Entity, CoreError> {
        self.despawn_with_relations(handle)
            .map(|(entity, _)| entity)
    }

    /// Removes an entity and the relations that touch it, returning both.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn despawn_with_relations(
        &mut self,
        handle: AssetHandle,
    ) -> Result<(Entity, Vec<Relation>), CoreError> {
        let slot = self
            .slots
            .get_mut(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .ok_or(CoreError::StaleHandle(handle))?;
        let entity = slot.entity.take().ok_or(CoreError::StaleHandle(handle))?;
        self.live -= 1;
        // A slot whose generation is exhausted is retired, never reused.
        if slot.generation < u32::MAX {
            slot.generation += 1;
            self.free.push(handle.index());
        }
        self.ids.remove(&entity.id);
        let relations = self.relations.remove_involving(entity.id);
        self.bump();
        Ok((entity, relations))
    }

    /// `true` if `handle` refers to a live entity.
    #[must_use]
    pub fn contains(&self, handle: AssetHandle) -> bool {
        self.get(handle).is_ok()
    }

    /// The entity behind `handle`.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn get(&self, handle: AssetHandle) -> Result<&Entity, CoreError> {
        self.slots
            .get(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .and_then(|slot| slot.entity.as_ref())
            .ok_or(CoreError::StaleHandle(handle))
    }

    /// The entity behind `handle`, mutably. This marks it changed.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn get_mut(&mut self, handle: AssetHandle) -> Result<&mut Entity, CoreError> {
        let revision = self.revision + 1;
        let entity = self
            .slots
            .get_mut(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .and_then(|slot| slot.entity.as_mut())
            .ok_or(CoreError::StaleHandle(handle))?;
        entity.revision = revision;
        self.revision = revision;
        Ok(entity)
    }

    /// The handle of the entity with `id`, if it is live.
    #[must_use]
    pub fn handle_of(&self, id: EntityId) -> Option<AssetHandle> {
        self.ids.get(&id).copied()
    }

    /// The entity with `id`.
    ///
    /// # Errors
    /// [`CoreError::UnknownId`] if no live entity has it.
    pub fn get_by_id(&self, id: EntityId) -> Result<&Entity, CoreError> {
        let handle = self.handle_of(id).ok_or(CoreError::UnknownId(id))?;
        self.get(handle)
    }

    /// The entity with `id`, mutably. This marks it changed.
    ///
    /// # Errors
    /// [`CoreError::UnknownId`] if no live entity has it.
    pub fn get_by_id_mut(&mut self, id: EntityId) -> Result<&mut Entity, CoreError> {
        let handle = self.handle_of(id).ok_or(CoreError::UnknownId(id))?;
        self.get_mut(handle)
    }

    /// The relations between entities.
    #[must_use]
    pub fn relations(&self) -> &RelationStore {
        &self.relations
    }

    /// Adds or updates a relation between two live entities, returning the
    /// edge it replaced.
    ///
    /// # Errors
    /// [`CoreError::UnknownId`] for a missing end, or [`CoreError::Relation`]
    /// if the edge breaks a tree.
    pub fn relate(&mut self, relation: Relation) -> Result<Option<Relation>, CoreError> {
        for id in [relation.from, relation.to] {
            if !self.ids.contains_key(&id) {
                return Err(CoreError::UnknownId(id));
            }
        }
        let previous = self
            .relations
            .get(relation.from, &relation.kind, relation.to);
        self.relations
            .insert(relation)
            .map_err(CoreError::Relation)?;
        self.bump();
        Ok(previous)
    }

    /// Removes a relation, returning it.
    pub fn unrelate(&mut self, from: EntityId, kind: &str, to: EntityId) -> Option<Relation> {
        let removed = self.relations.remove(from, kind, to);
        if removed.is_some() {
            self.bump();
        }
        removed
    }

    /// Registers a tree kind (one parent, no cycles) for this document.
    pub fn add_tree_kind(&mut self, kind: impl Into<String>) {
        self.relations.add_tree_kind(kind);
    }

    /// Borrows `read` shared and `write` exclusively at the same time, without
    /// copying either. This is the primitive behind zero-copy pipelines.
    ///
    /// # Errors
    /// [`CoreError::AliasedHandles`] if both handles are equal, or
    /// [`CoreError::StaleHandle`] if either is not live.
    pub fn split_pair(
        &mut self,
        read: AssetHandle,
        write: AssetHandle,
    ) -> Result<(&Entity, &mut Entity), CoreError> {
        if read == write {
            return Err(CoreError::AliasedHandles(read));
        }
        self.get(read)?;
        self.get(write)?;
        let revision = self.bump();
        // Two distinct live handles cannot share a slot: a slot has one generation.
        let (read_index, write_index) = (read.index() as usize, write.index() as usize);
        let (reader, writer) = if read_index < write_index {
            let (low, high) = self.slots.split_at_mut(write_index);
            (&low[read_index], &mut high[0])
        } else {
            let (low, high) = self.slots.split_at_mut(read_index);
            (&high[0], &mut low[write_index])
        };
        match (reader.entity.as_ref(), writer.entity.as_mut()) {
            (Some(reader), Some(writer)) => {
                writer.revision = revision;
                Ok((reader, writer))
            }
            (None, _) => Err(CoreError::StaleHandle(read)),
            (_, None) => Err(CoreError::StaleHandle(write)),
        }
    }

    /// Checks that `projection` could be attached to `handle` with a component
    /// of `kind`, without changing anything.
    ///
    /// # Errors
    /// The same errors [`Arena::attach_projection`] would return.
    pub fn check_attach(
        &self,
        handle: AssetHandle,
        projection: Projection,
        kind: ComponentKind,
    ) -> Result<(), CoreError> {
        if kind != projection.native_kind() {
            return Err(CoreError::ProjectionMismatch {
                projection,
                found: kind,
            });
        }
        let entity = self.get(handle)?;
        if entity.projections.contains(projection) || entity.components.contains_key(&kind) {
            return Err(CoreError::AlreadyProjected { handle, projection });
        }
        Ok(())
    }

    /// Attaches `projection` by adding `component` next to the existing ones.
    /// Existing buffers are neither copied, moved on the heap, nor mutated.
    /// An entity without a base takes this component as its base.
    ///
    /// # Errors
    /// [`CoreError::ProjectionMismatch`], [`CoreError::AlreadyProjected`] or
    /// [`CoreError::StaleHandle`].
    pub fn attach_projection(
        &mut self,
        handle: AssetHandle,
        projection: Projection,
        component: ComponentData,
    ) -> Result<(), CoreError> {
        let kind = component.kind();
        self.check_attach(handle, projection, kind)?;
        let entity = self.get_mut(handle)?;
        entity.components.insert(kind, component);
        entity.projections.insert(projection);
        if entity.base.is_none() {
            entity.base = Some(kind);
        }
        Ok(())
    }

    /// Detaches `projection` and returns its component, so the change can be
    /// reverted.
    ///
    /// # Errors
    /// [`CoreError::CannotDetachBase`], [`CoreError::NotProjected`] or
    /// [`CoreError::StaleHandle`].
    pub fn detach_projection(
        &mut self,
        handle: AssetHandle,
        projection: Projection,
    ) -> Result<ComponentData, CoreError> {
        let entity = self.get_mut(handle)?;
        let kind = projection.native_kind();
        if Some(kind) == entity.base {
            return Err(CoreError::CannotDetachBase { handle, projection });
        }
        let component = entity
            .components
            .remove(&kind)
            .ok_or(CoreError::NotProjected { handle, projection })?;
        entity.projections.remove(projection);
        Ok(component)
    }

    /// Every live entity with its handle, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (AssetHandle, &Entity)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let entity = slot.entity.as_ref()?;
            // `spawn` guarantees the slot count fits in u32.
            Some((AssetHandle::new(index as u32, slot.generation), entity))
        })
    }

    /// Handles of every live entity that supports `projection`.
    pub fn with_projection(&self, projection: Projection) -> impl Iterator<Item = AssetHandle> {
        self.iter()
            .filter(move |(_, entity)| entity.projections.contains(projection))
            .map(|(handle, _)| handle)
    }

    /// Handles of every live entity of `class`.
    pub fn with_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = AssetHandle> + 'a {
        self.iter()
            .filter(move |(_, entity)| entity.class == class)
            .map(|(handle, _)| handle)
    }

    /// Approximate heap footprint of all component payloads, in bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.iter().map(|(_, entity)| entity.heap_bytes()).sum()
    }

    /// Writes every live entity (in slot order) and every relation.
    pub fn encode(&self, out: &mut Writer) {
        self.encode_where(out, &|_| true);
    }

    /// Writes the entities `keep` accepts and the relations between them
    /// (an app's own scratch entities stay out of documents).
    pub fn encode_where(&self, out: &mut Writer, keep: &dyn Fn(&Entity) -> bool) {
        let kept: Vec<&Entity> = self
            .iter()
            .map(|(_, entity)| entity)
            .filter(|entity| keep(entity))
            .collect();
        out.len(kept.len());
        for entity in &kept {
            out.nested(|out| entity.encode(out));
        }
        if kept.len() == self.live {
            self.relations.encode(out);
            return;
        }
        let ids: std::collections::BTreeSet<EntityId> =
            kept.iter().map(|entity| entity.id).collect();
        let mut relations = RelationStore::new();
        for kind in self.relations.tree_kinds() {
            relations.add_tree_kind(kind);
        }
        for relation in self.relations.iter() {
            if ids.contains(&relation.from) && ids.contains(&relation.to) {
                // Valid in the full store, so valid in a subset of it.
                let _ = relations.insert(relation);
            }
        }
        relations.encode(out);
    }

    /// Reads an arena written by [`Self::encode`]. Handles are new; ids are
    /// kept.
    ///
    /// # Errors
    /// If the bytes are malformed or two entities share an id.
    pub fn decode(
        input: &mut Reader<'_>,
        registry: &ComponentRegistry,
    ) -> Result<Self, CodecError> {
        let count = input.len(1)?;
        let mut arena = Self::with_capacity(count);
        for _ in 0..count {
            let mut body = input.nested()?;
            let entity = Entity::decode(&mut body, registry)?;
            arena
                .insert_keep_id(entity)
                .map_err(|error| input.error(error.to_string()))?;
        }
        let relations = RelationStore::decode(input)?;
        for relation in relations.iter() {
            if !arena.ids.contains_key(&relation.from) || !arena.ids.contains_key(&relation.to) {
                return Err(input.error("a relation names a missing entity"));
            }
        }
        arena.relations = relations;
        Ok(arena)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CHILD, MeshBuffer3D, OpaqueComponent, PCMFloat1D, PixelFormat, RasterTile2D, TimeSpan,
    };

    #[test]
    fn spawn_get_despawn() {
        let mut arena = Arena::new();
        let handle = arena.spawn("note.txt", StringBuffer::from("hello"));
        assert_eq!(arena.len(), 1);
        let entity = arena.get(handle).unwrap();
        assert_eq!(entity.name(), "note.txt");
        assert_eq!(entity.base_kind(), Some(ComponentKind::StringBuffer));
        assert_eq!(entity.class(), "sue.text");
        assert_eq!(entity.string_buffer().unwrap().as_str(), "hello");
        let id = entity.id();
        assert_eq!(arena.handle_of(id), Some(handle));
        let removed = arena.despawn(handle).unwrap();
        assert_eq!(removed.name(), "note.txt");
        assert!(arena.is_empty());
        assert_eq!(arena.get(handle), Err(CoreError::StaleHandle(handle)));
        assert_eq!(arena.get_by_id(id), Err(CoreError::UnknownId(id)));
        assert!(arena.despawn(handle).is_err());
    }

    #[test]
    fn restored_entities_keep_their_handle_unless_the_slot_was_reused() {
        let mut arena = Arena::new();
        let handle = arena.spawn("a", StringBuffer::new());
        let id = arena.get(handle).unwrap().id();
        let entity = arena.despawn(handle).unwrap();
        assert_eq!(arena.restore(handle, entity), handle);
        assert!(arena.get(handle).is_ok());
        assert_eq!(arena.handle_of(id), Some(handle));
        let entity = arena.despawn(handle).unwrap();
        let other = arena.spawn("b", StringBuffer::new());
        let restored = arena.restore(handle, entity);
        assert_ne!(restored, handle);
        assert_ne!(restored, other);
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(restored).unwrap().id(), id, "the id survives");
    }

    #[test]
    fn ids_are_unique_within_an_arena() {
        let mut arena = Arena::new();
        let first = arena.spawn("a", StringBuffer::new());
        let id = arena.get(first).unwrap().id();
        let copy = arena.get(first).unwrap().clone();
        assert_eq!(
            arena.insert_keep_id(copy.clone()),
            Err(CoreError::DuplicateId(id))
        );
        let second = arena.insert(copy);
        assert_ne!(arena.get(second).unwrap().id(), id);
    }

    #[test]
    fn reused_slots_get_new_generations() {
        let mut arena = Arena::new();
        let first = arena.spawn("a", StringBuffer::new());
        arena.despawn(first).unwrap();
        let second = arena.spawn("b", StringBuffer::new());
        assert_eq!(first.index(), second.index());
        assert_eq!(second.generation(), first.generation() + 1);
        assert!(!arena.contains(first));
        assert!(arena.contains(second));
        assert!(!arena.contains(AssetHandle::NULL));
    }

    #[test]
    fn revisions_track_changes() {
        let mut arena = Arena::new();
        let a = arena.spawn("a", StringBuffer::new());
        let b = arena.spawn("b", StringBuffer::new());
        let seen = arena.get(a).unwrap().revision();
        let before = arena.revision();
        arena.get(a).unwrap();
        assert_eq!(arena.revision(), before, "reading changes nothing");
        arena.get_mut(b).unwrap();
        assert_eq!(arena.get(a).unwrap().revision(), seen);
        assert!(arena.get(b).unwrap().revision() > seen);
        assert!(arena.revision() > before);
    }

    #[test]
    fn split_pair_borrows_two_entities() {
        let mut arena = Arena::new();
        let source = arena.spawn("src", StringBuffer::from("abc"));
        let target = arena.spawn("dst", StringBuffer::new());
        for (read, write) in [(source, target), (target, source)] {
            let (reader, writer) = arena.split_pair(read, write).unwrap();
            let text = reader.string_buffer().unwrap().as_str().to_owned();
            let buffer = writer.string_buffer_mut().unwrap();
            let end = buffer.len();
            buffer.insert(end, &text).unwrap();
        }
        assert_eq!(
            arena.get(target).unwrap().string_buffer().unwrap().as_str(),
            "abc"
        );
        assert_eq!(
            arena.get(source).unwrap().string_buffer().unwrap().as_str(),
            "abcabc"
        );
        assert_eq!(
            arena.split_pair(source, source).unwrap_err(),
            CoreError::AliasedHandles(source)
        );
    }

    #[test]
    fn attaching_a_projection_never_moves_existing_buffers() {
        let mut arena = Arena::new();
        let pcm = PCMFloat1D::new(48_000, 1, vec![0.25; 4096]).unwrap();
        let handle = arena.spawn("take-1.wav", pcm);
        let before = arena
            .get(handle)
            .unwrap()
            .component(ComponentKind::PCMFloat1D)
            .unwrap();
        let address = before.f32_view().unwrap().as_ptr();

        let span = TimeSpan::from_seconds(0.0, 4096.0 / 48_000.0).unwrap();
        arena
            .attach_projection(handle, Projection::Timeline, span.into())
            .unwrap();
        let tile = RasterTile2D::new(64, 64, PixelFormat::Rgba8).unwrap();
        arena
            .attach_projection(handle, Projection::Raster2D, tile.into())
            .unwrap();

        let entity = arena.get(handle).unwrap();
        let after = entity.component(ComponentKind::PCMFloat1D).unwrap();
        assert_eq!(after.f32_view().unwrap().as_ptr(), address);
        assert_eq!(entity.projections().len(), 3);
        assert_eq!(arena.with_projection(Projection::Timeline).count(), 1);
    }

    #[test]
    fn projection_rules_are_enforced() {
        let mut arena = Arena::new();
        let handle = arena.spawn("tile", RasterTile2D::new(2, 2, PixelFormat::Gray8).unwrap());
        assert_eq!(
            arena.attach_projection(handle, Projection::Mesh3D, StringBuffer::new().into()),
            Err(CoreError::ProjectionMismatch {
                projection: Projection::Mesh3D,
                found: ComponentKind::StringBuffer
            })
        );
        arena
            .attach_projection(
                handle,
                Projection::Mesh3D,
                MeshBuffer3D::quad(2.0, 2.0).into(),
            )
            .unwrap();
        assert!(matches!(
            arena.attach_projection(handle, Projection::Mesh3D, MeshBuffer3D::default().into()),
            Err(CoreError::AlreadyProjected { .. })
        ));
        assert!(matches!(
            arena.detach_projection(handle, Projection::Raster2D),
            Err(CoreError::CannotDetachBase { .. })
        ));
        let mesh = arena.detach_projection(handle, Projection::Mesh3D).unwrap();
        assert_eq!(mesh.kind(), ComponentKind::MeshBuffer3D);
        assert!(matches!(
            arena.detach_projection(handle, Projection::Mesh3D),
            Err(CoreError::NotProjected { .. })
        ));
    }

    #[test]
    fn entities_carry_metadata_and_properties() {
        let mut arena = Arena::new();
        let handle = arena.spawn("photo.jxl", StringBuffer::new());
        let entity = arena.get_mut(handle).unwrap();
        assert_eq!(entity.set_meta("source", "C:/photos/photo.jxl"), None);
        assert_eq!(entity.set_meta("bytes", "1024"), None);
        assert_eq!(entity.set_meta("bytes", "2048").as_deref(), Some("1024"));
        entity.properties_mut().set("photo.exposure", 0.5);
        let entity = arena.get(handle).unwrap();
        assert_eq!(entity.meta("bytes"), Some("2048"));
        assert_eq!(entity.metadata().count(), 2);
        assert_eq!(entity.meta("missing"), None);
        assert_eq!(entity.properties().f64("photo.exposure"), Some(0.5));
    }

    #[test]
    fn empty_entities_and_relations() {
        let mut arena = Arena::new();
        let folder = arena.insert(Entity::empty("Album", "sue.folder"));
        let song = arena.spawn("song", PCMFloat1D::silence(48_000, 2, 16).unwrap());
        let folder_id = arena.get(folder).unwrap().id();
        let song_id = arena.get(song).unwrap().id();
        assert_eq!(arena.get(folder).unwrap().base_kind(), None);
        arena
            .relate(Relation::new(folder_id, CHILD, song_id))
            .unwrap();
        assert_eq!(arena.relations().children(folder_id), vec![song_id]);
        assert!(matches!(
            arena.relate(Relation::new(folder_id, CHILD, EntityId::new())),
            Err(CoreError::UnknownId(_))
        ));
        assert!(matches!(
            arena.relate(Relation::new(song_id, CHILD, folder_id)),
            Err(CoreError::Relation(_))
        ));
        let (_, relations) = arena.despawn_with_relations(song).unwrap();
        assert_eq!(relations.len(), 1);
        assert!(arena.relations().is_empty());
        assert_eq!(arena.with_class("sue.folder").count(), 1);
    }

    #[test]
    fn arenas_round_trip_through_bytes() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", StringBuffer::from("hello"));
        let group = arena.insert(
            Entity::empty("group", "sue.group")
                .with_property("ui.color", Value::Int(3))
                .with_ext(OpaqueComponent {
                    type_name: "future.thing".into(),
                    version: 9,
                    bytes: vec![1, 2, 3],
                }),
        );
        arena
            .attach_projection(
                doc,
                Projection::Timeline,
                TimeSpan::new(5, 10).unwrap().into(),
            )
            .unwrap();
        let (doc_id, group_id) = (arena.get(doc).unwrap().id(), arena.get(group).unwrap().id());
        arena
            .relate(Relation::new(group_id, CHILD, doc_id))
            .unwrap();
        arena.get_mut(doc).unwrap().set_meta("source", "a.txt");

        let mut out = Writer::new();
        arena.encode(&mut out);
        let bytes = out.into_bytes();
        let back = Arena::decode(&mut Reader::new(&bytes), &ComponentRegistry::new()).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.get_by_id(doc_id).unwrap(), arena.get(doc).unwrap());
        assert_eq!(back.get_by_id(group_id).unwrap(), arena.get(group).unwrap());
        assert_eq!(back.relations(), arena.relations());

        let mut again = Writer::new();
        back.encode(&mut again);
        assert_eq!(again.into_bytes(), bytes, "saving a loaded file is stable");
    }

    #[test]
    fn iterates_live_entities_only() {
        let mut arena = Arena::with_capacity(4);
        let a = arena.spawn("a", StringBuffer::from("1234"));
        let b = arena.spawn("b", StringBuffer::from("56"));
        arena.despawn(a).unwrap();
        let handles: Vec<_> = arena.iter().map(|(handle, _)| handle).collect();
        assert_eq!(handles, vec![b]);
        assert_eq!(arena.heap_bytes(), 2);
    }
}
