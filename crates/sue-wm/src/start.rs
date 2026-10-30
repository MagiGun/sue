//! Start pages: the page a studio opens on before any work is open, where
//! people start something new, find their projects and browse their
//! materials. A narrow rail of large buttons on the left, a row of icon
//! buttons at the top right, and in the main area a category list, a
//! search box, filter chips, a toolbar and a grid or list of cards.
//! Popups come up in the middle over a dimmed page, with a title, an X and
//! grouped rows.
//!
//! These are plain widgets drawn from the [`Theme`], sharp like the Hub's
//! boxes, so every studio can have a start page that looks like the
//! others. Each studio keeps its own state and decides what the pieces
//! do.

use egui::{
    Align2, Color32, CornerRadius, Id, Order, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, TextureId, Ui, UiBuilder, Vec2, pos2, vec2,
};

use crate::Theme;

/// A picture drawn for a button, a row or a card. Sue draws its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    /// A pen nib: start drawing.
    Draw,
    /// A stack of pictures: projects.
    Projects,
    /// A circle, a square and a triangle: materials.
    Materials,
    /// Two arrows, one each way: data transfers.
    Transfers,
    /// A question mark: help.
    Help,
    /// A gear: settings.
    Settings,
    /// A plus.
    Add,
    /// A bin.
    Delete,
    /// A pencil: rename.
    Rename,
    /// A heart: a favorite.
    Heart,
    /// A right chevron in a circle: details.
    Details,
    /// A right chevron.
    Chevron,
    /// A down chevron.
    ChevronDown,
    /// A magnifying glass.
    Search,
    /// Large cards.
    Grid,
    /// Small cards.
    GridSmall,
    /// Rows.
    List,
    /// Sorting, first to last.
    SortUp,
    /// Sorting, last to first.
    SortDown,
    /// An X.
    Close,
    /// A box with an arrow out of it: opens elsewhere.
    External,
    /// A crescent: dark mode.
    Moon,
    /// A folder.
    Folder,
    /// A wrench.
    Wrench,
    /// A tick.
    Check,
    /// A landscape in a frame: a picture.
    Picture,
    /// A brush.
    Brush,
    /// Crossed lines: a pattern.
    Pattern,
    /// Dots: a screentone.
    Tone,
    /// A cube: 3D.
    Cube,
    /// A clock: recently used.
    Clock,
    /// A tag: a label.
    Tag,
    /// An arrow into a box: moving to a label.
    Move,
    /// A book: tips and guides.
    Book,
    /// A speech bubble: feedback.
    Bubble,
    /// An envelope: contact.
    Mail,
    /// A grid of dots: everything.
    All,
}

fn line(painter: &Painter, points: &[Pos2], stroke: Stroke) {
    painter.add(Shape::line(points.to_vec(), stroke));
}

