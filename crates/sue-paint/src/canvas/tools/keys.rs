//! The modifier keys on the canvas, as Clip Studio Paint has them: what a
//! held Space, Shift, Ctrl or Alt does to a press before any tool sees
//! it, which tool and sub tool the press then goes to (Space borrows the
//! Hand, Alt the Eyedropper), how a selection combines, and the brush
//! size drag of Ctrl and Alt.

use egui::{Modifiers, Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use crate::canvas::input::angle;
use crate::select::Op;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// A press at `pos` (on screen) as the held keys see it: `None` when
    /// a key's own gesture took it, else the tool and sub tool it goes to
    /// and where it landed.
    pub(super) fn press_keys(
        &mut self,
        pos: Pos2,
        modifiers: Modifiers,
        space: bool,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) -> Option<(Tool, SubTool, Press)> {
        let view = self.docs.get(self.active)?.view;
        let p = view.to_picture(viewport, size, pos);
        let mut tool = self.toolbox.tool();
        // Clip Studio Paint's modifiers: Space scrolls, Shift and Space
        // turn the view, Ctrl and Space click to zoom in and Alt and Space
        // to zoom out; Ctrl and Shift pick the layer under the pointer;
        // Ctrl and Alt drag the brush size; Alt picks a color.
        if space && modifiers.shift {
            self.gesture = Some(Gesture::Rotate {
                start: angle(pos - viewport.center()),
                rotation: view.rotation,
            });
            return None;
        }
        if space && (modifiers.command || modifiers.alt) {
            let factor = if modifiers.alt { 1.0 / 1.5 } else { 1.5 };
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.view.zoom_at(viewport, size, pos, factor);
            }
            return None;
        }
        if self.light_tool && !space && self.light_table_press(p, modifiers, cx) {
            return None;
        }
        let rice = |key: &str, default: &str| cx.rice().get_str(key).unwrap_or(default).to_owned();
        let (alt_does, space_does, ctrl_shift) = (
            rice("sue_paint.alt_tool", "eyedropper"),
            rice("sue_paint.space_tool", "hand"),
            rice("sue_paint.ctrl_shift", "pick_layer"),
        );
        let space = space && space_does != "none";
        if modifiers.command && modifiers.shift && !space && ctrl_shift != "none" {
            self.pick_layer_at(&cx.world.arena, p);
            return None;
        }
        if modifiers.command && modifiers.alt && tool.paints() {
            self.gesture = Some(Gesture::BrushSize {
                start: pos.x,
                size: self.toolbox.current().tip.size,
            });
            return None;
        }
        if !space && self.transform.is_some() {
            self.transform_press(pos, view, viewport, size);
            return None;
        }
        if !space && self.warp.is_some() {
            self.warp_press(pos, view, modifiers, viewport, size, cx);
            return None;
        }
        if space {
            tool = Tool::Hand;
        } else if modifiers.alt
            && tool == Tool::Blend
            && self.toolbox.current().mode == crate::blend::COPY_STAMP
        {
            self.copy_stamp_source(p, cx);
            return None;
        } else if modifiers.alt && tool.paints() && tool != Tool::Eraser && tool != Tool::Figure {
            match alt_does.as_str() {
                "eyedropper" => tool = Tool::Eyedropper,
                "eraser" => tool = Tool::Eraser,
                _ => {}
            }
        }
        let sub: SubTool = if tool == self.toolbox.tool() {
            self.toolbox.current().clone()
        } else {
            self.toolbox.of(tool).clone()
        };
        let op = match (modifiers.shift, modifiers.alt) {
            (true, true) => Op::Intersect,
            (true, false) => Op::Add,
            (false, true) => Op::Subtract,
            (false, false) => self.select_op,
        };
        Some((
            tool,
            sub,
            Press {
                pos,
                p,
                modifiers,
                op,
                space,
                viewport,
                size,
            },
        ))
    }

    /// Ctrl and Alt dragged sideways to `pos`: the brush size, larger to
    /// the right.
    pub(super) fn brush_size_drag(&mut self, pos: Pos2) {
        let (Some(Gesture::BrushSize { start, size }), Some(_)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let factor = ((pos.x - *start) / 120.0).exp();
        let next = (*size * factor).clamp(0.5, 2000.0);
        self.toolbox.current_mut().tip.size = next;
    }
}
