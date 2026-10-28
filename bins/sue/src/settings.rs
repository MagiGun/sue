//! Settings: a floating window over the canvas (gear on the left rail, or
//! `Ctrl+,`). A category sidebar on the left; on the right only the chosen
//! category's controls, under a live filter. Pages are registered data from
//! the engine's catalog; a rice section no page claims lands on a final
//! "Other" page, so every parameter stays reachable (LAW 3). Every edit is
//! an undoable rice transaction; a slider previews live while dragged and
//! records one undo step when released.

use std::collections::HashMap;

use egui::{Align, Color32, Layout, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use sue_rice::{Keybind, RiceEntry, RiceParameter, RiceSchema, RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, Catalog, RiceEdit, SettingsGroup, SettingsItem,
    SettingsPage, ShellRequest, SueApplet, Theme, widgets,
};

/// The applet id.
pub const ID: &str = "settings";

/// `high-refresh` reads `High refresh`; numbers stay as they are.
fn choice_label(value: &str) -> String {
    let words = value.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The registered pages plus "Other" for unclaimed sections.
pub fn all_pages(catalog: &Catalog, rice: &RiceSchema) -> Vec<SettingsPage> {
    let mut pages = catalog.pages.clone();
    let claimed = |path: &str, section: &str| {
        pages
            .iter()
            .flat_map(SettingsPage::claims)
            .any(|item| match item {
                SettingsItem::Param(param) => param == path,
                SettingsItem::Section(name) => name == section,
                _ => false,
            })
    };
    let mut other: Vec<String> = Vec::new();
    for entry in rice.entries() {
        if !claimed(entry.path(), entry.section())
            && !other.iter().any(|seen| seen == entry.section())
        {
            other.push(entry.section().to_owned());
        }
    }
    if !other.is_empty() {
        pages.push(SettingsPage::new(
            "other",
            "Other",
            other
                .into_iter()
                .map(|section| {
                    let title = choice_label(&section);
                    SettingsGroup::new(title, vec![SettingsItem::Section(section)])
                })
                .collect(),
        ));
    }
    pages
}

/// The rice entries an item shows.
fn entries_of<'r>(rice: &'r RiceSchema, item: &SettingsItem) -> Vec<&'r RiceEntry> {
    match item {
        SettingsItem::Param(path) => rice.entry(path).into_iter().collect(),
        SettingsItem::Section(section) => rice
            .entries()
            .iter()
            .filter(|entry| entry.section() == section)
            .collect(),
        _ => Vec::new(),
    }
}

/// `true` if `entry` matches the lower-cased `needle`.
fn matches(entry: &RiceEntry, needle: &str) -> bool {
    needle.is_empty()
        || entry.label().to_lowercase().contains(needle)
        || entry.path().to_lowercase().contains(needle)
        || entry
            .description()
            .is_some_and(|text| text.to_lowercase().contains(needle))
}

/// The value typed into a field, typed like the parameter it edits.
///
/// # Errors
/// A message when the text does not fit the parameter.
pub fn typed_value(rice: &RiceSchema, path: &str, text: &str) -> Result<RiceValue, String> {
    match rice.parameter(path) {
        Some(RiceParameter::IntRange { .. }) => text
            .trim()
            .parse()
            .map(RiceValue::Int)
            .map_err(|_| format!("{text} is not a whole number")),
        Some(RiceParameter::FloatRange { .. }) => text
            .trim()
            .parse()
            .map(RiceValue::Float)
            .map_err(|_| format!("{text} is not a number")),
        Some(RiceParameter::ColorHex { .. }) => text
            .trim()
            .parse()
            .map(RiceValue::Color)
            .map_err(|_| format!("{text} is not a color like #808080")),
        _ => Ok(RiceValue::Text(text.to_owned())),
    }
}

/// A monochrome on and off switch.
fn toggle(ui: &mut Ui, on: bool, theme: &Theme) -> bool {
    let size = vec2(theme.ui_size * 2.6, theme.ui_size * 1.4);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let radius = rect.height() / 2.0;
    let track = if on { theme.text } else { theme.hover };
    ui.painter().rect_filled(rect, radius, track);
    ui.painter()
        .rect_stroke(rect, radius, theme.hairline(), egui::StrokeKind::Inside);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    let knob = if on { theme.void } else { theme.muted };
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius * 0.72, knob);
    response.clicked()
}

