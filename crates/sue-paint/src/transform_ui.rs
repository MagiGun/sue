//! Sue Paint's Scale/Rotate (Ctrl+T) and Free Transform (Ctrl+Shift+T), as
//! in Clip Studio Paint: the selected part of the layer (or all of it) in a
//! box with handles. Scale/Rotate drags a corner to scale (keeping the
//! aspect, Shift frees it), an edge to stretch, inside to move and outside
//! to turn; Free Transform drags each corner on its own. Enter keeps it as
//! one undo step, Esc puts the layer back.

use egui::{Color32, Id, Key, Modifiers, Pos2, Rect, RichText, Shape, Stroke, Ui, pos2, vec2};
use sue_core::{EntityId, RasterTile2D};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use crate::doc;
use crate::icons::Icon;
use crate::studio::{Gesture, SuePaint};
use crate::transform::{self, Quad};
use crate::view::View;

/// What a press in the transform box grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    /// A corner, 0 top left, then clockwise.
    Corner(usize),
    /// An edge: 0 top, 1 right, 2 bottom, 3 left.
    Edge(usize),
    /// The inside: moves it.
    Move,
    /// Outside the box: turns it.
    Rotate,
}

/// A transform in progress on one layer.
#[derive(Clone, Debug)]
pub(crate) struct TransformSession {
    pub layer: EntityId,
    pub size: [u32; 2],
    pub before: Vec<u8>,
    pub rest: Vec<u8>,
    pub piece: Vec<u8>,
    pub rect: [u32; 4],
    pub offset: [i64; 2],
    /// Free Transform: every corner its own.
    pub free: bool,
    pub center: [f32; 2],
    pub scale: [f32; 2],
    pub angle: f32,
    pub corners: Quad,
    pub shown: Option<Quad>,
    pub selected: bool,
}

impl TransformSession {
    fn piece_size(&self) -> [f32; 2] {
        [
            (self.rect[2] - self.rect[0]) as f32,
            (self.rect[3] - self.rect[1]) as f32,
        ]
    }

    /// The box as it stands, in the layer's pixels.
    pub(crate) fn quad(&self) -> Quad {
        if self.free {
            self.corners
        } else {
            let [w, h] = self.piece_size();
            transform::rotated(self.center, w, h, self.scale, self.angle)
        }
    }

    /// Mirrors the box: left to right when `horizontal`, else top to
    /// bottom, across its own middle.
    pub(crate) fn flip(&mut self, horizontal: bool) {
        if self.free {
            let middle = self.corners.iter().fold([0.0, 0.0], |sum, corner| {
                [sum[0] + corner[0] / 4.0, sum[1] + corner[1] / 4.0]
            });
            let axis = usize::from(!horizontal);
            for corner in &mut self.corners {
                corner[axis] = 2.0 * middle[axis] - corner[axis];
            }
        } else if horizontal {
            self.scale[0] = -self.scale[0];
        } else {
            self.scale[1] = -self.scale[1];
        }
    }

