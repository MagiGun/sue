//! Keyboard focus: the ring that shows where it is, and the scope that
//! says where it goes.
//!
//! Sue paints most of its controls by hand, and a hand-painted control
//! shows focus only when it draws [`focus_ring`]. The ring shows for focus
//! that came by keyboard: a click on a button does not leave a ring on
//! it.
//!
//! A [`FocusScope`] is a group of controls focus moves through as one
//! thing: a dialog (focus starts inside and Tab stays inside), a tab strip
//! or a menu (the arrow keys walk it).

use egui::{Context, EventFilter, Id, Key, Modifiers, Painter, Rect, Response, StrokeKind, Ui};

use super::motion;
use crate::Theme;

/// The gap between a control's edge and its focus ring.
const RING_GAP: f32 = 1.0;

/// What [`focus_visible`] remembers between frames.
#[derive(Clone, Copy)]
struct Visible {
    /// Focus last moved by keyboard.
    by_keyboard: bool,
    /// The frame that was last looked at.
    frame: u64,
}

/// `true` while keyboard focus should show: the last thing that could
/// move it was a key (Tab, an arrow, a letter) and not a click or a tap.
/// A shortcut (a key with Ctrl, Alt or Command) is not such a key: saving
/// with Ctrl+S puts no ring on the button that was clicked last.
///
/// This follows the input only in frames in which it is asked.
/// [`watch_focus`] asks in every frame, and
/// [`Theme::apply`](crate::Theme::apply) sets that up; [`focus_ring`] asks
/// too.
pub fn focus_visible(ctx: &Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    // Each window has its own input and its own focus.
    let id = Id::unique("sue-focus-visible").with(ctx.viewport_id());
    let known: Option<Visible> = ctx.data(|data| data.get_temp(id));
    if let Some(known) = known
        && known.frame == frame
    {
        return known.by_keyboard;
    }
    let (key, press) = ctx.input(|input| {
        let key = input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { pressed: true, modifiers, .. }
                    if !(modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd)
            )
        });
        (key, input.pointer.any_pressed())
    });
    // A press wins over a key held with it (Shift and click).
    let by_keyboard = if press {
        false
    } else {
        key || known.is_some_and(|known| known.by_keyboard)
    };
    ctx.data_mut(|data| data.insert_temp(id, Visible { by_keyboard, frame }));
    by_keyboard
}

/// Makes `ctx` follow the input for [`focus_visible`] in every frame,
/// whether or not anything draws a focus ring in it: a click on a page
/// without rings still ends the showing of keyboard focus. Asking twice
/// sets it up once.
pub fn watch_focus(ctx: &Context) {
    let id = Id::unique("sue-focus-watch");
    let watched = ctx.data_mut(|data| std::mem::replace(data.get_temp_mut_or(id, false), true));
    if !watched {
        ctx.on_begin_pass(
            "sue-focus-visible",
            std::sync::Arc::new(|ui: &mut Ui| {
                focus_visible(ui.ctx());
            }),
        );
    }
}

/// Paints the focus ring around `rect`, whose corners have `radius`, at
/// `opacity`. The ring lies just outside the control, inside the 3 points
/// egui leaves around every clip rectangle.
pub fn paint_focus_ring(painter: &Painter, rect: Rect, radius: f32, opacity: f32, theme: &Theme) {
    if opacity <= 0.0 {
        return;
    }
    let mut stroke = theme.focus_stroke();
    stroke.color = stroke.color.gamma_multiply(opacity.min(1.0));
    painter.rect_stroke(
        rect.expand(RING_GAP),
        radius + RING_GAP,
        stroke,
        StrokeKind::Outside,
    );
}

/// Draws the focus ring of a control whose corners have `radius`, when it
/// holds keyboard focus that should show. The ring fades in and out at
/// the fast pace. Returns `true` while the control shows focus.
pub fn focus_ring(ui: &Ui, response: &Response, radius: f32, theme: &Theme) -> bool {
    let shows = focus_visible(ui.ctx()) && response.has_focus();
    let opacity = motion::hover_t(ui.ctx(), response.id.with("sue-focus-ring"), shows);
    paint_focus_ring(ui.painter(), response.rect, radius, opacity, theme);
    shows
}

/// Which arrow keys walk a [`FocusScope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Roving {
    /// Left and Right: a tab strip, a toolbar.
    Horizontal,
    /// Up and Down: a menu, a list.
    Vertical,
    /// All four: a grid read row by row.
    Both,
}

