//! The settings of Sue Writer and of a page's furniture.

use egui::{Id, Ui, vec2};
use sue_core::Value;
use sue_model::manuscript;
use sue_wm::ui::{self, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{Furniture, Settings, SueWriter};

use super::toolbar::font_picker;

/// How wide a part of a header or a footer is, in the largest steps of
/// the spacing scale.
const PART_STEPS: f32 = 3.5;
/// Points in an inch: the margin is kept in points and shown in inches.
const POINTS_PER_INCH: f32 = 72.0;

impl SueWriter {
    /// Project Settings: the title and author, the look of the pages, the
    /// fonts and the targets.
    pub(super) fn settings_ui(&self, ui: &mut Ui, settings: &mut Settings, theme: &Theme) {
        ui::form_grid(ui, "writer-settings", theme, |form| {
            form.row(&tr!("title"), |ui| {
                let label = tr!("title");
                ui::text_field("writer-settings-title", &mut settings.title)
                    .label(&label)
                    .show(ui, theme)
            });
            form.row(&tr!("author"), |ui| {
                let label = tr!("author");
                ui::text_field("writer-settings-author", &mut settings.author)
                    .label(&label)
                    .show(ui, theme)
            });
            form.section(&tr!("page"));
            form.row(&tr!("styles"), |ui| {
                let keys: Vec<&str> = sue_type::SHEETS.iter().map(|(key, _)| *key).collect();
                let names: Vec<String> = keys.iter().map(|key| sue_type::sheet_name(key)).collect();
                pick(
                    ui,
                    "writer-sheet",
                    &tr!("styles"),
                    &keys,
                    &names,
                    &mut settings.sheet,
                    theme,
                );
            });
            form.row(&tr!("page"), |ui| {
                // Paper sizes (the shared list, less the slides').
                let pages: Vec<&str> = sue_model::PAGE_SIZES
                    .iter()
                    .map(|(name, _)| *name)
                    .filter(|name| !name.starts_with("Slide"))
                    .collect();
                pick(
                    ui,
                    "writer-page",
                    &tr!("page"),
                    &pages,
                    &pages,
                    &mut settings.page,
                    theme,
                );
            });
            form.row(&tr!("orientation"), |ui| {
                let options = [tr!("portrait"), tr!("landscape")];
                let picked = ui::segmented(
                    "writer-orientation",
                    &options,
                    Some(usize::from(settings.landscape)),
                )
                .show(ui, theme)
                .picked;
                if let Some(index) = picked {
                    settings.landscape = index == 1;
                }
            });
            form.row(&tr!("margins-inches"), |ui| {
                let mut inches = f64::from(settings.margin / POINTS_PER_INCH);
                let label = tr!("margins-inches");
                let response = ui::number_field("writer-margin", &mut inches)
                    .range(0.25..=3.0)
                    .step(0.05)
                    .decimals(2)
                    .label(&label)
                    .show(ui, theme);
                if response.changed() {
                    settings.margin = inches as f32 * POINTS_PER_INCH;
                }
            });
            form.section(&tr!("fonts"));
            form.row(&tr!("body-font"), |ui| {
                font_picker(
                    ui,
                    "writer-body-font",
                    &mut settings.font,
                    &mut settings.font_filter,
                );
            });
            form.row(&tr!("heading-font"), |ui| {
                let mut filter = String::new();
                font_picker(
                    ui,
                    "writer-heading-font",
                    &mut settings.heading_font,
                    &mut filter,
                );
            });
            form.row(&tr!("body-size-pt"), |ui| {
                let mut size = f64::from(settings.size);
                let label = tr!("body-size-pt");
                ui::number_field("writer-body-size", &mut size)
                    .range(6.0..=48.0)
                    .step(0.5)
                    .decimals(1)
                    .speed(0.25)
                    .label(&label)
                    .show(ui, theme);
                settings.size = size as f32;
            });
            if !sue_wm::fonts::ready() {
                form.wide(|ui| caption(ui, &tr!("still-finding-installed-fonts-ellipsis"), theme));
            }
            form.section(&tr!("targets"));
            form.row(&tr!("manuscript-target-words"), |ui| {
                words_field(
                    ui,
                    "writer-target",
                    &mut settings.target,
                    10_000_000,
                    100.0,
                    &tr!("manuscript-target-words"),
                    theme,
                );
            });
            form.row(&tr!("words-a-day"), |ui| {
                words_field(
                    ui,
                    "writer-session",
                    &mut settings.session,
                    100_000,
                    10.0,
                    &tr!("words-a-day"),
                    theme,
                );
            });
            form.wide(|ui| caption(ui, &tr!("zero-sets-no-target"), theme));
        });
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

    /// Keeps the project's settings, as one undo step.
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

/// A dropdown of `names` whose answer is kept as the matching one of
/// `keys` in `value`. A value that is none of them shows the first.
fn pick(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    keys: &[&str],
    names: &[impl AsRef<str>],
    value: &mut String,
    theme: &Theme,
) {
    let mut chosen = keys.iter().position(|key| *key == value).unwrap_or(0);
    if ui::dropdown(salt, &mut chosen, names)
        .label(label)
        .show(ui, theme)
        .changed()
        && let Some(key) = keys.get(chosen)
    {
        (*key).clone_into(value);
    }
}

/// A field of a number of words, from none to `most`, dragged `speed`
/// words a point.
fn words_field(
    ui: &mut Ui,
    salt: &str,
    value: &mut i64,
    most: i64,
    speed: f64,
    label: &str,
    theme: &Theme,
) {
    let mut words = *value as f64;
    ui::number_field(salt, &mut words)
        .range(0.0..=most as f64)
        .speed(speed)
        .label(label)
        .show(ui, theme);
    *value = (words.round() as i64).clamp(0, most);
}

/// A line of secondary text.
fn caption(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// The header and footer fields: three parts each, left, middle and right,
/// with buttons that put in a field where the typing is (the footer's
/// middle when nothing was typed in yet).
pub(super) fn furniture_ui(ui: &mut Ui, furniture: &mut Furniture, theme: &Theme) {
    let memory = Id::unique("writer-furniture-focus");
    let mut focused: Option<(usize, usize)> = ui.ctx().data(|data| data.get_temp(memory));
    let width = theme.space(Space::S9) * PART_STEPS;
    let parts = [tr!("left"), tr!("middle"), tr!("right")];
    let rows = [tr!("header"), tr!("footer")];
    // The labels' column, as wide as the wider of them.
    let label_width = rows
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.clone(), theme.text_font(TextRole::Body), theme.text)
                .size()
                .x
        })
        .fold(0.0, f32::max)
        .ceil();
    let gap = theme.space(Space::S4);
    let height = ui::Density::of(ui).control_height();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_exact_size(vec2(label_width, height), egui::Sense::hover());
        for part in &parts {
            ui.allocate_ui(vec2(width, height), |ui| {
                ui.set_width(width);
                caption(ui, part, theme);
            });
        }
    });
    let runnings = [&mut furniture.header, &mut furniture.footer];
    for (row, (label, running)) in rows.iter().zip(runnings).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let (rect, _) = ui.allocate_exact_size(vec2(label_width, height), egui::Sense::hover());
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                label,
                theme.text_font(TextRole::Body),
                theme.text_muted(),
            );
            for (column, part) in [&mut running.left, &mut running.center, &mut running.right]
                .into_iter()
                .enumerate()
            {
                let name = format!("{label}, {}", parts[column]);
                let response = ui::text_field(("writer-furniture", row, column), part)
                    .label(&name)
                    .width(width)
                    .show(ui, theme);
                if response.has_focus() || response.gained_focus() {
                    focused = Some((row, column));
                }
            }
        });
    }
    ui.add_space(theme.space(Space::S4));
    ui.horizontal_wrapped(|ui| {
        caption(ui, &tr!("insert"), theme);
        for (index, (field, name)) in sue_model::FIELDS.iter().enumerate() {
            let help = tr!("puts-the-field-where-you-type");
            let button = ui::button(("writer-furniture-field", index), name)
                .ghost()
                .show(ui, theme);
            if ui::tooltip(button, &Tip::new(name).help(&help), theme).clicked() {
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
    ui.add_space(theme.space(Space::S4));
    if ui::checkbox(
        "writer-different-first",
        &tr!("different-first-page-none-title-page"),
        furniture.different_first,
    )
    .show(ui, theme)
    .clicked()
    {
        furniture.different_first = !furniture.different_first;
    }
    ui.ctx().data_mut(|data| data.insert_temp(memory, focused));
}
