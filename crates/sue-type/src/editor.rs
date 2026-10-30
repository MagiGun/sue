//! The rich text editor every studio that writes uses: one or more rich
//! text documents (a chapter, or a whole folder of scenes one after
//! another) shown as printed pages or one long column, edited in place in
//! the arena. It draws its own caret and selection, so pages, styles,
//! lists and pictures stay exact, and takes the keyboard, the mouse, the
//! clipboard (keeping formatting between Sue's own documents) and input
//! methods. Edits go through [`crate::edit`], so undo covers them. The
//! caret glides to its new place and blinks at rest ([`crate::caret`]).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use egui::text::{CCursor, CharIndex, LayoutJob, TextFormat};
use egui::{Color32, Event, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use sue_core::{EntityId, Value};
use sue_model::{TextMarks, mark_kind as kind};
use sue_wm::{AppletContext, ArenaEdit};

use crate::caret::{self, CaretGlide, CaretMotion};
use crate::edit;
use crate::layout::{
    Block, Ink, Laid, PX_PER_PT, PageSetup, Setting, Source, ViewMode, byte_of_char, char_of_byte,
    lay_document, paginate,
};
use crate::style::{CharFormat, ParagraphLook, StyleSheet, char_format, paragraph_look};
pub use crate::tables::TableEdit;
use crate::tables::{CellCaret, TableSource, paint_table};

/// A place in the shown documents: which one, and a byte offset in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    /// The document, an index into the shown documents.
    pub doc: usize,
    /// The byte offset.
    pub offset: usize,
}

/// How the editor shows its documents.
pub struct EditorView<'a> {
    /// Pages or one column.
    pub mode: ViewMode,
    /// The page size and margins.
    pub page: &'a PageSetup,
    /// 1.0 is 100%.
    pub zoom: f32,
    /// The styles.
    pub sheet: &'a StyleSheet,
    /// The colors of the text and the page.
    pub ink: Ink,
    /// Around the pages.
    pub desk: Color32,
    /// The picture to show for an embedded entity (once it is uploaded),
    /// with its size in points.
    pub figures: &'a dyn Fn(EntityId) -> Option<(Option<egui::TextureId>, Vec2)>,
    /// Keep the caret's line in the middle of the view.
    pub typewriter: bool,
    /// Turn straight quotes into curly ones while typing.
    pub smart_quotes: bool,
    /// Nothing can be changed.
    pub read_only: bool,
    /// The names of the documents, shown between them when there are
    /// several.
    pub titles: &'a [String],
    /// Spelling as you type, while set: the words it also takes as right
    /// (lower case; a project's own dictionary).
    pub spelling: Option<&'a dyn Fn(&str) -> bool>,
    /// The room around the pages or the column, in screen points; `None`
    /// for the usual (24 around pages, 32 around a column). A phone wants
    /// less, so that the lines get the width.
    pub gap: Option<f32>,
}

/// What happened in a frame.
#[derive(Clone, Debug, Default)]
pub struct EditorOutput {
    /// The text or its formatting changed.
    pub changed: bool,
    /// The editor has the keyboard.
    pub focused: bool,
    /// The page the caret is on, from 0.
    pub page: usize,
    /// The number of pages.
    pub pages: usize,
    /// A link that was clicked with Ctrl held.
    pub link: Option<String>,
    /// Ctrl and the wheel asked to zoom by this factor.
    pub zoom: Option<f32>,
    /// A word to add to the dictionary (lower case), from the spelling
    /// menu.
    pub learn: Option<String>,
}

/// A heading of the shown documents.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineEntry {
    /// 1 for the biggest headings.
    pub level: u8,
    /// Its text.
    pub title: String,
    /// Where it is.
    pub at: Caret,
}

#[derive(Clone, Debug, PartialEq)]
struct DocKey {
    revision: u64,
    sheet: u64,
    width: u32,
    scale: u32,
    fonts: usize,
    ink: [u8; 4],
    /// The revisions of the tables it shows.
    embeds: u64,
}

struct Cached {
    key: DocKey,
    blocks: Vec<Block>,
}

/// Formatted text copied from one of Sue's documents, so pasting it back
/// keeps its look (the system clipboard holds the plain text).
fn clipboard() -> &'static Mutex<Option<(String, TextMarks)>> {
    static CLIPBOARD: OnceLock<Mutex<Option<(String, TextMarks)>>> = OnceLock::new();
    CLIPBOARD.get_or_init(Mutex::default)
}

/// The rich text editor.
pub struct Editor {
    id: Id,
    caret: Caret,
    anchor: Caret,
    /// The x to keep while moving up and down, in screen pixels.
    h_pos: Option<f32>,
    /// Scroll the caret into view this frame.
    follow: bool,
    /// Formatting chosen with nothing selected, for what is typed next.
    pending: Vec<(String, Option<Value>)>,
    /// Text an input method is composing.
    preedit: Option<String>,
    /// The last press: when, where and how many in a row.
    clicks: (f64, Pos2, u32),
    /// The last press was a finger's: a drag scrolls the page (as in
    /// every phone's word processor), a tap places the caret, and a long
    /// press selects a word for the drag to grow.
    touch: bool,
    /// A long press began a selection that the finger now drags.
    touch_select: bool,
    /// A finger down on the page, scrolling it as it moves: where it
    /// was last frame, and its recent moves (when, how far) for a flick.
    finger: Option<(Pos2, Vec<(f64, Vec2)>)>,
    docs: Vec<EntityId>,
    texts: Vec<String>,
    marks: Vec<TextMarks>,
    cache: HashMap<EntityId, Cached>,
    laid: Laid,
    /// The pieces of each block.
    pieces_of: Vec<Range<usize>>,
    /// The content's top left on screen, and where pages start across.
    origin: Pos2,
    page_x: f32,
    gap: f32,
    viewport: Rect,
    /// Asked for by the studio before the next frame.
    want_focus: bool,
    /// Keep the keyboard through this frame's press elsewhere (a toolbar
    /// button), so that a phone's keyboard stays up.
    hold: bool,
    /// The editor had the keyboard last frame.
    had_focus: bool,
    scroll_target: Option<Caret>,
    /// How the caret glides and blinks, from the rice and the hardware.
    motion: CaretMotion,
    glide: CaretGlide,
    /// The caret in a table cell, while it is in one.
    cell: Option<CellCaret>,
    /// The spelling menu of the word last pressed with the secondary
    /// button: its document, its range and what it may have meant.
    spell_menu: Option<(usize, Range<usize>, Vec<String>)>,
    /// Words this sitting takes as right (Ignore All).
    ignored: std::collections::HashSet<String>,
    /// Changes are recorded as made by this author on this date (track
    /// changes), while set.
    tracking: Option<sue_model::changes::Change>,
    /// The deletion under way goes forward (the Delete key).
    delete_forward: bool,
    /// A cell the caret left this frame, its typing still to record.
    left_cell: Option<CellCaret>,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor")
            .field("caret", &self.caret)
            .field("anchor", &self.anchor)
            .field("docs", &self.docs)
            .finish_non_exhaustive()
    }
}

impl Editor {
    /// An editor with its own egui id.
    #[must_use]
    pub fn new(salt: &str) -> Self {
        Self {
            id: Id::unique(("sue-type-editor", salt)),
            caret: Caret::default(),
            anchor: Caret::default(),
            h_pos: None,
            follow: false,
            pending: Vec::new(),
            preedit: None,
            clicks: (f64::NEG_INFINITY, Pos2::ZERO, 0),
            touch: false,
            touch_select: false,
            finger: None,
            docs: Vec::new(),
            texts: Vec::new(),
            marks: Vec::new(),
            cache: HashMap::new(),
            laid: Laid::default(),
            pieces_of: Vec::new(),
            origin: Pos2::ZERO,
            page_x: 0.0,
            gap: 24.0,
            viewport: Rect::NOTHING,
            want_focus: false,
            hold: false,
            had_focus: false,
            scroll_target: None,
            motion: CaretMotion::STILL,
            glide: CaretGlide::default(),
            cell: None,
            left_cell: None,
            tracking: None,
            delete_forward: false,
            spell_menu: None,
            ignored: std::collections::HashSet::new(),
        }
    }

    /// Its egui id (what has the keyboard when it does).
    #[must_use]
    pub fn id(&self) -> Id {
        self.id
    }

    // ----- What the studio reads and asks for ------------------------------

    /// The documents shown last frame.
    #[must_use]
    pub fn docs(&self) -> &[EntityId] {
        &self.docs
    }

    /// The document the caret is in.
    #[must_use]
    pub fn current(&self) -> Option<EntityId> {
        self.docs.get(self.caret.doc).copied()
    }

    /// The caret.
    #[must_use]
    pub fn caret(&self) -> Caret {
        self.caret
    }

