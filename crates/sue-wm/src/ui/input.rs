//! Inputs: the controls a person types a value into or picks one from.
//!
//! - [`text_field`]: one line of text.
//! - [`search_field`]: a text field with a magnifier before it and a
//!   button that clears it.
//! - [`number_field`]: a number, dragged sideways or typed, with a unit
//!   after it. The arrow keys step it while it is being typed in.
//! - [`dropdown`]: one of a list of answers, picked from a list that
//!   opens under it. The wheel over it steps through the answers and each
//!   step takes effect at once, so whatever the answer drives shows it
//!   live (a blend mode, a font).
//! - [`combo`]: a text field that also offers a list of answers, for
//!   values that are usually one of a few but may be anything (a size in
//!   points).
//!
//! Like every component of [`crate::ui`], each reads its sizes from the
//! [`Density`] of its `Ui` and its colors from the [`Theme`], shows where
//! keyboard focus is, tells assistive technology what it is and what it
//! holds, moves through [`motion`] and is named by a salt its caller
//! gives it: the salt, and not the place it is drawn at, keeps its focus,
//! what is being typed and whether its list is open from frame to frame.
//! Two controls in the same `Ui` need different salts.
//!
//! A field that is being typed in shows it by its outline, in the focus
//! color and twice as thick; that is its focus ring.
//!
//! [`sheet`] draws all of them, in every state, for review.

use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::AsIdSalt;
use egui::text::{CCursorRange, LayoutJob, TextWrapping};
use egui::{
    Align2, Area, Color32, CursorIcon, Event, EventFilter, FontId, Frame, Galley, Id, IdSalt, Key,
    Margin, Modifiers, MouseWheelUnit, Painter, Pos2, Rect, Response, RichText, Role, ScrollArea,
    Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use super::{Density, Elevation, Layer, Radius, Space, TextRole, contrast, focus, motion};
use crate::Theme;
use crate::icons::{self, Icon};

/// How long the pointer has to rest on a dropdown before the wheel steps
/// it rather than scrolling the page, in seconds. A page scrolled with the
/// wheel goes on scrolling when a dropdown passes under the pointer.
pub const WHEEL_SETTLE: f64 = 0.2;

/// How many points a trackpad has to scroll over a dropdown to step it
/// once, at the density's row height.
const POINTS_PER_STEP: f32 = 1.0;

/// How far the outline of a field moves toward the text color under the
/// pointer.
const EDGE_HOVER: f32 = 0.5;

/// How far the fill of a raised control moves toward the hover fill
/// while the pointer is on it.
const RAISED_HOVER: f32 = 1.0;

/// How many rows the list of a dropdown shows at most before it scrolls.
const LIST_ROWS: usize = 12;

// ----- Shared with the sliders ----------------------------------------------

/// `from` moved the share `t` of the way to `to`.
pub(super) fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

/// The text and marks of a control that cannot be used now. A control
/// does not know which surface it lies on, so this is the disabled color
/// that reads best on the page, on panels and on cards at once.
pub(super) fn disabled_ink(theme: &Theme) -> Color32 {
    let worst = |ink: Color32| {
        (0..=2)
            .map(|level| contrast(ink, theme.surface(level)))
            .fold(f32::INFINITY, f32::min)
    };
    (0..=2)
        .map(|level| theme.text_disabled(theme.surface(level)))
        .max_by(|a, b| worst(*a).total_cmp(&worst(*b)))
        .unwrap_or(theme.muted)
}

/// The id of the control its caller named `salt`, in `ui`: stable from
/// frame to frame wherever in `ui` it is drawn.
pub(super) fn control_id(ui: &Ui, salt: IdSalt) -> Id {
    ui.scope_id().with_salt(salt)
}

/// `rect`, grown about its middle until no side is shorter than `least`.
pub(super) fn hit_rect(rect: Rect, least: f32) -> Rect {
    let grow = ((Vec2::splat(least) - rect.size()) / 2.0).max(Vec2::ZERO);
    rect.expand2(grow)
}

/// One line of `text` in `font`, cut short with an ellipsis where it is
/// wider than `width`. Its color is given when it is painted.
pub(super) fn line(painter: &Painter, text: &str, font: FontId, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, Color32::PLACEHOLDER);
    if width.is_finite() {
        job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    }
    painter.layout_job(job)
}

/// What a control looks like at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Look {
    /// It can be used.
    pub enabled: bool,
    /// How far it is on its way to its look under the pointer, 0 to 1.
    pub hover: f32,
    /// The pointer is held down on it (or it is being dragged).
    pub pressed: bool,
    /// How far it is on its way to its look while it is being typed in or
    /// its list is open, 0 to 1.
    pub active: f32,
    /// The opacity of its focus ring, 0 to 1.
    pub focus: f32,
    /// What is typed in it is not a value it can take.
    pub invalid: bool,
}

impl Look {
    /// Nothing is happening to it.
    pub(super) const REST: Self = Self {
        enabled: true,
        hover: 0.0,
        pressed: false,
        active: 0.0,
        focus: 0.0,
        invalid: false,
    };

    /// Under the pointer.
    pub(super) const HOVER: Self = Self {
        hover: 1.0,
        ..Self::REST
    };

    /// Held down.
    pub(super) const PRESSED: Self = Self {
        hover: 1.0,
        pressed: true,
        ..Self::REST
    };

    /// Holding keyboard focus.
    pub(super) const FOCUS: Self = Self {
        focus: 1.0,
        ..Self::REST
    };

    /// Being typed in, or open.
    pub(super) const ACTIVE: Self = Self {
        active: 1.0,
        ..Self::REST
    };

    /// Not usable now.
    pub(super) const DISABLED: Self = Self {
        enabled: false,
        ..Self::REST
    };

    /// The look of the control behind `response`, whose hover fill and
    /// focus ring fade at the fast pace. `active` says whether it is being
    /// typed in or open.
    pub(super) fn of(ui: &Ui, response: &Response, enabled: bool, active: bool) -> Self {
        let ctx = ui.ctx();
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && (response.hovered() || pressed);
        let focused = enabled && !active && focus::focus_visible(ctx) && response.has_focus();
        Self {
            enabled,
            hover: motion::hover_t(ctx, response.id.with("sue-hover"), hovered),
            pressed,
            active: motion::hover_t(ctx, response.id.with("sue-active"), enabled && active),
            focus: motion::hover_t(ctx, response.id.with("sue-focus-ring"), focused),
            invalid: false,
        }
    }
}

/// The padding at either end of a field, at `density`.
pub(super) fn side_padding(theme: &Theme, density: Density) -> f32 {
    theme.space(match density {
        Density::Compact => Space::S3,
        Density::Regular => Space::S4,
        Density::Touch => Space::S5,
    })
}

/// Paints a chevron `size` wide about `center`, pointing down, or up
/// when `flip` is 1.
pub(super) fn paint_chevron(painter: &Painter, center: Pos2, size: f32, flip: f32, ink: Color32) {
    let half = size / 2.0;
    let rise = size * 0.25 * (1.0 - 2.0 * flip.clamp(0.0, 1.0));
    painter.line(
        vec![
            pos2(center.x - half, center.y - rise),
            pos2(center.x, center.y + rise),
            pos2(center.x + half, center.y - rise),
        ],
        Stroke::new(icons::weight(size * 2.0), ink),
    );
}

// ----- How fields are painted -------------------------------------------------

/// The two bodies a field can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    /// Sunk into the page: what is typed in (text, search, number and
    /// combo fields).
    Inset,
    /// Raised like a button: what opens a list (the dropdown).
    Raised,
}

/// The fill of a field's body in `look`.
fn body_fill(theme: &Theme, body: Body, look: Look) -> Color32 {
    match body {
        Body::Inset => theme.surface(0),
        Body::Raised if !look.enabled => theme.surface(2),
        Body::Raised if look.pressed || look.active > 0.5 => theme.fill_pressed(),
        Body::Raised => mix(
            theme.surface(2),
            theme.fill_hover(),
            look.hover * RAISED_HOVER,
        ),
    }
}

/// The outline of a field's body in `look`: thicker and in the focus
/// color while it is typed in or open.
fn body_edge(theme: &Theme, look: Look) -> Stroke {
    if !look.enabled {
        return Stroke::new(
            theme.border_width,
            mix(theme.border, theme.border_strong(), 0.5),
        );
    }
    if look.invalid {
        return Stroke::new(Theme::STRONG_STROKE, theme.danger());
    }
    let rest = mix(theme.border_strong(), theme.text, look.hover * EDGE_HOVER);
    let width = theme.border_width + (Theme::STRONG_STROKE - theme.border_width) * look.active;
    Stroke::new(width, mix(rest, theme.focus(), look.active))
}

/// The radius of a field `rect` high.
fn field_radius(theme: &Theme, rect: Rect) -> f32 {
    theme.radius(Radius::Md).min(rect.height() / 2.0)
}

/// The body of a field, as one shape.
fn body_shape(theme: &Theme, rect: Rect, body: Body, look: Look) -> Shape {
    Shape::Rect(egui::epaint::RectShape::new(
        rect,
        field_radius(theme, rect),
        body_fill(theme, body, look),
        body_edge(theme, look),
        StrokeKind::Inside,
    ))
}

/// The ink of what a field holds.
fn field_ink(theme: &Theme, look: Look) -> Color32 {
    if look.enabled {
        theme.text
    } else {
        disabled_ink(theme)
    }
}

/// The ink of a field's hint, unit and marks on `fill`.
fn quiet_ink(theme: &Theme, look: Look, fill: Color32) -> Color32 {
    if look.enabled {
        theme.text_muted_on(fill)
    } else {
        disabled_ink(theme)
    }
}

