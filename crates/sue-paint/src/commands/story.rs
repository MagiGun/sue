//! The Story menu's commands: a work's pages.

use sue_wm::{AppletContext, Command};

use crate::dialogs::PagesDialog;
use crate::doc;
use crate::story;
use crate::studio::SuePaint;

impl SuePaint {
    /// Runs a Story menu command; `false` if `command` is not one.
    pub(crate) fn story_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        match command {
            "story.manager" | "story.order" | "story.open_page" | "story.work_settings" => {
                let chosen = self
                    .story_here(&cx.world.arena)
                    .map_or(0, |(_, _, index)| index);
                self.dialog = Some(crate::dialogs::Dialog::Pages(PagesDialog { chosen }));
            }
            "story.first" | "story.previous" | "story.next" | "story.last" => {
                let Some((_, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.notify(tr!("canvas-one-picture-story-add-page"));
                    return true;
                };
                let to = match command {
                    "story.first" => 0,
                    "story.previous" => index.saturating_sub(1),
                    "story.next" => (index + 1).min(pages.len() - 1),
                    _ => pages.len() - 1,
                };
                if to != index {
                    self.go_to_page(cx, pages[to]);
                }
            }
            "story.add" | "story.insert" => {
                let Some(picture) = self.picture() else {
                    return true;
                };
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                let index = story::pages(&cx.world.arena, story)
                    .iter()
                    .position(|page| *page == picture)
                    .unwrap_or(0);
                let at = if command == "story.add" {
                    index + 1
                } else {
                    index
                };
                match story::add_page(&cx.world.arena, story, picture, at) {
                    Ok((edit, page)) => {
                        self.closed.insert(page);
                        if Self::apply(cx, edit) {
                            self.go_to_page(cx, page);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.duplicate_page" => {
                let Some(picture) = self.picture() else {
                    return true;
                };
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                match story::duplicate_page(&cx.world.arena, story, picture) {
                    Ok((edit, page)) => {
                        self.closed.insert(page);
                        if Self::apply(cx, edit) {
                            self.go_to_page(cx, page);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.delete" => {
                let Some((_, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("delete-page-works-storys-pages"));
                    return true;
                };
                if pages.len() < 2 {
                    cx.fail(tr!("story-keeps-least-one-page"));
                    return true;
                }
                let next = if index + 1 < pages.len() {
                    pages[index + 1]
                } else {
                    pages[index - 1]
                };
                let edit = story::delete_page(&cx.world.arena, pages[index]);
                if Self::apply(cx, edit) {
                    self.closed.remove(&next);
                    self.activate(&cx.world.arena, next);
                }
            }
            "story.combine" => {
                let Some((story, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("combine-pages-works-storys-pages"));
                    return true;
                };
                let arena = &cx.world.arena;
                let right_bound = story::bound_right(arena, story);
                // The page it faces in the spread.
                let partner = story::spreads_from(
                    pages.len(),
                    right_bound,
                    story::starts_paired(arena, story),
                )
                .into_iter()
                .find(|pair| pair.contains(&Some(index)))
                .and_then(|pair| pair.into_iter().flatten().find(|other| *other != index));
                let Some(partner) = partner else {
                    cx.fail(tr!("page-faces-no-other-page-add"));
                    return true;
                };
                let (first, second) = (index.min(partner), index.max(partner));
                // Read right to left, the later page is on the left.
                let (left, right) = if right_bound {
                    (pages[second], pages[first])
                } else {
                    (pages[first], pages[second])
                };
                match story::combine(arena, story, left, right) {
                    Ok(edit) => {
                        if Self::apply(cx, edit) {
                            self.closed.remove(&left);
                            self.go_to_page(cx, left);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.separate" => {
                let Some((story, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("separate-pages-works-storys-spread"));
                    return true;
                };
                let page = pages[index];
                let spread = cx
                    .world
                    .arena
                    .get_by_id(page)
                    .ok()
                    .and_then(|entity| entity.property(story::SPREAD))
                    .and_then(sue_core::Value::as_bool)
                    == Some(true);
                if !spread {
                    cx.fail(tr!("page-not-spread-combine-pages-makes"));
                    return true;
                }
                match story::separate(&cx.world.arena, story, page) {
                    Ok((edit, new_page)) => {
                        self.closed.insert(new_page);
                        Self::apply(cx, edit);
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.page_settings" => {
                let Some((story, _, _)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("change-page-settings-works-story"));
                    return true;
                };
                let Some(picture) = self.picture() else {
                    return true;
                };
                let entity = cx.world.arena.get_by_id(picture).ok();
                let number = |key: &str, default: f64| {
                    entity
                        .and_then(|entity| entity.property(key))
                        .and_then(sue_core::Value::as_f64)
                        .unwrap_or(default)
                };
                let [width, height] = entity.and_then(sue_model::picture::size).unwrap_or([1, 1]);
                self.dialog = Some(crate::dialogs::Dialog::PageSettings(
                    crate::dialogs::PageSettings {
                        story,
                        width,
                        height,
                        dpi: number(sue_model::picture::DPI, 72.0),
                        bleed: number(story::PAGE_BLEED, 3.0),
                        inner: number(story::PAGE_INNER, 10.0),
                    },
                ));
            }
            "story.info" => {
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                let entity = cx.world.arena.get_by_id(story).ok();
                self.dialog = Some(crate::dialogs::Dialog::StoryInfo(
                    crate::dialogs::StoryInfo {
                        story,
                        title: entity
                            .map(|entity| entity.name().to_owned())
                            .unwrap_or_default(),
                        author: entity
                            .and_then(|entity| entity.property(story::AUTHOR))
                            .and_then(sue_core::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        first: entity
                            .and_then(|entity| entity.property(story::FIRST_NUMBER))
                            .and_then(sue_core::Value::as_i64)
                            .unwrap_or(1),
                        right_bound: story::bound_right(&cx.world.arena, story),
                        paired: story::starts_paired(&cx.world.arena, story),
                        folio: story::Folio::of(&cx.world.arena, story),
                    },
                ));
            }
            "story.editor" => {
                let arena = &cx.world.arena;
                let pages = self.story_here(arena).map_or_else(
                    || self.picture().into_iter().collect(),
                    |(_, pages, _)| pages,
                );
                let mut texts = Vec::new();
                for (number, page) in pages.iter().enumerate() {
                    for info in doc::layers(arena, *page) {
                        if let Some(lettering) = arena
                            .get_by_id(info.id)
                            .ok()
                            .and_then(crate::lettering::Lettering::of)
                        {
                            texts.push((number + 1, *page, info.id, lettering.content));
                        }
                    }
                }
                if texts.is_empty() {
                    cx.notify(tr!("no-text-yet-text-tool-story"));
                    return true;
                }
                self.dialog = Some(crate::dialogs::Dialog::StoryEditor(texts));
            }
            "story.import_text" => {
                if self.picture().is_none() {
                    cx.fail(tr!("open-make-page-first"));
                    return true;
                }
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(sue_wm::ShellRequest::PickFile(
                    sue_wm::FilePick::open(
                        Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                        "story-text",
                        &tr!("batch-import-text"),
                    )
                    .extensions(&["txt", "md"])
                    .starting_in(start),
                ));
            }
            "story.export_page" => self.run("file.export.png", cx),
            "story.export_pages" => self.ask_export_pages(cx),
            _ => return false,
        }
        true
    }
}

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
