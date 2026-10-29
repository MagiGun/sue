//! OpenDocument text (`.odt`), the format of LibreOffice and many others:
//! named paragraph styles (headings with their outline levels, so the
//! navigator and contents work), character formatting, lists, footnotes,
//! links, pictures and page numbers.

use std::fmt::Write as _;

use super::xml::escape;
use super::{Book, PublishStyle, hex, paragraphs};
use crate::zip::ZipWriter;

const NS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\"";

fn style_name(key: &str) -> String {
    match key {
        "body" => "Text_20_body".to_owned(),
        key if key.starts_with("heading") => format!("Heading_20_{}", &key[7..]),
        key => format!(
            "Sue_20_{}",
            key.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        ),
    }
}

fn paragraph_style(style: &PublishStyle) -> String {
    let display = if style.key == "body" {
        "Text body".to_owned()
    } else if style.key.starts_with("heading") {
        format!("Heading {}", &style.key[7..])
    } else {
        style.name.clone()
    };
    let mut out = format!(
        "<style:style style:name=\"{}\" style:display-name=\"{}\" style:family=\"paragraph\"",
        style_name(&style.key),
        escape(&display)
    );
    if let Some(level) = style.outline {
        let _ = write!(out, " style:default-outline-level=\"{level}\"");
    }
    let _ = write!(
        out,
        "><style:paragraph-properties fo:margin-top=\"{}pt\" fo:margin-bottom=\"{}pt\" fo:margin-left=\"{}pt\" fo:margin-right=\"{}pt\" fo:text-indent=\"{}pt\" fo:line-height=\"{}%\" fo:text-align=\"{}\"{}{}/>",
        style.space_before,
        style.space_after,
        style.indent,
        style.indent_right,
        style.first_indent,
        (style.line_spacing * 100.0).round(),
        align(&style.align),
        if style.page_break {
            " fo:break-before=\"page\""
        } else {
            ""
        },
        if style.keep_with_next {
            " fo:keep-with-next=\"always\""
        } else {
            ""
        },
    );
    let _ = write!(
        out,
        "<style:text-properties style:font-name=\"{0}\" fo:font-family=\"{0}\" fo:font-size=\"{1}pt\" fo:font-weight=\"{2}\" fo:font-style=\"{3}\"/></style:style>",
        escape(&style.font),
        style.size,
        if style.bold { "bold" } else { "normal" },
        if style.italic { "italic" } else { "normal" },
    );
    out
}

fn align(align: &str) -> &'static str {
    match align {
        "center" => "center",
        "right" => "end",
        "justify" => "justify",
        _ => "start",
    }
}

