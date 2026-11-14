//! The color palettes: the wheel, the sliders, intermediate and approximate colors.

use egui::{Align2, Color32, Id, Rect, RichText, Sense, StrokeKind, Ui, pos2, vec2};
use sue_wm::Theme;

use crate::color::{self, Hsv};
use crate::icons::Icon;
use crate::panels::{child, slider_row};
use crate::studio::SuePaint;

impl SuePaint {
    pub(crate) fn color_panel(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        use crate::dock::Palette;
        let tabs = [
            (tr!("color-wheel"), Icon::Wheel, true, None),
            (
                tr!("color-slider"),
                Icon::ColorSlider,
                true,
                Some(Palette::ColorSlider),
            ),
            (
                tr!("intermediate-color"),
                Icon::Intermediate,
                true,
                Some(Palette::Intermediate),
            ),
            (
                tr!("approximate-color"),
                Icon::Approximate,
                true,
                Some(Palette::Approximate),
            ),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.color_tab, theme);
        self.color_tab = tab;
        let body = body.shrink(8.0);
        match self.color_tab {
            0 => self.color_wheel(ui, body, theme),
            2 => self.intermediate_colors(ui, body, theme),
            3 => self.approximate_colors(ui, body, theme),
            _ => {
                let mut inner = child(ui, body, "color-sliders");
                self.color_sliders(&mut inner, theme);
            }
        }
    }

    /// A grid of `colors(column, row)`, `columns` by `rows` in `area`; a
    /// click picks a color. The cell picked, if any.
    fn color_grid(
        ui: &mut Ui,
        area: Rect,
        columns: u32,
        rows: u32,
        id: &str,
        colors: impl Fn(u32, u32) -> [u8; 3],
    ) -> Option<[u8; 3]> {
        let cell = vec2(area.width() / columns as f32, area.height() / rows as f32);
        for row in 0..rows {
            for column in 0..columns {
                let [r, g, b] = colors(column, row);
                let rect = Rect::from_min_size(
                    area.min + vec2(column as f32 * cell.x, row as f32 * cell.y),
                    cell,
                );
                ui.painter()
                    .rect_filled(rect.shrink(0.5), 0.0, Color32::from_rgb(r, g, b));
            }
        }
        let response = ui.interact(
            area,
            Id::unique(("sue-paint-color-grid", id)),
            Sense::click(),
        );
        let at = response
            .interact_pointer_pos()
            .filter(|_| response.clicked())?;
        let column = (((at.x - area.left()) / cell.x) as u32).min(columns - 1);
        let row = (((at.y - area.top()) / cell.y) as u32).min(rows - 1);
        Some(colors(column, row))
    }

