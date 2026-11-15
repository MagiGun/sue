//! Scenes of the Gradient tool, gradient layers, and the Tool Property
//! rows of Fill and the Selection tool (Phase 4 finish).

use egui::{Pos2, vec2};

use super::super::Tour;

/// A drag from `from` to `to` with the primary button, in twelve steps.
fn drag(tour: &mut Tour<'_>, from: Pos2, to: Pos2) {
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::PointerMoved(from)]);
    input.push(vec![button(from, true)]);
    for step in 1..=12 {
        let at = from + (to - from) * (step as f32 / 12.0);
        input.push(vec![egui::Event::PointerMoved(at)]);
    }
    input.push(vec![button(to, false)]);
    // The pointer rests away from the canvas, so no hover shows.
    input.push(vec![egui::Event::PointerMoved(Pos2::new(700.0, 860.0))]);
}

/// Scrolls Tool Property by `delta` points (below 0: further down).
fn scroll(tour: &mut Tour<'_>, delta: f32) {
    let over = Pos2::new(200.0, 420.0);
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::PointerMoved(over)]);
    input.push(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, delta),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    }]);
    input.push(vec![egui::Event::PointerMoved(Pos2::new(700.0, 860.0))]);
}

/// 08t. The Gradient tool on Spectrum, laid over the sample, with Tool
/// Property's bar of stops, set row, shape and edge rows. 08u. Layer,
/// New Layer, Gradient: a live gradient layer, its line and ends shown
/// on the canvas and its own gradient in Tool Property. 08v. Fill's
/// Refer Other Layers: Refer multiple and the Exclude row of five
/// toggles. 08w. The Selection tool's Aspect, Adjust angle after fixed
/// and Anti-aliasing rows. Each scene's change is undone after it.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["tab/tool/1"]);
    tour.paint_command("tool.pick.gradient");
    // Spectrum, the tenth sub tool.
    for _ in 0..9 {
        tour.paint_command("tool.next_sub");
    }
    let middle = Pos2::new(811.0, 405.0);
    drag(tour, middle - vec2(220.0, 0.0), middle + vec2(220.0, 0.0));
    tour.shoot("08t-sue-paint-gradient")?;
    tour.paint_command("edit.undo");
    // Back to the first sub tool.
    for _ in 0..3 {
        tour.paint_command("tool.next_sub");
    }

    tour.paint_command("layer.new.gradient");
    tour.shoot("08u-sue-paint-gradient-layer")?;
    tour.paint_command("edit.undo");

    tour.paint_command("tool.pick.fill");
    // Down to Refer multiple and its Exclude row.
    scroll(tour, -2000.0);
    tour.shoot("08v-sue-paint-fill-refer")?;

    tour.paint_command("tool.selection");
    // Down to Aspect, Adjust angle after fixed and Anti-aliasing.
    scroll(tour, -2000.0);
    tour.shoot("08w-sue-paint-selection-property")?;
    scroll(tour, 4000.0);
    tour.paint_command("tool.pen");
    Ok(())
}
