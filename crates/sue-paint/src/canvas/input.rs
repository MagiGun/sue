//! Pointer, pen and touch input: the raw event stream read in order, and
//! which gesture each event drives. A brush stroke takes the pen's own
//! packets from `sue_platform::stylus::take_pen_samples` (see
//! `canvas/engine.rs`); nothing here reads a global "latest pressure".

use egui::{Event, Key, PointerButton, Pos2, Rect, Response, TouchPhase, Ui};
use sue_wm::AppletContext;

use super::view::on_screen;
use crate::modifier_keys::{Input, Keys};
use crate::studio::{Gesture, SuePaint};
use crate::tools::Tool;

/// A pen's lean on screen at `at` as a lean in the picture
/// (a sample's tilt): as long, pointing where that way on
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

/// A pen's twist on screen at `at` (degrees clockwise) as a twist in the
/// picture: the way it points on screen, followed into the picture
/// (`to_picture`), so a turned view turns it and a mirrored view mirrors
/// it, as [`lean_in_picture`] does the lean. 0 to 360.
pub(super) fn twist_in_picture(twist: f32, at: Pos2, to_picture: impl Fn(Pos2) -> Pos2) -> f32 {
    let (sin, cos) = twist.to_radians().sin_cos();
    let way = to_picture(at + egui::vec2(cos, sin) * 100.0) - to_picture(at);
    if !way.x.is_finite() || !way.y.is_finite() || way.length() <= 1e-6 {
        return twist.rem_euclid(360.0);
    }
    way.y.atan2(way.x).to_degrees().rem_euclid(360.0)
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
                    // Tone dots or the grid held: back where they were.
                    self.nudge_abandon(cx);
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

    /// How hard the pen presses now, through the pen pressure curve: its
    /// latest packet in the stream, else the window's touch (its force),
    /// else evenly (a mouse). For tools that read the pen as it moves
    /// (Correct Line's redraw), not for brush strokes, which take every
    /// packet as it came.
    pub(super) fn pressing_now(&self) -> f32 {
        self.pen_stream
            .iter()
            .rev()
            .filter(|sample| sample.source.is_pen())
            .find_map(|sample| sample.pressure)
            .map(|raw| self.pen_curve.eval(raw.clamp(0.0, 1.0)))
            .or(self.pressure)
            .unwrap_or(1.0)
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
        let blocked = self.dialog.is_some()
            || self.files.dialog_open()
            || self.menubar.is_open()
            || !self.ready;
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
        // Every packet of the pen, for a stroke of the brush engine.
        self.engine_take_samples();
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
                    self.touch_seen(id.0, phase, force);
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
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } => {
                    // A stroke still settling lands first, so the press
                    // finds the picture as that stroke leaves it.
                    if self.engine_ending() {
                        self.engine_settle(cx);
                    }
                    // Change Canvas Size open: its box's handles take
                    // the press, though the dialog holds the rest.
                    if button == PointerButton::Primary
                        && self.gesture.is_none()
                        && viewport.contains(pos)
                        && !covered(pos)
                        && self.canvas_size_press(pos, viewport, size)
                    {
                        continue;
                    }
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
                        // Draw with Fingers off: a finger moves the view.
                        PointerButton::Primary if self.finger_moves_view(cx) => {
                            self.gesture = Some(Gesture::Pan { last: pos });
                        }
                        PointerButton::Primary => {
                            self.press(pos, modifiers, space, viewport, size, cx)
                        }
                        // The middle button scrolls and a right-click picks
                        // a color, as in Clip Studio Paint, unless the
                        // Modifier Key Settings give them other tools.
                        PointerButton::Middle => {
                            self.press_mouse_button(Input::Middle, pos, viewport, size, cx);
                        }
                        PointerButton::Secondary => {
                            self.press_mouse_button(Input::Right, pos, viewport, size, cx);
                        }
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) => {
                    self.clock.0 += f64::from(self.clock.1);
                    self.drag(pos, viewport, size, cx);
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } => {
                    let _ = button;
                    if matches!(self.gesture, Some(Gesture::Engine(_))) {
                        // A brush stroke ends where the pen last was; the
                        // release is not a point of it.
                        self.engine_release(pos, viewport, size, cx);
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
        }
        // Two fingers or more move, zoom and turn the view.
        let pinch = ctx
            .input(egui::InputState::multi_touch)
            .filter(|touch| !covered(touch.center_pos));
        // Preferences, Touch gestures: which of them move the view.
        let gestures = crate::studio_mode::Gestures::of(cx.rice());
        if let Some(touch) = pinch.filter(|_| !blocked)
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            if gestures.swipe {
                doc.view.pan += touch.translation_delta;
            }
            if gestures.pinch && (touch.zoom_delta - 1.0).abs() > 1e-4 {
                doc.view
                    .zoom_at(viewport, size, touch.center_pos, touch.zoom_delta);
            }
            if gestures.rotate && touch.rotation_delta.abs() > 1e-4 {
                doc.view
                    .rotate_by(viewport, size, touch.rotation_delta.to_degrees());
            }
        }
        if response.hovered() && !blocked && pinch.is_none() {
            let (scroll, zoom) = ctx.input(|input| (input.smooth_scroll_delta, input.zoom_delta()));
            let keys = Keys::of(held, space);
            if let Some(pointer) = response.hover_pos().filter(|pos| !covered(*pos)) {
                if (zoom - 1.0).abs() > 1e-4 && !keys.ctrl {
                    // Two fingers pinching on a touchpad always zoom.
                    if let Some(doc) = self.docs.get_mut(self.active) {
                        doc.view.zoom_at(viewport, size, pointer, zoom);
                    }
                } else if (zoom - 1.0).abs() > 1e-4 {
                    // Ctrl and the wheel come as a zoom: back to a turn of
                    // the wheel, for the row of Ctrl.
                    self.wheel(keys, zoom.ln() / 0.0025, 0.0, pointer, viewport, size, cx);
                } else if scroll.x != 0.0 || scroll.y != 0.0 {
                    // Shift and the wheel can come sideways.
                    let (amount, across) = if keys.shift {
                        (scroll.y + scroll.x, 0.0)
                    } else {
                        (scroll.y, scroll.x)
                    };
                    self.wheel(keys, amount, across, pointer, viewport, size, cx);
                }
            }
            // The cursor of the tool a press would go to: the one a held
            // key borrows.
            let tool = self.tool_under_keys(keys, cx);
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
        let visible = self.on_screen.0;
        // The new engine's stroke: this frame's samples to its worker, and
        // what it laid since the last frame.
        self.engine_frame(viewport, size, cx);
        if self.engine_ending() {
            // The worker is still laying a lifted stroke: a quiet mark.
            ctx.set_cursor_icon(egui::CursorIcon::Progress);
        }
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