/// Draws `glyph` in `rect` in `color`.
pub fn glyph(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let side = rect.width().min(rect.height());
    let c = rect.center();
    let s = side / 2.0;
    let at = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new((side / 14.0).clamp(1.0, 2.5), color);
    match glyph {
        Glyph::Draw => {
            // A pen, nib down to the left.
            line(painter, &[at(-0.55, 0.55), at(-0.35, 0.05)], stroke);
            line(
                painter,
                &[
                    at(-0.55, 0.55),
                    at(-0.05, 0.35),
                    at(0.65, -0.35),
                    at(0.35, -0.65),
                    at(-0.35, 0.05),
                    at(-0.05, 0.35),
                ],
                stroke,
            );
            line(painter, &[at(0.2, -0.5), at(0.5, -0.2)], stroke);
        }
        Glyph::Projects => {
            painter.rect_stroke(
                Rect::from_min_max(at(-0.45, -0.3), at(0.65, 0.6)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
            line(
                painter,
                &[at(-0.65, 0.4), at(-0.65, -0.5), at(0.45, -0.5)],
                stroke,
            );
            line(
                painter,
                &[
                    at(-0.3, 0.45),
                    at(0.0, 0.1),
                    at(0.2, 0.3),
                    at(0.35, 0.15),
                    at(0.55, 0.45),
                ],
                stroke,
            );
        }
        Glyph::Materials => {
            painter.circle_stroke(at(-0.3, -0.3), 0.3 * s, stroke);
            painter.rect_stroke(
                Rect::from_min_max(at(0.05, -0.6), at(0.65, 0.0)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
            line(
                painter,
                &[at(-0.6, 0.65), at(-0.3, 0.1), at(0.0, 0.65), at(-0.6, 0.65)],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_min_max(at(0.15, 0.15), at(0.65, 0.65)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
        }
        Glyph::Transfers => {
            line(painter, &[at(-0.6, -0.25), at(0.6, -0.25)], stroke);
            line(
                painter,
                &[at(0.3, -0.55), at(0.6, -0.25), at(0.3, 0.05)],
                stroke,
            );
            line(painter, &[at(0.6, 0.3), at(-0.6, 0.3)], stroke);
            line(
                painter,
                &[at(-0.3, 0.0), at(-0.6, 0.3), at(-0.3, 0.6)],
                stroke,
            );
        }
        Glyph::Help => {
            painter.circle_stroke(c, 0.68 * s, stroke);
            line(
                painter,
                &[
                    at(-0.22, -0.2),
                    at(-0.15, -0.36),
                    at(0.0, -0.42),
                    at(0.16, -0.36),
                    at(0.22, -0.2),
                    at(0.15, -0.06),
                    at(0.0, 0.03),
                    at(0.0, 0.18),
                ],
                stroke,
            );
            painter.circle_filled(at(0.0, 0.37), stroke.width * 0.8, color);
        }
        Glyph::Settings => {
            let teeth = 8;
            let mut points = Vec::with_capacity(teeth * 4 + 1);
            for tooth in 0..teeth {
                let a = tooth as f32 / teeth as f32 * std::f32::consts::TAU;
                let w = std::f32::consts::TAU / teeth as f32;
                for (offset, radius) in [(-0.3, 0.48), (-0.18, 0.68), (0.18, 0.68), (0.3, 0.48)] {
                    let angle = a + offset * w;
                    points.push(at(angle.cos() * radius, angle.sin() * radius));
                }
            }
            points.push(points[0]);
            line(painter, &points, stroke);
            painter.circle_stroke(c, 0.2 * s, stroke);
        }
        Glyph::Add => {
            line(painter, &[at(-0.55, 0.0), at(0.55, 0.0)], stroke);
            line(painter, &[at(0.0, -0.55), at(0.0, 0.55)], stroke);
        }
        Glyph::Delete => {
            line(painter, &[at(-0.55, -0.4), at(0.55, -0.4)], stroke);
            line(
                painter,
                &[
                    at(-0.2, -0.4),
                    at(-0.15, -0.6),
                    at(0.15, -0.6),
                    at(0.2, -0.4),
                ],
                stroke,
            );
            line(
                painter,
                &[
                    at(-0.42, -0.4),
                    at(-0.32, 0.62),
                    at(0.32, 0.62),
                    at(0.42, -0.4),
                ],
                stroke,
            );
            line(painter, &[at(-0.12, -0.15), at(-0.1, 0.4)], stroke);
            line(painter, &[at(0.12, -0.15), at(0.1, 0.4)], stroke);
        }
        Glyph::Rename => {
            line(
                painter,
                &[
                    at(-0.55, 0.55),
                    at(-0.5, 0.25),
                    at(0.3, -0.55),
                    at(0.55, -0.3),
                    at(-0.25, 0.5),
                    at(-0.55, 0.55),
                ],
                stroke,
            );
            line(painter, &[at(0.15, -0.4), at(0.4, -0.15)], stroke);
        }
        Glyph::Heart => {
            let mut points = Vec::new();
            for step in 0..=32 {
                let t = step as f32 / 32.0 * std::f32::consts::TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos()
                    - 5.0 * (2.0 * t).cos()
                    - 2.0 * (3.0 * t).cos()
                    - (4.0 * t).cos();
                points.push(at(x / 26.0, -y / 26.0 + 0.08));
            }
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Glyph::Details => {
            painter.circle_stroke(c, 0.62 * s, stroke);
            line(
                painter,
                &[at(-0.1, -0.3), at(0.15, 0.0), at(-0.1, 0.3)],
                stroke,
            );
        }
        Glyph::Chevron => {
            line(
                painter,
                &[at(-0.15, -0.45), at(0.2, 0.0), at(-0.15, 0.45)],
                stroke,
            );
        }
        Glyph::ChevronDown => {
            line(
                painter,
                &[at(-0.45, -0.15), at(0.0, 0.2), at(0.45, -0.15)],
                stroke,
            );
        }
        Glyph::Search => {
            painter.circle_stroke(at(-0.12, -0.12), 0.4 * s, stroke);
            line(painter, &[at(0.18, 0.18), at(0.6, 0.6)], stroke);
        }
        Glyph::Grid => {
            for (x, y) in [(-0.6, -0.6), (0.08, -0.6), (-0.6, 0.08), (0.08, 0.08)] {
                painter.rect_filled(
                    Rect::from_min_size(at(x, y), Vec2::splat(0.52 * s)),
                    0.0,
                    color,
                );
            }
        }
        Glyph::GridSmall => {
            for row in 0..3 {
                for column in 0..3 {
                    let (x, y) = (-0.6 + column as f32 * 0.45, -0.6 + row as f32 * 0.45);
                    painter.rect_filled(
                        Rect::from_min_size(at(x, y), Vec2::splat(0.3 * s)),
                        0.0,
                        color,
                    );
                }
            }
        }
        Glyph::List => {
            for y in [-0.45, 0.0, 0.45] {
                line(painter, &[at(-0.6, y), at(0.6, y)], stroke);
            }
        }
        Glyph::SortUp | Glyph::SortDown => {
            let up = glyph == Glyph::SortUp;
            line(painter, &[at(-0.3, -0.6), at(-0.3, 0.6)], stroke);
            let tip = if up { -0.6 } else { 0.6 };
            line(
                painter,
                &[at(-0.6, tip * 0.5), at(-0.3, tip), at(0.0, tip * 0.5)],
                stroke,
            );
            for (index, width) in [0.15, 0.3, 0.45].into_iter().enumerate() {
                let y = -0.45 + index as f32 * 0.45;
                let width = if up { width } else { 0.6 - width };
                line(painter, &[at(0.15, y), at(0.15 + width, y)], stroke);
            }
        }
        Glyph::Close => {
            line(painter, &[at(-0.45, -0.45), at(0.45, 0.45)], stroke);
            line(painter, &[at(0.45, -0.45), at(-0.45, 0.45)], stroke);
        }
        Glyph::External => {
            line(
                painter,
                &[
                    at(0.0, -0.5),
                    at(-0.55, -0.5),
                    at(-0.55, 0.55),
                    at(0.5, 0.55),
                    at(0.5, 0.0),
                ],
                stroke,
            );
            line(painter, &[at(-0.05, 0.05), at(0.6, -0.6)], stroke);
            line(
                painter,
                &[at(0.15, -0.6), at(0.6, -0.6), at(0.6, -0.15)],
                stroke,
            );
        }
        Glyph::Moon => {
            let mut points = Vec::new();
            for step in 0..=24 {
                let a = 0.6 + step as f32 / 24.0 * 5.1;
                points.push(at(a.cos() * 0.6, a.sin() * 0.6));
            }
            for step in (0..=24).rev() {
                let a = 1.6 + step as f32 / 24.0 * 3.1;
                points.push(at(0.25 + a.cos() * 0.45, -0.15 + a.sin() * 0.45));
            }
            points.push(points[0]);
            line(painter, &points, stroke);
        }
        Glyph::Folder => {
            line(
                painter,
                &[
                    at(-0.65, 0.55),
                    at(-0.65, -0.5),
                    at(-0.2, -0.5),
                    at(-0.05, -0.32),
                    at(0.65, -0.32),
                    at(0.65, 0.55),
                    at(-0.65, 0.55),
                ],
                stroke,
            );
        }
        Glyph::Wrench => {
            line(painter, &[at(-0.55, 0.55), at(0.1, -0.1)], stroke);
            painter.circle_stroke(at(0.25, -0.25), 0.3 * s, stroke);
            line(
                painter,
                &[at(0.35, -0.35), at(0.6, -0.6)],
                Stroke::new(stroke.width * 2.0, color),
            );
        }
        Glyph::Check => {
            line(
                painter,
                &[at(-0.5, 0.0), at(-0.15, 0.4), at(0.55, -0.4)],
                stroke,
            );
        }
        Glyph::Picture => {
            painter.rect_stroke(
                Rect::from_min_max(at(-0.65, -0.5), at(0.65, 0.5)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
            line(
                painter,
                &[
                    at(-0.5, 0.4),
                    at(-0.15, 0.0),
                    at(0.1, 0.25),
                    at(0.3, 0.05),
                    at(0.55, 0.4),
                ],
                stroke,
            );
            painter.circle_filled(at(0.3, -0.22), 0.1 * s, color);
        }
        Glyph::Brush => {
            line(painter, &[at(0.6, -0.6), at(-0.05, 0.1)], stroke);
            let tip = [
                at(-0.05, 0.1),
                at(-0.25, -0.05),
                at(-0.55, 0.3),
                at(-0.6, 0.6),
                at(-0.25, 0.5),
                at(0.1, 0.25),
            ];
            painter.add(Shape::convex_polygon(tip.to_vec(), color, Stroke::NONE));
        }
        Glyph::Pattern => {
            painter.rect_stroke(
                Rect::from_min_max(at(-0.6, -0.6), at(0.6, 0.6)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
            // Diagonals y = x - offset, cut to the square.
            for offset in [-0.6_f32, -0.2, 0.2, 0.6] {
                let a = (offset - 0.6_f32).max(-0.6);
                let b = (offset + 0.6_f32).min(0.6);
                line(painter, &[at(a, a - offset), at(b, b - offset)], stroke);
            }
        }
        Glyph::Tone => {
            for row in 0..4 {
                for column in 0..4 {
                    let shift = if row % 2 == 0 { 0.0 } else { 0.17 };
                    let x = -0.55 + column as f32 * 0.34 + shift;
                    let y = -0.5 + row as f32 * 0.34;
                    if x < 0.65 {
                        painter.circle_filled(at(x, y), 0.08 * s, color);
                    }
                }
            }
        }
        Glyph::Cube => {
            line(
                painter,
                &[
                    at(0.0, -0.65),
                    at(0.6, -0.33),
                    at(0.6, 0.35),
                    at(0.0, 0.67),
                    at(-0.6, 0.35),
                    at(-0.6, -0.33),
                    at(0.0, -0.65),
                ],
                stroke,
            );
            line(
                painter,
                &[at(-0.6, -0.33), at(0.0, 0.0), at(0.6, -0.33)],
                stroke,
            );
            line(painter, &[at(0.0, 0.0), at(0.0, 0.67)], stroke);
        }
        Glyph::Clock => {
            painter.circle_stroke(c, 0.62 * s, stroke);
            line(
                painter,
                &[at(0.0, -0.4), at(0.0, 0.0), at(0.3, 0.2)],
                stroke,
            );
        }
        Glyph::Tag => {
            line(
                painter,
                &[
                    at(-0.6, -0.6),
                    at(0.05, -0.6),
                    at(0.65, 0.0),
                    at(0.0, 0.65),
                    at(-0.6, 0.05),
                    at(-0.6, -0.6),
                ],
                stroke,
            );
            painter.circle_filled(at(-0.28, -0.28), 0.1 * s, color);
        }
        Glyph::Move => {
            line(
                painter,
                &[at(-0.2, -0.6), at(0.6, -0.6), at(0.6, 0.6), at(-0.2, 0.6)],
                stroke,
            );
            line(painter, &[at(-0.65, 0.0), at(0.25, 0.0)], stroke);
            line(
                painter,
                &[at(-0.05, -0.3), at(0.25, 0.0), at(-0.05, 0.3)],
                stroke,
            );
        }
        Glyph::Book => {
            line(
                painter,
                &[
                    at(0.0, -0.4),
                    at(-0.3, -0.55),
                    at(-0.65, -0.5),
                    at(-0.65, 0.5),
                    at(-0.3, 0.45),
                    at(0.0, 0.6),
                    at(0.3, 0.45),
                    at(0.65, 0.5),
                    at(0.65, -0.5),
                    at(0.3, -0.55),
                    at(0.0, -0.4),
                    at(0.0, 0.6),
                ],
                stroke,
            );
        }
        Glyph::Bubble => {
            line(
                painter,
                &[
                    at(-0.6, -0.5),
                    at(0.6, -0.5),
                    at(0.6, 0.3),
                    at(-0.1, 0.3),
                    at(-0.4, 0.6),
                    at(-0.35, 0.3),
                    at(-0.6, 0.3),
                    at(-0.6, -0.5),
                ],
                stroke,
            );
        }
        Glyph::Mail => {
            painter.rect_stroke(
                Rect::from_min_max(at(-0.65, -0.45), at(0.65, 0.45)),
                0.0,
                stroke,
                StrokeKind::Middle,
            );
            line(
                painter,
                &[at(-0.65, -0.45), at(0.0, 0.1), at(0.65, -0.45)],
                stroke,
            );
        }
        Glyph::All => {
            for row in 0..3 {
                for column in 0..3 {
                    painter.circle_filled(
                        at(-0.45 + column as f32 * 0.45, -0.45 + row as f32 * 0.45),
                        0.09 * s,
                        color,
                    );
                }
            }
        }
    }
}

/// One button of a start page's rail.
#[derive(Clone, Debug)]
pub struct RailItem {
    /// Its picture.
    pub glyph: Glyph,
    /// What it says under the picture.
    pub label: String,
    /// `true` for the large round button at the top (Draw, Write).
    pub round: bool,
}

/// The rail: a column of large buttons with labels, filling `rect`.
/// Returns the button clicked.
pub fn rail(
    ui: &mut Ui,
    rect: Rect,
    items: &[RailItem],
    selected: Option<usize>,
    theme: &Theme,
) -> Option<usize> {
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, theme.panel);
    painter.line_segment([rect.right_top(), rect.right_bottom()], theme.hairline());
    let mut clicked = None;
    let mut y = rect.top() + 8.0;
    for (index, item) in items.iter().enumerate() {
        let height = if item.round { 86.0 } else { 70.0 };
        let cell = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), height));
        let response = ui.interact(
            cell,
            ui.scope_id().with(("sue-start-rail", index)),
            Sense::click(),
        );
        let on = selected == Some(index);
        let hot = response.hovered();
        let (ink, label) = if on || hot {
            (theme.text, theme.text)
        } else {
            (theme.muted, theme.muted)
        };
        if item.round {
            let center = pos2(cell.center().x, cell.top() + 32.0);
            let fill = if hot { theme.text } else { theme.border_active };
            painter.circle_filled(center, 25.0, fill);
            glyph(
                &painter,
                Rect::from_center_size(center, Vec2::splat(26.0)),
                item.glyph,
                theme.void,
            );
            painter.text(
                pos2(cell.center().x, cell.top() + 64.0),
                Align2::CENTER_TOP,
                &item.label,
                theme.font(theme.small_size + 1.0),
                theme.text,
            );
            // A line under the round button, as in a launcher.
            painter.line_segment(
                [
                    pos2(cell.left() + 14.0, cell.bottom() - 1.0),
                    pos2(cell.right() - 14.0, cell.bottom() - 1.0),
                ],
                theme.hairline(),
            );
        } else {
            if on || hot {
                painter.rect_filled(
                    cell.shrink2(vec2(4.0, 2.0)),
                    0.0,
                    if on { theme.active } else { theme.hover },
                );
            }
            if on {
                // The accent: a bar on the selected button's edge.
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(cell.left(), cell.top() + 2.0),
                        pos2(cell.left() + 3.0, cell.bottom() - 2.0),
                    ),
                    0.0,
                    theme.border_active,
                );
            }
            glyph(
                &painter,
                Rect::from_center_size(pos2(cell.center().x, cell.top() + 24.0), Vec2::splat(26.0)),
                item.glyph,
                ink,
            );
            let galley = painter.layout(
                item.label.clone(),
                theme.font(theme.small_size),
                label,
                cell.width() - 8.0,
            );
            let at = pos2(cell.center().x - galley.size().x / 2.0, cell.top() + 42.0);
            painter.galley(at, galley, label);
        }
        if response.clicked() {
            clicked = Some(index);
        }
        let _ = response.on_hover_text(&item.label);
        y += height + 4.0;
    }
    clicked
}

/// A square button with a glyph, lit when `on`.
pub fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    picture: Glyph,
    hint: &str,
    on: bool,
    theme: &Theme,
) -> Response {
    icon_button_enabled(ui, rect, picture, hint, on, true, theme)
}

/// A square button with a glyph, grayed and deaf when not `enabled`.
pub fn icon_button_enabled(
    ui: &mut Ui,
    rect: Rect,
    picture: Glyph,
    hint: &str,
    on: bool,
    enabled: bool,
    theme: &Theme,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(
        rect,
        ui.scope_id()
            .with(("sue-start-icon", hint, rect.min.x as i32, rect.min.y as i32)),
        sense,
    );
    let painter = ui.painter();
    let hot = enabled && response.hovered();
    if on || hot {
        painter.rect_filled(rect, 0.0, if on { theme.active } else { theme.hover });
    }
    if on {
        painter.rect_stroke(rect, 0.0, theme.hairline_active(), StrokeKind::Inside);
    }
    let ink = if !enabled {
        theme.muted.gamma_multiply(0.5)
    } else if on || hot {
        theme.text
    } else {
        theme.muted
    };
    let side = (rect.width().min(rect.height()) * 0.6).max(8.0);
    glyph(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        picture,
        ink,
    );
    if hint.is_empty() {
        response
    } else {
        response.on_hover_text(hint)
    }
}

/// A search box `width` wide with a magnifying glass. Returns the text
/// edit's response.
pub fn search_box(
    ui: &mut Ui,
    text: &mut String,
    hint: &str,
    width: f32,
    theme: &Theme,
) -> Response {
    let height = 28.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, 0.0, theme.void, theme.hairline(), StrokeKind::Inside);
    glyph(
        painter,
        Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), Vec2::splat(14.0)),
        Glyph::Search,
        theme.muted,
    );
    let field = Rect::from_min_max(
        pos2(rect.left() + 30.0, rect.top() + 2.0),
        pos2(rect.right() - 28.0, rect.bottom() - 2.0),
    );
    let response = ui.put(
        field,
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .frame(egui::Frame::NONE)
            .text_color(theme.text)
            .font(theme.font(theme.ui_size))
            .align(Align2::LEFT_CENTER),
    );
    if !text.is_empty() {
        let clear = Rect::from_center_size(
            pos2(rect.right() - 14.0, rect.center().y),
            Vec2::splat(20.0),
        );
        if icon_button(ui, clear, Glyph::Close, "", false, theme).clicked() {
            text.clear();
        }
    }
    response
}

/// A toolbar button: a glyph and a label, grayed and deaf when not
/// `enabled`, lit while `armed` (a first click that asks again).
pub fn action(
    ui: &mut Ui,
    picture: Glyph,
    label: &str,
    enabled: bool,
    armed: bool,
    theme: &Theme,
) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.font(theme.ui_size), theme.text);
    let size = vec2(galley.size().x + 38.0, 26.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let painter = ui.painter();
    let hot = enabled && response.hovered();
    if armed || hot {
        painter.rect_filled(rect, 0.0, if armed { theme.active } else { theme.hover });
    }
    if armed {
        painter.rect_stroke(rect, 0.0, theme.hairline_active(), StrokeKind::Inside);
    }
    let ink = if enabled {
        theme.text
    } else {
        theme.muted.gamma_multiply(0.5)
    };
    glyph(
        painter,
        Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), Vec2::splat(15.0)),
        picture,
        ink,
    );
    painter.galley(
        pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    response
}

