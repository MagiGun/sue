//! Small widgets every surface shares: pill frames, pill buttons,
//! segmented choices, section headings and response curve graphs, all
//! drawn from the [`Theme`].

use egui::{
    Align2, Button, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2, vec2,
};

use crate::Theme;

fn radius(value: f32) -> CornerRadius {
    CornerRadius::same(value.clamp(0.0, 255.0) as u8)
}

fn margin(value: f32) -> Margin {
    Margin::same(value.clamp(0.0, 127.0) as i8)
}

/// The frame of a floating pill: card fill, hairline border, soft shadow.
pub fn pill_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.pill)
        .stroke(theme.hairline())
        .corner_radius(radius(theme.pill_radius))
        .shadow(theme.shadow())
        .inner_margin(Margin::symmetric(
            (theme.padding * 0.5).clamp(0.0, 127.0) as i8,
            (theme.padding * 0.375).clamp(0.0, 127.0) as i8,
        ))
}

/// The frame of a floating window (Settings, the palette).
pub fn window_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.panel)
        .stroke(theme.hairline())
        .corner_radius(radius(theme.pill_radius))
        .shadow(theme.shadow())
        .inner_margin(margin(0.0))
}

/// A compact text button for pills. A chosen one shows the active fill and
/// a white hairline.
pub fn pill_button(ui: &mut Ui, text: &str, chosen: bool, theme: &Theme) -> Response {
    let button = Button::new(RichText::new(text).size(theme.ui_size).color(theme.text))
        .fill(if chosen {
            theme.active
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if chosen {
            theme.hairline_active()
        } else {
            Stroke::NONE
        })
        .corner_radius(radius(theme.control_radius))
        .min_size(vec2(
            0.0,
            (theme.pill_height - theme.padding * 0.75).max(18.0),
        ));
    ui.add(button)
}

/// A thin vertical divider between groups in a pill.
pub fn pill_divider(ui: &mut Ui, theme: &Theme) {
    let height = (theme.pill_height - theme.padding * 1.5).max(10.0);
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
    let font = theme.font(theme.ui_size);
    let padding = ui.spacing().button_padding.x;
    let width = options
        .iter()
        .map(|option| {
            ui.fonts_mut(|fonts| fonts.layout_no_wrap(option.clone(), font.clone(), theme.text))
                .size()
                .x
                + padding * 2.0
        })
        .sum::<f32>()
        + 2.0 * options.len().saturating_sub(1) as f32
        + 6.0;
    let height = ui.spacing().interact_size.y + 4.0;
    ui.allocate_ui_with_layout(
        vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            Frame::new()
                .fill(theme.void)
                .stroke(theme.hairline())
                .corner_radius(radius(theme.control_radius))
                .inner_margin(Margin::same(2))
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
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (index, option) in options.iter().enumerate() {
            let on = chosen == Some(index);
            let button = Button::new(RichText::new(option).size(theme.ui_size).color(if on {
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
            .corner_radius(radius((theme.control_radius - 1.0).max(0.0)));
            if ui.add(button).clicked() {
                *picked = Some(index);
            }
        }
    });
}

/// A small, muted, upper-case heading.
pub fn heading(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(theme.small_size)
            .color(theme.muted),
    );
}

/// A hairline rectangle around `rect`.
pub fn outline(ui: &Ui, rect: Rect, stroke: Stroke, corner: f32) {
    ui.painter()
        .rect_stroke(rect, radius(corner), stroke, StrokeKind::Inside);
}

/// What a response curve graph says along its edges.
#[derive(Clone, Copy, Debug)]
pub struct CurveLabels<'a> {
    /// Under the graph, in the middle: what goes in (`Pen pressure`).
    pub input: &'a str,
    /// Under the graph, at the left (`0%`).
    pub low: &'a str,
    /// Under the graph, at the right (`100%`).
    pub high: &'a str,
    /// Left of the graph, between 0% and 100%: what comes out (`Output`).
    pub output: &'a str,
}

impl CurveLabels<'_> {
    /// Pen pressure in, output out.
    pub const PRESSURE: CurveLabels<'static> = CurveLabels {
        input: "Pen pressure",
        low: "0%",
        high: "100%",
        output: "Output",
    };
}

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
    labels: CurveLabels<'_>,
    theme: &Theme,
) -> bool {
    let font = egui::FontId::proportional(theme.small_size);
    let left = 46.0;
    // A small graph puts what goes in on its own line, under the ends.
    let narrow = side < 140.0;
    let below = if narrow {
        2.0 * theme.small_size + 10.0
    } else {
        22.0
    };
    let (outer, _) =
        ui.allocate_exact_size(vec2(left + side + 6.0, side + 4.0 + below), Sense::hover());
    let plot = Rect::from_min_size(outer.min + vec2(left, 4.0), Vec2::splat(side));
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
            .filter(|(_, distance)| *distance <= 9.0)
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
        held = nearest(curve, pos).or_else(|| {
            let added = curve.insert(to_value(pos));
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
    if hovered.is_some() || held.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // Drawing.
    let painter = ui.painter();
    painter.rect_filled(plot, 0.0, theme.void);
    for step in 1..4 {
        let at = step as f32 / 4.0;
        let grid = Stroke::new(1.0, theme.grid);
        painter.line_segment([to_screen([at, 0.0]), to_screen([at, 1.0])], grid);
        painter.line_segment([to_screen([0.0, at]), to_screen([1.0, at])], grid);
    }
    painter.rect_stroke(
        plot,
        0.0,
        Stroke::new(1.0, theme.border),
        StrokeKind::Inside,
    );
    let points: Vec<egui::Pos2> = curve
        .points()
        .iter()
        .map(|&point| to_screen(point))
        .collect();
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(1.0, theme.muted.gamma_multiply(0.7)),
    ));
    let smooth: Vec<egui::Pos2> = (0..=64)
        .map(|step| to_screen(curve.at(step as f32 / 64.0)))
        .collect();
    painter.add(egui::Shape::line(smooth, Stroke::new(1.6, theme.text)));
    for (index, point) in points.iter().enumerate() {
        let on = held == Some(index) || hovered == Some(index);
        painter.rect_filled(
            Rect::from_center_size(*point, Vec2::splat(if on { 8.0 } else { 6.0 })),
            1.0,
            if on { theme.border_active } else { theme.text },
        );
    }
    let label = |pos: egui::Pos2, align: Align2, text: &str| {
        painter.text(pos, align, text, font.clone(), theme.muted);
    };
    // Out, from none to all, up the left; in, along the bottom.
    label(plot.left_top() + vec2(-4.0, 0.0), Align2::RIGHT_TOP, "100%");
    label(
        plot.left_center() + vec2(-4.0, 0.0),
        Align2::RIGHT_CENTER,
        labels.output,
    );
    label(
        plot.left_bottom() + vec2(-4.0, 0.0),
        Align2::RIGHT_BOTTOM,
        "0%",
    );
    label(
        plot.left_bottom() + vec2(0.0, 3.0),
        Align2::LEFT_TOP,
        labels.low,
    );
    label(
        plot.right_bottom() + vec2(0.0, 3.0),
        Align2::RIGHT_TOP,
        labels.high,
    );
    let input_at = if narrow {
        plot.center_bottom() + vec2(0.0, theme.small_size + 6.0)
    } else {
        plot.center_bottom() + vec2(0.0, 3.0)
    };
    label(input_at, Align2::CENTER_TOP, labels.input);
    changed
}
