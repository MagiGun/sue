//! The undoable world: the data arena, the live rice and the canvas scene,
//! under one history. Every change to any of them is a
//! [`ReversibleTransaction`] that stores its inverse delta (backbone rule 2).

use egui::Rect;
use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{
    Arena, AssetHandle, Chunk, ComponentData, ComponentRegistry, Entity, ReversibleTransaction,
    SueFile, TxnError,
};
use sue_rice::RiceSchema;

use crate::scene::SCENE_CHUNK;
use crate::{AppletId, NodeId, Scene, SceneNode};

/// Entities whose class starts with this belong to the app itself (the quick
/// note), not to the document: saving leaves them out, opening keeps them.
pub const APP_CLASS_PREFIX: &str = "sue.app.";

fn is_app(entity: &Entity) -> bool {
    entity.class().starts_with(APP_CLASS_PREFIX)
}

/// Everything the global undo tree can change.
/// One `UndoStack<World>` makes `Ctrl+Z` undo typing, a moved entity and a
/// slider in Settings alike.
#[derive(Debug, Default)]
pub struct World {
    /// The shared data arena.
    pub arena: Arena,
    /// The live rice.
    pub rice: RiceSchema,
    /// Where entities sit on the canvas.
    pub scene: Scene,
}

impl World {
    /// Writes the document (entities, relations and the canvas layout) into
    /// `file`, keeping every other chunk it already has.
    pub fn write_into(&self, file: &mut SueFile) {
        file.set_arena_where(&self.arena, &|entity| !is_app(entity));
        let mut out = Writer::new();
        self.scene.encode(&self.arena, &mut out);
        file.set(Chunk {
            tag: SCENE_CHUNK,
            version: 1,
            payload: out.into_bytes(),
        });
    }

    /// Replaces the document with the one in `file`. App entities stay;
    /// the rice is untouched. Old handles go stale (never reused for the new
    /// entities), so the caller must start a new undo history.
    ///
    /// # Errors
    /// If the file is malformed; the world is unchanged then.
    pub fn load_file(
        &mut self,
        file: &SueFile,
        registry: &ComponentRegistry,
    ) -> Result<(), CodecError> {
        let mut loaded = file.arena(registry)?;
        let layout = file
            .chunk(SCENE_CHUNK)
            .map(|chunk| chunk.payload.as_slice());
        if let Some(layout) = layout {
            Scene::decode(&mut Reader::new(layout), &loaded)?;
        }
        let current: Vec<AssetHandle> = self
            .arena
            .iter()
            .filter(|(_, entity)| !is_app(entity))
            .map(|(handle, _)| handle)
            .collect();
        for handle in current {
            let _ = self.arena.despawn(handle);
        }
        let kinds: Vec<String> = loaded.relations().tree_kinds().map(str::to_owned).collect();
        for kind in kinds {
            self.arena.add_tree_kind(kind);
        }
        let relations: Vec<_> = loaded.relations().iter().collect();
        let handles: Vec<AssetHandle> = loaded.iter().map(|(handle, _)| handle).collect();
        for handle in handles {
            if let Ok(entity) = loaded.despawn(handle) {
                self.arena.insert(entity);
            }
        }
        for relation in relations {
            let _ = self.arena.relate(relation);
        }
        self.scene = match layout {
            Some(layout) => Scene::decode(&mut Reader::new(layout), &self.arena)?,
            None => Scene::default(),
        };
        Ok(())
    }
}

/// Lifts an arena transaction onto the [`World`].
#[derive(Debug)]
pub struct ArenaEdit<X>(pub X);

impl<X: ReversibleTransaction<Arena>> ReversibleTransaction<World> for ArenaEdit<X> {
    fn label(&self) -> String {
        self.0.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.apply(&mut world.arena)
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.revert(&mut world.arena)
    }
}

/// Lifts a rice transaction onto the [`World`].
#[derive(Debug)]
pub struct RiceEdit<X>(pub X);

impl<X: ReversibleTransaction<RiceSchema>> ReversibleTransaction<World> for RiceEdit<X> {
    fn label(&self) -> String {
        self.0.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.apply(&mut world.rice)
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.revert(&mut world.rice)
    }
}

/// Several world transactions as one undo step (the kernel's generic
/// [`sue_core::txn::Batch`]).
pub type Batch = sue_core::txn::Batch<World>;

