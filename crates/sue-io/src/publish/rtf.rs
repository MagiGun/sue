//! Rich Text Format, which every word processor opens.

use std::fmt::Write as _;

use super::{Book, paragraphs};

/// Text escaped for RTF: braces and backslashes, everything past ASCII as
/// a Unicode escape.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '{' | '}' => {
                out.push('\\');
                out.push(ch);
            }
            '\t' => out.push_str("\\tab "),
            sue_model::LINE_BREAK => out.push_str("\\line "),
            ch if ch.is_ascii() => out.push(ch),
            ch => {
                let mut units = [0_u16; 2];
                for unit in ch.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{}?", *unit as i16);
                }
            }
        }
    }
    out
}

fn twips(points: f32) -> i32 {
    (points * 20.0).round() as i32
}

/// The book as RTF.
#[must_use]
pub fn to_rtf(book: &Book) -> String {
    let mut fonts: Vec<String> = Vec::new();
    let mut font = |name: &str| -> usize {
        if let Some(index) = fonts.iter().position(|known| known == name) {
            return index;
        }
        fonts.push(name.to_owned());
        fonts.len() - 1
    };
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut body = String::new();
    for (index, section) in book.sections.iter().enumerate() {
        if index > 0 && section.new_page {
            body.push_str("\\page\n");
        }
        for para in paragraphs(&section.text, &section.marks) {
            let style = book.style(&para.style);
            let base = font(&style.font);
            if let Some(table) = book.table(&para) {
                let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
                body.push_str(&table_rtf(table, width, base, style.size));
                continue;
            }
            if let Some(contents) = book.contents_of(&para) {
                let size = (style.size * 2.0).round() as i32;
                if !contents.title.is_empty() {
                    let _ = writeln!(
                        body,
                        "\\pard\\plain\\f{base}\\fs{}\\b {}\\par",
                        (style.size * 2.6).round() as i32,
                        escape(&contents.title)
                    );
                }
                for (level, title) in book.headings(contents) {
                    let _ = writeln!(
                        body,
                        "\\pard\\plain\\f{base}\\fs{size}\\li{} {}\\par",
                        twips(f32::from(level.saturating_sub(1)) * 18.0),
                        escape(&title)
                    );
                }
                continue;
            }
            let _ = write!(
                body,
                "\\pard\\plain\\f{base}\\fs{}",
                (style.size * 2.0).round() as i32
            );
            if style.bold {
                body.push_str("\\b");
            }
            if style.italic {
                body.push_str("\\i");
            }
            body.push_str(match para.alignment(book) {
                "center" => "\\qc",
                "right" => "\\qr",
                "justify" => "\\qj",
                _ => "\\ql",
            });
            let indent = para.indent.unwrap_or(style.indent)
                + if para.list.is_some() {
                    18.0 * (para.level as f32 + 1.0)
                } else {
                    0.0
                };
            let first = if para.list.is_some() {
                -18.0
            } else {
                para.first_indent.unwrap_or(style.first_indent)
            };
            let spacing = para.line_spacing.unwrap_or(style.line_spacing);
            let _ = write!(
                body,
                "\\li{}\\ri{}\\fi{}\\sb{}\\sa{}\\sl{}\\slmult1",
                twips(indent),
                twips(style.indent_right),
                twips(first),
                twips(para.space_before.unwrap_or(style.space_before)),
                twips(para.space_after.unwrap_or(style.space_after)),
                (spacing * 240.0).round() as i32
            );
            if style.page_break || para.page_break {
                body.push_str("\\pagebb");
            }
            if let Some(level) = style.outline {
                let _ = write!(body, "\\outlinelevel{}", level.saturating_sub(1));
            }
            body.push(' ');
            match para.list.as_deref() {
                Some("bullet") => body.push_str("\\bullet\\tab "),
                Some(_) => body.push_str("\\tab "),
                None => {}
            }
            for (text, look) in &para.runs {
                body.push('{');
                if look.bold {
                    body.push_str("\\b");
                }
                if look.italic {
                    body.push_str("\\i");
                }
                if look.underline || look.link.is_some() {
                    body.push_str("\\ul");
                }
                if look.strike {
                    body.push_str("\\strike");
                }
                if look.small_caps {
                    body.push_str("\\scaps");
                }
                if look.caps {
                    body.push_str("\\caps");
                }
                match look.script {
                    1 => body.push_str("\\super"),
                    -1 => body.push_str("\\sub"),
                    _ => {}
                }
                if look.code {
                    let _ = write!(body, "\\f{}", font("Courier New"));
                } else if let Some(name) = &look.font {
                    let _ = write!(body, "\\f{}", font(name));
                }
                if let Some(size) = look.size {
                    let _ = write!(body, "\\fs{}", (size * 2.0).round() as i32);
                }
                if let Some(color) = look.color {
                    let index = colors
                        .iter()
                        .position(|known| *known == color)
                        .unwrap_or_else(|| {
                            colors.push(color);
                            colors.len() - 1
                        });
                    let _ = write!(body, "\\cf{}", index + 1);
                }
                body.push(' ');
                body.push_str(&escape(text));
                body.push('}');
                if let Some(note) = &look.footnote {
                    let _ = write!(
                        body,
                        "{{\\super\\chftn}}{{\\footnote\\pard\\plain\\fs20 {{\\super\\chftn}} {}}}",
                        escape(note)
                    );
                }
            }
            body.push_str("\\par\n");
        }
    }
    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1\n{\\fonttbl");
    for (index, name) in fonts.iter().enumerate() {
        let _ = write!(out, "{{\\f{index}\\fnil {};}}", escape(name));
    }
    out.push_str("}\n{\\colortbl;");
    for [r, g, b] in &colors {
        let _ = write!(out, "\\red{r}\\green{g}\\blue{b};");
    }
    out.push_str("}\n");
    let _ = writeln!(
        out,
        "{{\\info{{\\title {}}}{{\\author {}}}}}",
        escape(&book.title),
        escape(&book.author)
    );
    let [top, right, bottom, left] = book.margins;
    let _ = writeln!(
        out,
        "\\paperw{}\\paperh{}\\margt{}\\margr{}\\margb{}\\margl{}\\fet0",
        twips(book.page[0]),
        twips(book.page[1]),
        twips(top),
        twips(right),
        twips(bottom),
        twips(left)
    );
    let width = (book.page[0] - book.margins[1] - book.margins[3]).max(72.0);
    if book.different_first {
        out.push_str("\\titlepg{\\headerf\\pard\\par}{\\footerf\\pard\\par}\n");
    }
    for (running, group) in [(&book.header, "header"), (&book.footer, "footer")] {
        if !running.is_empty() {
            out.push_str(&running_rtf(book, running, group, width));
        }
    }
    out.push_str(&body);
    out.push_str("}\n");
    out
}

/// A running header or footer group (`header` or `footer`): its parts
/// parted by tabs at a centered and a right tab stop, its page fields
/// RTF's own.
fn running_rtf(book: &Book, running: &sue_model::Running, group: &str, width: f32) -> String {
    use crate::publish::Live;
    let mut out = format!(
        "{{\\{group}\\pard\\plain\\tqc\\tx{}\\tqr\\tx{}\\fs18 ",
        twips(width / 2.0),
        twips(width)
    );
    for (index, part) in book.live(running).iter().enumerate() {
        if index > 0 {
            out.push_str("\\tab ");
        }
        for piece in part {
            match piece {
                Live::Text(text) => out.push_str(&escape(text)),
                Live::Page => out.push_str("{\\field{\\*\\fldinst PAGE}{\\fldrslt 1}}"),
                Live::Pages => out.push_str("{\\field{\\*\\fldinst NUMPAGES}{\\fldrslt 1}}"),
            }
        }
    }
    out.push_str("\\par}\n");
    out
}

/// A table as RTF rows, `width` points across, in font `font` at `size`.
fn table_rtf(table: &crate::publish::PublishTable, width: f32, font: usize, size: f32) -> String {
    let columns = table.columns().max(1);
    let mut out = String::new();
    for (row, cells) in table.cells.iter().enumerate() {
        out.push_str("\\trowd\\trgaph72");
        if row < table.header_rows {
            out.push_str("\\trhdr");
        }
        let mut right = 0.0;
        for column in 0..columns {
            right += width * table.share(column);
            for side in ["t", "l", "b", "r"] {
                let _ = write!(out, "\\clbrdr{side}\\brdrs\\brdrw10");
            }
            let _ = write!(out, "\\cellx{}", twips(right));
        }
        out.push('\n');
        for column in 0..columns {
            let text = cells.get(column).map_or("", String::as_str);
            let _ = writeln!(
                out,
                "\\pard\\intbl\\plain\\f{font}\\fs{}{} {}\\cell",
                (size * 2.0).round() as i32,
                if row < table.header_rows { "\\b" } else { "" },
                escape(text).replace('\n', "\\line ")
            );
        }
        out.push_str("\\row\n");
    }
    out.push_str("\\pard\n");
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn rtf_headers_and_footers_carry_live_page_fields() {
        let mut book = sample();
        book.header = sue_model::Running::parse("{title}");
        book.footer = sue_model::Running::parse("\t\t{page} of {pages}");
        book.different_first = true;
        let rtf = to_rtf(&book);
        assert!(rtf.contains("{\\header\\pard"), "{rtf}");
        assert!(rtf.contains("\\tab My Story\\tab"));
        assert!(rtf.contains("\\fldinst NUMPAGES"));
        assert!(rtf.contains("\\titlepg"));
    }

    use super::super::tests::sample;
    use super::super::tests::with_table;
    use super::*;

    #[test]
    fn rtf_carries_styles_runs_and_notes() {
        let rtf = to_rtf(&sample());
        assert!(rtf.starts_with("{\\rtf1"));
        assert!(rtf.contains("{\\b dark}"), "{rtf}");
        assert!(rtf.contains("\\footnote"));
        assert!(rtf.contains("\\bullet\\tab"));
        assert_eq!(rtf.matches('{').count(), rtf.matches('}').count());
        assert_eq!(escape("é{"), "\\u233?\\{");
    }

    #[test]
    fn rtf_tables_are_rows_of_cells() {
        let rtf = to_rtf(&with_table());
        assert_eq!(rtf.matches("\\row").count(), 2, "{rtf}");
        assert_eq!(rtf.matches("\\trhdr").count(), 1);
        assert_eq!(rtf.matches("\\cell\n").count(), 4);
    }
}
