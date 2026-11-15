//! What is drawn over the picture: selections, shape previews, the
//! brush outline, grids and rulers.

use egui::{Color32, Pos2, Rect, Response, Shape, Stroke, Ui, Vec2, pos2};

use super::marks;
use sue_wm::{AppletContext, Theme};

use super::shape::ellipse_points;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::Tool;

impl SuePaint {
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
        // Lines `step` apart through `origin`, in picture pixels.
        let lines = |step: f32, origin: [f32; 2], color: Color32| {
            let count = ((visible[2] - visible[0]) / step) + ((visible[3] - visible[1]) / step);
            if count > 600.0 || step <= 0.0 {
                return;
            }
            let mut x = origin[0] + ((visible[0] - origin[0]) / step).floor() * step;
            while x <= visible[2] {
                painter.line_segment(
                    [screen([x, visible[1]]), screen([x, visible[3]])],
                    Stroke::new(marks::hair(theme), color),
                );
                x += step;
            }
            let mut y = origin[1] + ((visible[1] - origin[1]) / step).floor() * step;
            while y <= visible[3] {
                painter.line_segment(
                    [screen([visible[0], y]), screen([visible[2], y])],
                    Stroke::new(marks::hair(theme), color),
                );
                y += step;
            }
        };
        if self.pixel_grid && view.zoom >= 12.0 {
            lines(1.0, [0.0, 0.0], Color32::from_black_alpha(40));
        }
        if self.grid {
            // The picture's grid (View, Grid Settings): its divisions
            // fainter than its main lines.
            let grid = crate::grid::Grid::of(&cx.world.arena, doc.picture);
            let origin = grid.origin_in(size);
            if grid.divisions > 1 {
                lines(
                    grid.step(),
                    origin,
                    Color32::from_rgba_unmultiplied(0, 120, 255, 30),
                );
            }
            lines(
                grid.gap,
                origin,
                Color32::from_rgba_unmultiplied(0, 120, 255, 70),
            );
        }
        // The selection.
        if self.show_selection
            && let Some(selection) = &doc.selection
        {
            let edges = doc.edges.get_or_insert_with(|| selection.edges());
            if edges.len() < 60_000 {
                for [a, b] in edges.iter() {
                    let (a, b) = (screen(*a), screen(*b));
                    painter.line_segment([a, b], Stroke::new(marks::bold(theme), Color32::BLACK));
                    painter.line_segment([a, b], Stroke::new(marks::hair(theme), Color32::WHITE));
                }
            }
        }
        // The Figure tool's shapes and lines under way (canvas/shape.rs).
        let guides = super::shape::Guides::of(&cx.world.arena, doc);
        let figure = super::shape::Preview {
            painter: &painter,
            screen: &screen,
            stroke: super::shape::Preview::line_of(theme.border_active),
            pointer: self.pointer.map(|at| {
                let at = view.to_picture(viewport, size, at);
                [at.x, at.y]
            }),
            shift: self.modifiers.shift,
            angle_step: self.toolbox.current().angle_step,
            guides: &guides,
        };
        if let Some(bend) = &self.figure_draft.bend
            && self.toolbox.tool() == Tool::Figure
            && bend.picture == doc.picture
        {
            figure.bend(bend);
        }
        // What a drag is drawing.
        match &self.gesture {
            Some(Gesture::Shape {
                tool: Tool::Figure,
                sub,
                from,
                to,
                ..
            }) => figure.drag(sub, *from, *to),
            Some(Gesture::Shape {
                tool,
                sub,
                from,
                to,
                ..
            }) => {
                let outline = Stroke::new(marks::hair(theme), theme.border_active);
                let (min, max) = (from.min(*to), from.max(*to));
                match tool {
                    Tool::Gradient => {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                        painter.circle_stroke(
                            screen([from.x, from.y]),
                            marks::knob(theme),
                            outline,
                        );
                        painter.circle_stroke(screen([to.x, to.y]), marks::knob(theme), outline);
                    }
                    Tool::Figure | Tool::Frame | Tool::Balloon | Tool::Ruler
                        if (tool == &Tool::Figure
                            && matches!(sub.mode, 0 | crate::tools::SPEED_LINES))
                            || (tool != &Tool::Figure && sub.mode == 1)
                            || (tool == &Tool::Balloon
                                && sub.mode == crate::comic::THOUGHT_TAIL)
                            || tool == &Tool::Ruler =>
                    {
                        // A ruler (not the figure ruler) keeps to its
                        // Step of angle, or Shift's 45 degrees, as it
                        // will be laid (canvas/tools/ruler.rs).
                        let end = if tool == &Tool::Ruler && sub.mode != 7 {
                            super::shape::line_constrained(
                                [from.x, from.y],
                                [to.x, to.y],
                                sub.angle_step,
                                self.modifiers.shift,
                            )
                        } else {
                            [to.x, to.y]
                        };
                        painter.line_segment([screen([from.x, from.y]), screen(end)], outline);
                    }
                    _ if (tool == &Tool::Select && sub.mode == 1)
                        || (tool == &Tool::Figure
                            && matches!(sub.mode, 2 | 4 | crate::tools::SATURATED_LINES))
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
                | Gesture::LassoFill { points, .. }
                | Gesture::ShrinkLasso { points, .. },
            ) => {
                // Two tones, so the path reads on light and dark drawing.
                let path = points.iter().copied().map(screen).collect::<Vec<_>>();
                marks::guide(&painter, path, false, theme);
            }
            Some(Gesture::Comic(edit)) => {
                for outline in edit.outlines() {
                    let path = outline.into_iter().map(screen).collect::<Vec<_>>();
                    painter.add(Shape::closed_line(
                        path,
                        Stroke::new(marks::firm(theme), theme.border_active),
                    ));
                }
            }
            _ => {}
        }
        // Corners being clicked out, and the line on to the pointer; a
        // curve balloon shows its curve.
        if let Some((Tool::Figure, mode)) = self.polyline_for
            && !self.polyline.is_empty()
        {
            // A Bezier Curve anchor being dragged out: the line runs to
            // it, not to the pointer pulling its handle.
            let pending = match &self.gesture {
                Some(Gesture::Shape {
                    tool: Tool::Figure,
                    sub,
                    from,
                    to,
                    ..
                }) if sub.mode == crate::tools::BEZIER_CURVE => {
                    Some(([from.x, from.y], [to.x, to.y]))
                }
                _ => None,
            };
            figure.corners(mode, &self.polyline, &self.figure_draft.handles, pending);
        } else if !self.polyline.is_empty() {
            let mut points = self.polyline.clone();
            if let Some(hover) = response.hover_pos() {
                let at = view.to_picture(viewport, size, hover);
                points.push([at.x, at.y]);
            }
            let curve = self.polyline_for == Some((Tool::Balloon, crate::comic::CURVE_BALLOON))
                && points.len() >= 3;
            if curve {
                let path = crate::manga::smooth_closed(&points, 10)
                    .into_iter()
                    .map(screen)
                    .collect();
                marks::guide(&painter, path, true, theme);
            } else {
                marks::guide(
                    &painter,
                    points.iter().copied().map(screen).collect(),
                    false,
                    theme,
                );
            }
            for corner in &self.polyline {
                marks::dot(&painter, screen(*corner), false, theme);
            }
        }
        // The Object tool on effect lines: their handles.
        if self.toolbox.tool() == Tool::Operation
            && self.gesture.is_none()
            && let Some(lines) = doc
                .layer
                .and_then(|layer| Self::effect_of(&cx.world.arena, layer))
        {
            let stroke = Stroke::new(marks::hair(theme), theme.border_active);
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
                painter.circle_filled(screen(end), marks::knob(theme), theme.border_active);
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
                theme,
            );
        }
        // A curve ruler being placed, or a ruler being drawn with the pen,
        // in the color rulers have.
        let violet = theme.focus();
        if !self.curve_points.is_empty() {
            let mut points = self.curve_points.clone();
            if let Some(pointer) = self.pointer {
                let at = view.to_picture(viewport, size, pointer);
                points.push([at.x, at.y]);
            }
            let path = crate::rulers::Path::curve(&points);
            let line: Vec<Pos2> = path.points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(marks::firm(theme), violet)));
            for point in &self.curve_points {
                painter.circle_stroke(
                    screen(*point),
                    marks::knob(theme),
                    Stroke::new(marks::firm(theme), violet),
                );
            }
        }
        if let Some(Gesture::RulerPen { points }) = &self.gesture {
            let line: Vec<Pos2> = points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(marks::firm(theme), violet)));
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
                        let rect =
                            Rect::from_center_size(at, Vec2::splat(marks::control_point(theme)));
                        if line.is_corner(index) {
                            painter.rect_filled(rect, egui::CornerRadius::ZERO, violet);
                        } else {
                            painter.rect_filled(rect, egui::CornerRadius::ZERO, Color32::WHITE);
                            painter.rect_stroke(
                                rect,
                                egui::CornerRadius::ZERO,
                                Stroke::new(marks::hair(theme), violet),
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
                painter.add(Shape::line(points, Stroke::new(marks::firm(theme), violet)));
            }
        }
        if tool == Tool::Operation
            && let Some(folder) = doc
                .layer
                .filter(|layer| crate::camera::is_camera(&cx.world.arena, *layer))
        {
            // The camera's frame on the layers it sees, with its handles
            // and the way it goes.
            let arena = &cx.world.arena;
            let (frame, time) = {
                let rate = crate::anim::timeline(arena, doc.picture)
                    .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
                (doc.frame, crate::camera::time_of(doc.frame, rate))
            };
            let camera = match self.camera_pending {
                Some((pending, at, camera)) if pending == folder && at == frame => camera,
                _ => crate::camera::Camera::of(arena, folder, Some(time), size),
            };
            let grip = match &self.gesture {
                Some(Gesture::Camera { grip, .. }) => Some(*grip),
                _ => None,
            };
            let name = doc::layers(arena, doc.picture)
                .into_iter()
                .find(|info| info.id == folder)
                .map(|info| info.name)
                .unwrap_or_default();
            camera_marks(
                &painter,
                &CameraView {
                    arena,
                    folder,
                    camera,
                    time,
                    size,
                    grip,
                    name: &name,
                },
                &screen,
                theme,
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
                        if lit {
                            marks::heavy(theme)
                        } else {
                            marks::hair(theme)
                        },
                        if lit {
                            Color32::from_rgb(255, 170, 40)
                        } else {
                            Color32::from_rgba_unmultiplied(80, 160, 255, 150)
                        },
                    ),
                );
                painter.circle_filled(
                    a,
                    if lit {
                        marks::ring_small(theme)
                    } else {
                        marks::knob_small(theme)
                    },
                    if lit {
                        Color32::from_rgb(255, 170, 40)
                    } else {
                        Color32::from_rgb(80, 160, 255)
                    },
                );
            }
        }
        if tool == Tool::Operation {
            // The picked vector lines, their box and the loop being drawn.
            self.lines_overlay(&painter, &screen, &cx.world.arena, theme);
        }
        if response.hovered()
            && (tool.paints()
                || tool == Tool::Blend
                || (tool == Tool::CorrectLine && matches!(mode, 2 | 3 | 4 | 6)))
            && let Some(pointer) = self.pointer
        {
            let radius =
                (self.toolbox.current().tip.size * 0.5 * view.zoom).max(marks::speck_mid(theme));
            // Preferences, Cursor: the brush's outline, a crosshair or a dot.
            let shape = cx
                .rice()
                .get_str("sue_paint.brush_cursor")
                .unwrap_or("brush");
            let dark = Stroke::new(marks::bold(theme), Color32::from_black_alpha(140));
            let light = Stroke::new(marks::hair(theme), Color32::from_white_alpha(220));
            match shape {
                "crosshair" => {
                    for stroke in [dark, light] {
                        painter.line_segment(
                            [
                                pointer - Vec2::X * marks::arm(theme),
                                pointer + Vec2::X * marks::arm(theme),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                pointer - Vec2::Y * marks::arm(theme),
                                pointer + Vec2::Y * marks::arm(theme),
                            ],
                            stroke,
                        );
                    }
                }
                "dot" => {
                    painter.circle_filled(
                        pointer,
                        marks::speck_big(theme),
                        Color32::from_black_alpha(160),
                    );
                    painter.circle_filled(pointer, marks::speck_mid(theme), Color32::WHITE);
                }
                _ => {
                    painter.circle_stroke(pointer, radius, dark);
                    painter.circle_stroke(pointer, radius, light);
                    painter.circle_filled(pointer, marks::speck(theme), Color32::WHITE);
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

/// Draws `rulers` (picture pixels, `screen` maps them) in the theme's
/// accent, faint while their snapping (`[special, linear]`) is off,
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
    theme: &Theme,
) {
    use crate::rulers::Special;
    // Sue's own color for rulers, the theme's accent (rule 10 of the
    // 0.0.8 plan: no color matched to Clip Studio Paint's).
    let violet = theme.focus();
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
                Stroke::new(marks::firm(theme), special),
                8.0,
                5.0,
            ));
        }
        painter.circle_stroke(
            screen(center),
            marks::ring_small(theme),
            Stroke::new(marks::firm(theme), special),
        );
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
        let faint = Stroke::new(marks::hair(theme), special.gamma_multiply(0.4));
        for point in &perspective.points {
            for target in &rim {
                painter.line_segment([screen(*point), screen(*target)], faint);
            }
        }
        let (origin, direction) = perspective.eye_level();
        across(origin, direction, Stroke::new(marks::firm(theme), special));
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
            painter.circle_stroke(
                screen(*point),
                marks::ring_small(theme),
                Stroke::new(marks::firm(theme), special),
            );
            painter.circle_filled(screen(*point), marks::speck_mid(theme), special);
        }
        if let Some(at) = rulers.eye_handle(size) {
            painter.rect_stroke(
                Rect::from_center_size(screen(at), Vec2::splat(marks::square(theme))),
                egui::CornerRadius::ZERO,
                Stroke::new(marks::firm(theme), special),
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
                    Stroke::new(marks::hair(theme), special),
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
                    Stroke::new(marks::hair(theme), special),
                );
            }
            painter.circle_stroke(
                screen(center),
                marks::ring_small(theme),
                Stroke::new(marks::firm(theme), special),
            );
        }
        Some(Special::Concentric(center)) => {
            for step in 1..=12 {
                let radius = step as f32 * diagonal / 12.0 * zoom;
                painter.circle_stroke(
                    screen(center),
                    radius,
                    Stroke::new(marks::hair(theme), special),
                );
            }
            painter.circle_stroke(
                screen(center),
                marks::ring_small(theme),
                Stroke::new(marks::firm(theme), special),
            );
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
        painter.add(Shape::line(points, Stroke::new(marks::firm(theme), linear)));
    }
    if let Some([a, b]) = rulers.linear {
        painter.line_segment(
            [screen(a), screen(b)],
            Stroke::new(marks::bold(theme), linear),
        );
        for end in [a, b] {
            painter.circle_filled(screen(end), marks::knob_small(theme), linear);
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
            Stroke::new(marks::firm(theme), violet),
            8.0,
            5.0,
        ));
    }
}