/// What a new entity starts with.
#[derive(Debug)]
pub struct NewEntity {
    /// Display name.
    pub name: String,
    /// The base component (moved into the arena, never copied).
    pub base: ComponentData,
    /// Metadata, as key and value.
    pub meta: Vec<(String, String)>,
    /// Size of its node, in canvas units.
    pub size: egui::Vec2,
    /// Initial node properties.
    pub props: Vec<(String, f64)>,
}

/// Creates an entity and places it on the canvas. Undo removes both and
/// keeps the entity, so redo brings back the very same buffers.
#[derive(Debug)]
pub struct SpawnNode {
    name: String,
    pending: Option<ComponentData>,
    meta: Vec<(String, String)>,
    applet: AppletId,
    rect: Rect,
    props: Vec<(String, f64)>,
    stash: Option<(AssetHandle, Entity)>,
    placed: Option<(NodeId, AssetHandle)>,
    id: Option<NodeId>,
}

impl SpawnNode {
    /// Spawns `entity` drawn by `applet` at `rect`.
    #[must_use]
    pub fn new(entity: NewEntity, applet: AppletId, rect: Rect) -> Self {
        Self {
            name: entity.name,
            pending: Some(entity.base),
            meta: entity.meta,
            applet,
            rect,
            props: entity.props,
            stash: None,
            placed: None,
            id: None,
        }
    }

    /// The node and entity, once applied.
    #[must_use]
    pub fn placed(&self) -> Option<(NodeId, AssetHandle)> {
        self.placed
    }
}

impl ReversibleTransaction<World> for SpawnNode {
    fn label(&self) -> String {
        format!("New {}", self.name)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        let handle = if let Some((handle, entity)) = self.stash.take() {
            world.arena.restore(handle, entity)
        } else {
            let base = self
                .pending
                .take()
                .ok_or_else(|| TxnError::new("the entity was already created"))?;
            let handle = world.arena.spawn(self.name.clone(), base);
            if let Ok(entity) = world.arena.get_mut(handle) {
                for (key, value) in &self.meta {
                    entity.set_meta(key.clone(), value.clone());
                }
            }
            handle
        };
        let id = *self.id.get_or_insert_with(|| world.scene.allocate());
        let mut node = SceneNode::new(handle, self.applet.clone(), self.rect);
        node.props.extend(self.props.iter().cloned());
        world.scene.insert(id, node);
        self.placed = Some((id, handle));
        Ok(())
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        let (id, handle) = self
            .placed
            .take()
            .ok_or_else(|| TxnError::new("the entity was never created"))?;
        world.scene.remove(id);
        self.stash = Some((handle, world.arena.despawn(handle)?));
        Ok(())
    }
}

/// Places an entity already in the arena (an opened file, a split clip) on
/// the canvas.
#[derive(Debug)]
pub struct PlaceNode {
    node: SceneNode,
    id: Option<NodeId>,
}

impl PlaceNode {
    /// Places `node`.
    #[must_use]
    pub fn new(node: SceneNode) -> Self {
        Self { node, id: None }
    }

    /// Places `node` under a known id.
    #[must_use]
    pub fn with_id(node: SceneNode, id: NodeId) -> Self {
        Self { node, id: Some(id) }
    }

    /// The node id, once applied.
    #[must_use]
    pub fn id(&self) -> Option<NodeId> {
        self.id
    }
}

impl ReversibleTransaction<World> for PlaceNode {
    fn label(&self) -> String {
        "Place on canvas".to_owned()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        world.arena.get(self.node.entity).map_err(TxnError::from)?;
        let id = *self.id.get_or_insert_with(|| world.scene.allocate());
        world.scene.insert(id, self.node.clone());
        Ok(())
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        let id = self
            .id
            .ok_or_else(|| TxnError::new("the node was never placed"))?;
        world
            .scene
            .remove(id)
            .ok_or_else(|| TxnError::new("the node is gone"))?;
        Ok(())
    }
}

/// Takes a node off the canvas; the entity stays in the arena.
#[derive(Debug)]
pub struct RemoveNode {
    id: NodeId,
    removed: Option<SceneNode>,
}

