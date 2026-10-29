//! Text editing over the arena. [`ArenaText`] lets an `egui::TextEdit` edit
//! an entity's `StringBuffer` in place (no copy of the text, backbone rule 5) while it
//! records every insert and delete. [`Typing`] groups those deltas into one
//! undo step per burst of typing and records it in the global `UndoStack`,
//! so `Ctrl+Z` and `Ctrl+Y` work the same in every text field (backbone rule 2).
//! Rich text, whose formatting moves with every edit, hands [`Typing`] the
//! formatting as it was when the burst began ([`Typing::record_with`]), so
//! the same undo step puts both back. Typing that is not a text buffer's (a
//! table cell's text) joins the same bursts with [`Typing::hold`].

use std::any::TypeId;
use std::ops::Range;
use std::time::{Duration, Instant};

use egui::TextBuffer;
use egui::text::CharIndex;
use sue_core::txn::{Batch, SetExtension, TextEdits, TextOp};
use sue_core::{
    AssetHandle, Component, EntityId, ReversibleTransaction, StringBuffer, TxnError, UndoStack,
};

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
    /// The extension component that changed with the text, as it was when
    /// the open group began.
    before: Option<(EntityId, Box<dyn Component>)>,
    /// Typing that is not a text buffer's, made in place: what it edits,
    /// the step that undoes the burst, and when it last typed.
    held: Option<(u64, Held, Instant)>,
}

/// A held burst's undo step, recorded as it is.
struct Held(Box<dyn ReversibleTransaction<World>>);

impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Held").field(&self.0.label()).finish()
    }
}

impl ReversibleTransaction<World> for Held {
    fn label(&self) -> String {
        self.0.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.apply(world)
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.revert(world)
    }
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
        if self.held.is_some() {
            self.flush(history);
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

    /// Records `ops` typed into `handle` (entity `id`) whose extension
    /// component changed with them, as rich text marks do: `before` is that
    /// component as it was before these ops. Only the first `before` of a
    /// group is kept, so the undo step restores the component as it was
    /// when the burst began.
    pub fn record_with(
        &mut self,
        history: &mut UndoStack<World>,
        handle: AssetHandle,
        id: EntityId,
        ops: Vec<TextOp>,
        before: Box<dyn Component>,
        now: Instant,
    ) {
        if ops.is_empty() {
            return;
        }
        self.record(history, handle, ops, now);
        let same = self.before.as_ref().is_some_and(|(known, component)| {
            *known == id && component.type_name() == before.type_name()
        });
        if !same {
            self.before = Some((id, before));
        }
    }

    /// Records typing that already happened in place but is not a text
    /// buffer's (a table cell typed in). `key` names what it edits: edits
    /// of one key in a row, without a pause, are one undo step, and `undo`
    /// is the step for the whole burst, so only the first one's is kept
    /// (taken from the state before the burst).
    pub fn hold(
        &mut self,
        history: &mut UndoStack<World>,
        key: u64,
        undo: Box<dyn ReversibleTransaction<World>>,
        now: Instant,
    ) {
        let joins = self.held.as_ref().is_some_and(|(known, _, last)| {
            *known == key && now.duration_since(*last) < TYPING_PAUSE
        });
        if joins {
            if let Some((_, _, last)) = &mut self.held {
                *last = now;
            }
            return;
        }
        self.flush(history);
        self.held = Some((key, Held(undo), now));
    }

    /// Closes the open group into one undo step.
    pub fn flush(&mut self, history: &mut UndoStack<World>) {
        if let Some((_, held, _)) = self.held.take() {
            history.record(held);
        }
        let before = self.before.take();
        if let Some((group, _)) = self.pending.take()
            && !group.is_empty()
        {
            match before {
                Some((id, component)) => {
                    let label = sue_core::ReversibleTransaction::label(&group);
                    // Text first, then the component; undone the other way.
                    let batch = Batch::new(label)
                        .then(group)
                        .then(SetExtension::set_boxed(id, component));
                    history.record(ArenaEdit(batch));
                }
                None => {
                    history.record(ArenaEdit(group));
                }
            }
        }
    }

    /// Closes the group once typing paused. Returns `true` if it did.
    pub fn flush_if_paused(&mut self, history: &mut UndoStack<World>, now: Instant) -> bool {
        let paused = |last: &Instant| now.duration_since(*last) >= TYPING_PAUSE;
        if self.pending.as_ref().is_some_and(|(_, last)| paused(last))
            || self.held.as_ref().is_some_and(|(_, _, last)| paused(last))
        {
            self.flush(history);
            return true;
        }
        false
    }

    /// When the open group will close by itself.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .map(|(_, last)| *last)
            .or(self.held.as_ref().map(|(_, _, last)| *last))
            .map(|last| last + TYPING_PAUSE)
    }

