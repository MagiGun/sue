//! Scenes of Sue Writer's bars: the find bar over the novel's pages.

use super::super::Tour;

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 14b. The find bar open on the novel, a word found, then closed again
/// with Escape (the first leaves the field, the second the bar).
pub(super) fn find_bar(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    tour.command(&writer, "edit.find");
    tour.camera
        .input
        .push(vec![egui::Event::Text("the".to_owned())]);
    tour.camera.input.push(vec![key(egui::Key::Enter)]);
    tour.shoot("14b-sue-writer-find-bar")?;
    tour.camera.input.push(vec![key(egui::Key::Escape)]);
    tour.camera.input.push(vec![key(egui::Key::Escape)]);
    tour.settle();
    Ok(())
}
