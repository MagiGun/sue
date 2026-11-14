//! The 3D drawing figure's commands, run from its tool property.

use sue_wm::{AppletContext, Call, Command, Flags};

use crate::studio::SuePaint;

/// Does what a Figure command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_figure(call.name, cx);
}

impl SuePaint {
    /// Does what a Figure command does.
    fn run_figure(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "figure.register_pose" => self.register_pose(cx),
            "figure.pose_scanner" => self.pick_scan(cx, false),
            "figure.hand_scanner" => self.pick_scan(cx, true),
            // A drawing figure's pose by name: a preset, flipped or reset.
            figure if figure.starts_with("figure.") => {
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
                let next = match &figure["figure.".len()..] {
                    "flip" => now.flipped(),
                    "reset" => body.standing(),
                    other => match other
                        .strip_prefix("preset.")
                        .and_then(|index| index.parse::<usize>().ok())
                    {
                        Some(index) => body.preset(index),
                        None => return,
                    },
                };
                if let Some(edit) =
                    crate::figure::set_pose(&cx.world.arena, layer, &next, &tr!("pose"))
                {
                    Self::apply(cx, edit);
                }
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "figure", run).state(super::doc_open)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "figure", run).state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("figure.register_pose", "register-pose-as-material"),
    c("figure.pose_scanner", "pose-scanner"),
    c("figure.hand_scanner", "hand-scanner"),
    c("figure.flip", "flip-pose").with(Flags::HIDDEN),
    c("figure.reset", "reset-pose").with(Flags::HIDDEN),
    family("figure.preset.", "poses").with(Flags::HIDDEN),
];
