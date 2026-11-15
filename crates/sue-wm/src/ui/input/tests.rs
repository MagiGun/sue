//! Tests of the inputs, driven through real egui frames: the pointer, the
//! keys and the wheel go in as events, as a window would send them.

use egui::output::OutputEvent;
use egui::{Context, Event, FullOutput, MouseWheelSource, PointerButton, RawInput, TouchPhase};
use sue_rice::RiceSchema;

use super::*;
use crate::ui::Motion;

// ----- A window to drive -------------------------------------------------------

/// One egui context in Sue's default theme, with time that only moves
/// when a frame runs or a test waits. Transitions are instant, so a look
/// is at its end the frame it is asked for.
pub(in crate::ui) struct Harness {
    pub ctx: Context,
    pub theme: Theme,
    time: f64,
}

/// The time between two frames, in seconds.
const FRAME: f64 = 1.0 / 60.0;

impl Harness {
    pub fn new() -> Self {
        let ctx = Context::default();
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the default rice reads");
        crate::register_rice(&mut rice).expect("the rice of sue-wm registers");
        let theme = Theme::from_rice(&rice);
        theme.apply(&ctx, None, None);
        Motion {
            instant: true,
            ..Motion::of(&ctx)
        }
        .store(&ctx);
        Self {
            ctx,
            theme,
            time: 0.0,
        }
    }

    /// Lets `seconds` pass without a frame.
    pub fn wait(&mut self, seconds: f64) {
        self.time += seconds;
    }

    /// Runs one frame with `events`, drawing with `draw`. Returns what
    /// `draw` gave and what egui put out.
    pub fn frame_output<R>(
        &mut self,
        events: Vec<Event>,
        mut draw: impl FnMut(&mut Ui, &Theme) -> R,
    ) -> (R, FullOutput) {
        self.time += FRAME;
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } => {
                    Some(*modifiers)
                }
                _ => None,
            })
            .unwrap_or_default();
        let mut all = vec![Event::ModifiersChanged(modifiers)];
        all.extend(events);
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            time: Some(self.time),
            predicted_dt: FRAME as f32,
            events: all,
            ..RawInput::default()
        };
        let theme = self.theme.clone();
        let mut result = None;
        let mut output = self.ctx.run_ui(input, |ui| result = Some(draw(ui, &theme)));
        output.textures_delta.clear();
        (result.expect("the frame ran"), output)
    }

    /// Runs one frame with `events`, drawing with `draw`.
    pub fn frame<R>(&mut self, events: Vec<Event>, draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
        self.frame_output(events, draw).0
    }

    /// Clicks at `at` over two frames. Returns what the second gave.
    /// Clicks at `at` over three frames: the pointer comes, goes down
    /// and up. Returns what the last gave. (egui finds what is under the
    /// pointer among what the frame before drew.)
    pub fn click<R>(&mut self, at: Pos2, mut draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
        self.frame(vec![moved(at)], &mut draw);
        self.frame(vec![button(at, true)], &mut draw);
        self.frame(vec![button(at, false)], &mut draw)
    }

    /// Presses at `from`, drags to `to` holding `modifiers`, and lets go,
    /// over several frames. Returns what the last frame gave.
    pub fn drag<R>(
        &mut self,
        from: Pos2,
        to: Pos2,
        modifiers: Modifiers,
        mut draw: impl FnMut(&mut Ui, &Theme) -> R,
    ) -> R {
        let held = |event: Event| vec![Event::ModifiersChanged(modifiers), event];
        self.frame(held(moved(from)), &mut draw);
        self.frame(held(button_with(from, true, modifiers)), &mut draw);
        for step in 1..=4 {
            let at = from + (to - from) * (step as f32 / 4.0);
            self.frame(held(moved(at)), &mut draw);
        }
        self.frame(held(button_with(to, false, modifiers)), &mut draw)
    }

    /// Presses `key` alone in one frame.
    pub fn key<R>(&mut self, key: Key, draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
        self.frame(vec![press(key, Modifiers::NONE)], draw)
    }
}

pub(in crate::ui) fn moved(at: Pos2) -> Event {
    Event::PointerMoved(at)
}

pub(in crate::ui) fn button(at: Pos2, pressed: bool) -> Event {
    button_with(at, pressed, Modifiers::NONE)
}

pub(in crate::ui) fn button_with(at: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

pub(in crate::ui) fn press(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

pub(in crate::ui) fn typed(text: &str) -> Event {
    Event::Text(text.to_owned())
}

/// A wheel turned `lines` notches, down (toward the end of a list) for
/// a positive count.
pub(in crate::ui) fn wheel(lines: f32) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        // egui's delta moves the content: a wheel turned down moves it up.
        delta: vec2(0.0, -lines),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    }
}

