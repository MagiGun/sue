//! Node graphs. Compositing trees, shader and material graphs, audio
//! patches (modular synths, effect chains), procedural geometry, game logic
//! and AI pipelines are all nodes with typed ports and links. One
//! [`NodeGraph`] stores any of them; a [`NodeRegistry`] holds the node types
//! features contribute (ports, parameters), checks links and orders nodes
//! for evaluation. Evaluating is the feature's job; the shape is shared, so
//! one graph can mix nodes from several features.

use std::collections::{BTreeMap, BTreeSet};

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{
    Arena, ComponentType, CoreError, EntityId, Properties, PropertyDef, ReversibleTransaction,
    TxnError, Value,
};

/// A node's key inside its graph.
pub type NodeKey = u64;

/// One node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The registered node type, e.g. `comp.blur`.
    pub node_type: String,
    /// Display label (empty: use the type's).
    pub label: String,
    /// Position in the graph editor.
    pub position: [f32; 2],
    /// Parameter values.
    pub params: Properties,
}

impl Node {
    /// A node of `node_type` at `position`.
    pub fn new(node_type: impl Into<String>, position: [f32; 2]) -> Self {
        Self {
            node_type: node_type.into(),
            label: String::new(),
            position,
            params: Properties::new(),
        }
    }
}

/// A link from an output port to an input port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Link {
    /// Source node.
    pub from: NodeKey,
    /// Source output port.
    pub output: String,
    /// Target node.
    pub to: NodeKey,
    /// Target input port.
    pub input: String,
}

/// Nodes and links.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeGraph {
    nodes: BTreeMap<NodeKey, Node>,
    links: BTreeSet<Link>,
    next: NodeKey,
}

impl NodeGraph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node, returning its key.
    pub fn add(&mut self, node: Node) -> NodeKey {
        self.next += 1;
        self.nodes.insert(self.next, node);
        self.next
    }

    /// Puts a node back under a known key (undo).
    pub fn restore(&mut self, key: NodeKey, node: Node) {
        self.next = self.next.max(key);
        self.nodes.insert(key, node);
    }

    /// Removes a node and its links, returning both.
    pub fn remove(&mut self, key: NodeKey) -> Option<(Node, Vec<Link>)> {
        let node = self.nodes.remove(&key)?;
        let links: Vec<Link> = self
            .links
            .iter()
            .filter(|link| link.from == key || link.to == key)
            .cloned()
            .collect();
        for link in &links {
            self.links.remove(link);
        }
        Some((node, links))
    }

    /// The node with `key`.
    #[must_use]
    pub fn node(&self, key: NodeKey) -> Option<&Node> {
        self.nodes.get(&key)
    }

    /// The node with `key`, mutably.
    pub fn node_mut(&mut self, key: NodeKey) -> Option<&mut Node> {
        self.nodes.get_mut(&key)
    }

    /// Every node.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeKey, &Node)> {
        self.nodes.iter().map(|(key, node)| (*key, node))
    }

    /// Every link.
    pub fn links(&self) -> impl Iterator<Item = &Link> {
        self.links.iter()
    }

    /// The link feeding `input` of `node`, if any.
    #[must_use]
    pub fn input_link(&self, node: NodeKey, input: &str) -> Option<&Link> {
        self.links
            .iter()
            .find(|link| link.to == node && link.input == input)
    }

    /// Adds a link without checks, replacing whatever fed that input.
    /// Returns the replaced link.
    pub fn link(&mut self, link: Link) -> Option<Link> {
        let replaced = self.input_link(link.to, &link.input).cloned();
        if let Some(old) = &replaced {
            self.links.remove(old);
        }
        self.links.insert(link);
        replaced
    }

    /// Removes a link.
    pub fn unlink(&mut self, link: &Link) -> bool {
        self.links.remove(link)
    }

    /// Nodes in an order where every node comes after the nodes feeding it.
    ///
    /// # Errors
    /// The nodes on a cycle, if there is one.
    pub fn order(&self) -> Result<Vec<NodeKey>, Vec<NodeKey>> {
        let mut incoming: BTreeMap<NodeKey, usize> =
            self.nodes.keys().map(|key| (*key, 0)).collect();
        for link in &self.links {
            if let Some(count) = incoming.get_mut(&link.to) {
                *count += 1;
            }
        }
        let mut ready: Vec<NodeKey> = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(key, _)| *key)
            .rev()
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(key) = ready.pop() {
            order.push(key);
            for link in self.links.iter().filter(|link| link.from == key) {
                if let Some(count) = incoming.get_mut(&link.to) {
                    *count -= 1;
                    if *count == 0 {
                        ready.push(link.to);
                    }
                }
            }
        }
        if order.len() == self.nodes.len() {
            Ok(order)
        } else {
            let placed: BTreeSet<NodeKey> = order.into_iter().collect();
            Err(self
                .nodes
                .keys()
                .filter(|key| !placed.contains(key))
                .copied()
                .collect())
        }
    }
}

