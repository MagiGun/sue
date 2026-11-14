//! The Help menu.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "help")
}

pub(super) const COMMANDS: &[Command<SueWriter>] =
    &[c("help.shortcuts", "keyboard-shortcuts").with(Flags::PALETTE)];
