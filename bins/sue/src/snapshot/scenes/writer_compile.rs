//! Scenes of Sue Writer's compile dialog.

use super::super::Tour;
use super::writer_view;

/// Scene 18. The compile dialog.
pub(super) fn dialog(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "dialog/compile", "18-sue-writer-compile")
}
