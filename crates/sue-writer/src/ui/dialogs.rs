//! The dialogs of the studio and the statistics.
//!
//! Every dialog is drawn by Sue's one dialog host (`sue_wm::ui::Dialog`):
//! the title, the body below, and the footer whose buttons always stand in
//! the same order, the one that accepts first and Cancel last. Enter
//! accepts and Escape cancels, as everywhere in Sue. The body only edits
//! the dialog's fields; what accepting does is [`SueWriter::accepted`].

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_core::Value;
use sue_io::text_ops::Endings;
use sue_model::manuscript;
use sue_model::mark_kind as kind;
use sue_wm::ui::{self, Answer, Radius, Roving, Space, TextRole, Tip};
use sue_wm::{AppletContext, ShellRequest, Theme, widgets};

use crate::compile::{FORMATS, Separator, Titles};
use crate::project;
use crate::studio::{Dialog, SueWriter};
use crate::templates::{Group, Template};

use super::After;
use super::settings::furniture_ui;

/// How many days the statistics' chart shows, the latest at the right.
const CHART_DAYS: usize = 14;
/// How tall the chart is, in the largest steps of the spacing scale.
const CHART_STEPS: f32 = 3.0;
/// How wide Header and Footer is, in the largest steps of the spacing
/// scale: a label and three parts of a line.
const FURNITURE_STEPS: f32 = 15.0;
/// How tall the list of shortcuts gets before it scrolls, in the largest
/// steps of the spacing scale.
const SHORTCUT_STEPS: f32 = 11.0;
/// How many lines a note or a comment field shows.
const NOTE_ROWS: usize = 4;

