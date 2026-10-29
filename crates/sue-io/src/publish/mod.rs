//! Publishing rich text: a [`Book`] (sections of rich text with the
//! paragraph styles they use) written as Word, OpenDocument, EPUB, HTML,
//! RTF, Markdown or plain text, and Word, HTML and Markdown read back into
//! rich text. Any studio that publishes (a writing studio, notes, a page
//! layout studio) fills a [`Book`]; the formats never know who made it.
//!
//! Lengths are in points.

mod docx;
mod epub;
mod html;
mod markdown;
mod odt;
mod pictures;
mod rtf;
pub mod xml;

use std::collections::HashMap;
use std::ops::Range;

use sue_core::{EntityId, Value};
use sue_model::{CharFormat, Running, RunningPiece, TextMarks, mark_kind as kind, running_pieces};

pub use docx::{from_docx, to_docx};
pub use epub::to_epub;
pub use html::{from_html, to_html};
pub use markdown::{from_markdown, to_markdown};
pub use odt::to_odt;
pub use pictures::{PictureBook, picture_epub, picture_pdf};
pub use rtf::to_rtf;

/// A paragraph style, as the formats need it.
#[derive(Clone, Debug, PartialEq)]
pub struct PublishStyle {
    /// The key paragraphs name it by (their `style` mark), e.g. `heading1`.
    pub key: String,
    /// What people read, e.g. `Heading 1`.
    pub name: String,
    /// Font family.
    pub font: String,
    /// Size.
    pub size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// `left`, `center`, `right` or `justify`.
    pub align: String,
    /// Left indent.
    pub indent: f32,
    /// Right indent.
    pub indent_right: f32,
    /// First-line indent.
    pub first_indent: f32,
    /// Space above.
    pub space_before: f32,
    /// Space below.
    pub space_after: f32,
    /// Line spacing, a multiple of single.
    pub line_spacing: f32,
    /// Outline level (1 for the biggest headings).
    pub outline: Option<u8>,
    /// Starts a new page.
    pub page_break: bool,
    /// Stays with the next paragraph.
    pub keep_with_next: bool,
    /// Monospace (code).
    pub mono: bool,
}

impl PublishStyle {
    /// Body text in `font` at `size`.
    #[must_use]
    pub fn body(font: &str, size: f32) -> Self {
        Self {
            key: "body".to_owned(),
            name: "Body Text".to_owned(),
            font: font.to_owned(),
            size,
            bold: false,
            italic: false,
            align: "left".to_owned(),
            indent: 0.0,
            indent_right: 0.0,
            first_indent: 0.0,
            space_before: 0.0,
            space_after: 8.0,
            line_spacing: 1.15,
            outline: None,
            page_break: false,
            keep_with_next: false,
            mono: false,
        }
    }

    /// A plain set: body text, a title, four headings, quotes and code.
    #[must_use]
    pub fn defaults() -> Vec<Self> {
        let body = Self::body("Georgia", 12.0);
        let mut styles = vec![body.clone()];
        styles.push(Self {
            key: "title".to_owned(),
            name: "Title".to_owned(),
            size: 28.0,
            outline: Some(1),
            keep_with_next: true,
            ..body.clone()
        });
        for (level, size) in [(1_u8, 20.0), (2, 16.0), (3, 14.0), (4, 12.0)] {
            styles.push(Self {
                key: format!("heading{level}"),
                name: format!("Heading {level}"),
                size,
                bold: level >= 3,
                space_before: 12.0,
                space_after: 6.0,
                outline: Some(level),
                keep_with_next: true,
                ..body.clone()
            });
        }
        styles.push(Self {
            key: "quote".to_owned(),
            name: "Quote".to_owned(),
            italic: true,
            indent: 36.0,
            indent_right: 36.0,
            ..body.clone()
        });
        styles.push(Self {
            key: "code".to_owned(),
            name: "Code".to_owned(),
            font: "Courier New".to_owned(),
            size: 10.0,
            line_spacing: 1.0,
            mono: true,
            ..body
        });
        styles
    }
}

/// One section of a book: a chapter or a scene, with its own text.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Its title (for an e-book's contents and a PDF's bookmarks).
    pub title: String,
    /// How deep it sits, 0 for the top.
    pub depth: usize,
    /// Its text (paragraphs end with a newline).
    pub text: String,
    /// Its formatting.
    pub marks: TextMarks,
    /// It starts on a new page (an e-book starts a new file).
    pub new_page: bool,
}

