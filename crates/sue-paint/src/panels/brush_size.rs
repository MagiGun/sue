//! The brush size palette and the brush size sliders column.

use egui::{Align2, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_wm::Theme;
use sue_wm::ui::{Radius, Space, TextRole};

use crate::icons::Icon;
use crate::panels::{header, tall_slider};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// Brush Size's sizes, Clip Studio Paint's steps from a hair to a broom.
const SIZES: [f32; 40] = [
    0.7, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 15.0, 17.0, 20.0, 25.0,
    30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0, 120.0, 150.0, 170.0, 200.0, 250.0, 300.0, 400.0,
    500.0, 600.0, 700.0, 800.0, 1000.0, 1200.0, 1500.0, 1700.0, 2000.0,
];

/// A size as Brush Size writes it under its dot.
fn size_text(size: f32) -> String {
    if size.fract() == 0.0 {
        format!("{size:.0}")
    } else {
        format!("{size}")
    }
}

impl SuePaint {
    /// `true` when the tool out has a brush size these palettes set.
    fn has_brush_size(&self) -> bool {
        let tool = self.toolbox.tool();
        tool.paints() || matches!(tool, Tool::Fill | Tool::Gradient)
    }

    /// Brush Size: Clip Studio Paint's grid of sizes, each a dot of its
    /// size (up to the cell) over its number; a click takes it. A locked
    /// sub tool shows its size and takes none.
    pub(super) fn brush_size_grid(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        if !self.has_brush_size() {
            ui.painter().text(
                body.center_top() + egui::Vec2::Y * theme.space(Space::S6),
                Align2::CENTER_CENTER,
                tr!("no-brush-size-for-this-tool"),
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
            return;
        }
        let locked = self.toolbox.current().locked;
        let step = theme.space(Space::S8) + theme.space(Space::S1);
        let columns = ((body.width() / step).floor() as usize).clamp(3, 8);
        let cell = body.width() / columns as f32;
        let label = theme.space(Space::S2);
        let now = self.toolbox.current().tip.size;
        let mut take = None;
        let painter = ui.painter().with_clip_rect(body);
        for (index, size) in SIZES.iter().enumerate() {
            let (row, column) = (index / columns, index % columns);
            let rect = Rect::from_min_size(
                pos2(
                    body.left() + column as f32 * cell,
                    body.top() + row as f32 * (cell + label),
                ),
                vec2(cell, cell + label),
            )
            .shrink(theme.space(Space::S1) / 2.0);
            // Whole rows only: a row the palette would cut through is
            // left out rather than shown as half dots (the first row
            // always shows).
            let row_bottom = body.top() + (row + 1) as f32 * (cell + label);
            if row > 0 && row_bottom > body.bottom() + 0.5 {
                break;
            }
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-size-cell", index)),
                if locked {
                    Sense::hover()
                } else {
                    Sense::click()
                },
            );
            let name = tr!("brush-size-of", size = size_text(*size));
            let chosen = (now - size).abs() < 0.01;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::ListItem, !locked, chosen, &name)
            });
            let radius = theme.radius(Radius::Sm);
            let fill = if chosen {
                Some(theme.fill_selected())
            } else if response.hovered() && !locked {
                Some(theme.fill_hover())
            } else {
                None
            };
            if let Some(fill) = fill {
                painter.rect_filled(rect, radius, fill);
            }
            let dot = rect
                .shrink2(egui::Vec2::Y * theme.space(Space::S3))
                .translate(egui::Vec2::Y * -theme.space(Space::S2));
            // Sizes up to 30 grow; larger ones fill the cell.
            let extent =
                (size / 30.0).clamp(0.0, 1.0).sqrt() * dot.height().min(dot.width()) * 0.48;
            painter.circle_filled(dot.center(), extent.max(0.8), theme.text);
            painter.text(
                rect.center_bottom() - egui::Vec2::Y * (theme.space(Space::S1) / 2.0),
                Align2::CENTER_BOTTOM,
                size_text(*size),
                theme.text_font(TextRole::Micro),
                // The chosen size reads on its fill in every preset.
                fill.map_or_else(|| theme.text_muted(), |fill| theme.text_muted_on(fill)),
            );
            sue_wm::ui::focus_ring(ui, &response, radius, theme);
            if response.on_hover_text(&name).clicked() {
                take = Some(*size);
            }
        }
        if let Some(size) = take
            && let Some(sub) = self.toolbox.settings_mut()
        {
            sub.tip.size = size;
        }
    }

    /// Clip Studio Paint's Tool Sliders: the brush size and opacity as two
    /// tall sliders side by side, their values above them.
    pub(crate) fn size_column(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .vline(rect.right(), rect.y_range(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("tool-sliders"), Icon::Sliders, true)],
            0,
            theme,
        );
        if !self.has_brush_size() {
            return;
        }
        let locked = self.toolbox.current().locked;
        let tip = self.toolbox.current().tip;
        let half = body.width() / 2.0;
        let label = theme.space(Space::S8) - theme.space(Space::S1);
        let lanes = [
            Rect::from_min_max(body.min, pos2(body.left() + half, body.bottom())),
            Rect::from_min_max(pos2(body.left() + half, body.top()), body.max),
        ];
        let disc_radius = theme.space(Space::S5) + theme.space(Space::S1) / 2.0;
        let value_font = theme.text_font(TextRole::Micro);
        // Size, from small at the bottom to large at the top, its value in
        // a disc as Clip Studio Paint shows it.
        let (low, high) = (0.5_f32.ln(), 1000.0_f32.ln());
        let disc = pos2(lanes[0].center().x, lanes[0].top() + label / 2.0);
        ui.painter()
            .circle_filled(disc, disc_radius, theme.surface(3));
        ui.painter().circle_stroke(
            disc,
            disc_radius,
            Stroke::new(theme.hairline().width, theme.border),
        );
        ui.painter().text(
            disc,
            Align2::CENTER_CENTER,
            format!("{:.1}\npx", tip.size),
            value_font.clone(),
            theme.text,
        );
        let at = (tip.size.max(0.5).ln() - low) / (high - low);
        let size = tall_slider(ui, lanes[0], label, at, false, "sue-paint-tool-size", theme)
            .map(|t| (low + t * (high - low)).exp());
        // Opacity, its value in a disc as Clip Studio Paint shows it.
        let disc = pos2(lanes[1].center().x, lanes[1].top() + label / 2.0);
        ui.painter().circle_filled(disc, disc_radius, theme.text);
        ui.painter().text(
            disc,
            Align2::CENTER_CENTER,
            format!("{:.0}\n%", tip.opacity * 100.0),
            value_font,
            theme.panel,
        );
        let opacity = tall_slider(
            ui,
            lanes[1],
            label,
            tip.opacity,
            true,
            "sue-paint-tool-opacity",
            theme,
        );
        if locked {
            return;
        }
        if let Some(sub) = self.toolbox.settings_mut() {
            if let Some(size) = size {
                sub.tip.size = size;
            }
            if let Some(opacity) = opacity {
                sub.tip.opacity = opacity;
            }
        }
    }
}
