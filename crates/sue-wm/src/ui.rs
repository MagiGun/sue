//! An immediate-mode widget toolkit over [`Canvas`]: widgets are declared
//! every frame, hit-tested against the pointer, and drawn in one pass.
//!
//! Pop-ups (menus, drop-downs) and tooltips draw on the overlay layer, so
//! they sit above everything and win hit tests. While a pop-up is open the
//! base layer ignores the pointer; a press outside the pop-up closes it.

use sue_platform::{
    CursorShape, FileDrop, InputEvent, Key, KeyEvent, MouseButton, MouseEvent, NamedKey, Point,
};

use crate::canvas::{Layer, intersect};
use crate::{BoxStyle, Canvas, Color, FontFace, Fonts, Rect, Theme};

/// Identifies a widget across frames.
pub type WidgetId = u64;

/// A stable id from a scope and a key (FNV-1a).
#[must_use]
pub fn widget_id(scope: &str, key: &str) -> WidgetId {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in scope.bytes().chain([0xFF]).chain(key.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn contains(rect: Rect, point: Point) -> bool {
    rect.contains(point.x, point.y)
}

/// Pointer and drag state accumulated between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiInput {
    /// Pointer position, or `None` outside the window.
    pub pointer: Option<Point>,
    /// The primary button is held.
    pub down: bool,
    /// The primary button went down since the last frame.
    pub pressed: bool,
    /// The primary button went up since the last frame.
    pub released: bool,
    /// The secondary button went down since the last frame.
    pub secondary_pressed: bool,
    /// Wheel movement since the last frame, in logical pixels.
    pub wheel: Point,
    /// A file from the OS hovers over the window.
    pub file_hover: bool,
}

impl UiInput {
    /// Folds one event in. Returns `true` if it can change what is drawn.
    pub fn apply(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::Mouse(MouseEvent::Moved { position }) => {
                self.pointer = Some(*position);
                true
            }
            InputEvent::Mouse(MouseEvent::Left) => {
                self.pointer = None;
                true
            }
            InputEvent::Mouse(MouseEvent::Button {
                button,
                pressed,
                position,
            }) => {
                self.pointer = Some(*position);
                match (button, pressed) {
                    (MouseButton::Left, true) => {
                        self.down = true;
                        self.pressed = true;
                    }
                    (MouseButton::Left, false) => {
                        self.down = false;
                        self.released = true;
                    }
                    (MouseButton::Right, true) => self.secondary_pressed = true,
                    _ => return false,
                }
                true
            }
            InputEvent::Mouse(MouseEvent::Wheel { delta_x, delta_y }) => {
                self.wheel.x += delta_x;
                self.wheel.y += delta_y;
                true
            }
            InputEvent::FileDrop(FileDrop::Hovered(_)) => {
                self.file_hover = true;
                true
            }
            InputEvent::FileDrop(FileDrop::Cancelled | FileDrop::Dropped(_)) => {
                self.file_hover = false;
                true
            }
            _ => false,
        }
    }

    /// Clears the one-frame edges (press, release, wheel).
    pub fn end_frame(&mut self) {
        self.pressed = false;
        self.released = false;
        self.secondary_pressed = false;
        self.wheel = Point::default();
    }
}

/// Widget state that survives between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiState {
    /// Widget under the pointer this frame.
    pub hot: Option<WidgetId>,
    /// Widget being pressed or dragged.
    pub active: Option<WidgetId>,
    /// Where the active press started.
    pub press_origin: Option<Point>,
    /// Pointer shape requested this frame.
    pub cursor: Option<CursorShape>,
    /// The open pop-up, and the widget that opened it.
    pub popup: Option<(WidgetId, WidgetId)>,
    /// A tooltip to show this frame: text and the rect it explains.
    pub tooltip: Option<(String, Rect)>,
    /// `true` while carets are in the visible half of their blink.
    pub caret_visible: bool,
    /// Set when any widget drew a caret this frame (so it blinks).
    pub caret_drawn: bool,
    hits: [Vec<(Rect, WidgetId)>; 2],
    previous_hits: Vec<(Rect, WidgetId)>,
    popup_area: Vec<Rect>,
}

impl UiState {
    /// Starts a frame: last frame's hit regions decide which widget is on top.
    pub fn begin_frame(&mut self) {
        let [base, overlay] = &mut self.hits;
        self.previous_hits = std::mem::take(base);
        self.previous_hits.append(overlay);
        self.hot = None;
        self.cursor = None;
        self.tooltip = None;
        self.caret_drawn = false;
        self.popup_area.clear();
    }

    /// Ends a frame: a release always ends any press, and a press outside an
    /// open pop-up closes it.
    pub fn end_frame(&mut self, input: &UiInput) {
        if input.pressed
            && self.popup.is_some()
            && input
                .pointer
                .is_none_or(|point| !self.popup_area.iter().any(|rect| contains(*rect, point)))
        {
            self.popup = None;
        }
        if input.released || !input.down {
            self.active = None;
            self.press_origin = None;
        }
    }

    /// The topmost widget under `point` in the last frame.
    #[must_use]
    pub fn hit_at(&self, point: Point) -> Option<WidgetId> {
        self.previous_hits
            .iter()
            .rev()
            .find(|(rect, _)| contains(*rect, point))
            .map(|&(_, id)| id)
    }

    /// Where widget `id` was drawn in the latest frame (its visible part).
    #[must_use]
    pub fn hit_rect(&self, id: WidgetId) -> Option<Rect> {
        self.hits[1]
            .iter()
            .rev()
            .chain(self.hits[0].iter().rev())
            .chain(self.previous_hits.iter().rev())
            .find(|(_, hit)| *hit == id)
            .map(|&(rect, _)| rect)
    }

