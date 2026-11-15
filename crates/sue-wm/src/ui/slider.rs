//! Sliders: the controls a person drags a value along.
//!
//! - [`slider`]: a horizontal track with a thin upright handle
//!   (`---[]---`), an optional label before it and a number field after
//!   it that types the same value.
//! - [`tall_slider`]: a vertical track whose top is the most, lit from the
//!   bottom up to a round knob, as the quick sliders beside a canvas
//!   stand. Its opacity form fades from solid at the top to clear at the
//!   bottom over a checkerboard.
//! - [`range`]: one track with two handles, for a lowest and a highest
//!   value.
//! - [`wedge`]: a triangle that grows to the right, lit up to the value,
//!   as the compact size and opacity sliders of a tool's settings are
//!   drawn.
//!
//! Every one follows the pointer from where it went down, takes the
//! arrow keys while it holds focus (Page Up and Page Down ten steps, Home
//! and End the ends), slows to a tenth while Shift is held during a drag,
//! can map its values by their logarithm (sizes, where the small end needs
//! room), and reads, draws and names itself as the inputs of
//! [`super::input`] do.
//!
//! [`sheet`] draws all of them, in every state, for review.

use std::ops::RangeInclusive;

use egui::AsIdSalt;
use egui::epaint::Mesh;
use egui::{
    Color32, EventFilter, Id, IdSalt, Key, Modifiers, Painter, Pos2, Rect, Response, Role, Sense,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use super::input::{
    Look, STATES, Table, control_id, disabled_ink, hit_rect, line, mix, number_field,
    paint_number_sample,
};
use super::{Density, Radius, Space, TextRole, focus, motion};
use crate::Theme;

/// How much a pressed handle grows, and a hovered one.
const GROW_PRESSED: f32 = 0.25;
const GROW_HOVER: f32 = 0.15;

/// A step of the keys, as a share of the track, when no step is given.
const KEY_SHARE: f64 = 0.01;

/// How much slower a drag goes while Shift is held.
const SLOW: f64 = 0.1;

// ----- Values on a track --------------------------------------------------------

/// How values map onto a track: evenly, or by their logarithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    low: f64,
    high: f64,
    log: bool,
}

impl Scale {
    /// The scale of `range`, by the logarithm when `log` (and the range
    /// reaches above 0).
    #[must_use]
    pub fn new(range: &RangeInclusive<f64>, log: bool) -> Self {
        let (low, high) = (*range.start(), *range.end());
        let (low, high) = if low <= high {
            (low, high)
        } else {
            (high, low)
        };
        Self {
            low,
            high,
            log: log && high > 0.0,
        }
    }

    /// The lowest value a logarithmic track reaches before its very end,
    /// which is the low end itself.
    fn floor(self) -> f64 {
        if self.low > 0.0 {
            self.low
        } else {
            (self.high * 1e-6).max(f64::MIN_POSITIVE)
        }
    }

    /// Where `value` lies along the track, from 0 at the low end to 1.
    #[must_use]
    pub fn to_unit(self, value: f64) -> f64 {
        if self.high.partial_cmp(&self.low) != Some(std::cmp::Ordering::Greater)
            || !value.is_finite()
        {
            return 0.0;
        }
        let t = if self.log {
            let floor = self.floor();
            (value.max(floor).ln() - floor.ln()) / (self.high.ln() - floor.ln())
        } else {
            (value - self.low) / (self.high - self.low)
        };
        t.clamp(0.0, 1.0)
    }

    /// The value at `t` along the track.
    #[must_use]
    pub fn from_unit(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if self.log {
            if t <= 0.0 {
                return self.low;
            }
            let floor = self.floor();
            (floor.ln() + t * (self.high.ln() - floor.ln()))
                .exp()
                .clamp(self.low, self.high)
        } else {
            self.low + t * (self.high - self.low)
        }
    }

    /// `value` held inside the range.
    #[must_use]
    pub fn clamp(self, value: f64) -> f64 {
        value.clamp(self.low, self.high)
    }
}

/// `value` rounded to `decimals` digits, without a negative zero.
fn rounded(value: f64, decimals: usize) -> f64 {
    let factor = 10_f64.powi(decimals.min(9) as i32);
    let value = (value * factor).round() / factor;
    if value == 0.0 { 0.0 } else { value }
}

/// What the keys asked of a slider that holds focus: steps (ten for a
/// page) and an end (`Some(true)` for End). Its arrows are its own while
/// it holds focus.
fn keyed(ui: &Ui, id: Id, focused: bool) -> (i64, Option<bool>) {
    if !focused {
        return (0, None);
    }
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..EventFilter::default()
            },
        );
    });
    ui.input_mut(|input| {
        let mut press = |key| input.count_and_consume_key(Modifiers::NONE, key) as i64;
        let steps = press(Key::ArrowRight) + press(Key::ArrowUp)
            - press(Key::ArrowLeft)
            - press(Key::ArrowDown)
            + 10 * (press(Key::PageUp) - press(Key::PageDown));
        let end = if press(Key::End) > 0 {
            Some(true)
        } else if press(Key::Home) > 0 {
            Some(false)
        } else {
            None
        };
        (steps, end)
    })
}

/// `value` moved by `steps` keys: `step` each where one is given, else a
/// hundredth of the track, rounded to `decimals` digits and kept inside
/// the range. Where rounding would undo the move (a hundredth of a short
/// track, or the small end of a logarithmic one), each key moves one unit
/// of the last digit kept instead, so a key always moves a value that can
/// move.
fn stepped(value: f64, steps: i64, scale: Scale, step: Option<f64>, decimals: usize) -> f64 {
    let moved = match step {
        Some(step) if step > 0.0 => value + steps as f64 * step,
        _ => scale.from_unit(scale.to_unit(value) + steps as f64 * KEY_SHARE),
    };
    let current = rounded(value, decimals);
    let mut next = rounded(moved, decimals);
    if steps != 0 && next == current {
        let unit = 10_f64.powi(-(decimals.min(9) as i32));
        next = rounded(current + steps as f64 * unit, decimals);
    }
    scale.clamp(next)
}

