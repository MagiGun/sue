//! Choices: the controls that hold a setting.
//!
//! - [`checkbox`]: one thing that is on or off, or partly on
//!   ([`Check::Mixed`]) when it stands for a set of which some are.
//! - [`radio`]: one of a few answers of which exactly one holds. A group
//!   of them is walked with the arrow keys through [`rove`].
//! - [`switch`]: something that takes effect at once when flipped.
//! - [`segmented`]: a few answers side by side, of which one holds, in
//!   one frame.
//! - [`chip`]: a small label that filters or tags, lit while on, with a
//!   cross to take it away where that makes sense.
//!
//! Like the buttons of [`super::button`], each reads its sizes from the
//! [`Density`] of its `Ui`, its colors from the [`Theme`], draws the
//! focus ring, tells assistive technology what it is and whether it is
//! on, moves through [`motion`] and is named by a salt of its caller.
//!
//! None of them changes anything: a click is an answer in the
//! [`Response`], and the caller sets the value. The control then follows
//! the value it is shown with, wherever that changed.
//!
//! On is told by shape as much as by shade, because the shipped themes
//! are monochrome: a box is solid and holds a mark, a switch is solid
//! with its knob at the far end, a chip and a segment carry the outline
//! of what is chosen.
//!
//! [`sheet`] draws all of them, in every state, for review.

use egui::{
    AsIdSalt, Color32, CornerRadius, EventFilter, IdSalt, Key, Modifiers, Painter, Pos2, Rect,
    Response, Role, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use super::button::{
    Sheet, Table, Visual, chosen_fill, control_id, density_columns, disabled_ink, edge, ink_on,
    interact, line, mix, paint_inner_ring, paint_line, place,
};
use super::{Density, Radius, Roving, Space, TextRole, focus, motion, nested_radius};
use crate::Theme;
use crate::icons::{self, Icon};

/// The width of the outline of an empty box, ring or track.
const OUTLINE: f32 = 1.5;

/// How far a solid mark moves toward its ink under the pointer, and
/// while pressed.
const SOLID_HOVER: f32 = 0.13;
const SOLID_PRESS: f32 = 0.26;

/// The state of a [`checkbox`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Check {
    /// Not chosen.
    #[default]
    Off,
    /// Chosen.
    On,
    /// It stands for a set of which some are chosen and some are not.
    Mixed,
}

impl From<bool> for Check {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// The colors a box, a ring or a track is drawn in.
struct MarkColors {
    /// The outline while empty.
    edge: Color32,
    /// The fill while empty.
    under: Color32,
    /// The fill while on.
    solid: Color32,
    /// What is drawn on the solid fill.
    inverse: Color32,
}

fn mark_colors(theme: &Theme, visual: Visual) -> MarkColors {
    if !visual.enabled {
        let ink = disabled_ink(theme);
        return MarkColors {
            edge: ink,
            under: Color32::TRANSPARENT,
            solid: ink,
            inverse: theme.surface(1),
        };
    }
    let solid = theme.text;
    let inverse = ink_on(solid, theme);
    MarkColors {
        edge: mix(theme.border_strong(), theme.text, visual.hover),
        under: if visual.pressed {
            theme.fill_pressed()
        } else {
            theme.fill_hover().gamma_multiply(visual.hover)
        },
        solid: mix(
            solid,
            inverse,
            if visual.pressed {
                SOLID_PRESS
            } else {
                SOLID_HOVER * visual.hover
            },
        ),
        inverse,
    }
}

/// The first `share` of the way along `points`.
fn trimmed(points: &[Pos2], share: f32) -> Vec<Pos2> {
    let whole: f32 = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    let mut left = whole * share.clamp(0.0, 1.0);
    let mut drawn = vec![points[0]];
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if length <= left {
            drawn.push(pair[1]);
            left -= length;
        } else {
            if length > 0.0 {
                drawn.push(pair[0] + (pair[1] - pair[0]) * (left / length));
            }
            break;
        }
    }
    drawn
}

/// Paints the box of a checkbox in `mark`. `on` runs from 0 (empty) to 1
/// (solid, with its mark drawn).
fn paint_box(painter: &Painter, mark: Rect, state: Check, theme: &Theme, visual: Visual, on: f32) {
    let side = mark.width();
    let radius = theme.radius(Radius::Sm).min(side * 0.3);
    let colors = mark_colors(theme, visual);
    if on < 1.0 {
        painter.rect(
            mark,
            radius,
            colors.under,
            Stroke::new(OUTLINE, colors.edge),
            StrokeKind::Inside,
        );
    }
    if on > 0.0 {
        painter.rect_filled(mark, radius, colors.solid.gamma_multiply(on));
        let at = |x: f32, y: f32| mark.min + vec2(x, y) * side;
        let stroke = Stroke::new((side * 0.11).max(OUTLINE), colors.inverse);
        let stroke_points = match state {
            Check::Mixed => vec![at(0.27, 0.5), at(0.73, 0.5)],
            _ => vec![at(0.24, 0.53), at(0.43, 0.71), at(0.77, 0.31)],
        };
        let drawn = trimmed(&stroke_points, on);
        if drawn.len() > 1 {
            painter.line(drawn, stroke);
        }
    }
    focus::paint_focus_ring(painter, mark, radius, visual.focus, theme);
}

/// Paints the ring of a radio button in `mark`.
fn paint_ring(painter: &Painter, mark: Rect, theme: &Theme, visual: Visual, on: f32) {
    let radius = mark.width() / 2.0;
    let center = mark.center();
    let colors = mark_colors(theme, visual);
    if on < 1.0 {
        painter.circle(
            center,
            radius - OUTLINE / 2.0,
            colors.under,
            Stroke::new(OUTLINE, colors.edge),
        );
    }
    if on > 0.0 {
        painter.circle_filled(center, radius, colors.solid.gamma_multiply(on));
        painter.circle_filled(center, mark.width() * 0.2 * on, colors.inverse);
    }
    focus::paint_focus_ring(painter, mark, radius, visual.focus, theme);
}

/// Paints the track and the knob of a switch in `mark`.
fn paint_track(painter: &Painter, mark: Rect, theme: &Theme, visual: Visual, on: f32) {
    let radius = mark.height() / 2.0;
    let colors = mark_colors(theme, visual);
    if on < 1.0 {
        painter.rect(
            mark,
            radius,
            colors.under,
            Stroke::new(OUTLINE, colors.edge),
            StrokeKind::Inside,
        );
    }
    if on > 0.0 {
        painter.rect_filled(mark, radius, colors.solid.gamma_multiply(on));
    }
    let knob = radius - (mark.height() * 0.16).max(2.5);
    // Held down, the knob reaches for the other end.
    let reach = if visual.pressed && visual.enabled {
        knob * 0.6
    } else {
        0.0
    };
    let center = egui::lerp((mark.left() + radius)..=(mark.right() - radius), on);
    let (left, right) = if on < 0.5 {
        (center - knob, center + knob + reach)
    } else {
        (center - knob - reach, center + knob)
    };
    let resting = if visual.enabled {
        mix(theme.text_muted(), theme.text, visual.hover)
    } else {
        colors.edge
    };
    painter.rect_filled(
        Rect::from_min_max(
            pos2(left, mark.center().y - knob),
            pos2(right, mark.center().y + knob),
        ),
        knob,
        mix(resting, colors.inverse, on),
    );
    focus::paint_focus_ring(painter, mark, radius, visual.focus, theme);
}

