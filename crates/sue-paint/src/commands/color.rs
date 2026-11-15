//! The color commands that are no tool key: Color Settings on the drawing
//! color (a double click on a color opens the same dialog), New Color Set
//! and Edit Color Sets (the Color Set palette's wrench has them). They are
//! here so that the command palette offers them and a key can be given to
//! them in Settings (none has one by default, as in Clip Studio Paint).
//!
//! Each opens a dialog and changes no document, so it has no undo pair, and
//! Auto Actions do not record it.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    Command::new(
        "color.settings",
        "color-settings",
        "color",
        |paint: &mut SuePaint, _, _| paint.open_drawing_color_settings(),
    )
    .with(Flags::PALETTE)
    .icon("palette-color-wheel"),
    // The Color Set palette's wrench has these too.
    Command::new(
        "color.new_set",
        "new-color-set",
        "color",
        |paint: &mut SuePaint, _, _| paint.open_set_name(None),
    )
    .with(Flags::PALETTE)
    .icon("plus"),
    Command::new(
        "color.edit_sets",
        "edit-color-sets",
        "color",
        |paint: &mut SuePaint, _, _| paint.open_edit_sets(),
    )
    .with(Flags::PALETTE)
    .icon("palette-color-set"),
];

#[cfg(test)]
mod tests {
    use super::COMMANDS;

    #[test]
    fn the_color_commands_are_offered_with_their_labels_and_icons() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        for command in COMMANDS {
            assert!(sue_lang::has("paint", command.label), "{}", command.name);
            assert!(command.flags.has(sue_wm::Flags::PALETTE));
            assert!(command.chords.is_empty(), "no key of its own");
            assert!(sue_wm::icons::Icon::from_name(command.icon).is_some());
        }
    }
}
