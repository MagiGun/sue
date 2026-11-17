//! A long stroke of a heavy brush, through the whole runtime: its lift
//! does not wait for the stroke to be merged (the worker does that, and
//! the stroke lands a few frames later as one undo step), and whatever
//! is pressed meanwhile finds the stroke landed first.
//!
//! The benchmark at the end is ignored: it times the frames of the
//! interface while such a stroke is drawn on a big canvas.

use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use std::time::{Duration, Instant};

/// The top layer's bytes.
fn top(driver: &PaintDriver) -> Vec<u8> {
    let arena = &driver.runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .expect("a picture");
    let layer = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .last()
        .expect("a layer");
    sue_paint::doc::raster(arena, layer.id)
        .expect("pixels")
        .bytes()
        .expect("bytes")
        .to_vec()
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A scribble of `count` moves about `middle`, `reach` points each way.
fn scribble(middle: Pos2, reach: egui::Vec2, count: usize) -> Vec<Pos2> {
    (0..=count)
        .map(|i| {
            let t = i as f32 / 12.0;
            middle + vec2(reach.x * (t * 1.9).sin(), reach.y * (t * 1.3 + 0.5).sin())
        })
        .collect()
}

/// Presses at the first of `points` and moves through the rest, a
/// frame each; each frame's time. No release.
fn press_and_draw(driver: &mut PaintDriver, points: &[Pos2]) -> Vec<Duration> {
    driver.step(vec![Event::PointerMoved(points[0])]);
    driver.step(vec![button(points[0], true)]);
    points[1..]
        .iter()
        .map(|&at| {
            let start = Instant::now();
            driver.step(vec![Event::PointerMoved(at)]);
            start.elapsed()
        })
        .collect()
}

/// Frames until the top layer is no longer `was`, at most a minute.
fn until_landed(driver: &mut PaintDriver, was: &[u8]) -> Vec<u8> {
    let began = Instant::now();
    loop {
        driver.step(Vec::new());
        let now = top(driver);
        if now != was {
            return now;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "the stroke never landed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A big mixing brush and a view that shows the whole picture.
fn heavy_brush(driver: &mut PaintDriver) {
    driver.command("view.fit");
    driver.command("subtool.brush.Oil_paint");
    for _ in 0..6 {
        driver.command("brush.bigger");
    }
    driver.settle(3);
}

#[test]
fn a_long_heavy_stroke_lands_after_its_lift_as_one_undo_step() {
    let mut driver = PaintDriver::new("paint-brush-long");
    heavy_brush(&mut driver);
    let clear = top(&driver);
    let middle = paint_middle(&driver.runtime);
    let points = scribble(middle, vec2(300.0, 220.0), 160);
    press_and_draw(&mut driver, &points);
    let last = *points.last().unwrap();
    driver.step(vec![button(last, false)]);
    // The lift's frame does not merge a stroke this big: the layer is
    // as it was, and the stroke still shows from its worker.
    assert!(
        top(&driver) == clear,
        "the lift waited for the stroke to be merged"
    );
    let landed = until_landed(&mut driver, &clear);
    // One undo step, there and back.
    driver.command("edit.undo");
    driver.settle(2);
    assert!(top(&driver) == clear, "the stroke is one undo step");
    driver.command("edit.redo");
    driver.settle(2);
    assert!(top(&driver) == landed, "and comes back whole");
}

#[test]
fn a_press_while_a_stroke_settles_finds_it_landed() {
    let mut driver = PaintDriver::new("paint-brush-long-press");
    heavy_brush(&mut driver);
    let clear = top(&driver);
    let middle = paint_middle(&driver.runtime);
    let points = scribble(middle, vec2(300.0, 220.0), 160);
    press_and_draw(&mut driver, &points);
    driver.step(vec![button(*points.last().unwrap(), false)]);
    assert!(top(&driver) == clear, "the first stroke is still settling");
    // The very next frame the pen is down again: nothing of either
    // stroke is lost, and they are two undo steps in the order drawn.
    let second = [middle - vec2(40.0, 0.0), middle + vec2(40.0, 0.0)];
    driver.step(vec![button(second[0], true)]);
    let first_alone = top(&driver);
    assert!(first_alone != clear, "the press landed the first stroke");
    for index in 1..=8 {
        let at = second[0] + (second[1] - second[0]) * (index as f32 / 8.0);
        driver.step(vec![Event::PointerMoved(at)]);
    }
    driver.step(vec![button(second[1], false)]);
    let both = until_landed(&mut driver, &first_alone);
    driver.command("edit.undo");
    driver.settle(2);
    assert!(
        top(&driver) == first_alone,
        "the second stroke undone alone"
    );
    driver.command("edit.undo");
    driver.settle(2);
    assert!(top(&driver) == clear, "then the first");
    driver.command("edit.redo");
    driver.command("edit.redo");
    driver.settle(2);
    assert!(top(&driver) == both, "and both come back");
}

/// The frames of the interface while a long stroke of a heavy brush is
/// drawn, and its lift: `cargo test --release -p sue --lib --
/// --ignored --nocapture heavy_stroke_frames`.
#[test]
#[ignore = "a benchmark: prints the interface's frame times"]
fn heavy_stroke_frames_benchmark() {
    let mut driver = PaintDriver::new("paint-brush-long-bench");
    driver.command("view.fit");
    for name in ["Oil_paint", "Round_watercolor_brush", "Gouache"] {
        driver.command(&format!("subtool.brush.{name}"));
        for _ in 0..6 {
            driver.command("brush.bigger");
        }
        driver.settle(3);
        let clear = top(&driver);
        let middle = paint_middle(&driver.runtime);
        let points = scribble(middle, vec2(300.0, 220.0), 1200);
        let mut frames = press_and_draw(&mut driver, &points);
        let start = Instant::now();
        driver.step(vec![button(*points.last().unwrap(), false)]);
        let lift = start.elapsed();
        let start = Instant::now();
        until_landed(&mut driver, &clear);
        let landed = start.elapsed();
        frames.sort();
        println!(
            "{name}: {} frames, median {:?}, 99th {:?}, worst {:?}; lift {lift:?}, landed after {landed:?}",
            frames.len(),
            frames[frames.len() / 2],
            frames[frames.len() * 99 / 100],
            frames[frames.len() - 1],
        );
        driver.command("edit.undo");
        driver.settle(2);
    }
}