/// The size of the box of a checkbox and of the ring of a radio button.
fn box_size(density: Density) -> Vec2 {
    Vec2::splat(density.icon())
}

/// The size of the track of a switch.
fn track_size(density: Density) -> Vec2 {
    let height: f32 = match density {
        Density::Compact => 16.0,
        Density::Regular => 20.0,
        Density::Touch => 28.0,
    };
    vec2((height * 1.8).round(), height)
}

/// Which mark a [`Marked`] control draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Box(Check),
    Ring(bool),
    Track(bool),
}

impl Mark {
    fn is_on(self) -> bool {
        !matches!(
            self,
            Self::Box(Check::Off) | Self::Ring(false) | Self::Track(false)
        )
    }

    fn size(self, density: Density) -> Vec2 {
        match self {
            Self::Box(_) | Self::Ring(_) => box_size(density),
            Self::Track(_) => track_size(density),
        }
    }

    fn info(self, enabled: bool, label: &str) -> WidgetInfo {
        match self {
            // A box that is neither on nor off says so by saying neither.
            Self::Box(Check::Mixed) => WidgetInfo::labeled(Role::CheckBox, enabled, label),
            Self::Box(state) => {
                WidgetInfo::selected(Role::CheckBox, enabled, state == Check::On, label)
            }
            Self::Ring(on) => WidgetInfo::selected(Role::RadioButton, enabled, on, label),
            Self::Track(on) => WidgetInfo::selected(Role::Switch, enabled, on, label),
        }
    }
}

/// A mark with its label after it: a checkbox, a radio button or a
/// switch. Made by [`checkbox`], [`radio`] and [`switch`].
#[derive(Clone, Debug)]
#[must_use = "a control is drawn by `show`"]
pub struct Marked<'a> {
    salt: IdSalt,
    label: &'a str,
    mark: Mark,
    enabled: bool,
    bare: bool,
}

/// A checkbox that says `label`. `state` is a `bool` or a [`Check`]. A
/// click asks to flip it; the caller does the flipping.
pub fn checkbox(salt: impl AsIdSalt, label: &str, state: impl Into<Check>) -> Marked<'_> {
    marked(salt, label, Mark::Box(state.into()))
}

/// A radio button that says `label`, chosen while `on`. A click asks to
/// choose it. The buttons of one question are a group: hand their
/// responses to [`rove`] so the arrow keys walk them.
pub fn radio(salt: impl AsIdSalt, label: &str, on: bool) -> Marked<'_> {
    marked(salt, label, Mark::Ring(on))
}

/// A switch that says `label`. A click asks to flip it; what it stands
/// for takes effect at once, with no button to confirm.
pub fn switch(salt: impl AsIdSalt, label: &str, on: bool) -> Marked<'_> {
    marked(salt, label, Mark::Track(on))
}

fn marked(salt: impl AsIdSalt, label: &str, mark: Mark) -> Marked<'_> {
    Marked {
        salt: IdSalt::new(salt),
        label,
        mark,
        enabled: true,
        bare: false,
    }
}

impl Marked<'_> {
    /// Whether it can be changed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the mark alone, where a row names it already. The label is
    /// still what assistive technology reads.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// The size it takes.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme, density: Density) -> Vec2 {
        let mark = self.mark.size(density);
        if self.bare || self.label.is_empty() {
            return mark;
        }
        let text = line(
            painter,
            self.label,
            theme.text_font(TextRole::Body),
            f32::INFINITY,
        );
        vec2(
            (mark.x + theme.space(Space::S4) + text.size().x).ceil(),
            density.row().max(mark.y),
        )
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let rect = place(ui, self.size(ui.painter(), theme, Density::of(ui)));
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let response = interact(ui, rect, control_id(ui, self.salt), enabled);
        // A box fills at the fast pace; a knob travels at the base pace.
        let pace = match self.mark {
            Mark::Track(_) => motion::Pace::Base,
            _ => motion::Pace::Fast,
        };
        let on = motion::timed(
            ui.ctx(),
            response.id.with("sue-on"),
            self.mark.is_on(),
            pace,
        );
        if ui.is_rect_visible(rect) {
            let visual = Visual::of(ui, &response, enabled);
            self.paint(ui.painter(), rect, theme, Density::of(ui), visual, on);
        }
        let (mark, label) = (self.mark, self.label);
        response.widget_info(|| mark.info(enabled, label));
        response
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visual: Visual,
        on: f32,
    ) {
        let size = self.mark.size(density);
        let mark = Rect::from_min_size(pos2(rect.left(), rect.center().y - size.y / 2.0), size);
        match self.mark {
            Mark::Box(state) => paint_box(painter, mark, state, theme, visual, on),
            Mark::Ring(_) => paint_ring(painter, mark, theme, visual, on),
            Mark::Track(_) => paint_track(painter, mark, theme, visual, on),
        }
        if self.bare || self.label.is_empty() {
            return;
        }
        let left = mark.right() + theme.space(Space::S4);
        let text = line(
            painter,
            self.label,
            theme.text_font(TextRole::Body),
            rect.right() - left,
        );
        let ink = if visual.enabled {
            theme.text
        } else {
            disabled_ink(theme)
        };
        paint_line(painter, left, rect.center().y, text, ink);
    }
}

/// Lets the arrow keys of `axis` walk a group of controls, as the
/// buttons of one radio question or the cells of a strip want: from the
/// member that holds focus, an arrow moves focus to the next or the
/// previous one that can be used, wrapping, and Home and End to the
/// first and the last. Call it after every member was shown, with their
/// responses in reading order.
///
/// Returns the member focus moved to. In a radio group that member is
/// also the new answer: choosing follows focus.
pub fn rove(ui: &Ui, members: &[Response], axis: Roving) -> Option<usize> {
    let index = members.iter().position(Response::has_focus)?;
    let (across, along) = (
        matches!(axis, Roving::Horizontal | Roving::Both),
        matches!(axis, Roving::Vertical | Roving::Both),
    );
    // The arrows are the group's: egui does not also move focus to
    // whatever lies in that direction.
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            members[index].id,
            EventFilter {
                horizontal_arrows: across,
                vertical_arrows: along,
                ..Default::default()
            },
        );
    });
    let pressed = |key: Key| ui.input_mut(|input| input.consume_key(Modifiers::NONE, key));
    let count = members.len();
    // A member that cannot be used was shown sensing no clicks (see
    // `interact`), and takes no focus.
    let usable = |at: usize| members[at].enabled() && members[at].sense.is_focusable();
    let step = |from: usize, forward: bool| {
        (1..=count)
            .map(|by| {
                if forward {
                    (from + by) % count
                } else {
                    (from + count - by) % count
                }
            })
            .find(|at| usable(*at))
            .unwrap_or(from)
    };
    let mut target = index;
    if (across && pressed(Key::ArrowRight)) || (along && pressed(Key::ArrowDown)) {
        target = step(target, true);
    }
    if (across && pressed(Key::ArrowLeft)) || (along && pressed(Key::ArrowUp)) {
        target = step(target, false);
    }
    if pressed(Key::Home) {
        target = (0..count).find(|at| usable(*at)).unwrap_or(index);
    }
    if pressed(Key::End) {
        target = (0..count).rev().find(|at| usable(*at)).unwrap_or(index);
    }
    if target == index {
        return None;
    }
    members[target].request_focus();
    Some(target)
}

