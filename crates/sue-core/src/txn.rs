//! Reversible transactions over the [`Arena`].
//!
//! Each transaction stores only its delta: the inserted text, the removed
//! range, or the component it attached.

use std::ops::Range;

use crate::{
    Arena, AssetHandle, ComponentData, ComponentKind, CoreError, Projection, ReversibleTransaction,
    StringBuffer, TxnError,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TimeSpan, UndoStack};

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
    fn mismatched_projection_is_rejected_without_losing_the_component() {
        let mut arena = Arena::new();
        let doc = arena.spawn("doc", StringBuffer::new());
        let mut txn = AttachProjection::new(doc, Projection::Audio, StringBuffer::new());
        assert!(txn.apply(&mut arena).is_err());
        assert!(txn.component.is_some());
    }
}
