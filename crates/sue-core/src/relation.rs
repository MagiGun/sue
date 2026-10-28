//! Relations between entities. Pages in a book, layers in a picture, tracks
//! in a song, clips on a timeline, rooms in a building, nodes in a game scene
//! and notes linking to notes are all the same thing: a typed, ordered edge
//! from one [`EntityId`] to another. The [`RelationStore`] keeps them, keyed
//! by stable ids so they survive saving and copying.
//!
//! [`CHILD`] (and any kind registered with [`RelationStore::add_tree_kind`])
//! forms a tree: one parent per child and no cycles. Other kinds form a
//! general graph. Siblings are ordered by an [`OrderKey`], a fractional key
//! that always has room for one more item between any two, so reordering
//! touches only the moved item.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::codec::{CodecError, Reader, Writer};
use crate::{EntityId, Properties};

/// The parent to child relation: folders, groups, layers, scene trees.
pub const CHILD: &str = "sue.child";
/// A plain reference: a link in a note, a material used by a mesh.
pub const REF: &str = "sue.ref";

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE: u8 = 36;

fn digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'z' => byte - b'a' + 10,
        _ => 0,
    }
}

/// A position among siblings. Keys compare as strings; [`OrderKey::between`]
/// finds a key strictly between any two.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderKey(String);

impl Default for OrderKey {
    fn default() -> Self {
        Self("i".to_owned())
    }
}

impl OrderKey {
    /// A key from its text (base-36 digits, not ending in `0`).
    ///
    /// # Errors
    /// If the text is empty, has other characters or ends in `0`.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.is_empty()
            || text.ends_with('0')
            || !text.bytes().all(|byte| DIGITS.contains(&byte))
        {
            return Err(format!("`{text}` is not an order key"));
        }
        Ok(Self(text.to_owned()))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A key after `before` and before `after` (either may be open).
    ///
    /// # Panics
    /// If `before` is not less than `after`.
    #[must_use]
    pub fn between(before: Option<&Self>, after: Option<&Self>) -> Self {
        let low = before.map_or("", |key| key.0.as_str());
        let high = after.map(|key| key.0.as_str());
        if let Some(high) = high {
            assert!(low < high, "order keys must be increasing");
        }
        Self(midpoint(low.as_bytes(), high.map(str::as_bytes)))
    }

    /// `count` increasing keys after `before`, for appending many items.
    #[must_use]
    pub fn sequence(before: Option<&Self>, count: usize) -> Vec<Self> {
        let mut keys = Vec::with_capacity(count);
        let mut last = before.cloned();
        for _ in 0..count {
            let next = Self::between(last.as_ref(), None);
            keys.push(next.clone());
            last = Some(next);
        }
        keys
    }
}

/// Fractional indexing midpoint: `low` < result < `high`, no trailing zero.
fn midpoint(low: &[u8], high: Option<&[u8]>) -> String {
    if let Some(high) = high {
        let mut common = 0;
        while common < high.len() && low.get(common).copied().unwrap_or(b'0') == high[common] {
            common += 1;
        }
        if common > 0 {
            let prefix = String::from_utf8_lossy(&high[..common]).into_owned();
            let rest_low = low.get(common..).unwrap_or(&[]);
            return prefix + &midpoint(rest_low, Some(&high[common..]));
        }
    }
    let low_digit = low.first().map_or(0, |byte| digit(*byte));
    let high_digit = high
        .and_then(|high| high.first())
        .map_or(BASE, |byte| digit(*byte));
    if high_digit - low_digit > 1 {
        let middle = (low_digit + high_digit).div_ceil(2);
        return char::from(DIGITS[middle as usize]).to_string();
    }
    if let Some(high) = high
        && high.len() > 1
    {
        return char::from(high[0]).to_string();
    }
    let rest_low = low.get(1..).unwrap_or(&[]);
    char::from(DIGITS[low_digit as usize]).to_string() + &midpoint(rest_low, None)
}

impl fmt::Display for OrderKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Relation {
    /// Source (the parent, for a tree).
    pub from: EntityId,
    /// Namespaced kind, e.g. [`CHILD`] or `audio.sidechain`.
    pub kind: String,
    /// Target (the child, for a tree).
    pub to: EntityId,
    /// Position among `from`'s targets of this kind.
    pub order: OrderKey,
    /// Data on the edge (a port name, a weight, a blend mode).
    pub properties: Properties,
}

