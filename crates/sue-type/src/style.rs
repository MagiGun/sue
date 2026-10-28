//! Paragraph styles and style sheets. A paragraph names its style with a
//! [`kind::STYLE`] mark; its own marks (alignment, indents, spacing, a
//! list) then override the style, as direct formatting does in a word
//! processor. [`paragraph_look`] resolves the two into what a paragraph
//! looks like; every view and every exporter reads that, so a paragraph
//! looks the same on screen, in a PDF and in a Word file.
//!
//! Lengths are in points (1/72 inch), as in print.

use sue_core::Value;
use sue_model::{Mark, mark_kind as kind};

pub use sue_model::{CharFormat, char_format, marks_by_paragraph, points_of, rgb_of, runs};

/// How a paragraph's lines sit between its margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// Flush left (the default).
    #[default]
    Left,
    /// Centered.
    Center,
    /// Flush right.
    Right,
    /// Flush on both sides, the last line flush left.
    Justify,
}

impl Align {
    /// Every alignment.
    pub const ALL: [Self; 4] = [Self::Left, Self::Center, Self::Right, Self::Justify];

    /// Reads a [`kind::ALIGN`] value.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "left" | "start" => Self::Left,
            "center" | "centre" => Self::Center,
            "right" | "end" => Self::Right,
            "justify" | "both" => Self::Justify,
            _ => return None,
        })
    }

    /// Its [`kind::ALIGN`] value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Justify => "justify",
        }
    }

    /// What people read.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Align Left",
            Self::Center => "Center",
            Self::Right => "Align Right",
            Self::Justify => "Justify",
        }
    }
}

/// A list a paragraph belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// Bullets.
    Bullet,
    /// Numbers.
    Number,
}

impl ListKind {
    /// Reads a [`kind::LIST`] value.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "bullet" => Some(Self::Bullet),
            "number" => Some(Self::Number),
            _ => None,
        }
    }

    /// Its [`kind::LIST`] value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Bullet => "bullet",
            Self::Number => "number",
        }
    }
}

/// A named paragraph style.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphStyle {
    /// Its key, stored in [`kind::STYLE`] marks, e.g. `heading1`.
    pub key: String,
    /// What people read, e.g. `Heading 1`.
    pub name: String,
    /// Its font family (`None` uses the sheet's body font).
    pub font: Option<String>,
    /// Its size in points.
    pub size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Typed letters become capitals (screenplay scene headings).
    pub caps: bool,
    /// A text color (`None` is the page's ink).
    pub color: Option<[u8; 3]>,
    /// Drawn in the page's lighter ink (subtitles).
    pub soft: bool,
    /// Alignment.
    pub align: Align,
    /// Left indent.
    pub indent: f32,
    /// Right indent.
    pub indent_right: f32,
    /// First-line indent.
    pub first_indent: f32,
    /// Space above.
    pub space_before: f32,
    /// Space below.
    pub space_after: f32,
    /// Line spacing, a multiple of single spacing.
    pub line_spacing: f32,
    /// Starts a new page.
    pub page_break: bool,
    /// Stays on the page of the paragraph after it (headings).
    pub keep_with_next: bool,
    /// Its outline level, 1 for the biggest headings; `None` for body text.
    pub outline: Option<u8>,
    /// The style Enter at its end goes on with (`None`: the same).
    pub next: Option<String>,
    /// The style Tab turns it into (screenplays).
    pub tab: Option<String>,
    /// A shaded block behind it (code).
    pub shaded: bool,
    /// A rule at its left (quotes).
    pub rule: bool,
}

impl ParagraphStyle {
    /// A body style of `size` points.
    #[must_use]
    pub fn new(key: &str, name: &str, size: f32) -> Self {
        Self {
            key: key.to_owned(),
            name: name.to_owned(),
            font: None,
            size,
            bold: false,
            italic: false,
            underline: false,
            caps: false,
            color: None,
            soft: false,
            align: Align::Left,
            indent: 0.0,
            indent_right: 0.0,
            first_indent: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            line_spacing: 1.0,
            page_break: false,
            keep_with_next: false,
            outline: None,
            next: None,
            tab: None,
            shaded: false,
            rule: false,
        }
    }

