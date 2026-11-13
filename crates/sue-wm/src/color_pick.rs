//! The color wheel every color control shares: a hue ring around a
//! saturation and value square, an opacity bar, and a swatch button that
//! opens it, so a color is picked by eye and never typed as a hex code.

use egui::{
    Color32, Id, Mesh, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
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

/// How the wheel's inner region reads: a square of saturation and value
/// (Sue's default), or a triangle whose hue corner turns with the hue, as
/// Clip Studio Paint's color wheel does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorShape {
    /// The saturation and value square, the hue in the wheel's ring.
    #[default]
    Square,
    /// A triangle of hue, white and black, the hue in its corner.
    Triangle,
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
    checker_fill(ui.painter(), rect, 0.0, 6.0);
}

/// What clear looks like: a checkerboard of `cell`-point squares filling
/// `rect`, its corners rounded by `radius` (a pill when it is half the
/// height). Every transparent swatch and opacity track in Sue draws it, so
/// clear looks the same everywhere.
pub fn checker_fill(painter: &egui::Painter, rect: Rect, radius: f32, cell: f32) {
    let ctx = painter.ctx();
    let id = Id::NULL.with("sue-checker-texture");
    let texture = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let (light, dark) = (Color32::WHITE, Color32::from_gray(204));
        let image = egui::ColorImage::new([2, 2], vec![light, dark, dark, light]);
        let handle = ctx.load_texture("sue-checker", image, egui::TextureOptions::NEAREST_REPEAT);
        ctx.data_mut(|data| data.insert_temp(id, handle.clone()));
        handle
    });
    let uv = Rect::from_min_size(pos2(0.0, 0.0), rect.size() / (cell.max(0.5) * 2.0));
    painter.add(
        egui::epaint::RectShape::filled(rect, radius, Color32::WHITE)
            .with_texture(texture.id(), uv),
    );
}

