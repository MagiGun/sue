//! The handles the canvas drags that are no tool's stroke: the layers of
//! the light table, the pins of the puppet warp, the camera frame and the
//! transform box.

use egui::{Modifiers, Pos2, Rect, pos2};
use sue_wm::AppletContext;

use super::Press;
use crate::canvas::input::angle;
use crate::studio::{Gesture, SuePaint};
use crate::view::View;

impl SuePaint {
    /// The Light table tool pressed at `p` (picture pixels): a drag moves
    /// the light table's layers (Shift turns them), the layers themselves
    /// as they are. `true` when there are layers to move.
    pub(super) fn light_table_press(
        &mut self,
        p: Pos2,
        modifiers: Modifiers,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let arena = &cx.world.arena;
        let mut layers = self
            .editing_cel(arena)
            .map(|cel| crate::anim::light_table(arena, cel))
            .unwrap_or_default();
        if self.light_general
            && let Some(picture) = self.picture()
        {
            layers.extend(crate::anim::general_light_table(arena, picture));
        }
        let placed: Vec<(sue_core::EntityId, [f32; 3])> = layers
            .into_iter()
            .filter_map(|layer| Some((layer, crate::anim::place_of(arena.get_by_id(layer).ok()?))))
            .collect();
        if !placed.is_empty() {
            self.gesture = Some(Gesture::LightTable {
                layers: placed,
                start: p,
                turn: modifiers.shift,
            });
            return true;
        }
        false
    }

    /// The light table's layers dragged to `pos`: shown as they move, one
    /// undo step when let go.
    pub(super) fn light_table_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let (
            Some(Gesture::LightTable {
                layers,
                start,
                turn,
            }),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        // Shown as it moves; one undo step when let go.
        let size = doc.compositor.display().1;
        let middle = pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
        let degrees = (angle(p - middle) - angle(*start - middle)).to_degrees();
        for (layer, [dx, dy, was]) in layers.iter() {
            let place = if *turn {
                [*dx, *dy, was + degrees]
            } else {
                [dx + p.x - start.x, dy + p.y - start.y, *was]
            };
            if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                entity
                    .properties_mut()
                    .set(crate::anim::PLACE, crate::anim::place_text(place));
            }
        }
        doc.compositor
            .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
    }

    /// The light table's layers let go.
    pub(super) fn light_table_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::LightTable { layers, .. } = gesture else {
            return;
        };
        // Back to where they began, then the move as one step.
        let mut edit = crate::doc::Edit::new(tr!("light-table-tool"));
        for (layer, was) in &layers {
            let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) else {
                continue;
            };
            let now = crate::anim::place_of(entity);
            entity
                .properties_mut()
                .set(crate::anim::PLACE, crate::anim::place_text(*was));
            if now != *was {
                edit = edit.then(sue_core::txn::SetProperty::new(
                    *layer,
                    crate::anim::PLACE,
                    crate::anim::place_text(now),
                ));
            }
        }
        if !edit.is_empty() {
            Self::apply(cx, edit);
        }
    }

    /// Puppet warp pressed at `pos` (on screen): a pin under the pointer
    /// is dragged (Alt takes it away), anywhere else a new pin goes in.
    pub(super) fn warp_press(
        &mut self,
        pos: Pos2,
        view: View,
        modifiers: Modifiers,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let p = view.to_picture(viewport, size, pos);
        let Some(warp) = self.warp.as_mut() else {
            return;
        };
        let local = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
        let offset = warp.offset;
        let hit = warp.pins.iter().position(|pin| {
            let at = pos2(pin.to[0] + offset[0] as f32, pin.to[1] + offset[1] as f32);
            view.to_screen(viewport, size, at).distance(pos) <= 9.0
        });
        match hit {
            Some(index) if modifiers.alt => {
                warp.pins.remove(index);
                self.warp_preview(cx);
            }
            Some(index) => self.gesture = Some(Gesture::Pin { index }),
            None => {
                warp.pins.push(crate::warp::Pin::at(local));
                self.gesture = Some(Gesture::Pin {
                    index: warp.pins.len() - 1,
                });
            }
        }
    }

    /// A puppet warp pin dragged to `pos`.
    pub(super) fn pin_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::Pin { index }) = self.gesture else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        if let Some(warp) = self.warp.as_mut()
            && let Some(pin) = warp.pins.get_mut(index)
        {
            pin.to = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
        }
        self.warp_preview(cx);
    }

    /// The Object tool pressed on a camera folder: inside the camera
    /// frame moves it, a corner zooms, outside turns it.
    pub(super) fn camera_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            viewport,
            size,
            ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some(folder) = self.camera_folder(&cx.world.arena) else {
            return;
        };
        let base = self.camera_now(&cx.world.arena, folder);
        let corners = base.frame(size);
        let near_corner = corners.iter().any(|corner| {
            doc.view
                .to_screen(viewport, size, pos2(corner[0], corner[1]))
                .distance(pos)
                <= 10.0
        });
        let inside = crate::camera::inside(&corners, [p.x, p.y]);
        let mode = if near_corner {
            1
        } else if inside {
            0
        } else {
            2
        };
        self.gesture = Some(Gesture::Camera {
            folder,
            start: [p.x, p.y],
            base,
            mode,
        });
    }

    /// The camera frame dragged to `pos`: pending until it is let go.
    pub(super) fn camera_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let Some(Gesture::Camera {
            folder,
            start,
            base,
            mode,
        }) = self.gesture
        else {
            return;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let frame = doc.frame;
        let mut camera = base;
        let center = base.center;
        match mode {
            0 => {
                camera.center = [center[0] + p.x - start[0], center[1] + p.y - start[1]];
            }
            1 => {
                // Corners further out see more: the camera zooms out.
                let from = (start[0] - center[0]).hypot(start[1] - center[1]).max(1.0);
                let to = (p.x - center[0]).hypot(p.y - center[1]).max(1.0);
                camera.scale = (base.scale * from / to).clamp(0.01, 20.0);
            }
            _ => {
                let from = (start[1] - center[1]).atan2(start[0] - center[0]);
                let to = (p.y - center[1]).atan2(p.x - center[0]);
                camera.angle = base.angle + (to - from);
            }
        }
        self.camera_pending = Some((folder, frame, camera));
    }

    /// The camera frame let go.
    pub(super) fn camera_release(&mut self, cx: &mut AppletContext<'_>) {
        // Keyed at this frame, as one step.
        if self.camera_pending.is_some() {
            self.run("anim.camera.key", cx);
        }
    }

    /// A handle of the transform box dragged to `pos`.
    pub(super) fn transform_box_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let offset = self
            .transform
            .as_ref()
            .map_or([0.0; 2], |session| session.offset.map(|value| value as f32));
        self.transform_drag([p.x - offset[0], p.y - offset[1]], self.modifiers);
        self.transform_preview(cx);
    }
}
