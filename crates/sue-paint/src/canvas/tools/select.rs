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
use crate::studio::{Gesture, SelectTurn, SuePaint};
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

/// How far a selection waiting for its angle has turned with the pointer
/// at `pointer` (picture pixels): the turn from the corner the drag ended
/// at to the pointer, round the shape's middle, in radians.
pub(in crate::canvas) fn turn_angle(turn: &SelectTurn, pointer: [f32; 2]) -> f32 {
    let middle = [
        (turn.from.x + turn.to.x) / 2.0,
        (turn.from.y + turn.to.y) / 2.0,
    ];
    let start = (turn.to.y - middle[1]).atan2(turn.to.x - middle[0]);
    let now = (pointer[1] - middle[1]).atan2(pointer[0] - middle[0]);
    if (pointer[0] - middle[0]).hypot(pointer[1] - middle[1]) < 1.0 {
        return 0.0;
    }
    now - start
}

/// The outline of a selection waiting for its angle, turned by `angle`
/// round its middle (picture pixels): four corners, or an ellipse of
/// many.
pub(in crate::canvas) fn turn_outline(turn: &SelectTurn, angle: f32) -> Vec<[f32; 2]> {
    let middle = [
        (turn.from.x + turn.to.x) / 2.0,
        (turn.from.y + turn.to.y) / 2.0,
    ];
    let half = [
        (turn.to.x - turn.from.x).abs() / 2.0,
        (turn.to.y - turn.from.y).abs() / 2.0,
    ];
    let (sin, cos) = angle.sin_cos();
    let place = |[x, y]: [f32; 2]| [middle[0] + x * cos - y * sin, middle[1] + x * sin + y * cos];
    if turn.sub.mode == 1 {
        (0..96)
            .map(|step| {
                let t = step as f32 / 96.0 * std::f32::consts::TAU;
                place([half[0] * t.cos(), half[1] * t.sin()])
            })
            .collect()
    } else {
        [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
            .into_iter()
            .map(|[sx, sy]| place([sx * half[0], sy * half[1]]))
            .collect()
    }
}

impl SuePaint {
    /// The Selection area tool pressed.
    pub(super) fn select_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, op, size, .. } = *press;
        let tool = Tool::Select;
        // A selection waiting for its angle: this click fixes it.
        if self.select_turn.is_some() {
            self.fix_select_turn(Some([p.x, p.y]), size);
            return;
        }
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

    /// Fixes the selection waiting for its angle, turned to `pointer`
    /// (picture pixels; `None` as it was dragged), on a picture of `size`.
    pub(in crate::canvas) fn fix_select_turn(&mut self, pointer: Option<[f32; 2]>, size: [u32; 2]) {
        let Some(turn) = self.select_turn.take() else {
            return;
        };
        if self.picture() != Some(turn.picture) {
            return;
        }
        let angle = pointer.map_or(0.0, |pointer| turn_angle(&turn, pointer));
        let mut shape = Selection::new(size[0], size[1]);
        shape.polygon(&turn_outline(&turn, angle), Op::Replace);
        self.select_shape(&shape, turn.op, turn.sub.area_aa);
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
    pub(super) fn select_shape(&mut self, shape: &Selection, op: Op, aa: AntiAlias) {
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
        if sub.adjust_angle
            && !tiny
            && let Some(picture) = self.picture()
        {
            // Adjust angle after fixed: it turns with the pointer until a
            // click fixes it.
            self.select_turn = Some(SelectTurn {
                picture,
                sub: Box::new(sub.clone()),
                op,
                from,
                to,
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::fill::tests::{SIZE, picture, selection, sub, with_layers};

    #[test]
    fn a_rectangle_selection_keeps_its_aspect_and_softens_its_edge() {
        // FGV-20: Aspect and Anti-aliasing of the Selection area tool.
        with_layers(vec![(picture(|_, _| false), false)], |paint, _, _| {
            let mut square = sub(Tool::Select, 0);
            square.aspect = Aspect::Square;
            let (from, to) = (Pos2::new(10.0, 5.0), Pos2::new(40.0, 15.0));
            // The drag's end keeps the square while it is dragged too.
            assert_eq!(
                shape_end(Tool::Select, &square, from, to),
                Pos2::new(40.0, 35.0)
            );
            paint.select_box_release(&square, Op::Replace, from, to, SIZE);
            assert_eq!(selection(paint).bounds(), Some([10, 5, 40, 35]));
            // Free keeps the drag as it is, with hard edges by default.
            let free = sub(Tool::Select, 0);
            paint.select_box_release(&free, Op::Replace, from, to, SIZE);
            let hard = selection(paint);
            assert_eq!(hard.bounds(), Some([10, 5, 40, 15]));
            assert!(hard.mask().iter().all(|value| matches!(value, 0 | 255)));
            // Strong anti-aliasing softens the edge.
            let mut soft = free.clone();
            soft.area_aa = AntiAlias::Strong;
            paint.select_box_release(&soft, Op::Replace, from, to, SIZE);
            assert!(
                selection(paint)
                    .mask()
                    .iter()
                    .any(|value| *value > 0 && *value < 255),
                "a soft edge"
            );
        });
    }
}
