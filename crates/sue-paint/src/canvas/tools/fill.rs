//! The Fill tool on the canvas: a click fills the area under it; Close
//! and fill and Paint unpainted area are drawn round and filled on
//! release.

use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Fill tool pressed.
    pub(super) fn fill_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, size, .. } = *press;
        if matches!(
            sub.mode,
            crate::tools::CLOSE_AND_FILL | crate::tools::PAINT_UNPAINTED
        ) {
            // Close and fill: drawn round, filled on release.
            if let Err(error) = self.paintable(&cx.world.arena) {
                cx.fail(error);
                return;
            }
            self.gesture = Some(Gesture::LassoFill {
                close: true,
                sub: Box::new(sub),
                points: vec![[p.x, p.y]],
            });
        } else {
            self.bucket(p, &sub, size, cx);
        }
    }
}
