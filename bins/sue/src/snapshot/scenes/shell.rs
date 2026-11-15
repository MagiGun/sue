//! Scenes of the shell: the Sue menu, the command palette, entering a
//! studio, languages and the Interface menu.

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

/// 19d. Sue in Japanese (chosen in Settings) in the Sue menu;
/// 19e. Sue Paint set apart in Spanish while Sue stays Japanese.
pub(super) fn languages(tour: &mut Tour<'_>) -> Result<(), String> {
    to_the_hub(tour);
    tour.request(ShellRequest::Language("ja".to_owned()));
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

/// 19f to 19k. The Sue menu's Interface in a computer's window: the phone
/// layout filling it and phone shaped, the tablet layout, and back.
pub(super) fn layouts(tour: &mut Tour<'_>) -> Result<(), String> {
    to_the_hub(tour);
    click_the_sue_button(tour);
    tour.shoot("19f-sue-menu-layout")?;
    tour.runtime.shell.dismiss_overlays();
    layout(tour, &["Interface", "Phone"]);
    tour.shoot("19g-desktop-as-phone-hub")?;
    open_paint(tour);
    tour.shoot("19h-desktop-as-phone-sue-paint")?;
    layout(tour, &["Interface", "Phone Shaped"]);
    tour.shoot("19i-desktop-phone-shaped-sue-paint")?;
    layout(tour, &["Interface", "Tablet"]);
    tour.shoot("19j-desktop-as-tablet-sue-paint")?;
    layout(tour, &["Interface", "Automatic (Desktop)"]);
    to_the_hub(tour);
    tour.shoot("19k-desktop-again-hub")?;
    tour.set(sue_wm::PHONE_FRAME_KEY, RiceValue::Bool(false));
    Ok(())
}

/// 19l. Notices on the toast stack over Sue Paint: a saved file, with
/// its button that shows the file, and a command that is not there.
/// 19m. Quitting with Sue Paint's work not saved: the question in the
/// dialog host. Escape answers Cancel, so the tour goes on.
pub(super) fn overlays(tour: &mut Tour<'_>) -> Result<(), String> {
    open_paint(tour);
    tour.settle();
    let home = tour.runtime.engine.catalog.folders["home"].clone();
    tour.request(ShellRequest::Saved {
        path: home.join("Picture.png"),
        note: "PNG".to_owned(),
    });
    tour.request(ShellRequest::AppletCommand {
        applet: tour.paint.clone(),
        command: "no.such.command".to_owned(),
    });
    tour.shoot("19l-notices")?;
    let unsaved = tour
        .runtime
        .applets
        .get(&tour.paint)
        .map(|applet| applet.unsaved(&tour.runtime.engine.world.arena))
        .unwrap_or_default();
    if unsaved.is_empty() {
        return Err("Sue Paint has no work to ask about".to_owned());
    }
    tour.runtime.shell.begin_quit();
    tour.shoot("19m-quit-question")?;
    tour.camera.input.push(Vec::new());
    tour.camera.input.push(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    tour.settle();
    if tour.runtime.shell.quitting() {
        return Err("Escape did not answer the quit question".to_owned());
    }
    Ok(())
}
