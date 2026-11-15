//! Scenes of the Hub.

use egui::Rect;
use sue_wm::{HubButton, ShellRequest};

use super::super::Tour;
use super::to_the_hub;
use crate::runtime::Runtime;

/// Where the Hub drew `applet`'s card and its pin in the last frame.
fn hub_box(runtime: &Runtime, applet: &str) -> Option<(Rect, Rect)> {
    runtime
        .shell
        .hub_boxes()
        .iter()
        .find(|(id, _, _)| id.as_str() == applet)
        .map(|(_, rect, pin)| (*rect, *pin))
}

/// Where the Hub drew `button` in the last frame.
fn hub_button(runtime: &Runtime, button: &HubButton) -> Option<Rect> {
    runtime
        .shell
        .hub_buttons()
        .iter()
        .find(|(drawn, _)| drawn == button)
        .map(|(_, rect)| *rect)
}

/// Lets the Hub's thumbnails finish, so the cards show their pictures.
fn thumbnails(tour: &mut Tour<'_>) {
    for _ in 0..200 {
        tour.settle();
        if !tour.runtime.shell.hub_working() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    tour.settle();
}

/// Applies the built-in preset `id`, as one step of the history.
fn wear(tour: &mut Tour<'_>, id: &str) -> Result<(), String> {
    let preset = sue_rice::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| format!("no {id} preset"))?;
    let txn = preset.transaction(&tour.runtime.engine.world.rice);
    tour.runtime
        .engine
        .history
        .apply(&mut tour.runtime.engine.world, sue_wm::RiceEdit(txn))
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Takes the last preset off again.
fn unwear(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.runtime
        .engine
        .history
        .undo(&mut tour.runtime.engine.world)
        .map_err(|error| error.to_string())?;
    Ok(())
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

/// 19b, 19c and 31a to 31f. The Hub with work in it: the novel saved as
/// a project, a picture opened in Sue Paint, and a picture another run
/// kept unsaved. Home in the four presets, Files in Void, Paper and Gray.
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
    // A picture opened in Sue Paint; the Hub reads its lists again when it
    // comes back.
    tour.request(ShellRequest::OpenFile(tour.home.join("photo.png")));
    tour.settle();
    to_the_hub(tour);
    thumbnails(tour);
    tour.shoot("19b-hub-home")?;
    for (preset, name) in [
        ("paper", "19c-hub-home-paper"),
        ("gray", "31a-hub-home-gray"),
        ("graphite", "31f-hub-home-graphite"),
    ] {
        wear(tour, preset)?;
        tour.shoot(name)?;
        unwear(tour)?;
    }
    // Your work's Unsaved Work button opens Files on the kept work.
    if let Some(button) = hub_button(&tour.runtime, &HubButton::Work(sue_wm::WorkShelf::Unsaved)) {
        tour.camera.click(button.center());
    }
    thumbnails(tour);
    tour.shoot("31b-hub-files-unsaved")?;
    // Home's All Files opens Files on everything, newest first.
    to_the_hub(tour);
    thumbnails(tour);
    if let Some(button) = hub_button(&tour.runtime, &HubButton::AllFiles) {
        tour.camera.click(button.center());
    }
    thumbnails(tour);
    tour.shoot("31c-hub-files")?;
    for (preset, name) in [
        ("paper", "31d-hub-files-paper"),
        ("gray", "31e-hub-files-gray"),
    ] {
        wear(tour, preset)?;
        tour.shoot(name)?;
        unwear(tour)?;
    }
    to_the_hub(tour);
    Ok(())
}

/// The Hub on a touch screen: Home, then Files.
pub(super) fn touch(tour: &mut Tour<'_>, prefix: &str) -> Result<(), String> {
    to_the_hub(tour);
    thumbnails(tour);
    tour.shoot(&format!("{prefix}-hub"))?;
    tour.request(ShellRequest::HubPlace(sue_wm::HUB_FILES.to_owned()));
    thumbnails(tour);
    tour.shoot(&format!("{prefix}-hub-files"))?;
    to_the_hub(tour);
    Ok(())
}
