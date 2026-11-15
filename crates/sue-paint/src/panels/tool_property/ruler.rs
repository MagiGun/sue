//! Tool Property for the Ruler tool.

use egui::Ui;

use super::{Category, Rows};
use crate::tools::SubTool;

/// The Ruler tool's sub tools, and a command they ask for: Create
/// Perspective Ruler or Delete Ruler.
pub(super) fn ruler_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
) -> Option<&'static str> {
    let mut command = None;
    match sub.mode {
        6 => {
            rows.hint(ui, &tr!("click-place-points-click-last-one"));
            rows.hint(ui, &tr!("ctrl-1-snap-ruler-turns-rulers"));
        }
        7 => {
            rows.choice_u8(
                ui,
                "ruler-figure",
                Category::Operation,
                &tr!("figure"),
                &mut sub.kind,
                &[tr!("rectangle"), tr!("ellipse")],
            );
            rows.hint(ui, &tr!("drag-lay-ruler-strokes-start-follow"));
        }
        8 => rows.hint(ui, &tr!("draw-ruler-freehand-strokes-start-follow")),
        5 => {
            rows.hint(ui, &tr!("drag-lay-straight-ruler-strokes-start"));
            rows.hint(ui, &tr!("ctrl-1-snap-ruler-turns-off"));
        }
        2 => {
            let names = super::names(&crate::rulers::Special::NAMES);
            rows.choice_u8(
                ui,
                "special-ruler",
                Category::Operation,
                &tr!("special-ruler"),
                &mut sub.kind,
                &names,
            );
            rows.hint(
                ui,
                &match sub.kind {
                    0 => tr!("drag-set-lines-direction-every-stroke"),
                    1 => tr!("click-middle-every-stroke-runs-toward"),
                    _ => tr!("click-middle-every-stroke-goes-round"),
                },
            );
            rows.hint(ui, &tr!("ctrl-2-snap-special-ruler-turns"));
        }
        1 => {
            rows.choice_u8(
                ui,
                "perspective-process",
                Category::Operation,
                &tr!("process"),
                &mut sub.kind,
                &[
                    tr!("add-vanishing-point-2"),
                    tr!("delete-vanishing-point-2"),
                ],
            );
            rows.check(
                ui,
                "fix-eye-level",
                Category::Operation,
                &tr!("fix-eye-level"),
                Some(&tr!("vanishing-points-slide-along-eye-level")),
                &mut sub.fix_eye_level,
            );
            rows.hint(
                ui,
                &if sub.kind == 1 {
                    tr!("click-vanishing-point-take-away")
                } else {
                    tr!("drag-along-two-edges-meet-distance")
                },
            );
            if rows.button(
                ui,
                "create-perspective-ruler",
                Category::Operation,
                &tr!("create-perspective-ruler-ellipsis"),
            ) {
                command = Some("layer.ruler.perspective");
            }
        }
        _ => {
            rows.hint(ui, &tr!("drag-canvas-lay-ruler-strokes-draw"));
            rows.slider_u32(
                ui,
                "ruler-lines",
                Category::Operation,
                &tr!("number-of-lines"),
                &mut sub.lines,
                2..=32,
            );
            rows.check(
                ui,
                "line-symmetry",
                Category::Operation,
                &tr!("line-symmetry"),
                Some(&tr!("every-other-line-mirrored-as-clip")),
                &mut sub.mirror,
            );
            rows.hint(ui, &tr!("ctrl-2-snap-special-ruler-turns"));
        }
    }
    if rows.button(
        ui,
        "delete-ruler",
        Category::Operation,
        &tr!("delete-ruler"),
    ) {
        command = Some("layer.ruler.delete");
    }
    command
}
