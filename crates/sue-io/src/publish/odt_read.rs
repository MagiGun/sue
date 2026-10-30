//! Reading OpenDocument text (`.odt`) into rich text: paragraph styles
//! (headings, quotes, title), alignment and page breaks, character
//! formatting from automatic and named styles, lists (bulleted or
//! numbered, nested), links, footnotes, tables (as table entities) and a
//! table of contents (as a contents entity). Pictures, frames and comments
//! are left out for now.

use std::collections::HashMap;

use sue_core::{Entity, Value};
use sue_model::{TextMarks, mark_kind as kind};

use super::docx::Imported;
use super::xml::{XmlEvent, parse};
use crate::zip::read_zip;

/// What a style says about the characters it covers.
#[derive(Clone, Debug, Default, PartialEq)]
struct Look {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    script: Option<&'static str>,
    color: Option<String>,
    highlight: Option<String>,
    size: Option<f32>,
    font: Option<String>,
    small_caps: Option<bool>,
    caps: Option<bool>,
}

impl Look {
    /// `self` with `over`'s settings on top.
    fn and(&self, over: &Self) -> Self {
        Self {
            bold: over.bold.or(self.bold),
            italic: over.italic.or(self.italic),
            underline: over.underline.or(self.underline),
            strike: over.strike.or(self.strike),
            script: over.script.or(self.script),
            color: over.color.clone().or_else(|| self.color.clone()),
            highlight: over.highlight.clone().or_else(|| self.highlight.clone()),
            size: over.size.or(self.size),
            font: over.font.clone().or_else(|| self.font.clone()),
            small_caps: over.small_caps.or(self.small_caps),
            caps: over.caps.or(self.caps),
        }
    }
}

/// A style of any family, as read.
#[derive(Clone, Debug, Default)]
struct Style {
    parent: Option<String>,
    look: Look,
    align: Option<String>,
    page_break: bool,
    /// A column's width in points (table columns).
    width: Option<f32>,
    /// The list style a paragraph style brings.
    list: Option<String>,
}

/// A length (`12pt`, `1.5in`, `2.54cm`, `10mm`, `16px`) in points.
fn points(length: &str) -> Option<f32> {
    let length = length.trim();
    let split = length
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-'))
        .unwrap_or(length.len());
    let (number, unit) = length.split_at(split);
    let number: f32 = number.parse().ok()?;
    Some(match unit {
        "pt" | "" => number,
        "in" => number * 72.0,
        "cm" => number * 72.0 / 2.54,
        "mm" => number * 72.0 / 25.4,
        "px" => number * 0.75,
        "pc" => number * 12.0,
        _ => return None,
    })
}

/// The paragraph style key Sue uses for an OpenDocument style name.
fn style_key(name: &str) -> String {
    let plain = name.replace("_20_", " ").to_ascii_lowercase();
    match plain.as_str() {
        "standard" | "text body" | "body text" | "default paragraph style" | "text_body" => {
            "body".to_owned()
        }
        "title" => "title".to_owned(),
        "subtitle" => "subtitle".to_owned(),
        "quotations" | "quote" => "quote".to_owned(),
        _ => {
            if let Some(level) = plain
                .strip_prefix("heading ")
                .and_then(|level| level.parse::<u8>().ok())
            {
                format!("heading{}", level.clamp(1, 4))
            } else if let Some(own) = plain.strip_prefix("sue ") {
                own.replace(' ', "")
            } else {
                "body".to_owned()
            }
        }
    }
}

