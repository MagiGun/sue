//! Scenes of Sue Writer's dialogs on Sue's dialog host: Project
//! Settings, Header and Footer, New Project, Insert Table, Link, Writing
//! Statistics, Keyboard Shortcuts and a new comment.

use super::super::Tour;

/// Scenes 18g to 18n. Each dialog over the novel's pages, opened by the
/// command its menu runs.
pub(super) fn dialogs(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    for (command, name) in [
        ("project.settings", "18g-sue-writer-project-settings"),
        ("insert.header_footer", "18h-sue-writer-header-footer"),
        ("file.new", "18i-sue-writer-new-project"),
        ("table.insert", "18j-sue-writer-insert-table"),
        ("insert.link", "18k-sue-writer-link"),
        ("project.statistics", "18l-sue-writer-statistics"),
        ("help.shortcuts", "18m-sue-writer-shortcuts"),
        ("review.comment", "18n-sue-writer-comment"),
    ] {
        tour.writer_go(&["dialog/close", "view/document"]);
        tour.command(&writer, command);
        tour.shoot(name)?;
    }
    tour.writer_go(&["dialog/close"]);
    Ok(())
}