/// A few answers side by side in one frame. Made by [`segmented`].
#[derive(Clone, Debug)]
#[must_use = "a control is drawn by `show`"]
pub struct Segmented<'a, T> {
    salt: IdSalt,
    options: &'a [T],
    icons: &'a [Icon],
    chosen: Option<usize>,
    enabled: bool,
    full_width: bool,
    icons_only: bool,
}

/// What a [`Segmented`] answers with.
#[derive(Clone, Debug)]
pub struct SegmentedResponse {
    /// The whole strip.
    pub response: Response,
    /// The answer asked for in this frame, by a click or an arrow key.
    pub picked: Option<usize>,
}

/// A strip of `options` of which `chosen` holds. The arrow keys walk
/// it, and the answer follows them.
pub fn segmented<T: AsRef<str>>(
    salt: impl AsIdSalt,
    options: &[T],
    chosen: Option<usize>,
) -> Segmented<'_, T> {
    Segmented {
        salt: IdSalt::new(salt),
        options,
        icons: &[],
        chosen,
        enabled: true,
        full_width: false,
        icons_only: false,
    }
}

impl<'a, T: AsRef<str>> Segmented<'a, T> {
    /// Gives the options icons, in their order.
    pub fn icons(mut self, icons: &'a [Icon]) -> Self {
        self.icons = icons;
        self
    }

    /// Shows only the icons. The options' names are still read out, and
    /// shown when the pointer rests on one.
    pub fn icons_only(mut self) -> Self {
        self.icons_only = true;
        self
    }

    /// Whether the answer can be changed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it as wide as the room it is shown in, with equal cells.
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    fn label(&self, index: usize) -> &str {
        if self.icons_only && self.icons.get(index).is_some() {
            ""
        } else {
            self.options[index].as_ref()
        }
    }

    /// The space between the frame and the cells, and between cells.
    fn inset(theme: &Theme) -> f32 {
        theme.space(Space::S1)
    }

    fn cell_padding(theme: &Theme, density: Density) -> f32 {
        theme.space(match density {
            Density::Compact => Space::S4,
            Density::Regular => Space::S5,
            Density::Touch => Space::S6,
        })
    }

    /// The width a cell takes when nothing stretches it.
    fn cell_width(&self, index: usize, painter: &Painter, theme: &Theme, density: Density) -> f32 {
        let label = self.label(index);
        let mut width = Self::cell_padding(theme, density) * 2.0;
        if self.icons.get(index).is_some() {
            width += density.icon();
            if !label.is_empty() {
                width += theme.space(Space::S3);
            }
        }
        if !label.is_empty() {
            width += line(
                painter,
                label,
                theme.text_font(TextRole::Body),
                f32::INFINITY,
            )
            .size()
            .x;
        }
        width.ceil()
    }

    /// The size it takes when nothing stretches it.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme, density: Density) -> Vec2 {
        let count = self.options.len();
        let cells: f32 = (0..count)
            .map(|index| self.cell_width(index, painter, theme, density))
            .sum();
        let inset = Self::inset(theme);
        vec2(
            cells + inset * (count.saturating_sub(1) as f32 + 2.0),
            density.control_height(),
        )
    }

    /// The cells inside `rect`. Room beyond what the cells need is
    /// shared out evenly; with [`full_width`](Self::full_width) all
    /// cells are equally wide.
    fn cells(&self, rect: Rect, painter: &Painter, theme: &Theme, density: Density) -> Vec<Rect> {
        let count = self.options.len();
        if count == 0 {
            return Vec::new();
        }
        let inset = Self::inset(theme);
        let room = (rect.width() - inset * (count as f32 + 1.0)).max(0.0);
        let natural: Vec<f32> = (0..count)
            .map(|index| self.cell_width(index, painter, theme, density))
            .collect();
        let needed: f32 = natural.iter().sum();
        let widths: Vec<f32> = if self.full_width || needed <= 0.0 {
            vec![room / count as f32; count]
        } else if room >= needed {
            let spare = (room - needed) / count as f32;
            natural.iter().map(|width| width + spare).collect()
        } else {
            natural.iter().map(|width| width * room / needed).collect()
        };
        let mut left = rect.left() + inset;
        widths
            .into_iter()
            .map(|width| {
                let cell = Rect::from_min_max(
                    pos2(left, rect.top() + inset),
                    pos2(left + width, rect.bottom() - inset),
                );
                left += width + inset;
                cell
            })
            .collect()
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> SegmentedResponse {
        let mut size = self.size(ui.painter(), theme, Density::of(ui));
        let room = ui.available_width();
        if self.full_width && room.is_finite() {
            size.x = size.x.max(room);
        }
        let rect = place(ui, size);
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> SegmentedResponse {
        let density = Density::of(ui);
        let enabled = self.enabled && ui.is_enabled();
        let id = control_id(ui, self.salt);
        let response = ui.interact(rect, id, egui::Sense::hover());
        response.widget_info(|| WidgetInfo::new(Role::RadioGroup));
        let cells = self.cells(rect, ui.painter(), theme, density);
        let mut picked = None;
        let members: Vec<Response> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let member = interact(ui, *cell, id.with(index), enabled);
                let (name, on) = (self.options[index].as_ref(), self.chosen == Some(index));
                member.widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, on, name));
                if member.clicked() {
                    picked = Some(index);
                }
                if self.label(index).is_empty() && !name.is_empty() {
                    member.on_hover_text(name)
                } else {
                    member
                }
            })
            .collect();
        if let Some(index) = rove(ui, &members, Roving::Horizontal) {
            picked = Some(index);
        }
        if ui.is_rect_visible(rect) {
            // The thumb slides to the chosen cell on the spring. It is
            // kept as an offset from the strip, so that scrolling the
            // strip moves nothing.
            let thumb = self.chosen.and_then(|index| cells.get(index)).map(|cell| {
                let left = motion::spring(
                    ui.ctx(),
                    id.with("sue-thumb-left"),
                    cell.left() - rect.left(),
                );
                let width = motion::spring(ui.ctx(), id.with("sue-thumb-width"), cell.width());
                Rect::from_min_size(
                    pos2(rect.left() + left, cell.top()),
                    vec2(width, cell.height()),
                )
            });
            let visuals: Vec<Visual> = members
                .iter()
                .map(|member| Visual::of(ui, member, enabled))
                .collect();
            self.paint(ui.painter(), rect, theme, density, &visuals, thumb);
        }
        SegmentedResponse { response, picked }
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visuals: &[Visual],
        thumb: Option<Rect>,
    ) {
        let enabled = visuals.first().is_none_or(|visual| visual.enabled);
        let radius = theme.radius(Radius::Md).min(rect.height() / 2.0);
        let inner = nested_radius(radius, Self::inset(theme));
        let track = theme.surface(0);
        painter.rect(
            rect,
            radius,
            track,
            Stroke::new(theme.border_width, edge(theme)),
            StrokeKind::Inside,
        );
        if let Some(thumb) = thumb {
            let (fill, stroke) = if enabled {
                (theme.fill_selected(), theme.hairline_active())
            } else {
                (
                    mix(track, theme.fill_selected(), 0.5),
                    Stroke::new(theme.border_width, disabled_ink(theme)),
                )
            };
            painter.rect(thumb, inner, fill, stroke, StrokeKind::Inside);
        }
        let cells = self.cells(rect, painter, theme, density);
        for (index, (cell, visual)) in cells.iter().zip(visuals).enumerate() {
            let on = self.chosen == Some(index);
            if visual.enabled && !on {
                let fill = if visual.pressed {
                    theme.fill_pressed()
                } else {
                    theme.fill_hover().gamma_multiply(visual.hover)
                };
                painter.rect_filled(*cell, inner, fill);
            }
            let ink = if !visual.enabled {
                disabled_ink(theme)
            } else if on {
                theme.text
            } else {
                mix(theme.text_muted_on(track), theme.text, visual.hover)
            };
            let label = self.label(index);
            let icon = self.icons.get(index).copied();
            let side = density.icon();
            let gap = if icon.is_some() && !label.is_empty() {
                theme.space(Space::S3)
            } else {
                0.0
            };
            let lead = if icon.is_some() { side + gap } else { 0.0 };
            let room = cell.width() - Self::cell_padding(theme, density) - lead;
            let text = line(painter, label, theme.text_font(TextRole::Body), room);
            let width = if label.is_empty() { 0.0 } else { text.size().x };
            let mut left = cell.center().x - (lead + width) / 2.0;
            if let Some(icon) = icon {
                icons::paint(
                    painter,
                    Rect::from_center_size(
                        pos2(left + side / 2.0, cell.center().y),
                        Vec2::splat(side),
                    ),
                    icon,
                    ink,
                );
                left += lead;
            }
            if !label.is_empty() {
                paint_line(painter, left, cell.center().y, text, ink);
            }
            let corner = CornerRadius::same(inner.round().clamp(0.0, 255.0) as u8);
            paint_inner_ring(painter, *cell, corner, theme.focus(), visual.focus, theme);
        }
    }
}

