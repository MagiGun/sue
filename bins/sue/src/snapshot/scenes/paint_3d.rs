//! Scenes of Sue Paint's 3D Tool Property: a drawing figure with the
//! object and camera rows, then scrolled to its body shape and chart.

use egui::{Pos2, pos2, vec2};

use super::super::Tour;

/// Where the last frame drew `text`, if it did.
fn drawn_at(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    let mut found = None;
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(drawn) = shape
                && drawn.galley.text() == text
            {
                found = Some(egui::Rect::from_min_size(drawn.pos, drawn.galley.size()));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
}

/// Turns the wheel `times` times by `by` points over `at`.
fn wheel(tour: &mut Tour<'_>, at: Pos2, by: f32, times: usize) {
    for _ in 0..times {
        tour.camera.input.push(vec![
            egui::Event::PointerMoved(at),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, by),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
    }
    tour.settle();
}

/// 08p and 08q. A drawing figure with the Object tool: Tool Property's
/// object and camera rows, then its body shape, type and chart.
pub(super) fn tool_property(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new.figure");
    tour.paint_go(&["tab/tool/1"]);
    tour.shoot("08p-sue-paint-3d-object")?;
    tour.settle();
    let output = tour.camera.frame(&mut tour.runtime);
    let tab = drawn_at(&output, "Tool Property");
    tour.camera.painter.apply_textures(output.textures_delta);
    if let Some(tab) = tab {
        let over = pos2(tab.center().x, tab.bottom() + 160.0);
        wheel(tour, over, -60.0, 22);
        tour.shoot("08q-sue-paint-3d-body")?;
        // Back to the top, so the scenes after find Tool Property as it was.
        wheel(tour, over, 400.0, 10);
    }
    tour.paint_command("edit.undo");
    tour.settle();
    Ok(())
}
