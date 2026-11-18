//! Tests of the dialog host, the sheet and the forms, driven through real
//! egui frames.

use egui::{Event, Pos2, RawInput, Rect, Ui, ViewportId, pos2, vec2};
use sue_rice::{RiceSchema, RiceValue};

use super::*;

/// A context with Sue's theme applied, and a clock and a screen of its
/// own. Shared with the tests of the toast stack.
pub(in crate::ui) struct Bench {
    pub ctx: Context,
    pub theme: Theme,
    pub screen: Rect,
    pub time: f64,
    /// The size of the screen the window is on, when the system says.
    pub monitor: Option<egui::Vec2>,
}

/// What a frame gave besides what the drawing returned.
pub(in crate::ui) struct Frame<R> {
    pub out: R,
    /// The frame asked to be followed at once.
    pub repaint_now: bool,
    /// How long until the frame asked to be followed.
    pub repaint_after: std::time::Duration,
}

/// One frame's length on the bench, in seconds.
pub(in crate::ui) const STEP: f64 = 1.0 / 60.0;

impl Bench {
    /// An 800 by 600 screen in the shipped theme.
    pub fn new() -> Self {
        Self::with(vec2(800.0, 600.0), &[])
    }

    /// A screen of `size` with the settings `set` changed.
    pub fn with(size: egui::Vec2, set: &[(&str, RiceValue)]) -> Self {
        let ctx = Context::default();
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped rice parses");
        crate::register_rice(&mut rice).expect("the window manager's keys register");
        for (key, value) in set {
            rice.set(key, value.clone()).expect("a known setting");
        }
        let theme = Theme::from_rice(&rice);
        theme.apply(&ctx, None, None);
        Self {
            ctx,
            theme,
            screen: Rect::from_min_size(Pos2::ZERO, size),
            time: 1.0,
            monitor: None,
        }
    }

    /// With transitions that take no time.
    pub fn instant() -> Self {
        Self::with(
            vec2(800.0, 600.0),
            &[("motion.instant_transitions", RiceValue::Bool(true))],
        )
    }

    /// Runs one frame with `events`, then moves the clock on.
    pub fn frame<R>(
        &mut self,
        events: Vec<Event>,
        mut draw: impl FnMut(&mut Ui, &Theme) -> R,
    ) -> Frame<R> {
        let theme = self.theme.clone();
        let mut input = RawInput {
            screen_rect: Some(self.screen),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(input.viewport_id)
            .or_default()
            .monitor_size = self.monitor;
        let mut out = None;
        let mut done = self.ctx.run_ui(input, |ui| out = Some(draw(ui, &theme)));
        done.textures_delta.clear();
        self.time += STEP;
        let delay = done
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |viewport| viewport.repaint_delay);
        Frame {
            out: out.expect("the frame drew"),
            repaint_now: delay.is_zero(),
            repaint_after: delay,
        }
    }

    /// Runs `count` frames without input and returns the last one's.
    pub fn frames<R>(&mut self, count: usize, mut draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
        let mut last = None;
        for _ in 0..count {
            last = Some(self.frame(Vec::new(), &mut draw).out);
        }
        last.expect("at least one frame")
    }

    /// Moves the pointer to `at`, presses and lets go, a frame each.
    /// Returns what each of the three frames drew.
    pub fn click<R>(&mut self, at: Pos2, mut draw: impl FnMut(&mut Ui, &Theme) -> R) -> Vec<R> {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        vec![
            self.frame(vec![Event::PointerMoved(at)], &mut draw).out,
            self.frame(vec![button(true)], &mut draw).out,
            self.frame(vec![button(false)], &mut draw).out,
        ]
    }

    /// Drags from `from` by `by` in `steps` moves, then lets go. Returns
    /// what each frame drew.
    pub fn drag<R>(
        &mut self,
        from: Pos2,
        by: egui::Vec2,
        steps: usize,
        mut draw: impl FnMut(&mut Ui, &Theme) -> R,
    ) -> Vec<R> {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut drawn = vec![
            self.frame(vec![Event::PointerMoved(from)], &mut draw).out,
            self.frame(vec![button(from, true)], &mut draw).out,
        ];
        let mut at = from;
        for _ in 0..steps {
            at += by / steps as f32;
            drawn.push(self.frame(vec![Event::PointerMoved(at)], &mut draw).out);
        }
        drawn.push(self.frame(vec![button(at, false)], &mut draw).out);
        drawn
    }
}

/// A key pressed with `modifiers`.
pub(in crate::ui) fn key_with(key: Key, modifiers: Modifiers) -> Vec<Event> {
    vec![
        Event::ModifiersChanged(modifiers),
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
    ]
}