/// What the camera's marks are drawn from.
struct CameraView<'a> {
    arena: &'a sue_core::Arena,
    folder: sue_core::EntityId,
    /// Where it stands at the frame shown (or as it is being dragged).
    camera: crate::camera::Camera,
    time: sue_core::Time,
    size: [u32; 2],
    /// The handle being dragged.
    grip: Option<crate::transform_ui::FrameGrip>,
    name: &'a str,
}

/// The camera's frame on the canvas: faint guides from the canvas's
/// corners to the frame's, the path its middle takes through its keys and
/// the frames at the keys before and after this one, the frame with its
/// corner and edge handles, the turning handle on its stalk, its middle,
/// and its name under its corner.
fn camera_marks(
    painter: &egui::Painter,
    view: &CameraView<'_>,
    screen: &dyn Fn([f32; 2]) -> Pos2,
    theme: &Theme,
) {
    use super::marks;
    use crate::transform_ui::FrameGrip;
    let (w, h) = (view.size[0] as f32, view.size[1] as f32);
    let quad = view.camera.frame(view.size).map(screen);
    let canvas = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(screen);
    for corner in 0..4 {
        marks::faint(painter, vec![canvas[corner], quad[corner]], false, theme);
    }
    let keys = crate::camera::keys(view.arena, view.folder);
    let at = |time: sue_core::Time| {
        crate::camera::Camera::of(view.arena, view.folder, Some(time), view.size)
    };
    if keys.len() > 1 {
        let path: Vec<Pos2> = keys.iter().map(|time| screen(at(*time).center)).collect();
        marks::faint(painter, path.clone(), false, theme);
        for point in path {
            marks::dot(painter, point, false, theme);
        }
    }
    let before = keys.iter().rev().find(|time| **time < view.time);
    let after = keys.iter().find(|time| **time > view.time);
    for time in [before, after].into_iter().flatten() {
        let frame = at(*time).frame(view.size).map(screen);
        marks::faint(painter, frame.to_vec(), true, theme);
    }
    marks::guide(painter, quad.to_vec(), true, theme);
    for (corner, at) in quad.iter().enumerate() {
        marks::handle(
            painter,
            *at,
            view.grip == Some(FrameGrip::Corner(corner)),
            theme,
        );
    }
    for edge in 0..4 {
        let (a, b) = (quad[edge], quad[(edge + 1) % 4]);
        marks::handle(
            painter,
            pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
            view.grip == Some(FrameGrip::Edge(edge)),
            theme,
        );
    }
    let (from, stalk) = crate::transform_ui::stalk_of(&quad, marks::STALK);
    for (corner, at) in quad.iter().enumerate() {
        crate::launchers::note_mark(painter.ctx(), &format!("camera.corner.{corner}"), *at);
    }
    crate::launchers::note_mark(painter.ctx(), "camera.stalk", stalk);
    crate::launchers::note_mark(painter.ctx(), "camera.middle", screen(view.camera.center));
    marks::stalk(
        painter,
        from,
        stalk,
        view.grip == Some(FrameGrip::Turn),
        theme,
    );
    marks::pivot(
        painter,
        screen(view.camera.center),
        view.grip == Some(FrameGrip::Inside),
        theme,
    );
    if !view.name.is_empty() {
        let corner = quad
            .iter()
            .copied()
            .max_by(|a, b| (a.y - a.x).total_cmp(&(b.y - b.x)))
            .unwrap_or(quad[3]);
        marks::tag(
            painter,
            corner + egui::Vec2::splat(theme.space(sue_wm::ui::Space::S1)),
            view.name,
            theme,
        );
    }
}
