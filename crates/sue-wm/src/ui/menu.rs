//! Menus: the one implementation behind the menu bar, the Sue menu,
//! right-click menus and popovers.
//!
//! A menu is a list of [`MenuItem`]s shown at a point, with its nested
//! menus opening beside it. [`MenuState`] is what it keeps between frames;
//! [`MenuState::show`] draws it and says what happened.
//!
//! With the pointer: a row lights up under it, a nested menu opens when
//! the pointer is on its row (and stays while the pointer travels to it),
//! a click picks, a click anywhere else closes.
//!
//! With the keyboard: Up and Down walk the rows (Home and End jump to the
//! first and the last), Right or Enter opens a nested menu, Left or Escape
//! leaves it, Enter or Space picks, Escape closes, and typing the start of
//! a label jumps to it. The keyboard may rest on an item that cannot be
//! used now, so that its note can say why.
//!
//! A menu taller than the room it has scrolls: with the wheel, by dragging
//! on a touch screen, by resting the pointer on the strip at either end,
//! and by itself to keep the keyboard's row in view.
//!
//! While a menu is open it holds the keyboard: whatever had keyboard focus
//! gives it up, and gets it back when the menu closes.

use egui::{
    Align2, Area, Context, CornerRadius, Event, Id, Key, Modifiers, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2,
};

use super::tooltip::{Side, Tip, tip_beside};
use super::{Density, Elevation, Layer, Radius, Space, TextRole, focus};
use crate::Theme;
use crate::menubar::{ItemState, MenuItem};

/// Height of a separator.
const GAP: f32 = 9.0;
/// The least width of a menu.
const MIN_WIDTH: f32 = 180.0;
/// Where a label starts in its row: the room of the check mark.
const MARK: f32 = 26.0;
/// Height of the strip at an end of a menu that scrolls.
const STRIP: f32 = 16.0;
/// How far the rows fade out where they pass under a strip.
const FADE: f32 = 10.0;
/// How long typed letters count as one word, in milliseconds.
const TYPE_AHEAD_MS: u64 = 1000;
/// Rows a second a menu scrolls while the pointer rests on a strip.
const STRIP_ROWS_PER_SECOND: f32 = 10.0;
/// The pointer speed, in points a second, over which it is on its way
/// somewhere and not choosing a row.
const TRAVEL_SPEED: f32 = 60.0;

/// What a menu did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// It is open and nothing was decided.
    Open,
    /// The item running this command was picked; the menu has closed.
    Picked(String),
    /// It is closed: by Escape, by a click elsewhere, or it never was open.
    Closed,
    /// Left was pressed where there is nothing to leave: a menu bar opens
    /// the menu before this one. The menu is still open.
    Previous,
    /// Right was pressed on a row with no nested menu: a menu bar opens
    /// the menu after this one. The menu is still open.
    Next,
}

/// What one frame of a menu needs.
#[derive(Clone, Copy)]
pub struct MenuRequest<'a> {
    /// Names the menu. It has to stay the same from frame to frame and
    /// differ from every other menu's.
    pub id: Id,
    /// Where the top left corner of the menu goes when there is room.
    pub anchor: Pos2,
    /// What opened the menu (a bar, a button): a click on it does not
    /// close the menu, since it toggles it itself.
    pub owner: Rect,
    /// The rows.
    pub items: &'a [MenuItem],
    /// How each command's row looks now.
    pub state: &'a dyn Fn(&str) -> ItemState,
    /// How tall the rows are.
    pub density: Density,
}

/// A menu between frames: whether it is open, which nested menus are, and
/// where the pointer or the keyboard is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    open: bool,
    /// The row whose nested menu is open, level by level.
    path: Vec<usize>,
    /// The level and row the pointer or the keyboard is on.
    cursor: Option<(usize, usize)>,
    /// The cursor was last moved by a key.
    by_key: bool,
    /// The cursor's row has to be scrolled into view.
    reveal: bool,
    /// The letters typed so far, in lower case.
    typed: String,
    /// When the last of them was typed, in milliseconds.
    typed_at: u64,
    /// Goes up with every opening, so that each starts fresh.
    serial: u32,
    /// Drawn at least once since it opened.
    shown: bool,
    /// What held keyboard focus when it opened.
    focus_before: Option<Id>,
    /// The focus has to be handed back.
    hand_back: bool,
}

/// What a menu remembers between frames that is no part of its state: the
/// pointer, where its menus were drawn, and how long the cursor has
/// rested.
#[derive(Clone, Debug, Default)]
struct Memo {
    pointer: Option<Pos2>,
    rects: Vec<Rect>,
    rest: Option<((usize, usize), f64)>,
}

/// Where a menu of some level goes.
#[derive(Clone, Copy)]
enum Anchor {
    /// The first menu: its top left corner.
    Root(Pos2),
    /// A nested menu: the menu it belongs to, and the row that opened it.
    Nested { parent: Rect, row: Rect },
}

/// What drawing one menu found.
struct Drawn {
    rect: Rect,
    /// The row whose nested menu is open.
    open_row: Option<Rect>,
    /// The cursor's row, when it is in view.
    cursor_row: Option<Rect>,
    picked: Option<String>,
}

/// The pointer, as the rows need it.
#[derive(Clone, Copy)]
struct Pointer {
    at: Option<Pos2>,
    /// It may move the cursor: it moved, or no key has taken the cursor.
    acts: bool,
    velocity: egui::Vec2,
}

/// `true` for a row the cursor can be on.
fn selectable(item: &MenuItem) -> bool {
    !matches!(item, MenuItem::Separator)
}

/// How the row of `item` looks: a nested menu can be opened while any
/// command in it works.
fn state_of(item: &MenuItem, state: &dyn Fn(&str) -> ItemState) -> ItemState {
    match item {
        MenuItem::Action { command, .. } => state(command),
        MenuItem::Sub { .. } => {
            if item.commands().iter().any(|command| state(command).enabled) {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        MenuItem::Separator => ItemState::off(),
    }
}

/// The rows of the menu reached by opening `path` from `root`.
fn items_at<'a>(root: &'a [MenuItem], path: &[usize]) -> Option<&'a [MenuItem]> {
    let mut items = root;
    for index in path {
        match items.get(*index) {
            Some(MenuItem::Sub { items: nested, .. }) => items = nested,
            _ => return None,
        }
    }
    Some(items)
}

/// The row after (or before) `from` that the cursor can be on, wrapping.
/// From nowhere it is the first (or the last).
fn step(items: &[MenuItem], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = items.len();
    let mut at = from;
    for _ in 0..count {
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        if selectable(&items[next]) {
            return Some(next);
        }
        at = Some(next);
    }
    None
}

/// The row whose label starts with what was `typed` (in lower case),
/// looking from the cursor on. One letter, or one letter typed again and
/// again, goes to the next such row each time.
fn type_ahead(items: &[MenuItem], from: Option<usize>, typed: &str) -> Option<usize> {
    let first = typed.chars().next()?;
    let again = typed.chars().all(|letter| letter == first);
    let (needle, start) = if again {
        (first.to_string(), from.map_or(0, |index| index + 1))
    } else {
        (typed.to_owned(), from.unwrap_or(0))
    };
    let count = items.len();
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|index| {
            let item = &items[*index];
            selectable(item)
                && item
                    .label()
                    .trim_start()
                    .to_lowercase()
                    .starts_with(&needle)
        })
}

