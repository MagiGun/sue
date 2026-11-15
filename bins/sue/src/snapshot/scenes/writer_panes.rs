//! Scenes of Sue Writer's side panes: the inspector beside the text and
//! beside the outliner (P4-11B).

use super::super::Tour;

/// 16a. The inspector beside the text of the sample novel: synopsis,
/// label and status, word target, keywords, statistics, notes and
/// snapshots.
pub(super) fn inspector(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    tour.writer_go(&["dialog/close", "view/document"]);
    tour.command(&writer, "view.inspector");
    tour.shoot("16a-sue-writer-inspector")?;
    // 16b. The same beside the outliner.
    tour.writer_go(&["view/outliner"]);
    tour.shoot("16b-sue-writer-outliner-inspector")?;
    tour.command(&writer, "view.inspector");
    tour.writer_go(&["view/document"]);
    Ok(())
}
