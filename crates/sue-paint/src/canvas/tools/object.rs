//! The Operation tool on the canvas: Select layer by a click, and Object,
//! which picks vector lines (Shift adds or takes away) and drags what is
//! picked, and hands frame border and effect line layers, 3D layers and
//! the camera frame to their own handles.

use egui::{Pos2, Rect};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Operation tool pressed.
    pub(super) fn operation_press(
        &mut self,
        press: &Press,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, modifiers, .. } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if sub.mode == 1 {
            self.pick_layer_at(&cx.world.arena, p);
        } else if self.on_comic_layer(&cx.world.arena) {
            // Frame borders and effect lines: their panels, corners and
            // ends move.
            let reach = 10.0 / doc.view.zoom.max(0.01);
            self.gesture = self.comic_object_at(p, reach, &cx.world.arena);
        } else if self
            .layer()
            .is_some_and(|layer| crate::three::is_3d(&cx.world.arena, layer))
        {
            self.three_press(press, cx);
        } else if self.camera_folder(&cx.world.arena).is_some() {
            self.camera_press(press, cx);
        } else if self.keyed_box(&cx.world.arena).is_some() {
            // A layer whose keyframes are on: its box's handles key it.
            self.keyed_press(press, cx);
        } else {
            // Object: click a vector line to pick it (Shift adds or
            // takes away), drag to move what is picked.
            let Some(info) = self.layer_info(&cx.world.arena) else {
                return;
            };
            if !crate::vector::is_vector(&cx.world.arena, info.id) {
                self.picked_lines = None;
                cx.notify(tr!("object-picks-vector-lines-pick-vector"));
                return;
            }
            let offset = info.style.offset;
            let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
            let geometry = crate::vector::geometry(&cx.world.arena, info.id);
            let lines = crate::vector::lines(&geometry);
            let reach = 8.0 / doc.view.zoom.max(0.01);
            let hit = crate::correct::line_at(&lines, local, reach).map(|(line, _)| line);
            let mut picked = match self.picked_lines.take() {
                Some((layer, picked)) if layer == info.id => picked,
                _ => Vec::new(),
            };
            match hit {
                None => {
                    if !modifiers.shift {
                        picked.clear();
                    }
                }
                Some(line) if modifiers.shift => {
                    if let Some(at) = picked.iter().position(|index| *index == line) {
                        picked.remove(at);
                    } else {
                        picked.push(line);
                    }
                }
                Some(line) => {
                    if !picked.contains(&line) {
                        picked = vec![line];
                    }
                }
            }
            let moving = hit.is_some() && !picked.is_empty();
            self.picked_lines = Some((info.id, picked));
            if moving
                && let Some(before) = doc::raster(&cx.world.arena, info.id)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
            {
                self.gesture = Some(Gesture::MoveLines {
                    layer: info.id,
                    before,
                    geometry,
                    base: lines,
                    start: local,
                    by: [0.0; 2],
                    offset,
                });
            }
        }
    }

    /// The vector lines picked, dragged to `pos`.
    pub(super) fn move_lines_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::MoveLines {
            layer,
            base,
            start,
            by,
            offset,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        *by = [
            p.x - offset[0] as f32 - start[0],
            p.y - offset[1] as f32 - start[1],
        ];
        let picked = self
            .picked_lines
            .as_ref()
            .map(|(_, picked)| picked.clone())
            .unwrap_or_default();
        let lines = crate::vector::moved(base, &picked, *by);
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(&lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The vector lines picked let go: the move as one undo step.
    pub(super) fn move_lines_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::MoveLines {
            layer,
            before,
            geometry,
            base,
            by,
            ..
        } = gesture
        else {
            return;
        };
        if by == [0.0; 2] {
            return;
        }
        let picked = self
            .picked_lines
            .as_ref()
            .map(|(_, picked)| picked.clone())
            .unwrap_or_default();
        let lines = crate::vector::moved(&base, &picked, by);
        let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let patch = doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                tr!("move-lines"),
                vec![patch],
            ))
        });
        let edit = crate::vector::keep(
            &mut cx.world.arena,
            layer,
            geometry,
            &lines,
            pixels,
            &tr!("move-lines"),
        );
        cx.record(ArenaEdit(edit));
    }
}