impl Relation {
    /// An edge with the default order and no data.
    pub fn new(from: EntityId, kind: impl Into<String>, to: EntityId) -> Self {
        Self {
            from,
            kind: kind.into(),
            to,
            order: OrderKey::default(),
            properties: Properties::new(),
        }
    }

    /// With an order key.
    #[must_use]
    pub fn ordered(mut self, order: OrderKey) -> Self {
        self.order = order;
        self
    }

    /// Writes the edge.
    pub fn encode(&self, out: &mut Writer) {
        out.raw(&self.from.to_bytes());
        out.str(&self.kind);
        out.raw(&self.to.to_bytes());
        out.str(self.order.as_str());
        self.properties.encode(out);
    }

    /// Reads an edge.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = |input: &mut Reader<'_>| -> Result<EntityId, CodecError> {
            let mut bytes = [0; 16];
            bytes.copy_from_slice(input.raw(16)?);
            Ok(EntityId::from_bytes(bytes))
        };
        let from = id(input)?;
        let kind = input.str()?;
        let to = id(input)?;
        let order = OrderKey::parse(&input.str()?).map_err(|message| input.error(message))?;
        let properties = Properties::decode(input)?;
        Ok(Self {
            from,
            kind,
            to,
            order,
            properties,
        })
    }
}

/// Why a relation was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationError {
    /// An entity cannot relate to itself.
    SelfLoop(EntityId),
    /// The child already has a parent of this tree kind.
    SecondParent {
        /// The child.
        child: EntityId,
        /// Its parent.
        parent: EntityId,
    },
    /// The edge would make the tree a cycle.
    Cycle(EntityId),
}

impl fmt::Display for RelationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfLoop(id) => write!(f, "entity {id} cannot relate to itself"),
            Self::SecondParent { child, parent } => {
                write!(f, "entity {child} already has the parent {parent}")
            }
            Self::Cycle(id) => write!(f, "entity {id} would become its own ancestor"),
        }
    }
}

impl std::error::Error for RelationError {}

type Key = (EntityId, String, EntityId);

/// Every relation in a document, indexed both ways.
#[derive(Clone, Debug, PartialEq)]
pub struct RelationStore {
    edges: BTreeMap<Key, (OrderKey, Properties)>,
    reverse: BTreeSet<(EntityId, String, EntityId)>,
    tree_kinds: BTreeSet<String>,
}

impl Default for RelationStore {
    fn default() -> Self {
        Self {
            edges: BTreeMap::new(),
            reverse: BTreeSet::new(),
            tree_kinds: BTreeSet::from([CHILD.to_owned()]),
        }
    }
}

impl RelationStore {
    /// An empty store where [`CHILD`] is a tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes `kind` a tree kind (one parent, no cycles).
    pub fn add_tree_kind(&mut self, kind: impl Into<String>) {
        self.tree_kinds.insert(kind.into());
    }

    /// `true` if `kind` is a tree kind.
    #[must_use]
    pub fn is_tree(&self, kind: &str) -> bool {
        self.tree_kinds.contains(kind)
    }

    /// Number of edges.
    #[must_use]
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// `true` if there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Checks that `relation` could be added, without changing anything.
    ///
    /// # Errors
    /// A self loop, or a second parent or a cycle in a tree kind.
    pub fn check(&self, relation: &Relation) -> Result<(), RelationError> {
        if relation.from == relation.to {
            return Err(RelationError::SelfLoop(relation.from));
        }
        if self.is_tree(&relation.kind) {
            if let Some(parent) = self.parent(relation.to, &relation.kind)
                && parent != relation.from
            {
                return Err(RelationError::SecondParent {
                    child: relation.to,
                    parent,
                });
            }
            let mut ancestor = Some(relation.from);
            while let Some(id) = ancestor {
                if id == relation.to {
                    return Err(RelationError::Cycle(relation.to));
                }
                ancestor = self.parent(id, &relation.kind);
            }
        }
        Ok(())
    }

