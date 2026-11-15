//! Scenes of Sue Paint's canvas chrome: a canvas in a window of its own.

use super::super::Tour;

/// 40a. Window, Canvas, Canvas in Its Own Window on the sample canvas:
/// where Sue has one window (as the snapshot camera does) it floats over
/// the page, with its own status strip and the button that puts it back,
/// and the column says where every canvas went. It comes back after.
pub(super) fn window(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("window.float");
    tour.settle();
    tour.shoot("40a-sue-paint-canvas-window")?;
    tour.paint_command("window.float");
    tour.settle();
    Ok(())
}
