//! Compiling: the draft (or any folder) put together as one book, with
//! folder titles as chapter headings, breaks between scenes and a title
//! page, then written in any publishing format Sue has: PDF typeset with
//! the project's own styles and fonts, Word, OpenDocument, EPUB, HTML, RTF,
//! Markdown or plain text.

use std::collections::HashMap;

use sue_core::{Arena, ComponentKind, Entity, EntityId, FormatHandler, Properties, Value};
use sue_io::DelimitedFormat;
use sue_io::publish::{Book, Figure, PublishStyle, PublishTable, Section};
use sue_io::text_ops::{Endings, TextOptions, tidy};
use sue_model::contents::Contents;
use sue_model::manuscript;
use sue_model::{TextMarks, mark_kind as kind, picture};
use sue_type::tables::TableSource;
use sue_type::typeset::{Embeds, PrintDoc, typeset};
use sue_type::{PageSetup, StyleSheet};

/// A format a project compiles to: key, name, file extension.
pub const FORMATS: &[(&str, &str, &str)] = &[
    ("pdf", "PDF", "pdf"),
    ("docx", "Word document", "docx"),
    ("odt", "OpenDocument text", "odt"),
    ("epub", "EPUB e-book", "epub"),
    ("html", "Web page", "html"),
    ("rtf", "Rich Text Format", "rtf"),
    ("md", "Markdown", "md"),
    ("txt", "Plain text", "txt"),
];

/// The name of format `key` (one of [`FORMATS`]) in the language in use.
#[must_use]
pub fn format_name(key: &str) -> String {
    match key {
        "docx" => tr!("word-document"),
        "odt" => tr!("opendocument-text"),
        "epub" => tr!("epub-e-book"),
        "html" => tr!("web-page"),
        "rtf" => tr!("rich-text-format"),
        "md" => tr!("markdown"),
        "txt" => tr!("plain-text"),
        other => FORMATS
            .iter()
            .find(|(format, ..)| *format == other)
            .map_or_else(|| other.to_owned(), |(_, name, _)| (*name).to_owned()),
    }
}

/// Which titles go into the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Titles {
    /// Folders become chapter headings.
    Folders,
    /// Folders and documents both.
    All,
    /// None: the text as written.
    None,
}

/// What goes between two documents in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Separator {
    /// A centered `#`, as manuscripts mark a scene break.
    SceneBreak,
    /// Nothing.
    None,
    /// A new page.
    Page,
}

/// How to compile.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The format, a key of [`FORMATS`].
    pub format: String,
    /// What to compile from (the draft, else any folder or document).
    pub from: EntityId,
    /// Titles in the text.
    pub titles: Titles,
    /// Between documents.
    pub separator: Separator,
    /// A title page first.
    pub title_page: bool,
    /// Each top folder starts a new page.
    pub chapter_pages: bool,
    /// How plain text is tidied on its way out: its line endings, spaces
    /// at line ends and tabs. Markdown takes the line endings only (two
    /// spaces at a line end are a line break there, and tabs carry lists
    /// and code); the other formats none of it.
    pub text: TextOptions,
}

/// The tidying a format takes of `text`: all of it for plain text, the
/// line endings for Markdown, none for the rest.
#[must_use]
pub fn tidying(format: &str, text: &TextOptions) -> Option<TextOptions> {
    match format {
        "txt" => Some(*text),
        "md" => Some(TextOptions {
            endings: text.endings,
            trim_trailing: false,
            tab_spaces: 0,
        }),
        _ => None,
    }
}

/// The line endings of this system: CRLF on Windows, LF elsewhere.
#[must_use]
pub fn system_endings() -> Endings {
    if sue_platform::crlf_line_ends() {
        Endings::CrLf
    } else {
        Endings::Lf
    }
}

/// Plain text tidied as the system writes it: its own line endings, the
/// spaces and tabs kept.
#[must_use]
pub fn system_text() -> TextOptions {
    TextOptions {
        endings: system_endings(),
        trim_trailing: false,
        tab_spaces: 0,
    }
}

