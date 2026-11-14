//! The 3D drawing figure's commands, run from its tool property.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "figure", super::run).state(super::state)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "figure", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("figure.register_pose", "register-pose-as-material"),
    c("figure.pose_scanner", "pose-scanner"),
    c("figure.hand_scanner", "hand-scanner"),
    c("figure.flip", "flip-pose").with(Flags::HIDDEN),
    c("figure.reset", "reset-pose").with(Flags::HIDDEN),
    family("figure.preset.", "poses").with(Flags::HIDDEN),
];
