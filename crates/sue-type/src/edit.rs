//! Changing rich text in the arena. Typing, deleting and pasting edit the
//! entity's `StringBuffer` in place and move its [`TextMarks`] with it,
//! recorded as one undo step per burst of typing (text and marks together);
//! formatting is a transaction of its own. Every studio that edits rich
//! text goes through these, so its undo works the same everywhere.

use std::ops::Range;

use sue_core::txn::{Batch, SetExtension, TextOp};
use sue_core::{Arena, EntityId, Value};
use sue_model::TextMarks;
use sue_wm::{AppletContext, ArenaEdit};

/// The text of document `id`.
#[must_use]
pub fn text_of(arena: &Arena, id: EntityId) -> Option<&str> {
    arena
        .get_by_id(id)
        .ok()?
        .string_buffer()
        .map(sue_core::StringBuffer::as_str)
}

/// The formatting of document `id` (none if it has no marks yet).
#[must_use]
pub fn marks_of(arena: &Arena, id: EntityId) -> TextMarks {
    arena
        .get_by_id(id)
        .ok()
        .and_then(|entity| entity.ext::<TextMarks>())
        .cloned()
        .unwrap_or_default()
}

/// Replaces `range` of document `id` with `text` (typing, deleting,
/// pasting), keeping its marks in place. `source` gives pasted text its own
/// formatting; `format` then sets character marks on the new text (what was
/// chosen with nothing selected). Returns the offset after the new text.
pub fn replace(
    cx: &mut AppletContext<'_>,
    id: EntityId,
    range: Range<usize>,
    text: &str,
    source: Option<&TextMarks>,
    format: &[(String, Option<Value>)],
) -> Option<usize> {
    let handle = cx.arena().handle_of(id)?;
    let entity = cx.world.arena.get_mut(handle).ok()?;
    let len = entity.string_buffer()?.as_str().len();
    let range = range.start.min(len)..range.end.min(len);
    if range.is_empty() && text.is_empty() {
        return Some(range.start);
    }
    let mut marks = entity.ext::<TextMarks>().cloned().unwrap_or_default();
    let mut ops = Vec::new();
    if !range.is_empty() {
        marks.delete_text(entity.string_buffer()?.as_str(), range.clone());
        let removed = entity.string_buffer_mut()?.remove(range.clone()).ok()?;
        ops.push(TextOp::Delete {
            offset: range.start,
            text: removed,
        });
    }
    if !text.is_empty() {
        marks.insert_text(entity.string_buffer()?.as_str(), range.start, text, source);
        if entity
            .string_buffer_mut()?
            .insert(range.start, text)
            .is_err()
        {
            return None;
        }
        ops.push(TextOp::Insert {
            offset: range.start,
            text: text.to_owned(),
        });
        let inserted = range.start..range.start + text.len();
        for (kind, value) in format {
            marks.set(inserted.clone(), kind, value.clone());
        }
    }
    let before = entity
        .set_ext(Box::new(marks))
        .unwrap_or_else(|| Box::new(TextMarks::new()));
    cx.record_rich_typing(handle, id, ops, before);
    Some(range.start + text.len())
}

/// Changes the formatting of document `id` with `change` (handed the text
/// and the marks) as one undo step labeled `label`. Returns `false` if
/// nothing changed.
pub fn reformat(
    cx: &mut AppletContext<'_>,
    id: EntityId,
    label: &str,
    change: impl FnOnce(&str, &mut TextMarks),
) -> bool {
    let Some(text) = text_of(cx.arena(), id).map(str::to_owned) else {
        return false;
    };
    let old = marks_of(cx.arena(), id);
    let mut marks = old.clone();
    change(&text, &mut marks);
    if marks == old {
        return false;
    }
    let batch = Batch::new(label).then(SetExtension::set(id, marks));
    match cx.apply(ArenaEdit(batch)) {
        Ok(_) => true,
        Err(error) => {
            cx.fail(error.to_string());
            false
        }
    }
}

/// Turns character kind `kind` on over `range` of document `id`, or off if
/// all of it already has it. Returns whether it is on now.
pub fn toggle(
    cx: &mut AppletContext<'_>,
    id: EntityId,
    range: Range<usize>,
    kind: &str,
    label: &str,
) -> bool {
    let on = !marks_of(cx.arena(), id).covers(range.clone(), kind);
    reformat(cx, id, label, |_, marks| {
        marks.set(range, kind, on.then_some(Value::Bool(true)));
    });
    on
}

/// Makes sure document `id` ends with the newline its last paragraph's
/// marks sit on.
pub fn ensure_final_newline(cx: &mut AppletContext<'_>, id: EntityId) {
    let Some(text) = text_of(cx.arena(), id) else {
        return;
    };
    if !text.ends_with('\n') {
        let end = text.len();
        replace(cx, id, end..end, "\n", None, &[]);
        cx.typing.flush(cx.history);
    }
}