impl MenuState {
    /// A closed menu.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn start(&mut self) {
        // What a menu that was closed without a frame still owes.
        let owed = self.focus_before.take().filter(|_| self.shown);
        *self = Self {
            open: true,
            serial: self.serial.wrapping_add(1),
            focus_before: owed,
            hand_back: false,
            shown: owed.is_some(),
            ..Self::default()
        };
        // A menu opened over one that held the keyboard keeps holding it.
    }

    /// Opens the menu. It appears at once, whole: a menu that faded in
    /// would need frames that Sue's frame pacing does not know of.
    pub fn open(&mut self) {
        self.start();
    }

    /// Opens the menu, for scripted runs. The same as
    /// [`open`](Self::open), kept so that scripts say what they mean.
    pub fn open_at_once(&mut self) {
        self.start();
    }

    /// Closes the menu and every nested one.
    pub fn close(&mut self) {
        if self.open {
            self.open = false;
            self.path.clear();
            self.cursor = None;
            self.typed.clear();
            self.hand_back = self.shown;
            self.shown = false;
        }
    }

    /// `true` while the menu is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The row whose nested menu is open, level by level.
    #[must_use]
    pub fn path(&self) -> &[usize] {
        &self.path
    }

    /// The level and the row the pointer or the keyboard is on.
    #[must_use]
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// Shows other rows in the open menu (a menu bar going to the next
    /// menu): nested menus close, and with `by_key` the keyboard starts
    /// on the first row.
    pub fn switch(&mut self, items: &[MenuItem], by_key: bool) {
        self.path.clear();
        self.typed.clear();
        self.serial = self.serial.wrapping_add(1);
        self.cursor = None;
        if by_key {
            self.cursor = step(items, None, true).map(|index| (0, index));
            self.by_key = true;
            self.reveal = true;
        }
    }

    /// Gives keyboard focus back to what had it, when a menu that was
    /// closed from outside still owes that. A closed menu that is not
    /// shown calls this each frame; [`show`](Self::show) does it too.
    pub fn idle(&mut self, ctx: &Context) {
        if self.open || !self.hand_back {
            return;
        }
        self.hand_back = false;
        if let Some(id) = self.focus_before.take() {
            // Only when nothing took the keyboard meanwhile.
            ctx.memory_mut(|memory| {
                if memory.focused().is_none() {
                    memory.request_focus(id);
                }
            });
        }
    }

    /// Closes the menu and hands the keyboard back.
    fn finish(&mut self, ctx: &Context) {
        self.close();
        self.idle(ctx);
    }

    /// Opens the nested menu of row `index` at `level`.
    fn open_nested(&mut self, level: usize, index: usize) {
        self.path.truncate(level);
        self.path.push(index);
    }

    /// Answers this frame's keys. Returns what they decided.
    fn keys(&mut self, ctx: &Context, request: &MenuRequest<'_>) -> Option<MenuEvent> {
        let level = self
            .cursor
            .map_or(self.path.len(), |(level, _)| level.min(self.path.len()));
        let items = items_at(request.items, &self.path[..level])?;
        let at = self
            .cursor
            .filter(|(cursor_level, index)| *cursor_level == level && *index < items.len())
            .map(|(_, index)| index);
        // Take the keys a menu answers, and everything typed, out of the
        // input: nothing drawn after the menu acts on them too.
        let (typed, now, mut pressed) = ctx.input_mut(|input| {
            let mut typed = String::new();
            input.events.retain(|event| match event {
                Event::Text(text) => {
                    typed.push_str(text);
                    false
                }
                _ => true,
            });
            let keys = [
                Key::ArrowDown,
                Key::ArrowUp,
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::Home,
                Key::End,
                Key::Enter,
                Key::Space,
                Key::Escape,
            ]
            .map(|key| (key, input.consume_key(Modifiers::NONE, key)));
            (typed, (input.time * 1000.0).max(0.0) as u64, keys)
        });
        let mut hit = |key: Key| {
            pressed
                .iter_mut()
                .find(|(known, _)| *known == key)
                .is_some_and(|(_, down)| std::mem::take(down))
        };
        let go = |state: &mut Self, index: Option<usize>| {
            if let Some(index) = index {
                state.cursor = Some((level, index));
                state.path.truncate(level);
                state.by_key = true;
                state.reveal = true;
            }
        };

        // Type-ahead. A space joins a word being typed; alone it picks.
        let mut space_typed = false;
        for letter in typed.chars().flat_map(char::to_lowercase) {
            if now.saturating_sub(self.typed_at) > TYPE_AHEAD_MS {
                self.typed.clear();
            }
            if letter.is_control() || (letter == ' ' && self.typed.is_empty()) {
                continue;
            }
            space_typed |= letter == ' ';
            self.typed.push(letter);
            self.typed_at = now;
            let found = type_ahead(items, at, &self.typed);
            go(self, found);
        }
        let at = self
            .cursor
            .filter(|(cursor_level, _)| *cursor_level == level)
            .map(|(_, index)| index);

        if hit(Key::ArrowDown) {
            go(self, step(items, at, true));
        }
        if hit(Key::ArrowUp) {
            go(self, step(items, at, false));
        }
        if hit(Key::Home) {
            go(self, step(items, None, true));
        }
        if hit(Key::End) {
            go(self, step(items, None, false));
        }
        let at = self
            .cursor
            .filter(|(cursor_level, _)| *cursor_level == level)
            .map(|(_, index)| index);
        let item = at.and_then(|index| items.get(index));
        let usable = item.is_some_and(|item| state_of(item, request.state).enabled);
        let enter = hit(Key::Enter);
        let space = hit(Key::Space) && !space_typed;
        let right = hit(Key::ArrowRight);
        let left = hit(Key::ArrowLeft);
        let escape = hit(Key::Escape);
        // Into a nested menu, the keyboard on its first row.
        let into = |state: &mut Self, index: usize, nested: &[MenuItem]| {
            state.open_nested(level, index);
            state.cursor = step(nested, None, true).map(|first| (level + 1, first));
            state.by_key = true;
            state.reveal = true;
        };
        if enter || space {
            match (at, item) {
                (Some(_), Some(MenuItem::Action { command, .. })) if usable => {
                    return Some(MenuEvent::Picked(command.clone()));
                }
                (Some(index), Some(MenuItem::Sub { items: nested, .. })) if usable => {
                    into(self, index, nested);
                }
                _ => {}
            }
        }
        if right {
            match (at, item) {
                (Some(index), Some(MenuItem::Sub { items: nested, .. })) if usable => {
                    into(self, index, nested);
                }
                _ => return Some(MenuEvent::Next),
            }
        }
        if left || escape {
            if level > 0 {
                // Out of the nested menu, back on the row that opened it.
                self.cursor = Some((level - 1, self.path[level - 1]));
                self.path.truncate(level - 1);
                self.by_key = true;
                self.reveal = true;
            } else if escape {
                return Some(MenuEvent::Closed);
            } else {
                return Some(MenuEvent::Previous);
            }
        }
        None
    }

    /// Draws the open menu and its open nested menus over everything, and
    /// answers the pointer and the keyboard. A closed menu draws nothing
    /// and answers [`MenuEvent::Closed`].
    pub fn show(&mut self, ctx: &Context, request: &MenuRequest<'_>, theme: &Theme) -> MenuEvent {
        if !self.open {
            self.idle(ctx);
            return MenuEvent::Closed;
        }
        let first_frame = !self.shown;
        if first_frame {
            // The menu holds the keyboard while it is open.
            self.shown = true;
            self.focus_before = ctx.memory(|memory| memory.focused());
        }
        if let Some(id) = ctx.memory(|memory| memory.focused()) {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
        // Rows may have gone since the last frame.
        while !self.path.is_empty() && items_at(request.items, &self.path).is_none() {
            self.path.pop();
        }
        if self
            .cursor
            .is_some_and(|(level, _)| level > self.path.len())
        {
            self.cursor = None;
        }

        // Whether the keyboard's row shows its ring follows the keys of
        // this frame, which the menu is about to take.
        focus::focus_visible(ctx);
        let decided = self.keys(ctx, request);
        match decided {
            Some(MenuEvent::Picked(_) | MenuEvent::Closed) => {
                self.finish(ctx);
                return decided.unwrap_or(MenuEvent::Closed);
            }
            // The rows are about to change: there is nothing to draw.
            Some(MenuEvent::Previous | MenuEvent::Next) => {
                return decided.unwrap_or(MenuEvent::Open);
            }
            _ => {}
        }

        let memo_id = request.id.with(("sue-menu-memo", self.serial));
        let before: Memo = ctx.data(|data| data.get_temp(memo_id)).unwrap_or_default();
        let (at, velocity, any_click, now) = ctx.input(|input| {
            (
                input.pointer.latest_pos(),
                input.pointer.velocity(),
                input.pointer.any_click(),
                input.time,
            )
        });
        let pointer = Pointer {
            at,
            acts: !self.by_key || (at.is_some() && at != before.pointer),
            velocity,
        };

        let mut rects = Vec::new();
        let mut picked = None;
        let mut cursor_row = None;
        let mut anchor = Anchor::Root(request.anchor);
        let mut level = 0;
        while let Some(items) = items_at(request.items, &self.path[..level.min(self.path.len())]) {
            if level > self.path.len() {
                break;
            }
            let drawn = self.popup(ctx, request, level, items, anchor, &pointer, &before, theme);
            rects.push(drawn.rect);
            if drawn.picked.is_some() {
                picked = drawn.picked;
            }
            if let Some(row) = drawn.cursor_row {
                cursor_row = Some((drawn.rect, row));
            }
            if level >= self.path.len() {
                break;
            }
            let Some(row) = drawn.open_row else {
                self.path.truncate(level);
                break;
            };
            anchor = Anchor::Nested {
                parent: drawn.rect,
                row,
            };
            level += 1;
        }
        self.reveal = false;

        // The note of the row the cursor rests on.
        let mut rest = None;
        if let (Some(cursor), Some((menu, row))) = (self.cursor, cursor_row)
            && let Some(MenuItem::Action { note, .. }) =
                items_at(request.items, &self.path[..cursor.0.min(self.path.len())])
                    .and_then(|items| items.get(cursor.1))
            && !note.is_empty()
        {
            let since = before
                .rest
                .filter(|(rested, _)| *rested == cursor)
                .map_or(now, |(_, since)| since);
            rest = Some((cursor, since));
            let delay = f64::from(ctx.global_style().interaction.tooltip_delay);
            let waited = now - since;
            if waited >= delay {
                let beside = Rect::from_x_y_ranges(menu.x_range(), row.y_range());
                tip_beside(
                    ctx,
                    request.id.with("sue-menu-note"),
                    beside,
                    Side::Right,
                    &Tip::new("").help(note),
                    theme,
                );
            } else {
                ctx.request_repaint_after_secs((delay - waited) as f32);
            }
        }

        let inside = at.is_some_and(|at| {
            request.owner.contains(at) || rects.iter().any(|rect| rect.contains(at))
        });
        ctx.data_mut(|data| {
            data.insert_temp(
                memo_id,
                Memo {
                    pointer: at,
                    rects,
                    rest,
                },
            );
        });
        if let Some(command) = picked {
            self.finish(ctx);
            return MenuEvent::Picked(command);
        }
        // The click that opened the menu does not close it.
        if any_click && !inside && !first_frame {
            self.finish(ctx);
            return MenuEvent::Closed;
        }
        MenuEvent::Open
    }

    /// One menu of the open ones.
    #[allow(clippy::too_many_arguments, reason = "one popup of a nested menu")]
    #[allow(clippy::too_many_lines, reason = "one menu, row by row")]
    fn popup(
        &mut self,
        ctx: &Context,
        request: &MenuRequest<'_>,
        level: usize,
        items: &[MenuItem],
        anchor: Anchor,
        pointer: &Pointer,
        before: &Memo,
        theme: &Theme,
    ) -> Drawn {
        let font = theme.text_font(TextRole::Body);
        // For a finger: taller rows, and no keyboard shortcuts to read.
        let touch = request.density == Density::Touch;
        let row_height = request.density.row();
        let pad = theme.space(Space::S2);
        let screen = ctx.content_rect().shrink(2.0);
        let width = ctx
            .fonts_mut(|fonts| {
                items
                    .iter()
                    .map(|item| {
                        let label = fonts
                            .layout_no_wrap(item.label().to_owned(), font.clone(), theme.text)
                            .size()
                            .x;
                        let right = match item {
                            MenuItem::Action { shortcut, .. } if !shortcut.is_empty() && !touch => {
                                fonts
                                    .layout_no_wrap(shortcut.clone(), font.clone(), theme.text)
                                    .size()
                                    .x
                                    + 32.0
                            }
                            MenuItem::Sub { .. } => 28.0,
                            _ => 0.0,
                        };
                        label + right + 44.0
                    })
                    .fold(MIN_WIDTH, f32::max)
            })
            .min(screen.width());
        let height_of = |item: &MenuItem| {
            if selectable(item) { row_height } else { GAP }
        };
        let full = items.iter().map(height_of).sum::<f32>() + pad * 2.0;

        // Where it goes, and how much of it fits.
        let (origin, height) = match anchor {
            Anchor::Root(at) => {
                let x = at.x.min(screen.right() - width).max(screen.left());
                let top = at.y.clamp(screen.top(), screen.bottom());
                let below = screen.bottom() - top;
                if full <= below {
                    (pos2(x, top), full)
                } else if full <= screen.height() {
                    // It fits the window, not the room under what opened
                    // it: it moves up just enough to show whole, rather
                    // than hide rows behind a scroll.
                    (pos2(x, screen.bottom() - full), full)
                } else if below >= (row_height * 6.0).min(full) {
                    // It keeps its place under what opened it and scrolls.
                    (pos2(x, top), below)
                } else {
                    let height = full.min(screen.height());
                    (pos2(x, screen.bottom() - height), height)
                }
            }
            Anchor::Nested { parent, row } => {
                let after = parent.right() - 2.0;
                let x = if after + width <= screen.right() {
                    after
                } else {
                    // No room at the right: it opens to the left.
                    (parent.left() + 2.0 - width).max(screen.left())
                };
                let height = full.min(screen.height());
                let y = (row.top() - pad)
                    .min(screen.bottom() - height)
                    .max(screen.top());
                (pos2(x, y), height)
            }
        };
        let area = Rect::from_min_size(origin, vec2(width, height));
        let overflow = (full - height).max(0.0);
        let own_path = self.path[..level.min(self.path.len())].to_vec();
        let scroll_id = request
            .id
            .with(("sue-menu-scroll", self.serial, own_path.clone()));
        let mut offset: f32 = if overflow > 0.0 {
            ctx.data(|data| data.get_temp(scroll_id)).unwrap_or(0.0)
        } else {
            0.0
        };

        // Keep the keyboard's row in view.
        if self.reveal
            && overflow > 0.0
            && let Some((cursor_level, cursor)) = self.cursor
            && cursor_level == level
        {
            let top = pad + items.iter().take(cursor).map(height_of).sum::<f32>();
            let bottom = top + row_height;
            if step(items, None, true) == Some(cursor) {
                offset = 0.0;
            } else if step(items, None, false) == Some(cursor) {
                offset = overflow;
            } else {
                // Clear of the strips and of the fade under them.
                let margin = STRIP + FADE;
                offset = offset.min(top - margin).max(bottom + margin - height);
            }
        }

        let surface = theme.surface(3);
        // The radii of the token table: menus round as dialogs and the
        // menus egui draws, their rows as chips.
        let radius = theme.radius(Radius::Xl);
        let row_radius = theme
            .radius(Radius::Sm)
            .min(super::nested_radius(radius, pad));
        let disabled = theme.text_disabled(surface);
        let muted = theme.text_muted_on(surface);
        // The row under the pointer or the keyboard. The hover fill is
        // nearly the menu's own surface in some themes, so it is the fill
        // of what is chosen, and keys on it read on it.
        let lit_fill = theme.fill_selected();
        let lit_muted = theme.text_muted_on(lit_fill);
        let stroke_width = theme.icon_stroke(16.0);
        let mut open_row = None;
        let mut cursor_row = None;
        let mut picked = None;
        let layer = Layer::Popover.id(request.id.with(("sue-menu-popup", level)));
        Area::new(layer.id)
            .order(layer.order)
            .fixed_pos(origin)
            .constrain(false)
            .fade_in(false)
            .sense(Sense::hover())
            .show(ctx, |ui| {
                let painter = ui.painter().clone();
                painter.add(
                    theme
                        .elevation(Elevation::Floating)
                        .as_shape(area, CornerRadius::same(radius.round() as u8)),
                );
                painter.rect_filled(area, radius, surface);
                painter.rect_stroke(area, radius, theme.hairline(), StrokeKind::Inside);
                ui.allocate_rect(area, Sense::hover());

                if overflow > 0.0 {
                    // A finger drags the rows; the wheel rolls them.
                    let drag = ui.interact(
                        area,
                        request.id.with(("sue-menu-drag", level)),
                        Sense::drag(),
                    );
                    offset -= drag.drag_delta().y;
                    let over = pointer.at.is_some_and(|at| area.contains(at))
                        && pointer.at.and_then(|at| ctx.layer_id_at(at)) == Some(layer);
                    if over {
                        offset -=
                            ctx.input_mut(|input| std::mem::take(&mut input.smooth_scroll_delta.y));
                    }
                    offset = offset.clamp(0.0, overflow);
                    // The pointer resting on a strip scrolls toward it.
                    let speed = row_height * STRIP_ROWS_PER_SECOND;
                    let dt = ctx.input(|input| input.stable_dt).min(0.1);
                    let top_strip =
                        Rect::from_min_max(area.min, pos2(area.right(), area.top() + STRIP));
                    let bottom_strip =
                        Rect::from_min_max(pos2(area.left(), area.bottom() - STRIP), area.max);
                    if over && let Some(at) = pointer.at {
                        if offset > 0.0 && top_strip.contains(at) {
                            offset = (offset - speed * dt).max(0.0);
                            ctx.request_repaint();
                        } else if offset < overflow && bottom_strip.contains(at) {
                            offset = (offset + speed * dt).min(overflow);
                            ctx.request_repaint();
                        }
                    }
                }
                let more_above = offset > 0.5;
                let more_below = offset < overflow - 0.5;
                // The rows show between the strips.
                let view = Rect::from_min_max(
                    pos2(
                        area.left(),
                        area.top() + if more_above { STRIP } else { 0.0 },
                    ),
                    pos2(
                        area.right(),
                        area.bottom() - if more_below { STRIP } else { 0.0 },
                    ),
                );
                let rows = painter.with_clip_rect(view.intersect(painter.clip_rect()));
                let show_ring = self.by_key && focus::focus_visible(ctx);

                let mut y = area.top() + pad - offset;
                for (index, item) in items.iter().enumerate() {
                    if !selectable(item) {
                        rows.hline(
                            (area.left() + pad * 2.0)..=(area.right() - pad * 2.0),
                            y + GAP / 2.0,
                            theme.hairline(),
                        );
                        y += GAP;
                        continue;
                    }
                    let row = Rect::from_min_size(
                        pos2(area.left() + pad, y),
                        vec2(width - pad * 2.0, row_height),
                    );
                    y += row_height;
                    let is_sub = matches!(item, MenuItem::Sub { .. });
                    let is_open = is_sub && self.path.get(level) == Some(&index);
                    if is_open {
                        open_row = Some(row);
                    }
                    if !row.intersects(view) {
                        continue;
                    }
                    let item_state = state_of(item, request.state);
                    let response = ui.interact(
                        row.intersect(view),
                        request.id.with(("sue-menu-item", &own_path, index)),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        let role = if is_sub {
                            egui::Role::MenuItem
                        } else if item_state.checked.is_some() {
                            egui::Role::MenuItemCheckBox
                        } else {
                            egui::Role::MenuItem
                        };
                        match item_state.checked {
                            Some(checked) => WidgetInfo::selected(
                                role,
                                item_state.enabled,
                                checked,
                                item.label(),
                            ),
                            None => WidgetInfo::labeled(role, item_state.enabled, item.label()),
                        }
                    });

                    if response.hovered() && pointer.acts {
                        // On its way to the open nested menu, the pointer
                        // crosses other rows without choosing them.
                        let travelling = self.path.get(level).is_some_and(|open| *open != index)
                            && pointer.velocity.length() > TRAVEL_SPEED
                            && before.rects.get(level + 1).is_some_and(|nested| {
                                pointer.at.is_some_and(|at| {
                                    nested.intersects_ray(at, pointer.velocity.normalized())
                                })
                            });
                        if travelling {
                            ctx.request_repaint();
                        } else {
                            self.cursor = Some((level, index));
                            self.by_key = false;
                            if is_sub && item_state.enabled {
                                if self.path.get(level) != Some(&index) {
                                    self.open_nested(level, index);
                                    // Opened this frame: drawn this frame too,
                                    // or the loop of levels shuts it again.
                                    open_row = Some(row);
                                }
                            } else {
                                self.path.truncate(level);
                            }
                        }
                    }
                    if response.clicked() {
                        self.cursor = Some((level, index));
                        self.by_key = false;
                        match item {
                            MenuItem::Action { command, .. } if item_state.enabled => {
                                picked = Some(command.clone());
                            }
                            MenuItem::Sub { .. } if item_state.enabled => {
                                self.open_nested(level, index);
                                open_row = Some(row);
                            }
                            _ => {}
                        }
                    }

                    let is_cursor = self.cursor == Some((level, index));
                    let is_open = is_sub && self.path.get(level) == Some(&index);
                    if is_cursor {
                        cursor_row = Some(row);
                    }
                    let lit = (is_cursor || is_open) && item_state.enabled;
                    if lit {
                        rows.rect_filled(row, row_radius, lit_fill);
                    }
                    if is_cursor && show_ring {
                        // Inside the row, so that the rows beside it and
                        // the edge of the menu stay clear.
                        rows.rect_stroke(row, row_radius, theme.focus_stroke(), StrokeKind::Inside);
                    }
                    let color = if item_state.enabled {
                        theme.text
                    } else {
                        disabled
                    };
                    if item_state.checked == Some(true) {
                        let mark = row.left_center() + vec2(MARK / 2.0, 0.0);
                        rows.line(
                            vec![
                                mark + vec2(-4.0, 0.0),
                                mark + vec2(-1.0, 3.5),
                                mark + vec2(5.0, -4.0),
                            ],
                            Stroke::new(stroke_width, color),
                        );
                    }
                    rows.text(
                        row.left_center() + vec2(MARK, 0.0),
                        Align2::LEFT_CENTER,
                        item.label(),
                        font.clone(),
                        color,
                    );
                    match item {
                        MenuItem::Action { shortcut, .. } => {
                            if !shortcut.is_empty() && !touch {
                                rows.text(
                                    row.right_center() - vec2(10.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    shortcut,
                                    font.clone(),
                                    match (item_state.enabled, lit) {
                                        (false, _) => disabled,
                                        (true, true) => lit_muted,
                                        (true, false) => muted,
                                    },
                                );
                            }
                        }
                        MenuItem::Sub { .. } => {
                            let tip = row.right_center() - vec2(12.0, 0.0);
                            rows.line(
                                vec![tip + vec2(-3.5, -4.0), tip, tip + vec2(-3.5, 4.0)],
                                Stroke::new(stroke_width, color),
                            );
                        }
                        MenuItem::Separator => {}
                    }
                }

                // The strips: more rows this way.
                let strip = |up: bool| {
                    let rect = if up {
                        Rect::from_min_max(area.min, pos2(area.right(), area.top() + STRIP))
                    } else {
                        Rect::from_min_max(pos2(area.left(), area.bottom() - STRIP), area.max)
                    }
                    .shrink2(vec2(1.0, 0.0));
                    let round = (radius - 1.0).max(0.0).round() as u8;
                    let corners = if up {
                        CornerRadius {
                            nw: round,
                            ne: round,
                            sw: 0,
                            se: 0,
                        }
                    } else {
                        CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: round,
                            se: round,
                        }
                    };
                    let rect = if up {
                        rect.with_min_y(rect.top() + 1.0)
                    } else {
                        rect.with_max_y(rect.bottom() - 1.0)
                    };
                    painter.rect_filled(rect, corners, surface);
                    // Where the rows pass under the strip they fade out,
                    // so a row cut by the strip shows no slivers of text.
                    let edge = if up { rect.bottom() } else { rect.top() };
                    let far = if up { edge + FADE } else { edge - FADE };
                    let clear = egui::Color32::TRANSPARENT;
                    let mut fade = egui::Mesh::default();
                    fade.colored_vertex(pos2(rect.left(), edge), surface);
                    fade.colored_vertex(pos2(rect.right(), edge), surface);
                    fade.colored_vertex(pos2(rect.right(), far), clear);
                    fade.colored_vertex(pos2(rect.left(), far), clear);
                    fade.add_triangle(0, 1, 2);
                    fade.add_triangle(0, 2, 3);
                    painter.add(fade);
                    let middle = rect.center();
                    let lift = if up { -2.0 } else { 2.0 };
                    painter.line(
                        vec![
                            middle + vec2(-4.5, -lift),
                            middle + vec2(0.0, lift),
                            middle + vec2(4.5, -lift),
                        ],
                        Stroke::new(stroke_width, muted),
                    );
                };
                if more_above {
                    strip(true);
                }
                if more_below {
                    strip(false);
                }
            });
        if overflow > 0.0 {
            ctx.data_mut(|data| data.insert_temp(scroll_id, offset));
        }
        // A cursor row behind a strip is not in view.
        Drawn {
            rect: area,
            open_row,
            cursor_row,
            picked,
        }
    }
}