/// A key pressed alone.
pub(in crate::ui) fn key(key: Key) -> Vec<Event> {
    key_with(key, Modifiers::NONE)
}

/// The dialog most tests show: OK and Cancel.
fn plain(theme: &Theme) -> Dialog<'_> {
    Dialog::new("test", "Rename Layer", theme).ok_cancel()
}

/// Shows `dialog` with an empty body and returns its response.
fn show(ui: &Ui, dialog: Dialog<'_>) -> DialogResponse<()> {
    dialog.show(ui.ctx(), |ui| {
        ui.label("Body");
    })
}

#[test]
fn ok_and_cancel_are_read_from_the_language_database() {
    let bench = Bench::new();
    let dialog = plain(&bench.theme);
    assert_eq!(dialog.label(Answer::Accept), Some(tr!("ok").as_str()));
    assert_eq!(dialog.label(Answer::Cancel), Some(tr!("cancel").as_str()));
    // English is the language of the tests.
    assert_eq!(dialog.label(Answer::Accept), Some("OK"));
    assert_eq!(dialog.label(Answer::Cancel), Some("Cancel"));
    // Every language has the four words of this file, in wm.ftl.
    for key in ["ok", "cancel", "dialog-close", "toast-dismiss", "notices"] {
        assert_ne!(tr!(key), key, "{key} has English text");
    }
}

#[test]
fn the_footer_stands_in_one_order() {
    let mut bench = Bench::new();
    let response = bench.frames(3, |ui, theme| {
        show(
            ui,
            Dialog::new("order", "Save Changes?", theme)
                .cancel()
                .alternate("Do Not Save")
                .extra("Help")
                .accept("Save"),
        )
    });
    let [extra, accept, alternate, cancel] =
        Answer::ORDER.map(|answer| response.button_rect(answer).expect("every button is drawn"));
    let margin = bench.theme.dialog_margin();
    // The extra action alone at the leading edge, the rest together at
    // the trailing one: Save, Do Not Save, Cancel.
    assert!((extra.left() - (response.rect.left() + margin)).abs() < 1.5);
    assert!(extra.right() < accept.left());
    assert!(accept.right() < alternate.left());
    assert!(alternate.right() < cancel.left());
    assert!((cancel.right() - (response.rect.right() - margin)).abs() < 1.5);
    let gap = bench.theme.space(Space::S4);
    assert!((alternate.left() - accept.right() - gap).abs() < 0.5);
    for rect in [extra, accept, alternate] {
        assert_eq!(rect.top(), cancel.top(), "one row");
        assert_eq!(rect.height(), Density::Regular.control_height());
    }
    // The footer is the last thing in the dialog.
    assert!((response.rect.bottom() - cancel.bottom() - margin).abs() < 1.5);
}

#[test]
fn a_footer_too_narrow_for_a_row_stacks_its_buttons() {
    let mut bench = Bench::with(vec2(300.0, 600.0), &[]);
    let response = bench.frames(3, |ui, theme| {
        show(
            ui,
            Dialog::new("narrow", "Close the Picture", theme)
                .accept("Save Everything")
                .alternate("Close Without Saving")
                .extra("Help")
                .cancel(),
        )
    });
    let rect = |answer| response.button_rect(answer).expect("drawn");
    // The accepting button on top, then the other answer, Cancel, and the
    // extra action last; each as wide as the footer.
    assert!(rect(Answer::Accept).bottom() < rect(Answer::Alternate).top());
    assert!(rect(Answer::Alternate).bottom() < rect(Answer::Cancel).top());
    assert!(rect(Answer::Cancel).bottom() < rect(Answer::Extra).top());
    for answer in Answer::ORDER {
        assert_eq!(rect(answer).width(), rect(Answer::Accept).width());
        assert!(response.rect.contains_rect(rect(answer)));
    }
    assert!(response.rect.width() <= 300.0);
}

#[test]
fn escape_cancels() {
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, plain(theme)));
    let response = bench.frame(key(Key::Escape), |ui, theme| show(ui, plain(theme)));
    assert_eq!(response.out.answer, Some(Answer::Cancel));
}

#[test]
fn enter_accepts() {
    // With the first focus on the button that accepts.
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, plain(theme)));
    let accept = plain(&bench.theme).button_id(Answer::Accept);
    assert_eq!(bench.ctx.memory(|memory| memory.focused()), Some(accept));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, plain(theme)));
    assert_eq!(response.out.answer, Some(Answer::Accept));
    // And with no focus at all.
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, plain(theme)));
    bench
        .ctx
        .memory_mut(|memory| memory.surrender_focus(accept));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, plain(theme)));
    assert_eq!(response.out.answer, Some(Answer::Accept));
    // Nothing is answered without a key.
    let response = bench.frame(Vec::new(), |ui, theme| show(ui, plain(theme)));
    assert_eq!(response.out.answer, None);
}

