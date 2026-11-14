//! The dialogs of the studio and the statistics.

use egui::{Align2, Id, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_core::Value;
use sue_model::manuscript;
use sue_model::mark_kind as kind;
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
        let keys = matches!(dialog, Dialog::Shortcuts).then(|| {
            sue_wm::shortcuts::rows(
                cx.rice(),
                crate::studio::KEYS_SECTION,
                crate::studio::shortcut_table(),
            )
        });
        let title = match &dialog {
            Dialog::NewProject { .. } => tr!("new-project"),
            Dialog::Compile(_) => tr!("compile"),
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
                        egui::Grid::new("writer-formats")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (index, (key, _, ext)) in FORMATS.iter().enumerate() {
                                    ui.radio_value(
                                        &mut choices.format,
                                        index,
                                        crate::compile::format_name(key),
                                    );
                                    ui.label(RichText::new(format!(".{ext}")).color(theme.muted));
                                    ui.end_row();
                                }
                            });
                        ui.add_space(6.0);
                        widgets::heading(ui, &tr!("what"), theme);
                        if self.project.is_some() {
                            ui.radio_value(&mut choices.whole, true, tr!("the-whole-manuscript"));
                            ui.radio_value(&mut choices.whole, false, tr!("what-is-open-now"));
                        }
                        ui.add_space(6.0);
                        widgets::heading(ui, &tr!("titles"), theme);
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::Folders,
                            tr!("folders-become-chapter-titles"),
                        );
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::All,
                            tr!("folders-and-documents-both"),
                        );
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::None,
                            tr!("no-titles-just-the-text"),
                        );
                        ui.add_space(6.0);
                        widgets::heading(ui, &tr!("between-documents"), theme);
                        ui.radio_value(
                            &mut choices.separator,
                            Separator::SceneBreak,
                            tr!("a-scene-break"),
                        );
                        ui.radio_value(&mut choices.separator, Separator::Page, tr!("a-new-page"));
                        ui.radio_value(&mut choices.separator, Separator::None, tr!("nothing"));
                        ui.add_space(6.0);
                        ui.checkbox(&mut choices.title_page, tr!("a-title-page-first"));
                        ui.checkbox(&mut choices.chapter_pages, tr!("each-chapter-new-page"));
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr!("export-ellipsis")).clicked() {
                                let picked = choices.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.ask_compile(cx, picked)
                                }));
                                keep = false;
                            }
                            if ui.button(tr!("cancel")).clicked() {
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