/// A small label that filters or tags. Made by [`chip`].
#[derive(Clone, Debug)]
#[must_use = "a chip is drawn by `show`"]
pub struct Chip<'a> {
    salt: IdSalt,
    label: &'a str,
    on: bool,
    icon: Option<Icon>,
    remove: Option<&'a str>,
    enabled: bool,
    squared: bool,
}

/// What a [`Chip`] answers with.
#[derive(Clone, Debug)]
pub struct ChipResponse {
    /// The chip: `clicked()` asks to flip it.
    pub response: Response,
    /// Its cross was clicked, or Delete or Backspace was pressed on it:
    /// take it away.
    pub removed: bool,
}

/// A chip that says `label`, lit while `on`. A click asks to flip it.
pub fn chip(salt: impl AsIdSalt, label: &str, on: bool) -> Chip<'_> {
    Chip {
        salt: IdSalt::new(salt),
        label,
        on,
        icon: None,
        remove: None,
        enabled: true,
        squared: false,
    }
}

impl<'a> Chip<'a> {
    /// Puts `icon` before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Gives it a cross that takes it away. `name` says so ("Remove the
    /// tag"): it is read out and shown on hover.
    pub fn removable(mut self, name: &'a str) -> Self {
        self.remove = Some(name);
        self
    }

    /// Whether it can be used.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Gives it the small corners of a chip inside a strip, where a row
    /// of pills would not sit flush.
    pub fn squared(mut self) -> Self {
        self.squared = true;
        self
    }

    fn height(density: Density) -> f32 {
        match density {
            Density::Compact => 22.0,
            Density::Regular => 26.0,
            Density::Touch => 34.0,
        }
    }

    fn role(density: Density) -> TextRole {
        match density {
            Density::Compact => TextRole::Caption,
            Density::Regular | Density::Touch => TextRole::Body,
        }
    }

    fn icon_side(density: Density) -> f32 {
        (Self::height(density) * 0.6).round()
    }

    /// The width its icon takes, with the gap after it.
    fn lead_width(&self, theme: &Theme, density: Density) -> f32 {
        if self.icon.is_some() {
            Self::icon_side(density) + theme.space(Space::S2)
        } else {
            0.0
        }
    }