#[test]
fn a_dialog_that_destroys_starts_on_cancel_and_enter_does_not_accept() {
    fn danger(theme: &Theme) -> Dialog<'_> {
        plain(theme).danger()
    }
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, danger(theme)));
    let cancel = plain(&bench.theme).button_id(Answer::Cancel);
    assert_eq!(bench.ctx.memory(|memory| memory.focused()), Some(cancel));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, danger(theme)));
    assert_eq!(response.out.answer, Some(Answer::Cancel));
    // Without focus, Enter does nothing.
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, danger(theme)));
    bench
        .ctx
        .memory_mut(|memory| memory.surrender_focus(cancel));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, danger(theme)));
    assert_eq!(response.out.answer, None);
    // Once the accepting button holds focus, Enter accepts.
    let accept = plain(&bench.theme).button_id(Answer::Accept);
    bench.ctx.memory_mut(|memory| memory.request_focus(accept));
    bench.frame(Vec::new(), |ui, theme| show(ui, danger(theme)));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, danger(theme)));
    assert_eq!(response.out.answer, Some(Answer::Accept));
}

#[test]
fn a_text_area_keeps_enter_and_a_line_field_gives_it_to_the_dialog() {
    for multiline in [true, false] {
        let mut bench = Bench::new();
        let mut text = String::from("Name");
        let field = egui::Id::unique(("field", multiline));
        let mut draw = |ui: &mut Ui, theme: &Theme| {
            plain(theme).show(ui.ctx(), |ui| {
                let edit = if multiline {
                    egui::TextEdit::multiline(&mut text)
                } else {
                    egui::TextEdit::singleline(&mut text)
                };
                ui.add(edit.id(field));
            })
        };
        bench.frames(2, &mut draw);
        bench.ctx.memory_mut(|memory| memory.request_focus(field));
        bench.frame(Vec::new(), &mut draw);
        assert_eq!(bench.ctx.memory(|memory| memory.focused()), Some(field));
        let response = bench.frame(key(Key::Enter), &mut draw).out;
        let answer = if multiline {
            None
        } else {
            Some(Answer::Accept)
        };
        assert_eq!(response.answer, answer, "multiline: {multiline}");
    }
}

#[test]
fn a_dialog_whose_body_takes_enter_is_not_accepted_by_it() {
    fn quiet(theme: &Theme) -> Dialog<'_> {
        plain(theme).enter_is_the_bodys()
    }
    let mut bench = Bench::new();
    bench.frames(2, |ui, theme| show(ui, quiet(theme)));
    let accept = plain(&bench.theme).button_id(Answer::Accept);
    bench
        .ctx
        .memory_mut(|memory| memory.surrender_focus(accept));
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, quiet(theme)));
    assert_eq!(response.out.answer, None);
}

#[test]
fn a_button_that_cannot_be_used_does_not_answer() {
    fn off(theme: &Theme) -> Dialog<'_> {
        plain(theme).accept_enabled(false)
    }
    let mut bench = Bench::new();
    let response = bench.frames(3, |ui, theme| show(ui, off(theme)));
    // The first focus skips it.
    let cancel = plain(&bench.theme).button_id(Answer::Cancel);
    assert_eq!(bench.ctx.memory(|memory| memory.focused()), Some(cancel));
    bench
        .ctx
        .memory_mut(|memory| memory.surrender_focus(cancel));
    let at = response
        .button_rect(Answer::Accept)
        .expect("drawn")
        .center();
    let answers: Vec<_> = bench
        .click(at, |ui, theme| show(ui, off(theme)).answer)
        .into_iter()
        .collect();
    assert_eq!(answers, vec![None, None, None]);
    let response = bench.frame(key(Key::Enter), |ui, theme| show(ui, off(theme)));
    assert_eq!(response.out.answer, None);
}

#[test]
fn clicking_a_footer_button_answers_with_it() {
    for answer in Answer::ORDER {
        fn full(theme: &Theme) -> Dialog<'_> {
            Dialog::new("click", "Four Buttons", theme)
                .ok_cancel()
                .alternate("Other")
                .extra("Reset")
        }
        let mut bench = Bench::new();
        let response = bench.frames(3, |ui, theme| show(ui, full(theme)));
        let at = response.button_rect(answer).expect("drawn").center();
        let answers = bench.click(at, |ui, theme| show(ui, full(theme)).answer);
        assert_eq!(answers.last(), Some(&Some(answer)));
    }
}