/// Every widget event egui put out in `output`.
pub(in crate::ui) fn widget_events(output: &FullOutput) -> Vec<&WidgetInfo> {
    output
        .platform_output
        .events
        .iter()
        .map(OutputEvent::widget_info)
        .collect()
}

/// The place most controls of these tests are drawn at.
fn place() -> Rect {
    Rect::from_min_size(
        pos2(100.0, 100.0),
        vec2(200.0, Density::Regular.control_height()),
    )
}

// ----- Numbers as they are typed -----------------------------------------------

#[test]
fn numbers_are_read_the_way_english_writes_them() {
    let _english = sue_lang::scope("en");
    assert_eq!(parse_number("12", ""), Some(12.0));
    assert_eq!(parse_number(" 1.5 ", ""), Some(1.5));
    assert_eq!(parse_number("1,200", ""), Some(1200.0));
    assert_eq!(parse_number("1,200.25", ""), Some(1200.25));
    assert_eq!(parse_number("-3", ""), Some(-3.0));
    assert_eq!(parse_number("\u{2212}3", ""), Some(-3.0));
    assert_eq!(parse_number("30 px", "px"), Some(30.0));
    assert_eq!(parse_number("30px", "px"), Some(30.0));
    // A comma that cannot be a group separator is no number.
    assert_eq!(parse_number("1,5", ""), None);
    for typo in ["", " ", "x", "12x", "1.2.3", "px", "inf", "NaN", "--1"] {
        assert_eq!(parse_number(typo, "px"), None, "{typo:?}");
    }
}

#[test]
fn numbers_are_read_the_way_german_and_french_write_them() {
    // The separators come from the language database, which loads on
    // its first read: a scope alone does not read it, so a test that runs
    // first or alone would find English separators.
    let _ = sue_lang::current();
    let _german = sue_lang::scope("de");
    assert_eq!(decimal_separator(), ",");
    assert_eq!(parse_number("1,5", ""), Some(1.5));
    assert_eq!(parse_number("1.200", ""), Some(1200.0));
    assert_eq!(parse_number("1.200,5", ""), Some(1200.5));
    // A point that cannot be a group separator is a decimal point.
    assert_eq!(parse_number("1.5", ""), Some(1.5));
    assert_eq!(plain_number(2.5, 1), "2,5");
    drop(_german);
    let _french = sue_lang::scope("fr");
    assert_eq!(parse_number("1 200,5", ""), Some(1200.5));
    assert_eq!(parse_number(&sue_lang::number(1200.5, 1), ""), Some(1200.5));
}

#[test]
fn a_number_given_to_be_typed_over_has_no_groups_and_no_negative_zero() {
    let _english = sue_lang::scope("en");
    assert_eq!(plain_number(1200.0, 0), "1200");
    assert_eq!(plain_number(12.3456, 2), "12.35");
    assert_eq!(plain_number(-0.001, 2), "0.00");
    assert_eq!(plain_number(-2.0, 0), "-2");
    for value in [0.0, 7.0, -12.5, 1234.5678, 1e6] {
        let text = plain_number(value, 4);
        assert_eq!(parse_number(&text, ""), Some((value * 1e4).round() / 1e4));
    }
}

// ----- Lists -------------------------------------------------------------------

#[test]
fn stepping_through_a_list_stops_at_its_ends() {
    assert_eq!(step_index(2, 1, 5), 3);
    assert_eq!(step_index(2, -5, 5), 0);
    assert_eq!(step_index(4, 9, 5), 5);
    assert_eq!(step_index(9, 0, 5), 5);
    assert_eq!(step_index(0, -1, 0), 0);
}

#[test]
fn the_first_answer_that_starts_with_what_is_typed_is_found() {
    let options = ["Normal", "Multiply", "Screen", "Soft Light"];
    assert_eq!(first_match(&options, "s"), Some(2));
    assert_eq!(first_match(&options, "SOFT"), Some(3));
    assert_eq!(first_match(&options, " mul "), Some(1));
    assert_eq!(first_match(&options, ""), None);
    assert_eq!(first_match(&options, "x"), None);
}

