//! The Frame Border tool on the canvas: a panel dragged as a rectangle
//! or a division, or a polyline frame clicked corner by corner.

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
        if clicks_corners(tool, sub.mode) {
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