/// Paints the mark of a value that cannot be taken into the square
/// `rect`: a ring with a stem over a dot.
fn paint_alert(painter: &Painter, rect: Rect, ink: Color32) {
    let side = rect.width().min(rect.height());
    let pen = icons::weight(side);
    let center = rect.center();
    let radius = side / 2.0 - pen / 2.0;
    painter.circle_stroke(center, radius, Stroke::new(pen, ink));
    painter.line_segment(
        [
            pos2(center.x, center.y - radius * 0.5),
            pos2(center.x, center.y + radius * 0.1),
        ],
        Stroke::new(pen, ink),
    );
    painter.circle_filled(pos2(center.x, center.y + radius * 0.45), pen * 0.6, ink);
}

/// Paints a cross into the square `rect`, under a round hover fill.
fn paint_clear(painter: &Painter, rect: Rect, theme: &Theme, hover: f32, ink: Color32) {
    if hover > 0.0 {
        painter.circle_filled(
            rect.center(),
            rect.width() / 2.0,
            theme.fill_hover().gamma_multiply(hover),
        );
    }
    let arm = rect.width() * 0.2;
    let center = rect.center();
    let stroke = Stroke::new(icons::weight(rect.width()), ink);
    painter.line_segment([center + vec2(-arm, -arm), center + vec2(arm, arm)], stroke);
    painter.line_segment([center + vec2(-arm, arm), center + vec2(arm, -arm)], stroke);
}

/// The square at the right end of a field `rect` that holds its
/// trailing button or mark.
fn trailing_square(rect: Rect) -> Rect {
    Rect::from_min_max(pos2(rect.right() - rect.height(), rect.top()), rect.max)
}

/// The square at the left end of a field `rect` that holds its leading
/// icon, inset by the side padding.
fn leading_square(rect: Rect, theme: &Theme, density: Density) -> Rect {
    let side = density.icon();
    Rect::from_center_size(
        pos2(
            rect.left() + side_padding(theme, density) + side / 2.0,
            rect.center().y,
        ),
        Vec2::splat(side),
    )
}

/// The room left and right of what is typed in a field: the padding, and
/// the icon and buttons at its ends.
fn text_margins(
    rect: Rect,
    theme: &Theme,
    density: Density,
    leading: bool,
    trailing: f32,
) -> (f32, f32) {
    let padding = side_padding(theme, density);
    let left = if leading {
        padding + density.icon() + theme.space(Space::S3)
    } else {
        padding
    };
    let right = if trailing > 0.0 { trailing } else { padding };
    (left.min(rect.width() / 2.0), right.min(rect.width() / 2.0))
}

/// The margin a text edit is given to keep its text between `left` and
/// `right` points from the ends of its field.
fn edit_margin(left: f32, right: f32) -> Margin {
    let points = |value: f32| value.round().clamp(0.0, 127.0) as i8;
    Margin {
        left: points(left),
        right: points(right),
        top: 0,
        bottom: 0,
    }
}

/// The text field's caret, drawn in a review sheet where no text edit
/// draws its own.
fn paint_caret(painter: &Painter, x: f32, rect: Rect, theme: &Theme) {
    let height = theme.text_size(TextRole::Body) * 1.2;
    painter.line_segment(
        [
            pos2(x, rect.center().y - height / 2.0),
            pos2(x, rect.center().y + height / 2.0),
        ],
        Stroke::new(2.0, theme.text),
    );
}

// ----- Text and search fields -------------------------------------------------

/// A field of one line of text. Made by [`text_field`] and
/// [`search_field`].
#[must_use = "a field is drawn by `show`"]
pub struct TextField<'a> {
    salt: IdSalt,
    text: &'a mut String,
    hint: &'a str,
    label: &'a str,
    enabled: bool,
    width: Option<f32>,
    password: bool,
    search: bool,
    lines: usize,
}

/// A field that holds one line of `text`. `salt` names it among the
/// controls of its `Ui`.
pub fn text_field(salt: impl AsIdSalt, text: &mut String) -> TextField<'_> {
    TextField {
        salt: IdSalt::new(salt),
        text,
        hint: "",
        label: "",
        enabled: true,
        width: None,
        password: false,
        search: false,
        lines: 1,
    }
}

/// A field to search with: a magnifier before what is typed, and a
/// button that clears it while it holds anything.
pub fn search_field(salt: impl AsIdSalt, text: &mut String) -> TextField<'_> {
    TextField {
        search: true,
        ..text_field(salt, text)
    }
}

impl<'a> TextField<'a> {
    /// Shows `hint` in the field while it is empty.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// The name assistive technology reads out for it, where no label
    /// beside it says what it is for. The hint is read out otherwise.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Whether it can be used. A field that cannot is grayed and takes
    /// neither typing nor focus.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide. Without it, it is as wide as the room it is
    /// shown in.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Shows dots for what is typed.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Makes it a field of several lines, `lines` tall, for a note or a
    /// summary: Enter starts a new line, the text wraps at its width, and
    /// it grows past `lines` as more is written. 1 (the default) is a
    /// field of one line. A search field stays one line.
    pub fn lines(mut self, lines: usize) -> Self {
        self.lines = lines.max(1);
        self
    }

    /// How many lines it shows: more than one only for a field asked to
    /// hold several and that is not a search field.
    fn shown_lines(&self) -> usize {
        if self.search { 1 } else { self.lines }
    }

    /// The height of a field of `lines` lines: a control's height for one,
    /// and room for each line and the field's padding for more.
    fn height(ui: &Ui, theme: &Theme, density: Density, lines: usize) -> f32 {
        if lines <= 1 {
            return density.control_height();
        }
        let row = ui.fonts_mut(|fonts| fonts.row_height(&theme.text_font(TextRole::Body)));
        let pad = side_padding(theme, density) / 2.0;
        (row * lines as f32 + pad * 2.0)
            .max(density.control_height())
            .ceil()
    }

    /// Shows it in the layout of `ui`. The response is that of the text:
    /// `changed()` when something was typed or cleared, `lost_focus()`
    /// when typing ended (with Enter, Tab, a click elsewhere or Escape).
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = field_width(ui, self.width, density);
        let lines = self.shown_lines();
        let mut height = Self::height(ui, theme, density, lines);
        if lines > 1 {
            // A field of several lines grows with what it holds: last
            // frame's height of the text, kept under its id.
            let grown: Option<f32> =
                ui.data(|data| data.get_temp(control_id(ui, self.salt).with("sue-grown")));
            height = height.max(grown.unwrap_or(0.0));
        }
        let rect = ui.allocate_space(vec2(width, height)).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let body = ui.painter().add(Shape::Noop);
        let clearable = self.search && enabled && !self.text.is_empty();
        let trailing = if clearable { rect.height() } else { 0.0 };
        let (left, right) = text_margins(rect, theme, density, self.search, trailing);
        let rest = Look {
            enabled,
            ..Look::REST
        };
        let fill = body_fill(theme, Body::Inset, rest);
        let hint = RichText::new(self.hint).color(quiet_ink(theme, rest, fill));
        let lines = self.shown_lines();
        let edit = if lines > 1 {
            let pad = edit_margin(side_padding(theme, density) / 2.0, 0.0).left;
            let mut margin = edit_margin(left, right);
            margin.top = pad;
            margin.bottom = pad;
            TextEdit::multiline(&mut *self.text)
                .frame(Frame::NONE.inner_margin(margin))
                .desired_rows(lines)
                .align(Align2::LEFT_TOP)
        } else {
            TextEdit::singleline(&mut *self.text)
                .frame(Frame::NONE.inner_margin(edit_margin(left, right)))
                .align(Align2::LEFT_CENTER)
        };
        let edit = edit
            .id(id)
            .font(theme.text_font(TextRole::Body))
            .text_color(field_ink(theme, rest))
            .hint_text(hint)
            .password(self.password)
            .interactive(enabled)
            .desired_width(rect.width());
        let shown = ui.put(rect, edit);
        if lines > 1 {
            let grown = shown.rect.height();
            ui.data_mut(|data| data.insert_temp(id.with("sue-grown"), grown));
        }
        let mut response = shown.with_new_rect(rect);
        let look = Look::of(ui, &response, enabled, response.has_focus());
        let look = Look { focus: 0.0, ..look };
        ui.painter()
            .set(body, body_shape(theme, rect, Body::Inset, look));
        if self.search {
            icons::paint(
                ui.painter(),
                leading_square(rect, theme, density),
                Icon::Search,
                quiet_ink(theme, look, fill),
            );
        }
        if clearable {
            let square = trailing_square(rect);
            let clear = ui.interact(square, id.with("sue-clear"), Sense::CLICK);
            let hover = motion::hover_t(ui.ctx(), clear.id, clear.hovered());
            paint_clear(
                ui.painter(),
                square.shrink(square.height() * 0.2),
                theme,
                hover,
                quiet_ink(theme, look, fill),
            );
            let name = tr!("field-clear");
            clear.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
            if clear.clicked() {
                self.text.clear();
                response.mark_changed();
                ui.memory_mut(|memory| memory.request_focus(id));
            }
        }
        if !self.label.is_empty() {
            let (label, text) = (self.label, self.text.clone());
            response.widget_info(|| {
                let mut info = WidgetInfo::labeled(Role::TextInput, enabled, label);
                info.current_text_value = Some(text.clone());
                info
            });
        }
        response
    }
}

/// The width of a field asked to be `width` wide, or of the room in
/// `ui`.
fn field_width(ui: &Ui, width: Option<f32>, density: Density) -> f32 {
    let least = density.control_height() * 2.0;
    match width {
        Some(width) => width.max(least),
        None => {
            let room = ui.available_width();
            if room.is_finite() {
                room.max(least)
            } else {
                least * 4.0
            }
        }
    }
}

// ----- Numbers ----------------------------------------------------------------

/// The decimal separator of the language being read now, such as `.` or
/// `,`.
fn decimal_separator() -> String {
    let half = sue_lang::number(0.5, 1);
    half.get(1..half.len().saturating_sub(1))
        .unwrap_or(".")
        .to_owned()
}

