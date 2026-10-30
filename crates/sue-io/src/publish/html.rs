//! HTML out (one page with its styles, also the body of each e-book
//! chapter) and in (paragraphs, headings, emphasis, links, lists, quotes).

use std::fmt::Write as _;

use sue_core::Value;
use sue_model::{TextMarks, mark_kind as kind};

use super::xml::{XmlEvent, escape, parse};
use super::{Book, Para, PublishStyle, hex, paragraphs};

/// A CSS class name for style `key`.
pub(crate) fn class(key: &str) -> String {
    key.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

/// The style sheet for the book's paragraph styles.
pub(crate) fn css(book: &Book) -> String {
    let mut out = String::new();
    let body = book.style("body");
    let _ = writeln!(
        out,
        "body {{ font-family: \"{}\", serif; font-size: {}pt; line-height: {}; margin: 0 auto; max-width: 40em; padding: 2em; }}",
        body.font,
        body.size,
        body.line_spacing * 1.2
    );
    out.push_str("p, h1, h2, h3, h4, h5, h6 { margin: 0; }\nimg { max-width: 100%; }\n.footnotes { font-size: 0.85em; border-top: 1px solid #999; margin-top: 2em; }\n");
    for style in &book.styles {
        let _ = writeln!(out, ".{} {{ {} }}", class(&style.key), declarations(style));
    }
    out
}

fn declarations(style: &PublishStyle) -> String {
    let mut out = format!(
        "font-family: \"{}\"{}; font-size: {}pt; text-align: {}; margin: {}pt {}pt {}pt {}pt; text-indent: {}pt; line-height: {};",
        style.font,
        if style.mono { ", monospace" } else { "" },
        style.size,
        style.align,
        style.space_before,
        style.indent_right,
        style.space_after,
        style.indent,
        style.first_indent,
        style.line_spacing * 1.2,
    );
    out.push_str(if style.bold {
        " font-weight: bold;"
    } else {
        " font-weight: normal;"
    });
    if style.italic {
        out.push_str(" font-style: italic;");
    }
    if style.page_break {
        out.push_str(" page-break-before: always; break-before: page;");
    }
    out
}

/// One paragraph as HTML. `notes` collects footnotes (numbered from
/// `notes.len() + 1`); `figure` names a picture's file.
pub(crate) fn para_html(
    book: &Book,
    para: &Para,
    notes: &mut Vec<String>,
    figure: &dyn Fn(sue_core::EntityId) -> Option<String>,
) -> String {
    if let Some(table) = book.table(para) {
        return table_html(table);
    }
    if let Some(contents) = book.contents_of(para) {
        let mut out = String::from("<nav class=\"contents\">");
        if !contents.title.is_empty() {
            let _ = write!(out, "<h2>{}</h2>", escape(&contents.title));
        }
        out.push_str("<ul>");
        for (level, title) in book.headings(contents) {
            let _ = write!(out, "<li class=\"level-{level}\">{}</li>", escape(&title));
        }
        out.push_str("</ul></nav>");
        return out;
    }
    let mut inner = String::new();
    if let Some(id) = para.embed
        && let Some(src) = figure(id)
    {
        let _ = write!(inner, "<img src=\"{}\" alt=\"\"/>", escape(&src));
    }
    for (text, look) in &para.runs {
        let mut piece = escape(text).replace(sue_model::LINE_BREAK, "<br/>");
        if look.code {
            piece = format!("<code>{piece}</code>");
        }
        if look.bold {
            piece = format!("<strong>{piece}</strong>");
        }
        if look.italic {
            piece = format!("<em>{piece}</em>");
        }
        if look.underline {
            piece = format!("<u>{piece}</u>");
        }
        if look.strike {
            piece = format!("<s>{piece}</s>");
        }
        if let Some(change) = &look.deleted {
            piece = format!(
                "<del title=\"{}, {}\">{piece}</del>",
                escape(&change.author),
                escape(&change.date)
            );
        } else if let Some(change) = &look.inserted {
            piece = format!(
                "<ins title=\"{}, {}\">{piece}</ins>",
                escape(&change.author),
                escape(&change.date)
            );
        }
        match look.script {
            1 => piece = format!("<sup>{piece}</sup>"),
            -1 => piece = format!("<sub>{piece}</sub>"),
            _ => {}
        }
        let mut css = String::new();
        if let Some(font) = &look.font {
            let _ = write!(css, "font-family: \"{}\";", escape(font));
        }
        if let Some(size) = look.size {
            let _ = write!(css, "font-size: {size}pt;");
        }
        if let Some(color) = look.color {
            let _ = write!(css, "color: #{};", hex(color));
        }
        if let Some(color) = look.highlight {
            let _ = write!(css, "background: #{};", hex(color));
        }
        if look.small_caps {
            css.push_str("font-variant: small-caps;");
        }
        if look.caps {
            css.push_str("text-transform: uppercase;");
        }
        if !css.is_empty() {
            piece = format!("<span style=\"{css}\">{piece}</span>");
        }
        if let Some(link) = &look.link {
            piece = format!("<a href=\"{}\">{piece}</a>", escape(link));
        }
        if let Some(thread) = look.comment.and_then(|id| book.comments.get(&id)) {
            let said: Vec<String> = std::iter::once(&thread.first)
                .chain(&thread.replies)
                .map(|note| format!("{}: {}", note.author, note.text))
                .collect();
            piece = format!(
                "<mark class=\"comment\" title=\"{}\">{piece}</mark>",
                escape(&said.join("\n"))
            );
        }
        inner.push_str(&piece);
        if let Some(note) = look.footnote.as_ref().or(look.endnote.as_ref()) {
            notes.push(note.clone());
            let n = notes.len();
            let _ = write!(
                inner,
                "<sup><a href=\"#note-{n}\" id=\"ref-{n}\">{n}</a></sup>"
            );
        }
    }
    if inner.is_empty() {
        inner.push_str("<br/>");
    }
    let mut style = String::new();
    if let Some(align) = &para.align {
        let _ = write!(style, "text-align: {align};");
    }
    if let Some(indent) = para.indent {
        let _ = write!(style, "margin-left: {indent}pt;");
    }
    if let Some(first) = para.first_indent {
        let _ = write!(style, "text-indent: {first}pt;");
    }
    if para.page_break {
        style.push_str("page-break-before: always;");
    }
    let attrs = if style.is_empty() {
        format!(" class=\"{}\"", class(&para.style))
    } else {
        format!(" class=\"{}\" style=\"{style}\"", class(&para.style))
    };
    match para.outline(book) {
        Some(level) => {
            let level = level.clamp(1, 6);
            format!("<h{level}{attrs}>{inner}</h{level}>")
        }
        None => format!("<p{attrs}>{inner}</p>"),
    }
}

/// A table as HTML: column widths, heading rows in `thead`.
pub(crate) fn table_html(table: &crate::publish::PublishTable) -> String {
    let columns = table.columns().max(1);
    let mut out = String::from("<table class=\"sue-table\"><colgroup>");
    for column in 0..columns {
        let _ = write!(
            out,
            "<col style=\"width: {:.2}%\"/>",
            table.share(column) * 100.0
        );
    }
    out.push_str("</colgroup>");
    let cell = |row: usize, column: usize| escape(table.cell(row, column)).replace('\n', "<br/>");
    let heading = table.header_rows.min(table.cells.len());
    if heading > 0 {
        out.push_str("<thead>");
        for row in 0..heading {
            out.push_str("<tr>");
            for column in 0..columns {
                let _ = write!(out, "<th>{}</th>", cell(row, column));
            }
            out.push_str("</tr>");
        }
        out.push_str("</thead>");
    }
    out.push_str("<tbody>");
    for row in heading..table.cells.len() {
        out.push_str("<tr>");
        for column in 0..columns {
            let _ = write!(out, "<td>{}</td>", cell(row, column));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    out
}

/// The body of a run of paragraphs: lists become `ul` and `ol`.
pub(crate) fn body_html(
    book: &Book,
    text: &str,
    marks: &TextMarks,
    notes: &mut Vec<String>,
    figure: &dyn Fn(sue_core::EntityId) -> Option<String>,
) -> String {
    let mut out = String::new();
    let mut open: Vec<&'static str> = Vec::new();
    for para in paragraphs(text, marks) {
        let want = para
            .list
            .as_deref()
            .map(|list| (if list == "number" { "ol" } else { "ul" }, para.level + 1));
        let depth = want.map_or(0, |(_, depth)| depth);
        while open.len() > depth {
            let tag = open.pop().unwrap_or("ul");
            let _ = writeln!(out, "</{tag}>");
        }
        if let Some((tag, depth)) = want {
            while open.len() < depth {
                open.push(tag);
                let _ = writeln!(out, "<{tag}>");
            }
            let item = para_html(book, &para, notes, figure);
            let _ = writeln!(out, "<li>{item}</li>");
            continue;
        }
        out.push_str(&para_html(book, &para, notes, figure));
        out.push('\n');
    }
    while let Some(tag) = open.pop() {
        let _ = writeln!(out, "</{tag}>");
    }
    out
}

/// Footnotes as an HTML list.
pub(crate) fn notes_html(notes: &[String]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut out = String::from("<section class=\"footnotes\"><ol>\n");
    for (index, note) in notes.iter().enumerate() {
        let n = index + 1;
        let _ = writeln!(
            out,
            "<li id=\"note-{n}\">{} <a href=\"#ref-{n}\">\u{21A9}</a></li>",
            escape(note)
        );
    }
    out.push_str("</ol></section>\n");
    out
}

/// The book as one HTML page with its styles. Pictures are inlined as
/// data URLs.
#[must_use]
pub fn to_html(book: &Book) -> String {
    let mut notes = Vec::new();
    let figure = |id: sue_core::EntityId| {
        book.figures
            .get(&id)
            .map(|figure| format!("data:image/png;base64,{}", base64(&figure.png)))
    };
    let mut body = String::new();
    for section in &book.sections {
        body.push_str(&body_html(
            book,
            &section.text,
            &section.marks,
            &mut notes,
            &figure,
        ));
    }
    body.push_str(&notes_html(&notes));
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\"/>\n<title>{}</title>\n<meta name=\"author\" content=\"{}\"/>\n<style>\n{}</style>\n</head>\n<body>\n{}</body>\n</html>\n",
        escape(&book.language),
        escape(&book.title),
        escape(&book.author),
        css(book),
        body
    )
}

/// Standard base64.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(TABLE[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// HTML read into rich text: paragraphs and headings, emphasis, code,
/// links, lists and quotes. Scripts and styles are skipped.
#[must_use]
pub fn from_html(html: &str) -> (String, TextMarks) {
    let mut text = String::new();
    let mut marks = TextMarks::new();
    let mut open: Vec<(String, usize, Option<String>)> = Vec::new();
    let mut lists: Vec<&'static str> = Vec::new();
    let mut skip = 0_usize;
    // Footnotes as Sue writes them: a numbered link to a list at the end.
    let mut in_ref = false;
    let mut refs: Vec<(String, usize)> = Vec::new();
    let mut notes_open = 0_usize;
    let mut note: Option<(String, String)> = None;
    let mut notes: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut para: Option<(usize, Option<String>, Option<&'static str>, usize)> = None;
    let blocks = [
        "p",
        "div",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "li",
        "blockquote",
        "pre",
        "tr",
    ];
    let end_para =
        |text: &mut String,
         marks: &mut TextMarks,
         para: &mut Option<(usize, Option<String>, Option<&'static str>, usize)>| {
            if let Some((start, style, list, level)) = para.take() {
                if text.len() == start {
                    return;
                }
                text.push('\n');
                let at = start..start;
                if let Some(style) = style {
                    marks.set_paragraphs(text, at.clone(), kind::STYLE, Some(style.into()));
                }
                if let Some(list) = list {
                    marks.set_paragraphs(text, at.clone(), kind::LIST, Some(list.into()));
                    if level > 0 {
                        marks.set_paragraphs(text, at, kind::LEVEL, Some(Value::Int(level as i64)));
                    }
                }
            }
        };
    for event in parse(html) {
        match event {
            XmlEvent::Open { name, attrs, empty } => {
                let name = name.to_ascii_lowercase();
                if matches!(name.as_str(), "script" | "style" | "head") && !empty {
                    skip += 1;
                    continue;
                }
                let attr = |key: &str| {
                    attrs
                        .iter()
                        .find(|(known, _)| known == key)
                        .map(|(_, value)| value.as_str())
                };
                if name == "section"
                    && attr("class").is_some_and(|class| class.contains("footnotes"))
                {
                    notes_open = 1;
                    continue;
                }
                if notes_open > 0 {
                    if name == "section" {
                        notes_open += 1;
                    }
                    if name == "li" {
                        note = attr("id")
                            .and_then(|id| id.strip_prefix("note-"))
                            .map(|number| (number.to_owned(), String::new()));
                    }
                    continue;
                }
                if name == "a"
                    && let Some(number) = attr("href").and_then(|href| href.strip_prefix("#note-"))
                {
                    refs.push((number.to_owned(), text.len()));
                    in_ref = true;
                    continue;
                }
                if name == "br" {
                    end_para(&mut text, &mut marks, &mut para);
                    continue;
                }
                if name == "ul" || name == "ol" {
                    lists.push(if name == "ol" { "number" } else { "bullet" });
                    continue;
                }
                if blocks.contains(&name.as_str()) {
                    end_para(&mut text, &mut marks, &mut para);
                    let style = match name.as_str() {
                        "h1" | "h2" | "h3" | "h4" => Some(format!("heading{}", &name[1..])),
                        "h5" | "h6" => Some("heading4".to_owned()),
                        "blockquote" => Some("quote".to_owned()),
                        "pre" => Some("code".to_owned()),
                        _ => None,
                    };
                    let list = (name == "li").then(|| lists.last().copied()).flatten();
                    para = Some((text.len(), style, list, lists.len().saturating_sub(1)));
                    continue;
                }
                if !empty {
                    let href = attrs
                        .iter()
                        .find(|(key, _)| key == "href")
                        .map(|(_, value)| value.clone());
                    open.push((name, text.len(), href));
                }
            }
            XmlEvent::Close(name) => {
                let name = name.to_ascii_lowercase();
                if matches!(name.as_str(), "script" | "style" | "head") {
                    skip = skip.saturating_sub(1);
                    continue;
                }
                if notes_open > 0 {
                    if name == "section" {
                        notes_open -= 1;
                    }
                    if name == "li"
                        && let Some((number, body)) = note.take()
                    {
                        let body = body.trim().trim_end_matches('\u{21A9}').trim().to_owned();
                        notes.insert(number, body);
                    }
                    continue;
                }
                if name == "a" && in_ref {
                    in_ref = false;
                    continue;
                }
                if name == "ul" || name == "ol" {
                    end_para(&mut text, &mut marks, &mut para);
                    lists.pop();
                    continue;
                }
                if blocks.contains(&name.as_str()) {
                    end_para(&mut text, &mut marks, &mut para);
                    continue;
                }
                if let Some(index) = open.iter().rposition(|(known, ..)| *known == name) {
                    let (tag, start, href) = open.remove(index);
                    let range = start..text.len();
                    let mark = match tag.as_str() {
                        "b" | "strong" => Some((kind::BOLD, Value::Bool(true))),
                        "i" | "em" => Some((kind::ITALIC, Value::Bool(true))),
                        "u" => Some((kind::UNDERLINE, Value::Bool(true))),
                        "s" | "del" | "strike" => Some((kind::STRIKE, Value::Bool(true))),
                        "code" => Some((kind::CODE, Value::Bool(true))),
                        "sup" => Some((kind::SCRIPT, Value::Text("super".into()))),
                        "sub" => Some((kind::SCRIPT, Value::Text("sub".into()))),
                        "a" => href.map(|href| (kind::LINK, Value::Text(href))),
                        _ => None,
                    };
                    if let Some((name, value)) = mark {
                        marks.set(range, name, Some(value));
                    }
                }
            }
            XmlEvent::Text(chunk) => {
                if notes_open > 0 {
                    if let Some((_, body)) = &mut note {
                        body.push_str(&chunk);
                    }
                    continue;
                }
                if skip > 0 || in_ref {
                    continue;
                }
                let collapsed: String = chunk.split_whitespace().collect::<Vec<_>>().join(" ");
                if collapsed.is_empty() {
                    if chunk.contains(' ') && !text.ends_with([' ', '\n']) && para.is_some() {
                        text.push(' ');
                    }
                    continue;
                }
                if para.is_none() {
                    para = Some((text.len(), None, None, 0));
                }
                if chunk.starts_with(char::is_whitespace)
                    && !text.ends_with([' ', '\n'])
                    && text.len() > para.as_ref().map_or(0, |p| p.0)
                {
                    text.push(' ');
                }
                text.push_str(&collapsed);
                if chunk.ends_with(char::is_whitespace) {
                    text.push(' ');
                }
            }
        }
    }
    end_para(&mut text, &mut marks, &mut para);
    for (number, at) in refs {
        if let Some(body) = notes.get(&number)
            && at > 0
        {
            let start = text[..at]
                .char_indices()
                .next_back()
                .map_or(0, |(start, _)| start);
            marks.set(start..at, kind::FOOTNOTE, Some(Value::Text(body.clone())));
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    marks.fit(&text);
    (text, marks)
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_comments_mark_their_words() {
        let html = to_html(&super::super::tests::with_comment());
        assert!(
            html.contains("<mark class=\"comment\" title=\"Ann: Too much?\nBo: Keep it.\">"),
            "{html}"
        );
    }

    #[test]
    fn html_contents_are_a_list_of_the_headings() {
        let html = to_html(&super::super::tests::with_contents());
        assert!(html.contains("<nav class=\"contents\"><h2>Contents</h2><ul><li class=\"level-1\">One</li><li class=\"level-2\">Two</li></ul></nav>"), "{html}");
    }

    use super::super::tests::sample;
    use super::super::tests::with_table;
    use super::*;

    #[test]
    fn html_goes_out_and_back() {
        let book = sample();
        let html = to_html(&book);
        assert!(
            html.contains("<h1 class=\"heading1\">My Story</h1>"),
            "{html}"
        );
        assert!(html.contains("<strong>dark</strong>"));
        assert!(html.contains("<ul>\n<li><p class=\"body\">First</p></li>"));
        assert!(html.contains("id=\"note-1\""));
        let (text, marks) = from_html(&html);
        assert!(
            text.starts_with("My Story\nIt was a dark and stormy night."),
            "{text:?}"
        );
        assert_eq!(
            marks
                .paragraph_value(&text, 0, kind::STYLE)
                .and_then(Value::as_str),
            Some("heading1")
        );
        let dark = text.find("dark").unwrap();
        assert!(marks.covers(dark..dark + 4, kind::BOLD));
        assert!(text.ends_with("stormy night.\nFirst\nSecond\n"), "{text:?}");
        let note = marks
            .marks()
            .iter()
            .find(|mark| mark.kind == kind::FOOTNOTE)
            .unwrap();
        assert_eq!(note.value.as_str(), Some("Or so they say."));
        assert_eq!(base64(b"Sue!"), "U3VlIQ==");
    }

    #[test]
    fn html_tables_have_a_head_and_a_body() {
        let html = to_html(&with_table());
        assert!(
            html.contains("<thead><tr><th>Item</th><th>Count &amp; size</th></tr></thead>"),
            "{html}"
        );
        assert!(html.contains("<tbody><tr><td>Bolt &lt;M8&gt;</td><td>40</td></tr></tbody>"));
        assert!(html.contains("width: 70.00%"));
    }
}
