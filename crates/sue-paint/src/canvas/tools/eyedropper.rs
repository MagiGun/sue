//! The Eyedropper tool on the canvas: a press picks the color under the
//! pointer, and a drag keeps picking.

use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Eyedropper pressed: picks as its sub tool says, and keeps
    /// picking while it drags.
    pub(super) fn eyedropper_press(
        &mut self,
        press: &Press,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press {
            pos,
            viewport,
            size,
            ..
        } = *press;
        self.pick(pos, sub.mode, viewport, size, cx);
        self.gesture = Some(Gesture::Pick { mode: sub.mode });
    }
}
