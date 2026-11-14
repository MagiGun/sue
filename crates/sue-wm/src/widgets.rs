//! Small widgets every surface shares: pill frames, pill buttons,
//! segmented choices, section headings, Sue's own window title bar and
//! resize edges, and response curve graphs, all drawn from the design
//! tokens of the [`Theme`] (see [`crate::ui`]).

use egui::{
    Align2, Button, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2, vec2,
};

use crate::Theme;
use crate::ui::{
    Density, Elevation, Radius, Space, TextRole, focus_ring, focus_visible, motion, nested_radius,
    paint_focus_ring,
};

fn radius(value: f32) -> CornerRadius {
    CornerRadius::same(value.round().clamp(0.0, 255.0) as u8)
}

fn margin(value: f32) -> Margin {
    Margin::same(value.clamp(0.0, 127.0) as i8)
}

/// The frame of a floating pill: card fill, hairline border, the shadow of
/// what floats over the page.
pub fn pill_frame(theme: &Theme) -> Frame {
    // Sized so a pill of pill buttons is `layout.pill_height` tall.
    let across = theme.space(Space::S2);
    let down = theme.space(Space::S3) / 2.0;
    Frame::new()
        .fill(theme.surface(2))
        .stroke(theme.hairline())
        .corner_radius(theme.corner(Radius::Xl))
        .shadow(theme.elevation(Elevation::Floating))
        .inner_margin(Margin::symmetric(
            across.clamp(0.0, 127.0) as i8,
            down.clamp(0.0, 127.0) as i8,
        ))
}

/// The frame of a floating window (Settings, the command palette): panel
/// fill, hairline border, the shadow of a dialog.
pub fn window_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.surface(1))
        .stroke(theme.hairline())
        .corner_radius(theme.corner(Radius::Xl))
        .shadow(theme.elevation(Elevation::Modal))
        .inner_margin(margin(0.0))
}

/// Notes whether Sue draws its windows' frames itself this frame (see
/// `sue_platform::own_frames`), so every window of its own adds
/// [`title_bar`] and [`resize_edges`].
pub fn set_own_frames(ctx: &egui::Context, own: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL.with("sue-own-frames"), own));
}

/// `true` when Sue draws its windows' frames itself (see [`set_own_frames`]).
#[must_use]
pub fn own_frames(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp(egui::Id::NULL.with("sue-own-frames")))
        .unwrap_or(false)
}

/// How tall Sue's own title bars are: the tab strip of a compact palette.
pub const TITLE_BAR: f32 = Density::Compact.tab_strip();

/// How wide each button of a title bar is.
const TITLE_BUTTON: f32 = 40.0;

/// Half the side of a title bar button's glyph.
const TITLE_GLYPH: f32 = 5.0;

