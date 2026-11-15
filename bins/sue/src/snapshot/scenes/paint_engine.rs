//! A scene of Sue Paint painting with its brush engine (P4-1a): a pen
//! line drawn through the canvas the way a hand draws it.

use egui::{Pos2, vec2};

use super::super::Tour;

/// Where the line starts, over the sample's empty top in the 1440 by
/// 900 window.
const START: Pos2 = Pos2::new(470.0, 300.0);

/// 08s. The G-pen's line on a new raster layer, drawn by the pointer
/// through the canvas, so the brush engine paints it. Last in the tour:
/// its steps stay in Sue Paint's history.
pub(super) fn pen_stroke(tour: &mut Tour<'_>) -> Result<(), String> {
    // In the desktop window, whatever size the scene before left, with
    // Sue Paint's palettes at rest.
    let size = tour.camera.size;
    tour.camera.size = tour.desktop;
    super::paint_at_rest(tour);
    tour.paint_command("layer.new");
    // The G-pen by its own command: Pen's key, pressed on the Pen, goes
    // on to the next tool of its group.
    tour.paint_command("subtool.pen.G-pen");
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let path: Vec<Pos2> = (0..=90)
        .map(|step| {
            let s = step as f32 / 90.0;
            START + vec2(520.0 * s, 40.0 * (s * std::f32::consts::TAU * 1.5).sin())
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
    input.push(vec![egui::Event::PointerMoved(last + vec2(0.0, 220.0))]);
    tour.shoot("08s-sue-paint-pen-stroke")?;
    tour.camera.size = size;
    Ok(())
}
