//! Rich text. The characters live in the entity's `StringBuffer`; their
//! formatting lives beside it as [`TextMarks`]: ranges with a kind and a
//! value (bold, a link, a heading level, a comment, inline math). Word
//! processing, notes, markdown, novels, screenplays, LaTeX and page layout
//! all read the same marks, and every text edit keeps them in place.
//!
//! Paragraphs end with a newline, and that newline carries the paragraph's
//! own marks (a heading, a list, the alignment), as a word processor's
//! paragraph mark does: a paragraph mark covers its whole paragraph, newline
//! included, so an empty paragraph keeps its style too. A rich text ends
//! with a newline for the same reason ([`ensure_final_newline`]). Edit with
//! [`TextMarks::insert_text`] and [`TextMarks::delete_text`] and the
//! paragraph marks follow their paragraphs.

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
    /// A named paragraph style, over a paragraph.
    pub const STYLE: &str = "style";
    /// Paragraph alignment: `left`, `center`, `right` or `justify`.
    pub const ALIGN: &str = "align";
    /// A paragraph's indent from the left, in points (`Float`).
    pub const INDENT: &str = "indent";
    /// A paragraph's indent from the right, in points (`Float`).
    pub const INDENT_RIGHT: &str = "indent_right";
    /// The first line's extra indent in points (`Float`; negative hangs).
    pub const FIRST_INDENT: &str = "first_indent";
    /// Line spacing as a multiple of single spacing (`Float`).
    pub const LINE_SPACING: &str = "line_spacing";
    /// Space above a paragraph, in points (`Float`).
    pub const SPACE_BEFORE: &str = "space_before";
    /// Space below a paragraph, in points (`Float`).
    pub const SPACE_AFTER: &str = "space_after";
    /// `true` starts a paragraph on a new page.
    pub const PAGE_BREAK: &str = "page_break";
    /// A list item's nesting level, 0 first (`Int`), over a paragraph.
    pub const LEVEL: &str = "level";
    /// A highlight behind the text (`Color`).
    pub const HIGHLIGHT: &str = "highlight";
    /// Raised or lowered text: `super` or `sub`.
    pub const SCRIPT: &str = "script";
    /// Small capitals (value `true`).
    pub const SMALL_CAPS: &str = "small_caps";
    /// A footnote after the marked text: the note itself, as text.
    pub const FOOTNOTE: &str = "footnote";
    /// An entity shown in the text (a picture), on an [`super::OBJECT`]
    /// character.
    pub const EMBED: &str = "embed";
}

/// The kinds that belong to whole paragraphs and travel with them.
pub const PARAGRAPH_KINDS: &[&str] = &[
    kind::HEADING,
    kind::LIST,
    kind::QUOTE,
    kind::STYLE,
    kind::ALIGN,
    kind::INDENT,
    kind::INDENT_RIGHT,
    kind::FIRST_INDENT,
    kind::LINE_SPACING,
    kind::SPACE_BEFORE,
    kind::SPACE_AFTER,
    kind::PAGE_BREAK,
    kind::LEVEL,
];

/// `true` if marks of `kind` cover whole paragraphs.
#[must_use]
pub fn is_paragraph_kind(kind: &str) -> bool {
    PARAGRAPH_KINDS.contains(&kind)
}

/// The character an embedded entity stands on ([`kind::EMBED`]).
pub const OBJECT: char = '\u{FFFC}';

/// The byte ranges of `text`'s paragraphs, each with its newline. A text
/// that does not end with a newline ends with a paragraph without one; an
/// empty text has no paragraphs.
#[must_use]
pub fn paragraphs(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            ranges.push(start..index + 1);
            start = index + 1;
        }
    }
    if start < text.len() {
        ranges.push(start..text.len());
    }
    ranges
}

/// The index of the paragraph holding byte `offset` of `text` (the last
/// paragraph for the very end).
#[must_use]
pub fn paragraph_at(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    let before = text.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    let count = paragraphs(text).len();
    before.min(count.saturating_sub(1))
}