/// The value at `t` along the track of `scale`, rounded to `decimals`
/// digits and kept inside the range (rounding can leave it).
fn value_at(scale: Scale, t: f64, decimals: usize) -> f64 {
    scale.clamp(rounded(scale.from_unit(t), decimals))
}

/// Where along `track` the pointer holds the control behind `response`,
/// from 0 to 1 (bottom to top when `upright`). While Shift is held during
/// a drag it moves a tenth of the drag from `now` instead.
fn held_share(ui: &Ui, response: &Response, track: Rect, upright: bool, now: f64) -> Option<f64> {
    if !response.is_pointer_button_down_on() {
        return None;
    }
    let length = if upright {
        track.height()
    } else {
        track.width()
    }
    .max(1.0);
    if response.dragged() && ui.input(|input| input.modifiers.shift_only()) {
        let delta = response.drag_delta();
        let along = if upright { -delta.y } else { delta.x };
        return Some((now + f64::from(along / length) * SLOW).clamp(0.0, 1.0));
    }
    let at = response.interact_pointer_pos()?;
    let t = if upright {
        (track.bottom() - at.y) / length
    } else {
        (at.x - track.left()) / length
    };
    Some(f64::from(t).clamp(0.0, 1.0))
}

// ----- How sliders are painted ----------------------------------------------------

/// The thickness of a horizontal track at `density`.
fn rail(density: Density) -> f32 {
    match density {
        Density::Compact | Density::Regular => 4.0,
        Density::Touch => 6.0,
    }
}

/// The size of the upright handle at `density`.
fn handle(density: Density) -> Vec2 {
    match density {
        Density::Compact => vec2(6.0, 14.0),
        Density::Regular => vec2(6.0, 18.0),
        Density::Touch => vec2(10.0, 28.0),
    }
}

/// The colors of a track: the part beyond the value, the part up to it.
fn track_colors(theme: &Theme, look: Look) -> (Color32, Color32) {
    if !look.enabled {
        let ink = disabled_ink(theme);
        return (mix(theme.border, ink, 0.4), ink);
    }
    let lit = mix(
        mix(theme.text, theme.text_muted(), 0.25),
        theme.text,
        look.hover,
    );
    (theme.border_strong(), lit)
}

/// The fill of a handle or a knob.
fn grip_fill(theme: &Theme, look: Look) -> Color32 {
    if look.enabled {
        theme.text
    } else {
        disabled_ink(theme)
    }
}

/// How much larger a handle draws than at rest.
fn grow(look: Look) -> f32 {
    if !look.enabled {
        1.0
    } else if look.pressed {
        1.0 + GROW_PRESSED
    } else {
        1.0 + GROW_HOVER * look.hover
    }
}

/// Paints a horizontal track over `track`, lit from the left up to `t`
/// (or from `from` to `t`, for a range).
fn paint_rail(painter: &Painter, track: Rect, from: f32, t: f32, theme: &Theme, look: Look) {
    let radius = theme.radius(Radius::Xs).min(track.height() / 2.0);
    let (rest, lit) = track_colors(theme, look);
    painter.rect_filled(track, radius, rest);
    let x = |share: f32| track.left() + share.clamp(0.0, 1.0) * track.width();
    let part = Rect::from_min_max(pos2(x(from), track.top()), pos2(x(t), track.bottom()));
    if part.width() > 0.0 {
        painter.rect_filled(part, radius, lit);
    }
}

