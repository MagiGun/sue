//! The Format menu: character formats, colors, alignment, line spacing,
//! lists and indents, and paragraph styles.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "format")
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
