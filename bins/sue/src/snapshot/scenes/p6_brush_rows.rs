//! Phase 6, "Brush settings in Tool Property (FG-4)": Tool Property and
//! Sub Tool Detail for a pen, a brush and the Spray, so the rows can be
//! looked at in every theme.

use super::super::Tour;

/// 36r and 36s. For the G-pen, Oil paint and Spray: Tool Property, then
/// Sub Tool Detail on its Ink, Brush Tip, Spraying Effect, Stroke and
/// Correction categories. The tour calls this where Sue Paint has a
/// picture open and its palettes at rest, and leaves them so.
pub(super) fn rows(tour: &mut Tour<'_>) -> Result<(), String> {
    let subs = [
        ("pen", "subtool.pen.G-pen", "pen"),
        ("brush", "subtool.brush.Oil_paint", "brush"),
        ("airbrush", "subtool.airbrush.Spray", "spray"),
    ];
    for (tool, sub, name) in subs {
        tour.paint_command(&format!("tool.pick.{tool}"));
        tour.paint_command(sub);
        tour.paint_go(&["tab/tool/1"]);
        tour.shoot(&format!("36r-sue-paint-rows-{name}-tool-property"))?;
        tour.paint_command("sub_tool.detail");
        for (index, category) in [
            (1, "ink"),
            (4, "brush-tip"),
            (5, "spraying-effect"),
            (6, "stroke"),
            (13, "correction"),
        ] {
            tour.paint_go(&[&format!("detail/{index}")]);
            tour.shoot(&format!("36s-sue-paint-rows-{name}-detail-{category}"))?;
        }
        tour.paint_command("window.sub_tool_detail");
    }
    tour.paint_command("tool.operation");
    Ok(())
}