/// The book as an OpenDocument text.
#[must_use]
pub fn to_odt(book: &Book) -> Vec<u8> {
    let mut automatic: Vec<String> = Vec::new();
    let mut span_style = |look: &sue_model::CharFormat| -> Option<String> {
        let mut props = String::new();
        if look.bold {
            props.push_str(" fo:font-weight=\"bold\"");
        }
        if look.italic {
            props.push_str(" fo:font-style=\"italic\"");
        }
        if look.underline || look.link.is_some() {
            props.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
        }
        if look.strike {
            props.push_str(" style:text-line-through-style=\"solid\"");
        }
        if look.small_caps {
            props.push_str(" fo:font-variant=\"small-caps\"");
        }
        match look.script {
            1 => props.push_str(" style:text-position=\"super 58%\""),
            -1 => props.push_str(" style:text-position=\"sub 58%\""),
            _ => {}
        }
        if look.code {
            props.push_str(" style:font-name=\"Courier New\" fo:font-family=\"Courier New\"");
        } else if let Some(font) = &look.font {
            let _ = write!(
                props,
                " style:font-name=\"{0}\" fo:font-family=\"{0}\"",
                escape(font)
            );
        }
        if let Some(size) = look.size {
            let _ = write!(props, " fo:font-size=\"{size}pt\"");
        }
        if let Some(color) = look.color {
            let _ = write!(props, " fo:color=\"#{}\"", hex(color));
        }
        if let Some(color) = look.highlight {
            let _ = write!(props, " fo:background-color=\"#{}\"", hex(color));
        }
        if props.is_empty() {
            return None;
        }
        let index = automatic
            .iter()
            .position(|known| *known == props)
            .unwrap_or_else(|| {
                automatic.push(props);
                automatic.len() - 1
            });
        Some(format!("T{}", index + 1))
    };
    let mut body = String::new();
    let mut notes = 0;
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut page_breaks = false;
    let mut tables = 0;
    let mut table_styles = String::new();
    for (index, section) in book.sections.iter().enumerate() {
        let mut open_list: Option<&str> = None;
        for (para_index, para) in paragraphs(&section.text, &section.marks)
            .into_iter()
            .enumerate()
        {
            let list = para
                .list
                .as_deref()
                .map(|list| if list == "number" { "L2" } else { "L1" });
            if open_list != list {
                if open_list.is_some() {
                    body.push_str("</text:list>");
                }
                if let Some(name) = list {
                    let _ = write!(body, "<text:list text:style-name=\"{name}\">");
                }
                open_list = list;
            }
            if let Some(table) = book.table(&para) {
                if open_list.take().is_some() {
                    body.push_str("</text:list>");
                }
                tables += 1;
                let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
                let (styles, xml) = table_xml(table, tables, width);
                table_styles.push_str(&styles);
                body.push_str(&xml);
                continue;
            }
            let mut inner = String::new();
            if let Some(id) = para.embed
                && let Some(figure) = book.figures.get(&id)
            {
                let name = format!("Pictures/figure{}.png", images.len() + 1);
                let width = (book.page[0] - book.margins[1] - book.margins[3])
                    .max(72.0)
                    .min(figure.width as f32 * 0.75);
                let height = width * figure.height as f32 / figure.width.max(1) as f32;
                let _ = write!(
                    inner,
                    "<draw:frame text:anchor-type=\"as-char\" svg:width=\"{width}pt\" svg:height=\"{height}pt\"><draw:image xlink:href=\"{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>"
                );
                images.push((name, figure.png.clone()));
            }
            for (text, look) in &para.runs {
                let mut piece = String::new();
                for (index, part) in text.split('\t').enumerate() {
                    if index > 0 {
                        piece.push_str("<text:tab/>");
                    }
                    piece.push_str(&escape(part));
                }
                if let Some(name) = span_style(look) {
                    piece = format!("<text:span text:style-name=\"{name}\">{piece}</text:span>");
                }
                if let Some(link) = &look.link {
                    piece = format!(
                        "<text:a xlink:type=\"simple\" xlink:href=\"{}\">{piece}</text:a>",
                        escape(link)
                    );
                }
                inner.push_str(&piece);
                if let Some(note) = &look.footnote {
                    notes += 1;
                    let _ = write!(
                        inner,
                        "<text:note text:id=\"note{notes}\" text:note-class=\"footnote\"><text:note-citation>{notes}</text:note-citation><text:note-body><text:p text:style-name=\"Footnote\">{}</text:p></text:note-body></text:note>",
                        escape(note)
                    );
                }
            }
            let new_page = para.page_break || (section.new_page && index > 0 && para_index == 0);
            page_breaks |= new_page;
            let style = if new_page && !book.style(&para.style).page_break {
                format!("{}_PB", style_name(&para.style))
            } else {
                style_name(&para.style)
            };
            let element = match para.outline(book) {
                Some(level) => format!(
                    "<text:h text:style-name=\"{style}\" text:outline-level=\"{level}\">{inner}</text:h>"
                ),
                None => format!("<text:p text:style-name=\"{style}\">{inner}</text:p>"),
            };
            if list.is_some() {
                let _ = write!(body, "<text:list-item>{element}</text:list-item>");
            } else {
                body.push_str(&element);
            }
        }
        if open_list.is_some() {
            body.push_str("</text:list>");
        }
    }
    let mut auto = table_styles;
    for (index, props) in automatic.iter().enumerate() {
        let _ = write!(
            auto,
            "<style:style style:name=\"T{}\" style:family=\"text\"><style:text-properties{props}/></style:style>",
            index + 1
        );
    }
    if page_breaks {
        for style in &book.styles {
            let _ = write!(
                auto,
                "<style:style style:name=\"{0}_PB\" style:family=\"paragraph\" style:parent-style-name=\"{0}\"><style:paragraph-properties fo:break-before=\"page\"/></style:style>",
                style_name(&style.key)
            );
        }
    }
    for (name, numbered) in [("L1", false), ("L2", true)] {
        let _ = write!(auto, "<text:list-style style:name=\"{name}\">");
        for level in 1..=9 {
            let indent = 18.0 * level as f32;
            let place = format!(
                "<style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\"><style:list-level-label-alignment text:label-followed-by=\"listtab\" fo:text-indent=\"-18pt\" fo:margin-left=\"{indent}pt\"/></style:list-level-properties>"
            );
            if numbered {
                let _ = write!(
                    auto,
                    "<text:list-level-style-number text:level=\"{level}\" style:num-suffix=\".\" style:num-format=\"1\">{place}</text:list-level-style-number>"
                );
            } else {
                let _ = write!(
                    auto,
                    "<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"{}\">{place}</text:list-level-style-bullet>",
                    ["\u{2022}", "\u{25E6}", "\u{25AA}"][(level - 1) % 3]
                );
            }
        }
        auto.push_str("</text:list-style>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS}><office:automatic-styles>{auto}</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    let mut styles = String::new();
    for style in &book.styles {
        styles.push_str(&paragraph_style(style));
    }
    styles.push_str("<style:style style:name=\"Footnote\" style:family=\"paragraph\"><style:text-properties fo:font-size=\"10pt\"/></style:style>");
    styles.push_str("<style:style style:name=\"Table_20_Heading\" style:display-name=\"Table Heading\" style:family=\"paragraph\"><style:text-properties fo:font-weight=\"bold\"/></style:style>");
    let [top, right, bottom, left] = book.margins;
    let footer = if book.page_numbers {
        "<style:footer><text:p text:style-name=\"Sue_20_Footer\"><text:page-number text:select-page=\"current\">1</text:page-number></text:p></style:footer>"
    } else {
        ""
    };
    let styles_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS}><office:styles>{styles}<style:style style:name=\"Sue_20_Footer\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"center\"/></style:style></office:styles><office:automatic-styles><style:page-layout style:name=\"Page\"><style:page-layout-properties fo:page-width=\"{}pt\" fo:page-height=\"{}pt\" fo:margin-top=\"{top}pt\" fo:margin-right=\"{right}pt\" fo:margin-bottom=\"{bottom}pt\" fo:margin-left=\"{left}pt\"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"Page\">{footer}</style:master-page></office:master-styles></office:document-styles>",
        book.page[0], book.page[1]
    );
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS}><office:meta><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{}</dc:language><meta:generator>Sue</meta:generator></office:meta></office:document-meta>",
        escape(&book.title),
        escape(&book.author),
        escape(&book.language)
    );
    let mut manifest = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>",
    );
    for (name, _) in &images {
        let _ = write!(
            manifest,
            "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"image/png\"/>"
        );
    }
    manifest.push_str("</manifest:manifest>");
    let mut zip = ZipWriter::new();
    zip.add("mimetype", b"application/vnd.oasis.opendocument.text", true);
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles_xml);
    zip.add_text("meta.xml", &meta);
    zip.add_text("META-INF/manifest.xml", &manifest);
    for (name, bytes) in images {
        zip.add(&name, &bytes, true);
    }
    zip.finish()
}