/// What one edit asks for.
enum Edit {
    Set(String, RiceValue),
    Preview(String, RiceValue),
    Release(String),
}

/// The Settings window.
#[derive(Debug, Default)]
pub struct Settings {
    page: usize,
    pending: Option<String>,
    filter: String,
    drag: Option<(String, RiceValue)>,
    recording: Option<String>,
    texts: HashMap<String, String>,
    error: Option<String>,
}

impl Settings {
    fn commit(&mut self, cx: &mut AppletContext<'_>, path: &str, value: RiceValue) {
        if cx.rice().get(path).as_ref() == Some(&value) {
            return;
        }
        match cx.edit_rice(SetRiceValue::new(path, value)) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn apply(&mut self, cx: &mut AppletContext<'_>, edit: Edit) {
        match edit {
            Edit::Set(path, value) => self.commit(cx, &path, value),
            Edit::Preview(path, value) => {
                if self
                    .drag
                    .as_ref()
                    .is_none_or(|(dragging, _)| dragging != &path)
                    && let Some(original) = cx.rice().get(&path)
                {
                    self.drag = Some((path.clone(), original));
                }
                let _ = cx.world.rice.set(&path, value);
            }
            Edit::Release(path) => {
                if let Some((dragging, original)) = self.drag.take()
                    && dragging == path
                    && let Some(value) = cx.rice().get(&path)
                {
                    let _ = cx.world.rice.set(&path, original);
                    self.commit(cx, &path, value);
                }
            }
        }
    }

    fn control(&mut self, ui: &mut Ui, entry: &RiceEntry, cx: &AppletContext<'_>) -> Option<Edit> {
        let theme = cx.theme;
        let path = entry.path().to_owned();
        if let Some(choices) = cx.catalog.choices.get(entry.path()) {
            let current = cx.rice().get_str(entry.path()).unwrap_or("").to_owned();
            let selected = choices.iter().position(|choice| choice.value == current);
            let text =
                selected.map_or_else(|| current.clone(), |index| choices[index].label.clone());
            let mut picked = None;
            egui::ComboBox::from_id_salt(("sue-choice", entry.path()))
                .selected_text(text)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for choice in choices {
                        let on = choice.value == current;
                        if ui
                            .add_enabled(
                                choice.enabled,
                                egui::Button::selectable(on, &choice.label),
                            )
                            .clicked()
                        {
                            picked = Some(choice.value.clone());
                        }
                    }
                });
            return picked.map(|value| Edit::Set(path, RiceValue::Text(value)));
        }
        match entry.parameter() {
            RiceParameter::Bool { value } => {
                toggle(ui, *value, theme).then(|| Edit::Set(path, RiceValue::Bool(!value)))
            }
            RiceParameter::IntRange { value, min, max } => {
                let (min, max) = (min.unwrap_or(0), max.unwrap_or(100));
                let mut number = *value;
                ui.label(
                    RichText::new(number.to_string())
                        .font(theme.mono(theme.ui_size))
                        .color(theme.text),
                );
                let response = ui.add(egui::Slider::new(&mut number, min..=max).show_value(false));
                slider_edit(&response, path, RiceValue::Int(number), number != *value)
            }
            RiceParameter::FloatRange { value, min, max } => {
                let (min, max) = (min.unwrap_or(0.0), max.unwrap_or(1.0));
                let mut number = *value;
                ui.label(
                    RichText::new(format!("{number:.2}"))
                        .font(theme.mono(theme.ui_size))
                        .color(theme.text),
                );
                let response = ui.add(egui::Slider::new(&mut number, min..=max).show_value(false));
                let rounded = (number * 100.0).round() / 100.0;
                slider_edit(
                    &response,
                    path,
                    RiceValue::Float(rounded),
                    (rounded - value).abs() > f64::EPSILON,
                )
            }
            RiceParameter::ColorHex { value } => {
                let text = self
                    .texts
                    .entry(path.clone())
                    .or_insert_with(|| value.to_string());
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .font(theme.mono(theme.ui_size))
                        .desired_width(84.0),
                );
                let (swatch, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                ui.painter().rect_filled(
                    swatch,
                    4.0,
                    Color32::from_rgba_unmultiplied(value.r, value.g, value.b, value.a),
                );
                ui.painter()
                    .rect_stroke(swatch, 4.0, theme.hairline(), egui::StrokeKind::Inside);
                if response.lost_focus() {
                    let typed = self.texts.remove(&path).unwrap_or_default();
                    return Some(match typed_value(cx.rice(), &path, &typed) {
                        Ok(value) => Edit::Set(path, value),
                        Err(error) => {
                            self.error = Some(error);
                            return None;
                        }
                    });
                }
                if !response.has_focus() {
                    self.texts.insert(path, value.to_string());
                }
                None
            }
            RiceParameter::EnumChoice { value, options } => {
                let labels: Vec<String> =
                    options.iter().map(|option| choice_label(option)).collect();
                let selected = options.iter().position(|option| option == value);
                let short = labels.len() <= 5 && labels.iter().map(String::len).sum::<usize>() < 48;
                if short {
                    widgets::segmented(ui, &labels, selected, theme)
                        .map(|index| Edit::Set(path, RiceValue::Text(options[index].clone())))
                } else {
                    let mut picked = None;
                    egui::ComboBox::from_id_salt(("sue-enum", entry.path()))
                        .selected_text(
                            selected.map_or_else(String::new, |index| labels[index].clone()),
                        )
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for (index, label) in labels.iter().enumerate() {
                                if ui
                                    .add(egui::Button::selectable(selected == Some(index), label))
                                    .clicked()
                                {
                                    picked = Some(options[index].clone());
                                }
                            }
                        });
                    picked.map(|value| Edit::Set(path, RiceValue::Text(value)))
                }
            }
            RiceParameter::Keybind { value } => {
                let recording = self.recording.as_deref() == Some(entry.path());
                let text = if recording {
                    "Press keys...".to_owned()
                } else {
                    value.to_string()
                };
                let button = egui::Button::new(
                    RichText::new(text)
                        .font(theme.mono(theme.ui_size))
                        .color(theme.text),
                )
                .fill(if recording { theme.active } else { theme.pill })
                .stroke(if recording {
                    theme.hairline_active()
                } else {
                    theme.hairline()
                });
                if ui
                    .add(button)
                    .on_hover_text("Click, then press the new shortcut")
                    .clicked()
                {
                    self.recording = Some(path.clone());
                }
                if recording {
                    return self.record_chord(ui, path);
                }
                None
            }
            RiceParameter::Text { value } | RiceParameter::ShaderPath { value } => {
                let text = self
                    .texts
                    .entry(path.clone())
                    .or_insert_with(|| value.clone());
                let response = ui.add(egui::TextEdit::singleline(text).desired_width(240.0));
                if response.lost_focus() {
                    let typed = self.texts.remove(&path).unwrap_or_default();
                    return (typed != *value).then_some(Edit::Set(path, RiceValue::Text(typed)));
                }
                if !response.has_focus() {
                    self.texts.insert(path, value.clone());
                }
                None
            }
        }
    }

    /// The next key press with its modifiers, as a chord.
    fn record_chord(&mut self, ui: &Ui, path: String) -> Option<Edit> {
        let pressed = ui.input_mut(|input| {
            let mut found = None;
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if found.is_none() => {
                    found = Some((*key, *modifiers));
                    false
                }
                _ => true,
            });
            found
        });
        let (key, modifiers) = pressed?;
        self.recording = None;
        if key == egui::Key::Escape && modifiers.is_none() {
            return None;
        }
        let mut chord = String::new();
        for (on, name) in [
            (modifiers.command, "Ctrl+"),
            (modifiers.alt, "Alt+"),
            (modifiers.shift, "Shift+"),
        ] {
            if on {
                chord.push_str(name);
            }
        }
        chord.push_str(key.symbol_or_name());
        match chord.parse::<Keybind>() {
            Ok(keybind) => Some(Edit::Set(path, RiceValue::Keybind(keybind))),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn param_row(&mut self, ui: &mut Ui, entry: &RiceEntry, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let width = ui.available_width();
        let mut edit = None;
        ui.horizontal(|ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.set_width((width * 0.5).max(160.0));
                ui.label(
                    RichText::new(entry.label())
                        .size(theme.ui_size)
                        .color(theme.text),
                );
                if let Some(description) = entry.description() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(description)
                                .size(theme.small_size)
                                .color(theme.muted),
                        )
                        .wrap(),
                    );
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                edit = self.control(ui, entry, cx);
            });
        });
        ui.add_space(4.0);
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(
            line.x_range(),
            line.top(),
            Stroke::new(theme.border_width, theme.border),
        );
        ui.add_space(6.0);
        if let Some(edit) = edit {
            self.apply(cx, edit);
        }
    }

    fn item(&mut self, ui: &mut Ui, item: &SettingsItem, needle: &str, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        match item {
            SettingsItem::Param(_) | SettingsItem::Section(_) => {
                let entries: Vec<RiceEntry> = entries_of(cx.rice(), item)
                    .into_iter()
                    .filter(|entry| matches(entry, needle))
                    .cloned()
                    .collect();
                for entry in entries {
                    self.param_row(ui, &entry, cx);
                }
            }
            SettingsItem::Fact(id) if needle.is_empty() => {
                if let Some((label, value)) = cx.catalog.facts.get(id).cloned() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(label).color(theme.muted));
                        ui.label(RichText::new(value).color(theme.text));
                    });
                    ui.add_space(6.0);
                }
            }
            SettingsItem::Table(id) if needle.is_empty() => {
                if let Some(table) = cx.catalog.tables.get(id).cloned() {
                    if table.rows.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(&table.empty).color(theme.muted)).wrap(),
                        );
                    } else {
                        egui::Grid::new(("sue-table", id.as_str()))
                            .striped(false)
                            .spacing([theme.padding * 2.0, 4.0])
                            .show(ui, |ui| {
                                for column in &table.columns {
                                    ui.label(
                                        RichText::new(column)
                                            .size(theme.small_size)
                                            .color(theme.muted),
                                    );
                                }
                                ui.end_row();
                                for row in &table.rows {
                                    for cell in row {
                                        ui.label(RichText::new(cell).color(theme.text));
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                    ui.add_space(6.0);
                }
            }
            SettingsItem::Actions(buttons) if needle.is_empty() => {
                ui.horizontal_wrapped(|ui| {
                    for button in buttons {
                        if widgets::pill_button(ui, &button.label, false, &theme).clicked() {
                            cx.request(ShellRequest::Command(button.command.clone()));
                        }
                    }
                });
                ui.add_space(6.0);
            }
            SettingsItem::Presets if needle.is_empty() => {
                ui.horizontal_wrapped(|ui| {
                    for preset in cx.catalog.presets.clone() {
                        let active = preset.is_active(cx.rice());
                        if widgets::pill_button(ui, &preset.name, active, &theme).clicked()
                            && !active
                        {
                            let txn = preset.transaction(cx.rice());
                            if let Err(error) = cx.apply(RiceEdit(txn)) {
                                self.error = Some(error.to_string());
                            }
                        }
                    }
                });
                ui.add_space(6.0);
            }
            _ => {}
        }
    }
}

