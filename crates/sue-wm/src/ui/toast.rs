//! Toasts: short notices that stack at the bottom of a window, above its
//! status bar.
//!
//! A [`Toasts`] is the stack of one window. Whoever owns the window keeps
//! it, [`push`](Toasts::push)es a [`Toast`] when something is worth
//! saying, and calls [`show`](Toasts::show) every frame with the room the
//! notices may take (the window without its status bar, and on a phone
//! without its tab bar). The newest notice is the lowest; each slides and
//! fades in, stays for its time (the clock stops while the pointer is over
//! the stack), and fades out while those above it settle down. A notice
//! can carry one action ("Show in Folder") and always has a button that
//! closes it.
//!
//! A notice can also show how far a piece of work running in the
//! background is ([`Toast::progress`]): a bar under its words that the
//! owner moves on with [`Toasts::set_progress`], and an action such as
//! "Cancel". Such a notice stays until its owner sends it away
//! ([`Toasts::dismiss`]); closing it only hides it, the work goes on.
//!
//! The stack stays small: at most [`MAX_SHOWN`] notices show on a wide
//! screen and [`MAX_SHOWN_NARROW`] on a narrow one. Older ones fold
//! behind the oldest shown, as the edges of cards under it and a button
//! that says how many more there are; it unfolds them, and they keep
//! their own time while folded, so old news leaves on its own. A stack
//! never holds more than [`MAX_HELD`]: past that the oldest leaves.
//!
//! A notice never covers a modal dialog: while one is open the stack
//! waits under the dialog's scrim with its clock stopped, and comes back
//! up, with all its time left, when the dialog closes.
//!
//! Every notice wears the glyph of its [`Tone`], so the four are told
//! apart without color: the shipped themes are monochrome.
//!
//! A stack that holds nothing draws nothing and asks for no frame, and a
//! stack at rest asks only for the frame in which its next notice leaves.

use std::sync::Arc;

use egui::{
    Area, Color32, Context, Galley, Id, LayerId, Painter, Rect, Response, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};

use super::focus::focus_ring;
use super::motion::{self, Motion, Pace};
use super::progress::ProgressBar;
use super::tokens::{Density, Elevation, Layer, Radius, Space, TextRole};
use super::tooltip::{Tip, tooltip};
use crate::Theme;
use crate::icons::{self, Icon};

/// How long a notice stays when it does not say, in seconds.
pub const DEFAULT_SECONDS: f32 = 5.0;

/// How many notices show at once on a wide screen; older ones fold
/// behind them.
pub const MAX_SHOWN: usize = 3;

/// How many notices show at once on a narrow screen (a phone): the
/// newest, with the others folded behind it.
pub const MAX_SHOWN_NARROW: usize = 1;

/// Narrower than this many points, a stack shows [`MAX_SHOWN_NARROW`].
pub const NARROW: f32 = 560.0;

/// The most notices a stack holds, folded ones included: a newer one
/// sends the oldest away.
pub const MAX_HELD: usize = 6;

/// The widest a notice gets, in points.
const MAX_WIDTH: f32 = 420.0;

/// How many card edges show behind a folded stack, at most.
const EDGES: usize = 2;

/// How much of a notice fades while the pointer is over it (not on its
/// buttons), so the work under it shows through.
const SEE_THROUGH: f32 = 0.75;

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
    tooltip(response, &Tip::new(label), theme)
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
    /// How far the work it is about has come, from 0 to 1, shown as a bar
    /// under its words; `None` shows no bar.
    pub progress: Option<f32>,
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
            progress: None,
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

    /// Shows a bar `fraction` full (0 to 1) under its words, for work
    /// still going on. It stays until it is sent away.
    #[must_use]
    pub fn progress(mut self, fraction: f32) -> Self {
        self.progress = Some(progress_of(fraction));
        self.seconds = None;
        self
    }
}

