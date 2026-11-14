//! Tooltips: what a control is called, the keys that run it, and one line
//! of help.
//!
//! [`tooltip`] gives a control its tip. With a mouse it shows when the
//! pointer rests on the control; on a touch screen, where nothing hovers,
//! it shows when the control is pressed and held, and stays for a moment
//! after the finger lifts. A control that cannot be used now shows its tip
//! all the same, so the help line can say why.
//!
//! [`tip_beside`] draws the same bubble next to any rectangle, for what is
//! not an egui control: a menu shows the note of an item with it.

use std::sync::Arc;

use egui::{
    Align2, Area, Context, Frame, Galley, Id, Margin, Pos2, Rect, Response, Sense, Ui, pos2, vec2,
};

use super::{Elevation, Layer, Radius, Space, TextRole};
use crate::Theme;

/// The widest a tip's help line runs before it wraps.
const HELP_WIDTH: f32 = 280.0;
/// How long a tip opened by a long press stays, in seconds.
const TOUCH_SECONDS: f64 = 3.0;

/// What a tooltip says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tip<'a> {
    /// What the control is called.
    pub name: &'a str,
    /// The keys that run it, such as `Ctrl+N`; empty for none.
    pub shortcut: &'a str,
    /// One line of help; empty for none.
    pub help: &'a str,
}

impl<'a> Tip<'a> {
    /// A tip that names a control.
    #[must_use]
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            shortcut: "",
            help: "",
        }
    }

    /// The same tip showing the keys that run the control.
    #[must_use]
    pub const fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = shortcut;
        self
    }

    /// The same tip with a line of help under the name.
    #[must_use]
    pub const fn help(mut self, help: &'a str) -> Self {
        self.help = help;
        self
    }

    /// `true` when there is nothing to say.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.name.is_empty() && self.shortcut.is_empty() && self.help.is_empty()
    }

    /// The tip as one line, for a screen reader.
    #[must_use]
    pub fn spoken(&self) -> String {
        [self.name, self.shortcut, self.help]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A tip laid out: its lines and how much room they take.
struct Laid {
    name: Option<Arc<Galley>>,
    shortcut: Option<Arc<Galley>>,
    help: Option<Arc<Galley>>,
    /// Space between the name and the shortcut.
    gap: f32,
    /// Space between the first line and the help.
    lead: f32,
    size: egui::Vec2,
}

impl Laid {
    fn new(ctx: &Context, tip: &Tip<'_>, theme: &Theme) -> Self {
        let surface = surface(theme);
        let muted = theme.text_muted_on(surface);
        let body = theme.text_font(TextRole::Body);
        let (name, shortcut, help) = ctx.fonts_mut(|fonts| {
            let mut line = |text: &str, color| {
                (!text.is_empty())
                    .then(|| fonts.layout_no_wrap(text.to_owned(), body.clone(), color))
            };
            let name = line(tip.name, theme.text);
            let shortcut = line(tip.shortcut, muted);
            // A help line alone is the tip's whole text and reads as such.
            let (help_role, help_color) = if tip.name.is_empty() {
                (TextRole::Body, theme.text)
            } else {
                (TextRole::Caption, muted)
            };
            let help = (!tip.help.is_empty()).then(|| {
                fonts.layout(
                    tip.help.to_owned(),
                    theme.text_font(help_role),
                    help_color,
                    HELP_WIDTH,
                )
            });
            (name, shortcut, help)
        });
        let width = |galley: &Option<Arc<Galley>>| galley.as_ref().map_or(0.0, |g| g.size().x);
        let height = |galley: &Option<Arc<Galley>>| galley.as_ref().map_or(0.0, |g| g.size().y);
        let gap = if name.is_some() && shortcut.is_some() {
            theme.space(Space::S5)
        } else {
            0.0
        };
        let first = vec2(
            width(&name) + gap + width(&shortcut),
            height(&name).max(height(&shortcut)),
        );
        let lead = if first.y > 0.0 && help.is_some() {
            theme.space(Space::S1)
        } else {
            0.0
        };
        let size = vec2(first.x.max(width(&help)), first.y + lead + height(&help));
        Self {
            name,
            shortcut,
            help,
            gap,
            lead,
            size,
        }
    }

    /// Paints the lines with their top left corner at `at`.
    fn paint(self, painter: &egui::Painter, at: Pos2, theme: &Theme) {
        let mut x = at.x;
        let mut line = 0.0_f32;
        if let Some(name) = self.name {
            line = line.max(name.size().y);
            x += name.size().x + self.gap;
            painter.galley(at, name, theme.text);
        }
        if let Some(shortcut) = self.shortcut {
            line = line.max(shortcut.size().y);
            painter.galley(pos2(x, at.y), shortcut, theme.text);
        }
        if let Some(help) = self.help {
            painter.galley(pos2(at.x, at.y + line + self.lead), help, theme.text);
        }
    }
}

/// The fill of a tip: the surface of popovers.
fn surface(theme: &Theme) -> egui::Color32 {
    theme.surface(3)
}

/// The space between a tip's edge and its text.
fn margin(theme: &Theme) -> Margin {
    Margin::symmetric(
        theme.space(Space::S4).round() as i8,
        theme.space(Space::S3).round() as i8,
    )
}

/// The frame every tip is drawn in.
fn frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(surface(theme))
        .stroke(theme.hairline())
        .corner_radius(theme.corner(Radius::Md))
        .shadow(theme.elevation(Elevation::Floating))
        .inner_margin(margin(theme))
}

