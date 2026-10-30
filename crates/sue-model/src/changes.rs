//! Tracked changes in rich text. While changes are recorded, typed text
//! carries an `inserted` mark and deleted text stays, struck through, under
//! a `deleted` mark ([`crate::mark_kind::INSERTED`],
//! [`crate::mark_kind::DELETED`]); each names who made it and when. To
//! accept a change keeps what it did, to reject it undoes it. Formats
//! that cannot show changes write the text as it would be with every
//! change accepted ([`accept`] over the whole text).

use std::collections::BTreeMap;
use std::ops::Range;

use sue_core::Value;

use crate::text::{TextMarks, kind};

/// A property of a document (or a project of documents) that is `true`
/// while its changes are recorded (`Bool`).
pub const TRACK: &str = "sue.track_changes";

/// Who made a change, and when.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// The author.
    pub author: String,
    /// The date, `YYYY-MM-DD` or a fuller time.
    pub date: String,
}

impl Change {
    /// A change by `author` on `date`.
    #[must_use]
    pub fn new(author: &str, date: &str) -> Self {
        Self {
            author: author.to_owned(),
            date: date.to_owned(),
        }
    }

    /// The value its mark holds.
    #[must_use]
    pub fn value(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("author".to_owned(), Value::Text(self.author.clone()));
        map.insert("date".to_owned(), Value::Text(self.date.clone()));
        Value::Map(map)
    }

    /// The change a mark's value names.
    #[must_use]
    pub fn of(value: &Value) -> Self {
        let text = |key: &str| match value {
            Value::Map(map) => map
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            _ => String::new(),
        };
        Self {
            author: text("author"),
            date: text("date"),
        }
    }
}

/// One tracked change: where, whether it inserted text (or deleted it),
/// and by whom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracked {
    /// The text it covers.
    pub range: Range<usize>,
    /// `true` for inserted text, `false` for deleted text.
    pub inserted: bool,
    /// Who and when.
    pub change: Change,
}

/// Every tracked change in `marks`, in order.
#[must_use]
pub fn changes(marks: &TextMarks) -> Vec<Tracked> {
    let mut out: Vec<Tracked> = marks
        .marks()
        .iter()
        .filter(|mark| mark.kind == kind::INSERTED || mark.kind == kind::DELETED)
        .filter(|mark| !mark.range.is_empty())
        .map(|mark| Tracked {
            range: mark.range.clone(),
            inserted: mark.kind == kind::INSERTED,
            change: Change::of(&mark.value),
        })
        .collect();
    out.sort_by_key(|tracked| (tracked.range.start, tracked.range.end));
    out
}

/// Keeps (`keep`) or undoes the changes of `text` that touch `range`:
/// kept insertions lose their mark, kept deletions lose their text,
/// undone insertions lose their text, undone deletions their mark.
/// Returns the new text and marks.
fn settle(text: &str, marks: &TextMarks, range: Range<usize>, keep: bool) -> (String, TextMarks) {
    let touched: Vec<Tracked> = changes(marks)
        .into_iter()
        .filter(|tracked| {
            tracked.range.start < range.end.max(range.start + 1) && range.start < tracked.range.end
        })
        .collect();
    let mut text = text.to_owned();
    let mut marks = marks.clone();
    let mut gone: Vec<Range<usize>> = Vec::new();
    for tracked in &touched {
        let kind = if tracked.inserted {
            kind::INSERTED
        } else {
            kind::DELETED
        };
        if tracked.inserted == keep {
            marks.set(tracked.range.clone(), kind, None);
        } else {
            gone.push(tracked.range.clone());
        }
    }
    gone.sort_by_key(|range| std::cmp::Reverse(range.start));
    for cut in gone {
        if cut.end <= text.len() {
            marks.delete_text(&text, cut.clone());
            text.replace_range(cut, "");
        }
    }
    (text, marks)
}

/// Accepts the changes touching `range` of `text`.
#[must_use]
pub fn accept(text: &str, marks: &TextMarks, range: Range<usize>) -> (String, TextMarks) {
    settle(text, marks, range, true)
}

/// Rejects the changes touching `range` of `text`.
#[must_use]
pub fn reject(text: &str, marks: &TextMarks, range: Range<usize>) -> (String, TextMarks) {
    settle(text, marks, range, false)
}

/// `text` as it is with every change accepted.
#[must_use]
pub fn accepted(text: &str, marks: &TextMarks) -> (String, TextMarks) {
    accept(text, marks, 0..text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked() -> (String, TextMarks) {
        // "The old cat" with "old " deleted and "big " inserted after it.
        let text = "The old big cat\n".to_owned();
        let mut marks = TextMarks::new();
        let ann = Change::new("Ann", "2026-10-30");
        marks.set(4..8, kind::DELETED, Some(ann.value()));
        marks.set(8..12, kind::INSERTED, Some(ann.value()));
        marks.set(12..15, kind::BOLD, Some(Value::Bool(true)));
        (text, marks)
    }

    #[test]
    fn changes_list_who_did_what_where() {
        let (_, marks) = marked();
        let found = changes(&marks);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].range, 4..8);
        assert!(!found[0].inserted);
        assert!(found[1].inserted);
        assert_eq!(found[1].change, Change::new("Ann", "2026-10-30"));
    }

    #[test]
    fn accepting_keeps_and_rejecting_undoes_each_change() {
        let (text, marks) = marked();
        let (kept, kept_marks) = accepted(&text, &marks);
        assert_eq!(kept, "The big cat\n");
        assert!(changes(&kept_marks).is_empty());
        assert!(kept_marks.covers(8..11, kind::BOLD), "{kept_marks:?}");
        let (undone, undone_marks) = reject(&text, &marks, 0..text.len());
        assert_eq!(undone, "The old cat\n");
        assert!(changes(&undone_marks).is_empty());
        // Only the change the range touches.
        let (one, one_marks) = accept(&text, &marks, 5..5);
        assert_eq!(one, "The big cat\n");
        assert_eq!(changes(&one_marks).len(), 1);
        assert!(changes(&one_marks)[0].inserted);
    }
}
