//! Rich text. The characters live in the entity's `StringBuffer`; their
//! formatting lives beside it as [`TextMarks`]: ranges with a kind and a
//! value (bold, a link, a heading level, a comment, inline math). Word
//! processing, notes, markdown, novels, screenplays, LaTeX and page layout
//! all read the same marks, and every text edit keeps them in place.

use std::ops::Range;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, EntityId, Value};

/// Well-known mark kinds. Features may add their own (namespaced).
pub mod kind {
    /// Bold (value `true`).
    pub const BOLD: &str = "bold";
    /// Italic (value `true`).
    pub const ITALIC: &str = "italic";
    /// Underline (value `true`).
    pub const UNDERLINE: &str = "underline";
    /// Strikethrough (value `true`).
    pub const STRIKE: &str = "strike";
    /// Monospace code (value `true`).
    pub const CODE: &str = "code";
    /// A link: a URL as text, or an entity.
    pub const LINK: &str = "link";
    /// A heading level 1 to 6 over a whole paragraph.
    pub const HEADING: &str = "heading";
    /// A list item: `bullet` or `number`, over a paragraph.
    pub const LIST: &str = "list";
    /// A block quote, over a paragraph.
    pub const QUOTE: &str = "quote";
    /// Font family name.
    pub const FONT: &str = "font";
    /// Font size (a length quantity, usually points).
    pub const SIZE: &str = "size";
    /// Text color.
    pub const COLOR: &str = "color";
    /// Inline math: the LaTeX source as text.
    pub const MATH: &str = "math";
    /// A comment thread: the comment entity.
    pub const COMMENT: &str = "comment";
    /// A named paragraph or character style.
    pub const STYLE: &str = "style";
}

/// One formatted range, in byte offsets of the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    /// The range it covers.
    pub range: Range<usize>,
    /// What it is, e.g. [`kind::BOLD`].
    pub kind: String,
    /// Its value.
    pub value: Value,
    /// `true` if typing at the end extends it (bold does, links do not).
    pub inclusive: bool,
}

impl Mark {
    /// A mark over `range`. Formatting kinds extend when typing at their end.
    pub fn new(range: Range<usize>, kind: impl Into<String>, value: impl Into<Value>) -> Self {
        let kind = kind.into();
        let inclusive = !matches!(kind.as_str(), kind::LINK | kind::COMMENT | kind::MATH);
        Self {
            range,
            kind,
            value: value.into(),
            inclusive,
        }
    }
}

/// The formatting of an entity's text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextMarks {
    marks: Vec<Mark>,
}

impl TextMarks {
    /// No formatting.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mark. Empty ranges are ignored.
    pub fn add(&mut self, mark: Mark) {
        if !mark.range.is_empty() {
            self.marks.push(mark);
            self.marks
                .sort_by_key(|mark| (mark.range.start, mark.range.end));
        }
    }

    /// Removes every mark of `kind` from `range`, splitting marks that
    /// stick out on either side. Returns the marks as they were, for undo.
    pub fn clear(&mut self, range: Range<usize>, kind: &str) -> Vec<Mark> {
        let before = self.marks.clone();
        let mut kept = Vec::with_capacity(self.marks.len());
        for mark in self.marks.drain(..) {
            let overlaps = mark.range.start < range.end && range.start < mark.range.end;
            if mark.kind != kind || !overlaps {
                kept.push(mark);
                continue;
            }
            if mark.range.start < range.start {
                let mut left = mark.clone();
                left.range.end = range.start;
                kept.push(left);
            }
            if range.end < mark.range.end {
                let mut right = mark;
                right.range.start = range.end;
                kept.push(right);
            }
        }
        self.marks = kept;
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
        before
    }

    /// Replaces all marks (undo of [`Self::clear`]).
    pub fn restore(&mut self, marks: Vec<Mark>) {
        self.marks = marks;
    }

    /// Every mark, ordered by start.
    #[must_use]
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The marks that cover byte `offset`.
    pub fn at(&self, offset: usize) -> impl Iterator<Item = &Mark> {
        self.marks
            .iter()
            .filter(move |mark| mark.range.contains(&offset))
    }

