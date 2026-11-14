//! The Format menu: character formats, colors, alignment, line spacing,
//! lists and indents, and paragraph styles.

use sue_core::Value;
use sue_model::mark_kind as kind;
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use crate::studio::{SueWriter, font_step};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "format", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("format.bold", "bold").keys("Ctrl+B"),
    c("format.italic", "italic").keys("Ctrl+I"),
    c("format.underline", "underline").keys("Ctrl+U"),
    c("format.strike", "strikethrough").keys("Ctrl+Shift+X"),
    c("format.superscript", "superscript").keys("Ctrl+Shift++ or Ctrl+Shift+="),
    c("format.subscript", "subscript").keys("Ctrl+="),
    c("format.small_caps", "small-capitals"),
    c("format.caps", "all-capitals"),
    c("format.grow", "grow-font").keys("Ctrl+]"),
    c("format.shrink", "shrink-font").keys("Ctrl+["),
    c("format.code", "code"),
    // `format.case.upper`: one of `sue_type::case::Case::ALL`.
    c("format.case.", "change-case").with(Flags::FAMILY),
    // `format.color.c0392b`: a color as hex, or `none`.
    c("format.color.", "text-color").with(Flags::FAMILY),
    // `format.highlight.yellow`: a highlighter's color, else none.
    c("format.highlight.", "highlight").with(Flags::FAMILY),
    c("format.clear", "clear-direct-formatting").keys("Ctrl+Space"),
    c("align.left", "left").keys("Ctrl+L"),
    c("align.center", "center").keys("Ctrl+E"),
    c("align.right", "right").keys("Ctrl+R"),
    c("align.justify", "justify").keys("Ctrl+J"),
    c("spacing.1", "line-spacing-single"),
    c("spacing.1.15", "line-spacing-1-15"),
    c("spacing.1.5", "line-spacing-1-5"),
    c("spacing.2", "line-spacing-double"),
    c("list.bullet", "bulleted-list").keys("Ctrl+Shift+L"),
    c("list.number", "numbered-list"),
    c("indent.more", "increase-indent"),
    c("indent.less", "decrease-indent"),
    // One per paragraph style of the built-in style sheets
    // (`sue_type::StyleSheet::named`).
    c("style.body", "body-text").keys("Ctrl+Alt+0"),
    c("style.title", "style-title"),
    c("style.subtitle", "style-subtitle"),
    c("style.heading1", "heading-1").keys("Ctrl+Alt+1"),
    c("style.heading2", "heading-2").keys("Ctrl+Alt+2"),
    c("style.heading3", "heading-3").keys("Ctrl+Alt+3"),
    c("style.heading4", "heading-4"),
    c("style.quote", "style-quote"),
    c("style.code", "style-code"),
    c("style.break", "style-break"),
    c("style.caption", "style-caption"),
    c("style.chapter", "style-chapter"),
    c("style.scene", "style-scene"),
    c("style.character", "style-character"),
    c("style.parenthetical", "style-parenthetical"),
    c("style.dialogue", "style-dialogue"),
    c("style.transition", "style-transition"),
];

