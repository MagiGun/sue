//! The color wheel every color control shares: a hue ring around a
//! saturation and value square, an opacity bar, and a swatch button that
//! opens it, so a color is picked by eye and never typed as a hex code.

use egui::{
    Color32, Id, Mesh, Order, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::Theme;

/// What a color control did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColorPick {
    /// The color changed.
    pub changed: bool,
    /// The pointer is still dragging, so the change is a preview.
    pub dragging: bool,
    /// A drag ended this frame: commit what it previewed.
    pub released: bool,
}

impl ColorPick {
    fn merge(self, other: Self) -> Self {
        Self {
            changed: self.changed || other.changed,
            dragging: self.dragging || other.dragging,
            released: self.released || other.released,
        }
    }
}

/// Hue (0 to 360), saturation and value (0 to 1) of sRGB bytes. Gray keeps
/// hue 0.
#[must_use]
pub fn hsv_of([r, g, b]: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|value| f32::from(value) / 255.0);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let h = if delta <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    [h, if max <= 0.0 { 0.0 } else { delta / max }, max]
}

/// sRGB bytes of hue (0 to 360), saturation and value (0 to 1).
#[must_use]
pub fn rgb_of([h, s, v]: [f32; 3]) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m].map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn opaque([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// A checkerboard under translucent colors.
fn checker(ui: &Ui, rect: Rect) {
    let cell = 6.0;
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(200));
    let (columns, rows) = (
        (rect.width() / cell).ceil() as usize,
        (rect.height() / cell).ceil() as usize,
    );
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            let min = rect.min + vec2(column as f32 * cell, row as f32 * cell);
            painter.rect_filled(
                Rect::from_min_size(min, vec2(cell, cell)),
                0.0,
                Color32::from_gray(150),
            );
        }
    }
}

/// A hue ring around a saturation and value square, `side` points across,
/// editing `rgba` in place. The hue of a gray is remembered under `id`, so
/// dragging into gray and back keeps it.
pub fn color_wheel(ui: &mut Ui, id: Id, rgba: &mut [u8; 4], side: f32) -> ColorPick {
    let (area, response) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let center = area.center();
    let outer = side / 2.0;
    let inner = outer * 0.8;
    let hue_id = id.with("hue");
    let mut hsv = hsv_of([rgba[0], rgba[1], rgba[2]]);
    if hsv[1] <= 0.0 || hsv[2] <= 0.0 {
        hsv[0] = ui.data(|data| data.get_temp(hue_id)).unwrap_or(hsv[0]);
    }

    // The hue ring.
    let mut ring = Mesh::default();
    let steps = 96;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU;
        let direction = vec2(angle.cos(), angle.sin());
        let shade = opaque(rgb_of([t * 360.0, 1.0, 1.0]));
        ring.colored_vertex(center + direction * inner, shade);
        ring.colored_vertex(center + direction * outer, shade);
        if step > 0 {
            let base = (step * 2) as u32;
            ring.add_triangle(base - 2, base - 1, base);
            ring.add_triangle(base - 1, base + 1, base);
        }
    }
    ui.painter().add(Shape::mesh(ring));

    // The saturation and value square.
    let half = inner * 0.68;
    let square = Rect::from_center_size(center, vec2(half * 2.0, half * 2.0));
    let mut mesh = Mesh::default();
    let cells = 24;
    for row in 0..=cells {
        for column in 0..=cells {
            let (s, v) = (
                column as f32 / cells as f32,
                1.0 - row as f32 / cells as f32,
            );
            mesh.colored_vertex(
                pos2(
                    square.left() + s * square.width(),
                    square.top() + (1.0 - v) * square.height(),
                ),
                opaque(rgb_of([hsv[0], s, v])),
            );
            if row > 0 && column > 0 {
                let at = (row * (cells + 1) + column) as u32;
                let up = at - (cells + 1) as u32;
                mesh.add_triangle(up - 1, up, at);
                mesh.add_triangle(up - 1, at, at - 1);
            }
        }
    }
    ui.painter().add(Shape::mesh(mesh));

    // Markers, white over black so they show on any color.
    let angle = hsv[0].to_radians();
    let ring_mark = center + vec2(angle.cos(), angle.sin()) * (inner + outer) / 2.0;
    let radius = (outer - inner) / 2.0;
    ui.painter()
        .circle_stroke(ring_mark, radius, Stroke::new(1.5, Color32::WHITE));
    ui.painter()
        .circle_stroke(ring_mark, radius + 1.0, Stroke::new(1.0, Color32::BLACK));
    let mark = pos2(
        square.left() + hsv[1] * square.width(),
        square.top() + (1.0 - hsv[2]) * square.height(),
    );
    ui.painter()
        .circle_stroke(mark, 4.5, Stroke::new(1.5, Color32::WHITE));
    ui.painter()
        .circle_stroke(mark, 5.5, Stroke::new(1.0, Color32::BLACK));

    // Picking: a press on the ring turns the hue, one inside sets
    // saturation and value, and the drag keeps doing what the press began.
    let mut pick = ColorPick {
        dragging: response.dragged(),
        released: response.drag_stopped(),
        ..ColorPick::default()
    };
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        let press = ui
            .input(|input| input.pointer.press_origin())
            .unwrap_or(pointer);
        let mut next = hsv;
        if (press - center).length() > inner * 0.95 {
            let offset = pointer - center;
            next[0] = offset.y.atan2(offset.x).to_degrees().rem_euclid(360.0);
        } else {
            next[1] = ((pointer.x - square.left()) / square.width()).clamp(0.0, 1.0);
            next[2] = 1.0 - ((pointer.y - square.top()) / square.height()).clamp(0.0, 1.0);
        }
        ui.data_mut(|data| data.insert_temp(hue_id, next[0]));
        let [r, g, b] = rgb_of(next);
        if [r, g, b] != [rgba[0], rgba[1], rgba[2]] {
            *rgba = [r, g, b, rgba[3]];
            pick.changed = true;
        }
    }
    pick
}

