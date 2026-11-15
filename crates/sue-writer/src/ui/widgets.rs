//! Small parts the panes share: colors, a splitter, drop zones, buttons
//! and the icons.

use egui::{
    Align2, Color32, Id, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use sue_core::EntityId;
use sue_wm::Theme;

use super::TOOL;

pub(super) fn desk(theme: &Theme) -> Color32 {
    let [r, g, b, _] = theme.void.to_array();
    let lift = |value: u8| value.saturating_add(0x18);
    Color32::from_rgb(lift(r), lift(g), lift(b))
}

pub(super) fn texture_key(id: EntityId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-writer-figure", id).hash(&mut hasher);
    hasher.finish()
}

/// A drag handle between two panels; returns its new x while dragged.
pub(super) fn splitter(
    ui: &mut Ui,
    name: &str,
    x: f32,
    span: egui::Rangef,
    theme: &Theme,
) -> Option<f32> {
    let rect = Rect::from_x_y_ranges(x - 3.0..=x + 3.0, span);
    let response = ui.interact(
        rect,
        Id::unique(("sue-writer-splitter", name)),
        Sense::drag(),
    );
    let color = if response.hovered() || response.dragged() {
        theme.border_active
    } else {
        theme.border
    };
    ui.painter().vline(x, span, Stroke::new(1.0, color));
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    response
        .dragged()
        .then(|| response.interact_pointer_pos().map_or(x, |pos| pos.x))
}

/// Where a drop lands on a binder row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zone {
    Before,
    Into,
    After,
}

pub(super) fn drop_zone(rect: Rect, pointer: Pos2, folder: bool) -> Zone {
    let t = (pointer.y - rect.top()) / rect.height().max(1.0);
    if folder && (0.25..0.75).contains(&t) {
        Zone::Into
    } else if t < 0.5 {
        Zone::Before
    } else {
        Zone::After
    }
}

/// A toggle button showing `text`.
/// A square text button `size` points wide (24 at a desk, a thumb's 44
/// on a phone).
pub(super) fn toggle(
    ui: &mut Ui,
    size: f32,
    text: RichText,
    on: bool,
    theme: &Theme,
) -> egui::Response {
    let grow = if size > 30.0 { 4.0 } else { 1.0 };
    let button = egui::Button::new(text.size(theme.ui_size + grow).color(if on {
        theme.text
    } else {
        theme.muted
    }))
    .fill(if on {
        theme.active
    } else {
        Color32::TRANSPARENT
    })
    .stroke(Stroke::NONE)
    .min_size(vec2(size, size));
    ui.add(button)
}

/// A square button with a drawn icon, `size` points wide.
pub(super) fn icon(
    ui: &mut Ui,
    size: f32,
    on: bool,
    theme: &Theme,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> egui::Response {
    // Not focusable, so the text keeps the keyboard (on a phone, a tap on
    // Bold leaves the on-screen keyboard up).
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), TOOL);
    paint_button(ui, rect, &response, on, ui.is_enabled(), theme, draw);
    response
}

fn paint_button(
    ui: &Ui,
    rect: Rect,
    response: &egui::Response,
    on: bool,
    enabled: bool,
    theme: &Theme,
    draw: impl FnOnce(&Painter, Rect, Color32),
) {
    if on {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.active);
    } else if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.hover);
    }
    let color = if !enabled {
        theme.muted.gamma_multiply(0.45)
    } else if on || response.hovered() {
        theme.text
    } else {
        theme.muted
    };
    // Half the button: a desk's 12 points, a thumb's 22.
    let inner = (rect.width() * 0.5).clamp(12.0, 22.0);
    draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(inner, inner)),
        color,
    );
}

/// A curved arrow back (undo) or forward (redo).
pub(super) fn undo_icon(painter: &Painter, rect: Rect, color: Color32, redo: bool) {
    let stroke = Stroke::new(1.6, color);
    let flip = |point: Pos2| {
        if redo {
            pos2(rect.left() + rect.right() - point.x, point.y)
        } else {
            point
        }
    };
    let center = pos2(rect.center().x + 1.0, rect.center().y + 2.0);
    let radius = rect.width() * 0.36;
    let points: Vec<Pos2> = (0..=12)
        .map(|step| {
            let angle = std::f32::consts::PI * (1.0 + step as f32 / 12.0);
            flip(center + vec2(angle.cos(), angle.sin()) * radius)
        })
        .chain([flip(pos2(rect.right() - 1.0, center.y + radius * 0.6))])
        .collect();
    painter.add(egui::Shape::line(points, stroke));
    let tip = flip(pos2(center.x - radius, center.y));
    let side = if redo { -1.0 } else { 1.0 };
    painter.line_segment([tip, tip + vec2(-3.5 * side, -4.0)], stroke);
    painter.line_segment([tip, tip + vec2(4.0 * side, -2.5)], stroke);
}

