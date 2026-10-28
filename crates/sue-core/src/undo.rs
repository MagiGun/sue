//! Reversible transactions and the delta-tree undo engine.

use std::error::Error;
use std::fmt;

use crate::CoreError;

/// Why a transaction could not be applied or reverted.
#[derive(Debug)]
pub struct TxnError {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl TxnError {
    /// An error with a message and no underlying cause.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// An error caused by `source`.
    pub fn with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TxnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for TxnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl From<CoreError> for TxnError {
    fn from(error: CoreError) -> Self {
        Self::with_source(error.to_string(), error)
    }
}

/// A state mutation that stores its own inverse delta.
///
/// Contract:
/// - `apply` is atomic: when it returns `Err`, `target` is unchanged.
/// - `revert` exactly undoes the most recent successful `apply`.
/// - Store deltas (inserted text, removed ranges, detached components), never
///   snapshots of the whole target.
pub trait ReversibleTransaction<T>: Send {
    /// Short label for history views, e.g. "Insert 3 characters".
    fn label(&self) -> String;

    /// Applies the change to `target`.
    ///
    /// # Errors
    /// Any reason the change cannot be applied. `target` must be unchanged.
    fn apply(&mut self, target: &mut T) -> Result<(), TxnError>;

    /// Reverts the most recent successful [`apply`](Self::apply).
    ///
    /// # Errors
    /// Any reason the change cannot be reverted.
    fn revert(&mut self, target: &mut T) -> Result<(), TxnError>;
}

/// Identifies a node of an [`UndoStack`] tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    /// The root: the state before any transaction.
    pub const ROOT: Self = Self(0);

