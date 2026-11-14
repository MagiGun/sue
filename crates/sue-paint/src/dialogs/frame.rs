//! New Frame Border Folder.

use egui::{Align, Layout, RichText, Ui, vec2};
use sue_wm::Theme;

use super::{Outcome, label};

/// New Frame Border Folder: a layout, the border and the gaps.
pub(super) fn frame_ui(ui: &mut Ui, frame: &mut crate::comic::FrameSpec, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-frame")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("layout"), theme);
            egui::ComboBox::from_id_salt("sue-paint-frame-layout")
                .selected_text(crate::localize(crate::manga::LAYOUTS[frame.layout].0))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (index, (name, _)) in crate::manga::LAYOUTS.iter().enumerate() {
                        ui.selectable_value(&mut frame.layout, index, crate::localize(name));
                    }
                });
            ui.end_row();
            label(ui, &tr!("border-width"), theme);
            ui.add(egui::Slider::new(&mut frame.border, 0.0..=40.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("margin"), theme);
            ui.add(egui::Slider::new(&mut frame.margin, 0.0..=400.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("gap-across"), theme);
            ui.add(egui::Slider::new(&mut frame.gap, 0.0..=200.0).suffix(tr!("px")));
            ui.end_row();
            label(ui, &tr!("gap-down"), theme);
            ui.add(egui::Slider::new(&mut frame.gap_y, 0.0..=200.0).suffix(tr!("px")));
            ui.end_row();
        });
    ui.label(
        RichText::new(tr!("layers-under-frame-show-only-inside"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("OK").clicked() {
            outcome = Outcome::Ok;
        }
        if ui.button(tr!("cancel")).clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}
