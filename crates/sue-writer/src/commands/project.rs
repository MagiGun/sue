//! The Project menu: documents split, merged, copied and trashed,
//! snapshots, status and labels, statistics and the project's settings.

use sue_core::Value;
use sue_model::manuscript;
use sue_wm::{AppletContext, ArenaEdit, Call, Command, Facts, Flags, ItemState};

use crate::project;
use crate::studio::{Dialog, Settings, SueWriter};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "project", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("project.split", "split-at-selection").keys("Ctrl+Shift+K"),
    c("project.merge", "merge-with-next"),
    c("project.duplicate", "duplicate"),
    c("project.trash", "move-to-trash"),
    c("project.empty_trash", "empty-trash"),
    c("project.snapshot", "take-snapshot").keys("Ctrl+5"),
    c("project.include", "include-in-export"),
    // `status.draft`: one of `project::STATUSES`, or `none`.
    c("status.", "status").with(Flags::FAMILY),
    // `label.idea`: one of `project::LABELS`, or `none`.
    c("label.", "label").with(Flags::FAMILY),
    c("project.statistics", "writing-statistics-ellipsis").with(Flags::PALETTE),
    c("project.settings", "project-settings-ellipsis"),
];

/// Does what a Project menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    let sheet = writer.sheet(cx);
    match command {
        "project.settings" => {
            let fonts = writer.fonts(cx);
            writer.dialog = Some(Dialog::Settings(Settings {
                title: writer.look(cx).title,
                author: writer.project_text(cx, manuscript::AUTHOR, ""),
                sheet: sheet.key.clone(),
                page: writer.project_text(cx, project::PAGE, "Letter"),
                landscape: writer
                    .project_entity(cx)
                    .and_then(|entity| entity.property(project::LANDSCAPE))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                margin: writer.project_number(cx, project::MARGIN, 72.0) as f32,
                font: fonts.body,
                heading_font: fonts.heading,
                size: fonts.size,
                target: writer.project_number(cx, manuscript::TARGET, 0.0) as i64,
                session: writer.project_number(cx, project::SESSION_TARGET, 500.0) as i64,
                font_filter: String::new(),
            }));
        }
        "project.statistics" => writer.dialog = Some(Dialog::Statistics),
        "project.split" => {
            if let Some(doc) = writer.editor.current() {
                let at = writer.editor.selection().start;
                if let Some((batch, new)) = project::split(cx.arena(), doc, at, "Untitled")
                    && writer.apply(cx, batch)
                {
                    writer.selected = Some(new);
                    writer.open = Some(new);
                    writer.editor.select(new, 0..0);
                }
            }
        }
        "project.merge" => {
            // Joins the selected document with the one after it.
            if let Some(doc) = writer.selected
                && let Some(parent) = manuscript::parent(cx.arena(), doc)
            {
                let siblings = manuscript::children(cx.arena(), parent);
                let at = siblings.iter().position(|id| *id == doc).unwrap_or(0);
                if let Some(next) = siblings.get(at + 1)
                    && let Some(batch) = project::merge(cx.arena(), &[doc, *next])
                {
                    writer.apply(cx, batch);
                }
            }
        }
        "project.duplicate" => {
            if let Some(id) = writer.selected
                && let Some(batch) = project::duplicate(cx.arena(), id)
            {
                writer.apply(cx, batch);
            }
        }
        "project.trash" => {
            if let Some(id) = writer.selected
                && let Some(batch) = project::trash(cx.arena(), id)
            {
                let parent = manuscript::parent(cx.arena(), id);
                if writer.apply(cx, batch) {
                    if writer.open == Some(id) {
                        writer.open = parent;
                    }
                    writer.selected = parent;
                }
            }
        }
        "project.empty_trash" => {
            if let Some(root) = writer.project
                && let Some(batch) = project::empty_trash(cx.arena(), root)
            {
                writer.apply(cx, batch);
            }
        }
        "project.snapshot" => {
            if let Some(doc) = writer.editor.current().or(writer.selected) {
                let taken = project::today();
                if let Some(batch) =
                    project::take_snapshot(cx.arena(), doc, &tr!("snapshot", taken = taken), &taken)
                    && writer.apply(cx, batch)
                {
                    cx.notify(tr!("took-a-snapshot"));
                }
            }
        }
        "project.include" => {
            if let Some(id) = writer.selected {
                let on = cx.arena().get_by_id(id).is_ok_and(manuscript::included);
                let _ = cx.apply(ArenaEdit(project::set(
                    id,
                    manuscript::INCLUDE,
                    Some(Value::Bool(!on)),
                )));
            }
        }
        command if command.starts_with("status.") || command.starts_with("label.") => {
            let (key, value) = if let Some(value) = command.strip_prefix("status.") {
                (manuscript::STATUS, value)
            } else {
                (manuscript::LABEL, &command["label.".len()..])
            };
            if let Some(id) = writer.selected {
                let value = (value != "none").then(|| Value::Text(value.to_owned()));
                let _ = cx.apply(ArenaEdit(project::set(id, key, value)));
            }
        }
        _ => {}
    }
}

/// Whether a Project menu command works now, and its check mark.
fn state(writer: &SueWriter, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let has_project = writer.project.is_some();
    let has_text = writer.editor.current().is_some();
    match call.name {
        "project.include" => ItemState {
            enabled: writer.selected.is_some(),
            checked: writer
                .selected
                .and_then(|id| facts.arena.get_by_id(id).ok())
                .map(manuscript::included),
        },
        "project.duplicate"
        | "project.trash"
        | "project.empty_trash"
        | "project.merge"
        | "project.statistics"
        | "project.settings" => {
            if has_project {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        command if command.starts_with("status.") || command.starts_with("label.") => {
            if has_project {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        "project.split" | "project.snapshot" => {
            if has_text {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        _ => ItemState::on(),
    }
}