/// A hue ring around `shape`'s inner region, `side` points across, editing
/// `rgba` in place. The hue is remembered under `id` where the color cannot
/// carry it, so dragging into gray and back keeps it and picking inside
/// either shape never turns it.
pub fn color_wheel(
    ui: &mut Ui,
    id: Id,
    rgba: &mut [u8; 4],
    side: f32,
    shape: ColorShape,
) -> ColorPick {
    let (area, response) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let center = area.center();
    let outer = side / 2.0;
    let inner = outer * 0.8;
    let hue_id = id.with("hue");
    let color_id = id.with("hue.color");
    let mut hsv = hsv_of([rgba[0], rgba[1], rgba[2]]);
    // The wheel remembers the hue where the color cannot carry it: this is
    // the very color the wheel made (byte for byte), so keep its hue
    // exactly, or the color is nearly gray and its bytes round the hue
    // away, by tens of degrees near gray. Picking inside a shape then never
    // turns it. Above this much chroma an outside color's round trip stays
    // within a few degrees.
    let remembered = ui.data(|data| data.get_temp::<f32>(hue_id));
    let produced = ui.data(|data| data.get_temp::<[u8; 4]>(color_id)) == Some(*rgba);
    if (produced || hsv[1] * hsv[2] < 0.08)
        && let Some(hue) = remembered
    {
        hsv[0] = hue;
    }

    // The hue ring. egui gives a mesh no antialiasing, so its two circular
    // edges are drawn as four bands of vertices, the outermost and
    // innermost fading to nothing: a one-point feather, laid inside the
    // radius so nothing spills out of the wheel.
    let mut ring = Mesh::default();
    let bands = [inner - 1.0, inner, outer - 1.0, outer];
    let steps = (outer as usize).clamp(96, 360);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU;
        let direction = vec2(angle.cos(), angle.sin());
        let shade = opaque(rgb_of([t * 360.0, 1.0, 1.0]));
        for (at, radius) in bands.iter().enumerate() {
            let faded = at == 0 || at + 1 == bands.len();
            ring.colored_vertex(
                center + direction * *radius,
                if faded {
                    shade.gamma_multiply(0.0)
                } else {
                    shade
                },
            );
        }
        if step > 0 {
            let back = bands.len() as u32;
            let base = step as u32 * back;
            for at in 0..back - 1 {
                let low = base - back + at;
                ring.add_triangle(low, low + 1, low + back);
                ring.add_triangle(low + 1, low + back + 1, low + back);
            }
        }
    }
    ui.painter().add(Shape::mesh(ring));

    // The inner region: the saturation and value square, or the hue
    // triangle. The triangle's hue corner is the hue, its white and black
    // corners the square's other two, and the triangle turns with the hue.
    let marker = match shape {
        ColorShape::Square => {
            let square = square_rect(center, inner);
            let mut mesh = Mesh::default();
            // Two points to a cell: fewer and the grid, and the crease along
            // each cell's diagonal, show up as a blocky pattern in the
            // gradient.
            let cells = ((inner * 0.68).round() as usize).clamp(24, 96);
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
            pos2(
                square.left() + hsv[1] * square.width(),
                square.top() + (1.0 - hsv[2]) * square.height(),
            )
        }
        ColorShape::Triangle => {
            let [hue_corner, white_corner, black_corner] = triangle_corners(center, inner, hsv[0]);
            let mut mesh = Mesh::default();
            mesh.colored_vertex(hue_corner, opaque(rgb_of([hsv[0], 1.0, 1.0])));
            mesh.colored_vertex(white_corner, Color32::WHITE);
            mesh.colored_vertex(black_corner, Color32::BLACK);
            mesh.add_triangle(0, 1, 2);
            ui.painter().add(Shape::mesh(mesh));
            let [hue_weight, white_weight, black_weight] =
                [hsv[1] * hsv[2], (1.0 - hsv[1]) * hsv[2], 1.0 - hsv[2]];
            pos2(
                hue_corner.x * hue_weight
                    + white_corner.x * white_weight
                    + black_corner.x * black_weight,
                hue_corner.y * hue_weight
                    + white_corner.y * white_weight
                    + black_corner.y * black_weight,
            )
        }
    };

    // Markers, white over black so they show on any color.
    let angle = hsv[0].to_radians();
    let ring_mark = center + vec2(angle.cos(), angle.sin()) * (inner + outer) / 2.0;
    let radius = (outer - inner) / 2.0;
    ui.painter()
        .circle_stroke(ring_mark, radius, Stroke::new(1.5, Color32::WHITE));
    ui.painter()
        .circle_stroke(ring_mark, radius + 1.0, Stroke::new(1.0, Color32::BLACK));
    ui.painter()
        .circle_stroke(marker, 4.5, Stroke::new(1.5, Color32::WHITE));
    ui.painter()
        .circle_stroke(marker, 5.5, Stroke::new(1.0, Color32::BLACK));

    // Picking: a press on the ring (past its inner edge) turns the hue, one
    // inside sets saturation and value, and the drag keeps doing what the
    // press began. The shapes reach up to that edge, so a press on a corner
    // or tip stays theirs.
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
        if (press - center).length() > inner {
            let offset = pointer - center;
            next[0] = offset.y.atan2(offset.x).to_degrees().rem_euclid(360.0);
        } else {
            match shape {
                ColorShape::Square => {
                    let square = square_rect(center, inner);
                    next[1] = ((pointer.x - square.left()) / square.width()).clamp(0.0, 1.0);
                    next[2] = 1.0 - ((pointer.y - square.top()) / square.height()).clamp(0.0, 1.0);
                }
                ColorShape::Triangle => {
                    let [hue_corner, white_corner, black_corner] =
                        triangle_corners(center, inner, next[0]);
                    let [hue_weight, white_weight, _] =
                        barycentric(pointer, hue_corner, white_corner, black_corner);
                    let value = hue_weight + white_weight;
                    next[1] = if value > 0.0 { hue_weight / value } else { 0.0 };
                    next[2] = value;
                }
            }
        }
        let [r, g, b] = rgb_of(next);
        if [r, g, b] != [rgba[0], rgba[1], rgba[2]] {
            *rgba = [r, g, b, rgba[3]];
            pick.changed = true;
        }
        ui.data_mut(|data| {
            data.insert_temp(hue_id, next[0]);
            data.insert_temp(color_id, *rgba);
        });
    }
    pick
}

/// The saturation and value square inside the hue ring's `inner` radius.
fn square_rect(center: Pos2, inner: f32) -> Rect {
    let half = inner * 0.68;
    Rect::from_center_size(center, vec2(half * 2.0, half * 2.0))
}