/// A thin bar showing `fraction` (0 to 1) done.
pub fn progress(ui: &mut Ui, width: f32, fraction: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.void);
    let done = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    painter.rect_filled(done, 0.0, theme.text);
    painter.rect_stroke(rect, 0.0, theme.hairline(), StrokeKind::Inside);
}

/// A chip: a small button that stays lit while `on`.
pub fn chip(ui: &mut Ui, label: &str, on: bool, theme: &Theme) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        theme.font(theme.small_size + 1.0),
        theme.text,
    );
    let size = vec2(galley.size().x + 24.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if on {
        theme.active
    } else if response.hovered() {
        theme.hover
    } else {
        theme.pill
    };
    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(
        rect,
        0.0,
        if on {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    painter.galley(
        rect.center() - galley.size() / 2.0,
        galley,
        if on { theme.text } else { theme.muted },
    );
    response
}

/// How much of a set is chosen, for a Select All box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// None.
    None,
    /// Some.
    Some,
    /// All.
    All,
}

/// A check box with a label. Returns its response.
pub fn check(ui: &mut Ui, label: &str, chosen: Chosen, theme: &Theme) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.font(theme.ui_size), theme.text);
    let size = vec2(galley.size().x + 32.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let square =
        Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), Vec2::splat(18.0));
    painter.rect(
        square,
        0.0,
        if response.hovered() {
            theme.hover
        } else {
            theme.void
        },
        if chosen == Chosen::None {
            theme.hairline()
        } else {
            theme.hairline_active()
        },
        StrokeKind::Inside,
    );
    match chosen {
        Chosen::All => glyph(painter, square.shrink(3.0), Glyph::Check, theme.text),
        Chosen::Some => {
            painter.line_segment(
                [
                    pos2(square.left() + 5.0, square.center().y),
                    pos2(square.right() - 5.0, square.center().y),
                ],
                Stroke::new(2.0, theme.text),
            );
        }
        Chosen::None => {}
    }
    painter.galley(
        pos2(
            square.right() + 8.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.text,
    );
    response
}

