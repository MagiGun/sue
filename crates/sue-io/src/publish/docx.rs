//! Word documents (`.docx`, Office Open XML): written with Word's own
//! style names (Normal, Title, Heading 1 and on) so its navigation pane and
//! table of contents work, with numbered and bulleted lists, footnotes,
//! links, pictures and page numbers; and read back into rich text.

use std::collections::HashMap;
use std::fmt::Write as _;

use sue_core::Value;
use sue_model::{TextMarks, mark_kind as kind};

use super::xml::{XmlEvent, escape, parse};
use super::{Book, PublishStyle, hex, paragraphs};
use crate::zip::{ZipWriter, read_zip};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn twips(points: f32) -> i32 {
    (points * 20.0).round() as i32
}

/// Word's style id for a style key.
fn style_id(key: &str) -> String {
    match key {
        "body" => "Normal".to_owned(),
        "title" => "Title".to_owned(),
        "subtitle" => "Subtitle".to_owned(),
        "quote" => "Quote".to_owned(),
        key if key.starts_with("heading") => format!("Heading{}", &key[7..]),
        key => {
            let mut chars = key.chars();
            chars.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(chars.filter(|ch| ch.is_ascii_alphanumeric()))
                    .collect()
            })
        }
    }
}

/// The style key for a Word style id (the reverse of [`style_id`]).
fn style_key(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    match lower.as_str() {
        "normal" | "bodytext" | "textbody" => "body".to_owned(),
        "title" => "title".to_owned(),
        "subtitle" => "subtitle".to_owned(),
        "quote" | "intensequote" | "blocktext" => "quote".to_owned(),
        _ => lower
            .strip_prefix("heading")
            .and_then(|level| level.parse::<u8>().ok())
            .map_or(lower.clone(), |level| {
                format!("heading{}", level.clamp(1, 4))
            }),
    }
}

fn run_props(look: &sue_model::CharFormat) -> String {
    let mut props = String::new();
    if look.code {
        props.push_str(
            "<w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\" w:cs=\"Courier New\"/>",
        );
    } else if let Some(font) = &look.font {
        let font = escape(font);
        let _ = write!(
            props,
            "<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>"
        );
    }
    if look.bold {
        props.push_str("<w:b/>");
    }
    if look.italic {
        props.push_str("<w:i/>");
    }
    if look.strike {
        props.push_str("<w:strike/>");
    }
    if look.small_caps {
        props.push_str("<w:smallCaps/>");
    }
    if let Some(color) = look.color {
        let _ = write!(props, "<w:color w:val=\"{}\"/>", hex(color));
    } else if look.link.is_some() {
        props.push_str("<w:color w:val=\"1A55B0\"/>");
    }
    if let Some(size) = look.size {
        let _ = write!(props, "<w:sz w:val=\"{}\"/>", (size * 2.0).round() as i32);
    }
    if look.underline || look.link.is_some() {
        props.push_str("<w:u w:val=\"single\"/>");
    }
    if let Some(color) = look.highlight {
        let _ = write!(
            props,
            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>",
            hex(color)
        );
    }
    match look.script {
        1 => props.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        -1 => props.push_str("<w:vertAlign w:val=\"subscript\"/>"),
        _ => {}
    }
    if props.is_empty() {
        props
    } else {
        format!("<w:rPr>{props}</w:rPr>")
    }
}

/// Text as Word runs' content: tabs as `w:tab`.
fn run_text(text: &str) -> String {
    let mut out = String::new();
    for (index, piece) in text.split('\t').enumerate() {
        if index > 0 {
            out.push_str("<w:tab/>");
        }
        if !piece.is_empty() {
            let _ = write!(out, "<w:t xml:space=\"preserve\">{}</w:t>", escape(piece));
        }
    }
    out
}

fn style_xml(style: &PublishStyle, body: &PublishStyle) -> String {
    let id = style_id(&style.key);
    let mut out = format!(
        "<w:style w:type=\"paragraph\"{} w:styleId=\"{id}\"><w:name w:val=\"{}\"/>",
        if style.key == "body" {
            " w:default=\"1\""
        } else {
            ""
        },
        escape(&if style.key == "body" {
            "Normal".to_owned()
        } else if style.key.starts_with("heading") {
            format!("heading {}", &style.key[7..])
        } else {
            style.name.clone()
        })
    );
    if style.key != "body" {
        out.push_str("<w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>");
    }
    out.push_str("<w:qFormat/><w:pPr>");
    if style.keep_with_next {
        out.push_str("<w:keepNext/>");
    }
    if style.page_break {
        out.push_str("<w:pageBreakBefore/>");
    }
    let _ = write!(
        out,
        "<w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{}\" w:lineRule=\"auto\"/>",
        twips(style.space_before),
        twips(style.space_after),
        (style.line_spacing * 240.0).round() as i32
    );
    if style.indent != 0.0 || style.indent_right != 0.0 || style.first_indent != 0.0 {
        let first = if style.first_indent >= 0.0 {
            format!("w:firstLine=\"{}\"", twips(style.first_indent))
        } else {
            format!("w:hanging=\"{}\"", twips(-style.first_indent))
        };
        let _ = write!(
            out,
            "<w:ind w:left=\"{}\" w:right=\"{}\" {first}/>",
            twips(style.indent),
            twips(style.indent_right)
        );
    }
    let _ = write!(out, "<w:jc w:val=\"{}\"/>", jc(&style.align));
    if let Some(level) = style.outline {
        let _ = write!(out, "<w:outlineLvl w:val=\"{}\"/>", level.saturating_sub(1));
    }
    out.push_str("</w:pPr><w:rPr>");
    if style.font != body.font || style.key == "body" {
        let font = escape(&style.font);
        let _ = write!(
            out,
            "<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>"
        );
    }
    if style.bold {
        out.push_str("<w:b/>");
    }
    if style.italic {
        out.push_str("<w:i/>");
    }
    let _ = write!(
        out,
        "<w:sz w:val=\"{}\"/>",
        (style.size * 2.0).round() as i32
    );
    out.push_str("</w:rPr></w:style>");
    out
}

