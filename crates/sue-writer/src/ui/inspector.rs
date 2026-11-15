//! The inspector: an item's synopsis, notes and properties, and beside
//! them the comments of what is shown.
//!
//! Everything in it is a control of the design system (`sue_wm::ui`):
//! the switch between Inspector and Comments, the fields, the label and
//! status lists, the export checkbox, the word target and its bar, the
//! snapshot buttons and the outline's links.

use egui::{CornerRadius, Rect, RichText, Ui, UiBuilder};
use sue_core::{Entity, EntityId, Value};
use sue_model::manuscript;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Space, TextRole, Tip};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use crate::project::{self, LABELS, STATUSES};
use crate::studio::{SueWriter, View};

use super::binder::keys_of;

/// The most words a word target can ask for.
const MOST_WORDS: f64 = 10_000_000.0;
/// How many words a point of dragging the word target adds.
const WORDS_PER_POINT: f64 = 50.0;
/// The words a minute of reading takes.
const WORDS_PER_MINUTE: usize = 230;

impl SueWriter {
    /// A text property edited in place, `rows` lines tall, committed when
    /// the field lets go.
    #[allow(
        clippy::too_many_arguments,
        reason = "the property, its field's look and the context"
    )]
    fn property_field(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        key: &'static str,
        rows: usize,
        hint: &str,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
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
        let response = ui::text_field(("writer-property", id, key), draft)
            .lines(rows)
            .hint(hint)
            .label(hint)
            .show(ui, theme);
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
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.panel);
        let target = self.selected.or(self.open);
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("writer-inspector")
                .max_rect(rect.shrink(theme.space(Space::S4)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let mut run: Option<String> = None;
        Density::Compact.scope(ui, |ui| {
            // The switch between the item and the comments, while there
            // are comments (or they are shown).
            if self.comments || !self.editor.comments().is_empty() {
                let names = [tr!("inspector"), tr!("comments")];
                let chosen = usize::from(self.comments);
                let picked = ui::segmented("writer-inspector-tabs", &names, Some(chosen))
                    .icons(&[Icon::Info, Icon::Comment])
                    .full_width()
                    .show(ui, theme)
                    .picked;
                if let Some(picked) = picked {
                    self.comments = picked == 1;
                }
                ui.add_space(theme.space(Space::S4));
            }
            if self.comments {
                ui::ScrollView::vertical("writer-comments")
                    .on(theme.panel)
                    .show(ui, theme, |ui| self.comments_ui(ui, cx, theme));
                return;
            }
            ui::ScrollView::vertical("writer-inspector")
                .on(theme.panel)
                .show(ui, theme, |ui| {
                    self.inspector_body(ui, target, cx, theme, &mut run);
                });
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

    /// What the inspector shows of `target`.
    #[allow(clippy::too_many_lines, reason = "the inspector's sections in order")]
    fn inspector_body(
        &mut self,
        ui: &mut Ui,
        target: Option<EntityId>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let Some(id) = target else {
            ui::empty_state("writer-inspector-empty", &tr!("inspector-empty"))
                .icon(Icon::Info)
                .show(ui, theme);
            return;
        };
        let Ok(entity) = cx.arena().get_by_id(id) else {
            return;
        };
        let name = entity.name().to_owned();
        let is_document = entity.string_buffer().is_some();
        let in_project = self.project.is_some();
        let gap = theme.space(Space::S5);
        ui.label(
            RichText::new(name)
                .font(theme.text_font(TextRole::Heading))
                .color(theme.text),
        );
        ui.add_space(theme.space(Space::S4));
        if in_project {
            ui::heading(ui, &tr!("synopsis"), theme);
            self.property_field(
                ui,
                id,
                manuscript::SYNOPSIS,
                4,
                &tr!("what-happens-here-line-two"),
                cx,
                theme,
            );
            ui.add_space(theme.space(Space::S4));
            self.properties_ui(ui, id, cx, theme, run);
            ui.add_space(gap);
            ui::heading(ui, &tr!("keywords"), theme);
            self.property_field(
                ui,
                id,
                manuscript::KEYWORDS,
                1,
                &tr!("comma-separated"),
                cx,
                theme,
            );
        }
        if is_document {
            ui.add_space(gap);
            ui::heading(ui, &tr!("statistics"), theme);
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
            let muted = |text: String| RichText::new(text).color(theme.text_muted());
            ui.label(muted(tr!(
                "words-characters-paragraphs",
                count = text.chars().filter(|ch| *ch != '\n').count(),
                words = words,
                paragraphs = paragraphs
            )));
            if !selection.is_empty() {
                ui.label(muted(tr!(
                    "selected-word-count",
                    words = sue_model::word_count(selection)
                )));
            }
            ui.label(muted(tr!(
                "reading-time",
                minutes = (words / WORDS_PER_MINUTE).max(1)
            )));
        }
        if in_project {
            ui.add_space(gap);
            ui::heading(ui, &tr!("notes"), theme);
            self.property_field(
                ui,
                id,
                manuscript::NOTES,
                6,
                &tr!("notes-for-yourself"),
                cx,
                theme,
            );
        }
        if is_document && in_project {
            ui.add_space(gap);
            self.snapshots_ui(ui, id, cx, theme, run);
        }
        // The headings of what is shown, to jump to.
        let outline = self.editor.outline();
        if !outline.is_empty() && self.view == View::Document {
            ui.add_space(gap);
            ui::heading(ui, &tr!("outline"), theme);
            for (index, entry) in outline.iter().enumerate() {
                let indent = f32::from(entry.level.saturating_sub(1)) * theme.space(Space::S5);
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    let response = ui::link(("writer-outline", index), &entry.title)
                        .role(TextRole::Caption)
                        .show(ui, theme);
                    let response = ui::tooltip(response, &Tip::new(&entry.title), theme);
                    if response.clicked()
                        && let Some(doc) = self.editor.docs().get(entry.at.doc).copied()
                    {
                        self.editor.select(doc, entry.at.offset..entry.at.offset);
                    }
                });
            }
        }
    }

    /// The label, the status, whether it is exported, and the word target
    /// with how far along it is.
    fn properties_ui(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let entity = cx.arena().get_by_id(id).ok();
        let text = |key: &str| {
            entity
                .and_then(|entity| entity.property(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let label = text(manuscript::LABEL);
        let status = text(manuscript::STATUS);
        let included = entity.is_some_and(manuscript::included);
        // A list of every label and of every status, the last answer
        // being none.
        let mut labels: Vec<String> = LABELS
            .iter()
            .map(|(name, _)| project::label_text(name))
            .collect();
        labels.push(tr!("no-label"));
        let mut statuses: Vec<String> = STATUSES
            .iter()
            .map(|name| project::status_text(name))
            .collect();
        statuses.push(tr!("no-status"));
        let mut label_at = label
            .as_deref()
            .and_then(|label| LABELS.iter().position(|(name, _)| *name == label))
            .unwrap_or(LABELS.len());
        let mut status_at = status
            .as_deref()
            .and_then(|status| STATUSES.iter().position(|name| *name == status))
            .unwrap_or(STATUSES.len());
        let half = (ui.available_width() - theme.space(Space::S2)) / 2.0;
        ui.horizontal(|ui| {
            let picked = ui::dropdown(("writer-label", id), &mut label_at, &labels)
                .label(&tr!("label"))
                .width(half)
                .show(ui, theme);
            let picked = ui::tooltip(picked, &Tip::new(&tr!("label")), theme);
            if picked.changed() {
                *run = Some(LABELS.get(label_at).map_or_else(
                    || "label.none".to_owned(),
                    |(name, _)| format!("label.{name}"),
                ));
            }
            let picked = ui::dropdown(("writer-status", id), &mut status_at, &statuses)
                .label(&tr!("status"))
                .width(half)
                .show(ui, theme);
            let picked = ui::tooltip(picked, &Tip::new(&tr!("status")), theme);
            if picked.changed() {
                *run = Some(
                    STATUSES
                        .get(status_at)
                        .map_or_else(|| "status.none".to_owned(), |name| format!("status.{name}")),
                );
            }
        });
        ui.add_space(theme.space(Space::S3));
        let export = tr!("include-in-export-2");
        let response = ui::checkbox(("writer-include", id), &export, included).show(ui, theme);
        if ui::tooltip(response, &Tip::new(&export), theme).clicked() {
            *run = Some("project.include".to_owned());
        }
        ui.add_space(theme.space(Space::S3));
        let target_words = cx
            .arena()
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(manuscript::TARGET))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let mut words_target = target_words as f64;
        let name = tr!("word-target");
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).color(theme.text_muted()));
            let response = ui::number_field(("writer-target", id), &mut words_target)
                .range(0.0..=MOST_WORDS)
                .decimals(0)
                .speed(WORDS_PER_POINT)
                .label(&name)
                .show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&name), theme);
            let settled = response.drag_stopped() || (response.changed() && !response.dragged());
            let wanted = words_target.round() as i64;
            if settled && wanted != target_words {
                let value = (wanted > 0).then_some(Value::Int(wanted));
                let _ = cx.apply(ArenaEdit(project::set(id, manuscript::TARGET, value)));
            }
        });
        if target_words > 0 {
            let words = manuscript::total_words(cx.arena(), id) as f32;
            ui.add_space(theme.space(Space::S2));
            ui::progress_bar(words / target_words as f32).show(ui, theme);
        }
    }

    /// The document's snapshots: Take, and each one with Restore.
    fn snapshots_ui(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        ui.horizontal(|ui| {
            ui::heading(ui, &tr!("snapshots"), theme);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = tr!("take-snapshot");
                let keys = keys_of("project.snapshot", cx);
                let response = ui::button("writer-snapshot-take", &tr!("take"))
                    .ghost()
                    .icon(Icon::Plus)
                    .show(ui, theme);
                let tip = Tip::new(&name).shortcut(&keys);
                if ui::tooltip(response, &tip, theme).clicked() {
                    *run = Some("project.snapshot".to_owned());
                }
            });
        });
        let shots = project::snapshots(cx.arena(), id);
        if shots.is_empty() {
            ui.label(
                RichText::new(tr!("none-yet-take-one-before-big"))
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
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
            let title = saved.name().to_owned();
            let note = tr!("word-count", count = words);
            let restore_name = tr!("restore");
            let actions = [ui::RowAction::new(Icon::History, &restore_name)];
            let row = ui::list_row(("writer-snapshot", *shot), &title)
                .icon(Icon::Recent)
                .note(&note)
                .actions(&actions)
                .show(ui, theme);
            if row.action == Some(0) {
                restore = Some(*shot);
            }
            let help = tr!("put-version-back-what-there-now");
            let _ = ui::tooltip(row.response, &Tip::new(&title).help(&help), theme);
        }
        if let Some(shot) = restore
            && let Some(batch) = project::restore(cx.arena(), id, shot, &project::today())
        {
            self.apply(cx, batch);
        }
    }
}
