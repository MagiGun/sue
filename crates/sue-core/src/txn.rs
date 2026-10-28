//! Reversible transactions over the [`Arena`].
//!
//! Each transaction stores only its delta: the inserted text, the removed
//! range, the component it attached, the value a property had. The generic
//! ones ([`SpawnEntity`], [`DespawnEntity`], [`SetProperty`], [`SetMeta`],
//! [`Rename`], [`Relate`], [`Unrelate`], [`SetExtension`] and [`Batch`])
//! name entities by [`EntityId`], so they stay valid however often an
//! entity is removed and brought back.

use std::fmt;
use std::ops::Range;

use crate::{
    Arena, AssetHandle, Component, ComponentData, ComponentKind, CoreError, Entity, EntityId,
    Projection, Relation, ReversibleTransaction, StringBuffer, TxnError, Value,
};

fn text_mut(arena: &mut Arena, handle: AssetHandle) -> Result<&mut StringBuffer, CoreError> {
    arena
        .get_mut(handle)?
        .string_buffer_mut()
        .ok_or(CoreError::MissingComponent {
            handle,
            kind: ComponentKind::StringBuffer,
        })
}

/// Inserts text into an entity's `StringBuffer`.
#[derive(Clone, Debug)]
pub struct InsertText {
    handle: AssetHandle,
    offset: usize,
    text: String,
}

impl InsertText {
    /// Inserts `text` at byte `offset` of `handle`'s text.
    pub fn new(handle: AssetHandle, offset: usize, text: impl Into<String>) -> Self {
        Self {
            handle,
            offset,
            text: text.into(),
        }
    }
}

impl ReversibleTransaction<Arena> for InsertText {
    fn label(&self) -> String {
        format!("Insert {} character(s)", self.text.chars().count())
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        text_mut(arena, self.handle)?.insert(self.offset, &self.text)?;
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let range = self.offset..self.offset + self.text.len();
        text_mut(arena, self.handle)?.remove(range)?;
        Ok(())
    }
}

/// Deletes a byte range from an entity's `StringBuffer`, capturing the removed
/// text as its delta when applied.
#[derive(Clone, Debug)]
pub struct DeleteText {
    handle: AssetHandle,
    range: Range<usize>,
    removed: Option<String>,
}

impl DeleteText {
    /// Deletes byte `range` of `handle`'s text.
    #[must_use]
    pub fn new(handle: AssetHandle, range: Range<usize>) -> Self {
        Self {
            handle,
            range,
            removed: None,
        }
    }
}

impl ReversibleTransaction<Arena> for DeleteText {
    fn label(&self) -> String {
        format!("Delete {} byte(s)", self.range.len())
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let removed = text_mut(arena, self.handle)?.remove(self.range.clone())?;
        self.removed = Some(removed);
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let removed = self
            .removed
            .as_deref()
            .ok_or_else(|| TxnError::new("DeleteText reverted before it was applied"))?;
        text_mut(arena, self.handle)?.insert(self.range.start, removed)?;
        self.removed = None;
        Ok(())
    }
}

/// One step of a [`TextEdits`] group, in byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextOp {
    /// `text` was inserted at `offset`.
    Insert {
        /// Byte offset.
        offset: usize,
        /// The inserted text.
        text: String,
    },
    /// `text` was removed from `offset`.
    Delete {
        /// Byte offset.
        offset: usize,
        /// The removed text, the delta that brings it back.
        text: String,
    },
}

impl TextOp {
    fn apply(&self, buffer: &mut StringBuffer) -> Result<(), TxnError> {
        match self {
            Self::Insert { offset, text } => buffer.insert(*offset, text)?,
            Self::Delete { offset, text } => {
                buffer.remove(*offset..*offset + text.len())?;
            }
        }
        Ok(())
    }

    fn revert(&self, buffer: &mut StringBuffer) -> Result<(), TxnError> {
        match self {
            Self::Insert { offset, text } => {
                buffer.remove(*offset..*offset + text.len())?;
            }
            Self::Delete { offset, text } => buffer.insert(*offset, text)?,
        }
        Ok(())
    }
}

