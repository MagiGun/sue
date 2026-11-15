//! The inspector: an item's synopsis, notes and properties.

use egui::{Rect, RichText, Ui, UiBuilder};
use sue_core::{Entity, EntityId, Value};
use sue_model::manuscript;
use sue_wm::{AppletContext, ArenaEdit, Theme, widgets};

use crate::project::{self, LABELS, STATUSES};
use crate::studio::{SueWriter, View};

use super::rgb;
use super::status::progress;

impl SueWriter {
    fn property_field(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        key: &'static str,
        rows: usize,
        hint: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let stored = cx
            .arena()
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let draft = self
            .drafts
            .entry((id, key))
            .or_insert_with(|| stored.clone());
        let edit = if rows > 1 {
            egui::TextEdit::multiline(draft)
                .desired_rows(rows)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
        } else {
            egui::TextEdit::singleline(draft)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
        };
        let response = ui.add(edit);
        if response.lost_focus() {
            let text = draft.clone();
            self.drafts.remove(&(id, key));
            if text != stored {
                let value = (!text.is_empty()).then_some(Value::Text(text));
                let _ = cx.apply(ArenaEdit(project::set(id, key, value)));
            }
        } else if !response.has_focus() && *draft != stored {
            // Changed elsewhere (undo): show what is stored.
            *draft = stored;
        }
    }

