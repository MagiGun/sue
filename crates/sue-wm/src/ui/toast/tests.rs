//! Tests of the toast stack, driven through real egui frames.

use egui::{Event, Pos2, Ui, pos2};

use super::*;
use crate::ui::dialog::tests::{Bench, STEP};

/// The room the notices may take on the bench: the window without a
/// status bar 24 points tall.
fn room(bench: &Bench) -> Rect {
    let mut room = bench.screen;
    room.max.y -= 24.0;
    room
}

/// Shows `toasts` for a frame with `events` and returns what happened.
fn show(bench: &mut Bench, toasts: &mut Toasts, events: Vec<Event>) -> Vec<ToastEvent> {
    let within = room(bench);
    bench
        .frame(events, |ui: &mut Ui, theme| {
            toasts.show(ui.ctx(), within, theme)
        })
        .out
}

/// Shows `toasts` for `count` frames and gathers what happened.
fn run(bench: &mut Bench, toasts: &mut Toasts, count: usize) -> Vec<ToastEvent> {
    (0..count)
        .flat_map(|_| show(bench, toasts, Vec::new()))
        .collect()
}

/// Clicks at `at` and gathers what happened.
fn click(bench: &mut Bench, toasts: &mut Toasts, at: Pos2) -> Vec<ToastEvent> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    [
        vec![Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ]
    .into_iter()
    .flat_map(|events| show(bench, toasts, events))
    .collect()
}

/// The frames that pass in `seconds`.
fn frames_in(seconds: f64) -> usize {
    (seconds / STEP).ceil() as usize
}

#[test]
fn an_empty_stack_draws_nothing_and_asks_for_no_frame() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("empty");
    let within = room(&bench);
    // A fresh context asks for its first frames itself.
    bench.frames(3, |_, _| ());
    let frame = bench.frame(Vec::new(), |ui, theme| {
        let events = toasts.show(ui.ctx(), within, theme);
        let drawn = ui.ctx().memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.order == Layer::Toast.order())
        });
        (events, drawn)
    });
    assert!(frame.out.0.is_empty());
    assert!(!frame.repaint_now);
    assert_eq!(frame.repaint_after, std::time::Duration::MAX);
    assert!(toasts.is_empty());
    assert_eq!(toasts.len(), 0);
}

#[test]
fn the_newest_notice_is_lowest_and_all_sit_above_the_status_bar() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("order");
    let first = toasts.push(Toast::info("Saved Illustration.sue"));
    let second = toasts.push(Toast::success("Exported as PNG").action("Show in Folder"));
    run(&mut bench, &mut toasts, 40);
    let first = toasts.rects(first).expect("drawn").toast;
    let second = toasts.rects(second).expect("drawn").toast;
    let margin = bench.theme.space(Space::S6);
    let within = room(&bench);
    assert!((second.bottom() - (within.bottom() - margin)).abs() < 1.0);
    assert!(first.bottom() < second.top(), "the older one is above");
    let gap = bench.theme.space(Space::S4);
    assert!((second.top() - first.bottom() - gap).abs() < 1.0);
    for rect in [first, second] {
        assert!(
            rect.bottom() <= within.bottom(),
            "the status bar stays clear"
        );
        assert!((rect.center().x - within.center().x).abs() < 1.0);
        assert!(rect.width() <= MAX_WIDTH);
    }
}

#[test]
fn a_notice_leaves_when_its_time_is_up() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("time");
    let id = toasts.push(Toast::warning("The disk is nearly full").seconds(1.0));
    let early = run(&mut bench, &mut toasts, frames_in(0.8));
    assert!(early.is_empty());
    assert_eq!(toasts.len(), 1);
    let late = run(&mut bench, &mut toasts, frames_in(0.4));
    assert_eq!(late, vec![ToastEvent::Expired(id)]);
    // It fades out, then the stack is empty and quiet.
    run(&mut bench, &mut toasts, frames_in(0.5));
    assert!(toasts.is_empty());
    assert_eq!(toasts.get(id), None);
    let within = room(&bench);
    let rest = bench.frame(Vec::new(), |ui, theme| toasts.show(ui.ctx(), within, theme));
    assert!(!rest.repaint_now);
}