/// Draws `tip` into `ui`: the inside of a tooltip.
fn tip_ui(ui: &mut Ui, tip: &Tip<'_>, theme: &Theme) {
    let laid = Laid::new(ui.ctx(), tip, theme);
    let (rect, _) = ui.allocate_exact_size(laid.size, Sense::hover());
    laid.paint(ui.painter(), rect.min, theme);
}

/// Which control a long press opened the tip of, and when.
#[derive(Clone, Copy)]
struct Held {
    widget: Id,
    since: f64,
}

/// `true` while the tip of `response` is open because the control was
/// pressed and held on a touch screen.
fn held_open(response: &Response) -> bool {
    let ctx = &response.ctx;
    let id = Id::unique("sue-tooltip-held").with(ctx.viewport_id());
    let (now, pressed) = ctx.input(|input| (input.time, input.pointer.any_pressed()));
    // A long press also opens the right-click menu of a control that has
    // one: the menu wins.
    let has_menu =
        response.context_menu_opened() || super::menu::has_context_menu(ctx, response.id);
    if response.long_touched() && !has_menu {
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                Held {
                    widget: response.id,
                    since: now,
                },
            );
        });
        return true;
    }
    let Some(held) = ctx.data(|data| data.get_temp::<Held>(id)) else {
        return false;
    };
    if held.widget != response.id {
        return false;
    }
    let left = TOUCH_SECONDS - (now - held.since);
    // The next touch anywhere puts it away, and so does time.
    if pressed || left <= 0.0 {
        ctx.data_mut(|data| data.remove::<Held>(id));
        return false;
    }
    ctx.request_repaint_after_secs(left as f32);
    true
}

/// Gives the control behind `response` the tooltip `tip`: shown when the
/// pointer rests on it, and on a touch screen when it is pressed and held
/// (the control has to sense clicks for that). What screen readers say
/// stays the control's own `widget_info`.
pub fn tooltip(response: Response, tip: &Tip<'_>, theme: &Theme) -> Response {
    if tip.is_empty() {
        return response;
    }
    let held = held_open(&response);
    if held || egui::Tooltip::should_show_tooltip(&response, false) {
        let mut tooltip = egui::Tooltip::for_widget(&response);
        tooltip.popup = tooltip
            .popup
            .frame(frame(theme))
            .gap(theme.space(Space::S2));
        tooltip.show(|ui| tip_ui(ui, tip, theme));
    }
    response
}

/// Where [`tip_beside`] puts its bubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Under the rectangle (over it when there is no room under).
    Below,
    /// To its right (to its left when there is no room at the right).
    Right,
}