#[test]
fn a_list_hangs_below_its_field_or_above_it_and_stays_on_the_screen() {
    let harness = Harness::new();
    let metrics = ListMetrics::new(&harness.theme, Density::Regular);
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let full = 6.0 * metrics.row + metrics.padding * 2.0;
    // Room below: it hangs below, as tall as its rows.
    let anchor = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 30.0));
    let list = list_rect(anchor, screen, 150.0, 6, &metrics);
    assert!(list.top() > anchor.bottom());
    assert_eq!(list.height(), full);
    assert_eq!(
        list.width(),
        anchor.width(),
        "never narrower than its field"
    );
    // Near the bottom: it goes above.
    let low = anchor.translate(vec2(0.0, 440.0));
    let list = list_rect(low, screen, 150.0, 6, &metrics);
    assert!(list.bottom() < low.top());
    assert_eq!(list.height(), full);
    // A long list is cut to the rows it may show, and a wide one is
    // moved left to stay inside the screen.
    let right = Rect::from_min_size(pos2(700.0, 100.0), vec2(80.0, 30.0));
    let list = list_rect(right, screen, 300.0, 40, &metrics);
    assert!(list.height() <= LIST_ROWS as f32 * metrics.row + metrics.padding * 2.0);
    assert!(screen.contains_rect(list), "{list:?}");
}

// ----- The text field ----------------------------------------------------------

#[test]
fn a_text_field_takes_typing_and_says_it_changed() {
    let mut harness = Harness::new();
    let mut text = String::new();
    let draw = |ui: &mut Ui, theme: &Theme, text: &mut String| {
        text_field("name", text)
            .hint("Name")
            .show_at(ui, place(), theme)
    };
    harness.click(place().center(), |ui, theme| draw(ui, theme, &mut text));
    let response = harness.frame(vec![typed("Sue")], |ui, theme| draw(ui, theme, &mut text));
    assert_eq!(text, "Sue");
    assert!(response.changed());
    assert!(response.has_focus());
    let response = harness.frame(vec![], |ui, theme| draw(ui, theme, &mut text));
    assert!(!response.changed());
}

#[test]
fn a_disabled_text_field_takes_no_typing() {
    let mut harness = Harness::new();
    let mut text = "Kept".to_owned();
    let draw = |ui: &mut Ui, theme: &Theme, text: &mut String| {
        text_field("name", text)
            .enabled(false)
            .show_at(ui, place(), theme)
    };
    harness.click(place().center(), |ui, theme| draw(ui, theme, &mut text));
    let response = harness.frame(vec![typed("x")], |ui, theme| draw(ui, theme, &mut text));
    assert_eq!(text, "Kept");
    assert!(!response.has_focus());
}

#[test]
fn the_clear_button_of_a_search_field_empties_it_and_keeps_typing_there() {
    let mut harness = Harness::new();
    let mut text = "lin".to_owned();
    let draw = |ui: &mut Ui, theme: &Theme, text: &mut String| {
        search_field("find", text).show_at(ui, place(), theme)
    };
    let clear = trailing_square(place()).center();
    let response = harness.click(clear, |ui, theme| draw(ui, theme, &mut text));
    assert!(text.is_empty());
    assert!(response.changed());
    // The field holds focus again, so typing goes on there.
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut text));
    harness.frame(vec![typed("ink")], |ui, theme| draw(ui, theme, &mut text));
    assert_eq!(text, "ink");
}

#[test]
fn an_empty_search_field_has_no_clear_button() {
    let mut harness = Harness::new();
    let mut text = String::new();
    let draw = |ui: &mut Ui, theme: &Theme, text: &mut String| {
        search_field("find", text).show_at(ui, place(), theme)
    };
    let clear = trailing_square(place()).center();
    let response = harness.click(clear, |ui, theme| draw(ui, theme, &mut text));
    assert!(!response.changed());
    // The click landed in the field and gave it focus instead.
    assert!(response.has_focus());
}

// ----- The number field --------------------------------------------------------

/// Draws a number field from 0 to 100 at [`place`].
fn percent(ui: &mut Ui, theme: &Theme, value: &mut f64) -> Response {
    number_field("amount", value)
        .range(0.0..=100.0)
        .suffix("%")
        .show_at(ui, place(), theme)
}

#[test]
fn dragging_a_number_field_to_the_right_adds_and_shift_slows_it() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let from = place().center();
    // 200 points cover the range of 100: 40 points add 20.
    let response = harness.drag(
        from,
        from + vec2(40.0, 0.0),
        Modifiers::NONE,
        |ui, theme| percent(ui, theme, &mut value),
    );
    assert_eq!(value, 70.0);
    assert!(!response.has_focus(), "a drag does not start typing");
    // Up adds too, and Shift goes ten times slower.
    harness.drag(
        from,
        from - vec2(0.0, 40.0),
        Modifiers::SHIFT,
        |ui, theme| percent(ui, theme, &mut value),
    );
    assert_eq!(value, 72.0);
    // It never leaves its range.
    harness.drag(
        from,
        from + vec2(300.0, 0.0),
        Modifiers::NONE,
        |ui, theme| percent(ui, theme, &mut value),
    );
    assert_eq!(value, 100.0);
}