/// A group of controls that keyboard focus treats as one thing.
///
/// Make one each frame, [`add`](Self::add) every control of the group in
/// reading order as it is drawn, and [`finish`](Self::finish) it:
///
/// - focus starts on the [`initial`](Self::add_initial) control (or the
///   first) on the frame the scope appears;
/// - with [`trap`](Self::trap), Tab and Shift+Tab stay inside, wrapping
///   from the last control to the first and back, as a dialog wants. Every
///   control of a trapped scope that can take focus has to be added;
/// - with [`roving`](Self::roving), the arrow keys move to the next and
///   the previous control, wrapping, and Home and End to the first and the
///   last, as menus, lists and tab strips want.
#[derive(Clone, Debug)]
pub struct FocusScope {
    id: Id,
    trap: bool,
    roving: Option<Roving>,
    initial: Option<Id>,
    members: Vec<Id>,
}

/// What a [`FocusScope`] remembers between frames.
#[derive(Clone, Copy)]
struct Seen {
    /// The frame it was last finished in.
    frame: u64,
    /// The last Tab pressed while it was on screen was Shift+Tab.
    back: bool,
}

impl FocusScope {
    /// A scope named by `salt`, which has to stay the same from frame to
    /// frame and differ from every other scope's on screen.
    #[must_use]
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::unique("sue-focus-scope").with(salt),
            trap: false,
            roving: None,
            initial: None,
            members: Vec::new(),
        }
    }

    /// Keeps Tab inside the scope.
    #[must_use]
    pub fn trap(mut self) -> Self {
        self.trap = true;
        self
    }

    /// Lets the arrow keys of `axis` walk the scope.
    #[must_use]
    pub fn roving(mut self, axis: Roving) -> Self {
        self.roving = Some(axis);
        self
    }

    /// Adds a control, in reading order.
    pub fn add(&mut self, response: &Response) {
        self.members.push(response.id);
    }

    /// Adds the control that takes focus when the scope appears.
    pub fn add_initial(&mut self, response: &Response) {
        self.initial = Some(response.id);
        self.add(response);
    }

    /// The controls added so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// `true` when no control was added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Ends the frame's scope: gives the first focus, holds Tab inside and
    /// answers the arrow keys. Returns the control that holds focus after
    /// it, when that is one of the scope's.
    pub fn finish(self, ctx: &Context) -> Option<Id> {
        let frame = ctx.cumulative_frame_nr();
        let tab = ctx.input(|input| input.key_pressed(Key::Tab).then_some(input.modifiers.shift));
        let before: Option<Seen> = ctx.data(|data| data.get_temp(self.id));
        // Which way Tab last went: egui moves focus a frame after the key,
        // when Shift may be up again.
        let back = tab.unwrap_or_else(|| before.is_some_and(|seen| seen.back));
        ctx.data_mut(|data| data.insert_temp(self.id, Seen { frame, back }));
        // Drawn last frame too (or earlier this frame): it is not new.
        let appeared = before.is_none_or(|seen| frame > seen.frame + 1);
        let first = *self.members.first()?;
        let last = *self.members.last()?;
        let focused = ctx.memory(|memory| memory.focused());
        let place = focused.and_then(|id| self.members.iter().position(|member| *member == id));
        let give = |id: Id| {
            ctx.memory_mut(|memory| memory.request_focus(id));
            Some(id)
        };
        if appeared {
            return match place {
                Some(index) => Some(self.members[index]),
                None => give(self.initial.unwrap_or(first)),
            };
        }
        let Some(index) = place else {
            // Focus left a trapped scope: Tab past the last control comes
            // back in at the first, Shift+Tab before the first at the last.
            if self.trap && focused.is_some() {
                return give(if back { last } else { first });
            }
            return None;
        };
        let Some(axis) = self.roving else {
            return Some(self.members[index]);
        };
        let (across, along) = (
            matches!(axis, Roving::Horizontal | Roving::Both),
            matches!(axis, Roving::Vertical | Roving::Both),
        );
        // The arrows are this scope's: egui does not also move focus to
        // whatever lies in that direction.
        ctx.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                self.members[index],
                EventFilter {
                    horizontal_arrows: across,
                    vertical_arrows: along,
                    ..Default::default()
                },
            );
        });
        let pressed = |key: Key| ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
        let count = self.members.len();
        let mut target = index;
        if (across && pressed(Key::ArrowRight)) || (along && pressed(Key::ArrowDown)) {
            target = (index + 1) % count;
        }
        if (across && pressed(Key::ArrowLeft)) || (along && pressed(Key::ArrowUp)) {
            target = (target + count - 1) % count;
        }
        if pressed(Key::Home) {
            target = 0;
        }
        if pressed(Key::End) {
            target = count - 1;
        }
        if target == index {
            Some(self.members[index])
        } else {
            give(self.members[target])
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, RawInput};

    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// One frame with three buttons, the first two in `scope` (when
    /// given), after `events`. Returns which button holds focus: 0, 1 or
    /// 2 (the one outside the scope).
    fn frame(
        ctx: &Context,
        events: Vec<Event>,
        scope: Option<fn() -> FocusScope>,
    ) -> Option<usize> {
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = RawInput {
            events: vec![Event::ModifiersChanged(modifiers)],
            ..Default::default()
        };
        input.events.extend(events);
        let mut ids = Vec::new();
        let mut done = ctx.run_ui(input, |ui| {
            let a = ui.button("One");
            let b = ui.button("Two");
            let c = ui.button("Outside");
            ids = vec![a.id, b.id, c.id];
            if let Some(scope) = scope {
                let mut scope = scope();
                scope.add(&a);
                scope.add_initial(&b);
                assert_eq!(scope.len(), 2);
                assert!(!scope.is_empty());
                scope.finish(ui.ctx());
            }
        });
        done.textures_delta.clear();
        let focused = ctx.memory(|memory| memory.focused());
        focused.and_then(|id| ids.iter().position(|member| *member == id))
    }

    #[test]
    fn a_scope_gives_the_first_focus_when_it_appears() {
        let ctx = Context::default();
        assert_eq!(frame(&ctx, Vec::new(), None), None);
        let dialog = || FocusScope::new("dialog").trap();
        assert_eq!(frame(&ctx, Vec::new(), Some(dialog)), Some(1), "initial");
        // It gives it once: focus that moved on is left alone.
        ctx.memory_mut(|memory| memory.surrender_focus(memory.focused().unwrap()));
        assert_eq!(frame(&ctx, Vec::new(), Some(dialog)), None);
        // Gone for a frame and back: it has appeared again.
        assert_eq!(frame(&ctx, Vec::new(), None), None);
        assert_eq!(frame(&ctx, Vec::new(), Some(dialog)), Some(1));
        // An empty scope does nothing.
        assert_eq!(FocusScope::new("empty").finish(&ctx), None);
    }

    #[test]
    fn a_trapped_scope_keeps_tab_inside() {
        let ctx = Context::default();
        let dialog = || FocusScope::new("dialog").trap();
        assert_eq!(frame(&ctx, Vec::new(), Some(dialog)), Some(1));
        // Tab from the last control of the scope: egui moves on to the
        // button outside, and the scope brings focus back to its first.
        let tab = || vec![key(Key::Tab, Modifiers::NONE)];
        let mut seen = Vec::new();
        for _ in 0..4 {
            // egui moves focus in the frames after the key; three frames
            // on, it has settled.
            let _ = frame(&ctx, tab(), Some(dialog));
            let _ = frame(&ctx, Vec::new(), Some(dialog));
            seen.push(frame(&ctx, Vec::new(), Some(dialog)));
        }
        assert_eq!(seen, [Some(0), Some(1), Some(0), Some(1)], "never outside");
        // Without the trap, Tab leaves.
        let ctx = Context::default();
        let open = || FocusScope::new("open");
        assert_eq!(frame(&ctx, Vec::new(), Some(open)), Some(1));
        let _ = frame(&ctx, tab(), Some(open));
        let _ = frame(&ctx, Vec::new(), Some(open));
        assert_eq!(frame(&ctx, Vec::new(), Some(open)), Some(2));
    }

    #[test]
    fn shift_tab_wraps_a_trapped_scope_to_its_last_control() {
        let ctx = Context::default();
        let dialog = || FocusScope::new("dialog").trap();
        assert_eq!(frame(&ctx, Vec::new(), Some(dialog)), Some(1));
        let back = || vec![key(Key::Tab, Modifiers::SHIFT)];
        // To the first control, then before it: around to the last.
        let mut seen = Vec::new();
        for _ in 0..3 {
            // Shift is up again before egui has moved focus.
            let _ = frame(&ctx, back(), Some(dialog));
            let _ = frame(&ctx, Vec::new(), Some(dialog));
            seen.push(frame(&ctx, Vec::new(), Some(dialog)));
        }
        assert_eq!(seen, [Some(0), Some(1), Some(0)]);
    }

    #[test]
    fn the_arrow_keys_walk_a_roving_scope() {
        let ctx = Context::default();
        let strip = || FocusScope::new("strip").roving(Roving::Horizontal);
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(1));
        // A frame for the scope to claim the arrows.
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(1));
        let press = |which: Key| vec![key(which, Modifiers::NONE)];
        assert_eq!(frame(&ctx, press(Key::ArrowLeft), Some(strip)), Some(0));
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(0));
        // Past the first: around to the last. Never to the button
        // outside, which lies below.
        assert_eq!(frame(&ctx, press(Key::ArrowLeft), Some(strip)), Some(1));
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(1));
        assert_eq!(frame(&ctx, press(Key::ArrowRight), Some(strip)), Some(0));
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(0));
        assert_eq!(frame(&ctx, press(Key::End), Some(strip)), Some(1));
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(1));
        assert_eq!(frame(&ctx, press(Key::Home), Some(strip)), Some(0));
        // The other axis is not this scope's.
        assert_eq!(frame(&ctx, Vec::new(), Some(strip)), Some(0));
        let menu = || FocusScope::new("menu").roving(Roving::Vertical);
        let ctx = Context::default();
        assert_eq!(frame(&ctx, Vec::new(), Some(menu)), Some(1));
        assert_eq!(frame(&ctx, Vec::new(), Some(menu)), Some(1));
        assert_eq!(frame(&ctx, press(Key::ArrowUp), Some(menu)), Some(0));
        assert_eq!(frame(&ctx, Vec::new(), Some(menu)), Some(0));
        assert_eq!(frame(&ctx, press(Key::ArrowDown), Some(menu)), Some(1));
    }

    #[test]
    fn a_watched_context_follows_the_input_with_no_ring_drawn() {
        let ctx = Context::default();
        // Twice: it is set up once all the same.
        watch_focus(&ctx);
        watch_focus(&ctx);
        let run = |events: Vec<Event>| {
            let mut seen = None;
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            // Asked in the next frame only: what it knows of this one, it
            // learned by watching.
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                seen = Some(focus_visible(ui.ctx()));
            });
            output.textures_delta.clear();
            seen.unwrap()
        };
        assert!(!run(Vec::new()));
        // A shortcut is not a move of focus.
        assert!(!run(vec![key(Key::S, Modifiers::CTRL)]));
        assert!(!run(vec![key(Key::S, Modifiers::COMMAND)]));
        assert!(!run(vec![key(Key::F, Modifiers::ALT)]));
        assert!(run(vec![key(Key::Tab, Modifiers::SHIFT)]));
        assert!(run(Vec::new()));
        let click = Event::PointerButton {
            pos: egui::pos2(40.0, 30.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        assert!(!run(vec![click]));
        // Without the watch, a frame nobody asks in is not seen.
        let blind = Context::default();
        let mut output = blind.run_ui(
            RawInput {
                events: vec![key(Key::Tab, Modifiers::NONE)],
                ..Default::default()
            },
            |_| {},
        );
        output.textures_delta.clear();
        let mut seen = None;
        let mut output = blind.run_ui(RawInput::default(), |ui| {
            seen = Some(focus_visible(ui.ctx()));
        });
        output.textures_delta.clear();
        assert_eq!(seen, Some(false));
    }

    #[test]
    fn the_ring_shows_for_keyboard_focus_only() {
        let ctx = Context::default();
        let theme = Theme::from_rice(&sue_rice::RiceSchema::new());
        let shown = |events: Vec<Event>| {
            let mut shows = false;
            let mut rect = Rect::NOTHING;
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let button = ui.button("One");
                    rect = button.rect;
                    button.request_focus();
                    shows = focus_ring(ui, &button, theme.control_radius, &theme);
                },
            );
            output.textures_delta.clear();
            (shows, rect, output)
        };
        // Focus alone, with no key pressed yet: no ring.
        let _ = shown(Vec::new());
        assert!(!shown(Vec::new()).0);
        // A key: the ring shows, and stays while no click comes.
        let (shows, rect, _) = shown(vec![key(Key::Tab, Modifiers::NONE)]);
        assert!(shows);
        assert!(focus_visible(&ctx));
        let (shows, _, output) = shown(Vec::new());
        assert!(shows);
        // It is drawn: a stroke in the focus color just outside the
        // button.
        fn strokes(shape: &egui::Shape, out: &mut Vec<(Rect, egui::Stroke)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| strokes(shape, out)),
                egui::Shape::Rect(rect) if rect.fill == egui::Color32::TRANSPARENT => {
                    out.push((rect.rect, rect.stroke));
                }
                _ => {}
            }
        }
        let mut rings = Vec::new();
        for clipped in &output.shapes {
            strokes(&clipped.shape, &mut rings);
        }
        let ring = rings
            .iter()
            .find(|(_, stroke)| stroke.width == Theme::STRONG_STROKE)
            .expect("a ring is drawn");
        assert_eq!(ring.0, rect.expand(RING_GAP));
        assert_eq!(ring.1.color.to_opaque(), theme.focus());
        // A click anywhere: the ring goes, though focus stays.
        let click = Event::PointerButton {
            pos: egui::pos2(400.0, 300.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        assert!(!shown(vec![click]).0);
        assert!(!focus_visible(&ctx));
    }
}
