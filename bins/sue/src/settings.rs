//! Settings: Sue's preferences as a Center Stage tab. A category sidebar on
//! the left; on the right only the chosen category's controls, under a live
//! filter. Pages are registered data from the engine's catalog; any rice
//! section no page claims is listed on a final "Other" page, so every
//! parameter stays reachable (LAW 3). Every edit is an undoable rice
//! transaction; a slider drag previews live and records one undo step.

use sue_platform::{CursorShape, InputEvent, Key, NamedKey, Point};
use sue_rice::{Keybind, RiceEntry, RiceParameter, RiceSchema, RiceValue, SetRiceValue};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, Catalog, Color,
    CommandButton, EditOutcome, FontFace, InfoTable, InputResponse, LineEdit, PropertyGroup, Rect,
    SettingsGroup, SettingsItem, SettingsPage, ShellRequest, SliderEvent, SueApplet, Ui, UiChoice,
    World, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "settings";

/// Which text input has the keyboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Field {
    #[default]
    None,
    Filter,
    Value {
        path: String,
        edit: LineEdit,
    },
}

/// The Preferences tab.
#[derive(Debug, Default)]
pub struct Settings {
    page: usize,
    pending: Option<String>,
    filter: LineEdit,
    field: Field,
    recording: Option<String>,
    drag: Option<(String, RiceValue)>,
    error: Option<String>,
    scroll: f32,
    hovered: Option<String>,
    page_title: String,
}

