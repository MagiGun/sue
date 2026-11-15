//! Scenes of the Figure tool on the canvas (P4-1c): a polygon and a
//! Bezier curve drawn, and a Curve bending to the pointer before the
//! click that sets it.

use egui::{Event, Modifiers, PointerButton, Pos2, vec2};

use super::super::Tour;

/// Steps from Straight Line to each Figure sub tool, in the Sub Tool
/// palette's order.
const POLYGON: usize = 9;
const BEZIER: usize = 8;
const CURVE: usize = 6;

/// The Figure tool with the sub tool `steps` on from Straight Line.
fn figure(tour: &mut Tour<'_>, steps: usize) {
    // From the Eraser (alone on its key), so the Figure tool comes up on
    // the sub tool it last had: Straight Line, its first, on the tour.
    tour.paint_command("tool.eraser");
    tour.paint_command("tool.figure");
    for _ in 0..steps {
        tour.paint_command("tool.next_sub");
    }
}

fn button(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A drag from `from` to `to` over the coming frames.
fn drag(tour: &mut Tour<'_>, from: Pos2, to: Pos2) {
    let input = &mut tour.camera.input;
    input.push(vec![Event::PointerMoved(from)]);
    input.push(vec![button(from, true)]);
    for index in 1..=8 {
        input.push(vec![Event::PointerMoved(
            from + (to - from) * (index as f32 / 8.0),
        )]);
    }
    input.push(vec![button(to, false)]);
}

/// 08q. A polygon and a Bezier curve on a new layer, and a Curve whose
/// ends are laid, bending toward the pointer.
pub(super) fn tool(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new");
    let middle = tour.runtime.shell.page_rect().center();
    figure(tour, POLYGON);
    drag(
        tour,
        middle + vec2(-260.0, -160.0),
        middle + vec2(-120.0, -20.0),
    );
    tour.settle();
    for _ in BEZIER..POLYGON {
        tour.paint_command("tool.previous_sub");
    }
    let base = middle + vec2(-60.0, -120.0);
    drag(tour, base, base + vec2(40.0, -70.0));
    drag(tour, base + vec2(180.0, 0.0), base + vec2(220.0, 70.0));
    // A click on the last anchor again ends the curve.
    tour.camera.click(base + vec2(180.0, 0.0));
    tour.settle();
    for _ in CURVE..BEZIER {
        tour.paint_command("tool.previous_sub");
    }
    drag(
        tour,
        middle + vec2(-200.0, 120.0),
        middle + vec2(200.0, 120.0),
    );
    tour.camera
        .input
        .push(vec![Event::PointerMoved(middle + vec2(40.0, 30.0))]);
    tour.shoot("08q-sue-paint-figure-tool")?;
    // Esc drops the curve; the drawing and the layer go back.
    tour.camera.input.push(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    tour.settle();
    for _ in 0..3 {
        tour.paint_command("edit.undo");
    }
    // Straight Line again, where the Figure tool was.
    for _ in 0..CURVE {
        tour.paint_command("tool.previous_sub");
    }
    tour.paint_command("tool.pen");
    Ok(())
}