fn jc(align: &str) -> &'static str {
    match align {
        "center" => "center",
        "right" => "right",
        "justify" => "both",
        _ => "left",
    }
}

/// The book as a Word document.
#[must_use]
pub fn to_docx(book: &Book) -> Vec<u8> {
    let mut relations: Vec<(String, String, String, bool)> = vec![
        (
            "rIdStyles".into(),
            "styles".into(),
            "styles.xml".into(),
            false,
        ),
        (
            "rIdNumbering".into(),
            "numbering".into(),
            "numbering.xml".into(),
            false,
        ),
        (
            "rIdFootnotes".into(),
            "footnotes".into(),
            "footnotes.xml".into(),
            false,
        ),
        (
            "rIdSettings".into(),
            "settings".into(),
            "settings.xml".into(),
            false,
        ),
    ];
    let has_header = !book.header.is_empty();
    let has_footer = !book.footer.is_empty();
    if has_header {
        relations.push((
            "rIdHeader".into(),
            "header".into(),
            "header1.xml".into(),
            false,
        ));
    }
    if has_footer {
        relations.push((
            "rIdFooter".into(),
            "footer".into(),
            "footer1.xml".into(),
            false,
        ));
    }
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // Comments: each opens once, before its first words, and closes at the
    // end of them (or of their paragraph).
    let mut comments: Vec<sue_core::EntityId> = Vec::new();
    let mut open_comment: Option<usize> = None;
    let mut revisions = 0;
    // Each numbered list starts again at 1: a numbering instance per list.
    let mut number_instances = 0_usize;
    let mut body = String::new();
    let mut drawing = 0;
    for (index, section) in book.sections.iter().enumerate() {
        let mut in_numbers = false;
        let mut current_number = 0;
        for (para_index, para) in paragraphs(&section.text, &section.marks)
            .into_iter()
            .enumerate()
        {
            let mut props = format!("<w:pStyle w:val=\"{}\"/>", style_id(&para.style));
            if (para.page_break || (section.new_page && index > 0 && para_index == 0))
                && !book.style(&para.style).page_break
            {
                props.push_str("<w:pageBreakBefore/>");
            }
            match para.list.as_deref() {
                Some("number") => {
                    if !in_numbers {
                        number_instances += 1;
                        current_number = 2 + number_instances;
                        in_numbers = true;
                    }
                    let _ = write!(
                        props,
                        "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{current_number}\"/></w:numPr>",
                        para.level
                    );
                }
                Some(_) => {
                    in_numbers = false;
                    let _ = write!(
                        props,
                        "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"1\"/></w:numPr>",
                        para.level
                    );
                }
                None => in_numbers = false,
            }
            let mut spacing = String::new();
            if let Some(before) = para.space_before {
                let _ = write!(spacing, " w:before=\"{}\"", twips(before));
            }
            if let Some(after) = para.space_after {
                let _ = write!(spacing, " w:after=\"{}\"", twips(after));
            }
            if let Some(line) = para.line_spacing {
                let _ = write!(
                    spacing,
                    " w:line=\"{}\" w:lineRule=\"auto\"",
                    (line * 240.0).round() as i32
                );
            }
            if !spacing.is_empty() {
                let _ = write!(props, "<w:spacing{spacing}/>");
            }
            if para.indent.is_some() || para.first_indent.is_some() {
                let mut ind = String::from("<w:ind");
                if let Some(indent) = para.indent {
                    let _ = write!(ind, " w:left=\"{}\"", twips(indent));
                }
                if let Some(first) = para.first_indent {
                    if first >= 0.0 {
                        let _ = write!(ind, " w:firstLine=\"{}\"", twips(first));
                    } else {
                        let _ = write!(ind, " w:hanging=\"{}\"", twips(-first));
                    }
                }
                ind.push_str("/>");
                props.push_str(&ind);
            }
            if let Some(align) = &para.align {
                let _ = write!(props, "<w:jc w:val=\"{}\"/>", jc(align));
            }
            if let Some(table) = book.table(&para) {
                let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
                body.push_str(&table_xml(table, width));
                continue;
            }
            if let Some(contents) = book.contents_of(&para) {
                let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
                body.push_str(&contents_xml(book, contents, width));
                continue;
            }
            let _ = write!(body, "<w:p><w:pPr>{props}</w:pPr>");
            if let Some(id) = para.embed
                && let Some(figure) = book.figures.get(&id)
            {
                drawing += 1;
                let rel = format!("rIdImage{drawing}");
                let name = format!("media/image{drawing}.png");
                relations.push((rel.clone(), "image".into(), name.clone(), false));
                images.push((name, figure.png.clone()));
                // Fit the text width, 96 pixels to the inch.
                let width_pt = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
                let scale = (width_pt / (figure.width as f32 * 0.75)).min(1.0);
                let cx = (figure.width as f32 * 9525.0 * scale).round() as i64;
                let cy = (figure.height as f32 * 9525.0 * scale).round() as i64;
                let _ = write!(
                    body,
                    "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{drawing}\" name=\"Picture {drawing}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{drawing}\" name=\"image{drawing}.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
                );
            }
            for (text, look) in &para.runs {
                let here = look.comment.filter(|id| book.comments.contains_key(id));
                let current = open_comment.map(|index| comments[index]);
                if here != current {
                    if let Some(index) = open_comment.take() {
                        body.push_str(&comment_end(index));
                    }
                    if let Some(id) = here
                        && !comments.contains(&id)
                    {
                        comments.push(id);
                        let index = comments.len() - 1;
                        let _ = write!(body, "<w:commentRangeStart w:id=\"{index}\"/>");
                        open_comment = Some(index);
                    }
                }
                let mut run = format!("<w:r>{}{}</w:r>", run_props(look), run_text(text));
                // Tracked changes, with who made them and when.
                for (change, tag) in [(&look.deleted, "w:del"), (&look.inserted, "w:ins")] {
                    if let Some(change) = change {
                        revisions += 1;
                        if tag == "w:del" {
                            run = run
                                .replace("<w:t xml:space=", "<w:delText xml:space=")
                                .replace("</w:t>", "</w:delText>");
                        }
                        run = format!(
                            "<{tag} w:id=\"{}\" w:author=\"{}\" w:date=\"{}\">{run}</{tag}>",
                            1000 + revisions,
                            escape(&change.author),
                            escape(&word_date(&change.date))
                        );
                        break;
                    }
                }
                match &look.link {
                    Some(link) => {
                        let rel = format!("rIdLink{}", relations.len());
                        relations.push((rel.clone(), "hyperlink".into(), link.clone(), true));
                        let _ = write!(body, "<w:hyperlink r:id=\"{rel}\">{run}</w:hyperlink>");
                    }
                    None => body.push_str(&run),
                }
                if let Some(note) = &look.footnote {
                    notes.push(note.clone());
                    let _ = write!(
                        body,
                        "<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:footnoteReference w:id=\"{}\"/></w:r>",
                        notes.len()
                    );
                }
            }
            if let Some(index) = open_comment.take() {
                body.push_str(&comment_end(index));
            }
            body.push_str("</w:p>");
        }
    }
    if !comments.is_empty() {
        relations.push((
            "rIdComments".into(),
            "comments".into(),
            "comments.xml".into(),
            false,
        ));
    }
    let comments_xml = comments_part(book, &comments);
    let [top, right, bottom, left] = book.margins;
    let mut references = String::new();
    if has_header {
        references.push_str("<w:headerReference w:type=\"default\" r:id=\"rIdHeader\"/>");
    }
    if has_footer {
        references.push_str("<w:footerReference w:type=\"default\" r:id=\"rIdFooter\"/>");
    }
    let title_page = if book.different_first {
        "<w:titlePg/>"
    } else {
        ""
    };
    let _ = write!(
        body,
        "<w:sectPr>{references}<w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>{title_page}</w:sectPr>",
        twips(book.page[0]),
        twips(book.page[1]),
        twips(top),
        twips(right),
        twips(bottom),
        twips(left)
    );
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}</w:body></w:document>"
    );

    let body_style = book.style("body");
    let mut styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W}\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"{0}\" w:hAnsi=\"{0}\" w:cs=\"{0}\"/><w:sz w:val=\"{1}\"/><w:lang w:val=\"{2}\"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>",
        escape(&body_style.font),
        (body_style.size * 2.0).round() as i32,
        escape(&book.language)
    );
    for style in &book.styles {
        styles.push_str(&style_xml(style, body_style));
    }
    styles.push_str("<w:style w:type=\"paragraph\" w:styleId=\"FootnoteText\"><w:name w:val=\"footnote text\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:rPr><w:sz w:val=\"20\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/></w:style></w:styles>");

    let mut numbering = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{W}\">"
    );
    for (abstract_id, bullet) in [(0, true), (1, false)] {
        let _ = write!(
            numbering,
            "<w:abstractNum w:abstractNumId=\"{abstract_id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
        );
        for level in 0..9 {
            let indent = 720 * (level + 1);
            let (format, text) = if bullet {
                (
                    "bullet",
                    ["\u{2022}", "\u{25E6}", "\u{25AA}"][level % 3].to_owned(),
                )
            } else {
                (
                    ["decimal", "lowerLetter", "lowerRoman"][level % 3],
                    format!("%{}.", level + 1),
                )
            };
            let _ = write!(
                numbering,
                "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{indent}\" w:hanging=\"360\"/></w:pPr></w:lvl>"
            );
        }
        numbering.push_str("</w:abstractNum>");
    }
    numbering.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>");
    for instance in 1..=number_instances {
        let _ = write!(
            numbering,
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>",
            2 + instance
        );
    }
    numbering.push_str("</w:numbering>");

    let mut footnotes = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:footnotes xmlns:w=\"{W}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"
    );
    for (index, note) in notes.iter().enumerate() {
        let _ = write!(
            footnotes,
            "<w:footnote w:id=\"{}\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr><w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:footnoteRef/></w:r><w:r>{}</w:r></w:p></w:footnote>",
            index + 1,
            run_text(&format!(" {note}"))
        );
    }
    footnotes.push_str("</w:footnotes>");

    // Word fills in a table of contents' page numbers when it opens.
    let update = if book.contents.is_empty() {
        ""
    } else {
        "<w:updateFields w:val=\"true\"/>"
    };
    let settings = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:settings xmlns:w=\"{W}\">{update}<w:defaultTabStop w:val=\"720\"/><w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat></w:settings>"
    );
    let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
    let header = running_xml(book, &book.header, "w:hdr", width);
    let footer = running_xml(book, &book.footer, "w:ftr", width);

    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (id, kind, target, external) in &relations {
        let _ = write!(
            rels,
            "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{}\"{}/>",
            escape(target),
            if *external {
                " TargetMode=\"External\""
            } else {
                ""
            }
        );
    }
    rels.push_str("</Relationships>");

    let main = "application/vnd.openxmlformats-officedocument.wordprocessingml";
    let mut types = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/word/document.xml\" ContentType=\"{main}.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"{main}.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"{main}.numbering+xml\"/><Override PartName=\"/word/footnotes.xml\" ContentType=\"{main}.footnotes+xml\"/><Override PartName=\"/word/settings.xml\" ContentType=\"{main}.settings+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>"
    );
    if has_header {
        let _ = write!(
            types,
            "<Override PartName=\"/word/header1.xml\" ContentType=\"{main}.header+xml\"/>"
        );
    }
    if !comments.is_empty() {
        let _ = write!(
            types,
            "<Override PartName=\"/word/comments.xml\" ContentType=\"{main}.comments+xml\"/>"
        );
    }
    if has_footer {
        let _ = write!(
            types,
            "<Override PartName=\"/word/footer1.xml\" ContentType=\"{main}.footer+xml\"/>"
        );
    }
    types.push_str("</Types>");
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>";
    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{}</dc:language></cp:coreProperties>",
        escape(&book.title),
        escape(&book.author),
        escape(&book.language)
    );
    let app = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Sue</Application></Properties>";

    let mut zip = ZipWriter::new();
    zip.add_text("[Content_Types].xml", &types);
    zip.add_text("_rels/.rels", root_rels);
    zip.add_text("docProps/core.xml", &core);
    zip.add_text("docProps/app.xml", app);
    zip.add_text("word/document.xml", &document);
    zip.add_text("word/styles.xml", &styles);
    zip.add_text("word/numbering.xml", &numbering);
    zip.add_text("word/footnotes.xml", &footnotes);
    zip.add_text("word/settings.xml", &settings);
    if has_header {
        zip.add_text("word/header1.xml", &header);
    }
    if !comments.is_empty() {
        zip.add_text("word/comments.xml", &comments_xml);
    }
    if has_footer {
        zip.add_text("word/footer1.xml", &footer);
    }
    zip.add_text("word/_rels/document.xml.rels", &rels);
    for (name, bytes) in images {
        zip.add(&format!("word/{name}"), &bytes, true);
    }
    zip.finish()
}

