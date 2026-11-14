//! The Story menu's commands: a work's pages.
//!
//! Each command is an entry of [`COMMANDS`] with its own `run` and
//! `state`. Clip Studio Paint ships no default key for any of them, so
//! none has a chord here; people give them keys in Settings.

use sue_core::{Arena, EntityId};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::when;
use crate::dialogs::{Dialog, PagesDialog};
use crate::doc;
use crate::story;
use crate::studio::SuePaint;

impl SuePaint {
    /// Runs the Story command `command` straight away, without asking its
    /// state first, as the Page Manager's buttons do: a page button that
    /// cannot work says why instead of doing nothing. `false` if
    /// `command` is not a Story command. Menus, keys and Auto Actions go
    /// through [`SuePaint::run`], which grays out what cannot work.
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

/// Works while the picture shown is a page of a story.
fn in_story(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.story_here(facts.arena).is_some())
}

/// First and Previous Page work when a page comes before the one shown,
/// Next and Last Page when one comes after it.
fn step_state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    when(
        paint
            .story_here(facts.arena)
            .is_some_and(|(_, pages, index)| match call.name {
                "story.first" | "story.previous" => index > 0,
                _ => index + 1 < pages.len(),
            }),
    )
}

/// Delete Page works while the story keeps a page after it.
fn delete_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .story_here(facts.arena)
            .is_some_and(|(_, pages, _)| pages.len() > 1),
    )
}

/// Combine Pages works when the page shown faces another page.
fn combine_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .story_here(facts.arena)
            .is_some_and(|(story, pages, index)| {
                facing(facts.arena, story, &pages, index).is_some()
            }),
    )
}

/// Split Pages works on a spread.
fn separate_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .story_here(facts.arena)
            .is_some_and(|(_, pages, index)| is_spread(facts.arena, pages[index])),
    )
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

/// Runs `work` as one undo step called `label`, however many edits it
/// applies. Add Page and Duplicate Page on a picture that is in no story
/// first make it one (an edit of its own); undone, both go together.
fn one_step(cx: &mut AppletContext<'_>, label: String, work: impl FnOnce(&mut AppletContext<'_>)) {
    // Words being typed are a step of their own, before this one.
    cx.typing.flush(cx.history);
    let mark = cx.history.cursor();
    work(cx);
    let mut steps = cx.history.take_since(mark);
    if steps.len() == 1 {
        cx.history.record_boxed(steps.remove(0));
    } else if !steps.is_empty() {
        let mut batch = sue_core::txn::Batch::new(label);
        for step in steps {
            batch.push(step);
        }
        cx.history.record(batch);
    }
}

/// Add Page puts a page after the one shown, Insert Page before it; the
/// picture becomes a story's first page if it is in none.
fn add(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    one_step(cx, tr!("add-page"), |cx| add_page(paint, cx, call));
}

fn add_page(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
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
    one_step(cx, tr!("duplicate-page"), |cx| duplicate_page(paint, cx));
}

