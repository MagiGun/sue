//! What each tool does when the pointer or pen lifts, with dust
//! removal, Smart Shape and the end of a polyline selection.

use egui::{Pos2, Rect};
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture;
use sue_wm::{AppletContext, ArenaEdit};

use super::view::shift_rects;
use crate::brush::Finish;
use crate::doc;
use crate::select::{Op, Selection};
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// Remove dust over the box from `from` to `to` (picture pixels), or the
    /// whole layer for a click.
    fn remove_dust(&mut self, from: Pos2, to: Pos2, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let offset = info.style.offset;
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let (width, height) = (raster.width(), raster.height());
        let rect = if (to - from).length() < 3.0 {
            [0, 0, width, height]
        } else {
            let local = |value: f32, by: i64, top: u32| {
                ((value - by as f32).round().max(0.0) as u32).min(top)
            };
            let (min, max) = (from.min(to), from.max(to));
            [
                local(min.x, offset[0], width),
                local(min.y, offset[1], height),
                local(max.x, offset[0], width),
                local(max.y, offset[1], height),
            ]
        };
        let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        let mut after = before.clone();
        let size = sub.amount.round().max(1.0) as u32;
        if crate::correct::remove_dust(&mut after, width, height, rect, size) == 0 {
            cx.notify(tr!("no-dust-that-small-there"));
            return;
        }
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id)
            && let Some(bytes) = raster.bytes_mut()
        {
            bytes.copy_from_slice(&after);
        }
        if let Some(patch) = doc::diff_patch(&before, &after, width, height) {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                info.id,
                tr!("remove-dust"),
                vec![patch],
            )));
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Smart Shape: while a stroke's pen rests in one place long enough,
    /// the stroke becomes the clean shape it was meant to be.
    pub(super) fn smart_shape(
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

    /// Closes the polyline selection clicked out so far, combined as `op`.
    pub(crate) fn finish_polyline(&mut self, op: Op) {
        let points = std::mem::take(&mut self.polyline);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let size = doc.compositor.display().1;
        let mut selection = doc
            .selection
            .clone()
            .filter(|_| op != Op::Replace)
            .unwrap_or_else(|| Selection::new(size[0], size[1]));
        selection.polygon(&points, op);
        doc.set_selection(Some(selection));
    }

    #[allow(clippy::too_many_lines, reason = "one arm per gesture")]
    pub(super) fn release(&mut self, viewport: Rect, size: [u32; 2], cx: &mut AppletContext<'_>) {
        let _ = viewport;
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let pressure = self.pen_pressure();
        match gesture {
            Gesture::Stroke {
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
            } => {
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
                    (seconds > 0.004)
                        .then(|| [(at[0] - from[0]) / seconds, (at[1] - from[1]) / seconds])
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
                    doc.compositor.refresh_view(
                        &cx.world.arena,
                        &rects,
                        Some(layer),
                        visible,
                        step,
                    );
                }
                let eraser = stroke.tip().eraser;
                let kept = drawn.map(|samples| crate::vector::Line::of(&stroke, samples, finish));
                if let Some(raster) = doc::raster(&cx.world.arena, layer) {
                    let edit = stroke.finish(raster, &label);
                    if !edit.is_empty() {
                        match kept {
                            Some(kept) => {
                                // The line stays on the vector layer, in
                                // the same undo step as its pixels.
                                let old = crate::vector::geometry(&cx.world.arena, layer);
                                let mut lines = crate::vector::lines(&old);
                                lines.push(kept);
                                let edit = crate::vector::keep(
                                    &mut cx.world.arena,
                                    layer,
                                    old,
                                    &lines,
                                    Some(edit),
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
            Gesture::Shape {
                tool,
                sub,
                op,
                from,
                to,
            } => match tool {
                Tool::Select => {
                    let tiny = (to - from).length() < 2.0;
                    let Some(doc) = self.docs.get_mut(self.active) else {
                        return;
                    };
                    if tiny && op == Op::Replace {
                        doc.set_selection(None);
                        return;
                    }
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    let (min, max) = (from.min(to), from.max(to));
                    if sub.mode == 1 {
                        selection.ellipse([min.x, min.y, max.x, max.y], op);
                    } else {
                        let clamp =
                            |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
                        selection.rect(
                            [
                                clamp(min.x, size[0]),
                                clamp(min.y, size[1]),
                                clamp(max.x, size[0]),
                                clamp(max.y, size[1]),
                            ],
                            op,
                        );
                    }
                    doc.set_selection(Some(selection));
                }
                Tool::Figure
                    if matches!(
                        sub.mode,
                        crate::comic::SATURATED_LINES | crate::comic::SPEED_LINES
                    ) =>
                {
                    self.effect_lines(from, to, &sub, cx);
                }
                Tool::Figure => self.figure(from, to, &sub, size, cx),
                Tool::Gradient => self.gradient(from, to, &sub, cx),
                Tool::Frame => self.frame_drag(from, to, &sub, cx),
                Tool::Ruler => self.ruler_drag(from, to, &sub, cx),
                Tool::Balloon => self.balloon_drag(from, to, &sub, cx),
                Tool::CorrectLine => self.remove_dust(from, to, &sub, cx),
                _ => {}
            },
            Gesture::VectorErase {
                layer,
                before,
                geometry,
                lines,
                changed,
                label,
                ..
            } => {
                if !changed {
                    return;
                }
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        label.clone(),
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    &label,
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::Correct {
                layer,
                before,
                geometry,
                mut lines,
                path,
                mut changed,
                label,
                ..
            } => {
                // Redraw vector line and its width act on the whole trace.
                let sub = self.toolbox.current().clone();
                let reach = (8.0
                    / self
                        .docs
                        .get(self.active)
                        .map_or(1.0, |doc| doc.view.zoom)
                        .max(0.01))
                .max(sub.tip.size / 2.0);
                let traced = match sub.mode {
                    5 => {
                        let points: Vec<[f32; 2]> = path.iter().map(|(point, _)| *point).collect();
                        crate::correct::redraw(&mut lines, &points, reach)
                    }
                    6 => crate::correct::redraw_width(&mut lines, &path, reach),
                    _ => false,
                };
                if traced {
                    if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
                        && let Some(painted) =
                            crate::vector::render(&lines, raster.width(), raster.height())
                        && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
                    {
                        into.copy_from_slice(from);
                    }
                    if let Some(doc) = self.docs.get_mut(self.active) {
                        doc.compositor
                            .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
                    }
                    changed = true;
                }
                if !changed {
                    return;
                }
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        label.clone(),
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    &label,
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::MoveLines {
                layer,
                before,
                geometry,
                base,
                by,
                ..
            } => {
                if by == [0.0; 2] {
                    return;
                }
                let picked = self
                    .picked_lines
                    .as_ref()
                    .map(|(_, picked)| picked.clone())
                    .unwrap_or_default();
                let lines = crate::vector::moved(&base, &picked, by);
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        tr!("move-lines"),
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    &tr!("move-lines"),
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::RulerPen { points } => {
                if points.len() >= 2 {
                    // Smoothed a little, as a pen line would be.
                    let path = crate::rulers::Path::curve(&crate::correct::simplified_points(
                        &points, 1.0,
                    ));
                    self.add_ruler_path(cx, path, &tr!("create-ruler"));
                }
            }
            Gesture::Camera { .. } => {
                // Keyed at this frame, as one step.
                if self.camera_pending.is_some() {
                    self.run("anim.camera.key", cx);
                }
            }
            Gesture::SelectPaint { .. } => {
                // Keep what is selected (nothing selected is no selection).
                if let Some(doc) = self.docs.get_mut(self.active)
                    && let Some(selection) = doc.selection.take()
                {
                    doc.set_selection(Some(selection));
                }
            }
            Gesture::Orbit {
                layer,
                base,
                before,
                ..
            } => self.finish_view3d(cx, layer, base, &before),
            Gesture::Pose {
                layer,
                base,
                before,
                ..
            } => self.finish_pose(cx, layer, &base, &before),
            Gesture::RulerHandle { handle, before } => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let key = handle.key();
                let after = crate::rulers::Rulers::of(&cx.world.arena, picture).text(key);
                // Back as it was, then the move as one undo step.
                if let Ok(entity) = cx.world.arena.get_by_id_mut(picture) {
                    match &before {
                        Some(text) => {
                            entity.properties_mut().set(key, text.clone());
                        }
                        None => {
                            entity.properties_mut().remove(key);
                        }
                    }
                }
                if after != before {
                    Self::apply(
                        cx,
                        crate::rulers::edit(picture, key, after, &tr!("move-ruler")),
                    );
                }
            }
            Gesture::Outline { sub, points } if sub.tool == Tool::Text => {
                self.text_on_curve(&points, cx);
            }
            Gesture::Outline { sub, points } => self.balloon_drawn(points, &sub, cx),
            Gesture::Comic(edit) => self.finish_comic_edit(*edit, cx),
            Gesture::LightTable { layers, .. } => {
                // Back to where they began, then the move as one step.
                let mut edit = crate::doc::Edit::new(tr!("light-table-tool"));
                for (layer, was) in &layers {
                    let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) else {
                        continue;
                    };
                    let now = crate::anim::place_of(entity);
                    entity
                        .properties_mut()
                        .set(crate::anim::PLACE, crate::anim::place_text(*was));
                    if now != *was {
                        edit = edit.then(sue_core::txn::SetProperty::new(
                            *layer,
                            crate::anim::PLACE,
                            crate::anim::place_text(now),
                        ));
                    }
                }
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            Gesture::LassoFill { sub, close, points } => {
                self.lasso_fill(&sub, close, &points, size, cx);
            }
            Gesture::Lasso { op, points } => {
                if let Some(doc) = self.docs.get_mut(self.active) {
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    selection.polygon(&points, op);
                    doc.set_selection(Some(selection));
                }
            }
            Gesture::Move { layer, old, .. } => {
                let now = cx
                    .world
                    .arena
                    .get_by_id(layer)
                    .map(|entity| {
                        [
                            entity.property(picture::X).cloned(),
                            entity.property(picture::Y).cloned(),
                        ]
                    })
                    .unwrap_or_default();
                if now != old {
                    let mut steps = Batch::new(tr!("move-layer"));
                    for (key, before) in [(picture::X, &old[0]), (picture::Y, &old[1])] {
                        steps = steps.then(match before {
                            Some(value) => SetProperty::new(layer, key, value.clone()),
                            None => SetProperty::remove(layer, key),
                        });
                    }
                    cx.record(ArenaEdit(steps));
                }
            }
            Gesture::Pan { .. }
            | Gesture::Rotate { .. }
            | Gesture::Pick { .. }
            | Gesture::Engine(_)
            | Gesture::Pin { .. }
            | Gesture::Transform { .. }
            | Gesture::BrushSize { .. } => {}
            Gesture::Blend {
                layer,
                before,
                label,
                ..
            } => {
                // The whole drag is one undo step.
                if let Some(after) = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let size = [raster.width(), raster.height()];
                    raster.bytes().map(|bytes| (bytes.to_vec(), size))
                }) && let Some(patch) =
                    doc::diff_patch(&before, &after.0, after.1[0], after.1[1])
                {
                    cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                        layer,
                        label,
                        vec![patch],
                    )));
                }
            }
        }
    }
}
