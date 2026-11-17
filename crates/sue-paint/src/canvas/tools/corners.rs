//! The sub tools that lay corners one click at a time (a polyline
//! selection, a polyline or curve balloon, a polyline frame, the Figure
//! tool's polyline and continuous curve): each click a corner; the first
//! or last one again (a double-click), or Enter, closes them. (The Figure
//! tool's Bezier Curve lays its anchors in `figure.rs`, a drag each.)

use egui::pos2;
use sue_wm::AppletContext;

use super::Press;
use crate::studio::SuePaint;
use crate::tools::{CURVE_BALLOON, POLYLINE_BALLOON, POLYLINE_FRAME, SubTool, Tool};

/// Whether `tool` in `mode` lays corners one click at a time (closed by
/// clicking the first or last corner again, or Enter).
pub(super) fn clicks_corners(tool: Tool, mode: u8) -> bool {
    match tool {
        Tool::Balloon => matches!(mode, CURVE_BALLOON | POLYLINE_BALLOON),
        Tool::Frame => mode == POLYLINE_FRAME,
        Tool::Select => mode == 3,
        Tool::Figure => matches!(
            mode,
            crate::tools::POLYLINE | crate::tools::CONTINUOUS_CURVE
        ),
        _ => false,
    }
}

impl SuePaint {
    /// A corner-laying sub tool pressed: a corner goes in, or the corners
    /// close.
    pub(super) fn corners_press(
        &mut self,
        tool: Tool,
        sub: &SubTool,
        press: &Press,
        cx: &mut AppletContext<'_>,
    ) {
        let Press {
            pos,
            p,
            op,
            viewport,
            size,
            ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        // Polyline selections, balloons and frames, and curve
        // balloons: each click a corner; the first or last one
        // again (a double-click), or Enter, closes it.
        let near = |q: Option<&[f32; 2]>| {
            q.is_some_and(|q| {
                doc.view
                    .to_screen(viewport, size, pos2(q[0], q[1]))
                    .distance(pos)
                    <= 8.0
            })
        };
        let first = near(self.polyline.first());
        let close = first || near(self.polyline.last());
        if self.polyline_for != Some((tool, sub.mode)) {
            self.polyline.clear();
            self.polyline_for = Some((tool, sub.mode));
        }
        if tool == Tool::Figure && close && self.polyline.len() >= 2 {
            // A click on the first corner closes the line; on the
            // last (a double-click) leaves it open.
            if first
                && self.polyline.len() >= 3
                && let Some(start) = self.polyline.first().copied()
            {
                self.polyline.push(start);
            }
            self.finish_corners(op, cx);
        } else if close && self.polyline.len() >= 3 {
            self.finish_corners(op, cx);
        } else {
            self.polyline.push([p.x, p.y]);
        }
    }

    /// Closes the corners clicked out with a corner-laying sub tool: a
    /// polyline or curve balloon, a polyline frame, the Figure tool's
    /// polyline or curve, or a polyline selection (combined as `op`).
    pub(crate) fn finish_corners(&mut self, op: crate::select::Op, cx: &mut AppletContext<'_>) {
        let points = std::mem::take(&mut self.polyline);
        let owner = self.polyline_for.take();
        let sub = self.toolbox.current().clone();
        // The Figure tool's Polyline and Curve: lines, open or closed.
        if let Some((Tool::Figure, mode)) = owner {
            self.figure_corners(points, mode, sub, cx);
            return;
        }
        if points.len() < 3 {
            return;
        }
        match owner {
            Some((Tool::Balloon, mode)) => self.balloon_corners(points, mode, sub, cx),
            Some((Tool::Frame, _)) => self.frame_corners(points, &sub, cx),
            _ => {
                self.polyline = points;
                self.finish_polyline(op);
            }
        }
    }
}