    /// The size it takes.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme, density: Density) -> Vec2 {
        let height = Self::height(density);
        let padding = (height * 0.45).round();
        let text = line(
            painter,
            self.label,
            theme.text_font(Self::role(density)),
            f32::INFINITY,
        );
        let tail = if self.remove.is_some() {
            height
        } else {
            padding
        };
        vec2(
            (padding + self.lead_width(theme, density) + text.size().x + tail).ceil(),
            height,
        )
    }

    /// The square its cross lies in, at the end of `rect`.
    fn cross(rect: Rect) -> Rect {
        Rect::from_min_max(pos2(rect.right() - rect.height(), rect.top()), rect.max)
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> ChipResponse {
        let rect = place(ui, self.size(ui.painter(), theme, Density::of(ui)));
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> ChipResponse {
        let enabled = self.enabled && ui.is_enabled();
        let id = control_id(ui, self.salt);
        let response = interact(ui, rect, id, enabled);
        let (label, on) = (self.label, self.on);
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, on, label));
        // The cross is sensed after the chip, so it lies above it.
        let cross = self.remove.map(|name| {
            let cross = ui.interact(
                Self::cross(rect),
                id.with("sue-chip-remove"),
                if enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            cross.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
            if name.is_empty() {
                cross
            } else {
                cross.on_hover_text(name)
            }
        });
        let by_key = enabled
            && self.remove.is_some()
            && response.has_focus()
            && ui.input_mut(|input| {
                input.consume_key(Modifiers::NONE, Key::Delete)
                    || input.consume_key(Modifiers::NONE, Key::Backspace)
            });
        if ui.is_rect_visible(rect) {
            let visual = Visual::of(ui, &response, enabled);
            let cross_visual = cross.as_ref().map(|cross| Visual::of(ui, cross, enabled));
            self.paint(
                ui.painter(),
                rect,
                theme,
                Density::of(ui),
                visual,
                cross_visual,
            );
        }
        ChipResponse {
            removed: by_key || cross.is_some_and(|cross| cross.clicked()),
            response,
        }
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visual: Visual,
        cross: Option<Visual>,
    ) {
        let radius = if self.squared {
            theme.radius(Radius::Sm)
        } else {
            theme.radius(Radius::Full)
        }
        .min(rect.height() / 2.0);
        let (fill, stroke, ink) = self.skin(theme, visual);
        painter.rect(rect, radius, fill, stroke, StrokeKind::Inside);
        self.paint_inside(painter, rect, theme, density, ink, cross);
        focus::paint_focus_ring(painter, rect, radius, visual.focus, theme);
    }

    /// Its fill and its text color in the state `visual`.
    #[cfg(test)]
    fn colors(&self, theme: &Theme, visual: Visual) -> (Color32, Color32) {
        let (fill, _, ink) = self.skin(theme, visual);
        (fill, ink)
    }

    /// Its fill, its outline and its text color in the state `visual`.
    fn skin(&self, theme: &Theme, visual: Visual) -> (Color32, Stroke, Color32) {
        let hairline = theme.border_width;
        if !visual.enabled {
            let ink = disabled_ink(theme);
            if self.on {
                let fill = mix(theme.surface(1), theme.fill_selected(), 0.5);
                (fill, Stroke::new(hairline, ink), theme.text_disabled(fill))
            } else {
                (
                    Color32::TRANSPARENT,
                    Stroke::new(hairline, theme.border),
                    ink,
                )
            }
        } else if self.on {
            (
                chosen_fill(theme, visual),
                theme.hairline_active(),
                theme.text,
            )
        } else {
            let fill = if visual.pressed {
                theme.fill_pressed()
            } else {
                mix(theme.surface(2), theme.fill_hover(), visual.hover)
            };
            (
                fill,
                Stroke::new(
                    hairline,
                    mix(edge(theme), theme.border_strong(), visual.hover),
                ),
                mix(theme.text_muted_on(fill), theme.text, visual.hover),
            )
        }
    }

    /// Paints its icon, its label and its cross over its body in `ink`.
    fn paint_inside(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        ink: Color32,
        cross: Option<Visual>,
    ) {
        let height = Self::height(density);
        let padding = (height * 0.45).round();
        let tail = if self.remove.is_some() {
            rect.height()
        } else {
            padding
        };
        let mut left = rect.left() + padding;
        if let Some(icon) = self.icon {
            let side = Self::icon_side(density);
            icons::paint(
                painter,
                Rect::from_center_size(pos2(left + side / 2.0, rect.center().y), Vec2::splat(side)),
                icon,
                ink,
            );
            left += self.lead_width(theme, density);
        }
        let text = line(
            painter,
            self.label,
            theme.text_font(Self::role(density)),
            rect.right() - tail - left,
        );
        paint_line(painter, left, rect.center().y, text, ink);
        if let Some(cross) = cross {
            let place = Self::cross(rect);
            let center = place.center();
            if cross.enabled {
                let under = if cross.pressed {
                    theme.fill_pressed()
                } else {
                    theme.fill_hover().gamma_multiply(cross.hover)
                };
                painter.circle_filled(center, place.height() * 0.36, mix(under, ink, 0.12));
            }
            let arm = (place.height() * 0.14).max(3.0);
            let pen = Stroke::new(icons::weight(arm * 4.0), ink);
            for turn in [1.0, -1.0] {
                painter.line_segment(
                    [
                        pos2(center.x - arm, center.y - arm * turn),
                        pos2(center.x + arm, center.y + arm * turn),
                    ],
                    pen,
                );
            }
            focus::paint_focus_ring(
                painter,
                Rect::from_center_size(center, Vec2::splat(place.height() * 0.72)),
                place.height(),
                cross.focus,
                theme,
            );
        }
    }
}

/// Draws the review sheet `page` of the choices over `rect`: what a
/// reviewer looks at to judge every one of them in every state. It is
/// drawn in `theme`, at the scale of the painter.
pub fn sheet(painter: &Painter, rect: Rect, theme: &Theme, page: Sheet) {
    match page {
        Sheet::States => states_sheet(painter, rect, theme),
        Sheet::Densities => {
            density_columns(painter, rect, theme, "Choices", |ui, _| {
                live_column(ui, theme);
            });
        }
    }
}

/// The names of the views a sample strip switches between.
const VIEWS: [&str; 3] = ["Pages", "Corkboard", "Outline"];

/// The icons of a sample strip of icons, and their names.
const MARKS: [Icon; 3] = [Icon::Star, Icon::Heart, Icon::Eye];
const MARK_NAMES: [&str; 3] = ["Favorites", "Liked", "Shown"];