impl SueWriter {
    /// Draws the dialog that is open, if one is, and does what its
    /// answer asks once it is given.
    pub(super) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let keys =
            matches!(dialog, Dialog::Shortcuts).then(|| crate::commands::shortcut_rows(cx.rice()));
        let title = title(&dialog);
        let mut host = footer(ui::Dialog::new(salt(&dialog), &title, theme), &dialog);
        if matches!(dialog, Dialog::Furniture(_)) {
            host = host.width(theme.space(Space::S9) * FURNITURE_STEPS);
        }
        let shown = host.show(ctx, |ui| {
            self.dialog_body(ui, cx, theme, &mut dialog, keys.as_deref())
        });
        // The body can accept too: Ctrl+Enter in a comment.
        let answer = if shown.inner {
            Some(Answer::Accept)
        } else {
            shown.answer
        };
        let after = match answer {
            None => {
                self.dialog = Some(dialog);
                None
            }
            Some(Answer::Accept) => Self::accepted(dialog),
            Some(Answer::Alternate) => Self::alternate(&dialog),
            Some(Answer::Extra) => Self::extra(&dialog),
            Some(Answer::Cancel) => None,
        };
        if let Some(after) = after {
            after(self, cx);
        }
    }

    /// The body of `dialog`, where its fields are edited. Returns `true`
    /// when the body itself accepts the dialog in this frame.
    fn dialog_body(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        dialog: &mut Dialog,
        keys: Option<&[(String, String)]>,
    ) -> bool {
        match dialog {
            Dialog::NewProject { template, title } => {
                self.new_project_ui(ui, template, title, theme);
            }
            Dialog::Compile(choices) => self.compile_ui(ui, choices, theme),
            Dialog::Settings(settings) => self.settings_ui(ui, settings, theme),
            Dialog::Link(url) => {
                ui::form_grid(ui, "writer-link", theme, |form| {
                    form.row(&tr!("web-address"), |ui| {
                        let (hint, label) = (tr!("link-hint"), tr!("web-address"));
                        ui::text_field("writer-link-url", url)
                            .hint(&hint)
                            .label(&label)
                            .show(ui, theme)
                    });
                });
            }
            Dialog::Footnote(note) => {
                note_ui(ui, "writer-footnote", note, &tr!("note-printed-foot"))
            }
            Dialog::Endnote(note) => note_ui(ui, "writer-endnote", note, &tr!("note-printed-end")),
            Dialog::Table {
                rows,
                columns,
                heading,
            } => {
                ui::form_grid(ui, "writer-table-size", theme, |form| {
                    form.row(&tr!("columns"), |ui| {
                        count_field(
                            ui,
                            "writer-table-columns",
                            columns,
                            63,
                            &tr!("columns"),
                            theme,
                        );
                    });
                    form.row(&tr!("rows"), |ui| {
                        count_field(ui, "writer-table-rows", rows, 999, &tr!("rows"), theme);
                    });
                    form.wide(|ui| {
                        check(
                            ui,
                            "writer-table-heading",
                            &tr!("heading-row-repeated-each-page"),
                            heading,
                            theme,
                        );
                    });
                });
            }
            Dialog::TableFile { path, heading, .. } => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                caption(ui, &name, theme);
                ui.add_space(theme.space(Space::S4));
                check(
                    ui,
                    "writer-table-heading-row",
                    &tr!("the-first-row-is-a-heading-row"),
                    heading,
                    theme,
                );
            }
            Dialog::About => {
                caption(
                    ui,
                    &tr!("version-number", value = env!("CARGO_PKG_VERSION")),
                    theme,
                );
                ui.add_space(theme.space(Space::S4));
                ui.label(tr!("sue-writer-about"));
            }
            Dialog::Comment(text) => {
                caption(ui, &tr!("by", value = self.author_name(cx)), theme);
                ui.add_space(theme.space(Space::S2));
                let field = ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(NOTE_ROWS)
                        .desired_width(ui.available_width())
                        .hint_text(tr!("your-comment")),
                );
                field.request_focus();
                ui::focus_ring(ui, &field, theme.radius(Radius::Md), theme);
                ui.add_space(theme.space(Space::S2));
                caption(ui, &tr!("ctrl-enter-adds-the-comment"), theme);
                return ui.input_mut(|input| {
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                });
            }
            Dialog::Contents(contents) => {
                ui::form_grid(ui, "writer-contents", theme, |form| {
                    form.row(&tr!("title"), |ui| {
                        let label = tr!("title");
                        ui::text_field("writer-contents-title", &mut contents.title)
                            .label(&label)
                            .show(ui, theme)
                    });
                    form.row(&tr!("heading-levels"), |ui| {
                        let mut levels = f64::from(contents.levels);
                        let label = tr!("heading-levels");
                        ui::number_field("writer-contents-levels", &mut levels)
                            .range(1.0..=6.0)
                            .label(&label)
                            .show(ui, theme);
                        contents.levels = levels.round().clamp(1.0, 6.0) as u8;
                    });
                });
                ui.add_space(theme.space(Space::S2));
                caption(ui, &tr!("lists-headings-their-pages-keeps-up"), theme);
            }
            Dialog::Furniture(furniture) => furniture_ui(ui, furniture, theme),
            Dialog::Statistics => self.statistics_ui(ui, cx, theme),
            Dialog::Shortcuts => {
                egui::ScrollArea::vertical()
                    .max_height(theme.space(Space::S9) * SHORTCUT_STEPS)
                    .show(ui, |ui| {
                        let keys = keys.unwrap_or_default();
                        sue_wm::shortcuts::grid_ui(ui, "writer-shortcuts", keys, theme);
                    });
            }
        }
        false
    }

    /// What accepting `dialog` does, run once it has closed.
    fn accepted(dialog: Dialog) -> Option<After> {
        let after: After = match dialog {
            Dialog::NewProject { template, title } => {
                let title = if title.trim().is_empty() {
                    tr!("untitled")
                } else {
                    title.trim().to_owned()
                };
                Box::new(move |me: &mut Self, cx| me.new_project(cx, &template, &title))
            }
            Dialog::Compile(choices) => {
                Box::new(move |me: &mut Self, cx| me.ask_compile(cx, choices))
            }
            Dialog::Settings(settings) => {
                Box::new(move |me: &mut Self, cx| me.apply_settings(cx, &settings))
            }
            Dialog::Link(url) => {
                let url = url.trim().to_owned();
                Box::new(move |me: &mut Self, cx| {
                    me.editor.set_char(
                        cx,
                        kind::LINK,
                        (!url.is_empty()).then_some(Value::Text(url)),
                        &tr!("link"),
                    );
                })
            }
            Dialog::Footnote(note) => return note_after(note, kind::FOOTNOTE, tr!("footnote")),
            Dialog::Endnote(note) => return note_after(note, kind::ENDNOTE, tr!("endnote")),
            Dialog::Table {
                rows,
                columns,
                heading,
            } => Box::new(move |me: &mut Self, cx| {
                // As one undo step, as Table from File puts a table in.
                let rows = rows.max(1);
                let columns = columns.max(1);
                let table = sue_type::tables::new_table(rows, columns, u32::from(heading));
                me.insert_table_entity(cx, table);
                me.editor.focus();
            }),
            Dialog::TableFile {
                path,
                heading,
                insert,
            } => Box::new(move |me: &mut Self, cx| {
                me.open_table_file(cx, &path, heading, insert);
            }),
            Dialog::Comment(text) => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return None;
                }
                Box::new(move |me: &mut Self, cx| me.add_comment(cx, &text))
            }
            Dialog::Contents(contents) => Box::new(move |me: &mut Self, cx| {
                me.editor.insert_contents(cx, &contents);
                me.editor.focus();
            }),
            Dialog::Furniture(furniture) => {
                Box::new(move |me: &mut Self, cx| me.apply_furniture(cx, &furniture))
            }
            Dialog::About | Dialog::Statistics | Dialog::Shortcuts => return None,
        };
        Some(after)
    }

    /// What the other answer of `dialog` does: Remove Link.
    fn alternate(dialog: &Dialog) -> Option<After> {
        matches!(dialog, Dialog::Link(_)).then(|| -> After {
            Box::new(|me: &mut Self, cx| {
                me.editor
                    .set_char(cx, kind::LINK, None, &tr!("remove-link"));
            })
        })
    }

    /// What the action at the footer's leading edge does: the shortcuts'
    /// Change Shortcuts opens Settings at the keymaps.
    fn extra(dialog: &Dialog) -> Option<After> {
        matches!(dialog, Dialog::Shortcuts).then(|| -> After {
            Box::new(|_: &mut Self, cx| {
                cx.request(ShellRequest::OpenSettings(Some("keymaps".to_owned())));
            })
        })
    }

    /// New Project: the templates, the presets for long work, and the
    /// title.
    fn new_project_ui(
        &self,
        ui: &mut Ui,
        template: &mut String,
        title: &mut String,
        theme: &Theme,
    ) {
        let templates: Vec<&Template> = self.templates.group(Group::Template).collect();
        let mut members = Vec::with_capacity(templates.len());
        let mut clicked = None;
        for (index, one) in templates.iter().enumerate() {
            let response = ui::radio(
                ("writer-new-template", index),
                &one.name(),
                *template == one.key,
            )
            .show(ui, theme);
            let about = one.about();
            let response = ui::tooltip(response, &Tip::new("").help(&about), theme);
            if response.clicked() {
                clicked = Some(index);
            }
            members.push(response);
        }
        if let Some(index) = ui::rove(ui, &members, Roving::Vertical).or(clicked) {
            templates[index].key.clone_into(template);
        }
        ui.add_space(theme.space(Space::S4));
        let presets: Vec<&Template> = self.templates.group(Group::Preset).collect();
        ui::form_grid(ui, "writer-new-form", theme, |form| {
            form.row(&tr!("presets"), |ui| {
                // The first answer is no preset: one of the templates
                // above.
                let mut names = vec![tr!("none")];
                names.extend(presets.iter().map(|one| one.name()));
                let mut chosen = presets
                    .iter()
                    .position(|one| one.key == *template)
                    .map_or(0, |index| index + 1);
                let label = tr!("presets");
                let response = ui::dropdown("writer-new-preset", &mut chosen, &names)
                    .label(&label)
                    .show(ui, theme);
                if response.changed() {
                    match chosen.checked_sub(1).and_then(|index| presets.get(index)) {
                        Some(one) => one.key.clone_into(template),
                        None => {
                            if let Some(first) = templates.first() {
                                first.key.clone_into(template);
                            }
                        }
                    }
                }
            });
            form.wide(|ui| {
                if let Some(one) = self.templates.get(template) {
                    caption(ui, &one.about(), theme);
                }
            });
            form.row(&tr!("title"), |ui| {
                let (hint, label) = (tr!("untitled"), tr!("title"));
                ui::text_field("writer-new-title", title)
                    .hint(&hint)
                    .label(&label)
                    .show(ui, theme)
            });
        });
    }

    /// Export: the format, what goes in, titles, what stands between
    /// documents, and the tidying of plain text.
    fn compile_ui(&self, ui: &mut Ui, choices: &mut crate::studio::CompileChoices, theme: &Theme) {
        // The answers of one question stand close; the questions apart.
        ui.spacing_mut().item_spacing.y = theme.space(Space::S1);
        widgets::heading(ui, &tr!("format"), theme);
        // Two columns of formats, each with its extension, so the whole
        // dialog fits a laptop's window.
        let mut formats = Vec::with_capacity(FORMATS.len());
        let mut format = None;
        egui::Grid::new("writer-formats")
            .num_columns(4)
            .spacing([theme.space(Space::S6), theme.space(Space::S1)])
            .show(ui, |ui| {
                for (index, (key, _, ext)) in FORMATS.iter().enumerate() {
                    let name = crate::compile::format_name(key);
                    let response =
                        ui::radio(("writer-format", index), &name, choices.format == index)
                            .show(ui, theme);
                    if response.clicked() {
                        format = Some(index);
                    }
                    formats.push(response);
                    caption(ui, &format!(".{ext}"), theme);
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
        if let Some(index) = radios(ui, theme, "writer-titles", &names, on.unwrap_or(0)) {
            choices.titles = titles[index];
        }
        ui.add_space(theme.space(Space::S4));
        widgets::heading(ui, &tr!("between-documents"), theme);
        let separators = [Separator::SceneBreak, Separator::Page, Separator::None];
        let names = [tr!("a-scene-break"), tr!("a-new-page"), tr!("nothing")];
        let on = separators.iter().position(|one| *one == choices.separator);
        if let Some(index) = radios(ui, theme, "writer-between", &names, on.unwrap_or(0)) {
            choices.separator = separators[index];
        }
        ui.add_space(theme.space(Space::S4));
        check(
            ui,
            "writer-title-page",
            &tr!("a-title-page-first"),
            &mut choices.title_page,
            theme,
        );
        check(
            ui,
            "writer-chapter-pages",
            &tr!("each-chapter-new-page"),
            &mut choices.chapter_pages,
            theme,
        );
        Self::tidy_ui(ui, choices, theme);
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
            check(
                ui,
                "writer-drop-spaces",
                &tr!("drop-spaces-at-line-ends"),
                &mut choices.text.trim_trailing,
                theme,
            );
            ui.add_space(theme.space(Space::S5));
            ui.label(tr!("tabs-as"));
            let mut spaces = f64::from(choices.text.tab_spaces);
            let label = tr!("tabs-as-spaces");
            let field = ui::number_field("writer-tab-spaces", &mut spaces)
                .range(0.0..=8.0)
                .step(1.0)
                .decimals(0)
                .label(&label)
                .show(ui, theme);
            let help = tr!("n-0-keeps-the-tabs");
            ui::tooltip(field, &Tip::new(&label).help(&help), theme);
            choices.text.tab_spaces = spaces.round().clamp(0.0, 8.0) as u8;
            ui.label(tr!("spaces"));
        });
    }

    /// Writing Statistics: words in this sitting, in the manuscript and
    /// in everything, the target, and a chart of the words of the last
    /// days.
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
        ui::form_grid(ui, "writer-stats", theme, |form| {
            form.row(&tr!("this-sitting"), |ui| {
                ui.label(tr!("word-count-change", change = format!("{session:+}")))
            });
            form.row(&tr!("the-manuscript"), |ui| {
                ui.label(tr!("word-count", count = draft))
            });
            form.row(&tr!("everything-with-research"), |ui| {
                ui.label(tr!("word-count", count = total))
            });
            if target > 0 {
                form.row(&tr!("target"), |ui| {
                    ui.label(tr!(
                        "words-percent-there",
                        value = sue_lang::number(draft as f64 * 100.0 / target as f64, 0),
                        target = target
                    ))
                });
            }
        });
        if target > 0 {
            ui.add_space(theme.space(Space::S2));
            let label = tr!("target");
            ui::progress_bar(draft as f32 / target as f32)
                .label(&label)
                .show(ui, theme);
        }
        let history = self
            .project_entity(cx)
            .map(project::history)
            .unwrap_or_default();
        ui.add_space(theme.space(Space::S4));
        if history.is_empty() {
            caption(ui, &tr!("days-you-write-show-here-once"), theme);
            return;
        }
        widgets::heading(ui, &tr!("words-by-day"), theme);
        let days: Vec<&(String, i64)> = history.iter().rev().take(CHART_DAYS).collect();
        chart(ui, &days, theme);
    }
}

