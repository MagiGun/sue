//! What each tool does when the pointer or pen goes down on the canvas.

use egui::{Modifiers, Pos2, Rect, pos2};
use sue_model::picture::{self, LayerKind};
use sue_wm::AppletContext;

use super::input::angle;
use crate::brush::{Stabilizer, Stroke as BrushStroke};
use crate::color;
use crate::doc;
use crate::select::{Op, Selection};
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    #[allow(clippy::too_many_lines, reason = "one arm per tool")]
    pub(super) fn press(
        &mut self,
        pos: Pos2,
        modifiers: Modifiers,
        space: bool,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let mut tool = self.toolbox.tool();
        // Clip Studio Paint's modifiers: Space scrolls, Shift and Space
        // turn the view, Ctrl and Space click to zoom in and Alt and Space
        // to zoom out; Ctrl and Shift pick the layer under the pointer;
        // Ctrl and Alt drag the brush size; Alt picks a color.
        if space && modifiers.shift {
            self.gesture = Some(Gesture::Rotate {
                start: angle(pos - viewport.center()),
                rotation: doc.view.rotation,
            });
            return;
        }
        if space && (modifiers.command || modifiers.alt) {
            let factor = if modifiers.alt { 1.0 / 1.5 } else { 1.5 };
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.view.zoom_at(viewport, size, pos, factor);
            }
            return;
        }
        // The Light table tool: a drag moves the light table's layers
        // (Shift turns them), the layers themselves as they are.
        if self.light_tool && !space {
            let arena = &cx.world.arena;
            let mut layers = self
                .editing_cel(arena)
                .map(|cel| crate::anim::light_table(arena, cel))
                .unwrap_or_default();
            if self.light_general
                && let Some(picture) = self.picture()
            {
                layers.extend(crate::anim::general_light_table(arena, picture));
            }
            let placed: Vec<(sue_core::EntityId, [f32; 3])> = layers
                .into_iter()
                .filter_map(|layer| {
                    Some((layer, crate::anim::place_of(arena.get_by_id(layer).ok()?)))
                })
                .collect();
            if !placed.is_empty() {
                self.gesture = Some(Gesture::LightTable {
                    layers: placed,
                    start: p,
                    turn: modifiers.shift,
                });
                return;
            }
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
            return;
        }
        if modifiers.command && modifiers.alt && tool.paints() {
            self.gesture = Some(Gesture::BrushSize {
                start: pos.x,
                size: self.toolbox.current().tip.size,
            });
            return;
        }
        if !space && self.transform.is_some() {
            let view = doc.view;
            self.transform_press(pos, view, viewport, size);
            return;
        }
        if !space && self.warp.is_some() {
            // Puppet warp: a pin under the pointer is dragged (Alt takes
            // it away), anywhere else a new pin goes in.
            let view = doc.view;
            let Some(warp) = self.warp.as_mut() else {
                return;
            };
            let local = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
            let offset = warp.offset;
            let hit = warp.pins.iter().position(|pin| {
                let at = pos2(pin.to[0] + offset[0] as f32, pin.to[1] + offset[1] as f32);
                view.to_screen(viewport, size, at).distance(pos) <= 9.0
            });
            match hit {
                Some(index) if modifiers.alt => {
                    warp.pins.remove(index);
                    self.warp_preview(cx);
                }
                Some(index) => self.gesture = Some(Gesture::Pin { index }),
                None => {
                    warp.pins.push(crate::warp::Pin::at(local));
                    self.gesture = Some(Gesture::Pin {
                        index: warp.pins.len() - 1,
                    });
                }
            }
            return;
        }
        if space {
            tool = Tool::Hand;
        } else if modifiers.alt
            && tool == Tool::Blend
            && self.toolbox.current().mode == crate::blend::COPY_STAMP
        {
            // Copy stamp: Alt and click sets where it copies from.
            self.stamp = Some(([p.x, p.y], None));
            cx.notify(tr!("copy-stamp-source-set"));
            return;
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
        match tool {
            Tool::Hand => {
                self.gesture = Some(if sub.mode == 1 && !space {
                    Gesture::Rotate {
                        start: angle(pos - viewport.center()),
                        rotation: doc.view.rotation,
                    }
                } else {
                    Gesture::Pan { last: pos }
                });
            }
            Tool::Zoom => {
                let factor = if (sub.mode == 1) != modifiers.alt {
                    1.0 / 1.5
                } else {
                    1.5
                };
                if let Some(doc) = self.docs.get_mut(self.active) {
                    doc.view.zoom_at(viewport, size, pos, factor);
                }
            }
            Tool::Eyedropper => {
                self.pick(pos, sub.mode, viewport, size, cx);
                self.gesture = Some(Gesture::Pick { mode: sub.mode });
            }
            Tool::Pen
            | Tool::Pencil
            | Tool::Brush
            | Tool::Airbrush
            | Tool::Eraser
            | Tool::Decoration => {
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
                    // setting says.
                    let Some(before) = doc::raster(&cx.world.arena, info.id)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec)
                    else {
                        return;
                    };
                    let geometry = crate::vector::geometry(&cx.world.arena, info.id);
                    self.gesture = Some(Gesture::VectorErase {
                        layer: info.id,
                        before,
                        lines: crate::vector::lines(&geometry),
                        geometry,
                        last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
                        radius: (tip.size / 2.0).max(0.5),
                        mode: crate::vector::Erase::of(sub.kind),
                        offset,
                        changed: false,
                        label: crate::localize(&sub.name),
                    });
                    self.vector_erase_drag(pos, viewport, size, cx);
                    return;
                }
                let (target, rgb, locked) = match target {
                    Some(mask) => (mask, [1.0; 3], false),
                    None => (info.id, rgb, info.alpha_locked && !tip.eraser && !vector),
                };
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
                if let Some((center, angle)) = rulers.symmetry.filter(|_| doc.ruler_on)
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
                    doc.ruler_on,
                    24.0 / doc.view.zoom.max(0.01),
                );
                let snap = snap.or_else(|| {
                    self.snap_grid
                        .then(|| {
                            crate::rulers::Snap::grid(
                                local,
                                [offset[0] as f32, offset[1] as f32],
                                64.0,
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
                    line: modifiers.shift
                        && cx.rice().get_bool("sue_paint.shift_line").unwrap_or(true),
                    snap,
                    vector,
                    rest: None,
                    shaped: !sub.smart_shape,
                    shaping: None,
                });
            }
            // Text on Curve: the curve is drawn first, then typed along.
            Tool::Text if sub.mode == 1 && !self.typing_holds(p) => {
                self.finish_typing(cx);
                self.gesture = Some(Gesture::Outline {
                    sub: Box::new(sub),
                    points: vec![[p.x, p.y]],
                });
            }
            Tool::Text => self.text_at(p, cx),
            Tool::Blend => {
                let info = match self.paintable(&cx.world.arena) {
                    Ok(info) => info,
                    Err(error) => {
                        cx.fail(error);
                        return;
                    }
                };
                let Some(before) = doc::raster(&cx.world.arena, info.id)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                else {
                    return;
                };
                let offset = info.style.offset;
                if sub.mode == crate::blend::COPY_STAMP {
                    match &mut self.stamp {
                        None => {
                            cx.fail(tr!("copy-stamp-needs-source"));
                            return;
                        }
                        // The first stroke fixes the distance; later ones
                        // keep it, as Clip Studio's aligned stamp does.
                        Some((source, gap @ None)) => {
                            *gap = Some([source[0] - p.x, source[1] - p.y]);
                        }
                        Some(_) => {}
                    }
                }
                self.gesture = Some(Gesture::Blend {
                    layer: info.id,
                    before,
                    last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
                    since: 0.0,
                    offset,
                    label: crate::localize(&sub.name),
                });
            }
            Tool::Ruler if sub.mode == 6 => {
                // Curve Ruler: each click places a point; clicking the
                // last one again (or Enter) lays the ruler.
                let near_last = self.curve_points.last().is_some_and(|last| {
                    doc.view
                        .to_screen(viewport, size, pos2(last[0], last[1]))
                        .distance(pos)
                        <= 8.0
                });
                if near_last {
                    self.finish_curve(cx);
                } else {
                    self.curve_points.push([p.x, p.y]);
                }
            }
            Tool::Ruler if sub.mode == 8 => {
                self.gesture = Some(Gesture::RulerPen {
                    points: vec![[p.x, p.y]],
                });
            }
            Tool::Ruler => {
                let picture = doc.picture;
                let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
                let reach = 9.0 / doc.view.zoom.max(0.01);
                let hit = doc
                    .show_rulers
                    .then(|| rulers.handle_at([p.x, p.y], reach, size))
                    .flatten();
                match hit {
                    Some(crate::rulers::Handle::Vanishing(index))
                        if sub.mode == 1 && sub.kind == 1 =>
                    {
                        // Delete vanishing point.
                        let mut points = rulers
                            .perspective
                            .map(|ruler| ruler.points)
                            .unwrap_or_default();
                        if index < points.len() {
                            points.remove(index);
                        }
                        let text = (!points.is_empty())
                            .then(|| crate::rulers::Perspective { points }.to_text());
                        Self::apply(
                            cx,
                            crate::rulers::edit(
                                picture,
                                crate::rulers::PERSPECTIVE,
                                text,
                                &tr!("delete-vanishing-point"),
                            ),
                        );
                    }
                    Some(handle) => {
                        self.gesture = Some(Gesture::RulerHandle {
                            handle,
                            before: rulers.text(handle.key()),
                        });
                    }
                    None => {
                        self.gesture = Some(Gesture::Shape {
                            tool,
                            sub: Box::new(sub),
                            op,
                            from: p,
                            to: p,
                        });
                    }
                }
            }
            Tool::Balloon if sub.mode == crate::comic::BALLOON_PEN => {
                self.gesture = Some(Gesture::Outline {
                    sub: Box::new(sub),
                    points: vec![[p.x, p.y]],
                });
            }
            Tool::Frame | Tool::Balloon if !crate::comic::clicks_corners(tool, sub.mode) => {
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                });
            }
            // Lasso fill and Close and fill: drawn round, filled on
            // release.
            Tool::Figure | Tool::Fill
                if (tool == Tool::Figure && sub.mode == crate::tools::LASSO_FILL)
                    || (tool == Tool::Fill
                        && matches!(
                            sub.mode,
                            crate::tools::CLOSE_AND_FILL | crate::tools::PAINT_UNPAINTED
                        )) =>
            {
                if let Err(error) = self.paintable(&cx.world.arena) {
                    cx.fail(error);
                    return;
                }
                self.gesture = Some(Gesture::LassoFill {
                    close: tool == Tool::Fill,
                    sub: Box::new(sub),
                    points: vec![[p.x, p.y]],
                });
            }
            // Effect lines make a layer of their own.
            Tool::Figure
                if matches!(
                    sub.mode,
                    crate::comic::SATURATED_LINES | crate::comic::SPEED_LINES
                ) =>
            {
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                });
            }
            Tool::Figure | Tool::Gradient if !crate::comic::clicks_corners(tool, sub.mode) => {
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
            }
            tool if crate::comic::clicks_corners(tool, sub.mode) => {
                // Polyline selections, balloons and frames, and curve
                // balloons: each click a corner; the first or last one
                // again (a double-click), or Enter, closes it.
                let near = |q: Option<&[f32; 2]>| {
                    q.is_some_and(|q| {
                        doc.view
                            .to_screen(viewport, size, pos2(q[0], q[1]))
                            .distance(pos)
                            <= 8.0
                    })
                };
                let first = near(self.polyline.first());
                let close = first || near(self.polyline.last());
                if self.polyline_for != Some((tool, sub.mode)) {
                    self.polyline.clear();
                    self.polyline_for = Some((tool, sub.mode));
                }
                if tool == Tool::Figure && close && self.polyline.len() >= 2 {
                    // A click on the first corner closes the line; on the
                    // last (a double-click) leaves it open.
                    if first
                        && self.polyline.len() >= 3
                        && let Some(start) = self.polyline.first().copied()
                    {
                        self.polyline.push(start);
                    }
                    self.finish_corners(op, cx);
                } else if close && self.polyline.len() >= 3 {
                    self.finish_corners(op, cx);
                } else {
                    self.polyline.push([p.x, p.y]);
                }
            }
            Tool::Select if matches!(sub.mode, 4 | 5) => {
                let add = sub.mode == 4;
                let radius = (sub.tip.size / 2.0).max(0.5);
                if let Some(doc) = self.docs.get_mut(self.active) {
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    selection.disc([p.x, p.y], radius, add);
                    doc.selection = Some(selection);
                    doc.edges = None;
                }
                self.gesture = Some(Gesture::SelectPaint {
                    add,
                    last: [p.x, p.y],
                });
            }
            Tool::Select => {
                self.gesture = Some(if sub.mode == 2 {
                    Gesture::Lasso {
                        op,
                        points: vec![[p.x, p.y]],
                    }
                } else {
                    Gesture::Shape {
                        tool,
                        sub: Box::new(sub),
                        op,
                        from: p,
                        to: p,
                    }
                });
            }
            Tool::AutoSelect => self.auto_select(p, &sub, op, size, cx),
            Tool::Fill => self.bucket(p, &sub, size, cx),
            Tool::MoveLayer => {
                let Some(info) = self.layer_info(&cx.world.arena) else {
                    return;
                };
                if info.kind != LayerKind::Raster || info.locked {
                    cx.fail(tr!("cannot-be-moved", name = info.name));
                    return;
                }
                let old = cx
                    .world
                    .arena
                    .get_by_id(info.id)
                    .map(|entity| {
                        [
                            entity.property(picture::X).cloned(),
                            entity.property(picture::Y).cloned(),
                        ]
                    })
                    .unwrap_or_default();
                self.gesture = Some(Gesture::Move {
                    layer: info.id,
                    start: p,
                    origin: info.style.offset,
                    old,
                });
            }
            Tool::Operation if sub.mode == 1 => self.pick_layer_at(&cx.world.arena, p),
            // Frame borders and effect lines: their panels, corners and
            // ends move.
            Tool::Operation if self.on_comic_layer(&cx.world.arena) => {
                let reach = 10.0 / doc.view.zoom.max(0.01);
                self.gesture = self.comic_object_at(p, reach, &cx.world.arena);
            }
            Tool::Operation
                if self
                    .layer()
                    .is_some_and(|layer| crate::three::is_3d(&cx.world.arena, layer)) =>
            {
                // A 3D layer: drag to turn the camera, Shift to move it,
                // Ctrl to come nearer or go further.
                let Some(layer) = self.layer() else {
                    return;
                };
                // A drawing figure: a body part under the pointer bends at
                // its joint (Shift twists it).
                if let Some(joint) = self.figure_part_at(&cx.world.arena, layer, p, size) {
                    let model = crate::figure::of_layer(&cx.world.arena, layer);
                    let (Some(base), Some(before)) = (
                        model
                            .and_then(|model| cx.world.arena.get_by_id(model).ok())
                            .map(crate::figure::Pose::of),
                        doc::raster(&cx.world.arena, layer)
                            .and_then(|raster| raster.bytes())
                            .map(<[u8]>::to_vec),
                    ) else {
                        return;
                    };
                    self.figure_joint = Some(joint);
                    self.gesture = Some(Gesture::Pose {
                        layer,
                        joint,
                        start: pos,
                        base,
                        before,
                        twist: modifiers.shift,
                    });
                    return;
                }
                let (Some(base), Some(before)) = (
                    cx.world
                        .arena
                        .get_by_id(layer)
                        .ok()
                        .map(crate::three::View3D::of),
                    doc::raster(&cx.world.arena, layer)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec),
                ) else {
                    return;
                };
                let mode = if modifiers.shift {
                    1
                } else if modifiers.command {
                    2
                } else {
                    0
                };
                self.gesture = Some(Gesture::Orbit {
                    layer,
                    start: pos,
                    base,
                    before,
                    mode,
                });
            }
            Tool::Operation if self.camera_folder(&cx.world.arena).is_some() => {
                // The camera frame: inside moves it, a corner zooms, outside
                // turns it.
                let Some(folder) = self.camera_folder(&cx.world.arena) else {
                    return;
                };
                let base = self.camera_now(&cx.world.arena, folder);
                let corners = base.frame(size);
                let near_corner = corners.iter().any(|corner| {
                    doc.view
                        .to_screen(viewport, size, pos2(corner[0], corner[1]))
                        .distance(pos)
                        <= 10.0
                });
                let inside = crate::camera::inside(&corners, [p.x, p.y]);
                let mode = if near_corner {
                    1
                } else if inside {
                    0
                } else {
                    2
                };
                self.gesture = Some(Gesture::Camera {
                    folder,
                    start: [p.x, p.y],
                    base,
                    mode,
                });
            }
            Tool::Operation => {
                // Object: click a vector line to pick it (Shift adds or
                // takes away), drag to move what is picked.
                let Some(info) = self.layer_info(&cx.world.arena) else {
                    return;
                };
                if !crate::vector::is_vector(&cx.world.arena, info.id) {
                    self.picked_lines = None;
                    cx.notify(tr!("object-picks-vector-lines-pick-vector"));
                    return;
                }
                let offset = info.style.offset;
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                let geometry = crate::vector::geometry(&cx.world.arena, info.id);
                let lines = crate::vector::lines(&geometry);
                let reach = 8.0 / doc.view.zoom.max(0.01);
                let hit = crate::correct::line_at(&lines, local, reach).map(|(line, _)| line);
                let mut picked = match self.picked_lines.take() {
                    Some((layer, picked)) if layer == info.id => picked,
                    _ => Vec::new(),
                };
                match hit {
                    None => {
                        if !modifiers.shift {
                            picked.clear();
                        }
                    }
                    Some(line) if modifiers.shift => {
                        if let Some(at) = picked.iter().position(|index| *index == line) {
                            picked.remove(at);
                        } else {
                            picked.push(line);
                        }
                    }
                    Some(line) => {
                        if !picked.contains(&line) {
                            picked = vec![line];
                        }
                    }
                }
                let moving = hit.is_some() && !picked.is_empty();
                self.picked_lines = Some((info.id, picked));
                if moving
                    && let Some(before) = doc::raster(&cx.world.arena, info.id)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec)
                {
                    self.gesture = Some(Gesture::MoveLines {
                        layer: info.id,
                        before,
                        geometry,
                        base: lines,
                        start: local,
                        by: [0.0; 2],
                        offset,
                    });
                }
            }
            Tool::CorrectLine if sub.mode == 7 => {
                // Remove dust: drag a box, or click for the whole layer.
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
            }
            Tool::CorrectLine => {
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
                        if let Some((line, index)) =
                            crate::correct::control_at(&lines, local, reach)
                        {
                            match kind {
                                2 => {
                                    changed =
                                        crate::correct::delete_control(&mut lines[line], index)
                                }
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
            other => cx.notify(tr!("arrives-later-sue-paint", label = other.label())),
        }
    }
}
