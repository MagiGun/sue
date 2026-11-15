//! The brush tools on the canvas (Pen, Pencil, Brush, Airbrush, Eraser
//! and Decoration): a stroke from press to release, with the rulers and
//! grid it snaps to, symmetry, the selection it stays inside, Smart
//! Shape, post correction and flick. A stroke the brush engine can draw
//! goes to it (`canvas/engine.rs`); this file keeps the legacy engine's
//! strokes for the rest.

use egui::{Pos2, Rect};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::brush::{Finish, Stabilizer, Stroke as BrushStroke};
use crate::canvas::view::shift_rects;
use crate::color;
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
            p,
            modifiers,
            viewport,
            size,
            ..
        } = *press;
        if self.refuse_keyframed(cx) {
            return;
        }
        // The brush engine draws every stroke it can (canvas/engine.rs);
        // the rest (a tip that dabs, a vector layer's lines) the legacy
        // engine does.
        if self.engine_press(pos, &sub, modifiers, viewport, size, cx) {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let info = match self.drawable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let offset = info.style.offset;
        let rgb = color::unit(self.drawing_color());
        let mut tip = sub.tip;
        if self.transparent {
            tip.eraser = true;
        }
        if tip.screen_size {
            // The size is the screen's: as many layer pixels as it
            // covers at this zoom.
            tip.size /= doc.view.zoom.max(0.01);
        }
        // Editing the mask: the pen shows the layer, the eraser
        // hides it, in the mask's own pixels.
        let target = doc
            .mask_edit
            .then(|| doc::mask_of(&cx.world.arena, info.id))
            .flatten();
        let vector = target.is_none() && crate::vector::is_vector(&cx.world.arena, info.id);
        if vector && tip.eraser {
            // The eraser takes vector lines away, as its Vector
            // setting says: on every picked vector layer when several
            // are picked, else on this one.
            if !self.vector_erase_layers_press(press.p, tip.size, &sub, cx) {
                self.vector_erase_press(info.id, offset, tip.size, &sub, press, cx);
            }
            return;
        }
        let (target, rgb, locked) = match target {
            Some(mask) => (mask, [1.0; 3], false),
            None => (info.id, rgb, info.alpha_locked && !tip.eraser && !vector),
        };
        // The legacy engine paints four bytes a pixel: a gray mask is the
        // brush engine's (above), never its.
        if doc::raster(&cx.world.arena, target)
            .is_some_and(|raster| raster.format() == sue_core::PixelFormat::Gray8)
        {
            return;
        }
        let mut stroke = BrushStroke::new(target, tip, rgb, locked);
        let other = if self.sub_active { self.main } else { self.sub };
        stroke.set_sub_color(color::unit(other));
        // The picture's rulers, in the layer's pixels.
        let rulers = if doc.show_rulers {
            crate::rulers::Rulers::of(&cx.world.arena, doc.picture)
                .shifted([-(offset[0] as f32), -(offset[1] as f32)])
        } else {
            crate::rulers::Rulers::default()
        };
        // Snap to Special Ruler, and the picture's own switch.
        let special = doc.ruler_on && crate::commands::special_snaps(&cx.world.arena, doc.picture);
        if let Some((center, angle)) = rulers.symmetry.filter(|_| special)
            && let Some(ruler) = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 0)
        {
            // The symmetrical ruler: a copy through every line.
            stroke.set_copies(crate::brush::symmetry(
                center,
                angle,
                crate::rulers::turns(ruler.lines, ruler.mirror),
                ruler.mirror,
            ));
        }
        let shifted = doc
            .selection
            .as_ref()
            .filter(|_| offset != [0, 0])
            .map(|selection| selection.shifted(offset));
        let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        // A stroke that starts near the linear ruler goes onto it.
        let snap = crate::rulers::Snap::begin(
            &rulers,
            local,
            doc.snap_ruler,
            special,
            24.0 / doc.view.zoom.max(0.01),
        );
        let snap = snap.or_else(|| {
            self.snap_grid
                .then(|| {
                    // The finest lines of the picture's grid, through its
                    // origin.
                    let grid = crate::grid::Grid::of(&cx.world.arena, doc.picture);
                    let origin = grid.origin_in(press.size);
                    crate::rulers::Snap::grid(
                        local,
                        [offset[0] as f32 - origin[0], offset[1] as f32 - origin[1]],
                        grid.step(),
                        12.0 / doc.view.zoom.max(0.01),
                    )
                })
                .flatten()
        });
        let local = snap.as_ref().map_or(local, |snap| snap.project(local));
        let mut stabilizer = Stabilizer::new(sub.stabilization, sub.adjust_by_speed);
        let start = stabilizer.sample(local, doc.view.zoom, self.clock.1);
        let pressure = self.pen_pressure();
        // Vector lines are kept whole, not cut by the selection.
        let selection = (!vector)
            .then(|| shifted.as_ref().or(doc.selection.as_ref()))
            .flatten();
        let (tilt, lean) = self.pen_lean(pos, viewport, size);
        stroke.set_tilt(tilt);
        stroke.set_lean(lean);
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, target) {
            stroke.to(raster, start, pressure, selection);
        }
        self.gesture = Some(Gesture::Stroke {
            stroke: Box::new(stroke),
            stabilizer,
            offset,
            raw: local,
            label: crate::localize(&sub.name),
            shifted,
            trail: vec![(self.clock.0, local)],
            correction: sub.correction,
            flick: sub.flick,
            // Preferences, Tools: Shift draws a straight line.
            line: modifiers.shift && cx.rice().get_bool("sue_paint.shift_line").unwrap_or(true),
            snap,
            vector,
            rest: None,
            shaped: !sub.smart_shape,
            shaping: None,
        });
    }

    /// A brush stroke carried on to `pos`.
    pub(super) fn stroke_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let pressure = self.pen_pressure();
        let (tilt, lean) = self.pen_lean(pos, viewport, size);
        let (time, step) = self.clock;
        let (
            Some(Gesture::Stroke {
                stroke,
                stabilizer,
                offset,
                raw,
                shifted,
                trail,
                line,
                snap,
                vector,
                rest,
                shaped,
                ..
            }),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let mut local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        if let Some(snap) = snap.as_mut() {
            // Following a ruler: nothing is drawn until the pen
            // shows which way it goes.
            match snap.follow(local, 4.0 / doc.view.zoom.max(0.01)) {
                Some(on) => local = on,
                None => return,
            }
        }
        *raw = local;
        if *line || (*shaped && rest.is_some()) {
            // A straight line waits for the release; a stroke that
            // became its shape stays it.
            return;
        }
        trail.push((time, local));
        if trail.len() > 64 {
            trail.drain(..trail.len() - 64);
        }
        let smooth = stabilizer.sample(local, doc.view.zoom, step);
        let smooth = snap.as_ref().map_or(smooth, |snap| snap.project(smooth));
        // Fast is 2000 screen points a second (velocity dynamics).
        stroke.set_speed(stabilizer.speed() / 2000.0);
        stroke.set_tilt(tilt);
        stroke.set_lean(lean);
        let selection = (!*vector)
            .then(|| shifted.as_ref().or(doc.selection.as_ref()))
            .flatten();
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer()) {
            stroke.to(raster, smooth, pressure, selection);
        }
    }

    /// Smart Shape: while a stroke's pen rests in one place long enough,
    /// the stroke becomes the clean shape it was meant to be.
    pub(in crate::canvas) fn smart_shape(
        &mut self,
        ctx: &egui::Context,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let (now, pointer, down) = ctx.input(|input| {
            (
                input.time,
                input.pointer.latest_pos(),
                input.pointer.primary_down(),
            )
        });
        if matches!(self.gesture, Some(Gesture::Engine(_))) {
            self.engine_smart(ctx, cx);
            return;
        }
        let Some(Gesture::Stroke {
            stroke,
            raw,
            rest,
            shaped,
            shaping,
            shifted,
            vector,
            snap,
            line,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        // Nothing to do on the straight line Shift draws on release, on a
        // stroke following a ruler, or with Smart Shape off for this tool.
        let (Some(pointer), true) = (pointer, down) else {
            return;
        };
        if *line || snap.is_some() || (*shaped && shaping.is_none()) {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        // Where the stroke last saw the pen, in layer pixels: the place the
        // samples and the corrected shape are in too. The drag handler keeps
        // it current even while a corrected shape is being carried.
        let pen = *raw;
        // A shape already corrected follows the pen until it lifts: dragging
        // moves a line's far end, or turns and sizes a shape around its
        // middle, and lifting the pen keeps what the drag left.
        if let Some(shaping) = shaping.as_mut() {
            if (pen[0] - shaping.at[0]).hypot(pen[1] - shaping.at[1]) < 0.5 {
                return;
            }
            let moved = crate::smart::dragged(&shaping.path, shaping.pivot, shaping.reference, pen);
            shaping.at = pen;
            let selection = (!*vector)
                .then(|| shifted.as_ref().or(doc.selection.as_ref()))
                .flatten();
            if let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer()) {
                stroke.reshape(raster, &moved, selection);
            }
            return;
        }
        match rest {
            Some((since, at)) if at.distance(pointer) <= crate::smart::HOLD_REACH => {
                let waited = now - *since;
                if waited < crate::smart::HOLD_SECONDS {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                        crate::smart::HOLD_SECONDS - waited,
                    ));
                    return;
                }
            }
            _ => {
                *rest = Some((now, pointer));
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    crate::smart::HOLD_SECONDS,
                ));
                return;
            }
        }
        // Rested long enough: the shape, if the stroke makes one.
        let points: Vec<[f32; 2]> = stroke.samples().iter().map(|sample| sample.at).collect();
        let step = (stroke.tip().size * 0.25).clamp(0.75, 4.0);
        let Some(path) = crate::smart::recognize(&points, step) else {
            // No shape fits (yet): the stroke carries on as drawn.
            *rest = Some((now, pointer));
            return;
        };
        // The point of the corrected shape the pen is at: a drag carries it
        // along, so a line's far end follows the pen, and a shape turns and
        // sizes around its pivot.
        let reference = path
            .iter()
            .copied()
            .min_by(|a, b| {
                let (from_a, from_b) = (
                    (a[0] - pen[0]).hypot(a[1] - pen[1]),
                    (b[0] - pen[0]).hypot(b[1] - pen[1]),
                );
                from_a.total_cmp(&from_b)
            })
            .unwrap_or(pen);
        let pivot = crate::smart::pivot(&path);
        let selection = (!*vector)
            .then(|| shifted.as_ref().or(doc.selection.as_ref()))
            .flatten();
        let _ = (viewport, size);
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer()) {
            stroke.reshape(raster, &path, selection);
        }
        *shaping = Some(crate::studio::Shaping {
            pivot,
            reference,
            at: pen,
            path,
        });
        *shaped = true;
    }

    /// A brush stroke ended: finished to where the pen lifted, corrected,
    /// and kept as one undo step (on a vector layer with its line).
    pub(super) fn stroke_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Stroke {
            mut stroke,
            mut stabilizer,
            offset,
            raw,
            label,
            shifted,
            trail,
            correction,
            flick,
            line,
            snap,
            vector,
            rest,
            shaped,
            shaping: _,
        } = gesture
        else {
            return;
        };
        let pressure = self.pen_pressure();
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        // A stroke that became its shape (Smart Shape) ends as it is.
        let smart = shaped && rest.is_some();
        let line = line || smart;
        // A line along a ruler stays exactly on it.
        let snapped = snap.as_ref().is_some_and(crate::rulers::Snap::chosen);
        let (correction, flick) = if line || snapped {
            (0, false)
        } else {
            (correction, flick)
        };
        let layer = stroke.layer();
        let selection = (!vector)
            .then(|| shifted.as_ref().or(doc.selection.as_ref()))
            .flatten();
        // The pen's velocity over its last few hundredths of a
        // second, for a flick.
        let velocity = trail.last().and_then(|&(end, at)| {
            let &(start, from) = trail.iter().find(|(time, _)| end - time <= 0.05)?;
            let seconds = (end - start) as f32;
            (seconds > 0.004).then(|| [(at[0] - from[0]) / seconds, (at[1] - from[1]) / seconds])
        });
        let fast = velocity.filter(|v| v[0].hypot(v[1]) * doc.view.zoom > 700.0);
        let finish = Finish {
            correction,
            flick: fast.filter(|_| flick),
        };
        // A pen's touch ends before its button does, so the last
        // points keep the stroke's own last pressure, not a full one.
        let last = stroke
            .samples()
            .last()
            .map_or(pressure, |sample| sample.pressure);
        let mut drawn = None;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
            if smart {
                // The shape is drawn already.
            } else if line {
                stroke.to(raster, raw, last, selection);
            } else {
                for point in stabilizer.catch_up(raw) {
                    let point = snap.as_ref().map_or(point, |snap| snap.project(point));
                    stroke.to(raster, point, last, selection);
                }
            }
            // A vector line keeps the samples as drawn.
            drawn = vector.then(|| stroke.samples().to_vec());
            // How long the pen was down, so a bounce is no tap.
            if let Some((pressed, _)) = trail.first() {
                let lifted = self
                    .clock
                    .0
                    .max(trail.last().map_or(*pressed, |last| last.0));
                stroke.set_held((lifted - pressed) as f32);
            }
            stroke.end(raster, finish, selection);
        }
        let rects = shift_rects(&stroke.take_dirty_tiles(), offset);
        if rects.is_empty() {
            doc.compositor.accept(&cx.world.arena, layer);
        } else {
            let (visible, step) = self.on_screen;
            doc.compositor
                .refresh_view(&cx.world.arena, &rects, Some(layer), visible, step);
        }
        let eraser = stroke.tip().eraser;
        let kept = drawn.map(|samples| crate::vector::Line::of(&stroke, samples, finish));
        if let Some(raster) = doc::raster(&cx.world.arena, layer) {
            let edit = stroke.finish(raster, &label);
            if !edit.is_empty() {
                match kept {
                    Some(kept) => {
                        // The line stays on the vector layer, in
                        // the same undo step as its pixels; Vector
                        // magnet joins its ends to the ends near them.
                        let old = crate::vector::geometry(&cx.world.arena, layer);
                        let lines = crate::vector::lines(&old);
                        let (lines, pixels) = self.magnet_stroke(cx, layer, kept, lines, edit);
                        let edit = crate::vector::keep(
                            &mut cx.world.arena,
                            layer,
                            old,
                            &lines,
                            pixels,
                            &label,
                        );
                        cx.record(ArenaEdit(edit));
                    }
                    None => {
                        cx.record(ArenaEdit(edit));
                    }
                }
                if !eraser {
                    self.recent.push(self.drawing_color());
                }
            }
        }
    }
}