/// Paints a color slider's track: `stops` spread evenly along it, as
/// tall as half a handle so the colors read, with a hairline around it.
/// A disabled one is dimmed.
fn paint_gradient_rail(
    painter: &Painter,
    track: Rect,
    stops: &[Color32],
    density: Density,
    theme: &Theme,
    look: Look,
) {
    let band = Rect::from_center_size(
        track.center(),
        vec2(track.width(), (handle(density).y / 2.0).max(track.height())),
    );
    let dim = if look.enabled { 1.0 } else { 0.4 };
    let mut mesh = Mesh::default();
    let last = (stops.len() - 1) as f32;
    for (index, stop) in stops.iter().enumerate() {
        let x = band.left() + band.width() * index as f32 / last;
        let color = stop.gamma_multiply(dim);
        mesh.colored_vertex(pos2(x, band.top()), color);
        mesh.colored_vertex(pos2(x, band.bottom()), color);
        if index > 0 {
            let at = u32::try_from(index * 2).unwrap_or(u32::MAX);
            mesh.add_triangle(at - 2, at - 1, at);
            mesh.add_triangle(at - 1, at, at + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.rect_stroke(band, 0.0, theme.hairline(), StrokeKind::Outside);
}

/// Paints the upright handle centered on `center`, with a ring of the
/// panel color that sets it off a lit track, and its focus ring.
fn paint_handle(painter: &Painter, center: Pos2, density: Density, theme: &Theme, look: Look) {
    let size = handle(density) * grow(look);
    let rect = Rect::from_center_size(center, size);
    let radius = theme.radius(Radius::Xs).min(size.x / 2.0);
    painter.rect(
        rect,
        radius,
        grip_fill(theme, look),
        Stroke::new(1.5, theme.surface(1)),
        StrokeKind::Outside,
    );
    focus::paint_focus_ring(painter, rect.expand(1.5), radius + 1.5, look.focus, theme);
}

/// Paints a round knob on `center`, `radius` across, as the tall slider
/// has.
fn paint_knob(painter: &Painter, center: Pos2, radius: f32, theme: &Theme, look: Look) {
    let radius = radius * grow(look);
    painter.circle(
        center,
        radius,
        grip_fill(theme, look),
        Stroke::new(1.5, theme.surface(1)),
    );
    let ring = Rect::from_center_size(center, Vec2::splat(radius * 2.0 + 3.0));
    focus::paint_focus_ring(painter, ring, ring.width() / 2.0, look.focus, theme);
}

/// Where the track of a slider lies in `room`: centered on its height,
/// inset at the ends by half a handle so the handle stays inside.
fn rail_in(room: Rect, density: Density) -> Rect {
    let inset = handle(density).x * (1.0 + GROW_PRESSED) / 2.0 + 1.5;
    Rect::from_center_size(
        room.center(),
        vec2((room.width() - inset * 2.0).max(1.0), rail(density)),
    )
}

/// Paints the label of a slider, quiet and cut short to `room`.
fn paint_label(painter: &Painter, room: Rect, text: &str, theme: &Theme, look: Look) {
    let galley = line(
        painter,
        text,
        theme.text_font(TextRole::Caption),
        room.width(),
    );
    let ink = if look.enabled {
        theme.text_muted()
    } else {
        disabled_ink(theme)
    };
    painter.galley(
        pos2(room.left(), room.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
}

// ----- The slider ----------------------------------------------------------------

/// A horizontal slider. Made by [`slider`].
#[must_use = "a slider is drawn by `show`"]
pub struct Slider<'a> {
    salt: IdSalt,
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    label: &'a str,
    show_label: bool,
    label_width: Option<f32>,
    field: bool,
    suffix: &'a str,
    decimals: usize,
    step: Option<f64>,
    log: bool,
    enabled: bool,
    width: Option<f32>,
    gradient: &'a [Color32],
}

/// A slider that holds `value` inside `range`. `salt` names it among the
/// controls of its `Ui`.
pub fn slider(salt: impl AsIdSalt, value: &mut f64, range: RangeInclusive<f64>) -> Slider<'_> {
    Slider {
        salt: IdSalt::new(salt),
        value,
        range,
        label: "",
        show_label: false,
        label_width: None,
        field: true,
        suffix: "",
        decimals: 0,
        step: None,
        log: false,
        enabled: true,
        width: None,
        gradient: &[],
    }
}

impl<'a> Slider<'a> {
    /// Writes `label` before the track, and names it so for assistive
    /// technology.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self.show_label = true;
        self
    }

    /// Names it `name` for assistive technology without writing it, where
    /// a label beside it says what it is.
    pub fn name(mut self, name: &'a str) -> Self {
        self.label = name;
        self.show_label = false;
        self
    }

    /// How wide the label is, so the tracks of a column line up.
    pub fn label_width(mut self, width: f32) -> Self {
        self.label_width = Some(width);
        self
    }

    /// Whether a number field after the track types the value (it does
    /// unless told otherwise).
    pub fn field(mut self, field: bool) -> Self {
        self.field = field;
        self
    }

    /// The unit the field shows after the number.
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    /// How many digits after the decimal separator it keeps.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(9);
        self
    }

    /// What one arrow key adds. Without it, a key moves a hundredth of
    /// the track.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.abs());
        self
    }

    /// Maps the values by their logarithm, so the low end gets as much
    /// room as the high end (sizes).
    pub fn logarithmic(mut self) -> Self {
        self.log = true;
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide, label and field included. Without it, it is
    /// as wide as the room it is shown in.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Paints the track as the colors `stops`, spread evenly from its
    /// start to its end, instead of the theme's rail: what each place on
    /// a color slider gives. Two stops at least; fewer leave the rail.
    pub fn gradient(mut self, stops: &'a [Color32]) -> Self {
        self.gradient = stops;
        self
    }

    /// The width of the label at `density`, when it shows one.
    fn label_room(&self, theme: &Theme) -> f32 {
        if self.show_label {
            self.label_width
                .unwrap_or_else(|| theme.space(Space::S9) * 2.0)
        } else {
            0.0
        }
    }

    /// Shows it in the layout of `ui`. `changed()` on the response says
    /// the value changed, by dragging, the keys or the field;
    /// `drag_stopped()` says a drag ended.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = match self.width {
            Some(width) => width,
            None if ui.available_width().is_finite() => ui.available_width(),
            None => density.control_height() * 8.0,
        };
        let rect = ui.allocate_space(vec2(width, density.control_height())).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let scale = Scale::new(&self.range, self.log);
        let gap = theme.space(Space::S3);
        let label_room = self.label_room(theme);
        let field_room = if self.field {
            (density.control_height() * 2.4).round()
        } else {
            0.0
        };
        let mut left = rect.left();
        let label_rect = Rect::from_min_size(rect.min, vec2(label_room, rect.height()));
        if label_room > 0.0 {
            left += label_room + gap;
        }
        let mut right = rect.right();
        let field_rect = Rect::from_min_max(pos2(rect.right() - field_room, rect.top()), rect.max);
        if field_room > 0.0 {
            right -= field_room + gap;
        }
        let room = Rect::from_min_max(pos2(left, rect.top()), pos2(right.max(left), rect.bottom()));
        let track = rail_in(room, density);
        let sense = if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let mut response = ui
            .interact(hit_rect(room, density.min_hit()), id, sense)
            .with_new_rect(room);
        let old = *self.value;
        let now = scale.to_unit(old);
        if enabled && let Some(t) = held_share(ui, &response, track, false, now) {
            *self.value = value_at(scale, t, self.decimals);
        }
        let (steps, end) = keyed(ui, id, enabled && response.has_focus());
        if steps != 0 {
            *self.value = stepped(*self.value, steps, scale, self.step, self.decimals);
        }
        if let Some(end) = end {
            *self.value = if end { scale.high } else { scale.low };
        }
        if self.field {
            let mut typed = *self.value;
            let field = number_field((self.salt, "sue-field"), &mut typed)
                .range(scale.low..=scale.high)
                .decimals(self.decimals)
                .step(self.step.unwrap_or(10_f64.powi(-(self.decimals as i32))))
                .suffix(self.suffix)
                .label(self.label)
                .enabled(enabled)
                .show_at(ui, field_rect, theme);
            if field.changed() {
                *self.value = typed;
            }
        }
        let look = Look::of(ui, &response, enabled, false);
        let look = Look {
            pressed: look.pressed || response.dragged(),
            ..look
        };
        let painter = ui.painter();
        if label_room > 0.0 {
            paint_label(painter, label_rect, self.label, theme, look);
        }
        let t = scale.to_unit(*self.value) as f32;
        if self.gradient.len() >= 2 {
            paint_gradient_rail(painter, track, self.gradient, density, theme, look);
        } else {
            paint_rail(painter, track, 0.0, t, theme, look);
        }
        paint_handle(
            painter,
            pos2(track.left() + t * track.width(), track.center().y),
            density,
            theme,
            look,
        );
        if *self.value != old {
            response.mark_changed();
        }
        let (label, value) = (self.label, *self.value);
        response.widget_info(|| WidgetInfo::slider(enabled, value, label));
        response
    }
}

