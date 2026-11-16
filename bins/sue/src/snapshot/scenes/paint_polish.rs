//! Sue Paint's default layout with the Timeline open under the canvas.

use super::super::Tour;

/// 11p1. Sue Paint's palettes at rest, the Timeline opened under the
/// canvas at the height it opens at, the sample animation's frames on
/// it, and the Sub Tool palette showing whole rows. The Timeline stays
/// open, as the timeline scenes before left it.
pub(super) fn timeline_docked(tour: &mut Tour<'_>) -> Result<(), String> {
    super::paint_at_rest(tour);
    tour.paint_command("window.timeline");
    tour.shoot("11p1-sue-paint-timeline-docked")
}
