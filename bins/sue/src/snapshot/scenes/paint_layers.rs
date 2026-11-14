//! Scenes of Sue Paint's layer palette and its tabs.

use super::super::Tour;

/// 08i. The layer's History, a tab of the layer palette. The tool palette
/// still shows the tab of the scene before; `first_tab` here and in
/// `paint_tools` put both back afterwards.
pub(super) fn history(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["tab/layer/2"]);
    tour.shoot("08i-sue-paint-history")
}

/// Puts the layer palette back on the tab it opens with.
pub(super) fn first_tab(tour: &mut Tour<'_>) {
    tour.paint_go(&["tab/layer/0"]);
}

/// 11d3. The Layer Property tab, as Clip Studio Paint's palette, over
/// the timeline of the scene before.
pub(super) fn property(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["tab/layer/1"]);
    tour.shoot("11d3-sue-paint-layer-property")?;
    first_tab(tour);
    Ok(())
}
