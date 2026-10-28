//! The rich text editor every studio that writes uses: one or more rich
//! text documents (a chapter, or a whole folder of scenes one after
//! another) shown as printed pages or one long column, edited in place in
//! the arena. It draws its own caret and selection, so pages, styles,
//! lists and pictures stay exact, and takes the keyboard, the mouse, the
//! clipboard (keeping formatting between Sue's own documents) and input
//! methods. Edits go through [`crate::edit`], so undo covers them.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use egui::text::{CCursor, CharIndex, LayoutJob, TextFormat};
use egui::{Color32, Event, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use sue_core::{EntityId, Value};
use sue_model::{TextMarks, mark_kind as kind};
use sue_wm::AppletContext;

use crate::edit;
use crate::layout::{
    Block, Ink, Laid, PX_PER_PT, PageSetup, Setting, Source, ViewMode, byte_of_char, char_of_byte,
    lay_document, paginate,
};
use crate::style::{CharFormat, ParagraphLook, StyleSheet, char_format, paragraph_look};

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
    /// The picture to show for an embedded entity, with its size in points.
    pub figures: &'a dyn Fn(EntityId) -> Option<(egui::TextureId, Vec2)>,
    /// Keep the caret's line in the middle of the view.
    pub typewriter: bool,
    /// Turn straight quotes into curly ones while typing.
    pub smart_quotes: bool,
    /// Nothing can be changed.
    pub read_only: bool,
    /// The names of the documents, shown between them when there are
    /// several.
    pub titles: &'a [String],
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
    scroll_target: Option<Caret>,
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
            scroll_target: None,
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
        edit::reformat(cx, doc, "Clear Formatting", |_, marks| {
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

    /// Puts an embedded entity (a picture) in a paragraph of its own at the
    /// caret.
    pub fn insert_embed(&mut self, cx: &mut AppletContext<'_>, entity: EntityId) {
        let Some(doc) = self.current() else {
            return;
        };
        let at = self.selection().start;
        let text = self.text().to_owned();
        let starts_line = at == 0 || text.as_bytes().get(at - 1) == Some(&b'\n');
        let ends_line = text.as_bytes().get(at) == Some(&b'\n');
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
        edit::reformat(cx, doc, "Insert Picture", |_, marks| {
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
        let mut output = EditorOutput {
            pages: self.laid.pages,
            ..EditorOutput::default()
        };
        if self.want_focus {
            ctx.memory_mut(|memory| memory.request_focus(self.id));
            self.want_focus = false;
        }
        let scroll = egui::ScrollArea::both()
            .id_salt(self.id.with("scroll"))
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
        for (index, id) in docs.iter().enumerate() {
            let revision = cx
                .arena()
                .get_by_id(*id)
                .map_or(0, sue_core::Entity::revision);
            let key = DocKey {
                revision,
                sheet,
                width: width.to_bits(),
                scale: scale.to_bits(),
                fonts,
                ink,
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
        self.gap = if paged { 24.0 } else { 32.0 };
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
        self.viewport = ui.clip_rect();
        let response = ui.interact(rect, self.id, Sense::click_and_drag());
        let ctx = ui.ctx().clone();

        // Ctrl and the wheel zoom.
        if response.hovered() {
            let zoom = ui.input(|input| input.zoom_delta());
            if (zoom - 1.0).abs() > 1e-3 {
                output.zoom = Some(zoom);
            }
        }
        self.pointer(ui, &response, output);
        let focused = ctx.memory(|memory| memory.has_focus(self.id));
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
            if !view.read_only && self.keyboard(ui, cx, view) {
                output.changed = true;
                let docs = self.docs.clone();
                self.prepare(&ctx, cx, &docs, view);
            }
        }
        self.paint(ui, view, focused);
        if let Some(target) = self.scroll_target.take() {
            self.caret = self.clamp(target);
            self.follow = true;
        }
        if self.follow
            && let Some(caret) = self.caret_rect(self.caret)
        {
            let align = view.typewriter.then_some(egui::Align::Center);
            ui.scroll_to_rect(caret.expand2(vec2(48.0, 36.0)), align);
            self.follow = false;
        }
        if focused && let Some(caret) = self.caret_rect(self.caret) {
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

    fn pointer(&mut self, ui: &Ui, response: &egui::Response, output: &mut EditorOutput) {
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
        if pressed && response.is_pointer_button_down_on() {
            ui.ctx().memory_mut(|memory| memory.request_focus(self.id));
            let (last, at, count) = self.clicks;
            let count = if time - last < 0.45 && at.distance(pos) < 6.0 {
                count % 3 + 1
            } else {
                1
            };
            self.clicks = (time, pos, count);
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
                    if modifiers.command {
                        if let Some(link) = self.link_at(caret) {
                            output.link = Some(link);
                        }
                    }
                    self.set_caret(caret, modifiers.shift);
                }
            }
            self.pending.clear();
        } else if response.dragged() && self.clicks.2 == 1 {
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
        let format = std::mem::take(&mut self.pending);
        let Some(end) = edit::replace(cx, doc, range, text, source, &format) else {
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
                let text = self.doc_text(self.caret.doc).to_owned();
                let at = self.caret.offset;
                // Backspace at the start of a list item ends the list first.
                if !forward && self.at_paragraph_start() && self.in_list() {
                    self.set_paragraph(cx, kind::LIST, None, "End List");
                    self.set_paragraph(cx, kind::LEVEL, None, "End List");
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
            Key::Enter => {
                // Enter on an empty list item ends the list.
                if selection.is_empty() && self.in_list() && self.paragraph_empty() {
                    self.set_paragraph(cx, kind::LIST, None, "End List");
                    self.set_paragraph(cx, kind::LEVEL, None, "End List");
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
                    self.set_style(cx, &next, "Next Style");
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
                        "List Level",
                    );
                    return true;
                }
                if let Some(tab) = view.sheet.get(&look.style).tab.clone()
                    && self.paragraph_empty()
                {
                    self.set_style(cx, &tab, "Change Element");
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

    fn paint(&self, ui: &Ui, view: &EditorView<'_>, focused: bool) {
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
                if view.page.numbers {
                    painter.text(
                        pos2(
                            rect.center().x,
                            rect.bottom() - view.page.margins[2] * scale * 0.5,
                        ),
                        egui::Align2::CENTER_CENTER,
                        (page + 1).to_string(),
                        small.clone(),
                        view.ink.soft,
                    );
                }
                if page > 0 && !view.page.header.is_empty() {
                    painter.text(
                        pos2(
                            rect.center().x,
                            rect.top() + view.page.margins[0] * scale * 0.5,
                        ),
                        egui::Align2::CENTER_CENTER,
                        &view.page.header,
                        small,
                        view.ink.soft,
                    );
                }
            }
        } else {
            let column = Rect::from_min_size(
                pos2(self.origin.x + self.page_x - 24.0, self.origin.y),
                vec2(
                    self.laid.page.x + 48.0,
                    self.laid.text_height + self.gap * 2.0,
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
            if let Some((entity, size)) = block.figure {
                let rect = Rect::from_min_size(
                    pos2(
                        text_left + block.indent + (block.width - size.x).max(0.0) / 2.0,
                        galley_at.y + piece.shift,
                    ),
                    size,
                );
                match (view.figures)(entity) {
                    Some((texture, _)) => {
                        painter.image(
                            texture,
                            rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
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
        if focused && let Some(caret) = self.caret_rect(self.caret) {
            let width = (1.5 * view.zoom).clamp(1.0, 3.0);
            if let Some(preedit) = &self.preedit {
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
            } else {
                painter.rect_filled(
                    Rect::from_min_max(caret.min, pos2(caret.min.x + width, caret.max.y)),
                    0.0,
                    view.ink.text,
                );
            }
        }
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
