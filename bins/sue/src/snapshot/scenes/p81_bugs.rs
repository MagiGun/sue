//! Scenes for the bugs people reported against 0.0.8: a new comic of
//! every kind made, drawn on and looked at through the real painter,
//! the Sub Tool palette of every tool group, and New for each kind of
//! work. `SUE_SNAPSHOT_ONLY=bugs` runs them alone; `bugs-comic`,
//! `bugs-subtool` and `bugs-new` one part of them.

use egui::{Pos2, pos2, vec2};

use super::super::Tour;
use super::paint_dialogs::{click, click_named};

/// A wavy line across the canvas's middle.
fn stroke(tour: &mut Tour<'_>) {
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let start = pos2(520.0, 420.0);
    let path: Vec<Pos2> = (0..=60)
        .map(|step| {
            let s = step as f32 / 60.0;
            start + vec2(360.0 * s, 60.0 * (s * std::f32::consts::TAU).sin())
        })
        .collect();
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::PointerMoved(path[0])]);
    input.push(vec![button(path[0], true)]);
    for at in &path[1..] {
        input.push(vec![egui::Event::PointerMoved(*at)]);
    }
    let last = path[path.len() - 1];
    input.push(vec![button(last, false)]);
    input.push(vec![egui::Event::PointerMoved(last + vec2(0.0, 200.0))]);
}

/// A new work of each kind through New, a stroke on it, and for a comic
/// the Page Manager and one more page.
pub(super) fn comics(tour: &mut Tour<'_>) -> Result<(), String> {
    for (kind, name) in [
        ("Comic", "bugs-comic"),
        ("Show all comic settings", "bugs-comic-all"),
        ("Webtoon", "bugs-webtoon"),
        ("Animation", "bugs-animation"),
    ] {
        tour.paint_go(&["dialog/close", "dialog/new"]);
        tour.settle();
        click_named(tour, kind)?;
        click(tour, "OK")?;
        tour.paint_command("subtool.pen.G-pen");
        stroke(tour);
        tour.shoot(&format!("{name}-1-drawn"))?;
        if kind.contains("omic") {
            tour.paint_command("story.add");
            tour.settle();
            tour.paint_go(&["dialog/pages"]);
            tour.shoot(&format!("{name}-2-pages"))?;
            tour.paint_go(&["dialog/close"]);
        }
    }
    Ok(())
}

/// The Sub Tool palette of every tool that paints, with its stroke
/// samples, and the current brush's sample in Tool Property.
pub(super) fn sub_tools(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["dialog/close"]);
    for (command, name) in [
        ("subtool.pen.G-pen", "pen"),
        ("subtool.pencil.Crayon", "pencil"),
        (
            "subtool.brush.Round_watercolor_brush",
            "brush-round-watercolor",
        ),
        (
            "subtool.brush.Flat_watercolor_brush",
            "brush-flat-watercolor",
        ),
        ("subtool.brush.Wet_wash", "brush-wet-wash"),
        ("subtool.airbrush.Soft", "airbrush"),
        ("subtool.decoration.Stars", "decoration"),
        ("subtool.eraser.Hard", "eraser"),
    ] {
        tour.paint_command(command);
        tour.shoot(&format!("bugs-subtool-{name}"))?;
    }
    Ok(())
}

/// The scenes `only` asks for: all of them for `bugs`.
pub(super) fn scenes(tour: &mut Tour<'_>, only: &str) -> Result<(), String> {
    let all = only == "bugs";
    if all || only == "bugs-comic" {
        comics(tour)?;
    }
    if all || only == "bugs-subtool" {
        sub_tools(tour)?;
    }
    Ok(())
}
