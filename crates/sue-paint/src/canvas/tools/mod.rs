//! The canvas tools: what each tool does when the pointer or pen goes
//! down on the canvas (press), moves across it (drag) and lifts
//! (release). This file only hands each event to the tool or gesture it
//! belongs to; each tool's own code is in its file, so the tools can
//! change one at a time:
//!
//! - `keys`: the modifier keys, before any tool sees a press
//! - `view`: Hand and Zoom, and the scroll and turn drags
//! - `eyedropper`: the Eyedropper
//! - `brush`: the strokes of Pen, Pencil, Brush, Airbrush, Eraser and
//!   Decoration, with Smart Shape
//! - `vector`: the eraser on vector lines
//! - `blend`: the Blend tool and Copy stamp
//! - `select`: Selection area and its polyline, the selection pen
//! - `fill` and `gradient`: Fill, Close and fill, and Gradient
//! - `figure`: Figure (lines, shapes, polylines, curves, lasso fill and
//!   effect lines)
//! - `corners`: the sub tools that lay corners one click at a time
//! - `frame` and `comic`: Frame Border, Balloon and the comic parts the
//!   Object tool drags
//! - `ruler`: the Ruler tool and ruler handles
//! - `text`: the Text tool
//! - `correct`: Correct line and Remove dust
//! - `move` and `object`: Move layer and the Operation tool
//! - `three`: the Operation tool on 3D layers
//! - `handles`: the light table, puppet warp pins, the camera frame and
//!   the transform box

use egui::{Modifiers, Pos2, Rect};
use sue_wm::AppletContext;

use crate::select::Op;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

mod blend;
mod brush;
mod comic;
mod corners;
mod correct;
mod eyedropper;
mod figure;
mod fill;
mod frame;
mod gradient;
mod handles;
mod keys;
mod r#move;
mod object;
mod ruler;
mod select;
mod text;
mod three;
mod vector;
mod view;

pub(in crate::canvas) use select::{turn_angle, turn_outline};

/// Where a press landed and what came with it, as the tool it goes to
/// sees it.
#[derive(Clone, Copy)]
struct Press {
    /// On screen.
    pos: Pos2,
    /// In picture pixels.
    p: Pos2,
    modifiers: Modifiers,
    /// How a selection made by this press combines with the one there.
    op: Op,
    /// Space is held for the view (as the Space key setting says).
    space: bool,
    viewport: Rect,
    size: [u32; 2],
}

impl SuePaint {
    /// The pointer or pen went down at `pos` (on screen): the held keys
    /// have their say, then the tool.
    pub(in crate::canvas) fn press(
        &mut self,
        pos: Pos2,
        modifiers: Modifiers,
        space: bool,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some((tool, sub, press)) = self.press_keys(pos, modifiers, space, viewport, size, cx)
        else {
            return;
        };
        self.press_as(tool, sub, &press, cx);
    }

    /// A press handed to `tool` with its sub tool `sub`: the tool the
    /// keys left, or one a key or a mouse button borrows.
    fn press_as(&mut self, tool: Tool, sub: SubTool, press: &Press, cx: &mut AppletContext<'_>) {
        let press = *press;
        match tool {
            Tool::Hand => self.hand_press(&press, &sub),
            Tool::Zoom => self.zoom_press(&press, &sub),
            Tool::Eyedropper => self.eyedropper_press(&press, &sub, cx),
            Tool::Pen
            | Tool::Pencil
            | Tool::Brush
            | Tool::Airbrush
            | Tool::Eraser
            | Tool::Decoration => self.brush_press(tool, &press, sub, cx),
            Tool::Text => self.text_press(&press, sub, cx),
            Tool::Blend => self.blend_press(&press, &sub, cx),
            Tool::Ruler => self.ruler_press(&press, sub, cx),
            Tool::Balloon => self.balloon_press(&press, sub, cx),
            Tool::Frame => self.frame_press(&press, sub, cx),
            Tool::Figure => self.figure_press(&press, sub, cx),
            Tool::Fill => self.fill_press(&press, sub, cx),
            Tool::Gradient => self.gradient_press(&press, sub, cx),
            Tool::Select => self.select_press(&press, sub, cx),
            Tool::AutoSelect => self.auto_select(press.p, &sub, press.op, press.size, cx),
            Tool::MoveLayer => self.move_press(&press, &sub, cx),
            Tool::Operation => self.operation_press(&press, &sub, cx),
            Tool::CorrectLine => self.correct_press(&press, sub, cx),
        }
    }

