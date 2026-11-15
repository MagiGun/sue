//! The Gradient tool on the canvas: a drag lays the gradient, on release.

use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Gradient tool pressed.
    pub(super) fn gradient_press(
        &mut self,
        press: &Press,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, op, .. } = *press;
        let tool = Tool::Gradient;
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
    }
}
