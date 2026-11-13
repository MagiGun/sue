//! Scenes of the shell: the Sue menu, the command palette, entering a
//! studio, languages and the Layout menu.

use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::super::Tour;
use super::{open_paint, to_the_hub};

/// Clicks the Sue button over the next frames.
fn click_the_sue_button(tour: &mut Tour<'_>) {
    let at = tour.runtime.shell.sue_button().center();
    tour.camera.click(at);
}

/// Scene 02. The Sue menu under the Sue button.
pub(super) fn sue_menu(tour: &mut Tour<'_>) -> Result<(), String> {
    click_the_sue_button(tour);
    tour.shoot("02-sue-menu")?;
    tour.runtime.shell.dismiss_overlays();
    Ok(())
}

/// Scene 03. The command palette.
pub(super) fn command_palette(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.runtime.shell.open_palette();
    tour.runtime.shell.palette_query("open");
    tour.shoot("03-command-palette")?;
    tour.runtime.shell.dismiss_overlays();
    Ok(())
}

/// Scene 09. The Sue menu over Sue Paint.
pub(super) fn sue_menu_in_paint(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.request(ShellRequest::SueMenu);
    tour.shoot("09-sue-menu-in-paint")?;
    tour.runtime.shell.dismiss_overlays();
    tour.settle();
    Ok(())
}

/// 12 and 13. Back to the Hub, then into Sue Paint again: the page comes
/// in over the Hub and its palettes slide in.
pub(super) fn entering(tour: &mut Tour<'_>) -> Result<(), String> {
    to_the_hub(tour);
    tour.settle();
    open_paint(tour);
    for (frames, name) in [(6, "12-entering-a"), (14, "13-entering-b")] {
        tour.shoot_entering(name, frames)?;
    }
    tour.settle();
    Ok(())
}

/// 19d. Sue in Japanese, chosen from the Sue menu's Language submenu;
/// 19e. Sue Paint set apart in Spanish while Sue stays Japanese.
pub(super) fn languages(tour: &mut Tour<'_>) -> Result<(), String> {
    to_the_hub(tour);
    click_the_sue_button(tour);
    let japanese = sue_lang::name("ja");
    tour.sue_menu_pick(&["Language", &japanese]);
    tour.settle();
    click_the_sue_button(tour);
    tour.shoot("19d-sue-menu-japanese")?;
    tour.runtime.shell.dismiss_overlays();
    tour.set("sue_paint.language", RiceValue::Text("es".to_owned()));
    open_paint(tour);
    click_the_sue_button(tour);
    tour.shoot("19e-sue-paint-spanish-in-japanese-sue")?;
    tour.runtime.shell.dismiss_overlays();
    for (key, value) in [
        (sue_wm::LANGUAGE_KEY, sue_lang::SYSTEM),
        ("sue_paint.language", sue_lang::FOLLOW),
    ] {
        tour.set(key, RiceValue::Text(value.to_owned()));
    }
    Ok(())
}

/// Picks `path` in the Sue menu, opened by a request.
fn layout(tour: &mut Tour<'_>, path: &[&str]) {
    tour.request(ShellRequest::SueMenu);
    tour.sue_menu_pick(path);
}

/// 19f to 19k. The Sue menu's Layout in a computer's window: the phone
/// layout filling it and phone shaped, the tablet layout, and back.
pub(super) fn layouts(tour: &mut Tour<'_>) -> Result<(), String> {
    to_the_hub(tour);
    click_the_sue_button(tour);
    tour.shoot("19f-sue-menu-layout")?;
    tour.runtime.shell.dismiss_overlays();
    layout(tour, &["Layout", "Phone"]);
    tour.shoot("19g-desktop-as-phone-hub")?;
    open_paint(tour);
    tour.shoot("19h-desktop-as-phone-sue-paint")?;
    layout(tour, &["Layout", "Phone Shaped"]);
    tour.shoot("19i-desktop-phone-shaped-sue-paint")?;
    layout(tour, &["Layout", "Tablet"]);
    tour.shoot("19j-desktop-as-tablet-sue-paint")?;
    layout(tour, &["Layout", "Automatic (Desktop)"]);
    to_the_hub(tour);
    tour.shoot("19k-desktop-again-hub")?;
    tour.set(sue_wm::PHONE_FRAME_KEY, RiceValue::Bool(false));
    Ok(())
}