/// The hue, white and black corners of the hue triangle inside the ring's
/// `inner` radius: the hue corner at `hue`, the white and black a third of
/// the circle either way, so the triangle turns with the hue.
fn triangle_corners(center: Pos2, inner: f32, hue: f32) -> [Pos2; 3] {
    let radius = inner * 0.94;
    let hue = hue.to_radians();
    let corner = |angle: f32| center + vec2(angle.cos(), angle.sin()) * radius;
    [
        corner(hue),
        corner(hue + std::f32::consts::TAU / 3.0),
        corner(hue - std::f32::consts::TAU / 3.0),
    ]
}

/// The barycentric weights of `point` in the triangle `a`, `b`, `c`, clamped
/// to the triangle and renormalized. A degenerate triangle gives black.
fn barycentric(point: Pos2, a: Pos2, b: Pos2, c: Pos2) -> [f32; 3] {
    let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if area.abs() < 1e-6 {
        return [0.0, 0.0, 0.0];
    }
    let w0 = ((b.x - point.x) * (c.y - point.y) - (b.y - point.y) * (c.x - point.x)) / area;
    let w1 = ((c.x - point.x) * (a.y - point.y) - (c.y - point.y) * (a.x - point.x)) / area;
    let w2 = 1.0 - w0 - w1;
    let weights = [w0.max(0.0), w1.max(0.0), w2.max(0.0)];
    let sum = weights[0] + weights[1] + weights[2];
    if sum <= 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        weights.map(|weight| weight / sum)
    }
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
        .on_hover_text(tr!("pick-a-color"));
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
            // The shape switch: a square or a triangle, as Clip Studio
            // Paint's color wheel offers. Remembered per color control.
            let shape_id = id.with("shape");
            let mut shape = ui
                .data(|data| data.get_temp::<ColorShape>(shape_id))
                .unwrap_or_default();
            let options = [tr!("square").to_owned(), tr!("triangle").to_owned()];
            if let Some(index) =
                crate::widgets::segmented(ui, &options, Some(shape as usize), theme)
            {
                shape = if index == 0 {
                    ColorShape::Square
                } else {
                    ColorShape::Triangle
                };
                ui.data_mut(|data| data.insert_temp(shape_id, shape));
            }
            pick = pick.merge(color_wheel(ui, id, rgba, 180.0, shape));
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
                        picks.push(color_wheel(
                            ui,
                            Id::unique("test"),
                            &mut rgba,
                            200.0,
                            ColorShape::Square,
                        ));
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

    /// The wheel stays smooth at every size: the ring's two circular edges
    /// are feathered, because egui antialiases no mesh, and the square's
    /// mesh follows the wheel so no grid shows in its gradient.
    #[test]
    fn the_wheel_stays_smooth_at_every_size() {
        let ctx = egui::Context::default();
        let mut largest = Vec::new();
        for side in [140.0_f32, 300.0] {
            let mut rgba = [255, 0, 0, 255];
            let mut faded = 0;
            let mut biggest = 0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    vec2(side + 20.0, side + 20.0),
                )),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                color_wheel(ui, Id::unique("test"), &mut rgba, side, ColorShape::Square);
            });
            // The meshes can sit in a layer's shape list inside another.
            fn look(shape: &Shape, faded: &mut usize, biggest: &mut usize) {
                match shape {
                    Shape::Mesh(mesh) => {
                        *faded += mesh
                            .vertices
                            .iter()
                            .filter(|vertex| vertex.color.a() == 0)
                            .count();
                        *biggest = (*biggest).max(mesh.vertices.len());
                    }
                    Shape::Vec(shapes) => {
                        for shape in shapes {
                            look(shape, faded, biggest);
                        }
                    }
                    _ => {}
                }
            }
            for clipped in &output.shapes {
                look(&clipped.shape, &mut faded, &mut biggest);
            }
            output.drop_without_applying_deltas();
            assert!(
                faded > 0,
                "the ring has no feathered edge at {side} points across"
            );
            largest.push(biggest);
        }
        assert!(
            largest[1] > largest[0],
            "the meshes do not follow the wheel's size: {largest:?}"
        );
    }

    #[test]
    fn the_triangle_weights_land_on_its_corners_and_center() {
        let a = pos2(100.0, 20.0);
        let b = pos2(30.0, 80.0);
        let c = pos2(170.0, 120.0);
        assert_eq!(barycentric(a, a, b, c), [1.0, 0.0, 0.0]);
        assert_eq!(barycentric(b, a, b, c), [0.0, 1.0, 0.0]);
        assert_eq!(barycentric(c, a, b, c), [0.0, 0.0, 1.0]);
        // The middle of the triangle: a third of each corner.
        let middle = pos2((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
        let [hue, white, black] = barycentric(middle, a, b, c);
        for weight in [hue, white, black] {
            assert!((weight - 1.0 / 3.0).abs() < 0.001);
        }
        // A point past an edge clamps to that edge's two corners.
        let past = pos2(a.x, a.y - 10.0);
        let [hue, white, black] = barycentric(past, a, b, c);
        assert!(black.abs() < 0.001);
        assert!((hue + white - 1.0).abs() < 0.001);
    }

    #[test]
    fn the_triangle_wheel_picks_by_eye() {
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
                egui::Area::new(Id::unique("triangle-wheel"))
                    .fixed_pos(Pos2::ZERO)
                    .show(ui.ctx(), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        picks.push(color_wheel(
                            ui,
                            Id::unique("test-triangle"),
                            &mut rgba,
                            200.0,
                            ColorShape::Triangle,
                        ));
                    });
            })
            .drop_without_applying_deltas();
        }
        // The middle of the wheel is the triangle's middle: a third of the
        // hue, white and black corners, so half saturation and two thirds
        // of the value, at hue 0.
        assert!(picks.iter().any(|pick| pick.changed));
        let [h, s, v] = hsv_of([rgba[0], rgba[1], rgba[2]]);
        assert!(
            h.abs() < 1.0 && (s - 0.5).abs() < 0.05 && (v - 2.0 / 3.0).abs() < 0.05,
            "{h} {s} {v}"
        );
        assert_eq!(rgba[3], 255, "opacity untouched");
    }

    /// The angle of the triangle mesh's hue corner, seen from the wheel's
    /// centre. The first three vertices of a triangle mesh are the hue,
    /// white and black corners.
    fn triangle_hue_angle(output: &egui::FullOutput, center: Pos2) -> Option<f32> {
        fn look(shape: &Shape, center: Pos2, found: &mut Option<f32>) {
            match shape {
                Shape::Mesh(mesh) if mesh.vertices.len() == 3 => {
                    let at = mesh.vertices[0].pos;
                    *found = Some((at.y - center.y).atan2(at.x - center.x).to_degrees());
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        look(shape, center, found);
                    }
                }
                _ => {}
            }
        }
        let mut found = None;
        for clipped in &output.shapes {
            look(&clipped.shape, center, &mut found);
        }
        found
    }

    #[test]
    fn picking_inside_the_triangle_leaves_its_hue_alone() {
        let ctx = egui::Context::default();
        // A saturated orange: its hue corner points at 30 degrees.
        let [r, g, b] = rgb_of([30.0, 1.0, 1.0]);
        let mut rgba = [r, g, b, 255];
        let center = pos2(100.0, 100.0);
        let mut angles = Vec::new();
        for frame in 0..7 {
            // The press lands just inside the black corner, where the color
            // has too little chroma for its bytes to carry the hue.
            let [_, _, black_corner] = triangle_corners(center, 80.0, 30.0);
            let press = center + (black_corner - center) * 0.98;
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
            let output = ctx.run_ui(raw, |ui| {
                egui::Area::new(Id::unique("still-triangle"))
                    .fixed_pos(Pos2::ZERO)
                    .show(ui.ctx(), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        color_wheel(
                            ui,
                            Id::unique("test-still"),
                            &mut rgba,
                            200.0,
                            ColorShape::Triangle,
                        );
                    });
            });
            angles.push(triangle_hue_angle(&output, center));
            output.drop_without_applying_deltas();
        }
        let before = angles[2].expect("the triangle is drawn");
        let after = angles[6].expect("the triangle is drawn");
        assert!(rgba != [r, g, b, 255], "the press picked a color");
        let turned = ((after - before + 180.0).rem_euclid(360.0) - 180.0).abs();
        assert!(turned < 2.0, "the triangle turned: {before} to {after}");
    }

    /// One frame of a plain 200 point wheel at the origin, whatever the
    /// picking tests need to drive.
    fn wheel_frame(
        ctx: &egui::Context,
        id: Id,
        rgba: &mut [u8; 4],
        shape: ColorShape,
        events: Vec<egui::Event>,
    ) {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            events,
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| {
            egui::Area::new(Id::unique("probe-wheel"))
                .fixed_pos(Pos2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    color_wheel(ui, id, rgba, 200.0, shape);
                });
        })
        .drop_without_applying_deltas();
    }

    fn press_at(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Click `id`'s wheel at `press` for a press and a release, and hand
    /// back the hue it now remembers.
    fn click_and_read_hue(
        ctx: &egui::Context,
        id: Id,
        rgba: &mut [u8; 4],
        shape: ColorShape,
        press: Pos2,
    ) -> Option<f32> {
        for frame in 0..6 {
            let events = match frame {
                3 => vec![egui::Event::PointerMoved(press), press_at(press, true)],
                4 => vec![press_at(press, false)],
                _ => Vec::new(),
            };
            wheel_frame(ctx, id, rgba, shape, events);
        }
        ctx.data(|data| data.get_temp::<f32>(id.with("hue")))
    }

    #[test]
    fn picking_the_squares_corner_keeps_the_hue() {
        let ctx = egui::Context::default();
        let [r, g, b] = rgb_of([200.0, 0.7, 0.9]);
        let mut rgba = [r, g, b, 255];
        // Top right, at full saturation and value: that corner reaches a
        // little past the ring's inner edge, and must still not turn the
        // hue.
        let inner = 80.0;
        let corner = pos2(100.0 + 0.68 * inner, 100.0 - 0.68 * inner);
        let hue = click_and_read_hue(
            &ctx,
            Id::unique("test-square-corner"),
            &mut rgba,
            ColorShape::Square,
            corner,
        );
        assert!(
            hue.is_some_and(|hue| (hue - 200.0).abs() < 1.0),
            "the corner turned the hue: {hue:?}"
        );
        assert_ne!(rgba, [r, g, b, 255], "the corner picked full saturation");
    }

    #[test]
    fn picking_just_past_the_triangles_tip_keeps_the_hue() {
        let ctx = egui::Context::default();
        let [r, g, b] = rgb_of([200.0, 0.8, 0.9]);
        let mut rgba = [r, g, b, 255];
        // Just past the hue corner, a little off its axis: still within the
        // ring's inner edge, so it must not turn the hue either.
        let angle = (215.0f32).to_radians();
        let press = pos2(100.0, 100.0) + vec2(angle.cos(), angle.sin()) * (80.0 * 0.98);
        let hue = click_and_read_hue(
            &ctx,
            Id::unique("test-triangle-tip"),
            &mut rgba,
            ColorShape::Triangle,
            press,
        );
        assert!(
            hue.is_some_and(|hue| (hue - 200.0).abs() < 1.0),
            "past the tip turned the hue: {hue:?}"
        );
        assert_ne!(rgba, [r, g, b, 255], "the press picked");
    }

    #[test]
    fn dragging_inside_the_square_keeps_the_hue_bit_exact() {
        let ctx = egui::Context::default();
        let [r, g, b] = rgb_of([200.0, 0.7, 0.9]);
        let mut rgba = [r, g, b, 255];
        let id = Id::unique("test-drag-hue");
        let square = |s: f32, v: f32| {
            pos2(
                100.0 + (s - 0.5) * 2.0 * 0.68 * 80.0,
                100.0 - (v - 0.5) * 2.0 * 0.68 * 80.0,
            )
        };
        let mut hues: Vec<f32> = Vec::new();
        for frame in 0..14 {
            let events = match frame {
                3 => vec![
                    egui::Event::PointerMoved(square(0.9, 0.9)),
                    press_at(square(0.9, 0.9), true),
                ],
                4..=10 => vec![egui::Event::PointerMoved(square(
                    0.9 - 0.1 * (frame - 3) as f32,
                    0.9 - 0.057 * (frame - 3) as f32,
                ))],
                11 => vec![press_at(square(0.2, 0.5), false)],
                _ => Vec::new(),
            };
            wheel_frame(&ctx, id, &mut rgba, ColorShape::Square, events);
            if let Some(hue) = ctx.data(|data| data.get_temp::<f32>(id.with("hue"))) {
                hues.push(hue);
            }
        }
        assert!(hues.len() >= 8, "{hues:?}");
        let first = hues[0];
        for hue in &hues {
            assert!((hue - first).abs() < 0.05, "the hue drifted: {hues:?}");
        }
        assert_ne!(rgba, [r, g, b, 255], "the drag picked");
    }
}
