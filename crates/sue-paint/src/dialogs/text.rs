//! The Text tool's dialog.

use egui::{RichText, Ui, vec2};
use sue_wm::Theme;

use super::{Outcome, label, ok_cancel};

/// The Text tool's words, font, size, ink and placement.
pub(super) fn text_ui(
    ui: &mut Ui,
    lettering: &mut crate::lettering::Lettering,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    let edit = ui.add(
        egui::TextEdit::multiline(&mut lettering.content)
            .desired_rows(5)
            .desired_width(f32::INFINITY)
            .hint_text(tr!("type-here")),
    );
    // The words take the keys while nothing else does.
    if ui.ctx().memory(|memory| memory.focused().is_none()) {
        edit.request_focus();
    }
    egui::Grid::new("sue-paint-text")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("font"), theme);
            let shown = if lettering.font.is_empty() {
                tr!("default-font-ubuntu-light")
            } else {
                lettering.font.clone()
            };
            egui::ComboBox::from_id_salt("sue-paint-text-font")
                .selected_text(shown)
                .width(240.0)
                .height(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut lettering.font,
                        String::new(),
                        tr!("default-font-ubuntu-light"),
                    );
                    let selected = lettering.font.clone();
                    if let Some(family) = sue_wm::fonts::list_ui(ui, &selected, "") {
                        lettering.font = family;
                    }
                });
            ui.end_row();
            label(ui, &tr!("size"), theme);
            ui.add(
                egui::Slider::new(&mut lettering.size, 6.0..=400.0)
                    .logarithmic(true)
                    .suffix(tr!("px")),
            );
            ui.end_row();
            label(ui, &tr!("color"), theme);
            ui.color_edit_button_srgb(&mut lettering.rgb);
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.checkbox(&mut lettering.center, tr!("centered"));
        ui.checkbox(
            &mut lettering.vertical,
            tr!("vertical-columns-right-to-left"),
        );
    });
    ui.label(
        RichText::new(tr!("enter-starts-new-line-ctrl-enter"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    let commit = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command
            )
        })
    });
    if commit {
        outcome = Outcome::Ok;
    }
    outcome
}
