//! Markdown out and in: headings, emphasis, strikethrough, code, links,
//! lists, quotes, code blocks and footnotes.

use sue_core::Value;
use sue_model::{TextMarks, mark_kind as kind};

use super::{Book, paragraphs};

/// Escapes the characters Markdown would read as markup.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`' | '[' | ']' | '~') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// The book as Markdown.
#[must_use]
pub fn to_markdown(book: &Book) -> String {
    let mut out = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut number = 0;
    for section in &book.sections {
        let mut in_code = false;
        for para in paragraphs(&section.text, &section.marks) {
            let style = book.style(&para.style);
            if style.mono {
                if !in_code {
                    out.push_str("```\n");
                    in_code = true;
                }
                out.push_str(&para.text());
                out.push('\n');
                continue;
            }
            if in_code {
                out.push_str("```\n\n");
                in_code = false;
            }
            if para.embed.is_some() {
                out.push_str("![](picture)\n\n");
                continue;
            }
            let mut line = String::new();
            for (text, look) in &para.runs {
                let mut piece = if look.code {
                    format!("`{text}`")
                } else {
                    escape(text)
                };
                if !look.code && !text.trim().is_empty() {
                    let (lead, core, trail) = split_spaces(&piece);
                    let mut core = core.to_owned();
                    if look.strike {
                        core = format!("~~{core}~~");
                    }
                    if look.italic {
                        core = format!("*{core}*");
                    }
                    if look.bold {
                        core = format!("**{core}**");
                    }
                    if let Some(link) = &look.link {
                        core = format!("[{core}]({link})");
                    }
                    piece = format!("{lead}{core}{trail}");
                }
                line.push_str(&piece);
                if let Some(note) = &look.footnote {
                    notes.push(note.clone());
                    line.push_str(&format!("[^{}]", notes.len()));
                }
            }
            match (para.outline(book), para.list.as_deref()) {
                (Some(level), _) => {
                    out.push_str(&"#".repeat(usize::from(level).clamp(1, 6)));
                    out.push(' ');
                    out.push_str(&line);
                    out.push_str("\n\n");
                }
                (None, Some(list)) => {
                    out.push_str(&"   ".repeat(para.level));
                    if list == "number" {
                        number += 1;
                        out.push_str(&format!("{number}. "));
                    } else {
                        out.push_str("- ");
                    }
                    out.push_str(&line);
                    out.push('\n');
                    continue;
                }
                (None, None) if para.style == "quote" => {
                    out.push_str("> ");
                    out.push_str(&line);
                    out.push_str("\n\n");
                }
                _ => {
                    if line.is_empty() {
                        continue;
                    }
                    out.push_str(&line);
                    out.push_str("\n\n");
                }
            }
            number = 0;
        }
        if in_code {
            out.push_str("```\n\n");
        }
    }
    for (index, note) in notes.iter().enumerate() {
        out.push_str(&format!("[^{}]: {}\n", index + 1, escape(note)));
    }
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    out
}

fn split_spaces(text: &str) -> (&str, &str, &str) {
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    (
        &text[..start],
        &text[start..end.max(start)],
        &text[end.max(start)..],
    )
}

