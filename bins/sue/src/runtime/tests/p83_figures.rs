//! Sue Paint 0.0.83: the 3D drawing figures. A body's anatomical detail
//! is off until Tool Property's tick turns it on, as one undo step that
//! draws the figure anew and is kept in the file; and posing draws each
//! frame quickly enough to follow the pointer.

use std::time::{Duration, Instant};

use super::p81_figures::{on_screen, sight};
use super::paint_3d::{click_control, model, new_3d, one_step, pixels};
use super::*;

use sue_paint::figure::Body;

/// Saves the work in focus as a Sue project at `path`.
fn save_as(paint: &mut PaintDriver, path: &std::path::Path) {
    paint.step(vec![key(egui::Key::S, egui::Modifiers::COMMAND)]);
    paint.settle(6);
    let asked = paint
        .runtime
        .pending_pick()
        .expect("Save asks where")
        .clone();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: asked,
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        paint.now,
    );
    let start = Instant::now();
    while !path.exists() || paint.runtime.transfers.poll() {
        assert!(start.elapsed() < Duration::from_secs(300), "the save ends");
        paint.step(Vec::new());
    }
    paint.settle(6);
}

#[test]
fn p83_anatomical_detail_is_off_until_ticked_one_step_and_kept_in_the_file() {
    let mut paint = PaintDriver::new("p83-figure-anatomy");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let entity = model(&paint, layer);
    assert!(
        entity.property(sue_paint::figure::ANATOMY).is_none(),
        "a new figure says nothing of it"
    );
    assert!(!Body::of(entity).anatomy, "and is a smooth mannequin");
    // Ticked: the body shows it, drawn again, as one step that undoes
    // and redoes exactly.
    one_step(
        &mut paint,
        layer,
        "anatomical detail ticked",
        |paint| click_control(paint, "Anatomical detail"),
        |paint| assert!(Body::of(model(paint, layer)).anatomy, "it is on"),
    );
    let on = pixels(&paint, layer);
    // Kept in the file and read back on.
    std::fs::create_dir_all(&paint.root).unwrap();
    let path = paint.root.join("p83-figure-anatomy.sue");
    save_as(&mut paint, &path);
    let bytes = std::fs::read(&path).unwrap();
    let (mut runtime, root) = runtime("p83-figure-anatomy-again");
    runtime
        .open_project(&path, &bytes)
        .expect("the project opens");
    let again = PaintDriver::with(runtime, root);
    assert!(
        Body::of(model(&again, layer)).anatomy,
        "the figure came back with its anatomical detail on"
    );
    assert_eq!(pixels(&again, layer), on, "and drawn as it was");
    // Unticked again: off, one more step.
    one_step(
        &mut paint,
        layer,
        "anatomical detail unticked",
        |paint| click_control(paint, "Anatomical detail"),
        |paint| assert!(!Body::of(model(paint, layer)).anatomy, "it is off"),
    );
}

#[test]
fn p83_posing_a_figure_draws_every_frame_quickly() {
    let mut paint = PaintDriver::new("p83-figure-speed");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let (pose, body, _) = sight(&paint, layer);
    let joints: Vec<[f32; 3]> = sue_paint::figure::solve(&pose, &body)
        .into_iter()
        .map(|(at, _)| at)
        .collect();
    // The left forearm, dragged in small moves, each frame timed from
    // the pointer's move to the layer's pixels shown.
    let forearm = [0, 1, 2].map(|axis| joints[6][axis] * 0.5 + joints[7][axis] * 0.5);
    let from = on_screen(&paint, layer, forearm);
    let drawn = pixels(&paint, layer);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(from)]);
    paint.step(vec![press(from, true)]);
    let mut frames = Vec::new();
    for index in 1..=24 {
        let at = from + vec2(index as f32 * 3.0, index as f32 * -2.0);
        let started = Instant::now();
        paint.step(vec![egui::Event::PointerMoved(at)]);
        frames.push(started.elapsed());
    }
    let to = from + vec2(72.0, -48.0);
    let started = Instant::now();
    paint.step(vec![press(to, false)]);
    let release = started.elapsed();
    paint.settle(6);
    assert_ne!(pixels(&paint, layer), drawn, "the figure was posed");
    frames.sort();
    let middle = frames[frames.len() / 2];
    let slowest = frames[frames.len() - 1];
    eprintln!("posing frames: middle {middle:?}, slowest {slowest:?}, let go {release:?}");
    // Generous, so only a real slowdown fails (a debug build on a busy
    // machine is many times slower than a release build).
    let bound = if cfg!(debug_assertions) {
        Duration::from_millis(2500)
    } else {
        Duration::from_millis(120)
    };
    assert!(middle < bound, "a posing frame takes {middle:?}");
}
