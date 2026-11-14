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

mod icons;
mod keys;

use std::collections::{HashMap, HashSet};

use egui::{Align, Layout, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use sue_platform::FormFactor;
use sue_rice::{Keybind, RiceEntry, RiceParameter, RiceSchema, RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, Catalog, RiceEdit, SettingsGroup, SettingsItem,
    SettingsPage, ShellRequest, SueApplet, Theme, widgets,
};

/// The applet id.
pub const ID: &str = "settings";

/// The id of the last page: the settings file, and every setting no page
/// claims.
pub const ADVANCED_PAGE: &str = "advanced";

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

/// `true` when a page of `pages` (or a key map of `catalog`) shows the
/// setting at `path` in `section`.
fn claimed(pages: &[SettingsPage], catalog: &Catalog, path: &str, section: &str) -> bool {
    catalog.keymaps.iter().any(|map| map.section == section)
        || pages
            .iter()
            .flat_map(SettingsPage::claims)
            .any(|item| match item {
                SettingsItem::Param(param) => param == path,
                SettingsItem::Section(name) => name == section,
                _ => false,
            })
}

/// The registered pages, with every setting no page shows folded into
/// Advanced, one closed group per section (a page of its own when there
/// is no Advanced page), so every setting stays reachable.
pub fn all_pages(catalog: &Catalog, rice: &RiceSchema) -> Vec<SettingsPage> {
    let mut pages = catalog.pages.clone();
    let mut other: Vec<(String, Vec<SettingsItem>)> = Vec::new();
    for entry in rice.entries() {
        if claimed(&pages, catalog, entry.path(), entry.section()) {
            continue;
        }
        let item = SettingsItem::Param(entry.path().to_owned());
        match other
            .iter_mut()
            .find(|(section, _)| section == entry.section())
        {
            Some((_, items)) => items.push(item),
            None => other.push((entry.section().to_owned(), vec![item])),
        }
    }
    if other.is_empty() {
        return pages;
    }
    let groups: Vec<SettingsGroup> = other
        .into_iter()
        .map(|(section, items)| SettingsGroup::new(choice_label(&section), items).advanced())
        .collect();
    match pages.iter_mut().find(|page| page.id == ADVANCED_PAGE) {
        Some(page) => page.groups.extend(groups),
        None => pages.push(SettingsPage::new(ADVANCED_PAGE, tr!("other"), groups)),
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

/// `true` if `entry` matches the lower-cased `needle`, by its name, what
/// it does, its path, or the other names `words` give it (rice path,
/// words).
fn matches(entry: &RiceEntry, needle: &str, words: &[(String, String)]) -> bool {
    needle.is_empty()
        || words
            .iter()
            .any(|(path, words)| path == entry.path() && words.to_lowercase().contains(needle))
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

/// Catches the next key press while Settings records a shortcut, before
/// the frame starts: the studio under the Settings window never sees it,
/// so recording R does not also pick the Rotate tool.
#[derive(Clone, Default)]
struct Recorder(std::sync::Arc<std::sync::Mutex<Recording>>);

/// What [`Recorder`] holds between frames.
#[derive(Default)]
struct Recording {
    /// A shortcut is being recorded: the next press is caught.
    armed: bool,
    /// The pass that last armed it: a Settings window that closed while
    /// recording stops catching after one frame.
    pass: u64,
    /// The press caught, for Settings to read this frame.
    caught: Option<(egui::Key, egui::Modifiers)>,
}

impl Recorder {
    /// The recorder of `ctx`, put in place the first time.
    fn of(ctx: &egui::Context) -> Self {
        let recorder = ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<Self>(egui::Id::unique("sue-settings-recorder"))
                .clone()
        });
        ctx.add_plugin(KeyRecorder(recorder.clone()));
        recorder
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Recording> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Catches the next press from the next frame on (pass `pass` is this
    /// one), or stops.
    fn arm(&self, armed: bool, pass: u64) {
        let mut state = self.state();
        state.armed = armed;
        state.pass = pass;
        if !armed {
            state.caught = None;
        }
    }

    /// The press caught, once.
    fn take(&self) -> Option<(egui::Key, egui::Modifiers)> {
        self.state().caught.take()
    }
}

/// The egui plugin that takes the press out of the frame's input.
struct KeyRecorder(Recorder);

impl egui::plugin::Plugin for KeyRecorder {
    fn debug_name(&self) -> &'static str {
        "sue-settings-key-recorder"
    }

    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let mut state = self.0.state();
        if !state.armed || state.caught.is_some() || ctx.cumulative_pass_nr() > state.pass + 1 {
            return;
        }
        let pressed = input
            .events
            .iter()
            .position(|event| matches!(event, egui::Event::Key { pressed: true, .. }));
        if let Some(index) = pressed
            && let egui::Event::Key { key, modifiers, .. } = input.events.remove(index)
        {
            state.caught = Some((key, modifiers));
        }
    }
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
    /// Reset All Settings was clicked once; a second click resets.
    confirm_reset: bool,
    /// The closed groups someone opened (and the open ones closed), by
    /// page and title.
    flipped: HashSet<String>,
    /// Every page's search words, (rice path, words), this frame.
    words: Vec<(String, String)>,
    /// A line for the information box: a key just set that another
    /// command holds too.
    notice: Option<String>,
    /// The icon packs.
    icons: icons::IconsUi,
}

/// The value `path` starts with: the one in the settings Sue ships.
fn default_of(path: &str) -> Option<RiceValue> {
    static SHIPPED: std::sync::OnceLock<Option<RiceSchema>> = std::sync::OnceLock::new();
    SHIPPED
        .get_or_init(|| RiceSchema::parse(sue_rice::DEFAULT_RICE).ok())
        .as_ref()?
        .get(path)
}

/// The settings among `entries` that differ from how Sue ships, with the
/// value each goes back to.
fn changed<'e>(
    entries: impl IntoIterator<Item = &'e RiceEntry>,
    rice: &RiceSchema,
) -> Vec<(String, RiceValue)> {
    entries
        .into_iter()
        .filter_map(|entry| {
            let shipped = default_of(entry.path())?;
            (rice.get(entry.path()).as_ref() != Some(&shipped))
                .then(|| (entry.path().to_owned(), shipped))
        })
        .collect()
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

    /// Puts `settings` back to how Sue ships them, each an undoable step.
    fn reset(&mut self, cx: &mut AppletContext<'_>, settings: Vec<(String, RiceValue)>) {
        for (path, value) in settings {
            self.commit(cx, &path, value);
        }
    }

    /// The small Reset button of a row that differs from how Sue ships it.
    fn reset_button(ui: &mut Ui, entry: &RiceEntry, cx: &AppletContext<'_>) -> Option<Edit> {
        let shipped = default_of(entry.path())?;
        if cx.rice().get(entry.path()).as_ref() == Some(&shipped) {
            return None;
        }
        let theme = &cx.theme;
        ui.add(
            egui::Button::new(
                RichText::new(tr!("settings-reset"))
                    .size(theme.small_size)
                    .color(theme.muted),
            )
            .frame(false),
        )
        .on_hover_text(tr!("settings-reset-hint"))
        .clicked()
        .then(|| Edit::Set(entry.path().to_owned(), shipped))
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
                    return self
                        .record_chord(ui, value, index)
                        .map(|next| Edit::Set(path, RiceValue::Keybind(next)));
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
    /// chord `index` of `current` (or added when `None`): the chords that
    /// result.
    fn record_chord(
        &mut self,
        ui: &Ui,
        current: &sue_rice::Keybinds,
        index: Option<usize>,
    ) -> Option<sue_rice::Keybinds> {
        let (key, modifiers) = Recorder::of(ui.ctx()).take()?;
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
                Some(next)
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
                if let Some(reset) = Self::reset_button(ui, entry, cx) {
                    edit = Some(reset);
                }
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
                if let Some(reset) = Self::reset_button(ui, entry, cx) {
                    *edit = Some(reset);
                }
            });
        });
    }

    fn item(&mut self, ui: &mut Ui, item: &SettingsItem, needle: &str, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        match item {
            SettingsItem::Param(_) | SettingsItem::Section(_) => {
                let entries: Vec<RiceEntry> = entries_of(cx.rice(), item)
                    .into_iter()
                    .filter(|entry| matches(entry, needle, &self.words))
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
                            match &button.applet {
                                // A studio's own dialog: over the studio,
                                // with Settings out of the way.
                                Some(applet) => {
                                    if let Ok(applet) = sue_wm::AppletId::new(applet) {
                                        cx.request(ShellRequest::CloseWindow);
                                        cx.request(ShellRequest::Open(applet.clone()));
                                        cx.request(ShellRequest::AppletCommand {
                                            applet,
                                            command: button.command.clone(),
                                        });
                                    }
                                }
                                None => {
                                    cx.request(ShellRequest::Command(button.command.clone()));
                                }
                            }
                        }
                    }
                });
                ui.add_space(6.0);
            }
            SettingsItem::Keys(id) => self.keys_ui(ui, id, needle, cx),
            SettingsItem::Icons if needle.is_empty() => self.icons_ui(ui, cx),
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
        manifest.summary = "Everything you can change in Sue.".to_owned();
        manifest
    }

    fn picked(
        &mut self,
        pick: &sue_wm::FilePick,
        path: &std::path::Path,
        cx: &mut AppletContext<'_>,
    ) {
        self.icons.picked(pick, path, cx);
    }

    fn navigate(&mut self, target: &str) -> bool {
        // A deep link shows its page, not the last search.
        self.pending = Some(target.to_owned());
        self.filter.clear();
        self.notice = None;
        true
    }

    fn captures_keys(&self) -> bool {
        self.recording.is_some()
    }

    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let pages = all_pages(cx.catalog, cx.rice());
        Recorder::of(ui.ctx()).arm(self.recording.is_some(), ui.ctx().cumulative_pass_nr());
        self.words = pages
            .iter()
            .flat_map(|page| page.search.iter().cloned())
            .collect();
        // A page by its id opens it; anything else shows the list of pages.
        if let Some(target) = self.pending.take() {
            match pages.iter().position(|page| page.answers(&target)) {
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

/// `true` while `group` is to be shown: always, or while the setting it
/// waits for holds its value.
fn shows(group: &SettingsGroup, rice: &RiceSchema) -> bool {
    group
        .when
        .as_ref()
        .is_none_or(|(path, value)| rice.get_str(path) == Some(value.as_str()))
}

impl Settings {
    /// `true` when `item` has something the lower-cased `needle` finds.
    fn item_finds(&self, item: &SettingsItem, needle: &str, cx: &AppletContext<'_>) -> bool {
        match item {
            SettingsItem::Keys(id) => cx.catalog.keymap(id).is_some_and(|map| {
                let resolved = keys::resolve(cx.rice(), map);
                map.groups.iter().any(|group| {
                    group.commands.iter().any(|command| {
                        let chords = resolved
                            .rows
                            .iter()
                            .find(|(name, _)| name == &command.command)
                            .map(|(_, chords)| chords.clone())
                            .unwrap_or_default();
                        keys::finds(
                            needle,
                            &group.title,
                            &command.label,
                            &command.command,
                            &chords,
                        )
                    })
                })
            }),
            _ => entries_of(cx.rice(), item)
                .iter()
                .any(|entry| matches(entry, needle, &self.words)),
        }
    }

    /// A group's title that opens and closes it; `true` while it is open.
    /// `closed` is how it starts; a filter opens every group it finds.
    fn fold(&mut self, ui: &mut Ui, key: &str, title: &str, closed: bool, theme: &Theme) -> bool {
        let flipped = self.flipped.contains(key);
        let open = !closed || flipped;
        let icon = if open {
            sue_wm::icons::Icon::ChevronDown
        } else {
            sue_wm::icons::Icon::ChevronRight
        };
        let response = ui
            .horizontal(|ui| {
                let size = egui::Vec2::splat(theme.ui_size);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                sue_wm::icons::paint(ui.painter(), rect, icon, theme.muted);
                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .size(theme.ui_size)
                            .strong()
                            .color(theme.text),
                    )
                    .sense(Sense::click()),
                )
            })
            .inner;
        if response.clicked() && closed {
            if flipped {
                self.flipped.remove(key);
            } else {
                self.flipped.insert(key.to_owned());
            }
        }
        open
    }

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
        if let Some(notice) = &self.notice {
            ui.add(egui::Label::new(RichText::new(notice).color(theme.text)).wrap());
        }
        // Back to how Sue ships: this page, or everything (asked twice).
        if needle.is_empty()
            && let Some(page) = pages.get(self.page)
        {
            let on_page: Vec<RiceEntry> = page
                .groups
                .iter()
                .flat_map(|group| &group.items)
                .flat_map(|item| entries_of(cx.rice(), item))
                .cloned()
                .collect();
            let page_changes = changed(&on_page, cx.rice());
            let all_changes = changed(cx.rice().entries(), cx.rice());
            let mut reset = None;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !page_changes.is_empty(),
                        egui::Button::new(tr!("settings-reset-page")),
                    )
                    .clicked()
                {
                    reset = Some(page_changes.clone());
                }
                let label = if self.confirm_reset {
                    tr!("settings-reset-all-confirm")
                } else {
                    tr!("settings-reset-all")
                };
                if ui
                    .add_enabled(!all_changes.is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    if self.confirm_reset {
                        reset = Some(all_changes.clone());
                        self.confirm_reset = false;
                    } else {
                        self.confirm_reset = true;
                    }
                }
            });
            if let Some(reset) = reset {
                self.reset(cx, reset);
            }
        }
        ui.add_space(theme.padding * 0.5);
        let shown: Vec<&SettingsPage> = if needle.is_empty() {
            pages.get(self.page).into_iter().collect()
        } else {
            pages.iter().collect()
        };
        ScrollArea::vertical()
            .id_salt(("sue-settings-scroll", self.page, needle.is_empty()))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for page in shown {
                    for group in &page.groups {
                        if !shows(group, cx.rice()) {
                            continue;
                        }
                        let visible = needle.is_empty()
                            || group
                                .items
                                .iter()
                                .any(|item| self.item_finds(item, &needle, cx));
                        if !visible {
                            continue;
                        }
                        ui.add_space(theme.padding * 0.5);
                        if group.advanced {
                            let key = format!("{}/{}", page.id, group.title);
                            if !self.fold(ui, &key, &group.title, needle.is_empty(), &theme) {
                                continue;
                            }
                        } else {
                            widgets::heading(ui, &group.title, &theme);
                        }
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
    fn every_setting_has_a_default_to_go_back_to() {
        let rice = every_setting();
        let missing: Vec<&str> = rice
            .entries()
            .iter()
            .map(RiceEntry::path)
            .filter(|path| default_of(path).is_none())
            .collect();
        assert!(missing.is_empty(), "no shipped value for {missing:?}");
        let mut changed_rice = every_setting();
        let path = "general.confirm_on_exit";
        let shipped = default_of(path).unwrap();
        let other = match shipped {
            RiceValue::Bool(on) => RiceValue::Bool(!on),
            _ => panic!("a switch"),
        };
        changed_rice.set(path, other).unwrap();
        let back = changed(changed_rice.entries(), &changed_rice);
        assert_eq!(back, vec![(path.to_owned(), shipped)]);
        assert!(changed(rice.entries(), &rice).is_empty(), "as shipped");
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
        let mut same = Vec::new();

        let mut file = String::new();
        let mut section = "";
        for entry in rice.entries() {
            if entry.section() != section {
                section = entry.section();
                file.push_str(&format!("\n## [{section}]\n\n"));
            }
            file.push_str(&english_of(entry));
            // What people read may say it in plainer words than the
            // declaration (the settings file's own comment); every such
            // line is in the ledger of reworded lines.
            let message = rice_message(entry.path());
            let reworded = REWORDED.contains(&("rice", message.as_str()));
            let as_declared = label_of(entry) == entry.label()
                && description_of(entry).as_deref() == entry.description();
            let mut fine = sue_lang::has("rice", &message) && (reworded || as_declared);
            if let Some(description) = entry.description() {
                let key = format!("{message}.description");
                fine &= sue_lang::has("rice", &key)
                    && (reworded || sue_lang::text("rice", &key) == description);
            }
            if reworded && as_declared {
                same.push(message.clone());
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
        assert!(
            same.is_empty(),
            "reworded, yet the English is the declaration's again: take {same:?} out of REWORDED"
        );
    }

    /// Lines whose English was reworded into plain words by 0.0.8 (P3-15),
    /// as (namespace, key). Until the translation pass, no other language
    /// may keep a translation of what the line used to say: the line
    /// falls back to English. The pass takes a key out of this list when
    /// it translates the new words.
    const REWORDED: &[(&str, &str)] = &[
        ("settings", "applet-summary"),
        ("sue", "could-not-save-the-rice"),
        ("sue", "rice-file-has-error"),
        ("sue", "rice-file-matches-what-sue-shows"),
        ("sue", "export-rice"),
        ("sue", "over-mib-limit-io-max-ingest"),
        ("rice", "profile-hardware"),
        ("rice", "display-frame_rate_cap"),
        ("rice", "display-ui_scale"),
        ("rice", "display-interface"),
        ("rice", "display-phone_frame"),
        ("rice", "display-gpu_preference"),
        ("rice", "motion-spring_stiffness"),
        ("rice", "motion-spring_damping"),
        ("rice", "motion-settle_grace_ms"),
        ("rice", "motion-message_seconds"),
        ("rice", "layout-border_width"),
        ("rice", "layout-control_radius"),
        ("rice", "layout-card_radius"),
        ("rice", "layout-pill_radius"),
        ("rice", "layout-pill_height"),
        ("rice", "layout-bar_height"),
        ("rice", "layout-settings_sidebar_width"),
        ("rice", "colors-void"),
        ("rice", "colors-panel"),
        ("rice", "colors-pill"),
        ("rice", "colors-hover"),
        ("rice", "colors-active"),
        ("rice", "colors-border"),
        ("rice", "colors-border_active"),
        ("rice", "colors-muted"),
        ("rice", "colors-selection"),
        ("rice", "colors-selection_text"),
        ("rice", "colors-grid"),
        ("rice", "typography-ui_font"),
        ("rice", "typography-ui_size"),
        ("rice", "typography-small_size"),
        ("rice", "typography-title_size"),
        ("rice", "typography-heading_size"),
        ("rice", "typography-mono_size"),
        ("rice", "input-wheel_line_px"),
        ("rice", "io-jxl_effort"),
        ("rice", "io-jpeg_lossless_transcode"),
        ("rice", "io-byte_map_width"),
        ("rice", "io-max_ingest_mib"),
        ("rice", "io-convert_dir"),
        ("rice", "audio-buffer_frames"),
        ("rice", "audio-realtime_priority"),
        ("rice", "packages-require_signatures"),
        ("rice", "sue_paint-dpi"),
        ("rice", "sue_paint-materials_dir"),
        ("rice", "sue_paint-touch_mode"),
        ("rice", "sue_paint-pen_pressure"),
        ("rice", "sue_paint-shift_line"),
        ("rice", "sue_paint-two_finger_tap"),
        ("rice", "sue_paint-three_finger_tap"),
        ("rice", "sue_paint-brush_cursor"),
        ("rice", "sue_paint-selection_layer_opacity"),
        ("rice", "sue_paint-rotate_step"),
        ("rice", "sue_paint-alt_tool"),
        ("rice", "sue_paint-space_tool"),
        ("rice", "sue_paint-ctrl_shift"),
        ("rice", "sue_paint-language"),
        ("rice", "sue_writer-language"),
        ("rice", "sue_paint_colors-void"),
        ("rice", "sue_paint_colors-panel"),
        ("rice", "sue_paint_colors-pill"),
        ("rice", "sue_paint_colors-hover"),
        ("rice", "sue_paint_colors-active"),
        ("rice", "sue_paint_colors-border"),
        ("rice", "sue_paint_colors-border_active"),
        ("rice", "sue_paint_colors-muted"),
        ("rice", "sue_paint_colors-selection"),
        ("rice", "sue_paint_colors-selection_text"),
        ("rice", "sue_paint_colors-grid"),
        ("rice", "sue_paint-custom_key_1"),
        ("rice", "sue_paint-custom_key_2"),
        ("rice", "sue_paint-custom_key_3"),
        ("rice", "sue_paint-custom_key_4"),
        ("rice", "sue_paint-custom_key_5"),
        ("rice", "sue_paint-custom_key_6"),
        ("rice", "sue_paint-custom_key_7"),
    ];

    /// The language files of every language but English, as (language,
    /// namespace, text).
    fn translations() -> Vec<(String, String, String)> {
        let locales =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/sue-lang/locales");
        let mut found = Vec::new();
        for language in std::fs::read_dir(&locales)
            .expect("the languages")
            .flatten()
        {
            let tag = language.file_name().to_string_lossy().into_owned();
            if tag == sue_lang::ENGLISH {
                continue;
            }
            for file in std::fs::read_dir(language.path())
                .expect("a language")
                .flatten()
            {
                let name = file.file_name().to_string_lossy().into_owned();
                if let Some(namespace) = name.strip_suffix(".ftl") {
                    let text = std::fs::read_to_string(file.path()).expect("a language file");
                    found.push((tag.clone(), namespace.to_owned(), text));
                }
            }
        }
        found
    }

    #[test]
    fn a_reworded_line_keeps_no_translation_of_its_old_words() {
        let files = translations();
        assert!(files.len() >= 8 * 3, "the language files are found");
        let mut stale = Vec::new();
        for (namespace, key) in REWORDED {
            assert!(sue_lang::has(namespace, key), "{namespace} {key}");
            for (language, file, text) in &files {
                let defines = text.lines().any(|line| {
                    line.strip_prefix(key)
                        .is_some_and(|rest| rest.trim_start().starts_with('='))
                });
                if file == namespace && defines {
                    stale.push(format!("{language}/{namespace}.ftl: {key}"));
                }
            }
        }
        assert!(
            stale.is_empty(),
            "a translation of words the English no longer says:\n{}",
            stale.join("\n")
        );
    }

    /// Settings as Sue boots: every page, with what no page claims folded
    /// into Advanced.
    fn booted() -> (crate::engine::Engine, Vec<SettingsPage>, std::path::PathBuf) {
        let (host, root) = crate::engine::tests::temp_host("settings-pages");
        let engine = crate::engine::Engine::boot(&host).expect("Sue boots");
        let pages = all_pages(&engine.catalog, &engine.world.rice);
        (engine, pages, root)
    }

    #[test]
    fn no_setting_has_two_rows_and_every_setting_has_one() {
        let (engine, pages, root) = booted();
        let rice = &engine.world.rice;
        let mut rows: Vec<String> = Vec::new();
        for page in &pages {
            for item in page.claims() {
                rows.extend(
                    entries_of(rice, item)
                        .iter()
                        .map(|entry| entry.path().to_owned()),
                );
            }
        }
        for map in &engine.catalog.keymaps {
            rows.extend(
                rice.entries()
                    .iter()
                    .filter(|entry| entry.section() == map.section)
                    .map(|entry| entry.path().to_owned()),
            );
        }
        let mut twice: Vec<&String> = rows
            .iter()
            .filter(|path| rows.iter().filter(|other| other == path).count() > 1)
            .collect();
        twice.dedup();
        assert!(twice.is_empty(), "rows shown twice: {twice:?}");
        for entry in rice.entries() {
            assert!(
                rows.iter().any(|row| row == entry.path()),
                "{} has no row",
                entry.path()
            );
        }
        // Sue's own pages, the studios' own, then the rest; no MCP page.
        let ids: Vec<&str> = pages.iter().map(|page| page.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "general",
                "language",
                "appearance",
                "display",
                "audio",
                "files",
                sue_paint::ID,
                sue_writer::ID,
                "keymaps",
                "packages",
                ADVANCED_PAGE,
            ]
        );
        // The deep links of before still open their pages.
        assert!(
            engine
                .catalog
                .page("studios")
                .is_some_and(|page| page.id == sue_paint::ID)
        );
        assert_eq!(
            pages.last().map(|page| page.groups.len()),
            Some(1),
            "nothing unclaimed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_row_and_group_reads_english() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let (engine, pages, root) = booted();
        let rice = &engine.world.rice;
        for page in &pages {
            assert!(!page.title.trim().is_empty(), "{}", page.id);
            for group in &page.groups {
                assert!(
                    !group.title.trim().is_empty(),
                    "{} has a group with no title",
                    page.id
                );
                for item in &group.items {
                    for entry in entries_of(rice, item) {
                        assert!(
                            sue_lang::has("rice", &rice_message(entry.path())),
                            "{} has no English name",
                            entry.path()
                        );
                    }
                    if let SettingsItem::Keys(id) = item {
                        let map = engine.catalog.keymap(id).expect("its key map");
                        for command in map.commands() {
                            assert!(!command.label.trim().is_empty(), "{}", command.command);
                            assert_ne!(
                                command.label, command.command,
                                "{} reads as a name",
                                command.command
                            );
                        }
                    }
                }
            }
        }
        // The two studios' shortcuts are grouped as their menus are.
        let paint = engine
            .catalog
            .keymap(sue_paint::ID)
            .expect("Sue Paint's keys");
        let titles: Vec<&str> = paint
            .groups
            .iter()
            .map(|group| group.title.as_str())
            .collect();
        assert_eq!(titles.first(), Some(&"File"), "{titles:?}");
        assert!(
            titles.contains(&"Edit") && titles.contains(&"Tools"),
            "{titles:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_held_twice_is_marked_on_both_rows_and_can_be_taken_back() {
        let (engine, _, root) = booted();
        let mut rice = engine.world.rice.clone();
        let map = engine
            .catalog
            .keymap(sue_paint::ID)
            .expect("Sue Paint's keys")
            .clone();
        let before = keys::resolve(&rice, &map);
        let r: sue_rice::Keybind = "R".parse().unwrap();
        assert!(keys::others(&before, "brush.bigger", &r).is_empty());
        // Bigger Brush on R as well as ]: both rows say who else holds it.
        let (path, value) = sue_wm::shortcuts::bind(
            &rice,
            &map.section,
            "brush.bigger",
            &"] or R".parse().unwrap(),
        );
        rice.set(&path, value).unwrap();
        let now = keys::resolve(&rice, &map);
        assert_eq!(
            keys::others(&now, "brush.bigger", &r),
            [keys::Holder::Command("tool.rotate".to_owned())]
        );
        assert_eq!(
            keys::others(&now, "tool.rotate", &r),
            [keys::Holder::Command("brush.bigger".to_owned())]
        );
        // The offer takes R from Rotate and leaves Bigger Brush whole.
        let other = keys::Holder::Command("tool.rotate".to_owned());
        let Some(Edit::Set(path, value)) = keys::take_from(&rice, &map, &now, &other, &r) else {
            panic!("an edit");
        };
        rice.set(&path, value).unwrap();
        let after = keys::resolve(&rice, &map);
        assert!(keys::others(&after, "brush.bigger", &r).is_empty());
        let chords = |name: &str| {
            after
                .rows
                .iter()
                .find(|(command, _)| command == name)
                .map(|(_, chords)| chords.to_string())
        };
        assert_eq!(chords("tool.rotate").as_deref(), Some(""));
        assert_eq!(chords("brush.bigger").as_deref(), Some("] or R"));
        // A command that ships with no key can be given one, and its row
        // is marked like any other.
        let unkeyed = map
            .commands()
            .find(|command| command.defaults.is_empty())
            .expect("a command without a key")
            .command
            .clone();
        let (path, value) =
            sue_wm::shortcuts::bind(&rice, &map.section, &unkeyed, &"]".parse().unwrap());
        rice.set(&path, value).unwrap();
        let bracket: sue_rice::Keybind = "]".parse().unwrap();
        assert_eq!(
            keys::others(&keys::resolve(&rice, &map), &unkeyed, &bracket),
            [keys::Holder::Command("brush.bigger".to_owned())]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_search_finds_a_command_by_its_name_or_its_key() {
        let keys: sue_rice::Keybinds = "Ctrl+E".parse().unwrap();
        assert!(keys::finds(
            "merge",
            "Layer",
            "Merge with Layer Below",
            "layer.merge_down",
            &keys
        ));
        assert!(keys::finds(
            "layer.merge",
            "Layer",
            "Merge",
            "layer.merge_down",
            &keys
        ));
        assert!(keys::finds(
            "ctrl+e",
            "Layer",
            "Merge",
            "layer.merge_down",
            &keys
        ));
        assert!(!keys::finds(
            "ctrl",
            "Layer",
            "Merge",
            "layer.merge_down",
            &keys
        ));
        assert!(keys::finds("", "Layer", "Merge", "layer.merge_down", &keys));
    }

    #[test]
    fn clip_studio_names_find_sue_paints_settings() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let (engine, pages, root) = booted();
        let words: Vec<(String, String)> = pages
            .iter()
            .flat_map(|page| page.search.iter().cloned())
            .collect();
        let cursor = engine
            .world
            .rice
            .entry("sue_paint.brush_cursor")
            .expect("a setting");
        assert!(matches(cursor, "brush-type cursor", &words));
        assert!(!matches(cursor, "brush-type cursor", &[]));
        let _ = std::fs::remove_dir_all(&root);
    }
}