/// The project's look: its style sheet and page.
pub struct Look {
    /// The styles.
    pub sheet: StyleSheet,
    /// The page.
    pub page: PageSetup,
    /// The title.
    pub title: String,
    /// The author.
    pub author: String,
}

/// Every section of the book, in order, with the folder titles turned into
/// heading paragraphs.
#[must_use]
pub fn sections(arena: &Arena, options: &Options, look: &Look) -> Vec<Section> {
    let mut out = Vec::new();
    if options.title_page {
        let author = if look.author.is_empty() {
            String::new()
        } else {
            look.author.clone()
        };
        let (text, marks) =
            crate::project::rich(&[("title", look.title.clone()), ("subtitle", author)]);
        let mut marks = marks;
        marks.set_paragraphs(
            &text,
            0..text.len(),
            kind::ALIGN,
            Some(Value::Text("center".into())),
        );
        marks.set_paragraphs(&text, 0..1, kind::SPACE_BEFORE, Some(Value::Float(200.0)));
        out.push(Section {
            title: look.title.clone(),
            depth: 0,
            text,
            marks,
            new_page: false,
        });
    }
    let from = arena.get_by_id(options.from).ok();
    let items: Vec<(EntityId, usize)> = match from {
        Some(entity) if manuscript::is_document(entity) => vec![(options.from, 0)],
        _ => manuscript::publishable(arena, options.from),
    };
    let mut last_was_text = false;
    for (index, (id, depth)) in items.iter().enumerate() {
        let Ok(entity) = arena.get_by_id(*id) else {
            continue;
        };
        let name = entity.name().to_owned();
        let is_text = manuscript::is_document(entity);
        let heading = match depth {
            0 if look.sheet.has("chapter") => "chapter".to_owned(),
            depth => format!("heading{}", (depth + 1).min(4)),
        };
        let new_page = options.chapter_pages && *depth == 0 && (index > 0 || options.title_page);
        if !is_text {
            last_was_text = false;
            if options.titles != Titles::None {
                let (text, marks) = crate::project::rich(&[(heading.as_str(), name.clone())]);
                out.push(Section {
                    title: name,
                    depth: *depth,
                    text,
                    marks,
                    new_page,
                });
            } else {
                out.push(Section {
                    title: name,
                    depth: *depth,
                    text: String::new(),
                    marks: TextMarks::new(),
                    new_page,
                });
            }
            continue;
        }
        let mut text = entity
            .string_buffer()
            .map(|buffer| buffer.as_str().to_owned())
            .unwrap_or_default();
        let mut marks = entity.ext::<TextMarks>().cloned().unwrap_or_default();
        sue_model::ensure_final_newline(&mut text);
        if options.titles == Titles::All {
            let (title, title_marks) = crate::project::rich(&[(heading.as_str(), name.clone())]);
            let mut joined = title_marks;
            joined.insert_text(&title, title.len(), &text, Some(&marks));
            text = format!("{title}{text}");
            marks = joined;
        }
        if last_was_text {
            match options.separator {
                Separator::SceneBreak => {
                    let (gap, gap_marks) = crate::project::rich(&[("break", "#".to_owned())]);
                    let mut joined = gap_marks;
                    joined.set_paragraphs(
                        &gap,
                        0..1,
                        kind::ALIGN,
                        Some(Value::Text("center".into())),
                    );
                    joined.insert_text(&gap, gap.len(), &text, Some(&marks));
                    text = format!("{gap}{text}");
                    marks = joined;
                }
                Separator::Page => {
                    marks.set_paragraphs(&text, 0..1, kind::PAGE_BREAK, Some(Value::Bool(true)));
                }
                Separator::None => {}
            }
        }
        last_was_text = true;
        out.push(Section {
            title: name,
            depth: *depth,
            text,
            marks,
            new_page,
        });
    }
    out.retain(|section| !section.text.is_empty());
    out
}

