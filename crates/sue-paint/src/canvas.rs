//! The canvas: tabs of open pictures, the turned and zoomed view, every
//! tool's gesture under the pointer or pen, the overlays (selection,
//! shape previews, brush outline, grids) and the status strip below.
//!
//! Pointer input is read as egui's raw event stream, in order, so a fast
//! stroke keeps every sample between frames and a pen's pressure arrives
//! with it (from touch events that carry force).

use egui::epaint::Vertex;
use egui::{
    Align2, Color32, Event, Id, Key, Mesh, Modifiers, PointerButton, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, TouchPhase, Ui, pos2, vec2,
};
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use crate::brush::{Finish, Stabilizer, Stroke as BrushStroke};
use crate::color;
use crate::composite::{self, RenderOptions};
use crate::doc;
use crate::fill;
use crate::icons::Icon;
use crate::panels::{child, icon_button};
use crate::select::{Op, Selection};
use crate::studio::{Gesture, SuePaint, texture_key};
use crate::tools::{SubTool, Tool};

/// The angle of a screen vector, in degrees.
fn angle(v: egui::Vec2) -> f32 {
    v.y.atan2(v.x).to_degrees()
}

/// Points around an ellipse inside two corners.
fn ellipse_points(from: Pos2, to: Pos2) -> Vec<[f32; 2]> {
    let center = from + (to - from) / 2.0;
    let (rx, ry) = (((to.x - from.x) / 2.0).abs(), ((to.y - from.y) / 2.0).abs());
    (0..=96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            [center.x + rx * t.cos(), center.y + ry * t.sin()]
        })
        .collect()
}

