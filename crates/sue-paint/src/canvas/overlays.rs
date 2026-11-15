//! What is drawn over the picture: selections, shape previews, the
//! brush outline, grids, rulers, the selection launcher and puppet warp.

use egui::{Color32, Id, Key, Pos2, Rect, Response, RichText, Shape, Stroke, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use super::shape::ellipse_points;
use crate::doc;
use crate::icons::Icon;
use crate::panels::icon_button;
use crate::select::Selection;
use crate::studio::{Gesture, SuePaint};
use crate::tools::Tool;

impl SuePaint {
    /// Clip Studio Paint's Selection Launcher: the commands for a selection,
    /// in a bar under it.
    pub(super) fn selection_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.launcher || self.gesture.is_some() {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some([x0, y0, x1, y1]) = doc.selection.as_ref().and_then(Selection::bounds) else {
            return;
        };
        let view = doc.view;
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x as f32, y as f32)));
        let bottom = corners
            .iter()
            .map(|corner| corner.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let middle = corners.iter().map(|corner| corner.x).sum::<f32>() / 4.0;
        let buttons: [(Icon, &str, &str); 9] = [
            (Icon::Deselect, &tr!("deselect"), "select.none"),
            (Icon::Reset, &tr!("invert-selected-area"), "select.invert"),
            (Icon::ZoomIn, &tr!("expand-selected-area"), "select.expand"),
            (Icon::ZoomOut, &tr!("shrink-selected-area"), "select.shrink"),
            (Icon::Clear, &tr!("delete"), "edit.clear"),
            (
                Icon::ClearOutside,
                &tr!("delete-outside-selection"),
                "edit.clear_outside",
            ),
            (Icon::FillSelection, &tr!("fill"), "edit.fill"),
            (Icon::Fit, &tr!("scale-rotate"), "edit.transform.scale"),
            (Icon::Frame, &tr!("crop"), "edit.crop"),
        ];
        let width = buttons.len() as f32 * 26.0 + 8.0;
        let bar = Rect::from_min_size(pos2(middle - width / 2.0, bottom + 10.0), vec2(width, 30.0));
        let bar = bar.translate(vec2(
            (viewport.left() + 4.0 - bar.left()).max(0.0)
                + (viewport.right() - 4.0 - bar.right()).min(0.0),
            (viewport.bottom() - 4.0 - bar.bottom()).min(0.0),
        ));
        crate::launchers::note(ui.ctx(), bar);
        ui.painter()
            .rect_filled(bar, theme.control_radius, theme.panel);
        ui.painter().rect_stroke(
            bar,
            theme.control_radius,
            theme.hairline(),
            egui::StrokeKind::Inside,
        );
        let mut run = None;
        for (index, (icon, tip, command)) in buttons.iter().enumerate() {
            let at = Rect::from_min_size(
                pos2(bar.left() + 4.0 + index as f32 * 26.0, bar.top() + 4.0),
                vec2(22.0, 22.0),
            );
            if icon_button(ui, at, *icon, true, false, tip, theme) {
                run = Some(*command);
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    #[allow(clippy::too_many_lines, reason = "every overlay of the canvas")]
    /// Puppet warp's pins, the mesh they bend, and its bar (Reset, Cancel,
    /// OK; Enter keeps the bend and Esc puts the layer back).
    pub(super) fn warp_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(warp) = self.warp.as_ref() else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let offset = [warp.offset[0] as f32, warp.offset[1] as f32];
        let screen =
            |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0] + offset[0], p[1] + offset[1]));
        let painter = ui.painter().with_clip_rect(viewport);
        // The mesh over the layer's pixels, bent as the pins bend them.
        if let Some([x0, y0, x1, y1]) = warp.bounds {
            let cells = 10;
            let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
            let at = |i: u32, j: u32| {
                let point = [
                    x0 as f32 + w * i as f32 / cells as f32,
                    y0 as f32 + h * j as f32 / cells as f32,
                ];
                screen(crate::warp::forward(point, &warp.pins))
            };
            let stroke = Stroke::new(1.0, theme.muted.gamma_multiply(0.7));
            for j in 0..=cells {
                painter.add(Shape::line((0..=cells).map(|i| at(i, j)).collect(), stroke));
            }
            for i in 0..=cells {
                painter.add(Shape::line((0..=cells).map(|j| at(i, j)).collect(), stroke));
            }
        }
        let held = match self.gesture {
            Some(Gesture::Pin { index }) => Some(index),
            _ => None,
        };
        for (index, pin) in warp.pins.iter().enumerate() {
            let at = screen(pin.to);
            let fill = if held == Some(index) {
                theme.border_active
            } else {
                theme.text
            };
            painter.circle_filled(at, 5.5, fill);
            painter.circle_stroke(at, 5.5, Stroke::new(1.5, Color32::BLACK));
        }
        // The bar.
        let pins = warp.pins.len();
        let mut action = None;
        egui::Area::new(Id::unique("sue-paint-warp-bar"))
            .fade_in(false)
            .order(egui::Order::Foreground)
            .fixed_pos(viewport.center_top() + vec2(-200.0, 10.0))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::pill_frame(theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr!("puppet-warp")).color(theme.text));
                        ui.label(
                            RichText::new(match pins {
                                0 => tr!("click-to-put-pins"),
                                1 => tr!("n-1-pin"),
                                count => tr!("pins", count = count),
                            })
                            .color(theme.muted),
                        );
                        if ui
                            .button(tr!("reset"))
                            .on_hover_text(tr!("take-every-pin-away"))
                            .clicked()
                        {
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
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
            )
        });
        if enter {
            action = Some(2);
        } else if escape {
            action = Some(1);
        }
        match action {
            Some(0) => {
                if let Some(warp) = self.warp.as_mut() {
                    warp.pins.clear();
                }
                self.warp_preview(cx);
            }
            Some(1) => self.finish_warp(cx, false),
            Some(2) => self.finish_warp(cx, true),
            _ => {}
        }
    }

    pub(super) fn overlays(
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
                    Tool::Figure | Tool::Frame | Tool::Balloon | Tool::Ruler
                        if (tool == &Tool::Figure
                            && matches!(sub.mode, 0 | crate::comic::SPEED_LINES))
                            || (tool != &Tool::Figure && sub.mode == 1)
                            || (tool == &Tool::Balloon
                                && sub.mode == crate::comic::THOUGHT_TAIL)
                            || tool == &Tool::Ruler =>
                    {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                    }
                    _ if (tool == &Tool::Select && sub.mode == 1)
                        || (tool == &Tool::Figure
                            && matches!(sub.mode, 2 | 4 | crate::comic::SATURATED_LINES))
                        || (tool == &Tool::Balloon
                            && matches!(
                                sub.mode,
                                crate::comic::ELLIPSE_BALLOON
                                    | crate::comic::CLOUD_BALLOON
                                    | crate::comic::FLASH_BALLOON
                            )) =>
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
            Some(
                Gesture::Lasso { points, .. }
                | Gesture::Outline { points, .. }
                | Gesture::LassoFill { points, .. },
            ) => {
                let path = points.iter().copied().map(screen).collect::<Vec<_>>();
                painter.add(Shape::line(path, Stroke::new(1.0, theme.border_active)));
            }
            Some(Gesture::Comic(edit)) => {
                for outline in edit.outlines() {
                    let path = outline.into_iter().map(screen).collect::<Vec<_>>();
                    painter.add(Shape::closed_line(
                        path,
                        Stroke::new(1.5, theme.border_active),
                    ));
                }
            }
            _ => {}
        }
        // Corners being clicked out, and the line on to the pointer; a
        // curve balloon shows its curve.
        if !self.polyline.is_empty() {
            let mut points = self.polyline.clone();
            if let Some(hover) = response.hover_pos() {
                let at = view.to_picture(viewport, size, hover);
                points.push([at.x, at.y]);
            }
            let curve = self.polyline_for == Some((Tool::Balloon, crate::comic::CURVE_BALLOON))
                && points.len() >= 3;
            let stroke = Stroke::new(1.0, theme.border_active);
            if curve {
                let path = crate::manga::smooth_closed(&points, 10)
                    .into_iter()
                    .map(screen)
                    .collect();
                painter.add(Shape::closed_line(path, stroke));
            } else {
                painter.add(Shape::line(
                    points.iter().copied().map(screen).collect(),
                    stroke,
                ));
            }
            for corner in &self.polyline {
                painter.circle_stroke(screen(*corner), 3.5, stroke);
            }
        }
        // The Object tool on effect lines: their handles.
        if self.toolbox.tool() == Tool::Operation
            && self.gesture.is_none()
            && let Some(lines) = doc
                .layer
                .and_then(|layer| Self::effect_of(&cx.world.arena, layer))
        {
            let stroke = Stroke::new(1.0, theme.border_active);
            let outline = crate::comic::effect_outline(&lines)
                .into_iter()
                .map(screen)
                .collect();
            painter.add(if lines.speed {
                Shape::line(outline, stroke)
            } else {
                Shape::closed_line(outline, stroke)
            });
            for end in [lines.a, lines.b] {
                painter.circle_filled(screen(end), 4.0, theme.border_active);
            }
        }
        if doc.show_rulers {
            let turns = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 0)
                .map_or(1, |sub| crate::rulers::turns(sub.lines, sub.mirror));
            let rulers = crate::rulers::Rulers::of(&cx.world.arena, doc.picture);
            ruler_overlay(
                &painter,
                &rulers,
                self.vanishing_line,
                [
                    doc.ruler_on && crate::commands::special_snaps(&cx.world.arena, doc.picture),
                    doc.snap_ruler,
                ],
                turns,
                size,
                view.zoom,
                &screen,
            );
        }
        // A curve ruler being placed, or a ruler being drawn with the pen.
        let violet = Color32::from_rgb(170, 120, 255);
        if !self.curve_points.is_empty() {
            let mut points = self.curve_points.clone();
            if let Some(pointer) = self.pointer {
                let at = view.to_picture(viewport, size, pointer);
                points.push([at.x, at.y]);
            }
            let path = crate::rulers::Path::curve(&points);
            let line: Vec<Pos2> = path.points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(1.5, violet)));
            for point in &self.curve_points {
                painter.circle_stroke(screen(*point), 4.0, Stroke::new(1.5, violet));
            }
        }
        if let Some(Gesture::RulerPen { points }) = &self.gesture {
            let line: Vec<Pos2> = points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(1.5, violet)));
        }
        // The brush outline under the pointer.
        let tool = self.toolbox.tool();
        let mode = self.toolbox.current().mode;
        if tool == Tool::CorrectLine
            && let Some(layer) = doc.layer
            && crate::vector::is_vector(&cx.world.arena, layer)
        {
            // The lines' control points, or the trace being drawn.
            let offset = doc::layers(&cx.world.arena, doc.picture)
                .into_iter()
                .find(|info| info.id == layer)
                .map_or([0, 0], |info| info.style.offset);
            let shift = |at: [f32; 2]| screen([at[0] + offset[0] as f32, at[1] + offset[1] as f32]);
            let lines = match &self.gesture {
                Some(Gesture::Correct { lines, .. }) => lines.clone(),
                _ => crate::vector::lines(&crate::vector::geometry(&cx.world.arena, layer)),
            };
            let violet = Color32::from_rgb(120, 160, 255);
            if mode == 0 {
                for line in &lines {
                    for index in line.controls() {
                        let at = shift(line.samples[index].at);
                        let rect = Rect::from_center_size(at, vec2(7.0, 7.0));
                        if line.is_corner(index) {
                            painter.rect_filled(rect, 0.0, violet);
                        } else {
                            painter.rect_filled(rect, 0.0, Color32::WHITE);
                            painter.rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(1.0, violet),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                }
            }
            if let Some(Gesture::Correct { path, .. }) = &self.gesture
                && path.len() > 1
            {
                let points: Vec<Pos2> = path.iter().map(|(at, _)| shift(*at)).collect();
                painter.add(Shape::line(points, Stroke::new(1.5, violet)));
            }
        }
        if tool == Tool::Operation
            && let Some(folder) = doc
                .layer
                .filter(|layer| crate::camera::is_camera(&cx.world.arena, *layer))
        {
            // The camera's frame on the layers it sees.
            let (frame, time) = {
                let rate = crate::anim::timeline(&cx.world.arena, doc.picture)
                    .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
                (doc.frame, crate::camera::time_of(doc.frame, rate))
            };
            let camera = match self.camera_pending {
                Some((pending, at, camera)) if pending == folder && at == frame => camera,
                _ => crate::camera::Camera::of(&cx.world.arena, folder, Some(time), size),
            };
            let corners = camera.frame(size).map(&screen);
            let red = Color32::from_rgb(235, 90, 90);
            painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(2.0, red)));
            for corner in corners {
                painter.rect_filled(Rect::from_center_size(corner, vec2(8.0, 8.0)), 0.0, red);
            }
            let middle = screen(camera.center);
            painter.line_segment(
                [middle - vec2(8.0, 0.0), middle + vec2(8.0, 0.0)],
                Stroke::new(1.5, red),
            );
            painter.line_segment(
                [middle - vec2(0.0, 8.0), middle + vec2(0.0, 8.0)],
                Stroke::new(1.5, red),
            );
        }
        if tool == Tool::Operation
            && let Some(layer) = doc.layer
            && let Some(model) = crate::figure::of_layer(&cx.world.arena, layer)
            && let (Ok(model), Ok(entity), Some(raster)) = (
                cx.world.arena.get_by_id(model),
                cx.world.arena.get_by_id(layer),
                doc::raster(&cx.world.arena, layer),
            )
        {
            // The figure's skeleton: its bones faint, its joints as dots,
            // the one being posed lit.
            let pose = crate::figure::Pose::of(model);
            let body = crate::figure::Body::of(model);
            let lens = crate::three::Camera::new(
                &crate::three::View3D::of(entity),
                [raster.width(), raster.height()],
                body.frame(),
            );
            let offset = doc::layers(&cx.world.arena, doc.picture)
                .into_iter()
                .find(|info| info.id == layer)
                .map_or([0, 0], |info| info.style.offset);
            let place = |point: [f32; 3]| {
                lens.project(point)
                    .map(|[x, y]| screen([x + offset[0] as f32, y + offset[1] as f32]))
            };
            let chosen = self.figure_joint;
            for (joint, (start, end)) in crate::figure::bones(&pose, &body).iter().enumerate() {
                let (Some(a), Some(b)) = (place(*start), place(*end)) else {
                    continue;
                };
                let lit = chosen == Some(joint);
                painter.line_segment(
                    [a, b],
                    Stroke::new(
                        if lit { 2.5 } else { 1.0 },
                        if lit {
                            Color32::from_rgb(255, 170, 40)
                        } else {
                            Color32::from_rgba_unmultiplied(80, 160, 255, 150)
                        },
                    ),
                );
                painter.circle_filled(
                    a,
                    if lit { 5.0 } else { 3.5 },
                    if lit {
                        Color32::from_rgb(255, 170, 40)
                    } else {
                        Color32::from_rgb(80, 160, 255)
                    },
                );
            }
        }
        if tool == Tool::Operation
            && let Some((layer, picked)) = &self.picked_lines
            && doc.layer == Some(*layer)
            && !picked.is_empty()
        {
            // The picked lines, outlined, in their box.
            let offset = doc::layers(&cx.world.arena, doc.picture)
                .into_iter()
                .find(|info| info.id == *layer)
                .map_or([0, 0], |info| info.style.offset);
            let base = crate::vector::lines(&crate::vector::geometry(&cx.world.arena, *layer));
            let lines = match &self.gesture {
                Some(Gesture::MoveLines { base, by, .. }) => {
                    crate::vector::moved(base, picked, *by)
                }
                _ => base,
            };
            let blue = Color32::from_rgb(70, 130, 255);
            let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
            for line in picked.iter().filter_map(|index| lines.get(*index)) {
                let points: Vec<Pos2> = line
                    .samples
                    .iter()
                    .map(|sample| {
                        for axis in 0..2 {
                            min[axis] = min[axis].min(sample.at[axis]);
                            max[axis] = max[axis].max(sample.at[axis]);
                        }
                        screen([
                            sample.at[0] + offset[0] as f32,
                            sample.at[1] + offset[1] as f32,
                        ])
                    })
                    .collect();
                painter.add(Shape::line(points, Stroke::new(1.5, blue)));
            }
            if min[0].is_finite() {
                let corners = [
                    [min[0], min[1]],
                    [max[0], min[1]],
                    [max[0], max[1]],
                    [min[0], max[1]],
                ]
                .map(|[x, y]| screen([x + offset[0] as f32, y + offset[1] as f32]));
                painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(1.0, blue)));
            }
        }
        if response.hovered()
            && (tool.paints()
                || tool == Tool::Blend
                || (tool == Tool::CorrectLine && matches!(mode, 2 | 3 | 4 | 6)))
            && let Some(pointer) = self.pointer
        {
            let radius = (self.toolbox.current().tip.size * 0.5 * view.zoom).max(1.5);
            // Preferences, Cursor: the brush's outline, a crosshair or a dot.
            let shape = cx
                .rice()
                .get_str("sue_paint.brush_cursor")
                .unwrap_or("brush");
            let dark = Stroke::new(2.0, Color32::from_black_alpha(140));
            let light = Stroke::new(1.0, Color32::from_white_alpha(220));
            match shape {
                "crosshair" => {
                    for stroke in [dark, light] {
                        painter.line_segment(
                            [pointer - vec2(8.0, 0.0), pointer + vec2(8.0, 0.0)],
                            stroke,
                        );
                        painter.line_segment(
                            [pointer - vec2(0.0, 8.0), pointer + vec2(0.0, 8.0)],
                            stroke,
                        );
                    }
                }
                "dot" => {
                    painter.circle_filled(pointer, 2.5, Color32::from_black_alpha(160));
                    painter.circle_filled(pointer, 1.5, Color32::WHITE);
                }
                _ => {
                    painter.circle_stroke(pointer, radius, dark);
                    painter.circle_stroke(pointer, radius, light);
                    painter.circle_filled(pointer, 1.0, Color32::WHITE);
                }
            }
        }
        let _ = cx;
    }
}