/// A dropdown showing `options[*current]`, `width` wide. Returns `true`
/// when the choice changed.
pub fn dropdown(
    ui: &mut Ui,
    id: Id,
    current: &mut usize,
    options: &[String],
    width: f32,
    theme: &Theme,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        0.0,
        if response.hovered() {
            theme.hover
        } else {
            theme.pill
        },
        theme.hairline(),
        StrokeKind::Inside,
    );
    let text = options.get(*current).cloned().unwrap_or_default();
    painter.text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme.font(theme.ui_size),
        theme.text,
    );
    glyph(
        painter,
        Rect::from_center_size(
            pos2(rect.right() - 12.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        Glyph::ChevronDown,
        theme.muted,
    );
    let mut changed = false;
    let popup = egui::Popup::menu(&response).id(id).width(width);
    popup.show(|ui| {
        for (index, option) in options.iter().enumerate() {
            if ui.selectable_label(index == *current, option).clicked() {
                changed |= index != *current;
                *current = index;
            }
        }
    });
    changed
}

/// A monochrome on and off switch. Returns `true` when clicked.
pub fn toggle(ui: &mut Ui, rect: Rect, on: bool, id: Id, theme: &Theme) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let radius = rect.height() / 2.0;
    let painter = ui.painter();
    painter.rect_filled(rect, radius, if on { theme.text } else { theme.hover });
    painter.rect_stroke(rect, radius, theme.hairline(), StrokeKind::Inside);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    painter.circle_filled(
        pos2(x, rect.center().y),
        radius * 0.72,
        if on { theme.void } else { theme.muted },
    );
    response.clicked()
}

