//! Scenes of Sue Paint's tool palette.

use super::super::Tour;

/// 08g and 08h. The Sub Tool list and the Brush Size grid, as tabs of the
/// tool palette.
pub(super) fn tabs(tour: &mut Tour<'_>) -> Result<(), String> {
    for (target, name) in [
        ("tab/tool/0", "08g-sue-paint-sub-tools"),
        ("tab/tool/2", "08h-sue-paint-brush-size"),
    ] {
        tour.paint_go(&[target]);
        tour.shoot(name)?;
    }
    Ok(())
}

/// Puts the tool palette back on the tab it opens with.
pub(super) fn first_tab(tour: &mut Tour<'_>) {
    tour.paint_go(&["tab/tool/1"]);
}