/// An opacity bar under the wheel, `width` points wide.
pub fn alpha_bar(ui: &mut Ui, rgba: &mut [u8; 4], width: f32) -> ColorPick {
    let (bar, response) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
    checker(ui, bar);
    let mut mesh = Mesh::default();
    let [r, g, b] = [rgba[0], rgba[1], rgba[2]];
    mesh.colored_vertex(bar.left_top(), Color32::from_rgba_unmultiplied(r, g, b, 0));
    mesh.colored_vertex(bar.right_top(), opaque([r, g, b]));
    mesh.colored_vertex(bar.right_bottom(), opaque([r, g, b]));
    mesh.colored_vertex(
        bar.left_bottom(),
        Color32::from_rgba_unmultiplied(r, g, b, 0),
    );
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
    let x = bar.left() + f32::from(rgba[3]) / 255.0 * bar.width();
    ui.painter().rect_stroke(
        Rect::from_center_size(pos2(x, bar.center().y), vec2(5.0, bar.height() + 4.0)),
        1.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Middle,
    );
    let mut pick = ColorPick {
        dragging: response.dragged(),
        released: response.drag_stopped(),
        ..ColorPick::default()
    };
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        let alpha =
            (((pointer.x - bar.left()) / bar.width()).clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha != rgba[3] {
            rgba[3] = alpha;
            pick.changed = true;
        }
    }
    pick
}

/// The swatch of a color, with a checkerboard under its translucent part.
pub fn swatch(ui: &mut Ui, rect: Rect, rgba: [u8; 4], theme: &Theme) {
    if rgba[3] < 255 {
        checker(ui, rect);
    }
    ui.painter().rect_filled(
        rect,
        theme.control_radius.min(6.0),
        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
    );
    ui.painter().rect_stroke(
        rect,
        theme.control_radius.min(6.0),
        theme.hairline(),
        StrokeKind::Inside,
    );
}

