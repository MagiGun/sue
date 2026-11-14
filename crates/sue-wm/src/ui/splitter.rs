//! The splitter: the line between two panes that is dragged to share the
//! room between them.
//!
//! At rest it is a hairline. Under the pointer and while it is dragged it
//! grows to a strong line in the focus color, and the pointer becomes a
//! resize arrow, one way only when the pane is at its least or its most.
//! A double click puts it back where it was at first. It takes keyboard
//! focus: the arrow keys move it a step, Home and End to its ends.
//!
//! What it moves is the size of one pane, the one its caller keeps.

use std::ops::RangeInclusive;

use egui::{
    CursorIcon, EventFilter, Id, Key, Modifiers, Rangef, Rect, Response, Role, Sense, Stroke, Ui,
    UiBuilder, WidgetInfo, pos2, vec2,
};

use super::focus::focus_visible;
use super::focus::paint_focus_ring;
use super::list::{blend, salted, scoped};
use super::motion;
use super::tokens::{Density, Space};
use crate::Theme;

/// Which way a splitter's line runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SplitAxis {
    /// Top to bottom, between two columns: it is dragged left and right.
    Columns,
    /// Left to right, between two rows: it is dragged up and down.
    Rows,
}

/// What a drag remembers: the size when it began and where the pointer
/// was.
#[derive(Clone, Copy, Debug)]
struct Held {
    size: f32,
    from: f32,
}

/// The line between two panes.
#[derive(Clone, Debug)]
#[must_use = "a splitter draws nothing until it is shown"]
pub struct Splitter<'a> {
    id: Id,
    axis: SplitAxis,
    range: RangeInclusive<f32>,
    reset: Option<f32>,
    after: bool,
    label: &'a str,
}

impl<'a> Splitter<'a> {
    fn new(salt: impl std::hash::Hash + std::fmt::Debug, axis: SplitAxis) -> Self {
        Self {
            id: salted("sue-splitter", salt),
            axis,
            range: 0.0..=f32::INFINITY,
            reset: None,
            after: false,
            label: "",
        }
    }

    /// A splitter between two columns. `salt` names it among what is drawn
    /// in the same `Ui` and stays the same from frame to frame.
    pub fn columns(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self::new(salt, SplitAxis::Columns)
    }

    /// A splitter between two rows.
    pub fn rows(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self::new(salt, SplitAxis::Rows)
    }

    /// The least and the most the pane may be.
    pub fn range(mut self, range: RangeInclusive<f32>) -> Self {
        self.range = range;
        self
    }

    /// The size a double click puts the pane back to.
    pub const fn reset_to(mut self, size: f32) -> Self {
        self.reset = Some(size);
        self
    }

    /// The pane it sizes lies after the line (right of it, or under it):
    /// dragging toward the pane makes it smaller.
    pub const fn sizes_after(mut self, after: bool) -> Self {
        self.after = after;
        self
    }

    /// What the pane it sizes is, for a screen reader.
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// How wide the strip is that takes the pointer, at `density`.
    #[must_use]
    pub fn reach(theme: &Theme, density: Density) -> f32 {
        theme.space(match density {
            Density::Compact | Density::Regular => Space::S3,
            Density::Touch => Space::S6,
        })
    }

    /// `size` kept inside the range (a range whose ends are the wrong way
    /// round holds its least).
    fn clamp(&self, size: f32) -> f32 {
        let (least, most) = (*self.range.start(), *self.range.end());
        size.min(most).max(least)
    }

