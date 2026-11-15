//! The Text tool on the canvas: a click types (or goes on typing), and
//! Text on Curve draws its curve first.

use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Text tool pressed.
    pub(super) fn text_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let p = press.p;
        // Text on Curve: the curve is drawn first, then typed along.
        if sub.mode == 1 && !self.typing_holds(p) {
            self.finish_typing(cx);
            self.gesture = Some(Gesture::Outline {
                sub: Box::new(sub),
                points: vec![[p.x, p.y]],
            });
        } else {
            self.text_at(p, cx);
        }
    }
}
