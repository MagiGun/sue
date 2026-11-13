//! Scenes of Sue Writer's outliner.

use super::super::Tour;
use super::writer_view;

/// Scene 16. The outliner of the sample novel.
pub(super) fn outliner(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "view/outliner", "16-sue-writer-outliner")
}
