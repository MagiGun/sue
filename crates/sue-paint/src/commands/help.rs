//! The Help menu's commands.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "help", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("help.guide", "sue-paint-guide").with(Flags::LEAVES_CANVAS),
    c("help.shortcuts", "keyboard-shortcuts")
        .keys("Ctrl+Alt+Shift+K or F1 or Alt+I")
        .with(Flags::PALETTE),
    c("help.updates", "check-for-updates").with(Flags::LEAVES_CANVAS),
    c("help.about", "about-sue-paint"),
];
