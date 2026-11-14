//! Scenes of Sue Paint's start page in a computer's window.

use super::super::Tour;
use super::open_paint;

/// 08a to 08f. Sue Paint opens on its start page while no canvas is open:
/// the projects, the materials and the popups.
pub(super) fn desktop(tour: &mut Tour<'_>) -> Result<(), String> {
    open_paint(tour);
    for (target, name) in [
        ("start/projects", "08a-paint-start-projects"),
        ("start/materials", "08b-paint-start-materials"),
        ("start/transfers", "08c-paint-start-transfers"),
        ("start/help", "08d-paint-start-help"),
        ("start/settings", "08e-paint-start-settings"),
        ("start/maintenance", "08f-paint-start-maintenance"),
    ] {
        tour.paint_go(&["start/close", target]);
        tour.shoot(name)?;
    }
    tour.paint_go(&["start/close", "start/projects"]);
    Ok(())
}