#[test]
fn the_close_button_cancels_and_can_be_left_out() {
    let mut bench = Bench::new();
    bench.frames(3, |ui, theme| show(ui, plain(theme)));
    let close = plain(&bench.theme).close_id();
    let rect = bench.ctx.read_response(close).expect("drawn").rect;
    let answers = bench.click(rect.center(), |ui, theme| show(ui, plain(theme)).answer);
    assert_eq!(answers.last(), Some(&Some(Answer::Cancel)));

    let mut bench = Bench::new();
    bench.frames(3, |ui, theme| show(ui, plain(theme).no_close_button()));
    assert!(bench.ctx.read_response(close).is_none());
}

#[test]
fn focus_stays_inside_a_modal_dialog() {
    let mut bench = Bench::new();
    let mut clicked_behind = false;
    let mut draw = |ui: &mut Ui, theme: &Theme| {
        let button = ui.add(egui::Button::new("Behind"));
        clicked_behind |= button.clicked();
        show(ui, plain(theme));
        button.id
    };
    let behind = bench.frames(3, &mut draw);
    let dialog = plain(&bench.theme);
    let inside = [
        dialog.button_id(Answer::Accept),
        dialog.button_id(Answer::Cancel),
        dialog.close_id(),
    ];
    let mut seen = std::collections::HashSet::new();
    for shift in [false, true] {
        let modifiers = if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        for _ in 0..8 {
            bench.frame(key_with(Key::Tab, modifiers), &mut draw);
            bench.frame(vec![Event::ModifiersChanged(Modifiers::NONE)], &mut draw);
            let focused = bench.ctx.memory(|memory| memory.focused());
            assert_ne!(focused, Some(behind), "Tab left the dialog");
            if let Some(id) = focused {
                assert!(inside.contains(&id), "focus is on something of the dialog");
                seen.insert(id);
            }
        }
    }
    // Tab went round all three.
    assert_eq!(seen.len(), 3);
    // A click on what is behind reaches nothing.
    let at = bench
        .ctx
        .read_response(behind)
        .expect("drawn")
        .rect
        .center();
    bench.click(at, &mut draw);
    assert!(!clicked_behind);
}

#[test]
fn a_modeless_dialog_leaves_what_is_behind_it_usable() {
    let mut bench = Bench::new();
    let mut clicked_behind = false;
    let mut draw = |ui: &mut Ui, theme: &Theme| {
        let button = ui.add(egui::Button::new("Behind"));
        clicked_behind |= button.clicked();
        show(ui, plain(theme).modeless());
        button.id
    };
    let behind = bench.frames(3, &mut draw);
    let at = bench
        .ctx
        .read_response(behind)
        .expect("drawn")
        .rect
        .center();
    bench.click(at, &mut draw);
    assert!(clicked_behind);
}

/// The plain dialog, whose scrim cancels when `cancels`.
fn scrimmed(theme: &Theme, cancels: bool) -> Dialog<'_> {
    let dialog = plain(theme);
    if cancels {
        dialog.scrim_cancels()
    } else {
        dialog
    }
}

#[test]
fn a_click_on_the_scrim_cancels_only_when_asked() {
    for cancels in [false, true] {
        let mut bench = Bench::new();
        bench.frames(3, |ui, theme| show(ui, scrimmed(theme, cancels)));
        let answers = bench.click(pos2(8.0, 8.0), |ui, theme| {
            show(ui, scrimmed(theme, cancels)).answer
        });
        let expected = cancels.then_some(Answer::Cancel);
        assert_eq!(answers.last(), Some(&expected), "scrim cancels: {cancels}");
        // A click on the panel itself never cancels.
        let mut bench = Bench::new();
        let response = bench.frames(3, |ui, theme| show(ui, scrimmed(theme, cancels)));
        let panel = pos2(response.rect.center().x, response.rect.top() + 4.0);
        let answers = bench.click(panel, |ui, theme| show(ui, scrimmed(theme, cancels)).answer);
        assert_eq!(answers.last(), Some(&None));
    }
}

