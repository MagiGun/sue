//! The Operation tool on 3D layers: a drag turns the camera (Shift moves
//! it, Ctrl comes nearer or goes further), and on a drawing figure a body
//! part bends at its joint (Shift twists it).

use egui::Pos2;
use sue_wm::AppletContext;

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};

impl SuePaint {
    /// The Operation tool pressed on a 3D layer.
    pub(super) fn three_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            modifiers,
            size,
            ..
        } = *press;
        // A 3D layer: drag to turn the camera, Shift to move it,
        // Ctrl to come nearer or go further.
        let Some(layer) = self.layer() else {
            return;
        };
        // A drawing figure: a body part under the pointer bends at
        // its joint (Shift twists it).
        if let Some(joint) = self.figure_part_at(&cx.world.arena, layer, p, size) {
            let model = crate::figure::of_layer(&cx.world.arena, layer);
            let (Some(base), Some(before)) = (
                model
                    .and_then(|model| cx.world.arena.get_by_id(model).ok())
                    .map(crate::figure::Pose::of),
                doc::raster(&cx.world.arena, layer)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec),
            ) else {
                return;
            };
            self.figure_joint = Some(joint);
            self.gesture = Some(Gesture::Pose {
                layer,
                joint,
                start: pos,
                base,
                before,
                twist: modifiers.shift,
            });
            return;
        }
        let (Some(base), Some(before)) = (
            cx.world
                .arena
                .get_by_id(layer)
                .ok()
                .map(crate::three::View3D::of),
            doc::raster(&cx.world.arena, layer)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec),
        ) else {
            return;
        };
        let mode = if modifiers.shift {
            1
        } else if modifiers.command {
            2
        } else {
            0
        };
        self.gesture = Some(Gesture::Orbit {
            layer,
            start: pos,
            base,
            before,
            mode,
        });
    }

    /// A body part of a drawing figure bent (or twisted) by a drag to
    /// `pos`.
    pub(super) fn pose_drag(&mut self, pos: Pos2, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Pose {
            layer,
            joint,
            start,
            base,
            twist,
            ..
        }) = &self.gesture
        else {
            return;
        };
        let (layer, joint, twist) = (*layer, *joint, *twist);
        let (dx, dy) = (pos.x - start.x, pos.y - start.y);
        let mut pose = base.clone();
        // Seen from behind, sideways drags swing the other way.
        let behind = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
            .is_some_and(|view| view.yaw.to_radians().cos() < 0.0);
        let side = if behind { -1.0 } else { 1.0 };
        let turn = &mut pose.0[joint];
        if twist {
            turn[1] = base.0[joint][1] + dx * 0.6;
        } else {
            turn[0] = (base.0[joint][0] + dy * 0.5).clamp(-180.0, 180.0);
            turn[2] = (base.0[joint][2] + dx * 0.5 * side).clamp(-180.0, 180.0);
        }
        self.preview_pose(cx, layer, &pose);
    }

    /// The 3D camera turned, moved or brought nearer by a drag to `pos`.
    pub(super) fn orbit_drag(&mut self, pos: Pos2, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Orbit {
            layer,
            start,
            base,
            mode,
            ..
        }) = &self.gesture
        else {
            return;
        };
        let (layer, start, base, mode) = (*layer, *start, *base, *mode);
        let (dx, dy) = (pos.x - start.x, pos.y - start.y);
        let mut view = base;
        match mode {
            1 => {
                let scale = base.distance / 600.0;
                view.pan = [base.pan[0] - dx * scale, base.pan[1] + dy * scale];
            }
            2 => view.distance = (base.distance * (dy / 200.0).exp()).clamp(0.6, 100.0),
            _ => {
                view.yaw = base.yaw - dx * 0.4;
                view.pitch = (base.pitch + dy * 0.4).clamp(-89.0, 89.0);
            }
        }
        self.preview_view3d(cx, layer, &view);
    }
}
