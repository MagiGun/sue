//! Scenes of Sue Paint's selection, fill and gradient tools (P4-1b).

use egui::{Pos2, vec2};

use super::super::Tour;

/// Where the sample's blue circle sits in the 1440 by 900 window.
const BLUE_CIRCLE: Pos2 = Pos2::new(811.0, 405.0);

/// 08p. The Selection area tool on Shrink selection, its seventh sub
/// tool, as the Sub Tool palette shows it. 08q: Shrink selection drawn
/// roughly round the sample's blue circle, its path showing while the
/// button is down. 08r: let go, the selection hugs the circle.
pub(super) fn shrink_selection(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("tool.selection");
    for _ in 0..6 {
        tour.paint_command("tool.next_sub");
    }
    tour.shoot("08p-sue-paint-shrink-selection")?;

    // A rough square round the circle, traced most of the way.
    let corners = [
        BLUE_CIRCLE + vec2(-105.0, -100.0),
        BLUE_CIRCLE + vec2(110.0, -95.0),
        BLUE_CIRCLE + vec2(100.0, 72.0),
        BLUE_CIRCLE + vec2(-95.0, 70.0),
        BLUE_CIRCLE + vec2(-105.0, -96.0),
    ];
    let mut path = Vec::new();
    for pair in corners.windows(2) {
        for step in 0..12 {
            path.push(pair[0] + (pair[1] - pair[0]) * (step as f32 / 12.0));
        }
    }
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::PointerMoved(path[0])]);
    input.push(vec![button(path[0], true)]);
    for at in &path[1..path.len() - 8] {
        input.push(vec![egui::Event::PointerMoved(*at)]);
    }
    tour.shoot("08q-sue-paint-shrink-selection-drawn")?;
    let input = &mut tour.camera.input;
    for at in &path[path.len() - 8..] {
        input.push(vec![egui::Event::PointerMoved(*at)]);
    }
    let last = path[path.len() - 1];
    input.push(vec![button(last, false)]);
    input.push(vec![egui::Event::PointerMoved(last + vec2(160.0, 0.0))]);
    tour.shoot("08r-sue-paint-shrink-selection-made")?;

    // Back to no selection, the first sub tool and the pen, as the
    // scenes after expect.
    tour.paint_command("select.none");
    tour.paint_command("tool.next_sub");
    tour.paint_command("tool.pen");
    Ok(())
}
