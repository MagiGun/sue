//! The Project menu: documents split, merged, copied and trashed,
//! snapshots, status and labels, statistics and the project's settings.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "project")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("project.split", "split-at-selection").keys("Ctrl+Shift+K"),
    c("project.merge", "merge-with-next"),
    c("project.duplicate", "duplicate"),
    c("project.trash", "move-to-trash"),
    c("project.empty_trash", "empty-trash"),
    c("project.snapshot", "take-snapshot").keys("Ctrl+5"),
    c("project.include", "include-in-compile"),
    // `status.draft`: one of `project::STATUSES`, or `none`.
    c("status.", "status").with(Flags::FAMILY),
    // `label.idea`: one of `project::LABELS`, or `none`.
    c("label.", "label").with(Flags::FAMILY),
    c("project.statistics", "writing-statistics-ellipsis").with(Flags::PALETTE),
    c("project.settings", "project-settings-ellipsis"),
];
