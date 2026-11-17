//! Tool Property for the comic tools: Frame Border and Balloon.

use egui::Ui;

use super::{Category, Rows};
use crate::tools::{BALLOON_PEN, ColorPick, FRAME_PEN, POLYLINE_FRAME, SubTool, THOUGHT_TAIL};

/// The Frame Border tool: how a panel is drawn or divided.
pub(super) fn frame_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    if sub.mode == 1 {
        // Frame Border Division: the gutters between the panels it cuts.
        rows.slider(
            ui,
            "frame-gutter-across",
            Category::Operation,
            &tr!("vertical-gutter"),
            &mut sub.gutter[0],
            0.0..=200.0,
            &tr!("px"),
            false,
        );
        rows.slider(
            ui,
            "frame-gutter-down",
            Category::Operation,
            &tr!("horizontal-gutter"),
            &mut sub.gutter[1],
            0.0..=200.0,
            &tr!("px"),
            false,
        );
        rows.hint(ui, &tr!("drag-line-across-panel-divide"));
        return;
    }
    let mut add = u8::from(sub.new_layer);
    if rows.choice_u8(
        ui,
        "frame-add",
        Category::Operation,
        &tr!("how-to-add"),
        &mut add,
        &[tr!("add-to-frame-folder"), tr!("create-new-frame-folder")],
    ) {
        sub.new_layer = add == 1;
    }
    rows.check(
        ui,
        "frame-draw-border",
        Category::Operation,
        &tr!("draw-border"),
        Some(&tr!("draw-border-hint")),
        &mut sub.edge,
    );
    if sub.edge {
        rows.slider(
            ui,
            "frame-border-width",
            Category::BrushSize,
            &tr!("brush-size"),
            &mut sub.tip.size,
            1.0..=50.0,
            &tr!("px"),
            false,
        );
    }
    if sub.mode == 0 {
        super::select::aspect_rows(ui, rows, &mut sub.aspect);
    }
    rows.hint(
        ui,
        &match sub.mode {
            POLYLINE_FRAME => tr!("click-corners-frame"),
            FRAME_PEN => tr!("draw-round-frame"),
            _ => tr!("drag-draw-rectangle-panel"),
        },
    );
}

/// The Balloon tool: its edge, line and fill colors, how a balloon is
/// added, and how a balloon or its tail is drawn.
pub(super) fn balloon_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let tail = matches!(sub.mode, 1 | THOUGHT_TAIL);
    if !tail {
        let mut add = u8::from(sub.new_layer);
        if rows.choice_u8(
            ui,
            "balloon-how-to-add",
            Category::Operation,
            &tr!("how-to-add"),
            &mut add,
            &[tr!("add-to-selected-layer"), tr!("create-new-layer-2")],
        ) {
            sub.new_layer = add == 1;
        }
        rows.check(
            ui,
            "balloon-edge",
            Category::Ink,
            &tr!("balloon-edge"),
            Some(&tr!("balloon-edge-hint")),
            &mut sub.edge,
        );
    }
    if sub.edge || tail {
        rows.slider(
            ui,
            "balloon-line-width",
            Category::BrushSize,
            &tr!("line-width-2"),
            &mut sub.tip.size,
            1.0..=20.0,
            &tr!("px"),
            false,
        );
    }
    if !tail {
        if sub.edge {
            color_pick_row(
                ui,
                rows,
                "balloon-line-color",
                &tr!("line-color"),
                &mut sub.line_color,
            );
        }
        color_pick_row(
            ui,
            rows,
            "balloon-fill-color",
            &tr!("fill-color"),
            &mut sub.fill_color,
        );
    }
    rows.hint(
        ui,
        &match sub.mode {
            1 | THOUGHT_TAIL => tr!("drag-inside-balloon-where-tail-points"),
            BALLOON_PEN => tr!("draw-round-balloon"),
            _ => tr!("drag-to-draw-a-balloon"),
        },
    );
}

/// A color that is the main, the sub or one of the sub tool's own, with
/// a swatch to pick its own.
fn color_pick_row(ui: &mut Ui, rows: &mut Rows<'_>, id: &str, label: &str, pick: &mut ColorPick) {
    let names = super::names(&ColorPick::NAMES);
    let mut chosen = pick.index();
    if rows.choice(ui, id, Category::Ink, label, &mut chosen, &names) {
        let own = match *pick {
            ColorPick::Own(rgb) => rgb,
            ColorPick::Drawing => [0, 0, 0],
            ColorPick::Other => [255, 255, 255],
        };
        *pick = ColorPick::at(chosen, own);
    }
    if let ColorPick::Own(mut rgb) = *pick {
        rows.row(
            ui,
            &format!("{id}-own"),
            Category::Ink,
            |ui, theme, enabled| {
                ui.horizontal(|ui| {
                    super::name_cell(ui, &tr!("own-color"), super::name_width(theme), theme);
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.color_edit_button_srgb(&mut rgb)
                            .on_hover_text(tr!("own-color"));
                    });
                });
            },
        );
        *pick = ColorPick::Own(rgb);
    }
}