    /// The selected bytes of the caret's document (empty: the caret).
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        if self.anchor.doc != self.caret.doc {
            return self.caret.offset..self.caret.offset;
        }
        let (a, b) = (self.anchor.offset, self.caret.offset);
        a.min(b)..a.max(b)
    }

    /// The selected text.
    #[must_use]
    pub fn selected_text(&self) -> &str {
        let range = self.selection();
        self.texts
            .get(self.caret.doc)
            .and_then(|text| text.get(range))
            .unwrap_or("")
    }

    /// The text of the caret's document, as of the last frame.
    #[must_use]
    pub fn text(&self) -> &str {
        self.texts.get(self.caret.doc).map_or("", String::as_str)
    }

    /// Puts the caret in document `doc` and selects `range` of it, then
    /// scrolls there.
    pub fn select(&mut self, doc: EntityId, range: Range<usize>) {
        let index = self
            .docs
            .iter()
            .position(|known| *known == doc)
            .unwrap_or(0);
        if self.docs.is_empty() {
            self.docs.push(doc);
        }
        self.anchor = Caret {
            doc: index,
            offset: range.start,
        };
        self.caret = Caret {
            doc: index,
            offset: range.end,
        };
        self.pending.clear();
        self.h_pos = None;
        self.scroll_target = Some(self.caret);
        self.want_focus = true;
    }

    /// Gives the editor the keyboard on the next frame.
    pub fn focus(&mut self) {
        self.want_focus = true;
    }

    /// Keeps the keyboard, if the editor has it, through a press this
    /// frame outside the text: the studio calls it when the press is on
    /// its own toolbar, so that tapping Bold leaves a phone's on-screen
    /// keyboard up and the caret where it was.
    pub fn keep_focus(&mut self) {
        self.hold = true;
    }

    /// `true` while the editor has the keyboard.
    #[must_use]
    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|memory| memory.has_focus(self.id))
    }

    /// How the caret's paragraph and the selection's text look, for the
    /// toolbar to show.
    #[must_use]
    pub fn look_here(&self, sheet: &StyleSheet) -> (ParagraphLook, CharFormat) {
        let (Some(text), Some(marks)) = (
            self.texts.get(self.caret.doc),
            self.marks.get(self.caret.doc),
        ) else {
            return (paragraph_look(sheet, &[]), CharFormat::default());
        };
        let paragraphs = sue_model::paragraphs(text);
        let index = sue_model::paragraph_at(text, self.caret.offset);
        let all: Vec<&sue_model::Mark> = marks.marks().iter().collect();
        let here: Vec<&sue_model::Mark> = paragraphs
            .get(index)
            .map(|range| {
                all.iter()
                    .copied()
                    .filter(|mark| mark.range.start < range.end && range.start < mark.range.end)
                    .collect()
            })
            .unwrap_or_default();
        let selection = self.selection();
        let probe = if selection.is_empty() {
            let at = selection.start.saturating_sub(1);
            let at = if paragraphs
                .get(index)
                .is_some_and(|range| range.start == selection.start)
            {
                selection.start
            } else {
                at
            };
            at..at + 1
        } else {
            selection
        };
        let mut chars = char_format(&here, &probe);
        for (name, value) in &self.pending {
            let on = value
                .as_ref()
                .is_some_and(|value| value.as_bool() != Some(false));
            match name.as_str() {
                kind::BOLD => chars.bold = on,
                kind::ITALIC => chars.italic = on,
                kind::UNDERLINE => chars.underline = on,
                kind::STRIKE => chars.strike = on,
                _ => {}
            }
        }
        (paragraph_look(sheet, &here), chars)
    }

    /// The headings of the shown documents, in order.
    #[must_use]
    pub fn outline(&self) -> Vec<OutlineEntry> {
        self.laid
            .blocks
            .iter()
            .filter_map(|block| {
                let level = block.look.outline?;
                let text = self.texts.get(block.doc)?;
                Some(OutlineEntry {
                    level,
                    title: text.get(block.range.clone())?.trim().to_owned(),
                    at: Caret {
                        doc: block.doc,
                        offset: block.range.start,
                    },
                })
            })
            .collect()
    }

    /// The page count of the last layout.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.laid.pages
    }

    // ----- Formatting --------------------------------------------------------

    /// Turns a character kind on or off over the selection; with nothing
    /// selected, for what is typed next.
    pub fn toggle(&mut self, cx: &mut AppletContext<'_>, name: &str, label: &str) {
        let Some(doc) = self.current() else {
            return;
        };
        let range = self.selection();
        if range.is_empty() {
            let on_now = self
                .pending
                .iter()
                .rev()
                .find(|(known, _)| known == name)
                .map(|(_, value)| value.is_some())
                .unwrap_or_else(|| edit::marks_of(cx.arena(), doc).covers(range.clone(), name));
            self.pending.retain(|(known, _)| known != name);
            self.pending
                .push((name.to_owned(), (!on_now).then_some(Value::Bool(true))));
            return;
        }
        edit::toggle(cx, doc, range, name, label);
    }

    /// Sets (or clears) a character kind over the selection; with nothing
    /// selected, for what is typed next.
    pub fn set_char(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        value: Option<Value>,
        label: &str,
    ) {
        let Some(doc) = self.current() else {
            return;
        };
        let range = self.selection();
        if range.is_empty() {
            self.pending.retain(|(known, _)| known != name);
            self.pending.push((name.to_owned(), value));
            return;
        }
        edit::reformat(cx, doc, label, |_, marks| marks.set(range, name, value));
    }

    /// Clears every character kind from the selection.
    pub fn clear_formatting(&mut self, cx: &mut AppletContext<'_>) {
        let Some(doc) = self.current() else {
            return;
        };
        let range = self.selection();
        self.pending.clear();
        if range.is_empty() {
            return;
        }
        edit::reformat(cx, doc, &tr!("clear-formatting"), |_, marks| {
            let kinds: Vec<String> = marks
                .marks()
                .iter()
                .map(|mark| mark.kind.clone())
                .filter(|name| !sue_model::is_paragraph_kind(name))
                .collect();
            for name in kinds {
                marks.set(range.clone(), &name, None);
            }
        });
    }

    /// Sets (or clears) a paragraph kind on every selected paragraph.
    pub fn set_paragraph(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        value: Option<Value>,
        label: &str,
    ) {
        let Some(doc) = self.current() else {
            return;
        };
        let range = self.selection();
        edit::reformat(cx, doc, label, |text, marks| {
            marks.set_paragraphs(text, range, name, value);
        });
    }

    /// Gives every selected paragraph style `key`.
    pub fn set_style(&mut self, cx: &mut AppletContext<'_>, key: &str, label: &str) {
        self.set_paragraph(cx, kind::STYLE, Some(Value::Text(key.to_owned())), label);
    }

    /// Types `text` over the selection.
    pub fn insert(&mut self, cx: &mut AppletContext<'_>, text: &str) {
        self.replace_selection(cx, text, None);
    }

    /// Changes the case of the selection (or of the word at the caret),
    /// keeping its formatting and selecting what it changed. Returns
    /// `false` if nothing changed.
    pub fn change_case(&mut self, cx: &mut AppletContext<'_>, case: crate::case::Case) -> bool {
        let Some(doc) = self.current() else {
            return false;
        };
        let mut range = self.selection();
        if range.is_empty() {
            let (start, end) = word_around(self.text(), range.start);
            range = start..end;
        }
        let text = self.text();
        let before = &text[range.clone()];
        let sentence_start = text[..range.start]
            .trim_end_matches([' ', '"', '\'', '\u{201C}', '\u{2018}', '('])
            .chars()
            .next_back()
            .is_none_or(|ch| matches!(ch, '.' | '!' | '?' | '\n'));
        let after = crate::case::change(before, case, sentence_start);
        if after == before {
            return false;
        }
        let marks = self.marks[self.caret.doc].slice(range.clone());
        self.commit(cx);
        cx.typing.flush(cx.history);
        let Some(end) = edit::replace(cx, doc, range.clone(), &after, Some(&marks), &[]) else {
            return false;
        };
        cx.typing.flush(cx.history);
        self.sync_text(cx);
        self.anchor = Caret {
            doc: self.caret.doc,
            offset: range.start,
        };
        self.caret = Caret {
            doc: self.caret.doc,
            offset: end,
        };
        self.follow = true;
        true
    }

    /// Puts an embedded entity (a picture) in a paragraph of its own at the
    /// caret.
    pub fn insert_embed(&mut self, cx: &mut AppletContext<'_>, entity: EntityId) {
        self.embed(cx, entity, &tr!("insert-picture"));
    }

    /// Puts a new table of `rows` by `columns` in a paragraph of its own at
    /// the caret, its first `header_rows` rows headings, and the caret in
    /// its first cell. Returns the table's entity.
    pub fn insert_table(
        &mut self,
        cx: &mut AppletContext<'_>,
        rows: u32,
        columns: u32,
        header_rows: u32,
    ) -> Option<EntityId> {
        let entity = crate::tables::new_table(rows.max(1), columns.max(1), header_rows);
        self.place_table(cx, entity, self.selection())
    }

    /// Every comment in the shown documents, in order: the document (an
    /// index into the shown documents), the words it is about and its
    /// entity.
    #[must_use]
    pub fn comments(&self) -> Vec<(usize, Range<usize>, EntityId)> {
        let mut out = Vec::new();
        for (doc, marks) in self.marks.iter().enumerate() {
            let mut found: Vec<(Range<usize>, EntityId)> = marks
                .marks()
                .iter()
                .filter(|mark| mark.kind == kind::COMMENT)
                .filter_map(|mark| match mark.value {
                    Value::Entity(id) => Some((mark.range.clone(), id)),
                    _ => None,
                })
                .collect();
            found.sort_by_key(|(range, _)| range.start);
            out.extend(found.into_iter().map(|(range, id)| (doc, range, id)));
        }
        out
    }

    /// The comment under the caret, if any.
    #[must_use]
    pub fn comment_here(&self) -> Option<EntityId> {
        let at = self.caret.offset;
        self.comments()
            .into_iter()
            .find(|(doc, range, _)| *doc == self.caret.doc && range.start <= at && at <= range.end)
            .map(|(_, _, id)| id)
    }

    /// Puts comment `entity` on the selection (or the word at the caret),
    /// spawning it, as one undo step. Returns `false` when there are no
    /// words to comment on.
    pub fn add_comment(&mut self, cx: &mut AppletContext<'_>, entity: sue_core::Entity) -> bool {
        let Some(doc) = self.current() else {
            return false;
        };
        let mut range = self.selection();
        if range.is_empty() {
            let (start, end) = word_around(self.text(), range.start);
            range = start..end;
        }
        if range.is_empty() {
            return false;
        }
        self.commit(cx);
        let id = entity.id();
        let mut marks = self.marks[self.caret.doc].clone();
        marks.set(range.clone(), kind::COMMENT, Some(Value::Entity(id)));
        let batch = sue_core::txn::Batch::new(tr!("new-comment"))
            .then(sue_core::txn::SpawnEntity::new(entity))
            .then(sue_core::txn::SetExtension::set(doc, marks));
        if cx.apply(ArenaEdit(batch)).is_err() {
            return false;
        }
        self.sync_text(cx);
        self.select(doc, range);
        true
    }

    /// Takes comment `id` and its replies out of the shown documents, as
    /// one undo step.
    pub fn remove_comment(&mut self, cx: &mut AppletContext<'_>, id: EntityId) -> bool {
        self.commit(cx);
        let mut batch = sue_core::txn::Batch::new(tr!("delete-comment"));
        let mut found = false;
        for (index, doc) in self.docs.iter().enumerate() {
            let mut marks = self.marks[index].clone();
            let ranges: Vec<Range<usize>> = marks
                .marks()
                .iter()
                .filter(|mark| mark.kind == kind::COMMENT && mark.value == Value::Entity(id))
                .map(|mark| mark.range.clone())
                .collect();
            if ranges.is_empty() {
                continue;
            }
            for range in ranges {
                marks.set(range, kind::COMMENT, None);
            }
            batch.push(Box::new(sue_core::txn::SetExtension::set(*doc, marks)));
            found = true;
        }
        if !found {
            return false;
        }
        let replies = cx
            .arena()
            .relations()
            .targets(id, sue_model::comments::REPLY);
        for reply in replies.into_iter().rev() {
            batch.push(Box::new(sue_core::txn::DespawnEntity::new(reply)));
        }
        batch.push(Box::new(sue_core::txn::DespawnEntity::new(id)));
        let done = cx.apply(ArenaEdit(batch)).is_ok();
        let docs = self.docs.clone();
        for (index, doc) in docs.iter().enumerate() {
            self.marks[index] = edit::marks_of(cx.arena(), *doc);
        }
        done
    }

    /// Puts a table of contents with `contents`' settings in a paragraph of
    /// its own at the caret. Returns its entity.
    pub fn insert_contents(
        &mut self,
        cx: &mut AppletContext<'_>,
        contents: &sue_model::contents::Contents,
    ) -> Option<EntityId> {
        self.commit(cx);
        self.current()?;
        let entity = contents.entity();
        let id = entity.id();
        cx.apply(ArenaEdit(sue_core::txn::SpawnEntity::new(entity)))
            .ok()?;
        self.embed(cx, id, &tr!("insert-table-of-contents"));
        Some(id)
    }

    /// Spawns table `entity` and sets it over `range` of the current
    /// document, with the caret in its first cell.
    fn place_table(
        &mut self,
        cx: &mut AppletContext<'_>,
        entity: sue_core::Entity,
        range: Range<usize>,
    ) -> Option<EntityId> {
        self.commit(cx);
        self.current()?;
        let id = entity.id();
        cx.apply(ArenaEdit(sue_core::txn::SpawnEntity::new(entity)))
            .ok()?;
        self.anchor = Caret {
            doc: self.caret.doc,
            offset: range.start,
        };
        self.caret = Caret {
            doc: self.caret.doc,
            offset: range.end,
        };
        self.embed(cx, id, &tr!("insert-table"));
        self.enter_cell(id, 0, 0);
        Some(id)
    }

    /// Changes the table the caret is in. Returns `false` if the caret is
    /// in none, or nothing changed.
    pub fn edit_table(&mut self, cx: &mut AppletContext<'_>, edit: TableEdit) -> bool {
        use sue_core::ReversibleTransaction;
        use sue_model::Reshape;
        self.commit(cx);
        let Some(cell) = self.cell.clone() else {
            return false;
        };
        let (rows, columns) = crate::tables::size_of(cx.arena(), cell.table);
        let (row, column) = (cell.row as u32, cell.column as u32);
        let fill = Some(Value::Text(String::new()));
        let id = cell.table;
        let widths = cx
            .arena()
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(sue_model::WIDTHS))
            .is_some();
        let reshape = |reshape: Reshape| {
            let batch = sue_core::txn::Batch::new(reshape.label()).then(reshape);
            if widths {
                // New columns make every column even again.
                batch.then(sue_core::txn::SetProperty::remove(id, sue_model::WIDTHS))
            } else {
                batch
            }
        };
        let (step, moved) = match edit {
            TableEdit::RowAbove => (
                reshape(Reshape::insert_rows(id, row, 1, fill)),
                (row, column),
            ),
            TableEdit::RowBelow => (
                reshape(Reshape::insert_rows(id, row + 1, 1, fill)),
                (row + 1, column),
            ),
            TableEdit::ColumnLeft => (
                reshape(Reshape::insert_columns(id, column, 1, fill)),
                (row, column),
            ),
            TableEdit::ColumnRight => (
                reshape(Reshape::insert_columns(id, column + 1, 1, fill)),
                (row, column + 1),
            ),
            TableEdit::DeleteRow if rows > 1 => (
                reshape(Reshape::remove_rows(id, row, 1)),
                (row.min(rows as u32 - 2), column),
            ),
            TableEdit::DeleteColumn if columns > 1 => (
                reshape(Reshape::remove_columns(id, column, 1)),
                (row, column.min(columns as u32 - 2)),
            ),
            TableEdit::HeaderRow => {
                let now = cx
                    .arena()
                    .get_by_id(id)
                    .ok()
                    .and_then(|entity| entity.property(sue_model::HEADER_ROWS))
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let step = sue_core::txn::Batch::new(tr!("heading-row")).then(
                    sue_core::txn::SetProperty::new(
                        id,
                        sue_model::HEADER_ROWS,
                        Value::Int(i64::from(now == 0)),
                    ),
                );
                (step, (row, column))
            }
            TableEdit::DeleteRow | TableEdit::DeleteColumn | TableEdit::DeleteTable => {
                return self.delete_table(cx);
            }
        };
        if cx.apply(ArenaEdit(step)).is_err() {
            return false;
        }
        if let Some(cell) = self.cell.as_mut() {
            cell.row = moved.0 as usize;
            cell.column = moved.1 as usize;
            cell.offset = 0;
        }
        true
    }

    /// Takes the table the caret is in out of the text.
    fn delete_table(&mut self, cx: &mut AppletContext<'_>) -> bool {
        let Some(cell) = self.cell.take() else {
            return false;
        };
        let Some(index) = self.table_block(cell.doc, cell.table) else {
            return false;
        };
        let Some(doc) = self.docs.get(cell.doc).copied() else {
            return false;
        };
        let range = self.laid.blocks[index].range.clone();
        let text = self.doc_text(cell.doc);
        let end = range.end + usize::from(text.as_bytes().get(range.end) == Some(&b'\n'));
        cx.typing.flush(cx.history);
        if edit::replace(cx, doc, range.start..end, "", None, &[]).is_none() {
            return false;
        }
        cx.typing.flush(cx.history);
        self.set_caret(
            Caret {
                doc: cell.doc,
                offset: range.start,
            },
            false,
        );
        self.sync_text(cx);
        true
    }

    /// Turns the table the caret is in into text: a paragraph for each
    /// row, its cells parted by tabs.
    pub fn table_to_text(&mut self, cx: &mut AppletContext<'_>) -> bool {
        self.commit(cx);
        let Some(cell) = self.cell.clone() else {
            return false;
        };
        let Some(source) = TableSource::read(cx.arena(), cell.table) else {
            return false;
        };
        let lines: Vec<String> = source
            .cells
            .iter()
            .map(|row| row.join("\t").replace('\n', " "))
            .collect();
        let Some(index) = self.table_block(cell.doc, cell.table) else {
            return false;
        };
        let Some(doc) = self.docs.get(cell.doc).copied() else {
            return false;
        };
        let range = self.laid.blocks[index].range.clone();
        self.cell = None;
        cx.typing.flush(cx.history);
        let Some(end) = edit::replace(cx, doc, range, &lines.join("\n"), None, &[]) else {
            return false;
        };
        cx.typing.flush(cx.history);
        self.set_caret(
            Caret {
                doc: cell.doc,
                offset: end,
            },
            false,
        );
        self.sync_text(cx);
        true
    }

    /// Turns the selected paragraphs into a table: a row for each, cut
    /// into cells at its tabs (or, without tabs, at its commas).
    pub fn text_to_table(&mut self, cx: &mut AppletContext<'_>) -> Option<EntityId> {
        let range = self.paragraph_span();
        let text = self.doc_text(self.caret.doc)[range.clone()].to_owned();
        let tabs = text.contains('\t');
        let cells: Vec<Vec<String>> = text
            .lines()
            .map(|line| {
                if tabs {
                    line.split('\t').map(str::to_owned).collect()
                } else {
                    line.split(',').map(|cell| cell.trim().to_owned()).collect()
                }
            })
            .collect();
        if cells.is_empty() {
            return None;
        }
        let entity = crate::tables::table_of(&cells, 0);
        self.place_table(cx, entity, range)
    }

    /// The selected paragraphs, without the last one's newline.
    fn paragraph_span(&self) -> Range<usize> {
        let text = self.doc_text(self.caret.doc);
        let selection = self.selection();
        let paragraphs = sue_model::paragraphs(text);
        let first = sue_model::paragraph_at(text, selection.start);
        let last = sue_model::paragraph_at(text, selection.end);
        let start = paragraphs.get(first).map_or(0, |range| range.start);
        let end = paragraphs.get(last).map_or(text.len(), |range| {
            range.end - usize::from(text[range.clone()].ends_with('\n'))
        });
        start..end.max(start)
    }

    /// Puts `entity` in a paragraph of its own over the selection, as one
    /// edit labeled `label`.
    fn embed(&mut self, cx: &mut AppletContext<'_>, entity: EntityId, label: &str) {
        let Some(doc) = self.current() else {
            return;
        };
        let at = self.selection().start;
        let text = self.text().to_owned();
        let starts_line = at == 0 || text.as_bytes().get(at - 1) == Some(&b'\n');
        let ends_line = text.as_bytes().get(self.selection().end) == Some(&b'\n');
        let mut insert = String::new();
        if !starts_line {
            insert.push('\n');
        }
        let object_at = at + insert.len();
        insert.push(sue_model::OBJECT);
        if !ends_line {
            insert.push('\n');
        }
        let Some(end) = edit::replace(cx, doc, self.selection(), &insert, None, &[]) else {
            return;
        };
        let object = object_at..object_at + sue_model::OBJECT.len_utf8();
        edit::reformat(cx, doc, label, |_, marks| {
            marks.set(object, kind::EMBED, Some(Value::Entity(entity)));
        });
        self.set_caret(
            Caret {
                doc: self.caret.doc,
                offset: end,
            },
            false,
        );
    }

    // ----- The frame ---------------------------------------------------------

    /// Shows `docs` one after another and lets people edit them.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        docs: &[EntityId],
        view: &EditorView<'_>,
    ) -> EditorOutput {
        let ctx = ui.ctx().clone();
        if docs != self.docs.as_slice() {
            // Keep the caret in the document it was in, if still shown.
            let current = self.current();
            let index = current
                .and_then(|id| docs.iter().position(|known| *known == id))
                .unwrap_or(0);
            if current.is_some_and(|id| !docs.contains(&id)) {
                self.caret = Caret::default();
                self.anchor = self.caret;
            } else {
                self.caret.doc = index;
                self.anchor.doc = index;
            }
        }
        self.prepare(&ctx, cx, docs, view);
        let rice = cx.rice();
        self.motion = CaretMotion::new(
            rice.get_bool(sue_wm::SMOOTH_CARET_KEY).unwrap_or(true),
            rice.get_i64(sue_wm::CARET_GLIDE_KEY)
                .unwrap_or(caret::GLIDE_MS),
            cx.view.profile.budget().spring_physics,
            &ui.visuals().text_cursor,
        );
        if !ctx.input(|input| input.focused) {
            // The window is in the background: a lit caret, no frames.
            self.motion.blink = None;
        }
        let mut output = EditorOutput {
            pages: self.laid.pages,
            ..EditorOutput::default()
        };
        if self.want_focus {
            ctx.memory_mut(|memory| memory.request_focus(self.id));
            self.want_focus = false;
        }
        // A finger's drag is the editor's to read (a scroll, or a
        // selection after a long press), never egui's as well.
        let scroll = egui::ScrollArea::both()
            .id_salt(self.id.with("scroll"))
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Never,
                ..egui::scroll_area::ScrollSource::default()
            })
            .auto_shrink([false, false]);
        scroll.show_viewport(ui, |ui, _viewport| {
            self.frame(ui, cx, view, &mut output);
        });
        output.focused = self.has_focus(&ctx);
        output.page = self.page_of(self.caret);
        output
    }

    fn prepare(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        docs: &[EntityId],
        view: &EditorView<'_>,
    ) {
        if !view.read_only {
            for id in docs {
                edit::ensure_final_newline(cx, *id);
            }
        }
        self.docs = docs.to_vec();
        self.texts = docs
            .iter()
            .map(|id| edit::text_of(cx.arena(), *id).unwrap_or("").to_owned())
            .collect();
        self.marks = docs
            .iter()
            .map(|id| edit::marks_of(cx.arena(), *id))
            .collect();
        let scale = view.zoom.clamp(0.1, 8.0) * PX_PER_PT;
        let width = match view.mode {
            ViewMode::Page => view.page.text_width(),
            ViewMode::Draft { width } => width,
        };
        let fonts = ctx.fonts(|fonts| fonts.definitions().font_data.len());
        let sheet = view.sheet.fingerprint();
        let ink = view.ink.text.to_array();
        let mut blocks = Vec::new();
        let mut starts = Vec::with_capacity(docs.len());
        let figure_size = |id: EntityId| (view.figures)(id).map(|(_, size)| size);
        let arena = cx.arena();
        let tables = |id: EntityId| TableSource::read(arena, id);
        let contents = |id: EntityId| {
            arena
                .get_by_id(id)
                .ok()
                .and_then(sue_model::contents::Contents::read)
        };
        for (index, id) in docs.iter().enumerate() {
            let revision = arena.get_by_id(*id).map_or(0, sue_core::Entity::revision);
            let embeds = self.marks[index]
                .marks()
                .iter()
                .filter(|mark| mark.kind == kind::EMBED)
                .filter_map(|mark| match mark.value {
                    Value::Entity(embedded) => arena.get_by_id(embedded).ok(),
                    _ => None,
                })
                .filter(|entity| entity.class() == sue_model::TEXT_TABLE)
                .fold(0_u64, |hash, entity| {
                    hash.rotate_left(7) ^ entity.revision()
                });
            let key = DocKey {
                revision,
                sheet,
                width: width.to_bits(),
                scale: scale.to_bits(),
                fonts,
                ink,
                embeds,
            };
            let fresh = self.cache.get(id).is_some_and(|cached| cached.key == key);
            if !fresh {
                let setting = Setting {
                    sheet: view.sheet,
                    scale,
                    width,
                    ink: view.ink,
                };
                let source = Source {
                    text: &self.texts[index],
                    marks: &self.marks[index],
                    figure_size: &figure_size,
                    tables: &tables,
                    contents: &contents,
                };
                let laid = lay_document(ctx, index, &source, &setting);
                self.cache.insert(*id, Cached { key, blocks: laid });
            }
            starts.push(blocks.len());
            if let Some(cached) = self.cache.get(id) {
                blocks.extend(cached.blocks.iter().cloned().map(|mut block| {
                    block.doc = index;
                    block
                }));
            }
        }
        self.cache.retain(|id, _| docs.contains(id));
        self.laid.blocks = blocks;
        self.laid.doc_starts = starts;
        let gap = if docs.len() > 1 && matches!(view.mode, ViewMode::Draft { .. }) {
            40.0
        } else {
            0.0
        };
        paginate(&mut self.laid, view.mode, view.page, scale, gap);
        if self
            .laid
            .blocks
            .iter()
            .any(|block| block.contents.is_some())
        {
            // Tables of contents list the headings with their pages: laid
            // out once to find the pages, then again with them.
            let setting = Setting {
                sheet: view.sheet,
                scale,
                width,
                ink: view.ink,
            };
            for pass in 0..2 {
                let pages = (pass > 0).then(|| {
                    let mut pages = vec![0; self.laid.blocks.len()];
                    for piece in self.laid.pieces.iter().rev() {
                        pages[piece.block] = piece.page;
                    }
                    pages
                });
                crate::contents::fill(
                    ctx,
                    &mut self.laid.blocks,
                    &self.texts,
                    &setting,
                    pages.as_deref(),
                );
                paginate(&mut self.laid, view.mode, view.page, scale, gap);
            }
        }
        self.pieces_of = vec![0..0; self.laid.blocks.len()];
        for (index, piece) in self.laid.pieces.iter().enumerate() {
            let range = &mut self.pieces_of[piece.block];
            if range.start == range.end {
                *range = index..index + 1;
            } else {
                range.end = index + 1;
            }
        }
        self.caret = self.clamp(self.caret);
        self.anchor = self.clamp(self.anchor);
        self.keep_cell(cx);
    }

    /// Keeps the cell caret in a cell of a table still shown, or lets it go.
    fn keep_cell(&mut self, cx: &mut AppletContext<'_>) {
        let Some(cell) = &mut self.cell else {
            return;
        };
        let table = self
            .laid
            .blocks
            .iter()
            .filter(|block| block.doc == cell.doc)
            .find_map(|block| {
                block
                    .table
                    .as_ref()
                    .filter(|table| table.entity == cell.table)
            });
        match table {
            Some(table) if !table.rows.is_empty() && table.columns() > 0 => {
                cell.row = cell.row.min(table.rows.len() - 1);
                cell.column = cell.column.min(table.columns() - 1);
                let text = &table.texts[cell.row][cell.column];
                cell.offset = cell.offset.min(text.len());
                while !text.is_char_boundary(cell.offset) {
                    cell.offset -= 1;
                }
            }
            _ => {
                if let Some(mut gone) = self.cell.take() {
                    gone.commit(cx);
                }
            }
        }
    }

    /// The table block holding `table` in shown document `doc`.
    fn table_block(&self, doc: usize, table: EntityId) -> Option<usize> {
        self.laid.blocks.iter().position(|block| {
            block.doc == doc
                && block
                    .table
                    .as_ref()
                    .is_some_and(|laid| laid.entity == table)
        })
    }

    /// Where row `row` of table block `index` is drawn: the top left of the
    /// table for the piece showing that row.
    fn table_origin(&self, index: usize, row: usize) -> Option<Pos2> {
        let block = &self.laid.blocks[index];
        let pieces = self.pieces_of.get(index)?.clone();
        let piece = pieces
            .clone()
            .find(|piece| self.laid.pieces[*piece].rows.contains(&row))
            .or(pieces.clone().next())?;
        let (galley, _) = self.piece_place(piece);
        let left = self.text_origin(self.laid.pieces[piece].page).x + block.indent;
        Some(pos2(left, galley.y))
    }

    /// The caret's rectangle on screen, in a cell or in the text.
    fn shown_caret(&self) -> Option<Rect> {
        match &self.cell {
            Some(cell) => {
                let index = self.table_block(cell.doc, cell.table)?;
                let table = self.laid.blocks[index].table.as_ref()?;
                let origin = self.table_origin(index, cell.row)?;
                let rect = table.caret(cell.row, cell.column, cell.offset)?;
                Some(rect.translate(origin.to_vec2()))
            }
            None => self.caret_rect(self.caret),
        }
    }

    /// `true` if `word` is spelled right for `view` (its dictionary and the
    /// words ignored this sitting).
    fn spelled_right(&self, view: &EditorView<'_>, word: &str) -> bool {
        let Some(also) = view.spelling else {
            return true;
        };
        crate::spell::is_correct(word, &|lower| also(lower) || self.ignored.contains(lower))
    }

    /// The menu a secondary press on a misspelled word opens: what it may
    /// have meant, Ignore All and Add to Dictionary. Returns `true` if it
    /// changed the text.
    fn spelling_menu(
        &mut self,
        ui: &Ui,
        cx: &mut AppletContext<'_>,
        response: &egui::Response,
        view: &EditorView<'_>,
        output: &mut EditorOutput,
    ) -> bool {
        if view.spelling.is_none() {
            return false;
        }
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let caret = self.hit(pos);
            let text = self.doc_text(caret.doc).to_owned();
            self.spell_menu = crate::spell::words(&text)
                .into_iter()
                .find(|range| range.start <= caret.offset && caret.offset <= range.end)
                .filter(|range| !self.spelled_right(view, &text[range.clone()]))
                .map(|range| {
                    let suggestions = crate::spell::suggest(&text[range.clone()], 6);
                    (caret.doc, range, suggestions)
                });
        }
        let Some((doc, range, suggestions)) = self.spell_menu.clone() else {
            return false;
        };
        let word = self
            .doc_text(doc)
            .get(range.clone())
            .unwrap_or("")
            .to_owned();
        let mut pick: Option<String> = None;
        let mut learn = false;
        let mut ignore = false;
        response.context_menu(|ui| {
            if suggestions.is_empty() {
                ui.label(tr!("no-suggestions"));
            }
            for suggestion in &suggestions {
                if ui.button(suggestion).clicked() {
                    pick = Some(suggestion.clone());
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr!("ignore-all")).clicked() {
                ignore = true;
                ui.close();
            }
            if ui.button(tr!("add-to-dictionary")).clicked() {
                learn = true;
                ui.close();
            }
        });
        let lower = word.to_lowercase().replace('\u{2019}', "'");
        if ignore {
            self.ignored.insert(lower.clone());
            self.spell_menu = None;
        }
        if learn {
            output.learn = Some(lower);
            self.spell_menu = None;
        }
        let Some(pick) = pick else {
            let _ = ui;
            return false;
        };
        self.spell_menu = None;
        self.anchor = Caret {
            doc,
            offset: range.start,
        };
        self.caret = Caret {
            doc,
            offset: range.end,
        };
        self.replace_selection(cx, &pick, None)
    }

    /// The heading a table of contents' entry under a point on screen
    /// leads to, if there is one.
    fn hit_entry(&self, pos: Pos2) -> Option<Caret> {
        for (index, piece) in self.laid.pieces.iter().enumerate() {
            let block = &self.laid.blocks[piece.block];
            let Some(listing) = block
                .table
                .as_ref()
                .and_then(|table| table.listing.as_ref().map(|listing| (table, listing)))
            else {
                continue;
            };
            let (table, listing) = listing;
            let (galley, clip) = self.piece_place(index);
            if !clip.contains(pos) {
                continue;
            }
            let y = pos.y - galley.y;
            let row = table
                .rows
                .iter()
                .position(|(top, bottom)| y >= *top && y < *bottom)?;
            let target = (*listing.targets.get(row)?)?;
            let heading = self.laid.blocks.get(target)?;
            return Some(Caret {
                doc: heading.doc,
                offset: heading.range.start,
            });
        }
        None
    }

    /// The table cell under a point on screen, if any.
    fn hit_cell(&self, pos: Pos2) -> Option<CellCaret> {
        for (index, piece) in self.laid.pieces.iter().enumerate() {
            let block = &self.laid.blocks[piece.block];
            let Some(table) = block.table.as_ref().filter(|table| table.listing.is_none()) else {
                continue;
            };
            let (galley, clip) = self.piece_place(index);
            let repeat = if piece.rows.start >= table.header_rows.max(1) {
                table.repeat_height()
            } else {
                0.0
            };
            if pos.y < clip.top() - repeat || pos.y > clip.bottom() {
                continue;
            }
            let left = self.text_origin(piece.page).x + block.indent;
            let origin = pos2(left, galley.y);
            let mut local = pos - origin;
            if pos.y < clip.top() {
                // In the heading rows repeated above this page's rows.
                local.y = pos.y - (clip.top() - repeat) + table.rows[0].0;
            }
            if let Some((row, column, offset)) = table.hit(local) {
                return Some(CellCaret::new(block.doc, table.entity, row, column, offset));
            }
        }
        None
    }

    /// Records the typing in the cell the caret is in as one undo step; a
    /// studio calls it before undo, redo and its own edits of the table.
    pub fn commit(&mut self, cx: &mut AppletContext<'_>) {
        if let Some(mut left) = self.left_cell.take() {
            left.commit(cx);
        }
        if let Some(cell) = &mut self.cell {
            cell.commit(cx);
        }
    }

    /// The table and the cell (row, column) the caret is in.
    #[must_use]
    pub fn cell(&self) -> Option<(EntityId, usize, usize)> {
        self.cell
            .as_ref()
            .map(|cell| (cell.table, cell.row, cell.column))
    }

    /// Puts the caret in cell `(row, column)` of table `table` in the
    /// current document.
    pub fn enter_cell(&mut self, table: EntityId, row: usize, column: usize) {
        if self.left_cell.is_none() {
            self.left_cell = self.cell.take();
        }
        self.cell = Some(CellCaret::new(self.caret.doc, table, row, column, 0));
        self.follow = true;
    }

    /// Leaves the table the caret is in, for the paragraph after it (or
    /// before it, with `up`).
    pub fn leave_table(&mut self, up: bool) {
        let Some(cell) = self.cell.take() else {
            return;
        };
        let doc = cell.doc;
        if let Some(index) = self.table_block(doc, cell.table) {
            let range = self.laid.blocks[index].range.clone();
            let text = self.doc_text(doc);
            let offset = if up {
                range.start.saturating_sub(1)
            } else if range.end < self.last_offset(doc) {
                range.end + usize::from(text.as_bytes().get(range.end) == Some(&b'\n'))
            } else {
                range.end
            };
            self.set_caret(Caret { doc, offset }, false);
        }
        if self.left_cell.is_none() {
            self.left_cell = Some(cell);
        }
    }

    /// Keys and text while the caret is in a cell. Returns `true` if the
    /// table changed.
    fn cell_keyboard(&mut self, ui: &Ui, cx: &mut AppletContext<'_>) -> bool {
        let events = ui.input(|input| input.events.clone());
        let mut changed = false;
        for event in events {
            let Some(cell) = self.cell.as_mut() else {
                break;
            };
            let at = cell.offset;
            match event {
                Event::Text(text) if !text.chars().all(char::is_control) => {
                    changed |= cell.replace(cx, at..at, &text);
                }
                Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    self.preedit = (!text.is_empty()).then_some(text);
                }
                Event::Ime(egui::ImeEvent::Commit(text)) => {
                    self.preedit = None;
                    changed |= cell.replace(cx, at..at, &text);
                }
                Event::Paste(text) => {
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    changed |= cell.replace(cx, at..at, text.trim_end_matches('\n'));
                }
                Event::Copy | Event::Cut => {
                    ui.ctx().copy_text(cell.text(cx.arena()));
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => changed |= self.cell_key(cx, key, modifiers),
                _ => {}
            }
        }
        if changed {
            self.follow = true;
        }
        changed
    }

    /// One key in a cell. Returns `true` if the table changed.
    fn cell_key(&mut self, cx: &mut AppletContext<'_>, key: Key, modifiers: Modifiers) -> bool {
        let Some(cell) = self.cell.as_mut() else {
            return false;
        };
        let text = cell.text(cx.arena());
        let (rows, columns) = crate::tables::size_of(cx.arena(), cell.table);
        let columns = columns.max(1);
        let at = cell.offset.min(text.len());
        let place = cell.row * columns + cell.column;
        match key {
            Key::Backspace if at > 0 => {
                let from = prev_boundary(&text, at);
                cell.replace(cx, from..at, "")
            }
            Key::Delete if at < text.len() => {
                let to = next_boundary(&text, at);
                cell.replace(cx, at..to, "")
            }
            Key::Enter => cell.replace(cx, at..at, "\n"),
            Key::ArrowLeft if at > 0 => {
                cell.offset = prev_boundary(&text, at);
                false
            }
            Key::ArrowRight if at < text.len() => {
                cell.offset = next_boundary(&text, at);
                false
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let forward = key == Key::ArrowRight;
                if forward && place + 1 < rows * columns {
                    cell.move_to(cx, (place + 1) / columns, (place + 1) % columns, false);
                } else if !forward && place > 0 {
                    cell.move_to(cx, (place - 1) / columns, (place - 1) % columns, true);
                } else {
                    self.leave_table(!forward);
                }
                false
            }
            Key::Home => {
                cell.offset = 0;
                false
            }
            Key::End => {
                cell.offset = text.len();
                false
            }
            Key::Tab if modifiers.shift => {
                if place > 0 {
                    cell.move_to(cx, (place - 1) / columns, (place - 1) % columns, true);
                }
                false
            }
            Key::Tab if place + 1 < rows * columns => {
                cell.move_to(cx, (place + 1) / columns, (place + 1) % columns, true);
                false
            }
            Key::Tab => {
                // Tab in the last cell adds a row, as in every word processor.
                cell.commit(cx);
                let table = cell.table;
                let grown = cx
                    .apply(ArenaEdit(sue_model::Reshape::insert_rows(
                        table,
                        rows as u32,
                        1,
                        Some(Value::Text(String::new())),
                    )))
                    .is_ok();
                if grown && let Some(cell) = self.cell.as_mut() {
                    cell.row = rows;
                    cell.column = 0;
                    cell.offset = 0;
                }
                grown
            }
            Key::ArrowUp | Key::ArrowDown => {
                let down = key == Key::ArrowDown;
                if down && cell.row + 1 < rows {
                    let (row, column) = (cell.row + 1, cell.column);
                    cell.move_to(cx, row, column, false);
                } else if !down && cell.row > 0 {
                    let (row, column) = (cell.row - 1, cell.column);
                    cell.move_to(cx, row, column, false);
                } else {
                    self.leave_table(!down);
                }
                false
            }
            Key::Escape => {
                self.leave_table(false);
                false
            }
            _ => false,
        }
    }

    /// Keeps a place inside the shown documents, before their last newline.
    fn clamp(&self, caret: Caret) -> Caret {
        let doc = caret.doc.min(self.texts.len().saturating_sub(1));
        let text = self.texts.get(doc).map_or("", String::as_str);
        let last = text.len().saturating_sub(usize::from(text.ends_with('\n')));
        let mut offset = caret.offset.min(last);
        while offset > 0 && !text.is_char_boundary(offset) {
            offset -= 1;
        }
        Caret { doc, offset }
    }

    fn frame(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        view: &EditorView<'_>,
        output: &mut EditorOutput,
    ) {
        let paged = matches!(view.mode, ViewMode::Page);
        self.gap = view.gap.unwrap_or(if paged { 24.0 } else { 32.0 }).max(0.0);
        let available = ui.available_size();
        let width = self.laid.page.x + self.gap * 2.0;
        let height = if paged {
            self.laid.pages as f32 * (self.laid.page.y + self.gap) + self.gap
        } else {
            self.laid.text_height + self.gap * 2.0 + available.y * 0.5
        };
        let size = vec2(width.max(available.x), height.max(available.y));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        self.origin = rect.min;
        self.page_x = ((rect.width() - self.laid.page.x) / 2.0).max(self.gap);
        // When the view changes size under the caret (a phone's keyboard
        // comes up and the window shrinks above it), the caret is shown
        // again, so the line being written is never hidden.
        let resized = (ui.clip_rect().size() - self.viewport.size()).length() > 1.0;
        self.viewport = ui.clip_rect();
        // A finger's drag is left to the scroll area around the page,
        // unless a long press began a selection.
        let (touching, pressed) =
            ui.input(|input| (input.any_touches(), input.pointer.any_pressed()));
        if pressed {
            self.touch = touching;
        }
        let sense = if self.touch {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let response = ui.interact(rect, self.id, sense);
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.hold) && self.had_focus {
            ctx.memory_mut(|memory| memory.request_focus(self.id));
        }

        // Ctrl and the wheel zoom.
        if response.hovered() {
            let zoom = ui.input(|input| input.zoom_delta());
            if (zoom - 1.0).abs() > 1e-3 {
                output.zoom = Some(zoom);
            }
        }
        self.pointer(ui, &response, output);
        if let Some(mut left) = self.left_cell.take() {
            left.commit(cx);
        }
        if !view.read_only && self.spelling_menu(ui, cx, &response, view, output) {
            output.changed = true;
            let docs = self.docs.clone();
            self.prepare(&ctx, cx, &docs, view);
        }
        let focused = ctx.memory(|memory| memory.has_focus(self.id));
        if focused && resized && self.had_focus {
            self.follow = true;
        }
        self.had_focus = focused;
        if focused {
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    self.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
            // The shell and the studios treat Sue's text fields as typing.
            egui::text_edit::TextEditState::default().store(&ctx, self.id);
            let typed = !view.read_only
                && if self.cell.is_some() {
                    self.cell_keyboard(ui, cx)
                } else {
                    self.keyboard(ui, cx, view)
                };
            if let Some(mut left) = self.left_cell.take() {
                left.commit(cx);
            }
            if typed {
                output.changed = true;
                let docs = self.docs.clone();
                self.prepare(&ctx, cx, &docs, view);
            }
        }
        let caret = self.caret_look(ui, view, focused, response.dragged());
        self.paint(ui, view, focused, caret);
        if let Some(target) = self.scroll_target.take() {
            self.caret = self.clamp(target);
            self.follow = true;
        }
        if self.follow
            && let Some(caret) = self.shown_caret()
        {
            let align = view.typewriter.then_some(egui::Align::Center);
            ui.scroll_to_rect(caret.expand2(vec2(48.0, 36.0)), align);
            self.follow = false;
        }
        if focused && let Some(caret) = self.shown_caret() {
            let to_global = ctx
                .layer_transform_to_global(ui.layer_id())
                .unwrap_or_default();
            ui.output_mut(|output| {
                output.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: to_global * self.viewport,
                    cursor_rect: to_global * caret,
                    should_interrupt_composition: false,
                });
            });
        }
    }

    // ----- Geometry ------------------------------------------------------------

    /// The top left of page `page`'s text area on screen.
    fn text_origin(&self, page: usize) -> Pos2 {
        let top = self.origin.y + self.gap + page as f32 * (self.laid.page.y + self.gap);
        pos2(self.origin.x + self.page_x, top) + self.laid.inset
    }

    fn page_rect(&self, page: usize) -> Rect {
        let top = self.origin.y + self.gap + page as f32 * (self.laid.page.y + self.gap);
        Rect::from_min_size(pos2(self.origin.x + self.page_x, top), self.laid.page)
    }

    /// Where piece `index`'s galley is drawn, and the rows it shows.
    fn piece_place(&self, index: usize) -> (Pos2, Rect) {
        let piece = &self.laid.pieces[index];
        let block = &self.laid.blocks[piece.block];
        let text = self.text_origin(piece.page);
        let top = text.y + piece.top;
        let rows = &block.rows[piece.rows.clone()];
        let height =
            rows.last().map_or(0.0, |last| last.1) - rows.first().map_or(0.0, |first| first.0);
        let galley = pos2(text.x + block.origin, top - piece.shift);
        let clip = Rect::from_min_max(
            pos2(text.x - self.laid.inset.x, top),
            pos2(
                text.x + self.laid.text_width + self.laid.inset.x,
                top + height,
            ),
        );
        (galley, clip)
    }

    /// The block holding `caret`.
    fn block_of(&self, caret: Caret) -> Option<usize> {
        let start = *self.laid.doc_starts.get(caret.doc)?;
        let end = self
            .laid
            .doc_starts
            .get(caret.doc + 1)
            .copied()
            .unwrap_or(self.laid.blocks.len());
        let blocks = &self.laid.blocks[start..end];
        let index = blocks
            .partition_point(|block| block.range.start <= caret.offset)
            .saturating_sub(1);
        (start + index < end).then_some(start + index)
    }

    fn cursor_in(&self, block: &Block, offset: usize, prefer_next_row: bool) -> CCursor {
        let text = &self.texts[block.doc];
        CCursor {
            index: CharIndex(char_of_byte(text, block, offset)),
            prefer_next_row,
        }
    }

    /// The caret's rectangle on screen.
    fn caret_rect(&self, caret: Caret) -> Option<Rect> {
        let index = self.block_of(caret)?;
        let block = &self.laid.blocks[index];
        let pieces = self.pieces_of.get(index)?.clone();
        if let Some((_, size)) = block.figure {
            let first = pieces.start;
            let (galley, _) = self.piece_place(first);
            let x = if caret.offset > block.range.start {
                galley.x + size.x
            } else {
                galley.x
            };
            let top = galley.y + self.laid.pieces[first].shift;
            return Some(Rect::from_min_max(
                pos2(x, top),
                pos2(x + 1.0, top + size.y),
            ));
        }
        let cursor = self.cursor_in(block, caret.offset, true);
        let row = block.galley.layout_from_cursor(cursor).row;
        let rect = block.galley.pos_from_cursor(cursor);
        let piece = pieces
            .clone()
            .find(|piece| self.laid.pieces[*piece].rows.contains(&row))
            .or(pieces.clone().last())?;
        let (galley, _) = self.piece_place(piece);
        let rect = rect.translate(galley.to_vec2());
        Some(Rect::from_min_max(
            rect.min,
            pos2(rect.min.x + 1.0, rect.max.y.max(rect.min.y + 4.0)),
        ))
    }

    fn page_of(&self, caret: Caret) -> usize {
        self.block_of(caret)
            .and_then(|index| self.pieces_of.get(index))
            .and_then(|pieces| self.laid.pieces.get(pieces.start))
            .map_or(0, |piece| piece.page)
    }

    /// The place under a point on screen.
    fn hit(&self, pos: Pos2) -> Caret {
        if self.laid.pieces.is_empty() {
            return Caret::default();
        }
        // The nearest piece by its rows' vertical span, then across.
        let mut best = 0;
        let mut distance = f32::INFINITY;
        for index in 0..self.laid.pieces.len() {
            let (_, clip) = self.piece_place(index);
            let dy = if pos.y < clip.top() {
                clip.top() - pos.y
            } else if pos.y > clip.bottom() {
                pos.y - clip.bottom()
            } else {
                0.0
            };
            if dy < distance {
                distance = dy;
                best = index;
            }
            if dy == 0.0 {
                break;
            }
        }
        let piece = &self.laid.pieces[best];
        let block = &self.laid.blocks[piece.block];
        let (galley, clip) = self.piece_place(best);
        let doc = block.doc;
        if let Some((_, size)) = block.figure {
            let offset = if pos.x > galley.x + size.x / 2.0 {
                block.range.end
            } else {
                block.range.start
            };
            return Caret { doc, offset };
        }
        let y = pos.y.clamp(clip.top() + 0.5, clip.bottom() - 0.5);
        let local = vec2(pos.x - galley.x, y - galley.y);
        let cursor = block.galley.cursor_from_pos(local);
        let offset = byte_of_char(&self.texts[doc], block, cursor.index.0);
        Caret { doc, offset }
    }

    // ----- Moving ----------------------------------------------------------------

    fn set_caret(&mut self, caret: Caret, extend: bool) {
        self.caret = self.clamp(caret);
        if !extend {
            self.anchor = self.caret;
        }
        self.follow = true;
        self.pending.clear();
    }

    fn doc_text(&self, doc: usize) -> &str {
        self.texts.get(doc).map_or("", String::as_str)
    }

    fn last_offset(&self, doc: usize) -> usize {
        let text = self.doc_text(doc);
        text.len().saturating_sub(usize::from(text.ends_with('\n')))
    }

    fn step_left(&self, caret: Caret, word: bool) -> Caret {
        let text = self.doc_text(caret.doc);
        if caret.offset == 0 {
            if caret.doc > 0 {
                return Caret {
                    doc: caret.doc - 1,
                    offset: self.last_offset(caret.doc - 1),
                };
            }
            return caret;
        }
        let offset = if word {
            word_start(text, caret.offset)
        } else {
            prev_boundary(text, caret.offset)
        };
        Caret {
            doc: caret.doc,
            offset,
        }
    }

    fn step_right(&self, caret: Caret, word: bool) -> Caret {
        let text = self.doc_text(caret.doc);
        if caret.offset >= self.last_offset(caret.doc) {
            if caret.doc + 1 < self.texts.len() {
                return Caret {
                    doc: caret.doc + 1,
                    offset: 0,
                };
            }
            return caret;
        }
        let offset = if word {
            word_end(text, caret.offset)
        } else {
            next_boundary(text, caret.offset)
        };
        Caret {
            doc: caret.doc,
            offset,
        }
    }

    /// One row up or down, keeping `h_pos`.
    fn step_vertical(&mut self, caret: Caret, down: bool, rows: usize) -> Caret {
        let mut caret = caret;
        let Some(rect) = self.caret_rect(caret) else {
            return caret;
        };
        let x = *self.h_pos.get_or_insert(rect.center().x);
        for _ in 0..rows {
            let Some(rect) = self.caret_rect(caret) else {
                break;
            };
            let probe = if down {
                pos2(x, rect.bottom() + 2.0)
            } else {
                pos2(x, rect.top() - 2.0)
            };
            let next = self.hit_row(probe, down, rect);
            if next == caret {
                break;
            }
            caret = next;
        }
        caret
    }

    /// The place on the first row strictly below (or above) `from`.
    fn hit_row(&self, probe: Pos2, down: bool, from: Rect) -> Caret {
        // Find the nearest row past `from` in reading order.
        let mut best: Option<(f32, Pos2)> = None;
        for index in 0..self.laid.pieces.len() {
            let piece = &self.laid.pieces[index];
            let block = &self.laid.blocks[piece.block];
            let (galley, _) = self.piece_place(index);
            for row in piece.rows.clone() {
                let (top, bottom) = block.rows[row];
                let (top, bottom) = (galley.y + top, galley.y + bottom);
                let middle = (top + bottom) / 2.0;
                let past = if down {
                    top >= from.bottom() - 1.0
                } else {
                    bottom <= from.top() + 1.0
                };
                if !past {
                    continue;
                }
                let distance = (middle - probe.y).abs();
                if best.is_none_or(|(known, _)| distance < known) {
                    best = Some((distance, pos2(probe.x, middle)));
                }
            }
        }
        best.map_or_else(
            || {
                if down {
                    let doc = self.texts.len().saturating_sub(1);
                    Caret {
                        doc,
                        offset: self.last_offset(doc),
                    }
                } else {
                    Caret::default()
                }
            },
            |(_, at)| self.hit(at),
        )
    }

    /// The start or end of the caret's row.
    fn row_edge(&self, caret: Caret, end: bool) -> Caret {
        let Some(index) = self.block_of(caret) else {
            return caret;
        };
        let block = &self.laid.blocks[index];
        let cursor = self.cursor_in(block, caret.offset, true);
        let edge = if end {
            block.galley.cursor_end_of_row(&cursor)
        } else {
            block.galley.cursor_begin_of_row(&cursor)
        };
        Caret {
            doc: caret.doc,
            offset: byte_of_char(&self.texts[caret.doc], block, edge.index.0),
        }
    }

    // ----- Input -------------------------------------------------------------------

    /// A finger on a touch screen moves the page with it, point for point
    /// and at once (egui's eased scrolling would lag it and lose some of
    /// the way), and a flick sends the page on as far as the finger's
    /// speed carries it.
    fn finger_scroll(&mut self, ui: &Ui, response: &egui::Response) {
        let (pressed, down, at, time) = ui.input(|input| {
            (
                input.pointer.any_pressed(),
                input.pointer.any_down(),
                input.pointer.latest_pos(),
                input.time,
            )
        });
        if pressed && response.hovered() {
            if let Some(at) = at {
                self.finger = Some((at, Vec::new()));
            }
            // A new touch stops a page still gliding from a flick.
            ui.scroll_with_delta_animation(Vec2::ZERO, egui::style::ScrollAnimation::none());
            return;
        }
        let Some((last, mut moves)) = self.finger.take() else {
            return;
        };
        if down {
            let Some(at) = at else {
                self.finger = Some((last, moves));
                return;
            };
            let delta = at - last;
            if delta != Vec2::ZERO {
                ui.scroll_with_delta_animation(delta, egui::style::ScrollAnimation::none());
                moves.push((time, delta));
                moves.retain(|(when, _)| time - when <= 0.1);
            }
            self.finger = Some((at, moves));
            return;
        }
        // Lifted: a flick is the last tenth of a second's moves, if the
        // finger was still moving when it left the glass.
        let moving = moves.last().is_some_and(|(when, _)| time - when < 0.06);
        let span = moves
            .first()
            .map_or(0.0, |(when, _)| (time - when) as f32)
            .max(1.0 / 60.0);
        let velocity = moves
            .iter()
            .fold(Vec2::ZERO, |sum, (_, delta)| sum + *delta)
            / span;
        if moving && velocity.length() > 250.0 {
            let glide = (velocity * 0.35).clamp(Vec2::splat(-4000.0), Vec2::splat(4000.0));
            ui.scroll_with_delta_animation(glide, egui::style::ScrollAnimation::duration(0.7));
        }
    }

    fn pointer(&mut self, ui: &Ui, response: &egui::Response, output: &mut EditorOutput) {
        if self.touch && !self.touch_select {
            self.finger_scroll(ui, response);
        }
        let Some(pos) = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|input| input.pointer.hover_pos()))
        else {
            return;
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        let (pressed, modifiers, time) =
            ui.input(|input| (input.pointer.primary_pressed(), input.modifiers, input.time));
        if self.touch {
            let down = ui.input(|input| input.pointer.any_down());
            if self.touch_select {
                // The finger moves the selection's end; lifting it ends
                // the selection (it is no tap).
                if down {
                    let mut caret = self.hit(pos);
                    if caret.doc != self.anchor.doc {
                        caret = self.anchor;
                    }
                    self.caret = self.clamp(caret);
                } else {
                    self.touch_select = false;
                }
                return;
            }
            if response.long_touched() {
                // A long press selects the word under the finger; dragging
                // on grows the selection from it. The keyboard stays down,
                // so the page does not move under the finger.
                self.cell = None;
                self.finger = None;
                let caret = self.hit(pos);
                let text = self.doc_text(caret.doc);
                let (start, end) = word_around(text, caret.offset);
                self.anchor = Caret {
                    doc: caret.doc,
                    offset: start,
                };
                self.caret = Caret {
                    doc: caret.doc,
                    offset: end,
                };
                self.clicks = (time, pos, 1);
                self.touch_select = true;
                self.h_pos = None;
                self.pending.clear();
                return;
            }
        }
        // A finger acts when it lifts without having dragged (the drag was
        // a scroll); a mouse as its button goes down.
        let pressed = if self.touch {
            response.clicked()
        } else {
            pressed && response.is_pointer_button_down_on()
        };
        if pressed {
            ui.ctx().memory_mut(|memory| memory.request_focus(self.id));
            let (last, at, count) = self.clicks;
            let count = if time - last < 0.45 && at.distance(pos) < 6.0 {
                count % 3 + 1
            } else {
                1
            };
            self.clicks = (time, pos, count);
            if let Some(hit) = self.hit_cell(pos) {
                // A press in a table puts the caret in that cell.
                let same = self.cell.as_ref().is_some_and(|old| {
                    (old.table, old.row, old.column) == (hit.table, hit.row, hit.column)
                });
                if same {
                    if let Some(cell) = self.cell.as_mut() {
                        cell.offset = hit.offset;
                    }
                } else {
                    if self.left_cell.is_none() {
                        self.left_cell = self.cell.take();
                    }
                    self.cell = Some(hit);
                }
                self.h_pos = None;
                self.follow = true;
                self.pending.clear();
                return;
            }
            if self.cell.is_some() && self.left_cell.is_none() {
                self.left_cell = self.cell.take();
            }
            self.cell = None;
            if let Some(heading) = self.hit_entry(pos) {
                // A press on a table of contents' entry goes to its heading.
                self.set_caret(heading, false);
                self.pending.clear();
                return;
            }
            let caret = self.hit(pos);
            self.h_pos = None;
            match count {
                2 => {
                    let text = self.doc_text(caret.doc);
                    let (start, end) = word_around(text, caret.offset);
                    self.anchor = Caret {
                        doc: caret.doc,
                        offset: start,
                    };
                    self.caret = Caret {
                        doc: caret.doc,
                        offset: end,
                    };
                }
                3 => {
                    let text = self.doc_text(caret.doc);
                    let index = sue_model::paragraph_at(text, caret.offset);
                    if let Some(range) = sue_model::paragraphs(text).get(index) {
                        self.anchor = Caret {
                            doc: caret.doc,
                            offset: range.start,
                        };
                        self.caret = self.clamp(Caret {
                            doc: caret.doc,
                            offset: range.end.saturating_sub(1),
                        });
                    }
                }
                _ => {
                    if modifiers.command
                        && let Some(link) = self.link_at(caret)
                    {
                        output.link = Some(link);
                    }
                    self.set_caret(caret, modifiers.shift);
                }
            }
            self.pending.clear();
        } else if response.dragged() && self.clicks.2 == 1 && self.cell.is_none() {
            let mut caret = self.hit(pos);
            // Selections stay in one document.
            if caret.doc != self.anchor.doc {
                caret = if caret.doc > self.anchor.doc {
                    Caret {
                        doc: self.anchor.doc,
                        offset: self.last_offset(self.anchor.doc),
                    }
                } else {
                    Caret {
                        doc: self.anchor.doc,
                        offset: 0,
                    }
                };
            }
            self.caret = self.clamp(caret);
            // Dragging past the edge scrolls.
            let view = self.viewport;
            let speed = if pos.y < view.top() + 20.0 {
                (view.top() + 20.0 - pos.y).min(60.0)
            } else if pos.y > view.bottom() - 20.0 {
                -(pos.y - view.bottom() + 20.0).min(60.0)
            } else {
                0.0
            };
            if speed != 0.0 {
                ui.scroll_with_delta(vec2(0.0, speed * 0.5));
                ui.ctx().request_repaint();
            }
        }
    }

    fn link_at(&self, caret: Caret) -> Option<String> {
        let marks = self.marks.get(caret.doc)?;
        marks
            .value_at(caret.offset, kind::LINK)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    /// Takes the frame's keys and text. Returns `true` if the text changed.
    fn keyboard(&mut self, ui: &Ui, cx: &mut AppletContext<'_>, view: &EditorView<'_>) -> bool {
        let events = ui.input(|input| input.events.clone());
        let mut changed = false;
        for event in events {
            match event {
                Event::Text(text) => {
                    if text.chars().all(char::is_control) {
                        continue;
                    }
                    let text = self.typed(&text, view);
                    changed |= self.replace_selection(cx, &text, None);
                }
                Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    self.preedit = (!text.is_empty()).then_some(text);
                }
                Event::Ime(egui::ImeEvent::Commit(text)) => {
                    self.preedit = None;
                    if !text.is_empty() {
                        changed |= self.replace_selection(cx, &text, None);
                    }
                }
                Event::Copy => self.copy(ui),
                Event::Cut => {
                    self.copy(ui);
                    changed |= self.replace_selection(cx, "", None);
                }
                Event::Paste(text) => {
                    changed |= self.paste(cx, &text);
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    changed |= self.key(cx, key, modifiers, view);
                }
                _ => {}
            }
        }
        changed
    }

    /// What typing `text` puts in: capitals in a capitals style, curly
    /// quotes if asked for.
    fn typed(&self, text: &str, view: &EditorView<'_>) -> String {
        let (look, _) = self.look_here(view.sheet);
        let mut out = if look.caps {
            text.to_uppercase()
        } else {
            text.to_owned()
        };
        if view.smart_quotes {
            let before = self.doc_text(self.caret.doc)[..self.selection().start]
                .chars()
                .next_back();
            let opening =
                before.is_none_or(|ch| ch.is_whitespace() || "([{\u{2018}\u{201C}".contains(ch));
            out = out
                .chars()
                .map(|ch| match ch {
                    '"' if opening => '\u{201C}',
                    '"' => '\u{201D}',
                    '\'' if opening => '\u{2018}',
                    '\'' => '\u{2019}',
                    other => other,
                })
                .collect();
        }
        out
    }

    /// Replaces the selection with `text` (deleting it when empty).
    fn replace_selection(
        &mut self,
        cx: &mut AppletContext<'_>,
        text: &str,
        source: Option<&TextMarks>,
    ) -> bool {
        let Some(doc) = self.current() else {
            return false;
        };
        let range = self.selection();
        if range.is_empty() && text.is_empty() {
            return false;
        }
        if let Some(change) = self.tracking.clone() {
            return self.replace_tracked(cx, doc, range, text, source, &change);
        }
        let format = std::mem::take(&mut self.pending);
        // Text typed while changes are not recorded is no one's insertion.
        let mut applied = format.clone();
        applied.push((kind::INSERTED.to_owned(), None));
        let Some(end) = edit::replace(cx, doc, range, text, source, &applied) else {
            return false;
        };
        if text.is_empty() {
            self.pending = format;
        }
        let caret = Caret {
            doc: self.caret.doc,
            offset: end,
        };
        self.caret = caret;
        self.anchor = caret;
        self.h_pos = None;
        self.follow = true;
        self.sync_text(cx);
        true
    }

    /// Records changes as `change` (who and when) while it is set: typing
    /// marks what it puts in as inserted, deleting strikes text through
    /// instead of taking it out (text a change inserted goes for good).
    pub fn set_tracking(&mut self, change: Option<sue_model::changes::Change>) {
        self.tracking = change;
    }

    /// `true` while changes are recorded.
    #[must_use]
    pub fn tracking(&self) -> bool {
        self.tracking.is_some()
    }

    /// Every tracked change in the shown documents, in order, with the
    /// document it is in (an index into the shown documents).
    #[must_use]
    pub fn changes(&self) -> Vec<(usize, sue_model::changes::Tracked)> {
        self.marks
            .iter()
            .enumerate()
            .flat_map(|(doc, marks)| {
                sue_model::changes::changes(marks)
                    .into_iter()
                    .map(move |tracked| (doc, tracked))
            })
            .collect()
    }

    /// [`Self::replace_selection`] while changes are recorded.
    fn replace_tracked(
        &mut self,
        cx: &mut AppletContext<'_>,
        doc: EntityId,
        range: Range<usize>,
        text: &str,
        source: Option<&TextMarks>,
        change: &sue_model::changes::Change,
    ) -> bool {
        let forward = std::mem::take(&mut self.delete_forward);
        let format = std::mem::take(&mut self.pending);
        let mut at = range.start;
        let mut caret = range.start;
        if !range.is_empty() {
            let marks = self.marks[self.caret.doc].clone();
            // What a change inserted goes; the rest is struck through.
            let mut inserted: Vec<Range<usize>> = marks
                .marks()
                .iter()
                .filter(|mark| mark.kind == kind::INSERTED)
                .map(|mark| mark.range.start.max(range.start)..mark.range.end.min(range.end))
                .filter(|part| !part.is_empty())
                .collect();
            inserted.sort_by_key(|part| part.start);
            let mut kept = Vec::new();
            let mut from = range.start;
            for part in &inserted {
                if part.start > from {
                    kept.push(from..part.start);
                }
                from = from.max(part.end);
            }
            if from < range.end {
                kept.push(from..range.end);
            }
            if !kept.is_empty() {
                let value = change.value();
                edit::reformat(cx, doc, &tr!("delete"), |_, marks| {
                    for part in kept {
                        marks.set(part, kind::DELETED, Some(value.clone()));
                    }
                });
            }
            let mut removed = 0;
            for part in inserted.iter().rev() {
                if edit::replace(cx, doc, part.clone(), "", None, &[]).is_some() {
                    removed += part.len();
                }
            }
            at = range.end - removed;
            caret = if forward { at } else { range.start };
        }
        if text.is_empty() {
            self.pending = format;
        } else {
            let mut applied = format;
            applied.push((kind::INSERTED.to_owned(), Some(change.value())));
            applied.push((kind::DELETED.to_owned(), None));
            match edit::replace(cx, doc, at..at, text, source, &applied) {
                Some(end) => caret = end,
                None => return false,
            }
        }
        let caret = Caret {
            doc: self.caret.doc,
            offset: caret,
        };
        self.caret = caret;
        self.anchor = caret;
        self.h_pos = None;
        self.follow = true;
        self.sync_text(cx);
        true
    }

    /// Reads the caret's document back after an edit.
    fn sync_text(&mut self, cx: &AppletContext<'_>) {
        let doc = self.caret.doc;
        if let Some(id) = self.docs.get(doc) {
            if let Some(text) = edit::text_of(cx.arena(), *id) {
                self.texts[doc] = text.to_owned();
            }
            self.marks[doc] = edit::marks_of(cx.arena(), *id);
        }
    }

    fn copy(&self, ui: &Ui) {
        let range = self.selection();
        if range.is_empty() {
            return;
        }
        let text = self.selected_text().to_owned();
        let marks = self
            .marks
            .get(self.caret.doc)
            .map(|marks| marks.slice(range))
            .unwrap_or_default();
        ui.ctx().copy_text(text.clone());
        if let Ok(mut clip) = clipboard().lock() {
            *clip = Some((text, marks));
        }
    }

    fn paste(&mut self, cx: &mut AppletContext<'_>, text: &str) -> bool {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return false;
        }
        let rich = clipboard()
            .lock()
            .ok()
            .and_then(|clip| clip.clone())
            .filter(|(copied, _)| *copied == text);
        match rich {
            Some((_, marks)) => self.replace_selection(cx, &text, Some(&marks)),
            None => self.replace_selection(cx, &text, None),
        }
    }

    #[allow(clippy::too_many_lines, reason = "one match over the editing keys")]
    fn key(
        &mut self,
        cx: &mut AppletContext<'_>,
        key: Key,
        modifiers: Modifiers,
        view: &EditorView<'_>,
    ) -> bool {
        let word = modifiers.ctrl || modifiers.alt || modifiers.mac_cmd;
        let shift = modifiers.shift;
        let selection = self.selection();
        match key {
            Key::ArrowLeft | Key::ArrowRight => {
                let right = key == Key::ArrowRight;
                let caret = if !shift && !selection.is_empty() && !word {
                    Caret {
                        doc: self.caret.doc,
                        offset: if right {
                            selection.end
                        } else {
                            selection.start
                        },
                    }
                } else if right {
                    self.step_right(self.caret, word)
                } else {
                    self.step_left(self.caret, word)
                };
                self.h_pos = None;
                self.set_caret(caret, shift);
                false
            }
            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                let down = matches!(key, Key::ArrowDown | Key::PageDown);
                let rows = if matches!(key, Key::PageUp | Key::PageDown) {
                    let row = self
                        .caret_rect(self.caret)
                        .map_or(20.0, |rect| rect.height().max(8.0));
                    ((self.viewport.height() / row) as usize)
                        .saturating_sub(1)
                        .max(1)
                } else {
                    1
                };
                let h_pos = self.h_pos;
                let caret = self.step_vertical(self.caret, down, rows);
                self.set_caret(caret, shift);
                self.h_pos = h_pos.or(self.h_pos);
                false
            }
            Key::Home | Key::End => {
                let end = key == Key::End;
                let caret = if modifiers.command {
                    if end {
                        let doc = self.texts.len().saturating_sub(1);
                        Caret {
                            doc,
                            offset: self.last_offset(doc),
                        }
                    } else {
                        Caret::default()
                    }
                } else {
                    self.row_edge(self.caret, end)
                };
                self.h_pos = None;
                self.set_caret(caret, shift);
                false
            }
            Key::A if modifiers.command => {
                self.anchor = Caret {
                    doc: self.caret.doc,
                    offset: 0,
                };
                self.caret = Caret {
                    doc: self.caret.doc,
                    offset: self.last_offset(self.caret.doc),
                };
                false
            }
            Key::Backspace | Key::Delete => {
                if !selection.is_empty() {
                    return self.replace_selection(cx, "", None);
                }
                let forward = key == Key::Delete;
                self.delete_forward = forward;
                let text = self.doc_text(self.caret.doc).to_owned();
                let at = self.caret.offset;
                // Backspace at the start of a list item ends the list first.
                if !forward && self.at_paragraph_start() && self.in_list() {
                    self.set_paragraph(cx, kind::LIST, None, &tr!("end-list"));
                    self.set_paragraph(cx, kind::LEVEL, None, &tr!("end-list"));
                    return true;
                }
                let range = if forward {
                    if at >= self.last_offset(self.caret.doc) {
                        return false;
                    }
                    at..if word {
                        word_end(&text, at)
                    } else {
                        next_boundary(&text, at)
                    }
                } else {
                    if at == 0 {
                        return false;
                    }
                    (if word {
                        word_start(&text, at)
                    } else {
                        prev_boundary(&text, at)
                    })..at
                };
                self.anchor = Caret {
                    doc: self.caret.doc,
                    offset: range.start,
                };
                self.caret = Caret {
                    doc: self.caret.doc,
                    offset: range.end,
                };
                self.replace_selection(cx, "", None)
            }
            Key::Enter if shift => {
                self.replace_selection(cx, &sue_model::LINE_BREAK.to_string(), None)
            }
            Key::Enter => {
                // Enter on an empty list item ends the list.
                if selection.is_empty() && self.in_list() && self.paragraph_empty() {
                    self.set_paragraph(cx, kind::LIST, None, &tr!("end-list"));
                    self.set_paragraph(cx, kind::LEVEL, None, &tr!("end-list"));
                    return true;
                }
                let (look, _) = self.look_here(view.sheet);
                let at_end = self.at_paragraph_end();
                let changed = self.replace_selection(cx, "\n", None);
                // Enter at the end of a heading goes on in its next style.
                if changed
                    && at_end
                    && let Some(next) = view.sheet.get(&look.style).next.clone()
                {
                    self.set_style(cx, &next, &tr!("next-style"));
                }
                changed
            }
            Key::Tab => {
                let (look, _) = self.look_here(view.sheet);
                if look.list.is_some() {
                    let level = if shift {
                        look.level.saturating_sub(1)
                    } else {
                        (look.level + 1).min(8)
                    };
                    self.set_paragraph(
                        cx,
                        kind::LEVEL,
                        Some(Value::Int(level as i64)),
                        &tr!("list-level"),
                    );
                    return true;
                }
                if let Some(tab) = view.sheet.get(&look.style).tab.clone()
                    && self.paragraph_empty()
                {
                    self.set_style(cx, &tab, &tr!("change-element"));
                    return true;
                }
                if shift {
                    return false;
                }
                self.replace_selection(cx, "\t", None)
            }
            _ => false,
        }
    }

    fn paragraph_range(&self) -> Range<usize> {
        let text = self.doc_text(self.caret.doc);
        let index = sue_model::paragraph_at(text, self.caret.offset);
        sue_model::paragraphs(text)
            .get(index)
            .cloned()
            .unwrap_or(0..0)
    }

    fn at_paragraph_start(&self) -> bool {
        self.paragraph_range().start == self.caret.offset
    }

    fn at_paragraph_end(&self) -> bool {
        let range = self.paragraph_range();
        self.selection().end + 1 >= range.end
    }

    fn paragraph_empty(&self) -> bool {
        self.paragraph_range().len() <= 1
    }

    fn in_list(&self) -> bool {
        let text = self.doc_text(self.caret.doc);
        let index = sue_model::paragraph_at(text, self.caret.offset);
        self.marks
            .get(self.caret.doc)
            .and_then(|marks| marks.paragraph_value(text, index, kind::LIST))
            .is_some()
    }

    // ----- Painting -----------------------------------------------------------------

    /// Draws the desk, the pages, the text, the selection and `caret` (on
    /// screen, from [`Self::caret_look`]).
    fn paint(&self, ui: &Ui, view: &EditorView<'_>, focused: bool, caret: Option<Rect>) {
        let painter = ui.painter();
        let paged = matches!(view.mode, ViewMode::Page);
        painter.rect_filled(self.viewport, 0.0, view.desk);
        let scale = view.zoom * PX_PER_PT;
        if paged {
            for page in 0..self.laid.pages {
                let rect = self.page_rect(page);
                if !rect.expand(12.0).intersects(self.viewport) {
                    continue;
                }
                painter.rect_filled(
                    rect.translate(vec2(0.0, 2.0)).expand(1.5),
                    2.0,
                    Color32::from_black_alpha(60),
                );
                painter.rect_filled(rect, 0.0, view.ink.paper);
                let small =
                    egui::FontId::new((9.0 * scale).max(6.0), egui::FontFamily::Proportional);
                let [margin_top, margin_right, margin_bottom, margin_left] =
                    view.page.margins.map(|margin| margin * scale);
                let furniture = view.page.furniture(page, self.laid.pages);
                let rows = [
                    rect.top() + margin_top * 0.5,
                    rect.bottom() - margin_bottom * 0.5,
                ];
                for (parts, y) in furniture.into_iter().zip(rows) {
                    let Some(parts) = parts else {
                        continue;
                    };
                    let places = [
                        (rect.left() + margin_left, egui::Align2::LEFT_CENTER),
                        (rect.center().x, egui::Align2::CENTER_CENTER),
                        (rect.right() - margin_right, egui::Align2::RIGHT_CENTER),
                    ];
                    for (text, (x, align)) in parts.iter().zip(places) {
                        if !text.is_empty() {
                            painter.text(pos2(x, y), align, text, small.clone(), view.ink.soft);
                        }
                    }
                }
            }
        } else {
            // The paper runs at least to the bottom of the view, so a short
            // text reads as a sheet rather than a strip cut from one.
            let column = Rect::from_min_size(
                pos2(self.origin.x + self.page_x - 24.0, self.origin.y),
                vec2(
                    self.laid.page.x + 48.0,
                    (self.laid.text_height + self.gap * 2.0)
                        .max(self.viewport.bottom() - self.origin.y),
                ),
            );
            painter.rect_filled(column.intersect(self.viewport), 0.0, view.ink.paper);
        }
        let selection = self.selection();
        let selecting = !selection.is_empty();
        let select_color = if focused {
            Color32::from_rgba_unmultiplied(0x3a, 0x7b, 0xd5, 0x55)
        } else {
            Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 0x45)
        };
        let mut last_doc = None;
        for index in 0..self.laid.pieces.len() {
            let piece = &self.laid.pieces[index];
            let block = &self.laid.blocks[piece.block];
            let (galley_at, clip) = self.piece_place(index);
            // Between documents, a divider with the next one's name.
            if last_doc != Some(block.doc) {
                if last_doc.is_some() && !paged && clip.top() - 20.0 < self.viewport.bottom() {
                    let y = clip.top() - 22.0;
                    let x = self.text_origin(0).x;
                    painter.hline(
                        x..=x + self.laid.text_width,
                        y,
                        Stroke::new(1.0, view.ink.soft.gamma_multiply(0.4)),
                    );
                    if let Some(title) = view.titles.get(block.doc) {
                        painter.text(
                            pos2(x + self.laid.text_width / 2.0, y),
                            egui::Align2::CENTER_CENTER,
                            format!("  {title}  "),
                            egui::FontId::proportional(11.0),
                            view.ink.soft,
                        );
                    }
                }
                last_doc = Some(block.doc);
            }
            if !clip.expand(8.0).intersects(self.viewport) {
                continue;
            }
            let text_left = self.text_origin(piece.page).x;
            if block.look.shaded {
                let shade = Rect::from_min_max(
                    pos2(text_left + block.indent - 6.0, clip.top() - 2.0),
                    pos2(
                        text_left + block.indent + block.width + 6.0,
                        clip.bottom() + 2.0,
                    ),
                );
                painter.rect_filled(shade, 3.0, view.ink.shade);
            }
            if block.look.rule {
                let x = text_left + block.indent - 12.0;
                painter.vline(
                    x,
                    clip.y_range(),
                    Stroke::new(3.0, view.ink.soft.gamma_multiply(0.5)),
                );
            }
            // The selection behind the text.
            if selecting && self.caret.doc == block.doc {
                self.paint_selection(painter, block, piece, galley_at, &selection, select_color);
            }
            if let Some(table) = block.table.as_ref().filter(|table| table.listing.is_some()) {
                let origin = pos2(text_left + block.indent, galley_at.y);
                crate::contents::paint_contents(
                    painter,
                    table,
                    piece.rows.clone(),
                    origin,
                    view.ink.text,
                );
                continue;
            }
            if let Some(table) = &block.table {
                let origin = pos2(text_left + block.indent, galley_at.y);
                paint_table(
                    painter,
                    table,
                    piece.rows.clone(),
                    origin,
                    view.ink.text,
                    view.ink.shade,
                );
                continue;
            }
            if let Some((entity, size)) = block.figure {
                let rect = Rect::from_min_size(
                    pos2(
                        text_left + block.indent + (block.width - size.x).max(0.0) / 2.0,
                        galley_at.y + piece.shift,
                    ),
                    size,
                );
                match (view.figures)(entity) {
                    Some((Some(texture), _)) => {
                        painter.image(
                            texture,
                            rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    _ => {
                        painter.rect_stroke(
                            rect,
                            2.0,
                            Stroke::new(1.0, view.ink.soft),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                continue;
            }
            painter
                .with_clip_rect(clip.intersect(self.viewport))
                .galley(galley_at, block.galley.clone(), view.ink.text);
            if view.spelling.is_some() {
                self.paint_spelling(
                    &painter.with_clip_rect(clip.intersect(self.viewport)),
                    view,
                    block,
                    galley_at,
                    focused,
                );
            }
            if piece.rows.start == 0
                && let Some(label) = &block.label
            {
                let row = block.galley.rows.first();
                let row_height = row.map_or(label.size().y, |row| row.rect().height());
                let at = pos2(
                    text_left + block.indent - label.size().x - 6.0 * scale,
                    galley_at.y + piece.shift + (row_height - label.size().y).max(0.0) * 0.7,
                );
                painter.galley(at, label.clone(), view.ink.text);
            }
        }
        if let Some(bar) = caret {
            let width = (1.5 * view.zoom).clamp(1.0, 3.0);
            painter.rect_filled(
                Rect::from_min_max(bar.min, pos2(bar.min.x + width, bar.max.y)),
                0.0,
                view.ink.text,
            );
        }
        if focused
            && let Some(preedit) = &self.preedit
            && let Some(caret) = self.caret_rect(self.caret)
        {
            let size = caret.height() * 0.75;
            let job = LayoutJob::single_section(
                preedit.clone(),
                TextFormat {
                    font_id: egui::FontId::proportional(size),
                    color: view.ink.text,
                    background: view.ink.paper,
                    underline: Stroke::new(1.0, view.ink.text),
                    ..TextFormat::default()
                },
            );
            let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
            painter.galley(caret.min, galley, view.ink.text);
        }
    }

    /// Red waves under the misspelled words of `block`, but not under the
    /// word being typed.
    fn paint_spelling(
        &self,
        painter: &egui::Painter,
        view: &EditorView<'_>,
        block: &Block,
        galley_at: Pos2,
        focused: bool,
    ) {
        let Some(text) = self.texts.get(block.doc) else {
            return;
        };
        let body = &text[block.range.clone()];
        let typing = (focused && self.caret.doc == block.doc).then_some(self.caret.offset);
        let wave = Stroke::new(1.0, Color32::from_rgb(0xd0, 0x30, 0x30));
        for range in crate::spell::words(body) {
            let word = &body[range.clone()];
            let at = block.range.start + range.start..block.range.start + range.end;
            if typing.is_some_and(|caret| caret == at.end) || self.spelled_right(view, word) {
                continue;
            }
            let cursor = |chars: usize| CCursor {
                index: CharIndex(chars),
                prefer_next_row: false,
            };
            let start = body[..range.start].chars().count();
            let end = start + word.chars().count();
            let from = block.galley.pos_from_cursor(cursor(start));
            let to = block.galley.pos_from_cursor(cursor(end));
            if (from.min.y - to.min.y).abs() > 0.5 {
                continue;
            }
            let y = galley_at.y + from.max.y - 1.0;
            let mut points = Vec::new();
            let mut x = galley_at.x + from.min.x;
            let right = galley_at.x + to.min.x;
            let mut up = false;
            while x < right {
                points.push(pos2(x, if up { y - 1.5 } else { y }));
                x += 2.0;
                up = !up;
            }
            if points.len() > 1 {
                painter.add(egui::Shape::line(points, wave));
            }
        }
    }

    /// Where to draw the caret this frame, on screen: gliding to its place
    /// and blinking at rest ([`crate::caret`]). `None` while the blink has
    /// it dark, or when it is not drawn (no keyboard, or an input method
    /// composing in its place). Asks for the frames that need it and no
    /// others.
    fn caret_look(
        &mut self,
        ui: &Ui,
        view: &EditorView<'_>,
        focused: bool,
        dragging: bool,
    ) -> Option<Rect> {
        let target = self.shown_caret();
        let shown = focused && self.preedit.is_none() && target.is_some();
        let offset = self.origin.to_vec2();
        let frame = caret::Frame {
            target: target.unwrap_or(Rect::NOTHING).translate(-offset),
            cause: self.caret_cause(),
            view: self.view_key(view),
            shown,
            viewport: self.viewport.translate(-offset),
            now: ui.input(|input| input.time),
            dragging,
        };
        let look = self.glide.frame(frame, self.motion);
        if look.moving {
            ui.ctx().request_repaint();
        } else if shown
            && let Some(wake) = look
                .wake
                .and_then(|wake| Duration::try_from_secs_f64(wake).ok())
        {
            ui.ctx().request_repaint_after(wake);
        }
        let target = target?;
        // At rest the caret's own bar is drawn, untouched.
        let bar = if look.moving {
            look.bar.translate(offset)
        } else {
            target
        };
        (shown && look.lit).then_some(bar)
    }

    /// What puts the caret where it is: its place and its document's
    /// revision. It glides when this changes.
    fn caret_cause(&self) -> u64 {
        let revision = self
            .docs
            .get(self.caret.doc)
            .and_then(|id| self.cache.get(id))
            .map_or(0, |cached| cached.key.revision);
        let mut hasher = DefaultHasher::new();
        (self.caret.doc, self.caret.offset, revision).hash(&mut hasher);
        hasher.finish()
    }

    /// How the documents are shown: which ones, the zoom, the page and
    /// where it sits. The caret never glides across a change of these.
    fn view_key(&self, view: &EditorView<'_>) -> u64 {
        let mode = match view.mode {
            ViewMode::Page => 0,
            ViewMode::Draft { width } => width.to_bits(),
        };
        let mut hasher = DefaultHasher::new();
        (
            &self.docs,
            mode,
            view.zoom.to_bits(),
            self.laid.page.x.to_bits(),
            self.laid.page.y.to_bits(),
            self.page_x.to_bits(),
            self.gap.to_bits(),
        )
            .hash(&mut hasher);
        hasher.finish()
    }

    fn paint_selection(
        &self,
        painter: &egui::Painter,
        block: &Block,
        piece: &crate::layout::Piece,
        galley_at: Pos2,
        selection: &Range<usize>,
        color: Color32,
    ) {
        let body = &block.range;
        // The paragraph's newline counts as selected when the selection
        // runs past it.
        let past_end = selection.end > body.end;
        if selection.end < body.start
            || selection.start > body.end
            || (selection.start == body.end && !past_end)
        {
            return;
        }
        let text = &self.texts[block.doc];
        let from = char_of_byte(text, block, selection.start.max(body.start));
        let to = char_of_byte(text, block, selection.end.min(body.end));
        if let Some((_, size)) = block.figure {
            let rect = Rect::from_min_size(galley_at + vec2(0.0, piece.shift), size);
            painter.rect_filled(rect, 0.0, color);
            return;
        }
        let mut start = 0;
        for (row_index, placed) in block.galley.rows.iter().enumerate() {
            let count = placed.row.glyphs.len();
            let end = start + count;
            if piece.rows.contains(&row_index) && to >= start && from <= end {
                let a = from.max(start) - start;
                let b = to.min(end) - start;
                let left = placed.pos.x + placed.row.x_offset(CharIndex(a));
                let mut right = placed.pos.x + placed.row.x_offset(CharIndex(b));
                let last_row = row_index + 1 == block.galley.rows.len();
                if (past_end && last_row) || (to > end && !last_row) {
                    right += 6.0;
                }
                if right - left < 1.0 && a == b && !(past_end && last_row) {
                    start = end + usize::from(placed.ends_with_newline);
                    continue;
                }
                let rect = Rect::from_min_max(
                    pos2(galley_at.x + left, galley_at.y + placed.min_y()),
                    pos2(galley_at.x + right, galley_at.y + placed.max_y()),
                );
                painter.rect_filled(rect, 0.0, color);
            }
            start = end + usize::from(placed.ends_with_newline);
        }
    }
}

// ----- Text boundaries ------------------------------------------------------------

fn prev_boundary(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(at, _)| at)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    text[offset.min(text.len())..]
        .chars()
        .next()
        .map_or(text.len(), |ch| offset + ch.len_utf8())
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '\'' || ch == '\u{2019}'
}

/// The start of the word before `offset` (skipping spaces first).
fn word_start(text: &str, offset: usize) -> usize {
    let mut at = offset;
    let mut seen_word = false;
    while at > 0 {
        let previous = prev_boundary(text, at);
        let ch = text[previous..at].chars().next().unwrap_or(' ');
        if is_word(ch) {
            seen_word = true;
        } else if seen_word || ch == '\n' {
            break;
        }
        at = previous;
    }
    at
}

/// The end of the word after `offset` (skipping spaces after it).
fn word_end(text: &str, offset: usize) -> usize {
    let mut at = offset;
    let mut seen_word = false;
    while at < text.len() {
        let ch = text[at..].chars().next().unwrap_or(' ');
        if ch == '\n' {
            break;
        }
        if is_word(ch) {
            seen_word = true;
        } else if seen_word {
            break;
        }
        at = next_boundary(text, at);
    }
    at
}

/// The word around `offset`.
fn word_around(text: &str, offset: usize) -> (usize, usize) {
    let mut start = offset;
    while start > 0 {
        let previous = prev_boundary(text, start);
        if !text[previous..start].chars().next().is_some_and(is_word) {
            break;
        }
        start = previous;
    }
    let mut end = offset;
    while end < text.len() && text[end..].chars().next().is_some_and(is_word) {
        end = next_boundary(text, end);
    }
    if start == end {
        end = next_boundary(text, end).min(text.len().saturating_sub(1).max(start));
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_characters_step_by_unicode() {
        let text = "Héllo, wide world\n";
        assert_eq!(next_boundary(text, 1), 3, "é is two bytes");
        assert_eq!(prev_boundary(text, 3), 1);
        assert_eq!(word_end(text, 0), 6);
        assert_eq!(word_start(text, 13), 8);
        assert_eq!(word_around(text, 9), (8, 12));
        assert_eq!(word_end(text, 13), 18);
    }
}
