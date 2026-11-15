//! Scenes of Sue Paint's dock: where its palettes sit, float and fold.
//! The tour calls this file where Sue Paint has a picture open and its
//! palettes are at rest.

use super::super::Tour;

/// 34a to 34c. The Layer palette's menu at the top left of its header,
/// with its own commands, where it docks, and Hide Layer Palette last;
/// every palette hidden by Hide All Palettes, the command bar staying;
/// the Animation workspace, the Timeline open under the canvas.
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
    Ok(())
}
