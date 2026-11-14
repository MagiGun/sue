//! Scenes of Settings.

use egui::Pos2;
use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::super::Tour;

/// 04 and 05. Settings, on two categories; 04b. the color wheel behind
/// the Void swatch.
pub(super) fn desktop(tour: &mut Tour<'_>) -> Result<(), String> {
    for (page, name) in [
        ("appearance", "04-settings-appearance"),
        ("general", "05-settings-general"),
    ] {
        tour.request(ShellRequest::OpenSettings(Some(page.to_owned())));
        tour.shoot(name)?;
        if page == "appearance" {
            tour.camera.click(Pos2::new(1176.0, 318.0));
            tour.shoot("04b-color-wheel")?;
            tour.camera.click(Pos2::new(700.0, 860.0));
            tour.settle();
        }
    }
    tour.runtime.shell.close_window();
    tour.runtime.shell.dismiss_overlays();
    Ok(())
}

/// 24 to 26. Settings shaped to each screen: a phone's list and page,
/// upright and on its side; a tablet's two panes, upright and on its
/// side; a small desktop window.
pub(super) fn screens(tour: &mut Tour<'_>) -> Result<(), String> {
    for (interface, size, page, name) in [
        ("phone", [393_u32, 851_u32], "", "24-phone-settings"),
        ("phone", [393, 851], "display", "24a-phone-settings-display"),
        (
            "phone",
            [851, 393],
            "appearance",
            "24b-phone-on-its-side-settings",
        ),
        (
            "tablet",
            [800, 1280],
            "appearance",
            "25-tablet-upright-settings",
        ),
        (
            "tablet",
            [1280, 800],
            "display",
            "25a-tablet-on-its-side-settings",
        ),
        ("desktop", [720, 520], "display", "26-small-window-settings"),
    ] {
        tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text(interface.to_owned()));
        tour.camera.size = size;
        tour.request(ShellRequest::OpenSettings(Some(page.to_owned())));
        tour.shoot(name)?;
    }
    tour.runtime.shell.close_window();
    tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text("auto".to_owned()));
    Ok(())
}
