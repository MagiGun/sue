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
