//! The canvas scene: where each entity sits on the infinite canvas. A node
//! holds an [`AssetHandle`] into the arena, never the media itself (backbone rule 5),
//! plus the applet that draws it and a few numeric properties (a document's
//! text size, an audio clip's range and gain, its place on the timeline).
//! Several nodes may show one entity (a split audio clip).

use std::collections::BTreeMap;

use egui::Rect;
use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{Arena, AssetHandle, EntityId};

use crate::AppletId;

/// The tag of the chunk that holds the canvas layout in `.sue` files.
pub const SCENE_CHUNK: [u8; 4] = *b"SCNE";

/// A node on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// One entity placed on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    /// The entity it shows.
    pub entity: AssetHandle,
    /// The applet that draws it.
    pub applet: AppletId,
    /// Where it sits, in canvas units.
    pub rect: Rect,
    /// Numeric properties, by name.
    pub props: BTreeMap<String, f64>,
    /// Stacking order; higher draws on top.
    pub z: u64,
}

impl SceneNode {
    /// A node for `entity` drawn by `applet` at `rect`.
    #[must_use]
    pub fn new(entity: AssetHandle, applet: AppletId, rect: Rect) -> Self {
        Self {
            entity,
            applet,
            rect,
            props: BTreeMap::new(),
            z: 0,
        }
    }

    /// A property, or `fallback`.
    #[must_use]
    pub fn prop(&self, key: &str, fallback: f64) -> f64 {
        self.props.get(key).copied().unwrap_or(fallback)
    }
}

/// Every node on the canvas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    nodes: BTreeMap<NodeId, SceneNode>,
    next_id: u64,
    next_z: u64,
}

impl Scene {
    /// A fresh id for a node about to be inserted.
    pub fn allocate(&mut self) -> NodeId {
        self.next_id += 1;
        NodeId(self.next_id)
    }

    /// Inserts `node` under `id`, on top of everything.
    pub fn insert(&mut self, id: NodeId, mut node: SceneNode) {
        self.next_id = self.next_id.max(id.0);
        self.next_z += 1;
        node.z = self.next_z;
        self.nodes.insert(id, node);
    }

    /// Removes a node.
    pub fn remove(&mut self, id: NodeId) -> Option<SceneNode> {
        self.nodes.remove(&id)
    }