/// A picture a book shows (its `embed` marks name the entity).
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    /// PNG bytes.
    pub png: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// A table a book shows (its `embed` marks name the entity).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PublishTable {
    /// Each cell's text, row by row (a cell's lines part at newlines).
    pub cells: Vec<Vec<String>>,
    /// Rows at the top that are headings.
    pub header_rows: usize,
    /// Each column's share of the width, adding up to one.
    pub shares: Vec<f32>,
}

impl PublishTable {
    /// The number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.shares
            .len()
            .max(self.cells.iter().map(Vec::len).max().unwrap_or(0))
    }

    /// Column `column`'s share of the width.
    #[must_use]
    pub fn share(&self, column: usize) -> f32 {
        self.shares
            .get(column)
            .copied()
            .unwrap_or(1.0 / self.columns().max(1) as f32)
    }

    /// Cell `(row, column)`'s text (empty where a row is short).
    #[must_use]
    pub fn cell(&self, row: usize, column: usize) -> &str {
        self.cells
            .get(row)
            .and_then(|cells| cells.get(column))
            .map_or("", String::as_str)
    }
}

/// What a publishing format writes.
#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    /// The title.
    pub title: String,
    /// The author.
    pub author: String,
    /// A language tag, e.g. `en`.
    pub language: String,
    /// A unique id (an e-book's identifier).
    pub identifier: String,
    /// The sections, in reading order.
    pub sections: Vec<Section>,
    /// The paragraph styles the sections use (`body` first).
    pub styles: Vec<PublishStyle>,
    /// The pictures.
    pub figures: HashMap<EntityId, Figure>,
    /// The tables.
    pub tables: HashMap<EntityId, PublishTable>,
    /// The page size.
    pub page: [f32; 2],
    /// Top, right, bottom and left margins.
    pub margins: [f32; 4],
    /// The running header, in the top margin.
    pub header: Running,
    /// The running footer (the page number in the middle by default).
    pub footer: Running,
    /// No header or footer on the first page.
    pub different_first: bool,
    /// The date a `{date}` field shows.
    pub date: String,
}

/// A piece of a running header or footer as a format writes it: text, or
/// a field the reader's program fills in on each page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Live {
    /// Text.
    Text(String),
    /// The page's number.
    Page,
    /// The number of pages.
    Pages,
}

impl Book {
    /// The left, middle and right parts of `running`, with the book's own
    /// title, author and date filled in and the page fields left live.
    pub(crate) fn live(&self, running: &Running) -> [Vec<Live>; 3] {
        [&running.left, &running.center, &running.right].map(|part| {
            running_pieces(part)
                .into_iter()
                .map(|piece| match piece {
                    RunningPiece::Text(text) => Live::Text(text.to_owned()),
                    RunningPiece::Field("page") => Live::Page,
                    RunningPiece::Field("pages") => Live::Pages,
                    RunningPiece::Field("title") => Live::Text(self.title.clone()),
                    RunningPiece::Field("author") => Live::Text(self.author.clone()),
                    RunningPiece::Field("date") => Live::Text(self.date.clone()),
                    RunningPiece::Field(other) => Live::Text(format!("{{{other}}}")),
                })
                .collect()
        })
    }
}

impl Book {
    /// A book of one section holding `text`, with the default styles.
    #[must_use]
    pub fn single(title: &str, text: &str, marks: &TextMarks) -> Self {
        Self {
            title: title.to_owned(),
            author: String::new(),
            language: "en".to_owned(),
            identifier: format!("urn:sue:{}", EntityId::new()),
            sections: vec![Section {
                title: title.to_owned(),
                depth: 0,
                text: text.to_owned(),
                marks: marks.clone(),
                new_page: false,
            }],
            styles: PublishStyle::defaults(),
            figures: HashMap::new(),
            tables: HashMap::new(),
            page: [612.0, 792.0],
            margins: [72.0; 4],
            header: Running::default(),
            footer: Running::parse("{page}"),
            different_first: false,
            date: String::new(),
        }
    }

    /// The table paragraph `para` shows, if it shows one.
    #[must_use]
    pub fn table(&self, para: &Para) -> Option<&PublishTable> {
        para.embed.and_then(|id| self.tables.get(&id))
    }