#[test]
fn a_dialog_sits_in_the_middle_and_fits_the_window() {
    let mut bench = Bench::new();
    let response = bench.frames(40, |ui, theme| show(ui, plain(theme)));
    let screen = bench.screen;
    assert!((response.rect.center().x - screen.center().x).abs() < 1.0);
    assert!((response.rect.center().y - screen.center().y).abs() < 1.0);
    assert_eq!(response.rect.width().round(), WIDTH);
    // A body taller than the window scrolls inside a dialog that fits.
    let response = bench.frames(3, |ui, theme| {
        plain(theme).show(ui.ctx(), |ui| {
            for line in 0..200 {
                ui.label(format!("Line {line}"));
            }
        })
    });
    assert!(screen.contains_rect(response.rect));
    // A narrow window takes a narrower dialog.
    let mut bench = Bench::with(vec2(320.0, 480.0), &[]);
    let response = bench.frames(3, |ui, theme| show(ui, plain(theme)));
    assert!(bench.screen.contains_rect(response.rect));
}

#[test]
fn a_dialog_comes_in_and_then_asks_for_no_frames() {
    let mut bench = Bench::new();
    let first = bench.frame(Vec::new(), |ui, theme| show(ui, plain(theme)));
    assert!(first.repaint_now, "it moves in");
    let id = plain(&bench.theme).id;
    let mut along = 0.0;
    for _ in 0..30 {
        let (_, now) = entered(&bench.ctx, id, Pace::Slow);
        assert!(now >= along, "it only comes nearer");
        along = now;
        bench.frame(Vec::new(), |ui, theme| show(ui, plain(theme)));
    }
    assert_eq!(along, 1.0);
    let rest = bench.frame(Vec::new(), |ui, theme| show(ui, plain(theme)));
    assert!(!rest.repaint_now, "nothing moves, nothing is drawn");
}

#[test]
fn with_instant_transitions_a_dialog_is_in_place_at_once() {
    let mut bench = Bench::instant();
    let id = plain(&bench.theme).id;
    bench.frame(Vec::new(), |ui, _| entered(ui.ctx(), id, Pace::Slow));
    let (appeared, along) = entered(&bench.ctx, id, Pace::Slow);
    assert!(!appeared);
    assert_eq!(along, 1.0);
    // egui sizes a new area in its first frames; after that the instant
    // dialog asks for nothing while the timed one still comes in.
    for (mut bench, moving) in [(Bench::instant(), false), (Bench::new(), true)] {
        let frame = (0..3)
            .map(|_| bench.frame(Vec::new(), |ui, theme| show(ui, plain(theme))))
            .last()
            .expect("three frames");
        assert_eq!(frame.repaint_now, moving);
    }
}

#[test]
fn a_dialog_that_closes_and_opens_again_comes_in_again() {
    let mut bench = Bench::new();
    let id = plain(&bench.theme).id;
    let (appeared, _) = bench
        .frame(Vec::new(), |ui, _| entered(ui.ctx(), id, Pace::Slow))
        .out;
    assert!(appeared);
    let (appeared, _) = bench
        .frame(Vec::new(), |ui, _| entered(ui.ctx(), id, Pace::Slow))
        .out;
    assert!(!appeared);
    // A frame without it: it closed.
    bench.frame(Vec::new(), |_, _| ());
    let (appeared, along) = bench
        .frame(Vec::new(), |ui, _| entered(ui.ctx(), id, Pace::Slow))
        .out;
    assert!(appeared);
    assert_eq!(along, 0.0);
}

#[test]
fn a_window_of_its_own_is_drawn_embedded_where_windows_are_embedded() {
    let mut bench = Bench::new();
    assert!(bench.ctx.embed_viewports());
    fn window(theme: &Theme) -> Dialog<'_> {
        plain(theme).host(DialogHost::Window)
    }
    let response = bench.frames(3, |ui, theme| show(ui, window(theme)));
    assert!(bench.screen.contains_rect(response.rect));
    let response = bench.frame(key(Key::Escape), |ui, theme| show(ui, window(theme)));
    assert_eq!(response.out.answer, Some(Answer::Cancel));
}

#[test]
fn a_window_of_its_own_blocks_its_parent_and_never_outgrows_the_screen() {
    let mut bench = Bench::new();
    bench.monitor = Some(vec2(800.0, 400.0));
    bench.ctx.set_embed_viewports(false);
    let clicked_behind = std::cell::Cell::new(false);
    let mut draw = |ui: &mut Ui, theme: &Theme| {
        let button = ui.add(egui::Button::new("Behind"));
        clicked_behind.set(clicked_behind.get() || button.clicked());
        let response = plain(theme).host(DialogHost::Window).show(ui.ctx(), |ui| {
            // A body far taller than any screen.
            ui.allocate_space(vec2(100.0, 5000.0));
        });
        (button.id, response)
    };
    let (behind, response) = bench.frames(6, &mut draw);
    assert!(response.answer.is_none());
    // Sized from what was measured, and kept on a screen 400 points
    // tall (the window it belongs to is taller).
    assert!(
        response.rect.height() <= 400.0,
        "{} points tall",
        response.rect.height()
    );
    // What is behind it takes no click while it is open.
    let at = bench
        .ctx
        .read_response(behind)
        .expect("drawn")
        .rect
        .center();
    bench.click(at, &mut draw);
    assert!(!clicked_behind.get());
    let answered = bench.frame(key(Key::Escape), &mut draw);
    assert_eq!(answered.out.1.answer, Some(Answer::Cancel));
}