    /// A node.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&SceneNode> {
        self.nodes.get(&id)
    }

    /// A node, mutably.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut SceneNode> {
        self.nodes.get_mut(&id)
    }

    /// Moves a node to the top of the stack.
    pub fn raise(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(&id)
            && node.z != self.next_z
        {
            self.next_z += 1;
            node.z = self.next_z;
        }
    }

    /// Node ids from bottom to top.
    #[must_use]
    pub fn stacked(&self) -> Vec<NodeId> {
        let mut ids: Vec<(u64, NodeId)> =
            self.nodes.iter().map(|(id, node)| (node.z, *id)).collect();
        ids.sort_unstable();
        ids.into_iter().map(|(_, id)| id).collect()
    }

    /// Every node, by id.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &SceneNode)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` without nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The box around every node.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        self.nodes
            .values()
            .map(|node| node.rect)
            .reduce(|all, rect| all.union(rect))
    }

    /// The topmost node drawn by `applet`.
    #[must_use]
    pub fn topmost_of(&self, applet: &AppletId) -> Option<NodeId> {
        self.stacked().into_iter().rev().find(|id| {
            self.nodes
                .get(id)
                .is_some_and(|node| &node.applet == applet)
        })
    }

    /// The nodes showing `entity`.
    pub fn showing(&self, entity: AssetHandle) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(move |(_, node)| node.entity == entity)
            .map(|(id, _)| *id)
    }

    /// Writes every node, bottom to top, naming its entity by stable id.
    /// Nodes whose entity is gone are left out.
    pub fn encode(&self, arena: &Arena, out: &mut Writer) {
        let nodes: Vec<(NodeId, &SceneNode, EntityId)> = self
            .stacked()
            .into_iter()
            .filter_map(|id| {
                let node = self.nodes.get(&id)?;
                let entity = arena.get(node.entity).ok()?.id();
                Some((id, node, entity))
            })
            .collect();
        out.len(nodes.len());
        for (id, node, entity) in nodes {
            out.u64(id.0);
            out.raw(&entity.to_bytes());
            out.str(node.applet.as_str());
            for value in [
                node.rect.min.x,
                node.rect.min.y,
                node.rect.max.x,
                node.rect.max.y,
            ] {
                out.f32(value);
            }
            out.len(node.props.len());
            for (key, value) in &node.props {
                out.str(key);
                out.f64(*value);
            }
        }
    }

    /// Reads a layout written by [`Self::encode`], finding each entity in
    /// `arena` by id. Nodes whose entity or applet is unknown are skipped.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(input: &mut Reader<'_>, arena: &Arena) -> Result<Self, CodecError> {
        let mut scene = Self::default();
        for _ in 0..input.len(41)? {
            let id = NodeId(input.u64()?);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(input.raw(16)?);
            let applet = input.str()?;
            let [x0, y0, x1, y1] = [input.f32()?, input.f32()?, input.f32()?, input.f32()?];
            let mut props = BTreeMap::new();
            for _ in 0..input.len(9)? {
                let key = input.str()?;
                props.insert(key, input.f64()?);
            }
            let handle = arena.handle_of(EntityId::from_bytes(bytes));
            if let (Some(handle), Ok(applet)) = (handle, AppletId::new(&applet)) {
                let mut node = SceneNode::new(
                    handle,
                    applet,
                    Rect::from_min_max((x0, y0).into(), (x1, y1).into()),
                );
                node.props = props;
                scene.insert(id, node);
            }
        }
        Ok(scene)
    }

    /// A free spot for a node of `size` near `center`, right of anything
    /// it would overlap.
    #[must_use]
    pub fn free_spot(&self, center: egui::Pos2, size: egui::Vec2, gap: f32) -> Rect {
        let mut rect = Rect::from_center_size(center, size);
        for _ in 0..64 {
            let blocker = self
                .nodes
                .values()
                .map(|node| node.rect)
                .find(|other| other.expand(gap * 0.5).intersects(rect));
            match blocker {
                Some(other) => {
                    rect = rect.translate(egui::vec2(other.right() + gap - rect.left(), 0.0))
                }
                None => break,
            }
        }
        rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn node(x: f32) -> SceneNode {
        SceneNode::new(
            AssetHandle::new(1, 1),
            AppletId::new("document").unwrap(),
            Rect::from_min_size(pos2(x, 0.0), vec2(100.0, 50.0)),
        )
    }

    #[test]
    fn nodes_stack_and_raise() {
        let mut scene = Scene::default();
        let a = scene.allocate();
        scene.insert(a, node(0.0));
        let b = scene.allocate();
        scene.insert(b, node(200.0));
        assert_eq!(scene.stacked(), [a, b]);
        scene.raise(a);
        assert_eq!(scene.stacked(), [b, a]);
        assert_eq!(scene.bounds().unwrap().width(), 300.0);
    }

    #[test]
    fn layouts_round_trip_by_entity_id() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", sue_core::StringBuffer::from("hi"));
        let mut scene = Scene::default();
        let a = scene.allocate();
        let mut first = SceneNode::new(
            doc,
            AppletId::new("document").unwrap(),
            Rect::from_min_size(pos2(1.0, 2.0), vec2(3.0, 4.0)),
        );
        first.props.insert("font_size".into(), 15.0);
        scene.insert(a, first);
        let b = scene.allocate();
        scene.insert(b, node(9.0));
        let mut out = Writer::new();
        scene.encode(&arena, &mut out);
        let bytes = out.into_bytes();
        // The second node's entity (a made-up handle) is not in the arena.
        let back = Scene::decode(&mut Reader::new(&bytes), &arena).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.get(a), scene.get(a));
    }

    #[test]
    fn free_spots_never_overlap() {
        let mut scene = Scene::default();
        let a = scene.allocate();
        scene.insert(a, node(0.0));
        let spot = scene.free_spot(pos2(50.0, 25.0), vec2(100.0, 50.0), 20.0);
        assert!(!spot.intersects(scene.get(a).unwrap().rect));
    }
}