/// `fraction` as a bar shows it: 0 to 1, and 0 for what is not a number.
fn progress_of(fraction: f32) -> f32 {
    if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
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
    /// When it was first drawn: its clock starts then.
    born: Option<f64>,
    /// When it last came into sight (a folded notice that comes forward
    /// rises in again).
    shown: Option<f64>,
    /// How long it has been showing, the time under the pointer and under
    /// a modal dialog left out.
    age: f64,
    /// It was sent away by [`Toasts::dismiss`] and leaves in the next
    /// frame.
    dismissed: bool,
    /// When it began to leave.
    leaving: Option<f64>,
    /// Where it was drawn last; `None` while it is folded.
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
    /// The folded notices were asked to show.
    unfolded: bool,
    /// How many notices were folded in the last frame.
    folded: usize,
    /// Where the button that folds and unfolds the stack was drawn last.
    fold: Option<Rect>,
    /// The band of its room the stack took last: from the top of what it
    /// drew to the room's bottom, across the room.
    area: Option<Rect>,
    /// The layer the stack was drawn on last.
    layer: Option<LayerId>,
}

/// A notice measured for a frame.
struct Laid {
    /// Which entry it is.
    index: usize,
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
            unfolded: false,
            folded: 0,
            fold: None,
            area: None,
            layer: None,
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
            shown: None,
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