    /// `true` if moving the pointer to `point` changes the hovered widget, so
    /// plain pointer motion only redraws when something visibly reacts.
    #[must_use]
    pub fn hover_changes(&self, point: Option<Point>) -> bool {
        self.active.is_some() || point.and_then(|point| self.hit_at(point)) != self.hot
    }
}

/// How a widget reacted this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    /// The pointer is over the widget.
    pub hovered: bool,
    /// The press started on the widget this frame.
    pub pressed: bool,
    /// The press ended on the widget this frame (a click).
    pub clicked: bool,
    /// The widget is held and the pointer may be moving.
    pub dragging: bool,
    /// The press on this widget ended this frame (anywhere).
    pub released: bool,
}

/// What a slider did this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderEvent {
    /// Nothing.
    Idle,
    /// Dragging: show this value live, but do not record it yet.
    Preview(f64),
    /// Released: record this value.
    Commit(f64),
}

/// One option of a segmented control or drop-down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiChoice {
    /// What the user sees.
    pub label: String,
    /// `false` greys it out.
    pub enabled: bool,
}

impl UiChoice {
    /// An enabled option.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
        }
    }
}

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// A clickable item.
    Item {
        /// Text.
        label: String,
        /// Shortcut shown on the right, e.g. `Ctrl+O`.
        shortcut: Option<String>,
        /// `false` greys it out.
        enabled: bool,
    },
    /// A hairline between groups.
    Separator,
}

impl MenuEntry {
    /// An enabled item.
    pub fn item(label: impl Into<String>, shortcut: Option<String>) -> Self {
        Self::Item {
            label: label.into(),
            shortcut,
            enabled: true,
        }
    }

    /// The same entry, enabled or not.
    #[must_use]
    pub fn enabled(self, enabled: bool) -> Self {
        match self {
            Self::Item {
                label, shortcut, ..
            } => Self::Item {
                label,
                shortcut,
                enabled,
            },
            Self::Separator => Self::Separator,
        }
    }
}

/// What a key did to a [`LineEdit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    /// The text changed.
    Changed,
    /// Only the caret moved.
    Moved,
    /// Enter.
    Submit,
    /// Escape.
    Cancel,
    /// Not an editing key.
    Ignored,
}

/// One line of editable text with a caret (a byte index).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineEdit {
    /// The text.
    pub text: String,
    /// Caret byte index (always on a character boundary).
    pub caret: usize,
}

impl LineEdit {
    /// Editing `text`, caret at the end.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            caret: text.len(),
            text,
        }
    }

    /// Applies one key press.
    pub fn handle(&mut self, key: &KeyEvent) -> EditOutcome {
        if !key.pressed {
            return EditOutcome::Ignored;
        }
        self.caret = self.caret.min(self.text.len());
        let previous = self.text[..self.caret]
            .char_indices()
            .next_back()
            .map(|(index, _)| index);
        let next = self.text[self.caret..]
            .chars()
            .next()
            .map(|ch| self.caret + ch.len_utf8());
        match key.key {
            Key::Named(NamedKey::Enter) => EditOutcome::Submit,
            Key::Named(NamedKey::Escape) => EditOutcome::Cancel,
            Key::Named(NamedKey::Backspace) => match previous {
                Some(start) => {
                    self.text.replace_range(start..self.caret, "");
                    self.caret = start;
                    EditOutcome::Changed
                }
                None => EditOutcome::Moved,
            },
            Key::Named(NamedKey::Delete) => match next {
                Some(end) => {
                    self.text.replace_range(self.caret..end, "");
                    EditOutcome::Changed
                }
                None => EditOutcome::Moved,
            },
            Key::Named(NamedKey::ArrowLeft) => {
                self.caret = previous.unwrap_or(0);
                EditOutcome::Moved
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.caret = next.unwrap_or(self.text.len());
                EditOutcome::Moved
            }
            Key::Named(NamedKey::Home) => {
                self.caret = 0;
                EditOutcome::Moved
            }
            Key::Named(NamedKey::End) => {
                self.caret = self.text.len();
                EditOutcome::Moved
            }
            _ => match &key.text {
                Some(typed)
                    if !key.modifiers.is_command()
                        && !typed.is_empty()
                        && !typed.chars().any(char::is_control) =>
                {
                    self.text.insert_str(self.caret, typed);
                    self.caret += typed.len();
                    EditOutcome::Changed
                }
                _ => EditOutcome::Ignored,
            },
        }
    }
}

/// The per-frame widget builder.
pub struct Ui<'a> {
    /// Where widgets draw.
    pub canvas: &'a mut Canvas,
    /// Fonts for measuring and drawing text.
    pub fonts: &'a Fonts,
    /// Colors and sizes.
    pub theme: &'a Theme,
    /// This frame's pointer input.
    pub input: &'a UiInput,
    /// State kept between frames.
    pub state: &'a mut UiState,
}