/// Draws the bubble of `tip` beside `anchor` now, over everything, without
/// taking the pointer. `id` names the bubble and has to stay the same
/// while it shows. Returns where the bubble is.
pub fn tip_beside(
    ctx: &Context,
    id: Id,
    anchor: Rect,
    side: Side,
    tip: &Tip<'_>,
    theme: &Theme,
) -> Option<Rect> {
    if tip.is_empty() {
        return None;
    }
    let laid = Laid::new(ctx, tip, theme);
    let margin = margin(theme);
    let size = laid.size + margin.sum();
    let gap = theme.space(Space::S2);
    let screen = ctx.content_rect().shrink(gap);
    let fit_x = |x: f32| x.min(screen.right() - size.x).max(screen.left());
    let fit_y = |y: f32| y.min(screen.bottom() - size.y).max(screen.top());
    let origin = match side {
        Side::Below => {
            let under = anchor.bottom() + gap;
            let y = if under + size.y <= screen.bottom() {
                under
            } else {
                anchor.top() - gap - size.y
            };
            pos2(fit_x(anchor.left()), fit_y(y))
        }
        Side::Right => {
            let after = anchor.right() + gap;
            let x = if after + size.x <= screen.right() {
                after
            } else {
                anchor.left() - gap - size.x
            };
            pos2(fit_x(x), fit_y(anchor.top()))
        }
    };
    let spoken = tip.spoken();
    let shown = Area::new(id)
        .order(Layer::Tooltip.order())
        .pivot(Align2::LEFT_TOP)
        .fixed_pos(origin)
        .constrain(false)
        .interactable(false)
        .accessible_name(spoken)
        .show(ctx, |ui| {
            frame(theme).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(laid.size, Sense::hover());
                laid.paint(ui.painter(), rect.min, theme);
            });
        });
    Some(shown.response.rect)
}

#[cfg(test)]
mod tests {
    use egui::{Event, RawInput, TouchDeviceId, TouchId, TouchPhase};

    use super::*;

    fn theme() -> Theme {
        Theme::from_rice(&sue_rice::RiceSchema::new())
    }

    /// Every piece of text a frame drew.
    fn texts(output: &egui::FullOutput) -> Vec<String> {
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }

    /// One frame at `time` seconds with a button that has a full tip.
    /// Returns what the frame drew and where the button is.
    fn frame_at(ctx: &Context, time: f64, events: Vec<Event>) -> (Vec<String>, Rect) {
        let theme = theme();
        let input = RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut button = Rect::NOTHING;
        let mut output = ctx.run_ui(input, |ui| {
            let response = ui.button("Brush");
            button = response.rect;
            tooltip(
                response,
                &Tip::new("Brush")
                    .shortcut("B")
                    .help("Paints with the current color."),
                &theme,
            );
        });
        output.textures_delta.clear();
        (texts(&output), button)
    }

    fn shows_tip(texts: &[String]) -> bool {
        texts.iter().any(|text| text == "B")
            && texts
                .iter()
                .any(|text| text == "Paints with the current color.")
    }

    #[test]
    fn a_tip_says_its_name_keys_and_help() {
        let tip = Tip::new("Brush").shortcut("B").help("Paints.");
        assert_eq!(tip.spoken(), "Brush, B, Paints.");
        assert_eq!(Tip::new("Brush").spoken(), "Brush");
        assert!(Tip::default().is_empty());
        assert!(!Tip::new("").help("Needs a selection").is_empty());
    }

    #[test]
    fn a_tip_shows_when_the_pointer_rests_on_its_control() {
        let ctx = Context::default();
        let (texts, button) = frame_at(&ctx, 0.0, Vec::new());
        assert!(!shows_tip(&texts), "nothing hovers yet");
        let at = button.center();
        let mut time = 0.1;
        frame_at(&ctx, time, vec![Event::PointerMoved(at)]);
        let mut shown = false;
        // The pointer rests: the tip comes after egui's delay.
        for _ in 0..40 {
            time += 0.05;
            shown = shows_tip(&frame_at(&ctx, time, Vec::new()).0);
            if shown {
                break;
            }
        }
        assert!(shown, "the tip shows its name, its keys and its help");
        // Away from the control, it goes.
        time += 0.05;
        frame_at(
            &ctx,
            time,
            vec![Event::PointerMoved(at + vec2(300.0, 200.0))],
        );
        let mut gone = false;
        for _ in 0..40 {
            time += 0.05;
            gone = !shows_tip(&frame_at(&ctx, time, Vec::new()).0);
            if gone {
                break;
            }
        }
        assert!(gone);
    }

