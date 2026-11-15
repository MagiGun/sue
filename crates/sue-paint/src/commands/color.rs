//! The color commands that are no tool key: Color Settings on the drawing
//! color. A double click on a color opens the same dialog; this entry is
//! there so that the command palette offers it and a key can be given to
//! it in Settings (it has none by default, as in Clip Studio Paint).
//!
//! It opens a dialog and changes no document, so it has no undo pair, and
//! Auto Actions do not record it.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

pub(super) const COMMANDS: &[Command<SuePaint>] = &[Command::new(
    "color.settings",
    "color-settings",
    "color",
    |paint: &mut SuePaint, _, _| paint.open_drawing_color_settings(),
)
.with(Flags::PALETTE)
.icon("palette-color-wheel")];

#[cfg(test)]
mod tests {
    use super::COMMANDS;

    #[test]
    fn color_settings_is_offered_with_its_label_and_icon() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        for command in COMMANDS {
            assert!(sue_lang::has("paint", command.label), "{}", command.name);
            assert!(command.flags.has(sue_wm::Flags::PALETTE));
            assert!(command.chords.is_empty(), "no key of its own");
            assert!(sue_wm::icons::Icon::from_name(command.icon).is_some());
        }
    }
}