/// Adds the newline a rich text ends with, if it is missing. Returns `true`
/// if it added one.
pub fn ensure_final_newline(text: &mut String) -> bool {
    if text.ends_with('\n') {
        false
    } else {
        text.push('\n');
        true
    }
}

/// The number of words in `text`: runs of letters and digits, with
/// apostrophes and hyphens inside a word.
#[must_use]
pub fn word_count(text: &str) -> usize {
    let mut words = 0;
    let mut inside = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_alphanumeric() {
            if !inside {
                words += 1;
                inside = true;
            }
        } else if inside
            && matches!(ch, '\'' | '\u{2019}' | '-')
            && chars.peek().is_some_and(|next| next.is_alphanumeric())
        {
            // "don't", "well-known": still one word.
        } else {
            inside = false;
        }
    }
    words
}

/// One paragraph's own marks: kind and value.
type ParagraphMarks = Vec<(String, Value)>;

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

impl TextMarks {
    /// Every paragraph's own marks, in order. A paragraph takes the last
    /// paragraph mark of each kind that overlaps it.
    fn paragraph_marks(&self, text: &str) -> Vec<ParagraphMarks> {
        let ranges = paragraphs(text);
        let mut table = vec![ParagraphMarks::new(); ranges.len()];
        let starts: Vec<usize> = ranges.iter().map(|range| range.start).collect();
        for mark in &self.marks {
            if !is_paragraph_kind(&mark.kind) {
                continue;
            }
            let first = starts
                .partition_point(|start| *start <= mark.range.start)
                .saturating_sub(1);
            for (index, range) in ranges.iter().enumerate().skip(first) {
                if range.start >= mark.range.end {
                    break;
                }
                if mark.range.start < range.end {
                    let own = &mut table[index];
                    own.retain(|(kind, _)| *kind != mark.kind);
                    own.push((mark.kind.clone(), mark.value.clone()));
                }
            }
        }
        table
    }

    /// Replaces every paragraph mark with one per paragraph of `text`, each
    /// covering its whole paragraph, as `table` says.
    fn set_paragraph_marks(&mut self, text: &str, table: &[ParagraphMarks]) {
        self.marks.retain(|mark| !is_paragraph_kind(&mark.kind));
        for (range, own) in paragraphs(text).into_iter().zip(table) {
            for (kind, value) in own {
                self.marks.push(Mark {
                    range: range.clone(),
                    kind: kind.clone(),
                    value: value.clone(),
                    inclusive: true,
                });
            }
        }
        self.marks
            .sort_by_key(|mark| (mark.range.start, mark.range.end));
    }

    /// The value of paragraph kind `kind` for paragraph `index` of `text`.
    #[must_use]
    pub fn paragraph_value(&self, text: &str, index: usize, kind: &str) -> Option<&Value> {
        let range = paragraphs(text).into_iter().nth(index)?;
        self.marks
            .iter()
            .rev()
            .find(|mark| {
                mark.kind == kind && mark.range.start < range.end && range.start < mark.range.end
            })
            .map(|mark| &mark.value)
    }

    /// Sets (or with `None` clears) paragraph kind `kind` on every
    /// paragraph of `text` that `range` touches.
    pub fn set_paragraphs(
        &mut self,
        text: &str,
        range: Range<usize>,
        kind: &str,
        value: Option<Value>,
    ) {
        let mut table = self.paragraph_marks(text);
        let first = paragraph_at(text, range.start);
        let last = paragraph_at(text, range.end.max(range.start + 1) - 1).max(first);
        for own in table.iter_mut().take(last + 1).skip(first) {
            own.retain(|(known, _)| known != kind);
            if let Some(value) = &value {
                own.push((kind.to_owned(), value.clone()));
            }
        }
        self.set_paragraph_marks(text, &table);
    }

    /// Sets (or with `None` clears) character kind `kind` over `range`.
    pub fn set(&mut self, range: Range<usize>, kind: &str, value: Option<Value>) {
        if range.is_empty() {
            return;
        }
        self.clear(range.clone(), kind);
        if let Some(value) = value {
            self.add(Mark::new(range, kind, value));
        }
        self.normalize();
    }

