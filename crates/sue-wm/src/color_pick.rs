//! The color wheel every color control shares: a hue ring around a
//! saturation and value square or a lightness and saturation triangle, an
//! opacity bar, and a swatch button that opens it, so a color is picked by
//! eye and never typed as a hex code.

use egui::{
    Color32, Id, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::Theme;
use crate::ui::{Density, Elevation, Layer, Radius, Space, TextRole, focus_ring, motion};

/// What a color control did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColorPick {
    /// The color changed.
    pub changed: bool,
    /// The pointer is still dragging, so the change is a preview.
    pub dragging: bool,
    /// A drag ended this frame: commit what it previewed.
    pub released: bool,
    /// The wheel's shape button was pressed: every wheel shows the other
    /// shape from now on (see [`wheel_shape`]).
    pub reshaped: bool,
}

impl ColorPick {
    fn merge(self, other: Self) -> Self {
        Self {
            changed: self.changed || other.changed,
            dragging: self.dragging || other.dragging,
            released: self.released || other.released,
            reshaped: self.reshaped || other.reshaped,
        }
    }
}

/// What lies inside the color wheel's hue ring. A button at the wheel's
/// bottom right switches between the two, and every wheel in Sue follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WheelShape {
    /// Saturation across and value up a square: the HSV color space.
    #[default]
    Square,
    /// A triangle between the hue, white and black: lightness from black
    /// to white along one side, saturation toward the hue (HLS).
    Triangle,
}

impl WheelShape {
    /// The shape the button switches to.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Square => Self::Triangle,
            Self::Triangle => Self::Square,
        }
    }
}

/// Where the shape every wheel draws is kept.
fn shape_id() -> Id {
    Id::NULL.with("sue-wheel-shape")
}

/// The shape every color wheel draws.
#[must_use]
pub fn wheel_shape(ctx: &egui::Context) -> WheelShape {
    ctx.data(|data| data.get_temp(shape_id()))
        .unwrap_or_default()
}

