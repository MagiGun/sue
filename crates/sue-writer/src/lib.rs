//! # sue-writer: Sue Writer, the writing and publishing studio
//!
//! Sue Writer fills Sue's page with a place to write anything from a
//! letter to a trilogy: a word processor's pages and toolbar, a binder of
//! chapters and scenes as in writing studios for long work, a corkboard of
//! index cards, an outliner, an inspector for synopses, notes, labels,
//! status, targets and snapshots, composition mode, find and replace
//! across the project, and compiling to PDF, Word, OpenDocument, EPUB,
//! HTML, RTF, Markdown and plain text.
//!
//! It owns little of its own; it puts Sue's shared features together:
//!
//! - A project is the shared manuscript of [`sue_model::manuscript`], its
//!   documents rich text ([`sue_model::TextMarks`]), so every other studio
//!   reads them, and they save in the `.sue` project.
//! - Editing, styles, pages and typesetting are `sue-type`'s, fonts are
//!   `sue_wm::fonts`, the publishing formats are `sue-io`'s.
//! - Every change is a transaction of the global undo history.
//! - Pictures from Sue Paint go straight into the text (the Sue menu's
//!   Send To), drawn with Sue Paint's own compositing.

mod compile;
mod menus;
mod project;
mod studio;
mod ui;

pub use compile::{FORMATS, Look, Options, Separator, Titles, compile, import, sections};
pub use project::{TEMPLATES, create};
pub use studio::{ID, SueWriter};

use sue_core::{EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::mark_kind as kind;
use sue_wm::{AppletContext, ArenaEdit};

/// Makes a sample novel with a few written scenes (formatting, a list, a
/// footnote, synopses, labels and status), every step undoable. Used by
/// the snapshots and the self-check. Returns the project.
pub fn sample_project(cx: &mut AppletContext<'_>) -> Option<EntityId> {
    let fonts = sue_type::Fonts::installed(12.0);
    let (batch, root) = project::create("novel", "The Lighthouse Keeper", &fonts);
    cx.apply(ArenaEdit(batch)).ok()?;
    cx.apply(ArenaEdit(project::set(
        root,
        manuscript::AUTHOR,
        Some(Value::Text("A. Writer".into())),
    )))
    .ok()?;
    let draft = manuscript::folder(cx.arena(), root, role::DRAFT)?;
    let chapters = manuscript::children(cx.arena(), draft);
    let scenes: Vec<EntityId> = chapters
        .iter()
        .filter_map(|chapter| manuscript::children(cx.arena(), *chapter).first().copied())
        .collect();
    let first = [
        "The storm came in off the water a little after six, the way storms always did on that coast: first the gulls went quiet, then the light turned the color of old pewter, and then the rain arrived all at once, as if someone had opened a door.",
        "Mara had kept the light for eleven years. She knew every sound the tower made. Tonight it made a new one.",
        "She took stock of what she had:",
        "A lamp with oil for three nights",
        "Her father's logbook",
        "One letter she had never opened",
        "Somewhere below, a bell rang once, though there was no bell on the island.",
    ];
    let second = [
        "By morning the sea had laid a line of wreckage along the north beach: rope, a crate stamped with a name she did not know, and a man, breathing.",
        "He would not say where he had come from. He asked only whether the light had stayed lit all night, and when she said it had, he closed his eyes and smiled.",
    ];
    for (doc, paragraphs, synopsis, label, status) in [
        (
            scenes.first(),
            &first[..],
            "The storm, and a sound the tower has never made.",
            "Scene",
            "Revised Draft",
        ),
        (
            scenes.get(1),
            &second[..],
            "A stranger washes up after the storm.",
            "Scene",
            "First Draft",
        ),
    ] {
        let Some(doc) = doc.copied() else {
            continue;
        };
        let lines: Vec<(&str, String)> = paragraphs
            .iter()
            .map(|line| (sue_type::BODY, (*line).to_owned()))
            .collect();
        let (text, mut marks) = project::rich(&lines);
        if let Some(at) = text.find("new one") {
            marks.set(at..at + 7, kind::ITALIC, Some(Value::Bool(true)));
        }
        if let Some(at) = text.find("eleven years") {
            marks.set(
                at..at + 12,
                kind::FOOTNOTE,
                Some(Value::Text("Since the winter her father died.".into())),
            );
        }
        if let Some(at) = text.find("breathing") {
            marks.set(at..at + 9, kind::BOLD, Some(Value::Bool(true)));
        }
        for item in ["A lamp", "Her father's", "One letter"] {
            if let Some(at) = text.find(item) {
                marks.set_paragraphs(
                    &text,
                    at..at,
                    kind::LIST,
                    Some(Value::Text("bullet".into())),
                );
            }
        }
        let step = project::replace_all(cx.arena(), doc, &text, marks, "Write")?;
        cx.apply(ArenaEdit(step)).ok()?;
        let mut batch = sue_core::txn::Batch::new("Card");
        batch.push(Box::new(project::set(
            doc,
            manuscript::SYNOPSIS,
            Some(Value::Text(synopsis.into())),
        )));
        batch.push(Box::new(project::set(
            doc,
            manuscript::LABEL,
            Some(Value::Text(label.into())),
        )));
        batch.push(Box::new(project::set(
            doc,
            manuscript::STATUS,
            Some(Value::Text(status.into())),
        )));
        cx.apply(ArenaEdit(batch)).ok()?;
    }
    Some(root)
}