/// A slider's edit: live while dragged, one undo step on release.
fn slider_edit(
    response: &egui::Response,
    path: String,
    value: RiceValue,
    changed: bool,
) -> Option<Edit> {
    if response.drag_stopped() {
        return Some(Edit::Release(path));
    }
    if !changed {
        return None;
    }
    if response.dragged() {
        Some(Edit::Preview(path, value))
    } else {
        Some(Edit::Set(path, value))
    }
}

impl SueApplet for Settings {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Settings", AppletRole::Window, "\u{2699}");
        manifest.summary = "Every setting and every rice parameter.".to_owned();
        manifest
    }

    fn navigate(&mut self, target: &str) -> bool {
        self.pending = Some(target.to_owned());
        true
    }

    fn captures_keys(&self) -> bool {
        self.recording.is_some()
    }

    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let pages = all_pages(cx.catalog, cx.rice());
        if let Some(target) = self.pending.take()
            && let Some(index) = pages.iter().position(|page| page.id == target)
        {
            self.page = index;
        }
        self.page = self.page.min(pages.len().saturating_sub(1));
        let full = ui.max_rect();
        let sidebar =
            egui::Rect::from_min_size(full.min, vec2(theme.settings_sidebar_width, full.height()));
        let pane =
            egui::Rect::from_min_max(egui::pos2(sidebar.right() + 1.0, full.top()), full.max);
        ui.painter()
            .rect_filled(sidebar, 0.0, theme.void.gamma_multiply(0.6));
        ui.painter()
            .vline(sidebar.right(), full.y_range(), theme.hairline());

        let mut side = ui.new_child(egui::UiBuilder::new().max_rect(sidebar.shrink(theme.padding)));
        side.spacing_mut().item_spacing.y = 2.0;
        for (index, page) in pages.iter().enumerate() {
            let chosen = index == self.page && self.filter.trim().is_empty();
            let (rect, response) = side.allocate_exact_size(
                vec2(side.available_width(), theme.ui_size * 2.1),
                Sense::click(),
            );
            if chosen || response.hovered() {
                side.painter().rect_filled(
                    rect,
                    theme.control_radius,
                    if chosen { theme.active } else { theme.hover },
                );
            }
            side.painter().text(
                rect.left_center() + vec2(theme.padding, 0.0),
                egui::Align2::LEFT_CENTER,
                &page.title,
                theme.font(theme.ui_size),
                if chosen { theme.text } else { theme.muted },
            );
            if response.clicked() {
                self.page = index;
                self.filter.clear();
            }
        }

        let mut right =
            ui.new_child(egui::UiBuilder::new().max_rect(pane.shrink(theme.padding * 1.5)));
        right.add(
            egui::TextEdit::singleline(&mut self.filter)
                .id(egui::Id::unique("sue-settings-filter"))
                .hint_text("Filter settings...")
                .desired_width(f32::INFINITY),
        );
        right.add_space(theme.padding);
        let needle = self.filter.trim().to_lowercase();
        let title = if needle.is_empty() {
            pages
                .get(self.page)
                .map_or_else(String::new, |page| page.title.clone())
        } else {
            format!("Results for \"{}\"", self.filter.trim())
        };
        right.label(
            RichText::new(title)
                .size(theme.heading_size)
                .color(theme.text),
        );
        if let Some(error) = &self.error {
            right.label(RichText::new(format!("! {error}")).color(theme.text));
        }
        right.add_space(theme.padding * 0.5);
        let shown: Vec<&SettingsPage> = if needle.is_empty() {
            pages.get(self.page).into_iter().collect()
        } else {
            pages.iter().collect()
        };
        ScrollArea::vertical()
            .id_salt("sue-settings-scroll")
            .auto_shrink([false, false])
            .show(&mut right, |ui| {
                for page in shown {
                    for group in &page.groups {
                        let visible = needle.is_empty()
                            || group.items.iter().any(|item| {
                                entries_of(cx.rice(), item)
                                    .iter()
                                    .any(|entry| matches(entry, &needle))
                            });
                        if !visible {
                            continue;
                        }
                        ui.add_space(theme.padding * 0.5);
                        widgets::heading(ui, &group.title, &theme);
                        ui.add_space(4.0);
                        for item in &group.items {
                            self.item(ui, item, &needle, cx);
                        }
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_follow_the_parameter_type() {
        let mut rice = RiceSchema::new();
        sue_wm::register_rice(&mut rice).unwrap();
        assert_eq!(
            typed_value(&rice, "display.frame_rate_cap", "120"),
            Ok(RiceValue::Int(120))
        );
        assert!(typed_value(&rice, "display.frame_rate_cap", "fast").is_err());
        assert!(matches!(
            typed_value(&rice, "colors.text", "#c0c0c0"),
            Ok(RiceValue::Color(_))
        ));
        assert_eq!(choice_label("high-refresh"), "High refresh");
    }
}
