//! The 3D drawing figure's commands, run from its tool property: a
//! preset pose, Flip pose, Reset pose, Register pose as material and the
//! two scanners that pose a figure or a hand from a photo; the 3D
//! object's Put on the ground, its resets and preset angles; and the
//! body shape's Initialize and Register body shape as material.
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

/// The group's commands that Auto Actions may record: all of them. A pose, Flip and Reset pose the
/// figure the same way every time they are replayed, Register pose keeps
/// the pose again under the next free name, and a scanner asks again for
/// its photo. A test holds it to the flags of the table, which Auto
/// Actions read.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const RECORDABLE: [&str; 14] = [
    "figure.register_pose",
    "figure.pose_scanner",
    "figure.hand_scanner",
    "figure.hand_scanner_camera",
    "figure.flip",
    "figure.reset",
    "figure.preset.",
    "figure.object.ground",
    "figure.object.reset_position",
    "figure.object.reset_angle",
    "figure.object.reset_scale",
    "figure.object.angle.",
    "figure.body.initialize",
    "figure.body.register",
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

/// Hand Scanner (camera): a hand held up to the camera poses the current
/// 3D hand, or a hand of the current drawing figure, live.
fn hand_scanner_camera(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.open_hand_scanner(cx);
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

/// Put on the ground: the current 3D layer's object lowered or lifted so
/// it stands on the ground.
fn ground(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.ground_object(cx);
}

/// Reset position: the current 3D layer's object back where it was made.
fn reset_position(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.place_object(cx, &tr!("reset-position"), |now, _| {
        Some(crate::three::Placement {
            position: [0.0; 3],
            ..now
        })
    });
}

/// Reset angle: the current 3D layer's object turned back as made.
fn reset_angle(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.place_object(cx, &tr!("reset-angle"), |now, _| {
        Some(crate::three::Placement {
            turn: [0.0; 3],
            ..now
        })
    });
}

/// Reset scale: the current 3D layer's object back to its size as made.
fn reset_scale(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.place_object(cx, &tr!("reset-scale"), |now, _| {
        Some(crate::three::Placement { scale: 1.0, ..now })
    });
}

/// `figure.object.angle.<n>`: the current 3D layer's object turned to
/// angle `n` of [`crate::three::ANGLES`].
fn angle(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some((_, turn)) = call
        .arg
        .parse::<usize>()
        .ok()
        .and_then(|index| crate::three::ANGLES.get(index))
    else {
        return;
    };
    paint.place_object(cx, &tr!("object-angle"), |now, _| {
        Some(crate::three::Placement { turn: *turn, ..now })
    });
}

/// Initialize: the figure's body shape back as Sue Paint makes it.
fn initialize_body(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.shape_figure(cx, &tr!("initialize-body-shape"), |now| {
        Some(now.initialized())
    });
}

/// Register body shape as material: the figure's body shape kept in the
/// materials folder.
fn register_body(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.register_body_shape(cx);
}

/// Works while the current layer is a 3D layer with an object.
fn on_object(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.object_3d(facts.arena).is_some())
}

/// `figure.object.angle.<n>` works while the current layer is a 3D layer
/// and there is an angle `n`.
fn on_angle(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    super::when(
        paint.object_3d(facts.arena).is_some()
            && call
                .arg
                .parse::<usize>()
                .is_ok_and(|index| index < crate::three::ANGLES.len()),
    )
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

/// Works while the current layer holds something with hands: a 3D hand
/// or a whole drawing figure.
fn on_hands(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(
        current_body(paint, facts.arena).is_some_and(|body| body.kind != crate::figure::Kind::Head),
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
    // Recorded as the photo scanners are: an Auto Action opens the
    // dialog, and what the camera sees then poses the hand.
    c(
        "figure.hand_scanner_camera",
        "hand-scanner-camera",
        Icon::ToolHand,
        hand_scanner_camera,
        on_hands,
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
    c(
        "figure.object.ground",
        "put-on-ground",
        Icon::ArrowDown,
        ground,
        on_object,
    ),
    c(
        "figure.object.reset_position",
        "reset-position",
        Icon::Reset,
        reset_position,
        on_object,
    ),
    c(
        "figure.object.reset_angle",
        "reset-angle",
        Icon::Reset,
        reset_angle,
        on_object,
    ),
    c(
        "figure.object.reset_scale",
        "reset-scale",
        Icon::Reset,
        reset_scale,
        on_object,
    ),
    c(
        "figure.object.angle.",
        "object-angles",
        Icon::RotateRight,
        angle,
        on_angle,
    )
    .with(Flags::FAMILY.with(Flags::HIDDEN)),
    c(
        "figure.body.initialize",
        "initialize-body-shape",
        Icon::Reset,
        initialize_body,
        on_body,
    ),
    c(
        "figure.body.register",
        "register-body-shape-as-material",
        Icon::MaterialPose,
        register_body,
        on_body,
    ),
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
        assert_eq!(family, ["figure.preset.", "figure.object.angle."]);
    }
}