    /// `true` if every byte of `range` has `kind` set to a true value
    /// (`range` empty: the byte before it, as typing there would get).
    #[must_use]
    pub fn covers(&self, range: Range<usize>, kind: &str) -> bool {
        let truthy = |value: &Value| value.as_bool() != Some(false);
        if range.is_empty() {
            let at = range.start.saturating_sub(1);
            return self.value_at(at, kind).is_some_and(truthy);
        }
        let mut position = range.start;
        let mut spans: Vec<&Mark> = self
            .marks
            .iter()
            .filter(|mark| {
                mark.kind == kind
                    && truthy(&mark.value)
                    && mark.range.start < range.end
                    && range.start < mark.range.end
            })
            .collect();
        spans.sort_by_key(|mark| mark.range.start);
        for mark in spans {
            if mark.range.start > position {
                return false;
            }
            position = position.max(mark.range.end);
        }
        position >= range.end
    }

    /// Joins marks of the same kind and value that touch or overlap.
    pub fn normalize(&mut self) {
        self.marks
            .sort_by(|a, b| (&a.kind, a.range.start).cmp(&(&b.kind, b.range.start)));
        let mut joined: Vec<Mark> = Vec::with_capacity(self.marks.len());
        for mark in self.marks.drain(..) {
            if let Some(last) = joined.last_mut()
                && last.kind == mark.kind
                && last.value == mark.value
                && last.inclusive == mark.inclusive
                && mark.range.start <= last.range.end
                && !is_paragraph_kind(&mark.kind)
            {
                last.range.end = last.range.end.max(mark.range.end);
                continue;
            }
            joined.push(mark);
        }
        joined.retain(|mark| !mark.range.is_empty());
        joined.sort_by_key(|mark| (mark.range.start, mark.range.end));
        self.marks = joined;
    }

    /// The marks inside `range`, moved to start at 0 (what a copy takes).
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Self {
        let mut out = Self::new();
        for mark in &self.marks {
            let start = mark.range.start.max(range.start);
            let end = mark.range.end.min(range.end);
            if start < end {
                let mut piece = mark.clone();
                piece.range = start - range.start..end - range.start;
                out.marks.push(piece);
            }
        }
        out
    }

    /// Keeps the marks in place after `inserted` went into `before` at byte
    /// `offset`. Characters typed take the formatting before them; the
    /// paragraphs a newline splits off keep their paragraph's marks.
    /// `source` gives pasted text its own formatting: its character marks,
    /// and its paragraphs' marks for the paragraphs its newlines end.
    pub fn insert_text(
        &mut self,
        before: &str,
        offset: usize,
        inserted: &str,
        source: Option<&Self>,
    ) {
        if inserted.is_empty() {
            return;
        }
        let offset = offset.min(before.len());
        let mut table = self.paragraph_marks(before);
        if table.is_empty() {
            table.push(ParagraphMarks::new());
        }
        let at = paragraph_at(before, offset).min(table.len() - 1);
        let own = table[at].clone();
        let breaks = inserted.bytes().filter(|byte| *byte == b'\n').count();
        let mut added: Vec<ParagraphMarks> = vec![own; breaks];
        if let Some(source) = source {
            let theirs = source.paragraph_marks(inserted);
            for (slot, marks) in added.iter_mut().zip(theirs) {
                *slot = marks;
            }
        }
        // The paragraphs the new newlines end come first; the paragraph's
        // own newline (and its marks) end the last piece.
        for (index, marks) in added.into_iter().enumerate() {
            table.insert(at + index, marks);
        }
        self.on_insert(offset, inserted.len());
        if let Some(source) = source {
            let range = offset..offset + inserted.len();
            let mut kinds: Vec<String> = source
                .marks
                .iter()
                .chain(&self.marks)
                .map(|mark| mark.kind.clone())
                .filter(|kind| !is_paragraph_kind(kind))
                .collect();
            kinds.sort();
            kinds.dedup();
            for kind in kinds {
                self.clear(range.clone(), &kind);
            }
            for mark in &source.marks {
                if !is_paragraph_kind(&mark.kind) {
                    let mut moved = mark.clone();
                    moved.range = mark.range.start + offset..mark.range.end + offset;
                    self.marks.push(moved);
                }
            }
        }
        let mut after = String::with_capacity(before.len() + inserted.len());
        after.push_str(&before[..offset]);
        after.push_str(inserted);
        after.push_str(&before[offset..]);
        table.truncate(paragraphs(&after).len());
        self.set_paragraph_marks(&after, &table);
        self.normalize();
    }