/// A right-click menu between frames, and where it was opened.
#[derive(Clone, Default)]
struct Shown {
    menu: MenuState,
    at: Pos2,
}

/// Where [`context_menu`] keeps the menu of the control `widget`.
fn context_id(widget: Id) -> Id {
    widget.with("sue-context-menu")
}

/// `true` when the control `widget` has a right-click menu from
/// [`context_menu`]: a long press on it opens that menu, not its tooltip.
pub(crate) fn has_context_menu(ctx: &Context, widget: Id) -> bool {
    ctx.data(|data| data.get_temp::<Shown>(context_id(widget)).is_some())
}

/// A right-click menu on the control behind `response`: it opens where the
/// control is clicked with the secondary button (or pressed and held on a
/// touch screen), with rows as tall as `ui` asks ([`Density::of`]).
/// Returns the command picked this frame.
pub fn context_menu(
    ui: &Ui,
    response: &Response,
    items: &[MenuItem],
    state: &dyn Fn(&str) -> ItemState,
    theme: &Theme,
) -> Option<String> {
    let ctx = ui.ctx();
    let id = context_id(response.id);
    let mut shown: Shown = ctx.data(|data| data.get_temp(id)).unwrap_or_default();
    if response.secondary_clicked() {
        shown.at = response
            .interact_pointer_pos()
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()))
            .unwrap_or_else(|| response.rect.left_bottom());
        shown.menu.open();
    }
    let event = shown.menu.show(
        ctx,
        &MenuRequest {
            id,
            anchor: shown.at,
            owner: Rect::NOTHING,
            items,
            state,
            density: Density::of(ui),
        },
        theme,
    );
    ctx.data_mut(|data| data.insert_temp(id, shown));
    match event {
        MenuEvent::Picked(command) => Some(command),
        _ => None,
    }
}

