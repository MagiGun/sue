//! EPUB 3 e-books, readable on every e-reader: one XHTML file per section
//! that starts a page, a navigation document and an NCX for older readers,
//! the paragraph styles as CSS, pictures and footnotes.

use std::fmt::Write as _;

use super::Book;
use super::html::{body_html, css, notes_html};
use super::xml::escape;
use crate::zip::ZipWriter;

/// The book as an EPUB 3 file.
#[must_use]
pub fn to_epub(book: &Book) -> Vec<u8> {
    // Sections that start a page start a file; the rest join the one
    // before.
    let mut chapters: Vec<(String, usize, Vec<usize>)> = Vec::new();
    for (index, section) in book.sections.iter().enumerate() {
        if chapters.is_empty() || section.new_page || section.depth == 0 {
            chapters.push((section.title.clone(), section.depth, vec![index]));
        } else if let Some((_, _, sections)) = chapters.last_mut() {
            sections.push(index);
        }
    }
    let mut figure_names = std::collections::HashMap::new();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    for (index, (id, figure)) in book.figures.iter().enumerate() {
        let name = format!("images/figure{}.png", index + 1);
        figure_names.insert(*id, name.clone());
        images.push((name, figure.png.clone()));
    }
    let figure = |id: sue_core::EntityId| figure_names.get(&id).cloned();
    let mut files = Vec::new();
    for (index, (title, _, sections)) in chapters.iter().enumerate() {
        let mut notes = Vec::new();
        let mut body = String::new();
        for section in sections {
            let section = &book.sections[*section];
            body.push_str(&body_html(
                book,
                &section.text,
                &section.marks,
                &mut notes,
                &figure,
            ));
        }
        body.push_str(&notes_html(&notes));
        let xhtml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"{0}\" lang=\"{0}\">\n<head>\n<meta charset=\"utf-8\"/>\n<title>{1}</title>\n<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n</head>\n<body>\n{2}</body>\n</html>\n",
            escape(&book.language),
            escape(title),
            body
        );
        files.push((format!("chapter{:03}.xhtml", index + 1), xhtml));
    }
    // The table of contents, nested by depth.
    let mut nav = String::new();
    let mut depth = 0;
    let mut first = true;
    for (index, (title, level, _)) in chapters.iter().enumerate() {
        let level = (*level).min(depth + 1);
        if first {
            nav.push_str("<ol>");
            first = false;
        } else if level > depth {
            nav.push_str("<ol>");
        } else {
            nav.push_str("</li>");
            for _ in level..depth {
                nav.push_str("</ol></li>");
            }
        }
        depth = level;
        let _ = write!(
            nav,
            "<li><a href=\"chapter{:03}.xhtml\">{}</a>",
            index + 1,
            escape(if title.is_empty() { "Untitled" } else { title })
        );
    }
    if !first {
        nav.push_str("</li>");
        for _ in 0..depth {
            nav.push_str("</ol></li>");
        }
        nav.push_str("</ol>");
    }
    let nav_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"{0}\" lang=\"{0}\">\n<head><meta charset=\"utf-8\"/><title>{1}</title></head>\n<body><nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{2}</nav></body>\n</html>\n",
        escape(&book.language),
        escape(&book.title),
        nav
    );
    let mut ncx_points = String::new();
    for (index, (title, _, _)) in chapters.iter().enumerate() {
        let _ = write!(
            ncx_points,
            "<navPoint id=\"nav{0}\" playOrder=\"{0}\"><navLabel><text>{1}</text></navLabel><content src=\"chapter{0:03}.xhtml\"/></navPoint>",
            index + 1,
            escape(if title.is_empty() { "Untitled" } else { title })
        );
    }
    let ncx = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><head><meta name=\"dtb:uid\" content=\"{}\"/></head><docTitle><text>{}</text></docTitle><navMap>{ncx_points}</navMap></ncx>",
        escape(&book.identifier),
        escape(&book.title)
    );
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/><item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>",
    );
    let mut spine = String::new();
    for (index, (name, _)) in files.iter().enumerate() {
        let _ = write!(
            manifest,
            "<item id=\"c{}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>",
            index + 1
        );
        let _ = write!(spine, "<itemref idref=\"c{}\"/>", index + 1);
    }
    for (index, (name, _)) in images.iter().enumerate() {
        let _ = write!(
            manifest,
            "<item id=\"img{}\" href=\"{name}\" media-type=\"image/png\"/>",
            index + 1
        );
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"bookid\">{}</dc:identifier><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{}</dc:language><meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta></metadata><manifest>{manifest}</manifest><spine toc=\"ncx\">{spine}</spine></package>",
        escape(&book.identifier),
        escape(&book.title),
        escape(&book.author),
        escape(&book.language)
    );
    let container = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>";
    let mut zip = ZipWriter::new();
    zip.add("mimetype", b"application/epub+zip", true);
    zip.add_text("META-INF/container.xml", container);
    zip.add_text("OEBPS/content.opf", &opf);
    zip.add_text("OEBPS/nav.xhtml", &nav_xhtml);
    zip.add_text("OEBPS/toc.ncx", &ncx);
    zip.add_text("OEBPS/style.css", &css(book));
    for (name, xhtml) in &files {
        zip.add_text(&format!("OEBPS/{name}"), xhtml);
    }
    for (name, bytes) in images {
        zip.add(&format!("OEBPS/{name}"), &bytes, true);
    }
    zip.finish()
}

#[cfg(test)]
mod tests {
    use super::super::Section;
    use super::super::tests::sample;
    use super::super::xml::{XmlEvent, parse};
    use super::*;
    use crate::zip::read_zip;

    #[test]
    fn e_books_have_their_chapters_and_contents() {
        let mut book = sample();
        let mut second = book.sections[0].clone();
        second.title = "Part Two".to_owned();
        second.new_page = true;
        book.sections.push(second);
        book.sections.push(Section {
            title: "Scene".to_owned(),
            depth: 1,
            ..book.sections[0].clone()
        });
        let bytes = to_epub(&book);
        let files = read_zip(&bytes).unwrap();
        assert_eq!(files[0].0, "mimetype");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"OEBPS/chapter002.xhtml"));
        assert!(
            !names.contains(&"OEBPS/chapter003.xhtml"),
            "the scene joins its chapter"
        );
        for (name, bytes) in &files {
            if name.ends_with("xhtml") || name.ends_with(".opf") || name.ends_with(".ncx") {
                let xml = String::from_utf8(bytes.clone()).unwrap();
                let events = parse(&xml);
                let opens = events
                    .iter()
                    .filter(|e| matches!(e, XmlEvent::Open { empty: false, .. }))
                    .count();
                let closes = events
                    .iter()
                    .filter(|e| matches!(e, XmlEvent::Close(_)))
                    .count();
                // <meta charset> in XHTML is self-closed; every open closes.
                assert_eq!(opens, closes, "{name}");
            }
        }
        let nav = files
            .iter()
            .find(|(name, _)| name == "OEBPS/nav.xhtml")
            .unwrap();
        assert!(String::from_utf8_lossy(&nav.1).contains("Part Two"));
    }
}