/// A row of a category list or tree.
#[derive(Clone, Debug)]
pub struct Category {
    /// Its picture, if any.
    pub glyph: Option<Glyph>,
    /// Its name.
    pub label: String,
    /// How deep in the tree, 0 at the top.
    pub depth: u8,
    /// `Some(open)` for a branch that opens and closes.
    pub open: Option<bool>,
}

/// What a click on a category row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryClick {
    /// Show it.
    Choose,
    /// Open or close the branch.
    Fold,
}

/// One category row `width` wide. Returns what was clicked, with the
/// row's response for a context menu.
pub fn category(
    ui: &mut Ui,
    width: f32,
    row: &Category,
    selected: bool,
    theme: &Theme,
) -> (Option<CategoryClick>, Response) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        theme.active
    } else if response.hovered() {
        theme.hover
    } else {
        theme.pill
    };
    painter.rect_filled(rect, 0.0, fill);
    if selected {
        painter.rect_stroke(rect, 0.0, theme.hairline_active(), StrokeKind::Inside);
    }
    let mut x = rect.left() + 6.0 + f32::from(row.depth) * 14.0;
    let mut click = None;
    if let Some(open) = row.open {
        let arrow = Rect::from_min_size(pos2(x, rect.top() + 5.0), Vec2::splat(16.0));
        let fold = ui.interact(arrow, response.id.with("fold"), Sense::click());
        glyph(
            painter,
            arrow.shrink(3.0),
            if open {
                Glyph::ChevronDown
            } else {
                Glyph::Chevron
            },
            theme.muted,
        );
        if fold.clicked() {
            click = Some(CategoryClick::Fold);
        }
        x += 18.0;
    }
    if let Some(picture) = row.glyph {
        glyph(
            painter,
            Rect::from_min_size(pos2(x, rect.top() + 6.0), Vec2::splat(14.0)),
            picture,
            theme.text,
        );
        x += 20.0;
    }
    painter.with_clip_rect(rect.shrink(1.0)).text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        &row.label,
        theme.font(theme.ui_size),
        if selected {
            theme.text
        } else {
            theme.muted.lerp_to_gamma(theme.text, 0.5)
        },
    );
    if row.open.is_none() && row.depth == 0 && row.glyph.is_none() {
        glyph(
            painter,
            Rect::from_center_size(
                pos2(rect.right() - 12.0, rect.center().y),
                Vec2::splat(12.0),
            ),
            Glyph::Chevron,
            theme.muted,
        );
    }
    if click.is_none() && response.clicked() {
        click = Some(CategoryClick::Choose);
    }
    (click, response)
}

/// A card or a list row to draw.
#[derive(Clone, Debug)]
pub struct Card {
    /// Its name.
    pub title: String,
    /// A second line, e.g. when it was saved.
    pub detail: String,
    /// The picture and its size in pixels, once there is one.
    pub picture: Option<(TextureId, [u32; 2])>,
    /// What to draw while there is no picture.
    pub placeholder: Glyph,
    /// `Some(on)` to show a heart.
    pub favorite: Option<bool>,
    /// Ticked for a group action.
    pub chosen: bool,
    /// What a hover says.
    pub hover: String,
}

