//! The Help menu's commands: the guide, the keys, updates and About.
//!
//! None of them changes the picture, so none is `RECORDABLE`: an Auto
//! Action never keeps one.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::when;
use crate::dialogs::Dialog;
use crate::studio::SuePaint;

/// The guide and the update notes read about a canvas: grayed without
/// one, as they were in the old match.
fn with_canvas(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some())
}

/// The key list and About work with or without a canvas.
fn always(_: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::on()
}

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

/// A command of this group.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
    state: fn(&SuePaint, &Facts<'_>, Call<'_>) -> ItemState,
) -> Command<SuePaint> {
    Command::new(name, label, "help", run)
        .icon(icon)
        .state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("help.guide", "sue-paint-guide", "book", guide, with_canvas).with(Flags::LEAVES_CANVAS),
    c(
        "help.shortcuts",
        "keyboard-shortcuts",
        "keyboard",
        shortcuts,
        always,
    )
    .keys("Ctrl+Alt+Shift+K or F1 or Alt+I")
    .with(Flags::PALETTE),
    c(
        "help.updates",
        "check-for-updates",
        "update",
        updates,
        with_canvas,
    )
    .with(Flags::LEAVES_CANVAS),
    c("help.about", "about-sue-paint", "info", about, always),
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
}