#[test]
fn confirm_asks_with_ok_and_cancel_unless_told_otherwise() {
    let mut bench = Bench::new();
    let ask = |ui: &mut Ui, theme: &Theme| {
        confirm(
            ui.ctx(),
            Dialog::new("confirm", "Delete Layer", theme),
            Tone::Warning,
            "The layer and everything on it goes.",
        )
    };
    bench.frames(2, ask);
    // OK and Cancel, and nothing else.
    let dialog = Dialog::new("confirm", "Delete Layer", &bench.theme);
    let drawn = |answer| bench.ctx.read_response(dialog.button_id(answer)).is_some();
    assert!(drawn(Answer::Accept) && drawn(Answer::Cancel));
    assert!(!drawn(Answer::Alternate) && !drawn(Answer::Extra));
    assert_eq!(bench.frame(key(Key::Enter), ask).out, Some(Answer::Accept));
}

#[test]
fn a_dialog_draws_regular_controls_and_touch_ones_on_touch() {
    let mut bench = Bench::new();
    let response = bench.frames(3, |ui, theme| show(ui, plain(theme)));
    let height = response
        .button_rect(Answer::Accept)
        .expect("drawn")
        .height();
    assert_eq!(height, Density::Regular.control_height());
    bench
        .ctx
        .data_mut(|data| data.insert_temp(egui::Id::NULL.with("sue-touch"), true));
    let response = bench.frames(3, |ui, theme| show(ui, plain(theme)));
    let height = response
        .button_rect(Answer::Accept)
        .expect("drawn")
        .height();
    assert_eq!(height, Density::Touch.control_height());
}

// The sheet.

/// Shows the test sheet.
fn sheet(ui: &mut Ui, theme: &Theme) -> SheetResponse<()> {
    Sheet::new("sheet", "Brush Size", theme).show(ui.ctx(), |ui| {
        for line in 0..4 {
            ui.label(format!("Row {line}"));
        }
    })
}

/// Shows the sheet until it has risen into place.
fn risen(bench: &mut Bench) -> SheetResponse<()> {
    bench.frames(40, sheet)
}

#[test]
fn a_sheet_rises_to_the_bottom_edge() {
    let mut bench = Bench::new();
    let response = risen(&mut bench);
    assert!(!response.dismissed);
    assert!((response.rect.bottom() - bench.screen.bottom()).abs() < 1.0);
    assert!((response.rect.center().x - bench.screen.center().x).abs() < 1.0);
    assert_eq!(response.rect.width(), SHEET_WIDTH);
    // The grip is the top of the sheet, inside its outline.
    assert!((response.grip.top() - response.rect.top()).abs() <= bench.theme.border_width);
    let rest = bench.frame(Vec::new(), sheet);
    assert!(!rest.repaint_now);
    // On a phone it spans the screen.
    let mut bench = Bench::with(vec2(390.0, 800.0), &[]);
    let response = risen(&mut bench);
    assert_eq!(response.rect.width(), 390.0);
}

#[test]
fn a_sheet_dragged_down_far_enough_leaves() {
    let mut bench = Bench::new();
    let response = risen(&mut bench);
    let height = response.rect.height();
    let drawn = bench.drag(response.grip.center(), vec2(0.0, height * 0.6), 12, sheet);
    // It follows the pointer down.
    let during = drawn[drawn.len() - 2].rect;
    assert!(during.top() > response.rect.top() + height * 0.4);
    // And slides out once let go, then says so once.
    let mut dismissed = 0;
    for _ in 0..60 {
        if bench.frame(Vec::new(), sheet).out.dismissed {
            dismissed += 1;
            break;
        }
    }
    assert_eq!(dismissed, 1);
}

#[test]
fn a_sheet_dragged_a_little_springs_back() {
    let mut bench = Bench::new();
    let response = risen(&mut bench);
    let drawn = bench.drag(response.grip.center(), vec2(0.0, 30.0), 6, sheet);
    assert!(drawn.iter().all(|frame| !frame.dismissed));
    let back = bench.frames(120, sheet);
    assert!(!back.dismissed);
    assert!((back.rect.top() - response.rect.top()).abs() < 0.5);
    assert!(!bench.frame(Vec::new(), sheet).repaint_now);
}

