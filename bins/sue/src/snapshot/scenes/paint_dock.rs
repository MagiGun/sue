//! Scenes of Sue Paint's dock: where its palettes sit, float and fold.
//! The tour calls this file where Sue Paint has a picture open and its
//! palettes are at rest.

use super::super::Tour;

/// 34a to 34c. The Layer palette's menu at the top left of its header,
/// with its own commands, where it docks, and Hide Layer Palette last;
/// every palette hidden by Hide All Palettes, the command bar staying;
/// the Animation workspace, the Timeline open under the canvas.
/// 34d and 34e: the Color Set palette floating over the canvas, whole and
/// rolled up to its header.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["dock/menu/layer"]);
    tour.shoot("34a-sue-paint-palette-menu")?;
    tour.paint_go(&["dock/menu"]);
    tour.paint_command("window.hide_all");
    tour.shoot("34b-sue-paint-hide-all-palettes")?;
    tour.paint_command("window.hide_all");
    tour.paint_command("window.workspace.preset.animation");
    tour.shoot("34c-sue-paint-workspace-animation")?;
    tour.paint_command("window.workspace.preset.illustration");
    // A floating palette: its header is its title bar, its name once,
    // with its menu, roll up, dock again and close; then rolled up.
    tour.paint_go(&["dock/float/color_set"]);
    tour.shoot("34d-sue-paint-floating-palette")?;
    tour.paint_go(&["dock/roll/color_set"]);
    tour.shoot("34e-sue-paint-floating-rolled-up")?;
    // Hidden and shown again, it goes back where the style keeps it.
    tour.paint_command("window.color_set");
    tour.paint_command("window.color_set");
    Ok(())
}