impl ComponentType for NodeGraph {
    const NAME: &'static str = "sue.graph";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.u64(self.next);
        out.len(self.nodes.len());
        for (key, node) in &self.nodes {
            out.u64(*key);
            out.str(&node.node_type);
            out.str(&node.label);
            out.f32(node.position[0]);
            out.f32(node.position[1]);
            node.params.encode(out);
        }
        out.len(self.links.len());
        for link in &self.links {
            out.u64(link.from);
            out.str(&link.output);
            out.u64(link.to);
            out.str(&link.input);
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut graph = Self {
            next: input.u64()?,
            ..Self::default()
        };
        for _ in 0..input.len(19)? {
            let key = input.u64()?;
            let node = Node {
                node_type: input.str()?,
                label: input.str()?,
                position: [input.f32()?, input.f32()?],
                params: Properties::decode(input)?,
            };
            graph.restore(key, node);
        }
        for _ in 0..input.len(18)? {
            let link = Link {
                from: input.u64()?,
                output: input.str()?,
                to: input.u64()?,
                input: input.str()?,
            };
            if !graph.nodes.contains_key(&link.from) || !graph.nodes.contains_key(&link.to) {
                return Err(input.error("a link names a missing node"));
            }
            graph.links.insert(link);
        }
        Ok(graph)
    }

    fn heap_bytes(&self) -> usize {
        self.nodes.len() * size_of::<Node>() + self.links.len() * size_of::<Link>()
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        for node in self.nodes.values_mut() {
            node.params
                .values_mut()
                .for_each(|value| value.remap_entities(map));
        }
    }
}

/// A port on a node type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    /// Name, unique per side.
    pub name: String,
    /// What flows through it, e.g. `image`, `audio`, `float`, `geometry`;
    /// `any` accepts everything.
    pub kind: String,
}

impl Port {
    /// A port.
    pub fn new(name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
        }
    }
}

/// What a feature registers for one kind of node.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeType {
    /// Unique, namespaced, e.g. `comp.blur`.
    pub name: String,
    /// Label in the node menu.
    pub label: String,
    /// Menu group.
    pub category: String,
    /// Inputs.
    pub inputs: Vec<Port>,
    /// Outputs.
    pub outputs: Vec<Port>,
    /// Parameters, with defaults.
    pub params: Vec<PropertyDef>,
}

impl NodeType {
    /// A node type with no ports.
    pub fn new(
        name: impl Into<String>,
        label: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            category: category.into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
        }
    }

    /// With an input.
    #[must_use]
    pub fn input(mut self, name: &str, kind: &str) -> Self {
        self.inputs.push(Port::new(name, kind));
        self
    }

    /// With an output.
    #[must_use]
    pub fn output(mut self, name: &str, kind: &str) -> Self {
        self.outputs.push(Port::new(name, kind));
        self
    }

    /// With a parameter.
    #[must_use]
    pub fn param(mut self, def: PropertyDef) -> Self {
        self.params.push(def);
        self
    }
}

/// Every node type, by name.
#[derive(Clone, Debug, Default)]
pub struct NodeRegistry {
    types: BTreeMap<String, NodeType>,
}

impl NodeRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a node type.
    pub fn register(&mut self, node_type: NodeType) {
        self.types.insert(node_type.name.clone(), node_type);
    }

    /// The type named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&NodeType> {
        self.types.get(name)
    }

    /// Every type in `category`.
    pub fn in_category<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a NodeType> + 'a {
        self.types
            .values()
            .filter(move |node_type| node_type.category == category)
    }

    /// A node of `name` with every parameter at its default.
    #[must_use]
    pub fn instantiate(&self, name: &str, position: [f32; 2]) -> Option<Node> {
        let node_type = self.get(name)?;
        let mut node = Node::new(name, position);
        for def in &node_type.params {
            node.params.set(def.key.clone(), def.default.clone());
        }
        Some(node)
    }

    /// Checks that `link` joins an existing output to an existing input of
    /// a compatible kind. Unknown node types (from a missing feature) are
    /// allowed through, so their graphs still load and save.
    ///
    /// # Errors
    /// A message for the UI.
    pub fn check_link(&self, graph: &NodeGraph, link: &Link) -> Result<(), String> {
        let (Some(from), Some(to)) = (graph.node(link.from), graph.node(link.to)) else {
            return Err("both nodes must exist".to_owned());
        };
        if link.from == link.to {
            return Err("a node cannot feed itself".to_owned());
        }
        let (Some(from_type), Some(to_type)) = (self.get(&from.node_type), self.get(&to.node_type))
        else {
            return Ok(());
        };
        let output = from_type
            .outputs
            .iter()
            .find(|port| port.name == link.output)
            .ok_or_else(|| format!("{} has no output {}", from_type.label, link.output))?;
        let input = to_type
            .inputs
            .iter()
            .find(|port| port.name == link.input)
            .ok_or_else(|| format!("{} has no input {}", to_type.label, link.input))?;
        if output.kind != input.kind && output.kind != "any" && input.kind != "any" {
            return Err(format!("cannot connect {} to {}", output.kind, input.kind));
        }
        Ok(())
    }
}

