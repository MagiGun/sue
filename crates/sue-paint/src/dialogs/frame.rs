//! New Frame Border Folder.

use egui::Ui;
use sue_wm::Theme;
use sue_wm::ui::{dropdown, form_grid, slider};

use super::{Outcome, bare, note, tip};

/// New Frame Border Folder: a layout, the border and the gaps.
pub(super) fn frame_ui(ui: &mut Ui, frame: &mut crate::comic::FrameSpec, theme: &Theme) -> Outcome {
    let layouts: Vec<String> = crate::manga::LAYOUTS
        .iter()
        .map(|(name, _)| crate::localize(name))
        .collect();
    let px = tr!("px");
    form_grid(ui, "sue-paint-frame", theme, |form| {
        let name = bare(tr!("layout"));
        form.row(&name, |ui| {
            let response = dropdown("sue-paint-frame-layout", &mut frame.layout, &layouts)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        for (salt, name, value, most) in [
            (
                "sue-paint-frame-border",
                bare(tr!("border-width")),
                &mut frame.border,
                40.0,
            ),
            (
                "sue-paint-frame-margin",
                bare(tr!("margin")),
                &mut frame.margin,
                400.0,
            ),
            (
                "sue-paint-frame-gap",
                bare(tr!("gap-across")),
                &mut frame.gap,
                200.0,
            ),
            (
                "sue-paint-frame-gap-down",
                bare(tr!("gap-down")),
                &mut frame.gap_y,
                200.0,
            ),
        ] {
            form.row(&name, |ui| {
                let mut points = f64::from(*value);
                let response = slider(salt, &mut points, 0.0..=most)
                    .name(&name)
                    .suffix(&px)
                    .show(ui, theme);
                if response.changed() {
                    *value = points as f32;
                }
                tip(response, &name, "", theme);
            });
        }
    });
    note(ui, &tr!("layers-under-frame-show-only-inside"), theme);
    Outcome::Open
}