/// The separator between groups of thousands in the language being read
/// now, if it has one that is not a space.
fn group_separator() -> Option<char> {
    // Large enough that every language groups it.
    let large = sue_lang::number(1e12, 0);
    let middle = large.get(1..large.len().saturating_sub(3))?;
    let mut chars = middle.chars();
    let group = chars.next()?;
    (!group.is_ascii_digit() && !group.is_whitespace()).then_some(group)
}

/// `value` with `decimals` digits after the language's decimal separator
/// and no group separators: what a field gives to be typed over.
#[must_use]
pub fn plain_number(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value);
    let text = if text
        .trim_start_matches('-')
        .bytes()
        .all(|byte| matches!(byte, b'0' | b'.'))
    {
        text.trim_start_matches('-').to_owned()
    } else {
        text
    };
    text.replace('.', &decimal_separator())
}

/// Reads a number typed in the language being read now: its decimal
/// separator, group separators only where three digits follow them, any
/// spaces, a minus sign or a hyphen, and `suffix` (the unit) at the end.
/// A point is read as a decimal point too where it cannot be a group
/// separator, so `1.5` is one and a half in every language. `None` when
/// it is not a number.
#[must_use]
pub fn parse_number(text: &str, suffix: &str) -> Option<f64> {
    let mut text = text.trim();
    let unit = suffix.trim();
    if !unit.is_empty()
        && let Some(rest) = text.strip_suffix(unit)
    {
        text = rest;
    }
    let decimal = decimal_separator();
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    let group = group_separator().filter(|group| decimal.chars().ne(std::iter::once(*group)));
    let mut kept = String::with_capacity(chars.len());
    for (index, c) in chars.iter().enumerate() {
        let is_group = Some(*c) == group && {
            let after = &chars[index + 1..];
            index > 0
                && chars[index - 1].is_ascii_digit()
                && after.len() >= 3
                && after[..3].iter().all(char::is_ascii_digit)
                && after.get(3).is_none_or(|next| !next.is_ascii_digit())
        };
        if !is_group {
            kept.push(*c);
        }
    }
    let kept = if decimal == "." {
        kept
    } else {
        kept.replace(decimal.as_str(), ".")
    };
    if kept.is_empty() || kept.matches('.').count() > 1 {
        return None;
    }
    kept.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// What is typed into a number field, kept between frames.
#[derive(Clone, Debug, Default)]
struct Typing {
    /// What is typed so far.
    text: String,
    /// The value the field had when it was typed: the typing is dropped
    /// when something else changes the value meanwhile.
    value: f64,
}

/// A field that holds a number. Made by [`number_field`].
#[must_use = "a field is drawn by `show`"]
pub struct NumberField<'a> {
    salt: IdSalt,
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    decimals: usize,
    speed: Option<f64>,
    suffix: &'a str,
    label: &'a str,
    enabled: bool,
    width: Option<f32>,
}

/// A field that holds the number `value`: dragged sideways to change it,
/// clicked to type it, stepped with the arrow keys while it is typed in.
/// `salt` names it among the controls of its `Ui`.
pub fn number_field(salt: impl AsIdSalt, value: &mut f64) -> NumberField<'_> {
    NumberField {
        salt: IdSalt::new(salt),
        value,
        range: f64::NEG_INFINITY..=f64::INFINITY,
        step: 1.0,
        decimals: 0,
        speed: None,
        suffix: "",
        label: "",
        enabled: true,
        width: None,
    }
}

impl<'a> NumberField<'a> {
    /// Keeps the value inside `range`. Dragging and typing never leave
    /// it; a value set from outside is shown as it is.
    pub fn range(mut self, range: RangeInclusive<f64>) -> Self {
        self.range = range;
        self
    }

    /// What one press of an arrow key adds (ten for Page Up).
    pub fn step(mut self, step: f64) -> Self {
        self.step = step.abs();
        self
    }

    /// How many digits it shows after the decimal separator, and keeps.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(9);
        self
    }

    /// How much one point of dragging adds. Without it, dragging across
    /// 200 points covers the whole range, or a step every two points
    /// when the range has no ends.
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = Some(speed.abs());
        self
    }

    /// The unit shown after the number, such as `px` or `%`. It may be
    /// typed after the number too.
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    /// The name assistive technology reads out for it.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide. Without it, it is wide enough for a few
    /// digits and the unit at its density.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The range, low end first.
    fn bounds(&self) -> (f64, f64) {
        let (low, high) = (*self.range.start(), *self.range.end());
        if low <= high {
            (low, high)
        } else {
            (high, low)
        }
    }

    /// `value` rounded to the field's decimals and held inside its range.
    fn tidy(&self, value: f64) -> f64 {
        let (low, high) = self.bounds();
        let factor = 10_f64.powi(self.decimals as i32);
        let rounded = (value * factor).round() / factor;
        let rounded = if rounded == 0.0 { 0.0 } else { rounded };
        rounded.clamp(low, high)
    }

    /// How much a point of dragging adds.
    fn drag_speed(&self) -> f64 {
        if let Some(speed) = self.speed {
            return speed;
        }
        let (low, high) = self.bounds();
        if (high - low).is_finite() && high > low {
            (high - low) / 200.0
        } else {
            self.step.max(10_f64.powi(-(self.decimals as i32))) / 2.0
        }
    }

    /// The default width at `density`.
    fn default_width(density: Density) -> f32 {
        (density.control_height() * 2.6).round()
    }

    /// Shows it in the layout of `ui`. `changed()` on the response says
    /// the value changed, by dragging, stepping or typing (which takes
    /// effect when typing ends).
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = self.width.unwrap_or_else(|| Self::default_width(density));
        let rect = ui.allocate_space(vec2(width, density.control_height())).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(mut self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let ctx = ui.ctx().clone();
        let typing_now = enabled
            && ui.memory_mut(|memory| {
                memory.interested_in_focus(id, ui.layer_id());
                memory.has_focus(id)
            });
        let was_typing =
            ctx.data(|data| data.get_temp::<bool>(id.with("sue-typing"))) == Some(true);
        let escape = ui.input(|input| input.key_pressed(Key::Escape));
        let old = *self.value;
        // Typing that ended without this field being drawn as a text edit
        // (focus went elsewhere in a frame it drew first) still counts.
        if was_typing && !typing_now {
            let typed = ctx.data_mut(|data| data.remove_temp::<Typing>(id));
            if !escape
                && let Some(typed) = typed.filter(|typed| typed.value == old)
                && let Some(parsed) = parse_number(&typed.text, self.suffix)
            {
                *self.value = self.tidy(parsed);
            }
        }
        let slot = ui.painter().add(Shape::Noop);
        let suffix_width = if self.suffix.is_empty() {
            0.0
        } else {
            line(
                ui.painter(),
                self.suffix,
                theme.text_font(TextRole::Body),
                f32::INFINITY,
            )
            .size()
            .x + unit_gap(theme)
        };
        let padding = side_padding(theme, density);
        let mut invalid = false;
        let mut response = if typing_now {
            // The arrow keys step the value while it is typed in.
            let steps = ui.input_mut(|input| {
                let count = |input: &mut egui::InputState, key| {
                    input.count_and_consume_key(Modifiers::NONE, key) as f64
                };
                count(input, Key::ArrowUp) - count(input, Key::ArrowDown)
                    + 10.0 * (count(input, Key::PageUp) - count(input, Key::PageDown))
            });
            if steps != 0.0 {
                let from = ctx
                    .data(|data| data.get_temp::<Typing>(id))
                    .and_then(|typed| parse_number(&typed.text, self.suffix))
                    .unwrap_or(*self.value);
                *self.value = self.tidy(from + steps * self.step);
                ctx.data_mut(|data| data.remove_temp::<Typing>(id));
            }
            let mut text = ctx
                .data_mut(|data| data.remove_temp::<Typing>(id))
                .filter(|typed| typed.value == *self.value)
                .map_or_else(
                    || plain_number(*self.value, self.decimals),
                    |typed| typed.text,
                );
            let rest = Look {
                enabled,
                ..Look::REST
            };
            let edit = TextEdit::singleline(&mut text)
                .id(id)
                .frame(Frame::NONE.inner_margin(edit_margin(padding, padding + suffix_width)))
                .font(theme.text_font(TextRole::Body))
                .text_color(field_ink(theme, rest))
                .align(Align2::RIGHT_CENTER)
                .desired_width(rect.width());
            let mut response = ui.put(rect, edit).with_new_rect(rect);
            if !was_typing || steps != 0.0 {
                select_all(&ctx, id, &text);
            }
            // Typing is not a change of the value: that comes when typing
            // ends.
            response.flags.remove(egui::response::Flags::CHANGED);
            invalid = !text.trim().is_empty() && parse_number(&text, self.suffix).is_none();
            if response.lost_focus() || !response.has_focus() {
                if !escape && let Some(parsed) = parse_number(&text, self.suffix) {
                    *self.value = self.tidy(parsed);
                }
                ctx.data_mut(|data| data.insert_temp(id.with("sue-typing"), false));
            } else {
                ctx.data_mut(|data| {
                    data.insert_temp(
                        id,
                        Typing {
                            text,
                            value: *self.value,
                        },
                    );
                    data.insert_temp(id.with("sue-typing"), true);
                });
            }
            response
        } else {
            ctx.data_mut(|data| data.insert_temp(id.with("sue-typing"), false));
            let sense = if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            };
            let response = ui
                .interact(hit_rect(rect, density.min_hit()), id, sense)
                .with_new_rect(rect);
            self.drag(ui, &response);
            if response.clicked() {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            if enabled && (response.hovered() || response.dragged()) {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
            response
        };
        let typing = response.has_focus() && typing_now;
        let look = Look {
            invalid,
            focus: 0.0,
            ..Look::of(ui, &response, enabled, typing)
        };
        ui.painter()
            .set(slot, body_shape(theme, rect, Body::Inset, look));
        let fill = body_fill(theme, Body::Inset, look);
        if !typing_now {
            paint_number_face(
                ui.painter(),
                rect,
                theme,
                density,
                &shown_number(*self.value, self.decimals),
                self.suffix,
                look,
            );
        } else if !self.suffix.is_empty() {
            let galley = line(
                ui.painter(),
                self.suffix,
                theme.text_font(TextRole::Body),
                f32::INFINITY,
            );
            let at = pos2(
                rect.right() - padding - galley.size().x,
                rect.center().y - galley.size().y / 2.0,
            );
            ui.painter()
                .galley(at, galley, quiet_ink(theme, look, fill));
        }
        if invalid {
            paint_invalid_mark(ui.painter(), rect, theme, density);
        }
        if *self.value != old {
            response.mark_changed();
        }
        let (label, value, suffix) = (self.label, *self.value, self.suffix);
        let shown = shown_number(value, self.decimals);
        response.widget_info(|| {
            let mut info = WidgetInfo::drag_value(enabled, value);
            if !label.is_empty() {
                info.label = Some(label.to_owned());
            }
            info.current_text_value = Some(if suffix.is_empty() {
                shown.clone()
            } else {
                format!("{shown} {suffix}")
            });
            info
        });
        response
    }

    /// Changes the value by the drag of `response`: to the right and up
    /// adds. Shift slows it ten times.
    fn drag(&mut self, ui: &Ui, response: &Response) {
        let precise = response.id.with("sue-precise");
        if response.drag_started() || !response.dragged() {
            ui.data_mut(|data| data.remove_temp::<f64>(precise));
        }
        if !response.dragged() {
            return;
        }
        let delta = response.drag_delta();
        let points = f64::from(delta.x - delta.y);
        if points == 0.0 {
            return;
        }
        let slow = ui.input(|input| input.modifiers.shift_only());
        let speed = self.drag_speed() * if slow { 0.1 } else { 1.0 };
        let from = ui
            .data(|data| data.get_temp::<f64>(precise))
            .unwrap_or(*self.value);
        let (low, high) = self.bounds();
        let to = (from + points * speed).clamp(low, high);
        ui.data_mut(|data| data.insert_temp(precise, to));
        *self.value = self.tidy(to);
    }
}