/// One undoable change to an entity's [`NodeGraph`].
#[derive(Clone, Debug, PartialEq)]
pub enum GraphOp {
    /// Add a node under a key.
    Add(NodeKey, Node),
    /// Remove a node (and its links).
    Remove(NodeKey),
    /// Link an output to an input.
    Link(Link),
    /// Remove a link.
    Unlink(Link),
    /// Set a parameter.
    SetParam(NodeKey, String, Value),
    /// Unset a parameter (back to the type's default).
    ClearParam(NodeKey, String),
    /// Move a node.
    Move(NodeKey, [f32; 2]),
}

/// Applies [`GraphOp`]s to an entity's graph as one undo step, storing the
/// inverse of each.
#[derive(Debug)]
pub struct GraphEdit {
    id: EntityId,
    label: String,
    ops: Vec<GraphOp>,
    undo: Vec<Vec<GraphOp>>,
}

impl GraphEdit {
    /// An edit of `id`'s graph.
    pub fn new(id: EntityId, label: impl Into<String>, ops: Vec<GraphOp>) -> Self {
        Self {
            id,
            label: label.into(),
            ops,
            undo: Vec::new(),
        }
    }
}

fn run(graph: &mut NodeGraph, op: &GraphOp) -> Result<Vec<GraphOp>, TxnError> {
    let missing = |key: NodeKey| TxnError::new(format!("node {key} does not exist"));
    Ok(match op {
        GraphOp::Add(key, node) => {
            if graph.node(*key).is_some() {
                return Err(TxnError::new(format!("node {key} already exists")));
            }
            graph.restore(*key, node.clone());
            vec![GraphOp::Remove(*key)]
        }
        GraphOp::Remove(key) => {
            let (node, links) = graph.remove(*key).ok_or_else(|| missing(*key))?;
            let mut inverse = vec![GraphOp::Add(*key, node)];
            inverse.extend(links.into_iter().map(GraphOp::Link));
            inverse
        }
        GraphOp::Link(link) => {
            if graph.node(link.from).is_none() || graph.node(link.to).is_none() {
                return Err(TxnError::new("both nodes must exist"));
            }
            let mut inverse = vec![GraphOp::Unlink(link.clone())];
            if let Some(replaced) = graph.link(link.clone()) {
                inverse.push(GraphOp::Link(replaced));
            }
            inverse
        }
        GraphOp::Unlink(link) => {
            if !graph.unlink(link) {
                return Err(TxnError::new("that link does not exist"));
            }
            vec![GraphOp::Link(link.clone())]
        }
        GraphOp::SetParam(key, name, value) => {
            let node = graph.node_mut(*key).ok_or_else(|| missing(*key))?;
            match node.params.set(name.clone(), value.clone()) {
                Some(old) => vec![GraphOp::SetParam(*key, name.clone(), old)],
                None => vec![GraphOp::ClearParam(*key, name.clone())],
            }
        }
        GraphOp::ClearParam(key, name) => {
            let node = graph.node_mut(*key).ok_or_else(|| missing(*key))?;
            node.params
                .remove(name)
                .map(|old| GraphOp::SetParam(*key, name.clone(), old))
                .into_iter()
                .collect()
        }
        GraphOp::Move(key, position) => {
            let node = graph.node_mut(*key).ok_or_else(|| missing(*key))?;
            let old = std::mem::replace(&mut node.position, *position);
            vec![GraphOp::Move(*key, old)]
        }
    })
}

fn run_inverse(graph: &mut NodeGraph, ops: &[GraphOp]) {
    for op in ops {
        // Inverses of ops that succeeded cannot fail.
        let _ = run(graph, op);
    }
}

fn graph_mut(arena: &mut Arena, id: EntityId) -> Result<&mut NodeGraph, TxnError> {
    arena
        .get_by_id_mut(id)?
        .ext_mut::<NodeGraph>()
        .ok_or_else(|| TxnError::from(CoreError::UnknownId(id)))
}