pub(super) fn align_icon(painter: &Painter, rect: Rect, color: Color32, align: sue_type::Align) {
    let stroke = Stroke::new(1.5, color);
    for (index, long) in [true, false, true, false].into_iter().enumerate() {
        let y = rect.top() + 1.0 + index as f32 * (rect.height() - 2.0) / 3.0;
        let width = if long || align == sue_type::Align::Justify {
            rect.width()
        } else {
            rect.width() * 0.6
        };
        let x = match align {
            sue_type::Align::Left | sue_type::Align::Justify => rect.left(),
            sue_type::Align::Center => rect.center().x - width / 2.0,
            sue_type::Align::Right => rect.right() - width,
        };
        painter.hline(x..=x + width, y, stroke);
    }
}

pub(super) fn list_icon(painter: &Painter, rect: Rect, color: Color32, numbered: bool) {
    for index in 0..3 {
        let y = rect.top() + 1.5 + index as f32 * (rect.height() - 3.0) / 2.0;
        if numbered {
            painter.text(
                pos2(rect.left() + 1.5, y),
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                egui::FontId::proportional(7.0),
                color,
            );
        } else {
            painter.circle_filled(pos2(rect.left() + 1.5, y), 1.4, color);
        }
        painter.hline(rect.left() + 5.0..=rect.right(), y, Stroke::new(1.5, color));
    }
}

pub(super) fn indent_icon(painter: &Painter, rect: Rect, color: Color32, more: bool) {
    let stroke = Stroke::new(1.5, color);
    for index in 0..4 {
        let y = rect.top() + 1.0 + index as f32 * (rect.height() - 2.0) / 3.0;
        let x = if index == 0 || index == 3 {
            rect.left()
        } else {
            rect.left() + 5.0
        };
        painter.hline(x..=rect.right(), y, stroke);
    }
    let mid = rect.center().y;
    let tip = if more { rect.left() + 3.5 } else { rect.left() };
    let back = if more { rect.left() } else { rect.left() + 3.5 };
    painter.add(egui::Shape::convex_polygon(
        vec![pos2(back, mid - 2.5), pos2(tip, mid), pos2(back, mid + 2.5)],
        color,
        Stroke::NONE,
    ));
}

pub(super) fn page_icon(painter: &Painter, rect: Rect, color: Color32, empty: bool) {
    let page = rect.shrink2(vec2(2.0, 0.5));
    painter.rect_stroke(page, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    if !empty {
        for index in 0..3 {
            let y = page.top() + 3.5 + index as f32 * 2.6;
            painter.hline(
                page.left() + 2.0..=page.right() - 2.0,
                y,
                Stroke::new(1.0, color.gamma_multiply(0.7)),
            );
        }
    }
}

pub(super) fn folder_icon(painter: &Painter, rect: Rect, color: Color32) {
    let body = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 3.0),
        rect.right_bottom() - vec2(0.0, 1.0),
    );
    painter.rect_stroke(body, 1.5, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.hline(
        rect.left() + 1.0..=rect.left() + 5.5,
        rect.top() + 1.6,
        Stroke::new(1.6, color),
    );
}

pub(super) fn book_icon(painter: &Painter, rect: Rect, color: Color32) {
    let body = rect.shrink2(vec2(1.5, 0.5));
    painter.rect_stroke(body, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.vline(body.left() + 3.0, body.y_range(), Stroke::new(1.2, color));
}

pub(super) fn trash_icon(painter: &Painter, rect: Rect, color: Color32) {
    let can = Rect::from_min_max(
        pos2(rect.left() + 2.0, rect.top() + 3.5),
        pos2(rect.right() - 2.0, rect.bottom()),
    );
    painter.rect_stroke(can, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.hline(
        rect.left() + 0.5..=rect.right() - 0.5,
        rect.top() + 2.0,
        Stroke::new(1.2, color),
    );
}

pub(super) fn picture_icon(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_stroke(
        rect.shrink(0.5),
        1.0,
        Stroke::new(1.2, color),
        StrokeKind::Inside,
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(rect.left() + 2.0, rect.bottom() - 2.0),
            pos2(rect.center().x - 1.0, rect.center().y),
            pos2(rect.right() - 2.0, rect.bottom() - 2.0),
        ],
        color.gamma_multiply(0.7),
        Stroke::NONE,
    ));
}