// ----- The tall slider ----------------------------------------------------------

/// A vertical slider. Made by [`tall_slider`].
#[must_use = "a slider is drawn by `show`"]
pub struct TallSlider<'a> {
    salt: IdSalt,
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    label: &'a str,
    fading: bool,
    log: bool,
    step: Option<f64>,
    decimals: usize,
    enabled: bool,
    height: Option<f32>,
}

/// A tall slider that holds `value` inside `range`, the top being the
/// most. `salt` names it among the controls of its `Ui`.
pub fn tall_slider(
    salt: impl AsIdSalt,
    value: &mut f64,
    range: RangeInclusive<f64>,
) -> TallSlider<'_> {
    TallSlider {
        salt: IdSalt::new(salt),
        value,
        range,
        label: "",
        fading: false,
        log: false,
        step: None,
        decimals: 0,
        enabled: true,
        height: None,
    }
}

impl<'a> TallSlider<'a> {
    /// The name assistive technology reads out for it.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Draws it as an opacity: solid at the top fading to clear at the
    /// bottom, over a checkerboard.
    pub fn fading(mut self) -> Self {
        self.fading = true;
        self
    }

    /// Maps the values by their logarithm (sizes).
    pub fn logarithmic(mut self) -> Self {
        self.log = true;
        self
    }

    /// What one arrow key adds.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.abs());
        self
    }

    /// How many digits after the decimal separator it keeps.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(9);
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `height` tall. Without it, it takes the height there is,
    /// or ten rows.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Shows it in the layout of `ui`, an icon button wide.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let height = match self.height {
            Some(height) => height,
            None if ui.available_height().is_finite() => ui.available_height(),
            None => density.row() * 10.0,
        };
        let rect = ui.allocate_space(vec2(density.icon_button(), height)).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it down the middle of `lane`.
    pub fn show_at(self, ui: &mut Ui, lane: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let scale = Scale::new(&self.range, self.log);
        let geometry = TallGeometry::new(ui.painter(), lane, density, self.fading);
        let sense = if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let mut response = ui
            .interact(hit_rect(lane, density.min_hit()), id, sense)
            .with_new_rect(lane);
        let old = *self.value;
        if enabled
            && let Some(t) = held_share(ui, &response, geometry.track, true, scale.to_unit(old))
        {
            *self.value = value_at(scale, t, self.decimals);
        }
        let (steps, end) = keyed(ui, id, enabled && response.has_focus());
        if steps != 0 {
            *self.value = stepped(*self.value, steps, scale, self.step, self.decimals);
        }
        if let Some(end) = end {
            *self.value = if end { scale.high } else { scale.low };
        }
        let look = Look::of(ui, &response, enabled, false);
        let look = Look {
            pressed: look.pressed || response.dragged(),
            ..look
        };
        paint_tall(
            ui.painter(),
            &geometry,
            scale.to_unit(*self.value) as f32,
            theme,
            look,
        );
        if *self.value != old {
            response.mark_changed();
        }
        let (label, value) = (self.label, *self.value);
        response.widget_info(|| WidgetInfo::slider(enabled, value, label));
        response
    }
}

/// Where the parts of a tall slider lie, on whole screen pixels so its
/// checkerboard stays crisp.
struct TallGeometry {
    track: Rect,
    knob: f32,
    cell: f32,
    fading: bool,
}

impl TallGeometry {
    fn new(painter: &Painter, lane: Rect, density: Density, fading: bool) -> Self {
        let pixel = 1.0 / painter.pixels_per_point();
        let snap = |value: f32| (value / pixel).round() * pixel;
        let cell = snap(3.0).max(pixel);
        let width = if fading {
            cell * 2.0
        } else {
            snap(rail(density))
        };
        let knob = density.icon() * 0.4;
        let inset = knob * (1.0 + GROW_PRESSED) + 2.0;
        let track = Rect::from_min_max(
            pos2(
                snap(lane.center().x - width / 2.0),
                snap(lane.top() + inset),
            ),
            pos2(
                snap(lane.center().x - width / 2.0) + width,
                snap((lane.bottom() - inset).max(lane.top() + inset + 1.0)),
            ),
        );
        Self {
            track,
            knob,
            cell,
            fading,
        }
    }
}

/// Paints a tall slider at `t` of the way up.
fn paint_tall(painter: &Painter, geometry: &TallGeometry, t: f32, theme: &Theme, look: Look) {
    let track = geometry.track;
    let y = track.bottom() - t.clamp(0.0, 1.0) * track.height();
    if geometry.fading {
        paint_fade(painter, track, geometry.cell, look.enabled);
        if !look.enabled {
            // A checkerboard is the loudest thing on a panel: a disabled
            // one sinks under a veil of the panel, as the other tracks
            // turn gray.
            painter.rect_filled(track, 0.0, theme.surface(1).gamma_multiply(0.6));
        }
        painter.rect_stroke(
            track,
            0.0,
            Stroke::new(1.0, track_colors(theme, look).0),
            StrokeKind::Outside,
        );
    } else {
        let radius = theme.radius(Radius::Xs).min(track.width() / 2.0);
        let (rest, lit) = track_colors(theme, look);
        painter.rect_filled(track, radius, rest);
        painter.rect_filled(
            Rect::from_min_max(pos2(track.left(), y), track.right_bottom()),
            radius,
            lit,
        );
    }
    paint_knob(
        painter,
        pos2(track.center().x, y),
        geometry.knob,
        theme,
        look,
    );
}

