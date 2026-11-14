//! The View menu: the text, the corkboard and the outliner, pages, panels,
//! composition mode and zoom.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "view")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("view.document", "text").keys("Ctrl+1"),
    c("view.corkboard", "corkboard").keys("Ctrl+2"),
    c("view.outliner", "outliner").keys("Ctrl+3"),
    c("view.page", "page-view"),
    c("view.draft", "draft-view"),
    c("view.binder", "binder"),
    c("view.inspector", "inspector"),
    c("view.composition", "composition-mode")
        .keys("F11")
        .with(Flags::PALETTE),
    c("view.typewriter", "typewriter-scrolling"),
    c("view.zoom_in", "zoom-in"),
    c("view.zoom_out", "zoom-out"),
    c("view.zoom_reset", "actual-size"),
    c("view.zoom_fit", "fit-to-window"),
];