/// Table number `number` as OpenDocument, `width` points across: its
/// automatic styles (the width of the table and each column, cell borders)
/// and the table, heading rows repeated.
fn table_xml(table: &crate::publish::PublishTable, number: usize, width: f32) -> (String, String) {
    let name = format!("Table{number}");
    let columns = table.columns().max(1);
    let mut styles = format!(
        "<style:style style:name=\"{name}\" style:family=\"table\"><style:table-properties style:width=\"{width}pt\" table:align=\"margins\"/></style:style><style:style style:name=\"{name}.Cell\" style:family=\"table-cell\"><style:table-cell-properties fo:padding=\"3pt\" fo:border=\"0.5pt solid #000000\"/></style:style>"
    );
    for column in 0..columns {
        let _ = write!(
            styles,
            "<style:style style:name=\"{name}.C{column}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{}pt\"/></style:style>",
            width * table.share(column)
        );
    }
    let mut xml = format!("<table:table table:name=\"{name}\" table:style-name=\"{name}\">");
    for column in 0..columns {
        let _ = write!(
            xml,
            "<table:table-column table:style-name=\"{name}.C{column}\"/>"
        );
    }
    for (row, cells) in table.cells.iter().enumerate() {
        if row == 0 && table.header_rows > 0 {
            xml.push_str("<table:table-header-rows>");
        }
        xml.push_str("<table:table-row>");
        for column in 0..columns {
            let _ = write!(
                xml,
                "<table:table-cell table:style-name=\"{name}.Cell\" office:value-type=\"string\">"
            );
            let text = cells.get(column).map_or("", String::as_str);
            for line in text.split('\n') {
                if row < table.header_rows {
                    let _ = write!(
                        xml,
                        "<text:p text:style-name=\"Table_20_Heading\">{}</text:p>",
                        escape(line)
                    );
                } else {
                    let _ = write!(xml, "<text:p>{}</text:p>", escape(line));
                }
            }
            xml.push_str("</table:table-cell>");
        }
        xml.push_str("</table:table-row>");
        if row + 1 == table.header_rows.min(table.cells.len()) {
            xml.push_str("</table:table-header-rows>");
        }
    }
    xml.push_str("</table:table>");
    (styles, xml)
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::super::tests::with_table;
    use super::super::xml::{XmlEvent, parse};
    use super::*;
    use crate::zip::read_zip;

    #[test]
    fn opendocument_text_is_well_formed() {
        let bytes = to_odt(&sample());
        let files = read_zip(&bytes).unwrap();
        assert_eq!(files[0].0, "mimetype");
        let content = files
            .iter()
            .find(|(name, _)| name == "content.xml")
            .unwrap();
        let xml = String::from_utf8(content.1.clone()).unwrap();
        assert!(
            xml.contains("text:outline-level=\"1\">My Story</text:h>"),
            "{xml}"
        );
        assert!(xml.contains("text:note-class=\"footnote\""));
        assert!(xml.contains("<text:list text:style-name=\"L1\">"));
        let events = parse(&xml);
        let opens = events
            .iter()
            .filter(|e| matches!(e, XmlEvent::Open { empty: false, .. }))
            .count();
        let closes = events
            .iter()
            .filter(|e| matches!(e, XmlEvent::Close(_)))
            .count();
        assert_eq!(opens, closes);
    }

    #[test]
    fn opendocument_tables_have_columns_and_heading_rows() {
        let bytes = to_odt(&with_table());
        let files = read_zip(&bytes).unwrap();
        let content = files
            .iter()
            .find(|(name, _)| name == "content.xml")
            .unwrap();
        let xml = String::from_utf8(content.1.clone()).unwrap();
        assert_eq!(
            xml.matches("<table:table-cell ").count(),
            xml.matches("</table:table-cell>").count()
        );
        assert!(xml.contains("<table:table table:name=\"Table1\""), "{xml}");
        assert_eq!(xml.matches("<table:table-column ").count(), 2);
        assert_eq!(xml.matches("<table:table-header-rows>").count(), 1);
        assert_eq!(xml.matches("<table:table-row>").count(), 2);
        assert!(xml.contains("Bolt &lt;M8&gt;"));
    }
}
