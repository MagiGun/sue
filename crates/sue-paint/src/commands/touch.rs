//! The touch commands: what fingers do on the canvas, and the edge key
//! bar of Studio Mode.
//!
//! Each changes a setting, not the picture, so none is `RECORDABLE` and
//! none has an undo pair of its own: the setting goes back as any
//! setting does.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use crate::studio::SuePaint;
use crate::studio_mode::{self, KeyBar};

/// Draw with Fingers: switches between fingers that draw with the tool
/// and fingers that only move, zoom and turn the view (the pen draws).
fn finger_draws(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let now = studio_mode::Gestures::of(cx.rice()).finger_draws;
    studio_mode::set_bool(cx, studio_mode::FINGER_KEY, !now);
}

fn finger_draws_on(_: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::check(studio_mode::Gestures::of(facts.rice).finger_draws)
}

/// The edge key bar: pinned beside the canvas, or put away.
fn key_bar(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.touch.bar = if paint.touch.bar == KeyBar::Hidden {
        KeyBar::Pinned
    } else {
        KeyBar::Hidden
    };
}

fn key_bar_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::check(paint.touch.bar != KeyBar::Hidden)
}

/// A command of this group, offered in the command palette.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "touch", run)
        .icon(icon)
        .with(Flags::PALETTE)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "touch.finger_draws",
        "draw-with-fingers",
        "finger",
        finger_draws,
    )
    .state(finger_draws_on),
    c("touch.key_bar", "edge-key-bar", "keyboard", key_bar).state(key_bar_on),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_touch_command_has_an_icon_of_the_set_and_a_label() {
        for command in COMMANDS {
            assert_eq!(command.group, "touch", "{}", command.name);
            assert!(command.name.starts_with("touch."), "{}", command.name);
            assert!(!command.flags.has(Flags::RECORDABLE), "{}", command.name);
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
