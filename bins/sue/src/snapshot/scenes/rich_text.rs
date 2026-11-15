//! Scenes of the rich text editor's chrome: what it marks on the page
//! (a misspelled word's wave, tracked changes, the selection), in the
//! theme of the run.

use super::super::Tour;

/// What a scene must leave as it found it: every text of the arena with
/// its marks, and which entities record their changes.
type Kept = Vec<(
    sue_core::EntityId,
    Option<String>,
    Option<sue_model::TextMarks>,
    bool,
)>;

/// What [`Kept`] holds now.
fn kept(tour: &Tour<'_>) -> Kept {
    let mut found: Kept = tour
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| {
            (
                entity.id(),
                entity
                    .string_buffer()
                    .map(|buffer| buffer.as_str().to_owned()),
                entity.ext::<sue_model::TextMarks>().cloned(),
                entity.property(sue_model::changes::TRACK).is_some(),
            )
        })
        .filter(|(_, text, _, track)| text.is_some() || *track)
        .collect();
    found.sort_by_key(|(id, _, _, _)| *id);
    found
}

/// A key pressed with `modifiers`, for the camera's input.
fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// 19a1. The sample novel with changes recorded: words typed in (one of
/// them misspelled, under its wave), the next word struck out, and the
/// typed words partly selected. Undone again afterwards, so the later
/// scenes find the novel as it was.
pub(super) fn marks(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    let before = kept(tour);
    tour.command(&writer, "review.track");
    tour.settle();
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::Text("A quiet mornnig, ".to_owned())]);
    for _ in 0..4 {
        input.push(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
    }
    for _ in 0..4 {
        input.push(vec![key(egui::Key::ArrowLeft, egui::Modifiers::NONE)]);
    }
    for _ in 0..8 {
        input.push(vec![key(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)]);
    }
    tour.shoot("19a1-rich-text-marks")?;
    // Undo step by step until the novel is as it was (the typing, the
    // deletions and turning recording on are steps of their own).
    for _ in 0..12 {
        if kept(tour) == before {
            return Ok(());
        }
        tour.runtime.typing.flush(&mut tour.runtime.engine.history);
        tour.camera
            .input
            .push(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        tour.settle();
    }
    Err("the rich text marks scene did not put the novel back".to_owned())
}
