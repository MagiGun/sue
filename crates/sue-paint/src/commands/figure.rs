//! The 3D drawing figure's commands, run from its tool property: a
//! preset pose, Flip pose, Reset pose, Register pose as material and the
//! two scanners that pose a figure or a hand from a photo.
//!
//! Every entry has its own `run` and `state`, its label key in
//! `paint.ftl`, its icon from `sue_wm::icons` and its flags. None ships
//! with a key (Clip Studio Paint gives these none either); each can be
//! given one in the shortcut settings, the hidden pose commands too
//! through Auto Actions and scripts.

use sue_core::Arena;
use sue_wm::icons::Icon;
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use crate::studio::SuePaint;

/// The group's commands that Auto Actions may record: all of them, as
/// `extras::recordable` has it today. A pose, Flip and Reset pose the
/// figure the same way every time they are replayed, Register pose keeps
/// the pose again under the next free name, and a scanner asks again for
/// its photo. Auto Actions
/// read it once `extras::recordable` goes (P3-14); a test holds it to the
/// flags of the table.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const RECORDABLE: [&str; 6] = [
    "figure.register_pose",
    "figure.pose_scanner",
    "figure.hand_scanner",
    "figure.flip",
    "figure.reset",
    "figure.preset.",
];

/// The group's commands that leave the canvas (quit, print or open a
/// system dialog): none. The scanners ask for a photo through Sue's own
/// file picker, as Open does.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const LEAVES_CANVAS: [&str; 0] = [];

/// Register pose as material: the current figure's pose kept in the
/// materials folder.
fn register_pose(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.register_pose(cx);
}

/// Pose Scanner: a photo poses the current figure.
fn pose_scanner(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pick_scan(cx, false);
}

/// Hand Scanner: a photo poses the current hand.
fn hand_scanner(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pick_scan(cx, true);
}

/// Flip pose: the figure mirrored left to right.
fn flip(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pose_figure(cx, |now, _| Some(now.flipped()));
}

/// Reset pose: the figure back to its kind's first preset.
fn reset(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pose_figure(cx, |_, body| Some(body.standing()));
}

/// `figure.preset.<n>`: the figure in its kind's preset pose `n`.
fn preset(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let index = call.arg.parse::<usize>().ok();
    paint.pose_figure(cx, |_, body| {
        index
            .filter(|index| *index < body.presets().len())
            .map(|index| body.preset(index))
    });
}

/// The body of the drawing figure, hand or head on the current layer.
fn current_body(paint: &SuePaint, arena: &Arena) -> Option<crate::figure::Body> {
    let model = crate::figure::of_layer(arena, paint.layer()?)?;
    arena.get_by_id(model).ok().map(crate::figure::Body::of)
}

/// Works while the current layer holds a drawing figure, hand or head.
fn on_figure(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(current_body(paint, facts.arena).is_some())
}

/// Works while the current layer holds a whole drawing figure.
fn on_body(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(
        current_body(paint, facts.arena).is_some_and(|body| body.kind == crate::figure::Kind::Body),
    )
}

/// Works while the current layer holds a 3D hand.
fn on_hand(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(
        current_body(paint, facts.arena).is_some_and(|body| body.kind == crate::figure::Kind::Hand),
    )
}

/// `figure.preset.<n>` works while the current layer's figure, hand or
/// head has a preset `n`.
fn on_preset(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let index = call.arg.parse::<usize>().ok();
    super::when(
        current_body(paint, facts.arena)
            .zip(index)
            .is_some_and(|(body, index)| index < body.presets().len()),
    )
}

impl SuePaint {
    /// Poses the drawing figure of the current layer as `next` says,
    /// given its pose and body now, as one undo step. `None` leaves it.
    fn pose_figure(
        &mut self,
        cx: &mut AppletContext<'_>,
        next: impl FnOnce(&crate::figure::Pose, &crate::figure::Body) -> Option<crate::figure::Pose>,
    ) {
        let Some(layer) = self.layer() else {
            return;
        };
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer) else {
            cx.notify(tr!("pick-a-drawing-figure-layer"));
            return;
        };
        let Some(entity) = cx.world.arena.get_by_id(model).ok() else {
            return;
        };
        let (now, body) = (
            crate::figure::Pose::of(entity),
            crate::figure::Body::of(entity),
        );
        let Some(next) = next(&now, &body) else {
            return;
        };
        if let Some(edit) = crate::figure::set_pose(&cx.world.arena, layer, &next, &tr!("pose")) {
            Self::apply(cx, edit);
        }
    }
}

/// A command of this group, recordable, that works as `state` says.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: Icon,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
    state: fn(&SuePaint, &Facts<'_>, Call<'_>) -> ItemState,
) -> Command<SuePaint> {
    Command::new(name, label, "figure", run)
        .icon(icon.name())
        .with(Flags::RECORDABLE)
        .state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "figure.register_pose",
        "register-pose-as-material",
        Icon::MaterialPose,
        register_pose,
        on_figure,
    ),
    c(
        "figure.pose_scanner",
        "pose-scanner",
        Icon::Picture,
        pose_scanner,
        on_body,
    ),
    c(
        "figure.hand_scanner",
        "hand-scanner",
        Icon::Picture,
        hand_scanner,
        on_hand,
    ),
    c(
        "figure.flip",
        "flip-pose",
        Icon::FlipHorizontal,
        flip,
        on_figure,
    )
    .with(Flags::HIDDEN),
    c("figure.reset", "reset-pose", Icon::Reset, reset, on_figure).with(Flags::HIDDEN),
    c(
        "figure.preset.",
        "poses",
        Icon::MaterialPose,
        preset,
        on_preset,
    )
    .with(Flags::FAMILY.with(Flags::HIDDEN)),
];

#[cfg(test)]
mod tests {
    use sue_wm::Flags;
    use sue_wm::icons::Icon;

    use super::{COMMANDS, LEAVES_CANVAS, RECORDABLE};

    #[test]
    fn the_lists_say_what_the_flags_say() {
        for command in COMMANDS {
            let name = command.name;
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                RECORDABLE.contains(&name),
                "{name}"
            );
            assert_eq!(
                command.flags.has(Flags::LEAVES_CANVAS),
                LEAVES_CANVAS.contains(&name),
                "{name}"
            );
            // Auto Actions record what they recorded before the flag
            // (P3-14 drops this line with `recordable`).
            assert_eq!(
                crate::extras::recordable(name),
                RECORDABLE.contains(&name),
                "{name}"
            );
        }
        for name in RECORDABLE.iter().chain(&LEAVES_CANVAS) {
            assert!(
                COMMANDS.iter().any(|command| command.name == *name),
                "{name}"
            );
        }
    }

    #[test]
    fn every_command_has_an_icon_of_the_set() {
        for command in COMMANDS {
            assert!(
                Icon::from_name(command.icon).is_some(),
                "{}: {:?}",
                command.name,
                command.icon
            );
            assert_eq!(command.group, "figure", "{}", command.name);
        }
    }

    #[test]
    fn the_names_are_the_ones_saved_before() {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        assert_eq!(names, RECORDABLE);
        let family: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.is_family())
            .map(|command| command.name)
            .collect();
        assert_eq!(family, ["figure.preset."]);
    }
}
