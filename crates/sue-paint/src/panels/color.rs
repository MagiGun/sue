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

/// How many colors a color slider's track is painted with.
const STOPS: usize = 13;

/// Where the Color Wheel palette puts its parts in `body`: the wheel as
/// large as the palette allows, the main and sub color circles at its
/// lower left, and the drawing color's red, green and blue as a small
/// table at its lower right, so the three read as one balanced group.
/// A palette too narrow for that puts the circles and the numbers in a
/// row under the wheel.
struct WheelLayout {
    /// The wheel's square.
    wheel: Rect,
    /// The center of the main color circle (see `color_circles`).
    main_at: egui::Pos2,
    /// One cell per channel: the letter at its left, the number at its
    /// right.
    values: [Rect; 3],
}

impl WheelLayout {
    fn new(ui: &Ui, body: Rect, theme: &Theme) -> Self {
        // The color circles' reach around the main circle's center, as
        // `color_circles` draws them: the sub circle down and right, the
        // swap button above it, the transparent color under both.
        let radius = theme.space(Space::S5);
        let swap = theme.space(Space::S5) + theme.space(Space::S1);
        let above = radius + swap * 0.85;
        let below = radius * 3.2 + swap;
        let circles = vec2(radius * 3.25, above + below);
        // A number cell: a letter, a gap and three digits.
        let font = theme.text_font(TextRole::Body);
        let painter = ui.painter();
        let measure = |text: &str| {
            painter
                .layout_no_wrap(text.to_owned(), font.clone(), theme.text)
                .size()
        };
        let digits = measure("888");
        let letter = measure("G");
        let gap = theme.space(Space::S4);
        let cell = vec2(
            letter.x + theme.space(Space::S3) + digits.x,
            digits.y.max(letter.y) + theme.space(Space::S2),
        );
        let row = Density::of(ui).control_height().max(cell.y);
        // Beside: circles, wheel, numbers in one band.
        let flanks = circles.x.max(cell.x) * 2.0 + gap * 2.0;
        let beside = (body.width() - flanks).min(body.height());
        // Under: the wheel above a band that holds the circles.
        let under = body.width().min(body.height() - circles.y - gap);
        if beside >= under {
            let side = beside.max(circles.y);
            let wheel = Rect::from_center_size(
                pos2(body.center().x, body.top() + side / 2.0),
                vec2(side, side),
            );
            // Both flanks sit on the wheel's lower edge, centered in the
            // room beside it.
            let flank = (wheel.left() - body.left()).max(circles.x);
            let main_at = pos2(
                body.left() + (flank - circles.x) / 2.0 + radius,
                wheel.bottom() - below,
            );
            let x = wheel.right() + (body.right() - wheel.right() - cell.x) / 2.0;
            let top = wheel.bottom() - row * 3.0;
            let values = [0.0, 1.0, 2.0].map(|line| {
                Rect::from_min_size(pos2(x, top + line * row + (row - cell.y) / 2.0), cell)
            });
            Self {
                wheel,
                main_at,
                values,
            }
        } else {
            let side = under.max(row * 3.0);
            let wheel = Rect::from_min_size(
                pos2(body.center().x - side / 2.0, body.top()),
                vec2(side, side),
            );
            let band = wheel.bottom() + gap;
            let main_at = pos2(wheel.left() + radius, band + above);
            // The three cells spread over the rest of the wheel's width,
            // level with the circles' middle.
            let start = main_at.x + circles.x - radius + gap;
            let room = (wheel.right() - start).max(cell.x * 3.0);
            let step = (room - cell.x) / 2.0;
            let y = band + circles.y / 2.0 - cell.y / 2.0;
            let values = [0.0, 1.0, 2.0]
                .map(|index| Rect::from_min_size(pos2(start + index * step, y), cell));
            Self {
                wheel,
                main_at,
                values,
            }
        }
    }
}

/// The colors along a color slider's track: what `at(t)` gives from `t`
/// 0 to 1.
fn stops(at: impl Fn(f32) -> [u8; 3]) -> Vec<egui::Color32> {
    (0..STOPS)
        .map(|index| {
            let [r, g, b] = at(index as f32 / (STOPS - 1) as f32);
            egui::Color32::from_rgb(r, g, b)
        })
        .collect()
}

/// A slider row of the color palettes: `name`, the track (painted with
/// `colors`, the color each place on it gives; none for a plain one) and
/// its field. `true` when the value changed.
#[allow(clippy::too_many_arguments, reason = "one row, said in full")]
fn slider_row(
    ui: &mut Ui,
    salt: impl egui::AsIdSalt,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f64>,
    unit: &str,
    colors: &[egui::Color32],
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let label = Density::of(ui).control_height() * LABEL_ROWS;
    let response = ui::slider(("sue-paint-color-slider", salt), &mut wide, range)
        .label(name)
        .label_width(label)
        .suffix(unit)
        .gradient(colors)
        .show(ui, theme);
    let changed = ui::tooltip(response, &ui::Tip::new(name), theme).changed();
    if changed {
        *value = wide as f32;
    }
    changed
}