    /// The value of `kind` at `offset`.
    #[must_use]
    pub fn value_at(&self, offset: usize, kind: &str) -> Option<&Value> {
        self.at(offset)
            .filter(|mark| mark.kind == kind)
            .last()
            .map(|mark| &mark.value)
    }

    /// Keeps marks in place after `len` bytes were inserted at `offset`.
    pub fn on_insert(&mut self, offset: usize, len: usize) {
        for mark in &mut self.marks {
            let Range { start, end } = mark.range;
            if offset <= start {
                mark.range = start + len..end + len;
            } else if offset < end || (offset == end && mark.inclusive) {
                mark.range.end = end + len;
            }
        }
    }

    /// Keeps marks in place after the bytes in `removed` were deleted.
    /// Marks that vanish entirely are dropped.
    pub fn on_delete(&mut self, removed: Range<usize>) {
        let len = removed.len();
        let shift = |position: usize| {
            if position <= removed.start {
                position
            } else if position >= removed.end {
                position - len
            } else {
                removed.start
            }
        };
        for mark in &mut self.marks {
            mark.range = shift(mark.range.start)..shift(mark.range.end);
        }
        self.marks.retain(|mark| !mark.range.is_empty());
    }
}

impl ComponentType for TextMarks {
    const NAME: &'static str = "sue.text.marks";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.len(self.marks.len());
        for mark in &self.marks {
            out.varint(mark.range.start as u64);
            out.varint(mark.range.end as u64);
            out.str(&mark.kind);
            mark.value.encode(out);
            out.bool(mark.inclusive);
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = input.len(5)?;
        let mut marks = Vec::with_capacity(count);
        for _ in 0..count {
            let start = usize::try_from(input.varint()?).map_err(|_| input.error("offset"))?;
            let end = usize::try_from(input.varint()?).map_err(|_| input.error("offset"))?;
            if start > end {
                return Err(input.error("mark range is reversed"));
            }
            marks.push(Mark {
                range: start..end,
                kind: input.str()?,
                value: Value::decode(input)?,
                inclusive: input.bool()?,
            });
        }
        Ok(Self { marks })
    }

    fn heap_bytes(&self) -> usize {
        self.marks.len() * size_of::<Mark>()
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        for mark in &mut self.marks {
            mark.value.remap_entities(map);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_follow_insertions_and_deletions() {
        // "Hello brave world": "brave" is bold, "world" is a link.
        let mut marks = TextMarks::new();
        marks.add(Mark::new(6..11, kind::BOLD, true));
        marks.add(Mark::new(12..17, kind::LINK, "https://example.com"));
        // Typing at the end of the bold word extends it; at a link's end it does not.
        marks.on_insert(11, 3);
        marks.on_insert(20, 1);
        assert_eq!(marks.marks()[0].range, 6..14);
        assert_eq!(marks.marks()[1].range, 15..20);
        // Typing before a mark shifts it.
        marks.on_insert(0, 2);
        assert_eq!(marks.marks()[0].range, 8..16);
        // Deleting across a mark's start trims it; deleting all of it drops it.
        marks.on_delete(5..10);
        assert_eq!(marks.marks()[0].range, 5..11);
        marks.on_delete(4..12);
        assert_eq!(marks.marks().len(), 1);
        assert_eq!(
            marks.value_at(5, kind::LINK).and_then(Value::as_str),
            Some("https://example.com")
        );
    }

    #[test]
    fn clearing_splits_and_round_trips() {
        let mut marks = TextMarks::new();
        marks.add(Mark::new(0..10, kind::ITALIC, true));
        marks.add(Mark::new(2..4, kind::BOLD, true));
        let before = marks.clear(3..6, kind::ITALIC);
        let ranges: Vec<_> = marks
            .marks()
            .iter()
            .map(|mark| (mark.kind.as_str(), mark.range.clone()))
            .collect();
        assert_eq!(
            ranges,
            vec![("italic", 0..3), ("bold", 2..4), ("italic", 6..10)]
        );
        let mut out = Writer::new();
        ComponentType::encode(&marks, &mut out);
        let bytes = out.into_bytes();
        let back = TextMarks::decode(1, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, marks);
        marks.restore(before);
        assert_eq!(marks.marks().len(), 2);
    }
}