impl RemoveNode {
    /// Removes node `id`.
    #[must_use]
    pub fn new(id: NodeId) -> Self {
        Self { id, removed: None }
    }
}

impl ReversibleTransaction<World> for RemoveNode {
    fn label(&self) -> String {
        "Remove from canvas".to_owned()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.removed = Some(
            world
                .scene
                .remove(self.id)
                .ok_or_else(|| TxnError::new("the node is gone"))?,
        );
        Ok(())
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        let node = self
            .removed
            .take()
            .ok_or_else(|| TxnError::new("nothing was removed"))?;
        world.scene.insert(self.id, node);
        Ok(())
    }
}

/// Moves or resizes a node.
#[derive(Debug)]
pub struct SetNodeRect {
    id: NodeId,
    from: Rect,
    to: Rect,
}

impl SetNodeRect {
    /// Moves node `id` from `from` to `to`.
    #[must_use]
    pub fn new(id: NodeId, from: Rect, to: Rect) -> Self {
        Self { id, from, to }
    }
}

impl ReversibleTransaction<World> for SetNodeRect {
    fn label(&self) -> String {
        if self.from.size() == self.to.size() {
            "Move".to_owned()
        } else {
            "Resize".to_owned()
        }
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        let node = world
            .scene
            .get_mut(self.id)
            .ok_or_else(|| TxnError::new("the node is gone"))?;
        node.rect = self.to;
        Ok(())
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        let node = world
            .scene
            .get_mut(self.id)
            .ok_or_else(|| TxnError::new("the node is gone"))?;
        node.rect = self.from;
        Ok(())
    }
}

/// Sets (or clears) a node property.
#[derive(Debug)]
pub struct SetNodeProp {
    id: NodeId,
    key: String,
    to: Option<f64>,
    from: Option<Option<f64>>,
    label: String,
}

impl SetNodeProp {
    /// Sets `key` of node `id` to `to` (`None` clears it).
    #[must_use]
    pub fn new(
        id: NodeId,
        key: impl Into<String>,
        to: Option<f64>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id,
            key: key.into(),
            to,
            from: None,
            label: label.into(),
        }
    }
}

impl ReversibleTransaction<World> for SetNodeProp {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        let node = world
            .scene
            .get_mut(self.id)
            .ok_or_else(|| TxnError::new("the node is gone"))?;
        let old = match self.to {
            Some(value) => node.props.insert(self.key.clone(), value),
            None => node.props.remove(&self.key),
        };
        self.from = Some(old);
        Ok(())
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        let node = world
            .scene
            .get_mut(self.id)
            .ok_or_else(|| TxnError::new("the node is gone"))?;
        match self.from.take().flatten() {
            Some(value) => node.props.insert(self.key.clone(), value),
            None => node.props.remove(&self.key),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};
    use sue_core::txn::InsertText;
    use sue_core::{StringBuffer, UndoStack};
    use sue_rice::{RiceValue, SetRiceValue};

    #[test]
    fn documents_save_and_open_without_the_app_entities() {
        let place = |world: &mut World, handle| {
            let id = world.scene.allocate();
            let node = SceneNode::new(
                handle,
                AppletId::new("document").unwrap(),
                Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            );
            world.scene.insert(id, node);
        };
        let mut world = World::default();
        let note = world.arena.spawn("note", StringBuffer::from("mine"));
        world.arena.get_mut(note).unwrap().set_class("sue.app.note");
        let doc = world.arena.spawn("doc", StringBuffer::from("shared"));
        let doc_id = world.arena.get(doc).unwrap().id();
        place(&mut world, doc);
        let mut file = SueFile::new();
        file.set(Chunk {
            tag: *b"XTRA",
            version: 1,
            payload: vec![7],
        });
        world.write_into(&mut file);
        let bytes = file.to_bytes();

        let mut other = World::default();
        let their_note = other.arena.spawn("note", StringBuffer::from("theirs"));
        other
            .arena
            .get_mut(their_note)
            .unwrap()
            .set_class("sue.app.note");
        let scratch = other.arena.spawn("scratch", StringBuffer::new());
        place(&mut other, scratch);
        let reopened = SueFile::from_bytes(&bytes).unwrap();
        other
            .load_file(&reopened, &ComponentRegistry::new())
            .unwrap();
        assert_eq!(other.arena.len(), 2, "their note and the document");
        assert!(other.arena.get(their_note).is_ok(), "app entities stay");
        assert!(!other.arena.contains(scratch));
        let loaded = other.arena.handle_of(doc_id).unwrap();
        assert_ne!(loaded, doc, "new handles never alias old ones");
        assert_eq!(other.scene.len(), 1);
        assert_eq!(other.scene.iter().next().unwrap().1.entity, loaded);
        assert_eq!(reopened.chunk(*b"XTRA").unwrap().payload, [7]);
        // An empty project leaves only the app's own entities.
        other
            .load_file(&SueFile::new(), &ComponentRegistry::new())
            .unwrap();
        assert_eq!(other.arena.len(), 1);
        assert!(other.scene.is_empty());
    }