    pub(super) fn inspector_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        let target = self.selected.or(self.open);
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(8.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let mut run: Option<String> = None;
        if self.comments {
            ui.horizontal(|ui| {
                if ui.selectable_label(false, tr!("inspector")).clicked() {
                    self.comments = false;
                }
                let _ = ui.selectable_label(true, tr!("comments"));
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("writer-comments")
                .auto_shrink([false, false])
                .show(ui, |ui| self.comments_ui(ui, cx, theme));
            return;
        }
        if !self.editor.comments().is_empty() {
            ui.horizontal(|ui| {
                let _ = ui.selectable_label(true, tr!("inspector"));
                if ui.selectable_label(false, tr!("comments")).clicked() {
                    self.comments = true;
                }
            });
            ui.add_space(4.0);
        }
        egui::ScrollArea::vertical()
            .id_salt("writer-inspector")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(id) = target else {
                    return;
                };
                let Ok(entity) = cx.arena().get_by_id(id) else {
                    return;
                };
                let name = entity.name().to_owned();
                let is_document = entity.string_buffer().is_some();
                let in_project = self.project.is_some();
                ui.label(
                    RichText::new(name)
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(6.0);
                if in_project {
                    widgets::heading(ui, &tr!("synopsis"), theme);
                    self.property_field(
                        ui,
                        id,
                        manuscript::SYNOPSIS,
                        4,
                        &tr!("what-happens-here-line-two"),
                        cx,
                    );
                    ui.add_space(6.0);
                    let entity = cx.arena().get_by_id(id).ok();
                    let status = entity
                        .and_then(|entity| entity.property(manuscript::STATUS))
                        .and_then(Value::as_str)
                        .map_or_else(|| tr!("no-status"), project::status_text);
                    let label = entity
                        .and_then(|entity| entity.property(manuscript::LABEL))
                        .and_then(Value::as_str)
                        .map_or_else(|| tr!("no-label"), project::label_text);
                    let included = entity.is_some_and(manuscript::included);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("writer-label", id))
                            .selected_text(label.clone())
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for (name, color) in LABELS {
                                    if ui
                                        .selectable_label(
                                            label == project::label_text(name),
                                            RichText::new(format!(
                                                "\u{25CF} {}",
                                                project::label_text(name)
                                            ))
                                            .color(rgb(*color)),
                                        )
                                        .clicked()
                                    {
                                        run = Some(format!("label.{name}"));
                                    }
                                }
                                if ui.selectable_label(false, tr!("no-label")).clicked() {
                                    run = Some("label.none".to_owned());
                                }
                            });
                        egui::ComboBox::from_id_salt(("writer-status", id))
                            .selected_text(status.clone())
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for name in STATUSES {
                                    let text = project::status_text(name);
                                    if ui.selectable_label(status == text, text).clicked() {
                                        run = Some(format!("status.{name}"));
                                    }
                                }
                                if ui.selectable_label(false, tr!("no-status")).clicked() {
                                    run = Some("status.none".to_owned());
                                }
                            });
                    });
                    let mut include = included;
                    if ui
                        .checkbox(&mut include, tr!("include-in-export-2"))
                        .changed()
                    {
                        run = Some("project.include".to_owned());
                    }
                    let target_words = cx
                        .arena()
                        .get_by_id(id)
                        .ok()
                        .and_then(|entity| entity.property(manuscript::TARGET))
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let mut words_target = target_words;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr!("word-target")).color(theme.muted));
                        let response = ui.add(
                            egui::DragValue::new(&mut words_target)
                                .range(0..=10_000_000)
                                .speed(50),
                        );
                        if (response.drag_stopped() || response.lost_focus())
                            && words_target != target_words
                        {
                            let value = (words_target > 0).then_some(Value::Int(words_target));
                            let _ =
                                cx.apply(ArenaEdit(project::set(id, manuscript::TARGET, value)));
                        }
                    });
                    if target_words > 0 {
                        let words = manuscript::total_words(cx.arena(), id) as f32;
                        progress(ui, words / target_words as f32, theme);
                    }
                    ui.add_space(6.0);
                    widgets::heading(ui, &tr!("keywords"), theme);
                    self.property_field(
                        ui,
                        id,
                        manuscript::KEYWORDS,
                        1,
                        &tr!("comma-separated"),
                        cx,
                    );
                }
                if is_document {
                    ui.add_space(8.0);
                    widgets::heading(ui, &tr!("statistics"), theme);
                    let text = cx
                        .arena()
                        .get_by_id(id)
                        .ok()
                        .and_then(Entity::string_buffer)
                        .map(|text| text.as_str().to_owned())
                        .unwrap_or_default();
                    let words = sue_model::word_count(&text);
                    let paragraphs = text.lines().filter(|line| !line.trim().is_empty()).count();
                    let selection = self.editor.selected_text();
                    ui.label(
                        RichText::new(tr!(
                            "words-characters-paragraphs",
                            count = text.chars().filter(|ch| *ch != '\n').count(),
                            words = words,
                            paragraphs = paragraphs
                        ))
                        .color(theme.muted),
                    );
                    if !selection.is_empty() {
                        ui.label(
                            RichText::new(tr!(
                                "selected-words",
                                value = sue_model::word_count(selection)
                            ))
                            .color(theme.muted),
                        );
                    }
                    ui.label(
                        RichText::new(tr!("about-minutes-to-read", value = (words / 230).max(1)))
                            .color(theme.muted),
                    );
                }
                if in_project {
                    ui.add_space(8.0);
                    widgets::heading(ui, &tr!("notes"), theme);
                    self.property_field(
                        ui,
                        id,
                        manuscript::NOTES,
                        6,
                        &tr!("notes-for-yourself"),
                        cx,
                    );
                }
                if is_document && in_project {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        widgets::heading(ui, &tr!("snapshots"), theme);
                        if ui
                            .small_button(tr!("take"))
                            .on_hover_text(tr!("keep-copy-document-as-now-ctrl"))
                            .clicked()
                        {
                            run = Some("project.snapshot".to_owned());
                        }
                    });
                    let shots = project::snapshots(cx.arena(), id);
                    if shots.is_empty() {
                        ui.label(
                            RichText::new(tr!("none-yet-take-one-before-big"))
                                .size(theme.small_size)
                                .color(theme.muted),
                        );
                    }
                    let mut restore = None;
                    for shot in shots.iter().rev() {
                        let Ok(saved) = cx.arena().get_by_id(*shot) else {
                            continue;
                        };
                        let words = saved
                            .string_buffer()
                            .map_or(0, |text| sue_model::word_count(text.as_str()));
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(tr!(
                                    "snapshot-words",
                                    saved = saved.name(),
                                    words = words
                                ))
                                .size(theme.small_size + 0.5),
                            );
                            if ui
                                .small_button(tr!("restore"))
                                .on_hover_text(tr!("put-version-back-what-there-now"))
                                .clicked()
                            {
                                restore = Some(*shot);
                            }
                        });
                    }
                    if let Some(shot) = restore
                        && let Some(batch) =
                            project::restore(cx.arena(), id, shot, &project::today())
                    {
                        self.apply(cx, batch);
                    }
                }
                // The headings of what is shown, to jump to.
                let outline = self.editor.outline();
                if !outline.is_empty() && self.view == View::Document {
                    ui.add_space(8.0);
                    widgets::heading(ui, &tr!("outline"), theme);
                    for entry in outline {
                        let indent = f32::from(entry.level.saturating_sub(1)) * 12.0;
                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            if ui
                                .link(RichText::new(&entry.title).size(theme.small_size + 0.5))
                                .clicked()
                                && let Some(doc) = self.editor.docs().get(entry.at.doc).copied()
                            {
                                self.editor.select(doc, entry.at.offset..entry.at.offset);
                            }
                        });
                    }
                }
            });
        if let Some(command) = run {
            let keep = self.selected;
            self.selected = target;
            self.run(&command, cx);
            if keep.is_some() {
                self.selected = keep;
            }
        }
    }

    // ----- Status, find, composition --------------------------------------------------
}