    /// Moves the bar of the notice `id` to `fraction` (0 to 1), in place:
    /// its time, its place and its words stay. A notice without a bar
    /// gets one.
    pub fn set_progress(&mut self, id: ToastId, fraction: f32) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.toast.progress = Some(progress_of(fraction));
        }
    }

    /// Takes every notice away at once.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.rest();
    }

    /// How many notices the stack holds, those folded and those leaving
    /// included.
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

    /// Where the notice `id` and its buttons were drawn last; `None`
    /// while it is folded behind newer ones.
    #[must_use]
    pub fn rects(&self, id: ToastId) -> Option<ToastRects> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.rects)
    }

    /// How many notices were folded behind the shown ones in the last
    /// frame.
    #[must_use]
    pub fn folded(&self) -> usize {
        self.folded
    }

    /// Where the button that unfolds the stack ("2 more") or folds it
    /// again was drawn last, while there is one.
    #[must_use]
    pub fn fold_rect(&self) -> Option<Rect> {
        self.fold
    }

    /// `true` while the folded notices were asked to show.
    #[must_use]
    pub fn unfolded(&self) -> bool {
        self.unfolded
    }

    /// How far up from the bottom of its room the stack reached in the
    /// last frame, in points (0 when it showed nothing): what a page may
    /// leave free at its end so nothing stays under the notices.
    #[must_use]
    pub fn covered(&self) -> f32 {
        self.area.map_or(0.0, |area| area.height())
    }

    /// The band of its room the stack took in the last frame: from the
    /// top of what it drew to the bottom of the room, across the room.
    #[must_use]
    pub fn area(&self) -> Option<Rect> {
        self.area
    }

    /// The layer the stack was drawn on in the last frame.
    #[must_use]
    pub fn layer(&self) -> Option<LayerId> {
        self.layer
    }

    /// Forgets what the last frame drew.
    fn rest(&mut self) {
        self.last = None;
        self.unfolded = false;
        self.folded = 0;
        self.fold = None;
        self.area = None;
        self.layer = None;
    }

    /// Draws the stack at the bottom of `within`, the part of the window
    /// notices may cover (leave the status bar and a tab bar out of it),
    /// and says what happened to which notice.
    #[allow(
        clippy::too_many_lines,
        reason = "one frame of the stack, from the clock to the drawing"
    )]
    pub fn show(&mut self, ctx: &Context, within: Rect, theme: &Theme) -> Vec<ToastEvent> {
        let mut events = Vec::new();
        if self.entries.is_empty() {
            self.rest();
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
        // A modal dialog (as the last frame left it): the stack waits
        // under its scrim, with its clock stopped.
        let modal = ctx.memory(|memory| memory.top_modal_layer()).is_some();

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
        let room_for = if within.width() < NARROW {
            MAX_SHOWN_NARROW
        } else {
            MAX_SHOWN
        };

        // The clock: it stands still while the pointer is over the stack.
        let pointer = ctx.pointer_hover_pos();
        let hovered = !modal
            && pointer.is_some_and(|at| {
                self.entries
                    .iter()
                    .filter_map(|entry| entry.rects.map(|rects| rects.toast))
                    .chain(self.fold)
                    .any(|rect| rect.expand(gap).contains(at))
            });
        let paused = modal || hovered;
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
        // More than a stack holds: the oldest make room.
        let staying = self
            .entries
            .iter()
            .filter(|entry| entry.leaving.is_none())
            .count();
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| entry.leaving.is_none())
            .take(staying.saturating_sub(MAX_HELD))
        {
            entry.leaving = Some(now);
            events.push(ToastEvent::Expired(entry.id));
        }
        // What has faded out is gone, and so is what left while it was
        // folded or before it was ever drawn.
        self.entries.retain(|entry| {
            entry
                .leaving
                .is_none_or(|since| entry.rects.is_some() && along(since) < 1.0)
        });
        if self.entries.is_empty() {
            self.rest();
            return events;
        }

        // Which show: the newest of those staying, and those fading out;
        // the older ones fold behind them.
        let staying: Vec<usize> = (0..self.entries.len())
            .filter(|&index| self.entries[index].leaving.is_none())
            .collect();
        if staying.len() <= room_for {
            self.unfolded = false;
        }
        let limit = if self.unfolded { MAX_HELD } else { room_for };
        let folded = &staying[..staying.len().saturating_sub(limit)];
        for &index in folded {
            let entry = &mut self.entries[index];
            entry.rects = None;
            entry.shown = None;
        }
        self.folded = folded.len();
        let visible: Vec<usize> = (0..self.entries.len())
            .filter(|index| !folded.contains(index))
            .collect();
        for &index in &visible {
            self.entries[index].shown.get_or_insert(now);
        }

        // Measure, then place from the bottom up: the newest is lowest.
        let laid: Vec<Laid> = ctx.fonts_mut(|fonts| {
            visible
                .iter()
                .map(|&index| {
                    let entry = &self.entries[index];
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
                    let entered = entry.shown.map_or(0.0, along);
                    let left = entry.leaving.map_or(0.0, along);
                    // A bar for work going on, under the words.
                    let bar = entry
                        .toast
                        .progress
                        .map_or(0.0, |_| tight + ProgressBar::thickness(theme, density));
                    Laid {
                        index,
                        height: text.size().y.max(control) + bar + pad.y * 2.0,
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
        let top_card = places
            .iter()
            .zip(&laid)
            .find(|(_, toast)| self.entries[toast.index].leaving.is_none())
            .map(|(place, _)| *place);

        // The folded ones: the edges of cards behind the oldest shown,
        // and over them a button that says how many; once unfolded, the
        // same button folds them again.
        let step = theme.space(Space::S3);
        let edges: Vec<Rect> = top_card.map_or_else(Vec::new, |card| {
            (1..=self.folded.min(EDGES))
                .map(|level| {
                    let level = level as f32;
                    Rect::from_min_max(
                        pos2(card.left() + pad.x * level, card.top() - step * level),
                        pos2(card.right() - pad.x * level, card.bottom() - step * level),
                    )
                })
                .collect()
        });
        let fold_label = if self.folded > 0 {
            Some((tr!("toast-more", count = self.folded), Icon::ChevronUp))
        } else if self.unfolded {
            Some((tr!("toast-fewer"), Icon::ChevronDown))
        } else {
            None
        };
        let fold_size = (density.min_hit() * 0.8).round();
        let fold = top_card.zip(fold_label).map(|(card, (label, glyph))| {
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    label.clone(),
                    theme.text_font(TextRole::Caption),
                    Color32::PLACEHOLDER,
                )
            });
            let above = edges.last().map_or(card.top(), |edge| edge.top());
            let size = vec2(
                galley.size().x + icon + tight + pad.x * 2.0,
                fold_size.max(galley.size().y + tight * 2.0),
            );
            let rect =
                Rect::from_min_size(pos2(card.right() - size.x, above - tight - size.y), size);
            (rect, label, galley, glyph)
        });
        self.fold = fold.as_ref().map(|(rect, ..)| *rect);
        let all = places
            .iter()
            .chain(&edges)
            .chain(fold.as_ref().map(|(rect, ..)| rect))
            .fold(Rect::NOTHING, |all, place| all.union(*place));
        self.area = Some(Rect::from_min_max(
            pos2(within.left(), all.top().min(within.bottom())),
            within.max,
        ));

        // Under a modal dialog the stack draws on the layer below the
        // dialogs' (its scrim dims it and it takes no clicks); else on
        // the notices' own, over what opened before it.
        let order = if modal {
            Layer::FloatingPalette.order()
        } else {
            Layer::Toast.order()
        };
        // The cards take no clicks: what lies under them (a canvas, a
        // timeline) still gets every press, and only their buttons, each
        // on a layer of its own just over the cards, can be pressed.
        let area = Area::new(self.id)
            .order(order)
            .fixed_pos(all.min)
            .constrain(false)
            .fade_in(false)
            .interactable(false)
            .role(egui::accesskit::Role::Alert)
            .accessible_name(tr!("notices"));
        self.layer = Some(area.layer());
        if fresh {
            ctx.move_to_top(area.layer());
        }
        let dismiss = tr!("toast-dismiss");
        let radius = theme.radius(Radius::Lg);
        let surface = theme.surface(3);
        let stack = self.id;
        let mut toggle = false;
        area.show(ctx, |ui| {
            ui.allocate_rect(all, Sense::hover());
            let on_top =
                |id: Id, rect: Rect, opacity: f32, add: &mut dyn FnMut(&Ui) -> Response| {
                    let area = Area::new(id.with("sue-on-top"))
                        .order(order)
                        .fixed_pos(rect.min)
                        .constrain(false)
                        .fade_in(false);
                    let layer = area.layer();
                    let response = area
                        .show(ctx, |ui| {
                            ui.set_opacity(opacity);
                            add(ui)
                        })
                        .inner;
                    ctx.move_to_top(layer);
                    response
                };
            // Behind the cards, the edges of the folded ones, the farthest
            // first.
            let painter = ui.painter();
            for edge in edges.iter().rev() {
                painter.rect(
                    *edge,
                    radius,
                    theme.surface(2),
                    theme.hairline(),
                    StrokeKind::Inside,
                );
            }
            for (toast, place) in laid.iter().zip(&places) {
                let entry = &mut self.entries[toast.index];
                let place = *place;
                let id = stack.with(entry.id);
                let mut child = ui.new_child(UiBuilder::new().max_rect(place).id_salt(id));
                // A pointer over the card (not on its buttons) sees
                // through it to the work under it.
                let row_top = place.top() + pad.y;
                let action_width = toast
                    .action
                    .as_ref()
                    .map_or(0.0, |label| label.size().x + pad.x * 2.0 + tight);
                let buttons = Rect::from_min_max(
                    pos2(place.right() - tight - control - action_width, row_top),
                    pos2(place.right(), row_top + control),
                );
                let through =
                    !modal && pointer.is_some_and(|at| place.contains(at) && !buttons.contains(at));
                let clear = motion::hover_t(ctx, id.with("sue-see-through"), through);
                let opacity = toast.presence * (1.0 - SEE_THROUGH * clear);
                child.set_opacity(opacity);
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
                let lines = toast.text.size().y.max(control);
                let text_at = pos2(
                    glyph.right() + gap,
                    row.top() + (lines - toast.text.size().y) / 2.0,
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
                    let pressed = on_top(id.with("sue-action-layer"), rect, opacity, &mut |ui| {
                        text_button(ui, id.with("sue-action"), rect, label, text, theme)
                    });
                    if pressed.clicked() && entry.leaving.is_none() {
                        entry.leaving = Some(now);
                        events.push(ToastEvent::Action(entry.id));
                    }
                }
                let closed = on_top(id.with("sue-close-layer"), close, opacity, &mut |ui| {
                    close_button(ui, id.with("sue-close"), close, &dismiss, theme)
                });
                if closed.clicked() && entry.leaving.is_none() {
                    entry.leaving = Some(now);
                    events.push(ToastEvent::Dismissed(entry.id));
                }
                if let Some(fraction) = entry.toast.progress {
                    let thick = ProgressBar::thickness(theme, density);
                    let track = Rect::from_min_max(
                        pos2(text_at.x, row.top() + lines + tight),
                        pos2(
                            row.right() - tight - control / 2.0,
                            row.top() + lines + tight + thick,
                        ),
                    );
                    let mut bar = child.new_child(UiBuilder::new().max_rect(track));
                    ProgressBar::new(fraction)
                        .width(track.width())
                        .label(entry.toast.text.as_str())
                        .show(&mut bar, theme);
                }
                entry.rects = Some(ToastRects {
                    toast: place,
                    action: action.map(|(rect, _)| rect),
                    close,
                });
            }
            if let Some((rect, label, galley, glyph)) = &fold {
                toggle = on_top(stack.with("sue-fold-layer"), *rect, 1.0, &mut |ui| {
                    fold_button(
                        ui,
                        stack.with("sue-fold"),
                        *rect,
                        (label, galley, *glyph),
                        theme,
                    )
                })
                .clicked();
            }
        });
        if toggle {
            self.unfolded = !self.unfolded;
            ctx.request_repaint();
        }

        // Frames: every one while something moves, else only the one in
        // which the next notice leaves (none while the clock stands).
        let moving = laid.iter().any(|toast| {
            self.entries
                .get(toast.index)
                .is_some_and(|entry| entry.leaving.is_some())
                || toast.entered < 1.0
        });
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

/// The button over a folded stack: how many more notices there are (or
/// that it folds them again) and an arrow, in a small hairline pill.
fn fold_button(
    ui: &Ui,
    id: Id,
    rect: Rect,
    (label, galley, glyph): (&str, &Arc<Galley>, Icon),
    theme: &Theme,
) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let radius = theme.radius(Radius::Full);
    let lit = motion::hover_t(ui.ctx(), id.with("sue-hover"), response.hovered());
    let fill = if response.is_pointer_button_down_on() {
        theme.fill_pressed()
    } else {
        theme.surface(3).lerp_to_gamma(theme.fill_hover(), lit)
    };
    let painter = ui.painter();
    painter.add(theme.elevation(Elevation::Floating).as_shape(rect, radius));
    painter.rect(rect, radius, fill, theme.hairline(), StrokeKind::Inside);
    let ink = theme.text_muted().lerp_to_gamma(theme.text, lit);
    let side = Density::of(ui).icon().min(rect.height());
    let pad = (rect.width() - galley.size().x - side - theme.space(Space::S2)) / 2.0;
    painter.galley(
        pos2(rect.left() + pad, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        ink,
    );
    icons::paint(
        painter,
        Rect::from_center_size(
            pos2(rect.right() - pad - side / 2.0, rect.center().y),
            egui::Vec2::splat(side),
        ),
        glyph,
        ink,
    );
    focus_ring(ui, &response, radius, theme);
    tooltip(response, &Tip::new(label), theme)
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
    tooltip(response, &Tip::new(text), theme)
}

#[cfg(test)]
mod tests;
