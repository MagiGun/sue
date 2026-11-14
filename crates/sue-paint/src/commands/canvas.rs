//! Edit, Rotate/Flip Canvas: the whole canvas turned or mirrored.
//!
//! Each command is an entry of its own: its label, its icon from the
//! shared set, its flags and the function that does it. Clip Studio Paint
//! gives none of them a key by default, so neither does Sue; any of them
//! can be given one in Settings.
//!
//! Flags: every command of the group changes the picture, so Auto
//! Actions may record each one (`RECORDABLE`), and none of them leaves
//! the canvas (`LEAVES_CANVAS`: none).

use sue_wm::{AppletContext, Call, Command, Flags};

use crate::doc;
use crate::studio::SuePaint;

/// Rotate 90 Degrees Left: the canvas a quarter turn counterclockwise.
fn rotate_left(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.turn_canvas(cx, false);
}

/// Rotate 90 Degrees Right: the canvas a quarter turn clockwise.
fn rotate_right(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.turn_canvas(cx, true);
}

/// Rotate 180 Degrees: the canvas a half turn.
fn rotate180(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.transform_canvas(cx, true, true, &tr!("rotate-canvas"));
}

/// Flip Horizontal: the canvas mirrored left to right.
fn flip_h(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.transform_canvas(cx, true, false, &tr!("flip-canvas"));
}

/// Flip Vertical: the canvas mirrored top to bottom.
fn flip_v(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.transform_canvas(cx, false, true, &tr!("flip-canvas"));
}

impl SuePaint {
    /// Turns the canvas a quarter turn, clockwise or not: its sides
    /// swap, so the selection goes and the view is fitted again.
    fn turn_canvas(&mut self, cx: &mut AppletContext<'_>, clockwise: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        match doc::rotate_canvas(&cx.world.arena, picture, clockwise) {
            Ok(edit) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.set_selection(None);
                    doc.view.fitted = false;
                }
            }
            Err(error) => cx.fail(error),
        }
    }
}

/// A command of this group: it works while a canvas is open, and Auto
/// Actions may record it.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "canvas", run)
        .icon(icon)
        .with(Flags::RECORDABLE)
        .state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "canvas.rotate_left",
        "rotate-90-degrees-left",
        "rotate-left",
        rotate_left,
    ),
    c(
        "canvas.rotate_right",
        "rotate-90-degrees-right",
        "rotate-right",
        rotate_right,
    ),
    // The set has no half turn of its own; a half turn goes the way of
    // the clockwise quarter turn.
    c(
        "canvas.rotate180",
        "rotate-180-degrees",
        "rotate-right",
        rotate180,
    ),
    c(
        "canvas.flip_h",
        "flip-horizontal",
        "flip-horizontal",
        flip_h,
    ),
    c("canvas.flip_v", "flip-vertical", "flip-vertical", flip_v),
];

#[cfg(test)]
mod tests {
    use sue_wm::Flags;
    use sue_wm::icons::Icon;

    use super::COMMANDS;
    use crate::commands::registry;

    /// The group as Edit, Rotate/Flip Canvas lists it, in its order.
    const NAMES: [&str; 5] = [
        "canvas.rotate_left",
        "canvas.rotate_right",
        "canvas.rotate180",
        "canvas.flip_h",
        "canvas.flip_v",
    ];

    #[test]
    fn each_canvas_command_is_an_entry_of_its_own() {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        assert_eq!(names, NAMES);
        for command in COMMANDS {
            let name = command.name;
            assert!(registry().has(name), "{name} is registered");
            assert_eq!(command.group, "canvas", "{name}");
            assert!(!command.is_family(), "{name}");
            assert!(
                sue_lang::has("paint", command.label),
                "{name}: {} has English",
                command.label
            );
            assert!(
                Icon::from_name(command.icon).is_some(),
                "{name}: {:?} is an icon of the set",
                command.icon
            );
            // Clip Studio Paint gives these no key by default.
            assert_eq!(command.chords, "", "{name}");
        }
    }

    #[test]
    fn every_canvas_command_is_recordable_and_none_leaves_the_canvas() {
        let recordable: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.flags.has(Flags::RECORDABLE))
            .map(|command| command.name)
            .collect();
        assert_eq!(recordable, NAMES);
        let leaving: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.flags.has(Flags::LEAVES_CANVAS))
            .map(|command| command.name)
            .collect();
        assert_eq!(leaving, Vec::<&str>::new());
        // In the menu, not deliberately hidden, not in the palette.
        for command in COMMANDS {
            assert!(!command.flags.has(Flags::HIDDEN), "{}", command.name);
            assert!(!command.flags.has(Flags::PALETTE), "{}", command.name);
        }
    }
}