#[test]
fn a_notice_at_rest_asks_only_for_the_frame_it_leaves_in() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("rest");
    toasts.push(Toast::info("Copied").seconds(3.0));
    run(&mut bench, &mut toasts, frames_in(0.5));
    let within = room(&bench);
    let frame = bench.frame(Vec::new(), |ui, theme| toasts.show(ui.ctx(), within, theme));
    assert!(!frame.repaint_now, "it is in place");
    let after = frame.repaint_after.as_secs_f64();
    assert!(after > 2.0 && after < 2.6, "asks for {after} seconds");
}

#[test]
fn the_clock_stops_while_the_pointer_is_over_the_stack() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("pause");
    let id = toasts.push(Toast::error("The file could not be read").seconds(1.0));
    run(&mut bench, &mut toasts, 30);
    let over = toasts.rects(id).expect("drawn").toast.center();
    show(&mut bench, &mut toasts, vec![Event::PointerMoved(over)]);
    let held = run(&mut bench, &mut toasts, frames_in(3.0));
    assert!(held.is_empty(), "it stays while pointed at");
    show(&mut bench, &mut toasts, vec![Event::PointerGone]);
    let gone = run(&mut bench, &mut toasts, frames_in(1.0));
    assert_eq!(gone, vec![ToastEvent::Expired(id)]);
}

#[test]
fn a_sticky_notice_stays_until_it_is_closed() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("sticky");
    let id = toasts.push(Toast::info("Sue Paint was updated").sticky());
    assert!(run(&mut bench, &mut toasts, frames_in(20.0)).is_empty());
    let close = toasts.rects(id).expect("drawn").close.center();
    let events = click(&mut bench, &mut toasts, close);
    assert_eq!(events, vec![ToastEvent::Dismissed(id)]);
    run(&mut bench, &mut toasts, 40);
    assert!(toasts.is_empty());
}

#[test]
fn the_action_button_says_so_and_sends_the_notice_away() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("action");
    let id = toasts.push(Toast::success("Exported").action("Show in Folder"));
    run(&mut bench, &mut toasts, 30);
    let rects = toasts.rects(id).expect("drawn");
    let action = rects.action.expect("it has an action");
    assert!(
        action.right() < rects.close.left(),
        "the action stands before the close button"
    );
    assert!(rects.toast.contains_rect(action) && rects.toast.contains_rect(rects.close));
    let events = click(&mut bench, &mut toasts, action.center());
    assert_eq!(events, vec![ToastEvent::Action(id)]);
}

#[test]
fn a_notice_sent_away_by_the_caller_leaves_without_an_event() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("dismiss");
    let id = toasts.push(Toast::info("Rendering"));
    run(&mut bench, &mut toasts, 10);
    toasts.dismiss(id);
    assert!(run(&mut bench, &mut toasts, 40).is_empty());
    assert!(toasts.is_empty());
}

#[test]
fn the_same_notice_twice_starts_its_time_again() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("twins");
    let first = toasts.push(Toast::info("Saved").seconds(1.0));
    run(&mut bench, &mut toasts, frames_in(0.8));
    let again = toasts.push(Toast::info("Saved").seconds(1.0));
    assert_eq!(first, again);
    assert_eq!(toasts.len(), 1);
    assert!(run(&mut bench, &mut toasts, frames_in(0.8)).is_empty());
    // A notice that says something else stacks.
    toasts.push(Toast::info("Saved a copy"));
    assert_eq!(toasts.len(), 2);
}

#[test]
fn more_notices_than_a_stack_holds_send_the_oldest_away() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("many");
    let ids: Vec<ToastId> = (0..MAX_HELD + 2)
        .map(|n| toasts.push(Toast::info(format!("Notice {n}"))))
        .collect();
    let events = show(&mut bench, &mut toasts, Vec::new());
    assert_eq!(
        events,
        vec![ToastEvent::Expired(ids[0]), ToastEvent::Expired(ids[1])]
    );
    run(&mut bench, &mut toasts, 40);
    assert_eq!(toasts.len(), MAX_HELD);
    assert!(ids[2..].iter().all(|id| toasts.get(*id).is_some()));
}

