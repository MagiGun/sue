//! Scenes of Sue Paint with no canvas open, and of its Materials place
//! in the Hub, in a computer's window. Sue 0.0.8 removed Sue Paint's own
//! start page: with no canvas it shows its menus, palettes and an empty
//! canvas area with New and Open.

use sue_wm::ShellRequest;

use super::super::Tour;
use super::{open_paint, to_the_hub};

/// 08a to 08d. Sue Paint with no canvas open, its Help menu, then the
/// Hub's Materials place and its menu. Open asks the system's dialog
/// first, answered here with a picture that is then closed, so the Hub
/// knows of one.
pub(super) fn desktop(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("file.open");
    let picture = tour.home.join("test-card.jxl");
    if !tour
        .runtime
        .answer_pick(&picture, &mut tour.camera.painter, tour.camera.now)
    {
        return Err("Open did not ask for a file".to_owned());
    }
    tour.paint_command("file.close");
    open_paint(tour);
    tour.shoot("08a-paint-no-canvas")?;
    tour.paint_go(&["menu/Help"]);
    tour.shoot("08b-paint-help-menu")?;
    tour.paint_go(&["dialog/close"]);
    tour.request(ShellRequest::HubPlace("materials".to_owned()));
    tour.shoot("08c-hub-materials")?;
    tour.paint_go(&["materials/menu"]);
    tour.shoot("08d-hub-materials-menu")?;
    tour.paint_go(&["materials/close"]);
    to_the_hub(tour);
    Ok(())
}