/// How far point `p` is from the segment `a` to `b`.
pub(super) fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// Draws `rulers` (picture pixels, `screen` maps them) in Clip Studio
/// Paint's violet, faint while their snapping (`[special, linear]`) is off,
/// with the first line of a vanishing point still to be drawn dashed.
#[allow(clippy::too_many_arguments, reason = "the rulers and how they show")]
#[allow(clippy::too_many_lines, reason = "one block per ruler")]
fn ruler_overlay(
    painter: &egui::Painter,
    rulers: &crate::rulers::Rulers,
    vanishing_line: Option<[[f32; 2]; 2]>,
    snapping: [bool; 2],
    turns: u32,
    size: [u32; 2],
    zoom: f32,
    screen: &dyn Fn([f32; 2]) -> Pos2,
) {
    use crate::rulers::Special;
    let violet = Color32::from_rgb(170, 120, 255);
    let tone = |on: bool| {
        if on {
            violet
        } else {
            violet.gamma_multiply(0.35)
        }
    };
    let (special, linear) = (tone(snapping[0]), tone(snapping[1]));
    let (w, h) = (size[0] as f32, size[1] as f32);
    let diagonal = w.hypot(h);
    let reach = diagonal * 4.0;
    let middle = [w / 2.0, h / 2.0];
    // A whole line through `at` along `direction`.
    let across = |at: [f32; 2], direction: [f32; 2], stroke: Stroke| {
        painter.line_segment(
            [
                screen([at[0] - direction[0] * reach, at[1] - direction[1] * reach]),
                screen([at[0] + direction[0] * reach, at[1] + direction[1] * reach]),
            ],
            stroke,
        );
    };
    if let Some((center, angle)) = rulers.symmetry {
        let turns = turns.max(1);
        for line in 0..turns {
            let theta = angle + line as f32 * std::f32::consts::PI / turns as f32;
            let (dx, dy) = (theta.cos() * reach, theta.sin() * reach);
            let ends = [
                screen([center[0] - dx, center[1] - dy]),
                screen([center[0] + dx, center[1] + dy]),
            ];
            painter.add(Shape::dashed_line(
                &ends,
                Stroke::new(1.5, special),
                8.0,
                5.0,
            ));
        }
        painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
    }
    if let Some(perspective) = &rulers.perspective {
        // Lines from every vanishing point to the canvas edge, all round.
        let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        let rim: Vec<[f32; 2]> = (0..4)
            .flat_map(|side| {
                let (a, b) = (corners[side], corners[(side + 1) % 4]);
                (0..4).map(move |step| {
                    let t = step as f32 / 4.0;
                    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                })
            })
            .collect();
        let faint = Stroke::new(1.0, special.gamma_multiply(0.4));
        for point in &perspective.points {
            for target in &rim {
                painter.line_segment([screen(*point), screen(*target)], faint);
            }
        }
        let (origin, direction) = perspective.eye_level();
        across(origin, direction, Stroke::new(1.5, special));
        let upright = [-direction[1], direction[0]];
        match perspective.points.len() {
            1 => across(origin, upright, faint),
            2 => {
                if let Some(at) = rulers.eye_handle(size) {
                    across(at, upright, faint);
                }
            }
            _ => {}
        }
        for point in &perspective.points {
            painter.circle_stroke(screen(*point), 5.0, Stroke::new(1.5, special));
            painter.circle_filled(screen(*point), 1.5, special);
        }
        if let Some(at) = rulers.eye_handle(size) {
            painter.rect_stroke(
                Rect::from_center_size(screen(at), vec2(8.0, 8.0)),
                0.0,
                Stroke::new(1.5, special),
                egui::StrokeKind::Middle,
            );
        }
    }
    match rulers.special {
        Some(Special::Parallel(angle)) => {
            let direction = [angle.cos(), angle.sin()];
            let normal = [-direction[1], direction[0]];
            for step in -6..=6 {
                let away = step as f32 * diagonal / 12.0;
                across(
                    [middle[0] + normal[0] * away, middle[1] + normal[1] * away],
                    direction,
                    Stroke::new(1.0, special),
                );
            }
        }
        Some(Special::Radial(center)) => {
            for step in 0..24 {
                let theta = step as f32 * std::f32::consts::TAU / 24.0;
                painter.line_segment(
                    [
                        screen(center),
                        screen([
                            center[0] + theta.cos() * reach,
                            center[1] + theta.sin() * reach,
                        ]),
                    ],
                    Stroke::new(1.0, special),
                );
            }
            painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
        }
        Some(Special::Concentric(center)) => {
            for step in 1..=12 {
                let radius = step as f32 * diagonal / 12.0 * zoom;
                painter.circle_stroke(screen(center), radius, Stroke::new(1.0, special));
            }
            painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
        }
        None => {}
    }
    for path in &rulers.paths {
        let mut points: Vec<Pos2> = path.points.iter().map(|point| screen(*point)).collect();
        if path.closed
            && let Some(first) = points.first().copied()
        {
            points.push(first);
        }
        painter.add(Shape::line(points, Stroke::new(1.5, linear)));
    }
    if let Some([a, b]) = rulers.linear {
        painter.line_segment([screen(a), screen(b)], Stroke::new(2.0, linear));
        for end in [a, b] {
            painter.circle_filled(screen(end), 3.5, linear);
        }
    }
    if let Some([a, b]) = vanishing_line {
        let direction = [b[0] - a[0], b[1] - a[1]];
        let length = direction[0].hypot(direction[1]).max(1e-3);
        let direction = [direction[0] / length, direction[1] / length];
        let ends = [
            screen([a[0] - direction[0] * reach, a[1] - direction[1] * reach]),
            screen([a[0] + direction[0] * reach, a[1] + direction[1] * reach]),
        ];
        painter.add(Shape::dashed_line(
            &ends,
            Stroke::new(1.5, violet),
            8.0,
            5.0,
        ));
    }
}