impl SuePaint {
    /// The canvas column: tabs, view and status strip.
    pub(crate) fn canvas_area(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        // While the studio comes in or goes out, the tabs and the status
        // strip slide off with the panels and the view takes their room.
        let slide = self.hide * 26.0;
        let tabs = Rect::from_min_size(rect.min - vec2(0.0, slide), vec2(rect.width(), 26.0));
        self.tabs(ui, tabs, cx, theme);
        let status = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - 26.0 + slide),
            pos2(rect.right(), rect.bottom() + slide),
        );
        let viewport = Rect::from_min_max(
            pos2(rect.left(), tabs.bottom()),
            pos2(rect.right(), status.top()),
        );
        self.viewport = viewport;
        ui.painter()
            .rect_filled(viewport, 0.0, theme.void.gamma_multiply(self.cover));
        let size = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(picture::size);
        let Some(size) = size else {
            self.empty_canvas(ui, viewport, cx, theme);
            self.status_strip(ui, status, None, cx, theme);
            return;
        };
        let (hide, ready) = (self.hide, self.ready);
        let exit_view = self.exit_view.filter(|_| self.leaving());
        if let Some(doc) = self.doc_mut() {
            if !ready && hide > 0.0 {
                // Coming in, the picture shrinks from filling the window to
                // its place; going out, it grows from the view it had back
                // to filling the window, where its card waits.
                let mut fitted = doc.view;
                fitted.fit(viewport, size);
                doc.view = match exit_view {
                    Some(from) => from.blend(&fitted, hide),
                    None => fitted,
                };
            } else if !doc.view.fitted {
                doc.view.fit(viewport, size);
            }
        }
        let response = ui.interact(
            viewport,
            Id::unique("sue-paint-canvas"),
            Sense::click_and_drag(),
        );
        self.canvas_input(ui, viewport, &response, size, cx);
        self.draw_picture(ui, viewport, size, cx, theme);
        self.overlays(ui, viewport, size, &response, cx, theme);
        self.status_strip(ui, status, Some(size), cx, theme);
    }

    fn tabs(&mut self, ui: &mut Ui, strip: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.bottom(), theme.hairline());
        let font = theme.font(theme.small_size + 0.5);
        let mut x = strip.left() + 2.0;
        let mut pick = None;
        let mut close = None;
        for (index, doc) in self.docs.iter().enumerate() {
            let name = cx
                .world
                .arena
                .get_by_id(doc.picture)
                .map_or_else(|_| "Picture".to_owned(), |entity| entity.name().to_owned());
            let galley = ui.painter().layout_no_wrap(name, font.clone(), theme.text);
            let tab = Rect::from_min_size(
                pos2(x, strip.top() + 2.0),
                vec2(galley.size().x + 38.0, 24.0),
            );
            x = tab.right() + 2.0;
            let chosen = index == self.active;
            let response = ui.interact(
                tab,
                Id::unique(("sue-paint-doc-tab", doc.picture.to_u128())),
                Sense::click(),
            );
            ui.painter().rect_filled(
                tab,
                0.0,
                if chosen {
                    theme.panel
                } else if response.hovered() {
                    theme.hover
                } else {
                    theme.pill
                },
            );
            ui.painter().galley(
                pos2(tab.left() + 10.0, tab.center().y - galley.size().y / 2.0),
                galley,
                if chosen { theme.text } else { theme.muted },
            );
            let cross =
                Rect::from_center_size(pos2(tab.right() - 13.0, tab.center().y), vec2(16.0, 16.0));
            let cross_response = ui.interact(
                cross,
                Id::unique(("sue-paint-doc-close", doc.picture.to_u128())),
                Sense::click(),
            );
            let cross_color = if cross_response.hovered() {
                theme.text
            } else {
                theme.muted
            };
            ui.painter().line_segment(
                [
                    cross.left_top() + vec2(4.0, 4.0),
                    cross.right_bottom() - vec2(4.0, 4.0),
                ],
                Stroke::new(1.2, cross_color),
            );
            ui.painter().line_segment(
                [
                    cross.right_top() + vec2(-4.0, 4.0),
                    cross.left_bottom() + vec2(4.0, -4.0),
                ],
                Stroke::new(1.2, cross_color),
            );
            if cross_response
                .on_hover_text("Close (the picture stays in your project)")
                .clicked()
            {
                close = Some(index);
            } else if response.clicked() {
                pick = Some(index);
            }
        }
        let plus = Rect::from_min_size(pos2(x + 2.0, strip.top() + 3.0), vec2(22.0, 22.0));
        if icon_button(ui, plus, Icon::NewLayer, true, false, "New (Ctrl+N)", theme) {
            self.run("file.new", cx);
        }
        if let Some(index) = pick {
            self.active = index;
        }
        if let Some(index) = close {
            self.active = index;
            self.run("file.close", cx);
        }
    }

    fn empty_canvas(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let card = Rect::from_center_size(viewport.center(), vec2(320.0, 150.0));
        let mut inner = child(ui, card, "empty-canvas");
        inner.multiply_opacity(self.cover);
        inner.vertical_centered(|ui| {
            ui.label(
                RichText::new("No canvas is open")
                    .size(theme.heading_size)
                    .color(theme.text),
            );
            ui.label(
                RichText::new("Pictures you make here are saved in your Sue project")
                    .color(theme.muted),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(70.0);
                if ui.button("New...").clicked() {
                    self.run("file.new", cx);
                }
                if ui.button("Open...").clicked() {
                    self.run("file.open", cx);
                }
            });
        });
    }

    // ----- Input ------------------------------------------------------------------

    fn canvas_input(
        &mut self,
        ui: &Ui,
        viewport: Rect,
        response: &Response,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let ctx = ui.ctx().clone();
        let blocked = self.dialog.is_some() || self.menubar.is_open() || !self.ready;
        let (events, space, time, dt) = ctx.input(|input| {
            (
                input.events.clone(),
                input.key_down(Key::Space),
                input.time,
                input.unstable_dt,
            )
        });
        let space = space && !ctx.text_edit_focused();
        // Spread this frame's pointer samples evenly over the frame's time,
        // so the stabilizer and flicks see the pen's real speed.
        let moves = events
            .iter()
            .filter(|event| matches!(event, Event::PointerMoved(_)))
            .count()
            .max(1);
        let frame = dt.clamp(0.001, 0.1);
        let step = frame / moves as f32;
        self.clock = (time - f64::from(frame), step);
        self.pointer = response.hover_pos();
        for event in events {
            match event {
                Event::Touch { phase, force, .. } => match phase {
                    TouchPhase::Start | TouchPhase::Move => {
                        if let Some(force) = force {
                            self.pressure = Some(force.clamp(0.0, 1.0));
                        }
                    }
                    TouchPhase::End | TouchPhase::Cancel => self.pressure = None,
                },
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } => {
                    if blocked
                        || self.gesture.is_some()
                        || !viewport.contains(pos)
                        || !response.hovered()
                    {
                        continue;
                    }
                    match button {
                        PointerButton::Primary => {
                            self.press(pos, modifiers, space, viewport, size, cx)
                        }
                        PointerButton::Middle => self.gesture = Some(Gesture::Pan { last: pos }),
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) => {
                    self.clock.0 += f64::from(self.clock.1);
                    self.drag(pos, viewport, size, cx);
                }
                Event::PointerButton {
                    pos,
                    pressed: false,
                    ..
                } if self.gesture.is_some() => {
                    self.drag(pos, viewport, size, cx);
                    self.release(viewport, size, cx);
                }
                _ => {}
            }
        }
        if response.hovered() && !blocked {
            let (scroll, zoom) = ctx.input(|input| (input.smooth_scroll_delta, input.zoom_delta()));
            if let (Some(pointer), Some(doc)) =
                (response.hover_pos(), self.docs.get_mut(self.active))
            {
                if (zoom - 1.0).abs() > 1e-4 {
                    doc.view.zoom_at(viewport, size, pointer, zoom);
                } else if scroll.y != 0.0 {
                    doc.view
                        .zoom_at(viewport, size, pointer, (scroll.y * 0.0025).exp());
                }
                if scroll.x != 0.0 {
                    doc.view.pan.x += scroll.x;
                }
            }
            let tool = if space {
                Tool::Hand
            } else {
                self.toolbox.tool()
            };
            let icon = match (tool, &self.gesture) {
                (_, Some(Gesture::Pan { .. })) => egui::CursorIcon::Grabbing,
                (Tool::Hand, _) => egui::CursorIcon::Grab,
                (Tool::Zoom, _) => egui::CursorIcon::ZoomIn,
                (Tool::MoveLayer, _) => egui::CursorIcon::Move,
                (tool, _) if tool.paints() => egui::CursorIcon::None,
                _ => egui::CursorIcon::Crosshair,
            };
            ctx.set_cursor_icon(icon);
        }
        // Show what the stroke painted this frame.
        if let Some(Gesture::Stroke { stroke, offset, .. }) = &mut self.gesture
            && let Some([x0, y0, x1, y1]) = stroke.take_dirty()
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
            let rect = [
                shift(x0, offset[0]),
                shift(y0, offset[1]),
                shift(x1, offset[0]),
                shift(y1, offset[1]),
            ];
            doc.compositor.refresh(&cx.world.arena, rect);
        }
        if self.gesture.is_some() {
            ctx.request_repaint();
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per tool")]
    fn press(
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
        if space {
            tool = Tool::Hand;
        } else if modifiers.alt && tool.paints() && tool != Tool::Eraser && tool != Tool::Figure {
            tool = Tool::Eyedropper;
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
            Tool::Pen | Tool::Pencil | Tool::Brush | Tool::Airbrush | Tool::Eraser => {
                let info = match self.paintable(&cx.world.arena) {
                    Ok(info) => info,
                    Err(error) => {
                        cx.fail(error);
                        return;
                    }
                };
                let offset = info.style.offset;
                let rgb = color::unit(self.drawing_color());
                let mut stroke =
                    BrushStroke::new(info.id, sub.tip, rgb, info.alpha_locked && !sub.tip.eraser);
                let shifted = doc
                    .selection
                    .as_ref()
                    .filter(|_| offset != [0, 0])
                    .map(|selection| selection.shifted(offset));
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                let mut stabilizer = Stabilizer::new(sub.stabilization, sub.adjust_by_speed);
                let start = stabilizer.sample(local, doc.view.zoom, self.clock.1);
                let pressure = self.pressure.unwrap_or(1.0);
                let selection = shifted.as_ref().or(doc.selection.as_ref());
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id) {
                    stroke.to(raster, start, pressure, selection);
                }
                self.gesture = Some(Gesture::Stroke {
                    stroke,
                    stabilizer,
                    offset,
                    raw: local,
                    label: sub.name.clone(),
                    shifted,
                    trail: vec![(self.clock.0, local)],
                    correction: sub.correction,
                    flick: sub.flick,
                });
            }
            Tool::Figure | Tool::Gradient => {
                if let Err(error) = self.paintable(&cx.world.arena) {
                    cx.fail(error);
                    return;
                }
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub,
                    op,
                    from: p,
                    to: p,
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
                        sub,
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
                    cx.fail(format!("{} cannot be moved", info.name));
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
            other => cx.notify(format!("{} arrives in a later Sue Paint", other.label())),
        }
    }

    fn drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2], cx: &mut AppletContext<'_>) {
        if let Some(Gesture::Pick { mode }) = self.gesture {
            self.pick(pos, mode, viewport, size, cx);
            return;
        }
        let pressure = self.pressure.unwrap_or(1.0);
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
                ..
            } => {
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                *raw = local;
                trail.push((time, local));
                if trail.len() > 64 {
                    trail.drain(..trail.len() - 64);
                }
                let smooth = stabilizer.sample(local, doc.view.zoom, step);
                let selection = shifted.as_ref().or(doc.selection.as_ref());
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer()) {
                    stroke.to(raster, smooth, pressure, selection);
                }
            }
            Gesture::Pan { last } => {
                doc.view.pan += pos - *last;
                *last = pos;
            }
            Gesture::Rotate { start, rotation } => {
                let now = angle(pos - viewport.center());
                doc.view
                    .set_rotation(viewport, size, *rotation + (now - *start));
            }
            Gesture::Shape { to, .. } => *to = p,
            Gesture::Lasso { points, .. } => {
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
                if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                    entity.properties_mut().set(picture::X, origin[0] + dx);
                    entity.properties_mut().set(picture::Y, origin[1] + dy);
                }
            }
            Gesture::Pick { .. } => {}
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per gesture")]
    fn release(&mut self, viewport: Rect, size: [u32; 2], cx: &mut AppletContext<'_>) {
        let _ = viewport;
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let pressure = self.pressure.unwrap_or(1.0);
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
            } => {
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                let layer = stroke.layer();
                let selection = shifted.as_ref().or(doc.selection.as_ref());
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
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
                    for point in stabilizer.catch_up(raw) {
                        stroke.to(raster, point, last, selection);
                    }
                    stroke.end(raster, finish, selection);
                }
                if let Some([x0, y0, x1, y1]) = stroke.take_dirty() {
                    let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
                    doc.compositor.refresh(
                        &cx.world.arena,
                        [
                            shift(x0, offset[0]),
                            shift(y0, offset[1]),
                            shift(x1, offset[0]),
                            shift(y1, offset[1]),
                        ],
                    );
                }
                let eraser = stroke.tip().eraser;
                if let Some(raster) = doc::raster(&cx.world.arena, layer) {
                    let edit = stroke.finish(raster, &label);
                    if !edit.is_empty() {
                        cx.record(ArenaEdit(edit));
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
                Tool::Figure => self.figure(from, to, &sub, size, cx),
                Tool::Gradient => self.gradient(from, to, &sub, cx),
                _ => {}
            },
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
                    let mut steps = Batch::new("Move Layer");
                    for (key, before) in [(picture::X, &old[0]), (picture::Y, &old[1])] {
                        steps = steps.then(match before {
                            Some(value) => SetProperty::new(layer, key, value.clone()),
                            None => SetProperty::remove(layer, key),
                        });
                    }
                    cx.record(ArenaEdit(steps));
                }
            }
            Gesture::Pan { .. } | Gesture::Rotate { .. } | Gesture::Pick { .. } => {}
        }
    }

    /// Picks the color under `pos`: as shown, or from the current layer.
    fn pick(
        &mut self,
        pos: Pos2,
        mode: u8,
        viewport: Rect,
        size: [u32; 2],
        cx: &AppletContext<'_>,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        if p.x < 0.0 || p.y < 0.0 || p.x >= size[0] as f32 || p.y >= size[1] as f32 {
            return;
        }
        let (x, y) = (p.x as u32, p.y as u32);
        let rgb = if mode == 0 {
            doc.compositor.pixel(x, y).map(|[r, g, b, _]| [r, g, b])
        } else {
            self.layer_info(&cx.world.arena).and_then(|info| {
                let raster = doc::raster(&cx.world.arena, info.id)?;
                let (lx, ly) = (
                    i64::from(x) - info.style.offset[0],
                    i64::from(y) - info.style.offset[1],
                );
                if lx < 0
                    || ly < 0
                    || lx >= i64::from(raster.width())
                    || ly >= i64::from(raster.height())
                {
                    return None;
                }
                let at = ((ly as u32 * raster.width() + lx as u32) * 4) as usize;
                let pixel = raster.bytes()?.get(at..at + 4)?;
                (pixel[3] > 0).then(|| [pixel[0], pixel[1], pixel[2]])
            })
        };
        if let Some(rgb) = rgb {
            self.set_drawing_color(rgb);
        }
    }

    /// The area a flood fill from `p` reaches, as a picture-sized mask.
    fn flood_mask(
        &self,
        p: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        arena: &sue_core::Arena,
    ) -> Option<Vec<u8>> {
        if p.x < 0.0 || p.y < 0.0 || p.x >= size[0] as f32 || p.y >= size[1] as f32 {
            return None;
        }
        let picture_id = self.picture()?;
        let reference = if sub.mode == 1 {
            composite::render(
                arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
            )
        } else {
            let info = self.layer_info(arena)?;
            let raster = doc::raster(arena, info.id)?;
            let bytes = raster.bytes()?;
            if info.style.offset == [0, 0] && [raster.width(), raster.height()] == size {
                bytes.to_vec()
            } else {
                let mut placed = vec![0_u8; (size[0] * size[1] * 4) as usize];
                for y in 0..raster.height() {
                    for x in 0..raster.width() {
                        let (px, py) = (
                            i64::from(x) + info.style.offset[0],
                            i64::from(y) + info.style.offset[1],
                        );
                        if px < 0 || py < 0 || px >= i64::from(size[0]) || py >= i64::from(size[1])
                        {
                            continue;
                        }
                        let from = ((y * raster.width() + x) * 4) as usize;
                        let to = ((py as u32 * size[0] + px as u32) * 4) as usize;
                        placed[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                    }
                }
                placed
            }
        };
        let mut mask = fill::flood(&reference, size, [p.x as u32, p.y as u32], sub.tolerance);
        if let Some(selection) = self.doc().and_then(|doc| doc.selection.as_ref()) {
            for (index, value) in mask.iter_mut().enumerate() {
                let (x, y) = (index as u32 % size[0], index as u32 / size[0]);
                *value = (f32::from(*value) * selection.at(x, y)).round() as u8;
            }
        }
        Some(mask)
    }

    /// The paint bucket.
    fn bucket(&mut self, p: Pos2, sub: &SubTool, size: [u32; 2], cx: &mut AppletContext<'_>) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(mask) = self.flood_mask(p, sub, size, &cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
        let (offset, alpha_lock, opacity) = (info.style.offset, info.alpha_locked, sub.tip.opacity);
        match doc::rewrite(&cx.world.arena, info.id, "Fill", |pixels, width, height| {
            fill::paint_mask(
                pixels,
                [width, height],
                offset,
                &mask,
                size,
                rgb,
                opacity,
                alpha_lock,
            );
        }) {
            Ok(Some(edit)) => {
                if Self::apply(cx, edit) {
                    self.recent.push(self.drawing_color());
                }
            }
            Ok(None) => {}
            Err(error) => cx.fail(error),
        }
    }

    /// The auto select tool: a flood fill that makes a selection.
    fn auto_select(
        &mut self,
        p: Pos2,
        sub: &SubTool,
        op: Op,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(mask) = self.flood_mask(p, sub, size, &cx.world.arena) else {
            return;
        };
        if let Some(doc) = self.doc_mut() {
            let mut selection = doc
                .selection
                .clone()
                .unwrap_or_else(|| Selection::new(size[0], size[1]));
            selection.apply_mask(&mask, op);
            doc.set_selection(Some(selection));
        }
    }

    /// The figure tool: lines and outlines with the brush, filled shapes as
    /// fills.
    fn figure(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
        let (min, max) = (from.min(to), from.max(to));
        if sub.mode >= 3 {
            let mut shape = Selection::new(size[0], size[1]);
            if sub.mode == 3 {
                let clamp = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
                shape.rect(
                    [
                        clamp(min.x, size[0]),
                        clamp(min.y, size[1]),
                        clamp(max.x, size[0]),
                        clamp(max.y, size[1]),
                    ],
                    Op::Replace,
                );
            } else {
                shape.ellipse([min.x, min.y, max.x, max.y], Op::Replace);
            }
            let selection = self.doc().and_then(|doc| doc.selection.clone());
            let mask: Vec<u8> = (0..size[0] * size[1])
                .map(|index| {
                    let (x, y) = (index % size[0], index / size[0]);
                    let inside = shape.at(x, y)
                        * selection
                            .as_ref()
                            .map_or(1.0, |selection| selection.at(x, y));
                    (inside * 255.0).round() as u8
                })
                .collect();
            let (offset, alpha_lock, opacity) =
                (info.style.offset, info.alpha_locked, sub.tip.opacity);
            if let Ok(Some(edit)) = doc::rewrite(
                &cx.world.arena,
                info.id,
                &sub.name,
                |pixels, width, height| {
                    fill::paint_mask(
                        pixels,
                        [width, height],
                        offset,
                        &mask,
                        size,
                        rgb,
                        opacity,
                        alpha_lock,
                    );
                },
            ) {
                Self::apply(cx, edit);
            }
            return;
        }
        let points: Vec<[f32; 2]> = match sub.mode {
            0 => vec![[from.x, from.y], [to.x, to.y]],
            1 => vec![
                [min.x, min.y],
                [max.x, min.y],
                [max.x, max.y],
                [min.x, max.y],
                [min.x, min.y],
            ],
            _ => ellipse_points(from, to),
        };
        let offset = info.style.offset;
        let shifted = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map(|selection| {
                if offset == [0, 0] {
                    selection.clone()
                } else {
                    selection.shifted(offset)
                }
            });
        let plain = crate::brush::Tip {
            taper_start: 0.0,
            taper_end: 0.0,
            mix: 0.0,
            ..sub.tip
        };
        let mut stroke = BrushStroke::new(info.id, plain, rgb, info.alpha_locked);
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id) {
            for point in points {
                stroke.to(
                    raster,
                    [point[0] - offset[0] as f32, point[1] - offset[1] as f32],
                    1.0,
                    shifted.as_ref(),
                );
            }
        }
        if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
            let edit = stroke.finish(raster, &sub.name);
            if !edit.is_empty() {
                cx.record(ArenaEdit(edit));
            }
        }
    }

    /// The gradient tool.
    fn gradient(&mut self, from: Pos2, to: Pos2, sub: &SubTool, cx: &mut AppletContext<'_>) {
        if (to - from).length() < 1.0 {
            return;
        }
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let [r, g, b] = color::unit(self.main);
        let opacity = sub.tip.opacity;
        let start = [r, g, b, opacity];
        let end = if sub.mode == 1 {
            let [r, g, b] = color::unit(self.sub);
            [r, g, b, opacity]
        } else {
            [r, g, b, 0.0]
        };
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let (offset, alpha_lock) = (info.style.offset, info.alpha_locked);
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            info.id,
            "Gradient",
            |pixels, width, height| {
                fill::gradient(
                    pixels,
                    [width, height],
                    offset,
                    [[from.x, from.y], [to.x, to.y]],
                    [start, end],
                    sub.mode == 2,
                    selection.as_ref(),
                    alpha_lock,
                );
            },
        ) {
            Self::apply(cx, edit);
        }
    }

    // ----- Drawing ----------------------------------------------------------------------

    fn draw_picture(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(viewport);
        let corners = doc.view.corners(viewport, size);
        let (pixels, display_size, version) = doc.compositor.display();
        let texture = (display_size == size)
            .then(|| {
                cx.textures.texture(
                    texture_key(doc.picture, 0),
                    version,
                    size,
                    4,
                    pixels,
                    doc.view.zoom >= 2.0,
                )
            })
            .flatten();
        match texture {
            Some(texture) => {
                let mut mesh = Mesh::with_texture(texture);
                let uvs = [
                    pos2(0.0, 0.0),
                    pos2(1.0, 0.0),
                    pos2(1.0, 1.0),
                    pos2(0.0, 1.0),
                ];
                let tint = Color32::WHITE.gamma_multiply(self.cover);
                for (corner, uv) in corners.iter().zip(uvs) {
                    mesh.vertices.push(Vertex {
                        pos: *corner,
                        uv,
                        color: tint,
                    });
                }
                mesh.indices = vec![0, 1, 2, 0, 2, 3];
                painter.add(Shape::mesh(mesh));
            }
            None => {
                painter.add(Shape::convex_polygon(
                    corners.to_vec(),
                    Color32::from_gray(235),
                    Stroke::NONE,
                ));
            }
        }
        painter.add(Shape::closed_line(
            corners.to_vec(),
            Stroke::new(1.0, theme.border.gamma_multiply(self.cover)),
        ));
    }

    #[allow(clippy::too_many_lines, reason = "every overlay of the canvas")]
    fn overlays(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        response: &Response,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.ready {
            return;
        }
        let painter = ui.painter().with_clip_rect(viewport);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let view = doc.view;
        let screen = |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0], p[1]));
        // Grids.
        let visible = {
            let points = [
                viewport.left_top(),
                viewport.right_top(),
                viewport.right_bottom(),
                viewport.left_bottom(),
            ]
            .map(|corner| view.to_picture(viewport, size, corner));
            let min_x = points
                .iter()
                .map(|p| p.x)
                .fold(f32::INFINITY, f32::min)
                .max(0.0);
            let max_x = points
                .iter()
                .map(|p| p.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(size[0] as f32);
            let min_y = points
                .iter()
                .map(|p| p.y)
                .fold(f32::INFINITY, f32::min)
                .max(0.0);
            let max_y = points
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(size[1] as f32);
            [min_x, min_y, max_x, max_y]
        };
        let lines = |step: f32, color: Color32| {
            let count = ((visible[2] - visible[0]) / step) + ((visible[3] - visible[1]) / step);
            if count > 600.0 || step <= 0.0 {
                return;
            }
            let mut x = (visible[0] / step).floor() * step;
            while x <= visible[2] {
                painter.line_segment(
                    [screen([x, visible[1]]), screen([x, visible[3]])],
                    Stroke::new(1.0, color),
                );
                x += step;
            }
            let mut y = (visible[1] / step).floor() * step;
            while y <= visible[3] {
                painter.line_segment(
                    [screen([visible[0], y]), screen([visible[2], y])],
                    Stroke::new(1.0, color),
                );
                y += step;
            }
        };
        if self.pixel_grid && view.zoom >= 12.0 {
            lines(1.0, Color32::from_black_alpha(40));
        }
        if self.grid {
            lines(64.0, Color32::from_rgba_unmultiplied(0, 120, 255, 70));
        }
        // The selection.
        if self.show_selection
            && let Some(selection) = &doc.selection
        {
            let edges = doc.edges.get_or_insert_with(|| selection.edges());
            if edges.len() < 60_000 {
                for [a, b] in edges.iter() {
                    let (a, b) = (screen(*a), screen(*b));
                    painter.line_segment([a, b], Stroke::new(2.0, Color32::BLACK));
                    painter.line_segment([a, b], Stroke::new(1.0, Color32::WHITE));
                }
            }
        }
        // What a drag is drawing.
        match &self.gesture {
            Some(Gesture::Shape {
                tool,
                sub,
                from,
                to,
                ..
            }) => {
                let outline = Stroke::new(1.0, theme.border_active);
                let (min, max) = (from.min(*to), from.max(*to));
                match tool {
                    Tool::Gradient => {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                        painter.circle_stroke(screen([from.x, from.y]), 4.0, outline);
                        painter.circle_stroke(screen([to.x, to.y]), 4.0, outline);
                    }
                    Tool::Figure if sub.mode == 0 => {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                    }
                    _ if (tool == &Tool::Select && sub.mode == 1)
                        || (tool == &Tool::Figure && matches!(sub.mode, 2 | 4)) =>
                    {
                        let points = ellipse_points(*from, *to).into_iter().map(screen).collect();
                        painter.add(Shape::closed_line(points, outline));
                    }
                    _ => {
                        let points = [
                            [min.x, min.y],
                            [max.x, min.y],
                            [max.x, max.y],
                            [min.x, max.y],
                        ]
                        .map(screen);
                        painter.add(Shape::closed_line(points.to_vec(), outline));
                    }
                }
            }
            Some(Gesture::Lasso { points, .. }) => {
                let path = points.iter().copied().map(screen).collect::<Vec<_>>();
                painter.add(Shape::line(path, Stroke::new(1.0, theme.border_active)));
            }
            _ => {}
        }
        // The brush outline under the pointer.
        let tool = self.toolbox.tool();
        if response.hovered()
            && tool.paints()
            && let Some(pointer) = self.pointer
        {
            let radius = (self.toolbox.current().tip.size * 0.5 * view.zoom).max(1.5);
            painter.circle_stroke(
                pointer,
                radius,
                Stroke::new(2.0, Color32::from_black_alpha(140)),
            );
            painter.circle_stroke(
                pointer,
                radius,
                Stroke::new(1.0, Color32::from_white_alpha(220)),
            );
            painter.circle_filled(pointer, 1.0, Color32::WHITE);
        }
        let _ = cx;
    }

    fn status_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        size: Option<[u32; 2]>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.top(), theme.hairline());
        let Some(size) = size else {
            return;
        };
        let viewport = self.viewport;
        let mut inner = child(ui, strip.shrink2(vec2(8.0, 2.0)), "status-strip");
        let pointer = self.pointer;
        let layer_name = self
            .layer_info(&cx.world.arena)
            .map(|info| info.name)
            .unwrap_or_default();
        let dpi = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(|entity| {
                entity
                    .property(picture::DPI)
                    .and_then(sue_core::Value::as_f64)
            })
            .unwrap_or(72.0);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        inner.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut percent = doc.view.zoom * 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut percent)
                        .range(1.0..=6400.0)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .changed()
            {
                doc.view
                    .set_zoom_at(viewport, size, viewport.center(), percent / 100.0);
            }
            if ui.small_button("-").on_hover_text("Zoom out").clicked() {
                doc.view.zoom_at(viewport, size, viewport.center(), 0.8);
            }
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                doc.view.zoom_at(viewport, size, viewport.center(), 1.25);
            }
            ui.separator();
            let mut rotation = doc.view.rotation;
            if ui
                .add(
                    egui::DragValue::new(&mut rotation)
                        .range(-180.0..=180.0)
                        .max_decimals(1)
                        .suffix(" \u{b0}"),
                )
                .changed()
            {
                doc.view.set_rotation(viewport, size, rotation);
            }
            if ui
                .small_button("\u{21ba}")
                .on_hover_text("Rotate left")
                .clicked()
            {
                doc.view.rotate_by(viewport, size, -15.0);
            }
            if ui
                .small_button("\u{21bb}")
                .on_hover_text("Rotate right")
                .clicked()
            {
                doc.view.rotate_by(viewport, size, 15.0);
            }
            if ui
                .selectable_label(doc.view.flip_h, "Flip")
                .on_hover_text("Flip the view")
                .clicked()
            {
                doc.view.flip(viewport, size, true);
            }
            ui.separator();
            let small = |text: String| {
                RichText::new(text)
                    .size(theme.small_size + 0.5)
                    .color(theme.muted)
            };
            ui.label(small(format!("{} x {} px, {dpi:.0} dpi", size[0], size[1])));
            if let Some(pointer) = pointer {
                let p = doc.view.to_picture(viewport, size, pointer);
                if p.x >= 0.0 && p.y >= 0.0 && p.x < size[0] as f32 && p.y < size[1] as f32 {
                    ui.separator();
                    ui.label(small(format!("{:.0}, {:.0}", p.x.floor(), p.y.floor())));
                }
            }
            if !layer_name.is_empty() {
                ui.separator();
                ui.label(small(layer_name.clone()));
            }
        });
        let _ = Align2::LEFT_CENTER;
    }
}
