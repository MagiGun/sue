//! The Help menu.

use sue_wm::{AppletContext, Call, Command, Flags};

use crate::studio::{Dialog, SueWriter};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "help", run)
}

pub(super) const COMMANDS: &[Command<SueWriter>] =
    &[c("help.shortcuts", "keyboard-shortcuts").with(Flags::PALETTE)];

/// Does what a Help menu command does.
fn run(writer: &mut SueWriter, _: &mut AppletContext<'_>, call: Call<'_>) {
    if call.name == "help.shortcuts" {
        writer.dialog = Some(Dialog::Shortcuts);
    }
}
