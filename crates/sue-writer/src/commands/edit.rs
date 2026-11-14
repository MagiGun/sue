//! The Edit menu: undo, redo, find and replace.

use sue_wm::Command;

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "edit")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    // The shell's Undo and Redo keys run these.
    c("edit.undo", "undo"),
    c("edit.redo", "redo"),
    c("edit.find", "find-ellipsis").keys("Ctrl+F"),
    c("edit.replace", "replace-ellipsis").keys("Ctrl+H"),
    c("edit.find_next", "find-next").keys("F3"),
    c("edit.find_previous", "find-previous").keys("Shift+F3"),
];
