//! The Hub's own pieces, drawn from the design system's tokens: the rail
//! and the tab bar of places, the studio cards, the buttons of Your work
//! and the headings of sections. Each answers the pointer with a fill
//! that fades on the riced motion (and snaps when transitions are
//! instant), darkens while pressed and draws the focus ring when the
//! keyboard is on it.

use egui::{
    Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::Theme;
use crate::icons::{self, Icon, IconStyle};
use crate::ui::{Density, Elevation, Radius, Space, TextRole, Tip, focus_ring, motion, tooltip};

/// The color `t` of the way from `from` to `to`.
fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

/// The rail's width (or the tab bar's height) and the pane beside it.
pub(super) fn split_area(
    area: Rect,
    narrow: bool,
    density: Density,
    theme: &Theme,
) -> (Rect, Rect) {
    if narrow {
        let height =
            density.icon() + theme.text_size(TextRole::Caption) + theme.space(Space::S5) * 2.0;
        let height = height.max(density.min_hit() + theme.space(Space::S4));
        let nav = Rect::from_min_max(pos2(area.left(), area.bottom() - height), area.max);
        let pane = Rect::from_min_max(area.min, pos2(area.right(), nav.top()));
        (nav, pane)
    } else {
        let width = density.icon_button() * 2.0 + theme.space(Space::S5);
        let nav = Rect::from_min_max(area.min, pos2(area.left() + width, area.bottom()));
        let pane = Rect::from_min_max(pos2(nav.right(), area.top()), area.max);
        (nav, pane)
    }
}

/// How lit a control is: its hover fading in and out, and whether it is
/// held down.
fn lit(ui: &Ui, response: &Response) -> (f32, bool) {
    let focused = response.has_focus() && crate::ui::focus_visible(ui.ctx());
    let over = ui.is_enabled() && (response.hovered() || focused);
    let t = motion::hover_t(ui.ctx(), response.id.with("sue-hub-lit"), over);
    (t, ui.is_enabled() && response.is_pointer_button_down_on())
}

/// The places in `nav`: a rail of icon and name cells, or a tab bar along
/// the bottom (`narrow`). The chosen one sits on a pill. Returns the
/// place clicked and where each was drawn.
pub(super) fn places(
    ui: &mut Ui,
    nav: Rect,
    items: &[(String, String, Icon)],
    current: &str,
    narrow: bool,
    theme: &Theme,
) -> (Option<String>, Vec<(String, Response)>) {
    let density = Density::of(ui);
    let painter = ui.painter().clone();
    painter.rect_filled(nav, CornerRadius::ZERO, theme.surface(1));
    if narrow {
        painter.hline(nav.x_range(), nav.top(), theme.hairline());
    } else {
        painter.vline(nav.right(), nav.y_range(), theme.hairline());
    }
    let gap = theme.space(Space::S2);
    let cell = if narrow {
        let width = nav.width() / items.len().max(1) as f32;
        vec2(width, nav.height())
    } else {
        let height = density.icon()
            + theme.text_size(TextRole::Caption)
            + theme.space(Space::S5) * 2.0
            + theme.space(Space::S4);
        vec2(nav.width(), height)
    };
    let mut clicked = None;
    let mut rects = Vec::new();
    for (index, (id, label, icon)) in items.iter().enumerate() {
        let rect = if narrow {
            Rect::from_min_size(pos2(nav.left() + cell.x * index as f32, nav.top()), cell)
        } else {
            Rect::from_min_size(
                pos2(
                    nav.left(),
                    nav.top() + theme.space(Space::S4) + (cell.y + gap) * index as f32,
                ),
                cell,
            )
        };
        let response = ui
            .interact(
                rect,
                ui.scope_id().with(("sue-hub-place", id.as_str())),
                Sense::click(),
            )
            .on_hover_text(label);
        let on = id == current;
        let (t, down) = lit(ui, &response);
        let icon_side = density.icon();
        let caption = theme.text_size(TextRole::Caption);
        let block = icon_side + theme.space(Space::S3) + caption;
        let top = rect.center().y - block / 2.0;
        // The pill behind the icon: solid when chosen, faint on hover.
        let pill = Rect::from_center_size(
            pos2(rect.center().x, top + icon_side / 2.0),
            vec2(icon_side * 2.5, icon_side + theme.space(Space::S3)),
        );
        let fill = if on {
            theme.fill_selected()
        } else if down {
            theme.fill_pressed()
        } else {
            blend(Color32::TRANSPARENT, theme.fill_hover(), t)
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(pill, theme.radius(Radius::Full), fill);
        }
        let ink = if on {
            theme.text
        } else {
            blend(theme.text_muted(), theme.text, t)
        };
        icons::paint_styled(
            &painter,
            Rect::from_center_size(pill.center(), Vec2::splat(icon_side)),
            *icon,
            if on {
                IconStyle::Solid
            } else {
                IconStyle::Line
            },
            ink,
        );
        let name = painter.layout(
            label.clone(),
            theme.text_font(TextRole::Caption),
            ink,
            rect.width() - gap * 2.0,
        );
        painter.galley(
            pos2(
                rect.center().x - name.size().x / 2.0,
                pill.bottom() + theme.space(Space::S1),
            ),
            name,
            ink,
        );
        focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
        if response.clicked() {
            clicked = Some(id.clone());
        }
        rects.push((id.clone(), response));
    }
    (clicked, rects)
}

/// A raised surface that answers the pointer: its shadow, fill and
/// outline. `attention` gives it the strong outline (work waiting).
/// Returns its fill, for the ink drawn on it.
fn surface(ui: &Ui, response: &Response, radius: f32, attention: bool, theme: &Theme) -> Color32 {
    let rect = response.rect;
    let painter = ui.painter();
    let (t, down) = lit(ui, response);
    if t > 0.0 {
        let mut shadow = theme.elevation(Elevation::Raised);
        shadow.color = shadow.color.gamma_multiply(t);
        painter.add(shadow.as_shape(rect, CornerRadius::same(radius.round() as u8)));
    }
    let fill = if down {
        theme.fill_pressed()
    } else {
        blend(theme.surface(2), theme.fill_hover(), t)
    };
    painter.rect_filled(rect, radius, fill);
    let edge = if attention {
        theme.selected_stroke()
    } else {
        Stroke::new(
            theme.border_width.max(1.0),
            blend(theme.border, theme.border_strong_on(fill), t),
        )
    };
    painter.rect_stroke(rect, radius, edge, StrokeKind::Inside);
    fill
}

/// A rounded square with an icon in it, as cards and buttons lead with.
fn badge(ui: &Ui, rect: Rect, icon: Icon, theme: &Theme) {
    let painter = ui.painter();
    painter.rect_filled(rect, theme.radius(Radius::Md), theme.surface(3));
    painter.rect_stroke(
        rect,
        theme.radius(Radius::Md),
        theme.hairline(),
        StrokeKind::Inside,
    );
    let side = rect.width() * 0.56;
    icons::paint(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        icon,
        theme.text,
    );
}

/// What a studio card shows.
pub(super) struct Studio<'a> {
    pub name: &'a str,
    pub summary: &'a str,
    pub icon: Icon,
    pub pinned: bool,
}