/// Sets the shape every color wheel draws, for a studio that keeps the
/// choice between sessions.
pub fn set_wheel_shape(ctx: &egui::Context, shape: WheelShape) {
    ctx.data_mut(|data| data.insert_temp(shape_id(), shape));
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

/// Hue (0 to 360), lightness and saturation (0 to 1) of sRGB bytes, the
/// numbers of the triangle wheel. Gray keeps hue 0 and saturation 0.
#[must_use]
pub fn hls_of(rgb: [u8; 3]) -> [f32; 3] {
    let [h, s, v] = hsv_of(rgb);
    let l = v * (1.0 - s / 2.0);
    let room = l.min(1.0 - l);
    let saturation = if room <= 0.0 {
        0.0
    } else {
        ((v - l) / room).clamp(0.0, 1.0)
    };
    [h, l, saturation]
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
fn checker(ui: &Ui, rect: Rect, radius: f32) {
    checker_fill(ui.painter(), rect, radius, 6.0);
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

/// The color of saturation `s` and value `v` at `hue`, as a vertex color.
fn shade(hue: f32, s: f32, v: f32) -> Color32 {
    opaque(rgb_of([hue, s, v]))
}

/// The hue ring. egui gives a mesh no antialiasing, so its two circular
/// edges are drawn as four bands of vertices, the outermost and innermost
/// fading to nothing: a one-point feather, laid inside the radius so
/// nothing spills out of the wheel.
fn ring_mesh(center: Pos2, inner: f32, outer: f32) -> Mesh {
    let mut ring = Mesh::default();
    let bands = [inner - 1.0, inner, outer - 1.0, outer];
    let steps = (outer as usize).clamp(96, 360);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU;
        let direction = vec2(angle.cos(), angle.sin());
        let color = shade(t * 360.0, 1.0, 1.0);
        for (at, radius) in bands.iter().enumerate() {
            let faded = at == 0 || at + 1 == bands.len();
            ring.colored_vertex(
                center + direction * *radius,
                if faded { Color32::TRANSPARENT } else { color },
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
    ring
}

/// The saturation and value square at `hue`.
fn square_mesh(square: Rect, hue: f32) -> Mesh {
    let mut mesh = Mesh::default();
    // Two points to a cell: fewer and the grid, and the crease along each
    // cell's diagonal, show up as a blocky pattern in the gradient.
    let cells = ((square.width() / 2.0).round() as usize).clamp(24, 96);
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
                shade(hue, s, v),
            );
            if row > 0 && column > 0 {
                let at = (row * (cells + 1) + column) as u32;
                let up = at - (cells + 1) as u32;
                mesh.add_triangle(up - 1, up, at);
                mesh.add_triangle(up - 1, at, at - 1);
            }
        }
    }
    mesh
}

/// The triangle of the HLS wheel: its corners at the pure hue, at white
/// and at black. The hue corner points at the hue on the ring.
#[derive(Clone, Copy, Debug)]
struct Triangle {
    hue: Pos2,
    white: Pos2,
    black: Pos2,
}

impl Triangle {
    /// The triangle inside a circle of `radius` around `center`, its hue
    /// corner toward `hue` degrees.
    fn new(center: Pos2, radius: f32, hue: f32) -> Self {
        let corner = |turn: f32| {
            let angle = (hue + turn).to_radians();
            center + vec2(angle.cos(), angle.sin()) * radius
        };
        Self {
            hue: corner(0.0),
            white: corner(120.0),
            black: corner(240.0),
        }
    }

    /// The point of shares `hue` of the pure hue and `white` of white, the
    /// rest black.
    fn mix(&self, hue: f32, white: f32) -> Pos2 {
        let black = 1.0 - hue - white;
        pos2(
            hue * self.hue.x + white * self.white.x + black * self.black.x,
            hue * self.hue.y + white * self.white.y + black * self.black.y,
        )
    }

    /// Where saturation `s` and value `v` lie: the color is the share
    /// `s * v` of the pure hue plus `v * (1 - s)` of white.
    fn at(&self, s: f32, v: f32) -> Pos2 {
        self.mix(s * v, v * (1.0 - s))
    }

    /// The shares of the pure hue, white and black at `point`; one is
    /// below 0 outside the triangle.
    fn weights(&self, point: Pos2) -> [f32; 3] {
        let (along, across, to) = (
            self.white - self.hue,
            self.black - self.hue,
            point - self.hue,
        );
        let (d00, d01, d11) = (along.dot(along), along.dot(across), across.dot(across));
        let (d20, d21) = (to.dot(along), to.dot(across));
        let denominator = d00 * d11 - d01 * d01;
        if denominator.abs() <= f32::EPSILON {
            return [1.0, 0.0, 0.0];
        }
        let white = (d11 * d20 - d01 * d21) / denominator;
        let black = (d00 * d21 - d01 * d20) / denominator;
        [1.0 - white - black, white, black]
    }

    /// The point of the triangle nearest `point`.
    fn nearest(&self, point: Pos2) -> Pos2 {
        if self.weights(point).iter().all(|share| *share >= 0.0) {
            return point;
        }
        let on_side = |a: Pos2, b: Pos2| {
            let side = b - a;
            let t = if side.length_sq() <= f32::EPSILON {
                0.0
            } else {
                ((point - a).dot(side) / side.length_sq()).clamp(0.0, 1.0)
            };
            a + side * t
        };
        [
            on_side(self.hue, self.white),
            on_side(self.white, self.black),
            on_side(self.black, self.hue),
        ]
        .into_iter()
        .min_by(|a, b| a.distance(point).total_cmp(&b.distance(point)))
        .unwrap_or(point)
    }

    /// Saturation and value of the point of the triangle nearest `point`.
    fn pick(&self, point: Pos2) -> [f32; 2] {
        let [hue, white, _] = self.weights(self.nearest(point));
        let v = (hue + white).clamp(0.0, 1.0);
        let s = if v <= 0.0 {
            0.0
        } else {
            (hue / v).clamp(0.0, 1.0)
        };
        [s, v]
    }

    /// The triangle filled with its colors at `hue`, cut into small enough
    /// pieces that its gradient shows no crease, its three sides feathered
    /// over `feather` points outward, since egui antialiases no mesh.
    fn mesh(&self, hue: f32, feather: f32) -> Mesh {
        let color = |share_hue: f32, share_white: f32| {
            let v = (share_hue + share_white).clamp(0.0, 1.0);
            let s = if v <= 0.0 { 0.0 } else { share_hue / v };
            shade(hue, s.clamp(0.0, 1.0), v)
        };
        let mut mesh = Mesh::default();
        let steps = ((self.hue.distance(self.white) / 2.0).round() as usize).clamp(24, 96);
        // Row `i` holds the points of hue share i/steps, from no white to
        // all that is left.
        let first = |i: usize| (i * (steps + 1) - i * i.saturating_sub(1) / 2) as u32;
        for i in 0..=steps {
            for j in 0..=steps - i {
                let (a, w) = (i as f32 / steps as f32, j as f32 / steps as f32);
                mesh.colored_vertex(self.mix(a, w), color(a, w));
            }
        }
        for i in 0..steps {
            let (row, next) = (first(i), first(i + 1));
            for j in 0..(steps - i) as u32 {
                mesh.add_triangle(row + j, next + j, row + j + 1);
                if (j as usize) + 1 < steps - i {
                    mesh.add_triangle(next + j, next + j + 1, row + j + 1);
                }
            }
        }
        // The feather along each side: the side's colors fading outward.
        let middle = self.mix(1.0 / 3.0, 1.0 / 3.0);
        let sides = [
            ((1.0, 0.0), (0.0, 1.0)),
            ((0.0, 1.0), (0.0, 0.0)),
            ((0.0, 0.0), (1.0, 0.0)),
        ];
        for ((a0, w0), (a1, w1)) in sides {
            let (from, to) = (self.mix(a0, w0), self.mix(a1, w1));
            let mut out = (to - from).rot90().normalized();
            if out.dot(from - middle) < 0.0 {
                out = -out;
            }
            let base = mesh.vertices.len() as u32;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let (a, w) = (a0 + (a1 - a0) * t, w0 + (w1 - w0) * t);
                let at = from + (to - from) * t;
                mesh.colored_vertex(at, color(a, w));
                mesh.colored_vertex(at + out * feather, Color32::TRANSPARENT);
                if step > 0 {
                    let low = base + (step as u32 - 1) * 2;
                    mesh.add_triangle(low, low + 1, low + 2);
                    mesh.add_triangle(low + 1, low + 3, low + 2);
                }
            }
        }
        mesh
    }
}

/// The side of the shape button of a wheel `side` points across, or
/// `None` when the wheel's corner is too small to hold one.
fn shape_button_side(ui: &Ui, side: f32) -> Option<f32> {
    // The corner of the wheel's square outside the ring holds a square of
    // a little over a quarter of the radius.
    let room = side / 2.0 * 0.26;
    let button = room.min(Density::of(ui).icon_button());
    (button >= 12.0).then_some(button)
}

/// The button at the wheel's bottom right that switches its shape. Its
/// picture is the shape it switches to.
fn shape_button(ui: &mut Ui, area: Rect, id: Id, shape: WheelShape) -> Option<Response> {
    let side = shape_button_side(ui, area.width())?;
    let rect = Rect::from_min_max(area.max - Vec2::splat(side), area.max);
    let response = ui.interact(rect, id.with("shape-button"), Sense::click());
    let label = match shape {
        WheelShape::Square => tr!("color-wheel-show-triangle"),
        WheelShape::Triangle => tr!("color-wheel-show-square"),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    // The fill, text and focus colors egui's own widgets have, which
    // [`Theme::visuals`] sets from the color roles, since a wheel is drawn
    // without a theme at hand.
    let visuals = ui.visuals().widgets.clone();
    let lit = response.hovered() || response.is_pointer_button_down_on();
    let t = motion::hover_t(ui.ctx(), response.id, lit);
    let corner = visuals.inactive.corner_radius;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        corner,
        Color32::TRANSPARENT.lerp_to_gamma(visuals.hovered.weak_bg_fill, t),
    );
    let ink = visuals
        .inactive
        .fg_stroke
        .color
        .lerp_to_gamma(visuals.hovered.fg_stroke.color, t);
    // The icon stroke rule of the tokens: 1.5 at 16 to 20 points.
    let icon = side * 0.55;
    let stroke = Stroke::new(1.5 * (icon / icon.clamp(16.0, 20.0)), ink);
    let c = rect.center();
    match shape.other() {
        WheelShape::Square => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(icon * 0.8)),
                1.0,
                stroke,
                StrokeKind::Middle,
            );
        }
        WheelShape::Triangle => {
            let r = icon * 0.55;
            let corners: Vec<Pos2> = [270.0_f32, 30.0, 150.0]
                .iter()
                .map(|degrees| {
                    let angle = degrees.to_radians();
                    c + vec2(angle.cos(), angle.sin()) * r + vec2(0.0, r * 0.12)
                })
                .collect();
            painter.add(Shape::closed_line(corners, stroke));
        }
    }
    if crate::ui::focus_visible(ui.ctx()) && response.has_focus() {
        painter.rect_stroke(rect, corner, visuals.active.bg_stroke, StrokeKind::Outside);
    }
    Some(response.on_hover_text(label))
}

