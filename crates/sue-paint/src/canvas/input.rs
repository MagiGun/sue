//! Pointer, pen and touch input: the raw event stream read in order,
//! pen pressure and lean, and which gesture each event drives.

use egui::{Event, Key, PointerButton, Pos2, Rect, Response, TouchPhase, Ui};
use sue_wm::AppletContext;

use super::view::{on_screen, shift_rects};
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::Tool;
use sue_platform::stylus::SampleKind;

/// How far a pen leaning `lean` (on screen, see
/// [`sue_platform::stylus::tilt`]) leans, 0 upright to 1 flat (tilt
/// dynamics). Without a source of tilt (a mouse, Windows Ink, WinTab, the
/// browser) it reads as an upright pen, 0, so settings that use tilt stay
/// at their minimum.
pub(super) fn tilt_of(lean: Option<[f32; 2]>) -> f32 {
    lean.map_or(0.0, |[x, y]| x.hypot(y).min(1.0))
}

/// A pen's lean on screen at `at` as a lean in the picture
/// ([`crate::brush::Sample::lean`]): as long, pointing where that way on
/// screen goes in the picture (`to_picture`), so a turned or mirrored view
/// turns and mirrors it too. Upright (or unknown), none.
pub(super) fn lean_in_picture(
    lean: Option<[f32; 2]>,
    at: Pos2,
    to_picture: impl Fn(Pos2) -> Pos2,
) -> [f32; 2] {
    let Some([x, y]) = lean else {
        return [0.0, 0.0];
    };
    let length = x.hypot(y).min(1.0);
    if length <= 1e-6 {
        return [0.0, 0.0];
    }
    // A step of a hundred points that way, in the picture.
    let way = to_picture(at + egui::vec2(x, y) * (100.0 / length)) - to_picture(at);
    let reach = way.length();
    if !reach.is_finite() || reach <= 1e-6 {
        return [0.0, 0.0];
    }
    [way.x / reach * length, way.y / reach * length]
}

/// The angle of a screen vector, in degrees.
pub(super) fn angle(v: egui::Vec2) -> f32 {
    v.y.atan2(v.x).to_degrees()
}