#[test]
fn escape_or_a_click_beside_it_sends_a_sheet_away() {
    for how in ["escape", "scrim"] {
        let mut bench = Bench::new();
        risen(&mut bench);
        if how == "escape" {
            bench.frame(key(Key::Escape), sheet);
        } else {
            bench.click(pos2(8.0, 8.0), sheet);
        }
        let gone = (0..60).any(|_| bench.frame(Vec::new(), sheet).out.dismissed);
        assert!(gone, "{how}");
    }
}

#[test]
fn a_sheet_sent_away_with_instant_transitions_goes_at_once() {
    let mut bench = Bench::instant();
    bench.frames(2, sheet);
    let response = bench.frame(key(Key::Escape), sheet);
    let next = bench.frame(Vec::new(), sheet);
    assert!(response.out.dismissed || next.out.dismissed);
}

// Forms.

#[test]
fn a_form_lines_its_controls_up_after_the_widest_label() {
    let mut bench = Bench::new();
    let rows = bench.frames(3, |ui, theme| {
        form_grid(ui, "form", theme, |form| {
            let short = form.row("Size", |ui| ui.button("A").rect);
            let long = form.row("Anti-aliasing amount", |ui| ui.button("B").rect);
            (short, long)
        })
    });
    let (short, long) = rows;
    assert_eq!(short.inner.left(), long.inner.left(), "one control column");
    assert!(long.response.rect.width() >= short.response.rect.width());
    // The control starts after the label and the gap between them.
    let gap = bench.theme.label_gap();
    assert!((long.inner.left() - long.response.rect.right() - gap).abs() < 1.0);
    // Label and control are level.
    assert!((short.inner.center().y - short.response.rect.center().y).abs() < 1.0);
}

#[test]
fn a_narrow_form_stands_each_label_over_its_control() {
    let mut bench = Bench::with(vec2(180.0, 400.0), &[]);
    let (label, control) = bench.frames(3, |ui, theme| {
        form_grid(ui, "narrow", theme, |form| {
            let row = form.row("Opacity of the layer", |ui| ui.button("100%").rect);
            (row.response.rect, row.inner)
        })
    });
    assert!(
        control.top() >= label.bottom(),
        "the control is under its label"
    );
    assert!((control.left() - label.left()).abs() < 1.0);
}

#[test]
fn a_form_section_and_a_wide_row_keep_to_the_columns() {
    let mut bench = Bench::new();
    let (wide, labelled) = bench.frames(3, |ui, theme| {
        form_grid(ui, "sections", theme, |form| {
            form.section("Stroke");
            let labelled = form.row("Width", |ui| ui.button("4").rect);
            let wide = form.wide(|ui| ui.button("Keep the ratio").rect);
            (wide, labelled.inner)
        })
    });
    assert_eq!(wide.left(), labelled.left());
}

/// A pointer event at `at`, pressed or let go.
fn button(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Runs dialogs in windows of their own on `ctx`, as the desktop does:
/// each frame of a dialog's window gets `events(frame)` and is 440 by
/// 300 points at `place` on the screen.
fn windows_of_their_own(
    events: impl Fn(usize) -> Vec<Event> + 'static,
    place: std::rc::Rc<std::cell::Cell<(usize, Pos2)>>,
) {
    Context::set_immediate_viewport_renderer(move |ctx, viewport| {
        let id = viewport.ids.this;
        let (frame, at) = place.get();
        let size = vec2(440.0, 300.0);
        let mut raw = RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events: events(frame),
            time: Some(1.0 + frame as f64 * STEP),
            ..Default::default()
        };
        let info = raw.viewports.entry(id).or_default();
        info.native_pixels_per_point = Some(1.0);
        info.focused = Some(true);
        info.outer_rect = Some(Rect::from_min_size(at, size));
        info.inner_rect = Some(Rect::from_min_size(at, size));
        let mut show = viewport.viewport_ui_cb;
        let mut out = ctx.run_ui(raw, |ui| show(ui));
        out.textures_delta.clear();
    });
}