/// A dialog's title.
fn title(dialog: &Dialog) -> String {
    match dialog {
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
    }
}

/// A name for each kind of dialog, so that one that follows another
/// starts afresh (its first focus, its entrance) rather than taking over
/// the state of the one before.
const fn salt(dialog: &Dialog) -> &'static str {
    match dialog {
        Dialog::NewProject { .. } => "writer-new-project",
        Dialog::Compile(_) => "writer-export",
        Dialog::Settings(_) => "writer-settings",
        Dialog::Link(_) => "writer-link",
        Dialog::Footnote(_) => "writer-footnote",
        Dialog::Endnote(_) => "writer-endnote",
        Dialog::Table { .. } => "writer-table",
        Dialog::Furniture(_) => "writer-furniture",
        Dialog::Contents(_) => "writer-contents",
        Dialog::Comment(_) => "writer-comment",
        Dialog::Statistics => "writer-statistics",
        Dialog::Shortcuts => "writer-shortcuts",
        Dialog::TableFile { .. } => "writer-table-file",
        Dialog::About => "writer-about",
    }
}

/// The footer buttons of `dialog`. A dialog that only tells something has
/// Close as its one button; the others accept with the verb they do and
/// can be cancelled.
fn footer<'a>(host: ui::Dialog<'a>, dialog: &Dialog) -> ui::Dialog<'a> {
    match dialog {
        Dialog::NewProject { .. } => host.accept(tr!("create")).cancel(),
        Dialog::Compile(_) => host.accept(tr!("export-ellipsis")).cancel(),
        Dialog::Settings(_) => host.accept(tr!("apply")).cancel(),
        Dialog::Link(_) => host
            .accept(tr!("link"))
            .alternate(tr!("remove-link"))
            .cancel(),
        Dialog::Footnote(_) => host.accept(tr!("add-footnote")).cancel(),
        Dialog::Endnote(_) => host.accept(tr!("add-endnote")).cancel(),
        Dialog::Table { .. } | Dialog::Contents(_) | Dialog::TableFile { insert: true, .. } => {
            host.accept(tr!("insert")).cancel()
        }
        Dialog::TableFile { .. } => host.accept(tr!("open")).cancel(),
        Dialog::Comment(_) => host.accept(tr!("add-comment")).cancel(),
        Dialog::Furniture(_) => host.ok_cancel(),
        Dialog::About | Dialog::Statistics => host.accept(tr!("close")),
        Dialog::Shortcuts => host
            .accept(tr!("close"))
            .extra(tr!("change-shortcuts-ellipsis")),
    }
}

