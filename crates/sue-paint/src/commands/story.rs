//! The Story menu's commands: a work's pages.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "story", super::nothing).state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("story.manager", "page-manager-ellipsis"),
    c("story.first", "first-page"),
    c("story.previous", "previous-page"),
    c("story.next", "next-page"),
    c("story.last", "last-page"),
    c("story.add", "add-page-ellipsis"),
    c("story.insert", "add-page-advanced-ellipsis"),
    c("story.delete", "delete-page"),
    c("story.order", "page-reordering-ellipsis"),
    c("story.combine", "combine-pages"),
    c("story.separate", "split-pages"),
    c("story.work_settings", "change-project-settings-ellipsis"),
    c("story.page_settings", "change-page-settings-ellipsis"),
    c("story.info", "change-story-information-ellipsis"),
    c("story.editor", "story-editor"),
    c("story.import_text", "batch-import-text-ellipsis"),
    c("story.open_page", "open-page"),
    c("story.duplicate_page", "duplicate-page"),
    c("story.export_page", "export-page-as-file-ellipsis"),
    c("story.export_pages", "export-all-pages-ellipsis"),
];
