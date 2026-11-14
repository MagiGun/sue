//! The Help menu's commands.

use sue_core::{Arena, UndoStack};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState, World};

use super::{Basis, when};
use crate::dialogs::Dialog;
use crate::studio::SuePaint;

/// Does what a Help command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_help(call.name, cx);
}

/// How an item for a Help command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_help(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Help command looks right now.
    fn state_help(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "help.shortcuts" | "help.about" => ItemState::on(),
            _ => when(has_doc),
        }
    }

    /// Does what a Help command does.
    fn run_help(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "help.guide" => {
                self.dialog = Some(Dialog::Message(
                    tr!("sue-paint-guide"),
                    crate::extras::guide(),
                ))
            }
            "help.updates" => {
                self.dialog = Some(Dialog::Message(
                    tr!("check-for-updates"),
                    crate::extras::updates(),
                ))
            }
            "help.shortcuts" => self.dialog = Some(Dialog::Shortcuts),
            "help.about" => cx.notify(tr!("sue-paint-layered-painting-inside-sue")),
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "help", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("help.guide", "sue-paint-guide").with(Flags::LEAVES_CANVAS),
    c("help.shortcuts", "keyboard-shortcuts")
        .keys("Ctrl+Alt+Shift+K or F1 or Alt+I")
        .with(Flags::PALETTE),
    c("help.updates", "check-for-updates").with(Flags::LEAVES_CANVAS),
    c("help.about", "about-sue-paint"),
];
