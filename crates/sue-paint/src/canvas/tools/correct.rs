//! The Correct line tool on the canvas: control points moved, added,
//! deleted and made corners; lines pinched, simplified, connected,
//! widened and redrawn; and Remove dust. Each drag is one undo step.

use egui::{Pos2, Rect};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Correct line tool pressed.
    pub(super) fn correct_press(
        &mut self,
        press: &Press,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, op, .. } = *press;
        let tool = Tool::CorrectLine;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if matches!(sub.mode, 7..=9) {
            // Remove dust, Fill leftover, Select garbage: drag a box, or
            // click for the whole layer.
            if let Err(error) = self.paintable(&cx.world.arena) {
                cx.fail(error);
                return;
            }
            self.gesture = Some(Gesture::Shape {
                tool,
                sub: Box::new(sub),
                op,
                from: p,
                to: p,
            });
        } else {
            let info = match self.drawable(&cx.world.arena) {
                Ok(info) => info,
                Err(error) => {
                    cx.fail(error);
                    return;
                }
            };
            if !crate::vector::is_vector(&cx.world.arena, info.id) {
                cx.fail(tr!("not-vector-layer-correct-line-works", name = info.name));
                return;
            }
            let Some(before) = doc::raster(&cx.world.arena, info.id)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec)
            else {
                return;
            };
            let offset = info.style.offset;
            let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
            // Control points are small: a generous reach, as in CSP.
            let reach = 12.0 / doc.view.zoom.max(0.01);
            let geometry = crate::vector::geometry(&cx.world.arena, info.id);
            let mut lines = crate::vector::lines(&geometry);
            let mut grab = None;
            let mut changed = false;
            match (sub.mode, sub.kind) {
                // Add control point, Split line: where the line is.
                (0, 1 | 6) => {
                    if let Some((line, along)) = crate::correct::line_at(&lines, local, reach) {
                        if sub.kind == 1 {
                            crate::correct::add_control(&mut lines[line], along);
                            changed = true;
                        } else {
                            changed = crate::correct::split(&mut lines, line, along);
                        }
                    }
                }
                (0, kind) => {
                    if let Some((line, index)) = crate::correct::control_at(&lines, local, reach) {
                        match kind {
                            2 => changed = crate::correct::delete_control(&mut lines[line], index),
                            3 => {
                                crate::correct::switch_corner(&mut lines[line], index);
                                changed = true;
                            }
                            _ => grab = Some((line, index, 0.0)),
                        }
                    }
                }
                (1, _) => {
                    grab = crate::correct::line_at(&lines, local, reach)
                        .map(|(line, along)| (line, 0, along));
                }
                _ => {}
            }
            self.gesture = Some(Gesture::Correct {
                layer: info.id,
                before,
                geometry,
                base: lines.clone(),
                lines,
                grab,
                start: local,
                last: local,
                path: Vec::new(),
                done: Vec::new(),
                offset,
                changed: false,
                label: crate::localize(&sub.name),
            });
            if changed {
                self.correct_changed(cx);
            }
            if matches!(sub.mode, 2..=6) {
                self.correct_to(local, cx);
            }
        }
    }

    /// Paints the Correct Line gesture's layer again from its lines.
    pub(super) fn correct_changed(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Correct {
            layer,
            lines,
            changed,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        *changed = true;
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            // The whole picture: the compositor keeps it to the canvas.
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// A Correct Line press or drag at `local` (layer pixels): reworks the
    /// lines as the sub tool says.
    pub(super) fn correct_to(&mut self, local: [f32; 2], cx: &mut AppletContext<'_>) {
        use crate::correct::{self, Widen};
        let sub = self.toolbox.current().clone();
        let pressure = self.pressing_now();
        let Some(Gesture::Correct {
            lines,
            base,
            grab,
            start,
            last,
            path,
            done,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let radius = (sub.tip.size / 2.0).max(1.0);
        let by = [local[0] - start[0], local[1] - start[1]];
        let mut edited = false;
        match sub.mode {
            0 => {
                if let Some((line, index, _)) = *grab {
                    let mut fresh = base[line].clone();
                    match sub.kind {
                        // Up widens (or makes more opaque), down narrows.
                        4 | 5 => correct::shape_control(
                            &mut fresh,
                            index,
                            (-by[1] / 60.0).exp(),
                            sub.kind == 5,
                        ),
                        _ => correct::move_control(&mut fresh, index, by),
                    }
                    lines[line] = fresh;
                    edited = true;
                }
            }
            1 => {
                if let Some((line, _, along)) = *grab {
                    lines[line] = correct::pinch(
                        &base[line],
                        along,
                        by,
                        sub.amount.max(1.0),
                        sub.level,
                        sub.kind,
                    );
                    edited = true;
                }
            }
            2..=4 => {
                // Every half brush along the way.
                let step = (radius * 0.5).max(0.5);
                let length = (local[0] - last[0]).hypot(local[1] - last[1]);
                let count = (length / step).ceil().max(1.0) as usize;
                for n in 1..=count {
                    let t = n as f32 / count as f32;
                    let at = [
                        last[0] + (local[0] - last[0]) * t,
                        last[1] + (local[1] - last[1]) * t,
                    ];
                    edited |= match sub.mode {
                        2 => correct::simplify(lines, at, radius, sub.level, sub.whole),
                        3 => correct::connect(lines, at, radius),
                        _ => correct::adjust_width(
                            lines,
                            at,
                            radius,
                            Widen::of(sub.kind),
                            sub.amount,
                            sub.whole.then_some(&mut *done),
                        ),
                    };
                }
            }
            _ => path.push((local, pressure)),
        }
        *last = local;
        if edited {
            self.correct_changed(cx);
        }
    }

    /// A Correct line drag carried on to `pos`.
    pub(super) fn correct_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::Correct { offset, .. }) = &self.gesture else {
            return;
        };
        let offset = *offset;
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        self.correct_to([p.x - offset[0] as f32, p.y - offset[1] as f32], cx);
    }

    /// A Correct line drag let go: the lines it changed as one undo step.
    pub(super) fn correct_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Correct {
            layer,
            before,
            geometry,
            base,
            mut lines,
            grab,
            path,
            mut changed,
            label,
            ..
        } = gesture
        else {
            return;
        };
        // Redraw vector line and its width act on the whole trace.
        let sub = self.toolbox.current().clone();
        let reach = (8.0
            / self
                .docs
                .get(self.active)
                .map_or(1.0, |doc| doc.view.zoom)
                .max(0.01))
        .max(sub.tip.size / 2.0);
        let mut traced = match sub.mode {
            5 => {
                let points: Vec<[f32; 2]> = path.iter().map(|(point, _)| *point).collect();
                // Simplify: the trace smoothed before it is laid.
                let points = if sub.amount > 0.0 && points.len() > 2 {
                    crate::correct::simplified_points(&points, sub.amount)
                } else {
                    points
                };
                let redrawn = crate::correct::redraw(&mut lines, &points, reach, sub.kind == 1);
                if redrawn && sub.connect {
                    let at = crate::correct::line_at(&lines, points[points.len() - 1], reach)
                        .map(|(line, _)| line);
                    if let Some(line) = at {
                        connect_ends(&mut lines, line, reach);
                    }
                }
                redrawn
            }
            6 => crate::correct::redraw_width(&mut lines, &path, reach),
            _ => false,
        };
        // Pinch: a control point where it was pulled (Add control
        // point), and its ends joined to lines they reach (Connect lines).
        if sub.mode == 1
            && changed
            && let Some((line, _, along)) = grab
            && line < lines.len()
        {
            if sub.add_points
                && let Some(base) = base.get(line)
            {
                crate::correct::add_control_after(&mut lines[line], base, along);
                traced = true;
            }
            if sub.connect {
                traced |= connect_ends(&mut lines, line, reach);
            }
        }
        // An end dragged onto the line's other end, or onto another
        // line's end, joins it.
        if sub.mode == 0
            && sub.kind == 0
            && changed
            && let Some((line, index, _)) = grab
        {
            traced |= crate::canvas::lines::join_dragged_end(&mut lines, line, index, reach);
        }
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
            let patch = doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                label.clone(),
                vec![patch],
            ))
        });
        let edit =
            crate::vector::keep(&mut cx.world.arena, layer, geometry, &lines, pixels, &label);
        cx.record(ArenaEdit(edit));
    }

    /// Remove dust, Fill leftover or Select garbage over the box from
    /// `from` to `to` (picture pixels), or the whole layer for a click.
    pub(super) fn remove_dust(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
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
        if sub.mode == crate::tools::SELECT_GARBAGE {
            // The specks become the selection; the layer is left alone.
            let (mask, found) = crate::correct::dust_mask(&before, width, height, rect, size, true);
            if found == 0 {
                cx.notify(tr!("no-dust-that-small-there"));
                return;
            }
            let Some(picture_size) = self
                .docs
                .get(self.active)
                .map(|doc| doc.compositor.display().1)
            else {
                return;
            };
            let [pw, ph] = picture_size;
            let mut shown = vec![0_u8; (pw * ph) as usize];
            for y in 0..height {
                for x in 0..width {
                    let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                    if px >= 0 && py >= 0 && px < i64::from(pw) && py < i64::from(ph) {
                        shown[(py as u32 * pw + px as u32) as usize] =
                            mask[(y * width + x) as usize];
                    }
                }
            }
            if let Some(shape) = crate::select::Selection::from_mask(pw, ph, shown) {
                self.select_shape(
                    &shape,
                    crate::select::Op::Replace,
                    crate::brush::AntiAlias::None,
                );
            }
            cx.notify(tr!("garbage-selected", count = found));
            return;
        }
        let done = if sub.mode == crate::tools::FILL_LEFTOVER {
            let [r, g, b] = self.drawing_color();
            crate::correct::fill_leftover(&mut after, width, height, rect, size, [r, g, b, 255])
        } else {
            crate::correct::remove_dust(&mut after, width, height, rect, size)
        };
        if done == 0 {
            cx.notify(if sub.mode == crate::tools::FILL_LEFTOVER {
                tr!("no-leftover-that-small-there")
            } else {
                tr!("no-dust-that-small-there")
            });
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
                crate::localize(&sub.name),
                vec![patch],
            )));
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }
}

/// Connect lines: each end of `lines[line]` within `reach` of another
/// line's end goes onto it and the two become one line. `true` when any
/// did.
fn connect_ends(lines: &mut Vec<crate::vector::Line>, line: usize, reach: f32) -> bool {
    let mut joined = false;
    let mut ends = Vec::new();
    for at_start in [true, false] {
        let Some(count) = lines.get(line).map(|found| found.samples.len()) else {
            break;
        };
        if count == 0 {
            break;
        }
        let index = if at_start { 0 } else { count - 1 };
        let snapped = crate::canvas::lines::join_dragged_end(lines, line, index, reach);
        let point = lines[line].samples[index].at;
        // Onto its own other end closes the line: nothing to join.
        let other = lines[line].samples[count - 1 - index].at;
        if snapped && (point[0] - other[0]).hypot(point[1] - other[1]) > 0.5 {
            ends.push(point);
        }
    }
    for point in ends {
        joined |= crate::correct::connect(lines, point, 0.5);
    }
    joined
}