#[test]
fn a_dialogs_own_window_moves_by_its_title_bar_and_opens_where_it_was_left() {
    let grip = pos2(100.0, crate::widgets::TITLE_BAR / 2.0);
    let place = std::rc::Rc::new(std::cell::Cell::new((0, pos2(500.0, 300.0))));
    windows_of_their_own(
        move |frame| match frame {
            3 => vec![Event::PointerMoved(grip)],
            4 => vec![button(grip, true)],
            5..=8 => vec![Event::PointerMoved(
                grip + vec2(12.0 * (frame - 4) as f32, 0.0),
            )],
            9 => vec![button(grip + vec2(48.0, 0.0), false)],
            _ => Vec::new(),
        },
        std::rc::Rc::clone(&place),
    );
    let mut bench = Bench::new();
    bench.ctx.set_embed_viewports(false);
    let mut asked = Vec::new();
    let mut open = true;
    let run = |bench: &mut Bench, frame: usize, open: bool, asked: &mut Vec<_>| {
        place.set((frame, place.get().1));
        let theme = bench.theme.clone();
        let input = RawInput {
            screen_rect: Some(bench.screen),
            time: Some(bench.time),
            ..Default::default()
        };
        bench.time += STEP;
        let mut viewport = None;
        let mut done = bench.ctx.run_ui(input, |ui| {
            crate::widgets::set_own_frames(ui.ctx(), true);
            if open {
                let dialog = plain(&theme).host(DialogHost::Window).resizable();
                viewport = Some(dialog.viewport());
                show(ui, dialog);
            }
        });
        done.textures_delta.clear();
        if let Some(out) = viewport.and_then(|id| done.viewport_output.get(&id)) {
            asked.push((out.builder.position, out.commands.clone()));
        }
    };
    for frame in 0..12 {
        run(&mut bench, frame, open, &mut asked);
    }
    // Its title bar asks its window to be dragged, and the commands come
    // with the frame of the window that showed it.
    assert!(
        asked
            .iter()
            .any(|(_, commands)| commands.contains(&ViewportCommand::StartDrag)),
        "{asked:#?}"
    );
    // Its place is asked for while it opens only: a window told where to
    // be every frame could not be moved.
    assert!(asked[0].0.is_some());
    assert!(
        asked[3..].iter().all(|(position, _)| position.is_none()),
        "{asked:#?}"
    );
    // The system moved it; it closes, and opens there again.
    place.set((20, pos2(40.0, 60.0)));
    run(&mut bench, 20, open, &mut asked);
    open = false;
    run(&mut bench, 21, open, &mut asked);
    asked.clear();
    run(&mut bench, 22, true, &mut asked);
    assert_eq!(asked[0].0, Some(pos2(40.0, 60.0)), "{asked:#?}");
}

#[test]
fn a_dialog_in_the_window_moves_by_its_header_resizes_and_opens_where_left() {
    let mut bench = Bench::instant();
    fn sizable(theme: &Theme) -> Dialog<'_> {
        plain(theme).resizable()
    }
    let first = bench.frames(4, |ui, theme| show(ui, sizable(theme)));
    // Centered at first.
    assert!(
        (first.rect.center() - bench.screen.center()).length() < 2.0,
        "{:?} in {:?}",
        first.rect,
        bench.screen
    );
    // Its header moves it.
    let header = first.rect.left_top() + vec2(40.0, 20.0);
    bench.drag(header, vec2(-100.0, -60.0), 4, |ui, theme| {
        show(ui, sizable(theme))
    });
    let moved = bench.frames(2, |ui, theme| show(ui, sizable(theme)));
    assert!(
        (moved.rect.min - (first.rect.min + vec2(-100.0, -60.0))).length() < 2.0,
        "{:?} from {:?}",
        moved.rect,
        first.rect
    );
    // Its bottom right corner sizes it.
    let corner = moved.rect.right_bottom() - vec2(4.0, 4.0);
    bench.drag(corner, vec2(120.0, 80.0), 4, |ui, theme| {
        show(ui, sizable(theme))
    });
    let sized = bench.frames(2, |ui, theme| show(ui, sizable(theme)));
    // As wide as dragged; at least as tall (a body has a least height).
    assert!(
        (sized.rect.width() - moved.rect.width() - 120.0).abs() < 2.0
            && sized.rect.height() >= moved.rect.height() + 78.0
            && sized.rect.min == moved.rect.min,
        "{:?} from {:?}",
        sized.rect,
        moved.rect
    );
    // Closed and opened again: where it was left, the size it was made.
    bench.frames(3, |_, _| ());
    let again = bench.frames(4, |ui, theme| show(ui, sizable(theme)));
    assert!(
        (again.rect.min - sized.rect.min).length() < 2.0,
        "{:?}",
        again.rect
    );
    assert!(
        (again.rect.size() - sized.rect.size()).length() < 3.0,
        "{:?}",
        again.rect
    );
}
