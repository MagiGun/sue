//! Toasts: short notices that stack at the bottom of a window, above its
//! status bar.
//!
//! A [`Toasts`] is the stack of one window. Whoever owns the window keeps
//! it, [`push`](Toasts::push)es a [`Toast`] when something is worth
//! saying, and calls [`show`](Toasts::show) every frame with the room the
//! notices may take (the window without its status bar). The newest
//! notice is the lowest; each slides and fades in, stays for its time
//! (the clock stops while the pointer is over the stack), and fades out
//! while those above it settle down. A notice can carry one action
//! ("Show in Folder") and always has a button that closes it.
//!
//! Every notice wears the glyph of its [`Tone`], so the four are told
//! apart without color: the shipped themes are monochrome.
//!
//! A stack that holds nothing draws nothing and asks for no frame, and a
//! stack at rest asks only for the frame in which its next notice leaves.

use std::sync::Arc;

use egui::{
    Area, Color32, Context, Galley, Id, Painter, Rect, Response, Role, Sense, Stroke, StrokeKind,
    Ui, UiBuilder, WidgetInfo, pos2, vec2,
};

use super::focus::focus_ring;
use super::motion::{self, Motion, Pace};
use super::tokens::{Density, Elevation, Layer, Radius, Space, TextRole};
use crate::Theme;

/// How long a notice stays when it does not say, in seconds.
pub const DEFAULT_SECONDS: f32 = 5.0;

/// How many notices show at once: a newer one sends the oldest away.
pub const MAX_SHOWN: usize = 4;

/// The widest a notice gets, in points.
const MAX_WIDTH: f32 = 420.0;

/// What a notice, or a question, is about. Each has a glyph of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    /// Something to know: a ring with a letter i.
    #[default]
    Info,
    /// Something went well: a ring with a check mark.
    Success,
    /// Something to mind: a triangle with an exclamation mark.
    Warning,
    /// Something failed: a ring with a cross.
    Error,
}

impl Tone {
    /// Every tone.
    pub const ALL: [Self; 4] = [Self::Info, Self::Success, Self::Warning, Self::Error];

    /// The color of the tone's glyph: the text color in the shipped
    /// themes.
    #[must_use]
    pub fn color(self, theme: &Theme) -> Color32 {
        match self {
            Self::Info => theme.info(),
            Self::Success => theme.success(),
            Self::Warning => theme.warning(),
            Self::Error => theme.danger(),
        }
    }

    /// Draws the tone's glyph as large as the shorter side of `rect`, in
    /// its middle. These four are drawn with plain lines until the icon
    /// set has them.
    pub fn paint(self, painter: &Painter, rect: Rect, theme: &Theme) {
        let side = rect.width().min(rect.height());
        if side <= 0.0 {
            return;
        }
        let stroke = Stroke::new(theme.icon_stroke(side), self.color(theme));
        let middle = rect.center();
        let at = |x: f32, y: f32| middle + vec2(x, y) * side;
        let ring = side / 2.0 - stroke.width / 2.0 - side / 24.0;
        let dot = stroke.width * 0.7;
        match self {
            Self::Info => {
                painter.circle_stroke(middle, ring, stroke);
                painter.line_segment([at(0.0, -0.04), at(0.0, 0.2)], stroke);
                painter.circle_filled(at(0.0, -0.2), dot, stroke.color);
            }
            Self::Success => {
                painter.circle_stroke(middle, ring, stroke);
                painter.line(
                    vec![at(-0.2, 0.02), at(-0.05, 0.17), at(0.21, -0.14)],
                    stroke,
                );
            }
            Self::Warning => {
                let (top, low, half) = (-0.36, 0.33, 0.42);
                painter.add(egui::Shape::closed_line(
                    vec![at(0.0, top), at(half, low), at(-half, low)],
                    stroke,
                ));
                painter.line_segment([at(0.0, -0.12), at(0.0, 0.08)], stroke);
                painter.circle_filled(at(0.0, 0.2), dot, stroke.color);
            }
            Self::Error => {
                painter.circle_stroke(middle, ring, stroke);
                let arm = 0.15;
                painter.line_segment([at(-arm, -arm), at(arm, arm)], stroke);
                painter.line_segment([at(arm, -arm), at(-arm, arm)], stroke);
            }
        }
    }
}