    /// Adds or updates an edge, returning the previous order and data.
    ///
    /// # Errors
    /// As [`Self::check`]; nothing changes on error.
    pub fn insert(
        &mut self,
        relation: Relation,
    ) -> Result<Option<(OrderKey, Properties)>, RelationError> {
        self.check(&relation)?;
        self.reverse
            .insert((relation.to, relation.kind.clone(), relation.from));
        Ok(self.edges.insert(
            (relation.from, relation.kind, relation.to),
            (relation.order, relation.properties),
        ))
    }

    /// Removes an edge, returning it.
    pub fn remove(&mut self, from: EntityId, kind: &str, to: EntityId) -> Option<Relation> {
        let key = (from, kind.to_owned(), to);
        let (order, properties) = self.edges.remove(&key)?;
        self.reverse.remove(&(to, key.1.clone(), from));
        Some(Relation {
            from,
            kind: key.1,
            to,
            order,
            properties,
        })
    }

    /// The edge from `from` to `to` of `kind`.
    #[must_use]
    pub fn get(&self, from: EntityId, kind: &str, to: EntityId) -> Option<Relation> {
        let (order, properties) = self.edges.get(&(from, kind.to_owned(), to))?;
        Some(Relation {
            from,
            kind: kind.to_owned(),
            to,
            order: order.clone(),
            properties: properties.clone(),
        })
    }

    /// The targets of `from` of `kind`, in order.
    #[must_use]
    pub fn targets(&self, from: EntityId, kind: &str) -> Vec<EntityId> {
        let mut found: Vec<(&OrderKey, EntityId)> = self
            .edges
            .range((from, kind.to_owned(), EntityId::NIL)..)
            .take_while(|((source, edge_kind, _), _)| *source == from && edge_kind == kind)
            .map(|((_, _, to), (order, _))| (order, *to))
            .collect();
        found.sort();
        found.into_iter().map(|(_, to)| to).collect()
    }

    /// The children of `parent` ([`CHILD`] targets), in order.
    #[must_use]
    pub fn children(&self, parent: EntityId) -> Vec<EntityId> {
        self.targets(parent, CHILD)
    }

    /// The sources pointing at `to` with `kind`.
    #[must_use]
    pub fn sources(&self, to: EntityId, kind: &str) -> Vec<EntityId> {
        self.reverse
            .range((to, kind.to_owned(), EntityId::NIL)..)
            .take_while(|(target, edge_kind, _)| *target == to && edge_kind == kind)
            .map(|(_, _, from)| *from)
            .collect()
    }

    /// The parent of `child` in tree `kind`.
    #[must_use]
    pub fn parent(&self, child: EntityId, kind: &str) -> Option<EntityId> {
        self.sources(child, kind).first().copied()
    }

    /// The order key that puts a new target of `from` last.
    #[must_use]
    pub fn next_order(&self, from: EntityId, kind: &str) -> OrderKey {
        let last = self
            .targets(from, kind)
            .last()
            .and_then(|to| self.edges.get(&(from, kind.to_owned(), *to)))
            .map(|(order, _)| order.clone());
        OrderKey::between(last.as_ref(), None)
    }

    /// Every edge that starts or ends at `id`.
    #[must_use]
    pub fn involving(&self, id: EntityId) -> Vec<Relation> {
        let mut found: Vec<Relation> = self
            .edges
            .range((id, String::new(), EntityId::NIL)..)
            .take_while(|((from, _, _), _)| *from == id)
            .map(|((from, kind, to), (order, properties))| Relation {
                from: *from,
                kind: kind.clone(),
                to: *to,
                order: order.clone(),
                properties: properties.clone(),
            })
            .collect();
        for (_, kind, from) in self
            .reverse
            .range((id, String::new(), EntityId::NIL)..)
            .take_while(|(to, _, _)| *to == id)
        {
            if let Some(relation) = self.get(*from, kind, id) {
                found.push(relation);
            }
        }
        found
    }

    /// Removes every edge that starts or ends at `id`, returning them.
    pub fn remove_involving(&mut self, id: EntityId) -> Vec<Relation> {
        let found = self.involving(id);
        for relation in &found {
            self.remove(relation.from, &relation.kind, relation.to);
        }
        found
    }