    /// Draws the line at `at` (its x for columns, its y for rows) across
    /// `span`, and moves `size` (the pane's) as it is dragged. The
    /// response says `changed` when `size` did.
    pub fn show(
        mut self,
        ui: &mut Ui,
        theme: &Theme,
        at: f32,
        span: Rangef,
        size: &mut f32,
    ) -> Response {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let reach = Self::reach(theme, density);
        let across = Rangef::new(at - reach / 2.0, at + reach / 2.0);
        let rect = match self.axis {
            SplitAxis::Columns => Rect::from_x_y_ranges(across, span),
            SplitAxis::Rows => Rect::from_x_y_ranges(span, across),
        };
        let enabled = ui.is_enabled();
        let sense = if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let mut response = ui.interact(rect, self.id, sense);
        let before = *size;
        let sign = if self.after { -1.0 } else { 1.0 };
        let along = |point: egui::Pos2| match self.axis {
            SplitAxis::Columns => point.x,
            SplitAxis::Rows => point.y,
        };
        let key = self.id.with("sue-held");
        // From where the button went down, not from where egui saw that
        // it was a drag: the line stays under the pointer from the start.
        if response.drag_started()
            && let Some(point) = ui
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos())
        {
            let held = Held {
                size: *size,
                from: along(point),
            };
            ui.data_mut(|data| data.insert_temp(key, held));
        }
        if response.dragged()
            && let Some(point) = response.interact_pointer_pos()
            && let Some(held) = ui.data(|data| data.get_temp::<Held>(key))
        {
            // From where the drag began, so a pane held at its end does not
            // drift from the pointer.
            *size = self.clamp(held.size + (along(point) - held.from) * sign);
        }
        if response.double_clicked()
            && let Some(reset) = self.reset
        {
            *size = self.clamp(reset);
        }
        if response.has_focus() {
            *size = self.keys(ui, theme, response.id, *size, sign);
        }
        if *size != before {
            response.mark_changed();
        }
        let (label, value) = (self.label, *size);
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::Splitter, enabled, label);
            info.value = Some(f64::from(value));
            info
        });
        let hot = enabled && (response.hovered() || response.dragged());
        if hot {
            ui.ctx().set_cursor_icon(self.cursor(*size, sign));
        }
        if ui.is_rect_visible(rect) {
            let lit = motion::hover_t(ui.ctx(), self.id.with("sue-lit"), hot);
            let thick = theme.border_width.max(1.0)
                + (Theme::STRONG_STROKE - theme.border_width.max(1.0)) * lit;
            let stroke = Stroke::new(thick, blend(theme.border, theme.focus(), lit));
            match self.axis {
                SplitAxis::Columns => {
                    ui.painter().vline(at, span, stroke);
                }
                SplitAxis::Rows => {
                    ui.painter().hline(span, at, stroke);
                }
            }
            let shows = focus_visible(ui.ctx()) && response.has_focus();
            let ring = motion::hover_t(ui.ctx(), self.id.with("sue-focus-ring"), shows);
            let line = match self.axis {
                SplitAxis::Columns => Rect::from_x_y_ranges(Rangef::point(at).expand(1.0), span),
                SplitAxis::Rows => Rect::from_x_y_ranges(span, Rangef::point(at).expand(1.0)),
            };
            paint_focus_ring(ui.painter(), line, 1.0, ring, theme);
        }
        response
    }

    /// The arrow keys, Home and End, while it holds focus.
    fn keys(&self, ui: &Ui, theme: &Theme, id: Id, size: f32, sign: f32) -> f32 {
        let (back, forth) = match self.axis {
            SplitAxis::Columns => (Key::ArrowLeft, Key::ArrowRight),
            SplitAxis::Rows => (Key::ArrowUp, Key::ArrowDown),
        };
        // The arrows are the splitter's: egui does not also move focus.
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                id,
                EventFilter {
                    horizontal_arrows: self.axis == SplitAxis::Columns,
                    vertical_arrows: self.axis == SplitAxis::Rows,
                    ..Default::default()
                },
            );
        });
        let step = theme.space(Space::S4);
        let mut size = size;
        let (least, most) = (*self.range.start(), *self.range.end());
        ui.input_mut(|input| {
            // Shift first: a key asked for without Shift matches it with
            // Shift held too.
            for (modifiers, distance) in [(Modifiers::SHIFT, step * 4.0), (Modifiers::NONE, step)] {
                if input.consume_key(modifiers, forth) {
                    size += distance * sign;
                }
                if input.consume_key(modifiers, back) {
                    size -= distance * sign;
                }
            }
            // To the line's own ends: Home toward the start of the room.
            let (home, end) = if sign > 0.0 {
                (least, most)
            } else {
                (most, least)
            };
            if input.consume_key(Modifiers::NONE, Key::Home) && home.is_finite() {
                size = home;
            }
            if input.consume_key(Modifiers::NONE, Key::End) && end.is_finite() {
                size = end;
            }
        });
        self.clamp(size)
    }

    /// The pointer over it: both ways, or only the way the pane can still
    /// go when it is at its least or its most.
    fn cursor(&self, size: f32, sign: f32) -> CursorIcon {
        let (least, most) = (*self.range.start(), *self.range.end());
        let low = size <= least;
        let high = size >= most;
        // Which way the line goes when the pane grows.
        let grows_forth = sign > 0.0;
        let (forth_only, back_only) = match (self.axis, grows_forth) {
            (SplitAxis::Columns, true) => (CursorIcon::ResizeEast, CursorIcon::ResizeWest),
            (SplitAxis::Columns, false) => (CursorIcon::ResizeWest, CursorIcon::ResizeEast),
            (SplitAxis::Rows, true) => (CursorIcon::ResizeSouth, CursorIcon::ResizeNorth),
            (SplitAxis::Rows, false) => (CursorIcon::ResizeNorth, CursorIcon::ResizeSouth),
        };
        match (low, high) {
            (true, false) => forth_only,
            (false, true) => back_only,
            _ => match self.axis {
                SplitAxis::Columns => CursorIcon::ResizeColumn,
                SplitAxis::Rows => CursorIcon::ResizeRow,
            },
        }
    }

    /// Splits the room `ui` has left in two panes with this splitter
    /// between them, and draws them: `first` before the line (left or
    /// above), `second` after it. `size` is the pane the splitter sizes;
    /// it is kept so that the other pane keeps at least the least of the
    /// range too.
    pub fn panes<R1, R2>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        size: &mut f32,
        first: impl FnOnce(&mut Ui) -> R1,
        second: impl FnOnce(&mut Ui) -> R2,
    ) -> (Response, R1, R2) {
        let room = ui.available_rect_before_wrap();
        let length = match self.axis {
            SplitAxis::Columns => room.width(),
            SplitAxis::Rows => room.height(),
        };
        let least = *self.range.start();
        let most = self.range.end().min(length - least).max(least);
        let splitter = self.range(least..=most);
        *size = splitter.clamp(*size);
        let lead = if splitter.after {
            length - *size
        } else {
            *size
        };
        let line = match splitter.axis {
            SplitAxis::Columns => room.left() + lead,
            SplitAxis::Rows => room.top() + lead,
        };
        let (one, two) = match splitter.axis {
            SplitAxis::Columns => (
                Rect::from_min_max(room.min, pos2(line, room.bottom())),
                Rect::from_min_max(pos2(line, room.top()), room.max),
            ),
            SplitAxis::Rows => (
                Rect::from_min_max(room.min, pos2(room.right(), line)),
                Rect::from_min_max(pos2(room.left(), line), room.max),
            ),
        };
        let half = theme.border_width.max(1.0) / 2.0;
        let pane = |ui: &mut Ui, rect: Rect, salt: &str| {
            let rect = match splitter.axis {
                SplitAxis::Columns => rect.shrink2(vec2(half, 0.0)),
                SplitAxis::Rows => rect.shrink2(vec2(0.0, half)),
            };
            ui.new_child(
                UiBuilder::new()
                    .id_salt(splitter.id.with(salt))
                    .max_rect(rect)
                    .layout(*ui.layout()),
            )
        };
        let mut left = pane(ui, one, "sue-first");
        let r1 = first(&mut left);
        let mut right = pane(ui, two, "sue-second");
        let r2 = second(&mut right);
        let span = match splitter.axis {
            SplitAxis::Columns => room.y_range(),
            SplitAxis::Rows => room.x_range(),
        };
        let response = splitter.show(ui, theme, line, span, size);
        ui.advance_cursor_after_rect(room);
        (response, r1, r2)
    }
}