impl SuePaint {
    /// Opens Color Settings on the main or the sub color when its circle,
    /// drawn by `color_circles` with the main one at `main_at` (in the
    /// tool strip and under the Color Wheel), was double clicked. The
    /// circles are found by the ids `color_circles` gives them.
    pub(super) fn color_circles_double_click(&mut self, ui: &Ui, main_at: egui::Pos2) {
        for (sub, tip) in [(false, tr!("main-color")), (true, tr!("sub-color"))] {
            let id = Id::unique((
                "sue-paint-color-circle",
                tip.as_str(),
                main_at.x as i32,
                main_at.y as i32,
            ));
            let Some(response) = ui.ctx().read_response(id) else {
                continue;
            };
            // Named, so assistive technology (and a test) finds it.
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &tip));
            if response.double_clicked() {
                self.open_color_settings(if sub {
                    crate::dialogs::ColorTarget::Sub
                } else {
                    crate::dialogs::ColorTarget::Main
                });
                return;
            }
        }
    }

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
        ui::focus_ring(ui, &response, theme.radius(Radius::Xs), theme);
        let name = match id {
            "intermediate" => tr!("intermediate-color"),
            _ => tr!("approximate-color"),
        };
        let response = ui::tooltip(
            response,
            &ui::Tip::new(&name).help(&tr!("click-a-color-to-draw-with-it")),
            theme,
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
            &[],
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
            &[],
            theme,
        ) {
            self.approximate.2 = spread / 100.0;
        }
    }

    fn color_wheel(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let layout = WheelLayout::new(ui, body, theme);
        let area = layout.wheel;
        let side = area.width();
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
        // The main and sub color beside (or under) the wheel, and the
        // numbers of the drawing color.
        self.color_circles(ui, layout.main_at, theme);
        let rgb = self.drawing_color();
        let font = theme.text_font(TextRole::Body);
        let letters = [tr!("color-r"), tr!("color-g"), tr!("color-b")];
        for ((value, letter), cell) in rgb.iter().zip(letters).zip(layout.values) {
            // The letter at the cell's left, the number at its right, so
            // the digits of the three line up whatever their count.
            ui.painter().text(
                cell.left_center(),
                Align2::LEFT_CENTER,
                letter,
                font.clone(),
                theme.text_muted(),
            );
            ui.painter().text(
                cell.right_center(),
                Align2::RIGHT_CENTER,
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
                    let colors = stops(|t| {
                        let mut at = rgb;
                        at[index] = (t * 255.0).round() as u8;
                        at
                    });
                    changed |= slider_row(
                        ui,
                        ("rgb", index),
                        name,
                        &mut values[index],
                        0.0..=255.0,
                        "",
                        &colors,
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
                let now = [values[0], values[1] / 100.0, values[2] / 100.0];
                for (index, name) in names.iter().enumerate() {
                    let (range, unit) = if index == 0 {
                        (0.0..=359.0, "\u{b0}")
                    } else {
                        (0.0..=100.0, "%")
                    };
                    // Hue shows every hue at full strength, so it reads
                    // even on a gray; saturation and value run between
                    // the color's own ends.
                    let colors = stops(|t| {
                        let [h, s, v] = match index {
                            0 => [t * 359.0, 1.0, 1.0],
                            1 => [now[0], t, now[2]],
                            _ => [now[0], now[1], t],
                        };
                        color::bytes(Hsv { h, s, v }.to_rgb())
                    });
                    changed |= slider_row(
                        ui,
                        ("hsv", index),
                        name,
                        &mut values[index],
                        range,
                        unit,
                        &colors,
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
                let inks = values.map(|ink| ink / 100.0);
                for (index, name) in names.iter().enumerate() {
                    let colors = stops(|t| {
                        let mut at = inks;
                        at[index] = t;
                        rgb_of_cmyk(at)
                    });
                    changed |= slider_row(
                        ui,
                        ("cmyk", index),
                        name,
                        &mut values[index],
                        0.0..=100.0,
                        "%",
                        &colors,
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
            let response = ui::tooltip(response, &ui::Tip::new(&tr!("hex")), theme);
            if response.lost_focus() {
                match color::parse_hex(&self.hex) {
                    Some(rgb) => self.set_drawing_color(rgb),
                    None => self.hex = color::hex(self.drawing_color()),
                }
            }
        });
    }
}
