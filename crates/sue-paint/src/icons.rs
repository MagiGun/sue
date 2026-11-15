//! Icons drawn with the painter, so they stay crisp at any interface scale
//! and take the theme's color: two-tone, each shape outlined over a soft
//! fill of its own color, so they read as objects and not wire.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Every icon Sue Paint draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Pen,
    Brush,
    /// Three lines: a palette's menu.
    Menu,
    /// A ring of hues: the color wheel.
    Wheel,
    /// A grid of squares: the color set.
    Swatches,
    /// Stacked sheets: the layers.
    Layers,
    /// A frame with a box inside: the navigator.
    Navigator,
    /// Growing dots: the brush size.
    BrushSize,
    /// A wrench: settings.
    Wrench,
    /// Lines with dots: a list.
    List,
    /// A sheet in a frame: a cel.
    Cel,
    /// Brush strokes in a stack: the sub tools.
    SubTool,
    /// Sliders: a tool's settings.
    Sliders,
    /// Bars with knobs: the color sliders.
    ColorSlider,
    /// A square shaded from its corners: the intermediate colors.
    Intermediate,
    /// A grid shading off: the approximate colors.
    Approximate,
    /// A clock: the history.
    History,
    /// A cube: the materials.
    Material,
    /// A picture with a mountain: the sub view.
    SubView,
    /// An i in a ring: information.
    Info,
    /// A play button over lines: the auto actions.
    AutoAction,
    /// An open box: the item bank.
    ItemBank,
    /// A sheet with a spark: the layer's properties.
    LayerProperty,
    /// A strip of film: the timeline.
    Timeline,
}

