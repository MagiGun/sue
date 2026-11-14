//! Scenes of Sue Writer's start page in a computer's window.

use super::super::Tour;

/// 19-start. The start page: templates as pages, and the Presets list.
/// The novel is opened again afterwards.
pub(super) fn desktop(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.writer_go(&["dialog/close", "view/document", "start"]);
    tour.shoot("19-sue-writer-start")?;
    tour.open_novel()
}
