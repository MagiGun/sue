//! Settings: a floating window over the page (the Sue menu's Settings, or
//! `Ctrl+,`). A category sidebar on the left; on the right only the chosen
//! category's controls, under a live filter. On a phone (or any sheet too
//! narrow for two panes) the categories are a list that opens one page at a
//! time with a back arrow, and each control sits under its name. Pages are registered data from
//! the engine's catalog; a rice section no page claims lands on a final
//! "Other" page, so every parameter stays reachable. Every edit is
//! an undoable rice transaction; a slider previews live while dragged and
//! records one undo step when released.

/// Text from the language database's `settings` namespace (the Settings window).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use std::collections::HashMap;

use egui::{Align, Layout, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use sue_platform::FormFactor;
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

/// The `rice` message of a setting: its path with a dash for the dot, e.g.
/// `display-ui_scale`. Its value is the setting's name in Settings, its
/// `.description` the line under it, and an attribute per choice
/// (`.high-refresh`) the choice's name.
#[must_use]
pub fn rice_message(path: &str) -> String {
    path.replace('.', "-")
}

/// What Settings calls `entry`, in the language in use.
fn label_of(entry: &RiceEntry) -> String {
    sue_lang::text_or("rice", &rice_message(entry.path()), &entry.label())
}

/// The line under `entry` in Settings, in the language in use.
fn description_of(entry: &RiceEntry) -> Option<String> {
    entry.description().map(|english| {
        let key = format!("{}.description", rice_message(entry.path()));
        sue_lang::text_or("rice", &key, english)
    })
}

/// `true` if choice `value` can be named in the language database (an
/// attribute: a small letter, then letters, digits, `-` and `_`). Numbers and
/// font names stay as they are.
fn named_choice(value: &str) -> bool {
    value.starts_with(|first: char| first.is_ascii_lowercase())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// The name of choice `value` of the setting at `path`, in the language in
/// use.
fn choice_of(path: &str, value: &str) -> String {
    if named_choice(value) {
        let key = format!("{}.{value}", rice_message(path));
        sue_lang::text_or("rice", &key, &choice_label(value))
    } else {
        choice_label(value)
    }
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
            tr!("other"),
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
        || label_of(entry).to_lowercase().contains(needle)
        || description_of(entry).is_some_and(|text| text.to_lowercase().contains(needle))
        || entry.path().to_lowercase().contains(needle)
        || entry
            .description()
            .is_some_and(|text| text.to_lowercase().contains(needle))
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
    /// The shortcut being recorded: its setting, and which chord (`None`
    /// adds one).
    recording: Option<(String, Option<usize>)>,
    texts: HashMap<String, String>,
    error: Option<String>,
    /// On a phone: a page is open (with a back arrow) instead of the list
    /// of pages.
    opened: bool,
    /// This frame's rows put the control under the label (narrow sheets).
    stacked: bool,
    /// How wide choices and text fields are this frame.
    field: f32,
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
                .width(self.field)
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
                let response = slider(
                    ui,
                    egui::Slider::new(&mut number, min..=max).show_value(false),
                );
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
                let response = slider(
                    ui,
                    egui::Slider::new(&mut number, min..=max).show_value(false),
                );
                let rounded = (number * 100.0).round() / 100.0;
                slider_edit(
                    &response,
                    path,
                    RiceValue::Float(rounded),
                    (rounded - value).abs() > f64::EPSILON,
                )
            }
            RiceParameter::ColorHex { value } => {
                // Picked by eye on the color wheel; dragging previews and
                // letting go makes one undo step.
                let mut rgba = [value.r, value.g, value.b, value.a];
                let pick = sue_wm::color_button(ui, ("sue-color", &path), &mut rgba, true, theme);
                if pick.released {
                    return Some(Edit::Release(path));
                }
                if !pick.changed {
                    return None;
                }
                let [r, g, b, a] = rgba;
                let picked = RiceValue::Color(sue_rice::Rgba { r, g, b, a });
                Some(if pick.dragging {
                    Edit::Preview(path, picked)
                } else {
                    Edit::Set(path, picked)
                })
            }
            RiceParameter::EnumChoice { value, options } => {
                let labels: Vec<String> = options
                    .iter()
                    .map(|option| choice_of(entry.path(), option))
                    .collect();
                let selected = options.iter().position(|option| option == value);
                // Side by side when they fit the row, else a list to pick from.
                let room = labels
                    .iter()
                    .map(|label| label.chars().count() as f32 * theme.ui_size * 0.6 + 24.0)
                    .sum::<f32>();
                let short = labels.len() <= 5
                    && labels.iter().map(String::len).sum::<usize>() < 48
                    && (!self.stacked || room <= self.field);
                if short {
                    widgets::segmented(ui, &labels, selected, theme)
                        .map(|index| Edit::Set(path, RiceValue::Text(options[index].clone())))
                } else {
                    let mut picked = None;
                    egui::ComboBox::from_id_salt(("sue-enum", entry.path()))
                        .selected_text(
                            selected.map_or_else(String::new, |index| labels[index].clone()),
                        )
                        .width(self.field)
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
                // Every chord as a chip: click one to change it, x to take
                // it away, + to add another.
                let recording = self
                    .recording
                    .as_ref()
                    .filter(|(known, _)| known == entry.path())
                    .map(|(_, index)| *index);
                let chip = |ui: &mut Ui, text: String, on: bool| {
                    ui.add(
                        egui::Button::new(
                            RichText::new(text)
                                .font(theme.mono(theme.ui_size))
                                .color(theme.text),
                        )
                        .fill(if on { theme.active } else { theme.pill })
                        .stroke(if on {
                            theme.hairline_active()
                        } else {
                            theme.hairline()
                        }),
                    )
                };
                let mut edit = None;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (index, chord) in value.chords().iter().enumerate() {
                        let on = recording == Some(Some(index));
                        let text = if on {
                            tr!("press-keys-ellipsis")
                        } else {
                            chord.to_string()
                        };
                        if chip(ui, text, on)
                            .on_hover_text(tr!("click-then-press-shortcut-put-here"))
                            .clicked()
                        {
                            self.recording = Some((path.clone(), Some(index)));
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("\u{00D7}").color(theme.muted))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text(tr!("take-this-shortcut-away"))
                            .clicked()
                        {
                            let mut next = value.clone();
                            next.remove(index);
                            edit = Some(Edit::Set(path.clone(), RiceValue::Keybind(next)));
                        }
                    }
                    if value.is_empty() && recording.is_none() {
                        ui.label(RichText::new(tr!("none")).color(theme.muted));
                    }
                    let adding = recording == Some(None);
                    if adding {
                        chip(ui, tr!("press-keys-ellipsis"), true);
                    } else if ui
                        .add(egui::Button::new(RichText::new("+").color(theme.text)).small())
                        .on_hover_text(tr!("add-another-shortcut-as-many-as"))
                        .clicked()
                    {
                        self.recording = Some((path.clone(), None));
                    }
                });
                if edit.is_some() {
                    return edit;
                }
                if let Some(index) = recording {
                    return self.record_chord(ui, path, value, index);
                }
                None
            }
            RiceParameter::Text { value } | RiceParameter::ShaderPath { value } => {
                let text = self
                    .texts
                    .entry(path.clone())
                    .or_insert_with(|| value.clone());
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(self.field.max(240.0).min(ui.available_width())),
                );
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

    /// The next key press with its modifiers, as a chord put in place of
    /// chord `index` of `current` (or added when `None`).
    fn record_chord(
        &mut self,
        ui: &Ui,
        path: String,
        current: &sue_rice::Keybinds,
        index: Option<usize>,
    ) -> Option<Edit> {
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
        let chord = sue_wm::shortcuts::chord_text(modifiers, key);
        match chord.parse::<Keybind>() {
            Ok(keybind) => {
                let mut next = current.clone();
                match index {
                    Some(index) => next.replace(index, keybind),
                    None => next.push(keybind),
                }
                Some(Edit::Set(path, RiceValue::Keybind(next)))
            }
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
        let switch = matches!(entry.parameter(), RiceParameter::Bool { .. });
        if self.stacked && !switch {
            // Narrow: the name and what it does, then the control under
            // them across the whole width.
            self.field = width;
            ui.label(
                RichText::new(label_of(entry))
                    .size(theme.ui_size)
                    .color(theme.text),
            );
            if let Some(description) = description_of(entry) {
                ui.add(
                    egui::Label::new(
                        RichText::new(description)
                            .size(theme.small_size)
                            .color(theme.muted),
                    )
                    .wrap(),
                );
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(width);
                ui.spacing_mut().slider_width = (width - 72.0).max(80.0);
                edit = self.control(ui, entry, cx);
            });
        } else {
            self.field = if self.stacked { width } else { 200.0 };
            self.side_by_side(ui, entry, width, &mut edit, cx);
        }
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

    /// A row with the name and what it does at the left and the control at
    /// the right.
    fn side_by_side(
        &mut self,
        ui: &mut Ui,
        entry: &RiceEntry,
        width: f32,
        edit: &mut Option<Edit>,
        cx: &mut AppletContext<'_>,
    ) {
        let theme = cx.theme.clone();
        let label_width = if self.stacked {
            (width - 64.0).max(80.0)
        } else {
            (width * 0.5).max(160.0)
        };
        ui.horizontal(|ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.set_width(label_width);
                ui.label(
                    RichText::new(label_of(entry))
                        .size(theme.ui_size)
                        .color(theme.text),
                );
                if let Some(description) = description_of(entry) {
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
                *edit = self.control(ui, entry, cx);
            });
        });
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
            SettingsItem::Note(text) if needle.is_empty() => {
                ui.add(egui::Label::new(RichText::new(text).color(theme.muted)).wrap());
                ui.add_space(6.0);
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

/// A slider whose knob stays a comfortable size when the controls around
/// it are tall for fingers (its track still takes the whole row).
fn slider(ui: &mut Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.scope(|ui| {
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = spacing.interact_size.y.min(26.0);
        ui.add(slider)
    })
    .inner
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
        let mut manifest = AppletManifest::new(ID, "Settings", AppletRole::Window, "\u{2699}")
            .localized("settings");
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
        // A page by its id opens it; anything else shows the list of pages.
        if let Some(target) = self.pending.take() {
            match pages.iter().position(|page| page.id == target) {
                Some(index) => {
                    self.page = index;
                    self.opened = true;
                }
                None => self.opened = false,
            }
        }
        self.page = self.page.min(pages.len().saturating_sub(1));
        let full = ui.max_rect();
        let form = cx.view.form_factor;
        // Fingers get rows and controls at least 44 points tall on a
        // phone, 36 on a tablet.
        let target = match form {
            FormFactor::Desktop => None,
            FormFactor::Tablet => Some(36.0),
            FormFactor::Mobile => Some(44.0),
        };
        let row = target.unwrap_or(theme.ui_size * 2.1);
        // An upright phone, or any sheet too narrow for two panes, lists
        // the pages and opens one at a time; wider sheets (a phone on its
        // side too) keep the list at the left, sized to the sheet.
        let upright = full.height() >= full.width();
        if (form == FormFactor::Mobile && upright) || full.width() < 560.0 {
            self.stacked = true;
            let mut body =
                ui.new_child(egui::UiBuilder::new().max_rect(full.shrink(theme.padding * 1.5)));
            fingers(&mut body, target);
            if self.opened || !self.filter.trim().is_empty() {
                if back_row(&mut body, row, &theme) {
                    self.opened = false;
                    self.filter.clear();
                }
                self.page_ui(&mut body, &pages, cx);
            } else {
                self.filter_box(&mut body, &theme);
                body.add_space(theme.padding);
                ScrollArea::vertical()
                    .id_salt("sue-settings-list")
                    .auto_shrink([false, false])
                    .show(&mut body, |ui| {
                        if let Some(index) = page_list(ui, &pages, None, row, true, &theme) {
                            self.page = index;
                            self.opened = true;
                        }
                    });
            }
            return;
        }
        self.stacked = false;
        let width = if form == FormFactor::Desktop {
            theme.settings_sidebar_width
        } else {
            (full.width() * 0.3).clamp(180.0, 300.0)
        };
        let sidebar = egui::Rect::from_min_size(full.min, vec2(width, full.height()));
        let pane =
            egui::Rect::from_min_max(egui::pos2(sidebar.right() + 1.0, full.top()), full.max);
        ui.painter()
            .rect_filled(sidebar, 0.0, theme.void.gamma_multiply(0.6));
        ui.painter()
            .vline(sidebar.right(), full.y_range(), theme.hairline());

        let mut side = ui.new_child(egui::UiBuilder::new().max_rect(sidebar.shrink(theme.padding)));
        side.spacing_mut().item_spacing.y = 2.0;
        let chosen = self.filter.trim().is_empty().then_some(self.page);
        ScrollArea::vertical()
            .id_salt("sue-settings-pages")
            .auto_shrink([false, false])
            .show(&mut side, |ui| {
                if let Some(index) = page_list(ui, &pages, chosen, row, false, &theme) {
                    self.page = index;
                    self.filter.clear();
                }
            });

        let mut right =
            ui.new_child(egui::UiBuilder::new().max_rect(pane.shrink(theme.padding * 1.5)));
        fingers(&mut right, target);
        self.filter_box(&mut right, &theme);
        right.add_space(theme.padding);
        self.page_ui(&mut right, &pages, cx);
    }
}

/// Controls at least `target` points tall, for fingers.
fn fingers(ui: &mut Ui, target: Option<f32>) {
    if let Some(height) = target {
        ui.spacing_mut().interact_size.y = height;
        ui.spacing_mut().button_padding = vec2(10.0, 8.0);
    }
}

/// The list of Settings pages, one row each; returns the one clicked.
/// `arrows` marks each row as opening a page (on a phone).
fn page_list(
    ui: &mut Ui,
    pages: &[SettingsPage],
    chosen: Option<usize>,
    row: f32,
    arrows: bool,
    theme: &Theme,
) -> Option<usize> {
    let mut clicked = None;
    for (index, page) in pages.iter().enumerate() {
        let on = chosen == Some(index);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), row), Sense::click());
        if on || response.hovered() {
            ui.painter().rect_filled(
                rect,
                theme.control_radius,
                if on { theme.active } else { theme.hover },
            );
        }
        ui.painter().text(
            rect.left_center() + vec2(theme.padding, 0.0),
            egui::Align2::LEFT_CENTER,
            &page.title,
            theme.font(theme.ui_size),
            if on || arrows {
                theme.text
            } else {
                theme.muted
            },
        );
        if arrows {
            ui.painter().text(
                rect.right_center() - vec2(theme.padding, 0.0),
                egui::Align2::RIGHT_CENTER,
                "\u{203A}",
                theme.font(theme.ui_size * 1.4),
                theme.muted,
            );
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() + 1.0,
                Stroke::new(theme.border_width, theme.border),
            );
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// A phone's back arrow to the list of pages; `true` when tapped.
fn back_row(ui: &mut Ui, row: f32, theme: &Theme) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), row), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.hover);
    }
    ui.painter().text(
        rect.left_center() + vec2(theme.padding * 0.5, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("\u{2039}  {}", tr!("all-settings")),
        theme.font(theme.ui_size),
        theme.text,
    );
    response.clicked()
}

