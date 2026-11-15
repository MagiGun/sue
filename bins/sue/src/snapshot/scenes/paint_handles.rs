//! Scenes of what is dragged on Sue Paint's canvas without being a
//! stroke: the Selection launcher and its settings, Mesh Transformation,
//! puppet warp's pins and bar, the camera frame and the light table's box.
//! Each scene puts the sample illustration back as it found it.

use egui::{Pos2, vec2};

use super::super::Tour;

/// Where the last frame drew the launcher button or mark `id`.
fn shown(tour: &Tour<'_>, id: &str) -> Option<Pos2> {
    sue_paint::shown_buttons(&tour.camera.ctx)
        .into_iter()
        .find(|(found, _)| found == id)
        .map(|(_, rect)| rect.center())
}

/// A drag with the primary button from `from` to `to` over the next
/// frames.
fn drag(tour: &mut Tour<'_>, from: Pos2, to: Pos2) {
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let input = &mut tour.camera.input;
    input.push(vec![egui::Event::PointerMoved(from)]);
    input.push(vec![press(from, true)]);
    for index in 1..=12 {
        let at = from + (to - from) * (index as f32 / 12.0);
        input.push(vec![egui::Event::PointerMoved(at)]);
    }
    input.push(vec![press(to, false)]);
    tour.settle();
}

/// The canvas's middle, where the sample illustration lies.
fn middle(tour: &Tour<'_>) -> Pos2 {
    tour.runtime.shell.page_rect().center() + vec2(8.0, -63.0)
}

/// 08ea to 08ef.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    // 08ea and 08eb. The Selection launcher under a selection, then its
    // settings over it.
    tour.paint_command("select.all");
    tour.shoot("08ea-sue-paint-selection-launcher")?;
    if let Some(at) = shown(tour, "launcher.settings") {
        tour.camera.click(at);
    }
    tour.shoot("08eb-sue-paint-selection-launcher-settings")?;
    if let Some(at) = shown(tour, "launcher.settings") {
        tour.camera.click(at);
    }
    tour.paint_command("select.none");
    tour.settle();

    // 08ec. Mesh Transformation's lattice and its bar.
    tour.paint_command("edit.transform.mesh");
    tour.settle();
    if let Some(corner) = shown(tour, "mark.transform.corner.2") {
        drag(tour, corner, corner + vec2(30.0, 24.0));
    }
    tour.shoot("08ec-sue-paint-mesh-transformation")?;
    tour.paint_command("edit.transform.cancel");
    tour.settle();

    // 08ed. Puppet warp: two pins, one picked, and its bar.
    tour.paint_command("edit.transform.puppet");
    tour.settle();
    let at = middle(tour);
    tour.camera.click(at - vec2(80.0, 0.0));
    tour.camera.click(at + vec2(60.0, 30.0));
    tour.shoot("08ed-sue-paint-puppet-warp")?;
    tour.camera.input.push(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    tour.settle();

    // 08ee. A 2D camera folder and its frame, zoomed in a little, with the
    // Object tool. New Timeline asks for its settings first (P4-7):
    // Enter takes what its dialog offers, so no dialog is left over the
    // frame.
    let depth = tour.runtime.engine.history.depth();
    tour.paint_command("anim.timeline.new");
    tour.settle();
    tour.camera.input.push(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    tour.settle();
    tour.paint_command("anim.layer.camera");
    tour.paint_command("tool.operation");
    tour.settle();
    if let Some(corner) = shown(tour, "mark.camera.corner.2") {
        drag(tour, corner, corner - vec2(90.0, 60.0));
    }
    tour.shoot("08ee-sue-paint-camera-frame")?;

    // 08ef. The Light table tool's box round a layer on the general light
    // table.
    tour.paint_command("anim.cel.new");
    tour.paint_command("layer.select_below");
    tour.paint_command("anim.light_table.register_general");
    tour.paint_command("anim.light_table.tool");
    tour.settle();
    tour.shoot("08ef-sue-paint-light-table-box")?;
    tour.paint_command("anim.light_table.tool");
    tour.paint_command("tool.pen");
    while tour.runtime.engine.history.depth() > depth {
        tour.paint_command("edit.undo");
    }
    tour.settle();
    Ok(())
}
