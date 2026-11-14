//! The Help menu.

use sue_wm::{AppletContext, Call, Command, Flags, ShellRequest};

use crate::studio::{Dialog, SueWriter};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "help", run)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("help.shortcuts", "keyboard-shortcuts").with(Flags::PALETTE),
    c("help.updates", "check-for-updates").with(Flags::LEAVES_CANVAS),
    c("help.about", "about-sue-writer"),
];

/// Does what a Help menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    match call.name {
        "help.shortcuts" => writer.dialog = Some(Dialog::Shortcuts),
        // Sue's own check, the same from every studio.
        "help.updates" => cx.request(ShellRequest::Command("app.check_updates".to_owned())),
        "help.about" => writer.dialog = Some(Dialog::About),
        _ => {}
    }
}
