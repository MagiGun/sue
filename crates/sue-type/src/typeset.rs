//! Setting rich text on printed pages, from the font files themselves: the
//! same styles and marks the editor shows, broken into lines with each
//! font's own widths, justified, paginated (headings kept with what
//! follows, no lone first lines at the foot of a page), with lists,
//! pictures, tables (broken between rows, their heading rows repeated),
//! links, footnotes gathered as notes at the end, page numbers and a
//! running head, then handed to the PDF writer in `sue-io`.

use std::collections::HashMap;
use std::sync::Arc;

use sue_core::{EntityId, Value};
use sue_io::pdf::{PdfBookmark, PdfDocument, PdfFont, PdfGlyph, PdfImage, PdfItem, PdfPage};
use sue_model::{CharFormat, TextMarks, mark_kind as kind};

use crate::layout::PageSetup;
use crate::style::{
    Align, ListKind, ParagraphLook, StyleSheet, marks_by_paragraph, paragraph_look, runs,
};
use crate::tables::TableSource;
use sue_model::contents::Contents;

/// One document to print.
pub struct PrintDoc<'a> {
    /// Its name (a bookmark when it has no heading of its own).
    pub title: &'a str,
    /// Its text.
    pub text: &'a str,
    /// Its formatting.
    pub marks: &'a TextMarks,
    /// Start it on a new page.
    pub new_page: bool,
}

/// A loaded face and the glyphs looked up so far.
struct Face {
    bytes: Arc<Vec<u8>>,
    index: u32,
    em: f32,
    ascent: f32,
    descent: f32,
    gap: f32,
    glyphs: HashMap<char, Option<(u16, f32)>>,
}

impl Face {
    fn load(bytes: Arc<Vec<u8>>, index: u32) -> Option<Self> {
        let face = ttf_parser::Face::parse(&bytes, index).ok()?;
        let em = f32::from(face.units_per_em()).max(1.0);
        let (ascent, descent, gap) = (
            f32::from(face.ascender()) / em,
            -f32::from(face.descender()) / em,
            f32::from(face.line_gap()) / em,
        );
        Some(Self {
            bytes,
            index,
            em,
            ascent,
            descent,
            gap,
            glyphs: HashMap::new(),
        })
    }

    /// The glyph id and advance (in ems) for `ch`.
    fn glyph(&mut self, ch: char) -> Option<(u16, f32)> {
        if let Some(found) = self.glyphs.get(&ch) {
            return *found;
        }
        let found = ttf_parser::Face::parse(&self.bytes, self.index)
            .ok()
            .and_then(|face| {
                let id = face.glyph_index(ch)?;
                let advance = f32::from(face.glyph_hor_advance(id).unwrap_or(0)) / self.em;
                Some((id.0, advance))
            });
        self.glyphs.insert(ch, found);
        found
    }
}

/// A face found for a family, bold and italic: its index, and whether
/// bold and italic must be faked.
type Choice = Option<(usize, bool, bool)>;

/// The faces a book uses.
#[derive(Default)]
struct Faces {
    faces: Vec<Face>,
    by_name: HashMap<(String, bool, bool), Choice>,
}

impl Faces {
    /// The face for `family` (with whether bold and italic must be faked).
    fn get(&mut self, family: &str, bold: bool, italic: bool) -> Choice {
        let key = (family.to_owned(), bold, italic);
        if let Some(known) = self.by_name.get(&key) {
            return *known;
        }
        let installed =
            sue_wm::fonts::find(family).and_then(|info| info.face(bold, italic).cloned());
        let (fake_bold, fake_italic) = match &installed {
            Some(face) => (bold && face.weight < 600, italic && !face.italic),
            None => (bold, italic),
        };
        let found = sue_wm::fonts::face_bytes(family, bold, italic)
            .or_else(|| sue_wm::fonts::face_bytes(sue_wm::fonts::SANS, false, false))
            .and_then(|(bytes, index)| {
                if let Some(at) = self
                    .faces
                    .iter()
                    .position(|face| Arc::ptr_eq(&face.bytes, &bytes) && face.index == index)
                {
                    return Some(at);
                }
                self.faces.push(Face::load(bytes, index)?);
                Some(self.faces.len() - 1)
            })
            .map(|at| (at, fake_bold, fake_italic));
        self.by_name.insert(key, found);
        found
    }
}

/// One glyph on its way to a line.
#[derive(Clone)]
struct Glyph {
    face: usize,
    id: u16,
    ch: char,
    size: f32,
    advance: f32,
    rise: f32,
    look: Arc<Look>,
}

/// How a run is drawn.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    color: [u8; 3],
    slant: bool,
    embolden: bool,
    underline: bool,
    strike: bool,
    highlight: Option<[u8; 3]>,
    link: Option<String>,
}

/// A line of glyphs, with its height.
struct Line {
    glyphs: Vec<(Glyph, f32)>,
    width: f32,
    ascent: f32,
    height: f32,
    last: bool,
}

/// A paragraph set into lines (or a picture), with its spacing.
enum Block {
    Text {
        look: ParagraphLook,
        lines: Vec<Line>,
        label: Option<Vec<(Glyph, f32)>>,
        title: String,
    },
    Picture {
        image: usize,
        size: [f32; 2],
        look: ParagraphLook,
    },
    /// A table of contents, until its entries are set.
    Contents {
        look: ParagraphLook,
        contents: Contents,
    },
    /// A table of contents, set.
    Listing {
        look: ParagraphLook,
        rows: Vec<ListingRow>,
    },
    Table {
        look: ParagraphLook,
        /// Column edges across, from the table's left.
        edges: Vec<f32>,
        /// Each row: its height and each cell's lines.
        rows: Vec<SetRow>,
        header_rows: usize,
    },
}

