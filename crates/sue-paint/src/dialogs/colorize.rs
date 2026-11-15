//! Edit, Colorize: the line art to fill, the hint layer its colors come
//! from (the current layer) and how soft the result is.

use egui::Ui;
use sue_core::EntityId;
use sue_wm::Theme;
use sue_wm::ui::{self, TextRole, Tip};

use super::Outcome;

/// Colorize's settings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColorizeDialog {
    /// The hint layer: the current layer, whose strokes give the colors.
    pub hints: EntityId,
    /// The layers that can be the line art: each one's id and name.
    pub choices: Vec<(EntityId, String)>,
    /// The line art chosen, an index into `choices`.
    pub lines: usize,
    /// Blur strength, pixels.
    pub blur: f32,
}

impl ColorizeDialog {
    /// The line art layer chosen.
    pub(crate) fn line_layer(&self) -> Option<EntityId> {
        self.choices.get(self.lines).map(|(id, _)| *id)
    }
}

/// Colorize's body.
pub(super) fn colorize_ui(ui: &mut Ui, dialog: &mut ColorizeDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let names: Vec<&str> = dialog
        .choices
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    let label = tr!("colorize-line-art");
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&label)
                .font(theme.text_font(TextRole::Body))
                .color(theme.text),
        );
        let response = ui::dropdown("sue-paint-colorize-lines", &mut dialog.lines, &names)
            .label(&label)
            .show(ui, theme);
        ui::tooltip(response, &Tip::new(&label), theme);
    });
    let mut blur = f64::from(dialog.blur);
    let name = tr!("colorize-blur-strength");
    let response = ui::slider("sue-paint-colorize-blur", &mut blur, 0.0..=20.0)
        .label(&name)
        .decimals(1)
        .show(ui, theme);
    if response.changed() {
        dialog.blur = blur as f32;
    }
    ui::tooltip(response, &Tip::new(&name), theme);
    ui.label(
        egui::RichText::new(tr!("colorize-hint-note"))
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
        let cancel = tr!("cancel");
        let response = ui::button("sue-paint-colorize-cancel", &cancel).show(ui, theme);
        if response.clicked() {
            outcome = Outcome::Cancel;
        }
        ui::tooltip(response, &Tip::new(&cancel).shortcut("Esc"), theme);
        let ok = tr!("ok");
        let response = ui::button("sue-paint-colorize-ok", &ok)
            .primary()
            .enabled(dialog.line_layer().is_some())
            .show(ui, theme);
        if response.clicked() {
            outcome = Outcome::Ok;
        }
        ui::tooltip(response, &Tip::new(&ok).shortcut("Enter"), theme);
    });
    outcome
}
