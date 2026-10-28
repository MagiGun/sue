//! Compiling: the draft (or any folder) put together as one book, with
//! folder titles as chapter headings, breaks between scenes and a title
//! page, then written in any publishing format Sue has: PDF typeset with
//! the project's own styles and fonts, Word, OpenDocument, EPUB, HTML, RTF,
//! Markdown or plain text.

use std::collections::HashMap;

use sue_core::{Arena, ComponentKind, EntityId, Value};
use sue_io::publish::{Book, Figure, PublishStyle, Section};
use sue_model::manuscript;
use sue_model::{TextMarks, mark_kind as kind, picture};
use sue_type::typeset::{PrintDoc, typeset};
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
        return sue_paint::composite::preview(arena, id, [max, max]);
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

/// Compiles with `options` into the bytes of a file.
///
/// # Errors
/// If the format is unknown or a PDF font cannot be read.
pub fn compile(arena: &Arena, options: &Options, look: &Look) -> Result<Vec<u8>, String> {
    let sections = sections(arena, options, look);
    if sections.is_empty() {
        return Err("there is nothing to compile: every document is left out".to_owned());
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
        let pdf = typeset(
            &docs,
            &look.sheet,
            &look.page,
            &pictures,
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
        sections,
        styles: publish_styles(&look.sheet),
        page: look.page.size,
        margins: look.page.margins,
        page_numbers: look.page.numbers,
    };
    use sue_io::publish as p;
    Ok(match options.format.as_str() {
        "docx" => p::to_docx(&book),
        "odt" => p::to_odt(&book),
        "epub" => p::to_epub(&book),
        "html" => p::to_html(&book).into_bytes(),
        "rtf" => p::to_rtf(&book).into_bytes(),
        "md" => p::to_markdown(&book).into_bytes(),
        "txt" => p::to_text(&book).into_bytes(),
        other => return Err(format!("Sue Writer cannot write {other} files")),
    })
}

/// A document file read into rich text, by its extension: Word, HTML,
/// Markdown or plain text.
///
/// # Errors
/// If the file cannot be read as that kind.
pub fn import(name: &str, bytes: &[u8]) -> Result<(String, TextMarks), String> {
    let ext = std::path::Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let text = || {
        String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{FEFF}')
            .replace("\r\n", "\n")
    };
    match ext.as_str() {
        "docx" => sue_io::publish::from_docx(bytes),
        "html" | "htm" | "xhtml" => Ok(sue_io::publish::from_html(&text())),
        "md" | "markdown" => Ok(sue_io::publish::from_markdown(&text())),
        "txt" | "text" | "" | "fountain" => {
            let mut text = text();
            sue_model::ensure_final_newline(&mut text);
            Ok((text, TextMarks::new()))
        }
        other => Err(format!("Sue Writer cannot read .{other} files yet")),
    }
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
        let (batch, root) = crate::project::create("novel", "Book", &sue_type::Fonts::default());
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
        }
        // Leaving everything out leaves nothing to compile.
        for child in manuscript::children(&arena, draft) {
            let step = sue_core::txn::SetProperty::new(child, manuscript::INCLUDE, false);
            history.apply(&mut arena, step).unwrap();
        }
        options.title_page = false;
        assert!(compile(&arena, &options, &look).is_err());
        let (text, marks) = import("a.md", b"# Hi\n\nthere").unwrap();
        assert_eq!(text, "Hi\nthere\n");
        assert!(marks.paragraph_value(&text, 0, kind::STYLE).is_some());
    }
}