/// A table of contents' row, set: its title's lines, set in, and its page
/// number.
struct ListingRow {
    indent: f32,
    lines: Vec<Line>,
    page: Vec<(Glyph, f32)>,
    height: f32,
}

/// Sets a table of contents' title and `entries` (level, title, page)
/// across `width` points.
fn set_listing(
    faces: &mut Faces,
    contents: &Contents,
    entries: &[(u8, String, Option<usize>)],
    look: &ParagraphLook,
    width: f32,
) -> Vec<ListingRow> {
    let plain = Arc::new(Look {
        color: INK,
        slant: false,
        embolden: false,
        underline: false,
        strike: false,
        highlight: None,
        link: None,
    });
    let page_room = 36.0_f32.min(width / 3.0);
    let mut rows = Vec::new();
    let mut row =
        |faces: &mut Faces, text: &str, page: &str, size: f32, bold: bool, indent: f32| {
            let format = CharFormat {
                bold,
                ..CharFormat::default()
            };
            let (_, fake_bold, _) = faces
                .get(&look.font, look.bold || bold, look.italic)
                .unwrap_or((0, false, false));
            let run = Arc::new(Look {
                embolden: fake_bold,
                ..(*plain).clone()
            });
            let glyphs = shaped(faces, text, &look.font, size, &format, look, 0.0, &run);
            let lines = break_lines(
                faces,
                glyphs,
                (width - indent - page_room).max(24.0),
                0.0,
                look.line_spacing.min(crate::tables::CELL_SPACING),
            );
            let mut x = 0.0;
            let page = shaped(
                faces,
                page,
                &look.font,
                look.size,
                &CharFormat::default(),
                look,
                0.0,
                &plain,
            )
            .into_iter()
            .map(|glyph| {
                let at = x;
                x += glyph.advance;
                (glyph, at)
            })
            .collect();
            let height = lines.iter().map(|line| line.height).sum::<f32>() + 2.0;
            rows.push(ListingRow {
                indent,
                lines,
                page,
                height,
            });
        };
    if !contents.title.trim().is_empty() {
        row(faces, contents.title.trim(), "", look.size * 1.3, true, 0.0);
    }
    for (level, title, page) in entries {
        let page = page.map(|page| page.to_string()).unwrap_or_default();
        let indent = f32::from(level.saturating_sub(1)) * 18.0;
        row(faces, title, &page, look.size, *level == 1, indent);
    }
    rows
}

/// Draws a table of contents' row with its left at `x`, its top at `y`
/// and `width` points across: the title, a dotted leader, the page at the
/// right.
fn draw_listing_row(
    items: &mut Vec<PdfItem>,
    faces: &Faces,
    row: &ListingRow,
    x: f32,
    y: f32,
    width: f32,
) {
    let mut top = y + 1.0;
    let mut end = x + row.indent;
    let mut baseline = top;
    for line in &row.lines {
        baseline = top + line.ascent;
        let placed: Vec<(Glyph, f32)> = line
            .glyphs
            .iter()
            .map(|(glyph, offset)| (glyph.clone(), x + row.indent + offset))
            .collect();
        end = x + row.indent + line.width;
        decorate(items, &placed, baseline, faces);
        push_runs(items, placed.into_iter(), baseline);
        top += line.height;
    }
    let page_width = row
        .page
        .last()
        .map_or(0.0, |(glyph, at)| at + glyph.advance);
    if page_width <= 0.0 {
        return;
    }
    let page_x = x + width - page_width;
    let mut dot = end + 4.0;
    while dot < page_x - 3.0 {
        items.push(PdfItem::Rect {
            rect: [dot, baseline - 0.8, 0.8, 0.8],
            color: [0x66, 0x66, 0x66],
        });
        dot += 3.5;
    }
    push_runs(
        items,
        row.page
            .iter()
            .map(|(glyph, at)| (glyph.clone(), page_x + at)),
        baseline,
    );
}

/// A table row set: its height and each cell's lines.
type SetRow = (f32, Vec<Vec<Line>>);

/// The room between a table cell's edge and its text, in points.
const CELL_PAD: f32 = 4.0;

/// Sets a table's cells into lines across `width` points.
fn set_table(
    faces: &mut Faces,
    source: &TableSource,
    look: &ParagraphLook,
    width: f32,
) -> (Vec<f32>, Vec<SetRow>) {
    let mut edges = vec![0.0];
    for share in &source.shares {
        let last = edges.last().copied().unwrap_or(0.0);
        edges.push(last + share * width);
    }
    let plain = Arc::new(Look {
        color: INK,
        slant: false,
        embolden: false,
        underline: false,
        strike: false,
        highlight: None,
        link: None,
    });
    let mut rows = Vec::with_capacity(source.cells.len());
    for (row, cells) in source.cells.iter().enumerate() {
        let format = CharFormat {
            bold: row < source.header_rows,
            ..CharFormat::default()
        };
        let (_, fake_bold, _) = faces
            .get(&look.font, look.bold || format.bold, look.italic)
            .unwrap_or((0, false, false));
        let run = Arc::new(Look {
            embolden: fake_bold,
            ..(*plain).clone()
        });
        let mut tallest = 0.0_f32;
        let mut set = Vec::with_capacity(cells.len());
        for (column, text) in cells.iter().enumerate().take(source.shares.len()) {
            let room = (edges[column + 1] - edges[column] - CELL_PAD * 2.0).max(12.0);
            let mut lines = Vec::new();
            for paragraph in text.split('\n') {
                let mut glyphs = shaped(
                    faces, paragraph, &look.font, look.size, &format, look, 0.0, &run,
                );
                if glyphs.is_empty() {
                    glyphs = shaped(faces, " ", &look.font, look.size, &format, look, 0.0, &run);
                }
                lines.extend(break_lines(
                    faces,
                    glyphs,
                    room,
                    0.0,
                    look.line_spacing.min(crate::tables::CELL_SPACING),
                ));
            }
            tallest = tallest.max(lines.iter().map(|line| line.height).sum());
            set.push(lines);
        }
        rows.push((tallest + CELL_PAD * 2.0, set));
    }
    (edges, rows)
}