    /// Turns the box a quarter, clockwise when `clockwise`.
    pub(crate) fn quarter_turn(&mut self, clockwise: bool) {
        let quarter = if clockwise {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        if self.free {
            let middle = self.corners.iter().fold([0.0, 0.0], |sum, corner| {
                [sum[0] + corner[0] / 4.0, sum[1] + corner[1] / 4.0]
            });
            for corner in &mut self.corners {
                let [x, y] = turn([corner[0] - middle[0], corner[1] - middle[1]], quarter);
                *corner = [middle[0] + x, middle[1] + y];
            }
        } else {
            self.angle += quarter;
        }
    }

    /// Back to where it started.
    fn reset(&mut self) {
        let [x0, y0, x1, y1] = self.rect.map(|value| value as f32);
        self.center = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        self.scale = [1.0, 1.0];
        self.angle = 0.0;
        self.corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    }
}

/// `v` turned by `angle` radians.
fn turn([x, y]: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = angle.sin_cos();
    [x * cos - y * sin, x * sin + y * cos]
}

impl SuePaint {
    /// Starts Scale/Rotate (or Free Transform) on the current layer, or the
    /// selected part of it.
    pub(crate) fn start_transform(&mut self, cx: &mut AppletContext<'_>, free: bool) {
        if self.transform.is_some() || self.warp.is_some() {
            return;
        }
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let selection = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map(|selection| selection.shifted(info.style.offset));
        let Some((rect, piece, rest)) =
            transform::lift(&before, size[0], size[1], selection.as_ref())
        else {
            cx.notify(tr!("nothing-to-transform-here"));
            return;
        };
        let mut session = TransformSession {
            layer: info.id,
            size,
            before,
            rest,
            piece,
            rect,
            offset: info.style.offset,
            free,
            center: [0.0; 2],
            scale: [1.0, 1.0],
            angle: 0.0,
            corners: [[0.0; 2]; 4],
            shown: None,
            selected: selection.is_some(),
        };
        session.reset();
        session.shown = Some(session.quad());
        self.transform = Some(session);
    }

    /// Shows the layer with the piece where the box puts it.
    pub(crate) fn transform_preview(&mut self, cx: &mut AppletContext<'_>) {
        let Some(session) = self.transform.as_mut() else {
            return;
        };
        let quad = session.quad();
        if session.shown == Some(quad) {
            return;
        }
        let size = session.piece_size().map(|value| value as u32);
        let pixels = transform::place(
            &session.rest,
            session.size[0],
            session.size[1],
            &session.piece,
            size,
            &quad,
        );
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, session.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == pixels.len())
        {
            bytes.copy_from_slice(&pixels);
        }
        session.shown = Some(quad);
    }