impl Ui<'_> {
    /// Hit-tests a widget. Only the topmost widget under the pointer (by last
    /// frame's drawing order) and nothing while another widget is held reacts.
    /// While a pop-up is open, only overlay widgets and its anchor react.
    pub fn interact(&mut self, id: WidgetId, rect: Rect) -> Response {
        let clip = self.canvas.clip();
        let visible = intersect(rect, clip);
        let layer = self.canvas.layer();
        let slot = usize::from(layer == Layer::Overlay);
        self.state.hits[slot].push((visible, id));
        let inside = self
            .input
            .pointer
            .is_some_and(|point| contains(visible, point));
        let on_top = self
            .input
            .pointer
            .and_then(|point| self.state.hit_at(point))
            .is_none_or(|top| top == id);
        let free = self.state.active.is_none_or(|active| active == id);
        let modal = match self.state.popup {
            Some((_, anchor)) => layer == Layer::Base && anchor != id,
            None => false,
        };
        let hovered = inside && on_top && free && !modal;
        if hovered {
            self.state.hot = Some(id);
        }
        let mut response = Response {
            hovered,
            ..Response::default()
        };
        if hovered && self.input.pressed {
            self.state.active = Some(id);
            self.state.press_origin = self.input.pointer;
            response.pressed = true;
        }
        let active = self.state.active == Some(id);
        if active && self.input.released {
            response.released = true;
            response.clicked = inside;
            self.state.active = None;
        }
        response.dragging = active && self.input.down;
        response
    }

    /// Requests a pointer shape for this frame.
    pub fn set_cursor(&mut self, shape: CursorShape) {
        self.state.cursor = Some(shape);
    }

    /// Shows `text` as a tooltip for `rect` while `response` is hovered.
    pub fn tooltip(&mut self, response: Response, rect: Rect, text: &str) {
        if response.hovered && !text.is_empty() {
            self.state.tooltip = Some((text.to_owned(), rect));
        }
    }

    /// Height of one UI text line.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.fonts.line_height(FontFace::Ui, self.theme.ui_size)
    }

    /// Height of one caption line.
    #[must_use]
    pub fn small_line_height(&self) -> f32 {
        self.fonts.line_height(FontFace::Ui, self.theme.small_size)
    }

    /// Height of a standard control row.
    #[must_use]
    pub fn row_height(&self) -> f32 {
        self.line_height() + self.theme.padding
    }

    /// Wheel movement over `rect` this frame (vertical).
    #[must_use]
    pub fn wheel_in(&self, rect: Rect) -> f32 {
        let clip = intersect(rect, self.canvas.clip());
        let modal = self.state.popup.is_some() && self.canvas.layer() == Layer::Base;
        match self.input.pointer {
            Some(point) if contains(clip, point) && !modal => self.input.wheel.y,
            _ => 0.0,
        }
    }

    /// A structural panel: flat fill, the panel radius (0 for flush docks).
    pub fn panel(&mut self, rect: Rect) {
        let theme = self.theme;
        self.canvas
            .draw_box(rect, BoxStyle::filled(theme.panel, theme.panel_radius));
    }

    /// A flat rectangle of `color`.
    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.canvas.draw_box(rect, BoxStyle::filled(color, 0.0));
    }

    /// A horizontal hairline at `y` from `x` over `width`.
    pub fn hline(&mut self, x: f32, y: f32, width: f32) {
        let line = self.theme.hairline();
        if line > 0.0 {
            let color = self.theme.border;
            self.fill(Rect::new(x, y, width, line), color);
        }
    }

    /// A vertical hairline at `x` from `y` over `height`.
    pub fn vline(&mut self, x: f32, y: f32, height: f32) {
        let line = self.theme.hairline();
        if line > 0.0 {
            let color = self.theme.border;
            self.fill(Rect::new(x, y, line, height), color);
        }
    }

    /// One line of text at `position` (top-left of the line box).
    pub fn text(&mut self, position: Point, text: &str, face: FontFace, size: f32, color: Color) {
        self.canvas.draw_text(position, text, face, size, color);
    }

    /// UI text, vertically centered in `rect`, elided to fit.
    pub fn label(&mut self, rect: Rect, text: &str, color: Color) {
        let size = self.theme.ui_size;
        self.label_sized(rect, text, size, color);
    }

    /// Text of `size`, vertically centered in `rect`, elided to fit.
    pub fn label_sized(&mut self, rect: Rect, text: &str, size: f32, color: Color) {
        let height = self.fonts.line_height(FontFace::Ui, size);
        let shown = self.fonts.elide(FontFace::Ui, size, text, rect.width);
        let y = rect.y + (rect.height - height) / 2.0;
        self.canvas
            .draw_text(Point::new(rect.x, y), &shown, FontFace::Ui, size, color);
    }

    /// UI text right-aligned in `rect`.
    pub fn label_right(&mut self, rect: Rect, text: &str, size: f32, color: Color) {
        let width = self.fonts.measure(FontFace::Ui, size, text);
        let shifted = Rect::new(
            (rect.right() - width).max(rect.x),
            rect.y,
            width.min(rect.width),
            rect.height,
        );
        self.label_sized(shifted, text, size, color);
    }

    /// UI text centered in `rect`.
    pub fn label_centered(&mut self, rect: Rect, text: &str, color: Color) {
        let size = self.theme.ui_size;
        self.label_centered_sized(rect, text, size, color);
    }

    /// Text of `size` centered in `rect`.
    pub fn label_centered_sized(&mut self, rect: Rect, text: &str, size: f32, color: Color) {
        let shown = self.fonts.elide(FontFace::Ui, size, text, rect.width);
        let width = self.fonts.measure(FontFace::Ui, size, &shown);
        let height = self.fonts.line_height(FontFace::Ui, size);
        let position = Point::new(
            rect.x + (rect.width - width) / 2.0,
            rect.y + (rect.height - height) / 2.0,
        );
        self.canvas
            .draw_text(position, &shown, FontFace::Ui, size, color);
    }

    /// Height `text` takes when wrapped to `width` at `size`.
    #[must_use]
    pub fn wrapped_height(&self, text: &str, width: f32, size: f32) -> f32 {
        let lines = self
            .fonts
            .wrap(FontFace::Ui, size, text, width.max(1.0))
            .len();
        lines as f32 * self.fonts.line_height(FontFace::Ui, size)
    }

    /// Draws `text` wrapped to `width` with its top at `(x, y)`; never cut.
    /// Returns the height used.
    pub fn wrapped(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        text: &str,
        size: f32,
        color: Color,
    ) -> f32 {
        let line = self.fonts.line_height(FontFace::Ui, size);
        let ranges = self.fonts.wrap(FontFace::Ui, size, text, width.max(1.0));
        for (row, range) in ranges.iter().enumerate() {
            self.canvas.draw_text(
                Point::new(x, y + row as f32 * line),
                &text[range.clone()],
                FontFace::Ui,
                size,
                color,
            );
        }
        ranges.len() as f32 * line
    }

    /// Draws a caret in the visible half of its blink.
    pub fn caret(&mut self, rect: Rect) {
        self.state.caret_drawn = true;
        if self.state.caret_visible {
            let color = self.theme.accent;
            self.fill(rect, color);
        }
    }

    /// A push button. Returns `true` when clicked.
    pub fn button(&mut self, id: WidgetId, rect: Rect, text: &str) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let fill = if response.dragging {
            theme.hover.mix(theme.accent, 0.25)
        } else if response.hovered {
            theme.hover
        } else {
            theme.surface
        };
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill,
                border: theme.border,
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        self.label_centered(rect, text, theme.text);
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// A button in the accent color, for the main action of a panel.
    pub fn primary_button(&mut self, id: WidgetId, rect: Rect, text: &str, enabled: bool) -> bool {
        let response = if enabled {
            self.interact(id, rect)
        } else {
            Response::default()
        };
        let theme = self.theme;
        let fill = if !enabled {
            theme.surface
        } else if response.hovered {
            theme.accent.mix(theme.on_accent, 0.12)
        } else {
            theme.accent
        };
        self.canvas
            .draw_box(rect, BoxStyle::filled(fill, theme.radius));
        let color = if enabled {
            theme.on_accent
        } else {
            theme.muted
        };
        self.label_centered(rect, text, color);
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// A flat text button, as in a menu bar or a link. `active` keeps it lit.
    pub fn flat_button(&mut self, id: WidgetId, rect: Rect, text: &str, active: bool) -> Response {
        let response = self.interact(id, rect);
        let theme = self.theme;
        if active || response.hovered {
            let fill = if active { theme.surface } else { theme.hover };
            self.canvas
                .draw_box(rect, BoxStyle::filled(fill, theme.radius));
        }
        let color = if active || response.hovered {
            theme.text
        } else {
            theme.muted
        };
        self.label_centered(rect, text, color);
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response
    }

    /// An icon button: a glyph centered in `rect`, lit when `active`.
    pub fn icon_button(
        &mut self,
        id: WidgetId,
        rect: Rect,
        glyph: &str,
        active: bool,
        tooltip: &str,
    ) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        if response.hovered {
            self.canvas.draw_box(
                rect.shrink(2.0),
                BoxStyle::filled(theme.hover, theme.radius),
            );
            self.set_cursor(CursorShape::Pointer);
        }
        let color = if active || response.hovered {
            theme.text
        } else {
            theme.muted
        };
        let size = theme.ui_size * 1.2;
        self.label_centered_sized(rect, glyph, size, color);
        self.tooltip(response, rect, tooltip);
        response.clicked
    }

    /// An on/off switch. Returns `true` when clicked.
    pub fn toggle(&mut self, id: WidgetId, rect: Rect, on: bool) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let height = (self.line_height() * 0.95).min(rect.height).round();
        let track = Rect::new(
            rect.x,
            rect.y + ((rect.height - height) / 2.0).round(),
            (height * 1.8).round(),
            height,
        );
        let fill = if on {
            theme.accent
        } else if response.hovered {
            theme.hover
        } else {
            theme.surface
        };
        self.canvas.draw_box(
            track,
            BoxStyle {
                fill,
                border: if on { theme.accent } else { theme.border },
                border_width: theme.hairline(),
                radius: height / 2.0,
            },
        );
        let knob = height - 6.0;
        let knob_x = if on {
            track.right() - knob - 3.0
        } else {
            track.x + 3.0
        };
        let knob_color = if on { theme.on_accent } else { theme.muted };
        self.canvas.draw_box(
            Rect::new(knob_x, track.y + 3.0, knob, knob),
            BoxStyle::filled(knob_color, knob / 2.0),
        );
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// A horizontal slider over `min..=max` with a slim track, an accent
    /// fill and a compact thumb. Dragging previews; releasing commits, so
    /// one drag is one undo step.
    pub fn slider(
        &mut self,
        id: WidgetId,
        rect: Rect,
        value: f64,
        min: f64,
        max: f64,
    ) -> SliderEvent {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let thumb = (theme.slider_track * 3.0).clamp(8.0, rect.height);
        let inner = Rect::new(
            rect.x + thumb / 2.0,
            rect.y,
            (rect.width - thumb).max(1.0),
            rect.height,
        );
        let span = (max - min).max(f64::EPSILON);
        let to_value =
            |x: f32| min + f64::from(((x - inner.x) / inner.width).clamp(0.0, 1.0)) * span;
        let pointer_value = self.input.pointer.map(|point| to_value(point.x));
        let shown = if response.dragging || response.released {
            pointer_value.unwrap_or(value)
        } else {
            value
        };
        let fraction = (((shown - min) / span).clamp(0.0, 1.0)) as f32;
        let track_height = theme.slider_track.min(rect.height);
        let track = Rect::new(
            inner.x,
            (rect.y + (rect.height - track_height) / 2.0).round(),
            inner.width,
            track_height,
        );
        self.canvas
            .draw_box(track, BoxStyle::filled(theme.surface, track_height / 2.0));
        self.canvas.draw_box(
            Rect::new(track.x, track.y, track.width * fraction, track.height),
            BoxStyle::filled(theme.accent, track_height / 2.0),
        );
        let lit = response.hovered || response.dragging;
        let knob = if lit { thumb + 2.0 } else { thumb };
        let center_x = inner.x + inner.width * fraction;
        self.canvas.draw_box(
            Rect::new(
                center_x - knob / 2.0,
                rect.y + (rect.height - knob) / 2.0,
                knob,
                knob,
            ),
            BoxStyle {
                fill: if lit { theme.accent } else { theme.text },
                border: theme.base,
                border_width: theme.hairline(),
                radius: knob / 2.0,
            },
        );
        if lit {
            self.set_cursor(CursorShape::ResizeHorizontal);
        }
        if response.released {
            SliderEvent::Commit(shown)
        } else if response.dragging {
            SliderEvent::Preview(shown)
        } else {
            SliderEvent::Idle
        }
    }

    /// A color swatch. Returns `true` when clicked.
    pub fn swatch(&mut self, id: WidgetId, rect: Rect, color: Color, selected: bool) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let border = if selected || response.hovered {
            theme.accent
        } else {
            theme.border
        };
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: color,
                border,
                border_width: if selected { 2.0 } else { theme.hairline() },
                radius: theme.radius,
            },
        );
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// The width a segmented control needs for `options` (`None` if it
    /// cannot fit `max_width`; use a drop-down then).
    #[must_use]
    pub fn segmented_width(&self, options: &[UiChoice], max_width: f32) -> Option<f32> {
        let pad = self.theme.padding;
        let width: f32 = options
            .iter()
            .map(|option| {
                self.fonts
                    .measure(FontFace::Ui, self.theme.ui_size, &option.label)
                    + pad * 2.0
            })
            .sum();
        (width <= max_width).then_some(width)
    }

    /// A row of mutually exclusive buttons. Returns the clicked index.
    pub fn segmented(
        &mut self,
        id: WidgetId,
        rect: Rect,
        options: &[UiChoice],
        selected: Option<usize>,
    ) -> Option<usize> {
        let theme = self.theme;
        let pad = theme.padding;
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.surface,
                border: theme.border,
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        let widths: Vec<f32> = options
            .iter()
            .map(|option| {
                self.fonts
                    .measure(FontFace::Ui, theme.ui_size, &option.label)
                    + pad * 2.0
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>().max(1.0);
        let stretch = rect.width / total;
        let mut x = rect.x;
        let mut picked = None;
        for (index, (option, width)) in options.iter().zip(widths).enumerate() {
            let cell = Rect::new(x, rect.y, width * stretch, rect.height);
            x = cell.right();
            let response = if option.enabled {
                self.interact(id.wrapping_add(index as u64 + 1), cell)
            } else {
                Response::default()
            };
            let chosen = selected == Some(index);
            if chosen {
                self.canvas.draw_box(
                    cell.shrink(2.0),
                    BoxStyle::filled(theme.accent, theme.radius),
                );
            } else if response.hovered {
                self.canvas.draw_box(
                    cell.shrink(2.0),
                    BoxStyle::filled(theme.hover, theme.radius),
                );
            }
            let color = if chosen {
                theme.on_accent
            } else if option.enabled {
                theme.text
            } else {
                theme.muted.fade(0.6)
            };
            self.label_centered(cell, &option.label, color);
            if response.hovered {
                self.set_cursor(CursorShape::Pointer);
            }
            if response.clicked && !chosen {
                picked = Some(index);
            }
        }
        picked
    }

    /// A compact drop-down. Closed it shows the current choice; clicked, it
    /// opens a list on the overlay. Returns the picked index.
    pub fn dropdown(
        &mut self,
        id: WidgetId,
        rect: Rect,
        options: &[UiChoice],
        selected: Option<usize>,
    ) -> Option<usize> {
        let theme = self.theme;
        let response = self.interact(id, rect);
        let open = self.state.popup.is_some_and(|(popup, _)| popup == id);
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: if response.hovered {
                    theme.hover
                } else {
                    theme.surface
                },
                border: if open { theme.accent } else { theme.border },
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        let pad = theme.padding;
        let chevron = self.fonts.measure(FontFace::Ui, theme.ui_size, "\u{25BE}");
        let current = selected
            .and_then(|index| options.get(index))
            .map_or("Choose", |option| option.label.as_str());
        self.label(
            Rect::new(
                rect.x + pad,
                rect.y,
                (rect.width - pad * 3.0 - chevron).max(0.0),
                rect.height,
            ),
            current,
            theme.text,
        );
        self.label(
            Rect::new(rect.right() - pad - chevron, rect.y, chevron, rect.height),
            "\u{25BE}",
            theme.muted,
        );
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        if response.clicked {
            self.state.popup = if open { None } else { Some((id, id)) };
        }
        if !open {
            return None;
        }
        let row = rect.height;
        let list = Rect::new(
            rect.x,
            rect.bottom() + 2.0,
            rect.width,
            row * options.len() as f32,
        );
        let mut picked = None;
        self.begin_popup(list);
        for (index, option) in options.iter().enumerate() {
            let item = Rect::new(list.x, list.y + row * index as f32, list.width, row);
            let response = if option.enabled {
                self.interact(id.wrapping_add(1000 + index as u64), item)
            } else {
                Response::default()
            };
            if response.hovered {
                self.canvas.draw_box(
                    item.shrink(2.0),
                    BoxStyle::filled(theme.hover, theme.radius),
                );
                self.set_cursor(CursorShape::Pointer);
            }
            let color = if selected == Some(index) {
                theme.accent
            } else if option.enabled {
                theme.text
            } else {
                theme.muted.fade(0.6)
            };
            self.label(
                Rect::new(item.x + pad, item.y, item.width - pad * 2.0, item.height),
                &option.label,
                color,
            );
            if response.clicked {
                picked = Some(index);
                self.state.popup = None;
            }
        }
        self.end_popup();
        picked
    }

    /// Starts drawing a pop-up panel at `rect` on the overlay.
    pub fn begin_popup(&mut self, rect: Rect) {
        self.canvas.set_layer(Layer::Overlay);
        self.state.popup_area.push(rect);
        let theme = self.theme;
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.panel,
                border: theme.border,
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        // A pop-up captures the pointer wherever it is drawn.
        self.state.hits[1].push((rect, widget_id("ui/popup", "panel")));
    }

    /// Ends a pop-up panel.
    pub fn end_popup(&mut self) {
        self.canvas.set_layer(Layer::Base);
    }

    /// `true` if the pop-up `id` is open.
    #[must_use]
    pub fn popup_open(&self, id: WidgetId) -> bool {
        self.state.popup.is_some_and(|(popup, _)| popup == id)
    }

    /// Opens pop-up `id`, anchored to the widget `anchor`.
    pub fn open_popup(&mut self, id: WidgetId, anchor: WidgetId) {
        self.state.popup = Some((id, anchor));
    }

    /// Closes any open pop-up.
    pub fn close_popup(&mut self) {
        self.state.popup = None;
    }

    /// The width a menu needs for `entries`.
    #[must_use]
    pub fn menu_width(&self, entries: &[MenuEntry]) -> f32 {
        let theme = self.theme;
        let pad = theme.padding;
        entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item {
                    label, shortcut, ..
                } => {
                    self.fonts.measure(FontFace::Ui, theme.ui_size, label)
                        + shortcut.as_ref().map_or(0.0, |shortcut| {
                            self.fonts.measure(FontFace::Ui, theme.small_size, shortcut) + pad * 4.0
                        })
                        + pad * 4.0
                }
                MenuEntry::Separator => 0.0,
            })
            .fold(160.0_f32, f32::max)
    }

    /// Draws menu `id` with its top-left at `origin` on the overlay. Returns
    /// the index of the clicked entry and closes the menu.
    pub fn menu(&mut self, id: WidgetId, origin: Point, entries: &[MenuEntry]) -> Option<usize> {
        let theme = self.theme;
        let pad = theme.padding;
        let row = self.row_height();
        let separator = pad;
        let width = self.menu_width(entries);
        let height: f32 = entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item { .. } => row,
                MenuEntry::Separator => separator,
            })
            .sum::<f32>()
            + pad / 2.0;
        let panel = Rect::new(origin.x, origin.y, width, height);
        self.begin_popup(panel);
        let mut y = panel.y + pad / 4.0;
        let mut picked = None;
        for (index, entry) in entries.iter().enumerate() {
            match entry {
                MenuEntry::Separator => {
                    self.hline(panel.x + pad / 2.0, y + separator / 2.0, panel.width - pad);
                    y += separator;
                }
                MenuEntry::Item {
                    label,
                    shortcut,
                    enabled,
                } => {
                    let item = Rect::new(panel.x + 3.0, y, panel.width - 6.0, row);
                    let response = if *enabled {
                        self.interact(id.wrapping_add(index as u64 + 1), item)
                    } else {
                        Response::default()
                    };
                    if response.hovered {
                        self.canvas
                            .draw_box(item, BoxStyle::filled(theme.accent, theme.radius));
                        self.set_cursor(CursorShape::Pointer);
                    }
                    let (color, hint) = if response.hovered {
                        (theme.on_accent, theme.on_accent)
                    } else if *enabled {
                        (theme.text, theme.muted)
                    } else {
                        (theme.muted.fade(0.6), theme.muted.fade(0.6))
                    };
                    self.label(
                        Rect::new(item.x + pad, item.y, item.width - pad * 2.0, row),
                        label,
                        color,
                    );
                    if let Some(shortcut) = shortcut {
                        self.label_right(
                            Rect::new(item.x, item.y, item.width - pad, row),
                            shortcut,
                            theme.small_size,
                            hint,
                        );
                    }
                    if response.clicked {
                        picked = Some(index);
                        self.state.popup = None;
                    }
                    y += row;
                }
            }
        }
        self.end_popup();
        picked
    }

    /// A one-line text field showing `text` with the caret at byte `caret`
    /// when focused, or `placeholder` in muted text when empty. Returns the
    /// response (a click usually takes focus).
    #[allow(clippy::too_many_arguments, reason = "a text field has many parts")]
    pub fn text_field(
        &mut self,
        id: WidgetId,
        rect: Rect,
        text: &str,
        caret: Option<usize>,
        placeholder: &str,
        face: FontFace,
    ) -> Response {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let focused = caret.is_some();
        let border = if focused {
            theme.accent
        } else if response.hovered {
            theme.muted.fade(0.6)
        } else {
            theme.border
        };
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.surface,
                border,
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        let size = match face {
            FontFace::Ui => theme.ui_size,
            FontFace::Mono => theme.mono_size,
        };
        let pad = theme.padding;
        let height = self.fonts.line_height(face, size);
        let y = rect.y + (rect.height - height) / 2.0;
        let inner = Rect::new(
            rect.x + pad,
            rect.y,
            (rect.width - pad * 2.0).max(0.0),
            rect.height,
        );
        self.canvas.push_clip(inner);
        // Keep the caret in view: scroll the text left when it runs past.
        let caret_x = caret.map_or(0.0, |caret| self.fonts.caret_x(face, size, text, caret));
        let scroll = (caret_x - inner.width + 2.0).max(0.0);
        if text.is_empty() && !placeholder.is_empty() {
            self.canvas
                .draw_text(Point::new(inner.x, y), placeholder, face, size, theme.muted);
        } else {
            self.canvas.draw_text(
                Point::new(inner.x - scroll, y),
                text,
                face,
                size,
                theme.text,
            );
        }
        if focused {
            self.caret(Rect::new(inner.x - scroll + caret_x, y, 1.5, height));
        }
        self.canvas.pop_clip();
        if response.hovered {
            self.set_cursor(CursorShape::Text);
        }
        response
    }

    /// Draws the tooltip requested this frame, if any, near its widget.
    pub fn draw_tooltip(&mut self) {
        let Some((text, anchor)) = self.state.tooltip.take() else {
            return;
        };
        let theme = self.theme;
        let pad = theme.padding;
        let size = theme.small_size;
        let width = self.fonts.measure(FontFace::Ui, size, &text) + pad * 2.0;
        let height = self.fonts.line_height(FontFace::Ui, size) + pad;
        let viewport = self.canvas.viewport();
        // Beside the anchor if there is room to its right, else below it.
        let (mut x, mut y) = if anchor.right() + width + pad < viewport.right()
            && anchor.width < anchor.height * 2.0
        {
            (
                anchor.right() + 4.0,
                anchor.y + (anchor.height - height) / 2.0,
            )
        } else {
            (anchor.x, anchor.bottom() + 4.0)
        };
        x = x.clamp(viewport.x, (viewport.right() - width).max(viewport.x));
        y = y.clamp(viewport.y, (viewport.bottom() - height).max(viewport.y));
        let rect = Rect::new(x, y, width, height);
        self.canvas.set_layer(Layer::Overlay);
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.surface,
                border: theme.border,
                border_width: theme.hairline(),
                radius: theme.radius,
            },
        );
        self.label_centered_sized(rect, &text, size, theme.text);
        self.canvas.set_layer(Layer::Base);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_platform::Modifiers;
    use sue_rice::{DEFAULT_RICE, RiceSchema};

    struct Harness {
        canvas: Canvas,
        fonts: Fonts,
        theme: Theme,
        input: UiInput,
        state: UiState,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                canvas: Canvas::new(Rect::new(0.0, 0.0, 400.0, 300.0)),
                fonts: Fonts::new().unwrap(),
                theme: Theme::from_rice(&RiceSchema::parse(DEFAULT_RICE).unwrap()),
                input: UiInput::default(),
                state: UiState::default(),
            }
        }

        /// Runs one frame, then clears one-frame input.
        fn frame<R>(&mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
            self.state.begin_frame();
            self.canvas = Canvas::new(Rect::new(0.0, 0.0, 400.0, 300.0));
            let result = build(&mut Ui {
                canvas: &mut self.canvas,
                fonts: &self.fonts,
                theme: &self.theme,
                input: &self.input,
                state: &mut self.state,
            });
            self.state.end_frame(&self.input);
            self.input.end_frame();
            result
        }

        fn event(&mut self, event: InputEvent) {
            self.input.apply(&event);
        }

        fn mouse(&mut self, x: f32, y: f32, pressed: Option<bool>) {
            let position = Point::new(x, y);
            match pressed {
                Some(pressed) => self.event(InputEvent::Mouse(MouseEvent::Button {
                    button: MouseButton::Left,
                    pressed,
                    position,
                })),
                None => self.event(InputEvent::Mouse(MouseEvent::Moved { position })),
            }
        }

        fn click<R>(&mut self, x: f32, y: f32, mut build: impl FnMut(&mut Ui<'_>) -> R) -> R {
            self.mouse(x, y, None);
            self.frame(&mut build);
            self.mouse(x, y, Some(true));
            self.frame(&mut build);
            self.mouse(x, y, Some(false));
            self.frame(&mut build)
        }
    }

    const BUTTON: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);

    #[test]
    fn buttons_click_on_release_inside() {
        let mut ui = Harness::new();
        let id = widget_id("test", "button");
        ui.mouse(20.0, 20.0, None);
        assert!(!ui.frame(|ui| ui.button(id, BUTTON, "Open")));
        ui.mouse(20.0, 20.0, Some(true));
        assert!(!ui.frame(|ui| ui.button(id, BUTTON, "Open")));
        assert_eq!(ui.state.active, Some(id));
        ui.mouse(25.0, 25.0, Some(false));
        assert!(ui.frame(|ui| ui.button(id, BUTTON, "Open")));
        assert_eq!(ui.state.active, None);
    }

    #[test]
    fn releasing_outside_cancels_the_click() {
        let mut ui = Harness::new();
        let id = widget_id("test", "button");
        ui.mouse(20.0, 20.0, Some(true));
        ui.frame(|ui| ui.button(id, BUTTON, "Open"));
        ui.mouse(300.0, 200.0, Some(false));
        assert!(!ui.frame(|ui| ui.button(id, BUTTON, "Open")));
    }

    #[test]
    fn the_topmost_widget_wins_overlapping_clicks() {
        let mut ui = Harness::new();
        let (card, pin) = (widget_id("test", "card"), widget_id("test", "pin"));
        let draw = |ui: &mut Ui<'_>| {
            let card_clicked = ui.button(card, Rect::new(0.0, 0.0, 200.0, 100.0), "Card");
            let pin_clicked = ui.button(pin, Rect::new(150.0, 10.0, 30.0, 30.0), "P");
            (card_clicked, pin_clicked)
        };
        assert_eq!(ui.click(160.0, 20.0, draw), (false, true));
        assert_eq!(
            ui.state.hit_rect(pin),
            Some(Rect::new(150.0, 10.0, 30.0, 30.0))
        );
    }

    #[test]
    fn sliders_preview_while_dragging_and_commit_on_release() {
        let mut ui = Harness::new();
        let id = widget_id("test", "slider");
        let track = Rect::new(0.0, 0.0, 212.0, 20.0);
        // The thumb is 12 px, so values run from x = 6 to x = 206.
        ui.mouse(106.0, 10.0, Some(true));
        assert_eq!(
            ui.frame(|ui| ui.slider(id, track, 8.0, 0.0, 64.0)),
            SliderEvent::Preview(32.0)
        );
        ui.mouse(156.0, 10.0, None);
        assert_eq!(
            ui.frame(|ui| ui.slider(id, track, 8.0, 0.0, 64.0)),
            SliderEvent::Preview(48.0)
        );
        ui.mouse(400.0, 10.0, Some(false));
        assert_eq!(
            ui.frame(|ui| ui.slider(id, track, 8.0, 0.0, 64.0)),
            SliderEvent::Commit(64.0)
        );
        assert_eq!(
            ui.frame(|ui| ui.slider(id, track, 64.0, 0.0, 64.0)),
            SliderEvent::Idle
        );
    }

    #[test]
    fn hover_redraws_only_when_the_hovered_widget_changes() {
        let mut ui = Harness::new();
        let id = widget_id("test", "button");
        ui.mouse(20.0, 20.0, None);
        ui.frame(|ui| ui.button(id, BUTTON, "Open"));
        ui.state.begin_frame();
        ui.state.hot = Some(id);
        assert!(!ui.state.hover_changes(Some(Point::new(30.0, 25.0))));
        assert!(ui.state.hover_changes(Some(Point::new(300.0, 250.0))));
    }

    #[test]
    fn segmented_controls_report_new_choices() {
        let mut ui = Harness::new();
        let options = [UiChoice::new("Auto"), UiChoice::new("Balanced")];
        let id = widget_id("test", "segments");
        let rect = Rect::new(0.0, 0.0, 200.0, 24.0);
        assert_eq!(
            ui.click(150.0, 12.0, |ui| ui.segmented(id, rect, &options, Some(0))),
            Some(1)
        );
        assert_eq!(
            ui.click(150.0, 12.0, |ui| ui.segmented(id, rect, &options, Some(1))),
            None
        );
        let (wide, narrow) = ui.frame(|ui| {
            (
                ui.segmented_width(&options, 1000.0),
                ui.segmented_width(&options, 10.0),
            )
        });
        assert!(wide.is_some() && narrow.is_none());
    }

    #[test]
    fn dropdowns_open_on_the_overlay_and_close_on_pick() {
        let mut ui = Harness::new();
        let options = [UiChoice::new("A"), UiChoice::new("B"), UiChoice::new("C")];
        let id = widget_id("test", "dropdown");
        let rect = Rect::new(10.0, 10.0, 120.0, 24.0);
        let draw = |ui: &mut Ui<'_>| ui.dropdown(id, rect, &options, Some(0));
        assert_eq!(ui.click(20.0, 20.0, draw), None);
        assert!(ui.state.popup.is_some(), "a click opens the list");
        // The third option sits two rows below the first, under the field.
        let third_y = rect.bottom() + 2.0 + 24.0 * 2.0 + 12.0;
        assert_eq!(ui.click(20.0, third_y, draw), Some(2));
        assert!(ui.state.popup.is_none());
    }

    #[test]
    fn a_press_outside_a_popup_closes_it_without_clicking_through() {
        let mut ui = Harness::new();
        let (menu, below) = (widget_id("test", "menu"), widget_id("test", "below"));
        let entries = [MenuEntry::item("Open", Some("Ctrl+O".into()))];
        ui.state.popup = Some((menu, menu));
        let draw = |ui: &mut Ui<'_>| {
            let clicked = ui.button(below, Rect::new(300.0, 200.0, 80.0, 30.0), "Below");
            let picked = if ui.popup_open(menu) {
                ui.menu(menu, Point::new(10.0, 10.0), &entries)
            } else {
                None
            };
            (clicked, picked)
        };
        assert_eq!(ui.click(320.0, 210.0, draw), (false, None));
        assert!(ui.state.popup.is_none());
    }

    #[test]
    fn line_edits_type_move_and_delete_by_character() {
        let mut edit = LineEdit::new("hé");
        let key = |key| KeyEvent::press(key, Modifiers::NONE);
        assert_eq!(edit.handle(&KeyEvent::typed('y')), EditOutcome::Changed);
        assert_eq!(edit.text, "héy");
        edit.handle(&key(Key::Named(NamedKey::ArrowLeft)));
        edit.handle(&key(Key::Named(NamedKey::ArrowLeft)));
        assert_eq!(edit.caret, 1);
        assert_eq!(
            edit.handle(&key(Key::Named(NamedKey::Delete))),
            EditOutcome::Changed
        );
        assert_eq!(edit.text, "hy");
        edit.handle(&key(Key::Named(NamedKey::End)));
        edit.handle(&key(Key::Named(NamedKey::Backspace)));
        assert_eq!((edit.text.as_str(), edit.caret), ("h", 1));
        assert_eq!(
            edit.handle(&key(Key::Named(NamedKey::Enter))),
            EditOutcome::Submit
        );
        assert_eq!(
            edit.handle(&key(Key::Named(NamedKey::Escape))),
            EditOutcome::Cancel
        );
        let command = KeyEvent::press(Key::Character('C'), Modifiers::CTRL);
        assert_eq!(edit.handle(&command), EditOutcome::Ignored);
    }

    #[test]
    fn carets_draw_only_in_the_visible_half_of_the_blink() {
        let mut ui = Harness::new();
        let rect = Rect::new(0.0, 0.0, 2.0, 10.0);
        ui.state.caret_visible = false;
        ui.frame(|ui| ui.caret(rect));
        assert!(ui.canvas.is_empty() && ui.state.caret_drawn);
        ui.state.caret_visible = true;
        ui.frame(|ui| ui.caret(rect));
        assert_eq!(ui.canvas.len(), 1);
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(widget_id("a", "b"), widget_id("a", "b"));
        assert_ne!(widget_id("a", "bc"), widget_id("ab", "c"));
    }
}