/// The room between a number and its unit.
fn unit_gap(theme: &Theme) -> f32 {
    theme.space(Space::S2)
}

/// `value` as a number field shows it: in the language's way, with
/// `decimals` digits after the separator.
fn shown_number(value: f64, decimals: usize) -> String {
    sue_lang::number(value, decimals)
}

/// Selects everything typed in the text edit `id`, as a field that was
/// just entered does.
fn select_all(ctx: &egui::Context, id: Id, text: &str) {
    let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::select_all_str(text)));
    state.store(ctx, id);
}

/// Paints the number and unit of a number field that is not being typed
/// in: at its right end, the unit quieter than the number.
fn paint_number_face(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    density: Density,
    number: &str,
    suffix: &str,
    look: Look,
) {
    let padding = side_padding(theme, density);
    let font = theme.text_font(TextRole::Body);
    let fill = body_fill(theme, Body::Inset, look);
    let mut right = rect.right() - padding;
    if !suffix.is_empty() {
        let unit = line(painter, suffix, font.clone(), f32::INFINITY);
        right -= unit.size().x;
        painter.galley(
            pos2(right, rect.center().y - unit.size().y / 2.0),
            unit,
            quiet_ink(theme, look, fill),
        );
        right -= unit_gap(theme);
    }
    let room = (right - rect.left() - padding).max(0.0);
    let galley = line(painter, number, font, room);
    painter.galley(
        pos2(
            right - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        field_ink(theme, look),
    );
}

/// Paints the mark of a value that cannot be taken at the left end of a
/// number field, where right-aligned digits leave room.
fn paint_invalid_mark(painter: &Painter, rect: Rect, theme: &Theme, density: Density) {
    let side = density.icon() * 0.8;
    let square = Rect::from_center_size(
        pos2(
            rect.left() + side_padding(theme, density) + side / 2.0,
            rect.center().y,
        ),
        Vec2::splat(side),
    );
    paint_alert(painter, square, theme.danger());
}

// ----- The open list ------------------------------------------------------------

/// The list of a dropdown or a combo, between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ListState {
    /// It is open.
    open: bool,
    /// The row the keyboard or the pointer is on.
    highlight: usize,
    /// The highlight moved by keyboard this frame: the list scrolls to
    /// show it.
    follow: bool,
}

fn list_state(ctx: &egui::Context, id: Id) -> ListState {
    ctx.data(|data| data.get_temp(id.with("sue-list")))
        .unwrap_or_default()
}

fn store_list(ctx: &egui::Context, id: Id, state: ListState) {
    ctx.data_mut(|data| data.insert_temp(id.with("sue-list"), state));
}

/// The id of row `index` of the list of `id`.
fn row_id(id: Id, index: usize) -> Id {
    id.with(("sue-row", index))
}

/// How a list is laid out: its rows' height and the padding around them.
struct ListMetrics {
    row: f32,
    padding: f32,
    radius: f32,
}

impl ListMetrics {
    fn new(theme: &Theme, density: Density) -> Self {
        Self {
            row: density.row(),
            padding: theme.space(Space::S2),
            radius: theme.radius(Radius::Xl),
        }
    }

    /// The radius of a row inside the list's corner.
    fn row_radius(&self, theme: &Theme) -> f32 {
        super::nested_radius(self.radius, self.padding).min(theme.radius(Radius::Md))
    }
}

/// Where the list of the field `anchor` goes in `screen`: under it, or
/// over it when there is more room there, as wide as the field or its
/// widest row, and as tall as its rows, up to [`LIST_ROWS`] of them and
/// the room there is.
fn list_rect(anchor: Rect, screen: Rect, width: f32, rows: usize, metrics: &ListMetrics) -> Rect {
    let margin = metrics.padding;
    let gap = metrics.padding;
    let full = rows.clamp(1, LIST_ROWS) as f32 * metrics.row + metrics.padding * 2.0;
    let below = screen.bottom() - anchor.bottom() - gap - margin;
    let above = anchor.top() - screen.top() - gap - margin;
    let width = width.max(anchor.width()).min(screen.width() - margin * 2.0);
    let left = anchor
        .left()
        .min(screen.right() - margin - width)
        .max(screen.left() + margin);
    let (top, height) = if full <= below || below >= above {
        (anchor.bottom() + gap, full.min(below.max(metrics.row)))
    } else {
        let height = full.min(above.max(metrics.row));
        (anchor.top() - gap - height, height)
    };
    Rect::from_min_size(pos2(left, top), vec2(width, height))
}

/// The width a list needs to show every one of `options` whole.
fn list_width(painter: &Painter, options: &[&str], theme: &Theme, density: Density) -> f32 {
    let font = theme.text_font(TextRole::Body);
    let widest = options
        .iter()
        .map(|option| line(painter, option, font.clone(), f32::INFINITY).size().x)
        .fold(0.0, f32::max);
    widest + check_column(theme, density) + side_padding(theme, density) * 2.0
}

/// The width of the column the check mark of the chosen row sits in.
fn check_column(theme: &Theme, density: Density) -> f32 {
    density.icon() + theme.space(Space::S2)
}

/// Paints the ground of an open list over `rect`: a floating surface
/// with its shadow.
fn paint_list_ground(painter: &Painter, rect: Rect, theme: &Theme, metrics: &ListMetrics) {
    painter.add(
        theme
            .elevation(Elevation::Floating)
            .as_shape(rect, metrics.radius),
    );
    painter.rect(
        rect,
        metrics.radius,
        theme.surface(3),
        theme.hairline(),
        StrokeKind::Inside,
    );
}

/// Paints one row of a list: `text`, the check mark when it is the
/// `chosen` one, the hover fill when it is `lit`.
#[allow(clippy::too_many_arguments, reason = "one row and how it looks")]
fn paint_row(
    painter: &Painter,
    rect: Rect,
    text: &str,
    chosen: bool,
    lit: bool,
    theme: &Theme,
    density: Density,
    metrics: &ListMetrics,
) {
    if lit {
        painter.rect_filled(rect, metrics.row_radius(theme), theme.fill_hover());
    }
    let padding = side_padding(theme, density);
    let column = check_column(theme, density);
    if chosen {
        let side = density.icon() * 0.8;
        let mark = Rect::from_center_size(
            pos2(rect.left() + padding + side / 2.0, rect.center().y),
            Vec2::splat(side),
        );
        let at = |x: f32, y: f32| mark.min + vec2(x, y) * side;
        painter.line(
            vec![at(0.18, 0.52), at(0.4, 0.74), at(0.82, 0.28)],
            Stroke::new(icons::weight(side * 1.4), theme.text),
        );
    }
    let left = rect.left() + padding + column;
    let galley = line(
        painter,
        text,
        theme.text_font(TextRole::Body),
        rect.right() - padding - left,
    );
    painter.galley(
        pos2(left, rect.center().y - galley.size().y / 2.0),
        galley,
        theme.text,
    );
}

/// What an open list did in one frame.
#[derive(Clone, Copy, Debug, Default)]
struct ListOutcome {
    /// The row that was clicked.
    picked: Option<usize>,
    /// Where the list lies.
    rect: Option<Rect>,
}