/// What one row of the states sheet shows.
enum Sample {
    Marked(Marked<'static>),
    Chip(Chip<'static>),
    Views,
    Marks,
}

fn states_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let density = Density::Regular;
    let rows: [(&str, Sample); 14] = [
        (
            "Checkbox, off",
            Sample::Marked(checkbox(0, "Show the grid", false)),
        ),
        (
            "Checkbox, on",
            Sample::Marked(checkbox(0, "Show the grid", true)),
        ),
        (
            "Checkbox, mixed",
            Sample::Marked(checkbox(0, "All layers", Check::Mixed)),
        ),
        ("Radio, off", Sample::Marked(radio(0, "Portrait", false))),
        ("Radio, on", Sample::Marked(radio(0, "Landscape", true))),
        ("Switch, off", Sample::Marked(switch(0, "Autosave", false))),
        ("Switch, on", Sample::Marked(switch(0, "Autosave", true))),
        (
            "Switch, bare",
            Sample::Marked(switch(0, "Autosave", true).bare()),
        ),
        ("Chip, off", Sample::Chip(chip(0, "Sketches", false))),
        ("Chip, on", Sample::Chip(chip(0, "Sketches", true))),
        (
            "Chip with icon",
            Sample::Chip(chip(0, "Pictures", true).icon(Icon::Picture)),
        ),
        (
            "Chip, removable",
            Sample::Chip(chip(0, "ink", false).removable("Remove the tag").squared()),
        ),
        ("Segmented", Sample::Views),
        ("Segmented, icons", Sample::Marks),
    ];
    let heads = Visual::STATES.map(|(name, _)| name);
    let table = Table::new(painter, rect, theme, "Choices", &heads, rows.len());
    for (row, (name, sample)) in rows.iter().enumerate() {
        table.name(painter, theme, row, name);
        for (column, (_, visual)) in Visual::STATES.into_iter().enumerate() {
            // The other cells of a strip, and a chip's cross, stay at
            // rest: the state of the column is that of one part.
            let rest = Visual {
                enabled: visual.enabled,
                ..Visual::REST
            };
            match sample {
                Sample::Marked(control) => {
                    let place = table.place(row, column, control.size(painter, theme, density));
                    let on = if control.mark.is_on() { 1.0 } else { 0.0 };
                    control.paint(painter, place, theme, density, visual, on);
                }
                Sample::Chip(chip) => {
                    let place = table.place(row, column, chip.size(painter, theme, density));
                    let cross = chip.remove.map(|_| rest);
                    chip.paint(painter, place, theme, density, visual, cross);
                }
                Sample::Views | Sample::Marks => {
                    let strip = if matches!(sample, Sample::Views) {
                        segmented(0, &VIEWS, Some(0))
                    } else {
                        segmented(0, &MARK_NAMES, Some(0))
                            .icons(&MARKS)
                            .icons_only()
                    };
                    let place = table.place(row, column, strip.size(painter, theme, density));
                    let cells = strip.cells(place, painter, theme, density);
                    strip.paint(
                        painter,
                        place,
                        theme,
                        density,
                        &[rest, visual, rest],
                        cells.first().copied(),
                    );
                }
            }
        }
    }
}

/// Every choice, live and at rest, at the density of `ui`.
fn live_column(ui: &mut Ui, theme: &Theme) {
    let _ = checkbox("grid", "Show the grid", true).show(ui, theme);
    let _ = checkbox("rulers", "Show the rulers", false).show(ui, theme);
    let _ = checkbox("layers", "All layers", Check::Mixed).show(ui, theme);
    let _ = checkbox("locked", "Locked by the file", true)
        .enabled(false)
        .show(ui, theme);
    let answers = [
        radio("portrait", "Portrait", false).show(ui, theme),
        radio("landscape", "Landscape", true).show(ui, theme),
    ];
    let _ = rove(ui, &answers, Roving::Vertical);
    let _ = switch("autosave", "Autosave", true).show(ui, theme);
    let _ = switch("sounds", "Sounds", false).show(ui, theme);
    let _ = switch("sync", "Sync", true).enabled(false).show(ui, theme);
    let _ = segmented("views", &VIEWS, Some(1)).show(ui, theme);
    let _ = segmented("marks", &MARK_NAMES, Some(0))
        .icons(&MARKS)
        .icons_only()
        .show(ui, theme);
    let _ = segmented("wide", &VIEWS, Some(2))
        .icons(&[Icon::Document, Icon::Picture, Icon::Folder])
        .full_width()
        .show(ui, theme);
    ui.horizontal_wrapped(|ui| {
        let _ = chip("sketches", "Sketches", true).show(ui, theme);
        let _ = chip("pictures", "Pictures", false)
            .icon(Icon::Picture)
            .show(ui, theme);
        let _ = chip("ink", "ink", false)
            .removable("Remove the tag")
            .squared()
            .show(ui, theme);
        let _ = chip("old", "Archived", false)
            .enabled(false)
            .show(ui, theme);
    });
}

#[cfg(test)]
mod tests {
    use egui::accesskit::{Role as NodeRole, Toggled};
    use egui::{Event, FullOutput, Ui};

    use super::super::button::testkit::{
        Bench, default_theme, key, moving, node, over, presets, ring_around,
    };
    use super::*;
    use crate::ui::contrast;