    /// Clip Studio Paint's Intermediate Color palette: the colors between
    /// four corners; a click on a corner sets it to the drawing color.
    pub(super) fn intermediate_colors(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let (corners, steps) = self.intermediate;
        let steps = steps.clamp(2, 24);
        let corner = 16.0;
        let side = (body.width() - corner * 2.0)
            .min(body.height() - corner * 2.0 - 30.0)
            .max(40.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top() + corner),
            vec2(side, side),
        );
        let mix = |a: [u8; 3], b: [u8; 3], t: f32| {
            [0, 1, 2].map(|channel| {
                (f32::from(a[channel]) + (f32::from(b[channel]) - f32::from(a[channel])) * t)
                    .round() as u8
            })
        };
        let picked = Self::color_grid(ui, area, steps, steps, "intermediate", |column, row| {
            let (u, v) = (
                column as f32 / (steps - 1) as f32,
                row as f32 / (steps - 1) as f32,
            );
            mix(
                mix(corners[0], corners[1], u),
                mix(corners[2], corners[3], u),
                v,
            )
        });
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        // The corners: click one to give it the drawing color.
        let drawing = self.drawing_color();
        for (index, at) in [
            area.left_top() - vec2(corner, corner),
            area.right_top() + vec2(0.0, -corner),
            area.left_bottom() + vec2(-corner, 0.0),
            area.right_bottom(),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::from_min_size(at, vec2(corner, corner));
            let [r, g, b] = corners[index];
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            ui.painter()
                .rect_stroke(rect, 2.0, theme.hairline(), StrokeKind::Inside);
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-intermediate-corner", index)),
                Sense::click(),
            );
            if response
                .on_hover_text(tr!("set-to-the-drawing-color"))
                .clicked()
            {
                self.intermediate.0[index] = drawing;
            }
        }
        let mut steps = self.intermediate.1 as f32;
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), area.bottom() + corner + 4.0), body.max),
            "intermediate-steps",
        );
        if slider_row(
            &mut inner,
            &tr!("steps-2"),
            &mut steps,
            2.0..=24.0,
            "",
            false,
            theme,
        ) {
            self.intermediate.1 = steps.round() as u32;
        }
    }

    /// Clip Studio Paint's Approximate Color palette: colors near the
    /// drawing color, changing one way across and another down.
    pub(super) fn approximate_colors(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let (across, down, reach) = self.approximate;
        let base = Hsv::from_rgb(color::unit(self.drawing_color()));
        let steps = 9_u32;
        let side = body.width().min(body.height() - 64.0).max(40.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top()),
            vec2(side, side),
        );
        let shift = |hsv: Hsv, axis: u8, t: f32| {
            let mut hsv = hsv;
            match axis {
                0 => hsv.h = (hsv.h + t * reach * 180.0).rem_euclid(360.0),
                1 => hsv.s = (hsv.s + t * reach).clamp(0.0, 1.0),
                _ => hsv.v = (hsv.v - t * reach).clamp(0.0, 1.0),
            }
            hsv
        };
        let picked = Self::color_grid(ui, area, steps, steps, "approximate", |column, row| {
            let (u, v) = (
                column as f32 / (steps - 1) as f32 * 2.0 - 1.0,
                row as f32 / (steps - 1) as f32 * 2.0 - 1.0,
            );
            color::bytes(shift(shift(base, across, u), down, v).to_rgb())
        });
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), area.bottom() + 6.0), body.max),
            "approximate-axes",
        );
        let names = [tr!("hue"), tr!("saturation"), tr!("brightness")];
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(tr!("across"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            egui::ComboBox::from_id_salt("sue-paint-approximate-across")
                .selected_text(names[usize::from(across.min(2))].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.approximate.0, index as u8, name.as_str());
                    }
                });
            ui.label(
                RichText::new(tr!("down"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            egui::ComboBox::from_id_salt("sue-paint-approximate-down")
                .selected_text(names[usize::from(down.min(2))].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.approximate.1, index as u8, name.as_str());
                    }
                });
        });
        let mut spread = reach * 100.0;
        if slider_row(
            &mut inner,
            &tr!("range"),
            &mut spread,
            5.0..=100.0,
            " %",
            false,
            theme,
        ) {
            self.approximate.2 = spread / 100.0;
        }
    }

    fn color_wheel(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        // Room under the wheel for the color circles and the pill.
        let side = (body.width()).min(body.height() - 70.0).max(60.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top()),
            vec2(side, side),
        );
        // Sue's own color wheel, the one every color control shares.
        let [r, g, b] = self.drawing_color();
        let mut rgba = [r, g, b, 255];
        let mut wheel = ui.new_child(egui::UiBuilder::new().max_rect(area));
        let pick = sue_wm::color_wheel(&mut wheel, Id::unique("sue-paint-wheel"), &mut rgba, side);
        if pick.changed {
            let [h, s, _] = sue_wm::hsv_of([rgba[0], rgba[1], rgba[2]]);
            if s > 0.0 {
                self.hue = h;
            }
            self.set_drawing_color([rgba[0], rgba[1], rgba[2]]);
        }
        // Chips and numbers below.
        let row = Rect::from_min_max(pos2(body.left(), area.bottom() + 6.0), body.max);
        self.color_circles(ui, row.min + vec2(14.0, 13.0), theme);
        // Red, green and blue, each behind a chip of its own, as Clip
        // Studio Paint shows them under its wheel.
        let rgb = self.drawing_color();
        let chips = [
            Color32::from_rgb(200, 40, 40),
            Color32::from_rgb(40, 160, 60),
            Color32::from_rgb(40, 70, 200),
        ];
        let step = ((body.right() - row.left() - 56.0) / 3.0).clamp(28.0, 44.0);
        for (index, (value, chip)) in rgb.iter().zip(chips).enumerate() {
            let x = row.left() + 54.0 + index as f32 * step;
            let mark = Rect::from_center_size(pos2(x + 3.0, row.top() + 14.0), vec2(6.0, 6.0));
            ui.painter().rect_filled(mark, 1.0, chip);
            ui.painter().text(
                pos2(x + 10.0, row.top() + 14.0),
                Align2::LEFT_CENTER,
                value.to_string(),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
    }

    pub(super) fn color_sliders(&mut self, ui: &mut Ui, theme: &Theme) {
        let rgb = self.drawing_color();
        let mut values = rgb.map(f32::from);
        let mut changed = false;
        for (index, name) in ["R", "G", "B"].iter().enumerate() {
            changed |= slider_row(ui, name, &mut values[index], 0.0..=255.0, "", false, theme);
        }
        let mut hsv = Hsv::from_rgb(color::unit(rgb));
        let mut hue = if hsv.s > 0.0 { hsv.h } else { self.hue };
        let mut saturation = hsv.s * 100.0;
        let mut value = hsv.v * 100.0;
        let mut hsv_changed = slider_row(ui, "H", &mut hue, 0.0..=359.0, "", false, theme);
        hsv_changed |= slider_row(ui, "S", &mut saturation, 0.0..=100.0, "", false, theme);
        hsv_changed |= slider_row(ui, "V", &mut value, 0.0..=100.0, "", false, theme);
        if changed {
            self.set_drawing_color(values.map(|value| value.round() as u8));
        } else if hsv_changed {
            hsv = Hsv {
                h: hue,
                s: saturation / 100.0,
                v: value / 100.0,
            };
            self.hue = hue;
            self.set_drawing_color(color::bytes(hsv.to_rgb()));
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr!("hex"))
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            );
            let response = ui.add(egui::TextEdit::singleline(&mut self.hex).desired_width(90.0));
            if response.lost_focus()
                && let Some(rgb) = color::parse_hex(&self.hex)
            {
                self.set_drawing_color(rgb);
            }
        });
    }
}