/// A run of typing in one text entity, recorded as the inserts and deletes
/// that happened, so one undo step takes back a burst of typing exactly.
#[derive(Clone, Debug)]
pub struct TextEdits {
    handle: AssetHandle,
    ops: Vec<TextOp>,
}

impl TextEdits {
    /// An empty group for `handle`.
    #[must_use]
    pub fn new(handle: AssetHandle) -> Self {
        Self {
            handle,
            ops: Vec::new(),
        }
    }

    /// The entity the group edits.
    #[must_use]
    pub fn handle(&self) -> AssetHandle {
        self.handle
    }

    /// Adds a step that already happened.
    pub fn push(&mut self, op: TextOp) {
        self.ops.push(op);
    }

    /// The recorded steps, oldest first.
    #[must_use]
    pub fn ops(&self) -> &[TextOp] {
        &self.ops
    }

    /// `true` if nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl ReversibleTransaction<Arena> for TextEdits {
    fn label(&self) -> String {
        let typed: usize = self
            .ops
            .iter()
            .map(|op| match op {
                TextOp::Insert { text, .. } => text.chars().count(),
                TextOp::Delete { .. } => 0,
            })
            .sum();
        let deleted: usize = self
            .ops
            .iter()
            .map(|op| match op {
                TextOp::Delete { text, .. } => text.chars().count(),
                TextOp::Insert { .. } => 0,
            })
            .sum();
        match (typed, deleted) {
            (0, deleted) => format!("Delete {deleted} character(s)"),
            (typed, 0) => format!("Type {typed} character(s)"),
            (typed, deleted) => format!("Edit text (+{typed} -{deleted})"),
        }
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let buffer = text_mut(arena, self.handle)?;
        for (done, op) in self.ops.iter().enumerate() {
            if let Err(error) = op.apply(buffer) {
                // Atomic: take back what already went in.
                for op in self.ops[..done].iter().rev() {
                    let _ = op.revert(buffer);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let buffer = text_mut(arena, self.handle)?;
        for (undone, op) in self.ops.iter().rev().enumerate() {
            if let Err(error) = op.revert(buffer) {
                let count = self.ops.len();
                for op in &self.ops[count - undone..] {
                    let _ = op.apply(buffer);
                }
                return Err(error);
            }
        }
        Ok(())
    }
}

/// Attaches a projection component to an entity, non-destructively.
#[derive(Clone, Debug)]
pub struct AttachProjection {
    handle: AssetHandle,
    projection: Projection,
    component: Option<ComponentData>,
}

impl AttachProjection {
    /// Attaches `projection` to `handle` using `component`.
    pub fn new(
        handle: AssetHandle,
        projection: Projection,
        component: impl Into<ComponentData>,
    ) -> Self {
        Self {
            handle,
            projection,
            component: Some(component.into()),
        }
    }
}

impl ReversibleTransaction<Arena> for AttachProjection {
    fn label(&self) -> String {
        format!("Project as {}", self.projection)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let kind = self
            .component
            .as_ref()
            .map(ComponentData::kind)
            .ok_or_else(|| TxnError::new("projection component is already attached"))?;
        // Validate first, so a failure keeps the component for a later retry.
        arena.check_attach(self.handle, self.projection, kind)?;
        if let Some(component) = self.component.take() {
            arena.attach_projection(self.handle, self.projection, component)?;
        }
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let component = arena.detach_projection(self.handle, self.projection)?;
        self.component = Some(component);
        Ok(())
    }
}

/// Several transactions as one undo step, applied in order and reverted in
/// reverse; a failure part way takes back what already went in.
pub struct Batch<T> {
    label: String,
    steps: Vec<Box<dyn ReversibleTransaction<T>>>,
}

impl<T> fmt::Debug for Batch<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Batch")
            .field("label", &self.label)
            .field("steps", &self.steps.len())
            .finish()
    }
}

impl<T> Batch<T> {
    /// An empty batch.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            steps: Vec::new(),
        }
    }

    /// Adds a step.
    #[must_use]
    pub fn then<X: ReversibleTransaction<T> + 'static>(mut self, step: X) -> Self {
        self.steps.push(Box::new(step));
        self
    }

    /// Adds a boxed step.
    pub fn push(&mut self, step: Box<dyn ReversibleTransaction<T>>) {
        self.steps.push(step);
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// `true` if there are no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl<T> ReversibleTransaction<T> for Batch<T> {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, target: &mut T) -> Result<(), TxnError> {
        for done in 0..self.steps.len() {
            if let Err(error) = self.steps[done].apply(target) {
                for step in self.steps[..done].iter_mut().rev() {
                    let _ = step.revert(target);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn revert(&mut self, target: &mut T) -> Result<(), TxnError> {
        let count = self.steps.len();
        for undone in 0..count {
            let index = count - 1 - undone;
            if let Err(error) = self.steps[index].revert(target) {
                for step in &mut self.steps[index + 1..] {
                    let _ = step.apply(target);
                }
                return Err(error);
            }
        }
        Ok(())
    }
}

fn handle_of(arena: &Arena, id: EntityId) -> Result<AssetHandle, CoreError> {
    arena.handle_of(id).ok_or(CoreError::UnknownId(id))
}

/// Creates an entity (and, optionally, relations to it). Undo removes it and
/// keeps it, so redo brings back the very same buffers under the same id.
#[derive(Debug)]
pub struct SpawnEntity {
    id: EntityId,
    name: String,
    entity: Option<Entity>,
    relations: Vec<Relation>,
    handle: Option<AssetHandle>,
}

impl SpawnEntity {
    /// Spawns `entity`.
    #[must_use]
    pub fn new(entity: Entity) -> Self {
        Self {
            id: entity.id(),
            name: entity.name().to_owned(),
            entity: Some(entity),
            relations: Vec::new(),
            handle: None,
        }
    }

    /// Also adds `relation` (e.g. to its parent) in the same step.
    #[must_use]
    pub fn with_relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }

    /// The new entity's id.
    #[must_use]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Its handle, once applied.
    #[must_use]
    pub fn handle(&self) -> Option<AssetHandle> {
        self.handle
    }
}

impl ReversibleTransaction<Arena> for SpawnEntity {
    fn label(&self) -> String {
        format!("New {}", self.name)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        if self.id.is_nil() || arena.handle_of(self.id).is_some() {
            return Err(CoreError::DuplicateId(self.id).into());
        }
        let entity = self
            .entity
            .take()
            .ok_or_else(|| TxnError::new("the entity already exists"))?;
        let handle = match self.handle {
            Some(previous) => arena.restore(previous, entity),
            None => arena.insert_keep_id(entity)?,
        };
        for (done, relation) in self.relations.iter().enumerate() {
            if let Err(error) = arena.relate(relation.clone()) {
                for relation in &self.relations[..done] {
                    arena.unrelate(relation.from, &relation.kind, relation.to);
                }
                self.entity = arena.despawn(handle).ok();
                return Err(error.into());
            }
        }
        self.handle = Some(handle);
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let handle = handle_of(arena, self.id)?;
        let (entity, relations) = arena.despawn_with_relations(handle)?;
        self.entity = Some(entity);
        self.relations = relations;
        self.handle = Some(handle);
        Ok(())
    }
}

/// Removes an entity and the relations that touch it. Undo restores both.
#[derive(Debug)]
pub struct DespawnEntity {
    id: EntityId,
    stash: Option<(AssetHandle, Entity, Vec<Relation>)>,
}

impl DespawnEntity {
    /// Removes the entity with `id`.
    #[must_use]
    pub fn new(id: EntityId) -> Self {
        Self { id, stash: None }
    }
}

impl ReversibleTransaction<Arena> for DespawnEntity {
    fn label(&self) -> String {
        match &self.stash {
            Some((_, entity, _)) => format!("Delete {}", entity.name()),
            None => "Delete".to_owned(),
        }
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let handle = handle_of(arena, self.id)?;
        let (entity, relations) = arena.despawn_with_relations(handle)?;
        self.stash = Some((handle, entity, relations));
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let (handle, entity, relations) = self
            .stash
            .take()
            .ok_or_else(|| TxnError::new("nothing to restore"))?;
        arena.restore(handle, entity);
        for relation in relations {
            arena.relate(relation)?;
        }
        Ok(())
    }
}

/// Sets (or, with `None`, removes) one property of an entity.
#[derive(Debug)]
pub struct SetProperty {
    id: EntityId,
    key: String,
    value: Option<Value>,
}

impl SetProperty {
    /// Sets `key` of `id` to `value`.
    pub fn new(id: EntityId, key: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            id,
            key: key.into(),
            value: Some(value.into()),
        }
    }

    /// Removes `key` of `id`.
    pub fn remove(id: EntityId, key: impl Into<String>) -> Self {
        Self {
            id,
            key: key.into(),
            value: None,
        }
    }

    // Applying and reverting are the same swap.
    fn swap(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let properties = arena.get_by_id_mut(self.id)?.properties_mut();
        self.value = match self.value.take() {
            Some(value) => properties.set(self.key.clone(), value),
            None => properties.remove(&self.key),
        };
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for SetProperty {
    fn label(&self) -> String {
        format!("Set {}", self.key)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }
}

/// Sets (or, with `None`, removes) one metadata entry of an entity.
#[derive(Debug)]
pub struct SetMeta {
    id: EntityId,
    key: String,
    value: Option<String>,
}

impl SetMeta {
    /// Sets `key` of `id` to `value`.
    pub fn new(id: EntityId, key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id,
            key: key.into(),
            value: Some(value.into()),
        }
    }

    /// Removes `key` of `id`.
    pub fn remove(id: EntityId, key: impl Into<String>) -> Self {
        Self {
            id,
            key: key.into(),
            value: None,
        }
    }

    fn swap(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let entity = arena.get_by_id_mut(self.id)?;
        self.value = match self.value.take() {
            Some(value) => entity.set_meta(self.key.clone(), value),
            None => entity.remove_meta(&self.key),
        };
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for SetMeta {
    fn label(&self) -> String {
        format!("Set {}", self.key)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }
}

/// Renames an entity.
#[derive(Debug)]
pub struct Rename {
    id: EntityId,
    name: String,
}

impl Rename {
    /// Renames `id` to `name`.
    pub fn new(id: EntityId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    fn swap(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let entity = arena.get_by_id_mut(self.id)?;
        self.name = entity.set_name(std::mem::take(&mut self.name));
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for Rename {
    fn label(&self) -> String {
        format!("Rename to {}", self.name)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }
}

/// Adds a relation, or updates its order and data.
#[derive(Debug)]
pub struct Relate {
    relation: Relation,
    previous: Option<Relation>,
}

impl Relate {
    /// Adds `relation`.
    #[must_use]
    pub fn new(relation: Relation) -> Self {
        Self {
            relation,
            previous: None,
        }
    }
}

impl ReversibleTransaction<Arena> for Relate {
    fn label(&self) -> String {
        format!("Link ({})", self.relation.kind)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.previous = arena.relate(self.relation.clone())?;
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let relation = &self.relation;
        match self.previous.take() {
            Some(previous) => {
                arena.relate(previous)?;
            }
            None => {
                arena.unrelate(relation.from, &relation.kind, relation.to);
            }
        }
        Ok(())
    }
}

/// Removes a relation.
#[derive(Debug)]
pub struct Unrelate {
    from: EntityId,
    kind: String,
    to: EntityId,
    removed: Option<Relation>,
}

impl Unrelate {
    /// Removes the `kind` edge from `from` to `to`.
    pub fn new(from: EntityId, kind: impl Into<String>, to: EntityId) -> Self {
        Self {
            from,
            kind: kind.into(),
            to,
            removed: None,
        }
    }
}

impl ReversibleTransaction<Arena> for Unrelate {
    fn label(&self) -> String {
        format!("Unlink ({})", self.kind)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let removed = arena
            .unrelate(self.from, &self.kind, self.to)
            .ok_or_else(|| TxnError::new(format!("there is no {} link to remove", self.kind)))?;
        self.removed = Some(removed);
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let removed = self
            .removed
            .take()
            .ok_or_else(|| TxnError::new("nothing to relink"))?;
        arena.relate(removed)?;
        Ok(())
    }
}

/// Adds, replaces or removes an extension component.
#[derive(Debug)]
pub struct SetExtension {
    id: EntityId,
    name: String,
    component: Option<Box<dyn Component>>,
}

impl SetExtension {
    /// Adds or replaces `component` on `id`.
    pub fn set(id: EntityId, component: impl Component) -> Self {
        Self::set_boxed(id, Box::new(component))
    }

    /// Adds or replaces a boxed component on `id`.
    #[must_use]
    pub fn set_boxed(id: EntityId, component: Box<dyn Component>) -> Self {
        Self {
            id,
            name: component.type_name().to_owned(),
            component: Some(component),
        }
    }

    /// Removes the component named `name` from `id`.
    pub fn remove(id: EntityId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            component: None,
        }
    }

    fn swap(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let entity = arena.get_by_id_mut(self.id)?;
        self.component = match self.component.take() {
            Some(component) => entity.set_ext(component),
            None => entity.remove_ext(&self.name),
        };
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for SetExtension {
    fn label(&self) -> String {
        format!("Edit {}", self.name)
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.swap(arena)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CHILD, OpaqueComponent, TimeSpan, UndoStack};

    fn text(arena: &Arena, handle: AssetHandle) -> &str {
        arena.get(handle).unwrap().string_buffer().unwrap().as_str()
    }

    #[test]
    fn text_edits_round_trip_through_the_undo_tree() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", StringBuffer::from("Faust"));
        let mut history = UndoStack::new();
        history
            .apply(&mut arena, InsertText::new(doc, 5, "ian"))
            .unwrap();
        history
            .apply(&mut arena, DeleteText::new(doc, 0..1))
            .unwrap();
        assert_eq!(text(&arena, doc), "austian");
        history.undo(&mut arena).unwrap();
        assert_eq!(text(&arena, doc), "Faustian");
        history.undo(&mut arena).unwrap();
        assert_eq!(text(&arena, doc), "Faust");
        history.redo(&mut arena).unwrap();
        history.redo(&mut arena).unwrap();
        assert_eq!(text(&arena, doc), "austian");
        assert_eq!(
            history.history(),
            vec!["Insert 3 character(s)", "Delete 1 byte(s)"]
        );
    }

    #[test]
    fn grouped_typing_undoes_and_redoes_as_one_step() {
        let mut arena = Arena::new();
        let note = arena.spawn("note", StringBuffer::from("ab".to_owned()));
        // The editor already typed "X" at 1 and deleted "b" (now at 2).
        arena
            .get_mut(note)
            .unwrap()
            .string_buffer_mut()
            .unwrap()
            .insert(1, "X")
            .unwrap();
        arena
            .get_mut(note)
            .unwrap()
            .string_buffer_mut()
            .unwrap()
            .remove(2..3)
            .unwrap();
        let mut group = TextEdits::new(note);
        group.push(TextOp::Insert {
            offset: 1,
            text: "X".to_owned(),
        });
        group.push(TextOp::Delete {
            offset: 2,
            text: "b".to_owned(),
        });
        let mut history: UndoStack<Arena> = UndoStack::new();
        history.record(group);
        assert_eq!(text(&arena, note), "aX");
        history.undo(&mut arena).unwrap();
        assert_eq!(text(&arena, note), "ab");
        history.redo(&mut arena).unwrap();
        assert_eq!(text(&arena, note), "aX");
    }

    #[test]
    fn invalid_edits_leave_text_untouched() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", StringBuffer::from("é"));
        let mut history = UndoStack::new();
        assert!(
            history
                .apply(&mut arena, InsertText::new(doc, 1, "x"))
                .is_err()
        );
        assert!(
            history
                .apply(&mut arena, DeleteText::new(doc, 0..1))
                .is_err()
        );
        assert_eq!(text(&arena, doc), "é");
        assert!(history.is_empty());
    }

    #[test]
    fn projections_attach_and_detach_reversibly() {
        let mut arena = Arena::new();
        let doc = arena.spawn("script", StringBuffer::from("INT. LAB - NIGHT"));
        let mut history = UndoStack::new();
        let span = TimeSpan::from_seconds(0.0, 5.0).unwrap();
        history
            .apply(
                &mut arena,
                AttachProjection::new(doc, Projection::Timeline, span),
            )
            .unwrap();
        assert!(
            arena
                .get(doc)
                .unwrap()
                .projections()
                .contains(Projection::Timeline)
        );
        history.undo(&mut arena).unwrap();
        assert!(
            !arena
                .get(doc)
                .unwrap()
                .projections()
                .contains(Projection::Timeline)
        );
        history.redo(&mut arena).unwrap();
        let entity = arena.get(doc).unwrap();
        let attached = entity.component(ComponentKind::TimeSpan).unwrap();
        assert_eq!(attached.as_time_span(), Some(span));
    }

    #[test]
    fn generic_edits_undo_and_redo_by_id() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let folder = Entity::empty("Album", "sue.folder");
        let folder_id = folder.id();
        history.apply(&mut arena, SpawnEntity::new(folder)).unwrap();
        let song = Entity::new("Song", StringBuffer::from("la la"));
        let song_id = song.id();
        history
            .apply(
                &mut arena,
                SpawnEntity::new(song).with_relation(Relation::new(folder_id, CHILD, song_id)),
            )
            .unwrap();
        history
            .apply(&mut arena, SetProperty::new(song_id, "audio.bpm", 120.0))
            .unwrap();
        history
            .apply(&mut arena, SetMeta::new(song_id, "source", "a.wav"))
            .unwrap();
        history
            .apply(&mut arena, Rename::new(song_id, "Hit"))
            .unwrap();
        history
            .apply(
                &mut arena,
                SetExtension::set(
                    song_id,
                    OpaqueComponent {
                        type_name: "x.y".into(),
                        version: 1,
                        bytes: vec![1],
                    },
                ),
            )
            .unwrap();
        let after = arena.get_by_id(song_id).unwrap().clone();
        assert_eq!(after.name(), "Hit");
        assert_eq!(after.properties().f64("audio.bpm"), Some(120.0));
        assert!(after.ext_named("x.y").is_some());

        // Delete the folder: its child link goes with it and comes back.
        history
            .apply(&mut arena, DespawnEntity::new(folder_id))
            .unwrap();
        assert!(arena.relations().is_empty());
        history.undo(&mut arena).unwrap();
        assert_eq!(arena.relations().children(folder_id), vec![song_id]);

        while history.undo(&mut arena).unwrap() {}
        assert!(arena.is_empty());
        while history.redo(&mut arena).unwrap() {}
        assert_eq!(arena.get_by_id(song_id).unwrap(), &after);
        assert!(arena.get_by_id(folder_id).is_err(), "the delete was redone");
        history.undo(&mut arena).unwrap();
        assert_eq!(arena.relations().parent(song_id, CHILD), Some(folder_id));
    }

    #[test]
    fn relations_and_batches_are_atomic() {
        let mut arena = Arena::new();
        let a = arena.insert(Entity::empty("a", "sue.group"));
        let b = arena.insert(Entity::empty("b", "sue.group"));
        let (a, b) = (arena.get(a).unwrap().id(), arena.get(b).unwrap().id());
        let mut history = UndoStack::new();
        history
            .apply(&mut arena, Relate::new(Relation::new(a, CHILD, b)))
            .unwrap();
        // The second step would make a cycle, so the whole batch fails.
        let batch = Batch::new("Bad")
            .then(SetProperty::new(a, "k", 1))
            .then(Relate::new(Relation::new(b, CHILD, a)));
        assert!(history.apply(&mut arena, batch).is_err());
        assert!(arena.get_by_id(a).unwrap().property("k").is_none());
        history
            .apply(&mut arena, Unrelate::new(a, CHILD, b))
            .unwrap();
        assert!(arena.relations().is_empty());
        history.undo(&mut arena).unwrap();
        history.undo(&mut arena).unwrap();
        assert!(arena.relations().is_empty());
        assert!(
            history
                .apply(&mut arena, Unrelate::new(a, CHILD, b))
                .is_err()
        );
    }

    #[test]
    fn mismatched_projection_is_rejected_without_losing_the_component() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", StringBuffer::new());
        let mut txn = AttachProjection::new(doc, Projection::Audio, StringBuffer::new());
        assert!(txn.apply(&mut arena).is_err());
        assert!(txn.component.is_some());
    }
}