/// What a studio card reports: the card, its pin, and whether either
/// was clicked.
pub(super) struct StudioOutput {
    /// The card, for the keyboard's walk.
    pub response: Response,
    pub card: Rect,
    pub pin: Rect,
    pub open: bool,
    pub toggle_pin: bool,
}

/// A studio's card, `size` big: its icon on a badge, its name and what it
/// does, and a round pin at its top right that shows on hover, on touch
/// and while pinned. A short card (a phone's row) puts it all on a line.
pub(super) fn studio_card(
    ui: &mut Ui,
    size: Vec2,
    studio: &Studio<'_>,
    salt: &str,
    theme: &Theme,
) -> StudioOutput {
    let density = Density::of(ui);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(
        rect,
        ui.scope_id().with(("sue-hub-studio", salt)),
        Sense::click(),
    );
    let row = size.y < density.row() * 3.5;
    let pad = theme.card_padding();
    let side = density.icon_button();
    let pin = if row {
        Rect::from_center_size(
            pos2(rect.right() - pad - side / 2.0, rect.center().y),
            Vec2::splat(side),
        )
    } else {
        Rect::from_min_size(
            pos2(rect.right() - pad - side, rect.top() + pad),
            Vec2::splat(side),
        )
    };
    // The pin is made after the card, so a click on it is the pin's.
    let pin_response = ui
        .interact(
            pin,
            ui.scope_id().with(("sue-hub-pin", salt)),
            Sense::click(),
        )
        .on_hover_text(if studio.pinned {
            tr!("pinned-sue-opens-here-launch-click")
        } else {
            tr!("pin-sue-opens-here-launch")
        });
    let fill = surface(ui, &response, theme.radius(Radius::Lg), false, theme);
    let painter = ui.painter().clone();
    let badge_side = if row {
        rect.height() - pad * 2.0
    } else {
        density.row() * 1.6
    };
    let badge_rect = Rect::from_min_size(rect.min + Vec2::splat(pad), Vec2::splat(badge_side));
    badge(ui, badge_rect, studio.icon, theme);
    let soft = theme.text_muted_on(fill);
    let text_left = if row {
        badge_rect.right() + theme.space(Space::S5)
    } else {
        rect.left() + pad
    };
    let text_right = if row {
        pin.left() - theme.space(Space::S4)
    } else {
        rect.right() - pad
    };
    let room = (text_right - text_left).max(0.0);
    let name = painter.layout_no_wrap(
        studio.name.to_owned(),
        theme.text_font(TextRole::Title),
        theme.text,
    );
    let summary_font = theme.text_font(TextRole::Caption);
    let mut summary =
        egui::text::LayoutJob::simple(studio.summary.to_owned(), summary_font, soft, room);
    summary.wrap.max_rows = if row { 1 } else { 2 };
    summary.wrap.break_anywhere = false;
    summary.wrap.overflow_character = Some('\u{2026}');
    let summary = painter.layout_job(summary);
    let clip = Rect::from_min_max(pos2(text_left, rect.top()), pos2(text_right, rect.bottom()));
    let text = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
    if row {
        let block = name.size().y + theme.space(Space::S1) + summary.size().y;
        let top = rect.center().y - block / 2.0;
        text.galley(pos2(text_left, top), name.clone(), theme.text);
        text.galley(
            pos2(text_left, top + name.size().y + theme.space(Space::S1)),
            summary,
            soft,
        );
    } else {
        let top = badge_rect.bottom() + theme.space(Space::S5);
        text.galley(pos2(text_left, top), name.clone(), theme.text);
        text.galley(
            pos2(text_left, top + name.size().y + theme.space(Space::S2)),
            summary,
            soft,
        );
    }
    // The pin: round, shown on hover, on touch, with focus and while
    // pinned.
    let touch = density == Density::Touch;
    let shown_t = motion::hover_t(
        ui.ctx(),
        response.id.with("sue-hub-pin-shown"),
        ui.is_enabled()
            && (response.hovered()
                || response.has_focus()
                || pin_response.hovered()
                || pin_response.has_focus()),
    );
    let show = if studio.pinned || touch { 1.0 } else { shown_t };
    if show > 0.0 {
        let (t, down) = lit(ui, &pin_response);
        let pin_fill = if down {
            theme.fill_pressed()
        } else if studio.pinned {
            blend(theme.fill_selected(), theme.fill_hover(), t)
        } else {
            blend(Color32::TRANSPARENT, theme.fill_hover(), t)
        };
        if pin_fill != Color32::TRANSPARENT {
            painter.rect_filled(
                pin,
                theme.radius(Radius::Full),
                pin_fill.gamma_multiply(show),
            );
        }
        let ink = if studio.pinned {
            theme.text
        } else {
            blend(soft, theme.text, t)
        };
        icons::paint_styled(
            &painter,
            Rect::from_center_size(pin.center(), Vec2::splat(density.icon())),
            Icon::Pin,
            if studio.pinned {
                IconStyle::Solid
            } else {
                IconStyle::Line
            },
            ink.gamma_multiply(show),
        );
    }
    focus_ring(ui, &pin_response, theme.radius(Radius::Full), theme);
    focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
    let toggle_pin = pin_response.clicked();
    let open = response.clicked() && !toggle_pin;
    StudioOutput {
        response,
        card: rect,
        pin,
        open,
        toggle_pin,
    }
}