/// A window's own title bar across `rect`, as Clip Studio Paint arranges
/// them: the title in the middle, minimize, maximize and close at the
/// right. Dragging it moves the window, a double click maximizes it. Acts
/// on the window it is drawn in. `true` when close was pressed (the caller
/// decides what closing means).
pub fn title_bar(
    ui: &mut Ui,
    rect: egui::Rect,
    title: &str,
    maximize: bool,
    theme: &Theme,
) -> bool {
    use egui::{ViewportCommand, pos2};
    let ctx = ui.ctx().clone();
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, theme.surface(0));
    // The hairline on a whole device pixel, so it stays sharp at 125 and
    // 150 percent too.
    let line = painter.round_to_pixel_center(rect.bottom() - theme.border_width / 2.0);
    painter.hline(rect.x_range(), line, theme.hairline());
    let count = if maximize { 3.0 } else { 2.0 };
    let buttons_left = rect.right() - TITLE_BUTTON * count;
    let grip = egui::Rect::from_min_max(rect.min, pos2(buttons_left, rect.bottom()));
    let response = ui.interact(
        grip,
        ui.scope_id().with("sue-title-bar"),
        Sense::click_and_drag(),
    );
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if response.double_clicked() && maximize {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if response.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    // The title, in the middle of the whole bar, cut short to fit.
    let inset = theme.space(Space::S4);
    let room = (buttons_left - rect.left() - 2.0 * inset).max(40.0);
    let galley = painter.layout(
        title.to_owned(),
        theme.text_font(TextRole::Caption),
        theme.text_muted(),
        room,
    );
    let width = galley.size().x.min(room);
    let x = (rect.center().x - width / 2.0)
        .min(buttons_left - inset - width)
        .max(rect.left() + inset);
    painter.with_clip_rect(grip).galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        theme.text_muted(),
    );
    let mut close = false;
    let kinds: &[u8] = if maximize { &[0, 1, 2] } else { &[0, 2] };
    for (index, kind) in kinds.iter().enumerate() {
        let at = egui::Rect::from_min_size(
            pos2(buttons_left + index as f32 * TITLE_BUTTON, rect.top()),
            vec2(TITLE_BUTTON, rect.height()),
        );
        let hit = ui.interact(
            at,
            ui.scope_id().with(("sue-title-button", kind)),
            Sense::click(),
        );
        let label = match kind {
            0 => tr!("window-minimize"),
            1 if maximized => tr!("window-restore"),
            1 => tr!("window-maximize"),
            _ => tr!("window-close"),
        };
        hit.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        // The fill comes up quickly under the pointer and darkens while
        // pressed; the glyph turns from muted to the text color with it.
        let lit = motion::hover_t(&ctx, hit.id, hit.hovered() || hit.has_focus());
        let fill = if hit.is_pointer_button_down_on() {
            theme.fill_pressed()
        } else {
            Color32::TRANSPARENT.lerp_to_gamma(theme.fill_hover(), lit)
        };
        painter.rect_filled(at, 0.0, fill);
        let color = theme.text_muted().lerp_to_gamma(theme.text, lit);
        let stroke = Stroke::new(theme.border_width.max(1.0), color);
        let c = at.center();
        let g = TITLE_GLYPH;
        match kind {
            0 => {
                painter.hline((c.x - g)..=(c.x + g), c.y, stroke);
            }
            1 if maximized => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(c + vec2(-1.5, 1.5), Vec2::splat(g * 1.6)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                painter.line(
                    vec![
                        c + vec2(-g * 0.5, -g * 0.5),
                        c + vec2(g * 0.9, -g * 0.5),
                        c + vec2(g * 0.9, g * 0.9),
                    ],
                    stroke,
                );
            }
            1 => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(c, Vec2::splat(g * 2.0)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            _ => {
                painter.line_segment([c + vec2(-g, -g), c + vec2(g, g)], stroke);
                painter.line_segment([c + vec2(g, -g), c + vec2(-g, g)], stroke);
            }
        }
        if focus_visible(&ctx) && hit.has_focus() {
            // Inside the bar: the window's edge would cut a ring outside.
            painter.rect_stroke(
                at.shrink(1.0),
                0.0,
                theme.focus_stroke(),
                StrokeKind::Inside,
            );
        }
        if hit.clicked() {
            match kind {
                0 => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                1 => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                _ => close = true,
            }
        }
        let _ = hit.on_hover_text(label);
    }
    close
}

