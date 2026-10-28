//! Text editing over the arena. [`ArenaText`] lets an `egui::TextEdit` edit
//! an entity's `StringBuffer` in place (no copy of the text, backbone rule 5) while it
//! records every insert and delete. [`Typing`] groups those deltas into one
//! undo step per burst of typing and records it in the global `UndoStack`,
//! so `Ctrl+Z` and `Ctrl+Y` work the same in every text field (backbone rule 2).

use std::any::TypeId;
use std::ops::Range;
use std::time::{Duration, Instant};

use egui::TextBuffer;
use egui::text::CharIndex;
use sue_core::txn::{TextEdits, TextOp};
use sue_core::{AssetHandle, StringBuffer, UndoStack};

use crate::{ArenaEdit, World};

/// How long a pause ends a burst of typing.
pub const TYPING_PAUSE: Duration = Duration::from_millis(900);

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// An entity's text, editable by egui, recording what changes.
pub struct ArenaText<'a> {
    buffer: &'a mut StringBuffer,
    ops: &'a mut Vec<TextOp>,
}

impl<'a> ArenaText<'a> {
    /// Wraps `buffer`; changes are appended to `ops`.
    pub fn new(buffer: &'a mut StringBuffer, ops: &'a mut Vec<TextOp>) -> Self {
        Self { buffer, ops }
    }
}

impl TextBuffer for ArenaText<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.buffer.as_str()
    }

    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        let offset = byte_of(self.buffer.as_str(), char_index.0);
        if text.is_empty() || self.buffer.insert(offset, text).is_err() {
            return 0;
        }
        self.ops.push(TextOp::Insert {
            offset,
            text: text.to_owned(),
        });
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<CharIndex>) {
        let text = self.buffer.as_str();
        let (start, end) = (
            byte_of(text, char_range.start.0),
            byte_of(text, char_range.end.0),
        );
        if start >= end {
            return;
        }
        if let Ok(removed) = self.buffer.remove(start..end) {
            self.ops.push(TextOp::Delete {
                offset: start,
                text: removed,
            });
        }
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<ArenaText<'static>>()
    }
}

/// Groups typing into undo steps.
#[derive(Debug, Default)]
pub struct Typing {
    pending: Option<(TextEdits, Instant)>,
}

impl Typing {
    /// Records `ops` typed into `handle` at `now`. A different entity or a
    /// pause first closes the open group.
    pub fn record(
        &mut self,
        history: &mut UndoStack<World>,
        handle: AssetHandle,
        ops: Vec<TextOp>,
        now: Instant,
    ) {
        if ops.is_empty() {
            return;
        }
        let stale = self.pending.as_ref().is_some_and(|(group, last)| {
            group.handle() != handle || now.duration_since(*last) >= TYPING_PAUSE
        });
        if stale {
            self.flush(history);
        }
        let (group, last) = self
            .pending
            .get_or_insert_with(|| (TextEdits::new(handle), now));
        for op in ops {
            group.push(op);
        }
        *last = now;
    }

    /// Closes the open group into one undo step.
    pub fn flush(&mut self, history: &mut UndoStack<World>) {
        if let Some((group, _)) = self.pending.take()
            && !group.is_empty()
        {
            history.record(ArenaEdit(group));
        }
    }

    /// Closes the group once typing paused. Returns `true` if it did.
    pub fn flush_if_paused(&mut self, history: &mut UndoStack<World>, now: Instant) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, last)| now.duration_since(*last) >= TYPING_PAUSE)
        {
            self.flush(history);
            return true;
        }
        false
    }

    /// When the open group will close by itself.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|(_, last)| *last + TYPING_PAUSE)
    }

    /// `true` while a group is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(world: &World, handle: AssetHandle) -> String {
        world
            .arena
            .get(handle)
            .unwrap()
            .string_buffer()
            .unwrap()
            .as_str()
            .to_owned()
    }

    #[test]
    fn typing_edits_the_arena_in_place_and_undoes_in_bursts() {
        let mut world = World::default();
        let handle = world
            .arena
            .spawn("doc", StringBuffer::from("héllo".to_owned()));
        let mut history = UndoStack::new();
        let mut typing = Typing::default();
        let start = Instant::now();
        let mut ops = Vec::new();
        {
            let buffer = world
                .arena
                .get_mut(handle)
                .unwrap()
                .string_buffer_mut()
                .unwrap();
            let mut editable = ArenaText::new(buffer, &mut ops);
            // Char indices, not bytes: "é" is two bytes.
            assert_eq!(editable.insert_text("!", CharIndex(5)), 1);
            editable.delete_char_range(CharIndex(0)..CharIndex(1));
            editable.insert_text("H", CharIndex(0));
        }
        assert_eq!(text(&world, handle), "Héllo!");
        typing.record(&mut history, handle, ops, start);
        // A second burst after a pause is its own step.
        let mut more = Vec::new();
        {
            let buffer = world
                .arena
                .get_mut(handle)
                .unwrap()
                .string_buffer_mut()
                .unwrap();
            ArenaText::new(buffer, &mut more).insert_text("?", CharIndex(6));
        }
        typing.record(&mut history, handle, more, start + TYPING_PAUSE * 2);
        typing.flush(&mut history);
        assert_eq!(history.len(), 2);
        history.undo(&mut world).unwrap();
        assert_eq!(text(&world, handle), "Héllo!");
        history.undo(&mut world).unwrap();
        assert_eq!(text(&world, handle), "héllo");
        history.redo(&mut world).unwrap();
        assert_eq!(text(&world, handle), "Héllo!");
    }
}