#[test]
fn older_notices_fold_behind_the_newest_and_unfold_on_a_click() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("fold");
    let ids: Vec<ToastId> = (0..5)
        .map(|n| toasts.push(Toast::info(format!("Notice {n}")).sticky()))
        .collect();
    run(&mut bench, &mut toasts, 40);
    // The newest three show; the two older fold behind them.
    assert_eq!(toasts.folded(), 2);
    assert!(ids[..2].iter().all(|id| toasts.rects(*id).is_none()));
    let shown: Vec<Rect> = ids[2..]
        .iter()
        .map(|id| toasts.rects(*id).expect("shown").toast)
        .collect();
    let fold = toasts.fold_rect().expect("a button says how many more");
    assert!(fold.bottom() < shown[0].top(), "it sits over the stack");
    assert!(fold.height() >= Density::Regular.min_hit() * 0.75);
    assert_eq!(tr!("toast-more", count = 2), "2 more");
    // A click unfolds them all, and the same button folds them again.
    click(&mut bench, &mut toasts, fold.center());
    run(&mut bench, &mut toasts, 40);
    assert!(toasts.unfolded());
    assert_eq!(toasts.folded(), 0);
    assert!(ids.iter().all(|id| toasts.rects(*id).is_some()));
    let fold = toasts.fold_rect().expect("Show fewer");
    click(&mut bench, &mut toasts, fold.center());
    run(&mut bench, &mut toasts, 40);
    assert!(!toasts.unfolded());
    assert_eq!(toasts.folded(), 2);
    // Closing a shown one brings the next older forward.
    let close = toasts.rects(ids[4]).expect("shown").close.center();
    assert_eq!(
        click(&mut bench, &mut toasts, close),
        vec![ToastEvent::Dismissed(ids[4])]
    );
    run(&mut bench, &mut toasts, 40);
    assert_eq!(toasts.folded(), 1);
    assert!(toasts.rects(ids[1]).is_some());
    // With no more than fit, there is no button.
    for id in &ids[..2] {
        toasts.dismiss(*id);
    }
    run(&mut bench, &mut toasts, 40);
    assert_eq!(toasts.folded(), 0);
    assert_eq!(toasts.fold_rect(), None);
}

#[test]
fn a_folded_notice_keeps_its_own_time_and_leaves_unseen() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("folded-time");
    let old = toasts.push(Toast::info("Old news").seconds(1.0));
    for n in 0..MAX_SHOWN {
        toasts.push(Toast::info(format!("Fresh {n}")).sticky());
    }
    run(&mut bench, &mut toasts, 10);
    assert!(toasts.rects(old).is_none(), "folded");
    let events = run(&mut bench, &mut toasts, frames_in(1.2));
    assert_eq!(events, vec![ToastEvent::Expired(old)]);
    assert_eq!(toasts.get(old), None, "gone at once: it was not in sight");
    assert_eq!(toasts.folded(), 0);
}

#[test]
fn a_narrow_room_shows_only_the_newest_notice() {
    let mut bench = Bench::with(egui::vec2(393.0, 851.0), &[]);
    let mut toasts = Toasts::new("narrow");
    let ids: Vec<ToastId> = (0..4)
        .map(|n| toasts.push(Toast::info(format!("Notice {n}"))))
        .collect();
    run(&mut bench, &mut toasts, 40);
    assert_eq!(toasts.folded(), 3);
    let newest = toasts.rects(ids[3]).expect("the newest shows").toast;
    let within = room(&bench);
    assert!(within.contains_rect(newest));
    // What it covers is the card, the edges behind it and the button.
    let fold = toasts.fold_rect().expect("3 more");
    let covered = toasts.covered();
    assert!((covered - (within.bottom() - fold.top())).abs() < 1.0);
    assert!(covered < within.height() / 4.0, "{covered}");
}