/// `true` unless a Word toggle (`<w:b w:val="0"/>`) turns it off.
fn on(event: &XmlEvent) -> bool {
    !matches!(event.attr("w:val"), Some("0" | "false" | "none"))
}

/// A Word document read into rich text: paragraph styles (Word's names
/// mapped to Sue's), alignment, lists, bold, italic, underline,
/// strikethrough, raised and lowered text, colors, sizes, fonts, links and
/// footnotes. Tables become their cells' paragraphs.
///
/// # Errors
/// If the file is not a Word document.
pub fn from_docx(bytes: &[u8]) -> Result<(String, TextMarks), String> {
    let files: HashMap<String, Vec<u8>> = read_zip(bytes)?.into_iter().collect();
    let read = |name: &str| {
        files
            .get(name)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    };
    let document = read("word/document.xml").ok_or("this is not a Word document")?;
    // Links and footnotes live in their own parts.
    let mut links = HashMap::new();
    if let Some(rels) = read("word/_rels/document.xml.rels") {
        for event in parse(&rels) {
            if let (Some(id), Some(target)) = (event.attr("Id"), event.attr("Target"))
                && event.attr("TargetMode") == Some("External")
            {
                links.insert(id.to_owned(), target.to_owned());
            }
        }
    }
    let mut notes: HashMap<String, String> = HashMap::new();
    if let Some(xml) = read("word/footnotes.xml") {
        let mut current: Option<(String, String)> = None;
        for event in parse(&xml) {
            match &event {
                XmlEvent::Open { name, .. } if name == "w:footnote" => {
                    current = event.attr("w:id").map(|id| (id.to_owned(), String::new()));
                }
                XmlEvent::Text(text) => {
                    if let Some((_, note)) = &mut current {
                        note.push_str(text);
                    }
                }
                XmlEvent::Close(name) if name == "w:footnote" => {
                    if let Some((id, note)) = current.take() {
                        notes.insert(id, note.trim().to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    // Which lists are numbered.
    let mut numbered: HashMap<String, bool> = HashMap::new();
    if let Some(xml) = read("word/numbering.xml") {
        let mut abstracts: HashMap<String, bool> = HashMap::new();
        let mut current = None;
        let mut first_level = false;
        let mut nums: Vec<(String, String)> = Vec::new();
        let mut num = None;
        for event in parse(&xml) {
            if let XmlEvent::Open { name, .. } = &event {
                match name.as_str() {
                    "w:abstractNum" => current = event.attr("w:abstractNumId").map(str::to_owned),
                    "w:lvl" => first_level = event.attr("w:ilvl") == Some("0"),
                    "w:numFmt" if first_level => {
                        if let Some(id) = &current {
                            abstracts.insert(id.clone(), event.attr("w:val") != Some("bullet"));
                        }
                    }
                    "w:num" => num = event.attr("w:numId").map(str::to_owned),
                    "w:abstractNumId" => {
                        if let (Some(num), Some(id)) = (&num, event.attr("w:val")) {
                            nums.push((num.clone(), id.to_owned()));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (num, id) in nums {
            numbered.insert(num, abstracts.get(&id).copied().unwrap_or(false));
        }
    }

    let mut text = String::new();
    let mut marks = TextMarks::new();
    #[derive(Default)]
    struct Paragraph {
        start: usize,
        style: Option<String>,
        align: Option<String>,
        list: Option<(String, usize)>,
        page_break: bool,
    }
    #[derive(Default, Clone)]
    struct Run {
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        script: Option<&'static str>,
        color: Option<String>,
        size: Option<f32>,
        font: Option<String>,
    }
    let mut para: Option<Paragraph> = None;
    let mut run = Run::default();
    let mut in_run_props = false;
    let mut in_para_props = false;
    let mut in_text = false;
    let mut link: Option<(usize, String)> = None;
    let mut list_num: Option<String> = None;
    let mut list_level = 0;
    for event in parse(&document) {
        match &event {
            XmlEvent::Open { name, empty, .. } => match name.as_str() {
                "w:p" => {
                    para = Some(Paragraph {
                        start: text.len(),
                        ..Paragraph::default()
                    });
                    if *empty {
                        text.push('\n');
                        para = None;
                    }
                }
                "w:pStyle" => {
                    if let (Some(para), Some(id)) = (&mut para, event.attr("w:val")) {
                        let key = style_key(id);
                        if key != "body" && key != "listparagraph" {
                            para.style = Some(key);
                        }
                    }
                }
                "w:jc" if !in_run_props => {
                    if let (Some(para), Some(value)) = (&mut para, event.attr("w:val")) {
                        para.align = match value {
                            "center" => Some("center".to_owned()),
                            "right" | "end" => Some("right".to_owned()),
                            "both" | "distribute" => Some("justify".to_owned()),
                            _ => None,
                        };
                    }
                }
                "w:pageBreakBefore" => {
                    if let Some(para) = &mut para {
                        para.page_break = on(&event);
                    }
                }
                "w:ilvl" => {
                    list_level = event
                        .attr("w:val")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0)
                }
                "w:numId" => list_num = event.attr("w:val").map(str::to_owned),
                "w:pPr" => in_para_props = !*empty,
                "w:r" => run = Run::default(),
                "w:rPr" => in_run_props = !*empty,
                "w:b" if in_run_props => run.bold = on(&event),
                "w:i" if in_run_props => run.italic = on(&event),
                "w:u" if in_run_props => run.underline = on(&event),
                "w:strike" | "w:dstrike" if in_run_props => run.strike = on(&event),
                "w:vertAlign" if in_run_props => {
                    run.script = match event.attr("w:val") {
                        Some("superscript") => Some("super"),
                        Some("subscript") => Some("sub"),
                        _ => None,
                    };
                }
                "w:color" if in_run_props => {
                    run.color = event
                        .attr("w:val")
                        .filter(|value| *value != "auto" && *value != "000000")
                        .map(str::to_owned);
                }
                "w:sz" if in_run_props => {
                    run.size = event
                        .attr("w:val")
                        .and_then(|v| v.parse::<f32>().ok())
                        .map(|half| half / 2.0)
                }
                "w:rFonts" if in_run_props => run.font = event.attr("w:ascii").map(str::to_owned),
                "w:t" => in_text = !*empty,
                "w:tab" if !in_run_props && !in_para_props => text.push('\t'),
                "w:br" | "w:cr" => {
                    if event.attr("w:type") == Some("page") {
                        if let Some(para) = &mut para {
                            para.page_break = true;
                        }
                    } else {
                        text.push(' ');
                    }
                }
                "w:hyperlink" => {
                    link = event
                        .attr("r:id")
                        .and_then(|id| links.get(id))
                        .map(|target| (text.len(), target.clone()));
                }
                "w:footnoteReference" => {
                    if let Some(note) = event.attr("w:id").and_then(|id| notes.get(id))
                        && text.len() > para.as_ref().map_or(0, |para| para.start)
                    {
                        let end = text.len();
                        let start = text[..end]
                            .char_indices()
                            .next_back()
                            .map_or(0, |(at, _)| at);
                        marks.set(start..end, kind::FOOTNOTE, Some(note.clone().into()));
                    }
                }
                _ => {}
            },
            XmlEvent::Text(chunk) if in_text => {
                let start = text.len();
                text.push_str(chunk);
                let range = start..text.len();
                let mut set =
                    |name: &str, value: Value| marks.set(range.clone(), name, Some(value));
                if run.bold {
                    set(kind::BOLD, Value::Bool(true));
                }
                if run.italic {
                    set(kind::ITALIC, Value::Bool(true));
                }
                if run.underline && link.is_none() {
                    set(kind::UNDERLINE, Value::Bool(true));
                }
                if run.strike {
                    set(kind::STRIKE, Value::Bool(true));
                }
                if let Some(script) = run.script {
                    set(kind::SCRIPT, Value::Text(script.to_owned()));
                }
                if let Some(color) = &run.color {
                    set(kind::COLOR, Value::Text(format!("#{color}")));
                }
                if let Some(size) = run.size {
                    set(kind::SIZE, Value::Float(f64::from(size)));
                }
                if let Some(font) = &run.font {
                    set(kind::FONT, Value::Text(font.clone()));
                }
            }
            XmlEvent::Close(name) => match name.as_str() {
                "w:rPr" => in_run_props = false,
                "w:pPr" => in_para_props = false,
                "w:t" => in_text = false,
                "w:hyperlink" => {
                    if let Some((start, target)) = link.take() {
                        marks.set(start..text.len(), kind::LINK, Some(target.into()));
                    }
                }
                "w:p" => {
                    if let Some(mut done) = para.take() {
                        if let Some(num) = list_num.take().filter(|num| num != "0") {
                            let kind = if numbered.get(&num).copied().unwrap_or(false) {
                                "number"
                            } else {
                                "bullet"
                            };
                            done.list = Some((kind.to_owned(), list_level));
                        }
                        list_level = 0;
                        text.push('\n');
                        let at = done.start..done.start;
                        if let Some(style) = done.style {
                            marks.set_paragraphs(
                                &text,
                                at.clone(),
                                kind::STYLE,
                                Some(style.into()),
                            );
                        }
                        if let Some(align) = done.align {
                            marks.set_paragraphs(
                                &text,
                                at.clone(),
                                kind::ALIGN,
                                Some(align.into()),
                            );
                        }
                        if let Some((list, level)) = done.list {
                            marks.set_paragraphs(&text, at.clone(), kind::LIST, Some(list.into()));
                            if level > 0 {
                                marks.set_paragraphs(
                                    &text,
                                    at.clone(),
                                    kind::LEVEL,
                                    Some(Value::Int(level as i64)),
                                );
                            }
                        }
                        if done.page_break {
                            marks.set_paragraphs(&text, at, kind::PAGE_BREAK, Some(true.into()));
                        }
                    }
                }
                _ => {}
            },
            XmlEvent::Text(_) => {}
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    marks.fit(&text);
    Ok((text, marks))
}

/// A date as Word writes it (`2026-10-29T00:00:00Z`).
fn word_date(date: &str) -> String {
    if date.len() == 10 {
        format!("{date}T00:00:00Z")
    } else {
        date.to_owned()
    }
}

/// The end of comment `index`'s words, and its mark in the text.
fn comment_end(index: usize) -> String {
    format!(
        "<w:commentRangeEnd w:id=\"{index}\"/><w:r><w:commentReference w:id=\"{index}\"/></w:r>"
    )
}

/// The comments part: each comment by its author and date, its replies
/// after it as paragraphs of their own, named.
fn comments_part(book: &Book, comments: &[sue_core::EntityId]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:comments xmlns:w=\"{W}\">"
    );
    for (index, id) in comments.iter().enumerate() {
        let Some(thread) = book.comments.get(id) else {
            continue;
        };
        let date = if thread.first.date.len() == 10 {
            format!("{}T00:00:00Z", thread.first.date)
        } else {
            thread.first.date.clone()
        };
        let initials: String = thread
            .first
            .author
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .collect();
        let _ = write!(
            out,
            "<w:comment w:id=\"{index}\" w:author=\"{}\" w:date=\"{}\" w:initials=\"{}\">",
            escape(&thread.first.author),
            escape(&date),
            escape(&initials)
        );
        let _ = write!(
            out,
            "<w:p><w:r>{}</w:r></w:p>",
            run_text(&thread.first.text)
        );
        for reply in &thread.replies {
            let _ = write!(
                out,
                "<w:p><w:r><w:rPr><w:b/></w:rPr>{}</w:r><w:r>{}</w:r></w:p>",
                run_text(&format!("{}: ", reply.author)),
                run_text(&reply.text)
            );
        }
        out.push_str("</w:comment>");
    }
    out.push_str("</w:comments>");
    out
}

/// A running header (`w:hdr`) or footer (`w:ftr`) part: one paragraph with
/// a centered and a right tab stop across `width` points, its page fields
/// Word's own `PAGE` and `NUMPAGES`.
fn running_xml(book: &Book, running: &sue_model::Running, tag: &str, width: f32) -> String {
    use crate::publish::Live;
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{tag} xmlns:w=\"{W}\"><w:p><w:pPr><w:tabs><w:tab w:val=\"center\" w:pos=\"{}\"/><w:tab w:val=\"right\" w:pos=\"{}\"/></w:tabs><w:rPr><w:sz w:val=\"18\"/></w:rPr></w:pPr>",
        twips(width / 2.0),
        twips(width)
    );
    for (index, part) in book.live(running).iter().enumerate() {
        if index > 0 {
            out.push_str("<w:r><w:tab/></w:r>");
        }
        for piece in part {
            match piece {
                Live::Text(text) if !text.is_empty() => {
                    let _ = write!(
                        out,
                        "<w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr>{}</w:r>",
                        run_text(text)
                    );
                }
                Live::Text(_) => {}
                Live::Page | Live::Pages => {
                    let code = if *piece == Live::Page {
                        "PAGE"
                    } else {
                        "NUMPAGES"
                    };
                    let _ = write!(
                        out,
                        "<w:fldSimple w:instr=\" {code} \"><w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr><w:t>1</w:t></w:r></w:fldSimple>"
                    );
                }
            }
        }
    }
    let _ = write!(out, "</w:p></{tag}>");
    out
}

/// A table of contents as Word's own `TOC` field over the headings, its
/// entries the book's headings (Word fills in the pages when it opens).
fn contents_xml(book: &Book, contents: &sue_model::contents::Contents, width: f32) -> String {
    let mut out = String::new();
    if !contents.title.is_empty() {
        let _ = write!(
            out,
            "<w:p><w:pPr><w:pStyle w:val=\"TOCHeading\"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr>{}</w:r></w:p>",
            run_text(&contents.title)
        );
    }
    let headings = book.headings(contents);
    let tabs = format!(
        "<w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"{}\"/></w:tabs>",
        twips(width)
    );
    let field = format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-{}\" \\h \\z \\u </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
        contents.levels
    );
    let end = "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    if headings.is_empty() {
        let _ = write!(out, "<w:p>{field}{end}</w:p>");
        return out;
    }
    for (index, (level, title)) in headings.iter().enumerate() {
        let _ = write!(
            out,
            "<w:p><w:pPr><w:pStyle w:val=\"TOC{level}\"/>{tabs}<w:ind w:left=\"{}\"/></w:pPr>",
            twips(f32::from(level.saturating_sub(1)) * 18.0)
        );
        if index == 0 {
            out.push_str(&field);
        }
        let _ = write!(out, "<w:r>{}</w:r>", run_text(title));
        if index + 1 == headings.len() {
            out.push_str(end);
        }
        out.push_str("</w:p>");
    }
    out
}

/// A table as WordprocessingML, `width` points across: a grid of the
/// column widths, single borders, the heading rows marked to repeat.
fn table_xml(table: &crate::publish::PublishTable, width: f32) -> String {
    let columns = table.columns().max(1);
    let widths: Vec<i32> = (0..columns)
        .map(|column| twips(width * table.share(column)))
        .collect();
    let mut out =
        String::from("<w:tbl><w:tblPr><w:tblW w:w=\"5000\" w:type=\"pct\"/><w:tblBorders>");
    for side in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        let _ = write!(
            out,
            "<w:{side} w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>"
        );
    }
    out.push_str("</w:tblBorders><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid>");
    for width in &widths {
        let _ = write!(out, "<w:gridCol w:w=\"{width}\"/>");
    }
    out.push_str("</w:tblGrid>");
    for (row, cells) in table.cells.iter().enumerate() {
        let heading = row < table.header_rows;
        out.push_str("<w:tr>");
        if heading {
            out.push_str("<w:trPr><w:tblHeader/></w:trPr>");
        }
        for (column, width) in widths.iter().enumerate() {
            let _ = write!(
                out,
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/></w:tcPr>"
            );
            let text = cells.get(column).map_or("", String::as_str);
            for line in text.split('\n') {
                out.push_str("<w:p>");
                if !line.is_empty() {
                    let bold = if heading { "<w:rPr><w:b/></w:rPr>" } else { "" };
                    let _ = write!(out, "<w:r>{bold}{}</w:r>", run_text(line));
                }
                out.push_str("</w:p>");
            }
            out.push_str("</w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn word_keeps_tracked_changes_with_their_authors() {
        use sue_model::changes::Change;
        let mut book = sample();
        let change = Change::new("Ann", "2026-10-30");
        book.sections[0]
            .marks
            .set(18..22, kind::DELETED, Some(change.value()));
        book.sections[0]
            .marks
            .set(23..26, kind::INSERTED, Some(change.value()));
        let bytes = to_docx(&book);
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("<w:del w:id="), "{xml}");
        assert!(xml.contains("w:author=\"Ann\" w:date=\"2026-10-30T00:00:00Z\""));
        assert!(xml.contains("<w:delText xml:space=\"preserve\">dark</w:delText>"));
        assert!(xml.contains("<w:ins w:id="));
    }

    #[test]
    fn word_comments_mark_their_words_and_keep_their_replies() {
        let bytes = to_docx(&super::super::tests::with_comment());
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("<w:commentRangeStart w:id=\"0\"/>"), "{xml}");
        assert!(xml.contains("<w:commentReference w:id=\"0\"/>"));
        let start = xml.find("commentRangeStart").unwrap();
        let end = xml.find("commentRangeEnd").unwrap();
        assert!(xml[start..end].contains("dark"));
        let comments = String::from_utf8(files["word/comments.xml"].clone()).unwrap();
        assert!(comments.contains("w:author=\"Ann\""), "{comments}");
        assert!(comments.contains("Too much?") && comments.contains("Keep it."));
        let types = String::from_utf8(files["[Content_Types].xml"].clone()).unwrap();
        assert!(types.contains("comments+xml"));
    }

    #[test]
    fn word_contents_are_a_toc_field_word_updates() {
        let bytes = to_docx(&super::super::tests::with_contents());
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("TOC \\o \"1-2\""), "{xml}");
        assert_eq!(xml.matches("w:fldCharType=\"begin\"").count(), 1);
        assert_eq!(xml.matches("w:fldCharType=\"end\"").count(), 1);
        assert!(xml.contains("<w:pStyle w:val=\"TOC2\"/>"));
        let settings = String::from_utf8(files["word/settings.xml"].clone()).unwrap();
        assert!(settings.contains("<w:updateFields w:val=\"true\"/>"));
    }

    #[test]
    fn word_headers_and_footers_carry_live_page_fields() {
        let mut book = sample();
        book.header = sue_model::Running::parse("{title}\t\tPage {page} of {pages}");
        book.different_first = true;
        let bytes = to_docx(&book);
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        let header = String::from_utf8(files["word/header1.xml"].clone()).unwrap();
        assert!(header.contains("My Story"), "{header}");
        assert!(header.contains("w:instr=\" PAGE \""));
        assert!(header.contains("w:instr=\" NUMPAGES \""));
        assert_eq!(header.matches("<w:tab/>").count(), 2);
        assert!(files.contains_key("word/footer1.xml"));
        let document = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(document.contains("<w:headerReference w:type=\"default\" r:id=\"rIdHeader\"/>"));
        assert!(document.contains("<w:titlePg/></w:sectPr>"));
        let rels = String::from_utf8(files["word/_rels/document.xml.rels"].clone()).unwrap();
        assert!(rels.contains("header1.xml"));
    }

    use super::super::tests::sample;
    use super::super::tests::with_table;
    use super::*;

    #[test]
    fn word_documents_round_trip_their_formatting() {
        let book = sample();
        let bytes = to_docx(&book);
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        for part in [
            "[Content_Types].xml",
            "word/document.xml",
            "word/styles.xml",
            "word/numbering.xml",
            "word/footnotes.xml",
            "word/_rels/document.xml.rels",
        ] {
            assert!(files.contains_key(part), "{part}");
        }
        let document = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert!(document.contains("<w:pStyle w:val=\"Heading1\"/>"));
        // Every part is well formed enough to read back.
        for (name, part) in &files {
            if name.ends_with(".xml") || name.ends_with(".rels") {
                let xml = String::from_utf8(part.clone()).unwrap();
                let opens = parse(&xml)
                    .iter()
                    .filter(|e| matches!(e, XmlEvent::Open { empty: false, .. }))
                    .count();
                let closes = parse(&xml)
                    .iter()
                    .filter(|e| matches!(e, XmlEvent::Close(_)))
                    .count();
                assert_eq!(opens, closes, "{name}");
            }
        }
        let (text, marks) = from_docx(&bytes).unwrap();
        assert_eq!(text, book.sections[0].text);
        assert_eq!(
            marks
                .paragraph_value(&text, 0, kind::STYLE)
                .and_then(Value::as_str),
            Some("heading1")
        );
        assert!(marks.covers(18..22, kind::BOLD));
        assert!(marks.covers(23..26, kind::ITALIC));
        assert_eq!(
            marks
                .paragraph_value(&text, 2, kind::LIST)
                .and_then(Value::as_str),
            Some("bullet")
        );
        let note = marks
            .marks()
            .iter()
            .find(|mark| mark.kind == kind::FOOTNOTE)
            .unwrap();
        assert_eq!(note.value.as_str(), Some("Or so they say."));
        assert!(from_docx(b"nope").is_err());
    }

    #[test]
    fn word_tables_have_their_grid_borders_and_heading_row() {
        let bytes = to_docx(&with_table());
        let files: HashMap<String, Vec<u8>> = read_zip(&bytes).unwrap().into_iter().collect();
        let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
        assert_eq!(
            xml.matches("<w:tc>").count(),
            xml.matches("</w:tc>").count()
        );
        assert!(xml.contains("<w:tbl>"), "{xml}");
        assert_eq!(xml.matches("<w:gridCol ").count(), 2);
        assert_eq!(xml.matches("<w:tr>").count(), 2);
        assert_eq!(xml.matches("<w:tblHeader/>").count(), 1);
        assert!(xml.contains("Count &amp; size"));
        assert!(xml.contains("Bolt &lt;M8&gt;"));
    }
}