/// `high-refresh` reads `High refresh`; numbers stay as they are.
fn choice_label(value: &str) -> String {
    let words = value.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The registered pages plus "Other" for unclaimed sections.
fn all_pages(catalog: &Catalog, rice: &RiceSchema) -> Vec<SettingsPage> {
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

/// How a choice parameter is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChoiceLayout {
    /// A segmented control in the control column.
    Inline,
    /// A segmented control on its own line under the description.
    Wide,
    /// A compact drop-down.
    Dropdown,
}

/// The options of a choice parameter (live choices from the catalog, or an
/// enum), the values they store, and the selected index.
fn choice_options(
    entry: &RiceEntry,
    catalog: &Catalog,
    rice: &RiceSchema,
) -> Option<(Vec<UiChoice>, Vec<String>, Option<usize>)> {
    if let Some(choices) = catalog.choices.get(entry.path()) {
        let current = rice.get_str(entry.path()).unwrap_or("");
        return Some((
            choices
                .iter()
                .map(|choice| UiChoice {
                    label: choice.label.clone(),
                    enabled: choice.enabled,
                })
                .collect(),
            choices.iter().map(|choice| choice.value.clone()).collect(),
            choices.iter().position(|choice| choice.value == current),
        ));
    }
    match entry.parameter() {
        RiceParameter::EnumChoice { value, options } => Some((
            options
                .iter()
                .map(|option| UiChoice::new(choice_label(option)))
                .collect(),
            options.clone(),
            options.iter().position(|option| option == value),
        )),
        _ => None,
    }
}

/// Segmented when two to six options fit (beside the label, else on a line
/// of their own), a drop-down otherwise.
fn choice_layout(ui: &Ui<'_>, options: &[UiChoice], control: f32, wide: f32) -> ChoiceLayout {
    if !(2..=6).contains(&options.len()) {
        ChoiceLayout::Dropdown
    } else if ui.segmented_width(options, control).is_some() {
        ChoiceLayout::Inline
    } else if ui.segmented_width(options, wide).is_some() {
        ChoiceLayout::Wide
    } else {
        ChoiceLayout::Dropdown
    }
}

/// What one row of the right pane shows.
enum Row<'a> {
    Heading(String),
    Param(&'a RiceEntry),
    Table(&'a InfoTable),
    Presets,
    Actions(&'a [CommandButton]),
    Fact(&'a str, &'a str),
}

fn slider_value(value: f64, integer: bool) -> RiceValue {
    if integer {
        RiceValue::Int(value.round() as i64)
    } else {
        RiceValue::Float((value * 100.0).round() / 100.0)
    }
}

fn value_text(value: f64, integer: bool) -> String {
    if integer {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

impl Settings {
    fn commit(&mut self, ctx: &mut AppletContext<'_>, path: &str, value: RiceValue) {
        match ctx.edit_rice(SetRiceValue::new(path, value)) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// The value typed into a field, typed like the parameter it edits.
    fn typed_value(rice: &RiceSchema, path: &str, text: &str) -> Result<RiceValue, String> {
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
            Some(RiceParameter::ColorHex { .. }) => Ok(RiceValue::Text(text.trim().to_owned())),
            _ => Ok(RiceValue::Text(text.to_owned())),
        }
    }

    fn editing(&self, path: &str) -> Option<&LineEdit> {
        match &self.field {
            Field::Value {
                path: editing,
                edit,
            } if editing == path => Some(edit),
            _ => None,
        }
    }

    /// The rows of the right pane: the page, or search results everywhere.
    fn rows<'a>(
        &self,
        pages: &'a [SettingsPage],
        catalog: &'a Catalog,
        rice: &'a RiceSchema,
    ) -> Vec<Row<'a>> {
        let needle = self.filter.text.trim().to_lowercase();
        let mut rows = Vec::new();
        if needle.is_empty() {
            let Some(page) = pages.get(self.page) else {
                return rows;
            };
            for group in &page.groups {
                rows.push(Row::Heading(group.title.to_uppercase()));
                for item in &group.items {
                    match item {
                        SettingsItem::Param(_) | SettingsItem::Section(_) => {
                            rows.extend(entries_of(rice, item).into_iter().map(Row::Param));
                        }
                        SettingsItem::Table(id) => {
                            if let Some(table) = catalog.tables.get(id) {
                                rows.push(Row::Table(table));
                            }
                        }
                        SettingsItem::Presets => rows.push(Row::Presets),
                        SettingsItem::Actions(buttons) => rows.push(Row::Actions(buttons)),
                        SettingsItem::Fact(id) => {
                            if let Some((label, value)) = catalog.facts.get(id) {
                                rows.push(Row::Fact(label, value));
                            }
                        }
                    }
                }
            }
        } else {
            for page in pages {
                let found: Vec<&RiceEntry> = page
                    .claims()
                    .flat_map(|item| entries_of(rice, item))
                    .filter(|entry| matches(entry, &needle))
                    .collect();
                if !found.is_empty() {
                    rows.push(Row::Heading(page.title.to_uppercase()));
                    rows.extend(found.into_iter().map(Row::Param));
                }
            }
        }
        rows
    }

    /// Draws one parameter's control in `control`; returns nothing, edits
    /// go straight to the undo tree.
    #[allow(clippy::too_many_lines, reason = "one arm per parameter type")]
    fn control(
        &mut self,
        ui: &mut Ui<'_>,
        entry: &RiceEntry,
        control: Rect,
        ctx: &mut AppletContext<'_>,
    ) {
        let theme = ui.theme;
        let pad = theme.padding;
        let path = entry.path().to_owned();
        let id = widget_id("settings", &path);
        let row = ui.row_height();
        let field = Rect::new(control.x, control.y, control.width, row);
        if let Some((options, values, selected)) = choice_options(entry, ctx.catalog, ctx.rice()) {
            let picked = match choice_layout(ui, &options, control.width, control.width) {
                ChoiceLayout::Inline => {
                    let width = ui
                        .segmented_width(&options, control.width)
                        .unwrap_or(control.width);
                    let rect = Rect::new(control.right() - width, control.y, width, row);
                    ui.segmented(id, rect, &options, selected)
                }
                ChoiceLayout::Wide => None,
                ChoiceLayout::Dropdown => ui.dropdown(id, field, &options, selected),
            };
            if let Some(picked) = picked {
                self.commit(ctx, &path, RiceValue::Text(values[picked].clone()));
            }
            return;
        }
        match entry.parameter().clone() {
            RiceParameter::Bool { value } => {
                let switch = Rect::new(control.right() - row * 1.8, control.y, row * 1.8, row);
                if ui.toggle(id, switch, value) {
                    self.commit(ctx, &path, RiceValue::Bool(!value));
                }
            }
            RiceParameter::IntRange {
                value,
                min: Some(min),
                max: Some(max),
            } => self.slider(
                ui,
                ctx,
                &path,
                control,
                value as f64,
                min as f64,
                max as f64,
                true,
            ),
            RiceParameter::FloatRange {
                value,
                min: Some(min),
                max: Some(max),
            } => self.slider(ui, ctx, &path, control, value, min, max, false),
            RiceParameter::IntRange { value, .. } => {
                self.text_input(ui, &path, field, &value.to_string(), FontFace::Mono);
            }
            RiceParameter::FloatRange { value, .. } => {
                self.text_input(ui, &path, field, &value_text(value, false), FontFace::Mono);
            }
            RiceParameter::ColorHex { value } => {
                let swatch = Rect::new(control.x, control.y, row * 1.6, row);
                if ui.swatch(
                    id,
                    swatch,
                    Color::from_rgba(value),
                    self.editing(&path).is_some(),
                ) {
                    self.field = Field::Value {
                        path: path.clone(),
                        edit: LineEdit::new(value.to_hex()),
                    };
                }
                let hex = Rect::new(
                    swatch.right() + pad / 2.0,
                    control.y,
                    control.width - swatch.width - pad / 2.0,
                    row,
                );
                self.text_input(ui, &path, hex, &value.to_hex(), FontFace::Mono);
            }
            // Choices are drawn above, from `choice_options`.
            RiceParameter::EnumChoice { .. } => {}
            RiceParameter::Keybind { value } => {
                let recording = self.recording.as_deref() == Some(path.as_str());
                let text = if recording {
                    "Press keys, Esc to cancel".to_owned()
                } else {
                    value.to_string()
                };
                // A key cap: as wide as its chord, at the right edge.
                let width = (ui.fonts.measure(FontFace::Mono, theme.mono_size, &text) + pad * 2.0)
                    .min(control.width);
                let cap = Rect::new(control.right() - width, control.y, width, row);
                let response = ui.interact(id, cap);
                ui.canvas.draw_box(
                    cap,
                    sue_wm::BoxStyle {
                        fill: if response.hovered {
                            theme.hover
                        } else {
                            theme.surface
                        },
                        border: if recording {
                            theme.accent
                        } else {
                            theme.border
                        },
                        border_width: theme.hairline(),
                        radius: theme.radius,
                    },
                );
                let height = ui.fonts.line_height(FontFace::Mono, theme.mono_size);
                ui.text(
                    Point::new(cap.x + pad, cap.y + (row - height) / 2.0),
                    &text,
                    FontFace::Mono,
                    theme.mono_size,
                    if recording { theme.accent } else { theme.text },
                );
                if response.hovered {
                    ui.set_cursor(CursorShape::Pointer);
                }
                ui.tooltip(response, cap, "Click, then press the new shortcut");
                if response.clicked {
                    self.recording = Some(path.clone());
                    self.field = Field::None;
                }
            }
            RiceParameter::ShaderPath { value } | RiceParameter::Text { value } => {
                self.text_input(ui, &path, field, &value, FontFace::Ui);
            }
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "a slider row has its range and type"
    )]
    fn slider(
        &mut self,
        ui: &mut Ui<'_>,
        ctx: &mut AppletContext<'_>,
        path: &str,
        control: Rect,
        value: f64,
        min: f64,
        max: f64,
        integer: bool,
    ) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        let value_width = ui
            .fonts
            .measure(FontFace::Mono, theme.mono_size, "00000.00")
            + pad;
        let track = Rect::new(control.x, control.y, control.width - value_width - pad, row);
        let number = Rect::new(control.right() - value_width, control.y, value_width, row);
        match ui.slider(widget_id("settings", path), track, value, min, max) {
            SliderEvent::Preview(new) => {
                if self
                    .drag
                    .as_ref()
                    .is_none_or(|(dragging, _)| dragging != path)
                    && let Some(start) = ctx.rice().get(path)
                {
                    self.drag = Some((path.to_owned(), start));
                }
                let _ = ctx.world.rice.set(path, slider_value(new, integer));
            }
            SliderEvent::Commit(new) => {
                if let Some((dragging, start)) = self.drag.take()
                    && dragging == path
                {
                    let _ = ctx.world.rice.set(&dragging, start);
                }
                self.commit(ctx, path, slider_value(new, integer));
            }
            SliderEvent::Idle => {}
        }
        if let Some(edit) = self.editing(path).cloned() {
            ui.text_field(
                widget_id("settings/number", path),
                number,
                &edit.text,
                Some(edit.caret),
                "",
                FontFace::Mono,
            );
        } else {
            let response = ui.interact(widget_id("settings/number", path), number);
            if response.hovered {
                ui.set_cursor(CursorShape::Text);
            }
            ui.label_right(
                number,
                &value_text(value, integer),
                theme.mono_size,
                if response.hovered {
                    theme.text
                } else {
                    theme.muted
                },
            );
            ui.tooltip(response, number, "Click to type a value");
            if response.clicked {
                self.field = Field::Value {
                    path: path.to_owned(),
                    edit: LineEdit::new(value_text(value, integer)),
                };
            }
        }
    }

    fn text_input(
        &mut self,
        ui: &mut Ui<'_>,
        path: &str,
        rect: Rect,
        current: &str,
        face: FontFace,
    ) {
        let editing = self.editing(path).cloned();
        let shown = editing.as_ref().map_or(current, |edit| edit.text.as_str());
        let response = ui.text_field(
            widget_id("settings/field", path),
            rect,
            shown,
            editing.as_ref().map(|edit| edit.caret),
            "Empty",
            face,
        );
        if response.clicked && editing.is_none() {
            self.field = Field::Value {
                path: path.to_owned(),
                edit: LineEdit::new(current),
            };
        }
    }

    /// Height of a parameter row whose text column is `width` wide.
    fn param_height(ui: &Ui<'_>, entry: &RiceEntry, width: f32) -> f32 {
        let theme = ui.theme;
        let description = entry
            .description()
            .map_or(0.0, |text| ui.wrapped_height(text, width, theme.small_size));
        (ui.line_height() + description).max(ui.row_height()) + theme.padding * 1.5
    }

    fn draw_table(ui: &mut Ui<'_>, table: &InfoTable, x: f32, y: f32, width: f32) -> f32 {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        if table.rows.is_empty() {
            return ui.wrapped(x, y, width, &table.empty, theme.small_size, theme.muted) + pad;
        }
        let columns = table.columns.len().max(1);
        let column_width = width / columns as f32;
        for (column, title) in table.columns.iter().enumerate() {
            ui.label_sized(
                Rect::new(x + column as f32 * column_width, y, column_width - pad, row),
                title,
                theme.small_size,
                theme.muted,
            );
        }
        let mut top = y + row;
        for cells in &table.rows {
            ui.hline(x, top, width);
            let height = cells
                .iter()
                .map(|cell| ui.wrapped_height(cell, column_width - pad, theme.ui_size))
                .fold(ui.line_height(), f32::max)
                + pad;
            for (column, cell) in cells.iter().enumerate() {
                ui.wrapped(
                    x + column as f32 * column_width,
                    top + pad / 2.0,
                    column_width - pad,
                    cell,
                    theme.ui_size,
                    if column == 0 { theme.text } else { theme.muted },
                );
            }
            top += height;
        }
        top - y + pad
    }
}

impl SueApplet for Settings {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Settings id is valid"),
            name: "Settings".to_owned(),
            category: AppletCategory::System,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Every preference, grouped by category, with search.".to_owned(),
            opens: Vec::new(),
            icon: "\u{2699}".to_owned(),
            singleton: true,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn navigate(&mut self, target: &str) -> bool {
        self.pending = Some(target.to_owned());
        self.filter = LineEdit::default();
        true
    }

    fn title(&self, _world: &World) -> Option<String> {
        Some("Settings".to_owned())
    }

    fn captures_keys(&self) -> bool {
        self.recording.is_some()
    }

    fn inspect(&self, world: &World) -> Vec<PropertyGroup> {
        let mut groups = vec![PropertyGroup::new(
            "Settings",
            [
                ("Page", self.page_title.clone()),
                ("Parameters", world.rice.len().to_string()),
            ],
        )];
        if let Some(entry) = self
            .hovered
            .as_deref()
            .and_then(|path| world.rice.entry(path))
        {
            let range = match entry.parameter() {
                RiceParameter::IntRange {
                    min: Some(min),
                    max: Some(max),
                    ..
                } => format!("{min} to {max}"),
                RiceParameter::FloatRange {
                    min: Some(min),
                    max: Some(max),
                    ..
                } => format!("{min} to {max}"),
                RiceParameter::EnumChoice { options, .. } => options.join(", "),
                _ => "Any".to_owned(),
            };
            groups.push(PropertyGroup::new(
                "Parameter",
                [
                    ("Key", entry.path().to_owned()),
                    ("Type", entry.parameter().type_name().to_owned()),
                    ("Value", entry.parameter().value().to_string()),
                    ("Accepts", range),
                ],
            ));
        }
        groups
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let InputEvent::Keyboard(key) = event else {
            return Ok(InputResponse::Ignored);
        };
        if !key.pressed {
            return Ok(InputResponse::Ignored);
        }
        let done = InputResponse::Consumed { redraw: true };
        if let Some(path) = self.recording.clone() {
            self.recording = None;
            if key.key != Key::Named(NamedKey::Escape) {
                let chord = Keybind::new(key.modifiers, key.key.normalized());
                self.commit(ctx, &path, RiceValue::Keybind(chord));
            }
            return Ok(done);
        }
        match std::mem::take(&mut self.field) {
            Field::Filter => {
                match self.filter.handle(key) {
                    EditOutcome::Cancel => {
                        self.filter = LineEdit::default();
                        return Ok(done);
                    }
                    EditOutcome::Ignored => {
                        self.field = Field::Filter;
                        return Ok(InputResponse::Ignored);
                    }
                    EditOutcome::Changed => self.scroll = 0.0,
                    EditOutcome::Submit | EditOutcome::Moved => {}
                }
                self.field = Field::Filter;
                Ok(done)
            }
            Field::Value { path, mut edit } => {
                match edit.handle(key) {
                    EditOutcome::Submit => match Self::typed_value(ctx.rice(), &path, &edit.text) {
                        Ok(value) => self.commit(ctx, &path, value),
                        Err(reason) => {
                            self.error = Some(reason);
                            self.field = Field::Value { path, edit };
                        }
                    },
                    EditOutcome::Cancel => {}
                    EditOutcome::Ignored => {
                        self.field = Field::Value { path, edit };
                        return Ok(InputResponse::Ignored);
                    }
                    EditOutcome::Changed | EditOutcome::Moved => {
                        self.field = Field::Value { path, edit };
                    }
                }
                Ok(done)
            }
            Field::None => match &key.text {
                // Typing anywhere starts a search, as in most preference windows.
                Some(text)
                    if !key.modifiers.is_command()
                        && !text.trim().is_empty()
                        && !text.chars().any(char::is_control) =>
                {
                    self.filter.handle(key);
                    self.field = Field::Filter;
                    self.scroll = 0.0;
                    Ok(done)
                }
                _ => Ok(InputResponse::Ignored),
            },
        }
    }

    #[allow(clippy::too_many_lines, reason = "sidebar, search and the rows")]
    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        if !ctx.focused {
            self.field = Field::None;
            self.recording = None;
        }
        let catalog = ctx.catalog;
        let pages = all_pages(catalog, ctx.rice());
        if let Some(target) = self.pending.take()
            && let Some(index) = pages.iter().position(|page| page.id == target)
        {
            self.page = index;
            self.scroll = 0.0;
        }
        self.page = self.page.min(pages.len().saturating_sub(1));
        self.page_title = pages
            .get(self.page)
            .map_or_else(String::new, |page| page.title.clone());

        // Sidebar.
        let sidebar = Rect::new(
            area.x,
            area.y,
            theme.settings_sidebar_width.min(area.width / 2.0),
            area.height,
        );
        ui.panel(sidebar);
        ui.vline(
            sidebar.right() - theme.hairline(),
            sidebar.y,
            sidebar.height,
        );
        let needle = self.filter.text.trim().to_lowercase();
        let mut y = sidebar.y + pad;
        for (index, page) in pages.iter().enumerate() {
            let item = Rect::new(sidebar.x + pad / 2.0, y, sidebar.width - pad, row);
            let response = ui.interact(widget_id("settings/page", &page.id), item);
            let selected = index == self.page && needle.is_empty();
            let hits = if needle.is_empty() {
                None
            } else {
                Some(
                    page.claims()
                        .flat_map(|claim| entries_of(ctx.rice(), claim))
                        .filter(|entry| matches(entry, &needle))
                        .count(),
                )
            };
            if selected {
                ui.canvas
                    .draw_box(item, sue_wm::BoxStyle::filled(theme.surface, theme.radius));
                ui.fill(
                    Rect::new(
                        item.x,
                        item.y + pad / 2.0,
                        theme.tab_accent.max(1.0),
                        item.height - pad,
                    ),
                    theme.accent,
                );
            } else if response.hovered {
                ui.canvas
                    .draw_box(item, sue_wm::BoxStyle::filled(theme.hover, theme.radius));
            }
            if response.hovered {
                ui.set_cursor(CursorShape::Pointer);
            }
            let color = if selected {
                theme.text
            } else if hits == Some(0) {
                theme.muted.fade(0.6)
            } else {
                theme.muted.mix(theme.text, 0.35)
            };
            ui.label(
                Rect::new(item.x + pad, item.y, item.width - pad * 3.0, row),
                &page.title,
                color,
            );
            if let Some(hits) = hits.filter(|hits| *hits > 0) {
                ui.label_right(
                    Rect::new(item.x, item.y, item.width - pad, row),
                    &hits.to_string(),
                    theme.small_size,
                    theme.accent,
                );
            }
            if response.clicked {
                self.page = index;
                self.filter = LineEdit::default();
                self.field = Field::None;
                self.scroll = 0.0;
            }
            y += row + 2.0;
        }

        // Main pane: the filter on top, then the rows.
        let margin = pad * 3.0;
        let pane = Rect::new(
            sidebar.right() + margin,
            area.y + pad * 2.0,
            (area.right() - sidebar.right() - margin * 2.0).max(1.0),
            area.height - pad * 2.0,
        );
        let filter_rect = Rect::new(pane.x, pane.y, pane.width, row);
        let filtering = self.field == Field::Filter;
        let response = ui.text_field(
            widget_id("settings", "filter"),
            filter_rect,
            &self.filter.text,
            filtering.then_some(self.filter.caret),
            "Filter settings...",
            FontFace::Ui,
        );
        if response.clicked {
            self.field = Field::Filter;
            self.recording = None;
        }
        let title = if needle.is_empty() {
            self.page_title.clone()
        } else {
            format!("Results for \"{}\"", self.filter.text.trim())
        };
        let title_y = filter_rect.bottom() + pad * 2.0;
        ui.text(
            Point::new(pane.x, title_y),
            &title,
            FontFace::Ui,
            theme.heading_size,
            theme.text,
        );
        let mut top = title_y + ui.fonts.line_height(FontFace::Ui, theme.heading_size) + pad;
        if let Some(error) = &self.error {
            top += ui.wrapped(
                pane.x,
                top,
                pane.width,
                error,
                theme.small_size,
                theme.alert,
            ) + pad / 2.0;
        }
        let list = Rect::new(pane.x, top, pane.width, (pane.bottom() - top).max(0.0));
        let rice = ctx.world.rice.clone();
        let rows = self.rows(&pages, catalog, &rice);
        if rows.is_empty() {
            ui.label(
                Rect::new(list.x, list.y, list.width, row),
                "Nothing matches. Try another word.",
                theme.muted,
            );
            return;
        }
        let control_width = (list.width * 0.42)
            .clamp(200.0, 340.0)
            .min(list.width * 0.6);
        let text_width = (list.width - control_width - pad * 2.0).max(1.0);
        ui.canvas.push_clip(list);
        let mut y = list.y - self.scroll;
        self.hovered = None;
        for (index, item) in rows.iter().enumerate() {
            match item {
                Row::Heading(heading) => {
                    if index > 0 {
                        y += pad;
                    }
                    ui.label_sized(
                        Rect::new(list.x, y, list.width, row),
                        heading,
                        theme.small_size,
                        theme.accent,
                    );
                    y += row;
                }
                Row::Param(entry) => {
                    let keybind = matches!(entry.parameter(), RiceParameter::Keybind { .. });
                    let wide = choice_options(entry, catalog, &rice).filter(|(options, ..)| {
                        choice_layout(ui, options, control_width, list.width) == ChoiceLayout::Wide
                    });
                    let mut height = Self::param_height(ui, entry, text_width);
                    if wide.is_some() {
                        height += row + pad / 2.0;
                    }
                    let visible = y + height >= list.y && y <= list.bottom();
                    if visible {
                        let rect = Rect::new(list.x, y, list.width, height);
                        if ui
                            .input
                            .pointer
                            .is_some_and(|point| rect.contains(point.x, point.y))
                        {
                            self.hovered = Some(entry.path().to_owned());
                        }
                        let label = if keybind {
                            entry
                                .description()
                                .map_or_else(|| entry.label(), str::to_owned)
                        } else {
                            entry.label()
                        };
                        ui.text(
                            Point::new(list.x, y + pad / 2.0),
                            &label,
                            FontFace::Ui,
                            theme.ui_size,
                            theme.text,
                        );
                        if !keybind && let Some(description) = entry.description() {
                            ui.wrapped(
                                list.x,
                                y + pad / 2.0 + ui.line_height(),
                                text_width,
                                description,
                                theme.small_size,
                                theme.muted,
                            );
                        }
                        let control = Rect::new(
                            list.right() - control_width,
                            y + pad / 2.0,
                            control_width,
                            height - pad,
                        );
                        if let Some((options, values, selected)) = &wide {
                            let width = ui
                                .segmented_width(options, list.width)
                                .unwrap_or(list.width);
                            let rect = Rect::new(list.x, y + height - row - pad, width, row);
                            let id = widget_id("settings", entry.path());
                            if let Some(picked) = ui.segmented(id, rect, options, *selected) {
                                self.commit(
                                    ctx,
                                    entry.path(),
                                    RiceValue::Text(values[picked].clone()),
                                );
                            }
                        } else {
                            self.control(ui, entry, control, ctx);
                        }
                        ui.hline(list.x, y + height - theme.hairline(), list.width);
                    }
                    y += height;
                }
                Row::Table(table) => {
                    y += Self::draw_table(ui, table, list.x, y + pad / 2.0, list.width);
                }
                Row::Presets => {
                    let presets = &catalog.presets;
                    let options: Vec<UiChoice> = presets
                        .iter()
                        .map(|preset| UiChoice::new(preset.name.clone()))
                        .collect();
                    let selected = presets
                        .iter()
                        .position(|preset| preset.is_active(ctx.rice()));
                    let width = ui
                        .segmented_width(&options, list.width)
                        .unwrap_or(list.width)
                        .max(control_width);
                    let rect = Rect::new(list.x, y + pad / 2.0, width, row);
                    if let Some(picked) =
                        ui.segmented(widget_id("settings", "presets"), rect, &options, selected)
                    {
                        let txn = presets[picked].transaction(ctx.rice());
                        if let Err(error) = ctx.edit_rice(txn) {
                            self.error = Some(error.to_string());
                        }
                    }
                    let note = if selected.is_none() {
                        "Custom colors. Pick a preset to replace them; Ctrl+Z brings yours back."
                    } else {
                        "Presets replace the colors below as one step you can undo."
                    };
                    let note_y = rect.bottom() + pad / 2.0;
                    y = note_y
                        + ui.wrapped(
                            list.x,
                            note_y,
                            list.width,
                            note,
                            theme.small_size,
                            theme.muted,
                        )
                        + pad;
                }
                Row::Actions(buttons) => {
                    let mut x = list.x;
                    for button in *buttons {
                        let width = ui.fonts.measure(FontFace::Ui, theme.ui_size, &button.label)
                            + pad * 3.0;
                        let rect = Rect::new(x, y + pad / 2.0, width, row);
                        if ui.button(
                            widget_id("settings/action", &button.command),
                            rect,
                            &button.label,
                        ) {
                            ctx.request(ShellRequest::Command(button.command.clone()));
                        }
                        x += width + pad / 2.0;
                    }
                    y += row + pad * 1.5;
                }
                Row::Fact(label, value) => {
                    ui.text(
                        Point::new(list.x, y + pad / 2.0),
                        label,
                        FontFace::Ui,
                        theme.ui_size,
                        theme.text,
                    );
                    let value_x = list.right() - control_width;
                    let height = ui.wrapped(
                        value_x,
                        y + pad / 2.0,
                        control_width,
                        value,
                        theme.small_size,
                        theme.muted,
                    );
                    let height = height.max(ui.line_height()) + pad * 1.5;
                    ui.hline(list.x, y + height - theme.hairline(), list.width);
                    y += height;
                }
            }
        }
        ui.canvas.pop_clip();
        let content = y + self.scroll - list.y;
        self.scroll =
            (self.scroll - ui.wheel_in(list)).clamp(0.0, (content - list.height + pad).max(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_rice::DEFAULT_RICE;
    use sue_wm::SettingsGroup;

    #[test]
    fn choices_read_as_words() {
        assert_eq!(choice_label("high-refresh"), "High refresh");
        assert_eq!(choice_label("48000"), "48000");
        assert_eq!(choice_label("auto"), "Auto");
    }

    #[test]
    fn unclaimed_sections_land_on_other_and_search_spans_pages() {
        let rice = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let catalog = Catalog {
            pages: vec![SettingsPage::new(
                "look",
                "Look",
                vec![SettingsGroup::new(
                    "Colors",
                    vec![SettingsItem::Section("colors".into())],
                )],
            )],
            ..Catalog::default()
        };
        let pages = all_pages(&catalog, &rice);
        assert_eq!(pages.last().unwrap().title, "Other");
        let other: Vec<&SettingsItem> = pages.last().unwrap().claims().collect();
        assert!(other.contains(&&SettingsItem::Section("keys".into())));
        assert!(!other.contains(&&SettingsItem::Section("colors".into())));

        let mut settings = Settings {
            filter: LineEdit::new("ACCENT"),
            ..Settings::default()
        };
        let found = settings.rows(&pages, &catalog, &rice);
        let paths: Vec<&str> = found
            .iter()
            .filter_map(|row| match row {
                Row::Param(entry) => Some(entry.path()),
                _ => None,
            })
            .collect();
        assert!(paths.contains(&"colors.accent"));
        assert!(paths.contains(&"layout.tab_accent"), "{paths:?}");
        settings.filter = LineEdit::default();
        assert!(matches!(
            settings.rows(&pages, &catalog, &rice)[0],
            Row::Heading(_)
        ));
    }

    #[test]
    fn typed_values_follow_the_parameter_type() {
        let rice = RiceSchema::parse(DEFAULT_RICE).unwrap();
        assert_eq!(
            Settings::typed_value(&rice, "layout.gap", " 4 "),
            Ok(RiceValue::Int(4))
        );
        assert!(Settings::typed_value(&rice, "layout.gap", "four").is_err());
        assert_eq!(
            Settings::typed_value(&rice, "display.ui_scale", "1.25"),
            Ok(RiceValue::Float(1.25))
        );
        assert_eq!(slider_value(7.6, true), RiceValue::Int(8));
        assert_eq!(value_text(170.0, false), "170.00");
    }
}
