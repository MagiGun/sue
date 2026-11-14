//! What each tool does while the pointer or pen moves across the
//! canvas, with the correction, ruler and blend helpers it drives.

use egui::{Pos2, Rect, pos2};
use sue_model::picture;
use sue_wm::AppletContext;

use super::input::angle;
use crate::doc;
use crate::select::Selection;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
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
        let pressure = self.pen_pressure();
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
                        sub.kind == 0,
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

    /// The eraser on a vector layer dragged to `pos`: every line it passes
    /// over goes as its Vector setting says, and the layer is painted again
    /// from the lines left.
    pub(super) fn vector_erase_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let Some(Gesture::VectorErase {
            layer,
            lines,
            last,
            radius,
            mode,
            offset,
            changed,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let to = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        let step = (*radius * 0.5).max(0.5);
        let length = (to[0] - last[0]).hypot(to[1] - last[1]);
        let count = (length / step).ceil().max(1.0) as usize;
        let mut erased = false;
        for n in 1..=count {
            let t = n as f32 / count as f32;
            let at = [
                last[0] + (to[0] - last[0]) * t,
                last[1] + (to[1] - last[1]) * t,
            ];
            if let Some(left) = crate::vector::erase(lines, at, *radius, *mode) {
                *lines = left;
                erased = true;
            }
        }
        *last = to;
        if !erased {
            return;
        }
        *changed = true;
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, size[0], size[1]]);
        }
    }

    /// A Ruler tool drag from `from` to `to` (picture pixels): lays the
    /// sub tool's ruler, as Clip Studio Paint's Create ruler tools do.
    pub(super) fn ruler_drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        use crate::rulers::{self, Rulers, Special};
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        let Some(picture) = self.picture() else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = dx.hypot(dy);
        let dragged = length >= 4.0;
        // Shift keeps the line to every 45 degrees (Snap angle).
        let angle = if self.modifiers.shift {
            (dy.atan2(dx) / FRAC_PI_4).round() * FRAC_PI_4
        } else {
            dy.atan2(dx)
        };
        let end = [from.x + angle.cos() * length, from.y + angle.sin() * length];
        let (key, text, label) = match sub.mode {
            7 => {
                if !dragged {
                    return;
                }
                let path = rulers::Path::figure([from.x, from.y], [to.x, to.y], sub.kind == 1);
                let mut all = Rulers::of(&cx.world.arena, picture);
                all.paths.push(path);
                (
                    rulers::PATHS,
                    all.text(rulers::PATHS),
                    tr!("create-figure-ruler"),
                )
            }
            5 => {
                if !dragged {
                    return;
                }
                let ruler = Rulers {
                    linear: Some([[from.x, from.y], end]),
                    ..Rulers::default()
                };
                (
                    rulers::LINEAR,
                    ruler.text(rulers::LINEAR),
                    tr!("create-linear-ruler"),
                )
            }
            2 => {
                let special = match sub.kind {
                    0 => Special::Parallel(if dragged { angle } else { 0.0 }),
                    1 => Special::Radial([from.x, from.y]),
                    _ => Special::Concentric([from.x, from.y]),
                };
                (
                    rulers::SPECIAL,
                    Some(special.to_text()),
                    tr!("create-special-ruler"),
                )
            }
            1 => {
                // Add vanishing point: two lines along edges that meet in
                // the distance; where they cross is the point.
                if !dragged {
                    return;
                }
                let line = [[from.x, from.y], end];
                let Some(first) = self.vanishing_line.take() else {
                    self.vanishing_line = Some(line);
                    return;
                };
                let Some(point) = rulers::crossing(first, line) else {
                    cx.notify(tr!("those-lines-run-side-side-so"));
                    return;
                };
                let current = Rulers::of(&cx.world.arena, picture).perspective;
                let Some(ruler) = rulers::Perspective::with_point(current.as_ref(), point) else {
                    cx.fail(tr!("perspective-ruler-has-most-three-vanishing"));
                    return;
                };
                (
                    rulers::PERSPECTIVE,
                    Some(ruler.to_text()),
                    tr!("add-vanishing-point"),
                )
            }
            _ => {
                // Through where the drag began, along it (upright for a
                // click).
                let ruler = Rulers {
                    symmetry: Some(([from.x, from.y], if dragged { angle } else { FRAC_PI_2 })),
                    ..Rulers::default()
                };
                (
                    rulers::SYMMETRY,
                    ruler.text(rulers::SYMMETRY),
                    tr!("create-symmetrical-ruler"),
                )
            }
        };
        if Self::apply(cx, rulers::edit(picture, key, text, &label))
            && let Some(doc) = self.doc_mut()
        {
            doc.show_rulers = true;
            if matches!(sub.mode, 5..=8) {
                doc.snap_ruler = true;
            } else {
                doc.ruler_on = true;
            }
        }
    }

    /// A Blend tool drag to `pos`: dabs every fifth of the brush along the
    /// way, each one reading the pixels as they were before it.
    fn blend_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let sub = self.toolbox.current().clone();
        let kind = crate::blend::BlendKind::of_mode(sub.mode);
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let stamp = self.stamp.and_then(|(_, gap)| gap);
        let Some(Gesture::Blend {
            layer,
            before,
            last,
            since,
            offset,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let (layer, origin) = (*layer, *offset);
        let to = [p.x - origin[0] as f32, p.y - origin[1] as f32];
        let radius = (sub.tip.size / 2.0).max(1.0);
        let step = (radius * 0.2).max(1.0);
        let (dx, dy) = (to[0] - last[0], to[1] - last[1]);
        let length = dx.hypot(dy);
        if length < 0.5 {
            return;
        }
        let mut dirty: Option<[u32; 4]> = None;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
            let (width, height) = (raster.width(), raster.height());
            if let Some(pixels) = raster.bytes_mut() {
                let mut along = step - *since;
                let unit = [dx / length, dy / length];
                while along <= length {
                    let at = [last[0] + unit[0] * along, last[1] + unit[1] * along];
                    let motion = [unit[0] * step, unit[1] * step];
                    let changed = if sub.mode == crate::blend::COPY_STAMP {
                        stamp.and_then(|gap| {
                            crate::blend::stamp(
                                pixels,
                                width,
                                height,
                                before,
                                at,
                                radius,
                                sub.tip.opacity,
                                gap,
                            )
                        })
                    } else {
                        crate::blend::dab(
                            pixels,
                            width,
                            height,
                            kind,
                            at,
                            radius,
                            sub.tip.opacity,
                            motion,
                        )
                    };
                    if let Some([x0, y0, x1, y1]) = changed {
                        dirty = Some(dirty.map_or([x0, y0, x1, y1], |[a, b, c, d]| {
                            [a.min(x0), b.min(y0), c.max(x1), d.max(y1)]
                        }));
                    }
                    along += step;
                }
                *since = length - (along - step);
            }
        }
        *last = to;
        if let (Some([x0, y0, x1, y1]), Some(doc)) = (dirty, self.docs.get_mut(self.active)) {
            let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
            let rect = [
                shift(x0, origin[0]),
                shift(y0, origin[1]),
                shift(x1, origin[0]),
                shift(y1, origin[1]),
            ];
            doc.compositor.refresh(&cx.world.arena, rect);
        }
    }

    pub(super) fn drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if let Some(Gesture::Pick { mode }) = self.gesture {
            self.pick(pos, mode, viewport, size, cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::Blend { .. })) {
            self.blend_drag(pos, viewport, size, cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::VectorErase { .. })) {
            self.vector_erase_drag(pos, viewport, size, cx);
            return;
        }
        if let Some(Gesture::MoveLines {
            layer,
            base,
            start,
            by,
            offset,
            ..
        }) = self.gesture.as_mut()
        {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            *by = [
                p.x - offset[0] as f32 - start[0],
                p.y - offset[1] as f32 - start[1],
            ];
            let picked = self
                .picked_lines
                .as_ref()
                .map(|(_, picked)| picked.clone())
                .unwrap_or_default();
            let lines = crate::vector::moved(base, &picked, *by);
            let layer = *layer;
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
            return;
        }
        if let Some(Gesture::Pose {
            layer,
            joint,
            start,
            base,
            twist,
            ..
        }) = &self.gesture
        {
            let (layer, joint, twist) = (*layer, *joint, *twist);
            let (dx, dy) = (pos.x - start.x, pos.y - start.y);
            let mut pose = base.clone();
            // Seen from behind, sideways drags swing the other way.
            let behind = cx
                .world
                .arena
                .get_by_id(layer)
                .ok()
                .map(crate::three::View3D::of)
                .is_some_and(|view| view.yaw.to_radians().cos() < 0.0);
            let side = if behind { -1.0 } else { 1.0 };
            let turn = &mut pose.0[joint];
            if twist {
                turn[1] = base.0[joint][1] + dx * 0.6;
            } else {
                turn[0] = (base.0[joint][0] + dy * 0.5).clamp(-180.0, 180.0);
                turn[2] = (base.0[joint][2] + dx * 0.5 * side).clamp(-180.0, 180.0);
            }
            self.preview_pose(cx, layer, &pose);
            return;
        }
        if let Some(Gesture::Orbit {
            layer,
            start,
            base,
            mode,
            ..
        }) = &self.gesture
        {
            let (layer, start, base, mode) = (*layer, *start, *base, *mode);
            let (dx, dy) = (pos.x - start.x, pos.y - start.y);
            let mut view = base;
            match mode {
                1 => {
                    let scale = base.distance / 600.0;
                    view.pan = [base.pan[0] - dx * scale, base.pan[1] + dy * scale];
                }
                2 => view.distance = (base.distance * (dy / 200.0).exp()).clamp(0.6, 100.0),
                _ => {
                    view.yaw = base.yaw - dx * 0.4;
                    view.pitch = (base.pitch + dy * 0.4).clamp(-89.0, 89.0);
                }
            }
            self.preview_view3d(cx, layer, &view);
            return;
        }
        if let Some(Gesture::Camera {
            folder,
            start,
            base,
            mode,
        }) = self.gesture
        {
            let Some(doc) = self.docs.get(self.active) else {
                return;
            };
            let p = doc.view.to_picture(viewport, size, pos);
            let frame = doc.frame;
            let mut camera = base;
            let center = base.center;
            match mode {
                0 => {
                    camera.center = [center[0] + p.x - start[0], center[1] + p.y - start[1]];
                }
                1 => {
                    // Corners further out see more: the camera zooms out.
                    let from = (start[0] - center[0]).hypot(start[1] - center[1]).max(1.0);
                    let to = (p.x - center[0]).hypot(p.y - center[1]).max(1.0);
                    camera.scale = (base.scale * from / to).clamp(0.01, 20.0);
                }
                _ => {
                    let from = (start[1] - center[1]).atan2(start[0] - center[0]);
                    let to = (p.y - center[1]).atan2(p.x - center[0]);
                    camera.angle = base.angle + (to - from);
                }
            }
            self.camera_pending = Some((folder, frame, camera));
            return;
        }
        if let Some(Gesture::Correct { offset, .. }) = &self.gesture {
            let offset = *offset;
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            self.correct_to([p.x - offset[0] as f32, p.y - offset[1] as f32], cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::Transform { .. })) {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            let offset = self
                .transform
                .as_ref()
                .map_or([0.0; 2], |session| session.offset.map(|value| value as f32));
            self.transform_drag([p.x - offset[0], p.y - offset[1]], self.modifiers);
            self.transform_preview(cx);
            return;
        }
        if let Some(Gesture::RulerHandle { handle, .. }) = self.gesture {
            let Some(doc) = self.docs.get(self.active) else {
                return;
            };
            let p = doc.view.to_picture(viewport, size, pos);
            let picture = doc.picture;
            let fix = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 1)
                .is_none_or(|sub| sub.fix_eye_level);
            let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture).moved(
                handle,
                [p.x, p.y],
                fix,
                size,
            );
            if let Some(text) = rulers.text(handle.key())
                && let Ok(entity) = cx.world.arena.get_by_id_mut(picture)
            {
                // Shown as it moves; one undo step when it is let go.
                entity.properties_mut().set(handle.key(), text);
            }
            return;
        }
        if let Some(Gesture::Pin { index }) = self.gesture {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            if let Some(warp) = self.warp.as_mut()
                && let Some(pin) = warp.pins.get_mut(index)
            {
                pin.to = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
            }
            self.warp_preview(cx);
            return;
        }
        let pressure = self.pen_pressure();
        let (tilt, lean) = self.pen_lean(pos, viewport, size);
        let (time, step) = self.clock;
        let (Some(gesture), Some(doc)) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        match gesture {
            Gesture::Stroke {
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
            } => {
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
            Gesture::Pan { last } => {
                doc.view.pan += pos - *last;
                *last = pos;
            }
            Gesture::BrushSize { start, size } => {
                let factor = ((pos.x - *start) / 120.0).exp();
                let next = (*size * factor).clamp(0.5, 2000.0);
                self.toolbox.current_mut().tip.size = next;
            }
            Gesture::Rotate { start, rotation } => {
                let now = angle(pos - viewport.center());
                doc.view
                    .set_rotation(viewport, size, *rotation + (now - *start));
            }
            Gesture::Shape { to, .. } => *to = p,
            Gesture::SelectPaint { add, last } => {
                let radius = (self.toolbox.current().tip.size / 2.0).max(0.5);
                let to = [p.x, p.y];
                let length = (to[0] - last[0]).hypot(to[1] - last[1]);
                let count = (length / (radius * 0.5).max(0.5)).ceil().max(1.0) as usize;
                let mut selection = doc
                    .selection
                    .clone()
                    .unwrap_or_else(|| Selection::new(size[0], size[1]));
                for n in 1..=count {
                    let t = n as f32 / count as f32;
                    selection.disc(
                        [
                            last[0] + (to[0] - last[0]) * t,
                            last[1] + (to[1] - last[1]) * t,
                        ],
                        radius,
                        *add,
                    );
                }
                doc.selection = Some(selection);
                doc.edges = None;
                *last = to;
            }
            Gesture::RulerPen { points } => {
                if points.last().is_none_or(|last| {
                    (last[0] - p.x).hypot(last[1] - p.y) > 2.0 / doc.view.zoom.max(0.01)
                }) {
                    points.push([p.x, p.y]);
                }
            }
            Gesture::Comic(edit) => edit.to(p),
            Gesture::LightTable {
                layers,
                start,
                turn,
            } => {
                // Shown as it moves; one undo step when let go.
                let size = doc.compositor.display().1;
                let middle = pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
                let degrees = (angle(p - middle) - angle(*start - middle)).to_degrees();
                for (layer, [dx, dy, was]) in layers.iter() {
                    let place = if *turn {
                        [*dx, *dy, was + degrees]
                    } else {
                        [dx + p.x - start.x, dy + p.y - start.y, *was]
                    };
                    if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                        entity
                            .properties_mut()
                            .set(crate::anim::PLACE, crate::anim::place_text(place));
                    }
                }
                doc.compositor
                    .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
            }
            Gesture::Lasso { points, .. }
            | Gesture::Outline { points, .. }
            | Gesture::LassoFill { points, .. } => {
                if points.last().is_none_or(|last| {
                    (last[0] - p.x).hypot(last[1] - p.y) > 1.5 / doc.view.zoom.max(0.01)
                }) {
                    points.push([p.x, p.y]);
                }
            }
            Gesture::Move {
                layer,
                start,
                origin,
                ..
            } => {
                let (dx, dy) = (
                    (p.x - start.x).round() as i64,
                    (p.y - start.y).round() as i64,
                );
                let (layer, next) = (*layer, [origin[0] + dx, origin[1] + dy]);
                // Where the layer was and where it goes, both drawn again:
                // the compositor only sees pixels change by itself.
                let Some((was, size)) = cx.world.arena.get_by_id(layer).ok().and_then(|entity| {
                    let raster = doc::raster(&cx.world.arena, layer)?;
                    let at = |key: &str| {
                        entity
                            .property(key)
                            .and_then(sue_core::Value::as_i64)
                            .unwrap_or(0)
                    };
                    Some((
                        [at(picture::X), at(picture::Y)],
                        [raster.width(), raster.height()],
                    ))
                }) else {
                    return;
                };
                if was == next {
                    return;
                }
                if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
                    entity.properties_mut().set(picture::X, next[0]);
                    entity.properties_mut().set(picture::Y, next[1]);
                }
                let area = |at: [i64; 2]| {
                    [
                        at[0].max(0),
                        at[1].max(0),
                        (at[0] + i64::from(size[0])).max(0),
                        (at[1] + i64::from(size[1])).max(0),
                    ]
                };
                let (a, b) = (area(was), area(next));
                let rect = [
                    a[0].min(b[0]) as u32,
                    a[1].min(b[1]) as u32,
                    a[2].max(b[2]).min(i64::from(u32::MAX)) as u32,
                    a[3].max(b[3]).min(i64::from(u32::MAX)) as u32,
                ];
                if let Some(doc) = self.doc_mut() {
                    doc.compositor.refresh(&cx.world.arena, rect);
                }
            }
            Gesture::Pick { .. }
            | Gesture::Pin { .. }
            | Gesture::Transform { .. }
            | Gesture::Blend { .. }
            | Gesture::RulerHandle { .. }
            | Gesture::VectorErase { .. }
            | Gesture::Correct { .. }
            | Gesture::MoveLines { .. }
            | Gesture::Camera { .. }
            | Gesture::Orbit { .. }
            | Gesture::Pose { .. } => {}
        }
    }
}
