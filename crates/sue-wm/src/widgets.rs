//! Small widgets every surface shares: pill frames, pill and rail buttons,
//! segmented choices and section headings, all drawn from the [`Theme`].

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

/// A square icon button for the activity rails. The chosen one gets a
/// white bar on its outer edge.
pub fn rail_button(
    ui: &mut Ui,
    icon: &str,
    tooltip: &str,
    chosen: bool,
    left: bool,
    theme: &Theme,
) -> Response {
    let side = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect.shrink(4.0), radius(theme.control_radius), theme.hover);
    }
    if chosen {
        let bar = if left {
            Rect::from_min_size(rect.min, vec2(2.0, rect.height()))
        } else {
            Rect::from_min_size(rect.right_top() - vec2(2.0, 0.0), vec2(2.0, rect.height()))
        };
        painter.rect_filled(bar, 0.0, theme.border_active);
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        theme.font(theme.ui_size * 1.25),
        if chosen || response.hovered() {
            theme.text
        } else {
            theme.muted
        },
    );
    response.on_hover_text(tooltip)
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