/// Paints opacity down `track`: solid black at the top fading to clear
/// at the bottom over Sue's checkerboard, evenly to the eye. This is a
/// picture of transparency, as a canvas shows it, so it keeps the
/// checkerboard's own colors in every theme; disabled, it is half as
/// strong.
fn paint_fade(painter: &Painter, track: Rect, cell: f32, enabled: bool) {
    crate::checker_fill(painter, track, 0.0, cell);
    let strength = if enabled { 1.0 } else { 0.5 };
    let mut mesh = Mesh::default();
    let steps = 48;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let opacity = 1.0 - t;
        // The screen blends in linear light: each step's alpha is the one
        // that darkens as much as the opacity does to the eye.
        let alpha =
            1.0 - egui::ecolor::linear_f32_from_gamma_u8(((1.0 - opacity) * 255.0).round() as u8);
        let color = Color32::from_black_alpha((alpha * strength * 255.0).round() as u8);
        let y = track.top() + t * track.height();
        mesh.colored_vertex(pos2(track.left(), y), color);
        mesh.colored_vertex(pos2(track.right(), y), color);
        if step > 0 {
            let at = step * 2;
            mesh.add_triangle(at - 2, at - 1, at + 1);
            mesh.add_triangle(at - 2, at + 1, at);
        }
    }
    painter.add(Shape::mesh(mesh));
}

// ----- The range ----------------------------------------------------------------

/// A track with two handles. Made by [`range`].
#[must_use = "a range is drawn by `show`"]
pub struct RangeSlider<'a> {
    salt: IdSalt,
    low: &'a mut f64,
    high: &'a mut f64,
    bounds: RangeInclusive<f64>,
    label: &'a str,
    gap: f64,
    fields: bool,
    suffix: &'a str,
    decimals: usize,
    step: Option<f64>,
    log: bool,
    enabled: bool,
    width: Option<f32>,
}

/// A track from `bounds` with a handle at `low` and one at `high`, which
/// never pass each other. `salt` names it among the controls of its `Ui`.
pub fn range<'a>(
    salt: impl AsIdSalt,
    low: &'a mut f64,
    high: &'a mut f64,
    bounds: RangeInclusive<f64>,
) -> RangeSlider<'a> {
    RangeSlider {
        salt: IdSalt::new(salt),
        low,
        high,
        bounds,
        label: "",
        gap: 0.0,
        fields: true,
        suffix: "",
        decimals: 0,
        step: None,
        log: false,
        enabled: true,
        width: None,
    }
}

impl<'a> RangeSlider<'a> {
    /// The name assistive technology reads out for it; its handles are
    /// read out as its start and its end.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// The least the two may lie apart.
    pub fn gap(mut self, gap: f64) -> Self {
        self.gap = gap.abs();
        self
    }

    /// Whether number fields at either end type the two values (they do
    /// unless told otherwise).
    pub fn fields(mut self, fields: bool) -> Self {
        self.fields = fields;
        self
    }

