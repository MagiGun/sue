//! Scenes of Sue Paint's selection, fill and gradient tools (P4-1b).

use super::super::Tour;

/// 08p. The Selection area tool on Shrink selection, its seventh sub
/// tool, as the Sub Tool palette shows it.
pub(super) fn shrink_selection(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("tool.selection");
    for _ in 0..6 {
        tour.paint_command("tool.next_sub");
    }
    tour.shoot("08p-sue-paint-shrink-selection")?;
    // Back to the first sub tool and the pen, as the scenes after expect.
    tour.paint_command("tool.next_sub");
    tour.paint_command("tool.pen");
    Ok(())
}
