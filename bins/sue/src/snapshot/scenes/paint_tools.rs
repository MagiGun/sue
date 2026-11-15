//! Scenes of Sue Paint's tool palettes: Tool, Sub Tool, Tool Property,
//! Brush Size and Sub Tool Detail.

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

/// 36a to 36e. The Tool palette's own menu; the Sub Tool palette's menu;
/// Tool Property with the names of its categories shown; Sub Tool Detail
/// beside the palettes, every setting of one category with its eye; the
/// dialog Duplicate Sub Tool opens. The tour calls this where Sue Paint
/// has a picture open, its palettes at rest and the Operation tool out,
/// and leaves them so.
pub(super) fn palettes(tour: &mut Tour<'_>) -> Result<(), String> {
    // The Pen out: the tool with the most to show.
    tour.paint_command("tool.pen");
    tour.paint_go(&["dock/menu/tool"]);
    tour.shoot("36a-sue-paint-tool-palette-menu")?;
    tour.paint_go(&["dock/menu/sub_tool"]);
    tour.shoot("36b-sue-paint-sub-tool-menu")?;
    tour.paint_go(&["dock/menu"]);
    tour.paint_command("sub_tool.categories");
    tour.shoot("36c-sue-paint-tool-property-categories")?;
    tour.paint_command("sub_tool.categories");
    tour.paint_command("sub_tool.detail");
    tour.shoot("36d-sue-paint-sub-tool-detail")?;
    tour.paint_command("window.sub_tool_detail");
    tour.paint_command("sub_tool.duplicate");
    tour.shoot("36e-sue-paint-duplicate-sub-tool")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("tool.operation");
    Ok(())
}