/// Triangles filling the simple polygon `points` (ear clipping), for the
/// soft fill of a shape that need not be convex.
fn fill_polygon(points: &[Pos2]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let ccw = area > 0.0;
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut guard = 0;
    while left.len() > 3 && guard < n * n {
        guard += 1;
        let m = left.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (left[(i + m - 1) % m], left[i], left[(i + 1) % m]);
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            let turn = cross(a, b, c);
            if (turn > 0.0) != ccw || turn.abs() < 1e-6 {
                continue;
            }
            let inside = left.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let q = points[j];
                let (d1, d2, d3) = (cross(a, b, q), cross(b, c, q), cross(c, a, q));
                if ccw {
                    d1 > 0.0 && d2 > 0.0 && d3 > 0.0
                } else {
                    d1 < 0.0 && d2 < 0.0 && d3 < 0.0
                }
            });
            if inside {
                continue;
            }
            out.push([ia, ib, ic]);
            left.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if left.len() == 3 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = 14;
    (0..=steps)
        .map(|step| {
            let angle = (from + (to - from) * step as f32 / steps as f32).to_radians();
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

/// Draws `icon` inside `rect` in `color`.
#[allow(clippy::too_many_lines, reason = "one drawing per icon")]
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let area = Rect::from_center_size(rect.center(), vec2(side, side));
    let p = |x: f32, y: f32| pos2(area.left() + x * side, area.top() + y * side);
    let width = (side / 11.0).clamp(1.3, 2.6);
    let stroke = Stroke::new(width, color);
    // The soft fill under every outline.
    let soft = color.gamma_multiply(0.26);
    let fill = |points: Vec<Pos2>| {
        let mut mesh = egui::Mesh::default();
        for point in &points {
            mesh.colored_vertex(*point, soft);
        }
        for [a, b, c] in fill_polygon(&points) {
            mesh.add_triangle(a as u32, b as u32, c as u32);
        }
        painter.add(Shape::mesh(mesh));
    };
    let boxed = |rect: Rect, rounding: f32| {
        painter.rect_filled(rect, rounding, soft);
        painter.rect_stroke(rect, rounding, stroke, egui::StrokeKind::Middle);
    };
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            stroke,
        ));
    };
    let closed = |points: &[(f32, f32)]| {
        let points: Vec<Pos2> = points.iter().map(|(x, y)| p(*x, *y)).collect();
        fill(points.clone());
        painter.add(Shape::closed_line(points, stroke));
    };
    let filled = |points: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let circle = |x: f32, y: f32, r: f32| {
        painter.circle_filled(p(x, y), r * side, soft);
        painter.circle_stroke(p(x, y), r * side, stroke);
    };
    let dot = |x: f32, y: f32, r: f32| painter.circle_filled(p(x, y), r * side, color);
    let curve = |x: f32, y: f32, r: f32, from: f32, to: f32| {
        painter.add(Shape::line(arc(p(x, y), r * side, from, to), stroke));
    };
    match icon {
        Icon::Pen => {
            closed(&[
                (0.18, 0.84),
                (0.28, 0.58),
                (0.7, 0.16),
                (0.86, 0.32),
                (0.44, 0.74),
            ]);
            line(&[(0.18, 0.84), (0.36, 0.66)]);
        }
        Icon::Brush => {
            line(&[(0.5, 0.5), (0.86, 0.14)]);
            filled(&[
                (0.42, 0.44),
                (0.58, 0.6),
                (0.38, 0.76),
                (0.14, 0.88),
                (0.26, 0.6),
            ]);
        }
        Icon::Menu => {
            for y in [0.3, 0.5, 0.7] {
                line(&[(0.22, y), (0.78, y)]);
            }
        }
        Icon::Wheel => {
            circle(0.5, 0.5, 0.34);
            circle(0.5, 0.5, 0.18);
            for angle in [0.0_f32, 60.0, 120.0, 180.0, 240.0, 300.0] {
                let (sin, cos) = angle.to_radians().sin_cos();
                line(&[
                    (0.5 + cos * 0.18, 0.5 + sin * 0.18),
                    (0.5 + cos * 0.34, 0.5 + sin * 0.34),
                ]);
            }
        }
        Icon::Swatches => {
            for (x, y) in [(0.18, 0.18), (0.54, 0.18), (0.18, 0.54), (0.54, 0.54)] {
                painter.rect_filled(
                    Rect::from_min_max(p(x, y), p(x + 0.28, y + 0.28)),
                    0.0,
                    color,
                );
            }
        }
        Icon::Layers => {
            closed(&[(0.5, 0.18), (0.86, 0.36), (0.5, 0.54), (0.14, 0.36)]);
            line(&[(0.14, 0.52), (0.5, 0.7), (0.86, 0.52)]);
            line(&[(0.14, 0.66), (0.5, 0.84), (0.86, 0.66)]);
        }
        Icon::Navigator => {
            closed(&[(0.14, 0.2), (0.86, 0.2), (0.86, 0.8), (0.14, 0.8)]);
            closed(&[(0.4, 0.38), (0.66, 0.38), (0.66, 0.6), (0.4, 0.6)]);
        }
        Icon::BrushSize => {
            dot(0.24, 0.62, 0.05);
            dot(0.44, 0.58, 0.08);
            dot(0.7, 0.5, 0.14);
        }
        Icon::Wrench => {
            curve(0.66, 0.34, 0.18, 60.0, 330.0);
            line(&[(0.54, 0.46), (0.2, 0.8)]);
            line(&[(0.27, 0.86), (0.14, 0.73)]);
        }
        Icon::List => {
            for y in [0.28, 0.5, 0.72] {
                dot(0.2, y, 0.04);
                line(&[(0.34, y), (0.84, y)]);
            }
        }
        Icon::Cel => {
            closed(&[(0.16, 0.16), (0.84, 0.16), (0.84, 0.84), (0.16, 0.84)]);
            closed(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.7), (0.3, 0.7)]);
        }
        Icon::SubTool => {
            for (y, w) in [(0.28, 0.5), (0.5, 0.62), (0.72, 0.44)] {
                painter.add(Shape::line(
                    (0..=10)
                        .map(|step| {
                            let t = step as f32 / 10.0;
                            p(
                                0.16 + w * t * 1.1,
                                y - 0.06 * (t * std::f32::consts::PI).sin(),
                            )
                        })
                        .collect(),
                    Stroke::new(width * (1.0 + y), color),
                ));
            }
        }
        Icon::Sliders => {
            for (y, knob) in [(0.28, 0.66), (0.5, 0.34), (0.72, 0.56)] {
                line(&[(0.14, y), (0.86, y)]);
                boxed(
                    Rect::from_center_size(p(knob, y), vec2(side * 0.12, side * 0.2)),
                    side * 0.02,
                );
            }
        }
        Icon::ColorSlider => {
            for (x, knob) in [(0.28, 0.36), (0.5, 0.64), (0.72, 0.46)] {
                line(&[(x, 0.14), (x, 0.86)]);
                dot(x, knob, 0.08);
            }
        }
        Icon::Intermediate => {
            let r = Rect::from_min_max(p(0.16, 0.16), p(0.84, 0.84));
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(r.left_top(), color);
            mesh.colored_vertex(r.right_top(), color.gamma_multiply(0.45));
            mesh.colored_vertex(r.right_bottom(), color.gamma_multiply(0.1));
            mesh.colored_vertex(r.left_bottom(), color.gamma_multiply(0.6));
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(Shape::mesh(mesh));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Icon::Approximate => {
            for row in 0..3 {
                for column in 0..3 {
                    let (x, y) = (0.16 + column as f32 * 0.24, 0.16 + row as f32 * 0.24);
                    painter.rect_filled(
                        Rect::from_min_max(p(x, y), p(x + 0.2, y + 0.2)),
                        side * 0.02,
                        color.gamma_multiply(1.0 - (row + column) as f32 * 0.18),
                    );
                }
            }
        }
        Icon::History => {
            circle(0.5, 0.52, 0.34);
            line(&[(0.5, 0.3), (0.5, 0.52), (0.66, 0.62)]);
        }
        Icon::Material => {
            closed(&[(0.5, 0.14), (0.84, 0.32), (0.5, 0.5), (0.16, 0.32)]);
            closed(&[(0.16, 0.32), (0.5, 0.5), (0.5, 0.88), (0.16, 0.7)]);
            closed(&[(0.84, 0.32), (0.84, 0.7), (0.5, 0.88), (0.5, 0.5)]);
        }
        Icon::SubView => {
            boxed(Rect::from_min_max(p(0.12, 0.2), p(0.88, 0.8)), side * 0.04);
            filled(&[
                (0.2, 0.72),
                (0.42, 0.42),
                (0.58, 0.62),
                (0.66, 0.52),
                (0.8, 0.72),
            ]);
            dot(0.7, 0.36, 0.06);
        }
        Icon::Info => {
            circle(0.5, 0.5, 0.36);
            line(&[(0.5, 0.46), (0.5, 0.7)]);
            dot(0.5, 0.33, 0.045);
        }
        Icon::AutoAction => {
            for y in [0.3, 0.5, 0.7] {
                line(&[(0.14, y), (0.5, y)]);
            }
            filled(&[(0.6, 0.32), (0.88, 0.5), (0.6, 0.68)]);
        }
        Icon::ItemBank => {
            closed(&[(0.14, 0.42), (0.86, 0.42), (0.8, 0.86), (0.2, 0.86)]);
            line(&[(0.14, 0.42), (0.28, 0.18), (0.72, 0.18), (0.86, 0.42)]);
            line(&[(0.38, 0.58), (0.62, 0.58)]);
        }
        Icon::LayerProperty => {
            closed(&[(0.14, 0.22), (0.62, 0.22), (0.62, 0.86), (0.14, 0.86)]);
            filled(&[
                (0.76, 0.12),
                (0.8, 0.3),
                (0.94, 0.34),
                (0.8, 0.38),
                (0.76, 0.56),
                (0.72, 0.38),
                (0.58, 0.34),
                (0.72, 0.3),
            ]);
        }
        Icon::Timeline => {
            boxed(Rect::from_min_max(p(0.1, 0.26), p(0.9, 0.74)), 0.0);
            for x in [0.2, 0.36, 0.52, 0.68, 0.84] {
                painter.rect_filled(
                    Rect::from_center_size(p(x - 0.02, 0.33), vec2(side * 0.07, side * 0.05)),
                    0.0,
                    color,
                );
                painter.rect_filled(
                    Rect::from_center_size(p(x - 0.02, 0.67), vec2(side * 0.07, side * 0.05)),
                    0.0,
                    color,
                );
            }
        }
    }
}