    /// Keeps the marks in place after `range` of `before` was deleted. When
    /// paragraphs join, the joined paragraph keeps the marks of the first,
    /// unless the deletion took that one from its very start (deleting
    /// whole paragraphs leaves the next one as it was).
    pub fn delete_text(&mut self, before: &str, range: Range<usize>) {
        let range = range.start.min(before.len())..range.end.min(before.len());
        if range.is_empty() {
            return;
        }
        let mut table = self.paragraph_marks(before);
        let ranges = paragraphs(before);
        let first = paragraph_at(before, range.start);
        let joins = before.as_bytes()[range.clone()]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        if joins > 0 && first < table.len() {
            let from_start = ranges
                .get(first)
                .is_some_and(|own| own.start == range.start);
            let last = (first + joins).min(table.len() - 1);
            let kept = if from_start {
                table[last].clone()
            } else {
                table[first].clone()
            };
            table.splice(first..=last, [kept]);
        }
        self.on_delete(range.clone());
        let mut after = String::with_capacity(before.len() - range.len());
        after.push_str(&before[..range.start]);
        after.push_str(&before[range.end..]);
        table.truncate(paragraphs(&after).len());
        self.set_paragraph_marks(&after, &table);
        self.normalize();
    }

    /// Fits the marks to `text`: drops what lies past its end and gives each
    /// paragraph one mark of each of its paragraph kinds. For marks made
    /// elsewhere (an import) before they are edited.
    pub fn fit(&mut self, text: &str) {
        let len = text.len();
        for mark in &mut self.marks {
            mark.range.end = mark.range.end.min(len);
            mark.range.start = mark.range.start.min(mark.range.end);
        }
        self.marks.retain(|mark| !mark.range.is_empty());
        let table = self.paragraph_marks(text);
        self.set_paragraph_marks(text, &table);
        self.normalize();
    }
}

/// How a run of characters is formatted: the character kinds, read from
/// the marks over it (paragraph kinds are read with the paragraph).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharFormat {
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
    /// Monospace code.
    pub code: bool,
    /// Small capitals.
    pub small_caps: bool,
    /// Another font family.
    pub font: Option<String>,
    /// Another size, in points.
    pub size: Option<f32>,
    /// A text color.
    pub color: Option<[u8; 3]>,
    /// A highlight.
    pub highlight: Option<[u8; 3]>,
    /// Raised (`1`) or lowered (`-1`).
    pub script: i8,
    /// A link's target.
    pub link: Option<String>,
    /// `true` inside a comment.
    pub comment: bool,
    /// A footnote that follows this run.
    pub footnote: Option<String>,
}

/// The sRGB bytes of a color value.
#[must_use]
pub fn rgb_of(value: &Value) -> Option<[u8; 3]> {
    match value {
        Value::Color(color) => {
            let srgb = color.to_space(sue_core::ColorSpace::Srgb);
            Some(
                srgb.rgb
                    .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8),
            )
        }
        Value::Text(hex) => {
            let hex = hex.trim_start_matches('#');
            if hex.len() != 6 {
                return None;
            }
            let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
            Some([byte(0)?, byte(2)?, byte(4)?])
        }
        _ => None,
    }
}

/// A size value in points: a number, or a length quantity.
#[must_use]
pub fn points_of(value: &Value) -> Option<f32> {
    match value {
        Value::Quantity(quantity) => quantity.in_unit("pt").map(|points| points as f32),
        other => other.as_f64().map(|points| points as f32),
    }
}