    /// The unit the fields show after the numbers.
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    /// How many digits after the decimal separator it keeps.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(9);
        self
    }

    /// What one arrow key adds.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.abs());
        self
    }

    /// Maps the values by their logarithm.
    pub fn logarithmic(mut self) -> Self {
        self.log = true;
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide, fields included.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Shows it in the layout of `ui`. `changed()` on the response says
    /// either value changed.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = match self.width {
            Some(width) => width,
            None if ui.available_width().is_finite() => ui.available_width(),
            None => density.control_height() * 10.0,
        };
        let rect = ui.allocate_space(vec2(width, density.control_height())).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(mut self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let scale = Scale::new(&self.bounds, self.log);
        let gap = theme.space(Space::S3);
        let field_room = if self.fields {
            (density.control_height() * 2.4).round()
        } else {
            0.0
        };
        let low_field = Rect::from_min_size(rect.min, vec2(field_room, rect.height()));
        let high_field = Rect::from_min_max(pos2(rect.right() - field_room, rect.top()), rect.max);
        let inset = if self.fields { field_room + gap } else { 0.0 };
        let room = Rect::from_min_max(
            pos2(rect.left() + inset, rect.top()),
            pos2(
                (rect.right() - inset).max(rect.left() + inset),
                rect.bottom(),
            ),
        );
        let track = rail_in(room, density);
        let (old_low, old_high) = (*self.low, *self.high);
        // The two in order, inside the bounds.
        *self.high = scale.clamp(*self.high);
        *self.low = scale.clamp(*self.low).min(*self.high);
        let sense = if enabled {
            Sense::CLICK | Sense::DRAG
        } else {
            Sense::hover()
        };
        let mut response = ui
            .interact(hit_rect(room, density.min_hit()), id, sense)
            .with_new_rect(room);
        let x_of = |value: f64| track.left() + scale.to_unit(value) as f32 * track.width();
        let handle_rect = |value: f64| {
            hit_rect(
                Rect::from_center_size(pos2(x_of(value), track.center().y), handle(density)),
                density.min_hit(),
            )
        };
        let focusable = if enabled {
            Sense::focusable_noninteractive()
        } else {
            Sense::hover()
        };
        let low_id = id.with("sue-low");
        let high_id = id.with("sue-high");
        let low_handle = ui.interact(handle_rect(*self.low), low_id, focusable);
        let high_handle = ui.interact(handle_rect(*self.high), high_id, focusable);
        // Which handle the pointer holds: the one nearer where it went
        // down. On two handles that lie together, the way the pointer
        // then goes picks one, so they can always be parted.
        let grab_key = id.with("sue-grab");
        let grabbed = ui.data(|data| data.get_temp::<bool>(grab_key));
        let picked = grabbed.or_else(|| {
            let (at, now) =
                ui.input(|input| (input.pointer.press_origin(), input.pointer.latest_pos()));
            let at = at.map_or(track.center().x, |at| at.x);
            let (to_low, to_high) = ((at - x_of(*self.low)).abs(), (at - x_of(*self.high)).abs());
            if (to_low - to_high).abs() >= f32::EPSILON {
                Some(to_high < to_low)
            } else if to_high > density.min_hit() / 2.0 {
                Some(at > x_of(*self.high))
            } else {
                let moved = now.map_or(0.0, |now| now.x - at);
                (moved.abs() >= 1.0).then_some(moved > 0.0)
            }
        });
        if enabled
            && response.is_pointer_button_down_on()
            && let Some(high) = picked
        {
            if grabbed.is_none() {
                ui.data_mut(|data| data.insert_temp(grab_key, high));
                ui.memory_mut(|memory| memory.request_focus(if high { high_id } else { low_id }));
            }
            let now = scale.to_unit(if high { *self.high } else { *self.low });
            if let Some(t) = held_share(ui, &response, track, false, now) {
                let value = value_at(scale, t, self.decimals);
                self.set(high, value, scale);
            }
        } else if !(enabled && response.is_pointer_button_down_on()) {
            ui.data_mut(|data| data.remove_temp::<bool>(grab_key));
        }
        for (high, held) in [(false, &low_handle), (true, &high_handle)] {
            let (steps, end) = keyed(ui, held.id, enabled && held.has_focus());
            let current = if high { *self.high } else { *self.low };
            let mut value = current;
            if steps != 0 {
                value = stepped(value, steps, scale, self.step, self.decimals);
            }
            if let Some(end) = end {
                value = if end { scale.high } else { scale.low };
            }
            if value != current {
                self.set(high, value, scale);
            }
        }
        // Its two ends, as assistive technology reads them out.
        let start = tr!("range-start", name = self.label.to_owned());
        let end = tr!("range-end", name = self.label.to_owned());
        if self.fields {
            for (high, place) in [(false, low_field), (true, high_field)] {
                let mut typed = if high { *self.high } else { *self.low };
                let field = number_field((self.salt, "sue-field", high), &mut typed)
                    .range(scale.low..=scale.high)
                    .decimals(self.decimals)
                    .step(self.step.unwrap_or(10_f64.powi(-(self.decimals as i32))))
                    .suffix(self.suffix)
                    .label(if high { &end } else { &start })
                    .enabled(enabled)
                    .show_at(ui, place, theme);
                if field.changed() {
                    self.set(high, typed, scale);
                }
            }
        }
        let look = Look::of(ui, &response, enabled, false);
        let held: Option<bool> = ui.data(|data| data.get_temp(grab_key));
        let look_of = |handle: &Response, high: bool| Look {
            pressed: held == Some(high) && (look.pressed || response.dragged()),
            focus: motion::hover_t(
                ui.ctx(),
                handle.id.with("sue-focus-ring"),
                enabled && focus::focus_visible(ui.ctx()) && handle.has_focus(),
            ),
            ..look
        };
        let painter = ui.painter();
        let (from, to) = (
            scale.to_unit(*self.low) as f32,
            scale.to_unit(*self.high) as f32,
        );
        paint_rail(painter, track, from, to, theme, look);
        for (high, handle_response, value) in [
            (false, &low_handle, *self.low),
            (true, &high_handle, *self.high),
        ] {
            paint_handle(
                painter,
                pos2(x_of(value), track.center().y),
                density,
                theme,
                look_of(handle_response, high),
            );
        }
        let changed = *self.low != old_low || *self.high != old_high;
        let label = self.label;
        let (low, high) = (*self.low, *self.high);
        low_handle.widget_info(|| WidgetInfo::slider(enabled, low, &start));
        high_handle.widget_info(|| WidgetInfo::slider(enabled, high, &end));
        response.widget_info(|| WidgetInfo::labeled(Role::Group, enabled, label));
        response = response.union(low_handle).union(high_handle);
        if changed {
            response.mark_changed();
        }
        response
    }

    /// Moves the `high` handle (or the low one) to `value`, held at least
    /// the gap from the other and inside the bounds.
    fn set(&mut self, high: bool, value: f64, scale: Scale) {
        let value = scale.clamp(value);
        if high {
            *self.high = value.max(*self.low + self.gap).min(scale.high);
            *self.low = (*self.low).min(*self.high - self.gap).max(scale.low);
        } else {
            *self.low = value.min(*self.high - self.gap).max(scale.low);
            *self.high = (*self.high).max(*self.low + self.gap).min(scale.high);
        }
    }
}

// ----- The wedge ----------------------------------------------------------------

/// A wedge slider. Made by [`wedge`].
#[must_use = "a wedge is drawn by `show`"]
pub struct Wedge<'a> {
    salt: IdSalt,
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    label: &'a str,
    log: bool,
    step: Option<f64>,
    decimals: usize,
    enabled: bool,
    width: Option<f32>,
}

/// A wedge that holds `value` inside `range`. `salt` names it among the
/// controls of its `Ui`.
pub fn wedge(salt: impl AsIdSalt, value: &mut f64, range: RangeInclusive<f64>) -> Wedge<'_> {
    Wedge {
        salt: IdSalt::new(salt),
        value,
        range,
        label: "",
        log: false,
        step: None,
        decimals: 0,
        enabled: true,
        width: None,
    }
}

/// The height of a wedge at `density`.
fn wedge_height(density: Density) -> f32 {
    (density.control_height() * 0.75).round()
}