/// Shows the open list of the field `anchor`, whose id is `id`, with
/// `options`, the `chosen` one checked, at `opacity`.
#[allow(clippy::too_many_arguments, reason = "a list and where it hangs")]
fn show_list(
    ui: &Ui,
    id: Id,
    anchor: Rect,
    options: &[&str],
    chosen: Option<usize>,
    state: &mut ListState,
    theme: &Theme,
    density: Density,
    opacity: f32,
) -> ListOutcome {
    let ctx = ui.ctx();
    let metrics = ListMetrics::new(theme, density);
    let width = list_width(ui.painter(), options, theme, density);
    let place = list_rect(anchor, ctx.content_rect(), width, options.len(), &metrics);
    let lift = (1.0 - opacity) * metrics.padding;
    let place = place.translate(vec2(
        0.0,
        if place.top() > anchor.top() {
            -lift
        } else {
            lift
        },
    ));
    let mut outcome = ListOutcome {
        picked: None,
        rect: Some(place),
    };
    let follow = std::mem::take(&mut state.follow);
    Area::new(id.with("sue-list-area"))
        .order(Layer::Popover.order())
        .fixed_pos(place.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.multiply_opacity(opacity);
            ui.set_min_size(place.size());
            paint_list_ground(ui.painter(), place, theme, &metrics);
            let inner = place.shrink(metrics.padding);
            let mut rows = ui.new_child(UiBuilder::new().max_rect(inner));
            ScrollArea::vertical()
                .id_salt(id.with("sue-list-scroll"))
                .max_height(inner.height())
                .auto_shrink([false, false])
                .show(&mut rows, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let moved = ui.input(|input| input.pointer.is_moving());
                    for (index, option) in options.iter().enumerate() {
                        let rect = ui.allocate_space(vec2(inner.width(), metrics.row)).1;
                        let row = ui.interact(rect, row_id(id, index), Sense::CLICK);
                        if row.hovered() && moved {
                            state.highlight = index;
                        }
                        if follow && index == state.highlight {
                            ui.scroll_to_rect(rect, None);
                        }
                        paint_row(
                            ui.painter(),
                            rect,
                            option,
                            chosen == Some(index),
                            state.highlight == index,
                            theme,
                            density,
                            &metrics,
                        );
                        row.widget_info(|| {
                            WidgetInfo::selected(
                                Role::ListBoxOption,
                                true,
                                chosen == Some(index),
                                *option,
                            )
                        });
                        if row.clicked() {
                            outcome.picked = Some(index);
                        }
                    }
                });
        });
    outcome
}

/// What the keys did to an open list this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ListKeys {
    /// Enter or Space picked the highlighted row.
    pick: bool,
    /// Escape or Tab closed it.
    close: bool,
    /// It was Escape.
    escape: bool,
}

/// Answers the keys of an open list of `count` rows: Up and Down move the
/// highlight, Home and End to the ends, Page Up and Page Down by a page,
/// Enter picks, Escape and Tab close, and a letter goes to the next row
/// that starts with it (when `type_ahead`). `space_picks` lets Space pick
/// too, where it is not typed.
fn list_keys(
    ui: &Ui,
    state: &mut ListState,
    options: &[&str],
    type_ahead: bool,
    space_picks: bool,
) -> ListKeys {
    let count = options.len();
    if count == 0 {
        return ListKeys {
            close: ui.input(|input| input.key_pressed(Key::Escape) || input.key_pressed(Key::Tab)),
            ..ListKeys::default()
        };
    }
    let last = count - 1;
    let page = LIST_ROWS.saturating_sub(1).max(1);
    let mut keys = ListKeys::default();
    let before = state.highlight;
    ui.input_mut(|input| {
        let mut press = |key| input.count_and_consume_key(Modifiers::NONE, key);
        let down = press(Key::ArrowDown) + page * press(Key::PageDown);
        let up = press(Key::ArrowUp) + page * press(Key::PageUp);
        let home = press(Key::Home) > 0;
        let end = press(Key::End) > 0;
        let enter = press(Key::Enter) > 0;
        let space = space_picks && press(Key::Space) > 0;
        let escape = press(Key::Escape) > 0;
        let mut at = state.highlight.min(last);
        at = (at + down).min(last).saturating_sub(up);
        if home {
            at = 0;
        }
        if end {
            at = last;
        }
        state.highlight = at;
        keys.pick = enter || space;
        keys.close = escape || input.key_pressed(Key::Tab);
        keys.escape = escape;
    });
    if type_ahead {
        let typed: Vec<char> = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    Event::Text(text) => text.chars().next(),
                    _ => None,
                })
                .filter(|c| !c.is_whitespace() || !space_picks)
                .collect()
        });
        for letter in typed {
            let letter: String = letter.to_lowercase().collect();
            let start = state.highlight + 1;
            if let Some(found) = (0..count)
                .map(|step| (start + step) % count)
                .find(|index| options[*index].to_lowercase().starts_with(&letter))
            {
                state.highlight = found;
            }
        }
    }
    if state.highlight != before {
        state.follow = true;
    }
    keys
}

/// Closes the list of `id` when a press lands outside both the field
/// `anchor` and the list `list`. Returns `true` when it did.
fn pressed_outside(ui: &Ui, anchor: Rect, list: Option<Rect>) -> bool {
    ui.input(|input| {
        input.pointer.any_pressed()
            && input.pointer.press_origin().is_some_and(|at| {
                !anchor.contains(at) && list.is_none_or(|list| !list.contains(at))
            })
    })
}

// ----- The dropdown -------------------------------------------------------------

/// Whole steps the wheel turned over the control `id` this frame, down
/// (toward the end of a list) counting up. A trackpad's points are added
/// up between frames, `notch` points a step.
fn wheel_steps(ui: &Ui, id: Id, notch: f32) -> i64 {
    let turned: f32 = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if !(modifiers.ctrl || modifiers.command || modifiers.mac_cmd) => {
                    let along = if delta.y == 0.0 { delta.x } else { delta.y };
                    Some(match unit {
                        MouseWheelUnit::Point => along / notch.max(1.0),
                        MouseWheelUnit::Line | MouseWheelUnit::Page => along,
                    })
                }
                _ => None,
            })
            .sum()
    });
    let key = id.with("sue-wheel");
    let carried: f32 = ui.data(|data| data.get_temp(key)).unwrap_or(0.0);
    let total = carried + turned;
    let whole = total.trunc();
    ui.data_mut(|data| data.insert_temp(key, total - whole));
    // A wheel turned up moves the content down: a step back.
    -(whole as i64)
}

/// `true` once the pointer has rested on the control `id`, which lies at
/// `rect`, long enough for the wheel to be meant for it: [`WHEEL_SETTLE`]
/// seconds after it came, with the control standing still in between. A
/// control that the page scrolls under the pointer moves, so the wait
/// starts again and the page goes on scrolling.
fn wheel_is_ours(ui: &Ui, id: Id, rect: Rect, hovered: bool) -> bool {
    let key = id.with("sue-hover-since");
    let now = ui.input(|input| input.time);
    if !hovered {
        ui.data_mut(|data| data.remove_temp::<Option<(f64, Rect)>>(key));
        return false;
    }
    let since = ui.data_mut(|data| {
        let held = data.get_temp_mut_or(key, None::<(f64, Rect)>);
        match held {
            Some((since, at)) if *at == rect => *since,
            _ => {
                *held = Some((now, rect));
                now
            }
        }
    });
    if now - since < WHEEL_SETTLE {
        // Still settling: ask to be looked at again when it has.
        ui.ctx()
            .request_repaint_after_secs((WHEEL_SETTLE - (now - since)) as f32);
        return false;
    }
    true
}

/// A field that shows one of a list of answers and opens the list when
/// clicked. Made by [`dropdown`].
#[must_use = "a dropdown is drawn by `show`"]
pub struct Dropdown<'a, S> {
    salt: IdSalt,
    chosen: &'a mut usize,
    options: &'a [S],
    label: &'a str,
    enabled: bool,
    width: Option<f32>,
}

/// A dropdown showing `options[*chosen]`. `salt` names it among the
/// controls of its `Ui`.
///
/// A click opens the list; the arrow keys, Home and End step it while it
/// holds focus and is closed, and Enter, Space or Alt and Down open it.
/// The wheel over it steps it too, once the pointer has rested on it for
/// [`WHEEL_SETTLE`] seconds, and the page under it does not scroll then.
/// Each step changes the choice at once, so what it drives previews it.
pub fn dropdown<'a, S: AsRef<str>>(
    salt: impl AsIdSalt,
    chosen: &'a mut usize,
    options: &'a [S],
) -> Dropdown<'a, S> {
    Dropdown {
        salt: IdSalt::new(salt),
        chosen,
        options,
        label: "",
        enabled: true,
        width: None,
    }
}