    fn heading(key: &str, name: &str, size: f32, level: u8) -> Self {
        Self {
            outline: Some(level),
            keep_with_next: true,
            next: Some(BODY.to_owned()),
            ..Self::new(key, name, size)
        }
    }
}

/// The body text style every sheet has.
pub const BODY: &str = "body";

/// A set of paragraph styles that belong together.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleSheet {
    /// Its key, e.g. `standard`.
    pub key: String,
    /// What people read.
    pub name: String,
    /// The font of body text.
    pub body_font: String,
    /// The font of headings.
    pub heading_font: String,
    /// The font of code.
    pub mono_font: String,
    /// The styles, in the order the style menu lists them.
    pub styles: Vec<ParagraphStyle>,
}

/// The sheets Sue ships: key and name.
pub const SHEETS: &[(&str, &str)] = &[
    ("standard", "Standard"),
    ("manuscript", "Manuscript"),
    ("screenplay", "Screenplay"),
];

impl StyleSheet {
    /// The sheet `key` (else the standard one), with these fonts.
    #[must_use]
    pub fn named(key: &str, fonts: &Fonts) -> Self {
        match key {
            "manuscript" => Self::manuscript(fonts),
            "screenplay" => Self::screenplay(fonts),
            _ => Self::standard(fonts),
        }
    }

    /// Word processing: body text, a title, headings, quotes and code.
    #[must_use]
    pub fn standard(fonts: &Fonts) -> Self {
        let body = ParagraphStyle {
            space_after: 8.0,
            line_spacing: 1.15,
            ..ParagraphStyle::new(BODY, "Body Text", fonts.size)
        };
        let heading_font = Some(fonts.heading.clone());
        let mut styles = vec![
            body,
            ParagraphStyle {
                font: heading_font.clone(),
                space_after: 6.0,
                ..ParagraphStyle::heading("title", "Title", fonts.size * 2.3, 1)
            },
            ParagraphStyle {
                font: heading_font.clone(),
                soft: true,
                space_after: 12.0,
                outline: None,
                keep_with_next: false,
                ..ParagraphStyle::heading("subtitle", "Subtitle", fonts.size * 1.3, 1)
            },
        ];
        for (level, scale) in [(1, 1.65), (2, 1.35), (3, 1.15), (4, 1.0)] {
            styles.push(ParagraphStyle {
                font: heading_font.clone(),
                bold: level >= 3,
                italic: level == 4,
                space_before: 18.0 - f32::from(level) * 2.0,
                space_after: 6.0,
                ..ParagraphStyle::heading(
                    &format!("heading{level}"),
                    &format!("Heading {level}"),
                    fonts.size * scale,
                    level,
                )
            });
        }
        styles.extend([
            ParagraphStyle {
                italic: true,
                indent: 36.0,
                indent_right: 36.0,
                space_after: 8.0,
                line_spacing: 1.15,
                rule: true,
                ..ParagraphStyle::new("quote", "Quote", fonts.size)
            },
            ParagraphStyle {
                font: Some(fonts.mono.clone()),
                shaded: true,
                space_after: 8.0,
                ..ParagraphStyle::new("code", "Code", fonts.size * 0.85)
            },
            ParagraphStyle {
                align: Align::Center,
                space_before: 12.0,
                space_after: 12.0,
                ..ParagraphStyle::new("break", "Scene Break", fonts.size)
            },
            ParagraphStyle {
                soft: true,
                align: Align::Center,
                space_after: 8.0,
                ..ParagraphStyle::new("caption", "Caption", fonts.size * 0.85)
            },
        ]);
        Self {
            key: "standard".to_owned(),
            name: "Standard".to_owned(),
            body_font: fonts.body.clone(),
            heading_font: fonts.heading.clone(),
            mono_font: fonts.mono.clone(),
            styles,
        }
    }