    /// Ends the transform: keeps it as one undo step (the selection moves
    /// with it), or puts the layer back.
    pub(crate) fn finish_transform(&mut self, cx: &mut AppletContext<'_>, keep: bool) {
        let Some(session) = self.transform.take() else {
            return;
        };
        self.gesture = None;
        let quad = session.quad();
        let after = if keep {
            let size = session.piece_size().map(|value| value as u32);
            transform::place(
                &session.rest,
                session.size[0],
                session.size[1],
                &session.piece,
                size,
                &quad,
            )
        } else {
            session.before.clone()
        };
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, session.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == after.len())
        {
            bytes.copy_from_slice(&after);
        }
        if !keep {
            return;
        }
        let label = if session.free {
            tr!("free-transform")
        } else {
            tr!("scale-rotate")
        };
        if let Some(patch) =
            doc::diff_patch(&session.before, &after, session.size[0], session.size[1])
        {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                session.layer,
                label,
                vec![patch],
            )));
        }
        if session.selected
            && let Some(doc) = self.doc_mut()
            && let Some(old) = doc.selection.as_ref()
        {
            let [width, height] = old.size();
            let mut moved = crate::select::Selection::new(width, height);
            let points: Vec<[f32; 2]> = quad
                .iter()
                .map(|[x, y]| [x + session.offset[0] as f32, y + session.offset[1] as f32])
                .collect();
            moved.polygon(&points, crate::select::Op::Replace);
            doc.set_selection(Some(moved));
        }
    }

    /// A press while transforming: grabs a handle, the inside or the
    /// outside.
    pub(crate) fn transform_press(
        &mut self,
        pos: Pos2,
        view: View,
        viewport: Rect,
        size: [u32; 2],
    ) {
        let Some(session) = self.transform.as_ref() else {
            return;
        };
        let offset = session.offset.map(|value| value as f32);
        let quad = session.quad();
        let screen =
            |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0] + offset[0], p[1] + offset[1]));
        let near = |p: [f32; 2]| screen(p).distance(pos) <= 9.0;
        let handle = (0..4)
            .find(|&corner| near(quad[corner]))
            .map(Handle::Corner)
            .or_else(|| {
                (0..4)
                    .find(|&edge| {
                        let (a, b) = (quad[edge], quad[(edge + 1) % 4]);
                        near([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
                    })
                    .map(Handle::Edge)
            })
            .unwrap_or_else(|| {
                let p = view.to_picture(viewport, size, pos);
                let local = [p.x - offset[0], p.y - offset[1]];
                if crate::manga::signed_distance(&quad, local) <= 0.0 || session.free {
                    Handle::Move
                } else {
                    Handle::Rotate
                }
            });
        let p = view.to_picture(viewport, size, pos);
        self.gesture = Some(Gesture::Transform {
            handle,
            start: [p.x - offset[0], p.y - offset[1]],
            quad,
            center: session.center,
            scale: session.scale,
            angle: session.angle,
        });
    }

    /// A drag while transforming, to `p` in the layer's pixels.
    pub(crate) fn transform_drag(&mut self, p: [f32; 2], modifiers: Modifiers) {
        let Some(Gesture::Transform {
            handle,
            start,
            quad,
            center,
            scale,
            angle,
        }) = self.gesture
        else {
            return;
        };
        let Some(session) = self.transform.as_mut() else {
            return;
        };
        let [w, h] = session.piece_size();
        let delta = [p[0] - start[0], p[1] - start[1]];
        if session.free {
            let mut corners = quad;
            match handle {
                Handle::Corner(i) => corners[i] = [quad[i][0] + delta[0], quad[i][1] + delta[1]],
                Handle::Edge(i) => {
                    for corner in [i, (i + 1) % 4] {
                        corners[corner] = [quad[corner][0] + delta[0], quad[corner][1] + delta[1]];
                    }
                }
                Handle::Move | Handle::Rotate => {
                    corners = quad.map(|[x, y]| [x + delta[0], y + delta[1]]);
                }
            }
            session.corners = corners;
            return;
        }
        match handle {
            Handle::Move => session.center = [center[0] + delta[0], center[1] + delta[1]],
            Handle::Rotate => {
                let from = (start[1] - center[1]).atan2(start[0] - center[0]);
                let to = (p[1] - center[1]).atan2(p[0] - center[0]);
                let mut turned = angle + (to - from);
                if modifiers.shift {
                    // Shift turns in steps of 15 degrees.
                    let step = 15_f32.to_radians();
                    turned = (turned / step).round() * step;
                }
                session.angle = turned;
            }
            Handle::Corner(i) => {
                let fixed = quad[(i + 2) % 4];
                let local = turn([p[0] - fixed[0], p[1] - fixed[1]], -angle);
                let (mut sx, mut sy) = (local[0].abs() / w.max(1.0), local[1].abs() / h.max(1.0));
                if !modifiers.shift {
                    let both = sx.max(sy);
                    (sx, sy) = (both, both);
                }
                let half = [
                    local[0].signum() * w * sx / 2.0,
                    local[1].signum() * h * sy / 2.0,
                ];
                let shift = turn(half, angle);
                session.scale = [sx.max(0.01), sy.max(0.01)];
                session.center = [fixed[0] + shift[0], fixed[1] + shift[1]];
            }
            Handle::Edge(i) => {
                let (a, b) = (quad[(i + 2) % 4], quad[(i + 3) % 4]);
                let fixed = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let local = turn([p[0] - fixed[0], p[1] - fixed[1]], -angle);
                let mut next = scale;
                let half = if i % 2 == 0 {
                    next[1] = (local[1].abs() / h.max(1.0)).max(0.01);
                    [0.0, local[1].signum() * h * next[1] / 2.0]
                } else {
                    next[0] = (local[0].abs() / w.max(1.0)).max(0.01);
                    [local[0].signum() * w * next[0] / 2.0, 0.0]
                };
                let shift = turn(half, angle);
                session.scale = next;
                session.center = [fixed[0] + shift[0], fixed[1] + shift[1]];
            }
        }
    }

    /// The box, its handles and the bar (the scale and angle as numbers,
    /// Reset, Cancel, OK; Enter keeps, Esc puts the layer back).
    pub(crate) fn transform_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let Some(session) = self.transform.as_mut() else {
            return;
        };
        let offset = session.offset.map(|value| value as f32);
        let screen =
            |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0] + offset[0], p[1] + offset[1]));
        let quad = session.quad();
        let corners: Vec<Pos2> = quad.iter().map(|p| screen(*p)).collect();
        let painter = ui.painter().with_clip_rect(viewport);
        painter.add(Shape::closed_line(
            corners.clone(),
            Stroke::new(2.5, Color32::from_black_alpha(160)),
        ));
        painter.add(Shape::closed_line(
            corners.clone(),
            Stroke::new(1.0, theme.border_active),
        ));
        let mut handles = corners.clone();
        for edge in 0..4 {
            let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
            handles.push(pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
        }
        for at in handles {
            let square = Rect::from_center_size(at, vec2(8.0, 8.0));
            painter.rect_filled(square, 1.0, theme.text);
            painter.rect_stroke(
                square,
                1.0,
                Stroke::new(1.0, Color32::BLACK),
                egui::StrokeKind::Outside,
            );
        }
        if !session.free {
            let middle = screen(session.center);
            painter.line_segment(
                [middle - vec2(5.0, 0.0), middle + vec2(5.0, 0.0)],
                Stroke::new(1.0, theme.border_active),
            );
            painter.line_segment(
                [middle - vec2(0.0, 5.0), middle + vec2(0.0, 5.0)],
                Stroke::new(1.0, theme.border_active),
            );
        }
        // The bar.
        let mut action = None;
        let free = session.free;
        let mut width = session.scale[0] * 100.0;
        let mut height = session.scale[1] * 100.0;
        let mut degrees = session.angle.to_degrees();
        let mut typed = false;
        egui::Area::new(Id::unique("sue-paint-transform-bar"))
            .fade_in(false)
            .order(egui::Order::Foreground)
            .fixed_pos(viewport.center_top() + vec2(-250.0, 10.0))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::pill_frame(theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if free {
                                tr!("free-transform")
                            } else {
                                tr!("scale-rotate")
                            })
                            .color(theme.text),
                        );
                        if !free {
                            typed |= ui
                                .add(
                                    egui::DragValue::new(&mut width)
                                        .suffix(" %")
                                        .range(1.0..=2000.0),
                                )
                                .on_hover_text(tr!("width-2"))
                                .changed();
                            typed |= ui
                                .add(
                                    egui::DragValue::new(&mut height)
                                        .suffix(" %")
                                        .range(1.0..=2000.0),
                                )
                                .on_hover_text(tr!("height-2"))
                                .changed();
                            typed |= ui
                                .add(
                                    egui::DragValue::new(&mut degrees)
                                        .suffix(tr!("deg"))
                                        .range(-360.0..=360.0),
                                )
                                .on_hover_text(tr!("angle"))
                                .changed();
                        }
                        if ui.button(tr!("reset")).clicked() {
                            action = Some(0);
                        }
                        if ui.button(tr!("cancel")).clicked() {
                            action = Some(1);
                        }
                        if ui.button("OK").clicked() {
                            action = Some(2);
                        }
                    });
                });
            });
        if typed {
            session.scale = [width / 100.0, height / 100.0];
            session.angle = degrees.to_radians();
        }
        // The Transform launcher, under the box.
        if self.transform_launcher {
            let buttons: [crate::launchers::Button; 7] = [
                (Icon::Reset, tr!("reset"), false),
                (Icon::FlipH, tr!("flip-horizontal"), false),
                (Icon::FlipV, tr!("flip-vertical"), false),
                (Icon::RotateLeft, tr!("rotate-90-degrees-left"), false),
                (Icon::RotateRight, tr!("rotate-90-degrees-right"), false),
                (Icon::Cancel, tr!("cancel"), false),
                (Icon::Confirm, tr!("ok"), false),
            ];
            match crate::launchers::bar(ui, viewport, &corners, &buttons, theme) {
                Some(0) => action = Some(0),
                Some(1) => session.flip(true),
                Some(2) => session.flip(false),
                Some(3) => session.quarter_turn(false),
                Some(4) => session.quarter_turn(true),
                Some(5) => action = Some(1),
                Some(_) => action = Some(2),
                None => {}
            }
        }
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
            )
        });
        if enter && !ui.ctx().text_edit_focused() {
            action = Some(2);
        } else if escape {
            action = Some(1);
        }
        match action {
            Some(0) => {
                session.reset();
                self.transform_preview(cx);
            }
            Some(1) => self.finish_transform(cx, false),
            Some(2) => self.finish_transform(cx, true),
            _ => self.transform_preview(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> TransformSession {
        let mut session = TransformSession {
            layer: EntityId::new(),
            size: [100, 100],
            before: Vec::new(),
            rest: Vec::new(),
            piece: Vec::new(),
            rect: [10, 10, 30, 20],
            offset: [0, 0],
            free: false,
            center: [0.0; 2],
            scale: [1.0, 1.0],
            angle: 0.0,
            corners: [[0.0; 2]; 4],
            shown: None,
            selected: false,
        };
        session.reset();
        session
    }

    fn grab(paint: &mut SuePaint, handle: Handle, start: [f32; 2]) {
        let session = paint.transform.as_ref().unwrap();
        paint.gesture = Some(Gesture::Transform {
            handle,
            start,
            quad: session.quad(),
            center: session.center,
            scale: session.scale,
            angle: session.angle,
        });
    }

    #[test]
    fn handles_scale_stretch_turn_and_bend_the_box() {
        let mut paint = SuePaint {
            transform: Some(session()),
            ..SuePaint::default()
        };
        // The bottom right corner out to twice the size: the top left stays.
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([50.0, 30.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert!((quad[0][0] - 10.0).abs() < 1e-3 && (quad[0][1] - 10.0).abs() < 1e-3);
        assert!((quad[2][0] - 50.0).abs() < 1e-3 && (quad[2][1] - 30.0).abs() < 1e-3);
        // With Shift, the aspect is free.
        paint.transform = Some(session());
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([50.0, 20.0], Modifiers::SHIFT);
        assert_eq!(paint.transform.as_ref().unwrap().scale, [2.0, 1.0]);
        // Outside the box turns it around its middle.
        paint.transform = Some(session());
        grab(&mut paint, Handle::Rotate, [40.0, 15.0]);
        paint.transform_drag([20.0, 35.0], Modifiers::NONE);
        let angle = paint.transform.as_ref().unwrap().angle;
        assert!(
            (angle - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "{angle}"
        );
        // The launcher's flips mirror the box; a quarter turn turns it.
        let mut flipped = session();
        flipped.flip(true);
        let quad = flipped.quad();
        assert!((quad[0][0] - 30.0).abs() < 1e-3, "the left edge went right");
        flipped.flip(true);
        assert_eq!(flipped.scale, [1.0, 1.0]);
        let mut turned = session();
        turned.quarter_turn(true);
        assert!((turned.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let mut free = session();
        free.free = true;
        free.flip(false);
        assert!(
            (free.corners[0][1] - 20.0).abs() < 1e-3,
            "the top went down"
        );
        // Free Transform moves one corner alone.
        let mut free = session();
        free.free = true;
        paint.transform = Some(free);
        grab(&mut paint, Handle::Corner(1), [30.0, 10.0]);
        paint.transform_drag([40.0, 0.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert_eq!(quad[1], [40.0, 0.0]);
        assert_eq!(quad[0], [10.0, 10.0]);
    }
}