/// A hue ring around a saturation and value square or a lightness and
/// saturation triangle ([`wheel_shape`]), `side` points across, editing
/// `rgba` in place. A button at the bottom right switches the shape of
/// every wheel. The hue of a gray is remembered under `id`, so dragging
/// into gray and back keeps it.
pub fn color_wheel(ui: &mut Ui, id: Id, rgba: &mut [u8; 4], side: f32) -> ColorPick {
    let (area, response) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let center = area.center();
    let outer = side / 2.0;
    let inner = outer * 0.8;
    let hue_id = id.with("hue");
    let shape = wheel_shape(ui.ctx());
    let mut hsv = hsv_of([rgba[0], rgba[1], rgba[2]]);
    if hsv[1] <= 0.0 || hsv[2] <= 0.0 {
        hsv[0] = ui.data(|data| data.get_temp(hue_id)).unwrap_or(hsv[0]);
    }

    ui.painter()
        .add(Shape::mesh(ring_mesh(center, inner, outer)));
    let square = Rect::from_center_size(center, Vec2::splat(inner * 0.68 * 2.0));
    let triangle = Triangle::new(center, inner * 0.94, hsv[0]);
    let mark = match shape {
        WheelShape::Square => {
            ui.painter().add(Shape::mesh(square_mesh(square, hsv[0])));
            pos2(
                square.left() + hsv[1] * square.width(),
                square.top() + (1.0 - hsv[2]) * square.height(),
            )
        }
        WheelShape::Triangle => {
            ui.painter().add(Shape::mesh(triangle.mesh(hsv[0], 1.0)));
            triangle.at(hsv[1], hsv[2])
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
        .circle_stroke(mark, 4.5, Stroke::new(1.5, Color32::WHITE));
    ui.painter()
        .circle_stroke(mark, 5.5, Stroke::new(1.0, Color32::BLACK));

    // The shape button lies over the wheel's corner, so a press on it is
    // not a pick. egui hands a drag that starts on a click-only button to
    // the wheel under it, so a hand that slips while pressing the button
    // must not turn the hue either.
    // egui forgets where the press began on the frame it ends, so it is
    // remembered for the frame the drag stops.
    let button = shape_button(ui, area, id, shape);
    let held_id = id.with("press-on-shape-button");
    let on_button = match ui.input(|input| input.pointer.press_origin()) {
        Some(origin) => {
            let on = button
                .as_ref()
                .is_some_and(|button| button.rect.contains(origin));
            ui.data_mut(|data| data.insert_temp(held_id, on));
            on
        }
        None => ui
            .data_mut(|data| data.remove_temp::<bool>(held_id))
            .unwrap_or(false),
    };
    let mut pick = ColorPick {
        dragging: response.dragged() && !on_button,
        released: response.drag_stopped() && !on_button,
        ..ColorPick::default()
    };
    if let Some(button) = button
        && button.clicked()
    {
        set_wheel_shape(ui.ctx(), shape.other());
        pick.reshaped = true;
        ui.ctx().request_repaint();
    }

    // Picking: a press on the ring turns the hue, one inside sets
    // saturation and value, and the drag keeps doing what the press began.
    if let Some(pointer) = response.interact_pointer_pos()
        && !on_button
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
            match shape {
                WheelShape::Square => {
                    next[1] = ((pointer.x - square.left()) / square.width()).clamp(0.0, 1.0);
                    next[2] = 1.0 - ((pointer.y - square.top()) / square.height()).clamp(0.0, 1.0);
                }
                WheelShape::Triangle => {
                    [next[1], next[2]] = triangle.pick(pointer);
                }
            }
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
    checker(ui, bar, 0.0);
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
    // The handle, white over black like the wheel's marks, so it shows on
    // the light checkerboard of a light theme too.
    let x = bar.left() + f32::from(rgba[3]) / 255.0 * bar.width();
    let handle = Rect::from_center_size(pos2(x, bar.center().y), vec2(5.0, bar.height() + 4.0));
    ui.painter().rect_stroke(
        handle,
        1.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Middle,
    );
    ui.painter().rect_stroke(
        handle.expand(1.0),
        2.0,
        Stroke::new(1.0, Color32::BLACK),
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

/// A swatch with corners of `radius`, a checkerboard under its
/// translucent part and `edge` around it.
fn swatch_with(ui: &Ui, rect: Rect, rgba: [u8; 4], radius: f32, edge: Stroke) {
    if rgba[3] < 255 {
        checker(ui, rect, radius);
    }
    ui.painter().rect_filled(
        rect,
        radius,
        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
    );
    ui.painter()
        .rect_stroke(rect, radius, edge, StrokeKind::Inside);
}

/// The swatch of a color, with a checkerboard under its translucent part.
pub fn swatch(ui: &mut Ui, rect: Rect, rgba: [u8; 4], theme: &Theme) {
    swatch_with(ui, rect, rgba, theme.radius(Radius::Xs), theme.hairline());
}

/// A swatch button that opens the color wheel (and the opacity bar when
/// `alpha`) in a popup next to it, with the color's numbers underneath.
/// It is as tall as the other controls of its row, so it never makes a
/// toolbar or a settings row taller (a touch layout makes the whole row
/// taller, not one swatch), and twice as wide.
pub fn color_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    rgba: &mut [u8; 4],
    alpha: bool,
    theme: &Theme,
) -> ColorPick {
    let height = ui.spacing().interact_size.y.round();
    let (rect, response) = ui.allocate_exact_size(vec2(height * 2.0, height), Sense::click());
    let corner = theme.radius(Radius::Md);
    let edge = if response.hovered() {
        theme.hairline_active()
    } else {
        Stroke::new(theme.border_width, theme.border_strong())
    };
    swatch_with(ui, rect, *rgba, corner, edge);
    focus_ring(ui, &response, corner, theme);
    let label = tr!("pick-a-color");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);
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
    // The popup fades in when it opens and out when it closes.
    let shown = motion::fade(ui.ctx(), id.with("fade"), open);
    if shown > 0.0 {
        // Below the swatch, right-aligned to it, flipped above when the
        // window has no room; over the window it was opened from.
        let gap = theme.space(Space::S3);
        let margin = theme.space(Space::S2);
        let screen = ui.ctx().content_rect();
        let size = ui
            .data(|data| data.get_temp::<Vec2>(size_id))
            .unwrap_or(vec2(200.0, 290.0));
        let mut at = pos2(rect.right() - size.x, rect.bottom() + gap);
        if at.y + size.y > screen.bottom() - margin {
            at.y = (rect.top() - gap - size.y).max(screen.top() + margin);
        }
        at.x = at.x.clamp(
            screen.left() + margin,
            (screen.right() - size.x - margin).max(margin),
        );
        let popup = egui::Area::new(id.with("popup"))
            .order(Layer::Popover.order())
            .fixed_pos(at)
            .fade_in(false)
            .interactable(open)
            .show(ui.ctx(), |ui| {
                ui.set_opacity(shown);
                crate::widgets::window_frame(theme)
                    .shadow(theme.elevation(Elevation::Floating))
                    .inner_margin(egui::Margin::same(
                        theme.card_padding().clamp(0.0, 127.0) as i8
                    ))
                    .show(ui, |ui| color_popup(ui, id, rgba, alpha, theme))
                    .inner
            });
        if open {
            pick = popup.inner;
            let inside = popup.response.rect;
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
    }
    ui.data_mut(|data| data.insert_temp(open_id, open));
    pick
}

/// How wide the wheel in the color button's popup is.
const POPUP_WHEEL: f32 = 180.0;

/// What the color button's popup holds: the wheel, the opacity bar, the
/// numbers and a row of grays.
fn color_popup(ui: &mut Ui, id: Id, rgba: &mut [u8; 4], alpha: bool, theme: &Theme) -> ColorPick {
    let mut pick = ColorPick::default();
    ui.spacing_mut().item_spacing = Vec2::splat(theme.space(Space::S3));
    pick = pick.merge(color_wheel(ui, id, rgba, POPUP_WHEEL));
    if alpha {
        pick = pick.merge(alpha_bar(ui, rgba, POPUP_WHEEL));
    }
    // The numbers of the color space the wheel shows.
    let percent = |share: f32| share * 100.0;
    let numbers = match wheel_shape(ui.ctx()) {
        WheelShape::Square => {
            let [h, s, v] = hsv_of([rgba[0], rgba[1], rgba[2]]);
            format!("H {h:.0}  S {:.0}  V {:.0}", percent(s), percent(v))
        }
        WheelShape::Triangle => {
            let [h, l, s] = hls_of([rgba[0], rgba[1], rgba[2]]);
            format!("H {h:.0}  L {:.0}  S {:.0}", percent(l), percent(s))
        }
    };
    let numbers = if alpha {
        format!("{numbers}  A {:.0}%", f32::from(rgba[3]) / 2.55)
    } else {
        numbers
    };
    ui.label(
        egui::RichText::new(numbers)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
    // A row of grays for Sue's monochrome look, as wide as the wheel.
    const GRAYS: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
    let gap = theme.space(Space::S1);
    let cell = (POPUP_WHEEL - gap * (GRAYS.len() - 1) as f32) / GRAYS.len() as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for value in GRAYS {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
            swatch(ui, rect, [value, value, value, 255], theme);
            if response.hovered() {
                ui.painter().rect_stroke(
                    rect,
                    theme.radius(Radius::Xs),
                    theme.hairline_active(),
                    StrokeKind::Inside,
                );
            }
            if response.clicked() {
                let next = [value, value, value, rgba[3]];
                if next != *rgba {
                    *rgba = next;
                    pick.changed = true;
                }
            }
        }
    });
    pick
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2};

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
    fn hls_numbers_follow_lightness_and_saturation() {
        assert_eq!(hls_of([255, 0, 0]), [0.0, 0.5, 1.0]);
        assert_eq!(hls_of([255, 255, 255]), [0.0, 1.0, 0.0]);
        assert_eq!(hls_of([0, 0, 0]), [0.0, 0.0, 0.0]);
        let [h, l, s] = hls_of([128, 128, 128]);
        assert!(h == 0.0 && (l - 0.502).abs() < 0.01 && s == 0.0);
        // A light, half saturated blue: lightness 0.75, saturation 0.5.
        let [h, l, s] = hls_of([159, 159, 223]);
        assert!((h - 240.0).abs() < 0.5, "{h}");
        assert!((l - 0.749).abs() < 0.01, "{l}");
        assert!((s - 0.5).abs() < 0.01, "{s}");
    }

    fn press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Draws a wheel 200 points across at the top left for each frame of
    /// events, and answers what each frame picked.
    fn wheel_frames(
        ctx: &egui::Context,
        rgba: &mut [u8; 4],
        frames: Vec<Vec<Event>>,
    ) -> Vec<ColorPick> {
        let mut picks = Vec::new();
        for events in frames {
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
                        picks.push(color_wheel(ui, Id::unique("test"), rgba, 200.0));
                    });
            })
            .drop_without_applying_deltas();
        }
        picks
    }

    /// A click at `at` after two quiet frames, so egui knows the wheel.
    fn click_at(at: Pos2) -> Vec<Vec<Event>> {
        vec![
            Vec::new(),
            Vec::new(),
            vec![Event::PointerMoved(at)],
            vec![press(at, true)],
            vec![press(at, false)],
            Vec::new(),
        ]
    }

    /// The color button's popup sits on the popover layer, the order of
    /// dialogs: opened from a dialog, it still lies over it and takes the
    /// picks, and a press beside it closes it.
    #[test]
    fn the_popup_over_a_dialog_takes_the_pick() {
        let ctx = egui::Context::default();
        let theme = Theme::from_rice(&sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap());
        let dialog = Id::unique("test-dialog");
        let mut rgba = [255, 255, 255, 255];
        let frame = |events: Vec<Event>, rgba: &mut [u8; 4]| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 600.0))),
                events,
                ..egui::RawInput::default()
            };
            let mut pick = ColorPick::default();
            ctx.run_ui(raw, |ui| {
                egui::Area::new(dialog)
                    .order(Layer::Dialog.order())
                    .fixed_pos(pos2(100.0, 20.0))
                    .show(ui.ctx(), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        ui.set_min_size(vec2(400.0, 400.0));
                        pick = color_button(ui, "test", rgba, false, &theme);
                    });
            })
            .drop_without_applying_deltas();
            pick
        };
        let click = |at: Pos2| {
            vec![
                vec![Event::PointerMoved(at)],
                vec![press(at, true)],
                vec![press(at, false)],
            ]
        };
        for _ in 0..2 {
            frame(Vec::new(), &mut rgba);
        }
        // The swatch is the dialog's first widget, at its top left.
        for events in click(pos2(110.0, 30.0)) {
            frame(events, &mut rgba);
        }
        for _ in 0..30 {
            frame(Vec::new(), &mut rgba);
        }
        let popup = ctx.memory(|memory| {
            memory
                .layer_ids()
                .filter(|layer| layer.order == Layer::Popover.order() && layer.id != dialog)
                .find_map(|layer| memory.area_rect(layer.id))
        });
        let popup = popup.expect("the popup opened");
        let top = ctx.memory(|memory| memory.layer_ids().last());
        assert_ne!(
            top.map(|layer| layer.id),
            Some(dialog),
            "the dialog covers the popup"
        );
        // The middle of the wheel, the first thing in the popup.
        let margin = theme.card_padding().clamp(0.0, 127.0).trunc();
        let middle = popup.min + Vec2::splat(margin + POPUP_WHEEL / 2.0);
        let mut changed = false;
        for events in click(middle) {
            changed |= frame(events, &mut rgba).changed;
        }
        assert!(changed, "the press on the popup's wheel picked nothing");
        assert_ne!(rgba, [255, 255, 255, 255]);
        // A press beside it closes it.
        for events in click(pos2(560.0, 560.0)) {
            frame(events, &mut rgba);
        }
        for _ in 0..30 {
            frame(Vec::new(), &mut rgba);
        }
        let before = rgba;
        for events in click(middle) {
            frame(events, &mut rgba);
        }
        assert_eq!(rgba, before, "the closed popup still picked");
    }

    #[test]
    fn the_wheel_picks_by_eye() {
        let ctx = egui::Context::default();
        let mut rgba = [255, 255, 255, 255];
        let picks = wheel_frames(&ctx, &mut rgba, click_at(pos2(100.0, 100.0)));
        // The middle of the square: half saturation, half value, hue 0.
        assert!(picks.iter().any(|pick| pick.changed));
        let [h, s, v] = hsv_of([rgba[0], rgba[1], rgba[2]]);
        assert!(h.abs() < 1.0 && (s - 0.5).abs() < 0.05 && (v - 0.5).abs() < 0.05);
        assert_eq!(rgba[3], 255, "opacity untouched");
    }

    /// Crash tips, tip 17: the button at the wheel's bottom right switches
    /// every wheel between the HSV square and the HLS triangle, and a
    /// press on it picks no color.
    #[test]
    fn the_shape_button_switches_every_wheel_between_square_and_triangle() {
        let ctx = egui::Context::default();
        let mut rgba = [200, 40, 40, 255];
        assert_eq!(wheel_shape(&ctx), WheelShape::Square);
        let corner = pos2(196.0, 196.0);
        let picks = wheel_frames(&ctx, &mut rgba, click_at(corner));
        assert_eq!(wheel_shape(&ctx), WheelShape::Triangle);
        assert!(picks.iter().any(|pick| pick.reshaped));
        assert!(!picks.iter().any(|pick| pick.changed), "the press picked");
        assert_eq!(rgba, [200, 40, 40, 255]);
        let picks = wheel_frames(&ctx, &mut rgba, click_at(corner));
        assert_eq!(wheel_shape(&ctx), WheelShape::Square);
        assert!(picks.iter().any(|pick| pick.reshaped));
    }

    /// A press on the shape button that slips into a drag picks nothing:
    /// egui hands that drag to the wheel under the button, which would
    /// turn the hue to the corner's angle.
    #[test]
    fn a_drag_from_the_shape_button_picks_no_color() {
        let ctx = egui::Context::default();
        let mut rgba = [200, 40, 40, 255];
        let corner = pos2(196.0, 196.0);
        let mut frames = vec![
            Vec::new(),
            Vec::new(),
            vec![Event::PointerMoved(corner)],
            vec![press(corner, true)],
        ];
        let mut at = corner;
        for _ in 0..3 {
            at -= vec2(8.0, 4.0);
            frames.push(vec![Event::PointerMoved(at)]);
        }
        frames.push(vec![press(at, false)]);
        frames.push(Vec::new());
        let picks = wheel_frames(&ctx, &mut rgba, frames);
        assert_eq!(rgba, [200, 40, 40, 255], "the drag turned the hue");
        assert!(
            !picks
                .iter()
                .any(|pick| pick.changed || pick.dragging || pick.released),
            "{picks:?}"
        );
        assert_eq!(wheel_shape(&ctx), WheelShape::Square, "a drag is no click");
    }

    /// In the triangle, the middle of the side from white to black is a
    /// middle gray, the hue corner the pure hue, and a press outside the
    /// triangle (but inside the ring) takes the nearest color of its edge.
    #[test]
    fn the_triangle_picks_lightness_and_saturation() {
        let ctx = egui::Context::default();
        set_wheel_shape(&ctx, WheelShape::Triangle);
        let center = pos2(100.0, 100.0);
        let triangle = Triangle::new(center, 100.0 * 0.8 * 0.94, 0.0);

        let mut rgba = [255, 0, 0, 255];
        let gray = triangle.white.lerp(triangle.black, 0.5);
        wheel_frames(&ctx, &mut rgba, click_at(gray));
        let [_, l, s] = hls_of([rgba[0], rgba[1], rgba[2]]);
        assert!((l - 0.5).abs() < 0.02 && s < 0.02, "{rgba:?}");

        // Back to the hue corner, a little inside it.
        let corner = triangle.hue.lerp(center, 0.02);
        wheel_frames(&ctx, &mut rgba, click_at(corner));
        let [h, l, s] = hls_of([rgba[0], rgba[1], rgba[2]]);
        assert!(
            h.abs() < 1.0 && (l - 0.5).abs() < 0.04 && s > 0.95,
            "{rgba:?}"
        );

        // Beyond the white and black side, still inside the ring: the
        // edge's gray.
        let beyond = gray + (gray - center).normalized() * 6.0;
        wheel_frames(&ctx, &mut rgba, click_at(beyond));
        let [_, l, s] = hls_of([rgba[0], rgba[1], rgba[2]]);
        assert!((l - 0.5).abs() < 0.04 && s < 0.02, "{rgba:?}");
    }

    #[test]
    fn the_triangle_places_and_finds_every_color_alike() {
        let triangle = Triangle::new(pos2(50.0, 50.0), 40.0, 210.0);
        for s in [0.0, 0.25, 0.5, 1.0] {
            for v in [0.1, 0.5, 0.9, 1.0] {
                let [s2, v2] = triangle.pick(triangle.at(s, v));
                assert!((v2 - v).abs() < 1e-3, "v {v} came back {v2}");
                assert!((s2 - s).abs() < 1e-3, "s {s} at v {v} came back {s2}");
            }
        }
    }

    /// The wheel stays smooth at every size and in both shapes: the
    /// ring's two circular edges and the triangle's sides are feathered,
    /// because egui antialiases no mesh, and the inner mesh follows the
    /// wheel so no grid shows in its gradient.
    #[test]
    fn the_wheel_stays_smooth_at_every_size() {
        for shape in [WheelShape::Square, WheelShape::Triangle] {
            let ctx = egui::Context::default();
            set_wheel_shape(&ctx, shape);
            let mut largest = Vec::new();
            for side in [140.0_f32, 300.0] {
                let mut rgba = [255, 0, 0, 255];
                let mut faded = Vec::new();
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
                    color_wheel(ui, Id::unique("test"), &mut rgba, side);
                });
                // The meshes can sit in a layer's shape list inside another.
                fn look(shape: &Shape, faded: &mut Vec<usize>, biggest: &mut usize) {
                    match shape {
                        Shape::Mesh(mesh) => {
                            faded.push(
                                mesh.vertices
                                    .iter()
                                    .filter(|vertex| vertex.color.a() == 0)
                                    .count(),
                            );
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
                    faded.first().is_some_and(|count| *count > 0),
                    "the ring has no feathered edge at {side} points across"
                );
                if shape == WheelShape::Triangle {
                    assert!(
                        faded.get(1).is_some_and(|count| *count > 0),
                        "the triangle has no feathered side at {side} points across"
                    );
                }
                largest.push(biggest);
            }
            assert!(
                largest[1] > largest[0],
                "the meshes do not follow the wheel's size: {largest:?}"
            );
        }
    }
}