/// The styles of `xml` (both `office:styles` and automatic ones), and
/// which list styles are numbered at their first level.
fn read_styles(xml: &str, styles: &mut HashMap<String, Style>, lists: &mut HashMap<String, bool>) {
    let mut current: Option<String> = None;
    let mut list: Option<String> = None;
    for event in parse(xml) {
        match &event {
            XmlEvent::Open { name, .. } => match name.as_str() {
                "style:style" => {
                    current = event.attr("style:name").map(str::to_owned);
                    if let Some(id) = &current {
                        let style = styles.entry(id.clone()).or_default();
                        style.parent = event.attr("style:parent-style-name").map(str::to_owned);
                        style.list = event.attr("style:list-style-name").map(str::to_owned);
                    }
                }
                "text:list-style" => list = event.attr("style:name").map(str::to_owned),
                "text:list-level-style-number" | "text:list-level-style-bullet" => {
                    if let Some(id) = &list
                        && event.attr("text:level") == Some("1")
                    {
                        lists.insert(id.clone(), name == "text:list-level-style-number");
                    }
                }
                "style:text-properties" => {
                    if let Some(style) = current.as_ref().and_then(|id| styles.get_mut(id)) {
                        style.look = text_look(&event);
                    }
                }
                "style:paragraph-properties" => {
                    if let Some(style) = current.as_ref().and_then(|id| styles.get_mut(id)) {
                        style.align = match event.attr("fo:text-align") {
                            Some("center") => Some("center".to_owned()),
                            Some("end" | "right") => Some("right".to_owned()),
                            Some("justify") => Some("justify".to_owned()),
                            Some("start" | "left") => Some("left".to_owned()),
                            _ => None,
                        };
                        style.page_break = event.attr("fo:break-before") == Some("page");
                    }
                }
                "style:table-column-properties" => {
                    if let Some(style) = current.as_ref().and_then(|id| styles.get_mut(id)) {
                        style.width = event.attr("style:column-width").and_then(points);
                    }
                }
                _ => {}
            },
            XmlEvent::Close(name) if name == "style:style" => current = None,
            XmlEvent::Close(name) if name == "text:list-style" => list = None,
            _ => {}
        }
    }
}

/// What `style:text-properties` says.
fn text_look(event: &XmlEvent) -> Look {
    let yes = |key: &str, off: &[&str]| event.attr(key).map(|value| !off.contains(&value));
    Look {
        bold: event
            .attr("fo:font-weight")
            .map(|weight| weight == "bold" || weight.parse::<u32>().is_ok_and(|w| w >= 600)),
        italic: yes("fo:font-style", &["normal"]),
        underline: yes("style:text-underline-style", &["none"]),
        strike: yes("style:text-line-through-style", &["none"]),
        script: event.attr("style:text-position").and_then(|position| {
            if position.starts_with("super") || position.starts_with("33%") {
                Some("super")
            } else if position.starts_with("sub") || position.starts_with('-') {
                Some("sub")
            } else {
                None
            }
        }),
        color: event
            .attr("fo:color")
            .filter(|color| *color != "#000000")
            .map(str::to_owned),
        highlight: event
            .attr("fo:background-color")
            .filter(|color| *color != "transparent")
            .map(str::to_owned),
        size: event.attr("fo:font-size").and_then(points),
        font: event
            .attr("style:font-name")
            .or_else(|| event.attr("fo:font-family"))
            .map(|font| font.trim_matches(['\'', '"']).to_owned()),
        small_caps: event
            .attr("fo:font-variant")
            .map(|variant| variant == "small-caps"),
        caps: event
            .attr("fo:text-transform")
            .map(|transform| transform == "uppercase"),
    }
}

/// A style's settings with its parents' under them.
fn resolved(styles: &HashMap<String, Style>, name: &str) -> Style {
    let mut chain = Vec::new();
    let mut at = Some(name.to_owned());
    while let Some(id) = at.take() {
        if chain.len() > 16 {
            break;
        }
        if let Some(style) = styles.get(&id) {
            at.clone_from(&style.parent);
            chain.push((id, style.clone()));
        }
    }
    let mut out = Style::default();
    for (_, style) in chain.iter().rev() {
        out.look = out.look.and(&style.look);
        out.align = style.align.clone().or(out.align);
        out.page_break |= style.page_break;
        out.list = style.list.clone().or(out.list);
    }
    out
}

