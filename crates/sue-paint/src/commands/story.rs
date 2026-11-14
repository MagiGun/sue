//! The Story menu's commands: a work's pages.
//!
//! Each command is an entry of [`COMMANDS`] with its own `run` and
//! `state`. Clip Studio Paint ships no default key for any of them, so
//! none has a chord here; people give them keys in Settings.

use sue_wm::{AppletContext, Call, Command};

use crate::dialogs::{Dialog, PagesDialog};
use crate::doc;
use crate::story;
use crate::studio::SuePaint;

impl SuePaint {
    /// Runs a Story menu command; `false` if `command` is not one.
    pub(crate) fn story_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        let Some((entry, call)) = super::registry().find(command) else {
            return false;
        };
        if entry.group != "story" {
            return false;
        }
        (entry.run)(self, cx, call);
        true
    }
}

/// Page Manager, Page Reordering, Open Page and Change Project Settings:
/// the Page Manager, on the page shown.
fn manager(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let chosen = paint
        .story_here(&cx.world.arena)
        .map_or(0, |(_, _, index)| index);
    paint.dialog = Some(Dialog::Pages(PagesDialog { chosen }));
}

/// First, Previous, Next and Last Page.
fn step(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some((_, pages, index)) = paint.story_here(&cx.world.arena) else {
        cx.notify(tr!("canvas-one-picture-story-add-page"));
        return;
    };
    let to = match call.name {
        "story.first" => 0,
        "story.previous" => index.saturating_sub(1),
        "story.next" => (index + 1).min(pages.len() - 1),
        _ => pages.len() - 1,
    };
    if to != index {
        paint.go_to_page(cx, pages[to]);
    }
}

