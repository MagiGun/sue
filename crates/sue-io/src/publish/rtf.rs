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
    if book.page_numbers {
        out.push_str(
            "{\\footer\\pard\\qc\\plain\\fs20 {\\field{\\*\\fldinst PAGE}{\\fldrslt 1}}\\par}\n",
        );
    }
    out.push_str(&body);
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
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
}