/// A splitter between two columns; see [`Splitter`].
pub fn splitter<'a>(salt: impl std::hash::Hash + std::fmt::Debug) -> Splitter<'a> {
    Splitter::columns(salt)
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, PointerButton, Pos2};

    use super::super::list::harness::Run;
    use super::*;

    /// A splitter between columns at x 200, from y 0 to 300.
    fn line(ui: &mut Ui, theme: &Theme, size: &mut f32, after: bool) -> Response {
        Splitter::columns("test")
            .range(100.0..=300.0)
            .reset_to(180.0)
            .sizes_after(after)
            .show(ui, theme, 200.0, Rangef::new(0.0, 300.0), size)
    }

    #[test]
    fn a_drag_moves_the_size_with_the_pointer_inside_its_range() {
        let mut run = Run::new();
        let mut size = 200.0;
        let seen = run.drag(
            Pos2::new(200.0, 50.0),
            Pos2::new(240.0, 50.0),
            4,
            |ui, theme| line(ui, theme, &mut size, false).changed(),
        );
        assert_eq!(size, 240.0);
        assert!(seen.iter().any(|changed| *changed));
        // Far past its most, then back: it follows the pointer again at
        // once, without drifting.
        for x in [900.0, 230.0] {
            let _ = run.frame(
                vec![Event::PointerMoved(Pos2::new(x, 50.0))],
                |ui, theme| line(ui, theme, &mut size, false),
            );
        }
        assert_eq!(size, 230.0);
        let _ = run.release(Pos2::new(230.0, 50.0), |ui, theme| {
            line(ui, theme, &mut size, false)
        });
    }

    #[test]
    fn a_pane_after_the_line_grows_when_the_line_moves_back() {
        let mut run = Run::new();
        let mut size = 200.0;
        let _ = run.drag(
            Pos2::new(200.0, 50.0),
            Pos2::new(150.0, 50.0),
            4,
            |ui, theme| line(ui, theme, &mut size, true),
        );
        assert_eq!(size, 250.0);
    }

    #[test]
    fn a_double_click_puts_it_back() {
        let mut size = 260.0;
        let at = Pos2::new(200.0, 50.0);
        let mut run = Run::new();
        let _ = run.click(at, |ui, theme| line(ui, theme, &mut size, false));
        assert_eq!(size, 260.0, "one click moves nothing");
        // Two, a tenth of a second apart.
        let mut run = Run::new();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![Event::PointerMoved(at), button(true)],
            vec![button(false)],
            vec![button(true)],
            vec![button(false)],
        ] {
            let _ = run.frame(events, |ui, theme| line(ui, theme, &mut size, false));
        }
        assert_eq!(size, 180.0);
    }

    #[test]
    fn the_keys_move_it_in_steps_and_to_its_ends() {
        let mut run = Run::new();
        let mut size = 200.0;
        let response = run
            .frame(Vec::new(), |ui, theme| line(ui, theme, &mut size, false))
            .0;
        run.focus(response.id);
        let press = |run: &mut Run, size: &mut f32, key, modifiers| {
            run.key(key, modifiers, |ui, theme| line(ui, theme, size, false));
        };
        press(&mut run, &mut size, egui::Key::ArrowRight, Modifiers::NONE);
        press(&mut run, &mut size, egui::Key::ArrowRight, Modifiers::SHIFT);
        press(&mut run, &mut size, egui::Key::ArrowLeft, Modifiers::NONE);
        press(&mut run, &mut size, egui::Key::ArrowDown, Modifiers::NONE);
        assert_eq!(size, 200.0 + 8.0 + 32.0 - 8.0, "Down is not its axis");
        press(&mut run, &mut size, egui::Key::End, Modifiers::NONE);
        assert_eq!(size, 300.0);
        press(&mut run, &mut size, egui::Key::Home, Modifiers::NONE);
        assert_eq!(size, 100.0);
        assert_eq!(run.focused(), Some(response.id), "the arrows kept focus");
    }

    #[test]
    fn an_open_range_has_no_end_to_go_to() {
        let mut run = Run::new();
        let mut size = 200.0;
        let draw = |ui: &mut Ui, theme: &Theme, size: &mut f32| {
            Splitter::rows("open").show(ui, theme, 100.0, Rangef::new(0.0, 400.0), size)
        };
        let response = run
            .frame(Vec::new(), |ui, theme| draw(ui, theme, &mut size))
            .0;
        run.focus(response.id);
        run.key(egui::Key::End, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, &mut size)
        });
        assert_eq!(size, 200.0);
        run.key(egui::Key::Home, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, &mut size)
        });
        assert_eq!(size, 0.0);
        run.key(egui::Key::ArrowUp, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, &mut size)
        });
        assert_eq!(size, 0.0, "never under nothing");
    }

    #[test]
    fn the_pointer_says_which_way_the_pane_can_go() {
        let columns = Splitter::columns("c").range(100.0..=300.0);
        assert_eq!(columns.cursor(200.0, 1.0), CursorIcon::ResizeColumn);
        assert_eq!(columns.cursor(100.0, 1.0), CursorIcon::ResizeEast);
        assert_eq!(columns.cursor(300.0, 1.0), CursorIcon::ResizeWest);
        assert_eq!(columns.cursor(100.0, -1.0), CursorIcon::ResizeWest);
        let rows = Splitter::rows("r").range(100.0..=300.0);
        assert_eq!(rows.cursor(200.0, 1.0), CursorIcon::ResizeRow);
        assert_eq!(rows.cursor(100.0, 1.0), CursorIcon::ResizeSouth);
        assert_eq!(rows.cursor(300.0, -1.0), CursorIcon::ResizeSouth);
    }

    #[test]
    fn panes_share_the_room_and_leave_the_other_its_least() {
        let mut run = Run::new();
        let mut size = 5000.0;
        let (one, two) = run
            .frame(Vec::new(), |ui, theme| {
                let (_, one, two) = Splitter::columns("panes")
                    .range(120.0..=f32::INFINITY)
                    .panes(ui, theme, &mut size, |ui| ui.max_rect(), |ui| ui.max_rect());
                (one, two)
            })
            .0;
        assert!(two.width() >= 119.0, "{two:?}");
        assert!((one.right() - two.left()).abs() <= 1.0);
        assert!((size - (one.width() + 0.5)).abs() <= 1.0, "{size} {one:?}");
    }
}