/// A button that closes what it sits on: a cross that takes the hover
/// fill. `label` is what it tells a screen reader and shows as its tip.
pub(super) fn close_button(ui: &Ui, id: Id, rect: Rect, label: &str, theme: &Theme) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let radius = theme.radius(Radius::Md);
    let lit = motion::hover_t(ui.ctx(), id.with("sue-hover"), response.hovered());
    let fill = if response.is_pointer_button_down_on() {
        theme.fill_pressed()
    } else {
        Color32::TRANSPARENT.lerp_to_gamma(theme.fill_hover(), lit)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, radius, fill);
    let side = Density::of(ui).icon().min(rect.width().min(rect.height()));
    let arm = side * 0.27;
    let stroke = Stroke::new(
        theme.icon_stroke(side),
        theme.text_muted().lerp_to_gamma(theme.text, lit),
    );
    let middle = rect.center();
    painter.line_segment([middle + vec2(-arm, -arm), middle + vec2(arm, arm)], stroke);
    painter.line_segment([middle + vec2(arm, -arm), middle + vec2(-arm, arm)], stroke);
    focus_ring(ui, &response, radius, theme);
    response.on_hover_text(label)
}

/// One notice.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    /// What it is about.
    pub tone: Tone,
    /// What it says, already in the reader's language.
    pub text: String,
    /// The label of its one action, when it has one.
    pub action: Option<String>,
    /// How long it stays, in seconds. `None` stays until it is closed.
    pub seconds: Option<f32>,
}

impl Toast {
    /// A notice of `tone` that says `text` for [`DEFAULT_SECONDS`].
    #[must_use]
    pub fn new(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
            action: None,
            seconds: Some(DEFAULT_SECONDS),
        }
    }

    /// Something to know.
    #[must_use]
    pub fn info(text: impl Into<String>) -> Self {
        Self::new(Tone::Info, text)
    }

    /// Something went well.
    #[must_use]
    pub fn success(text: impl Into<String>) -> Self {
        Self::new(Tone::Success, text)
    }

    /// Something to mind.
    #[must_use]
    pub fn warning(text: impl Into<String>) -> Self {
        Self::new(Tone::Warning, text)
    }

    /// Something failed.
    #[must_use]
    pub fn error(text: impl Into<String>) -> Self {
        Self::new(Tone::Error, text)
    }

    /// Gives it an action button that says `label`.
    #[must_use]
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// Makes it stay for `seconds`.
    #[must_use]
    pub fn seconds(mut self, seconds: f32) -> Self {
        self.seconds = Some(seconds.max(0.0));
        self
    }

    /// Makes it stay until it is closed.
    #[must_use]
    pub fn sticky(mut self) -> Self {
        self.seconds = None;
        self
    }
}

/// The name of a notice in its stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToastId(u64);

/// What happened to a notice in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToastEvent {
    /// Its action button was pressed. It leaves.
    Action(ToastId),
    /// Its close button was pressed. It leaves.
    Dismissed(ToastId),
    /// Its time was up, or newer notices took its place. It leaves.
    Expired(ToastId),
}

/// Where a notice and its buttons were drawn last, for whoever points at
/// them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToastRects {
    /// The whole notice.
    pub toast: Rect,
    /// Its action button, when it has one.
    pub action: Option<Rect>,
    /// Its close button.
    pub close: Rect,
}

/// A notice in the stack.
#[derive(Clone, Debug)]
struct Entry {
    id: ToastId,
    toast: Toast,
    /// When it was first drawn.
    born: Option<f64>,
    /// How long it has been showing, the time under the pointer left out.
    age: f64,
    /// It was sent away by [`Toasts::dismiss`] and leaves in the next
    /// frame.
    dismissed: bool,
    /// When it began to leave.
    leaving: Option<f64>,
    /// Where it was drawn last.
    rects: Option<ToastRects>,
}

