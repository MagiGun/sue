//! Sue Paint's comic work (package P4-15): the Page Manager docked beside
//! a page, its right-click menu, webtoon columns, the 3D Preview for
//! Binding, and frame borders and balloons on a page.

use egui::{Pos2, vec2};

use super::Tour;

/// Every piece of text the next frame draws, with its rect.
fn texts(tour: &mut Tour<'_>) -> Vec<(String, egui::Rect)> {
    let output = tour.camera.frame(&mut tour.runtime);
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                found.push((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                ));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    tour.camera.painter.apply_textures(output.textures_delta);
    found
}

/// The middle of the Page Manager's cell of page `number`, from the
/// number under it.
fn page_cell(tour: &mut Tour<'_>, number: usize) -> Result<Pos2, String> {
    let open = format!("{number} (open)");
    texts(tour)
        .into_iter()
        .filter(|(text, _)| *text == number.to_string() || *text == open)
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .map(|rect| rect.center() - vec2(0.0, 40.0))
        .ok_or_else(|| format!("page {number} is not in the Page Manager"))
}

/// Presses `key` once.
fn key(tour: &mut Tour<'_>, key: egui::Key) {
    tour.camera.input.push(vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
}

/// Presses `button` at `at` once, or twice for a double-click.
fn press(tour: &mut Tour<'_>, at: Pos2, button: egui::PointerButton, times: usize) {
    let event = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    tour.camera.input.push(vec![egui::Event::PointerMoved(at)]);
    for _ in 0..times {
        tour.camera.input.push(vec![event(true)]);
        tour.camera.input.push(vec![event(false)]);
    }
}

/// 11c1 to 11c4. Where the Page Manager scene (11c) left the story of
/// five pages: a page opened from the Page Manager, which docks beside
/// it; its right-click menu; webtoon columns; the 3D preview.
pub(super) fn pages(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("story.manager");
    tour.settle();
    let second = page_cell(tour, 2)?;
    press(tour, second, egui::PointerButton::Primary, 2);
    tour.shoot("11c1-sue-paint-pages-docked")?;
    let third = page_cell(tour, 3)?;
    press(tour, third, egui::PointerButton::Secondary, 1);
    tour.shoot("11c2-sue-paint-pages-menu")?;
    key(tour, egui::Key::Escape);
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("story.webtoon.right_to_left");
    tour.paint_command("story.manager");
    tour.shoot("11c3-sue-paint-pages-webtoon")?;
    tour.paint_go(&["dialog/close"]);
    // The direction was the scene's own: put it back.
    tour.paint_command("edit.undo");
    tour.paint_command("story.preview_binding");
    tour.shoot("11c4-sue-paint-binding-preview")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("story.first");
    tour.settle();
    Ok(())
}

/// 11c5 to 11c7. Frame borders on the first page: a panel picked with
/// the Object tool, Divide Frame Border Equally, the panel divided (both
/// halves picked). Both steps are undone after.
pub(super) fn frames(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new.frame");
    tour.settle();
    key(tour, egui::Key::Enter);
    tour.settle();
    key(tour, egui::Key::O);
    let middle = tour.runtime.shell.page_rect().center() + vec2(8.0, -63.0);
    press(tour, middle, egui::PointerButton::Primary, 1);
    tour.shoot("11c5-sue-paint-frame-picked")?;
    tour.paint_command("layer.frame.divide_equally");
    tour.shoot("11c6-sue-paint-divide-frame")?;
    key(tour, egui::Key::Enter);
    tour.shoot("11c7-sue-paint-frame-divided")?;
    tour.paint_command("edit.undo");
    tour.paint_command("edit.undo");
    tour.settle();
    Ok(())
}