    #[test]
    fn every_mark_and_chip_reads_in_every_preset_and_state() {
        for (id, theme) in presets() {
            for (state, visual) in Visual::STATES {
                let colors = mark_colors(&theme, visual);
                // An empty box, ring or track stands out from what it lies
                // on (3 to 1, the rule for edges), and so does a solid
                // one; the mark on a solid one stands out from it.
                let least = if visual.enabled { 3.0 } else { 2.0 };
                for level in 0..=2 {
                    let surface = theme.surface(level);
                    let edge = contrast(colors.edge, over(surface, colors.under));
                    assert!(edge >= least, "{id}: {state} edge on {level}: {edge:.2}");
                    let solid = contrast(colors.solid, surface);
                    assert!(solid >= least, "{id}: {state} solid on {level}: {solid:.2}");
                }
                let mark = contrast(colors.inverse, colors.solid);
                assert!(mark >= least, "{id}: {state} mark: {mark:.2}");
            }
            // A chip's label reads in every state, lit or not.
            for on in [false, true] {
                for (state, visual) in Visual::STATES {
                    let (fill, ink) = chip(0, "Sketches", on).colors(&theme, visual);
                    let least = if visual.enabled { 4.5 } else { 2.5 };
                    for level in 0..=2 {
                        let ratio = contrast(ink, over(theme.surface(level), fill));
                        assert!(
                            ratio >= least,
                            "{id}: chip on={on} {state} on {level}: {ratio:.2}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn on_and_off_differ_by_shape_and_every_state_differs() {
        let theme = default_theme();
        for (state, visual) in Visual::STATES {
            let colors = mark_colors(&theme, visual);
            // On is solid where off is open: never a shade alone.
            assert_ne!(colors.solid, colors.under, "{state}");
        }
        let [rest, hover, pressed, ..] = Visual::STATES.map(|(_, visual)| visual);
        for (a, b, what) in [(rest, hover, "hover"), (hover, pressed, "press")] {
            let (one, two) = (mark_colors(&theme, a), mark_colors(&theme, b));
            assert!(
                one.edge != two.edge || one.under != two.under,
                "an empty mark changes on {what}"
            );
            assert_ne!(one.solid, two.solid, "a solid mark changes on {what}");
            for on in [false, true] {
                let chip = chip(0, "Tag", on);
                assert_ne!(
                    chip.colors(&theme, a),
                    chip.colors(&theme, b),
                    "a chip on={on} changes on {what}"
                );
            }
        }
        assert_ne!(
            chip(0, "Tag", true).colors(&theme, rest),
            chip(0, "Tag", false).colors(&theme, rest),
            "a lit chip is told from an unlit one"
        );
    }

    #[test]
    fn choices_tell_assistive_technology_what_they_are() {
        let mut bench = Bench::new(true);
        let (_, output) = bench.frame(Density::Regular, Vec::new(), |ui, theme| {
            let _ = checkbox("grid", "Grid", true).show(ui, theme);
            let _ = checkbox("rulers", "Rulers", false).show(ui, theme);
            let _ = checkbox("layers", "All layers", Check::Mixed).show(ui, theme);
            let _ = radio("portrait", "Portrait", false).show(ui, theme);
            let _ = radio("landscape", "Landscape", true).show(ui, theme);
            let _ = switch("autosave", "Autosave", true).show(ui, theme);
            let _ = switch("sync", "Sync", false)
                .enabled(false)
                .bare()
                .show(ui, theme);
            let _ = segmented("views", &["Pages", "Outline"], Some(1)).show(ui, theme);
            let _ = segmented("marks", &["Favorites"], Some(0))
                .icons(&[Icon::Star])
                .icons_only()
                .show(ui, theme);
            let _ = chip("ink", "ink", true)
                .removable("Remove the tag")
                .show(ui, theme);
        });
        assert_eq!(
            node(&output, "Grid"),
            (
                NodeRole::CheckBox,
                "Grid".to_owned(),
                Some(Toggled::True),
                false
            )
        );
        assert_eq!(node(&output, "Rulers").2, Some(Toggled::False));
        assert_eq!(node(&output, "All layers").0, NodeRole::CheckBox);
        assert_ne!(node(&output, "All layers").2, Some(Toggled::True));
        assert_ne!(node(&output, "All layers").2, Some(Toggled::False));
        assert_eq!(node(&output, "Portrait").0, NodeRole::RadioButton);
        assert_eq!(node(&output, "Landscape").2, Some(Toggled::True));
        assert_eq!(node(&output, "Autosave").0, NodeRole::Switch);
        assert_eq!(node(&output, "Autosave").2, Some(Toggled::True));
        // A bare switch is still named, and says it is disabled.
        assert_eq!(node(&output, "Sync").0, NodeRole::Switch);
        assert!(node(&output, "Sync").3);
        assert_eq!(node(&output, "Outline").0, NodeRole::RadioButton);
        assert_eq!(node(&output, "Outline").2, Some(Toggled::True));
        assert_eq!(node(&output, "Pages").2, Some(Toggled::False));
        // An answer shown by its icon alone still has its name.
        assert_eq!(node(&output, "Favorites").0, NodeRole::RadioButton);
        assert_eq!(node(&output, "ink").2, Some(Toggled::True));
        assert_eq!(node(&output, "Remove the tag").0, NodeRole::Button);
    }

    #[test]
    fn choices_take_the_sizes_of_their_density() {
        let mut bench = Bench::new(true);
        for density in Density::ALL {
            let ((marks, strip, wide, room), _) = bench.frame(density, Vec::new(), |ui, theme| {
                let room = ui.available_width();
                let marks = [
                    checkbox("grid", "Grid", true).show(ui, theme),
                    radio("one", "One", true).show(ui, theme),
                    switch("auto", "Auto", true).show(ui, theme),
                    chip("tag", "Tag", false).show(ui, theme).response,
                ]
                .map(|response| (response.rect, response.interact_rect));
                let strip = segmented("views", &["A", "B"], Some(0))
                    .show(ui, theme)
                    .response
                    .rect;
                let wide = segmented("wide", &["A", "B"], Some(0))
                    .full_width()
                    .show(ui, theme)
                    .response
                    .rect;
                (marks, strip, wide, room)
            });
            for (drawn, _) in &marks[..3] {
                assert_eq!(drawn.height(), density.row(), "{density:?}");
            }
            assert_eq!(strip.height(), density.control_height(), "{density:?}");
            assert_eq!(wide.width(), room, "{density:?}");
            // Nothing is harder to hit than the density allows.
            for (drawn, hit) in marks {
                assert!(hit.contains_rect(drawn), "{density:?}");
                assert!(
                    hit.width() >= density.min_hit() && hit.height() >= density.min_hit(),
                    "{density:?}: {hit:?}"
                );
            }
        }
        // The marks grow with the density.
        let sizes = Density::ALL.map(|density| (box_size(density), track_size(density)));
        assert!(sizes.windows(2).all(|pair| pair[0].0.x < pair[1].0.x));
        assert!(sizes.windows(2).all(|pair| pair[0].1.y < pair[1].1.y));
    }

    /// The controls the click test clicks, top to bottom.
    fn column(ui: &mut Ui, theme: &Theme) -> Vec<Response> {
        vec![
            checkbox("grid", "Grid", false).show(ui, theme),
            radio("one", "One", false).show(ui, theme),
            switch("auto", "Auto", false).show(ui, theme),
            switch("off", "Off", false).enabled(false).show(ui, theme),
            chip("tag", "A tag", false).show(ui, theme).response,
        ]
    }

    #[test]
    fn a_click_on_the_label_asks_to_change_the_choice() {
        let mut bench = Bench::new(true);
        let (shown, _) = bench.frame(Density::Regular, Vec::new(), column);
        let places: Vec<Rect> = shown.iter().map(|response| response.rect).collect();
        for (index, place) in places.iter().enumerate() {
            // Near the end of the label, far from the mark: the whole row
            // is what a click aims at.
            let at = pos2(place.right() - 4.0, place.center().y);
            let clicked = bench.click(at, Density::Regular, |ui, theme| {
                column(ui, theme)
                    .iter()
                    .map(Response::clicked)
                    .collect::<Vec<_>>()
            });
            let expected: Vec<bool> = (0..places.len())
                .map(|at| at == index && index != 3)
                .collect();
            assert_eq!(clicked, expected, "a click on control {index}");
        }
    }

    #[test]
    fn a_chip_goes_by_its_cross_or_by_delete_and_flips_by_its_body() {
        let mut bench = Bench::new(true);
        let show = |ui: &mut Ui, theme: &Theme| {
            chip("tag", "A tag", false)
                .removable("Remove")
                .show(ui, theme)
        };
        let (shown, _) = bench.frame(Density::Regular, Vec::new(), show);
        let rect = shown.response.rect;
        let cross = bench.click(
            pos2(rect.right() - rect.height() / 2.0, rect.center().y),
            Density::Regular,
            show,
        );
        assert!(cross.removed, "the cross removes");
        assert!(!cross.response.clicked(), "and does not flip");
        let body = bench.click(
            pos2(rect.left() + 6.0, rect.center().y),
            Density::Regular,
            show,
        );
        assert!(!body.removed && body.response.clicked(), "the body flips");
        // From the keyboard: Tab to it, then Delete.
        bench.click(pos2(700.0, 500.0), Density::Regular, show);
        let _ = bench.frame(Density::Regular, vec![key(Key::Tab)], show);
        let (focused, _) = bench.frame(Density::Regular, Vec::new(), show);
        assert!(focused.response.has_focus(), "Tab reaches the chip");
        let (gone, _) = bench.frame(Density::Regular, vec![key(Key::Delete)], show);
        assert!(gone.removed, "Delete removes the focused chip");
        // A chip with no cross keeps itself.
        let plain = |ui: &mut Ui, theme: &Theme| chip("plain", "Plain", false).show(ui, theme);
        let _ = bench.frame(Density::Regular, vec![key(Key::Tab)], plain);
        let (kept, _) = bench.frame(Density::Regular, vec![key(Key::Delete)], plain);
        assert!(!kept.removed);
    }

    /// One frame of a radio group of three of which the second is
    /// disabled, answering with where the arrows moved focus and which
    /// member holds it.
    fn group(bench: &mut Bench, events: Vec<Event>, chosen: usize) -> (Option<usize>, [bool; 3]) {
        bench
            .frame(Density::Regular, events, |ui, theme| {
                let answers = [
                    radio("a", "A", chosen == 0).show(ui, theme),
                    radio("b", "B", chosen == 1).enabled(false).show(ui, theme),
                    radio("c", "C", chosen == 2).show(ui, theme),
                ];
                let moved = rove(ui, &answers, Roving::Vertical);
                (moved, answers.map(|answer| answer.has_focus()))
            })
            .0
    }

    #[test]
    fn the_arrow_keys_walk_a_radio_group_and_skip_what_is_disabled() {
        let mut bench = Bench::new(true);
        let _ = group(&mut bench, vec![key(Key::Tab)], 0);
        assert_eq!(group(&mut bench, Vec::new(), 0).1, [true, false, false]);
        // Down skips the disabled B and lands on C.
        assert_eq!(group(&mut bench, vec![key(Key::ArrowDown)], 0).0, Some(2));
        assert_eq!(
            group(&mut bench, Vec::new(), 2).1,
            [false, false, true],
            "focus followed the arrow"
        );
        // Down again wraps to A, and Up wraps back.
        assert_eq!(group(&mut bench, vec![key(Key::ArrowDown)], 2).0, Some(0));
        let _ = group(&mut bench, Vec::new(), 0);
        assert_eq!(group(&mut bench, vec![key(Key::ArrowUp)], 0).0, Some(2));
        let _ = group(&mut bench, Vec::new(), 2);
        assert_eq!(group(&mut bench, vec![key(Key::Home)], 2).0, Some(0));
        let _ = group(&mut bench, Vec::new(), 0);
        assert_eq!(group(&mut bench, vec![key(Key::End)], 0).0, Some(2));
        // Left and Right are not a vertical group's.
        let _ = group(&mut bench, Vec::new(), 2);
        assert_eq!(group(&mut bench, vec![key(Key::ArrowRight)], 2).0, None);
        assert_eq!(group(&mut bench, Vec::new(), 2).1, [false, false, true]);
    }

    /// One frame of a strip of three views with `chosen` holding.
    fn views(bench: &mut Bench, events: Vec<Event>, chosen: usize) -> SegmentedResponse {
        bench
            .frame(Density::Regular, events, |ui, theme| {
                segmented("views", &VIEWS, Some(chosen)).show(ui, theme)
            })
            .0
    }

    #[test]
    fn a_segmented_strip_answers_clicks_and_arrows() {
        let mut bench = Bench::new(true);
        let rect = views(&mut bench, Vec::new(), 0).response.rect;
        // A click on the last cell picks the last answer.
        let clicked = bench.click(
            pos2(rect.right() - 10.0, rect.center().y),
            Density::Regular,
            |ui, theme| segmented("views", &VIEWS, Some(0)).show(ui, theme),
        );
        assert_eq!(clicked.picked, Some(2));
        // A click does not move focus; Tab brings it to the first cell,
        // and the arrows pick from the cell that holds it, wrapping.
        let _ = views(&mut bench, vec![key(Key::Tab)], 2);
        let _ = views(&mut bench, Vec::new(), 2);
        assert_eq!(
            views(&mut bench, vec![key(Key::ArrowRight)], 2).picked,
            Some(1)
        );
        let _ = views(&mut bench, Vec::new(), 1);
        assert_eq!(
            views(&mut bench, vec![key(Key::ArrowLeft)], 1).picked,
            Some(0)
        );
        let _ = views(&mut bench, Vec::new(), 0);
        assert_eq!(
            views(&mut bench, vec![key(Key::ArrowLeft)], 0).picked,
            Some(2)
        );
        // Up and Down are not a strip's.
        let _ = views(&mut bench, Vec::new(), 2);
        assert_eq!(views(&mut bench, vec![key(Key::ArrowDown)], 2).picked, None);
        // A disabled strip answers nothing.
        let off = bench.click(
            pos2(rect.left() + 10.0, rect.center().y),
            Density::Regular,
            |ui, theme| {
                segmented("views", &VIEWS, Some(2))
                    .enabled(false)
                    .show(ui, theme)
            },
        );
        assert_eq!(off.picked, None);
    }

    #[test]
    fn the_focus_ring_rounds_the_mark_of_a_choice_in_every_density() {
        let mut bench = Bench::new(true);
        let show = |ui: &mut Ui, theme: &Theme| checkbox("grid", "Grid", true).show(ui, theme);
        for density in Density::ALL {
            let _ = bench.click(pos2(700.0, 500.0), density, show);
            let _ = bench.frame(density, vec![key(Key::Tab)], show);
            let (response, output) = bench.frame(density, Vec::new(), show);
            assert!(response.has_focus(), "{density:?}");
            let size = box_size(density);
            let mark = Rect::from_min_size(
                pos2(
                    response.rect.left(),
                    response.rect.center().y - size.y / 2.0,
                ),
                size,
            );
            assert!(ring_around(&output, mark), "{density:?}: a ring");
            let _ = bench.click(pos2(700.0, 500.0), density, show);
            let (_, output) = bench.frame(density, Vec::new(), show);
            assert!(
                !ring_around(&output, mark),
                "{density:?}: none after a click"
            );
        }
    }

    #[test]
    fn a_choice_is_named_by_its_salt_not_by_its_place() {
        let mut bench = Bench::new(true);
        let ids = |bench: &mut Bench, before: usize| {
            bench
                .frame(Density::Regular, Vec::new(), |ui, theme| {
                    for line in 0..before {
                        ui.label(format!("line {line}"));
                    }
                    [
                        checkbox("a", "Same", true).show(ui, theme).id,
                        checkbox("b", "Same", true).show(ui, theme).id,
                        switch("c", "Switch", true).show(ui, theme).id,
                        chip("d", "Chip", true).show(ui, theme).response.id,
                        segmented("e", &["A"], Some(0)).show(ui, theme).response.id,
                    ]
                })
                .0
        };
        let first = ids(&mut bench, 0);
        assert_eq!(first, ids(&mut bench, 4), "moving down keeps every id");
        assert_ne!(first[0], first[1], "two salts, two controls");
    }

    /// One frame of a switch, a checkbox and a strip, all showing `on`.
    fn flips(bench: &mut Bench, on: bool) -> FullOutput {
        bench
            .frame(Density::Regular, Vec::new(), |ui, theme| {
                let _ = switch("auto", "Auto", on).show(ui, theme);
                let _ = checkbox("grid", "Grid", on).show(ui, theme);
                let _ = segmented("views", &VIEWS, Some(usize::from(on))).show(ui, theme);
            })
            .1
    }

    #[test]
    fn a_flip_moves_then_rests_and_lands_at_once_when_motion_is_off() {
        let mut bench = Bench::new(false);
        let settles = |bench: &mut Bench, on: bool| (0..120).any(|_| !moving(&flips(bench, on)));
        assert!(settles(&mut bench, false), "at rest while nothing changes");
        assert!(moving(&flips(&mut bench, true)), "a flip moves");
        assert!(settles(&mut bench, true), "and comes to rest");
        // With instant transitions a flip lands in the frame it happens.
        let mut instant = Bench::new(true);
        assert!(settles(&mut instant, false));
        assert!(
            !moving(&flips(&mut instant, true)),
            "instant: no frames asked"
        );
    }
}
