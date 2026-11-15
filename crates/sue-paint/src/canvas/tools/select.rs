//! The Selection area tool on the canvas: rectangles and ellipses
//! dragged, the lasso, polylines clicked corner by corner, and the
//! selection pen and eraser painted.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::select::{Op, Selection};
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Selection area tool pressed.
    pub(super) fn select_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, op, size, .. } = *press;
        let tool = Tool::Select;
        if clicks_corners(tool, sub.mode) {
            self.corners_press(tool, &sub, press, cx);
        } else if matches!(sub.mode, 4 | 5) {
            let add = sub.mode == 4;
            let radius = (sub.tip.size / 2.0).max(0.5);
            if let Some(doc) = self.docs.get_mut(self.active) {
                let mut selection = doc
                    .selection
                    .clone()
                    .unwrap_or_else(|| Selection::new(size[0], size[1]));
                selection.disc([p.x, p.y], radius, add);
                doc.selection = Some(selection);
                doc.edges = None;
            }
            self.gesture = Some(Gesture::SelectPaint {
                add,
                last: [p.x, p.y],
            });
        } else {
            self.gesture = Some(if sub.mode == 2 {
                Gesture::Lasso {
                    op,
                    points: vec![[p.x, p.y]],
                }
            } else {
                Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                }
            });
        }
    }

    /// The selection pen or eraser dragged to `pos`.
    pub(super) fn select_paint_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::SelectPaint { add, last }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let radius = (self.toolbox.current().tip.size / 2.0).max(0.5);
        let to = [p.x, p.y];
        let length = (to[0] - last[0]).hypot(to[1] - last[1]);
        let count = (length / (radius * 0.5).max(0.5)).ceil().max(1.0) as usize;
        let mut selection = doc
            .selection
            .clone()
            .unwrap_or_else(|| Selection::new(size[0], size[1]));
        for n in 1..=count {
            let t = n as f32 / count as f32;
            selection.disc(
                [
                    last[0] + (to[0] - last[0]) * t,
                    last[1] + (to[1] - last[1]) * t,
                ],
                radius,
                *add,
            );
        }
        doc.selection = Some(selection);
        doc.edges = None;
        *last = to;
    }

    /// The selection pen or eraser lifted.
    pub(super) fn select_paint_release(&mut self) {
        // Keep what is selected (nothing selected is no selection).
        if let Some(doc) = self.docs.get_mut(self.active)
            && let Some(selection) = doc.selection.take()
        {
            doc.set_selection(Some(selection));
        }
    }

    /// A rectangle or ellipse selection dragged from `from` to `to`
    /// (picture pixels) and let go.
    pub(super) fn select_box_release(
        &mut self,
        sub: &SubTool,
        op: Op,
        from: Pos2,
        to: Pos2,
        size: [u32; 2],
    ) {
        let tiny = (to - from).length() < 2.0;
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        if tiny && op == Op::Replace {
            doc.set_selection(None);
            return;
        }
        let mut selection = doc
            .selection
            .clone()
            .unwrap_or_else(|| Selection::new(size[0], size[1]));
        let (min, max) = (from.min(to), from.max(to));
        if sub.mode == 1 {
            selection.ellipse([min.x, min.y, max.x, max.y], op);
        } else {
            let clamp = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
            selection.rect(
                [
                    clamp(min.x, size[0]),
                    clamp(min.y, size[1]),
                    clamp(max.x, size[0]),
                    clamp(max.y, size[1]),
                ],
                op,
            );
        }
        doc.set_selection(Some(selection));
    }

    /// The lasso let go: the outline it traced, combined as it says.
    pub(super) fn lasso_release(&mut self, gesture: Gesture, size: [u32; 2]) {
        let Gesture::Lasso { op, points } = gesture else {
            return;
        };
        if let Some(doc) = self.docs.get_mut(self.active) {
            let mut selection = doc
                .selection
                .clone()
                .unwrap_or_else(|| Selection::new(size[0], size[1]));
            selection.polygon(&points, op);
            doc.set_selection(Some(selection));
        }
    }

    /// Closes the polyline selection clicked out so far, combined as `op`.
    pub(crate) fn finish_polyline(&mut self, op: Op) {
        let points = std::mem::take(&mut self.polyline);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let size = doc.compositor.display().1;
        let mut selection = doc
            .selection
            .clone()
            .filter(|_| op != Op::Replace)
            .unwrap_or_else(|| Selection::new(size[0], size[1]));
        selection.polygon(&points, op);
        doc.set_selection(Some(selection));
    }
}