/// The stack of notices of one window.
#[derive(Clone, Debug)]
pub struct Toasts {
    id: Id,
    entries: Vec<Entry>,
    next: u64,
    /// When the stack was last shown.
    last: Option<f64>,
}

/// A notice measured for a frame.
struct Laid {
    text: Arc<Galley>,
    action: Option<Arc<Galley>>,
    height: f32,
    /// How much of it is there: 0 gone, 1 in place.
    presence: f32,
    /// How far it has come in.
    entered: f32,
}

impl Toasts {
    /// A stack named by `salt`, which has to differ from every other
    /// stack's on screen.
    #[must_use]
    pub fn new(salt: impl egui::AsIdSalt) -> Self {
        Self {
            id: Id::unique("sue-toasts").with(salt),
            entries: Vec::new(),
            next: 0,
            last: None,
        }
    }

    /// Adds a notice under the others. One that says exactly what a
    /// notice still showing says starts that one's time again instead of
    /// stacking a twin.
    pub fn push(&mut self, toast: Toast) -> ToastId {
        if let Some(twin) = self
            .entries
            .iter_mut()
            .find(|entry| entry.leaving.is_none() && !entry.dismissed && entry.toast == toast)
        {
            twin.age = 0.0;
            return twin.id;
        }
        let id = ToastId(self.next);
        self.next += 1;
        self.entries.push(Entry {
            id,
            toast,
            born: None,
            age: 0.0,
            dismissed: false,
            leaving: None,
            rects: None,
        });
        id
    }

