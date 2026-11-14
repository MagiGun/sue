//! Edit, Rotate/Flip Canvas: the whole canvas turned or mirrored.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "canvas", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("canvas.rotate_left", "rotate-90-degrees-left"),
    c("canvas.rotate_right", "rotate-90-degrees-right"),
    c("canvas.rotate180", "rotate-180-degrees"),
    c("canvas.flip_h", "flip-horizontal"),
    c("canvas.flip_v", "flip-vertical"),
];