/// The styles of `sheet` as the publishing formats need them.
#[must_use]
pub fn publish_styles(sheet: &StyleSheet) -> Vec<PublishStyle> {
    sheet
        .styles
        .iter()
        .map(|style| PublishStyle {
            key: style.key.clone(),
            name: style.name.clone(),
            font: style
                .font
                .clone()
                .unwrap_or_else(|| sheet.body_font.clone()),
            size: style.size,
            bold: style.bold,
            italic: style.italic,
            align: style.align.name().to_owned(),
            indent: style.indent,
            indent_right: style.indent_right,
            first_indent: style.first_indent,
            space_before: style.space_before,
            space_after: style.space_after,
            line_spacing: style.line_spacing,
            outline: style.outline,
            page_break: style.page_break,
            keep_with_next: style.keep_with_next,
            mono: style.font.as_deref() == Some(sheet.mono_font.as_str()),
        })
        .collect()
}

/// The pixels of a picture or image entity, as straight RGBA.
#[must_use]
pub fn picture_pixels(arena: &Arena, id: EntityId, max: u32) -> Option<(Vec<u8>, [u32; 2])> {
    let entity = arena.get_by_id(id).ok()?;
    if picture::is_picture(entity) {
        return sue_picture::composite::preview(arena, id, [max, max]);
    }
    let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
    let (width, height) = (raster.width(), raster.height());
    let bytes = raster.bytes()?;
    let pixels = match raster.format() {
        sue_core::PixelFormat::Rgba8 => bytes.to_vec(),
        sue_core::PixelFormat::Gray8 => bytes.iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        _ => return None,
    };
    if width.max(height) <= max {
        return Some((pixels, [width, height]));
    }
    // Shrink by nearest sampling; good enough for a page.
    let scale = max as f32 / width.max(height) as f32;
    let (w, h) = (
        ((width as f32 * scale) as u32).max(1),
        ((height as f32 * scale) as u32).max(1),
    );
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f32 / scale) as u32).min(width - 1);
            let sy = ((y as f32 / scale) as u32).min(height - 1);
            let at = ((sy * width + sx) * 4) as usize;
            out.extend_from_slice(&pixels[at..at + 4]);
        }
    }
    Some((out, [w, h]))
}

/// The pictures `sections` show, as PNG.
fn figures(arena: &Arena, sections: &[Section]) -> HashMap<EntityId, Figure> {
    let mut out = HashMap::new();
    for section in sections {
        for mark in section.marks.marks() {
            if mark.kind == kind::EMBED
                && let Value::Entity(id) = mark.value
                && !out.contains_key(&id)
                && let Some((pixels, [width, height])) = picture_pixels(arena, id, 2400)
                && let Ok(png) = sue_io::encode_png_rgba(width, height, &pixels)
            {
                out.insert(id, Figure { png, width, height });
            }
        }
    }
    out
}

/// The tables the sections show, for the publishing formats.
fn tables(arena: &Arena, sections: &[Section]) -> HashMap<EntityId, PublishTable> {
    let mut out = HashMap::new();
    for section in sections {
        for mark in section.marks.marks() {
            if mark.kind == kind::EMBED
                && let Value::Entity(id) = mark.value
                && !out.contains_key(&id)
                && let Some(table) = TableSource::read(arena, id)
            {
                out.insert(
                    id,
                    PublishTable {
                        cells: table.cells,
                        header_rows: table.header_rows,
                        shares: table.shares,
                    },
                );
            }
        }
    }
    out
}

/// The tables of contents the sections show, for the publishing formats.
fn contents(arena: &Arena, sections: &[Section]) -> HashMap<EntityId, Contents> {
    let mut out = HashMap::new();
    for section in sections {
        for mark in section.marks.marks() {
            if mark.kind == kind::EMBED
                && let Value::Entity(id) = mark.value
                && let Some(contents) = arena.get_by_id(id).ok().and_then(Contents::read)
            {
                out.insert(id, contents);
            }
        }
    }
    out
}