    /// Sends a notice away, as its close button does. Nothing is said
    /// about it in the events of the next frame: the caller knows.
    pub fn dismiss(&mut self, id: ToastId) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.dismissed = true;
        }
    }

    /// Takes every notice away at once.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// How many notices the stack holds, those leaving included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the stack holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The notice `id`, while the stack holds it.
    #[must_use]
    pub fn get(&self, id: ToastId) -> Option<&Toast> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.toast)
    }

    /// Where the notice `id` and its buttons were drawn last.
    #[must_use]
    pub fn rects(&self, id: ToastId) -> Option<ToastRects> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.rects)
    }

    /// Draws the stack at the bottom of `within`, the part of the window
    /// notices may cover (leave the status bar out of it), and says what
    /// happened to which notice.
    pub fn show(&mut self, ctx: &Context, within: Rect, theme: &Theme) -> Vec<ToastEvent> {
        let mut events = Vec::new();
        if self.entries.is_empty() {
            self.last = None;
            return events;
        }
        let now = ctx.input(|input| input.time);
        let passed = self.last.map_or(0.0, |last| (now - last).max(0.0));
        self.last = Some(now);
        let density = if crate::widgets::is_touch(ctx) {
            Density::Touch
        } else {
            Density::Regular
        };
        let seconds = Motion::of(ctx).seconds(Pace::Base);
        let along = |since: f64| {
            if seconds <= 0.0 {
                1.0
            } else {
                motion::ease(((now - since) as f32 / seconds).clamp(0.0, 1.0))
            }
        };

        // Sizes.
        let margin = theme.space(Space::S6);
        let gap = theme.space(Space::S4);
        let pad = vec2(theme.space(Space::S5), theme.space(Space::S3));
        let tight = theme.space(Space::S2);
        let control = density.min_hit();
        let icon = density.icon();
        let width = MAX_WIDTH
            .min(within.width() - margin * 2.0)
            .max(control * 4.0);
        let font = theme.text_font(TextRole::Body);

        // The clock: it stands still while the pointer is over the stack.
        let pointer = ctx.pointer_hover_pos();
        let paused = pointer.is_some_and(|at| {
            self.entries
                .iter()
                .filter_map(|entry| entry.rects)
                .any(|rects| rects.toast.expand(gap).contains(at))
        });
        let mut fresh = false;
        for entry in &mut self.entries {
            if entry.leaving.is_some() {
                continue;
            }
            if entry.dismissed {
                entry.leaving = Some(now);
                continue;
            }
            if entry.born.is_none() {
                fresh = true;
            } else if !paused {
                entry.age += passed;
            }
            let over = entry
                .toast
                .seconds
                .is_some_and(|stay| entry.age >= f64::from(stay));
            if over && !paused && entry.born.is_some() {
                entry.leaving = Some(now);
                events.push(ToastEvent::Expired(entry.id));
            }
            entry.born.get_or_insert(now);
        }
        // More than fit: the oldest make room.
        let staying = self
            .entries
            .iter()
            .filter(|entry| entry.leaving.is_none())
            .count();
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| entry.leaving.is_none())
            .take(staying.saturating_sub(MAX_SHOWN))
        {
            entry.leaving = Some(now);
            events.push(ToastEvent::Expired(entry.id));
        }
        // What has faded out is gone, and so is what left before it was
        // ever drawn.
        self.entries.retain(|entry| {
            entry
                .leaving
                .is_none_or(|since| entry.rects.is_some() && along(since) < 1.0)
        });
        if self.entries.is_empty() {
            self.last = None;
            return events;
        }

        // Measure, then place from the bottom up: the newest is lowest.
        let laid: Vec<Laid> = ctx.fonts_mut(|fonts| {
            self.entries
                .iter()
                .map(|entry| {
                    let action =
                        entry.toast.action.as_ref().map(|label| {
                            fonts.layout_no_wrap(label.clone(), font.clone(), theme.text)
                        });
                    let taken = pad.x
                        + icon
                        + gap
                        + action
                            .as_ref()
                            .map_or(0.0, |label| gap + label.size().x + pad.x * 2.0)
                        + tight
                        + control
                        + tight;
                    let text = fonts.layout(
                        entry.toast.text.clone(),
                        font.clone(),
                        theme.text,
                        (width - taken).max(control),
                    );
                    let entered = entry.born.map_or(0.0, along);
                    let left = entry.leaving.map_or(0.0, along);
                    Laid {
                        height: text.size().y.max(control) + pad.y * 2.0,
                        text,
                        action,
                        presence: entered * (1.0 - left),
                        entered,
                    }
                })
                .collect()
        });
        let left = (within.center().x - width / 2.0).round();
        let mut bottom = within.bottom() - margin;
        let mut places = vec![Rect::NOTHING; laid.len()];
        for (index, toast) in laid.iter().enumerate().rev() {
            // It rises into its place, and its slot opens and closes with
            // it, so the notices above move over smoothly.
            let rise = (1.0 - toast.entered) * pad.x;
            places[index] = Rect::from_min_size(
                pos2(left, (bottom - toast.height + rise).round()),
                vec2(width, toast.height),
            );
            bottom -= (toast.height + gap) * toast.presence;
        }
        let all = places
            .iter()
            .fold(Rect::NOTHING, |all, place| all.union(*place));

        let area = Area::new(self.id)
            .order(Layer::Toast.order())
            .fixed_pos(all.min)
            .constrain(false)
            .fade_in(false)
            .sense(Sense::hover())
            .role(egui::accesskit::Role::Alert)
            .accessible_name(tr!("notices"));
        if fresh {
            // A new notice shows over whatever opened before it.
            ctx.move_to_top(area.layer());
        }
        let dismiss = tr!("toast-dismiss");
        let radius = theme.radius(Radius::Lg);
        let surface = theme.surface(3);
        let stack = self.id;
        area.show(ctx, |ui| {
            ui.allocate_rect(all, Sense::hover());
            for ((entry, toast), place) in self.entries.iter_mut().zip(&laid).zip(&places) {
                let place = *place;
                let id = stack.with(entry.id);
                let mut child = ui.new_child(UiBuilder::new().max_rect(place).id_salt(id));
                child.set_opacity(toast.presence);
                let body = child.interact(place, id.with("sue-toast"), Sense::hover());
                body.widget_info(|| {
                    WidgetInfo::labeled(Role::Label, true, entry.toast.text.as_str())
                });
                let painter = child.painter().clone();
                painter.add(theme.elevation(Elevation::Floating).as_shape(place, radius));
                painter.rect(place, radius, surface, theme.hairline(), StrokeKind::Inside);
                let row = Rect::from_min_max(
                    pos2(place.left() + pad.x, place.top() + pad.y),
                    pos2(place.right() - tight, place.bottom() - pad.y),
                );
                let glyph = Rect::from_center_size(
                    pos2(row.left() + icon / 2.0, row.top() + control / 2.0),
                    vec2(icon, icon),
                );
                entry.toast.tone.paint(&painter, glyph, theme);
                let text_at = pos2(
                    glyph.right() + gap,
                    row.center().y - toast.text.size().y / 2.0,
                );
                painter.galley(text_at, toast.text.clone(), theme.text);
                let close = Rect::from_center_size(
                    pos2(row.right() - control / 2.0, row.top() + control / 2.0),
                    vec2(control, control),
                );
                let action = toast.action.as_ref().map(|label| {
                    let size = vec2(label.size().x + pad.x * 2.0, control);
                    let rect =
                        Rect::from_min_size(pos2(close.left() - tight - size.x, close.top()), size);
                    (rect, label)
                });
                // In reading order, so Tab walks them that way.
                if let Some((rect, label)) = action {
                    let text = entry.toast.action.as_deref().unwrap_or_default();
                    let pressed =
                        text_button(&child, id.with("sue-action"), rect, label, text, theme);
                    if pressed.clicked() && entry.leaving.is_none() {
                        entry.leaving = Some(now);
                        events.push(ToastEvent::Action(entry.id));
                    }
                }
                let closed = close_button(&child, id.with("sue-close"), close, &dismiss, theme);
                if closed.clicked() && entry.leaving.is_none() {
                    entry.leaving = Some(now);
                    events.push(ToastEvent::Dismissed(entry.id));
                }
                entry.rects = Some(ToastRects {
                    toast: place,
                    action: action.map(|(rect, _)| rect),
                    close,
                });
            }
        });

        // Frames: every one while something moves, else only the one in
        // which the next notice leaves.
        let moving = self
            .entries
            .iter()
            .zip(&laid)
            .any(|(entry, toast)| entry.leaving.is_some() || toast.entered < 1.0);
        if moving {
            ctx.request_repaint();
        } else if !paused {
            let next = self
                .entries
                .iter()
                .filter_map(|entry| {
                    let stay = f64::from(entry.toast.seconds?);
                    Some((stay - entry.age).max(0.0))
                })
                .fold(f64::INFINITY, f64::min);
            if next.is_finite() {
                ctx.request_repaint_after_secs(next as f32);
            }
        }
        events
    }
}

/// The action button of a notice: its label in a hairline box.
fn text_button(
    ui: &Ui,
    id: Id,
    rect: Rect,
    label: &Arc<Galley>,
    text: &str,
    theme: &Theme,
) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
    let radius = theme.radius(Radius::Md);
    let lit = motion::hover_t(ui.ctx(), id.with("sue-hover"), response.hovered());
    let fill = if response.is_pointer_button_down_on() {
        theme.fill_pressed()
    } else {
        Color32::TRANSPARENT.lerp_to_gamma(theme.fill_hover(), lit)
    };
    let painter = ui.painter();
    let edge = Stroke::new(theme.border_width, theme.border_strong_on(theme.surface(3)));
    painter.rect(rect, radius, fill, edge, StrokeKind::Inside);
    painter.galley(
        rect.center() - label.size() / 2.0,
        label.clone(),
        theme.text,
    );
    focus_ring(ui, &response, radius, theme);
    response
}

#[cfg(test)]
mod tests;