/// What a click on a card asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardClick {
    /// Open it (a click on the picture or the name).
    Open,
    /// Tick it, or take its tick away.
    Choose,
    /// Make it a favorite, or not.
    Favorite,
    /// Show what it is (the `>` button).
    Details,
}

/// A card `size` big: its picture on top, its name under it, a tick box
/// at the top left, and a heart and a `>` at the bottom right. Returns the
/// click and the card's response for a context menu.
pub fn card(ui: &mut Ui, size: Vec2, card: &Card, theme: &Theme) -> (Option<CardClick>, Response) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let compact = size.x < 120.0;
    let footer = if compact { 22.0 } else { 44.0 };
    let picture_rect = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - footer));
    let hot = response.hovered();
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, if hot { theme.hover } else { theme.pill });
    picture(&painter, picture_rect, card, theme);
    painter.rect_stroke(
        rect,
        0.0,
        if card.chosen {
            Stroke::new(2.0, theme.border_active)
        } else if hot {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    let name_rect = Rect::from_min_max(
        pos2(rect.left() + 6.0, picture_rect.bottom()),
        pos2(rect.right() - 6.0, rect.bottom()),
    );
    painter.with_clip_rect(name_rect).text(
        pos2(
            name_rect.left(),
            picture_rect.bottom() + if compact { 11.0 } else { 12.0 },
        ),
        Align2::LEFT_CENTER,
        &card.title,
        theme.font(if compact {
            theme.small_size
        } else {
            theme.ui_size
        }),
        theme.text,
    );
    let mut click = None;
    // The tick box, at the top left; it shows on hover or once ticked.
    let tick = Rect::from_min_size(rect.min + vec2(5.0, 5.0), Vec2::splat(18.0));
    let tick_response = ui.interact(tick, response.id.with("tick"), Sense::click());
    if card.chosen || hot || tick_response.hovered() {
        painter.rect(
            tick,
            0.0,
            theme.void,
            theme.hairline_active(),
            StrokeKind::Inside,
        );
        if card.chosen {
            glyph(&painter, tick.shrink(3.0), Glyph::Check, theme.text);
        }
    }
    if tick_response.clicked() {
        click = Some(CardClick::Choose);
    }
    if !compact {
        let mut right = rect.right() - 4.0;
        let row_y = rect.bottom() - 13.0;
        let details = Rect::from_center_size(pos2(right - 10.0, row_y), Vec2::splat(20.0));
        if icon_button(ui, details, Glyph::Details, "", false, theme).clicked() {
            click = Some(CardClick::Details);
        }
        right -= 24.0;
        if let Some(on) = card.favorite {
            let heart = Rect::from_center_size(pos2(right - 10.0, row_y), Vec2::splat(20.0));
            let heart_response = ui.interact(heart, response.id.with("heart"), Sense::click());
            let ink = if on {
                theme.text
            } else if heart_response.hovered() {
                theme.muted
            } else {
                theme.muted.gamma_multiply(0.55)
            };
            glyph(&painter, heart.shrink(3.0), Glyph::Heart, ink);
            if heart_response.clicked() {
                click = Some(CardClick::Favorite);
            }
        }
        painter.with_clip_rect(name_rect).text(
            pos2(name_rect.left(), row_y),
            Align2::LEFT_CENTER,
            &card.detail,
            theme.font(theme.small_size),
            theme.muted,
        );
    } else if card.favorite == Some(true) {
        let heart = Rect::from_center_size(
            pos2(rect.right() - 11.0, rect.bottom() - 11.0),
            Vec2::splat(14.0),
        );
        glyph(&painter, heart, Glyph::Heart, theme.text);
    }
    if click.is_none() && response.clicked() {
        click = Some(
            if response
                .ctx
                .input(|input| input.modifiers.command || input.modifiers.shift)
            {
                CardClick::Choose
            } else {
                CardClick::Open
            },
        );
    }
    let response = if card.hover.is_empty() {
        response
    } else {
        response.on_hover_text(&card.hover)
    };
    (click, response)
}

/// A card's picture fitted inside `rect` on a light ground, or its
/// placeholder glyph.
fn picture(painter: &Painter, rect: Rect, card: &Card, theme: &Theme) {
    let inner = rect.shrink(1.0);
    match card.picture {
        Some((texture, [width, height])) if width > 0 && height > 0 => {
            painter.rect_filled(inner, 0.0, theme.void);
            let scale = (inner.width() / width as f32).min(inner.height() / height as f32);
            let shown = vec2(width as f32 * scale, height as f32 * scale);
            painter.image(
                texture,
                Rect::from_center_size(inner.center(), shown),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        _ => {
            painter.rect_filled(inner, 0.0, theme.void);
            let side = (inner.width().min(inner.height()) * 0.4).max(10.0);
            glyph(
                painter,
                Rect::from_center_size(inner.center(), Vec2::splat(side)),
                card.placeholder,
                theme.muted,
            );
        }
    }
}

/// A card as a row `width` wide: a small picture, the name and detail,
/// the heart and the `>` at the right.
pub fn list_row(
    ui: &mut Ui,
    width: f32,
    card: &Card,
    theme: &Theme,
) -> (Option<CardClick>, Response) {
    let height = 44.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let hot = response.hovered();
    let painter = ui.painter().clone();
    painter.rect(
        rect,
        0.0,
        if hot { theme.hover } else { theme.pill },
        if card.chosen {
            Stroke::new(2.0, theme.border_active)
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    let mut click = None;
    let tick = Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), Vec2::splat(18.0));
    let tick_response = ui.interact(tick, response.id.with("tick"), Sense::click());
    painter.rect(
        tick,
        0.0,
        theme.void,
        if card.chosen {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    if card.chosen {
        glyph(&painter, tick.shrink(3.0), Glyph::Check, theme.text);
    }
    if tick_response.clicked() {
        click = Some(CardClick::Choose);
    }
    let thumb = Rect::from_min_size(pos2(rect.left() + 32.0, rect.top() + 4.0), vec2(48.0, 36.0));
    picture(&painter, thumb, card, theme);
    let text_left = thumb.right() + 10.0;
    let text_rect = Rect::from_min_max(
        pos2(text_left, rect.top()),
        pos2(rect.right() - 60.0, rect.bottom()),
    );
    painter.with_clip_rect(text_rect).text(
        pos2(text_left, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        &card.title,
        theme.font(theme.ui_size),
        theme.text,
    );
    painter.with_clip_rect(text_rect).text(
        pos2(text_left, rect.top() + 31.0),
        Align2::LEFT_CENTER,
        &card.detail,
        theme.font(theme.small_size),
        theme.muted,
    );
    let details = Rect::from_center_size(
        pos2(rect.right() - 16.0, rect.center().y),
        Vec2::splat(22.0),
    );
    if icon_button(ui, details, Glyph::Details, "", false, theme).clicked() {
        click = Some(CardClick::Details);
    }
    if let Some(on) = card.favorite {
        let heart = Rect::from_center_size(
            pos2(rect.right() - 42.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let heart_response = ui.interact(heart, response.id.with("heart"), Sense::click());
        glyph(
            &painter,
            heart.shrink(3.0),
            Glyph::Heart,
            if on {
                theme.text
            } else {
                theme.muted.gamma_multiply(0.55)
            },
        );
        if heart_response.clicked() {
            click = Some(CardClick::Favorite);
        }
    }
    if click.is_none() && response.clicked() {
        click = Some(
            if response
                .ctx
                .input(|input| input.modifiers.command || input.modifiers.shift)
            {
                CardClick::Choose
            } else {
                CardClick::Open
            },
        );
    }
    let response = if card.hover.is_empty() {
        response
    } else {
        response.on_hover_text(&card.hover)
    };
    (click, response)
}

/// What sits at the right end of a popup row.
#[derive(Clone, Debug, PartialEq)]
pub enum Trailing {
    /// Nothing.
    None,
    /// A chevron: it opens more.
    More,
    /// A box with an arrow: it opens elsewhere (a web page, a folder).
    External,
    /// A switch, on or off.
    Switch(bool),
    /// Short text, e.g. a version.
    Text(String),
}

/// One row of a group in a popup.
#[derive(Clone, Debug)]
pub struct Row {
    /// Its picture, if any.
    pub glyph: Option<Glyph>,
    /// What it says.
    pub label: String,
    /// A second line in small text, if any.
    pub detail: String,
    /// What sits at its right end.
    pub trailing: Trailing,
    /// `false` grays it out.
    pub enabled: bool,
}

impl Row {
    /// A row saying `label`, with `glyph` and `trailing`.
    #[must_use]
    pub fn new(glyph: Option<Glyph>, label: impl Into<String>, trailing: Trailing) -> Self {
        Self {
            glyph,
            label: label.into(),
            detail: String::new(),
            trailing,
            enabled: true,
        }
    }

    /// The same row with a second line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
}

/// A group of rows sharing one box, the width of `ui`. Returns the row
/// clicked (a click on a row's switch counts).
pub fn group(ui: &mut Ui, rows: &[Row], theme: &Theme) -> Option<usize> {
    let width = ui.available_width();
    let mut clicked = None;
    let top = ui.cursor().min.y;
    for (index, row) in rows.iter().enumerate() {
        let height = if row.detail.is_empty() { 42.0 } else { 52.0 };
        let sense = if row.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            0.0,
            if row.enabled && response.hovered() {
                theme.hover
            } else {
                theme.pill
            },
        );
        if index > 0 {
            painter.line_segment(
                [
                    rect.left_top() + vec2(12.0, 0.0),
                    rect.right_top() - vec2(12.0, 0.0),
                ],
                theme.hairline(),
            );
        }
        let ink = if row.enabled {
            theme.text
        } else {
            theme.muted.gamma_multiply(0.6)
        };
        let mut x = rect.left() + 14.0;
        if let Some(picture) = row.glyph {
            glyph(
                painter,
                Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0)),
                picture,
                ink,
            );
            x += 28.0;
        }
        if row.detail.is_empty() {
            painter.text(
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                &row.label,
                theme.font(theme.ui_size + 1.0),
                ink,
            );
        } else {
            painter.text(
                pos2(x, rect.top() + 16.0),
                Align2::LEFT_CENTER,
                &row.label,
                theme.font(theme.ui_size + 1.0),
                ink,
            );
            let detail_rect = Rect::from_min_max(
                pos2(x, rect.top()),
                pos2(rect.right() - 60.0, rect.bottom()),
            );
            painter.with_clip_rect(detail_rect).text(
                pos2(x, rect.top() + 36.0),
                Align2::LEFT_CENTER,
                &row.detail,
                theme.font(theme.small_size),
                theme.muted,
            );
        }
        let end = pos2(rect.right() - 14.0, rect.center().y);
        let mut hit = row.enabled && response.clicked();
        match &row.trailing {
            Trailing::None => {}
            Trailing::More => glyph(
                painter,
                Rect::from_center_size(end - vec2(6.0, 0.0), Vec2::splat(14.0)),
                Glyph::Chevron,
                theme.muted,
            ),
            Trailing::External => glyph(
                painter,
                Rect::from_center_size(end - vec2(7.0, 0.0), Vec2::splat(15.0)),
                Glyph::External,
                theme.muted,
            ),
            Trailing::Text(text) => {
                painter.text(
                    end,
                    Align2::RIGHT_CENTER,
                    text,
                    theme.font(theme.small_size),
                    theme.muted,
                );
            }
            Trailing::Switch(on) => {
                let switch = Rect::from_min_max(end - vec2(46.0, 12.0), end + vec2(0.0, 12.0));
                hit |= toggle(ui, switch, *on, response.id.with("switch"), theme);
            }
        }
        if hit {
            clicked = Some(index);
        }
    }
    let bottom = ui.cursor().min.y - ui.spacing().item_spacing.y;
    ui.painter().rect_stroke(
        Rect::from_min_max(
            pos2(ui.min_rect().left(), top),
            pos2(ui.min_rect().left() + width, bottom),
        ),
        0.0,
        theme.hairline(),
        StrokeKind::Inside,
    );
    clicked
}

/// A popup's frame, title and X: what it answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupOutcome {
    /// Still open.
    Open,
    /// Closed (the X, Escape or a click on the dimmed page).
    Closed,
}

/// Shows a popup `width` wide (and at most `height` tall) in the middle of
/// the screen over the dimmed page, with `title` and an X; `body` draws its
/// rows. Escape or a click beside it closes it.
pub fn popup(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    theme: &Theme,
    body: impl FnOnce(&mut Ui),
) -> PopupOutcome {
    let screen = ctx.content_rect();
    let mut outcome = PopupOutcome::Open;
    egui::Area::new(Id::unique(("sue-start-scrim", id)))
        .fade_in(false)
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let response = ui.allocate_rect(screen, Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.55));
            if response.clicked() {
                outcome = PopupOutcome::Closed;
            }
        });
    let size = size.min(screen.size() - vec2(16.0, 16.0));
    let rect = Rect::from_center_size(screen.center(), size);
    egui::Area::new(Id::unique(("sue-start-popup", id)))
        .fade_in(false)
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(size);
            ui.set_max_size(size);
            let painter = ui.painter();
            painter.rect_filled(
                rect.translate(vec2(0.0, 6.0)).expand(2.0),
                0.0,
                theme.shadow.gamma_multiply(0.5),
            );
            painter.rect(
                rect,
                CornerRadius::ZERO,
                theme.panel,
                theme.hairline(),
                StrokeKind::Inside,
            );
            painter.text(
                pos2(rect.center().x, rect.top() + 28.0),
                Align2::CENTER_CENTER,
                title,
                theme.font(theme.heading_size),
                theme.text,
            );
            let close = Rect::from_center_size(
                pos2(rect.right() - 28.0, rect.top() + 28.0),
                Vec2::splat(26.0),
            );
            if icon_button(ui, close, Glyph::Close, "", false, theme).clicked() {
                outcome = PopupOutcome::Closed;
            }
            let inner = Rect::from_min_max(
                pos2(rect.left() + 20.0, rect.top() + 56.0),
                pos2(rect.right() - 20.0, rect.bottom() - 20.0),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(inner);
            egui::ScrollArea::vertical()
                .id_salt(("sue-start-popup-body", id))
                .auto_shrink([false, true])
                .show(&mut child, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.set_width(inner.width());
                    body(ui);
                });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        outcome = PopupOutcome::Closed;
    }
    outcome
}