    /// The style `key`, else body text.
    #[must_use]
    pub fn style(&self, key: &str) -> &PublishStyle {
        static FALLBACK: std::sync::OnceLock<PublishStyle> = std::sync::OnceLock::new();
        self.styles
            .iter()
            .find(|style| style.key == key)
            .or_else(|| self.styles.iter().find(|style| style.key == "body"))
            .or(self.styles.first())
            .unwrap_or_else(|| FALLBACK.get_or_init(|| PublishStyle::body("Georgia", 12.0)))
    }
}

/// One paragraph, read for writing out.
#[derive(Clone, Debug, PartialEq)]
pub struct Para {
    /// Its style's key.
    pub style: String,
    /// Its own alignment, if not its style's.
    pub align: Option<String>,
    /// `bullet` or `number` in a list.
    pub list: Option<String>,
    /// Its list level.
    pub level: usize,
    /// Its own indents and spacing, if set.
    pub indent: Option<f32>,
    /// Its own first-line indent.
    pub first_indent: Option<f32>,
    /// Its own space above.
    pub space_before: Option<f32>,
    /// Its own space below.
    pub space_after: Option<f32>,
    /// Its own line spacing.
    pub line_spacing: Option<f32>,
    /// It starts a new page.
    pub page_break: bool,
    /// Its text in runs of one look.
    pub runs: Vec<(String, CharFormat)>,
    /// The picture it shows instead of text.
    pub embed: Option<EntityId>,
}

impl Para {
    /// Its plain text.
    #[must_use]
    pub fn text(&self) -> String {
        self.runs.iter().map(|(text, _)| text.as_str()).collect()
    }

    /// Its outline level, by its style.
    #[must_use]
    pub fn outline(&self, book: &Book) -> Option<u8> {
        book.style(&self.style).outline
    }

    /// Its alignment.
    #[must_use]
    pub fn alignment<'a>(&'a self, book: &'a Book) -> &'a str {
        self.align
            .as_deref()
            .unwrap_or(&book.style(&self.style).align)
    }
}

/// The paragraphs of `text` with `marks`, read for writing out.
#[must_use]
pub fn paragraphs(text: &str, marks: &TextMarks) -> Vec<Para> {
    let ranges = sue_model::paragraphs(text);
    let by = sue_model::marks_by_paragraph(marks.marks(), &ranges);
    ranges
        .into_iter()
        .zip(by)
        .map(|(range, own)| {
            let body: Range<usize> =
                range.start..range.end - usize::from(text[range.clone()].ends_with('\n'));
            let value = |name: &str| {
                own.iter()
                    .rfind(|mark| mark.kind == name)
                    .map(|mark| &mark.value)
            };
            let number = |name: &str| value(name).and_then(Value::as_f64).map(|v| v as f32);
            let style = value(kind::STYLE)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    value(kind::HEADING)
                        .and_then(Value::as_i64)
                        .map(|level| format!("heading{}", level.clamp(1, 4)))
                })
                .or_else(|| value(kind::QUOTE).map(|_| "quote".to_owned()))
                .unwrap_or_else(|| "body".to_owned());
            let embed = (text[body.clone()] == sue_model::OBJECT.to_string())
                .then(|| {
                    own.iter()
                        .find(|mark| mark.kind == kind::EMBED)
                        .and_then(|mark| match mark.value {
                            Value::Entity(id) => Some(id),
                            _ => None,
                        })
                })
                .flatten();
            Para {
                style,
                align: value(kind::ALIGN)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                list: value(kind::LIST).and_then(Value::as_str).map(str::to_owned),
                level: value(kind::LEVEL)
                    .and_then(Value::as_i64)
                    .map_or(0, |level| level.clamp(0, 8) as usize),
                indent: number(kind::INDENT),
                first_indent: number(kind::FIRST_INDENT),
                space_before: number(kind::SPACE_BEFORE),
                space_after: number(kind::SPACE_AFTER),
                line_spacing: number(kind::LINE_SPACING),
                page_break: value(kind::PAGE_BREAK)
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                runs: sue_model::runs(&own, body.clone())
                    .into_iter()
                    .map(|(piece, look)| (text[piece].to_owned(), look))
                    .collect(),
                embed,
            }
        })
        .collect()
}

