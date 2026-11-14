//! The Help menu's commands: the guide, the keys, updates, About, and
//! the pages on the web that the start page's Help popup linked to.
//!
//! None of them changes the picture, so none is `RECORDABLE`: an Auto
//! Action never keeps one. The links go outside the canvas into the
//! system's browser, so they are `LEAVES_CANVAS`; the rest open a
//! window or a notice over the canvas and stay.

use sue_wm::{AppletContext, Call, Command, Flags, ShellRequest};

use crate::dialogs::Dialog;
use crate::studio::SuePaint;

/// Basic tips: Sue's home page.
pub(crate) const TIPS: &str = "https://magigun.github.io/sue/";
/// Sue Paint's part of the guide for people who use Sue, on the web.
pub(crate) const ONLINE_GUIDE: &str =
    "https://github.com/MagiGun/sue/blob/main/HUMANS.md#sue-paint-painting-and-illustration";
/// Questions people asked, with their answers.
pub(crate) const FAQ: &str = "https://github.com/MagiGun/sue/issues?q=label%3Aquestion";
/// Where to write to the people who make Sue.
pub(crate) const CONTACT: &str = "https://github.com/MagiGun/sue/issues/new/choose";
/// Where to ask for a feature.
pub(crate) const FEEDBACK: &str = "https://github.com/MagiGun/sue/issues/new?template=feature.yml";

/// Help, Sue Paint Guide: the guide in a window over the canvas.
fn guide(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Message(
        tr!("sue-paint-guide"),
        crate::extras::guide(),
    ));
}

/// Help, Keyboard Shortcuts: the list of keys and gestures.
fn shortcuts(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Shortcuts);
}

/// Help, Check for Updates: the version and where new ones are found.
fn updates(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Message(
        tr!("check-for-updates"),
        crate::extras::updates(),
    ));
}

/// Help, About Sue Paint: a notice of what the studio is.
fn about(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.notify(tr!("sue-paint-layered-painting-inside-sue"));
}

/// Asks the shell to open `link` in the system's browser.
fn open(cx: &mut AppletContext<'_>, link: &str) {
    cx.request(ShellRequest::Command(format!("open-url:{link}")));
}

/// Help, Basic Tips.
fn tips(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    open(cx, TIPS);
}

/// Help, Using Sue Paint (Online).
fn online_guide(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    open(cx, ONLINE_GUIDE);
}

/// Help, Frequently Asked Questions.
fn faq(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    open(cx, FAQ);
}

/// Help, Contact.
fn contact(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    open(cx, CONTACT);
}

/// Help, Feedback and Requests.
fn feedback(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    open(cx, FEEDBACK);
}

/// A command of this group. None reads the canvas, so each works with
/// or without one (the guide and the update notes too).
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "help", run).icon(icon)
}

/// A link of the start page's Help popup: offered in the command
/// palette, and it goes outside the canvas into the browser.
const fn link(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    c(name, label, icon, run).with(Flags::PALETTE.with(Flags::LEAVES_CANVAS))
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("help.guide", "sue-paint-guide", "book", guide),
    c(
        "help.shortcuts",
        "keyboard-shortcuts",
        "keyboard",
        shortcuts,
    )
    .keys("Ctrl+Alt+Shift+K or F1 or Alt+I")
    .with(Flags::PALETTE),
    c("help.updates", "check-for-updates", "update", updates),
    c("help.about", "about-sue-paint", "info", about),
    link("help.tips", "help-basic-tips", "help", tips),
    link(
        "help.online_guide",
        "help-using-sue-paint-online",
        "external",
        online_guide,
    ),
    link("help.faq", "help-faq", "question", faq),
    link("help.contact", "help-contact", "mail", contact),
    link(
        "help.feedback",
        "help-feedback-and-requests",
        "comment",
        feedback,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of this group's commands that carry `flag`.
    fn flagged(flag: Flags) -> Vec<&'static str> {
        COMMANDS
            .iter()
            .filter(|command| command.flags.has(flag))
            .map(|command| command.name)
            .collect()
    }

    #[test]
    fn no_help_command_is_kept_by_an_auto_action() {
        // What an Auto Action records changes the picture or the view;
        // Help does neither.
        assert!(flagged(Flags::RECORDABLE).is_empty());
    }

    #[test]
    fn only_the_links_leave_the_canvas() {
        assert_eq!(
            flagged(Flags::LEAVES_CANVAS),
            [
                "help.tips",
                "help.online_guide",
                "help.faq",
                "help.contact",
                "help.feedback",
            ]
        );
    }

    #[test]
    fn every_help_command_has_an_icon_of_the_set_and_a_label() {
        for command in COMMANDS {
            assert_eq!(command.group, "help", "{}", command.name);
            assert!(command.name.starts_with("help."), "{}", command.name);
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{}: no icon called {:?}",
                command.name,
                command.icon
            );
            assert!(
                sue_lang::has("paint", command.label),
                "{}: {}",
                command.name,
                command.label
            );
        }
    }

    #[test]
    fn the_links_go_to_sues_own_pages() {
        // Sue's own site and repository only: never another program's.
        for link in [TIPS, ONLINE_GUIDE, FAQ, CONTACT, FEEDBACK] {
            assert!(
                link.starts_with("https://magigun.github.io/sue/")
                    || link.starts_with("https://github.com/MagiGun/sue/"),
                "{link}"
            );
        }
    }

    #[test]
    fn every_help_command_works_without_a_canvas() {
        // The guide, the update notes and the rest read nothing of a
        // picture, so Help stays usable before one is open.
        let paint = SuePaint::default();
        assert!(paint.doc().is_none());
        let rice = sue_rice::RiceSchema::new();
        let world = sue_wm::World {
            arena: sue_core::Arena::new(),
            rice,
        };
        let history = sue_core::UndoStack::new();
        let facts = sue_wm::Facts {
            arena: &world.arena,
            history: &history,
            rice: &world.rice,
        };
        let registry = crate::registry();
        for command in COMMANDS {
            assert_eq!(
                registry.state(&paint, &facts, command.name),
                sue_wm::ItemState::on(),
                "{}",
                command.name
            );
        }
    }
}