/// A gap between groups in a popup.
pub fn gap(ui: &mut Ui) {
    ui.add_space(18.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [Glyph; 37] = [
        Glyph::Draw,
        Glyph::Projects,
        Glyph::Materials,
        Glyph::Transfers,
        Glyph::Help,
        Glyph::Settings,
        Glyph::Add,
        Glyph::Delete,
        Glyph::Rename,
        Glyph::Heart,
        Glyph::Details,
        Glyph::Chevron,
        Glyph::ChevronDown,
        Glyph::Search,
        Glyph::Grid,
        Glyph::GridSmall,
        Glyph::List,
        Glyph::SortUp,
        Glyph::SortDown,
        Glyph::Close,
        Glyph::External,
        Glyph::Moon,
        Glyph::Folder,
        Glyph::Wrench,
        Glyph::Check,
        Glyph::Picture,
        Glyph::Brush,
        Glyph::Pattern,
        Glyph::Tone,
        Glyph::Cube,
        Glyph::Clock,
        Glyph::Tag,
        Glyph::Move,
        Glyph::Book,
        Glyph::Bubble,
        Glyph::Mail,
        Glyph::All,
    ];

    #[test]
    fn every_widget_draws_and_a_popup_closes_on_escape() {
        let mut rice = sue_rice::RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        let theme = Theme::from_rice(&rice);
        let ctx = egui::Context::default();
        let mut closed = false;
        for frame in 0..2 {
            let mut input = egui::RawInput::default();
            if frame == 1 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let output = ctx.run_ui(input, |ui| {
                for (index, picture) in EVERY.iter().enumerate() {
                    let rect =
                        Rect::from_min_size(pos2(index as f32 * 20.0, 0.0), Vec2::splat(18.0));
                    glyph(ui.painter(), rect, *picture, theme.text);
                }
                let items = [
                    RailItem {
                        glyph: Glyph::Draw,
                        label: "Draw".into(),
                        round: true,
                    },
                    RailItem {
                        glyph: Glyph::Projects,
                        label: "Projects".into(),
                        round: false,
                    },
                ];
                let rect = Rect::from_min_size(pos2(0.0, 40.0), vec2(80.0, 400.0));
                rail(ui, rect, &items, Some(1), &theme);
                let mut text = "cat".to_owned();
                search_box(ui, &mut text, "Search", 200.0, &theme);
                chip(ui, "Tones", true, &theme);
                check(ui, "Select All", Chosen::Some, &theme);
                let mut current = 0;
                dropdown(
                    ui,
                    Id::unique("sort"),
                    &mut current,
                    &["Name".to_owned()],
                    120.0,
                    &theme,
                );
                let row = Category {
                    glyph: Some(Glyph::All),
                    label: "All".into(),
                    depth: 0,
                    open: Some(true),
                };
                category(ui, 180.0, &row, true, &theme);
                let shown = Card {
                    title: "Cat".into(),
                    detail: "just now".into(),
                    picture: None,
                    placeholder: Glyph::Picture,
                    favorite: Some(true),
                    chosen: true,
                    hover: String::new(),
                };
                card(ui, vec2(144.0, 190.0), &shown, &theme);
                card(ui, vec2(96.0, 118.0), &shown, &theme);
                list_row(ui, 400.0, &shown, &theme);
                let outcome = popup(ui.ctx(), "test", "Help", vec2(420.0, 300.0), &theme, |ui| {
                    group(
                        ui,
                        &[
                            Row::new(Some(Glyph::Moon), "Dark mode", Trailing::Switch(true)),
                            Row::new(None, "Version", Trailing::Text("1.0".into())).detail("Sue"),
                        ],
                        &theme,
                    );
                });
                closed |= outcome == PopupOutcome::Closed;
            });
            assert!(!output.shapes.is_empty());
            output.drop_without_applying_deltas();
        }
        assert!(closed, "Escape closes a popup");
    }
}