    #[test]
    fn one_history_undoes_data_theme_and_canvas_edits_in_order() {
        let mut world = World {
            rice: RiceSchema::parse("[layout]\ngap = 8 # [0..64]\n").unwrap(),
            ..World::default()
        };
        let mut history = UndoStack::new();
        let spawn = SpawnNode::new(
            NewEntity {
                name: "Note".to_owned(),
                base: StringBuffer::new().into(),
                meta: Vec::new(),
                size: vec2(200.0, 100.0),
                props: vec![("font_size".to_owned(), 15.0)],
            },
            AppletId::new("document").unwrap(),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0)),
        );
        history.apply(&mut world, spawn).unwrap();
        let (id, handle) = world
            .scene
            .iter()
            .map(|(id, node)| (id, node.entity))
            .next()
            .unwrap();
        history
            .apply(&mut world, ArenaEdit(InsertText::new(handle, 0, "hi")))
            .unwrap();
        let moved = Rect::from_min_size(pos2(50.0, 0.0), vec2(200.0, 100.0));
        let from = world.scene.get(id).unwrap().rect;
        history
            .apply(&mut world, SetNodeRect::new(id, from, moved))
            .unwrap();
        history
            .apply(
                &mut world,
                RiceEdit(SetRiceValue::new("layout.gap", RiceValue::Int(20))),
            )
            .unwrap();
        assert_eq!(world.rice.get_i64("layout.gap"), Some(20));
        history.undo(&mut world).unwrap();
        assert_eq!(world.rice.get_i64("layout.gap"), Some(8));
        history.undo(&mut world).unwrap();
        assert_eq!(world.scene.get(id).unwrap().rect.left(), 0.0);
        history.undo(&mut world).unwrap();
        history.undo(&mut world).unwrap();
        assert!(world.scene.is_empty());
        assert!(world.arena.is_empty());
        // Redo brings back the same node and the same buffer.
        history.redo(&mut world).unwrap();
        history.redo(&mut world).unwrap();
        let node = world.scene.get(id).unwrap();
        assert_eq!(node.prop("font_size", 0.0), 15.0);
        let text = world
            .arena
            .get(node.entity)
            .unwrap()
            .string_buffer()
            .unwrap();
        assert_eq!(text.as_str(), "hi");
    }

    #[test]
    fn batches_are_one_step_and_atomic() {
        let mut world = World::default();
        let handle = world.arena.spawn("clip", StringBuffer::new());
        let node = SceneNode::new(
            handle,
            AppletId::new("audio").unwrap(),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
        );
        let mut history = UndoStack::new();
        history
            .apply(&mut world, PlaceNode::new(node.clone()))
            .unwrap();
        let first = world.scene.iter().next().unwrap().0;
        let split = Batch::new("Split")
            .then(SetNodeProp::new(first, "clip_end", Some(5.0), "Trim"))
            .then(PlaceNode::new(node));
        history.apply(&mut world, split).unwrap();
        assert_eq!(world.scene.len(), 2);
        history.undo(&mut world).unwrap();
        assert_eq!(world.scene.len(), 1);
        assert_eq!(world.scene.get(first).unwrap().props.get("clip_end"), None);
        // A failing step leaves nothing behind.
        let broken = Batch::new("Broken")
            .then(SetNodeProp::new(first, "gain", Some(2.0), "Gain"))
            .then(RemoveNode::new(NodeId(999)));
        assert!(history.apply(&mut world, broken).is_err());
        assert_eq!(world.scene.get(first).unwrap().props.get("gain"), None);
    }
}
