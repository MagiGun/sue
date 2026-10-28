//! An immediate-mode widget toolkit over [`Canvas`]: widgets are declared
//! every frame, hit-tested against the pointer, and drawn in one pass.

use sue_platform::{CursorShape, FileDrop, InputEvent, MouseButton, MouseEvent, Point};

use crate::canvas::intersect;
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
    point.x >= rect.x && point.y >= rect.y && point.x < rect.right() && point.y < rect.bottom()
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
    hits: Vec<(Rect, WidgetId)>,
    previous_hits: Vec<(Rect, WidgetId)>,
}

impl UiState {
    /// Starts a frame: last frame's hit regions decide which widget is on top.
    pub fn begin_frame(&mut self) {
        self.previous_hits = std::mem::take(&mut self.hits);
        self.hot = None;
        self.cursor = None;
    }

    /// Ends a frame: a release always ends any press.
    pub fn end_frame(&mut self, input: &UiInput) {
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
    pub fn interact(&mut self, id: WidgetId, rect: Rect) -> Response {
        let clip = self.canvas.clip();
        let visible = intersect(rect, clip);
        self.state.hits.push((visible, id));
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
        let hovered = inside && on_top && free;
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

    /// Height of one UI text line.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.fonts.line_height(FontFace::Ui, self.theme.ui_size)
    }

    /// Height of a standard control row.
    #[must_use]
    pub fn row_height(&self) -> f32 {
        self.line_height() + self.theme.padding()
    }

    /// Wheel movement over `rect` this frame (vertical).
    #[must_use]
    pub fn wheel_in(&self, rect: Rect) -> f32 {
        let clip = intersect(rect, self.canvas.clip());
        match self.input.pointer {
            Some(point) if contains(clip, point) => self.input.wheel.y,
            _ => 0.0,
        }
    }

    /// A pane surface.
    pub fn panel(&mut self, rect: Rect) {
        let theme = self.theme;
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.surface,
                border: theme.raised,
                border_width: theme.border_width,
                radius: theme.radius,
            },
        );
    }

    /// One line of text at `position` (top-left of the line box).
    pub fn text(&mut self, position: Point, text: &str, face: FontFace, size: f32, color: Color) {
        self.canvas.draw_text(position, text, face, size, color);
    }

    /// UI text, vertically centered in `rect`, elided to fit.
    pub fn label(&mut self, rect: Rect, text: &str, color: Color) {
        let size = self.theme.ui_size;
        let height = self.fonts.line_height(FontFace::Ui, size);
        let shown = self.fonts.elide(FontFace::Ui, size, text, rect.width);
        let y = rect.y + (rect.height - height) / 2.0;
        self.canvas
            .draw_text(Point::new(rect.x, y), &shown, FontFace::Ui, size, color);
    }

    /// UI text centered in `rect`.
    pub fn label_centered(&mut self, rect: Rect, text: &str, color: Color) {
        let size = self.theme.ui_size;
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

    /// A push button. Returns `true` when clicked.
    pub fn button(&mut self, id: WidgetId, rect: Rect, text: &str) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let fill = if response.dragging {
            theme.accent.fade(0.35)
        } else if response.hovered {
            theme.hover
        } else {
            theme.raised
        };
        self.canvas
            .draw_box(rect, BoxStyle::filled(fill, theme.radius));
        self.label_centered(rect, text, theme.text);
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// An on/off switch. Returns `true` when clicked.
    pub fn toggle(&mut self, id: WidgetId, rect: Rect, on: bool) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let height = rect.height.min(self.line_height());
        let track = Rect::new(
            rect.x,
            rect.y + (rect.height - height) / 2.0,
            height * 1.9,
            height,
        );
        let fill = if on { theme.accent } else { theme.raised };
        let border = if response.hovered {
            theme.accent
        } else {
            theme.muted.fade(0.5)
        };
        self.canvas.draw_box(
            track,
            BoxStyle {
                fill,
                border,
                border_width: theme.border_width.max(1.0),
                radius: height / 2.0,
            },
        );
        let knob = height - 4.0;
        let knob_x = if on {
            track.right() - knob - 2.0
        } else {
            track.x + 2.0
        };
        let knob_color = if on { theme.on_accent } else { theme.muted };
        self.canvas.draw_box(
            Rect::new(knob_x, track.y + 2.0, knob, knob),
            BoxStyle::filled(knob_color, knob / 2.0),
        );
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// A horizontal slider over `min..=max`. Dragging previews; releasing
    /// commits, so one drag is one undo step.
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
        let span = (max - min).max(f64::EPSILON);
        let to_value =
            |x: f32| min + f64::from(((x - rect.x) / rect.width.max(1.0)).clamp(0.0, 1.0)) * span;
        let pointer_value = self.input.pointer.map(|point| to_value(point.x));
        let shown = if response.dragging || response.released {
            pointer_value.unwrap_or(value)
        } else {
            value
        };
        let fraction = (((shown - min) / span).clamp(0.0, 1.0)) as f32;
        let track_height = (rect.height / 5.0).max(2.0);
        let track = Rect::new(
            rect.x,
            rect.y + (rect.height - track_height) / 2.0,
            rect.width,
            track_height,
        );
        self.canvas
            .draw_box(track, BoxStyle::filled(theme.raised, track_height / 2.0));
        self.canvas.draw_box(
            Rect::new(track.x, track.y, track.width * fraction, track.height),
            BoxStyle::filled(theme.accent, track_height / 2.0),
        );
        let knob = (rect.height * 0.7).max(track_height * 2.0);
        let knob_rect = Rect::new(
            rect.x + rect.width * fraction - knob / 2.0,
            rect.y + (rect.height - knob) / 2.0,
            knob,
            knob,
        );
        let knob_fill = if response.hovered || response.dragging {
            theme.accent
        } else {
            theme.text
        };
        self.canvas
            .draw_box(knob_rect, BoxStyle::filled(knob_fill, knob / 2.0));
        if response.hovered || response.dragging {
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
            theme.muted.fade(0.6)
        };
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: color,
                border,
                border_width: if selected { 2.0 } else { 1.0 },
                radius: theme.radius / 2.0,
            },
        );
        if response.hovered {
            self.set_cursor(CursorShape::Pointer);
        }
        response.clicked
    }

    /// A vertical list of choices, one row each. Returns the clicked index.
    pub fn choices(
        &mut self,
        id: WidgetId,
        rect: Rect,
        options: &[String],
        selected: usize,
    ) -> Option<usize> {
        let row = rect.height / options.len().max(1) as f32;
        let mut picked = None;
        for (index, option) in options.iter().enumerate() {
            let item = Rect::new(rect.x, rect.y + row * index as f32, rect.width, row - 2.0);
            let response = self.interact(id.wrapping_add(index as u64 + 1), item);
            let theme = self.theme;
            let (fill, color) = if index == selected {
                (theme.accent, theme.on_accent)
            } else if response.hovered {
                (theme.hover, theme.text)
            } else {
                (theme.raised, theme.text)
            };
            self.canvas
                .draw_box(item, BoxStyle::filled(fill, theme.radius / 2.0));
            let inset = Rect::new(
                item.x + theme.padding(),
                item.y,
                item.width - theme.padding() * 2.0,
                item.height,
            );
            self.label(inset, option, color);
            if response.hovered {
                self.set_cursor(CursorShape::Pointer);
            }
            if response.clicked {
                picked = Some(index);
            }
        }
        picked
    }

    /// A one-line text field. Returns `true` when clicked (to take focus).
    pub fn text_field(
        &mut self,
        id: WidgetId,
        rect: Rect,
        text: &str,
        focused: bool,
        face: FontFace,
    ) -> bool {
        let response = self.interact(id, rect);
        let theme = self.theme;
        let border = if focused {
            theme.accent
        } else if response.hovered {
            theme.muted
        } else {
            theme.raised
        };
        self.canvas.draw_box(
            rect,
            BoxStyle {
                fill: theme.background,
                border,
                border_width: theme.border_width.max(1.0),
                radius: theme.radius / 2.0,
            },
        );
        let size = match face {
            FontFace::Ui => theme.ui_size,
            FontFace::Mono => theme.mono_size,
        };
        let pad = theme.padding() / 2.0;
        let height = self.fonts.line_height(face, size);
        let y = rect.y + (rect.height - height) / 2.0;
        self.canvas.push_clip(Rect::new(
            rect.x + pad,
            rect.y,
            rect.width - pad * 2.0,
            rect.height,
        ));
        self.canvas
            .draw_text(Point::new(rect.x + pad, y), text, face, size, theme.text);
        if focused {
            let caret = self.fonts.measure(face, size, text);
            self.canvas.draw_box(
                Rect::new(rect.x + pad + caret + 1.0, y, 1.5, height),
                BoxStyle::filled(theme.accent, 0.0),
            );
        }
        self.canvas.pop_clip();
        if response.hovered {
            self.set_cursor(CursorShape::Text);
        }
        response.clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        ui.mouse(160.0, 20.0, None);
        ui.frame(draw);
        ui.mouse(160.0, 20.0, Some(true));
        ui.frame(draw);
        ui.mouse(160.0, 20.0, Some(false));
        assert_eq!(ui.frame(draw), (false, true));
    }

    #[test]
    fn sliders_preview_while_dragging_and_commit_on_release() {
        let mut ui = Harness::new();
        let id = widget_id("test", "slider");
        let track = Rect::new(0.0, 0.0, 200.0, 20.0);
        ui.mouse(100.0, 10.0, Some(true));
        assert_eq!(
            ui.frame(|ui| ui.slider(id, track, 8.0, 0.0, 64.0)),
            SliderEvent::Preview(32.0)
        );
        ui.mouse(150.0, 10.0, None);
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
    fn choices_report_the_clicked_row() {
        let mut ui = Harness::new();
        let options: Vec<String> = ["auto", "balanced"].map(str::to_owned).into();
        let id = widget_id("test", "choices");
        let rect = Rect::new(0.0, 0.0, 200.0, 60.0);
        ui.mouse(10.0, 45.0, None);
        ui.frame(|ui| ui.choices(id, rect, &options, 0));
        ui.mouse(10.0, 45.0, Some(true));
        ui.frame(|ui| ui.choices(id, rect, &options, 0));
        ui.mouse(10.0, 45.0, Some(false));
        assert_eq!(ui.frame(|ui| ui.choices(id, rect, &options, 0)), Some(1));
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(widget_id("a", "b"), widget_id("a", "b"));
        assert_ne!(widget_id("a", "bc"), widget_id("ab", "c"));
    }
}
