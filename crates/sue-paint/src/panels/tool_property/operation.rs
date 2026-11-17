//! Tool Property for the tools that look, pick and move: Zoom, the
//! Operation tool's four sub tools (Object, Select layer, Light table and
//! Edit timeline), Move layer and the Eyedropper.

use egui::Ui;

use super::{Category, Rows, fill, name_cell, name_width};
use crate::tools::{
    OPERATION_EDIT_TIMELINE, OPERATION_LIGHT_TABLE, PICKS_3D, PICKS_CAMERA, PICKS_COMIC,
    PICKS_RASTER, PICKS_VECTOR, SubTool,
};

/// The Zoom tool: what a click does (in or out, Alt the other way) and
/// what a drag does (zooms as it goes, zooms to the area dragged out, or
/// nothing).
pub(super) fn zoom_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.choice_u8(
        ui,
        "zoom-direction",
        Category::Operation,
        &tr!("tp-click"),
        &mut sub.mode,
        &[tr!("tp-zoom-in"), tr!("tp-zoom-out")],
    );
    rows.choice_u8(
        ui,
        "zoom-drag",
        Category::Operation,
        &tr!("drag"),
        &mut sub.kind,
        &[
            tr!("tp-zoom-as-it-goes"),
            tr!("tp-zoom-to-the-area"),
            tr!("tp-nothing"),
        ],
    );
    rows.hint(ui, &tr!("alt-click-does-the-opposite"));
}

/// The Operation tool, by sub tool: Object's Operation of transparent
/// part and Selectable object, Select layer's Exclude row, the Light
/// table's drag and click, Edit timeline's click and drag.
pub(super) fn operation_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    match sub.mode {
        1 => {
            let mut exclude = sub.exclude;
            rows.row(
                ui,
                "select-layer-exclude",
                Category::Reference,
                |ui, theme, enabled| {
                    fill::exclude_row(ui, &mut exclude, false, enabled, theme);
                },
            );
            sub.exclude = exclude;
            rows.hint(ui, &tr!("tp-select-layer-hint"));
        }
        OPERATION_LIGHT_TABLE => {
            rows.choice_u8(
                ui,
                "light-table-drag",
                Category::Operation,
                &tr!("drag"),
                &mut sub.kind,
                &[tr!("drag-move"), tr!("drag-rotate")],
            );
            rows.choice_u8(
                ui,
                "light-table-click",
                Category::Operation,
                &tr!("tp-click"),
                &mut sub.click,
                &[tr!("tp-nothing"), tr!("tp-reset-position")],
            );
            rows.hint(ui, &tr!("tp-light-table-hint"));
        }
        OPERATION_EDIT_TIMELINE => {
            rows.choice_u8(
                ui,
                "timeline-click",
                Category::Operation,
                &tr!("tp-click"),
                &mut sub.kind,
                &[tr!("tp-next-frame"), tr!("tp-previous-frame")],
            );
            rows.choice_u8(
                ui,
                "timeline-drag",
                Category::Operation,
                &tr!("drag"),
                &mut sub.drag,
                &[tr!("tp-step-through-frames"), tr!("tp-nothing")],
            );
        }
        _ => object_properties(ui, rows, sub),
    }
}

/// Object: what a press on a raster layer's clear part does, and which
/// kinds of things it picks.
fn object_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.choice_u8(
        ui,
        "object-transparent",
        Category::Operation,
        &tr!("tp-transparent-part"),
        &mut sub.transparent,
        &[
            tr!("tp-transparent-moves-layer"),
            tr!("tp-transparent-picks-layer"),
            tr!("tp-nothing"),
        ],
    );
    let mut objects = sub.objects;
    rows.row(
        ui,
        "object-selectable",
        Category::Operation,
        |ui, theme, enabled| {
            ui.horizontal_top(|ui| {
                name_cell(ui, &tr!("tp-selectable"), name_width(theme), theme);
                ui.vertical(|ui| {
                    for (bit, name) in [
                        (PICKS_VECTOR, tr!("tp-picks-vector")),
                        (PICKS_RASTER, tr!("tp-picks-raster")),
                        (PICKS_COMIC, tr!("tp-picks-comic")),
                        (PICKS_3D, tr!("tp-picks-3d")),
                        (PICKS_CAMERA, tr!("tp-picks-camera")),
                    ] {
                        let on = objects & bit != 0;
                        let response =
                            sue_wm::ui::checkbox(("sue-paint-object-picks", bit), &name, on)
                                .enabled(enabled)
                                .show(ui, theme);
                        if response.clicked() {
                            objects ^= bit;
                        }
                    }
                });
            });
        },
    );
    sub.objects = objects;
    rows.hint(ui, &tr!("object-tool-hint"));
}

/// Move layer: what it moves (the layer, the layer's tone dots, the
/// grid), and how it picks the layers.
pub(super) fn move_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.choice_u8(
        ui,
        "move-target",
        Category::Operation,
        &tr!("tp-object-to-move"),
        &mut sub.move_target,
        &[
            tr!("tp-move-layer"),
            tr!("tp-move-tone"),
            tr!("tp-move-grid"),
        ],
    );
    if sub.move_target != crate::tools::MOVES_GRID {
        rows.check(
            ui,
            "move-clicked",
            Category::Operation,
            &tr!("tp-move-clicked"),
            Some(&tr!("tp-move-clicked-hint")),
            &mut sub.move_clicked,
        );
    }
    if sub.move_target == crate::tools::MOVES_LAYER {
        rows.check(
            ui,
            "move-in-selection",
            Category::Operation,
            &tr!("tp-move-in-selection"),
            Some(&tr!("tp-move-in-selection-hint")),
            &mut sub.move_in_selection,
        );
    }
    if sub.move_clicked && sub.move_target != crate::tools::MOVES_GRID {
        rows.check(
            ui,
            "move-select",
            Category::Operation,
            &tr!("tp-move-select"),
            None,
            &mut sub.move_select,
        );
    }
    let hint = match sub.move_target {
        crate::tools::MOVES_TONE => tr!("tp-move-tone-hint"),
        crate::tools::MOVES_GRID => tr!("tp-move-grid-hint"),
        _ => tr!("drag-move-current-layer"),
    };
    rows.hint(ui, &hint);
}

/// The Eyedropper: what it reads (what is shown or the layers), and for
/// the layers which ones; both leave out what the Exclude row says.
pub(super) fn eyedropper_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.choice_u8(
        ui,
        "eyedropper-reference",
        Category::Operation,
        &tr!("refer"),
        &mut sub.mode,
        &[tr!("obtain-display-color"), tr!("obtain-layer-color")],
    );
    if sub.mode == 0 {
        let mut exclude = sub.exclude;
        rows.row(
            ui,
            "eyedropper-exclude",
            Category::Reference,
            |ui, theme, enabled| {
                fill::exclude_row(ui, &mut exclude, true, enabled, theme);
            },
        );
        sub.exclude = exclude;
    } else {
        fill::refer_rows(ui, rows, sub);
    }
}