/// A swatch button that opens the color wheel (and the opacity bar when
/// `alpha`) in a popup next to it, with the color's numbers underneath.
pub fn color_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    rgba: &mut [u8; 4],
    alpha: bool,
    theme: &Theme,
) -> ColorPick {
    let (rect, response) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::click());
    swatch(ui, rect, *rgba, theme);
    if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            theme.control_radius.min(6.0),
            Stroke::new(theme.border_width.max(1.0), theme.border_active),
            StrokeKind::Inside,
        );
    }
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Pick a color");
    let id = ui.scope_id().with(id_salt);
    let open_id = id.with("open");
    let size_id = id.with("size");
    let mut open = ui
        .data(|data| data.get_temp::<bool>(open_id))
        .unwrap_or(false);
    if response.clicked() {
        open = !open;
    }
    let mut pick = ColorPick::default();
    if open {
        // Below the swatch, right-aligned to it, flipped above when the
        // window has no room; drawn over every window.
        let screen = ui.ctx().content_rect();
        let size = ui
            .data(|data| data.get_temp::<Vec2>(size_id))
            .unwrap_or(vec2(200.0, 290.0));
        let mut at = pos2(rect.right() - size.x, rect.bottom() + 6.0);
        if at.y + size.y > screen.bottom() - 4.0 {
            at.y = (rect.top() - 6.0 - size.y).max(screen.top() + 4.0);
        }
        at.x = at.x.clamp(
            screen.left() + 4.0,
            (screen.right() - size.x - 4.0).max(4.0),
        );
        let shown = egui::Area::new(id.with("popup"))
            .order(Order::Tooltip)
            .fixed_pos(at)
            .fade_in(false)
            .show(ui.ctx(), |ui| {
                crate::widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| color_popup(ui, id, rgba, alpha, theme))
                    .inner
            });
        pick = shown.inner;
        let inside = shown.response.rect;
        ui.data_mut(|data| data.insert_temp(size_id, inside.size()));
        // A press anywhere else, or Esc, closes it.
        let outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|point| !inside.contains(point) && !rect.contains(point))
        });
        let escape =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if outside || escape {
            open = false;
        }
    }
    ui.data_mut(|data| data.insert_temp(open_id, open));
    pick
}

/// What the color button's popup holds: the wheel, the opacity bar, the
/// numbers and a row of grays.
fn color_popup(ui: &mut Ui, id: Id, rgba: &mut [u8; 4], alpha: bool, theme: &Theme) -> ColorPick {
    let mut pick = ColorPick::default();
    {
        {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            pick = pick.merge(color_wheel(ui, id, rgba, 180.0));
            if alpha {
                pick = pick.merge(alpha_bar(ui, rgba, 180.0));
            }
            let [h, s, v] = hsv_of([rgba[0], rgba[1], rgba[2]]);
            let numbers = if alpha {
                format!(
                    "H {h:.0}  S {:.0}  V {:.0}  A {:.0}%",
                    s * 100.0,
                    v * 100.0,
                    f32::from(rgba[3]) / 2.55
                )
            } else {
                format!("H {h:.0}  S {:.0}  V {:.0}", s * 100.0, v * 100.0)
            };
            ui.label(
                egui::RichText::new(numbers)
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            // A row of grays for Sue's monochrome look.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for value in [0_u8, 32, 64, 96, 128, 160, 192, 224, 255] {
                    let (cell, cell_response) =
                        ui.allocate_exact_size(Vec2::splat(17.0), Sense::click());
                    swatch(ui, cell, [value, value, value, 255], theme);
                    if cell_response.clicked() {
                        let next = [value, value, value, rgba[3]];
                        if next != *rgba {
                            *rgba = next;
                            pick.changed = true;
                        }
                    }
                }
            });
        }
    }
    pick
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    #[test]
    fn hsv_round_trips_through_rgb() {
        for rgb in [
            [255, 0, 0],
            [12, 200, 99],
            [128, 128, 128],
            [0, 0, 0],
            [250, 250, 10],
        ] {
            assert_eq!(rgb_of(hsv_of(rgb)), rgb, "{rgb:?}");
        }
        assert_eq!(hsv_of([255, 0, 0]), [0.0, 1.0, 1.0]);
        assert_eq!(rgb_of([120.0, 1.0, 1.0]), [0, 255, 0]);
    }

    #[test]
    fn the_wheel_picks_by_eye() {
        let ctx = egui::Context::default();
        let mut rgba = [255, 255, 255, 255];
        let mut picks = Vec::new();
        for frame in 0..6 {
            let press = pos2(100.0, 100.0);
            let events = match frame {
                3 => vec![
                    egui::Event::PointerMoved(press),
                    egui::Event::PointerButton {
                        pos: press,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                4 => vec![egui::Event::PointerButton {
                    pos: press,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => Vec::new(),
            };
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
                events,
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                egui::Area::new(Id::unique("wheel"))
                    .fixed_pos(Pos2::ZERO)
                    .show(ui.ctx(), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        picks.push(color_wheel(ui, Id::unique("test"), &mut rgba, 200.0));
                    });
            })
            .drop_without_applying_deltas();
        }
        // The middle of the square: half saturation, half value, hue 0.
        assert!(picks.iter().any(|pick| pick.changed));
        let [h, s, v] = hsv_of([rgba[0], rgba[1], rgba[2]]);
        assert!(h.abs() < 1.0 && (s - 0.5).abs() < 0.05 && (v - 0.5).abs() < 0.05);
        assert_eq!(rgba[3], 255, "opacity untouched");
    }
}