    #[test]
    fn a_long_press_on_a_touch_screen_shows_the_tip_and_the_next_touch_puts_it_away() {
        let ctx = Context::default();
        let (_, button) = frame_at(&ctx, 0.0, Vec::new());
        let touch = |phase: TouchPhase, at: Pos2| Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos: at,
            force: None,
        };
        let press = |pressed: bool, at: Pos2| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // A finger down on `at` at `time`, and up again `hold` seconds
        // later. Returns whether the tip showed just before it lifted, and
        // the time after it lifted.
        let tap = |time: f64, at: Pos2, hold: f64| {
            let (texts, _) = frame_at(
                &ctx,
                time,
                vec![
                    touch(TouchPhase::Start, at),
                    Event::PointerMoved(at),
                    press(true, at),
                ],
            );
            assert!(!shows_tip(&texts), "a touch alone shows nothing");
            let mut now = time;
            let mut shown = false;
            while now < time + hold {
                now += 0.05;
                shown = shows_tip(&frame_at(&ctx, now, vec![touch(TouchPhase::Move, at)]).0);
            }
            now += 0.05;
            frame_at(
                &ctx,
                now,
                vec![
                    touch(TouchPhase::End, at),
                    press(false, at),
                    Event::PointerGone,
                ],
            );
            (shown, now + 0.05)
        };
        let at = button.center();
        let away = at + vec2(250.0, 200.0);

        // Held past egui's long press time, the tip shows, and stays after
        // the finger lifts.
        let (shown, time) = tap(0.1, at, 1.1);
        assert!(shown, "pressing and holding shows the tip");
        assert!(shows_tip(&frame_at(&ctx, time, Vec::new()).0));
        // The next touch anywhere puts it away.
        let (_, time) = tap(time + 0.5, away, 0.1);
        assert!(!shows_tip(&frame_at(&ctx, time, Vec::new()).0));

        // So does time.
        let (shown, time) = tap(time + 0.5, at, 1.1);
        assert!(shown);
        assert!(shows_tip(&frame_at(&ctx, time, Vec::new()).0));
        let time = time + TOUCH_SECONDS + 0.5;
        frame_at(&ctx, time, Vec::new());
        assert!(!shows_tip(&frame_at(&ctx, time + 0.05, Vec::new()).0));

        // A short tap shows nothing.
        let (shown, time) = tap(time + 0.5, at, 0.2);
        assert!(!shown);
        assert!(!shows_tip(&frame_at(&ctx, time, Vec::new()).0));
    }

    #[test]
    fn a_bubble_stays_on_the_screen() {
        let ctx = Context::default();
        let theme = theme();
        let tip = Tip::new("").help("Needs a selection");
        let mut placed = Vec::new();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        // Two frames: the first is egui's sizing pass of a new area.
        for _ in 0..2 {
            placed.clear();
            let input = RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                // Room at the right: it goes there.
                let left = Rect::from_min_size(pos2(20.0, 40.0), vec2(100.0, 24.0));
                // None at the right: it goes to the left.
                let right = Rect::from_min_size(pos2(290.0, 40.0), vec2(100.0, 24.0));
                // None below: it goes above.
                let low = Rect::from_min_size(pos2(20.0, 270.0), vec2(100.0, 24.0));
                for (salt, anchor, side) in [
                    ("a", left, Side::Right),
                    ("b", right, Side::Right),
                    ("c", low, Side::Below),
                ] {
                    let rect = tip_beside(&ctx, Id::unique(salt), anchor, side, &tip, &theme)
                        .expect("a tip with text is drawn");
                    placed.push((anchor, rect));
                }
            });
            output.textures_delta.clear();
        }
        let (anchor, rect) = placed[0];
        assert!(
            rect.left() >= anchor.right(),
            "{rect:?} right of {anchor:?}"
        );
        let (anchor, rect) = placed[1];
        assert!(rect.right() <= anchor.left(), "{rect:?} left of {anchor:?}");
        let (anchor, rect) = placed[2];
        assert!(rect.bottom() <= anchor.top(), "{rect:?} over {anchor:?}");
        for (_, rect) in &placed {
            assert!(screen.contains_rect(*rect), "{rect:?} is on the screen");
        }
        assert_eq!(
            tip_beside(
                &ctx,
                Id::unique("d"),
                screen,
                Side::Below,
                &Tip::default(),
                &theme
            ),
            None
        );
    }
}