/// A note's field with the line that says where it is printed.
fn note_ui(ui: &mut Ui, salt: &str, note: &mut String, about: &str) {
    ui.label(about);
    let field = ui.add(
        egui::TextEdit::multiline(note)
            .id_salt(salt)
            .desired_rows(NOTE_ROWS)
            .desired_width(ui.available_width()),
    );
    if ui.memory(|memory| memory.focused().is_none()) {
        field.request_focus();
    }
}

/// What accepting a footnote or an endnote does: nothing for an empty
/// note.
fn note_after(note: String, mark: &'static str, label: String) -> Option<After> {
    let note = note.trim().to_owned();
    if note.is_empty() {
        return None;
    }
    Some(Box::new(move |me: &mut SueWriter, cx| {
        me.editor
            .set_char(cx, mark, Some(Value::Text(note)), &label);
    }))
}

/// A line of secondary text: what a value is, or a note under a field.
fn caption(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// A check box that flips `on` when clicked.
fn check(ui: &mut Ui, salt: &str, label: &str, on: &mut bool, theme: &Theme) {
    if ui::checkbox(salt, label, *on).show(ui, theme).clicked() {
        *on = !*on;
    }
}

/// A field of a whole number from 1 to `most`.
fn count_field(ui: &mut Ui, salt: &str, value: &mut u32, most: u32, label: &str, theme: &Theme) {
    let mut number = f64::from(*value);
    ui::number_field(salt, &mut number)
        .range(1.0..=f64::from(most))
        .label(label)
        .show(ui, theme);
    *value = number.round().clamp(1.0, f64::from(most)) as u32;
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

/// The words written on each of `days` (the latest first) as columns,
/// the latest at the right in the text color, the others muted, each
/// with its day of the month under it. A column names its day and words
/// when the pointer rests on it.
fn chart(ui: &mut Ui, days: &[&(String, i64)], theme: &Theme) {
    let most = days
        .iter()
        .map(|(_, words)| *words)
        .max()
        .unwrap_or(1)
        .max(1);
    let height = theme.space(Space::S9) * CHART_STEPS;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let font = theme.text_font(TextRole::Micro);
    let under = theme.text_size(TextRole::Micro) + theme.space(Space::S1);
    let gap = theme.space(Space::S1);
    let bar = rect.width() / days.len().max(1) as f32;
    let room = rect.height() - under - gap;
    let radius = theme.radius(Radius::Xs);
    for (index, (day, words)) in days.iter().rev().enumerate() {
        let tall = (*words as f32 / most as f32) * room;
        let left = rect.left() + index as f32 * bar;
        let column = Rect::from_min_max(
            pos2(left + gap, rect.bottom() - under - tall),
            pos2(left + bar - gap, rect.bottom() - under),
        );
        let latest = index + 1 == days.len();
        let ink = if latest {
            theme.text
        } else {
            theme.text_muted()
        };
        ui.painter().rect_filled(column, radius, ink);
        let day_of_month = day.get(8..).unwrap_or(day);
        ui.painter().text(
            pos2(column.center().x, rect.bottom()),
            Align2::CENTER_BOTTOM,
            day_of_month,
            font.clone(),
            theme.text_muted(),
        );
        let slot = Rect::from_min_max(pos2(left, rect.top()), pos2(left + bar, rect.bottom()));
        let id = ui.scope_id().with(("writer-chart-day", index));
        let hover = ui.interact(slot, id, Sense::hover());
        hover.on_hover_text(tr!("day-words", day = day.as_str(), count = *words));
    }
}