/// Markdown read into rich text: headings, `**bold**`, `*italic*`,
/// `~~strike~~`, `` `code` ``, links, lists, quotes and code blocks.
#[must_use]
pub fn from_markdown(markdown: &str) -> (String, TextMarks) {
    let mut text = String::new();
    let mut marks = TextMarks::new();
    let mut in_code = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        let start = text.len();
        if in_code {
            text.push_str(line);
            text.push('\n');
            marks.set_paragraphs(&text, start..start, kind::STYLE, Some("code".into()));
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
        let (body, style, list, level) =
            if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
                (
                    &trimmed[hashes + 1..],
                    Some(format!("heading{}", hashes.min(4))),
                    None,
                    0,
                )
            } else if let Some(rest) = trimmed.strip_prefix("> ") {
                (rest, Some("quote".to_owned()), None, 0)
            } else if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "))
            {
                (rest, None, Some("bullet"), (line.len() - trimmed.len()) / 2)
            } else if let Some(dot) = trimmed
                .find(". ")
                .filter(|dot| *dot > 0 && trimmed[..*dot].bytes().all(|b| b.is_ascii_digit()))
            {
                (
                    &trimmed[dot + 2..],
                    None,
                    Some("number"),
                    (line.len() - trimmed.len()) / 3,
                )
            } else {
                (trimmed, None, None, 0)
            };
        inline(body, &mut text, &mut marks);
        text.push('\n');
        let at = start..start;
        if let Some(style) = style {
            marks.set_paragraphs(&text, at.clone(), kind::STYLE, Some(style.into()));
        }
        if let Some(list) = list {
            marks.set_paragraphs(&text, at.clone(), kind::LIST, Some(list.into()));
            if level > 0 {
                marks.set_paragraphs(
                    &text,
                    at,
                    kind::LEVEL,
                    Some(Value::Int(level.min(8) as i64)),
                );
            }
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    (text, marks)
}

/// Appends a line's inline Markdown to `text` as marks.
fn inline(line: &str, text: &mut String, marks: &mut TextMarks) {
    let mut open: Vec<(&'static str, &'static str, usize)> = Vec::new();
    let mut rest = line;
    while let Some(ch) = rest.chars().next() {
        if ch == '\\' && rest.len() > 1 {
            let next = rest[1..].chars().next().unwrap_or('\\');
            text.push(next);
            rest = &rest[1 + next.len_utf8()..];
            continue;
        }
        if ch == '`'
            && let Some(end) = rest[1..].find('`')
        {
            let start = text.len();
            text.push_str(&rest[1..1 + end]);
            marks.set(start..text.len(), kind::CODE, Some(true.into()));
            rest = &rest[end + 2..];
            continue;
        }
        if ch == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close..].find(')')
        {
            let label = &rest[1..close];
            let url = &rest[close + 2..close + end];
            let start = text.len();
            inline(label, text, marks);
            marks.set(start..text.len(), kind::LINK, Some(url.into()));
            rest = &rest[close + end + 1..];
            continue;
        }
        let token = [
            ("**", kind::BOLD),
            ("__", kind::BOLD),
            ("~~", kind::STRIKE),
            ("*", kind::ITALIC),
            ("_", kind::ITALIC),
        ]
        .into_iter()
        .find(|(token, _)| rest.starts_with(token));
        if let Some((token, name)) = token {
            if let Some(index) = open.iter().rposition(|(known, _, _)| *known == token) {
                let (_, name, start) = open.remove(index);
                marks.set(start..text.len(), name, Some(true.into()));
                rest = &rest[token.len()..];
                continue;
            }
            // Only opens when something follows on this line that closes it.
            if rest[token.len()..].contains(token) && !rest[token.len()..].starts_with(' ') {
                open.push((token, name, text.len()));
                rest = &rest[token.len()..];
                continue;
            }
        }
        text.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::*;

    #[test]
    fn markdown_goes_out_and_back() {
        let book = sample();
        let markdown = to_markdown(&book);
        assert!(markdown.starts_with("# My Story\n"), "{markdown}");
        assert!(markdown.contains("**dark**"));
        assert!(markdown.contains("*and*"));
        assert!(markdown.contains("[^1]: Or so they say."));
        assert!(markdown.contains("- First\n- Second"));
        let (text, marks) = from_markdown(
            "# Title\n\nSome **bold** and *it* `x` [here](https://a.b)\n\n- one\n  - two\n",
        );
        assert_eq!(text, "Title\nSome bold and it x here\none\ntwo\n");
        assert_eq!(
            marks
                .paragraph_value(&text, 0, kind::STYLE)
                .and_then(Value::as_str),
            Some("heading1")
        );
        assert!(marks.covers(11..15, kind::BOLD));
        assert!(marks.covers(20..22, kind::ITALIC));
        assert_eq!(
            marks.value_at(26, kind::LINK).and_then(Value::as_str),
            Some("https://a.b")
        );
        assert_eq!(
            marks.paragraph_value(&text, 3, kind::LEVEL),
            Some(&Value::Int(1))
        );
    }
}
