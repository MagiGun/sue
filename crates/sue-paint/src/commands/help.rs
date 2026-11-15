//! The Help menu's commands.

use sue_core::{Arena, UndoStack};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState, ShellRequest, World};

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
            link if link_of(link).is_some() => ItemState::on(),
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
            link => {
                if let Some(page) = link_of(link) {
                    cx.request(ShellRequest::Command(format!("open-url:{page}")));
                }
            }
        }
    }
}

/// The web pages the Help menu opens in the system's browser, by
/// command: what the help popup of Sue Paint's removed launcher held.
const LINKS: &[(&str, &str)] = &[
    ("help.tips", "https://magigun.github.io/sue/"),
    (
        "help.online_guide",
        "https://github.com/MagiGun/sue/blob/main/HUMANS.md#sue-paint-painting-and-illustration",
    ),
    (
        "help.faq",
        "https://github.com/MagiGun/sue/issues?q=label%3Aquestion",
    ),
    (
        "help.contact",
        "https://github.com/MagiGun/sue/issues/new/choose",
    ),
    (
        "help.feedback",
        "https://github.com/MagiGun/sue/issues/new?template=feature.yml",
    ),
];

/// The web page the Help command `command` opens, if it opens one.
fn link_of(command: &str) -> Option<&'static str> {
    LINKS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, page)| *page)
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
    c("help.tips", "help-basic-tips").with(Flags::PALETTE.with(Flags::LEAVES_CANVAS)),
    c("help.online_guide", "help-using-sue-paint-online")
        .with(Flags::PALETTE.with(Flags::LEAVES_CANVAS)),
    c("help.faq", "help-faq").with(Flags::PALETTE.with(Flags::LEAVES_CANVAS)),
    c("help.contact", "help-contact").with(Flags::PALETTE.with(Flags::LEAVES_CANVAS)),
    c("help.feedback", "help-feedback-and-requests")
        .with(Flags::PALETTE.with(Flags::LEAVES_CANVAS)),
];
