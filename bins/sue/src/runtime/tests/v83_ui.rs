//! Sue 0.0.83's interface fixes, through the real runtime: nothing in
//! Sue Paint's palettes, its dialogs or Settings runs out sideways at
//! 1366 by 768 points (a common laptop at 100 percent).

use super::*;
use egui::{Rect, Shape};

/// The window the checks run in: 1366 by 768 at 100 percent.
const LAPTOP: egui::Vec2 = egui::vec2(1366.0, 768.0);

/// One frame of `paint` in a window of `size` with `events`; what it
/// drew.
fn frame(paint: &mut PaintDriver, size: egui::Vec2, events: Vec<egui::Event>) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
}

/// `count` frames at `size`; what the last drew.
fn settle(paint: &mut PaintDriver, size: egui::Vec2, count: usize) -> egui::FullOutput {
    let mut last = frame(paint, size, Vec::new());
    for _ in 1..count {
        last.textures_delta.clear();
        last = frame(paint, size, Vec::new());
    }
    last
}

/// Every piece of text the frame drew that its clip cuts at the left or
/// the right (a part of it hidden sideways, so a person would have to
/// scroll or widen to read it), with where: the text, how far it runs
/// out and its clip. Text wholly outside its clip is not drawn at all
/// and does not count; nor does the canvas's own content.
fn cut_sideways(output: &egui::FullOutput, screen: Rect) -> Vec<String> {
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let clip = clipped.clip_rect;
        if clip.width() >= screen.width() - 1.0 {
            continue;
        }
        let mut look = |shape: &Shape| {
            if let Shape::Text(text) = shape {
                let rect = Rect::from_min_size(text.pos, text.galley.size());
                let seen = rect.intersect(clip);
                let words = text.galley.text().trim();
                if words.is_empty() || seen.width() <= 0.0 || seen.height() < rect.height() / 2.0 {
                    return;
                }
                let right = rect.right() - clip.right();
                let left = clip.left() - rect.left();
                if right > 1.0 || left > 1.0 {
                    found.push(format!(
                        "{words:?} runs out by {:.0} points of {clip:?}",
                        right.max(left)
                    ));
                }
            }
        };
        match &clipped.shape {
            Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
}

#[test]
fn nothing_in_sue_paint_runs_out_sideways_on_a_laptop() {
    let screen = Rect::from_min_size(Pos2::ZERO, LAPTOP);
    let mut paint = PaintDriver::new("v83-fit");
    let mut found = Vec::new();
    let mut look = |paint: &mut PaintDriver, what: &str, shows: &str| {
        let mut output = settle(paint, LAPTOP, 30);
        output.textures_delta.clear();
        let texts = text_rects(&output);
        assert!(
            texts.iter().any(|(text, _)| text == shows),
            "{what} shows {shows:?}: {texts:?}"
        );
        found.extend(
            cut_sideways(&output, screen)
                .into_iter()
                .map(|line| format!("{what}: {line}")),
        );
    };
    look(&mut paint, "the palettes", "Sub Tool");
    for (what, command, shows) in [
        ("New", "file.new", "Resolution"),
        ("Export", "file.export.png", "Export Settings"),
        (
            "Canvas Properties",
            "edit.canvas_properties",
            "Canvas Properties",
        ),
        ("Timelapse", "file.timelapse.export", "Export Timelapse"),
        ("Preferences", "file.preferences", "Layout Style"),
    ] {
        paint.command(command);
        look(&mut paint, what, shows);
        paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        paint.runtime.shell.close_window();
        settle(&mut paint, LAPTOP, 10).textures_delta.clear();
    }
    assert!(found.is_empty(), "{found:#?}");
}

/// The check sees text its clip cuts sideways, and leaves whole text and
/// text that is not drawn alone.
#[test]
fn the_check_sees_text_cut_sideways() {
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(Pos2::ZERO, LAPTOP);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..egui::RawInput::default()
        },
        |ui| {
            let narrow = Rect::from_min_size(Pos2::new(10.0, 10.0), egui::vec2(60.0, 40.0));
            let mut cut = ui.new_child(egui::UiBuilder::new().max_rect(narrow));
            cut.set_clip_rect(narrow);
            cut.add(egui::Label::new("A label far wider than its place").extend());
            let wide = Rect::from_min_size(Pos2::new(10.0, 100.0), egui::vec2(400.0, 40.0));
            let mut whole = ui.new_child(egui::UiBuilder::new().max_rect(wide));
            whole.set_clip_rect(wide);
            whole.label("Fits");
        },
    );
    output.textures_delta.clear();
    let found = cut_sideways(&output, screen);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("A label far wider"), "{found:?}");
}