/// How the bytes `range` look under `marks` (character kinds only).
#[must_use]
pub fn char_format(marks: &[&Mark], range: &Range<usize>) -> CharFormat {
    let mut look = CharFormat::default();
    for mark in marks {
        if mark.range.start > range.start || mark.range.end < range.end || range.is_empty() {
            continue;
        }
        let on = mark.value.as_bool() != Some(false);
        match mark.kind.as_str() {
            kind::BOLD => look.bold = on,
            kind::ITALIC => look.italic = on,
            kind::UNDERLINE => look.underline = on,
            kind::STRIKE => look.strike = on,
            kind::CODE => look.code = on,
            kind::SMALL_CAPS => look.small_caps = on,
            kind::FONT => look.font = mark.value.as_str().map(str::to_owned),
            kind::SIZE => look.size = points_of(&mark.value),
            kind::COLOR => look.color = rgb_of(&mark.value),
            kind::HIGHLIGHT => look.highlight = rgb_of(&mark.value),
            kind::SCRIPT => {
                look.script = match mark.value.as_str() {
                    Some("super") => 1,
                    Some("sub") => -1,
                    _ => 0,
                };
            }
            kind::LINK => look.link = mark.value.as_str().map(str::to_owned),
            kind::COMMENT => look.comment = true,
            kind::FOOTNOTE if mark.range.end == range.end => {
                look.footnote = mark.value.as_str().map(str::to_owned);
            }
            _ => {}
        }
    }
    look
}