    /// A novel manuscript, as publishers ask for it: double spaced, an
    /// indent on every paragraph, chapters on a new page.
    #[must_use]
    pub fn manuscript(fonts: &Fonts) -> Self {
        let mut sheet = Self::standard(fonts);
        sheet.key = "manuscript".to_owned();
        sheet.name = "Manuscript".to_owned();
        sheet.heading_font = fonts.body.clone();
        for style in &mut sheet.styles {
            if style.key == BODY {
                style.first_indent = 36.0;
                style.space_after = 0.0;
                style.line_spacing = 2.0;
            } else if style.outline.is_some() || style.key == "title" || style.key == "subtitle" {
                style.font = Some(fonts.body.clone());
                style.align = Align::Center;
            }
        }
        sheet.styles.insert(
            1,
            ParagraphStyle {
                align: Align::Center,
                page_break: true,
                space_before: 144.0,
                space_after: 36.0,
                font: Some(fonts.body.clone()),
                ..ParagraphStyle::heading("chapter", "Chapter Title", fonts.size * 1.5, 1)
            },
        );
        sheet
    }

    /// A screenplay in the standard format: Courier-like type at 12 points,
    /// each element at its own indent, Enter and Tab moving between them.
    #[must_use]
    pub fn screenplay(fonts: &Fonts) -> Self {
        let element = |key: &str, name: &str| ParagraphStyle {
            font: Some(fonts.mono.clone()),
            ..ParagraphStyle::new(key, name, 12.0)
        };
        let styles = vec![
            ParagraphStyle {
                space_before: 12.0,
                next: Some("character".to_owned()),
                tab: Some("character".to_owned()),
                ..element(BODY, "Action")
            },
            ParagraphStyle {
                caps: true,
                space_before: 24.0,
                keep_with_next: true,
                outline: Some(1),
                next: Some(BODY.to_owned()),
                tab: Some(BODY.to_owned()),
                ..element("scene", "Scene Heading")
            },
            ParagraphStyle {
                caps: true,
                indent: 158.4,
                space_before: 12.0,
                keep_with_next: true,
                next: Some("dialogue".to_owned()),
                tab: Some("parenthetical".to_owned()),
                ..element("character", "Character")
            },
            ParagraphStyle {
                indent: 115.2,
                indent_right: 144.0,
                keep_with_next: true,
                next: Some("dialogue".to_owned()),
                tab: Some("dialogue".to_owned()),
                ..element("parenthetical", "Parenthetical")
            },
            ParagraphStyle {
                indent: 72.0,
                indent_right: 108.0,
                next: Some("character".to_owned()),
                tab: Some("parenthetical".to_owned()),
                ..element("dialogue", "Dialogue")
            },
            ParagraphStyle {
                caps: true,
                align: Align::Right,
                space_before: 12.0,
                next: Some("scene".to_owned()),
                tab: Some("scene".to_owned()),
                ..element("transition", "Transition")
            },
            ParagraphStyle {
                caps: true,
                align: Align::Center,
                space_after: 24.0,
                ..element("title", "Title")
            },
        ];
        Self {
            key: "screenplay".to_owned(),
            name: "Screenplay".to_owned(),
            body_font: fonts.mono.clone(),
            heading_font: fonts.mono.clone(),
            mono_font: fonts.mono.clone(),
            styles,
        }
    }

    /// The style `key`, else body text.
    #[must_use]
    pub fn get(&self, key: &str) -> &ParagraphStyle {
        self.styles
            .iter()
            .find(|style| style.key == key)
            .or_else(|| self.styles.iter().find(|style| style.key == BODY))
            .unwrap_or(&self.styles[0])
    }