#[test]
fn a_number_typed_into_a_field_is_taken_when_typing_ends() {
    let _english = sue_lang::scope("en");
    let mut harness = Harness::new();
    let mut value = 50.0;
    harness.click(place().center(), |ui, theme| percent(ui, theme, &mut value));
    // The click starts typing, with all of the number chosen.
    let response = harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
    assert!(response.has_focus());
    let response = harness.frame(vec![typed("42")], |ui, theme| {
        percent(ui, theme, &mut value)
    });
    assert_eq!(value, 50.0, "typing is no change until it ends");
    assert!(!response.changed());
    let response = harness.key(Key::Enter, |ui, theme| percent(ui, theme, &mut value));
    assert_eq!(value, 42.0);
    assert!(response.changed());
}

#[test]
fn a_number_typed_with_its_unit_or_past_its_range_is_taken_in_range() {
    let _english = sue_lang::scope("en");
    let mut harness = Harness::new();
    let mut value = 50.0;
    for (text, taken) in [("30 %", 30.0), ("250", 100.0), ("-4", 0.0), ("12.6", 13.0)] {
        harness.click(place().center(), |ui, theme| percent(ui, theme, &mut value));
        harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
        harness.frame(vec![typed(text)], |ui, theme| {
            percent(ui, theme, &mut value)
        });
        harness.key(Key::Enter, |ui, theme| percent(ui, theme, &mut value));
        assert_eq!(value, taken, "{text:?}");
    }
}

#[test]
fn escape_or_a_typo_leaves_a_number_field_as_it_was() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    for (text, end) in [("9", Key::Escape), ("4x2", Key::Enter)] {
        harness.click(place().center(), |ui, theme| percent(ui, theme, &mut value));
        harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
        harness.frame(vec![typed(text)], |ui, theme| {
            percent(ui, theme, &mut value)
        });
        let response = harness.key(end, |ui, theme| percent(ui, theme, &mut value));
        assert_eq!(value, 50.0, "{text:?}");
        assert!(!response.changed());
        let response = harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
        assert!(!response.has_focus(), "typing ended");
    }
}

#[test]
fn the_arrow_keys_step_a_number_field_while_it_is_typed_in() {
    let mut harness = Harness::new();
    let mut value = 98.0;
    harness.click(place().center(), |ui, theme| percent(ui, theme, &mut value));
    harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
    let response = harness.key(Key::ArrowUp, |ui, theme| percent(ui, theme, &mut value));
    assert_eq!(value, 99.0);
    assert!(response.changed());
    assert!(response.has_focus(), "typing goes on");
    harness.key(Key::ArrowUp, |ui, theme| percent(ui, theme, &mut value));
    harness.key(Key::ArrowUp, |ui, theme| percent(ui, theme, &mut value));
    assert_eq!(value, 100.0, "held in range");
    harness.key(Key::PageDown, |ui, theme| percent(ui, theme, &mut value));
    assert_eq!(value, 90.0);
}

#[test]
fn a_value_changed_from_outside_while_typing_drops_the_typing() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    harness.click(place().center(), |ui, theme| percent(ui, theme, &mut value));
    harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
    harness.frame(vec![typed("7")], |ui, theme| percent(ui, theme, &mut value));
    value = 20.0;
    harness.frame(vec![], |ui, theme| percent(ui, theme, &mut value));
    harness.key(Key::Enter, |ui, theme| percent(ui, theme, &mut value));
    assert_eq!(value, 20.0);
}

#[test]
fn a_disabled_number_field_neither_drags_nor_types() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        number_field("amount", value)
            .range(0.0..=100.0)
            .enabled(false)
            .show_at(ui, place(), theme)
    };
    let from = place().center();
    harness.drag(
        from,
        from + vec2(40.0, 0.0),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut value),
    );
    harness.click(from, |ui, theme| draw(ui, theme, &mut value));
    let response = harness.frame(vec![typed("1")], |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 50.0);
    assert!(!response.has_focus());
}

// ----- The dropdown ------------------------------------------------------------

const BLENDS: [&str; 5] = ["Normal", "Darken", "Multiply", "Screen", "Overlay"];

