//! Scenes of Sue Paint's start page in a computer's window.

use super::super::{Tour, online};
use super::open_paint;

/// 08a to 08f. Sue Paint opens on its start page while no canvas is open:
/// the projects, the materials and the popups. 08y and 08z. Browse Assets
/// and a material's page, read from the network when that is wanted.
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
    // Read from CLIP STUDIO ASSETS (or its cached copy), so give the site
    // a few seconds.
    if online() {
        for (target, name) in [
            ("start/browse", "08y-paint-start-browse"),
            ("start/browse/2317055", "08z-paint-start-asset"),
        ] {
            tour.paint_go(&["start/close", target]);
            tour.wait_for_the_network();
            tour.shoot(name)?;
        }
    }
    tour.paint_go(&["start/close", "start/projects"]);
    Ok(())
}