impl SuePaint {
    /// Keeps track of the fingers on the canvas. A second finger takes back
    /// the stroke the first one began (the fingers move the view instead);
    /// a quick tap of two fingers undoes, of three redoes, as in Clip
    /// Studio Paint on tablets.
    #[allow(
        clippy::too_many_arguments,
        reason = "one touch event and where it lands"
    )]
    fn track_touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        pos: Pos2,
        time: f64,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let _ = (viewport, size);
        let view = self
            .docs
            .get(self.active)
            .map_or((1.0, egui::Vec2::ZERO), |doc| (doc.view.zoom, doc.view.pan));
        match phase {
            TouchPhase::Start => {
                if self.touches.is_empty() {
                    self.touch_gesture = Some(crate::studio::TouchTap {
                        began: time,
                        most: 1,
                        travel: 0.0,
                        view,
                    });
                }
                self.touches.retain(|(known, _)| *known != id);
                self.touches.push((id, pos));
                if let Some(tap) = self.touch_gesture.as_mut() {
                    tap.most = tap.most.max(self.touches.len());
                }
                if self.touches.len() >= 2 {
                    // A second finger: a pinch or a pan, not a line. What the
                    // first finger began drawing is taken back as if never
                    // drawn (nothing to undo, so no undo can take anything
                    // else).
                    if matches!(self.gesture, Some(Gesture::Engine(_))) {
                        self.engine_abandon(cx);
                    }
                    if let Some(Gesture::Stroke { stroke, offset, .. }) = self.gesture.take()
                        && let Some(doc) = self.docs.get_mut(self.active)
                        && let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer())
                    {
                        let layer = stroke.layer();
                        let rects = shift_rects(&stroke.abandon(raster), offset);
                        doc.compositor
                            .refresh_rects(&cx.world.arena, &rects, Some(layer));
                    }
                    self.gesture = None;
                }
            }
            TouchPhase::Move => {
                if let Some((_, at)) = self.touches.iter_mut().find(|(known, _)| *known == id) {
                    let moved = at.distance(pos);
                    *at = pos;
                    if let Some(tap) = self.touch_gesture.as_mut() {
                        tap.travel += moved;
                    }
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                self.touches.retain(|(known, _)| *known != id);
                // A tap: quick, the fingers all but still, and the view
                // neither zoomed nor moved (that was a pinch).
                if self.touches.is_empty()
                    && let Some(tap) = self.touch_gesture.take()
                    && phase == TouchPhase::End
                    && time - tap.began < 0.3
                    && tap.travel < 12.0
                    && (tap.view.0 - view.0).abs() <= tap.view.0 * 0.01
                    && (tap.view.1 - view.1).length() < 2.0
                {
                    // Preferences, Touch gestures: what each tap does.
                    let does = |key: &str, default: &str| {
                        cx.rice().get_str(key).unwrap_or(default) != "none"
                    };
                    match tap.most {
                        2 if does("sue_paint.two_finger_tap", "undo") => self.run("edit.undo", cx),
                        3 if does("sue_paint.three_finger_tap", "redo") => {
                            self.run("edit.redo", cx)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// How hard the pen presses, through the pen pressure curve: from the
    /// window's touch events (Windows Ink, macOS, Linux, phones), else from
    /// a WinTab tablet or a pen in a browser. With neither, a brush stroke
    /// under way goes on as hard as its last point (a pen whose driver
    /// said it lifted before the window handed over its last events: never
    /// full pressure for a stroke's end), and anything else is full (a
    /// mouse, whose strokes are full from their first point).
    pub(super) fn pen_pressure(&self) -> f32 {
        self.pressure
            .or_else(|| sue_platform::stylus::pressure().map(|raw| self.pen_curve.eval(raw)))
            .or_else(|| match &self.gesture {
                Some(Gesture::Stroke { stroke, .. }) => {
                    stroke.samples().last().map(|sample| sample.pressure)
                }
                _ => None,
            })
            .unwrap_or(1.0)
    }

    /// Tells the brush stroke under way where the button was let go
    /// (`pos`, on screen): the place its line is finished to, as a move
    /// there would have set it, without painting a point there. Letting go
    /// is the end of the line, not one more point of it.
    fn lift_at(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (
            Some(Gesture::Stroke {
                offset, raw, snap, ..
            }),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let mut local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        if let Some(snap) = snap.as_mut() {
            // Along a ruler, as a move: only once the pen has shown which
            // way it goes.
            match snap.follow(local, 4.0 / doc.view.zoom.max(0.01)) {
                Some(on) => local = on,
                None => return,
            }
        }
        *raw = local;
    }

    /// How far the pen leans at `pos` (0 upright to 1 flat, tilt dynamics)
    /// and which way in the picture (Turn with pen tilt): from the reading
    /// that came with this pointer event, else the pen's latest.
    pub(super) fn pen_lean(&self, pos: Pos2, viewport: Rect, size: [u32; 2]) -> (f32, [f32; 2]) {
        let lean = self.sample_lean.or_else(sue_platform::stylus::tilt);
        let way = self.docs.get(self.active).map_or([0.0, 0.0], |doc| {
            lean_in_picture(lean, pos, |at| doc.view.to_picture(viewport, size, at))
        });
        (tilt_of(lean), way)
    }

    /// Takes the pen's reading for this frame's pointer event of `kind` at
    /// `pos`, when the next one in `samples` is it: its pressure (through
    /// the pen pressure curve) and lean count for that event, not the
    /// frame's last. `true` when there was one.
    fn take_sample(
        &mut self,
        samples: &mut std::collections::VecDeque<sue_platform::stylus::Sample>,
        kind: sue_platform::stylus::SampleKind,
        pos: Pos2,
    ) -> bool {
        if !samples
            .front()
            .is_some_and(|sample| sample.is_for(kind, [pos.x, pos.y]))
        {
            return false;
        }
        let Some(sample) = samples.pop_front() else {
            return false;
        };
        self.pressure = sample.pressure.map(|raw| self.pen_curve.eval(raw));
        if sample.tilt.is_some() {
            self.sample_lean = sample.tilt;
        }
        true
    }

    /// The first time a pen's pressure shows up while its curve is still
    /// the one Sue Paint starts with, says where to fit it to the hand.
    fn pen_seen(&mut self, cx: &mut AppletContext<'_>) {
        if self.pen_hinted {
            return;
        }
        self.pen_hinted = true;
        let default = crate::dialogs::DEFAULT_PEN_PRESSURE
            .parse::<sue_model::Response>()
            .unwrap_or_default();
        if self.pen_curve == default {
            cx.notify(tr!("pen-pressure-found-hint"));
        }
    }

    pub(super) fn canvas_input(
        &mut self,
        ui: &Ui,
        viewport: Rect,
        response: &Response,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let ctx = ui.ctx().clone();
        let blocked = self.dialog.is_some() || self.menubar.is_open() || !self.ready;
        let (events, space, time, dt, held, gone) = ctx.input(|input| {
            (
                input.events.clone(),
                input.key_down(Key::Space),
                input.time,
                input.unstable_dt,
                input.modifiers,
                input.pointer.hover_pos().is_none(),
            )
        });
        // The pen's own reading for each of its pointer events this frame,
        // where the system's pen comes through Sue (a Wayland tablet), so
        // each point of a stroke has the pressure and lean it had there.
        let mut samples = sue_platform::stylus::take_samples();
        // Every packet of the pen, for a stroke of the new brush engine.
        self.engine_take_samples(cx);
        if sue_platform::stylus::pressure().is_some()
            || samples.iter().any(|sample| sample.pressure.is_some())
        {
            self.pen_seen(cx);
        }
        // The pen turned over to its eraser end picks the Eraser, and back
        // the tool before; only between strokes, so a stroke under way
        // finishes with the tool it began with.
        if self.eraser_end.follow(
            sue_platform::stylus::eraser(),
            self.gesture.is_some(),
            &mut self.toolbox,
        ) {
            ctx.request_repaint();
        }
        // Studio Mode's modifier keys count as held.
        let held = held | self.touch.modifiers();
        self.modifiers = held;
        let space = (space || self.touch.space) && !ctx.text_edit_focused();
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
        // Under a menu, popup, dialog or sheet the canvas neither paints
        // nor shows its brush (see sue_wm::cover).
        let layer = ui.layer_id();
        let launchers = crate::launchers::take(&ctx);
        let covered = |pos: Pos2| {
            sue_wm::cover::covered(&ctx, layer, pos)
                || launchers.iter().any(|bar| bar.expand(2.0).contains(pos))
        };
        self.pointer = response.hover_pos().filter(|pos| !covered(*pos));
        let mut lifted = false;
        for event in events {
            match event {
                Event::Touch {
                    id,
                    phase,
                    pos,
                    force,
                    ..
                } => {
                    // A finger landing on something over the canvas is not
                    // the canvas's.
                    if phase == TouchPhase::Start && covered(pos) {
                        continue;
                    }
                    self.track_touch(id.0, phase, pos, time, viewport, size, cx);
                    match phase {
                        TouchPhase::Start | TouchPhase::Move => {
                            if let Some(force) = force {
                                // Through the pen pressure curve every tool shares.
                                self.pressure = Some(self.pen_curve.eval(force.clamp(0.0, 1.0)));
                            }
                        }
                        // The pen's release comes after its lift, in the same
                        // frame, and its last point keeps the pen's last
                        // pressure, not a full one: the pressure goes once
                        // this frame's events are handled.
                        TouchPhase::End | TouchPhase::Cancel => lifted = true,
                    }
                    if force.is_some() {
                        self.pen_seen(cx);
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } => {
                    self.take_sample(&mut samples, SampleKind::Press, pos);
                    // Two fingers down are a pinch or a pan, never a stroke.
                    // A pen tapped quickly can come near, touch and go away
                    // within one frame: nothing is hovered by the frame's
                    // end, yet the touch was on the canvas.
                    if blocked
                        || self.gesture.is_some()
                        || self.touches.len() >= 2
                        || !viewport.contains(pos)
                        || !(response.hovered() || gone)
                        || covered(pos)
                    {
                        continue;
                    }
                    match button {
                        PointerButton::Primary => {
                            self.press(pos, modifiers, space, viewport, size, cx)
                        }
                        PointerButton::Middle => self.gesture = Some(Gesture::Pan { last: pos }),
                        // A right-click picks a color, as in Clip Studio Paint.
                        PointerButton::Secondary => {
                            self.pick(pos, 0, viewport, size, cx);
                            self.gesture = Some(Gesture::Pick { mode: 0 });
                        }
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) => {
                    self.take_sample(&mut samples, SampleKind::Move, pos);
                    self.clock.0 += f64::from(self.clock.1);
                    self.drag(pos, viewport, size, cx);
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } => {
                    // The tip's release keeps the pressure it had for the
                    // stroke's end; it goes once this frame is handled.
                    if self.take_sample(&mut samples, SampleKind::Release, pos)
                        && button == PointerButton::Primary
                    {
                        lifted = true;
                    }
                    if matches!(self.gesture, Some(Gesture::Engine(_))) {
                        // The new engine's stroke ends where the pen last
                        // was; the release is not a point of it.
                        self.engine_release(pos, viewport, size, cx);
                    } else if matches!(self.gesture, Some(Gesture::Stroke { .. })) {
                        // A brush stroke takes no point from its release:
                        // the line is finished to where the button was let
                        // go, at the pressure of its own last point.
                        self.lift_at(pos, viewport, size);
                        self.release(viewport, size, cx);
                    } else if self.gesture.is_some() {
                        // Every other gesture ends where it was let go.
                        self.drag(pos, viewport, size, cx);
                        self.release(viewport, size, cx);
                    }
                }
                _ => {}
            }
        }
        if lifted {
            self.pressure = None;
            self.sample_lean = None;
        }
        // Two fingers or more move, zoom and turn the view.
        let pinch = ctx
            .input(egui::InputState::multi_touch)
            .filter(|touch| !covered(touch.center_pos));
        if let Some(touch) = pinch.filter(|_| !blocked)
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            doc.view.pan += touch.translation_delta;
            if (touch.zoom_delta - 1.0).abs() > 1e-4 {
                doc.view
                    .zoom_at(viewport, size, touch.center_pos, touch.zoom_delta);
            }
            if touch.rotation_delta.abs() > 1e-4 {
                doc.view
                    .rotate_by(viewport, size, touch.rotation_delta.to_degrees());
            }
        }
        if response.hovered() && !blocked && pinch.is_none() {
            let (scroll, zoom, shift) = ctx.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.zoom_delta(),
                    input.modifiers.shift,
                )
            });
            if let (Some(pointer), Some(doc)) = (
                response.hover_pos().filter(|pos| !covered(*pos)),
                self.docs.get_mut(self.active),
            ) {
                if shift && (scroll.x != 0.0 || scroll.y != 0.0) {
                    // Shift and the wheel turn the view.
                    doc.view
                        .rotate_by(viewport, size, (scroll.x + scroll.y) * 0.15);
                } else if (zoom - 1.0).abs() > 1e-4 {
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
        if let Some(doc) = self.docs.get(self.active) {
            self.on_screen = on_screen(&doc.view, viewport, size, ctx.pixels_per_point());
        }
        self.smart_shape(&ctx, viewport, size, cx);
        let (visible, step) = self.on_screen;
        // Show what the stroke painted this frame: only the tiles it
        // changed, over the kept stacks around its layer, at the detail
        // the screen shows.
        if let Some(Gesture::Stroke { stroke, offset, .. }) = &mut self.gesture
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            let rects = shift_rects(&stroke.take_dirty_tiles(), *offset);
            if rects.is_empty() {
                // The layer was written to without a pixel changing: that
                // is no reason to recomposite the picture.
                doc.compositor.accept(&cx.world.arena, stroke.layer());
            } else {
                doc.compositor.refresh_view(
                    &cx.world.arena,
                    &rects,
                    Some(stroke.layer()),
                    visible,
                    step,
                );
            }
        }
        // The new engine's stroke: this frame's samples to its worker, and
        // what it laid since the last frame.
        self.engine_frame(viewport, size, cx);
        if self.gesture.is_some() {
            ctx.request_repaint();
        } else if let Some(doc) = self.docs.get_mut(self.active) {
            // Between strokes, what was shown coarse or was off screen
            // comes to full detail, a little each frame.
            let tiles = 32 * crate::composite::workers();
            if doc.compositor.refine(&cx.world.arena, visible, tiles) {
                ctx.request_repaint();
            }
        }
    }
}
