//! Tool Property for the Ruler tool.

use egui::Ui;
use sue_wm::Theme;

use super::small_text;
use crate::panels::slider_row;
use crate::tools::SubTool;

/// The Ruler tool's sub tools, and a command they ask for: Create
/// Perspective Ruler or Delete Ruler.
pub(super) fn ruler_properties(
    ui: &mut Ui,
    sub: &mut SubTool,
    theme: &Theme,
) -> Option<&'static str> {
    let small = small_text(theme);
    let mut command = None;
    match sub.mode {
        6 => {
            ui.label(small(&tr!("click-place-points-click-last-one")));
            ui.label(small(&tr!("ctrl-1-snap-ruler-turns-rulers")));
        }
        7 => {
            ui.horizontal(|ui| {
                ui.label(small(&tr!("figure")));
                ui.selectable_value(&mut sub.kind, 0, tr!("rectangle"));
                ui.selectable_value(&mut sub.kind, 1, tr!("ellipse"));
            });
            ui.label(small(&tr!("drag-lay-ruler-strokes-start-follow")));
        }
        8 => {
            ui.label(small(&tr!("draw-ruler-freehand-strokes-start-follow")));
        }
        5 => {
            ui.label(small(&tr!("drag-lay-straight-ruler-strokes-start")));
            ui.label(small(&tr!("ctrl-1-snap-ruler-turns-off")));
        }
        2 => {
            ui.horizontal(|ui| {
                ui.label(small(&tr!("special-ruler")));
                egui::ComboBox::from_id_salt("sue-paint-special-ruler")
                    .selected_text(crate::localize(
                        crate::rulers::Special::NAMES[usize::from(sub.kind.min(2))],
                    ))
                    .show_ui(ui, |ui| {
                        for (kind, name) in crate::rulers::Special::NAMES.iter().enumerate() {
                            ui.selectable_value(&mut sub.kind, kind as u8, crate::localize(name));
                        }
                    });
            });
            ui.label(small(&match sub.kind {
                0 => tr!("drag-set-lines-direction-every-stroke"),
                1 => tr!("click-middle-every-stroke-runs-toward"),
                _ => tr!("click-middle-every-stroke-goes-round"),
            }));
            ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
        }
        1 => {
            ui.horizontal(|ui| {
                ui.label(small(&tr!("process")));
                egui::ComboBox::from_id_salt("sue-paint-perspective-process")
                    .selected_text(if sub.kind == 1 {
                        tr!("delete-vanishing-point-2")
                    } else {
                        tr!("add-vanishing-point-2")
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut sub.kind, 0, tr!("add-vanishing-point-2"));
                        ui.selectable_value(&mut sub.kind, 1, tr!("delete-vanishing-point-2"));
                    });
            });
            ui.checkbox(&mut sub.fix_eye_level, small(&tr!("fix-eye-level")))
                .on_hover_text(tr!("vanishing-points-slide-along-eye-level"));
            ui.label(small(&if sub.kind == 1 {
                tr!("click-vanishing-point-take-away")
            } else {
                tr!("drag-along-two-edges-meet-distance")
            }));
            if ui
                .button(tr!("create-perspective-ruler-ellipsis"))
                .clicked()
            {
                command = Some("layer.ruler.perspective");
            }
        }
        _ => {
            ui.label(small(&tr!("drag-canvas-lay-ruler-strokes-draw")));
            let mut lines = sub.lines as f32;
            if slider_row(
                ui,
                &tr!("number-of-lines"),
                &mut lines,
                2.0..=32.0,
                "",
                false,
                theme,
            ) {
                sub.lines = lines.round() as u32;
            }
            ui.checkbox(&mut sub.mirror, small(&tr!("line-symmetry")))
                .on_hover_text(tr!("every-other-line-mirrored-as-clip"));
            ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
        }
    }
    if ui.button(tr!("delete-ruler")).clicked() {
        command = Some("layer.ruler.delete");
    }
    command
}