/// A button of Your work, `size` big: an icon on a badge, a name and a
/// line under it (a count). `attention` draws the strong outline; `tip`
/// is its tooltip (its name, its key when it has one, and a line of
/// help), which a long press shows on a touch screen.
pub(super) fn work_button(
    ui: &mut Ui,
    size: Vec2,
    (icon, name, detail): (Icon, &str, &str),
    attention: bool,
    tip: &Tip<'_>,
    salt: &str,
    theme: &Theme,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(
        rect,
        ui.scope_id().with(("sue-hub-work", salt)),
        Sense::click(),
    );
    let fill = surface(ui, &response, theme.radius(Radius::Lg), attention, theme);
    let pad = theme.space(Space::S5);
    let side = (rect.height() - pad * 2.0).min(Density::of(ui).icon_button());
    let badge_rect = Rect::from_min_size(
        pos2(rect.left() + pad, rect.center().y - side / 2.0),
        Vec2::splat(side),
    );
    badge(ui, badge_rect, icon, theme);
    let painter = ui.painter();
    let left = badge_rect.right() + theme.space(Space::S5);
    let room = (rect.right() - pad - left).max(0.0);
    let clip = Rect::from_min_max(pos2(left, rect.top()), pos2(left + room, rect.bottom()));
    let text = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
    // Each line ends in an ellipsis where the button ends, never cut.
    let one_line = |text: &str, font: egui::FontId, color: Color32| {
        let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, room);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('\u{2026}');
        painter.layout_job(job)
    };
    let title = one_line(name, theme.text_font(TextRole::Body), theme.text);
    let soft = theme.text_muted_on(fill);
    let line = one_line(detail, theme.text_font(TextRole::Caption), soft);
    let block = title.size().y + theme.space(Space::S1) + line.size().y;
    let top = rect.center().y - block / 2.0;
    let below = top + title.size().y + theme.space(Space::S1);
    text.galley(pos2(left, top), title, theme.text);
    text.galley(pos2(left, below), line, soft);
    focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
    tooltip(response, tip, theme)
}