    /// `true` if the sheet has style `key`.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.styles.iter().any(|style| style.key == key)
    }

    /// A short fingerprint, to tell when layouts made with the sheet are
    /// stale.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut hasher);
        hasher.finish()
    }
}

/// The fonts a sheet is made with.
#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    /// Body text.
    pub body: String,
    /// Headings.
    pub heading: String,
    /// Code and screenplays.
    pub mono: String,
    /// The body size in points.
    pub size: f32,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            body: sue_wm::fonts::SANS.to_owned(),
            heading: sue_wm::fonts::SANS.to_owned(),
            mono: sue_wm::fonts::MONO.to_owned(),
            size: 12.0,
        }
    }
}

impl Fonts {
    /// Good fonts this device has: a serif for body text, a sans for
    /// headings and a monospace, each the first installed of a list of
    /// common ones, else Sue's own.
    #[must_use]
    pub fn installed(size: f32) -> Self {
        use sue_wm::fonts::first_installed;
        Self {
            body: first_installed(&[
                "Georgia",
                "Cambria",
                "Times New Roman",
                "Charter",
                "Noto Serif",
                "DejaVu Serif",
                "Liberation Serif",
            ]),
            heading: first_installed(&[
                "Segoe UI",
                "Helvetica Neue",
                "Arial",
                "Noto Sans",
                "DejaVu Sans",
                "Liberation Sans",
            ]),
            mono: {
                let mono = first_installed(&[
                    "Courier Prime",
                    "Courier New",
                    "Courier",
                    "Liberation Mono",
                    "DejaVu Sans Mono",
                ]);
                if mono == sue_wm::fonts::SANS {
                    sue_wm::fonts::MONO.to_owned()
                } else {
                    mono
                }
            },
            size,
        }
    }
}

/// How one paragraph looks: its style with its own marks on top.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphLook {
    /// The style's key.
    pub style: String,
    /// Font family.
    pub font: String,
    /// Size in points.
    pub size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Typed letters become capitals.
    pub caps: bool,
    /// Text color, if not the ink.
    pub color: Option<[u8; 3]>,
    /// Drawn in the lighter ink.
    pub soft: bool,
    /// Alignment.
    pub align: Align,
    /// Left indent in points (lists add to it).
    pub indent: f32,
    /// Right indent.
    pub indent_right: f32,
    /// First-line indent.
    pub first_indent: f32,
    /// Space above.
    pub space_before: f32,
    /// Space below.
    pub space_after: f32,
    /// Line spacing.
    pub line_spacing: f32,
    /// Starts a new page.
    pub page_break: bool,
    /// Stays with the next paragraph.
    pub keep_with_next: bool,
    /// Outline level.
    pub outline: Option<u8>,
    /// The list it is in.
    pub list: Option<ListKind>,
    /// Its list level.
    pub level: usize,
    /// Shaded (code).
    pub shaded: bool,
    /// A rule at its left (quotes).
    pub rule: bool,
}

/// The indent each list level adds, in points.
pub const LIST_INDENT: f32 = 22.0;

