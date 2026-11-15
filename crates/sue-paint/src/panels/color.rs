//! The color palettes: the wheel, the sliders, intermediate and
//! approximate colors. Every control is one of `sue_wm::ui`'s, so each
//! takes the keyboard, shows its focus ring and names itself.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use sue_wm::Theme;
use sue_wm::ui::{self, Density, Radius, Space, TextRole};

use crate::color::{self, Hsv};
use crate::colorsets::SliderMode;
use crate::dialogs::{cmyk_of, rgb_of_cmyk};
use crate::icons::Icon;
use crate::panels::child;
use crate::studio::SuePaint;

/// How many control heights the label column of a slider row is wide.
const LABEL_ROWS: f32 = 0.9;

/// A slider row of the color palettes: `name`, the track and its field.
/// `true` when the value changed.
fn slider_row(
    ui: &mut Ui,
    salt: impl egui::AsIdSalt,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f64>,
    unit: &str,
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let label = Density::of(ui).control_height() * LABEL_ROWS;
    let changed = ui::slider(("sue-paint-color-slider", salt), &mut wide, range)
        .label(name)
        .label_width(label)
        .suffix(unit)
        .show(ui, theme)
        .changed();
    if changed {
        *value = wide as f32;
    }
    changed
}

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
        let body = body.shrink(theme.space(Space::S4));
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
        (columns, rows): (u32, u32),
        id: &str,
        theme: &Theme,
        colors: impl Fn(u32, u32) -> [u8; 3],
    ) -> Option<[u8; 3]> {
        let cell = vec2(area.width() / columns as f32, area.height() / rows as f32);
        let gap = theme.hairline().width / 2.0;
        for row in 0..rows {
            for column in 0..columns {
                let [r, g, b] = colors(column, row);
                let rect = Rect::from_min_size(
                    area.min + vec2(column as f32 * cell.x, row as f32 * cell.y),
                    cell,
                );
                ui.painter().rect_filled(
                    rect.shrink(gap),
                    CornerRadius::ZERO,
                    egui::Color32::from_rgb(r, g, b),
                );
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
        let density = Density::of(ui);
        let corner = density.icon();
        let gap = theme.space(Space::S2);
        let footer = density.control_height() + theme.space(Space::S4);
        let side = (body.width() - corner * 2.0)
            .min(body.height() - corner * 2.0 - footer)
            .max(corner * 2.0);
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
        let picked = Self::color_grid(
            ui,
            area,
            (steps, steps),
            "intermediate",
            theme,
            |column, row| {
                let (u, v) = (
                    column as f32 / (steps - 1) as f32,
                    row as f32 / (steps - 1) as f32,
                );
                mix(
                    mix(corners[0], corners[1], u),
                    mix(corners[2], corners[3], u),
                    v,
                )
            },
        );
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        // The corners: click one to give it the drawing color.
        let drawing = self.drawing_color();
        let names = [
            tr!("corner-top-left"),
            tr!("corner-top-right"),
            tr!("corner-bottom-left"),
            tr!("corner-bottom-right"),
        ];
        for (index, at) in [
            area.left_top() - vec2(corner, corner),
            pos2(area.right(), area.top() - corner),
            pos2(area.left() - corner, area.bottom()),
            area.right_bottom(),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::from_min_size(at, vec2(corner, corner)).shrink(gap / 2.0);
            let [r, g, b] = corners[index];
            sue_wm::swatch(ui, rect, [r, g, b, u8::MAX], theme);
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-intermediate-corner", index)),
                Sense::click(),
            );
            ui::focus_ring(ui, &response, theme.radius(Radius::Xs), theme);
            let response = ui::tooltip(
                response,
                &ui::Tip::new(&names[index]).help(&tr!("set-to-the-drawing-color")),
                theme,
            );
            if response.clicked() {
                self.intermediate.0[index] = drawing;
            }
        }
        let mut steps = self.intermediate.1 as f32;
        let mut inner = child(
            ui,
            Rect::from_min_max(
                pos2(body.left(), area.bottom() + corner + theme.space(Space::S2)),
                body.max,
            ),
            "intermediate-steps",
        );
        if slider_row(
            &mut inner,
            "intermediate-steps",
            &tr!("steps-2"),
            &mut steps,
            2.0..=24.0,
            "",
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
        let density = Density::of(ui);
        let footer = density.control_height() * 2.0 + theme.space(Space::S5);
        let side = body
            .width()
            .min(body.height() - footer)
            .max(density.control_height() * 2.0);
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
        let picked = Self::color_grid(
            ui,
            area,
            (steps, steps),
            "approximate",
            theme,
            |column, row| {
                let (u, v) = (
                    column as f32 / (steps - 1) as f32 * 2.0 - 1.0,
                    row as f32 / (steps - 1) as f32 * 2.0 - 1.0,
                );
                color::bytes(shift(shift(base, across, u), down, v).to_rgb())
            },
        );
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        let mut inner = child(
            ui,
            Rect::from_min_max(
                pos2(body.left(), area.bottom() + theme.space(Space::S3)),
                body.max,
            ),
            "approximate-axes",
        );
        let names = [tr!("hue"), tr!("saturation"), tr!("brightness")];
        inner.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space(Space::S2);
            let half = (ui.available_width() - theme.space(Space::S2)) / 2.0;
            for (salt, label, axis) in [
                ("across", tr!("across"), &mut self.approximate.0),
                ("down", tr!("down"), &mut self.approximate.1),
            ] {
                let mut chosen = usize::from((*axis).min(2));
                let response = ui::dropdown(("sue-paint-approximate", salt), &mut chosen, &names)
                    .label(&label)
                    .width(half)
                    .show(ui, theme);
                let response = ui::tooltip(response, &ui::Tip::new(&label), theme);
                if response.changed() {
                    *axis = u8::try_from(chosen).unwrap_or(0);
                }
            }
        });
        let mut spread = reach * 100.0;
        if slider_row(
            &mut inner,
            "approximate-range",
            &tr!("range"),
            &mut spread,
            5.0..=100.0,
            "%",
            theme,
        ) {
            self.approximate.2 = spread / 100.0;
        }
    }

    fn color_wheel(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        // Room under the wheel for the color circles and the numbers.
        let density = Density::of(ui);
        let below = density.control_height() * 3.0;
        let side = body.width().min(body.height() - below).max(below);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top()),
            vec2(side, side),
        );
        // Sue's own color wheel, the one every color control shares.
        let [r, g, b] = self.drawing_color();
        let mut rgba = [r, g, b, u8::MAX];
        let mut wheel = ui.new_child(UiBuilder::new().max_rect(area));
        let pick = sue_wm::color_wheel(&mut wheel, Id::unique("sue-paint-wheel"), &mut rgba, side);
        if pick.changed {
            let [h, s, _] = sue_wm::hsv_of([rgba[0], rgba[1], rgba[2]]);
            if s > 0.0 {
                self.hue = h;
            }
            self.set_drawing_color([rgba[0], rgba[1], rgba[2]]);
        }
        // The main and sub color under the wheel, then the numbers.
        let row = Rect::from_min_max(
            pos2(body.left(), area.bottom() + theme.space(Space::S3)),
            body.max,
        );
        let circles = theme.space(Space::S6);
        self.color_circles(ui, row.min + vec2(circles, circles), theme);
        let rgb = self.drawing_color();
        let font = theme.text_font(TextRole::Caption);
        let start = row.left() + circles * 3.0 + theme.space(Space::S2);
        let step = ((body.right() - start) / 3.0).max(circles * 2.0);
        let letters = [tr!("color-r"), tr!("color-g"), tr!("color-b")];
        for (index, (value, letter)) in rgb.iter().zip(letters).enumerate() {
            let x = start + index as f32 * step;
            let y = row.top() + circles;
            let name = ui.painter().text(
                pos2(x, y),
                Align2::LEFT_CENTER,
                letter,
                font.clone(),
                theme.text_muted(),
            );
            ui.painter().text(
                pos2(name.right() + theme.space(Space::S1), y),
                Align2::LEFT_CENTER,
                value.to_string(),
                font.clone(),
                theme.text,
            );
        }
    }

    /// The Color Slider palette: a switch between hue, saturation and
    /// value; red, green and blue; and cyan, magenta, yellow and black,
    /// their sliders, and the hex code.
    pub(super) fn color_sliders(&mut self, ui: &mut Ui, theme: &Theme) {
        let modes: Vec<String> = SliderMode::ALL.iter().map(|mode| mode.label()).collect();
        let chosen = SliderMode::ALL
            .iter()
            .position(|mode| *mode == self.color_ui.slider_mode);
        let picked = ui::segmented("sue-paint-color-slider-mode", &modes, chosen)
            .full_width()
            .show(ui, theme)
            .picked;
        if let Some(mode) = picked.and_then(|index| SliderMode::ALL.get(index)) {
            self.color_ui.slider_mode = *mode;
        }
        ui.add_space(theme.space(Space::S2));
        let rgb = self.drawing_color();
        let mut next = None;
        match self.color_ui.slider_mode {
            SliderMode::Rgb => {
                let mut values = rgb.map(f32::from);
                let mut changed = false;
                let names = [tr!("color-r"), tr!("color-g"), tr!("color-b")];
                for (index, name) in names.iter().enumerate() {
                    changed |= slider_row(
                        ui,
                        ("rgb", index),
                        name,
                        &mut values[index],
                        0.0..=255.0,
                        "",
                        theme,
                    );
                }
                if changed {
                    next = Some(values.map(|value| value.round().clamp(0.0, 255.0) as u8));
                }
            }
            SliderMode::Hsv => {
                let hsv = Hsv::from_rgb(color::unit(rgb));
                let mut values = [
                    if hsv.s > 0.0 { hsv.h } else { self.hue },
                    hsv.s * 100.0,
                    hsv.v * 100.0,
                ];
                let mut changed = false;
                let names = [tr!("color-h"), tr!("color-s"), tr!("color-v")];
                for (index, name) in names.iter().enumerate() {
                    let (range, unit) = if index == 0 {
                        (0.0..=359.0, "\u{b0}")
                    } else {
                        (0.0..=100.0, "%")
                    };
                    changed |= slider_row(
                        ui,
                        ("hsv", index),
                        name,
                        &mut values[index],
                        range,
                        unit,
                        theme,
                    );
                }
                if changed {
                    self.hue = values[0];
                    next = Some(color::bytes(
                        Hsv {
                            h: values[0],
                            s: values[1] / 100.0,
                            v: values[2] / 100.0,
                        }
                        .to_rgb(),
                    ));
                }
            }
            SliderMode::Cmyk => {
                let mut values = cmyk_of(rgb).map(|ink| ink * 100.0);
                let mut changed = false;
                let names = [
                    tr!("color-c"),
                    tr!("color-m"),
                    tr!("color-y"),
                    tr!("color-k"),
                ];
                for (index, name) in names.iter().enumerate() {
                    changed |= slider_row(
                        ui,
                        ("cmyk", index),
                        name,
                        &mut values[index],
                        0.0..=100.0,
                        "%",
                        theme,
                    );
                }
                if changed {
                    next = Some(rgb_of_cmyk(values.map(|ink| ink / 100.0)));
                }
            }
        }
        if let Some(rgb) = next {
            let hue = self.hue;
            self.set_drawing_color(rgb);
            // A hue set on its slider stays while the color is gray.
            if self.color_ui.slider_mode == SliderMode::Hsv {
                self.hue = hue;
            }
        }
        ui.add_space(theme.space(Space::S2));
        let label = Density::of(ui).control_height() * LABEL_ROWS;
        ui::form_row(ui, &tr!("hex"), label, theme, |ui| {
            let response = ui::text_field("sue-paint-color-hex", &mut self.hex)
                .label(&tr!("hex"))
                .show(ui, theme);
            if response.lost_focus() {
                match color::parse_hex(&self.hex) {
                    Some(rgb) => self.set_drawing_color(rgb),
                    None => self.hex = color::hex(self.drawing_color()),
                }
            }
        });
    }
}
