//! Scenes of Sue Paint's command bar and Quick Access: a bar with a
//! button of every kind, its right-click menu, its popovers, the settings
//! dialog, and Quick Access with its sets. The tour calls this file where
//! Sue Paint has a picture open and its palettes are at rest.

use super::super::Tour;

/// The places on the sample bar (`bar/sample`) of the buttons the scenes
/// open: Undo, the Pen tool, the Color Slider that pops up, the group of
/// two colors.
const UNDO: usize = 4;
const PEN: usize = 26;
const POPUP: usize = 29;
const GROUP: usize = 32;

/// 43a to 43h.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    // 43a. A button of every kind: commands, tools, a sub tool, a palette
    // that pops up, drawing colors, a group of colors, an Auto Action.
    tour.paint_go(&["dialog/close", "bar/sample"]);
    tour.shoot("43a-sue-paint-command-bar-every-kind")?;
    // 43b. The right-click menu of the Pen's button.
    if let Some(rect) = sue_paint::bar_button_rect(&tour.camera.ctx, PEN) {
        let at = rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        tour.camera.input.push(vec![egui::Event::PointerMoved(at)]);
        tour.camera.input.push(vec![button(true)]);
        tour.camera.input.push(vec![button(false)]);
    }
    tour.shoot("43b-sue-paint-command-bar-menu")?;
    tour.camera.input.push(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    tour.settle();
    // 43c to 43e. Icon Settings on Undo; the group's list; the Color
    // Slider popping up.
    for (route, name) in [
        (
            format!("bar/icons/{UNDO}"),
            "43c-sue-paint-command-bar-icons",
        ),
        (
            format!("bar/group/{GROUP}"),
            "43d-sue-paint-command-bar-group",
        ),
        (
            format!("bar/popup/{POPUP}"),
            "43e-sue-paint-command-bar-popup",
        ),
    ] {
        tour.paint_go(&["bar/close", route.as_str()]);
        tour.shoot(name)?;
    }
    tour.paint_go(&["bar/close"]);
    // 43f. Command Bar Settings.
    tour.paint_command("file.command_bar");
    tour.shoot("43f-sue-paint-command-bar-settings")?;
    tour.paint_go(&["dialog/close", "bar/default"]);
    // 43g. Quick Access, Set 1 as Clip Studio Paint ships it; 43h. its
    // settings.
    tour.paint_command("window.quick_access");
    tour.shoot("43g-sue-paint-quick-access")?;
    tour.paint_command("window.quick_access.settings");
    tour.shoot("43h-sue-paint-quick-access-settings")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("window.quick_access");
    tour.settle();
    Ok(())
}
