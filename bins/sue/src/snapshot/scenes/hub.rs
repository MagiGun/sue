//! Scenes of the Hub.

use egui::Rect;
use sue_wm::ShellRequest;

use super::super::Tour;
use super::to_the_hub;
use crate::runtime::Runtime;

/// Where the Hub drew `applet`'s box and its pin in the last frame.
fn hub_box(runtime: &Runtime, applet: &str) -> Option<(Rect, Rect)> {
    runtime
        .shell
        .hub_boxes()
        .iter()
        .find(|(id, _, _)| id.as_str() == applet)
        .map(|(_, rect, pin)| (*rect, *pin))
}

/// Scene 00. The Hub Sue opens on; 01. Sue Paint pinned with a click on its
/// pin, and unpinned again.
pub(super) fn opening(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.shoot("00-hub")?;
    if let Some((_, pin)) = hub_box(&tour.runtime, sue_paint::ID) {
        tour.camera.click(pin.center());
    }
    tour.shoot("01-hub-pinned")?;
    tour.request(ShellRequest::Pin(None));
    Ok(())
}

/// 19b and 19c. The Hub's lists: the novel saved as a project, the
/// picture opened earlier, and a picture another run kept unsaved; then
/// the same Hub in Paper.
pub(super) fn lists(tour: &mut Tour<'_>) -> Result<(), String> {
    let project = tour.home.join("Novel.sue");
    tour.runtime.save_project_to(&project)?;
    let bytes = std::fs::read(&project).map_err(|error| error.to_string())?;
    let mut other = sue_wm::WorkList::open(tour.runtime.engine.paths.data_dir.join("work"));
    other
        .keep(
            &bytes,
            None,
            "Illustration",
            Some(tour.paint.clone()),
            sue_wm::unix_now().saturating_sub(300),
        )
        .map_err(|error| error.to_string())?;
    to_the_hub(tour);
    tour.shoot("19b-hub-lists")?;
    let paper = sue_rice::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == "paper")
        .ok_or("no Paper preset")?;
    let txn = paper.transaction(&tour.runtime.engine.world.rice);
    tour.runtime
        .engine
        .history
        .apply(&mut tour.runtime.engine.world, sue_wm::RiceEdit(txn))
        .map_err(|error| error.to_string())?;
    tour.shoot("19c-hub-lists-paper")?;
    tour.runtime
        .engine
        .history
        .undo(&mut tour.runtime.engine.world)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// The Hub on a touch screen.
pub(super) fn touch(tour: &mut Tour<'_>, prefix: &str) -> Result<(), String> {
    to_the_hub(tour);
    tour.shoot(&format!("{prefix}-hub"))
}