/// The runs of one paragraph: the byte ranges (inside `range`) where the
/// character formatting stays the same, with how each looks.
#[must_use]
pub fn runs(marks: &[&Mark], range: Range<usize>) -> Vec<(Range<usize>, CharFormat)> {
    let mut cuts = vec![range.start, range.end];
    for mark in marks {
        if is_paragraph_kind(&mark.kind) {
            continue;
        }
        for at in [mark.range.start, mark.range.end] {
            if at > range.start && at < range.end {
                cuts.push(at);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(Range<usize>, CharFormat)> = Vec::new();
    for pair in cuts.windows(2) {
        let piece = pair[0]..pair[1];
        let look = char_format(marks, &piece);
        match out.last_mut() {
            Some((last, known)) if *known == look => last.end = piece.end,
            _ => out.push((piece, look)),
        }
    }
    out
}

/// The marks that touch each paragraph: `paragraphs` in order, `marks`
/// sorted by start.
#[must_use]
pub fn marks_by_paragraph<'a>(
    marks: &'a [Mark],
    paragraphs: &[Range<usize>],
) -> Vec<Vec<&'a Mark>> {
    let mut out = Vec::with_capacity(paragraphs.len());
    let mut active: Vec<&Mark> = Vec::new();
    let mut next = 0;
    for paragraph in paragraphs {
        while next < marks.len() && marks[next].range.start < paragraph.end {
            active.push(&marks[next]);
            next += 1;
        }
        active.retain(|mark| mark.range.end > paragraph.start);
        out.push(
            active
                .iter()
                .copied()
                .filter(|mark| mark.range.start < paragraph.end)
                .collect(),
        );
    }
    out
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

    fn styles(text: &str, marks: &TextMarks) -> Vec<Option<String>> {
        (0..paragraphs(text).len())
            .map(|index| {
                marks
                    .paragraph_value(text, index, kind::STYLE)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect()
    }

    #[test]
    fn paragraphs_keep_their_marks_through_edits() {
        let mut text = String::from("Title\nBody text\n");
        let mut marks = TextMarks::new();
        marks.set_paragraphs(&text, 0..1, kind::STYLE, Some("heading1".into()));
        assert_eq!(marks.marks()[0].range, 0..6, "the newline carries it");
        assert_eq!(styles(&text, &marks), [Some("heading1".to_owned()), None]);

        // Enter at the end of the heading: both halves are headings (the
        // editor picks the next style itself), and the body stays as it was.
        let before = text.clone();
        text.insert(5, '\n');
        marks.insert_text(&before, 5, "\n", None);
        assert_eq!(text, "Title\n\nBody text\n");
        assert_eq!(
            styles(&text, &marks),
            [
                Some("heading1".to_owned()),
                Some("heading1".to_owned()),
                None
            ]
        );
        // An empty paragraph keeps its own style.
        marks.set_paragraphs(&text, 6..6, kind::STYLE, Some("quote".into()));
        assert_eq!(styles(&text, &marks)[1].as_deref(), Some("quote"));

        // Typing at the start of a paragraph stays in that paragraph.
        let before = text.clone();
        text.insert_str(6, "Hi");
        marks.insert_text(&before, 6, "Hi", None);
        assert_eq!(styles(&text, &marks)[1].as_deref(), Some("quote"));
        assert_eq!(styles(&text, &marks)[0].as_deref(), Some("heading1"));

        // Backspace at the start of a paragraph joins it to the one above,
        // which keeps its marks.
        let before = text.clone();
        text.replace_range(5..6, "");
        marks.delete_text(&before, 5..6);
        assert_eq!(text, "TitleHi\nBody text\n");
        assert_eq!(styles(&text, &marks), [Some("heading1".to_owned()), None]);

        // Deleting a whole paragraph leaves the next one as it was.
        let mut text = String::from("A\nB\nC\n");
        let mut marks = TextMarks::new();
        marks.set_paragraphs(&text, 2..2, kind::STYLE, Some("b".into()));
        marks.set_paragraphs(&text, 4..4, kind::STYLE, Some("c".into()));
        let before = text.clone();
        text.replace_range(2..4, "");
        marks.delete_text(&before, 2..4);
        assert_eq!(text, "A\nC\n");
        assert_eq!(styles(&text, &marks), [None, Some("c".to_owned())]);
    }

    #[test]
    fn pasting_brings_its_own_formatting() {
        let source_text = "Big\nsmall";
        let mut source = TextMarks::new();
        source.add(Mark::new(0..3, kind::BOLD, true));
        source.set_paragraphs(source_text, 0..1, kind::HEADING, Some(1_i64.into()));
        let mut text = String::from("ab\n");
        let mut marks = TextMarks::new();
        marks.add(Mark::new(0..2, kind::ITALIC, true));
        let before = text.clone();
        text.insert_str(1, source_text);
        marks.insert_text(&before, 1, source_text, Some(&source));
        assert_eq!(text, "aBig\nsmallb\n");
        assert!(marks.covers(1..4, kind::BOLD));
        assert!(
            !marks.covers(1..4, kind::ITALIC),
            "pasted text keeps its own look"
        );
        assert!(marks.covers(0..1, kind::ITALIC) && marks.covers(10..11, kind::ITALIC));
        assert_eq!(
            marks.paragraph_value(&text, 0, kind::HEADING),
            Some(&Value::Int(1))
        );
        assert_eq!(marks.paragraph_value(&text, 1, kind::HEADING), None);
        let copied = marks.slice(1..4);
        assert!(copied.covers(0..3, kind::BOLD));
    }

    #[test]
    fn toggling_joins_marks_and_words_count() {
        let mut marks = TextMarks::new();
        marks.set(0..3, kind::BOLD, Some(true.into()));
        marks.set(3..6, kind::BOLD, Some(true.into()));
        assert_eq!(marks.marks().len(), 1);
        assert!(marks.covers(0..6, kind::BOLD));
        marks.set(2..4, kind::BOLD, None);
        assert!(!marks.covers(0..6, kind::BOLD));
        assert_eq!(marks.marks().len(), 2);
        assert_eq!(word_count("It's a well-known fact - 42 times."), 6);
        assert_eq!(word_count(""), 0);
        let mut text = String::from("x");
        assert!(ensure_final_newline(&mut text));
        assert!(!ensure_final_newline(&mut text));
        assert_eq!(paragraphs("a\n\nb"), vec![0..2, 2..3, 3..4]);
        assert_eq!(paragraph_at("a\n\nb", 4), 2);
        let mut stray = TextMarks::new();
        stray.add(Mark::new(0..1, kind::HEADING, 2_i64));
        stray.add(Mark::new(3..40, kind::BOLD, true));
        stray.fit("ab\ncd\n");
        assert_eq!(stray.marks()[0].range, 0..3);
        assert_eq!(stray.marks()[1].range, 3..6);
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