impl Settings {
    fn filter_box(&mut self, ui: &mut Ui, theme: &Theme) {
        let height = ui.spacing().interact_size.y;
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .id(egui::Id::unique("sue-settings-filter"))
                .hint_text(tr!("filter-settings-ellipsis"))
                .desired_width(f32::INFINITY)
                .min_size(vec2(0.0, height))
                .align(egui::Align2::LEFT_CENTER)
                .font(theme.font(theme.ui_size)),
        );
    }

    /// The open page's title and its groups of settings, or every page's
    /// settings that match the filter.
    fn page_ui(&mut self, ui: &mut Ui, pages: &[SettingsPage], cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let needle = self.filter.trim().to_lowercase();
        let title = if needle.is_empty() {
            pages
                .get(self.page)
                .map_or_else(String::new, |page| page.title.clone())
        } else {
            tr!("results-for", filter = self.filter.trim())
        };
        ui.add(
            egui::Label::new(
                RichText::new(title)
                    .size(theme.heading_size)
                    .color(theme.text),
            )
            .wrap(),
        );
        if let Some(error) = &self.error {
            ui.add(egui::Label::new(RichText::new(format!("! {error}")).color(theme.text)).wrap());
        }
        ui.add_space(theme.padding * 0.5);
        let shown: Vec<&SettingsPage> = if needle.is_empty() {
            pages.get(self.page).into_iter().collect()
        } else {
            pages.iter().collect()
        };
        ScrollArea::vertical()
            .id_salt("sue-settings-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
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
    fn the_codec_table_reads_from_the_language_database() {
        for codec in sue_io::builtin_codecs() {
            assert!(
                sue_lang::has_english("io", codec.abilities),
                "io.ftl lacks {}",
                codec.abilities
            );
        }
    }

    #[test]
    fn choices_read_like_words() {
        assert_eq!(choice_label("high-refresh"), "High refresh");
    }

    /// Every setting Sue declares, with its English name, description and
    /// choices.
    fn every_setting() -> RiceSchema {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped rice parses");
        sue_wm::register_rice(&mut rice).expect("the shell's settings register");
        rice.register_all(&crate::engine::shell_declarations())
            .expect("the engine's settings register");
        rice.register_all(
            &crate::applets::launchpad()
                .expect("the built-in applets install")
                .rice_declarations(),
        )
        .expect("the applets' settings register");
        rice
    }

    /// The `rice.ftl` lines of `entry` in English.
    fn english_of(entry: &RiceEntry) -> String {
        let mut out = format!("{} = {}\n", rice_message(entry.path()), entry.label());
        if let Some(description) = entry.description() {
            out.push_str(&format!("    .description = {description}\n"));
        }
        if let RiceParameter::EnumChoice { options, .. } = entry.parameter() {
            for option in options {
                if named_choice(option) {
                    out.push_str(&format!("    .{option} = {}\n", choice_label(option)));
                }
            }
        }
        out
    }

    #[test]
    fn every_setting_reads_its_english_from_the_language_database() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let rice = every_setting();
        let mut wrong = Vec::new();
        let mut file = String::new();
        let mut section = "";
        for entry in rice.entries() {
            if entry.section() != section {
                section = entry.section();
                file.push_str(&format!("\n## [{section}]\n\n"));
            }
            file.push_str(&english_of(entry));
            let mut fine = label_of(entry) == entry.label()
                && sue_lang::has("rice", &rice_message(entry.path()))
                && description_of(entry).as_deref() == entry.description();
            if let Some(description) = entry.description() {
                let key = format!("{}.description", rice_message(entry.path()));
                fine &= sue_lang::has("rice", &key) && sue_lang::text("rice", &key) == description;
            }
            if let RiceParameter::EnumChoice { options, .. } = entry.parameter() {
                for option in options {
                    if named_choice(option) {
                        let key = format!("{}.{option}", rice_message(entry.path()));
                        fine &= sue_lang::has("rice", &key);
                    }
                }
            }
            if !fine {
                wrong.push(english_of(entry));
            }
        }
        assert!(
            wrong.is_empty(),
            "crates/sue-lang/locales/en/rice.ftl lacks or differs from these settings; \
             the lines they need:\n\n{}\nThe whole file reads:\n{file}",
            wrong.join("")
        );
    }
}