impl<'a, S: AsRef<str>> Dropdown<'a, S> {
    /// The name assistive technology reads out for it.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide. Without it, it is wide enough for its
    /// widest answer.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Shows it in the layout of `ui`. `changed()` on the response says
    /// the choice changed.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = self.width.unwrap_or_else(|| {
            let options: Vec<&str> = self.options.iter().map(AsRef::as_ref).collect();
            let text = options
                .iter()
                .map(|option| {
                    line(
                        ui.painter(),
                        option,
                        theme.text_font(TextRole::Body),
                        f32::INFINITY,
                    )
                    .size()
                    .x
                })
                .fold(0.0, f32::max);
            (text + side_padding(theme, density) * 2.0 + density.control_height())
                .ceil()
                .min(ui.available_width().max(density.control_height() * 3.0))
        });
        let rect = ui.allocate_space(vec2(width, density.control_height())).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled() && !self.options.is_empty();
        let options: Vec<&str> = self.options.iter().map(AsRef::as_ref).collect();
        let last = options.len().saturating_sub(1);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let mut response = ui
            .interact(hit_rect(rect, density.min_hit()), id, sense)
            .with_new_rect(rect);
        let old = *self.chosen;
        let mut state = list_state(ui.ctx(), id);
        if !enabled {
            state.open = false;
        }
        if enabled && state.open {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                        tab: false,
                    },
                );
            });
        }
        // The wheel, over it while it is closed.
        let ours = enabled && !state.open && wheel_is_ours(ui, id, rect, response.hovered());
        if ours {
            let steps = wheel_steps(ui, id, density.row() * POINTS_PER_STEP);
            if steps != 0 {
                *self.chosen = step_index(*self.chosen, steps, last);
            }
            ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
        }
        // A click of the pointer opens and closes it. Enter and Space
        // count as clicks in egui too, but they are the list's keys here.
        let pointer_click = enabled && response.clicked_by(egui::PointerButton::Primary);
        // The keys, while it holds focus and is closed.
        if enabled && !state.open && response.has_focus() && !pointer_click {
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
            let (open, steps, home, end) = ui.input_mut(|input| {
                let open = input.consume_key(Modifiers::ALT, Key::ArrowDown)
                    || input.consume_key(Modifiers::NONE, Key::Enter)
                    || input.consume_key(Modifiers::NONE, Key::Space);
                let mut press = |key| input.count_and_consume_key(Modifiers::NONE, key) as i64;
                let steps = press(Key::ArrowDown) + press(Key::ArrowRight)
                    - press(Key::ArrowUp)
                    - press(Key::ArrowLeft);
                (open, steps, press(Key::Home) > 0, press(Key::End) > 0)
            });
            if steps != 0 {
                *self.chosen = step_index(*self.chosen, steps, last);
            }
            if home {
                *self.chosen = 0;
            }
            if end {
                *self.chosen = last;
            }
            if open {
                state = ListState {
                    open: true,
                    highlight: (*self.chosen).min(last),
                    follow: true,
                };
            }
        } else if pointer_click {
            // A click opens it, and gives it focus so that the keys walk
            // the list (no ring shows for a click).
            ui.memory_mut(|memory| memory.request_focus(id));
            state = if state.open {
                ListState::default()
            } else {
                ListState {
                    open: true,
                    highlight: (*self.chosen).min(last),
                    follow: true,
                }
            };
        }
        let opacity = motion::fade(ui.ctx(), id.with("sue-list-fade"), state.open);
        if state.open {
            let keys = list_keys(ui, &mut state, &options, true, true);
            let outcome = show_list(
                ui,
                id,
                rect,
                &options,
                Some(*self.chosen),
                &mut state,
                theme,
                density,
                opacity,
            );
            let pick = outcome
                .picked
                .or_else(|| keys.pick.then_some(state.highlight));
            if let Some(index) = pick {
                *self.chosen = index.min(last);
                state.open = false;
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            let focus_left = ui.memory(|memory| {
                memory
                    .focused()
                    .is_some_and(|focused| focused != id && !is_row(id, focused, options.len()))
            });
            if keys.close || focus_left || pressed_outside(ui, response.interact_rect, outcome.rect)
            {
                state.open = false;
            }
            if keys.escape {
                // Escape closes the list, and focus stays on the dropdown.
                ui.memory_mut(|memory| memory.request_focus(id));
            }
        }
        store_list(ui.ctx(), id, state);
        if *self.chosen != old {
            response.mark_changed();
        }
        let look = Look::of(ui, &response, enabled, state.open);
        let shown = options.get(*self.chosen).copied().unwrap_or_default();
        paint_dropdown(
            ui.painter(),
            rect,
            theme,
            density,
            shown,
            look,
            motion::timed(
                ui.ctx(),
                id.with("sue-chevron"),
                state.open,
                motion::Pace::Fast,
            ),
        );
        let (label, shown) = (self.label, shown.to_owned());
        let open = state.open;
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::ComboBox, enabled, label);
            info.current_text_value = Some(shown.clone());
            info.selected = Some(open);
            info
        });
        response
    }
}

/// `true` when `focused` is one of the `count` rows of the list of `id`.
fn is_row(id: Id, focused: Id, count: usize) -> bool {
    (0..count).any(|index| row_id(id, index) == focused)
}

/// `index` moved `steps` along a list whose last index is `last`,
/// stopping at its ends.
fn step_index(index: usize, steps: i64, last: usize) -> usize {
    let moved = index.min(last) as i64 + steps;
    moved.clamp(0, last as i64) as usize
}

/// Paints a dropdown showing `text`, in `look`, its chevron turned
/// `flip` of the way up.
fn paint_dropdown(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    density: Density,
    text: &str,
    look: Look,
    flip: f32,
) {
    painter.add(body_shape(theme, rect, Body::Raised, look));
    let fill = body_fill(theme, Body::Raised, look);
    let padding = side_padding(theme, density);
    let square = trailing_square(rect);
    let room = (square.left() - rect.left() - padding).max(0.0);
    let galley = line(painter, text, theme.text_font(TextRole::Body), room);
    painter.galley(
        pos2(
            rect.left() + padding,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        field_ink(theme, look),
    );
    paint_chevron(
        painter,
        square.center(),
        density.icon() * 0.5,
        flip,
        quiet_ink(theme, look, fill),
    );
    focus::paint_focus_ring(painter, rect, field_radius(theme, rect), look.focus, theme);
}

// ----- The combo ----------------------------------------------------------------

/// A text field that offers a list of answers. Made by [`combo`].
#[must_use = "a combo is drawn by `show`"]
pub struct Combo<'a, S> {
    salt: IdSalt,
    text: &'a mut String,
    options: &'a [S],
    hint: &'a str,
    label: &'a str,
    enabled: bool,
    width: Option<f32>,
}

/// A field that holds `text`, with a button that opens a list of
/// `options`. A pick from the list puts it into the field. `salt` names it
/// among the controls of its `Ui`.
///
/// While the field is typed in, Down or Alt and Down open the list, and
/// while it is open, Up and Down walk it, Enter takes the highlighted
/// answer and Escape closes it. What is typed highlights the first answer
/// that starts with it.
pub fn combo<'a, S: AsRef<str>>(
    salt: impl AsIdSalt,
    text: &'a mut String,
    options: &'a [S],
) -> Combo<'a, S> {
    Combo {
        salt: IdSalt::new(salt),
        text,
        options,
        hint: "",
        label: "",
        enabled: true,
        width: None,
    }
}

impl<'a, S: AsRef<str>> Combo<'a, S> {
    /// Shows `hint` in the field while it is empty.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// The name assistive technology reads out for it.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it `width` wide. Without it, it is as wide as the room it is
    /// shown in.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Shows it in the layout of `ui`. The response is that of the text:
    /// `changed()` when something was typed or picked.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let width = field_width(ui, self.width, density);
        let rect = ui.allocate_space(vec2(width, density.control_height())).1;
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &mut Ui, rect: Rect, theme: &Theme) -> Response {
        let density = Density::of(ui);
        let id = control_id(ui, self.salt);
        let enabled = self.enabled && ui.is_enabled();
        let options: Vec<&str> = self.options.iter().map(AsRef::as_ref).collect();
        let mut state = list_state(ui.ctx(), id);
        if !enabled || options.is_empty() {
            state.open = false;
        }
        let typing = enabled && ui.memory(|memory| memory.has_focus(id));
        // Escape closes an open list, also where egui took focus from the
        // field for it before this frame drew it.
        let escape = state.open && ui.input(|input| input.key_pressed(Key::Escape));
        // The keys the list takes, before the text edit sees them.
        let mut keys = ListKeys::default();
        if typing && state.open {
            keys = list_keys(ui, &mut state, &options, false, false);
        } else if typing && !options.is_empty() {
            let open = ui.input_mut(|input| {
                input.consume_key(Modifiers::ALT, Key::ArrowDown)
                    || input.consume_key(Modifiers::NONE, Key::ArrowDown)
            });
            if open {
                state = ListState {
                    open: true,
                    highlight: first_match(&options, self.text).unwrap_or(0),
                    follow: true,
                };
            }
        }
        let body = ui.painter().add(Shape::Noop);
        let square = trailing_square(rect);
        let (left, right) = text_margins(rect, theme, density, false, square.width());
        let rest = Look {
            enabled,
            ..Look::REST
        };
        let fill = body_fill(theme, Body::Inset, rest);
        let edit = TextEdit::singleline(&mut *self.text)
            .id(id)
            .frame(Frame::NONE.inner_margin(edit_margin(left, right)))
            .font(theme.text_font(TextRole::Body))
            .text_color(field_ink(theme, rest))
            .hint_text(RichText::new(self.hint).color(quiet_ink(theme, rest, fill)))
            .align(Align2::LEFT_CENTER)
            .interactive(enabled)
            .event_filter(combo_keys(state.open))
            .desired_width(rect.width());
        let mut response = ui.put(rect, edit).with_new_rect(rect);
        if response.changed()
            && state.open
            && let Some(found) = first_match(&options, self.text)
        {
            state.highlight = found;
            state.follow = true;
        }
        // The button that opens the list.
        let sense = if enabled && !options.is_empty() {
            Sense::CLICK
        } else {
            Sense::hover()
        };
        let button = ui.interact(square, id.with("sue-open"), sense);
        if button.clicked() {
            state = if state.open {
                ListState::default()
            } else {
                ListState {
                    open: true,
                    highlight: first_match(&options, self.text).unwrap_or(0),
                    follow: true,
                }
            };
            ui.memory_mut(|memory| memory.request_focus(id));
        }
        let opacity = motion::fade(ui.ctx(), id.with("sue-list-fade"), state.open);
        if state.open {
            let outcome = show_list(
                ui,
                id,
                rect,
                &options,
                options
                    .iter()
                    .position(|option| *option == self.text.as_str()),
                &mut state,
                theme,
                density,
                opacity,
            );
            let pick = outcome
                .picked
                .or_else(|| keys.pick.then_some(state.highlight));
            if let Some(index) = pick
                && let Some(option) = options.get(index)
            {
                if self.text.as_str() != *option {
                    *self.text = (*option).to_owned();
                    response.mark_changed();
                }
                state.open = false;
                ui.memory_mut(|memory| memory.request_focus(id));
                let end = self.text.chars().count();
                let mut edit = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                edit.cursor
                    .set_char_range(Some(CCursorRange::one(egui::text::CCursor::new(end))));
                edit.store(ui.ctx(), id);
            }
            let focus_left = ui.memory(|memory| {
                memory
                    .focused()
                    .is_some_and(|focused| focused != id && !is_row(id, focused, options.len()))
            });
            if keys.close || escape || focus_left || pressed_outside(ui, rect, outcome.rect) {
                state.open = false;
            }
            if escape {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
        }
        // Up, Down and Escape are the list's while it is open, from the next
        // frame on too.
        ui.memory_mut(|memory| memory.set_focus_lock_filter(id, combo_keys(state.open)));
        store_list(ui.ctx(), id, state);
        let look = Look {
            focus: 0.0,
            ..Look::of(ui, &response, enabled, response.has_focus() || state.open)
        };
        ui.painter()
            .set(body, body_shape(theme, rect, Body::Inset, look));
        let hover = motion::hover_t(ui.ctx(), button.id, button.hovered());
        paint_combo_button(
            ui.painter(),
            square,
            theme,
            density,
            look,
            hover,
            motion::timed(
                ui.ctx(),
                id.with("sue-chevron"),
                state.open,
                motion::Pace::Fast,
            ),
        );
        let name = tr!("field-choices");
        button.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &name));
        if !self.label.is_empty() {
            let (label, text) = (self.label, self.text.clone());
            response.widget_info(|| {
                let mut info = WidgetInfo::labeled(Role::ComboBox, enabled, label);
                info.current_text_value = Some(text.clone());
                info
            });
        }
        response
    }
}