/// The edges and corners of a window Sue frames itself, around `rect`:
/// dragging one resizes the window, as with the system's frame. How far
/// they reach follows the [`Density`]: wider on touch.
pub fn resize_edges(ui: &mut Ui, rect: egui::Rect) {
    use egui::viewport::ResizeDirection as Way;
    use egui::{CursorIcon, ViewportCommand, pos2};
    let ctx = ui.ctx().clone();
    if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
        return;
    }
    // A fifth of the least hit target along the sides, half of it at the
    // corners: 5 and 12 points at the compact density.
    let reach = Density::of(ui).min_hit();
    let edge = (reach / 5.0).round();
    let corner = reach / 2.0;
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let zones = [
        (
            egui::Rect::from_min_max(pos2(l, t), pos2(l + corner, t + corner)),
            Way::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        (
            egui::Rect::from_min_max(pos2(r - corner, t), pos2(r, t + corner)),
            Way::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            egui::Rect::from_min_max(pos2(l, b - corner), pos2(l + corner, b)),
            Way::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (
            egui::Rect::from_min_max(pos2(r - corner, b - corner), pos2(r, b)),
            Way::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
        (
            egui::Rect::from_min_max(pos2(l, t), pos2(r, t + edge)),
            Way::North,
            CursorIcon::ResizeNorth,
        ),
        (
            egui::Rect::from_min_max(pos2(l, b - edge), pos2(r, b)),
            Way::South,
            CursorIcon::ResizeSouth,
        ),
        (
            egui::Rect::from_min_max(pos2(l, t), pos2(l + edge, b)),
            Way::West,
            CursorIcon::ResizeWest,
        ),
        (
            egui::Rect::from_min_max(pos2(r - edge, t), pos2(r, b)),
            Way::East,
            CursorIcon::ResizeEast,
        ),
    ];
    for (index, (zone, way, cursor)) in zones.into_iter().enumerate() {
        let response = ui.interact(
            zone,
            ui.scope_id().with(("sue-resize-edge", index)),
            Sense::drag(),
        );
        if response.hovered() || response.dragged() {
            ctx.set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(way));
        }
    }
}

/// Notes for this frame whether Sue is laid out for touch (a phone or a
/// tablet), so shared widgets size themselves for a finger.
pub fn set_touch(ctx: &egui::Context, touch: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL.with("sue-touch"), touch));
}

/// `true` when Sue is laid out for touch this frame (see [`set_touch`]).
#[must_use]
pub fn is_touch(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp(egui::Id::NULL.with("sue-touch")))
        .unwrap_or(false)
}

/// A compact text button for pills. A chosen one shows the chosen fill and
/// an outline in the active border color, so it is told apart by more
/// than its fill.
pub fn pill_button(ui: &mut Ui, text: &str, chosen: bool, theme: &Theme) -> Response {
    let corner = theme.radius(Radius::Md);
    let button = Button::new(
        RichText::new(text)
            .font(theme.text_font(TextRole::Body))
            .color(theme.text),
    )
    .fill(if chosen {
        theme.fill_selected()
    } else {
        Color32::TRANSPARENT
    })
    .stroke(if chosen {
        theme.hairline_active()
    } else {
        Stroke::NONE
    })
    .corner_radius(radius(corner))
    .min_size(vec2(
        0.0,
        (theme.pill_height - theme.space(Space::S3)).max(18.0),
    ));
    let response = ui.add(button);
    focus_ring(ui, &response, corner, theme);
    response
}

/// A thin vertical divider between groups in a pill.
pub fn pill_divider(ui: &mut Ui, theme: &Theme) {
    let height = (theme.pill_height - theme.space(Space::S5)).max(10.0);
    let (rect, _) =
        ui.allocate_exact_size(vec2(theme.border_width.max(1.0), height), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme.border);
}

/// A row of options; returns the index clicked this frame.
pub fn segmented(
    ui: &mut Ui,
    options: &[String],
    chosen: Option<usize>,
    theme: &Theme,
) -> Option<usize> {
    let mut picked = None;
    // Sized to its options and always left to right, even inside a
    // right-aligned row.
    let font = theme.text_font(TextRole::Body);
    let padding = ui.spacing().button_padding.x;
    let inset = theme.space(Space::S1);
    let width = options
        .iter()
        .map(|option| {
            ui.fonts_mut(|fonts| fonts.layout_no_wrap(option.clone(), font.clone(), theme.text))
                .size()
                .x
                + padding * 2.0
        })
        .sum::<f32>()
        + inset * options.len().saturating_sub(1) as f32
        + 2.0 * (inset + theme.border_width);
    let height = ui.spacing().interact_size.y + 2.0 * inset;
    ui.allocate_ui_with_layout(
        vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            Frame::new()
                .fill(theme.surface(0))
                .stroke(theme.hairline())
                .corner_radius(theme.corner(Radius::Md))
                .inner_margin(margin(inset))
                .show(ui, |ui| {
                    segment_buttons(ui, options, chosen, theme, &mut picked)
                });
        },
    );
    picked
}