impl<'a> Wedge<'a> {
    /// The name assistive technology reads out for it.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Maps the values by their logarithm (sizes).
    pub fn logarithmic(mut self) -> Self {
        self.log = true;
        self
    }

    /// What one arrow key adds.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.abs());
        self
    }

    /// How many digits after the decimal separator it keeps.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(9);
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide. Without it, it is as wide as the room.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = match self.width {
            Some(width) => width,
            None if ui.available_width().is_finite() => ui.available_width(),
            None => density.control_height() * 5.0,
        };
        let rect = ui
            .allocate_space(vec2(width.max(24.0), wedge_height(density)))
            .1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let scale = Scale::new(&self.range, self.log);
        let shape = wedge_rect(rect);
        let sense = if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let mut response = ui
            .interact(hit_rect(rect, density.min_hit()), id, sense)
            .with_new_rect(rect);
        let old = *self.value;
        if enabled && let Some(t) = held_share(ui, &response, shape, false, scale.to_unit(old)) {
            *self.value = value_at(scale, t, self.decimals);
        }
        let (steps, end) = keyed(ui, id, enabled && response.has_focus());
        if steps != 0 {
            *self.value = stepped(*self.value, steps, scale, self.step, self.decimals);
        }
        if let Some(end) = end {
            *self.value = if end { scale.high } else { scale.low };
        }
        let look = Look::of(ui, &response, enabled, false);
        let look = Look {
            pressed: look.pressed || response.dragged(),
            ..look
        };
        paint_wedge(
            ui.painter(),
            rect,
            scale.to_unit(*self.value) as f32,
            theme,
            look,
        );
        if *self.value != old {
            response.mark_changed();
        }
        let (label, value) = (self.label, *self.value);
        response.widget_info(|| WidgetInfo::slider(enabled, value, label));
        response
    }
}

/// Where the triangle of a wedge lies in `rect`.
fn wedge_rect(rect: Rect) -> Rect {
    rect.shrink2(vec2(1.0, 2.0))
}

/// Paints a wedge over `rect`, lit up to `t`.
fn paint_wedge(painter: &Painter, rect: Rect, t: f32, theme: &Theme, look: Look) {
    let wedge = wedge_rect(rect);
    let x = wedge.left() + t.clamp(0.0, 1.0) * wedge.width();
    // The top edge at `x`: a point at the left, full height at the right.
    let top_at = |x: f32| {
        let u = (x - wedge.left()) / wedge.width().max(1.0);
        wedge.bottom() - wedge.height() * (0.08 + 0.92 * u)
    };
    let lit = if look.enabled {
        let hot = look.hover.max(if look.pressed { 1.0 } else { 0.0 });
        mix(mix(theme.text, theme.surface(1), 0.22), theme.text, hot)
    } else {
        disabled_ink(theme)
    };
    let dark = theme.surface(0);
    let part = |from: f32, to: f32, color: Color32| {
        if to - from < 0.5 {
            return;
        }
        painter.add(Shape::convex_polygon(
            vec![
                pos2(from, wedge.bottom()),
                pos2(from, top_at(from)),
                pos2(to, top_at(to)),
                pos2(to, wedge.bottom()),
            ],
            color,
            Stroke::NONE,
        ));
    };
    part(wedge.left(), x, lit);
    part(x, wedge.right(), dark);
    let edge = if look.enabled {
        theme.border_strong()
    } else {
        mix(theme.border, disabled_ink(theme), 0.4)
    };
    painter.add(Shape::closed_line(
        vec![
            pos2(wedge.left(), wedge.bottom()),
            pos2(wedge.left(), top_at(wedge.left())),
            pos2(wedge.right(), wedge.top()),
            pos2(wedge.right(), wedge.bottom()),
        ],
        Stroke::new(1.0, edge),
    ));
    let bar = if look.enabled {
        theme.text
    } else {
        disabled_ink(theme)
    };
    painter.line_segment(
        [pos2(x, wedge.top()), pos2(x, wedge.bottom())],
        Stroke::new(if look.pressed { 2.0 } else { 1.5 }, bar),
    );
    focus::paint_focus_ring(painter, rect, theme.radius(Radius::Xs), look.focus, theme);
}

// ----- The review sheet ---------------------------------------------------------

/// What a review sheet of the sliders shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sheet {
    /// Every slider in every state, painted as it would look.
    States,
    /// Every slider in the three densities, live.
    Densities,
}

impl Sheet {
    /// Both pages.
    pub const ALL: [Self; 2] = [Self::States, Self::Densities];

    /// Its name in file names.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::States => "states",
            Self::Densities => "densities",
        }
    }
}

/// Draws the review sheet `page` of the sliders over `rect`, in `theme`,
/// at the scale of the painter.
pub fn sheet(painter: &Painter, rect: Rect, theme: &Theme, page: Sheet) {
    match page {
        Sheet::States => states_sheet(painter, rect, theme),
        Sheet::Densities => densities_sheet(painter, rect, theme),
    }
}

