//! Comments on text. A comment is an entity of class [`COMMENT`]: its text
//! in its `StringBuffer`, its author, date and whether it is resolved as
//! properties. A `comment` mark ([`crate::mark_kind::COMMENT`]) names it
//! over the words it is about. Replies are comments too, related to the
//! one they answer by ordered [`REPLY`] relations, so a thread is a comment
//! and its replies in order. Any studio that shows text reads the same
//! threads.

use sue_core::{Arena, Entity, EntityId, StringBuffer, Value};

/// The class of a comment.
pub const COMMENT: &str = "sue.comment";
/// Who wrote it (`Text`).
pub const AUTHOR: &str = "sue.comment.author";
/// When, as `YYYY-MM-DD` or a fuller time (`Text`).
pub const DATE: &str = "sue.comment.date";
/// `true` once it is resolved (`Bool`).
pub const RESOLVED: &str = "sue.comment.resolved";
/// The relation from a comment to each reply, in order.
pub const REPLY: &str = "sue.comment.reply";

/// A new comment entity by `author` on `date` saying `text`.
#[must_use]
pub fn comment(author: &str, date: &str, text: &str) -> Entity {
    Entity::new("Comment", StringBuffer::from(text.to_owned()))
        .with_class(COMMENT)
        .with_property(AUTHOR, Value::Text(author.to_owned()))
        .with_property(DATE, Value::Text(date.to_owned()))
}

/// One comment, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Its entity.
    pub id: EntityId,
    /// Who wrote it.
    pub author: String,
    /// When.
    pub date: String,
    /// What it says.
    pub text: String,
}

/// A comment and its replies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    /// The comment.
    pub first: Note,
    /// Its replies, in order.
    pub replies: Vec<Note>,
    /// `true` once it is resolved.
    pub resolved: bool,
}

/// Comment `id` read, if it is one.
#[must_use]
pub fn note(arena: &Arena, id: EntityId) -> Option<Note> {
    let entity = arena.get_by_id(id).ok()?;
    if entity.class() != COMMENT {
        return None;
    }
    let text = |key: &str| {
        entity
            .property(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    Some(Note {
        id,
        author: text(AUTHOR),
        date: text(DATE),
        text: entity
            .string_buffer()
            .map(|buffer| buffer.as_str().to_owned())
            .unwrap_or_default(),
    })
}

/// The thread comment `id` starts.
#[must_use]
pub fn thread(arena: &Arena, id: EntityId) -> Option<Thread> {
    let first = note(arena, id)?;
    let resolved = arena
        .get_by_id(id)
        .ok()
        .and_then(|entity| entity.property(RESOLVED))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let replies = arena
        .relations()
        .targets(id, REPLY)
        .into_iter()
        .filter_map(|reply| note(arena, reply))
        .collect();
    Some(Thread {
        first,
        replies,
        resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::txn::SpawnEntity;
    use sue_core::{Relation, ReversibleTransaction};

    #[test]
    fn threads_read_a_comment_and_its_replies_in_order() {
        let mut arena = Arena::new();
        let first = comment("Ann", "2026-10-29", "Is this right?");
        let id = first.id();
        SpawnEntity::new(first).apply(&mut arena).unwrap();
        for (index, text) in ["Yes.", "Checked twice."].into_iter().enumerate() {
            let reply = comment("Bo", "2026-10-30", text);
            let reply_id = reply.id();
            let order = arena.relations().next_order(id, REPLY);
            SpawnEntity::new(reply)
                .with_relation(Relation::new(id, REPLY, reply_id).ordered(order))
                .apply(&mut arena)
                .unwrap();
            assert_eq!(thread(&arena, id).unwrap().replies.len(), index + 1);
        }
        let read = thread(&arena, id).unwrap();
        assert_eq!(read.first.author, "Ann");
        assert_eq!(read.first.text, "Is this right?");
        assert_eq!(
            read.replies
                .iter()
                .map(|reply| reply.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Yes.", "Checked twice."]
        );
        assert!(!read.resolved);
        assert_eq!(thread(&arena, EntityId::new()), None);
    }
}