    /// Every edge.
    pub fn iter(&self) -> impl Iterator<Item = Relation> + '_ {
        self.edges
            .iter()
            .map(|((from, kind, to), (order, properties))| Relation {
                from: *from,
                kind: kind.clone(),
                to: *to,
                order: order.clone(),
                properties: properties.clone(),
            })
    }

    /// Writes every edge and the tree kinds.
    pub fn encode(&self, out: &mut Writer) {
        out.len(self.tree_kinds.len());
        for kind in &self.tree_kinds {
            out.str(kind);
        }
        out.len(self.edges.len());
        for relation in self.iter() {
            relation.encode(out);
        }
    }

    /// Reads a store written by [`Self::encode`].
    ///
    /// # Errors
    /// If the bytes are malformed or the edges break a tree.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut store = Self::new();
        for _ in 0..input.len(1)? {
            store.add_tree_kind(input.str()?);
        }
        for _ in 0..input.len(34)? {
            let relation = Relation::decode(input)?;
            store
                .insert(relation)
                .map_err(|error| input.error(error.to_string()))?;
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_keys_always_fit_between() {
        let mut keys = vec![OrderKey::default()];
        // Insert repeatedly at the front, the back and in the middle.
        for round in 0..300 {
            let index = match round % 3 {
                0 => 0,
                1 => keys.len(),
                _ => keys.len() / 2,
            };
            let before = index.checked_sub(1).map(|index| &keys[index]);
            let after = keys.get(index);
            let key = OrderKey::between(before, after);
            if let Some(before) = before {
                assert!(*before < key, "{before} < {key}");
            }
            if let Some(after) = after {
                assert!(key < *after, "{key} < {after}");
            }
            assert!(!key.as_str().ends_with('0'));
            keys.insert(index, key);
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, keys);
        let run = OrderKey::sequence(None, 50);
        assert!(run.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(OrderKey::parse("a0").is_err());
    }

    #[test]
    fn trees_have_one_parent_and_no_cycles() {
        let [root, folder, file] = [EntityId::new(), EntityId::new(), EntityId::new()];
        let mut store = RelationStore::new();
        store.insert(Relation::new(root, CHILD, folder)).unwrap();
        store.insert(Relation::new(folder, CHILD, file)).unwrap();
        assert_eq!(store.parent(file, CHILD), Some(folder));
        assert_eq!(
            store.insert(Relation::new(root, CHILD, file)),
            Err(RelationError::SecondParent {
                child: file,
                parent: folder
            })
        );
        assert_eq!(
            store.insert(Relation::new(file, CHILD, root)),
            Err(RelationError::Cycle(root))
        );
        assert_eq!(
            store.insert(Relation::new(file, CHILD, file)),
            Err(RelationError::SelfLoop(file))
        );
        // References are a graph: anything may point at anything.
        store.insert(Relation::new(file, REF, root)).unwrap();
        store.insert(Relation::new(folder, REF, root)).unwrap();
        assert_eq!(store.sources(root, REF).len(), 2);
        assert_eq!(store.involving(folder).len(), 3);
        let removed = store.remove_involving(folder);
        assert_eq!(removed.len(), 3);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn children_keep_their_order_and_the_store_round_trips() {
        let parent = EntityId::new();
        let mut store = RelationStore::new();
        let kids: Vec<EntityId> = (0..5).map(|_| EntityId::new()).collect();
        for kid in &kids {
            let order = store.next_order(parent, CHILD);
            store
                .insert(Relation::new(parent, CHILD, *kid).ordered(order))
                .unwrap();
        }
        assert_eq!(store.children(parent), kids);
        // Move the last child between the first two.
        let first = store.get(parent, CHILD, kids[0]).unwrap().order;
        let second = store.get(parent, CHILD, kids[1]).unwrap().order;
        let mut moved = store.get(parent, CHILD, kids[4]).unwrap();
        moved.order = OrderKey::between(Some(&first), Some(&second));
        store.insert(moved).unwrap();
        assert_eq!(
            store.children(parent),
            vec![kids[0], kids[4], kids[1], kids[2], kids[3]]
        );

        store.add_tree_kind("bim.contains");
        let mut out = Writer::new();
        store.encode(&mut out);
        let bytes = out.into_bytes();
        let back = RelationStore::decode(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, store);
        assert!(back.is_tree("bim.contains"));
    }
}
