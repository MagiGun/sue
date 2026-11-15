//! Scenes of Sue Paint's Modifier Key Settings.

use super::super::Tour;

/// 11m. Modifier Key Settings: General's keys on the canvas, as Sue
/// starts.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("file.modifiers");
    tour.shoot("11m-sue-paint-modifier-keys")?;
    tour.paint_go(&["dialog/close"]);
    Ok(())
}