fn states_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let density = Density::Regular;
    let split = rect.top() + rect.height() * 0.62;
    let upper = Rect::from_min_max(rect.min, pos2(rect.right(), split));
    let lower = Rect::from_min_max(pos2(rect.left(), split - theme.space(Space::S6)), rect.max);
    let columns: Vec<&str> = STATES.iter().map(|(name, _)| *name).collect();
    let rows = ["Slider", "Slider, label and field", "Range", "Wedge"];
    let table = Table::new(
        painter,
        upper,
        theme,
        "Sliders",
        &columns,
        rows.len(),
        density.control_height() + theme.space(Space::S7),
    );
    let width = (table.cell(0, 0).width() - theme.space(Space::S6)).min(230.0);
    for (row, name) in rows.iter().enumerate() {
        table.name(painter, theme, row, name);
        for (column, (_, look)) in STATES.iter().enumerate() {
            let place = table.place(row, column, vec2(width, density.control_height()));
            match row {
                0 => {
                    let track = rail_in(place, density);
                    paint_rail(painter, track, 0.0, 0.6, theme, *look);
                    paint_handle(
                        painter,
                        pos2(track.left() + 0.6 * track.width(), track.center().y),
                        density,
                        theme,
                        *look,
                    );
                }
                1 => paint_slider_sample(painter, place, theme, density, *look),
                2 => {
                    let track = rail_in(place, density);
                    paint_rail(painter, track, 0.25, 0.7, theme, *look);
                    for (share, focused) in [(0.25, false), (0.7, true)] {
                        let handle_look = Look {
                            focus: if focused { look.focus } else { 0.0 },
                            pressed: focused && look.pressed,
                            ..*look
                        };
                        paint_handle(
                            painter,
                            pos2(track.left() + share * track.width(), track.center().y),
                            density,
                            theme,
                            handle_look,
                        );
                    }
                }
                _ => {
                    let place = Rect::from_min_size(
                        pos2(place.left(), place.center().y - wedge_height(density) / 2.0),
                        vec2(width, wedge_height(density)),
                    );
                    paint_wedge(painter, place, 0.45, theme, *look);
                }
            }
        }
    }
    // The tall sliders, a pair in each state.
    let tall = Table::new(
        painter,
        lower,
        theme,
        "Tall sliders: size and opacity",
        &columns,
        1,
        f32::INFINITY,
    );
    for (column, (_, look)) in STATES.iter().enumerate() {
        let cell = tall.cell(0, column);
        let height = (cell.height() - theme.space(Space::S6)).max(40.0);
        for (index, fading) in [false, true].into_iter().enumerate() {
            let lane = Rect::from_min_size(
                pos2(
                    cell.left() + index as f32 * density.icon_button() * 1.5,
                    cell.top(),
                ),
                vec2(density.icon_button(), height),
            );
            let geometry = TallGeometry::new(painter, lane, density, fading);
            paint_tall(
                painter,
                &geometry,
                if fading { 0.8 } else { 0.35 },
                theme,
                *look,
            );
        }
    }
}

/// Paints a slider with a label and a field, as [`Slider::show_at`] lays
/// it out, in `look`.
fn paint_slider_sample(
    painter: &Painter,
    place: Rect,
    theme: &Theme,
    density: Density,
    look: Look,
) {
    let gap = theme.space(Space::S3);
    let label = theme.space(Space::S9) * 1.5;
    let field = (density.control_height() * 2.4).round();
    paint_label(
        painter,
        Rect::from_min_size(place.min, vec2(label, place.height())),
        "Size",
        theme,
        look,
    );
    let room = Rect::from_min_max(
        pos2(place.left() + label + gap, place.top()),
        pos2(place.right() - field - gap, place.bottom()),
    );
    let track = rail_in(room, density);
    paint_rail(painter, track, 0.0, 0.3, theme, look);
    paint_handle(
        painter,
        pos2(track.left() + 0.3 * track.width(), track.center().y),
        density,
        theme,
        look,
    );
    let field_rect = Rect::from_min_max(pos2(place.right() - field, place.top()), place.max);
    let field_look = Look {
        hover: 0.0,
        pressed: false,
        focus: 0.0,
        ..look
    };
    paint_number_sample(
        painter, field_rect, theme, density, 12.0, "px", field_look, None,
    );
}

/// The live column of one density: one of each slider, as a studio draws
/// them.
fn live_column(ui: &mut Ui, theme: &Theme) {
    let width = ui.available_width();
    let mut size = 12.0;
    let mut plain = 0.6;
    let mut low = 20.0;
    let mut high = 80.0;
    let mut wedge_value = 40.0;
    let mut tall_size = 30.0;
    let mut opacity = 80.0;
    let mut off = 50.0;
    slider("size", &mut size, 1.0..=1000.0)
        .label("Size")
        .suffix("px")
        .logarithmic()
        .decimals(1)
        .width(width)
        .show(ui, theme);
    slider("plain", &mut plain, 0.0..=1.0)
        .name("Plain")
        .field(false)
        .decimals(2)
        .width(width)
        .show(ui, theme);
    range("range", &mut low, &mut high, 0.0..=100.0)
        .label("Pressure")
        .suffix("%")
        .width(width)
        .show(ui, theme);
    wedge("wedge", &mut wedge_value, 0.0..=100.0)
        .label("Opacity")
        .width(width.min(160.0))
        .show(ui, theme);
    slider("off", &mut off, 0.0..=100.0)
        .label("Disabled")
        .enabled(false)
        .width(width)
        .show(ui, theme);
    ui.horizontal(|ui| {
        let height = ui.spacing().interact_size.y * 6.0;
        tall_slider("tall", &mut tall_size, 1.0..=100.0)
            .label("Size")
            .height(height)
            .show(ui, theme);
        tall_slider("opacity", &mut opacity, 0.0..=100.0)
            .label("Opacity")
            .fading()
            .height(height)
            .show(ui, theme);
    });
}

fn densities_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let columns = ["Compact", "Regular", "Touch"];
    let table = Table::new(
        painter,
        rect,
        theme,
        "Sliders, live",
        &columns,
        1,
        f32::INFINITY,
    );
    for (column, density) in Density::ALL.into_iter().enumerate() {
        let cell = table.cell(0, column);
        let room = Rect::from_min_max(
            cell.min,
            pos2(
                cell.right() - theme.space(Space::S7),
                rect.bottom() - theme.space(Space::S9),
            ),
        );
        let mut ui = Ui::new(
            painter.ctx().clone(),
            Id::unique(("sue-sliders-sheet", column)),
            UiBuilder::new().max_rect(room).layer_id(painter.layer_id()),
        );
        ui.spacing_mut().item_spacing = Vec2::splat(theme.space(Space::S5));
        density.scope(&mut ui, |ui| live_column(ui, theme));
    }
}

#[cfg(test)]
mod tests;