/// The keys the field of a combo keeps from egui's focus moves: the
/// arrows, and Escape while its list is `open`.
fn combo_keys(open: bool) -> EventFilter {
    EventFilter {
        horizontal_arrows: true,
        vertical_arrows: true,
        tab: false,
        escape: open,
    }
}

/// The first of `options` that starts with `typed`, ignoring case.
fn first_match(options: &[&str], typed: &str) -> Option<usize> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return None;
    }
    options
        .iter()
        .position(|option| option.to_lowercase().starts_with(&typed))
}

/// Paints the button at the end of a combo: a chevron, over a hover fill
/// inside the field's corner.
fn paint_combo_button(
    painter: &Painter,
    square: Rect,
    theme: &Theme,
    density: Density,
    look: Look,
    hover: f32,
    flip: f32,
) {
    let inset = theme.space(Space::S1) + Theme::STRONG_STROKE;
    let inner = square.shrink(inset);
    if hover > 0.0 && look.enabled {
        painter.rect_filled(
            inner,
            super::nested_radius(theme.radius(Radius::Md), inset),
            theme.fill_hover().gamma_multiply(hover),
        );
    }
    let fill = body_fill(theme, Body::Inset, look);
    paint_chevron(
        painter,
        square.center(),
        density.icon() * 0.5,
        flip,
        quiet_ink(theme, look, fill),
    );
}

// ----- The review sheet ---------------------------------------------------------

/// What a review sheet of the inputs shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sheet {
    /// Every input in every state, painted as it would look.
    States,
    /// The open lists, and a number being typed.
    Open,
    /// Every input at rest in the three densities, live.
    Densities,
}

impl Sheet {
    /// Every page.
    pub const ALL: [Self; 3] = [Self::States, Self::Open, Self::Densities];

    /// Its name in file names.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::States => "states",
            Self::Open => "open",
            Self::Densities => "densities",
        }
    }
}

/// The table a review sheet is laid out on: a title, a head per column
/// and a name per row. A sheet is a picture for reviewers and no part of
/// Sue's interface, so what it says is not in the language database.
pub(super) struct Table {
    /// The top left corner of the first cell.
    origin: Pos2,
    /// The size of a cell.
    cell: Vec2,
}

impl Table {
    /// The width of the column that names the rows.
    const NAMES: f32 = 168.0;

    /// The height of the title and the column heads.
    const HEAD: f32 = 84.0;

    /// Paints the ground, the `title` and the heads of `columns`, and
    /// makes room for `rows` rows no higher than `tallest`.
    pub(super) fn new(
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        title: &str,
        columns: &[&str],
        rows: usize,
        tallest: f32,
    ) -> Self {
        let card = rect.shrink(theme.space(Space::S6));
        painter.rect(
            card,
            theme.radius(Radius::Lg),
            theme.surface(1),
            theme.hairline(),
            StrokeKind::Inside,
        );
        let inner = card.shrink(theme.space(Space::S7));
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            title,
            theme.text_font(TextRole::Heading),
            theme.text,
        );
        let origin = pos2(inner.left() + Self::NAMES, inner.top() + Self::HEAD);
        let cell = vec2(
            (inner.width() - Self::NAMES) / columns.len().max(1) as f32,
            ((inner.height() - Self::HEAD) / rows.max(1) as f32).min(tallest),
        );
        for (column, head) in columns.iter().enumerate() {
            painter.text(
                pos2(
                    origin.x + cell.x * column as f32,
                    origin.y - theme.space(Space::S5),
                ),
                Align2::LEFT_BOTTOM,
                head,
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
        }
        Self { origin, cell }
    }

    /// The cell at `row` and `column`.
    pub(super) fn cell(&self, row: usize, column: usize) -> Rect {
        Rect::from_min_size(
            self.origin + vec2(self.cell.x * column as f32, self.cell.y * row as f32),
            self.cell,
        )
    }

    /// The place of something `size` large in the cell at `row` and
    /// `column`: at its left, centered on its height.
    pub(super) fn place(&self, row: usize, column: usize, size: Vec2) -> Rect {
        let cell = self.cell(row, column);
        Rect::from_min_size(pos2(cell.left(), cell.center().y - size.y / 2.0), size)
    }

    /// Writes the name of `row`.
    pub(super) fn name(&self, painter: &Painter, theme: &Theme, row: usize, name: &str) {
        let cell = self.cell(row, 0);
        painter.text(
            pos2(cell.left() - Self::NAMES, cell.center().y),
            Align2::LEFT_CENTER,
            name,
            theme.text_font(TextRole::Caption),
            theme.text_muted(),
        );
    }
}

/// The names of the columns of a states sheet, and the look of each.
pub(super) const STATES: [(&str, Look); 5] = [
    ("Rest", Look::REST),
    ("Hover", Look::HOVER),
    ("Pressed", Look::PRESSED),
    ("Focus or typing", Look::FOCUS),
    ("Disabled", Look::DISABLED),
];

/// What a text-like field of the sheet holds: what is typed, the hint,
/// and whether it is a search field.
struct Sample {
    text: &'static str,
    hint: &'static str,
    search: bool,
}