/// Resolves how a paragraph looks from its style and the paragraph marks
/// among `marks` (the marks that touch the paragraph, any order).
#[must_use]
pub fn paragraph_look(sheet: &StyleSheet, marks: &[&Mark]) -> ParagraphLook {
    let own = |name: &str| -> Option<&Value> {
        marks
            .iter()
            .rfind(|mark| mark.kind == name)
            .map(|mark| &mark.value)
    };
    // A plain heading mark (from an import) picks the heading style.
    let style_key = own(kind::STYLE)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            own(kind::HEADING)
                .and_then(Value::as_i64)
                .map(|level| format!("heading{}", level.clamp(1, 4)))
        })
        .or_else(|| own(kind::QUOTE).map(|_| "quote".to_owned()))
        .unwrap_or_else(|| BODY.to_owned());
    let style = sheet.get(&style_key);
    let number = |name: &str, default: f32| {
        own(name)
            .and_then(Value::as_f64)
            .map_or(default, |value| value as f32)
    };
    let list = own(kind::LIST)
        .and_then(Value::as_str)
        .and_then(ListKind::parse);
    let level = own(kind::LEVEL)
        .and_then(Value::as_i64)
        .map_or(0, |level| level.clamp(0, 8) as usize);
    let mut indent = number(kind::INDENT, style.indent);
    if list.is_some() {
        indent += LIST_INDENT * (level as f32 + 1.0);
    }
    ParagraphLook {
        style: style.key.clone(),
        font: style
            .font
            .clone()
            .unwrap_or_else(|| sheet.body_font.clone()),
        size: style.size,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        caps: style.caps,
        color: style.color,
        soft: style.soft,
        align: own(kind::ALIGN)
            .and_then(Value::as_str)
            .and_then(Align::parse)
            .unwrap_or(style.align),
        indent,
        indent_right: number(kind::INDENT_RIGHT, style.indent_right),
        first_indent: if list.is_some() {
            0.0
        } else {
            number(kind::FIRST_INDENT, style.first_indent)
        },
        space_before: number(kind::SPACE_BEFORE, style.space_before),
        space_after: number(kind::SPACE_AFTER, style.space_after),
        line_spacing: number(kind::LINE_SPACING, style.line_spacing).clamp(0.5, 5.0),
        page_break: own(kind::PAGE_BREAK)
            .and_then(Value::as_bool)
            .unwrap_or(style.page_break),
        keep_with_next: style.keep_with_next,
        outline: style.outline,
        list,
        level,
        shaded: style.shaded,
        rule: style.rule,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_model::TextMarks;

    #[test]
    fn a_paragraph_takes_its_style_then_its_own_marks() {
        let sheet = StyleSheet::standard(&Fonts::default());
        let text = "Title\nBody\n";
        let mut marks = TextMarks::new();
        marks.set_paragraphs(text, 0..1, kind::STYLE, Some("heading1".into()));
        marks.set_paragraphs(text, 0..1, kind::ALIGN, Some("center".into()));
        marks.set_paragraphs(text, 6..7, kind::LIST, Some("bullet".into()));
        marks.set(6..8, kind::BOLD, Some(true.into()));
        let paragraphs = sue_model::paragraphs(text);
        let by = marks_by_paragraph(marks.marks(), &paragraphs);
        let heading = paragraph_look(&sheet, &by[0]);
        assert_eq!(heading.style, "heading1");
        assert_eq!(heading.align, Align::Center);
        assert_eq!(heading.outline, Some(1));
        assert!(heading.size > sheet.get(BODY).size);
        let item = paragraph_look(&sheet, &by[1]);
        assert_eq!(item.list, Some(ListKind::Bullet));
        assert!((item.indent - LIST_INDENT).abs() < 0.01);
        let pieces = runs(&by[1], 6..10);
        assert_eq!(pieces.len(), 2);
        assert!(pieces[0].1.bold && !pieces[1].1.bold);
        assert_eq!(pieces[0].0, 6..8);
    }

    #[test]
    fn screenplay_elements_lead_to_each_other() {
        let sheet = StyleSheet::screenplay(&Fonts::default());
        assert_eq!(sheet.get("character").next.as_deref(), Some("dialogue"));
        assert_eq!(sheet.get("dialogue").tab.as_deref(), Some("parenthetical"));
        assert!(sheet.get("scene").caps);
        let manuscript = StyleSheet::manuscript(&Fonts::default());
        assert!((manuscript.get(BODY).line_spacing - 2.0).abs() < 0.01);
        assert!(manuscript.get("chapter").page_break);
        assert_ne!(sheet.fingerprint(), manuscript.fingerprint());
        assert_eq!(rgb_of(&Value::Text("#ff8000".into())), Some([255, 128, 0]));
    }
}