/// Does what a Format menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    let sheet = writer.sheet(cx);
    match command {
        "format.bold" => writer.editor.toggle(cx, kind::BOLD, &tr!("bold")),
        "format.italic" => writer.editor.toggle(cx, kind::ITALIC, &tr!("italic")),
        "format.underline" => writer.editor.toggle(cx, kind::UNDERLINE, &tr!("underline")),
        "format.strike" => writer
            .editor
            .toggle(cx, kind::STRIKE, &tr!("strikethrough")),
        "format.code" => writer.editor.toggle(cx, kind::CODE, &tr!("code")),
        case if case.starts_with("format.case.") => {
            let Some(case) = sue_type::case::Case::parse(&case["format.case.".len()..]) else {
                return;
            };
            writer.editor.change_case(cx, case);
        }
        "format.caps" => writer.editor.toggle(cx, kind::CAPS, &tr!("all-capitals")),
        "format.grow" | "format.shrink" => {
            let (para, chars) = writer.editor.look_here(&sheet);
            let now = chars.size.unwrap_or(para.size);
            let next = font_step(now, command == "format.grow");
            writer.editor.set_char(
                cx,
                kind::SIZE,
                Some(Value::Float(f64::from(next))),
                &tr!("font-size"),
            );
        }
        "format.small_caps" => writer
            .editor
            .toggle(cx, kind::SMALL_CAPS, &tr!("small-capitals")),
        "format.superscript" | "format.subscript" => {
            let script = if command.ends_with("superscript") {
                "super"
            } else {
                "sub"
            };
            let (_, look) = writer.editor.look_here(&sheet);
            let on = look.script == if script == "super" { 1 } else { -1 };
            writer.editor.set_char(
                cx,
                kind::SCRIPT,
                (!on).then(|| Value::Text(script.to_owned())),
                &tr!("raise-or-lower"),
            );
        }
        "format.clear" => writer.editor.clear_formatting(cx),
        command if command.starts_with("format.highlight.") => {
            let color = match &command["format.highlight.".len()..] {
                "yellow" => Some("#fff176"),
                "green" => Some("#a5e3a1"),
                "blue" => Some("#9fd2ff"),
                "pink" => Some("#ffb3d1"),
                _ => None,
            };
            writer.editor.set_char(
                cx,
                kind::HIGHLIGHT,
                color.map(|hex| Value::Text(hex.to_owned())),
                &tr!("highlight"),
            );
        }
        command if command.starts_with("format.color.") => {
            let hex = &command["format.color.".len()..];
            let value = (hex != "none").then(|| Value::Text(format!("#{hex}")));
            writer
                .editor
                .set_char(cx, kind::COLOR, value, &tr!("text-color"));
        }
        command if command.starts_with("style.") => {
            let key = &command["style.".len()..];
            writer.editor.set_style(cx, key, &tr!("paragraph-style"));
        }
        command if command.starts_with("align.") => {
            let align = &command["align.".len()..];
            let value = (align != "left").then(|| Value::Text(align.to_owned()));
            writer
                .editor
                .set_paragraph(cx, kind::ALIGN, value, &tr!("alignment"));
        }
        command if command.starts_with("spacing.") => {
            let spacing: f64 = command["spacing.".len()..].parse().unwrap_or(1.0);
            writer.editor.set_paragraph(
                cx,
                kind::LINE_SPACING,
                Some(Value::Float(spacing)),
                &tr!("line-spacing"),
            );
        }
        "list.bullet" | "list.number" => {
            let list = &command["list.".len()..];
            let (para, _) = writer.editor.look_here(&sheet);
            let on = para.list.map(sue_type::ListKind::name) == Some(list);
            writer.editor.set_paragraph(
                cx,
                kind::LIST,
                (!on).then(|| Value::Text(list.to_owned())),
                &tr!("list"),
            );
        }
        "indent.more" | "indent.less" => {
            let (para, _) = writer.editor.look_here(&sheet);
            if para.list.is_some() {
                let level = if command.ends_with("more") {
                    (para.level + 1).min(8)
                } else {
                    para.level.saturating_sub(1)
                };
                writer.editor.set_paragraph(
                    cx,
                    kind::LEVEL,
                    Some(Value::Int(level as i64)),
                    &tr!("list-level"),
                );
            } else {
                let style = sheet.get(&para.style).indent;
                let own = para.indent - style;
                let next = if command.ends_with("more") {
                    own + 36.0
                } else {
                    (own - 36.0).max(0.0)
                };
                writer.editor.set_paragraph(
                    cx,
                    kind::INDENT,
                    (next > 0.0).then(|| Value::Float(f64::from(style + next))),
                    &tr!("indent"),
                );
            }
        }
        _ => {}
    }
}

/// Whether a Format menu command works now: with a text open.
fn state(writer: &SueWriter, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    if writer.editor.current().is_some() {
        ItemState::on()
    } else {
        ItemState::off()
    }
}
