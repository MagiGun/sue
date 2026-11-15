//! Scenes of Sue Writer's bars: the find bar over the novel's pages, and
//! the font list open under the toolbar. Clicks land where a frame drew
//! what they click.

use egui::Rect;

use super::super::Tour;

/// Where the next frame draws `text` above `below` (in points), the
/// leftmost first.
fn text_above(tour: &mut Tour<'_>, text: &str, below: f32) -> Option<Rect> {
    let output = tour.camera.frame(&mut tour.runtime);
    tour.camera
        .painter
        .apply_textures(output.textures_delta.clone());
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(shown) = shape
                && shown.galley.text() == text
            {
                found.push(Rect::from_min_size(shown.pos, shown.galley.size()));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
        .into_iter()
        .filter(|rect| rect.bottom() < below)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 14b. The find bar open on the novel with a word typed, then closed again
/// with Escape (the first leaves the field, the second the bar).
pub(super) fn find_bar(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    tour.command(&writer, "edit.find");
    tour.settle();
    tour.camera
        .input
        .push(vec![egui::Event::Text("storm".to_owned())]);
    tour.shoot("14b-sue-writer-find-bar")?;
    tour.camera.input.push(vec![key(egui::Key::Escape)]);
    tour.camera.input.push(vec![key(egui::Key::Escape)]);
    tour.settle();
    Ok(())
}

/// 14c. The toolbar's font box open on the novel: a search field over the
/// installed fonts, each drawn in its own face; Escape closes it.
pub(super) fn font_list(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.settle();
    let font =
        text_above(tour, "Georgia", 80.0).ok_or("the toolbar shows no font box reading Georgia")?;
    tour.camera.click(font.center());
    tour.settle();
    tour.shoot("14c-sue-writer-font-list")?;
    tour.camera.input.push(vec![key(egui::Key::Escape)]);
    tour.settle();
    Ok(())
}