fn duplicate_page(paint: &mut SuePaint, cx: &mut AppletContext<'_>) {
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
fn facing(arena: &Arena, story: EntityId, pages: &[EntityId], index: usize) -> Option<usize> {
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
fn is_spread(arena: &Arena, page: EntityId) -> bool {
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

/// The story property that says which way a webtoon's columns of pages
/// read in the Page Manager: `"right"` (right to left) or `"left"` (left
/// to right). Saved with the work; unset, it follows the binding
/// (`story.binding`). Saved data: never renamed without a migration.
pub(crate) const WEBTOON_READING: &str = "story.webtoon_reading";

/// `true` when the webtoon columns of `story` read right to left.
pub(crate) fn webtoon_right_to_left(arena: &Arena, story: EntityId) -> bool {
    match arena
        .get_by_id(story)
        .ok()
        .and_then(|entity| entity.property(WEBTOON_READING))
        .and_then(sue_core::Value::as_str)
    {
        Some("right") => true,
        Some("left") => false,
        _ => story::bound_right(arena, story),
    }
}

/// Webtoon Reading Direction, Right to Left or Left to Right: one undo
/// step, none when it already reads that way.
fn webtoon(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some((story, _, _)) = paint.story_here(&cx.world.arena) else {
        return;
    };
    let right = call.name == "story.webtoon.right_to_left";
    if webtoon_right_to_left(&cx.world.arena, story) == right {
        return;
    }
    let edit =
        doc::Edit::new(tr!("webtoon-reading-direction")).then(sue_core::txn::SetProperty::new(
            story,
            WEBTOON_READING,
            if right { "right" } else { "left" },
        ));
    SuePaint::apply(cx, edit);
}

/// The direction the webtoon reads now carries the check.
fn webtoon_state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint
        .story_here(facts.arena)
        .map_or_else(ItemState::off, |(story, _, _)| {
            super::flag(
                webtoon_right_to_left(facts.arena, story)
                    == (call.name == "story.webtoon.right_to_left"),
            )
        })
}

/// A command of this group that Auto Actions may record: it changes the
/// pages or which page is shown, or opens a dialog on them.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    unrecorded(name, label, icon, run).with(Flags::RECORDABLE)
}

/// A command of this group that Auto Actions leave out, as they leave
/// out the File menu: it reads or writes files, or (the Story Editor)
/// edits the text of every page at once.
const fn unrecorded(
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
    c("story.first", "first-page", "chevrons-left", step).state(step_state),
    c("story.previous", "previous-page", "chevron-left", step).state(step_state),
    c("story.next", "next-page", "chevron-right", step).state(step_state),
    c("story.last", "last-page", "chevrons-right", step).state(step_state),
    c("story.add", "add-page-ellipsis", "file-new", add),
    c("story.insert", "add-page-advanced-ellipsis", "plus", add),
    c("story.delete", "delete-page", "delete", delete).state(delete_state),
    c(
        "story.order",
        "page-reordering-ellipsis",
        "move-to",
        manager,
    ),
    c("story.combine", "combine-pages", "book", combine).state(combine_state),
    c("story.separate", "split-pages", "cut", separate).state(separate_state),
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
    )
    .state(in_story),
    c(
        "story.info",
        "change-story-information-ellipsis",
        "info",
        info,
    ),
    unrecorded("story.editor", "story-editor", "edit", editor),
    unrecorded(
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
    unrecorded(
        "story.export_page",
        "export-page-as-file-ellipsis",
        "export",
        export_page,
    ),
    unrecorded(
        "story.export_pages",
        "export-all-pages-ellipsis",
        "export",
        export_pages,
    )
    .state(in_story),
    // The Page Manager's Webtoon reading direction (CTV-6).
    c(
        "story.webtoon.right_to_left",
        "webtoon-reading-right-to-left",
        "doc-webtoon",
        webtoon,
    )
    .state(webtoon_state)
    .with(Flags::PALETTE),
    c(
        "story.webtoon.left_to_right",
        "webtoon-reading-left-to-right",
        "doc-webtoon",
        webtoon,
    )
    .state(webtoon_state)
    .with(Flags::PALETTE),
];

#[cfg(test)]
mod tests {
    use sue_core::{Arena, EntityId, UndoStack};
    use sue_wm::{AppletContext, Facts, Flags, World};

    use crate::doc::{self, NewPicture};
    use crate::story;
    use crate::studio::SuePaint;

    /// Runs `work` with a studio and an applet context of its own.
    fn with_studio<R>(work: impl FnOnce(&mut SuePaint, &mut AppletContext<'_>) -> R) -> R {
        let rice = sue_rice::RiceSchema::new();
        let theme = sue_wm::Theme::from_rice(&rice);
        let mut world = World {
            arena: Arena::new(),
            rice,
        };
        let mut history = UndoStack::new();
        let mut typing = sue_wm::Typing::default();
        let mut requests = Vec::new();
        let catalog = sue_wm::Catalog::default();
        let mut textures = sue_platform::NoTextures;
        let mut transport = sue_wm::Transport::default();
        let work_list = sue_wm::WorkList::default();
        let mut transfers = sue_wm::Transfers::default();
        let mut cx = AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: sue_wm::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        let mut paint = SuePaint::default();
        work(&mut paint, &mut cx)
    }

    /// A picture of 40 by 30, open and shown.
    fn picture(paint: &mut SuePaint, cx: &mut AppletContext<'_>) -> EntityId {
        let (edit, picture) = doc::create(&NewPicture {
            name: "Page".to_owned(),
            width: 40,
            height: 30,
            ..NewPicture::default()
        })
        .expect("a picture");
        assert!(SuePaint::apply(cx, edit));
        paint.activate(&cx.world.arena, picture);
        assert_eq!(paint.picture(), Some(picture));
        picture
    }

    fn enabled(paint: &SuePaint, cx: &AppletContext<'_>, command: &str) -> bool {
        paint.state(command, &Facts::of(cx)).enabled
    }

    fn pages(paint: &SuePaint, cx: &AppletContext<'_>) -> Vec<EntityId> {
        paint
            .story_here(&cx.world.arena)
            .map(|(_, pages, _)| pages)
            .unwrap_or_default()
    }

    #[test]
    fn auto_actions_record_what_changes_the_pages() {
        // What `extras::recordable` said for "story.", but for the
        // commands that read or write files, which it kept out only under
        // "file.": the Story menu's file commands are left out the same
        // way now. No Story command leaves the canvas.
        let unrecorded: Vec<&str> = super::COMMANDS
            .iter()
            .filter(|command| !command.flags.has(Flags::RECORDABLE))
            .map(|command| command.name)
            .collect();
        assert_eq!(
            unrecorded,
            [
                "story.editor",
                "story.import_text",
                "story.export_page",
                "story.export_pages"
            ]
        );
        assert!(
            super::COMMANDS
                .iter()
                .all(|command| !command.flags.has(Flags::LEAVES_CANVAS))
        );
        for command in super::COMMANDS {
            assert_eq!(
                crate::commands::registry().flags(command.name),
                command.flags,
                "{}",
                command.name
            );
        }
    }

    #[test]
    fn without_a_canvas_every_story_command_is_grayed() {
        with_studio(|paint, cx| {
            for command in super::COMMANDS {
                assert!(!enabled(paint, cx, command.name), "{}", command.name);
            }
        });
    }

    #[test]
    fn story_commands_gray_out_where_they_cannot_work() {
        with_studio(|paint, cx| {
            picture(paint, cx);
            // A lone picture: what makes it a story works, what needs
            // pages does not.
            for command in [
                "story.manager",
                "story.add",
                "story.insert",
                "story.duplicate_page",
                "story.info",
                "story.editor",
                "story.import_text",
                "story.export_page",
            ] {
                assert!(enabled(paint, cx, command), "{command}");
            }
            for command in [
                "story.first",
                "story.previous",
                "story.next",
                "story.last",
                "story.delete",
                "story.combine",
                "story.separate",
                "story.page_settings",
                "story.export_pages",
            ] {
                assert!(!enabled(paint, cx, command), "{command}");
            }
            // Two pages, the second shown: back, not on.
            paint.run("story.add", cx);
            assert_eq!(pages(paint, cx).len(), 2);
            assert_eq!(
                paint.story_here(&cx.world.arena).map(|here| here.2),
                Some(1)
            );
            assert!(enabled(paint, cx, "story.first"));
            assert!(enabled(paint, cx, "story.previous"));
            assert!(!enabled(paint, cx, "story.next"));
            assert!(!enabled(paint, cx, "story.last"));
            assert!(enabled(paint, cx, "story.delete"));
            assert!(enabled(paint, cx, "story.page_settings"));
            assert!(enabled(paint, cx, "story.export_pages"));
            assert!(!enabled(paint, cx, "story.separate"), "not a spread");
            // Bound on the left and starting alone, page 2 faces page 3.
            assert!(
                !enabled(paint, cx, "story.combine"),
                "page 2 faces none yet"
            );
            paint.run("story.add", cx);
            paint.run("story.previous", cx);
            assert_eq!(
                paint.story_here(&cx.world.arena).map(|here| here.2),
                Some(1)
            );
            assert!(enabled(paint, cx, "story.next"));
            assert!(enabled(paint, cx, "story.combine"));
            paint.run("story.combine", cx);
            assert_eq!(pages(paint, cx).len(), 2);
            assert!(enabled(paint, cx, "story.separate"), "a spread");
            paint.run("story.first", cx);
            assert!(!enabled(paint, cx, "story.first"));
            assert!(!enabled(paint, cx, "story.previous"));
            assert!(!enabled(paint, cx, "story.separate"));
        });
    }

    #[test]
    fn a_grayed_command_does_nothing_and_the_page_buttons_still_explain() {
        with_studio(|paint, cx| {
            picture(paint, cx);
            paint.run("story.add", cx);
            paint.run("story.delete", cx);
            assert_eq!(pages(paint, cx).len(), 1);
            let depth = cx.history.depth();
            cx.requests.clear();
            // The menu item is grayed: run through the studio, nothing.
            paint.run("story.delete", cx);
            assert_eq!(cx.history.depth(), depth);
            assert!(cx.requests.is_empty(), "{:?}", cx.requests);
            // The Page Manager's button says why.
            assert!(paint.story_command("story.delete", cx));
            assert_eq!(cx.history.depth(), depth);
            assert!(
                matches!(cx.requests.last(), Some(sue_wm::ShellRequest::Fail(_))),
                "{:?}",
                cx.requests
            );
            assert!(!paint.story_command("layer.new", cx));
            assert!(!paint.story_command("story.nothing", cx));
        });
    }

    /// The checks of the two Webtoon Reading Direction items.
    fn webtoon_checks(paint: &SuePaint, cx: &AppletContext<'_>) -> [Option<bool>; 2] {
        ["story.webtoon.right_to_left", "story.webtoon.left_to_right"]
            .map(|command| paint.state(command, &Facts::of(cx)).checked)
    }

    #[test]
    fn webtoon_reading_direction_is_one_undo_step_on_the_story() {
        with_studio(|paint, cx| {
            picture(paint, cx);
            assert!(!enabled(paint, cx, "story.webtoon.right_to_left"));
            assert!(!enabled(paint, cx, "story.webtoon.left_to_right"));
            paint.run("story.add", cx);
            let story = paint.story_here(&cx.world.arena).expect("a story").0;
            // Bound on the left, it reads left to right until told.
            assert_eq!(webtoon_checks(paint, cx), [Some(false), Some(true)]);
            let depth = cx.history.depth();
            paint.run("story.webtoon.left_to_right", cx);
            assert_eq!(cx.history.depth(), depth, "already so: no step");
            paint.run("story.webtoon.right_to_left", cx);
            assert_eq!(cx.history.depth(), depth + 1);
            assert_eq!(webtoon_checks(paint, cx), [Some(true), Some(false)]);
            let reading = |cx: &AppletContext<'_>| {
                cx.world
                    .arena
                    .get_by_id(story)
                    .ok()
                    .and_then(|entity| entity.property(super::WEBTOON_READING))
                    .and_then(sue_core::Value::as_str)
                    .map(str::to_owned)
            };
            assert_eq!(reading(cx).as_deref(), Some("right"));
            assert!(cx.history.undo(cx.world).expect("undo"));
            assert_eq!(reading(cx), None, "undo takes the property away");
            assert_eq!(webtoon_checks(paint, cx), [Some(false), Some(true)]);
            assert!(cx.history.redo(cx.world).expect("redo"));
            assert_eq!(reading(cx).as_deref(), Some("right"));
            // Set, it no longer follows the binding.
            assert!(!story::bound_right(&cx.world.arena, story));
            paint.run("story.webtoon.left_to_right", cx);
            assert_eq!(reading(cx).as_deref(), Some("left"));
            assert_eq!(webtoon_checks(paint, cx), [Some(false), Some(true)]);
        });
    }

    #[test]
    fn webtoon_reading_follows_the_binding_until_set() {
        with_studio(|paint, cx| {
            picture(paint, cx);
            paint.run("story.add", cx);
            let story = paint.story_here(&cx.world.arena).expect("a story").0;
            assert!(SuePaint::apply(
                cx,
                doc::set(story, story::BINDING, "right")
            ));
            assert!(super::webtoon_right_to_left(&cx.world.arena, story));
            assert_eq!(webtoon_checks(paint, cx), [Some(true), Some(false)]);
        });
    }
}
