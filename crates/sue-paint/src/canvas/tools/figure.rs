//! The Figure tool on the canvas: straight lines, rectangles and
//! ellipses dragged, polylines and curves clicked corner by corner, lasso
//! fill, and the saturated and speed effect lines, each on a layer of its
//! own.

use egui::Pos2;
use sue_core::Value;
use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::manga;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SPEED_LINES, SubTool, Tool};

impl SuePaint {
    /// The Figure tool pressed.
    pub(super) fn figure_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, op, .. } = *press;
        let tool = Tool::Figure;
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
        } else if matches!(
            sub.mode,
            crate::tools::SATURATED_LINES | crate::tools::SPEED_LINES
        ) {
            // Effect lines make a layer of their own.
            self.gesture = Some(Gesture::Shape {
                tool,
                sub: Box::new(sub),
                op,
                from: p,
                to: p,
            });
        } else if !clicks_corners(tool, sub.mode) {
            if let Err(error) = self.paintable(&cx.world.arena) {
                cx.fail(error);
                return;
            }
            self.gesture = Some(Gesture::Shape {
                tool,
                sub: Box::new(sub),
                op,
                from: p,
                to: p,
            });
        } else {
            self.corners_press(tool, &sub, press, cx);
        }
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
        if matches!(
            sub.mode,
            crate::tools::SATURATED_LINES | crate::tools::SPEED_LINES
        ) {
            self.effect_lines(from, to, sub, cx);
        } else {
            self.figure(from, to, sub, size, cx);
        }
    }

    /// The Figure tool's Polyline and Curve closed with the corners
    /// `points`: lines, open or closed.
    pub(super) fn figure_corners(
        &mut self,
        points: Vec<[f32; 2]>,
        mode: u8,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        if points.len() >= 2 {
            let line = if mode == crate::tools::CURVE {
                manga::smooth_open(&points, 12)
            } else {
                points
            };
            self.stroke_line(line, &sub, cx);
        }
    }

    /// The Figure tool's Saturated line (lines from all round stopping at
    /// the ellipse dragged) or Speed line (lines along the drag), as in
    /// Clip Studio Paint each on a new layer of its own.
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
        let name = if speed {
            tr!("speed-lines")
        } else {
            tr!("focus-lines")
        };
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