#[test]
fn notices_wait_under_a_modal_dialog_with_their_clock_stopped() {
    use crate::ui::Dialog;
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("modal");
    let id = toasts.push(Toast::info("Saved").seconds(1.0));
    run(&mut bench, &mut toasts, 10);
    assert_eq!(
        toasts.layer().map(|layer| layer.order),
        Some(Layer::Toast.order())
    );
    let within = room(&bench);
    let with_dialog = |bench: &mut Bench, toasts: &mut Toasts, count: usize| {
        let mut events = Vec::new();
        for _ in 0..count {
            events.extend(
                bench
                    .frame(Vec::new(), |ui, theme| {
                        let events = toasts.show(ui.ctx(), within, theme);
                        Dialog::new("modal", "Settings", theme)
                            .ok()
                            .show(ui.ctx(), |_| ());
                        events
                    })
                    .out,
            );
        }
        events
    };
    // Long past its time, it is still there, under the dialog's layer.
    assert!(with_dialog(&mut bench, &mut toasts, frames_in(3.0)).is_empty());
    let layer = toasts.layer().expect("drawn");
    let modal = bench
        .ctx
        .memory(|memory| memory.top_modal_layer())
        .expect("the dialog is modal");
    assert!(layer.order < modal.order, "the notice is under the dialog");
    // It takes no click while the dialog is up.
    let close = toasts.rects(id).expect("drawn").close.center();
    let button = |pressed| Event::PointerButton {
        pos: close,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(close)],
        vec![button(true)],
        vec![button(false)],
    ] {
        let out = bench.frame(events, |ui, theme| {
            let events = toasts.show(ui.ctx(), within, theme);
            Dialog::new("modal", "Settings", theme)
                .ok()
                .show(ui.ctx(), |_| ());
            events
        });
        assert!(out.out.is_empty());
    }
    show(&mut bench, &mut toasts, vec![Event::PointerGone]);
    // The dialog closes: the notice comes back over everything with all
    // of its time, then leaves.
    run(&mut bench, &mut toasts, 2);
    assert_eq!(
        toasts.layer().map(|layer| layer.order),
        Some(Layer::Toast.order())
    );
    assert!(toasts.get(id).is_some());
    let events = run(&mut bench, &mut toasts, frames_in(1.2));
    assert_eq!(events, vec![ToastEvent::Expired(id)]);
}

#[test]
fn a_long_notice_wraps_inside_its_width() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("long");
    let short = toasts.push(Toast::info("Short"));
    let long = toasts.push(Toast::info(
        "This notice says a great deal more than fits on one line of a notice, \
         so it wraps onto a second and a third line instead of running out",
    ));
    run(&mut bench, &mut toasts, 30);
    let short = toasts.rects(short).expect("drawn").toast;
    let long = toasts.rects(long).expect("drawn").toast;
    assert_eq!(short.width(), long.width());
    assert!(long.height() > short.height());
}

#[test]
fn notices_come_in_at_once_with_instant_transitions() {
    let mut bench = Bench::instant();
    let mut toasts = Toasts::new("instant");
    toasts.push(Toast::info("Saved").seconds(2.0));
    let within = room(&bench);
    // egui sizes a new area in its first frames; after that nothing
    // moves, where a timed notice still comes in.
    for _ in 0..2 {
        bench.frame(Vec::new(), |ui, theme| toasts.show(ui.ctx(), within, theme));
    }
    let frame = bench.frame(Vec::new(), |ui, theme| toasts.show(ui.ctx(), within, theme));
    assert!(!frame.repaint_now, "nothing moves");
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("timed");
    toasts.push(Toast::info("Saved").seconds(2.0));
    let moving = (0..3)
        .map(|_| bench.frame(Vec::new(), |ui, theme| toasts.show(ui.ctx(), within, theme)))
        .last()
        .expect("three frames");
    assert!(moving.repaint_now);
}

#[test]
fn every_tone_has_a_glyph_in_the_text_color_of_the_shipped_theme() {
    let bench = Bench::new();
    for tone in Tone::ALL {
        // Monochrome: the glyph, not a hue, tells them apart.
        assert_eq!(tone.color(&bench.theme), bench.theme.text);
    }
    // Each draws something into a room, and nothing into none.
    for (rect, draws) in [
        (
            Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(20.0, 20.0)),
            true,
        ),
        (Rect::NOTHING, false),
    ] {
        for tone in Tone::ALL {
            let theme = bench.theme.clone();
            let input = egui::RawInput::default();
            let mut output = bench.ctx.run_ui(input, |ui| {
                tone.paint(ui.painter(), rect, &theme);
            });
            output.textures_delta.clear();
            assert_eq!(!output.shapes.is_empty(), draws, "{tone:?}");
        }
    }
}

#[test]
fn a_notice_reads_as_an_alert_to_a_screen_reader() {
    let mut bench = Bench::new();
    let mut toasts = Toasts::new("alert");
    let id = toasts.push(Toast::info("Saved"));
    run(&mut bench, &mut toasts, 5);
    let close = toasts.rects(id).expect("drawn").close;
    // The close button says what it does.
    let label = tr!("toast-dismiss");
    assert_eq!(label, "Dismiss");
    assert!(close.width() >= crate::ui::Density::Regular.min_hit());
}
