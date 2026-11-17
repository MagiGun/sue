//! The Frame Border tool on the canvas: a panel dragged as a rectangle
//! or a division, a polyline frame clicked corner by corner, or a frame
//! drawn round with the frame pen.

use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Frame Border tool pressed.
    pub(super) fn frame_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, op, .. } = *press;
        let tool = Tool::Frame;
        if sub.mode == crate::tools::FRAME_PEN {
            self.gesture = Some(Gesture::Outline {
                sub: Box::new(sub),
                points: vec![[p.x, p.y]],
            });
        } else if clicks_corners(tool, sub.mode) {
            self.corners_press(tool, &sub, press, cx);
        } else {
            self.gesture = Some(Gesture::Shape {
                tool,
                sub: Box::new(sub),
                op,
                from: p,
                to: p,
            });
        }
    }
}
