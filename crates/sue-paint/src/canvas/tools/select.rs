//! The Selection area tool on the canvas: rectangles and ellipses
//! dragged (keeping the sub tool's Aspect), the lasso and Shrink
//! selection, polylines clicked corner by corner, and the selection pen
//! and eraser painted. Shapes soften their edge by the sub tool's
//! Anti-aliasing.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::brush::AntiAlias;
use crate::select::{Aspect, Op, SHRINK_SELECTION, Selection};
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

/// Where a box dragged from `from` to `p` ends: a rectangle or ellipse
/// selection keeps its sub tool's Aspect while it is dragged.
pub(super) fn shape_end(tool: Tool, sub: &SubTool, from: Pos2, p: Pos2) -> Pos2 {
    if tool != Tool::Select || !matches!(sub.mode, 0 | 1) || sub.aspect == Aspect::Free {
        return p;
    }
    let [x, y] = sub.aspect.constrain([from.x, from.y], [p.x, p.y]);
    Pos2::new(x, y)
}

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
        } else if sub.mode == SHRINK_SELECTION {
            self.gesture = Some(Gesture::ShrinkLasso {
                sub: Box::new(sub),
                op,
                points: vec![[p.x, p.y]],
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

    /// `shape` (a mask over the picture, as made with [`Op::Replace`])
    /// softened by `aa` and combined into the selection as `op` says.
    fn select_shape(&mut self, shape: &Selection, op: Op, aa: AntiAlias) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let [width, height] = shape.size();
        let mask = crate::fill::antialias(shape.mask(), width, height, aa);
        let mut selection = doc
            .selection
            .clone()
            .unwrap_or_else(|| Selection::new(width, height));
        selection.apply_mask(&mask, op);
        doc.set_selection(Some(selection));
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
        let to = shape_end(Tool::Select, sub, from, to);
        let tiny = (to - from).length() < 2.0;
        if tiny && op == Op::Replace {
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.set_selection(None);
            }
            return;
        }
        let mut shape = Selection::new(size[0], size[1]);
        let (min, max) = (from.min(to), from.max(to));
        if sub.mode == 1 {
            shape.ellipse([min.x, min.y, max.x, max.y], Op::Replace);
        } else {
            let clamp = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
            shape.rect(
                [
                    clamp(min.x, size[0]),
                    clamp(min.y, size[1]),
                    clamp(max.x, size[0]),
                    clamp(max.y, size[1]),
                ],
                Op::Replace,
            );
        }
        self.select_shape(&shape, op, sub.area_aa);
    }

    /// The lasso let go: the outline it traced, combined as it says.
    pub(super) fn lasso_release(&mut self, gesture: Gesture, size: [u32; 2]) {
        let Gesture::Lasso { op, points } = gesture else {
            return;
        };
        let mut shape = Selection::new(size[0], size[1]);
        shape.polygon(&points, Op::Replace);
        let aa = self.selection_aa();
        self.select_shape(&shape, op, aa);
    }

    /// Shrink selection traced on to `pos`.
    pub(super) fn shrink_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::ShrinkLasso { points, .. }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        if points
            .last()
            .is_none_or(|last| (last[0] - p.x).hypot(last[1] - p.y) > 1.5 / doc.view.zoom.max(0.01))
        {
            points.push([p.x, p.y]);
        }
    }

    /// Shrink selection let go: of the area traced round, what is drawn
    /// in it (on the layers the sub tool refers to, at the frame shown),
    /// with the clear holes the drawing closes in.
    pub(super) fn shrink_release(
        &mut self,
        gesture: Gesture,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Gesture::ShrinkLasso { sub, op, points } = gesture else {
            return;
        };
        if points.len() < 3 {
            return;
        }
        let mut lasso = Selection::new(size[0], size[1]);
        lasso.polygon(&points, Op::Replace);
        let Some(reference) = self.fill_reference(&sub, size, &cx.world.arena) else {
            return;
        };
        let drawn = crate::select::shrink_to_drawn(lasso.mask(), &reference, size);
        let Some(shape) = Selection::from_mask(size[0], size[1], drawn) else {
            return;
        };
        self.select_shape(&shape, op, sub.area_aa);
    }

    /// The anti-aliasing of the Selection sub tool picked (none for the
    /// other tools).
    fn selection_aa(&self) -> AntiAlias {
        let sub = self.toolbox.current();
        if sub.tool == Tool::Select {
            sub.area_aa
        } else {
            AntiAlias::None
        }
    }

    /// Closes the polyline selection clicked out so far, combined as `op`.
    pub(crate) fn finish_polyline(&mut self, op: Op) {
        let points = std::mem::take(&mut self.polyline);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let size = doc.compositor.display().1;
        let mut shape = Selection::new(size[0], size[1]);
        shape.polygon(&points, Op::Replace);
        let aa = self.selection_aa();
        self.select_shape(&shape, op, aa);
    }
}

