//! Line icons drawn with the painter, so they stay crisp at any interface
//! scale and take the theme's color.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Every icon Sue Paint draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Zoom,
    Hand,
    Operation,
    MoveLayer,
    Select,
    AutoSelect,
    Eyedropper,
    Pen,
    Pencil,
    Brush,
    Airbrush,
    Decoration,
    Eraser,
    Blend,
    Fill,
    Gradient,
    Figure,
    Frame,
    Ruler,
    Text,
    Balloon,
    CorrectLine,
    New,
    Open,
    Save,
    Undo,
    Redo,
    Clear,
    ClearOutside,
    FillSelection,
    Deselect,
    FlipH,
    FlipV,
    RotateLeft,
    RotateRight,
    Reset,
    ZoomIn,
    ZoomOut,
    Fit,
    Actual,
    Eye,
    Lock,
    LockAlpha,
    Clip,
    Draft,
    Reference,
    Mask,
    NewLayer,
    NewVector,
    NewFolder,
    Duplicate,
    MergeDown,
    Delete,
    Paper,
    Swap,
    Play,
    Stop,
    First,
    Last,
    Prev,
    Next,
    Loop,
    Canvas,
    Up,
    Down,
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
    /// A magnifier with a line: search.
    Search,
    /// A question mark in a ring: help.
    Help,
    /// A wrench: settings.
    Wrench,
    /// An arrow into a tray: import.
    Import,
    /// An arrow out of a tray: export.
    Export,
    /// A plus: add.
    Add,
    /// Two arrows in a circle: replace.
    Replace,
    /// Lines with dots: a list.
    List,
    /// A sheet in a frame: a cel.
    Cel,
    /// Two overlapping sheets: onion skin.
    Onion,
    /// A pencil over a line: edit.
    Edit,
    /// A minus: take away or collapse.
    Minus,
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
    let width = (side / 13.0).clamp(1.0, 2.0);
    let stroke = Stroke::new(width, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            stroke,
        ));
    };
    let closed = |points: &[(f32, f32)]| {
        painter.add(Shape::closed_line(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            stroke,
        ));
    };
    let filled = |points: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let circle = |x: f32, y: f32, r: f32| painter.circle_stroke(p(x, y), r * side, stroke);
    let dot = |x: f32, y: f32, r: f32| painter.circle_filled(p(x, y), r * side, color);
    let curve = |x: f32, y: f32, r: f32, from: f32, to: f32| {
        painter.add(Shape::line(arc(p(x, y), r * side, from, to), stroke));
    };
    let dashed_box = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let corners = [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1), p(x0, y0)];
        for pair in corners.windows(2) {
            painter.add(Shape::dashed_line(
                &[pair[0], pair[1]],
                stroke,
                side * 0.09,
                side * 0.07,
            ));
        }
    };
    let magnifier = |plus: Option<bool>| {
        circle(0.42, 0.42, 0.25);
        line(&[(0.6, 0.6), (0.86, 0.86)]);
        if let Some(plus) = plus {
            line(&[(0.3, 0.42), (0.54, 0.42)]);
            if plus {
                line(&[(0.42, 0.3), (0.42, 0.54)]);
            }
        }
    };
    let page = |plus: bool| {
        closed(&[
            (0.25, 0.12),
            (0.6, 0.12),
            (0.78, 0.3),
            (0.78, 0.88),
            (0.25, 0.88),
        ]);
        line(&[(0.6, 0.12), (0.6, 0.3), (0.78, 0.3)]);
        if plus {
            line(&[(0.38, 0.6), (0.64, 0.6)]);
            line(&[(0.51, 0.47), (0.51, 0.73)]);
        }
    };
    let folder = |plus: bool| {
        closed(&[
            (0.12, 0.25),
            (0.4, 0.25),
            (0.48, 0.34),
            (0.88, 0.34),
            (0.88, 0.8),
            (0.12, 0.8),
        ]);
        if plus {
            line(&[(0.38, 0.57), (0.62, 0.57)]);
            line(&[(0.5, 0.45), (0.5, 0.69)]);
        }
    };
    let turn = |left: bool| {
        if left {
            curve(0.5, 0.55, 0.28, 200.0, 470.0);
            filled(&[(0.12, 0.42), (0.34, 0.42), (0.2, 0.6)]);
        } else {
            curve(0.5, 0.55, 0.28, -290.0, -20.0);
            filled(&[(0.88, 0.42), (0.66, 0.42), (0.8, 0.6)]);
        }
    };
    let lock = || {
        painter.rect_stroke(
            Rect::from_min_max(p(0.24, 0.45), p(0.76, 0.86)),
            side * 0.05,
            stroke,
            egui::StrokeKind::Middle,
        );
        curve(0.5, 0.45, 0.17, 180.0, 360.0);
    };
    match icon {
        Icon::Zoom => magnifier(None),
        Icon::ZoomIn => magnifier(Some(true)),
        Icon::ZoomOut => magnifier(Some(false)),
        Icon::Hand => {
            for (x, top) in [(0.33, 0.3), (0.45, 0.18), (0.57, 0.2), (0.69, 0.3)] {
                line(&[(x, top), (x, 0.55)]);
            }
            line(&[(0.21, 0.48), (0.33, 0.62)]);
            painter.add(Shape::line(
                arc(p(0.51, 0.62), 0.2 * side, 0.0, 180.0),
                stroke,
            ));
            line(&[(0.69, 0.55), (0.71, 0.62)]);
        }
        Icon::Operation => closed(&[
            (0.3, 0.12),
            (0.3, 0.8),
            (0.45, 0.66),
            (0.57, 0.9),
            (0.66, 0.85),
            (0.54, 0.62),
            (0.74, 0.62),
        ]),
        Icon::MoveLayer => {
            line(&[(0.5, 0.12), (0.5, 0.88)]);
            line(&[(0.12, 0.5), (0.88, 0.5)]);
            for (a, b, c) in [
                ((0.38, 0.24), (0.5, 0.12), (0.62, 0.24)),
                ((0.38, 0.76), (0.5, 0.88), (0.62, 0.76)),
                ((0.24, 0.38), (0.12, 0.5), (0.24, 0.62)),
                ((0.76, 0.38), (0.88, 0.5), (0.76, 0.62)),
            ] {
                line(&[a, b, c]);
            }
        }
        Icon::Select => dashed_box(0.16, 0.22, 0.84, 0.78),
        Icon::AutoSelect => {
            line(&[(0.18, 0.86), (0.62, 0.42)]);
            line(&[(0.74, 0.14), (0.74, 0.4)]);
            line(&[(0.61, 0.27), (0.87, 0.27)]);
            line(&[(0.65, 0.18), (0.83, 0.36)]);
        }
        Icon::Eyedropper => {
            line(&[(0.16, 0.86), (0.2, 0.72), (0.56, 0.36)]);
            line(&[(0.28, 0.84), (0.64, 0.48)]);
            line(&[(0.16, 0.86), (0.28, 0.84)]);
            dot(0.7, 0.3, 0.13);
        }
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
        Icon::Pencil => {
            closed(&[(0.22, 0.64), (0.66, 0.2), (0.82, 0.36), (0.38, 0.8)]);
            filled(&[(0.22, 0.64), (0.38, 0.8), (0.14, 0.88)]);
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
        Icon::Airbrush => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.3, 0.4), p(0.58, 0.88)),
                side * 0.05,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.38, 0.4), (0.38, 0.28), (0.5, 0.28), (0.5, 0.4)]);
            for (x, y) in [
                (0.68, 0.18),
                (0.78, 0.26),
                (0.68, 0.32),
                (0.84, 0.14),
                (0.86, 0.36),
            ] {
                dot(x, y, 0.035);
            }
        }
        Icon::Decoration => closed(&[
            (0.5, 0.12),
            (0.6, 0.38),
            (0.88, 0.38),
            (0.66, 0.56),
            (0.74, 0.84),
            (0.5, 0.68),
            (0.26, 0.84),
            (0.34, 0.56),
            (0.12, 0.38),
            (0.4, 0.38),
        ]),
        Icon::Eraser => {
            closed(&[
                (0.14, 0.6),
                (0.52, 0.22),
                (0.86, 0.56),
                (0.5, 0.86),
                (0.36, 0.82),
            ]);
            line(&[(0.33, 0.41), (0.67, 0.75)]);
        }
        Icon::Blend => {
            curve(0.5, 0.62, 0.22, -20.0, 200.0);
            line(&[(0.29, 0.55), (0.5, 0.14), (0.71, 0.55)]);
        }
        Icon::Fill => {
            closed(&[(0.18, 0.46), (0.48, 0.16), (0.78, 0.46), (0.48, 0.76)]);
            line(&[(0.18, 0.46), (0.78, 0.46)]);
            filled(&[(0.82, 0.58), (0.88, 0.76), (0.76, 0.76)]);
            dot(0.82, 0.78, 0.06);
        }
        Icon::Gradient => {
            let r = Rect::from_min_max(p(0.14, 0.24), p(0.86, 0.76));
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(r.left_top(), color);
            mesh.colored_vertex(r.right_top(), Color32::TRANSPARENT);
            mesh.colored_vertex(r.right_bottom(), Color32::TRANSPARENT);
            mesh.colored_vertex(r.left_bottom(), color);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(Shape::mesh(mesh));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Icon::Figure => {
            line(&[(0.14, 0.86), (0.5, 0.5)]);
            circle(0.68, 0.32, 0.18);
            painter.rect_stroke(
                Rect::from_min_max(p(0.12, 0.14), p(0.4, 0.42)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Frame => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.14, 0.14), p(0.86, 0.86)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.14, 0.46), (0.86, 0.4)]);
            line(&[(0.5, 0.43), (0.46, 0.86)]);
        }
        Icon::Ruler => {
            closed(&[(0.1, 0.62), (0.62, 0.1), (0.9, 0.38), (0.38, 0.9)]);
            for t in [0.25, 0.4, 0.55, 0.7] {
                let (x, y) = (0.1 + 0.52 * t / 0.85 + 0.05, 0.62 - 0.52 * t / 0.85 - 0.05);
                line(&[(x, y), (x + 0.08, y + 0.08)]);
            }
        }
        Icon::Text => {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(side * 0.78),
                color,
            );
        }
        Icon::Balloon => {
            painter.add(Shape::closed_line(
                arc(p(0.5, 0.44), 0.32 * side, 0.0, 360.0),
                stroke,
            ));
            line(&[(0.36, 0.72), (0.26, 0.9), (0.5, 0.75)]);
        }
        Icon::CorrectLine => {
            painter.add(Shape::line(
                (0..=16)
                    .map(|step| {
                        let t = step as f32 / 16.0;
                        p(
                            0.14 + 0.72 * t,
                            0.62 - 0.3 * (t * std::f32::consts::PI).sin(),
                        )
                    })
                    .collect(),
                stroke,
            ));
            for (x, y) in [(0.14, 0.62), (0.5, 0.32), (0.86, 0.62)] {
                painter.rect_filled(
                    Rect::from_center_size(p(x, y), vec2(side * 0.12, side * 0.12)),
                    0.0,
                    color,
                );
            }
        }
        Icon::New | Icon::NewLayer => page(true),
        Icon::Paper => {
            painter.rect_filled(
                Rect::from_min_max(p(0.22, 0.14), p(0.78, 0.86)),
                side * 0.03,
                color,
            );
        }
        Icon::NewVector => {
            page(false);
            curve(0.5, 0.75, 0.16, 200.0, 340.0);
        }
        Icon::Open => folder(false),
        Icon::NewFolder => folder(true),
        Icon::Save => {
            closed(&[
                (0.16, 0.16),
                (0.72, 0.16),
                (0.84, 0.28),
                (0.84, 0.84),
                (0.16, 0.84),
            ]);
            painter.rect_stroke(
                Rect::from_min_max(p(0.3, 0.16), p(0.64, 0.38)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.28, 0.56), p(0.72, 0.84)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Undo => {
            curve(0.52, 0.56, 0.26, -90.0, 90.0);
            line(&[(0.52, 0.82), (0.3, 0.82)]);
            line(&[(0.52, 0.3), (0.18, 0.3)]);
            filled(&[(0.14, 0.3), (0.3, 0.18), (0.3, 0.42)]);
        }
        Icon::Redo => {
            curve(0.48, 0.56, 0.26, 90.0, 270.0);
            line(&[(0.48, 0.82), (0.7, 0.82)]);
            line(&[(0.48, 0.3), (0.82, 0.3)]);
            filled(&[(0.86, 0.3), (0.7, 0.18), (0.7, 0.42)]);
        }
        Icon::Clear => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.18, 0.18), p(0.82, 0.82)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.34, 0.34), (0.66, 0.66)]);
            line(&[(0.66, 0.34), (0.34, 0.66)]);
        }
        Icon::ClearOutside => {
            dashed_box(0.3, 0.3, 0.7, 0.7);
            line(&[(0.1, 0.1), (0.24, 0.24)]);
            line(&[(0.9, 0.1), (0.76, 0.24)]);
            line(&[(0.1, 0.9), (0.24, 0.76)]);
            line(&[(0.9, 0.9), (0.76, 0.76)]);
        }
        Icon::FillSelection => {
            dashed_box(0.16, 0.16, 0.84, 0.84);
            painter.rect_filled(Rect::from_min_max(p(0.3, 0.3), p(0.7, 0.7)), 0.0, color);
        }
        Icon::Deselect => {
            dashed_box(0.16, 0.16, 0.84, 0.84);
            line(&[(0.1, 0.9), (0.9, 0.1)]);
        }
        Icon::FlipH => {
            line(&[(0.5, 0.1), (0.5, 0.9)]);
            closed(&[(0.42, 0.24), (0.42, 0.76), (0.12, 0.76)]);
            filled(&[(0.58, 0.24), (0.88, 0.76), (0.58, 0.76)]);
        }
        Icon::FlipV => {
            line(&[(0.1, 0.5), (0.9, 0.5)]);
            closed(&[(0.24, 0.42), (0.76, 0.42), (0.76, 0.12)]);
            filled(&[(0.24, 0.58), (0.76, 0.58), (0.76, 0.88)]);
        }
        Icon::RotateLeft => turn(true),
        Icon::RotateRight => turn(false),
        Icon::Reset => {
            curve(0.5, 0.5, 0.3, -60.0, 240.0);
            filled(&[(0.62, 0.16), (0.82, 0.2), (0.7, 0.36)]);
        }
        Icon::Fit => {
            for (a, b, c) in [
                ((0.14, 0.36), (0.14, 0.14), (0.36, 0.14)),
                ((0.64, 0.14), (0.86, 0.14), (0.86, 0.36)),
                ((0.86, 0.64), (0.86, 0.86), (0.64, 0.86)),
                ((0.36, 0.86), (0.14, 0.86), (0.14, 0.64)),
            ] {
                line(&[a, b, c]);
            }
        }
        Icon::Actual => {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "1:1",
                egui::FontId::proportional(side * 0.5),
                color,
            );
        }
        Icon::Eye => {
            painter.add(Shape::line(
                arc(p(0.5, 0.86), 0.5 * side, 223.0, 317.0),
                stroke,
            ));
            painter.add(Shape::line(
                arc(p(0.5, 0.14), 0.5 * side, 43.0, 137.0),
                stroke,
            ));
            dot(0.5, 0.5, 0.11);
        }
        Icon::Lock => lock(),
        Icon::LockAlpha => {
            lock();
            for (x, y) in [
                (0.32, 0.52),
                (0.5, 0.52),
                (0.41, 0.66),
                (0.59, 0.66),
                (0.32, 0.78),
                (0.5, 0.78),
            ] {
                painter.rect_filled(
                    Rect::from_min_size(p(x, y), vec2(side * 0.09, side * 0.09)),
                    0.0,
                    color,
                );
            }
        }
        Icon::Clip => {
            line(&[(0.3, 0.14), (0.3, 0.7), (0.72, 0.7)]);
            filled(&[(0.86, 0.7), (0.7, 0.58), (0.7, 0.82)]);
        }
        Icon::Draft => {
            closed(&[(0.22, 0.64), (0.66, 0.2), (0.82, 0.36), (0.38, 0.8)]);
            for x in [0.14, 0.32, 0.5, 0.68] {
                line(&[(x, 0.92), (x + 0.1, 0.92)]);
            }
        }
        Icon::Reference => {
            circle(0.5, 0.5, 0.3);
            dot(0.5, 0.5, 0.1);
        }
        Icon::Mask => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.16, 0.16), p(0.84, 0.84)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            dot(0.5, 0.5, 0.2);
        }
        Icon::Duplicate => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.14, 0.14), p(0.62, 0.62)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.38, 0.38), p(0.86, 0.86)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::MergeDown => {
            line(&[(0.5, 0.12), (0.5, 0.6)]);
            filled(&[(0.34, 0.5), (0.66, 0.5), (0.5, 0.7)]);
            line(&[(0.16, 0.84), (0.84, 0.84)]);
        }
        Icon::Delete => {
            line(&[(0.18, 0.26), (0.82, 0.26)]);
            line(&[(0.4, 0.26), (0.4, 0.16), (0.6, 0.16), (0.6, 0.26)]);
            closed(&[(0.26, 0.26), (0.74, 0.26), (0.68, 0.88), (0.32, 0.88)]);
            line(&[(0.44, 0.4), (0.44, 0.76)]);
            line(&[(0.56, 0.4), (0.56, 0.76)]);
        }
        Icon::Swap => {
            curve(0.24, 0.76, 0.52, -90.0, 0.0);
            filled(&[(0.24, 0.12), (0.36, 0.24), (0.24, 0.36)]);
            filled(&[(0.64, 0.76), (0.88, 0.76), (0.76, 0.88)]);
        }
        Icon::Play => filled(&[(0.3, 0.18), (0.82, 0.5), (0.3, 0.82)]),
        Icon::Stop => {
            painter.rect_filled(Rect::from_min_max(p(0.26, 0.26), p(0.74, 0.74)), 0.0, color);
        }
        Icon::First => {
            line(&[(0.24, 0.2), (0.24, 0.8)]);
            filled(&[(0.8, 0.2), (0.8, 0.8), (0.3, 0.5)]);
        }
        Icon::Last => {
            line(&[(0.76, 0.2), (0.76, 0.8)]);
            filled(&[(0.2, 0.2), (0.7, 0.5), (0.2, 0.8)]);
        }
        Icon::Prev => {
            filled(&[(0.5, 0.22), (0.5, 0.78), (0.14, 0.5)]);
            filled(&[(0.86, 0.22), (0.86, 0.78), (0.5, 0.5)]);
        }
        Icon::Next => {
            filled(&[(0.14, 0.22), (0.5, 0.5), (0.14, 0.78)]);
            filled(&[(0.5, 0.22), (0.86, 0.5), (0.5, 0.78)]);
        }
        Icon::Loop => {
            curve(0.5, 0.5, 0.3, 20.0, 160.0);
            curve(0.5, 0.5, 0.3, 200.0, 340.0);
            filled(&[(0.2, 0.36), (0.12, 0.56), (0.32, 0.54)]);
            filled(&[(0.8, 0.64), (0.88, 0.44), (0.68, 0.46)]);
        }
        Icon::Canvas => {
            for x in [0.22, 0.5, 0.78] {
                for y in [0.22, 0.5, 0.78] {
                    dot(x, y, 0.06);
                }
            }
        }
        Icon::Up => line(&[(0.22, 0.64), (0.5, 0.34), (0.78, 0.64)]),
        Icon::Down => line(&[(0.22, 0.36), (0.5, 0.66), (0.78, 0.36)]),
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
        Icon::Search => {
            circle(0.42, 0.42, 0.24);
            line(&[(0.6, 0.6), (0.84, 0.84)]);
        }
        Icon::Help => {
            circle(0.5, 0.5, 0.36);
            curve(0.5, 0.4, 0.12, 180.0, 400.0);
            line(&[(0.52, 0.52), (0.5, 0.6)]);
            dot(0.5, 0.7, 0.035);
        }
        Icon::Wrench => {
            curve(0.66, 0.34, 0.18, 60.0, 330.0);
            line(&[(0.54, 0.46), (0.2, 0.8)]);
            line(&[(0.27, 0.86), (0.14, 0.73)]);
        }
        Icon::Import => {
            line(&[(0.5, 0.14), (0.5, 0.6)]);
            line(&[(0.32, 0.44), (0.5, 0.62), (0.68, 0.44)]);
            line(&[(0.16, 0.64), (0.16, 0.84), (0.84, 0.84), (0.84, 0.64)]);
        }
        Icon::Export => {
            line(&[(0.5, 0.62), (0.5, 0.16)]);
            line(&[(0.32, 0.32), (0.5, 0.14), (0.68, 0.32)]);
            line(&[(0.16, 0.64), (0.16, 0.84), (0.84, 0.84), (0.84, 0.64)]);
        }
        Icon::Add => {
            line(&[(0.5, 0.2), (0.5, 0.8)]);
            line(&[(0.2, 0.5), (0.8, 0.5)]);
        }
        Icon::Replace => {
            curve(0.5, 0.5, 0.3, 200.0, 340.0);
            curve(0.5, 0.5, 0.3, 20.0, 160.0);
            filled(&[(0.78, 0.32), (0.86, 0.5), (0.68, 0.46)]);
            filled(&[(0.22, 0.68), (0.14, 0.5), (0.32, 0.54)]);
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
        Icon::Onion => {
            closed(&[(0.14, 0.14), (0.62, 0.14), (0.62, 0.62), (0.14, 0.62)]);
            closed(&[(0.38, 0.38), (0.86, 0.38), (0.86, 0.86), (0.38, 0.86)]);
        }
        Icon::Edit => {
            closed(&[
                (0.62, 0.16),
                (0.8, 0.34),
                (0.38, 0.76),
                (0.2, 0.8),
                (0.24, 0.58),
            ]);
            line(&[(0.16, 0.88), (0.84, 0.88)]);
        }
        Icon::Minus => line(&[(0.22, 0.5), (0.78, 0.5)]),
    }
}