fn segment_buttons(
    ui: &mut Ui,
    options: &[String],
    chosen: Option<usize>,
    theme: &Theme,
    picked: &mut Option<usize>,
) {
    let inset = theme.space(Space::S1);
    let corner = nested_radius(theme.radius(Radius::Md), inset);
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = inset;
        for (index, option) in options.iter().enumerate() {
            let on = chosen == Some(index);
            let button = Button::new(
                RichText::new(option)
                    .font(theme.text_font(TextRole::Body))
                    .color(if on { theme.text } else { theme.text_muted() }),
            )
            .fill(if on {
                theme.fill_selected()
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE)
            .corner_radius(radius(corner));
            let response = ui.add(button);
            focus_ring(ui, &response, corner, theme);
            if response.clicked() {
                *picked = Some(index);
            }
        }
    });
}

/// A small, muted, upper-case heading.
pub fn heading(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// A hairline rectangle around `rect`.
pub fn outline(ui: &Ui, rect: Rect, stroke: Stroke, corner: f32) {
    ui.painter()
        .rect_stroke(rect, radius(corner), stroke, StrokeKind::Inside);
}

/// What a response curve graph says along its edges.
#[derive(Clone, Debug)]
pub struct CurveLabels {
    /// Under the graph, in the middle: what goes in (`Pen pressure`).
    pub input: String,
    /// Under the graph, at the left (`0%`).
    pub low: String,
    /// Under the graph, at the right (`100%`).
    pub high: String,
    /// Left of the graph, between 0% and 100%: what comes out (`Output`).
    pub output: String,
}

impl CurveLabels {
    /// Pen pressure in, output out, in the language in use.
    #[must_use]
    pub fn pressure() -> Self {
        Self {
            input: tr!("curve-pen-pressure"),
            low: "0%".to_owned(),
            high: "100%".to_owned(),
            output: tr!("curve-output"),
        }
    }
}

/// Room left of a response curve graph for what it says along its side.
const CURVE_LEFT: f32 = 46.0;

/// Room above a response curve graph.
const CURVE_TOP: f32 = 4.0;

/// Edits a response curve as a graph, the way paint programs show pen
/// pressure: a grid, the control points joined by straight lines and the
/// smooth curve they shape. Drag a point to move it, press an empty spot
/// to add one, and drag a point out of the graph or double-click it to
/// take it away. `side` is the graph's width and height. Returns `true`
/// when the curve changed.
pub fn response_curve(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    curve: &mut sue_model::Response,
    side: f32,
    labels: CurveLabels,
    theme: &Theme,
) -> bool {
    let font = theme.text_font(TextRole::Caption);
    let caption = theme.text_size(TextRole::Caption);
    let gap = theme.space(Space::S2);
    let left = CURVE_LEFT;
    // A small graph puts what goes in on its own line, under the ends.
    let narrow = side < 140.0;
    let below = if narrow { 2.0 * caption + 10.0 } else { 22.0 };
    // How near a point the pointer must be to take it: half an icon of
    // the density, so a finger reaches further than a pen.
    let reach = Density::of(ui).icon() / 2.0;
    let (outer, _) =
        ui.allocate_exact_size(vec2(left + side + 6.0, side + 4.0 + below), Sense::hover());
    let plot = Rect::from_min_size(outer.min + vec2(left, CURVE_TOP), Vec2::splat(side));
    let id = ui.scope_id().with(("response-curve", id));
    let response = ui.interact(plot.expand(8.0), id, Sense::click_and_drag());
    let to_screen = |[x, y]: [f32; 2]| egui::pos2(plot.left() + x * side, plot.bottom() - y * side);
    let to_value = |pos: egui::Pos2| {
        [
            ((pos.x - plot.left()) / side).clamp(0.0, 1.0),
            ((plot.bottom() - pos.y) / side).clamp(0.0, 1.0),
        ]
    };
    let nearest = |curve: &sue_model::Response, pos: egui::Pos2| {
        curve
            .points()
            .iter()
            .enumerate()
            .map(|(index, &point)| (index, to_screen(point).distance(pos)))
            .filter(|(_, distance)| *distance <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    };

    // Editing.
    let mut changed = false;
    let mut held: Option<usize> = ui.data(|data| data.get_temp(id));
    let pointer = response.interact_pointer_pos();
    if response.drag_started()
        && let Some(pos) = pointer
    {
        // What is grabbed is what the button went down on. A drag is only
        // told from a click some points later, and a fast hand is past
        // the point's reach by then.
        let press = ui
            .input(|input| input.pointer.press_origin())
            .unwrap_or(pos);
        held = nearest(curve, press).or_else(|| {
            let added = curve.insert(to_value(press));
            changed |= added.is_some();
            added
        });
    }
    if response.dragged()
        && let (Some(index), Some(pos)) = (held, pointer)
    {
        let outside = plot.expand(24.0);
        let last = curve.points().len() - 1;
        if !outside.contains(pos) && index != 0 && index != last {
            curve.remove(index);
            held = None;
        } else {
            curve.set(index, to_value(pos));
        }
        changed = true;
    }
    if response.drag_stopped() {
        held = None;
    }
    if response.double_clicked()
        && let Some(index) = pointer.and_then(|pos| nearest(curve, pos))
    {
        changed |= curve.remove(index);
    }
    ui.data_mut(|data| match held {
        Some(index) => {
            data.insert_temp(id, index);
        }
        None => data.remove::<usize>(id),
    });
    let hovered = response.hover_pos().and_then(|pos| nearest(curve, pos));
    if held.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, labels.input.as_str()));

    // Drawing.
    let painter = ui.painter();
    let page = theme.surface(0);
    painter.rect_filled(plot, 0.0, page);
    let hairline = theme.border_width.max(1.0);
    for step in 1..4 {
        let at = step as f32 / 4.0;
        let grid = Stroke::new(hairline, theme.grid);
        painter.line_segment([to_screen([at, 0.0]), to_screen([at, 1.0])], grid);
        painter.line_segment([to_screen([0.0, at]), to_screen([1.0, at])], grid);
    }
    painter.rect_stroke(
        plot,
        0.0,
        Stroke::new(hairline, theme.border_strong_on(page)),
        StrokeKind::Inside,
    );
    if focus_visible(ui.ctx()) && response.has_focus() {
        paint_focus_ring(painter, plot, 0.0, 1.0, theme);
    }
    let points: Vec<egui::Pos2> = curve
        .points()
        .iter()
        .map(|&point| to_screen(point))
        .collect();
    // The control points joined by straight lines, quieter than the curve
    // they shape, which is drawn at the weight of what is chosen.
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(hairline, theme.border_strong_on(page)),
    ));
    let smooth: Vec<egui::Pos2> = (0..=64)
        .map(|step| to_screen(curve.at(step as f32 / 64.0)))
        .collect();
    painter.add(egui::Shape::line(
        smooth,
        Stroke::new(Theme::STRONG_STROKE * 0.8, theme.text),
    ));
    let mark = theme.radius(Radius::Xs);
    for (index, point) in points.iter().enumerate() {
        let on = held == Some(index) || hovered == Some(index);
        painter.rect_filled(
            Rect::from_center_size(*point, Vec2::splat(if on { 8.0 } else { 6.0 })),
            mark.min(2.0),
            if on {
                theme.selected_stroke().color
            } else {
                theme.text
            },
        );
    }
    let label = |pos: egui::Pos2, align: Align2, text: &str| {
        painter.text(pos, align, text, font.clone(), theme.text_muted());
    };
    // Out, from none to all, up the left; in, along the bottom.
    label(plot.left_top() + vec2(-gap, 0.0), Align2::RIGHT_TOP, "100%");
    label(
        plot.left_center() + vec2(-gap, 0.0),
        Align2::RIGHT_CENTER,
        &labels.output,
    );
    label(
        plot.left_bottom() + vec2(-gap, 0.0),
        Align2::RIGHT_BOTTOM,
        "0%",
    );
    let under = theme.space(Space::S1) + 1.0;
    label(
        plot.left_bottom() + vec2(0.0, under),
        Align2::LEFT_TOP,
        &labels.low,
    );
    label(
        plot.right_bottom() + vec2(0.0, under),
        Align2::RIGHT_TOP,
        &labels.high,
    );
    let input_at = if narrow {
        plot.center_bottom() + vec2(0.0, caption + theme.space(Space::S3))
    } else {
        plot.center_bottom() + vec2(0.0, under)
    };
    label(input_at, Align2::CENTER_TOP, &labels.input);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2, pos2};

    fn test_theme() -> Theme {
        let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        crate::register_rice(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    /// One frame of a response curve graph `side` points across at the
    /// top left of the screen. Answers where its plot is.
    fn curve_frame(
        ctx: &egui::Context,
        curve: &mut sue_model::Response,
        side: f32,
        events: Vec<Event>,
    ) -> Rect {
        let theme = test_theme();
        let mut plot = Rect::NOTHING;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            events,
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| {
            egui::Area::new(egui::Id::unique("curve-test"))
                .fixed_pos(Pos2::ZERO)
                .show(ui.ctx(), |ui| {
                    let corner = ui.cursor().min + vec2(CURVE_LEFT, CURVE_TOP);
                    plot = Rect::from_min_size(corner, Vec2::splat(side));
                    response_curve(ui, "test", curve, side, CurveLabels::pressure(), &theme);
                });
        })
        .drop_without_applying_deltas();
        plot
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Hands-on H-8: a point is grabbed by where the button went down. A
    /// hand that moves 12 points a frame is past the point's reach when
    /// the drag is recognised, and the graph used to add a second point
    /// there and leave the one that was pressed where it was.
    #[test]
    fn the_curve_grabs_the_point_the_button_went_down_on() {
        let ctx = egui::Context::default();
        let side = 200.0;
        let mut curve = sue_model::Response::new(&[[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]);
        let plot = curve_frame(&ctx, &mut curve, side, Vec::new());
        let on_screen = |[x, y]: [f32; 2]| pos2(plot.left() + x * side, plot.bottom() - y * side);
        let press = on_screen([0.5, 0.5]);
        curve_frame(&ctx, &mut curve, side, vec![Event::PointerMoved(press)]);
        curve_frame(&ctx, &mut curve, side, vec![button(press, true)]);
        // Straight up, 12 points a frame, for three frames.
        let mut at = press;
        for _ in 0..3 {
            at.y -= 12.0;
            curve_frame(&ctx, &mut curve, side, vec![Event::PointerMoved(at)]);
        }
        curve_frame(&ctx, &mut curve, side, vec![button(at, false)]);
        let points = curve.points();
        assert_eq!(points.len(), 3, "a point was added: {points:?}");
        let moved = on_screen(points[1]);
        assert!(
            moved.distance(at) < 0.5,
            "the pressed point is at {moved:?}, the pointer let go at {at:?}"
        );
    }

    /// A press on an empty spot still adds a point, where the press was,
    /// and the drag carries it along.
    #[test]
    fn a_press_on_an_empty_spot_of_the_curve_adds_a_point_there() {
        let ctx = egui::Context::default();
        let side = 200.0;
        let mut curve = sue_model::Response::LINEAR;
        let plot = curve_frame(&ctx, &mut curve, side, Vec::new());
        let on_screen = |[x, y]: [f32; 2]| pos2(plot.left() + x * side, plot.bottom() - y * side);
        let press = on_screen([0.25, 0.75]);
        curve_frame(&ctx, &mut curve, side, vec![Event::PointerMoved(press)]);
        curve_frame(&ctx, &mut curve, side, vec![button(press, true)]);
        let at = press + vec2(12.0, 0.0);
        curve_frame(&ctx, &mut curve, side, vec![Event::PointerMoved(at)]);
        curve_frame(&ctx, &mut curve, side, vec![button(at, false)]);
        let points = curve.points();
        assert_eq!(points.len(), 3, "{points:?}");
        assert!(on_screen(points[1]).distance(at) < 0.5, "{points:?}");
    }
}