/// The comment threads the sections' words carry, for the formats that
/// keep comments.
fn comments(arena: &Arena, sections: &[Section]) -> HashMap<EntityId, sue_model::comments::Thread> {
    let mut out = HashMap::new();
    for section in sections {
        for mark in section.marks.marks() {
            if mark.kind == kind::COMMENT
                && let Value::Entity(id) = mark.value
                && let Some(thread) = sue_model::comments::thread(arena, id)
            {
                out.insert(id, thread);
            }
        }
    }
    out
}

/// Compiles with `options` into the bytes of a file.
///
/// # Errors
/// If the format is unknown or a PDF font cannot be read.
pub fn compile(arena: &Arena, options: &Options, look: &Look) -> Result<Vec<u8>, String> {
    let mut sections = sections(arena, options, look);
    if sections.is_empty() {
        return Err(tr!("there-is-nothing-to-export-every-document-left"));
    }
    // Word and web pages show tracked changes; the rest print the text as
    // it is with every change accepted.
    if !matches!(options.format.as_str(), "docx" | "html") {
        for section in &mut sections {
            let (text, marks) = sue_model::changes::accepted(&section.text, &section.marks);
            section.text = text;
            section.marks = marks;
        }
    }
    if options.format == "pdf" {
        let docs: Vec<PrintDoc<'_>> = sections
            .iter()
            .map(|section| PrintDoc {
                title: &section.title,
                text: &section.text,
                marks: &section.marks,
                new_page: section.new_page,
            })
            .collect();
        let pictures = |id: EntityId| {
            let (pixels, [width, height]) = picture_pixels(arena, id, 2400)?;
            let rgb = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|px| {
                    // Over white paper.
                    let a = u32::from(px[3]);
                    [0, 1, 2].map(|c| ((u32::from(px[c]) * a + 255 * (255 - a)) / 255) as u8)
                })
                .collect();
            Some(sue_io::pdf::PdfImage { width, height, rgb })
        };
        let contents = |id: EntityId| arena.get_by_id(id).ok().and_then(Contents::read);
        let embeds = Embeds {
            figures: &pictures,
            tables: &|id| TableSource::read(arena, id),
            contents: &contents,
        };
        let pdf = typeset(
            &docs,
            &look.sheet,
            &look.page,
            &embeds,
            &look.title,
            &look.author,
        );
        return sue_io::pdf::write_pdf(&pdf);
    }
    let book = Book {
        title: look.title.clone(),
        author: look.author.clone(),
        language: "en".to_owned(),
        identifier: format!("urn:sue:{}", options.from),
        figures: figures(arena, &sections),
        tables: tables(arena, &sections),
        contents: contents(arena, &sections),
        comments: comments(arena, &sections),
        sections,
        styles: publish_styles(&look.sheet),
        page: look.page.size,
        margins: look.page.margins,
        header: look.page.header.clone(),
        footer: look.page.footer.clone(),
        different_first: look.page.different_first,
        date: look.page.date.clone(),
    };
    use sue_io::publish as p;
    Ok(match options.format.as_str() {
        "docx" => p::to_docx(&book),
        "odt" => p::to_odt(&book),
        "epub" => p::to_epub(&book),
        "html" => p::to_html(&book).into_bytes(),
        "rtf" => p::to_rtf(&book).into_bytes(),
        "md" | "txt" => {
            let text = if options.format == "md" {
                p::to_markdown(&book)
            } else {
                p::to_text(&book)
            };
            match tidying(&options.format, &options.text) {
                Some(tidied) => tidy(&text, &tidied),
                None => text,
            }
            .into_bytes()
        }
        other => return Err(tr!("sue-writer-cannot-write-files", other = other)),
    })
}