/// Paints a text field of the sheet in `look`: its body, its text or hint,
/// and, while it is typed in, a caret after the text.
fn paint_text_sample(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    density: Density,
    sample: &Sample,
    look: Look,
    typing: bool,
) {
    let look = Look {
        active: if typing { 1.0 } else { look.active },
        focus: 0.0,
        ..look
    };
    painter.add(body_shape(theme, rect, Body::Inset, look));
    let fill = body_fill(theme, Body::Inset, look);
    let clearable = sample.search && look.enabled && !sample.text.is_empty();
    let trailing = if clearable { rect.height() } else { 0.0 };
    let (left, right) = text_margins(rect, theme, density, sample.search, trailing);
    if sample.search {
        icons::paint(
            painter,
            leading_square(rect, theme, density),
            Icon::Search,
            quiet_ink(theme, look, fill),
        );
    }
    let (shown, ink) = if sample.text.is_empty() {
        (sample.hint, quiet_ink(theme, look, fill))
    } else {
        (sample.text, field_ink(theme, look))
    };
    let galley = line(
        painter,
        shown,
        theme.text_font(TextRole::Body),
        rect.width() - left - right,
    );
    let width = if sample.text.is_empty() {
        0.0
    } else {
        galley.size().x
    };
    painter.galley(
        pos2(rect.left() + left, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    if typing {
        paint_caret(painter, rect.left() + left + width + 1.0, rect, theme);
    }
    if clearable {
        let square = trailing_square(rect);
        paint_clear(
            painter,
            square.shrink(square.height() * 0.2),
            theme,
            look.hover,
            quiet_ink(theme, look, fill),
        );
    }
}

/// Paints a number field of the sheet in `look`. While it is typed in it
/// shows what is typed (all of it chosen) and the unit.
#[allow(clippy::too_many_arguments, reason = "one sample in one state")]
pub(super) fn paint_number_sample(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    density: Density,
    value: f64,
    suffix: &str,
    look: Look,
    typed: Option<&str>,
) {
    let typing = typed.is_some();
    let invalid = typed.is_some_and(|text| parse_number(text, suffix).is_none());
    let look = Look {
        active: if typing { 1.0 } else { look.active },
        focus: 0.0,
        invalid,
        ..look
    };
    painter.add(body_shape(theme, rect, Body::Inset, look));
    let Some(typed) = typed else {
        paint_number_face(
            painter,
            rect,
            theme,
            density,
            &shown_number(value, 0),
            suffix,
            look,
        );
        return;
    };
    let fill = body_fill(theme, Body::Inset, look);
    let padding = side_padding(theme, density);
    let font = theme.text_font(TextRole::Body);
    let mut right = rect.right() - padding;
    if !suffix.is_empty() {
        let unit = line(painter, suffix, font.clone(), f32::INFINITY);
        right -= unit.size().x;
        painter.galley(
            pos2(right, rect.center().y - unit.size().y / 2.0),
            unit,
            quiet_ink(theme, look, fill),
        );
        right -= unit_gap(theme);
    }
    let galley = line(painter, typed, font, f32::INFINITY);
    let text_rect = Rect::from_min_size(
        pos2(
            right - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley.size(),
    );
    if !invalid {
        painter.rect_filled(text_rect, 0.0, theme.selection);
    }
    painter.galley(
        text_rect.min,
        galley,
        if invalid {
            theme.text
        } else {
            theme.selection_text
        },
    );
    if invalid {
        paint_caret(painter, text_rect.right() + 1.0, rect, theme);
        paint_invalid_mark(painter, rect, theme, density);
    }
}

/// Draws the review sheet `page` of the inputs over `rect`, in `theme`,
/// at the scale of the painter.
pub fn sheet(painter: &Painter, rect: Rect, theme: &Theme, page: Sheet) {
    match page {
        Sheet::States => states_sheet(painter, rect, theme),
        Sheet::Open => open_sheet(painter, rect, theme),
        Sheet::Densities => densities_sheet(painter, rect, theme),
    }
}

fn states_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let density = Density::Regular;
    let columns: Vec<&str> = STATES.iter().map(|(name, _)| *name).collect();
    let rows: [&str; 9] = [
        "Text field, empty",
        "Text field",
        "Search field, empty",
        "Search field",
        "Number field",
        "Number field, unit",
        "Number field, typo",
        "Dropdown",
        "Combo",
    ];
    let table = Table::new(
        painter,
        rect,
        theme,
        "Inputs",
        &columns,
        rows.len(),
        density.control_height() + theme.space(Space::S6),
    );
    let size = vec2(
        (table.cell(0, 0).width() - theme.space(Space::S6)).min(220.0),
        density.control_height(),
    );
    for (row, name) in rows.iter().enumerate() {
        table.name(painter, theme, row, name);
        for (column, (_, look)) in STATES.iter().enumerate() {
            let place = table.place(row, column, size);
            let typing = column == 3;
            match row {
                0..=3 => {
                    let sample = Sample {
                        text: if row % 2 == 1 { "Morning light" } else { "" },
                        hint: if row < 2 { "Title" } else { "Search" },
                        search: row >= 2,
                    };
                    paint_text_sample(painter, place, theme, density, &sample, *look, typing);
                }
                4 | 5 => {
                    let (value, suffix) = if row == 4 { (24.0, "") } else { (1200.0, "px") };
                    let typed = typing.then(|| plain_number(value, 0));
                    paint_number_sample(
                        painter,
                        place,
                        theme,
                        density,
                        value,
                        suffix,
                        *look,
                        typed.as_deref(),
                    );
                }
                6 => {
                    let typed = (column != 4).then_some("12x");
                    paint_number_sample(painter, place, theme, density, 12.0, "px", *look, typed);
                }
                7 => {
                    let look = if typing { Look::FOCUS } else { *look };
                    paint_dropdown(painter, place, theme, density, "Multiply", look, 0.0);
                }
                _ => {
                    let sample = Sample {
                        text: "12",
                        hint: "",
                        search: false,
                    };
                    paint_text_sample(painter, place, theme, density, &sample, *look, typing);
                    let square = trailing_square(place);
                    paint_combo_button(
                        painter,
                        square,
                        theme,
                        density,
                        *look,
                        if column == 1 || column == 2 { 1.0 } else { 0.0 },
                        0.0,
                    );
                }
            }
        }
    }
}

/// The blend modes the open sheet lists (Sue's own, in its order): a long
/// list, as dropdowns hold.
const SAMPLE_OPTIONS: [&str; 16] = [
    "Normal",
    "Darken",
    "Multiply",
    "Color Burn",
    "Linear Burn",
    "Subtract",
    "Darker Color",
    "Lighten",
    "Screen",
    "Color Dodge",
    "Add",
    "Lighter Color",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Vivid Light",
];

/// Paints an open list of `options` under `anchor` as it would hang in
/// `screen`, `chosen` checked and `lit` highlighted, scrolled `scroll`
/// rows down. Returns where it lies.
#[allow(clippy::too_many_arguments, reason = "one list and how it lies")]
fn paint_open_list(
    painter: &Painter,
    anchor: Rect,
    screen: Rect,
    options: &[&str],
    chosen: Option<usize>,
    lit: usize,
    scroll: usize,
    theme: &Theme,
    density: Density,
) -> Rect {
    let metrics = ListMetrics::new(theme, density);
    let width = list_width(painter, options, theme, density);
    let place = list_rect(anchor, screen, width, options.len(), &metrics);
    paint_list_ground(painter, place, theme, &metrics);
    let inner = place.shrink(metrics.padding);
    let clipped = painter.with_clip_rect(inner);
    for (index, option) in options.iter().enumerate().skip(scroll) {
        let top = inner.top() + (index - scroll) as f32 * metrics.row;
        if top >= inner.bottom() {
            break;
        }
        let row = Rect::from_min_size(pos2(inner.left(), top), vec2(inner.width(), metrics.row));
        paint_row(
            &clipped,
            row,
            option,
            chosen == Some(index),
            index == lit,
            theme,
            density,
            &metrics,
        );
    }
    place
}

fn open_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let columns = ["Compact", "Regular", "Touch"];
    let table = Table::new(
        painter,
        rect,
        theme,
        "Inputs, open",
        &columns,
        1,
        f32::INFINITY,
    );
    for (column, density) in Density::ALL.into_iter().enumerate() {
        let cell = table.cell(0, column);
        let width = (cell.width() - theme.space(Space::S7)).min(240.0);
        let mut top = cell.top();
        let at = |top: f32, height: f32| {
            Rect::from_min_size(pos2(cell.left(), top), vec2(width, height))
        };
        let height = density.control_height();
        // A dropdown open on its chosen answer, the pointer on another.
        let anchor = at(top, height);
        paint_dropdown(
            painter,
            anchor,
            theme,
            density,
            "Multiply",
            Look::ACTIVE,
            1.0,
        );
        let screen = Rect::from_min_max(anchor.min, pos2(cell.right(), rect.bottom()));
        let options = &SAMPLE_OPTIONS[..6];
        let list = paint_open_list(
            painter,
            anchor,
            screen,
            options,
            Some(2),
            4,
            0,
            theme,
            density,
        );
        top = list.bottom() + theme.space(Space::S7);
        // A combo being typed in, its list on the first match.
        let anchor = at(top, height);
        let sample = Sample {
            text: "1",
            hint: "",
            search: false,
        };
        paint_text_sample(painter, anchor, theme, density, &sample, Look::REST, true);
        paint_combo_button(
            painter,
            trailing_square(anchor),
            theme,
            density,
            Look::ACTIVE,
            0.0,
            1.0,
        );
        let sizes = ["8", "10", "12", "14", "18", "24"];
        let list = paint_open_list(
            painter,
            anchor,
            Rect::from_min_max(anchor.min, pos2(cell.right(), rect.bottom())),
            &sizes,
            None,
            1,
            0,
            theme,
            density,
        );
        top = list.bottom() + theme.space(Space::S7);
        // A long list that scrolls, in a short room.
        let anchor = at(top, height);
        paint_dropdown(
            painter,
            anchor,
            theme,
            density,
            "Overlay",
            Look::ACTIVE,
            1.0,
        );
        let room = Rect::from_min_max(
            anchor.min,
            pos2(cell.right(), anchor.bottom() + 5.5 * density.row()),
        );
        let list = paint_open_list(
            painter,
            anchor,
            room,
            &SAMPLE_OPTIONS,
            Some(12),
            12,
            8,
            theme,
            density,
        );
        top = list.bottom() + theme.space(Space::S7);
        // A number being typed: all of it chosen, then a typo.
        let field = at(top, height);
        top = field.bottom() + theme.space(Space::S6);
        let field = Rect::from_min_size(field.min, vec2(width.min(120.0), height));
        paint_number_sample(
            painter,
            field,
            theme,
            density,
            35.0,
            "%",
            Look::REST,
            Some(&plain_number(35.0, 0)),
        );
        let field = at(top, height);
        top = field.bottom() + theme.space(Space::S6);
        let field = Rect::from_min_size(field.min, vec2(width.min(120.0), height));
        paint_number_sample(
            painter,
            field,
            theme,
            density,
            35.0,
            "%",
            Look::REST,
            Some("3,5,"),
        );
    }
}

/// The live column of one density: one of each input, as a studio draws
/// them.
fn live_column(ui: &mut Ui, theme: &Theme) {
    let width = ui.available_width();
    let mut empty = String::new();
    let mut title = "Morning light".to_owned();
    let mut search = "lin".to_owned();
    let mut number = 24.0;
    let mut size = 1200.0;
    let mut chosen = 2;
    let mut points = "12".to_owned();
    text_field("empty", &mut empty)
        .hint("Title")
        .width(width)
        .show(ui, theme);
    text_field("title", &mut title).width(width).show(ui, theme);
    search_field("search", &mut search)
        .hint("Search")
        .width(width)
        .show(ui, theme);
    ui.horizontal(|ui| {
        number_field("number", &mut number).show(ui, theme);
        number_field("size", &mut size)
            .suffix("px")
            .range(1.0..=4000.0)
            .show(ui, theme);
    });
    dropdown("blend", &mut chosen, &SAMPLE_OPTIONS).show(ui, theme);
    combo("points", &mut points, &["8", "10", "12", "14"])
        .width(width.min(140.0))
        .show(ui, theme);
    text_field("disabled", &mut title)
        .width(width)
        .enabled(false)
        .show(ui, theme);
}

fn densities_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let columns = ["Compact", "Regular", "Touch"];
    let table = Table::new(
        painter,
        rect,
        theme,
        "Inputs, live",
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
            Id::unique(("sue-inputs-sheet", column)),
            UiBuilder::new().max_rect(room).layer_id(painter.layer_id()),
        );
        ui.spacing_mut().item_spacing = Vec2::splat(theme.space(Space::S5));
        density.scope(&mut ui, |ui| live_column(ui, theme));
    }
}

#[cfg(test)]
pub(super) mod tests;
