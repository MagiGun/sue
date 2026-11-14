//! The settings of Sue Writer and of a page's furniture.

use egui::{Id, RichText, Ui, vec2};
use sue_core::Value;
use sue_model::manuscript;
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{Furniture, Settings, SueWriter};

use super::toolbar::font_picker;

impl SueWriter {
    pub(super) fn settings_ui(&self, ui: &mut Ui, settings: &mut Settings, theme: &Theme) {
        egui::Grid::new("writer-settings")
            .num_columns(2)
            .spacing(vec2(12.0, 6.0))
            .show(ui, |ui| {
                ui.label(tr!("title"));
                ui.text_edit_singleline(&mut settings.title);
                ui.end_row();
                ui.label(tr!("author"));
                ui.text_edit_singleline(&mut settings.author);
                ui.end_row();
                ui.label(tr!("styles"));
                egui::ComboBox::from_id_salt("writer-sheet")
                    .selected_text(sue_type::sheet_name(&settings.sheet))
                    .show_ui(ui, |ui| {
                        for (key, _) in sue_type::SHEETS {
                            ui.selectable_value(
                                &mut settings.sheet,
                                (*key).to_owned(),
                                sue_type::sheet_name(key),
                            );
                        }
                    });
                ui.end_row();
                ui.label(tr!("page"));
                egui::ComboBox::from_id_salt("writer-page")
                    .selected_text(settings.page.clone())
                    .show_ui(ui, |ui| {
                        // Paper sizes (the shared list, less the slides').
                        for (page, _) in sue_model::PAGE_SIZES
                            .iter()
                            .filter(|(name, _)| !name.starts_with("Slide"))
                        {
                            ui.selectable_value(&mut settings.page, (*page).to_owned(), *page);
                        }
                    });
                ui.end_row();
                ui.label(tr!("orientation"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut settings.landscape, false, tr!("portrait"));
                    ui.radio_value(&mut settings.landscape, true, tr!("landscape"));
                });
                ui.end_row();
                ui.label(tr!("margins-inches"));
                let mut inches = settings.margin / 72.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut inches)
                            .range(0.25..=3.0)
                            .speed(0.05)
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    settings.margin = inches * 72.0;
                }
                ui.end_row();
                ui.label(tr!("body-font"));
                font_picker(
                    ui,
                    "writer-body-font",
                    &mut settings.font,
                    &mut settings.font_filter,
                );
                ui.end_row();
                ui.label(tr!("heading-font"));
                let mut filter = String::new();
                font_picker(
                    ui,
                    "writer-heading-font",
                    &mut settings.heading_font,
                    &mut filter,
                );
                ui.end_row();
                ui.label(tr!("body-size-pt"));
                ui.add(
                    egui::DragValue::new(&mut settings.size)
                        .range(6.0..=48.0)
                        .speed(0.25),
                );
                ui.end_row();
                ui.label(tr!("manuscript-target-words"));
                ui.add(
                    egui::DragValue::new(&mut settings.target)
                        .range(0..=10_000_000)
                        .speed(100),
                );
                ui.end_row();
                ui.label(tr!("words-a-day"));
                ui.add(
                    egui::DragValue::new(&mut settings.session)
                        .range(0..=100_000)
                        .speed(10),
                );
                ui.end_row();
            });
        if !sue_wm::fonts::ready() {
            ui.label(
                RichText::new(tr!("still-finding-installed-fonts-ellipsis"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
        }
    }

    /// Keeps the header and footer on the project, as one undo step.
    pub(super) fn apply_furniture(&mut self, cx: &mut AppletContext<'_>, furniture: &Furniture) {
        let Some(root) = self.project else {
            return;
        };
        let batch = sue_core::txn::Batch::new(tr!("header-and-footer"))
            .then(project::set(
                root,
                project::HEADER,
                Some(Value::Text(furniture.header.store())),
            ))
            .then(project::set(
                root,
                project::FOOTER,
                Some(Value::Text(furniture.footer.store())),
            ))
            .then(project::set(
                root,
                project::DIFFERENT_FIRST,
                Some(Value::Bool(furniture.different_first)),
            ));
        self.apply(cx, batch);
    }

    pub(super) fn apply_settings(&mut self, cx: &mut AppletContext<'_>, settings: &Settings) {
        let Some(root) = self.project else {
            return;
        };
        let mut batch = sue_core::txn::Batch::new(tr!("project-settings"));
        let text = |value: &str| (!value.is_empty()).then(|| Value::Text(value.to_owned()));
        batch.push(Box::new(project::set(
            root,
            manuscript::TITLE,
            text(&settings.title),
        )));
        batch.push(Box::new(project::set(
            root,
            manuscript::AUTHOR,
            text(&settings.author),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SHEET,
            text(&settings.sheet),
        )));
        batch.push(Box::new(project::set(
            root,
            project::PAGE,
            text(&settings.page),
        )));
        batch.push(Box::new(project::set(
            root,
            project::LANDSCAPE,
            settings.landscape.then_some(Value::Bool(true)),
        )));
        batch.push(Box::new(project::set(
            root,
            project::MARGIN,
            Some(Value::Float(f64::from(settings.margin))),
        )));
        batch.push(Box::new(project::set(
            root,
            project::FONT,
            text(&settings.font),
        )));
        batch.push(Box::new(project::set(
            root,
            project::HEADING_FONT,
            text(&settings.heading_font),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SIZE,
            Some(Value::Float(f64::from(settings.size))),
        )));
        batch.push(Box::new(project::set(
            root,
            manuscript::TARGET,
            (settings.target > 0).then_some(Value::Int(settings.target)),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SESSION_TARGET,
            (settings.session > 0).then_some(Value::Int(settings.session)),
        )));
        if !settings.title.trim().is_empty() {
            batch.push(Box::new(project::rename(root, settings.title.trim())));
        }
        self.apply(cx, batch);
    }
}

/// A combo box of installed fonts with a search field.
/// The header and footer fields: three parts each, left, middle and right,
/// with buttons that put in a field.
pub(super) fn furniture_ui(ui: &mut Ui, furniture: &mut Furniture, theme: &Theme) {
    let mut focused: Option<(usize, usize)> = ui
        .ctx()
        .data(|data| data.get_temp(Id::unique("writer-furniture-focus")));
    egui::Grid::new("writer-furniture")
        .num_columns(4)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("");
            for part in [tr!("left"), tr!("middle"), tr!("right")] {
                ui.label(
                    RichText::new(part)
                        .size(theme.small_size)
                        .color(theme.muted),
                );
            }
            ui.end_row();
            for (row, (label, running)) in [
                (tr!("header"), &mut furniture.header),
                (tr!("footer"), &mut furniture.footer),
            ]
            .into_iter()
            .enumerate()
            {
                ui.label(label);
                for (column, part) in [&mut running.left, &mut running.center, &mut running.right]
                    .into_iter()
                    .enumerate()
                {
                    let response = ui.add(egui::TextEdit::singleline(part).desired_width(130.0));
                    if response.has_focus() || response.gained_focus() {
                        focused = Some((row, column));
                    }
                }
                ui.end_row();
            }
        });
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(tr!("insert"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        for (field, name) in sue_model::FIELDS {
            if ui.small_button(*name).clicked() {
                let (row, column) = focused.unwrap_or((1, 1));
                let running = if row == 0 {
                    &mut furniture.header
                } else {
                    &mut furniture.footer
                };
                let part = match column {
                    0 => &mut running.left,
                    1 => &mut running.center,
                    _ => &mut running.right,
                };
                part.push_str(&format!("{{{field}}}"));
            }
        }
    });
    ui.checkbox(
        &mut furniture.different_first,
        tr!("different-first-page-none-title-page"),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(Id::unique("writer-furniture-focus"), focused));
}