/// Every footnote of a book in order, numbered from 1, as the formats
/// number them.
#[must_use]
pub fn footnotes(book: &Book) -> Vec<String> {
    book.sections
        .iter()
        .flat_map(|section| paragraphs(&section.text, &section.marks))
        .flat_map(|para| para.runs.into_iter().filter_map(|(_, look)| look.footnote))
        .collect()
}

/// The book as plain text: paragraphs on their own lines, a blank line
/// between sections, footnotes at the end.
#[must_use]
pub fn to_text(book: &Book) -> String {
    let mut out = String::new();
    let mut notes = Vec::new();
    for section in &book.sections {
        if !out.is_empty() {
            out.push('\n');
        }
        for para in paragraphs(&section.text, &section.marks) {
            if let Some(table) = book.table(&para) {
                for row in &table.cells {
                    out.push_str(&row.join("\t").replace('\n', " "));
                    out.push('\n');
                }
                continue;
            }
            if para.embed.is_some() {
                continue;
            }
            if para.list.as_deref() == Some("bullet") {
                out.push_str(&"  ".repeat(para.level));
                out.push_str("- ");
            }
            for (text, look) in &para.runs {
                out.push_str(text);
                if let Some(note) = &look.footnote {
                    notes.push(note.clone());
                    out.push_str(&format!("[{}]", notes.len()));
                }
            }
            out.push('\n');
        }
    }
    if !notes.is_empty() {
        out.push_str("\nNotes\n");
        for (index, note) in notes.iter().enumerate() {
            out.push_str(&format!("[{}] {note}\n", index + 1));
        }
    }
    out
}

/// A color as six hex digits.
pub(crate) fn hex(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A book with a heading, formatted body text, a list and a footnote.
    pub(crate) fn sample() -> Book {
        let text = "My Story\nIt was a dark and stormy night.\nFirst\nSecond\n";
        let mut marks = TextMarks::new();
        marks.set_paragraphs(text, 0..1, kind::STYLE, Some("heading1".into()));
        marks.set(18..22, kind::BOLD, Some(true.into()));
        marks.set(23..26, kind::ITALIC, Some(true.into()));
        marks.set(27..33, kind::FOOTNOTE, Some("Or so they say.".into()));
        marks.set_paragraphs(text, 41..42, kind::LIST, Some("bullet".into()));
        marks.set_paragraphs(text, 47..48, kind::LIST, Some("bullet".into()));
        let mut book = Book::single("My Story", text, &marks);
        book.author = "A. Writer".to_owned();
        book
    }

    /// A book holding a table between two paragraphs.
    pub(crate) fn with_table() -> Book {
        let text = format!("Parts\n{}\nThe end.\n", sue_model::OBJECT);
        let mut marks = TextMarks::new();
        let at = text.find(sue_model::OBJECT).unwrap_or(0);
        let id = EntityId::new();
        marks.set(
            at..at + sue_model::OBJECT.len_utf8(),
            kind::EMBED,
            Some(Value::Entity(id)),
        );
        let mut book = Book::single("Parts", &text, &marks);
        book.tables.insert(
            id,
            PublishTable {
                cells: vec![
                    vec!["Item".to_owned(), "Count & size".to_owned()],
                    vec!["Bolt <M8>".to_owned(), "40".to_owned()],
                ],
                header_rows: 1,
                shares: vec![0.7, 0.3],
            },
        );
        book
    }

    #[test]
    fn tables_go_into_plain_text_by_rows() {
        let plain = to_text(&with_table());
        assert!(
            plain.contains("Item\tCount & size\nBolt <M8>\t40\n"),
            "{plain}"
        );
    }

    #[test]
    fn paragraphs_read_their_style_runs_and_lists() {
        let book = sample();
        let paras = paragraphs(&book.sections[0].text, &book.sections[0].marks);
        assert_eq!(paras.len(), 4);
        assert_eq!(paras[0].style, "heading1");
        assert_eq!(paras[0].outline(&book), Some(1));
        assert_eq!(paras[1].text(), "It was a dark and stormy night.");
        assert!(
            paras[1]
                .runs
                .iter()
                .any(|(text, look)| text == "dark" && look.bold)
        );
        assert_eq!(paras[2].list.as_deref(), Some("bullet"));
        assert_eq!(footnotes(&book), vec!["Or so they say.".to_owned()]);
        let plain = to_text(&book);
        assert!(plain.contains("stormy[1]"), "{plain}");
        assert!(plain.contains("- First"));
    }
}
