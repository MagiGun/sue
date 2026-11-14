//! Scenes of Sue Writer's compile dialog.

use super::super::Tour;
use super::writer_view;

/// Scene 18. The compile dialog.
pub(super) fn dialog(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "dialog/compile", "18-sue-writer-compile")
}

/// Scenes 18b to 18f. The Export dialog on Plain text with its tidying,
/// the question a CSV asks before it opens, About, and the File and
/// Table menus of the shared skeleton.
pub(super) fn export_and_tables(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(
        tour,
        "dialog/compile-txt",
        "18b-sue-writer-export-plain-text",
    )?;
    writer_view(tour, "dialog/table-file", "18c-sue-writer-table-file")?;
    writer_view(tour, "dialog/about", "18d-sue-writer-about")?;
    writer_view(tour, "menu/File", "18e-sue-writer-file-menu")?;
    writer_view(tour, "menu/Table", "18f-sue-writer-table-menu")
}
