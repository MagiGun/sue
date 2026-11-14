//! The brush size palette and the brush size sliders column.

use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_wm::Theme;

use crate::icons::Icon;
use crate::panels::{header, tall_slider};
use crate::studio::SuePaint;
use crate::tools::Tool;

impl SuePaint {
    /// Brush Size: Clip Studio Paint's grid of sizes, each a dot of its
    /// size (up to the cell) over its number; a click takes it.
    pub(super) fn brush_size_grid(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        const SIZES: [f32; 40] = [
            0.7, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 15.0, 17.0, 20.0,
            25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0, 120.0, 150.0, 170.0, 200.0, 250.0,
            300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 1000.0, 1200.0, 1500.0, 1700.0, 2000.0,
        ];
        let tool = self.toolbox.tool();
        if !tool.paints() && !matches!(tool, Tool::Fill | Tool::Gradient) {
            ui.painter().text(
                body.center_top() + vec2(0.0, 16.0),
                Align2::CENTER_CENTER,
                tr!("no-brush-size-for-this-tool"),
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            return;
        }
        let columns = ((body.width() / 34.0).floor() as usize).clamp(3, 8);
        let cell = body.width() / columns as f32;
        let now = self.toolbox.current().tip.size;
        let mut take = None;
        let painter = ui.painter().with_clip_rect(body);
        for (index, size) in SIZES.iter().enumerate() {
            let (row, column) = (index / columns, index % columns);
            let rect = Rect::from_min_size(
                pos2(
                    body.left() + column as f32 * cell,
                    body.top() + row as f32 * (cell + 4.0),
                ),
                vec2(cell, cell + 4.0),
            )
            .shrink(1.0);
            if rect.top() > body.bottom() {
                break;
            }
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-size-cell", index)),
                Sense::click(),
            );
            let chosen = (now - size).abs() < 0.01;
            if chosen {
                painter.rect_filled(rect, theme.control_radius, theme.active);
            } else if response.hovered() {
                painter.rect_filled(rect, theme.control_radius, theme.hover);
            }
            let dot = rect.shrink2(vec2(0.0, 7.0)).translate(vec2(0.0, -5.0));
            // Sizes up to 30 grow; larger ones fill the cell.
            let radius =
                (size / 30.0).clamp(0.0, 1.0).sqrt() * dot.height().min(dot.width()) * 0.48;
            painter.circle_filled(dot.center(), radius.max(0.8), theme.text);
            painter.text(
                rect.center_bottom() - vec2(0.0, 1.0),
                Align2::CENTER_BOTTOM,
                if size.fract() == 0.0 {
                    format!("{size:.0}")
                } else {
                    format!("{size}")
                },
                theme.font(theme.small_size - 1.0),
                theme.muted,
            );
            if response.clicked() {
                take = Some(*size);
            }
        }
        if let Some(size) = take {
            self.toolbox.current_mut().tip.size = size;
        }
    }

    /// Clip Studio Paint's Tool Sliders: the brush size and opacity as two
    /// tall sliders side by side, their values above them.
    pub(crate) fn size_column(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .vline(rect.right(), rect.y_range(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("tool-sliders"), Icon::Sliders, true)],
            0,
            theme,
        );
        let tool = self.toolbox.tool();
        if !tool.paints() && !matches!(tool, Tool::Fill | Tool::Gradient) {
            return;
        }
        let tip = &mut self.toolbox.current_mut().tip;
        let half = body.width() / 2.0;
        let label = 30.0;
        let lanes = [
            Rect::from_min_max(body.min, pos2(body.left() + half, body.bottom())),
            Rect::from_min_max(pos2(body.left() + half, body.top()), body.max),
        ];
        // Size, from small at the bottom to large at the top, its value in
        // a gray disc as Clip Studio Paint shows it.
        let (low, high) = (0.5_f32.ln(), 1000.0_f32.ln());
        let disc = pos2(lanes[0].center().x, lanes[0].top() + label / 2.0);
        ui.painter().circle_filled(disc, 13.0, theme.void);
        ui.painter()
            .circle_stroke(disc, 13.0, Stroke::new(1.0, theme.border));
        ui.painter().text(
            disc,
            Align2::CENTER_CENTER,
            format!("{:.1}\npx", tip.size),
            theme.font(theme.small_size - 1.0),
            Color32::WHITE,
        );
        let at = (tip.size.max(0.5).ln() - low) / (high - low);
        if let Some(t) = tall_slider(ui, lanes[0], label, at, false, "sue-paint-tool-size", theme) {
            tip.size = (low + t * (high - low)).exp();
        }
        // Opacity, its value in a black disc as Clip Studio Paint shows it.
        let disc = pos2(lanes[1].center().x, lanes[1].top() + label / 2.0);
        ui.painter().circle_filled(disc, 13.0, Color32::BLACK);
        ui.painter().text(
            disc,
            Align2::CENTER_CENTER,
            format!("{:.0}\n%", tip.opacity * 100.0),
            theme.font(theme.small_size - 1.0),
            Color32::WHITE,
        );
        if let Some(t) = tall_slider(
            ui,
            lanes[1],
            label,
            tip.opacity,
            true,
            "sue-paint-tool-opacity",
            theme,
        ) {
            tip.opacity = t;
        }
    }
}