/// Draws one table row with its top left at `(x, y)` on the page.
fn draw_row(
    items: &mut Vec<PdfItem>,
    faces: &Faces,
    edges: &[f32],
    row: &SetRow,
    heading: bool,
    x: f32,
    y: f32,
) {
    let (height, cells) = row;
    let right = x + edges.last().copied().unwrap_or(0.0);
    if heading {
        items.push(PdfItem::Rect {
            rect: [x, y, right - x, *height],
            color: [0xee, 0xee, 0xee],
        });
    }
    for (column, lines) in cells.iter().enumerate() {
        let mut top = y + CELL_PAD;
        for line in lines {
            let baseline = top + line.ascent;
            let at = x + edges[column] + CELL_PAD;
            let placed: Vec<(Glyph, f32)> = line
                .glyphs
                .iter()
                .map(|(glyph, offset)| (glyph.clone(), at + offset))
                .collect();
            decorate(items, &placed, baseline, faces);
            push_runs(items, placed.into_iter(), baseline);
            top += line.height;
        }
    }
    let rule = |from: [f32; 2], to: [f32; 2]| PdfItem::Line {
        from,
        to,
        width: 0.5,
        color: [0x44, 0x44, 0x44],
    };
    items.push(rule([x, y], [right, y]));
    items.push(rule([x, y + height], [right, y + height]));
    for edge in edges {
        items.push(rule([x + edge, y], [x + edge, y + height]));
    }
}

const INK: [u8; 3] = [0x18, 0x18, 0x18];

#[allow(
    clippy::too_many_arguments,
    reason = "one run of text and everything about how it looks"
)]
fn shaped(
    faces: &mut Faces,
    text: &str,
    family: &str,
    size: f32,
    format: &CharFormat,
    base: &ParagraphLook,
    rise: f32,
    look: &Arc<Look>,
) -> Vec<Glyph> {
    let bold = base.bold || format.bold;
    let italic = base.italic || format.italic;
    let Some((face, ..)) = faces.get(family, bold, italic) else {
        return Vec::new();
    };
    let fallback = faces
        .get(sue_wm::fonts::SANS, false, false)
        .map(|(face, ..)| face);
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let breaks = ch == sue_model::LINE_BREAK;
        let ch = if ch == '\t' || breaks { ' ' } else { ch };
        let mut found = faces.faces[face].glyph(ch).map(|glyph| (face, glyph));
        if found.is_none_or(|(_, (id, _))| id == 0)
            && let Some(fallback) = fallback
            && let Some(glyph) = faces.faces[fallback].glyph(ch).filter(|(id, _)| *id != 0)
        {
            found = Some((fallback, glyph));
        }
        let Some((face, (id, advance))) = found else {
            continue;
        };
        let advance = if breaks {
            0.0
        } else {
            advance * size * if text.starts_with('\t') { 4.0 } else { 1.0 }
        };
        out.push(Glyph {
            face,
            id,
            ch: if breaks { sue_model::LINE_BREAK } else { ch },
            size,
            advance,
            rise,
            look: look.clone(),
        });
    }
    out
}

/// Breaks `glyphs` into lines no wider than `width` (the first `first`
/// narrower), at spaces, and inside a word only when it is wider than a
/// line.
fn break_lines(
    faces: &Faces,
    glyphs: Vec<Glyph>,
    width: f32,
    first: f32,
    spacing: f32,
) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current: Vec<Glyph> = Vec::new();
    let mut line_width = 0.0;
    let mut word: Vec<Glyph> = Vec::new();
    let mut word_width = 0.0;
    let room = |lines: &Vec<Line>| {
        if lines.is_empty() {
            width - first
        } else {
            width
        }
    };
    let finish = |glyphs: Vec<Glyph>, last: bool, lines: &mut Vec<Line>| {
        let mut x = 0.0;
        let mut placed = Vec::with_capacity(glyphs.len());
        let (mut ascent, mut descent, mut gap) = (0.0_f32, 0.0_f32, 0.0_f32);
        for glyph in glyphs {
            let face = &faces.faces[glyph.face];
            ascent = ascent.max(face.ascent * glyph.size + glyph.rise.max(0.0));
            descent = descent.max(face.descent * glyph.size - glyph.rise.min(0.0));
            gap = gap.max(face.gap * glyph.size);
            let advance = glyph.advance;
            placed.push((glyph, x));
            x += advance;
        }
        // Trailing spaces do not count toward the width.
        let width = placed
            .iter()
            .rev()
            .find(|(glyph, _)| glyph.ch != ' ')
            .map_or(0.0, |(glyph, x)| x + glyph.advance);
        let natural = ascent + descent + gap;
        lines.push(Line {
            glyphs: placed,
            width,
            ascent: ascent + (natural * spacing - natural) / 2.0,
            height: natural * spacing,
            last,
        });
    };
    for glyph in glyphs {
        if glyph.ch == sue_model::LINE_BREAK {
            // A line break: this line ends here, set as a paragraph's last.
            if line_width + word_width > room(&lines) && !current.is_empty() {
                finish(std::mem::take(&mut current), false, &mut lines);
            }
            current.append(&mut word);
            finish(std::mem::take(&mut current), true, &mut lines);
            line_width = 0.0;
            word_width = 0.0;
            continue;
        }
        if glyph.ch == ' ' {
            if line_width + word_width > room(&lines) && !current.is_empty() {
                finish(std::mem::take(&mut current), false, &mut lines);
                line_width = 0.0;
            }
            line_width += word_width + glyph.advance;
            current.append(&mut word);
            current.push(glyph);
            word_width = 0.0;
            continue;
        }
        word_width += glyph.advance;
        word.push(glyph);
        if word_width > room(&lines) && line_width == 0.0 && word.len() > 1 {
            // A word wider than a line breaks where it must.
            let last = word.pop();
            finish(std::mem::take(&mut word), false, &mut lines);
            word_width = 0.0;
            if let Some(last) = last {
                word_width = last.advance;
                word.push(last);
            }
        }
    }
    if line_width + word_width > room(&lines) && !current.is_empty() {
        finish(std::mem::take(&mut current), false, &mut lines);
    }
    current.append(&mut word);
    finish(current, true, &mut lines);
    lines
}