/// A document file read into rich text, by its extension: Word or
/// OpenDocument (their tables as table entities), HTML, Markdown or plain
/// text.
///
/// # Errors
/// If the file cannot be read as that kind.
pub fn import(name: &str, bytes: &[u8]) -> Result<sue_io::publish::Imported, String> {
    let ext = std::path::Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let text = || {
        String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{FEFF}')
            .replace("\r\n", "\n")
    };
    let plain = |(text, marks): (String, TextMarks)| sue_io::publish::Imported {
        text,
        marks,
        entities: Vec::new(),
    };
    match ext.as_str() {
        // Templates are the same files under another name.
        "docx" | "dotx" | "docm" => sue_io::publish::read_docx(bytes),
        "odt" | "ott" => sue_io::publish::read_odt(bytes),
        "pdf" => sue_io::publish::from_pdf(bytes).map(plain),
        "html" | "htm" | "xhtml" => Ok(plain(sue_io::publish::from_html(&text()))),
        "md" | "markdown" => Ok(plain(sue_io::publish::from_markdown(&text()))),
        "txt" | "text" | "" | "fountain" => {
            let mut text = text();
            sue_model::ensure_final_newline(&mut text);
            Ok(plain((text, TextMarks::new())))
        }
        "csv" | "tsv" | "tab" => import_table(name, bytes, true),
        other => Err(tr!("sue-writer-cannot-read-files-yet", other = other)),
    }
}

/// `true` for a file Sue Writer reads as a table: `.csv`, `.tsv` or
/// `.tab`, any case.
#[must_use]
pub fn is_table_file(name: &str) -> bool {
    DelimitedFormat::for_name(name).is_some()
}

/// `true` for a tab-separated file name (`.tsv` or `.tab`).
#[must_use]
pub fn is_tsv(name: &str) -> bool {
    DelimitedFormat::for_name(name).is_some_and(|format| format.info().id == "tsv")
}

/// A table file read into a table entity, through the registered
/// [`DelimitedFormat`]: each cell holds its text as the file wrote it,
/// and the first row is a heading row when `heading` says so.
///
/// # Errors
/// If `name` is not a table file, the file is not UTF-8 text, or it holds
/// no rows.
pub fn read_table(name: &str, bytes: &[u8], heading: bool) -> Result<Entity, String> {
    let Some(format) = DelimitedFormat::for_name(name) else {
        let ext = std::path::Path::new(name)
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(tr!("sue-writer-cannot-read-files-yet", other = ext));
    };
    let rows = format
        .read_rows(bytes)
        .map_err(|_| tr!("the-table-file-is-not-text"))?;
    if rows.iter().all(|row| row.iter().all(String::is_empty)) {
        return Err(tr!("the-table-file-is-empty"));
    }
    Ok(sue_model::text_table(&rows, u32::from(heading), &[]))
}

/// A table file read as a document that holds the one table, as Word and
/// OpenDocument files bring theirs (the table entity in
/// [`Imported::entities`](sue_io::publish::Imported), set in the text).
///
/// # Errors
/// As [`read_table`].
pub fn import_table(
    name: &str,
    bytes: &[u8],
    heading: bool,
) -> Result<sue_io::publish::Imported, String> {
    let table = read_table(name, bytes, heading)?;
    let mut text = String::new();
    text.push(sue_model::OBJECT);
    text.push('\n');
    let mut marks = TextMarks::new();
    marks.set(
        0..sue_model::OBJECT.len_utf8(),
        kind::EMBED,
        Some(Value::Entity(table.id())),
    );
    Ok(sue_io::publish::Imported {
        text,
        marks,
        entities: vec![table],
    })
}