impl ReversibleTransaction<Arena> for GraphEdit {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let graph = graph_mut(arena, self.id)?;
        let mut undo = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            match run(graph, op) {
                Ok(inverse) => undo.push(inverse),
                Err(error) => {
                    for inverse in undo.iter().rev() {
                        run_inverse(graph, inverse);
                    }
                    return Err(error);
                }
            }
        }
        self.undo = undo;
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        let graph = graph_mut(arena, self.id)?;
        for inverse in self.undo.drain(..).rev() {
            run_inverse(graph, &inverse);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Entity, UndoStack};

    fn registry() -> NodeRegistry {
        let mut registry = NodeRegistry::new();
        registry.register(NodeType::new("comp.read", "Read", "Image").output("image", "image"));
        registry.register(
            NodeType::new("comp.blur", "Blur", "Filter")
                .input("image", "image")
                .output("image", "image")
                .param(PropertyDef::new("radius", "Radius", 4.0).range(0.0, 500.0)),
        );
        registry.register(NodeType::new("audio.osc", "Oscillator", "Audio").output("out", "audio"));
        registry
    }

    fn link(from: NodeKey, output: &str, to: NodeKey, input: &str) -> Link {
        Link {
            from,
            output: output.into(),
            to,
            input: input.into(),
        }
    }

    #[test]
    fn links_are_type_checked_and_nodes_ordered() {
        let registry = registry();
        let mut graph = NodeGraph::new();
        let read = graph.add(registry.instantiate("comp.read", [0.0, 0.0]).unwrap());
        let blur = graph.add(registry.instantiate("comp.blur", [200.0, 0.0]).unwrap());
        let blur2 = graph.add(registry.instantiate("comp.blur", [400.0, 0.0]).unwrap());
        let osc = graph.add(registry.instantiate("audio.osc", [0.0, 200.0]).unwrap());
        assert_eq!(graph.node(blur).unwrap().params.f64("radius"), Some(4.0));
        assert!(
            registry
                .check_link(&graph, &link(read, "image", blur, "image"))
                .is_ok()
        );
        assert!(
            registry
                .check_link(&graph, &link(osc, "out", blur, "image"))
                .is_err()
        );
        assert!(
            registry
                .check_link(&graph, &link(read, "nope", blur, "image"))
                .is_err()
        );
        graph.link(link(blur, "image", blur2, "image"));
        graph.link(link(read, "image", blur, "image"));
        let order = graph.order().unwrap();
        let position = |key| order.iter().position(|k| *k == key).unwrap();
        assert!(position(read) < position(blur) && position(blur) < position(blur2));
        graph.link(link(blur2, "image", blur, "image"));
        assert!(graph.order().is_err(), "a cycle is reported");
        // One input takes one link: the cycle link replaced read -> blur.
        assert_eq!(graph.input_link(blur, "image").unwrap().from, blur2);
        let unknown = graph.add(Node::new("plugin.missing", [0.0, 0.0]));
        assert!(
            registry
                .check_link(&graph, &link(unknown, "x", blur2, "image"))
                .is_ok()
        );
    }

    #[test]
    fn graph_edits_undo_exactly() {
        let registry = registry();
        let mut graph = NodeGraph::new();
        let read = graph.add(registry.instantiate("comp.read", [0.0, 0.0]).unwrap());
        let blur = graph.add(registry.instantiate("comp.blur", [1.0, 0.0]).unwrap());
        graph.link(link(read, "image", blur, "image"));
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::empty("Comp", "comp.tree").with_ext(graph.clone()));
        let id = arena.get(handle).unwrap().id();
        let mut history = UndoStack::new();
        history
            .apply(
                &mut arena,
                GraphEdit::new(
                    id,
                    "Tweak",
                    vec![
                        GraphOp::SetParam(blur, "radius".into(), Value::Float(12.0)),
                        GraphOp::SetParam(blur, "quality".into(), Value::Int(2)),
                        GraphOp::Move(blur, [5.0, 5.0]),
                        GraphOp::Remove(read),
                    ],
                ),
            )
            .unwrap();
        let now = arena
            .get(handle)
            .unwrap()
            .ext::<NodeGraph>()
            .unwrap()
            .clone();
        assert_eq!(now.nodes().count(), 1);
        assert_eq!(now.node(blur).unwrap().params.f64("radius"), Some(12.0));
        history.undo(&mut arena).unwrap();
        assert_eq!(
            arena.get(handle).unwrap().ext::<NodeGraph>().unwrap(),
            &graph
        );
        // A failing op takes back the ones before it.
        let bad = GraphEdit::new(
            id,
            "Bad",
            vec![GraphOp::Move(blur, [9.0, 9.0]), GraphOp::Remove(99)],
        );
        assert!(history.apply(&mut arena, bad).is_err());
        assert_eq!(
            arena.get(handle).unwrap().ext::<NodeGraph>().unwrap(),
            &graph
        );

        let mut out = Writer::new();
        ComponentType::encode(&graph, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            NodeGraph::decode(1, &mut Reader::new(&bytes)).unwrap(),
            graph
        );
    }
}
