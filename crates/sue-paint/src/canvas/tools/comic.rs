//! The comic tools on the canvas: the Balloon tool (a balloon dragged,
//! drawn with the balloon pen, or clicked corner by corner) and the
//! Object tool's drags of frame border panels and effect line ends.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use super::corners::clicks_corners;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Balloon tool pressed.
    pub(super) fn balloon_press(
        &mut self,
        press: &Press,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, op, .. } = *press;
        let tool = Tool::Balloon;
        if sub.mode == crate::comic::BALLOON_PEN {
            self.gesture = Some(Gesture::Outline {
                sub: Box::new(sub),
                points: vec![[p.x, p.y]],
            });
        } else if !clicks_corners(tool, sub.mode) {
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

    /// A frame border panel or corner, or an effect line end, dragged to
    /// `pos`.
    pub(super) fn comic_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::Comic(edit)), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        edit.to(p);
    }
}
