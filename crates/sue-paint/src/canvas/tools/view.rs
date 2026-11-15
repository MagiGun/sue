//! The view tools on the canvas: Hand (scroll, or turn with its Rotate
//! sub tool) and Zoom, and the scroll and turn drags that the middle
//! button and the Space and Shift keys borrow.

use egui::{Pos2, Rect};

use super::Press;
use crate::canvas::input::angle;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Hand tool pressed: a drag scrolls the view, or turns it with
    /// the Rotate sub tool (with Space held it always scrolls).
    pub(super) fn hand_press(&mut self, press: &Press, sub: &SubTool) {
        let Press {
            pos,
            space,
            viewport,
            ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        self.gesture = Some(if sub.mode == 1 && !space {
            Gesture::Rotate {
                start: angle(pos - viewport.center()),
                rotation: doc.view.rotation,
            }
        } else {
            Gesture::Pan { last: pos }
        });
    }

    /// The Zoom tool pressed: in or out by half again (Alt the other way).
    pub(super) fn zoom_press(&mut self, press: &Press, sub: &SubTool) {
        let Press {
            pos,
            modifiers,
            viewport,
            size,
            ..
        } = *press;
        let factor = if (sub.mode == 1) != modifiers.alt {
            1.0 / 1.5
        } else {
            1.5
        };
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.view.zoom_at(viewport, size, pos, factor);
        }
    }

    /// A scroll of the view carried on to `pos`.
    pub(super) fn pan_drag(&mut self, pos: Pos2) {
        let (Some(Gesture::Pan { last }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        doc.view.pan += pos - *last;
        *last = pos;
    }

    /// A turn of the view carried on to `pos`.
    pub(super) fn rotate_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::Rotate { start, rotation }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let now = angle(pos - viewport.center());
        doc.view
            .set_rotation(viewport, size, *rotation + (now - *start));
    }
}
