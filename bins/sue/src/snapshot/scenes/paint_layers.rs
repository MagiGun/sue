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
    // Layer Property is a palette of its own in the dock, which Layer,
    // Layer Property brings forward.
    tour.paint_command("layer.properties");
    tour.shoot("11d3-sue-paint-layer-property")?;
    first_tab(tour);
    Ok(())
}

/// 11d4. The Layer palette's rows: a folder with a palette color and a
/// layer color, locked, holding a draft layer with a mask, both picked.
/// Every step is undone after, so the scenes that follow see the
/// picture as it was.
pub(super) fn rows(tour: &mut Tour<'_>) -> Result<(), String> {
    let depth = tour.runtime.engine.history.depth();
    for command in [
        "layer.new",
        "layer.palette_color.blue",
        "layer.toggle.draft",
        "layer.mask.outside",
        "layer.folder_insert",
        "layer.palette_color.red",
        "layer.toggle.layer_color",
        "layer.toggle.lock",
        "layer.select_all",
    ] {
        tour.paint_command(command);
    }
    // The Layer palette in front of its stack: Window, Layer closes it
    // and opens it again chosen; History is chosen again after.
    tour.paint_command("window.layer");
    tour.paint_command("window.layer");
    tour.shoot("11d4-sue-paint-layer-rows")?;
    tour.paint_command("layer.select_none");
    while tour.runtime.engine.history.depth() > depth {
        tour.paint_command("edit.undo");
    }
    tour.paint_command("window.history");
    tour.paint_command("window.history");
    tour.settle();
    Ok(())
}