    /// The pointer or pen moved to `pos` (on screen): the gesture under
    /// way goes on.
    pub(in crate::canvas) fn drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        match self.gesture {
            None => {}
            Some(Gesture::Pick { mode }) => self.pick(pos, mode, viewport, size, cx),
            Some(Gesture::Engine(_)) => self.engine_drag(pos, viewport, size),
            Some(Gesture::Blend { .. }) => self.blend_drag(pos, viewport, size, cx),
            Some(Gesture::VectorErase { .. }) => self.vector_erase_drag(pos, viewport, size, cx),
            Some(Gesture::MoveLines { .. }) => self.move_lines_drag(pos, viewport, size, cx),
            Some(Gesture::Pose { .. }) => self.pose_drag(pos, cx),
            Some(Gesture::Orbit { .. }) => self.orbit_drag(pos, cx),
            Some(Gesture::Camera { .. }) => self.camera_drag(pos, viewport, size),
            Some(Gesture::Correct { .. }) => self.correct_drag(pos, viewport, size, cx),
            Some(Gesture::Transform { .. }) => self.transform_box_drag(pos, viewport, size, cx),
            Some(Gesture::RulerHandle { .. }) => self.ruler_handle_drag(pos, viewport, size, cx),
            Some(Gesture::Pin { .. }) => self.pin_drag(pos, viewport, size, cx),
            Some(Gesture::Pan { .. }) => self.pan_drag(pos),
            Some(Gesture::BrushSize { .. }) => self.brush_size_drag(pos),
            Some(Gesture::Rotate { .. }) => self.rotate_drag(pos, viewport, size),
            Some(Gesture::Shape { .. }) => self.shape_drag(pos, viewport, size),
            Some(Gesture::SelectPaint { .. }) => self.select_paint_drag(pos, viewport, size),
            Some(Gesture::RulerPen { .. }) => self.ruler_pen_drag(pos, viewport, size),
            Some(Gesture::Comic(_)) => self.comic_drag(pos, viewport, size),
            Some(Gesture::LightTable { .. }) => self.light_table_drag(pos, viewport, size, cx),
            Some(
                Gesture::Lasso { .. }
                | Gesture::Outline { .. }
                | Gesture::LassoFill { .. }
                | Gesture::ShrinkLasso { .. },
            ) => {
                self.trace_drag(pos, viewport, size);
            }
            Some(Gesture::Move { .. }) => self.move_drag(pos, viewport, size, cx),
            Some(Gesture::Bucket { .. }) => self.bucket_drag(pos, viewport, size),
            Some(Gesture::Scrub { .. }) => self.scrub_drag(pos, viewport, size, cx),
            Some(Gesture::Keyed { .. }) => self.keyed_drag(pos, viewport, size, cx),
            Some(Gesture::Lines(_)) => self.lines_drag(pos, viewport, size, cx),
            Some(Gesture::GradientLine { .. }) => self.gradient_line_drag(pos, viewport, size),
            Some(Gesture::Nudge { .. }) => self.nudge_drag(pos, viewport, size, cx),
            Some(Gesture::CanvasSize { .. }) => self.canvas_size_drag(pos, viewport, size),
        }
    }

    /// The box or line most tools drag out (a selection, a figure, a
    /// gradient, a ruler, a frame, a balloon, a dust box) carried on to
    /// `pos`.
    fn shape_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (
            Some(Gesture::Shape {
                tool,
                sub,
                from,
                to,
                ..
            }),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        *to = select::shape_end(*tool, sub, *from, p);
    }

    /// An outline traced freehand (the lasso, the balloon pen, Text on
    /// Curve, lasso fill, Shrink selection) carried on to `pos`.
    fn trace_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (
            Some(
                Gesture::Lasso { points, .. }
                | Gesture::Outline { points, .. }
                | Gesture::LassoFill { points, .. }
                | Gesture::ShrinkLasso { points, .. },
            ),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
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

    /// The pointer or pen lifted: the gesture under way ends, most of
    /// them as one undo step.
    pub(in crate::canvas) fn release(
        &mut self,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        match gesture {
            Gesture::Shape {
                tool,
                sub,
                op,
                from,
                to,
            } => match tool {
                Tool::Select => self.select_box_release(&sub, op, from, to, size),
                Tool::Figure => self.figure_release(from, to, &sub, size, cx),
                Tool::Gradient => self.gradient(from, to, &sub, cx),
                Tool::Frame => self.frame_drag(from, to, &sub, cx),
                Tool::Ruler => self.ruler_drag(from, to, &sub, cx),
                Tool::Balloon => self.balloon_drag(from, to, &sub, cx),
                Tool::CorrectLine => self.remove_dust(from, to, &sub, cx),
                Tool::Operation => self.select_layers_in(from, to, &sub, cx),
                Tool::Zoom => self.zoom_area(from, to, &sub, viewport, size),
                _ => {}
            },
            gesture @ Gesture::VectorErase { .. } => self.vector_erase_release(gesture, cx),
            gesture @ Gesture::Correct { .. } => self.correct_release(gesture, cx),
            gesture @ Gesture::MoveLines { .. } => self.move_lines_release(gesture, cx),
            gesture @ Gesture::RulerPen { .. } => self.ruler_pen_release(gesture, cx),
            Gesture::Camera { .. } => self.camera_release(cx),
            Gesture::SelectPaint { .. } => self.select_paint_release(),
            Gesture::Orbit {
                layer,
                base,
                before,
                ..
            } => self.finish_view3d(cx, layer, base, &before),
            Gesture::Pose {
                layer,
                base,
                before,
                ..
            } => self.finish_pose(cx, layer, &base, &before),
            gesture @ Gesture::RulerHandle { .. } => self.ruler_handle_release(gesture, cx),
            Gesture::Outline { sub, points } if sub.tool == Tool::Text => {
                self.text_on_curve(&points, cx);
            }
            Gesture::Outline { sub, points } => self.balloon_drawn(points, &sub, cx),
            Gesture::Comic(edit) => self.finish_comic_edit(*edit, cx),
            gesture @ Gesture::LightTable { .. } => self.light_table_let_go(gesture, cx),
            Gesture::LassoFill { sub, close, points } => {
                self.lasso_fill(&sub, close, &points, size, cx);
            }
            gesture @ Gesture::Lasso { .. } => self.lasso_release(gesture, size),
            gesture @ Gesture::Move { .. } => self.move_release(gesture, cx),
            gesture @ Gesture::Scrub { .. } => self.scrub_release(gesture, viewport, size, cx),
            // Move layer's drag of the selected pixels ends with it.
            Gesture::Transform { .. } if self.move_lift => self.move_lift_release(cx),
            gesture @ Gesture::Keyed { .. } => self.keyed_release(gesture, cx),
            Gesture::Pan { .. }
            | Gesture::Rotate { .. }
            | Gesture::Pick { .. }
            | Gesture::Engine(_)
            | Gesture::Pin { .. }
            | Gesture::Transform { .. }
            | Gesture::BrushSize { .. } => {}
            gesture @ Gesture::Blend { .. } => self.blend_release(gesture, cx),
            gesture @ Gesture::Bucket { .. } => self.bucket_release(gesture, size, cx),
            gesture @ Gesture::ShrinkLasso { .. } => self.shrink_release(gesture, size, cx),
            Gesture::Lines(gesture) => self.lines_release(*gesture, cx),
            gesture @ Gesture::GradientLine { .. } => self.gradient_line_release(gesture, cx),
            gesture @ Gesture::Nudge { .. } => self.nudge_release(gesture, cx),
            Gesture::CanvasSize { .. } => {}
        }
    }
}
