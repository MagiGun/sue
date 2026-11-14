//! The Insert menu: breaks, notes, links, pictures, dates and formatting
//! marks.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "insert")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("insert.page_break", "page-break").keys("Ctrl+Enter"),
    c("insert.scene_break", "scene-break"),
    c("insert.footnote", "footnote-ellipsis").keys("Ctrl+Alt+F"),
    c("insert.endnote", "endnote-ellipsis").keys("Ctrl+Alt+D"),
    c("insert.header_footer", "header-and-footer-ellipsis"),
    c("insert.contents", "table-of-contents-ellipsis"),
    c("insert.link", "link-ellipsis").keys("Ctrl+K"),
    c("insert.picture", "picture-ellipsis").with(Flags::LEAVES_CANVAS),
    c("insert.date", "todays-date"),
    c("insert.mark.nbsp", "non-breaking-space").keys("Ctrl+Shift+Space"),
    c("insert.mark.nbhyphen", "non-breaking-hyphen"),
    c("insert.mark.soft_hyphen", "soft-hyphen"),
    c("insert.mark.narrow", "narrow-non-breaking-space"),
    c("insert.mark.zwsp", "zero-width-space"),
];