/// Add Page puts a page after the one shown, Insert Page before it; the
/// picture becomes a story's first page if it is in none.
fn add(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let Some(story) = paint.ensure_story(cx) else {
        return;
    };
    let index = story::pages(&cx.world.arena, story)
        .iter()
        .position(|page| *page == picture)
        .unwrap_or(0);
    let at = if call.name == "story.add" {
        index + 1
    } else {
        index
    };
    match story::add_page(&cx.world.arena, story, picture, at) {
        Ok((edit, page)) => {
            paint.closed.insert(page);
            if SuePaint::apply(cx, edit) {
                paint.go_to_page(cx, page);
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// Duplicate Page: a copy of the page shown, after it.
fn duplicate(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let Some(story) = paint.ensure_story(cx) else {
        return;
    };
    match story::duplicate_page(&cx.world.arena, story, picture) {
        Ok((edit, page)) => {
            paint.closed.insert(page);
            if SuePaint::apply(cx, edit) {
                paint.go_to_page(cx, page);
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// Delete Page, then the page after it (or before it, at the end).
fn delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((_, pages, index)) = paint.story_here(&cx.world.arena) else {
        cx.fail(tr!("delete-page-works-storys-pages"));
        return;
    };
    if pages.len() < 2 {
        cx.fail(tr!("story-keeps-least-one-page"));
        return;
    }
    let next = if index + 1 < pages.len() {
        pages[index + 1]
    } else {
        pages[index - 1]
    };
    let edit = story::delete_page(&cx.world.arena, pages[index]);
    if SuePaint::apply(cx, edit) {
        paint.closed.remove(&next);
        paint.activate(&cx.world.arena, next);
    }
}

/// Where the page at `index` of `pages` is among the spreads of `story`:
/// the index of the page it faces, if it faces one.
fn facing(
    arena: &sue_core::Arena,
    story: sue_core::EntityId,
    pages: &[sue_core::EntityId],
    index: usize,
) -> Option<usize> {
    story::spreads_from(
        pages.len(),
        story::bound_right(arena, story),
        story::starts_paired(arena, story),
    )
    .into_iter()
    .find(|pair| pair.contains(&Some(index)))
    .and_then(|pair| pair.into_iter().flatten().find(|other| *other != index))
}

/// Combine Pages: the page shown and the page it faces become one spread.
fn combine(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((story, pages, index)) = paint.story_here(&cx.world.arena) else {
        cx.fail(tr!("combine-pages-works-storys-pages"));
        return;
    };
    let arena = &cx.world.arena;
    let Some(partner) = facing(arena, story, &pages, index) else {
        cx.fail(tr!("page-faces-no-other-page-add"));
        return;
    };
    let (first, second) = (index.min(partner), index.max(partner));
    // Read right to left, the later page is on the left.
    let (left, right) = if story::bound_right(arena, story) {
        (pages[second], pages[first])
    } else {
        (pages[first], pages[second])
    };
    match story::combine(arena, story, left, right) {
        Ok(edit) => {
            if SuePaint::apply(cx, edit) {
                paint.closed.remove(&left);
                paint.go_to_page(cx, left);
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// `true` if `page` is a spread that Combine Pages made.
fn is_spread(arena: &sue_core::Arena, page: sue_core::EntityId) -> bool {
    arena
        .get_by_id(page)
        .ok()
        .and_then(|entity| entity.property(story::SPREAD))
        .and_then(sue_core::Value::as_bool)
        == Some(true)
}

/// Split Pages: a spread becomes its two pages again.
fn separate(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((story, pages, index)) = paint.story_here(&cx.world.arena) else {
        cx.fail(tr!("separate-pages-works-storys-spread"));
        return;
    };
    let page = pages[index];
    if !is_spread(&cx.world.arena, page) {
        cx.fail(tr!("page-not-spread-combine-pages-makes"));
        return;
    }
    match story::separate(&cx.world.arena, story, page) {
        Ok((edit, new_page)) => {
            paint.closed.insert(new_page);
            SuePaint::apply(cx, edit);
        }
        Err(error) => cx.fail(error),
    }
}

/// Change Page Settings, filled from the page shown.
fn page_settings(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((story, _, _)) = paint.story_here(&cx.world.arena) else {
        cx.fail(tr!("change-page-settings-works-story"));
        return;
    };
    let Some(picture) = paint.picture() else {
        return;
    };
    let entity = cx.world.arena.get_by_id(picture).ok();
    let number = |key: &str, default: f64| {
        entity
            .and_then(|entity| entity.property(key))
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(default)
    };
    let [width, height] = entity.and_then(sue_model::picture::size).unwrap_or([1, 1]);
    paint.dialog = Some(Dialog::PageSettings(crate::dialogs::PageSettings {
        story,
        width,
        height,
        dpi: number(sue_model::picture::DPI, 72.0),
        bleed: number(story::PAGE_BLEED, 3.0),
        inner: number(story::PAGE_INNER, 10.0),
    }));
}

/// Change Story Information; the picture becomes a story if it is in none.
fn info(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(story) = paint.ensure_story(cx) else {
        return;
    };
    let entity = cx.world.arena.get_by_id(story).ok();
    paint.dialog = Some(Dialog::StoryInfo(crate::dialogs::StoryInfo {
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
    }));
}

/// Story Editor: every text layer of the story's pages, in page order.
fn editor(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let arena = &cx.world.arena;
    let pages = paint.story_here(arena).map_or_else(
        || paint.picture().into_iter().collect(),
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
        return;
    }
    paint.dialog = Some(Dialog::StoryEditor(texts));
}

/// Batch Import Text: asks for the text file.
fn import_text(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_none() {
        cx.fail(tr!("open-make-page-first"));
        return;
    }
    let start = paint
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

/// Export Page as File: the page shown, as Export as PNG writes it.
fn export_page(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.run("file.export.png", cx);
}

/// Export All Pages: asks where.
fn export_pages(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.ask_export_pages(cx);
}

/// A command of this group.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "story", run)
        .icon(icon)
        .state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "story.manager",
        "page-manager-ellipsis",
        "doc-comic",
        manager,
    ),
    c("story.first", "first-page", "chevrons-left", step),
    c("story.previous", "previous-page", "chevron-left", step),
    c("story.next", "next-page", "chevron-right", step),
    c("story.last", "last-page", "chevrons-right", step),
    c("story.add", "add-page-ellipsis", "file-new", add),
    c("story.insert", "add-page-advanced-ellipsis", "plus", add),
    c("story.delete", "delete-page", "delete", delete),
    c(
        "story.order",
        "page-reordering-ellipsis",
        "move-to",
        manager,
    ),
    c("story.combine", "combine-pages", "book", combine),
    c("story.separate", "split-pages", "cut", separate),
    c(
        "story.work_settings",
        "change-project-settings-ellipsis",
        "settings",
        manager,
    ),
    c(
        "story.page_settings",
        "change-page-settings-ellipsis",
        "doc-comic-settings",
        page_settings,
    ),
    c(
        "story.info",
        "change-story-information-ellipsis",
        "info",
        info,
    ),
    c("story.editor", "story-editor", "edit", editor),
    c(
        "story.import_text",
        "batch-import-text-ellipsis",
        "import",
        import_text,
    ),
    c("story.open_page", "open-page", "file-open", manager),
    c(
        "story.duplicate_page",
        "duplicate-page",
        "duplicate",
        duplicate,
    ),
    c(
        "story.export_page",
        "export-page-as-file-ellipsis",
        "export",
        export_page,
    ),
    c(
        "story.export_pages",
        "export-all-pages-ellipsis",
        "export",
        export_pages,
    ),
];