    /// `true` while a group is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.pending.is_some() || self.held.is_some()
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

    #[test]
    fn held_typing_is_one_step_per_burst() {
        #[derive(Debug)]
        struct Name(AssetHandle, String);
        impl ReversibleTransaction<crate::World> for Name {
            fn label(&self) -> String {
                "Typing".to_owned()
            }
            fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
                self.revert(world)
            }
            fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
                let entity = world.arena.get_mut(self.0)?;
                let now = entity.name().to_owned();
                entity.set_name(std::mem::replace(&mut self.1, now));
                Ok(())
            }
        }
        let mut world = World::default();
        let handle = world.arena.spawn("", StringBuffer::from(String::new()));
        let name = |world: &World| world.arena.get(handle).unwrap().name().to_owned();
        let mut history = UndoStack::new();
        let mut typing = Typing::default();
        let start = Instant::now();
        // Two edits in place, one burst: the first one's undo is kept.
        for (index, (before, now)) in [("", "a"), ("a", "ab")].into_iter().enumerate() {
            world
                .arena
                .get_mut(handle)
                .unwrap()
                .set_name(now.to_owned());
            let at = start + Duration::from_millis(100 * index as u64);
            typing.hold(
                &mut history,
                7,
                Box::new(Name(handle, before.to_owned())),
                at,
            );
        }
        assert!(typing.is_open());
        assert!(typing.flush_if_paused(&mut history, start + TYPING_PAUSE * 2));
        assert_eq!(history.len(), 1);
        history.undo(&mut world).unwrap();
        assert_eq!(name(&world), "");
        history.redo(&mut world).unwrap();
        assert_eq!(name(&world), "ab");
    }

    #[test]
    fn rich_typing_undoes_the_text_and_its_formatting_together() {
        let mut world = World::default();
        let handle = world
            .arena
            .spawn("doc", StringBuffer::from("ab".to_owned()));
        let id = world.arena.get(handle).unwrap().id();
        let marks = |bytes: &[u8]| sue_core::OpaqueComponent {
            type_name: "test.marks".to_owned(),
            version: 1,
            bytes: bytes.to_vec(),
        };
        world
            .arena
            .get_mut(handle)
            .unwrap()
            .set_ext(Box::new(marks(b"before")));
        let mut history = UndoStack::new();
        let mut typing = Typing::default();
        let start = Instant::now();
        for (step, ch) in ["c", "d"].into_iter().enumerate() {
            let before = world
                .arena
                .get(handle)
                .unwrap()
                .ext_named("test.marks")
                .unwrap()
                .clone_box();
            let mut ops = Vec::new();
            let entity = world.arena.get_mut(handle).unwrap();
            ArenaText::new(entity.string_buffer_mut().unwrap(), &mut ops)
                .insert_text(ch, CharIndex(2 + step));
            entity.set_ext(Box::new(marks(format!("after {step}").as_bytes())));
            typing.record_with(&mut history, handle, id, ops, before, start);
        }
        typing.flush(&mut history);
        assert_eq!(history.len(), 1, "one burst, one step");
        let read = |world: &World| {
            let entity = world.arena.get(handle).unwrap();
            let bytes = entity
                .ext_named("test.marks")
                .and_then(|component| {
                    component
                        .as_any()
                        .downcast_ref::<sue_core::OpaqueComponent>()
                })
                .map(|component| component.bytes.clone())
                .unwrap();
            (text(world, handle), String::from_utf8(bytes).unwrap())
        };
        history.undo(&mut world).unwrap();
        assert_eq!(read(&world), ("ab".to_owned(), "before".to_owned()));
        history.redo(&mut world).unwrap();
        assert_eq!(read(&world), ("abcd".to_owned(), "after 1".to_owned()));
    }
}
