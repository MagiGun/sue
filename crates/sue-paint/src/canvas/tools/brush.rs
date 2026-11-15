//! The brush tools on the canvas (Pen, Pencil, Brush, Airbrush, Eraser
//! and Decoration): a stroke from press to release. Every stroke goes to
//! the brush engine (`canvas/engine.rs`), with the rulers and grid it
//! snaps to, symmetry, the selection it stays inside, Smart Shape, post
//! correction and flick; the eraser on a vector layer takes lines away
//! instead (`canvas/lines.rs`).

use egui::Rect;
use sue_wm::AppletContext;

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// A layer whose keyframes are on moves as its keys say: it takes no
    /// strokes (the Layer palette shows its pen locked). `true`, with a
    /// notice saying why, when the current layer is one.
    pub(super) fn refuse_keyframed(&self, cx: &mut AppletContext<'_>) -> bool {
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return false;
        };
        if !cx
            .world
            .arena
            .get_by_id(info.id)
            .is_ok_and(crate::anim::motion::keyframes_on)
        {
            return false;
        }
        cx.fail(tr!("keyframes-on-no-drawing", name = info.name));
        true
    }

    /// A brush tool pressed: a stroke begins.
    pub(super) fn brush_press(
        &mut self,
        _tool: Tool,
        press: &Press,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press {
            pos,
            modifiers,
            viewport,
            size,
            ..
        } = *press;
        if self.refuse_keyframed(cx) {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if let Ok(info) = self.drawable(&cx.world.arena) {
            let mask = doc
                .mask_edit
                .then(|| doc::mask_of(&cx.world.arena, info.id))
                .flatten();
            let eraser = sub.tip.eraser || self.transparent;
            if mask.is_none() && eraser && crate::vector::is_vector(&cx.world.arena, info.id) {
                // The eraser takes vector lines away, as its Vector
                // setting says: on every picked vector layer when several
                // are picked, else on this one.
                let mut tip_size = sub.tip.size;
                if sub.tip.screen_size {
                    tip_size /= doc.view.zoom.max(0.01);
                }
                if !self.vector_erase_layers_press(press.p, tip_size, &sub, cx) {
                    let offset = info.style.offset;
                    self.vector_erase_press(info.id, offset, tip_size, &sub, press, cx);
                }
                return;
            }
        }
        self.engine_press(pos, &sub, modifiers, viewport, size, cx);
    }

    /// Smart Shape: while a stroke's pen rests in one place long enough,
    /// the stroke becomes the clean shape it was meant to be.
    pub(in crate::canvas) fn smart_shape(
        &mut self,
        ctx: &egui::Context,
        _viewport: Rect,
        _size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if matches!(self.gesture, Some(Gesture::Engine(_))) {
            self.engine_smart(ctx, cx);
        }
    }
}