/// A section's heading across `width`, with an optional quiet link at its
/// right. Returns the link's response.
pub(super) fn heading(
    ui: &mut Ui,
    width: f32,
    text: &str,
    link: Option<(&str, Icon)>,
    theme: &Theme,
) -> Option<Response> {
    let density = Density::of(ui);
    let height = density.min_hit();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        text,
        theme.text_font(TextRole::Title),
        theme.text,
    );
    let (label, icon) = link?;
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        theme.text_font(TextRole::Body),
        theme.text_muted(),
    );
    let pad = theme.space(Space::S4);
    let icon_side = density.icon();
    let link_width = galley.size().x + pad * 2.0 + icon_side + theme.space(Space::S2);
    let link = Rect::from_min_max(pos2(rect.right() - link_width, rect.top()), rect.max);
    let response = ui.interact(
        link,
        ui.scope_id().with(("sue-hub-link", text)),
        Sense::click(),
    );
    let (t, down) = lit(ui, &response);
    let fill = if down {
        theme.fill_pressed()
    } else {
        blend(Color32::TRANSPARENT, theme.fill_hover(), t)
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(link, theme.radius(Radius::Md), fill);
    }
    let ink = blend(theme.text_muted(), theme.text, t);
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.text_font(TextRole::Body), ink);
    ui.painter().galley(
        pos2(link.left() + pad, link.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    icons::paint(
        ui.painter(),
        Rect::from_center_size(
            pos2(link.right() - pad - icon_side / 2.0, link.center().y),
            Vec2::splat(icon_side),
        ),
        icon,
        ink,
    );
    focus_ring(ui, &response, theme.radius(Radius::Md), theme);
    Some(response)
}

/// The columns of a grid of cards at least `least` wide in `width`, and
/// the width each takes so that a row ends flush.
pub(super) fn grid(width: f32, least: f32, gap: f32) -> (usize, f32) {
    let columns = ((width + gap) / (least + gap)).floor().max(1.0) as usize;
    let each = ((width - gap * (columns - 1) as f32) / columns as f32).floor();
    (columns, each)
}

/// As [`grid`], for `count` cards: when they take more than one row, the
/// rows are as even as they can be (five cards in four columns become
/// three and two, never four and a lone one).
pub(super) fn balanced_grid(width: f32, least: f32, gap: f32, count: usize) -> (usize, f32) {
    let (most, _) = grid(width, least, gap);
    let rows = count.max(1).div_ceil(most);
    let columns = count.max(1).div_ceil(rows);
    let each = ((width - gap * (columns - 1) as f32) / columns as f32).floor();
    (columns, each)
}

#[cfg(test)]
mod tests {
    use super::{balanced_grid, grid};

    #[test]
    fn a_grid_of_buttons_never_leaves_one_alone_on_a_row() {
        // Room for four: five become three and two.
        let (four, _) = grid(1000.0, 200.0, 12.0);
        assert_eq!(four, 4);
        assert_eq!(balanced_grid(1000.0, 200.0, 12.0, 5).0, 3);
        // What fits on one row stays on one row.
        assert_eq!(balanced_grid(1000.0, 200.0, 12.0, 4).0, 4);
        assert_eq!(balanced_grid(1300.0, 200.0, 12.0, 5).0, 5);
        // Each column fills its share of the width.
        let (columns, each) = balanced_grid(1000.0, 200.0, 12.0, 5);
        assert!(
            (each * columns as f32 + 12.0 * (columns - 1) as f32 - 1000.0).abs() <= columns as f32
        );
    }
}
