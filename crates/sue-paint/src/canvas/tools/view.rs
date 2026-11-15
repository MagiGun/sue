//! The view tools on the canvas: Hand (scroll, or turn with its Rotate
//! sub tool, in steps of its Snap angle or of 45 degrees with Shift) and
//! Zoom (a click zooms by half again, a drag zooms as it goes or around
//! the area dragged out), and the scroll and turn drags that the middle
//! button and the Space and Shift keys borrow.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use crate::canvas::input::angle;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

/// How far a press may wander, in screen points, and still be a click.
const CLICK: f32 = 4.0;

/// How far the pointer goes sideways, in screen points, for the Zoom
/// tool's drag to double (or halve) the zoom.
const SCRUB_DOUBLES: f32 = 140.0;

/// How far the pointer goes sideways, in screen points, for Edit
/// timeline's drag to step one frame.
const SCRUB_FRAME: f32 = 16.0;

/// The Zoom tool's drag: 0 zooms as the pointer goes (right zooms in),
/// 1 zooms to the area dragged out (Zoom tool, Tool Property).
pub(crate) const ZOOM_AREA: u8 = 1;

/// The Shift step of a turn when the Rotate sub tool has no Snap angle.
const SHIFT_STEP: f32 = 45.0;

/// `rotation` snapped to `step` degrees (0: as it is).
pub(crate) fn snapped(rotation: f32, step: f32) -> f32 {
    if step > 0.0 {
        (rotation / step).round() * step
    } else {
        rotation
    }
}

impl SuePaint {
    /// The Hand tool pressed: a drag scrolls the view, or turns it with
    /// the Rotate sub tool.
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
        // A turn borrowed with Shift and Space does not snap by Shift.
        self.turning = if space {
            (0.0, false)
        } else {
            (sub.angle_step.max(0.0), true)
        };
        self.gesture = Some(if sub.mode == 1 {
            Gesture::Rotate {
                start: angle(pos - viewport.center()),
                rotation: doc.view.rotation,
            }
        } else {
            Gesture::Pan { last: pos }
        });
    }

    /// The Zoom tool pressed: a click zooms in or out by half again (Alt
    /// the other way); a drag zooms as it goes, or to the area dragged
    /// out, as its sub tool says.
    pub(super) fn zoom_press(&mut self, press: &Press, sub: &SubTool) {
        let Press {
            pos, p, modifiers, ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        // The direction a click goes, with Alt taken in.
        let mut sub = sub.clone();
        sub.mode = u8::from((sub.mode == 1) != modifiers.alt);
        self.gesture = Some(if sub.kind == ZOOM_AREA {
            Gesture::Shape {
                tool: Tool::Zoom,
                sub: Box::new(sub),
                op: self.select_op,
                from: p,
                to: p,
            }
        } else {
            Gesture::Scrub {
                tool: Tool::Zoom,
                sub: Box::new(sub),
                start: pos,
                base: doc.view.zoom,
                moved: false,
            }
        });
    }

    /// A click of the Zoom tool at `pos` (on screen): in or out by half
    /// again, as `sub`'s direction says.
    fn zoom_click(&mut self, pos: Pos2, sub: &SubTool, viewport: Rect, size: [u32; 2]) {
        let factor = if sub.mode == 1 { 1.0 / 1.5 } else { 1.5 };
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.view.zoom_at(viewport, size, pos, factor);
        }
    }

    /// The Zoom tool's area, dragged out from `from` to `to` (picture
    /// pixels), let go: the area fills the view (zooming out, the view
    /// shrinks into the area). A box too small to see is a click.
    pub(super) fn zoom_area(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        viewport: Rect,
        size: [u32; 2],
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let (a, b) = (
            view.to_screen(viewport, size, from),
            view.to_screen(viewport, size, to),
        );
        let (wide, tall) = ((a.x - b.x).abs(), (a.y - b.y).abs());
        if wide.max(tall) <= CLICK {
            self.zoom_click(a, sub, viewport, size);
            return;
        }
        let (wide, tall) = (wide.max(1.0), tall.max(1.0));
        let factor = if sub.mode == 1 {
            (wide / viewport.width()).max(tall / viewport.height())
        } else {
            (viewport.width() / wide).min(viewport.height() / tall)
        };
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.view.center_on(
                viewport,
                size,
                Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0),
            );
            doc.view.zoom_at(viewport, size, viewport.center(), factor);
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

    /// A turn of the view carried on to `pos`: in steps of the Rotate sub
    /// tool's Snap angle, or by 45 degrees (or that angle) with Shift.
    pub(super) fn rotate_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (step, shift_snaps) = self.turning;
        let step = if shift_snaps && self.modifiers.shift {
            if step > 0.0 { step } else { SHIFT_STEP }
        } else {
            step
        };
        let (Some(Gesture::Rotate { start, rotation }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let now = angle(pos - viewport.center());
        doc.view
            .set_rotation(viewport, size, snapped(*rotation + (now - *start), step));
    }

    /// A scrub carried on to `pos`: the Zoom tool zooms around where it
    /// was pressed, Edit timeline steps through the frames.
    pub(super) fn scrub_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::Scrub {
            tool,
            start,
            base,
            moved,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        if !*moved && pos.distance(*start) <= CLICK {
            return;
        }
        *moved = true;
        let (tool, start, base, across) = (*tool, *start, *base, pos.x - start.x);
        if tool == Tool::Zoom {
            if let Some(doc) = self.docs.get_mut(self.active) {
                let zoom = base * 2_f32.powf(across / SCRUB_DOUBLES);
                doc.view.set_zoom_at(viewport, size, start, zoom);
            }
        } else {
            // A scrub stops at the timeline's ends; it does not go round.
            let last = self
                .timeline_here(&cx.world.arena)
                .map_or(0, |(_, _, length)| i64::from(length) - 1);
            let frame = (base as i64 + (across / SCRUB_FRAME).round() as i64).clamp(0, last.max(0));
            self.go_to_frame(&cx.world.arena, frame);
        }
    }

    /// A scrub let go: one that never moved is a click (the Zoom tool
    /// zooms by half again, Edit timeline steps a frame as it says).
    pub(super) fn scrub_release(
        &mut self,
        gesture: Gesture,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Gesture::Scrub {
            tool,
            sub,
            start,
            moved: false,
            ..
        } = gesture
        else {
            return;
        };
        if tool == Tool::Zoom {
            self.zoom_click(start, &sub, viewport, size);
        } else {
            self.timeline_click(&sub, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_snaps_to_its_step_and_nothing_turns_freely() {
        assert_eq!(snapped(50.0, 45.0), 45.0);
        assert_eq!(snapped(70.0, 45.0), 90.0);
        assert_eq!(snapped(-100.0, 45.0), -90.0);
        assert_eq!(snapped(13.0, 15.0), 15.0);
        assert_eq!(snapped(13.5, 0.0), 13.5);
    }
}
