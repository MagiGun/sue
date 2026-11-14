//! Scenes of Sue Writer's corkboard.

use super::super::Tour;
use super::writer_view;

/// Scene 15. The corkboard of the sample novel.
pub(super) fn corkboard(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "view/corkboard", "15-sue-writer-corkboard")
}