/// What the text shows in place of its object characters (`embed` marks).
pub struct Embeds<'a> {
    /// Pictures.
    pub figures: &'a dyn Fn(EntityId) -> Option<PdfImage>,
    /// Tables.
    pub tables: &'a dyn Fn(EntityId) -> Option<TableSource>,
    /// Tables of contents.
    pub contents: &'a dyn Fn(EntityId) -> Option<Contents>,
}

impl Embeds<'_> {
    /// Nothing shown: object characters print as nothing.
    pub const NONE: Embeds<'static> = Embeds {
        figures: &|_| None,
        tables: &|_| None,
        contents: &|_| None,
    };
}

/// Sets `docs` on pages with `sheet`, as a PDF document. A table of
/// contents is set twice: once to find each heading's page, then with
/// them.
#[must_use]
pub fn typeset(
    docs: &[PrintDoc<'_>],
    sheet: &StyleSheet,
    page: &PageSetup,
    embeds: &Embeds<'_>,
    title: &str,
    author: &str,
) -> PdfDocument {
    let (pdf, pages, listed) = set_pass(docs, sheet, page, embeds, None, title, author);
    if listed {
        set_pass(docs, sheet, page, embeds, Some(&pages), title, author).0
    } else {
        pdf
    }
}

/// One setting of `docs`: the document, each block's page, and whether
/// there was a table of contents. `known` gives each block's page from an
/// earlier pass, for the contents.
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "one pass over the book"
)]
fn set_pass(
    docs: &[PrintDoc<'_>],
    sheet: &StyleSheet,
    page: &PageSetup,
    embeds: &Embeds<'_>,
    known: Option<&[usize]>,
    title: &str,
    author: &str,
) -> (PdfDocument, Vec<usize>, bool) {
    let figures = embeds.figures;
    let tables = embeds.tables;
    let mut faces = Faces::default();
    let mut images = Vec::new();
    let mut blocks: Vec<(usize, Block)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let width = page.text_width();
    for (doc_index, doc) in docs.iter().enumerate() {
        let paragraphs = sue_model::paragraphs(doc.text);
        let by = marks_by_paragraph(doc.marks.marks(), &paragraphs);
        let mut numbers = [0_usize; 9];
        for (range, own) in paragraphs.iter().zip(&by) {
            let look = paragraph_look(sheet, own);
            let body =
                range.start..range.end - usize::from(doc.text[range.clone()].ends_with('\n'));
            let content = &doc.text[body.clone()];
            let embed = (content == sue_model::OBJECT.to_string())
                .then(|| {
                    own.iter()
                        .find(|mark| mark.kind == kind::EMBED)
                        .and_then(|mark| match mark.value {
                            Value::Entity(id) => Some(id),
                            _ => None,
                        })
                })
                .flatten();
            if let Some(contents) = embed.and_then(embeds.contents) {
                blocks.push((doc_index, Block::Contents { look, contents }));
                continue;
            }
            if let Some(source) = embed.and_then(tables) {
                let room = (width - look.indent - look.indent_right).max(36.0);
                let (edges, rows) = set_table(&mut faces, &source, &look, room);
                blocks.push((
                    doc_index,
                    Block::Table {
                        look,
                        edges,
                        rows,
                        header_rows: source.header_rows,
                    },
                ));
                continue;
            }
            if content == sue_model::OBJECT.to_string()
                && let Some(id) =
                    own.iter()
                        .find(|mark| mark.kind == kind::EMBED)
                        .and_then(|mark| match mark.value {
                            Value::Entity(id) => Some(id),
                            _ => None,
                        })
                && let Some(image) = figures(id)
            {
                let room = width - look.indent - look.indent_right;
                let natural = [image.width as f32 * 0.75, image.height as f32 * 0.75];
                let scale = (room / natural[0].max(1.0))
                    .min(page.text_height() / natural[1].max(1.0))
                    .min(1.0);
                images.push(image);
                blocks.push((
                    doc_index,
                    Block::Picture {
                        image: images.len() - 1,
                        size: [natural[0] * scale, natural[1] * scale],
                        look,
                    },
                ));
                continue;
            }
            let mut glyphs = Vec::new();
            for (piece, format) in runs(own, body.clone()) {
                let size =
                    format.size.unwrap_or(look.size) * if format.script != 0 { 0.66 } else { 1.0 };
                let rise = f32::from(format.script) * look.size * 0.33;
                let family = if format.code {
                    sheet.mono_font.clone()
                } else {
                    format.font.clone().unwrap_or_else(|| look.font.clone())
                };
                let (_, fake_bold, fake_italic) = faces
                    .get(
                        &family,
                        look.bold || format.bold,
                        look.italic || format.italic,
                    )
                    .unwrap_or((0, false, false));
                let color = format
                    .color
                    .or(look.color)
                    .unwrap_or(if format.link.is_some() {
                        [0x1a, 0x55, 0xb0]
                    } else if look.soft {
                        [0x6a, 0x6a, 0x6a]
                    } else {
                        INK
                    });
                let run_look = Arc::new(Look {
                    color,
                    slant: fake_italic,
                    embolden: fake_bold,
                    underline: look.underline || format.underline || format.link.is_some(),
                    strike: format.strike,
                    highlight: format
                        .highlight
                        .or(format.code.then_some([0xee, 0xee, 0xee])),
                    link: format.link.clone(),
                });
                let text = &doc.text[piece];
                let text = if format.small_caps || format.caps {
                    text.to_uppercase()
                } else {
                    text.to_owned()
                };
                glyphs.extend(shaped(
                    &mut faces, &text, &family, size, &format, &look, rise, &run_look,
                ));
                if let Some(note) = &format.footnote {
                    notes.push(note.clone());
                    let mark = Arc::new(Look {
                        underline: false,
                        strike: false,
                        highlight: None,
                        link: None,
                        ..(*run_look).clone()
                    });
                    glyphs.extend(shaped(
                        &mut faces,
                        &notes.len().to_string(),
                        &look.font,
                        look.size * 0.66,
                        &CharFormat::default(),
                        &look,
                        look.size * 0.33,
                        &mark,
                    ));
                }
            }
            if glyphs.is_empty() {
                // An empty paragraph keeps its height.
                let plain = Arc::new(Look {
                    color: INK,
                    slant: false,
                    embolden: false,
                    underline: false,
                    strike: false,
                    highlight: None,
                    link: None,
                });
                glyphs = shaped(
                    &mut faces,
                    " ",
                    &look.font,
                    look.size,
                    &CharFormat::default(),
                    &look,
                    0.0,
                    &plain,
                );
            }
            let first = look.first_indent.max(0.0);
            let lines = break_lines(
                &faces,
                glyphs,
                (width - look.indent - look.indent_right).max(36.0),
                first,
                look.line_spacing,
            );
            let label = look.list.map(|list| {
                numbers[look.level + 1..].iter_mut().for_each(|n| *n = 0);
                numbers[look.level] += 1;
                let text = match list {
                    ListKind::Bullet => {
                        ["\u{2022}", "\u{25E6}", "\u{25AA}"][look.level % 3].to_owned()
                    }
                    ListKind::Number => format!("{}.", numbers[look.level]),
                };
                let plain = Arc::new(Look {
                    color: INK,
                    slant: false,
                    embolden: false,
                    underline: false,
                    strike: false,
                    highlight: None,
                    link: None,
                });
                let mut x = 0.0;
                shaped(
                    &mut faces,
                    &text,
                    &look.font,
                    look.size,
                    &CharFormat::default(),
                    &look,
                    0.0,
                    &plain,
                )
                .into_iter()
                .map(|glyph| {
                    let at = x;
                    x += glyph.advance;
                    (glyph, at)
                })
                .collect()
            });
            if look.list.is_none() {
                numbers = [0; 9];
            }
            blocks.push((
                doc_index,
                Block::Text {
                    look,
                    lines,
                    label,
                    title: content.trim().to_owned(),
                },
            ));
        }
    }
    // Tables of contents: the headings they list, with their pages.
    let listed = blocks
        .iter()
        .any(|(_, block)| matches!(block, Block::Contents { .. }));
    if listed {
        let headings: Vec<(usize, u8, String)> = blocks
            .iter()
            .enumerate()
            .filter_map(|(index, (_, block))| match block {
                Block::Text { look, title, .. } if !title.is_empty() => {
                    Some((index, look.outline?, title.clone()))
                }
                _ => None,
            })
            .collect();
        for (_, block) in &mut blocks {
            let Block::Contents { look, contents } = &*block else {
                continue;
            };
            let (look, contents) = (look.clone(), contents.clone());
            let room = (width - look.indent - look.indent_right).max(72.0);
            let entries: Vec<(u8, String, Option<usize>)> = headings
                .iter()
                .filter(|(_, level, _)| contents.lists(*level))
                .map(|(block, level, title)| {
                    let page = known
                        .and_then(|known| known.get(*block))
                        .map(|page| page + 1);
                    (*level, title.clone(), page)
                })
                .collect();
            let rows = set_listing(&mut faces, &contents, &entries, &look, room);
            *block = Block::Listing { look, rows };
        }
    }
    // Notes at the end, under their own heading.
    if !notes.is_empty() {
        let heading = paragraph_look_of(
            sheet,
            if sheet.has("heading2") {
                "heading2"
            } else {
                "body"
            },
        );
        let plain = Arc::new(Look {
            color: INK,
            slant: false,
            embolden: false,
            underline: false,
            strike: false,
            highlight: None,
            link: None,
        });
        let glyphs = shaped(
            &mut faces,
            &tr!("notes"),
            &heading.font,
            heading.size,
            &CharFormat::default(),
            &heading,
            0.0,
            &plain,
        );
        let lines = break_lines(&faces, glyphs, width, 0.0, heading.line_spacing);
        blocks.push((
            docs.len(),
            Block::Text {
                look: heading,
                lines,
                label: None,
                title: tr!("notes"),
            },
        ));
        let body = paragraph_look_of(sheet, "body");
        for (index, note) in notes.iter().enumerate() {
            let text = format!("{}. {note}", index + 1);
            let glyphs = shaped(
                &mut faces,
                &text,
                &body.font,
                body.size * 0.9,
                &CharFormat::default(),
                &body,
                0.0,
                &plain,
            );
            let lines = break_lines(&faces, glyphs, width, 0.0, 1.15);
            blocks.push((
                docs.len(),
                Block::Text {
                    look: ParagraphLook {
                        space_before: 0.0,
                        space_after: 4.0,
                        first_indent: 0.0,
                        outline: None,
                        ..body.clone()
                    },
                    lines,
                    label: None,
                    title: String::new(),
                },
            ));
        }
    }

    // Pages.
    let [top, _, _, left] = page.margins;
    let height = page.text_height();
    let mut pages: Vec<PdfPage> = vec![PdfPage {
        size: page.size,
        items: Vec::new(),
    }];
    let mut bookmarks = Vec::new();
    let mut y = 0.0_f32;
    let mut last_doc = usize::MAX;
    let mut block_pages = vec![0; blocks.len()];
    let new_page = |pages: &mut Vec<PdfPage>, y: &mut f32| {
        pages.push(PdfPage {
            size: page.size,
            items: Vec::new(),
        });
        *y = 0.0;
    };
    for index in 0..blocks.len() {
        let doc_index = blocks[index].0;
        let starts_doc = doc_index != last_doc;
        last_doc = doc_index;
        if starts_doc && docs.get(doc_index).is_some_and(|doc| doc.new_page) && y > 0.0 {
            new_page(&mut pages, &mut y);
        }
        block_pages[index] = pages.len() - 1;
        match &blocks[index].1 {
            Block::Contents { .. } => {}
            Block::Listing { look, rows } => {
                if y > 0.0 {
                    y += look.space_before;
                }
                let room = (width - look.indent - look.indent_right).max(72.0);
                let x = left + look.indent;
                for row in rows {
                    if y + row.height > height + 0.5 && y > 0.0 {
                        new_page(&mut pages, &mut y);
                    }
                    let page_index = pages.len() - 1;
                    draw_listing_row(&mut pages[page_index].items, &faces, row, x, top + y, room);
                    y += row.height;
                }
                y += look.space_after;
            }
            Block::Table {
                look,
                edges,
                rows,
                header_rows,
            } => {
                if y > 0.0 {
                    y += look.space_before.max(crate::tables::TABLE_SPACE);
                }
                let headings = (*header_rows).min(rows.len());
                let repeat: f32 = rows[..headings].iter().map(|(height, _)| height).sum();
                let x = left + look.indent;
                for (row_index, row) in rows.iter().enumerate() {
                    if y + row.0 > height + 0.5 && y > 0.0 {
                        new_page(&mut pages, &mut y);
                        // The heading rows again at the top of the page.
                        if row_index >= headings && repeat + row.0 <= height {
                            for heading in &rows[..headings] {
                                let page_index = pages.len() - 1;
                                draw_row(
                                    &mut pages[page_index].items,
                                    &faces,
                                    edges,
                                    heading,
                                    true,
                                    x,
                                    top + y,
                                );
                                y += heading.0;
                            }
                        }
                    }
                    let page_index = pages.len() - 1;
                    draw_row(
                        &mut pages[page_index].items,
                        &faces,
                        edges,
                        row,
                        row_index < headings,
                        x,
                        top + y,
                    );
                    y += row.0;
                }
                y += look.space_after.max(crate::tables::TABLE_SPACE);
            }
            Block::Picture { image, size, look } => {
                if y > 0.0 {
                    y += look.space_before;
                }
                if y + size[1] > height && y > 0.0 {
                    new_page(&mut pages, &mut y);
                }
                let room = width - look.indent - look.indent_right;
                let x = left + look.indent + (room - size[0]).max(0.0) / 2.0;
                let page_index = pages.len() - 1;
                pages[page_index].items.push(PdfItem::Image {
                    image: *image,
                    rect: [x, top + y, size[0], size[1]],
                });
                y += size[1] + look.space_after;
            }
            Block::Text {
                look,
                lines,
                label,
                title,
            } => {
                if look.page_break && y > 0.0 {
                    new_page(&mut pages, &mut y);
                }
                if y > 0.0 {
                    y += look.space_before;
                }
                let total: f32 = lines.iter().map(|line| line.height).sum();
                // A heading stays with the first line after it.
                if look.keep_with_next && y > 0.0 {
                    let next = match blocks.get(index + 1).map(|(_, block)| block) {
                        Some(Block::Text { lines, look, .. }) => {
                            lines.first().map_or(0.0, |line| line.height) + look.space_before
                        }
                        Some(Block::Picture { size, .. }) => size[1],
                        Some(Block::Table { rows, .. }) => rows.first().map_or(0.0, |row| row.0),
                        Some(Block::Listing { rows, .. }) => {
                            rows.first().map_or(0.0, |row| row.height)
                        }
                        Some(Block::Contents { .. }) => 0.0,
                        None => 0.0,
                    };
                    if y + total + next > height && total + next <= height {
                        new_page(&mut pages, &mut y);
                    }
                }
                // No lone first line at the foot of a page.
                if lines.len() > 2 && y > 0.0 && y + lines[0].height + lines[1].height > height {
                    new_page(&mut pages, &mut y);
                }
                if starts_doc
                    && look.outline.is_none()
                    && docs.get(doc_index).is_some_and(|doc| !doc.title.is_empty())
                {
                    bookmarks.push(PdfBookmark {
                        title: docs[doc_index].title.to_owned(),
                        level: 1,
                        page: pages.len() - 1,
                        y: top + y,
                    });
                }
                block_pages[index] = pages.len() - 1;
                if let Some(level) = look.outline
                    && !title.is_empty()
                {
                    bookmarks.push(PdfBookmark {
                        title: title.clone(),
                        level,
                        page: pages.len() - 1,
                        y: top + y,
                    });
                }
                let room = width - look.indent - look.indent_right;
                for (line_index, line) in lines.iter().enumerate() {
                    if y + line.height > height + 0.5 && y > 0.0 {
                        new_page(&mut pages, &mut y);
                    }
                    let first = line_index == 0;
                    let indent = look.indent
                        + if first {
                            look.first_indent.max(0.0)
                        } else {
                            0.0
                        };
                    let line_room = room
                        - if first {
                            look.first_indent.max(0.0)
                        } else {
                            0.0
                        };
                    let spaces = line
                        .glyphs
                        .iter()
                        .filter(|(glyph, x)| {
                            glyph.ch == ' ' && x + glyph.advance <= line.width + 0.01
                        })
                        .count();
                    let (shift, stretch) = match look.align {
                        Align::Left => (0.0, 0.0),
                        Align::Center => ((line_room - line.width) / 2.0, 0.0),
                        Align::Right => (line_room - line.width, 0.0),
                        Align::Justify if !line.last && spaces > 0 => {
                            (0.0, (line_room - line.width) / spaces as f32)
                        }
                        Align::Justify => (0.0, 0.0),
                    };
                    let baseline = top + y + line.ascent;
                    let x0 = left + indent + shift.max(0.0);
                    let page_index = pages.len() - 1;
                    let items = &mut pages[page_index].items;
                    if first && let Some(label) = label {
                        let label_width = label.last().map_or(0.0, |(glyph, x)| x + glyph.advance);
                        let at = left + look.indent - label_width - 6.0;
                        push_runs(
                            items,
                            label.iter().map(|(glyph, x)| (glyph.clone(), at + x)),
                            baseline,
                        );
                    }
                    let mut extra = 0.0;
                    let placed: Vec<(Glyph, f32)> = line
                        .glyphs
                        .iter()
                        .map(|(glyph, x)| {
                            let at = x0 + x + extra;
                            if glyph.ch == ' ' {
                                extra += stretch;
                            }
                            (glyph.clone(), at)
                        })
                        .collect();
                    // Backgrounds and lines under the text, then the text.
                    decorate(items, &placed, baseline, &faces);
                    push_runs(items, placed.into_iter(), baseline);
                    y += line.height;
                }
                y += look.space_after;
            }
        }
    }
    // The running header and footer.
    let numbers_look = paragraph_look_of(sheet, "body");
    let plain = Arc::new(Look {
        color: [0x55, 0x55, 0x55],
        slant: false,
        embolden: false,
        underline: false,
        strike: false,
        highlight: None,
        link: None,
    });
    let count = pages.len();
    for (index, pdf_page) in pages.iter_mut().enumerate() {
        let mut furniture = Vec::new();
        let [header, footer] = page.furniture(index, count);
        for (parts, baseline) in [
            (header, page.margins[0] / 2.0),
            (footer, page.size[1] - page.margins[2] / 2.0),
        ] {
            for (part, text) in parts.into_iter().flatten().enumerate() {
                if !text.is_empty() {
                    furniture.push((text, part, baseline));
                }
            }
        }
        for (text, part, baseline) in furniture {
            let glyphs = shaped(
                &mut faces,
                &text,
                &numbers_look.font,
                9.0,
                &CharFormat::default(),
                &numbers_look,
                0.0,
                &plain,
            );
            let width: f32 = glyphs.iter().map(|glyph| glyph.advance).sum();
            let mut x = match part {
                0 => page.margins[3],
                1 => (page.size[0] - width) / 2.0,
                _ => page.size[0] - page.margins[1] - width,
            };
            let placed: Vec<(Glyph, f32)> = glyphs
                .into_iter()
                .map(|glyph| {
                    let at = x;
                    x += glyph.advance;
                    (glyph, at)
                })
                .collect();
            push_runs(&mut pdf_page.items, placed.into_iter(), baseline);
        }
    }
    let pdf = PdfDocument {
        title: title.to_owned(),
        author: author.to_owned(),
        fonts: faces
            .faces
            .iter()
            .map(|face| PdfFont {
                bytes: face.bytes.clone(),
                index: face.index,
            })
            .collect(),
        images,
        pages,
        bookmarks,
    };
    (pdf, block_pages, listed)
}

fn paragraph_look_of(sheet: &StyleSheet, key: &str) -> ParagraphLook {
    let mark = sue_model::Mark::new(0..1, kind::STYLE, key);
    paragraph_look(sheet, &[&mark])
}

/// Highlights, underlines, strike lines and links of a line.
fn decorate(items: &mut Vec<PdfItem>, placed: &[(Glyph, f32)], baseline: f32, faces: &Faces) {
    let mut start = 0;
    while start < placed.len() {
        let look = placed[start].0.look.clone();
        let mut end = start + 1;
        while end < placed.len() && Arc::ptr_eq(&placed[end].0.look, &look) {
            end += 1;
        }
        let first = &placed[start];
        let last = &placed[end - 1];
        let x0 = first.1;
        let x1 = last.1 + last.0.advance;
        let size = first.0.size;
        let face = &faces.faces[first.0.face];
        let ascent = face.ascent * size;
        let descent = face.descent * size;
        if let Some(color) = look.highlight {
            items.push(PdfItem::Rect {
                rect: [x0, baseline - ascent, x1 - x0, ascent + descent],
                color,
            });
        }
        if look.underline {
            let y = baseline + size * 0.12;
            items.push(PdfItem::Line {
                from: [x0, y],
                to: [x1, y],
                width: size * 0.05,
                color: look.color,
            });
        }
        if look.strike {
            let y = baseline - size * 0.28;
            items.push(PdfItem::Line {
                from: [x0, y],
                to: [x1, y],
                width: size * 0.05,
                color: look.color,
            });
        }
        if let Some(uri) = &look.link {
            items.push(PdfItem::Link {
                rect: [x0, baseline - ascent, x1 - x0, ascent + descent],
                uri: uri.clone(),
            });
        }
        start = end;
    }
}

/// Turns placed glyphs into text items, one per run of font, size, color
/// and rise.
fn push_runs(items: &mut Vec<PdfItem>, placed: impl Iterator<Item = (Glyph, f32)>, baseline: f32) {
    /// The run being gathered: font, size, rise, look and glyphs.
    type Open = Option<(usize, f32, f32, Arc<Look>, Vec<PdfGlyph>)>;
    let mut current: Open = None;
    let flush = |current: &mut Open, items: &mut Vec<PdfItem>| {
        if let Some((font, size, rise, look, glyphs)) = current.take()
            && !glyphs.is_empty()
        {
            items.push(PdfItem::Text {
                font,
                size,
                baseline: baseline - rise,
                color: look.color,
                slant: look.slant,
                embolden: look.embolden,
                glyphs,
            });
        }
    };
    for (glyph, x) in placed {
        let same = current.as_ref().is_some_and(|(font, size, rise, look, _)| {
            *font == glyph.face
                && (*size - glyph.size).abs() < 0.01
                && (*rise - glyph.rise).abs() < 0.01
                && look.color == glyph.look.color
                && look.slant == glyph.look.slant
                && look.embolden == glyph.look.embolden
        });
        if !same {
            flush(&mut current, items);
            current = Some((
                glyph.face,
                glyph.size,
                glyph.rise,
                glyph.look.clone(),
                Vec::new(),
            ));
        }
        if let Some((.., glyphs)) = &mut current
            && glyph.ch != ' '
        {
            glyphs.push(PdfGlyph {
                id: glyph.id,
                x,
                text: glyph.ch.to_string(),
            });
        }
    }
    flush(&mut current, items);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Fonts;

    #[test]
    fn a_manuscript_sets_into_pages_with_bookmarks_and_notes() {
        let line = "All work and no play makes for a long, long day. ".repeat(40);
        let text = format!("Chapter One\n{line}\nThe end.\n");
        let mut marks = TextMarks::new();
        marks.set_paragraphs(&text, 0..1, kind::STYLE, Some("heading1".into()));
        marks.set_paragraphs(&text, 12..13, kind::ALIGN, Some("justify".into()));
        marks.set(12..15, kind::BOLD, Some(true.into()));
        marks.set(16..20, kind::FOOTNOTE, Some("A note.".into()));
        let sheet = StyleSheet::standard(&Fonts::default());
        let page = PageSetup {
            size: [300.0, 400.0],
            margins: [36.0; 4],
            ..PageSetup::default()
        };
        let docs = [PrintDoc {
            title: "One",
            text: &text,
            marks: &marks,
            new_page: false,
        }];
        let pdf = typeset(&docs, &sheet, &page, &Embeds::NONE, "Book", "Me");
        assert!(pdf.pages.len() >= 2, "{} pages", pdf.pages.len());
        assert_eq!(pdf.bookmarks[0].title, "Chapter One");
        assert!(!pdf.fonts.is_empty());
        // Every glyph sits inside the page's text area across.
        for page_out in &pdf.pages {
            for item in &page_out.items {
                if let PdfItem::Text { glyphs, .. } = item {
                    for glyph in glyphs {
                        assert!(glyph.x >= 0.0 && glyph.x < 300.0, "{}", glyph.x);
                    }
                }
            }
        }
        // The notes come at the end.
        let last = pdf.pages.last().unwrap();
        let words: String = last
            .items
            .iter()
            .filter_map(|item| match item {
                PdfItem::Text { glyphs, .. } => Some(
                    glyphs
                        .iter()
                        .map(|glyph| glyph.text.as_str())
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect();
        assert!(words.contains("Notes"), "{words}");
        let bytes = sue_io::pdf::write_pdf(&pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7"));
    }

    #[test]
    fn contents_print_the_headings_with_the_pages_they_land_on() {
        let filler = "Words to fill the page out to its foot. ".repeat(60);
        let text = format!(
            "{}\nChapter One\n{filler}\nChapter Two\n{filler}\n",
            sue_model::OBJECT
        );
        let mut marks = TextMarks::new();
        let id = EntityId::new();
        marks.set(
            0..sue_model::OBJECT.len_utf8(),
            kind::EMBED,
            Some(Value::Entity(id)),
        );
        for heading in ["Chapter One", "Chapter Two"] {
            let at = text.find(heading).unwrap();
            marks.set_paragraphs(&text, at..at, kind::STYLE, Some("heading1".into()));
        }
        let sheet = StyleSheet::standard(&Fonts::default());
        let page = PageSetup {
            size: [300.0, 400.0],
            margins: [36.0; 4],
            ..PageSetup::default()
        };
        let docs = [PrintDoc {
            title: "",
            text: &text,
            marks: &marks,
            new_page: false,
        }];
        let contents = |known: EntityId| (known == id).then(Contents::default);
        let embeds = Embeds {
            contents: &contents,
            ..Embeds::NONE
        };
        let pdf = typeset(&docs, &sheet, &page, &embeds, "Book", "Me");
        let words = |page: &PdfPage| -> String {
            page.items
                .iter()
                .filter_map(|item| match item {
                    PdfItem::Text { glyphs, .. } => Some(
                        glyphs
                            .iter()
                            .map(|glyph| glyph.text.as_str())
                            .collect::<String>(),
                    ),
                    _ => None,
                })
                .collect()
        };
        let first = words(&pdf.pages[0]);
        let two = pdf
            .bookmarks
            .iter()
            .find(|mark| mark.title == "Chapter Two")
            .unwrap();
        assert!(two.page > 0, "chapter two is further on");
        assert!(first.contains("Contents"), "{first}");
        assert!(
            first.contains(&format!("ChapterTwo{}", two.page + 1))
                || first.contains(&format!("Chapter Two{}", two.page + 1)),
            "{first}"
        );
    }
}
