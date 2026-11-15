//! The Figure tool on the canvas: straight lines, rectangles, ellipses
//! and polygons dragged; the Curve dragged, bent and clicked; polylines,
//! continuous curves and Bezier curves clicked corner by corner; lasso
//! fill; and the saturated and speed effect lines, each on a layer of its
//! own named after its sub tool.

use egui::{Pos2, pos2};
use sue_core::Value;
use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::canvas::shape::{
    BEZIER_CLICK, Bend, Guides, bent, corner_line, filled, line_constrained, outline,
};
use crate::manga;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{BEZIER_CURVE, CURVE, POLYLINE, SATURATED_LINES, SPEED_LINES, SubTool, Tool};

impl SuePaint {
    /// The Figure tool pressed.
    pub(super) fn figure_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, op, .. } = *press;
        let tool = Tool::Figure;
        if sub.mode != CURVE {
            self.figure_draft.bend = None;
        }
        if sub.mode == crate::tools::LASSO_FILL {
            // Lasso fill: drawn round, filled on release.
            if let Err(error) = self.paintable(&cx.world.arena) {
                cx.fail(error);
                return;
            }
            self.gesture = Some(Gesture::LassoFill {
                close: false,
                sub: Box::new(sub),
                points: vec![[p.x, p.y]],
            });
            return;
        }
        if sub.mode == CURVE
            && let Some(bend) = self.figure_draft.bend.take()
            && self.picture() == Some(bend.picture)
        {
            // The click that sets a Curve's bend: the line passes
            // through it.
            let line = bent(bend.from, bend.to, [p.x, p.y]);
            self.stroke_line(line, &bend.sub, cx);
            return;
        }
        if matches!(sub.mode, SATURATED_LINES | SPEED_LINES) {
            // Effect lines make a layer of their own.
        } else if clicks_corners(tool, sub.mode) {
            let mut press = *press;
            if sub.mode == POLYLINE
                && self.polyline_for == Some((tool, POLYLINE))
                && let Some(last) = self.polyline.last().copied()
            {
                // The next corner follows the ruler the last one is on,
                // or Shift's 45 degrees.
                let guides = self.figure_guides(cx);
                let at = guides.line_end(last, [p.x, p.y]).unwrap_or_else(|| {
                    line_constrained(last, [p.x, p.y], sub.angle_step, press.modifiers.shift)
                });
                press.p = pos2(at[0], at[1]);
            }
            self.corners_press(tool, &sub, &press, cx);
            return;
        } else if sub.mode == BEZIER_CURVE {
            if self.bezier_press(press, cx) {
                return;
            }
        } else {
            let ready = if filled(sub.mode) {
                self.paintable(&cx.world.arena).map(|_| ())
            } else {
                self.drawable(&cx.world.arena).map(|_| ())
            };
            if let Err(error) = ready {
                cx.fail(error);
                return;
            }
        }
        self.gesture = Some(Gesture::Shape {
            tool,
            sub: Box::new(sub),
            op,
            from: p,
            to: p,
        });
    }

    /// The rulers a Figure line follows on the current picture.
    fn figure_guides(&self, cx: &AppletContext<'_>) -> Guides {
        self.doc()
            .map(|doc| Guides::of(&cx.world.arena, doc))
            .unwrap_or_default()
    }

    /// The Bezier Curve pressed: on its first anchor the curve closes,
    /// on its last (a double-click) it ends; `true` when that happened.
    /// Elsewhere a new anchor begins, its handle pulled out by the drag.
    fn bezier_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) -> bool {
        let owner = (Tool::Figure, BEZIER_CURVE);
        if self.polyline_for != Some(owner) {
            self.polyline.clear();
            self.figure_draft.handles.clear();
            self.polyline_for = Some(owner);
        }
        let Some(doc) = self.docs.get(self.active) else {
            return true;
        };
        let near = |q: Option<&[f32; 2]>| {
            q.is_some_and(|q| {
                doc.view
                    .to_screen(press.viewport, press.size, pos2(q[0], q[1]))
                    .distance(press.pos)
                    <= 8.0
            })
        };
        let first = near(self.polyline.first());
        let last = near(self.polyline.last());
        if self.polyline.len() >= 2 && (first || last) {
            if first
                && self.polyline.len() >= 3
                && let (Some(start), Some(handle)) = (
                    self.polyline.first().copied(),
                    self.figure_draft.handles.first().copied(),
                )
            {
                self.polyline.push(start);
                self.figure_draft.handles.push(handle);
            }
            self.finish_corners(press.op, cx);
            return true;
        }
        if let Err(error) = self.drawable(&cx.world.arena) {
            cx.fail(error);
            return true;
        }
        false
    }

    /// The Figure tool let go after a drag from `from` to `to` (picture
    /// pixels).
    pub(super) fn figure_release(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        match sub.mode {
            SATURATED_LINES | SPEED_LINES => self.effect_lines(from, to, sub, cx),
            CURVE => {
                // The ends are laid; the pointer bends the line until a
                // click sets it.
                let guides = self.figure_guides(cx);
                let line = outline(
                    sub,
                    [from.x, from.y],
                    [to.x, to.y],
                    self.modifiers.shift,
                    &guides,
                );
                let (a, b) = (line.points[0], line.points[line.points.len() - 1]);
                if let Some(picture) = self.picture()
                    && (b[0] - a[0]).hypot(b[1] - a[1]) >= 4.0
                {
                    self.figure_draft.bend = Some(Bend {
                        picture,
                        from: a,
                        to: b,
                        sub: sub.clone(),
                    });
                }
            }
            BEZIER_CURVE => {
                // A new anchor, and the handle its drag pulled out (none
                // for a click).
                let handle = if from.distance(to) < BEZIER_CLICK {
                    from
                } else {
                    to
                };
                self.polyline.push([from.x, from.y]);
                self.figure_draft.handles.push([handle.x, handle.y]);
            }
            _ => self.figure(from, to, sub, size, cx),
        }
    }

    /// The Figure tool's Polyline, Continuous Curve and Bezier Curve
    /// closed with the corners `points`: lines, open or closed (the last
    /// corner the first again).
    pub(super) fn figure_corners(
        &mut self,
        mut points: Vec<[f32; 2]>,
        mode: u8,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let mut handles = std::mem::take(&mut self.figure_draft.handles);
        if points.len() < 2 {
            return;
        }
        let closed = points.len() >= 4 && points.first() == points.last();
        let line = if mode == POLYLINE {
            points
        } else {
            if closed {
                points.pop();
                handles.truncate(points.len());
            }
            let mut line = corner_line(mode, &points, &handles, closed);
            if closed && let Some(first) = line.first().copied() {
                line.push(first);
            }
            line
        };
        // On a vector layer the figure is laid as one vector line.
        if self.figure_on_vector(&line, mode == POLYLINE, &sub, cx) {
            return;
        }
        self.stroke_line(line, &sub, cx);
    }

    /// The Figure tool's Saturated line (lines from all round stopping at
    /// the ellipse dragged) or Speed line (lines along the drag), as in
    /// Clip Studio Paint each on a new layer named after its sub tool.
    pub(super) fn effect_lines(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let (a, b) = ([from.x, from.y], [to.x, to.y]);
        if (b[0] - a[0]).hypot(b[1] - a[1]) < 4.0 {
            return;
        }
        let speed = sub.mode == SPEED_LINES;
        let lines = manga::EffectLines {
            speed,
            a,
            b,
            count: if speed { 90 } else { 120 },
            line: sub.tip.size.max(0.5),
            seed: (a[0].to_bits() ^ b[1].to_bits().rotate_left(7)) | 1,
            rgb: self.drawing_color(),
        };
        // The layer takes the sub tool's name (CTV-15).
        let name = crate::localize(&sub.name);
        // An effect line layer: the Object tool moves its ends later, and
        // Tool Property changes its lines.
        self.spawn_drawn_layer(
            cx,
            &name,
            manga::EFFECT,
            lines.draw(width, height),
            vec![(manga::EFFECT_SPEC, Value::from(lines.encode()))],
        );
    }
}
