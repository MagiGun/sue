//! Vector layers on the canvas: the eraser that takes vector lines away,
//! as its Vector setting says, and paints the layer again from the lines
//! left.

use egui::{Pos2, Rect};
use sue_core::EntityId;
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The eraser pressed on the vector layer `layer` (at `offset` in the
    /// picture) with a tip `tip_size` across.
    pub(super) fn vector_erase_press(
        &mut self,
        layer: EntityId,
        offset: [i64; 2],
        tip_size: f32,
        sub: &SubTool,
        press: &Press,
        cx: &mut AppletContext<'_>,
    ) {
        let Press {
            pos,
            p,
            viewport,
            size,
            ..
        } = *press;
        let Some(before) = doc::raster(&cx.world.arena, layer)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return;
        };
        let geometry = crate::vector::geometry(&cx.world.arena, layer);
        self.gesture = Some(Gesture::VectorErase {
            layer,
            before,
            lines: crate::vector::lines(&geometry),
            geometry,
            last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
            radius: (tip_size / 2.0).max(0.5),
            mode: crate::vector::Erase::of(sub.kind),
            offset,
            changed: false,
            label: crate::localize(&sub.name),
        });
        self.vector_erase_drag(pos, viewport, size, cx);
    }

    /// The eraser on a vector layer dragged to `pos`: every line it passes
    /// over goes as its Vector setting says, and the layer is painted again
    /// from the lines left.
    pub(super) fn vector_erase_drag(
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
        let Some(Gesture::VectorErase {
            layer,
            lines,
            last,
            radius,
            mode,
            offset,
            changed,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let to = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        let step = (*radius * 0.5).max(0.5);
        let length = (to[0] - last[0]).hypot(to[1] - last[1]);
        let count = (length / step).ceil().max(1.0) as usize;
        let mut erased = false;
        for n in 1..=count {
            let t = n as f32 / count as f32;
            let at = [
                last[0] + (to[0] - last[0]) * t,
                last[1] + (to[1] - last[1]) * t,
            ];
            if let Some(left) = crate::vector::erase(lines, at, *radius, *mode) {
                *lines = left;
                erased = true;
            }
        }
        *last = to;
        if !erased {
            return;
        }
        *changed = true;
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, size[0], size[1]]);
        }
    }

    /// The eraser lifted from a vector layer: the lines left and their
    /// pixels as one undo step.
    pub(super) fn vector_erase_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::VectorErase {
            layer,
            before,
            geometry,
            lines,
            changed,
            label,
            ..
        } = gesture
        else {
            return;
        };
        if !changed {
            return;
        }
        let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let patch = doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                label.clone(),
                vec![patch],
            ))
        });
        let edit =
            crate::vector::keep(&mut cx.world.arena, layer, geometry, &lines, pixels, &label);
        cx.record(ArenaEdit(edit));
    }
}