/// The paragraph style key of style `name`: its own, or the first of its
/// parents Sue knows.
fn paragraph_key(styles: &HashMap<String, Style>, name: &str) -> String {
    let mut at = Some(name.to_owned());
    let mut steps = 0;
    while let Some(id) = at.take() {
        let key = style_key(&id);
        if key != "body" || styles.get(&id).is_none_or(|style| style.parent.is_none()) {
            return key;
        }
        at = styles.get(&id).and_then(|style| style.parent.clone());
        steps += 1;
        if steps > 16 {
            break;
        }
    }
    "body".to_owned()
}

/// A table being read.
#[derive(Default)]
struct TableRead {
    start: usize,
    rows: Vec<Vec<String>>,
    row: Vec<String>,
    cell: usize,
    header_rows: u32,
    in_header: bool,
    widths: Vec<f32>,
}

/// Reads an OpenDocument text into rich text, its tables and table of
/// contents as entities set in the text.
///
/// # Errors
/// If it is not an OpenDocument text.
#[allow(clippy::too_many_lines, reason = "one pass over the document")]
pub fn read_odt(bytes: &[u8]) -> Result<Imported, String> {
    let files: HashMap<String, Vec<u8>> = read_zip(bytes)?.into_iter().collect();
    let read = |name: &str| {
        files
            .get(name)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    };
    let content = read("content.xml").ok_or("this is not an OpenDocument text")?;
    let mut styles: HashMap<String, Style> = HashMap::new();
    let mut lists: HashMap<String, bool> = HashMap::new();
    if let Some(xml) = read("styles.xml") {
        read_styles(&xml, &mut styles, &mut lists);
    }
    read_styles(&content, &mut styles, &mut lists);

    let mut text = String::new();
    let mut marks = TextMarks::new();
    let mut entities: Vec<Entity> = Vec::new();
    // The paragraph under way: where it starts, its style key, alignment,
    // page break and character look.
    let mut paragraph: Option<(usize, String, Option<String>, bool, Look)> = None;
    let mut spans: Vec<Look> = Vec::new();
    let mut list_styles: Vec<Option<String>> = Vec::new();
    let mut link: Option<(usize, String)> = None;
    let mut note: Option<String> = None;
    let mut citation = false;
    let mut skip = 0_usize;
    let mut table_depth = 0_usize;
    let mut table: Option<TableRead> = None;
    let mut in_body = false;
    for event in parse(&content) {
        let open = |wanted: &str| matches!(&event, XmlEvent::Open { name, .. } if name == wanted);
        let close = |wanted: &str| matches!(&event, XmlEvent::Close(name) if name == wanted);
        if open("office:text") {
            in_body = true;
            continue;
        }
        if !in_body {
            continue;
        }
        // What is left out: pictures and frames, comments, and a table of
        // contents' generated body (Sue makes its own).
        if let XmlEvent::Open { name, empty, .. } = &event
            && matches!(
                name.as_str(),
                "draw:frame"
                    | "office:annotation"
                    | "text:index-body"
                    | "text:table-of-content-source"
            )
            && !*empty
        {
            skip += 1;
            if name == "text:index-body" && skip == 1 {
                // The table of contents itself becomes a contents entity.
                let contents = sue_model::contents::Contents::default();
                let entity = contents.entity();
                let at = text.len();
                text.push(sue_model::OBJECT);
                text.push('\n');
                marks.set(
                    at..at + sue_model::OBJECT.len_utf8(),
                    kind::EMBED,
                    Some(Value::Entity(entity.id())),
                );
                entities.push(entity);
            }
            continue;
        }
        if let XmlEvent::Close(name) = &event
            && matches!(
                name.as_str(),
                "draw:frame"
                    | "office:annotation"
                    | "text:index-body"
                    | "text:table-of-content-source"
            )
        {
            skip = skip.saturating_sub(1);
            continue;
        }
        if skip > 0 {
            continue;
        }
        // Tables.
        if open("table:table") {
            table_depth += 1;
            if table_depth == 1 {
                table = Some(TableRead {
                    start: text.len(),
                    ..TableRead::default()
                });
            }
            continue;
        }
        if table_depth == 1
            && let Some(read) = &mut table
        {
            match &event {
                XmlEvent::Open { name, .. } if name == "table:table-column" => {
                    let width = event
                        .attr("table:style-name")
                        .and_then(|style| styles.get(style))
                        .and_then(|style| style.width)
                        .unwrap_or(0.0);
                    let repeat: usize = event
                        .attr("table:number-columns-repeated")
                        .and_then(|count| count.parse().ok())
                        .unwrap_or(1);
                    for _ in 0..repeat.min(256) {
                        read.widths.push(width);
                    }
                }
                XmlEvent::Open { name, .. } if name == "table:table-header-rows" => {
                    read.in_header = true;
                }
                XmlEvent::Close(name) if name == "table:table-header-rows" => {
                    read.in_header = false;
                }
                XmlEvent::Open { name, empty, .. } if name == "table:table-row" => {
                    read.row.clear();
                    if *empty {
                        read.rows.push(Vec::new());
                    }
                }
                XmlEvent::Open { name, empty, .. } if name == "table:table-cell" => {
                    read.cell = text.len();
                    if *empty {
                        read.row.push(String::new());
                    }
                }
                XmlEvent::Close(name) if name == "table:table-cell" => {
                    let cell = text[read.cell.min(text.len())..]
                        .trim_end_matches('\n')
                        .to_owned();
                    read.row.push(cell);
                }
                XmlEvent::Close(name) if name == "table:table-row" => {
                    if read.in_header {
                        read.header_rows += 1;
                    }
                    read.rows.push(std::mem::take(&mut read.row));
                }
                _ => {}
            }
        }
        if close("table:table") {
            if table_depth == 1
                && let Some(read) = table.take()
            {
                let start = read.start.min(text.len());
                marks.delete_text(&text, start..text.len());
                text.truncate(start);
                let entity = sue_model::text_table(&read.rows, read.header_rows, &read.widths);
                let at = text.len();
                text.push(sue_model::OBJECT);
                text.push('\n');
                marks.set(
                    at..at + sue_model::OBJECT.len_utf8(),
                    kind::EMBED,
                    Some(Value::Entity(entity.id())),
                );
                entities.push(entity);
            }
            table_depth = table_depth.saturating_sub(1);
            continue;
        }
        // Footnotes: the citation is left out, the body is the note.
        match &event {
            XmlEvent::Open { name, empty, .. } if name == "text:note" && !*empty => {
                note = Some(String::new());
                continue;
            }
            XmlEvent::Open { name, empty, .. } if name == "text:note-citation" && !*empty => {
                citation = true;
                continue;
            }
            XmlEvent::Close(name) if name == "text:note-citation" => {
                citation = false;
                continue;
            }
            XmlEvent::Close(name) if name == "text:note" => {
                if let Some(body) = note.take() {
                    let start = paragraph.as_ref().map_or(0, |(start, ..)| *start);
                    let end = text.len();
                    if end > start
                        && let Some((at, _)) = text[..end].char_indices().next_back()
                    {
                        marks.set(at..end, kind::FOOTNOTE, Some(body.trim().to_owned().into()));
                    }
                }
                continue;
            }
            _ => {}
        }
        if let Some(body) = &mut note {
            match &event {
                XmlEvent::Text(chunk) if !citation => body.push_str(chunk),
                XmlEvent::Close(name) if name == "text:p" && !body.is_empty() => body.push(' '),
                _ => {}
            }
            continue;
        }
        match &event {
            XmlEvent::Open { name, empty, .. } => match name.as_str() {
                "text:p" | "text:h" => {
                    let style = event.attr("text:style-name").unwrap_or("Standard");
                    let mut key = paragraph_key(&styles, style);
                    if name == "text:h" && !key.starts_with("heading") && key != "title" {
                        let level: u8 = event
                            .attr("text:outline-level")
                            .and_then(|level| level.parse().ok())
                            .unwrap_or(1);
                        key = format!("heading{}", level.clamp(1, 4));
                    }
                    let own = resolved(&styles, style);
                    let start = text.len();
                    paragraph = Some((start, key, own.align, own.page_break, own.look));
                    spans.clear();
                    if *empty {
                        close_paragraph(
                            &mut text,
                            &mut marks,
                            &mut paragraph,
                            &list_styles,
                            &lists,
                            &styles,
                        );
                    }
                }
                "text:span" if !*empty => {
                    let look = event
                        .attr("text:style-name")
                        .map(|style| resolved(&styles, style).look)
                        .unwrap_or_default();
                    spans.push(look);
                }
                "text:a" if !*empty => {
                    link = event
                        .attr("xlink:href")
                        .map(|href| (text.len(), href.to_owned()));
                }
                "text:s" => {
                    let count: usize = event
                        .attr("text:c")
                        .and_then(|count| count.parse().ok())
                        .unwrap_or(1);
                    text.push_str(&" ".repeat(count.min(1000)));
                }
                "text:tab" => text.push('\t'),
                "text:line-break" => text.push(' '),
                "text:list" if !*empty => {
                    let style = event
                        .attr("text:style-name")
                        .map(str::to_owned)
                        .or_else(|| list_styles.last().cloned().flatten());
                    list_styles.push(style);
                }
                _ => {}
            },
            XmlEvent::Text(chunk) if paragraph.is_some() => {
                let start = text.len();
                text.push_str(chunk);
                let base = paragraph
                    .as_ref()
                    .map(|(.., look)| look.clone())
                    .unwrap_or_default();
                let look = spans.iter().fold(base, |all, span| all.and(span));
                apply(&mut marks, start..text.len(), &look);
            }
            XmlEvent::Close(name) => match name.as_str() {
                "text:span" => {
                    spans.pop();
                }
                "text:a" => {
                    if let Some((start, href)) = link.take()
                        && text.len() > start
                    {
                        marks.set(start..text.len(), kind::LINK, Some(href.into()));
                    }
                }
                "text:p" | "text:h" => {
                    close_paragraph(
                        &mut text,
                        &mut marks,
                        &mut paragraph,
                        &list_styles,
                        &lists,
                        &styles,
                    );
                }
                "text:list" => {
                    list_styles.pop();
                }
                _ => {}
            },
            _ => {}
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    marks.fit(&text);
    Ok(Imported {
        text,
        marks,
        entities,
    })
}

/// Reads an OpenDocument text for its rich text alone.
///
/// # Errors
/// If it is not an OpenDocument text.
pub fn from_odt(bytes: &[u8]) -> Result<(String, TextMarks), String> {
    read_odt(bytes).map(|read| (read.text, read.marks))
}

/// Ends the paragraph under way: its newline and its own marks (style,
/// alignment, page break, list).
fn close_paragraph(
    text: &mut String,
    marks: &mut TextMarks,
    paragraph: &mut Option<(usize, String, Option<String>, bool, Look)>,
    list_styles: &[Option<String>],
    lists: &HashMap<String, bool>,
    styles: &HashMap<String, Style>,
) {
    let Some((start, key, align, page_break, _)) = paragraph.take() else {
        return;
    };
    text.push('\n');
    let at = start..start;
    if key != "body" {
        marks.set_paragraphs(text, at.clone(), kind::STYLE, Some(key.into()));
    }
    if let Some(align) = align.filter(|align| align != "left") {
        marks.set_paragraphs(text, at.clone(), kind::ALIGN, Some(align.into()));
    }
    if page_break && start > 0 {
        marks.set_paragraphs(text, at.clone(), kind::PAGE_BREAK, Some(true.into()));
    }
    if !list_styles.is_empty() {
        let numbered = list_styles
            .iter()
            .rev()
            .flatten()
            .next()
            .and_then(|style| {
                lists.get(style).copied().or_else(|| {
                    styles
                        .get(style)
                        .and_then(|paragraph| paragraph.list.as_ref())
                        .and_then(|list| lists.get(list).copied())
                })
            })
            .unwrap_or(false);
        let list = if numbered { "number" } else { "bullet" };
        marks.set_paragraphs(text, at.clone(), kind::LIST, Some(list.into()));
        if list_styles.len() > 1 {
            marks.set_paragraphs(
                text,
                at,
                kind::LEVEL,
                Some(Value::Int((list_styles.len() - 1).min(8) as i64)),
            );
        }
    }
}

/// Sets `look`'s character marks over `range`.
fn apply(marks: &mut TextMarks, range: std::ops::Range<usize>, look: &Look) {
    let mut set = |name: &str, value: Value| marks.set(range.clone(), name, Some(value));
    for (on, name) in [
        (look.bold, kind::BOLD),
        (look.italic, kind::ITALIC),
        (look.underline, kind::UNDERLINE),
        (look.strike, kind::STRIKE),
        (look.small_caps, kind::SMALL_CAPS),
        (look.caps, kind::CAPS),
    ] {
        if on == Some(true) {
            set(name, Value::Bool(true));
        }
    }
    if let Some(script) = look.script {
        set(kind::SCRIPT, Value::Text(script.to_owned()));
    }
    if let Some(color) = &look.color {
        set(kind::COLOR, Value::Text(color.clone()));
    }
    if let Some(color) = &look.highlight {
        set(kind::HIGHLIGHT, Value::Text(color.clone()));
    }
    if let Some(size) = look.size {
        set(kind::SIZE, Value::Float(f64::from(size)));
    }
    if let Some(font) = &look.font {
        set(kind::FONT, Value::Text(font.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{sample, with_contents, with_table};
    use super::*;
    use crate::publish::to_odt;

    #[test]
    fn opendocument_texts_come_back_with_their_formatting() {
        let book = sample();
        let (text, marks) = from_odt(&to_odt(&book)).unwrap();
        assert_eq!(text, book.sections[0].text, "{text:?}");
        assert_eq!(
            marks.paragraph_value(&text, 0, kind::STYLE),
            Some(&Value::Text("heading1".into()))
        );
        assert!(marks.covers(18..22, kind::BOLD), "{marks:?}");
        assert!(marks.covers(23..26, kind::ITALIC));
        assert!(marks.marks().iter().any(|mark| mark.kind == kind::FOOTNOTE
            && mark.value == Value::Text("Or so they say.".into())));
        assert_eq!(
            marks.paragraph_value(&text, 2, kind::LIST),
            Some(&Value::Text("bullet".into()))
        );
    }

    #[test]
    fn opendocument_tables_and_contents_come_back_as_entities() {
        let read = read_odt(&to_odt(&with_table())).unwrap();
        assert_eq!(read.entities.len(), 1);
        let table = read.entities[0].ext::<sue_model::Table>().unwrap();
        assert_eq!(
            table.texts(),
            vec![vec!["Item", "Count & size"], vec!["Bolt <M8>", "40"]]
        );
        assert_eq!(
            read.entities[0].property(sue_model::HEADER_ROWS),
            Some(&Value::Int(1))
        );
        assert_eq!(
            read.text,
            format!("Parts\n{}\nThe end.\n", sue_model::OBJECT)
        );
        let read = read_odt(&to_odt(&with_contents())).unwrap();
        assert_eq!(read.entities[0].class(), sue_model::contents::CONTENTS);
        assert!(
            read.text
                .starts_with(&format!("{}\nOne\n", sue_model::OBJECT)),
            "{:?}",
            read.text
        );
        assert!(from_odt(b"nope").is_err());
        assert_eq!(points("1in"), Some(72.0));
        assert!((points("2.54cm").unwrap() - 72.0).abs() < 0.01);
    }
}