/// A floating surface under `anchor` (over it when there is no room
/// under) holding whatever `add_contents` draws: a picker, a list of
/// choices, a small form. It shows while `open` is true and sets it to
/// false on Escape and on a click outside it. Returns what `add_contents`
/// returned while it shows.
pub fn popover<R>(
    ctx: &Context,
    id: Id,
    anchor: Rect,
    open: &mut bool,
    theme: &Theme,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let layer = Layer::Popover.id(id);
    let pad = theme.space(Space::S4).round() as i8;
    let frame = egui::Frame::new()
        .fill(theme.surface(3))
        .stroke(theme.hairline())
        .corner_radius(theme.corner(Radius::Xl))
        .shadow(theme.elevation(Elevation::Floating))
        .inner_margin(egui::Margin::same(pad));
    egui::Popup::new(id, ctx.clone(), anchor, layer)
        .open_bool(open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .gap(theme.space(Space::S2))
        .frame(frame)
        .show(add_contents)
        .map(|shown| shown.inner)
}

#[cfg(test)]
mod tests {
    use egui::{MouseWheelUnit, PointerButton, RawInput, Vec2};

    use super::*;

    fn theme() -> Theme {
        Theme::from_rice(&sue_rice::RiceSchema::new())
    }

    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    fn text(text: &str) -> Event {
        Event::Text(text.to_owned())
    }

    fn click(at: Pos2, button: PointerButton) -> Vec<Vec<Event>> {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        vec![
            vec![Event::PointerMoved(at)],
            vec![press(true)],
            vec![press(false)],
        ]
    }

    /// Every piece of text a frame drew where it can be seen, with the
    /// part of its rect that its clip leaves.
    fn texts(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, clip: Rect, found: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    let rect = Rect::from_min_size(text.pos, text.galley.size()).intersect(clip);
                    if rect.is_positive() {
                        found.push((text.galley.text().to_owned(), rect));
                    }
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|shape| walk(shape, clip, found));
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, clipped.clip_rect, &mut found);
        }
        found
    }

    fn find(drawn: &[(String, Rect)], text: &str) -> Option<Rect> {
        drawn
            .iter()
            .find(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
    }

    /// Commands whose name starts with `off` cannot be used now.
    fn state(command: &str) -> ItemState {
        if command.starts_with("off") {
            ItemState::off()
        } else {
            ItemState::on()
        }
    }

    /// A menu shown in frames of its own: a window of `size`, a clock
    /// that moves 50 ms a frame, and what the last frame drew.
    struct Rig {
        ctx: Context,
        menu: MenuState,
        size: Vec2,
        anchor: Pos2,
        time: f64,
        drawn: Vec<(String, Rect)>,
        theme: Theme,
    }

    impl Rig {
        fn new(size: Vec2, anchor: Pos2) -> Self {
            let mut menu = MenuState::new();
            menu.open_at_once();
            Self {
                ctx: Context::default(),
                menu,
                size,
                anchor,
                time: 0.0,
                drawn: Vec::new(),
                theme: theme(),
            }
        }

        /// One frame with `events`: what the menu did.
        fn frame(&mut self, items: &[MenuItem], events: Vec<Event>) -> MenuEvent {
            self.time += 0.05;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut event = MenuEvent::Closed;
            let (menu, theme, anchor) = (&mut self.menu, &self.theme, self.anchor);
            let mut output = self.ctx.run_ui(input, |ui| {
                event = menu.show(
                    ui.ctx(),
                    &MenuRequest {
                        id: Id::unique("test-menu"),
                        anchor,
                        owner: Rect::NOTHING,
                        items,
                        state: &state,
                        density: Density::Compact,
                    },
                    theme,
                );
            });
            output.textures_delta.clear();
            self.drawn = texts(&output);
            event
        }

        /// Frames with `events` one after the other; what the last did.
        fn frames(&mut self, items: &[MenuItem], events: Vec<Vec<Event>>) -> MenuEvent {
            let mut last = MenuEvent::Open;
            for events in events {
                last = self.frame(items, events);
            }
            last
        }

        fn rect_of(&self, text: &str) -> Option<Rect> {
            find(&self.drawn, text)
        }

        fn shows(&self, text: &str) -> bool {
            self.rect_of(text).is_some()
        }
    }

    fn rows(labels: &[&str]) -> Vec<MenuItem> {
        labels
            .iter()
            .map(|label| match *label {
                "-" => MenuItem::Separator,
                label => MenuItem::action(label, label.to_lowercase()),
            })
            .collect()
    }

    #[test]
    fn steps_skip_lines_and_wrap() {
        let items = rows(&["-", "A", "-", "B", "-"]);
        assert_eq!(step(&items, None, true), Some(1));
        assert_eq!(step(&items, None, false), Some(3));
        assert_eq!(step(&items, Some(1), true), Some(3));
        assert_eq!(step(&items, Some(3), true), Some(1), "it wraps");
        assert_eq!(step(&items, Some(1), false), Some(3), "both ways");
        assert_eq!(step(&rows(&["-", "-"]), None, true), None);
        assert_eq!(step(&[], None, true), None);
    }

    #[test]
    fn typing_finds_rows_by_the_start_of_their_label() {
        let items = rows(&["Copy", "Cut", "-", "Paste", "Clear", "Paste Special"]);
        // One letter again and again goes from row to row.
        assert_eq!(type_ahead(&items, None, "c"), Some(0));
        assert_eq!(type_ahead(&items, Some(0), "c"), Some(1));
        assert_eq!(type_ahead(&items, Some(1), "cc"), Some(4));
        assert_eq!(type_ahead(&items, Some(4), "ccc"), Some(0), "it wraps");
        // A word stays on the row while it still fits.
        assert_eq!(type_ahead(&items, Some(3), "paste"), Some(3));
        assert_eq!(type_ahead(&items, Some(3), "paste s"), Some(5));
        assert_eq!(type_ahead(&items, Some(0), "cu"), Some(1));
        assert_eq!(type_ahead(&items, Some(0), "z"), None);
        assert_eq!(type_ahead(&items, None, ""), None);
    }

    #[test]
    fn the_keyboard_walks_picks_and_types_ahead() {
        let items = rows(&["Copy", "Cut", "-", "Paste", "off Crop"]);
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        assert_eq!(rig.frame(&items, Vec::new()), MenuEvent::Open);
        // The first frame is egui's sizing pass of a new area.
        rig.frame(&items, Vec::new());
        assert!(rig.shows("Copy") && rig.shows("off Crop"));
        assert_eq!(rig.menu.cursor(), None, "no row is chosen yet");
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        assert_eq!(rig.menu.cursor(), Some((0, 0)));
        rig.frame(&items, vec![key(Key::End)]);
        assert_eq!(
            rig.menu.cursor(),
            Some((0, 4)),
            "the keyboard rests on a row that cannot be used"
        );
        // Enter on it does nothing.
        assert_eq!(rig.frame(&items, vec![key(Key::Enter)]), MenuEvent::Open);
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        assert_eq!(rig.menu.cursor(), Some((0, 0)), "Down wraps");
        rig.frame(&items, vec![key(Key::ArrowUp)]);
        assert_eq!(rig.menu.cursor(), Some((0, 4)), "Up wraps");
        rig.frame(&items, vec![key(Key::Home)]);
        assert_eq!(rig.menu.cursor(), Some((0, 0)));
        // Typing goes to the row whose label starts so.
        rig.frame(&items, vec![text("p")]);
        assert_eq!(rig.menu.cursor(), Some((0, 3)));
        rig.frame(&items, vec![text("c")]);
        assert_eq!(rig.menu.cursor(), Some((0, 3)), "no row starts with pc");
        // A second later the word starts again.
        rig.time += 1.5;
        rig.frame(&items, vec![text("c")]);
        assert_eq!(
            rig.menu.cursor(),
            Some((0, 0)),
            "Crop has an off at the start"
        );
        rig.frame(&items, vec![text("c")]);
        assert_eq!(rig.menu.cursor(), Some((0, 1)));
        // Space picks, as Enter does.
        rig.time += 1.5;
        assert_eq!(
            rig.frame(&items, vec![key(Key::Space)]),
            MenuEvent::Picked("cut".to_owned())
        );
        assert!(!rig.menu.is_open());
        assert_eq!(rig.frame(&items, Vec::new()), MenuEvent::Closed);
        assert!(!rig.shows("Copy"), "a closed menu draws nothing");
    }

    #[test]
    fn right_and_left_go_into_nested_menus_and_out() {
        let items = vec![
            MenuItem::action("New", "new"),
            MenuItem::sub("Export", rows(&["PNG", "PSD"])),
            MenuItem::sub("Nothing Yet", rows(&["off One"])),
        ];
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frame(&items, Vec::new());
        // Right on a row with nothing nested asks for the next menu, Left
        // on the first level for the one before.
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        assert_eq!(
            rig.frame(&items, vec![key(Key::ArrowRight)]),
            MenuEvent::Next
        );
        assert_eq!(
            rig.frame(&items, vec![key(Key::ArrowLeft)]),
            MenuEvent::Previous
        );
        assert!(rig.menu.is_open());
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        rig.frame(&items, vec![key(Key::ArrowRight)]);
        assert_eq!(rig.menu.path(), [1]);
        assert_eq!(rig.menu.cursor(), Some((1, 0)));
        rig.frame(&items, Vec::new());
        let png = rig.rect_of("PNG").expect("the nested menu shows");
        let export = rig.rect_of("Export").unwrap();
        assert!(png.left() > export.right(), "{png:?} right of {export:?}");
        // Escape leaves the nested menu only.
        rig.frame(&items, vec![key(Key::Escape)]);
        assert_eq!(rig.menu.path(), [] as [usize; 0]);
        assert_eq!(rig.menu.cursor(), Some((0, 1)));
        assert!(rig.menu.is_open());
        rig.frame(&items, Vec::new());
        assert!(!rig.shows("PNG"));
        // Enter opens it too, and picks in it.
        rig.frame(&items, vec![key(Key::Enter)]);
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        assert_eq!(
            rig.frame(&items, vec![key(Key::Enter)]),
            MenuEvent::Picked("psd".to_owned())
        );
        // A nested menu with nothing usable in it does not open.
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frame(&items, vec![key(Key::End)]);
        assert_eq!(
            rig.frame(&items, vec![key(Key::ArrowRight)]),
            MenuEvent::Next
        );
        assert_eq!(rig.menu.path(), [] as [usize; 0]);
        // Escape on the first level closes.
        assert_eq!(rig.frame(&items, vec![key(Key::Escape)]), MenuEvent::Closed);
        assert!(!rig.menu.is_open());
    }

    #[test]
    fn the_pointer_on_a_row_opens_its_nested_menu() {
        let items = vec![
            MenuItem::action("Copy", "copy"),
            MenuItem::sub("Export", rows(&["PNG"])),
        ];
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frames(&items, vec![Vec::new(); 2]);
        let export = rig.rect_of("Export").unwrap().center();
        rig.frame(&items, vec![Event::PointerMoved(export)]);
        rig.frames(&items, vec![Vec::new(); 2]);
        assert_eq!(rig.menu.path(), [1]);
        assert!(rig.shows("PNG"), "the nested menu shows");
        // A click on the row keeps it open.
        rig.frames(&items, click(export, PointerButton::Primary));
        assert_eq!(rig.menu.path(), [1]);
        assert!(rig.shows("PNG"));
    }

    #[test]
    fn a_nested_menu_opens_to_the_left_when_there_is_no_room_at_the_right() {
        let items = vec![MenuItem::sub("Export", rows(&["PNG"]))];
        let mut rig = Rig::new(vec2(400.0, 300.0), pos2(250.0, 20.0));
        rig.frame(&items, vec![key(Key::ArrowDown)]);
        rig.frame(&items, vec![key(Key::ArrowRight)]);
        rig.frame(&items, Vec::new());
        let png = rig.rect_of("PNG").expect("the nested menu shows");
        let export = rig.rect_of("Export").unwrap();
        assert!(png.right() < export.left(), "{png:?} left of {export:?}");
        assert!(png.left() >= 0.0);
        // The first menu itself moved left to fit.
        assert!(export.left() < 250.0);
    }

    #[test]
    fn the_pointer_lights_picks_and_a_click_elsewhere_closes() {
        let items = rows(&["Copy", "off Crop", "Paste"]);
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frames(&items, vec![Vec::new(); 2]);
        let crop = rig.rect_of("off Crop").unwrap().center();
        rig.frame(&items, vec![Event::PointerMoved(crop)]);
        assert_eq!(rig.menu.cursor(), Some((0, 1)));
        // A click on a row that cannot be used does nothing.
        assert_eq!(
            rig.frames(&items, click(crop, PointerButton::Primary)),
            MenuEvent::Open
        );
        let paste = rig.rect_of("Paste").unwrap().center();
        assert_eq!(
            rig.frames(&items, click(paste, PointerButton::Primary)),
            MenuEvent::Picked("paste".to_owned())
        );
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frame(&items, Vec::new());
        assert_eq!(
            rig.frames(&items, click(pos2(700.0, 500.0), PointerButton::Primary)),
            MenuEvent::Closed
        );
        assert!(!rig.menu.is_open());
    }

    #[test]
    fn a_menu_that_fits_the_window_but_not_below_moves_up_whole() {
        // Opened low (a palette's menu button near the bottom), twelve
        // rows do not fit under it but fit the window: they show whole,
        // none behind a scroll.
        let labels: Vec<String> = (0..12).map(|row| format!("Row {row:02}")).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let items = rows(&labels);
        let size = vec2(600.0, 500.0);
        let mut rig = Rig::new(size, pos2(20.0, 260.0));
        rig.frames(&items, vec![Vec::new(), Vec::new()]);
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        for label in &labels {
            let rect = rig
                .rect_of(label)
                .unwrap_or_else(|| panic!("{label} shows"));
            assert!(screen.contains_rect(rect), "{label} at {rect:?}");
        }
        assert!(rig.rect_of("Row 00").unwrap().top() < 260.0, "it moved up");
    }

    #[test]
    fn a_menu_taller_than_the_window_scrolls() {
        let labels: Vec<String> = (0..40).map(|row| format!("Row {row:02}")).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let items = rows(&labels);
        let size = vec2(600.0, 300.0);
        let mut rig = Rig::new(size, pos2(20.0, 30.0));
        rig.frames(&items, vec![Vec::new(), Vec::new()]);
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let on_screen = |rig: &Rig| {
            rig.drawn
                .iter()
                .filter(|(text, _)| text.starts_with("Row"))
                .all(|(_, rect)| screen.contains_rect(*rect))
        };
        assert!(rig.shows("Row 00") && !rig.shows("Row 39"));
        assert!(on_screen(&rig), "nothing is cut off");
        // It keeps its place under what opened it.
        assert!(rig.rect_of("Row 00").unwrap().top() > 30.0);
        // End scrolls to the last row; Home back to the first.
        rig.frame(&items, vec![key(Key::End)]);
        rig.frame(&items, Vec::new());
        assert!(rig.shows("Row 39") && !rig.shows("Row 00"));
        assert!(on_screen(&rig));
        rig.frame(&items, vec![key(Key::Home)]);
        rig.frame(&items, Vec::new());
        assert!(rig.shows("Row 00") && !rig.shows("Row 39"));
        // Down past the bottom row keeps the keyboard's row in view.
        for _ in 0..14 {
            rig.frame(&items, vec![key(Key::ArrowDown)]);
        }
        rig.frame(&items, Vec::new());
        assert_eq!(rig.menu.cursor(), Some((0, 14)));
        assert!(rig.shows("Row 14") && !rig.shows("Row 00"));
        assert!(on_screen(&rig));
        // The wheel rolls the rows.
        rig.frame(&items, vec![key(Key::Home)]);
        let middle = rig.rect_of("Row 03").unwrap().center();
        rig.frame(&items, vec![Event::PointerMoved(middle)]);
        rig.frame(
            &items,
            vec![Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, -400.0),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            }],
        );
        rig.frames(&items, vec![Vec::new(); 20]);
        assert!(!rig.shows("Row 00"), "the wheel scrolled");
        // Resting the pointer on the top strip scrolls back up.
        let top = rig
            .drawn
            .iter()
            .filter(|(text, _)| text.starts_with("Row"))
            .map(|(_, rect)| rect.top())
            .fold(f32::MAX, f32::min);
        let strip = pos2(middle.x, top - 10.0);
        rig.frame(&items, vec![Event::PointerMoved(strip)]);
        rig.frames(&items, vec![Vec::new(); 60]);
        assert!(rig.shows("Row 00"), "the strip scrolled back to the top");
    }

    #[test]
    fn the_note_of_the_row_the_keyboard_rests_on_shows_after_a_moment() {
        let items = vec![
            MenuItem::action("Copy", "copy"),
            MenuItem::action("Crop", "off.crop").note("Needs a selection"),
        ];
        let mut rig = Rig::new(vec2(800.0, 600.0), pos2(20.0, 20.0));
        rig.frame(&items, vec![key(Key::End)]);
        assert!(!rig.shows("Needs a selection"), "not at once");
        rig.frames(&items, vec![Vec::new(); 14]);
        let note = rig.rect_of("Needs a selection").expect("the note shows");
        let crop = rig.rect_of("Crop").unwrap();
        assert!(note.left() > crop.right(), "beside the menu");
        // Away from the row, it goes.
        rig.frame(&items, vec![key(Key::Home)]);
        rig.frames(&items, vec![Vec::new(); 2]);
        assert!(!rig.shows("Needs a selection"));
    }

    #[test]
    fn the_keyboard_goes_back_where_it_was_when_the_menu_closes() {
        let ctx = Context::default();
        let theme = theme();
        let items = rows(&["Copy"]);
        let field = Id::unique("field");
        let mut menu = MenuState::new();
        let mut typed = String::new();
        let mut run = |menu: &mut MenuState, events: Vec<Event>| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.add(egui::TextEdit::singleline(&mut typed).id(field));
                menu.show(
                    ui.ctx(),
                    &MenuRequest {
                        id: Id::unique("menu"),
                        anchor: pos2(20.0, 200.0),
                        owner: Rect::NOTHING,
                        items: &items,
                        state: &state,
                        density: Density::Compact,
                    },
                    &theme,
                );
            });
            output.textures_delta.clear();
        };
        run(&mut menu, Vec::new());
        ctx.memory_mut(|memory| memory.request_focus(field));
        run(&mut menu, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(field)));
        menu.open();
        run(&mut menu, Vec::new());
        assert_eq!(ctx.memory(egui::Memory::focused), None, "the menu holds it");
        // Typing goes to the menu, not to the field.
        run(&mut menu, vec![text("c")]);
        assert_eq!(menu.cursor(), Some((0, 0)));
        run(&mut menu, vec![key(Key::Escape)]);
        assert!(!menu.is_open());
        run(&mut menu, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(field)), "it is back");
        // A menu closed from outside hands it back on its next frame.
        menu.open();
        run(&mut menu, Vec::new());
        assert_eq!(ctx.memory(egui::Memory::focused), None);
        menu.close();
        run(&mut menu, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(field)));
        drop(run);
        assert!(typed.is_empty(), "the field got no letter");
    }

    /// One frame of a control with a right-click menu; what was picked.
    fn context_frame(
        ctx: &Context,
        events: Vec<Event>,
        items: &[MenuItem],
        drawn: &mut Vec<(String, Rect)>,
    ) -> Option<String> {
        let theme = theme();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut picked = None;
        let mut output = ctx.run_ui(input, |ui| {
            let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 100.0));
            let response = ui.interact(rect, Id::unique("control"), Sense::click());
            picked = context_menu(ui, &response, items, &state, &theme);
        });
        output.textures_delta.clear();
        *drawn = texts(&output);
        picked
    }

    #[test]
    fn a_right_click_opens_a_menu_where_it_was_clicked() {
        let ctx = Context::default();
        let items = rows(&["Copy", "Paste"]);
        let mut drawn = Vec::new();
        context_frame(&ctx, Vec::new(), &items, &mut drawn);
        assert!(find(&drawn, "Copy").is_none(), "closed at first");
        let at = pos2(150.0, 140.0);
        for events in click(at, PointerButton::Secondary) {
            context_frame(&ctx, events, &items, &mut drawn);
        }
        for _ in 0..2 {
            context_frame(&ctx, Vec::new(), &items, &mut drawn);
        }
        let copy = find(&drawn, "Copy").expect("the menu is open");
        assert!(
            copy.top() > at.y && (copy.left() - at.x).abs() < 40.0,
            "{copy:?} at {at:?}"
        );
        context_frame(&ctx, vec![key(Key::ArrowDown)], &items, &mut drawn);
        context_frame(&ctx, vec![key(Key::ArrowDown)], &items, &mut drawn);
        let picked = context_frame(&ctx, vec![key(Key::Enter)], &items, &mut drawn);
        assert_eq!(picked.as_deref(), Some("paste"));
        context_frame(&ctx, Vec::new(), &items, &mut drawn);
        assert!(find(&drawn, "Copy").is_none(), "it closed");
        // The control is known to have a menu, so a long press on it is
        // not taken for its tooltip.
        assert!(has_context_menu(&ctx, Id::unique("control")));
        assert!(!has_context_menu(&ctx, Id::unique("other")));
    }

    #[test]
    fn a_popover_shows_until_escape() {
        let ctx = Context::default();
        let theme = theme();
        let mut open = true;
        let run = |open: &mut bool, events: Vec<Event>| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut shown = None;
            let mut output = ctx.run_ui(input, |ui| {
                let anchor = Rect::from_min_size(pos2(100.0, 100.0), vec2(80.0, 24.0));
                shown = popover(ui.ctx(), Id::unique("pop"), anchor, open, &theme, |ui| {
                    ui.label("Inside");
                    7
                });
            });
            let drawn = texts(&output);
            output.textures_delta.clear();
            (shown, drawn)
        };
        let (shown, _) = run(&mut open, Vec::new());
        assert_eq!(shown, Some(7));
        let (_, drawn) = run(&mut open, Vec::new());
        let inside = find(&drawn, "Inside").expect("it shows");
        assert!(inside.top() > 124.0, "under its anchor: {inside:?}");
        run(&mut open, vec![key(Key::Escape)]);
        assert!(!open);
        let (shown, _) = run(&mut open, Vec::new());
        assert_eq!(shown, None);
    }
}