/// Table `table` written as the table file `name`: TSV for `.tsv` and
/// `.tab`, else CSV, through the registered [`DelimitedFormat`]. Every
/// row, the heading rows too, is a line of the file.
///
/// # Errors
/// If `table` is not a table in `arena`.
pub fn export_table(arena: &Arena, table: EntityId, name: &str) -> Result<Vec<u8>, String> {
    let format = DelimitedFormat::for_name(name).unwrap_or_else(DelimitedFormat::csv);
    format
        .export(arena, &[table], &Properties::new())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::UndoStack;
    use sue_model::manuscript::role;

    #[test]
    fn a_novel_compiles_to_every_format() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (batch, root) =
            crate::project::create_built_in("novel", "Book", &sue_type::Fonts::default()).unwrap();
        history.apply(&mut arena, batch).unwrap();
        let draft = manuscript::folder(&arena, root, role::DRAFT).unwrap();
        // Write a little in each scene.
        for (id, _) in manuscript::walk(&arena, draft) {
            if manuscript::is_document(arena.get_by_id(id).unwrap()) {
                let step = crate::project::replace_all(
                    &arena,
                    id,
                    "It began.\n",
                    TextMarks::new(),
                    "Write",
                )
                .unwrap();
                history.apply(&mut arena, step).unwrap();
            }
        }
        let fonts = sue_type::Fonts::default();
        let look = Look {
            sheet: StyleSheet::manuscript(&fonts),
            page: PageSetup::default(),
            title: "Book".to_owned(),
            author: "Me".to_owned(),
        };
        let mut options = Options {
            format: "md".to_owned(),
            from: draft,
            titles: Titles::Folders,
            separator: Separator::SceneBreak,
            title_page: true,
            chapter_pages: true,
            text: system_text(),
        };
        let sections = sections(&arena, &options, &look);
        let titles: Vec<&str> = sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["Book", "Chapter 1", "Scene 1", "Chapter 2", "Scene 1"]
        );
        assert!(sections[1].new_page);
        let markdown = String::from_utf8(compile(&arena, &options, &look).unwrap()).unwrap();
        assert!(markdown.contains("# Chapter 1"), "{markdown}");
        assert!(markdown.contains("It began."));
        for (format, _, _) in FORMATS {
            options.format = (*format).to_owned();
            let bytes = compile(&arena, &options, &look).unwrap();
            assert!(bytes.len() > 40, "{format}");
            // What Writer saves as Word, OpenDocument or PDF opens again,
            // the text and the headings kept.
            if matches!(*format, "docx" | "odt" | "pdf") {
                let read = import(&format!("Book.{format}"), &bytes).unwrap();
                assert!(read.text.contains("It began."), "{format}: {}", read.text);
                let chapter = read
                    .text
                    .lines()
                    .position(|line| line == "Chapter 1")
                    .unwrap_or_else(|| panic!("{format}: {}", read.text));
                assert!(
                    read.marks
                        .paragraph_value(&read.text, chapter, kind::STYLE)
                        .is_some(),
                    "{format}: Chapter 1 is a heading"
                );
            }
        }
        // Leaving everything out leaves nothing to compile.
        for child in manuscript::children(&arena, draft) {
            let step = sue_core::txn::SetProperty::new(child, manuscript::INCLUDE, false);
            history.apply(&mut arena, step).unwrap();
        }
        options.title_page = false;
        assert!(compile(&arena, &options, &look).is_err());
        let sue_io::publish::Imported { text, marks, .. } =
            import("a.md", b"# Hi\n\nthere").unwrap();
        assert_eq!(text, "Hi\nthere\n");
        assert!(marks.paragraph_value(&text, 0, kind::STYLE).is_some());
    }

    /// One loose document holding `text`, and the options to export it.
    fn loose(text: &str) -> (Arena, Options, Look) {
        let mut arena = Arena::new();
        let doc = crate::project::document_entity("Notes", text.to_owned(), TextMarks::new());
        let from = doc.id();
        arena.insert(doc);
        let options = Options {
            format: "txt".to_owned(),
            from,
            titles: Titles::None,
            separator: Separator::None,
            title_page: false,
            chapter_pages: false,
            text: TextOptions {
                endings: Endings::CrLf,
                trim_trailing: true,
                tab_spaces: 4,
            },
        };
        let fonts = sue_type::Fonts::default();
        let look = Look {
            sheet: StyleSheet::manuscript(&fonts),
            page: PageSetup::default(),
            title: "Notes".to_owned(),
            author: String::new(),
        };
        (arena, options, look)
    }

    #[test]
    fn plain_text_is_tidied_and_markdown_keeps_its_spaces_and_tabs() {
        let (arena, mut options, look) = loose("one  \n\ttwo\n");
        let text = String::from_utf8(compile(&arena, &options, &look).unwrap()).unwrap();
        assert!(text.contains("one\r\n    two\r\n"), "{text:?}");
        assert!(!text.contains('\t') && !text.contains(" \r\n"), "{text:?}");
        assert!(
            !text.replace("\r\n", "").contains('\n'),
            "every line ends with CRLF: {text:?}"
        );
        // Markdown: the line endings only.
        options.format = "md".to_owned();
        let markdown = String::from_utf8(compile(&arena, &options, &look).unwrap()).unwrap();
        assert!(markdown.contains("\r\n"), "{markdown:?}");
        assert!(!markdown.replace("\r\n", "").contains('\n'), "{markdown:?}");
        assert_eq!(
            tidying("md", &options.text),
            Some(TextOptions {
                endings: Endings::CrLf,
                trim_trailing: false,
                tab_spaces: 0,
            })
        );
        // LF wanted, LF written.
        options.format = "txt".to_owned();
        options.text.endings = Endings::Lf;
        let text = String::from_utf8(compile(&arena, &options, &look).unwrap()).unwrap();
        assert!(!text.contains('\r'), "{text:?}");
        // The other formats are not touched.
        assert_eq!(tidying("docx", &options.text), None);
        assert_eq!(tidying("html", &options.text), None);
        assert_eq!(system_text().tab_spaces, 0);
        assert!(!system_text().trim_trailing);
    }

    #[test]
    fn a_table_file_becomes_a_document_with_its_table_and_goes_back_out() {
        let file = "Part,Code\r\nBolt,007\r\n\"M8, zinc\",2\r\n";
        let read = import_table("parts.csv", file.as_bytes(), true).unwrap();
        assert_eq!(read.text, format!("{}\n", sue_model::OBJECT));
        assert_eq!(read.entities.len(), 1);
        let table = &read.entities[0];
        assert_eq!(table.class(), sue_model::TEXT_TABLE);
        assert_eq!(
            read.marks.value_at(0, kind::EMBED),
            Some(&Value::Entity(table.id()))
        );
        assert_eq!(table.property(sue_model::HEADER_ROWS), Some(&Value::Int(1)));
        let cells = table.ext::<sue_model::Table>().unwrap();
        assert_eq!(cells.size(), (3, 2));
        assert_eq!(cells.text(1, 1), "007");
        assert_eq!(cells.text(2, 0), "M8, zinc");
        // Without the heading row, no heading.
        let plain = read_table("parts.CSV", file.as_bytes(), false).unwrap();
        assert_eq!(plain.property(sue_model::HEADER_ROWS), None);
        // Back out as CSV, byte for byte; as TSV by the name.
        let mut arena = Arena::new();
        let id = table.id();
        arena.insert(table.clone());
        assert_eq!(
            export_table(&arena, id, "out.csv").unwrap(),
            file.as_bytes()
        );
        assert_eq!(
            String::from_utf8(export_table(&arena, id, "out.tsv").unwrap()).unwrap(),
            "Part\tCode\r\nBolt\t007\r\nM8, zinc\t2\r\n"
        );
        // The general import reads one too, its first row a heading.
        let tsv = import("t.tab", b"a\tb\n").unwrap();
        assert_eq!(tsv.entities.len(), 1);
        assert!(is_table_file("x.TSV") && !is_table_file("x.txt"));
        assert!(read_table("empty.csv", b"", true).is_err());
        assert!(read_table("bad.csv", &[0xff, 0xfe], true).is_err());
        assert!(read_table("x.txt", b"a", true).is_err());
    }
}