/// Draws the dropdown of [`BLENDS`] at `at`.
fn blends_at(ui: &mut Ui, theme: &Theme, chosen: &mut usize, at: Rect) -> Response {
    dropdown("blend", chosen, &BLENDS)
        .label("Blend mode")
        .show_at(ui, at, theme)
}

fn blends(ui: &mut Ui, theme: &Theme, chosen: &mut usize) -> Response {
    blends_at(ui, theme, chosen, place())
}

/// The middle of row `index` of the open list of the dropdown at
/// [`place`], which hangs below it.
fn row_center(theme: &Theme, index: usize) -> Pos2 {
    let metrics = ListMetrics::new(theme, Density::Regular);
    let top = place().bottom() + metrics.padding * 2.0;
    pos2(
        place().left() + place().width() / 2.0,
        top + metrics.row * (index as f32 + 0.5),
    )
}

#[test]
fn a_click_opens_a_dropdown_and_the_keys_pick_from_its_list() {
    let mut harness = Harness::new();
    let mut chosen = 1;
    let response = harness.click(place().center(), |ui, theme| blends(ui, theme, &mut chosen));
    assert!(!response.changed());
    let response = harness.frame(vec![], |ui, theme| blends(ui, theme, &mut chosen));
    let id = response.id;
    let open = |ctx: &Context| list_state(ctx, id).open;
    assert!(open(&harness.ctx));
    harness.key(Key::ArrowDown, |ui, theme| blends(ui, theme, &mut chosen));
    harness.key(Key::ArrowDown, |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(chosen, 1, "walking the list changes nothing yet");
    let response = harness.key(Key::Enter, |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(chosen, 3);
    assert!(response.changed());
    assert!(!open(&harness.ctx));
    assert!(response.has_focus(), "focus goes back to the dropdown");
}

#[test]
fn escape_and_a_click_outside_close_a_dropdown_unchanged() {
    let mut harness = Harness::new();
    let mut chosen = 2;
    for close in [None, Some(pos2(600.0, 500.0)), Some(place().center())] {
        harness.click(place().center(), |ui, theme| blends(ui, theme, &mut chosen));
        let response = harness.key(Key::ArrowDown, |ui, theme| blends(ui, theme, &mut chosen));
        assert!(list_state(&harness.ctx, response.id).open);
        let response = match close {
            None => harness.key(Key::Escape, |ui, theme| blends(ui, theme, &mut chosen)),
            Some(at) => harness.click(at, |ui, theme| blends(ui, theme, &mut chosen)),
        };
        assert!(!list_state(&harness.ctx, response.id).open, "{close:?}");
        assert_eq!(chosen, 2);
        assert!(!response.changed());
    }
}

#[test]
fn a_click_on_a_row_picks_it() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    harness.click(place().center(), |ui, theme| blends(ui, theme, &mut chosen));
    harness.frame(vec![], |ui, theme| blends(ui, theme, &mut chosen));
    let at = row_center(&harness.theme, 4);
    let response = harness.click(at, |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(chosen, 4);
    assert!(response.changed());
    assert!(!list_state(&harness.ctx, response.id).open);
}

#[test]
fn the_keys_step_a_closed_dropdown_that_holds_focus() {
    let mut harness = Harness::new();
    let mut chosen = 2;
    let response = harness.key(Key::Tab, |ui, theme| blends(ui, theme, &mut chosen));
    let response = if response.has_focus() {
        response
    } else {
        harness.frame(vec![], |ui, theme| blends(ui, theme, &mut chosen))
    };
    assert!(response.has_focus(), "Tab reaches the dropdown");
    for (key, expected) in [
        (Key::ArrowDown, 3),
        (Key::ArrowRight, 4),
        (Key::ArrowDown, 4),
        (Key::ArrowUp, 3),
        (Key::Home, 0),
        (Key::End, 4),
    ] {
        let response = harness.key(key, |ui, theme| blends(ui, theme, &mut chosen));
        assert_eq!(chosen, expected, "{key:?}");
        assert!(!list_state(&harness.ctx, response.id).open);
    }
    // Space opens it on the chosen row.
    let response = harness.key(Key::Space, |ui, theme| blends(ui, theme, &mut chosen));
    let state = list_state(&harness.ctx, response.id);
    assert!(state.open);
    assert_eq!(state.highlight, 4);
}

#[test]
fn a_letter_goes_to_the_next_answer_that_starts_with_it() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    harness.click(place().center(), |ui, theme| blends(ui, theme, &mut chosen));
    let response = harness.frame(vec![typed("o")], |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(list_state(&harness.ctx, response.id).highlight, 4);
    let response = harness.frame(vec![typed("D")], |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(list_state(&harness.ctx, response.id).highlight, 1);
}

#[test]
fn the_wheel_steps_a_dropdown_once_the_pointer_rests_on_it() {
    let mut harness = Harness::new();
    let mut chosen = 2;
    let over = place().center();
    harness.frame(vec![moved(over)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    // At once, the wheel is still the page's.
    let response = harness.frame(vec![wheel(1.0)], |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(chosen, 2);
    assert!(!response.changed());
    harness.wait(WHEEL_SETTLE);
    // Each notch is one step, and each step takes effect at once.
    let response = harness.frame(vec![wheel(1.0)], |ui, theme| blends(ui, theme, &mut chosen));
    assert_eq!(chosen, 3);
    assert!(response.changed(), "what it drives previews the step");
    harness.frame(vec![wheel(-2.0)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    assert_eq!(chosen, 1);
    harness.frame(vec![wheel(-5.0)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    assert_eq!(chosen, 0, "it stops at the first answer");
    assert!(
        !list_state(&harness.ctx, response.id).open,
        "it does not open"
    );
}

#[test]
fn a_trackpad_steps_a_dropdown_by_the_height_of_a_row() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    let over = place().center();
    for _ in 0..2 {
        harness.frame(vec![moved(over)], |ui, theme| {
            blends(ui, theme, &mut chosen)
        });
    }
    harness.wait(WHEEL_SETTLE);
    let row = Density::Regular.row();
    let swipe = |points: f32| Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -points),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Trackpad,
        modifiers: Modifiers::NONE,
    };
    harness.frame(vec![swipe(row * 0.6)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    assert_eq!(chosen, 0);
    harness.frame(vec![swipe(row * 0.6)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    assert_eq!(chosen, 1, "the points add up between frames");
}

/// A page 300 points high that scrolls, holding a dropdown `above`
/// points down and much more below it. Returns where the dropdown lies
/// and how far the page is scrolled.
fn page(ui: &mut Ui, theme: &Theme, chosen: &mut usize, above: f32) -> (Rect, f32) {
    let output = ScrollArea::vertical()
        .id_salt("page")
        .max_height(300.0)
        .show(ui, |ui| {
            ui.add_space(above);
            let response = dropdown("blend", chosen, &BLENDS).show(ui, theme);
            ui.add_space(2000.0);
            response.rect
        });
    (output.inner, output.state.clamped_offset().y)
}

#[test]
fn a_page_scrolled_with_the_wheel_goes_on_scrolling_over_a_dropdown() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    let (start, _) = harness.frame(vec![], |ui, theme| page(ui, theme, &mut chosen, 200.0));
    // The pointer rests above the dropdown, which the page scrolls up
    // under it and past it.
    let pointer = start.center() - vec2(0.0, 120.0);
    let mut under = 0.0;
    let mut last = 0.0;
    for _ in 0..400 {
        let (rect, offset) = harness.frame(vec![moved(pointer), wheel(0.05)], |ui, theme| {
            page(ui, theme, &mut chosen, 200.0)
        });
        if rect.contains(pointer) {
            under += FRAME + 0.03;
        }
        assert!(offset >= last, "the page kept scrolling");
        last = offset;
        harness.wait(0.03);
    }
    assert!(
        under > WHEEL_SETTLE * 2.0,
        "the dropdown was under the pointer longer than the wait: {under}"
    );
    assert!(last > 120.0, "the page scrolled past it: {last}");
    assert_eq!(chosen, 0, "the dropdown was not stepped");
}

#[test]
fn the_wheel_over_a_resting_dropdown_steps_it_and_not_the_page() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    let (rect, _) = harness.frame(vec![], |ui, theme| page(ui, theme, &mut chosen, 40.0));
    for _ in 0..2 {
        harness.frame(vec![moved(rect.center())], |ui, theme| {
            page(ui, theme, &mut chosen, 40.0)
        });
    }
    harness.wait(WHEEL_SETTLE);
    harness.frame(vec![wheel(1.0)], |ui, theme| {
        page(ui, theme, &mut chosen, 40.0)
    });
    assert_eq!(chosen, 1);
    for _ in 0..30 {
        let (_, offset) = harness.frame(vec![], |ui, theme| page(ui, theme, &mut chosen, 40.0));
        assert_eq!(offset, 0.0, "the page did not scroll");
    }
}

#[test]
fn a_disabled_dropdown_neither_opens_nor_steps() {
    let mut harness = Harness::new();
    let mut chosen = 2;
    let draw = |ui: &mut Ui, theme: &Theme, chosen: &mut usize| {
        dropdown("blend", chosen, &BLENDS)
            .enabled(false)
            .show_at(ui, place(), theme)
    };
    let response = harness.click(place().center(), |ui, theme| draw(ui, theme, &mut chosen));
    assert!(!list_state(&harness.ctx, response.id).open);
    harness.wait(WHEEL_SETTLE);
    harness.frame(vec![wheel(1.0)], |ui, theme| draw(ui, theme, &mut chosen));
    harness.wait(WHEEL_SETTLE);
    harness.frame(vec![wheel(1.0)], |ui, theme| draw(ui, theme, &mut chosen));
    assert_eq!(chosen, 2);
}

#[test]
fn a_dropdown_keeps_its_state_by_its_salt_wherever_it_is_drawn() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    harness.click(place().center(), |ui, theme| blends(ui, theme, &mut chosen));
    // Drawn somewhere else, after another control, it is still open.
    let elsewhere = place().translate(vec2(150.0, 200.0));
    let (other, response) = harness.frame(vec![], |ui, theme| {
        let mut other = 0;
        let other = dropdown("other", &mut other, &BLENDS).show_at(ui, place(), theme);
        (other.id, blends_at(ui, theme, &mut chosen, elsewhere))
    });
    assert!(list_state(&harness.ctx, response.id).open);
    assert!(!list_state(&harness.ctx, other).open);
}

#[test]
fn a_dropdown_tells_assistive_technology_its_name_and_answer() {
    let mut harness = Harness::new();
    let mut chosen = 2;
    let (_, output) = harness.frame_output(vec![press(Key::Tab, Modifiers::NONE)], |ui, theme| {
        blends(ui, theme, &mut chosen)
    });
    let (_, later) = harness.frame_output(vec![], |ui, theme| blends(ui, theme, &mut chosen));
    let events: Vec<&WidgetInfo> = widget_events(&output)
        .into_iter()
        .chain(widget_events(&later))
        .collect();
    let info = events
        .iter()
        .find(|info| info.role == Role::ComboBox)
        .expect("focus reached the dropdown");
    assert_eq!(info.label.as_deref(), Some("Blend mode"));
    assert_eq!(info.current_text_value.as_deref(), Some("Multiply"));
    assert_eq!(info.selected, Some(false), "closed");
}

#[test]
fn a_dropdown_that_got_focus_by_keyboard_shows_its_ring_and_a_click_does_not() {
    let mut harness = Harness::new();
    let mut chosen = 0;
    let look = |ui: &mut Ui, theme: &Theme, chosen: &mut usize| {
        let response = blends(ui, theme, chosen);
        Look::of(ui, &response, true, false).focus
    };
    harness.key(Key::Tab, |ui, theme| look(ui, theme, &mut chosen));
    let ring = harness.frame(vec![], |ui, theme| look(ui, theme, &mut chosen));
    assert_eq!(ring, 1.0);
    // A click ends the showing of keyboard focus; one that opens the list
    // and one that picks a row give focus without a ring.
    harness.click(pos2(700.0, 500.0), |ui, theme| look(ui, theme, &mut chosen));
    harness.click(place().center(), |ui, theme| look(ui, theme, &mut chosen));
    harness.frame(vec![], |ui, theme| look(ui, theme, &mut chosen));
    let row = row_center(&harness.theme, 2);
    harness.click(row, |ui, theme| look(ui, theme, &mut chosen));
    assert_eq!(chosen, 2);
    let ring = harness.frame(vec![], |ui, theme| look(ui, theme, &mut chosen));
    assert_eq!(ring, 0.0);
}

// ----- The combo ---------------------------------------------------------------

const SIZES: [&str; 5] = ["8", "10", "12", "14", "18"];

fn sizes(ui: &mut Ui, theme: &Theme, text: &mut String) -> Response {
    combo("size", text, &SIZES).show_at(ui, place(), theme)
}

#[test]
fn a_combo_takes_any_text_and_offers_its_list_on_down() {
    let mut harness = Harness::new();
    let mut text = String::new();
    harness.click(place().left_center() + vec2(20.0, 0.0), |ui, theme| {
        sizes(ui, theme, &mut text)
    });
    let response = harness.frame(vec![typed("13")], |ui, theme| sizes(ui, theme, &mut text));
    assert_eq!(text, "13", "a value outside the list is fine");
    assert!(response.changed());
    // Down opens the list; what is typed picks the row it starts on.
    harness.frame(vec![], |ui, theme| sizes(ui, theme, &mut text));
    for _ in 0..2 {
        harness.key(Key::Backspace, |ui, theme| sizes(ui, theme, &mut text));
    }
    harness.frame(vec![typed("1")], |ui, theme| sizes(ui, theme, &mut text));
    let response = harness.key(Key::ArrowDown, |ui, theme| sizes(ui, theme, &mut text));
    let state = list_state(&harness.ctx, response.id);
    assert!(state.open);
    assert_eq!(state.highlight, 1, "the first answer starting with 1");
    harness.key(Key::ArrowDown, |ui, theme| sizes(ui, theme, &mut text));
    let response = harness.key(Key::Enter, |ui, theme| sizes(ui, theme, &mut text));
    assert_eq!(text, "12");
    assert!(response.changed());
    assert!(!list_state(&harness.ctx, response.id).open);
    assert!(response.has_focus(), "typing goes on after a pick");
}

#[test]
fn the_button_of_a_combo_opens_its_list_and_a_row_click_fills_it() {
    let mut harness = Harness::new();
    let mut text = "9".to_owned();
    let button = trailing_square(place()).center();
    let response = harness.click(button, |ui, theme| sizes(ui, theme, &mut text));
    assert!(list_state(&harness.ctx, response.id).open);
    harness.frame(vec![], |ui, theme| sizes(ui, theme, &mut text));
    let at = row_center(&harness.theme, 3);
    let response = harness.click(at, |ui, theme| sizes(ui, theme, &mut text));
    assert_eq!(text, "14");
    assert!(response.changed());
    assert!(!list_state(&harness.ctx, response.id).open);
    // Escape closes an open list and keeps what is typed.
    harness.click(button, |ui, theme| sizes(ui, theme, &mut text));
    let response = harness.key(Key::Escape, |ui, theme| sizes(ui, theme, &mut text));
    assert!(!list_state(&harness.ctx, response.id).open);
    assert_eq!(text, "14");
    let response = harness.frame(vec![], |ui, theme| sizes(ui, theme, &mut text));
    assert!(response.has_focus(), "typing goes on after Escape");
}

// ----- Every component, laid out ------------------------------------------------

#[test]
fn every_input_fits_its_density_and_the_review_sheets_draw() {
    let mut harness = Harness::new();
    for density in Density::ALL {
        let heights = harness.frame(vec![], |ui, theme| {
            density.scope(ui, |ui| {
                let mut text = "Text".to_owned();
                let mut number = 3.0;
                let mut chosen = 0;
                [
                    text_field("text", &mut text).show(ui, theme).rect.height(),
                    search_field("search", &mut text)
                        .show(ui, theme)
                        .rect
                        .height(),
                    number_field("number", &mut number)
                        .show(ui, theme)
                        .rect
                        .height(),
                    dropdown("drop", &mut chosen, &BLENDS)
                        .show(ui, theme)
                        .rect
                        .height(),
                    combo("combo", &mut text, &SIZES)
                        .show(ui, theme)
                        .rect
                        .height(),
                ]
            })
        });
        for height in heights {
            assert_eq!(height, density.control_height(), "{density:?}");
        }
    }
    for page in Sheet::ALL {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 1200.0));
        harness.frame(vec![], |ui, theme| sheet(ui.painter(), rect, theme, page));
        assert!(!page.name().is_empty());
    }
}

// ----- The picker box -----------------------------------------------------------

/// Draws a picker box showing "Georgia", open while `open`.
fn font_box(ui: &mut Ui, theme: &Theme, open: bool) -> Response {
    picker_box(ui, "font", "Georgia", "Font", open, 160.0, theme)
}

#[test]
fn a_picker_box_is_clicked_named_and_says_what_it_shows() {
    let mut harness = Harness::new();
    let response = harness.frame(vec![], |ui, theme| font_box(ui, theme, false));
    assert_eq!(response.rect.width(), 160.0);
    let at = response.rect.center();
    let response = harness.click(at, |ui, theme| font_box(ui, theme, false));
    assert!(response.clicked(), "a click reaches the caller");
    let response = harness.frame(vec![], |ui, theme| font_box(ui, theme, true));
    assert!(response.has_focus(), "a click gives it the keys");
    let response = harness.key(Key::Enter, |ui, theme| font_box(ui, theme, false));
    assert!(
        response.clicked(),
        "Enter while it holds focus opens it too"
    );
    harness.ctx.enable_accesskit();
    let (_, output) = harness.frame_output(vec![], |ui, theme| font_box(ui, theme, true));
    let node = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .map(|(_, node)| node)
        .find(|node| node.label() == Some("Font"))
        .expect("the box is named");
    assert_eq!(node.role(), egui::accesskit::Role::ComboBox);
    assert_eq!(node.value(), Some("Georgia"));
}
