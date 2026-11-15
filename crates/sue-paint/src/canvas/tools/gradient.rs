//! The Gradient tool on the canvas.
//!
//! A drag lays the sub tool's gradient from where it starts to where it
//! ends, on release: on the editing layer (or the mask being edited),
//! or, with Where to create set to a new gradient layer, as a layer that
//! stays editable. Shift, or the sub tool's Step of angle, keeps the line
//! to steps. On a gradient layer a drag lays its line again, and a press
//! on one of its ends moves that end alone. Contour line fill's sub tools
//! fill the area clicked from its outline inward.

use egui::{Pos2, Rect};
use sue_core::{EntityId, ReversibleTransaction};
use sue_wm::AppletContext;

use super::Press;
use crate::canvas::shape::line_constrained;
use crate::doc;
use crate::gradient::{Lay, Look};
use crate::studio::{Gesture, SuePaint};
use crate::tools::{CONTOUR_FILL, SubTool};

/// How near to one of a gradient layer's ends, in screen points, a press
/// takes that end.
const END_REACH: f32 = 10.0;

impl SuePaint {
    /// The Gradient tool pressed.
    pub(super) fn gradient_press(
        &mut self,
        press: &Press,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, op, .. } = *press;
        let arena = &cx.world.arena;
        if sub.mode != CONTOUR_FILL
            && let Some((layer, gradient)) = self.gradient_layer(arena)
        {
            if let Some(info) = self.layer_info(arena)
                && info.locked
            {
                cx.fail(tr!("is-locked", name = info.name));
                return;
            }
            let zoom = self.doc().map_or(1.0, |doc| doc.view.zoom.max(0.01));
            let reach = END_REACH / zoom;
            let near = |end: [f32; 2]| (end[0] - p.x).hypot(end[1] - p.y) <= reach;
            let [start, finish] = gradient.line;
            let (line, end) = if near(finish) {
                (gradient.line, 1)
            } else if near(start) {
                (gradient.line, 0)
            } else {
                ([[p.x, p.y]; 2], 1)
            };
            self.gesture = Some(Gesture::GradientLine { layer, line, end });
            return;
        }
        if sub.mode == CONTOUR_FILL {
            if let Err(error) = self.area_target(arena) {
                cx.fail(error);
                return;
            }
            self.gesture = Some(Gesture::Bucket {
                sub: Box::new(sub),
                seeds: vec![[p.x, p.y]],
            });
            return;
        }
        if !sub.new_layer
            && let Err(error) = self.area_target(arena)
        {
            cx.fail(error);
            return;
        }
        if sub.new_layer && self.picture().is_none() {
            return;
        }
        self.gesture = Some(Gesture::Shape {
            tool: crate::tools::Tool::Gradient,
            sub: Box::new(sub),
            op,
            from: p,
            to: p,
        });
    }

    /// The current layer and its gradient, when it is a gradient layer.
    pub(crate) fn gradient_layer(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(EntityId, doc::Gradient)> {
        let layer = self.layer()?;
        let entity = arena.get_by_id(layer).ok()?;
        (entity.class() == doc::GRADIENT_LAYER).then(|| (layer, doc::Gradient::of(entity)))
    }

    /// The end of a gradient layer's line under way moved to `pos`.
    pub(super) fn gradient_line_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let step = self.toolbox.current().angle_step;
        let shift = self.modifiers.shift;
        let (Some(Gesture::GradientLine { line, end, .. }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let kept = line[1 - *end];
        line[*end] = line_constrained(kept, [p.x, p.y], step, shift);
    }

    /// A gradient layer's line let go: kept, as one undo step.
    pub(super) fn gradient_line_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::GradientLine { layer, line, .. } = gesture else {
            return;
        };
        let [from, to] = line;
        if (to[0] - from[0]).hypot(to[1] - from[1]) < 1.0 {
            return;
        }
        let Ok(entity) = cx.world.arena.get_by_id(layer) else {
            return;
        };
        let mut gradient = doc::Gradient::of(entity);
        if gradient.line == line {
            return;
        }
        gradient.line = line;
        Self::apply(cx, doc::set_gradient(layer, &gradient, &tr!("gradient")));
    }

    /// The sub tool's gradient as it is laid now: the main, sub and
    /// drawing colors filled in; on a mask every stop is white, so only
    /// its opacity counts.
    fn gradient_look_now(&self, sub: &SubTool, mask: bool) -> Look {
        let mut look = sub.gradient_look(self.main, self.sub, self.drawing_color());
        if mask {
            for index in 0..look.map.stops().len() {
                if let Some(stop) = look.map.stop_mut(index) {
                    stop.rgb = [u8::MAX; 3];
                }
            }
        }
        look
    }

    /// The gradient tool let go after a drag from `from` to `to`: laid on
    /// the editing layer (or the mask being edited), or made a gradient
    /// layer; with the transparent color, a gradient of the drawing color
    /// clears as much as it would have laid.
    pub(in crate::canvas) fn gradient(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let end = line_constrained(
            [from.x, from.y],
            [to.x, to.y],
            sub.angle_step,
            self.modifiers.shift,
        );
        if (end[0] - from.x).hypot(end[1] - from.y) < 1.0 {
            return;
        }
        let line = [[from.x, from.y], end];
        if sub.new_layer {
            self.gradient_layer_from(line, sub, cx);
            return;
        }
        let Ok(target) = self.area_target(&cx.world.arena) else {
            return;
        };
        let look = self.gradient_look_now(sub, target.mask);
        let takes_drawing = sub
            .gradient
            .stops()
            .iter()
            .any(|stop| stop.color == crate::gradient_map::StopColor::Drawing);
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let how = Lay {
            selection: selection.as_ref(),
            opacity: sub.tip.opacity,
            blend: if target.mask {
                sue_model::BlendMode::Normal
            } else {
                sub.blend
            },
            alpha_lock: target.alpha_lock,
            clear: target.color.is_none() && takes_drawing,
        };
        let offset = target.offset;
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            target.id,
            &tr!("gradient"),
            |pixels, width, height| {
                crate::gradient::lay(pixels, [width, height], offset, line, &look, &how);
            },
        ) {
            Self::apply(cx, edit);
        }
    }

    /// Where to create, New gradient layer: a gradient layer of the sub
    /// tool's gradient along `line` over the current layer, with its
    /// blending mode and opacity, masked to the selection when there is
    /// one, as one undo step; it becomes the current layer.
    fn gradient_layer_from(
        &mut self,
        line: [[f32; 2]; 2],
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let mut look = self.gradient_look_now(sub, false);
        // The layer keeps the colors it is made with.
        look.map = look.map.owned();
        let gradient = doc::Gradient { line, look };
        let name = tr!("gradient");
        let (mut edit, layer) =
            doc::new_gradient_layer(&cx.world.arena, picture, self.layer(), &name, gradient);
        edit = edit.then(sue_core::txn::SetProperty::new(
            layer,
            sue_model::picture::OPACITY,
            f64::from(sub.tip.opacity.clamp(0.0, 1.0)),
        ));
        if sub.blend != sue_model::BlendMode::Normal {
            edit = edit.then(sue_core::txn::SetProperty::new(
                layer,
                sue_model::picture::BLEND,
                sub.blend.name(),
            ));
        }
        if let Some(selection) = self.doc().and_then(|doc| doc.selection.clone()) {
            // The mask is made against the layer, so the layer is laid
            // for a moment, then taken back; both go in as one step.
            let arena = &mut cx.world.arena;
            if edit.apply(arena).is_err() {
                return;
            }
            let mask = doc::new_mask(arena, layer, |x, y| {
                if x < 0 || y < 0 {
                    0.0
                } else {
                    selection.at(x as u32, y as u32)
                }
            });
            if edit.revert(arena).is_err() {
                return;
            }
            if let Ok((mask, _)) = mask {
                edit = edit.then(mask);
            }
        }
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(layer);
        }
    }

    /// Contour line fill let go: the areas the click (or drag) reached
    /// take the gradient from their outline inward, as one undo step.
    pub(super) fn contour_fill(
        &mut self,
        seeds: &[[f32; 2]],
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let target = match self.area_target(&cx.world.arena) {
            Ok(target) => target,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(mask) = self.flood_mask(seeds, sub, size, &cx.world.arena, true) else {
            return;
        };
        let places = crate::gradient::contour_places(&mask, size, sub.contour_width);
        let look = self.gradient_look_now(sub, target.mask);
        let takes_drawing = sub
            .gradient
            .stops()
            .iter()
            .any(|stop| stop.color == crate::gradient_map::StopColor::Drawing);
        let how = Lay {
            selection: None,
            opacity: sub.tip.opacity,
            blend: if target.mask {
                sue_model::BlendMode::Normal
            } else {
                sub.blend
            },
            alpha_lock: target.alpha_lock,
            clear: target.color.is_none() && takes_drawing,
        };
        let offset = target.offset;
        let label = crate::localize(&sub.name);
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            target.id,
            &label,
            |pixels, width, height| {
                crate::gradient::lay_each(pixels, [width, height], offset, &how, |px, py| {
                    if px < 0 || py < 0 || px >= i64::from(size[0]) || py >= i64::from(size[1]) {
                        return None;
                    }
                    let index = (py as u32 * size[0] + px as u32) as usize;
                    let t = places.get(index).copied().flatten()?;
                    let mut color = look.color_at(t);
                    color[3] *= f32::from(mask[index]) / 255.0;
                    Some(color)
                });
            },
        ) {
            Self::apply(cx, edit);
        }
    }

    /// A gradient layer changed from Tool Property: shown at once while a
    /// control is held, kept as one undo step when it is let go (`held`
    /// says whether a pointer button is down).
    pub(crate) fn gradient_frame(&mut self, held: bool, cx: &mut AppletContext<'_>) {
        // The gradient sets, read once, for the bar's set row.
        self.gradients.load(crate::colorsets::folder(cx));
        let pending = self.gradient_work.pending.take();
        if let Some((layer, gradient)) = pending {
            if self
                .gradient_work
                .preview
                .as_ref()
                .is_some_and(|(shown, _)| *shown != layer)
            {
                self.finish_gradient_preview(cx);
            }
            if self.gradient_work.preview.is_none() {
                let Ok(entity) = cx.world.arena.get_by_id(layer) else {
                    return;
                };
                let before = doc::Gradient::of(entity);
                self.gradient_work.preview = Some((layer, before));
            }
            // Shown now, outside the history: the step comes on release.
            let mut shown = doc::set_gradient(layer, &gradient, "");
            if shown.apply(&mut cx.world.arena).is_ok() {
                self.refresh_gradient(cx);
            }
        }
        if !held {
            self.finish_gradient_preview(cx);
        }
    }

    /// Keeps the gradient layer shown changed as one undo step from what
    /// it was.
    fn finish_gradient_preview(&mut self, cx: &mut AppletContext<'_>) {
        let Some((layer, before)) = self.gradient_work.preview.take() else {
            return;
        };
        let Ok(entity) = cx.world.arena.get_by_id(layer) else {
            return;
        };
        let after = doc::Gradient::of(entity);
        if after == before {
            return;
        }
        // Back to what it was, then forward again as a step of its own.
        let mut back = doc::set_gradient(layer, &before, "");
        if back.apply(&mut cx.world.arena).is_ok() {
            Self::apply(cx, doc::set_gradient(layer, &after, &tr!("gradient")));
        }
        self.refresh_gradient(cx);
    }

    /// Draws the whole picture again on the next frame.
    fn refresh_gradient(&mut self, cx: &AppletContext<'_>) {
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }
}
