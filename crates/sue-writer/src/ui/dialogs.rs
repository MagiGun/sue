//! The dialogs of the studio and the statistics.

use egui::{Align2, Id, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_core::Value;
use sue_io::text_ops::Endings;
use sue_model::manuscript;
use sue_model::mark_kind as kind;
use sue_wm::ui::{self, Roving, Space};
use sue_wm::{AppletContext, ShellRequest, Theme, widgets};

use crate::compile::{FORMATS, Separator, Titles};
use crate::project;
use crate::studio::{Dialog, SueWriter};
use crate::templates::{Group, Template};

use super::After;
use super::settings::furniture_ui;
use super::status::progress;

impl SueWriter {
    #[allow(clippy::too_many_lines, reason = "every dialog of the studio")]
    pub(super) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut after: Option<After> = None;
        let keys =
            matches!(dialog, Dialog::Shortcuts).then(|| crate::commands::shortcut_rows(cx.rice()));
        let title = match &dialog {
            Dialog::NewProject { .. } => tr!("new-project"),
            Dialog::Compile(_) => tr!("export"),
            Dialog::Settings(_) => tr!("project-settings"),
            Dialog::Link(_) => tr!("link"),
            Dialog::Footnote(_) => tr!("footnote"),
            Dialog::Endnote(_) => tr!("endnote"),
            Dialog::Table { .. } => tr!("insert-table"),
            Dialog::Furniture(_) => tr!("header-and-footer"),
            Dialog::Contents(_) => tr!("table-of-contents"),
            Dialog::Comment(_) => tr!("new-comment"),
            Dialog::Statistics => tr!("writing-statistics"),
            Dialog::Shortcuts => tr!("keyboard-shortcuts"),
            Dialog::TableFile { insert: true, .. } => tr!("table-from-file"),
            Dialog::TableFile { .. } => tr!("open-table"),
            Dialog::About => tr!("about-sue-writer"),
        };
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let id = Id::unique("sue-writer-dialog");
        egui::Modal::new(id)
            .area(egui::Modal::default_area(id).fade_in(false))
            .backdrop_color(theme.shadow.gamma_multiply(0.45))
            .frame(widgets::window_frame(theme).inner_margin(16))
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(title)
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(8.0);
                match &mut dialog {
                    Dialog::NewProject { template, title } => {
                        let templates = &self.templates;
                        for one in templates.group(Group::Template) {
                            ui.radio_value(template, one.key.clone(), RichText::new(one.name()))
                                .on_hover_text(one.about());
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(tr!("presets"));
                            let shown = templates
                                .get(template)
                                .filter(|one| one.group == Group::Preset)
                                .map_or_else(|| tr!("none"), Template::name);
                            egui::ComboBox::from_id_salt("writer-new-preset")
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    for one in templates.group(Group::Preset) {
                                        ui.selectable_value(template, one.key.clone(), one.name())
                                            .on_hover_text(one.about());
                                    }
                                });
                        });
                        if let Some(one) = templates.get(template) {
                            ui.label(
                                RichText::new(one.about())
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(tr!("title"));
                            ui.text_edit_singleline(title);
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr!("create")).clicked() {
                                let key = template.clone();
                                let title = if title.trim().is_empty() {
                                    tr!("untitled")
                                } else {
                                    title.trim().to_owned()
                                };
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.new_project(cx, &key, &title)
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Compile(choices) => {
                        widgets::heading(ui, &tr!("format"), theme);
                        // Two columns of formats, each with its extension,
                        // so the whole dialog fits a laptop's window.
                        let mut formats = Vec::with_capacity(FORMATS.len());
                        let mut format = None;
                        egui::Grid::new("writer-formats")
                            .num_columns(4)
                            .spacing([theme.space(Space::S6), theme.space(Space::S1)])
                            .show(ui, |ui| {
                                for (index, (key, _, ext)) in FORMATS.iter().enumerate() {
                                    let name = crate::compile::format_name(key);
                                    let response = ui::radio(
                                        ("writer-format", index),
                                        &name,
                                        choices.format == index,
                                    )
                                    .show(ui, theme);
                                    if response.clicked() {
                                        format = Some(index);
                                    }
                                    formats.push(response);
                                    ui.label(RichText::new(format!(".{ext}")).color(theme.muted));
                                    if index % 2 == 1 {
                                        ui.end_row();
                                    }
                                }
                            });
                        if let Some(index) = ui::rove(ui, &formats, Roving::Both).or(format) {
                            choices.format = index;
                        }
                        if self.project.is_some() {
                            ui.add_space(theme.space(Space::S4));
                            widgets::heading(ui, &tr!("what"), theme);
                            let whole = [tr!("the-whole-manuscript"), tr!("what-is-open-now")];
                            if let Some(index) = radios(
                                ui,
                                theme,
                                "writer-what",
                                &whole,
                                usize::from(!choices.whole),
                            ) {
                                choices.whole = index == 0;
                            }
                        }
                        ui.add_space(theme.space(Space::S4));
                        widgets::heading(ui, &tr!("titles"), theme);
                        let titles = [Titles::Folders, Titles::All, Titles::None];
                        let names = [
                            tr!("folders-become-chapter-titles"),
                            tr!("folders-and-documents-both"),
                            tr!("no-titles-just-the-text"),
                        ];
                        let on = titles.iter().position(|one| *one == choices.titles);
                        if let Some(index) =
                            radios(ui, theme, "writer-titles", &names, on.unwrap_or(0))
                        {
                            choices.titles = titles[index];
                        }
                        ui.add_space(theme.space(Space::S4));
                        widgets::heading(ui, &tr!("between-documents"), theme);
                        let separators = [Separator::SceneBreak, Separator::Page, Separator::None];
                        let names = [tr!("a-scene-break"), tr!("a-new-page"), tr!("nothing")];
                        let on = separators.iter().position(|one| *one == choices.separator);
                        if let Some(index) =
                            radios(ui, theme, "writer-between", &names, on.unwrap_or(0))
                        {
                            choices.separator = separators[index];
                        }
                        ui.add_space(theme.space(Space::S4));
                        if ui::checkbox(
                            "writer-title-page",
                            &tr!("a-title-page-first"),
                            choices.title_page,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            choices.title_page = !choices.title_page;
                        }
                        if ui::checkbox(
                            "writer-chapter-pages",
                            &tr!("each-chapter-new-page"),
                            choices.chapter_pages,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            choices.chapter_pages = !choices.chapter_pages;
                        }
                        Self::tidy_ui(ui, choices, theme);
                        ui.add_space(theme.space(Space::S5));
                        ui.horizontal(|ui| {
                            if ui::button("writer-export-go", &tr!("export-ellipsis"))
                                .primary()
                                .show(ui, theme)
                                .clicked()
                            {
                                let picked = choices.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.ask_compile(cx, picked)
                                }));
                                keep = false;
                            }
                            if ui::button("writer-export-cancel", &tr!("cancel"))
                                .show(ui, theme)
                                .clicked()
                            {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Settings(settings) => {
                        self.settings_ui(ui, settings, theme);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr!("apply")).clicked() {
                                let settings = settings.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.apply_settings(cx, &settings)
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Link(url) => {
                        ui.label(tr!("web-address"));
                        ui.add(egui::TextEdit::singleline(url).desired_width(340.0));
                        ui.horizontal(|ui| {
                            if ui.button(tr!("link")).clicked() {
                                let url = url.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.editor.set_char(
                                        cx,
                                        kind::LINK,
                                        (!url.is_empty()).then_some(Value::Text(url)),
                                        &tr!("link"),
                                    )
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("remove-link")).clicked() {
                                after = Some(Box::new(|me: &mut Self, cx| {
                                    me.editor
                                        .set_char(cx, kind::LINK, None, &tr!("remove-link"))
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Endnote(note) => {
                        ui.label(tr!("note-printed-end"));
                        ui.add(
                            egui::TextEdit::multiline(note)
                                .desired_rows(4)
                                .desired_width(340.0),
                        );
                        ui.horizontal(|ui| {
                            if ui.button(tr!("add-endnote")).clicked() {
                                let note = note.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    if !note.is_empty() {
                                        me.editor.set_char(
                                            cx,
                                            kind::ENDNOTE,
                                            Some(Value::Text(note)),
                                            &tr!("endnote"),
                                        );
                                    }
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Footnote(note) => {
                        ui.label(tr!("note-printed-foot"));
                        ui.add(
                            egui::TextEdit::multiline(note)
                                .desired_rows(4)
                                .desired_width(340.0),
                        );
                        ui.horizontal(|ui| {
                            if ui.button(tr!("add-footnote")).clicked() {
                                let note = note.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    if !note.is_empty() {
                                        me.editor.set_char(
                                            cx,
                                            kind::FOOTNOTE,
                                            Some(Value::Text(note)),
                                            &tr!("footnote"),
                                        );
                                    }
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Table {
                        rows,
                        columns,
                        heading,
                    } => {
                        egui::Grid::new("writer-table-size")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(tr!("columns"));
                                ui.add(egui::DragValue::new(columns).range(1..=63));
                                ui.end_row();
                                ui.label(tr!("rows"));
                                ui.add(egui::DragValue::new(rows).range(1..=999));
                                ui.end_row();
                            });
                        ui.checkbox(heading, tr!("heading-row-repeated-each-page"));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr!("insert")).clicked() {
                                let (rows, columns, heading) = (*rows, *columns, *heading);
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.editor
                                        .insert_table(cx, rows, columns, u32::from(heading));
                                    me.editor.focus();
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::TableFile {
                        path,
                        heading,
                        insert,
                    } => {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(RichText::new(name).color(theme.muted));
                        ui.add_space(theme.space(Space::S4));
                        if ui::checkbox(
                            "writer-table-heading-row",
                            &tr!("the-first-row-is-a-heading-row"),
                            *heading,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            *heading = !*heading;
                        }
                        ui.add_space(theme.space(Space::S5));
                        ui.horizontal(|ui| {
                            let go = if *insert { tr!("insert") } else { tr!("open") };
                            if ui::button("writer-table-file-go", &go)
                                .primary()
                                .show(ui, theme)
                                .clicked()
                            {
                                let (path, heading, insert) = (path.clone(), *heading, *insert);
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.open_table_file(cx, &path, heading, insert);
                                }));
                                keep = false;
                            }
                            if ui::button("writer-table-file-cancel", &tr!("cancel"))
                                .show(ui, theme)
                                .clicked()
                            {
                                keep = false;
                            }
                        });
                    }
                    Dialog::About => {
                        ui.label(
                            RichText::new(tr!("version-number", value = env!("CARGO_PKG_VERSION")))
                                .color(theme.muted),
                        );
                        ui.add_space(theme.space(Space::S4));
                        ui.label(tr!("sue-writer-about"));
                        ui.add_space(theme.space(Space::S5));
                        if ui::button("writer-about-close", &tr!("close"))
                            .primary()
                            .show(ui, theme)
                            .clicked()
                        {
                            keep = false;
                        }
                    }
                    Dialog::Comment(text) => {
                        ui.label(tr!("by", value = self.author_name(cx)));
                        let response = ui.add(
                            egui::TextEdit::multiline(text)
                                .desired_rows(4)
                                .desired_width(340.0)
                                .hint_text(tr!("your-comment")),
                        );
                        response.request_focus();
                        let send = ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .button(tr!("add-comment"))
                                .on_hover_text("Ctrl+Enter")
                                .clicked()
                                || send
                            {
                                let text = text.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    if !text.is_empty() {
                                        me.add_comment(cx, &text);
                                    }
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Contents(contents) => {
                        egui::Grid::new("writer-contents")
                            .num_columns(2)
                            .spacing(vec2(12.0, 8.0))
                            .show(ui, |ui| {
                                ui.label(tr!("title"));
                                ui.text_edit_singleline(&mut contents.title);
                                ui.end_row();
                                ui.label(tr!("heading-levels"));
                                ui.add(egui::DragValue::new(&mut contents.levels).range(1..=6));
                                ui.end_row();
                            });
                        ui.label(
                            RichText::new(tr!("lists-headings-their-pages-keeps-up"))
                                .size(theme.small_size)
                                .color(theme.muted),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr!("insert")).clicked() {
                                let contents = contents.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.editor.insert_contents(cx, &contents);
                                    me.editor.focus();
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Furniture(furniture) => {
                        furniture_ui(ui, furniture, theme);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() {
                                let furniture = furniture.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.apply_furniture(cx, &furniture);
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Statistics => {
                        self.statistics_ui(ui, cx, theme);
                        if ui.button(tr!("close")).clicked() {
                            keep = false;
                        }
                    }
                    Dialog::Shortcuts => {
                        egui::ScrollArea::vertical()
                            .max_height(460.0)
                            .show(ui, |ui| {
                                let keys = keys.as_deref().unwrap_or_default();
                                sue_wm::shortcuts::grid_ui(ui, "writer-shortcuts", keys, theme);
                            });
                        ui.horizontal(|ui| {
                            if ui.button(tr!("close")).clicked() {
                                keep = false;
                            }
                            if ui
                                .button(tr!("change-shortcuts-ellipsis"))
                                .on_hover_text(tr!("settings-keymaps-any-number-keys-each"))
                                .clicked()
                            {
                                keep = false;
                                after = Some(Box::new(|_: &mut Self, cx| {
                                    cx.request(ShellRequest::OpenSettings(Some(
                                        "keymaps".to_owned(),
                                    )));
                                }));
                            }
                        });
                    }
                }
            });
        if escape {
            keep = false;
        }
        if keep {
            self.dialog = Some(dialog);
        }
        if let Some(after) = after {
            after(self, cx);
        }
    }

    /// The Export dialog's tidying of plain text: line endings for Plain
    /// text and Markdown; spaces at line ends and tabs for Plain text only
    /// (in Markdown two spaces end a line and tabs carry its lists).
    fn tidy_ui(ui: &mut Ui, choices: &mut crate::studio::CompileChoices, theme: &Theme) {
        let (key, ..) = FORMATS[choices.format.min(FORMATS.len() - 1)];
        if crate::compile::tidying(key, &choices.text).is_none() {
            return;
        }
        ui.add_space(theme.space(Space::S4));
        widgets::heading(ui, &tr!("line-endings"), theme);
        // One row each, so the dialog stays short enough for a laptop.
        let mut endings = Vec::with_capacity(2);
        let mut clicked = None;
        ui.horizontal(|ui| {
            let names = [tr!("lf-linux-macos"), tr!("crlf-windows")];
            let on = usize::from(choices.text.endings == Endings::CrLf);
            for (index, name) in names.iter().enumerate() {
                if index > 0 {
                    ui.add_space(theme.space(Space::S5));
                }
                let response =
                    ui::radio(("writer-endings", index), name, index == on).show(ui, theme);
                if response.clicked() {
                    clicked = Some(index);
                }
                endings.push(response);
            }
        });
        if let Some(index) = ui::rove(ui, &endings, Roving::Horizontal).or(clicked) {
            choices.text.endings = if index == 0 {
                Endings::Lf
            } else {
                Endings::CrLf
            };
        }
        if key != "txt" {
            return;
        }
        ui.horizontal(|ui| {
            if ui::checkbox(
                "writer-drop-spaces",
                &tr!("drop-spaces-at-line-ends"),
                choices.text.trim_trailing,
            )
            .show(ui, theme)
            .clicked()
            {
                choices.text.trim_trailing = !choices.text.trim_trailing;
            }
            ui.add_space(theme.space(Space::S5));
            ui.label(tr!("tabs-as"));
            let mut spaces = f64::from(choices.text.tab_spaces);
            let label = tr!("tabs-as-spaces");
            ui::number_field("writer-tab-spaces", &mut spaces)
                .range(0.0..=8.0)
                .step(1.0)
                .decimals(0)
                .label(&label)
                .show(ui, theme)
                .on_hover_text(tr!("n-0-keeps-the-tabs"));
            choices.text.tab_spaces = spaces.round().clamp(0.0, 8.0) as u8;
            ui.label(tr!("spaces"));
        });
    }

    fn statistics_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let (draft, session) = self.counts(cx);
        let total = self
            .project
            .map_or(0, |root| manuscript::total_words(cx.arena(), root));
        let target = self
            .project_entity(cx)
            .and_then(|entity| entity.property(manuscript::TARGET))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        egui::Grid::new("writer-stats")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(tr!("this-sitting"));
                ui.label(tr!("word-count-change", change = format!("{session:+}")));
                ui.end_row();
                ui.label(tr!("the-manuscript"));
                ui.label(tr!("word-count", count = draft));
                ui.end_row();
                ui.label(tr!("everything-with-research"));
                ui.label(tr!("word-count", count = total));
                ui.end_row();
                if target > 0 {
                    ui.label(tr!("target"));
                    ui.label(tr!(
                        "words-percent-there",
                        value = sue_lang::number(draft as f64 * 100.0 / target as f64, 0),
                        target = target
                    ));
                    ui.end_row();
                }
            });
        if target > 0 {
            progress(ui, draft as f32 / target as f32, theme);
        }
        let history = self
            .project_entity(cx)
            .map(project::history)
            .unwrap_or_default();
        if history.is_empty() {
            ui.label(
                RichText::new(tr!("days-you-write-show-here-once"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            return;
        }
        ui.add_space(8.0);
        widgets::heading(ui, &tr!("words-by-day"), theme);
        let days: Vec<&(String, i64)> = history.iter().rev().take(14).collect();
        let most = days
            .iter()
            .map(|(_, words)| *words)
            .max()
            .unwrap_or(1)
            .max(1);
        let (rect, _) = ui.allocate_exact_size(vec2(380.0, 110.0), Sense::hover());
        let bar = rect.width() / days.len().max(1) as f32;
        for (index, (day, words)) in days.iter().rev().enumerate() {
            let height = (*words as f32 / most as f32) * (rect.height() - 20.0);
            let column = Rect::from_min_max(
                pos2(
                    rect.left() + index as f32 * bar + 2.0,
                    rect.bottom() - 16.0 - height,
                ),
                pos2(
                    rect.left() + (index + 1) as f32 * bar - 2.0,
                    rect.bottom() - 16.0,
                ),
            );
            ui.painter()
                .rect_filled(column, 2.0, theme.text.gamma_multiply(0.7));
            ui.painter().text(
                pos2(column.center().x, rect.bottom()),
                Align2::CENTER_BOTTOM,
                &day[8..],
                theme.font(9.0),
                theme.muted,
            );
        }
    }
}

/// One question's radio buttons, one row each, the arrow keys walking
/// them. `on`: the answer chosen now. Returns the answer clicked or
/// walked to in this frame.
fn radios(ui: &mut Ui, theme: &Theme, salt: &str, names: &[String], on: usize) -> Option<usize> {
    let mut clicked = None;
    let members: Vec<egui::Response> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let response = ui::radio((salt, index), name, index == on).show(ui, theme);
            if response.clicked() {
                clicked = Some(index);
            }
            response
        })
        .collect();
    ui::rove(ui, &members, Roving::Vertical).or(clicked)
}