    /// Position in creation order (the root is `0`).
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

struct Node<T> {
    txn: Option<Box<dyn ReversibleTransaction<T>>>,
    label: String,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    preferred: Option<NodeId>,
    depth: usize,
}

/// Undo history as a tree of transactions.
///
/// Undoing and then applying something new does not discard the old future: it
/// becomes a sibling branch. [`redo`](Self::redo) follows the most recently
/// created or visited branch, and [`checkout`](Self::checkout) can reach any
/// node.
pub struct UndoStack<T> {
    nodes: Vec<Node<T>>,
    cursor: NodeId,
}

impl<T> Default for UndoStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for UndoStack<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UndoStack")
            .field("transactions", &self.len())
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl<T> UndoStack<T> {
    /// An empty history positioned at the root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                txn: None,
                label: "Initial state".to_owned(),
                parent: None,
                children: Vec::new(),
                preferred: None,
                depth: 0,
            }],
            cursor: NodeId::ROOT,
        }
    }

    /// Applies `txn` to `target` and records it as a child of the cursor.
    ///
    /// # Errors
    /// The transaction's own error. Nothing is recorded in that case.
    pub fn apply<X>(&mut self, target: &mut T, txn: X) -> Result<NodeId, TxnError>
    where
        X: ReversibleTransaction<T> + 'static,
    {
        self.apply_boxed(target, Box::new(txn))
    }

    /// [`apply`](Self::apply) for an already-boxed transaction.
    ///
    /// # Errors
    /// The transaction's own error. Nothing is recorded in that case.
    pub fn apply_boxed(
        &mut self,
        target: &mut T,
        mut txn: Box<dyn ReversibleTransaction<T>>,
    ) -> Result<NodeId, TxnError> {
        txn.apply(target)?;
        let id = NodeId(self.nodes.len());
        let parent = self.cursor;
        let depth = self.nodes[parent.0].depth + 1;
        self.nodes.push(Node {
            label: txn.label(),
            txn: Some(txn),
            parent: Some(parent),
            children: Vec::new(),
            preferred: None,
            depth,
        });
        let parent_node = &mut self.nodes[parent.0];
        parent_node.children.push(id);
        parent_node.preferred = Some(id);
        self.cursor = id;
        Ok(id)
    }

    /// Reverts the transaction at the cursor and moves to its parent.
    /// Returns `false` at the root.
    ///
    /// # Errors
    /// The transaction's revert error. The cursor does not move in that case.
    pub fn undo(&mut self, target: &mut T) -> Result<bool, TxnError> {
        let current = self.cursor;
        let Some(parent) = self.nodes[current.0].parent else {
            return Ok(false);
        };
        if let Some(txn) = self.nodes[current.0].txn.as_mut() {
            txn.revert(target)?;
        }
        self.nodes[parent.0].preferred = Some(current);
        self.cursor = parent;
        Ok(true)
    }

    /// Re-applies the preferred child of the cursor. Returns `false` if there is
    /// nothing to redo.
    ///
    /// # Errors
    /// The transaction's apply error. The cursor does not move in that case.
    pub fn redo(&mut self, target: &mut T) -> Result<bool, TxnError> {
        let Some(next) = self.nodes[self.cursor.0].preferred else {
            return Ok(false);
        };
        if let Some(txn) = self.nodes[next.0].txn.as_mut() {
            txn.apply(target)?;
        }
        self.cursor = next;
        Ok(true)
    }

    /// Chooses which child of the cursor [`redo`](Self::redo) follows.
    ///
    /// # Errors
    /// If `child` is not a child of the cursor.
    pub fn select_redo_branch(&mut self, child: NodeId) -> Result<(), TxnError> {
        let node = &mut self.nodes[self.cursor.0];
        if !node.children.contains(&child) {
            return Err(TxnError::new(format!(
                "undo node {} is not a child of the cursor",
                child.0
            )));
        }
        node.preferred = Some(child);
        Ok(())
    }

    /// Moves to any node: undoes up to the lowest common ancestor, then redoes
    /// down to `node`.
    ///
    /// # Errors
    /// An unknown node, or the first transaction that fails. The cursor then
    /// stays on the last node that was reached.
    pub fn checkout(&mut self, target: &mut T, node: NodeId) -> Result<(), TxnError> {
        if node.0 >= self.nodes.len() {
            return Err(TxnError::new(format!("unknown undo node {}", node.0)));
        }
        let mut path_down = Vec::new();
        let mut ancestor = node;
        while self.nodes[ancestor.0].depth > self.depth() {
            path_down.push(ancestor);
            ancestor = self.parent_of(ancestor);
        }
        while self.depth() > self.nodes[ancestor.0].depth {
            self.undo(target)?;
        }
        while self.cursor != ancestor {
            self.undo(target)?;
            path_down.push(ancestor);
            ancestor = self.parent_of(ancestor);
        }
        for step in path_down.into_iter().rev() {
            self.select_redo_branch(step)?;
            self.redo(target)?;
        }
        Ok(())
    }

    fn parent_of(&self, node: NodeId) -> NodeId {
        self.nodes[node.0].parent.unwrap_or(NodeId::ROOT)
    }

    /// The current node.
    #[must_use]
    pub fn cursor(&self) -> NodeId {
        self.cursor
    }

    /// Number of recorded transactions, across all branches.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    /// `true` if nothing has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Distance from the root to the cursor.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.nodes[self.cursor.0].depth
    }

    /// `true` if the cursor is not at the root.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.cursor != NodeId::ROOT
    }

    /// `true` if the cursor has a child to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.nodes[self.cursor.0].preferred.is_some()
    }

    /// The children (redo branches) of `node`.
    #[must_use]
    pub fn children(&self, node: NodeId) -> &[NodeId] {
        self.nodes
            .get(node.0)
            .map_or(&[], |n| n.children.as_slice())
    }

    /// The label of `node`.
    #[must_use]
    pub fn label(&self, node: NodeId) -> Option<&str> {
        self.nodes.get(node.0).map(|n| n.label.as_str())
    }

    /// Labels from the first transaction to the cursor.
    #[must_use]
    pub fn history(&self) -> Vec<&str> {
        let mut labels = Vec::with_capacity(self.depth());
        let mut node = self.cursor;
        while node != NodeId::ROOT {
            labels.push(self.nodes[node.0].label.as_str());
            node = self.parent_of(node);
        }
        labels.reverse();
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adds `delta` to an `i64`: an exactly invertible delta.
    struct Add(i64);

    impl ReversibleTransaction<i64> for Add {
        fn label(&self) -> String {
            format!("add {}", self.0)
        }

        fn apply(&mut self, target: &mut i64) -> Result<(), TxnError> {
            *target = target
                .checked_add(self.0)
                .ok_or_else(|| TxnError::new("overflow"))?;
            Ok(())
        }

        fn revert(&mut self, target: &mut i64) -> Result<(), TxnError> {
            *target -= self.0;
            Ok(())
        }
    }

    #[test]
    fn linear_undo_redo() {
        let mut value = 0;
        let mut stack = UndoStack::new();
        stack.apply(&mut value, Add(2)).unwrap();
        stack.apply(&mut value, Add(3)).unwrap();
        assert_eq!(value, 5);
        assert!(stack.undo(&mut value).unwrap());
        assert_eq!(value, 2);
        assert!(stack.redo(&mut value).unwrap());
        assert_eq!(value, 5);
        assert!(!stack.redo(&mut value).unwrap());
        stack.undo(&mut value).unwrap();
        stack.undo(&mut value).unwrap();
        assert!(!stack.undo(&mut value).unwrap());
        assert_eq!(value, 0);
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
    }

    #[test]
    fn new_edits_after_undo_create_branches() {
        let mut value = 0;
        let mut stack = UndoStack::new();
        stack.apply(&mut value, Add(1)).unwrap();
        let old_future = stack.apply(&mut value, Add(10)).unwrap();
        stack.undo(&mut value).unwrap();
        let new_future = stack.apply(&mut value, Add(100)).unwrap();
        assert_eq!(value, 101);
        assert_eq!(stack.len(), 3);
        let branch_point = stack.children(NodeId::ROOT)[0];
        assert_eq!(stack.children(branch_point), &[old_future, new_future]);

        stack.checkout(&mut value, old_future).unwrap();
        assert_eq!(value, 11);
        assert_eq!(stack.cursor(), old_future);
        assert_eq!(stack.history(), vec!["add 1", "add 10"]);

        stack.checkout(&mut value, NodeId::ROOT).unwrap();
        assert_eq!(value, 0);
        stack.checkout(&mut value, new_future).unwrap();
        assert_eq!(value, 101);
    }

    #[test]
    fn redo_follows_the_last_visited_branch() {
        let mut value = 0;
        let mut stack = UndoStack::new();
        let a = stack.apply(&mut value, Add(1)).unwrap();
        stack.undo(&mut value).unwrap();
        let b = stack.apply(&mut value, Add(2)).unwrap();
        stack.undo(&mut value).unwrap();
        stack.redo(&mut value).unwrap();
        assert_eq!(stack.cursor(), b);
        stack.undo(&mut value).unwrap();
        stack.select_redo_branch(a).unwrap();
        stack.redo(&mut value).unwrap();
        assert_eq!((stack.cursor(), value), (a, 1));
        assert!(stack.select_redo_branch(b).is_err());
    }

    #[test]
    fn failed_apply_is_not_recorded() {
        let mut value = i64::MAX;
        let mut stack = UndoStack::new();
        let error = stack.apply(&mut value, Add(1)).unwrap_err();
        assert_eq!(error.message(), "overflow");
        assert!(stack.is_empty());
        assert_eq!(value, i64::MAX);
    }

    #[test]
    fn core_errors_keep_their_source() {
        let error = TxnError::from(CoreError::InvalidBuffer("x"));
        assert_eq!(error.to_string(), "invalid buffer: x");
        assert!(error.source().is_some());
    }
}
