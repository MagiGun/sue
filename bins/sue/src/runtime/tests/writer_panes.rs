//! Sue Writer's side and middle panes driven through real frames: the
//! binder, the outliner, the corkboard, the inspector and the comments
//! (P4-11B). Clicks find their place by the text the last frame drew.

use super::*;
use sue_model::manuscript::{self, role};

/// Where the binder ends and the middle of the page begins.
const BINDER: f32 = 235.0;

/// Sue Writer at 1440 by 900 with a new novel open, a frame at a time.
struct Panes {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
    texts: Vec<(String, egui::Rect)>,
    /// The novel.
    novel: sue_core::EntityId,
}

fn writer() -> AppletId {
    AppletId::new(sue_writer::ID).unwrap()
}

fn button(at: Pos2, pointer: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: pointer,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

impl Panes {
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        let now = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        runtime.request(
            ShellRequest::Create {
                applet: writer(),
                key: "novel".to_owned(),
            },
            &mut NoTextures,
            now,
        );
        let novel = runtime
            .engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| manuscript::is_manuscript(entity))
            .map(|(_, entity)| entity.id())
            .expect("a novel was made");
        let mut panes = Self {
            runtime,
            ctx,
            now,
            root,
            texts: Vec::new(),
            novel,
        };
        // Until the Hub has gone and Sue Writer has come in whole.
        for _ in 0..400 {
            panes.step(Vec::new());
            if panes.rect_of("Welcome to Sue").is_none() && !panes.moving() {
                break;
            }
        }
        panes.settle(4);
        panes
    }

    /// Whether the shell still animates.
    fn moving(&self) -> bool {
        matches!(self.runtime.pacing(self.now), FramePacing::Animate { .. })
    }

    fn arena(&self) -> &sue_core::Arena {
        &self.runtime.engine.world.arena
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        self.texts = text_rects(&output);
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: writer(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.settle(6);
    }

    /// Where the last frame drew `text`, the lowest first.
    fn rect_of(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
    }

    /// Where `text` is drawn in the column of the screen between `left`
    /// and `right`.
    fn rect_in(&self, text: &str, left: f32, right: f32) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, rect)| found == text && rect.left() >= left && rect.right() <= right)
            .map(|(_, rect)| *rect)
            .next()
    }

    fn shown(&self) -> Vec<&String> {
        self.texts.iter().map(|(text, _)| text).collect()
    }

    fn at(&self, text: &str) -> Pos2 {
        self.rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.shown()))
            .center()
    }

    /// The middle of the binder row that says `text`: the binder is the
    /// left column of the page.
    fn binder_row(&self, text: &str) -> Pos2 {
        self.rect_in(text, 0.0, BINDER)
            .unwrap_or_else(|| panic!("{text:?} is in the binder: {:?}", self.shown()))
            .center()
    }

    fn click(&mut self, at: Pos2) {
        use egui::PointerButton::Primary;
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![button(at, Primary, true)]);
        self.step(vec![button(at, Primary, false)]);
        self.settle(6);
    }

    fn double_click(&mut self, at: Pos2) {
        use egui::PointerButton::Primary;
        // Long enough after any click before, so that this one is not
        // taken for the end of a triple click.
        self.settle(25);
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![button(at, Primary, true)]);
        self.step(vec![button(at, Primary, false)]);
        self.step(vec![button(at, Primary, true)]);
        self.step(vec![button(at, Primary, false)]);
        self.settle(6);
    }

    fn right_click(&mut self, at: Pos2) {
        use egui::PointerButton::Secondary;
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![button(at, Secondary, true)]);
        self.step(vec![button(at, Secondary, false)]);
        self.settle(10);
    }

    /// Presses at `from`, moves to `to` in steps and lets go there.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        use egui::PointerButton::Primary;
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![button(from, Primary, true)]);
        for step in 1..=12 {
            let t = step as f32 / 12.0;
            self.step(vec![egui::Event::PointerMoved(from + (to - from) * t)]);
        }
        self.step(vec![button(to, Primary, false)]);
        self.settle(6);
    }

    fn hover(&mut self, at: Pos2) {
        self.step(vec![egui::Event::PointerMoved(at)]);
        // A tooltip comes after the pointer has rested a moment.
        for _ in 0..60 {
            self.step(Vec::new());
        }
    }

    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.step(vec![super::key(key, modifiers)]);
        self.settle(4);
    }

    fn typed(&mut self, text: &str) {
        self.step(vec![egui::Event::Text(text.to_owned())]);
        self.settle(2);
    }

    fn undo(&mut self) {
        self.runtime.typing.flush(&mut self.runtime.engine.history);
        self.key(egui::Key::Z, egui::Modifiers::COMMAND);
    }

    fn name(&self, id: sue_core::EntityId) -> String {
        self.arena().get_by_id(id).unwrap().name().to_owned()
    }

    fn text(&self, id: sue_core::EntityId, key: &str) -> Option<String> {
        self.arena()
            .get_by_id(id)
            .ok()?
            .property(key)
            .and_then(sue_core::Value::as_str)
            .map(str::to_owned)
    }

    fn draft(&self) -> sue_core::EntityId {
        manuscript::folder(self.arena(), self.novel, role::DRAFT).unwrap()
    }

    fn trash(&self) -> sue_core::EntityId {
        manuscript::folder(self.arena(), self.novel, role::TRASH).unwrap()
    }

    /// The first chapter and its first scene.
    fn chapter_and_scene(&self) -> (sue_core::EntityId, sue_core::EntityId) {
        let chapter = manuscript::children(self.arena(), self.draft())[0];
        let scene = manuscript::children(self.arena(), chapter)[0];
        (chapter, scene)
    }

    fn finish(self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn sue_writers_binder_renames_in_place_by_double_click_and_f2_with_undo() {
    let mut panes = Panes::new("writer-binder-rename");
    let (chapter, scene) = panes.chapter_and_scene();
    let old = panes.name(scene);
    // A double click on the scene's row puts its name in a field there.
    let row = panes.binder_row(&old);
    panes.double_click(row);
    // The whole name is chosen: what is typed replaces it.
    panes.typed("The Storm");
    panes.key(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(panes.name(scene), "The Storm");
    assert!(panes.rect_in("The Storm", 0.0, BINDER).is_some());
    panes.undo();
    assert_eq!(panes.name(scene), old, "undo gives the old name back");

    // F2 on the chapter's row (chosen by a click) renames it too, and
    // Escape leaves it as it was.
    let chapter_name = panes.name(chapter);
    let row = panes.binder_row(&chapter_name);
    panes.click(row);
    panes.key(egui::Key::F2, egui::Modifiers::NONE);
    panes.key(egui::Key::A, egui::Modifiers::COMMAND);
    panes.typed("Never");
    panes.key(egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(panes.name(chapter), chapter_name, "Escape keeps the name");
    panes.finish();
}

#[test]
fn sue_writers_binder_menu_duplicates_and_delete_trashes_with_undo() {
    let mut panes = Panes::new("writer-binder-menu");
    let (chapter, scene) = panes.chapter_and_scene();
    let name = panes.name(scene);
    let before = manuscript::children(panes.arena(), chapter).len();
    // A right click opens the shared context menu, with the registry's
    // labels.
    let row = panes.binder_row(&name);
    panes.right_click(row);
    let duplicate = panes.at("Duplicate");
    assert!(
        panes.rect_of("Move to Trash").is_some() && panes.rect_of("Rename").is_some(),
        "{:?}",
        panes.shown()
    );
    panes.click(duplicate);
    assert_eq!(
        manuscript::children(panes.arena(), chapter).len(),
        before + 1
    );
    panes.undo();
    assert_eq!(manuscript::children(panes.arena(), chapter).len(), before);

    // Delete on a folder's row, chosen by a click (which leaves the keys
    // with the binder), moves it to the Trash; undo brings it back.
    let second = manuscript::children(panes.arena(), panes.draft())[1];
    let row = panes.binder_row(&panes.name(second));
    panes.click(row);
    panes.key(egui::Key::Delete, egui::Modifiers::NONE);
    let trashed = manuscript::children(panes.arena(), panes.trash());
    assert_eq!(trashed, vec![second], "the chapter went to the Trash");
    panes.undo();
    assert_eq!(
        manuscript::parent(panes.arena(), second),
        Some(panes.draft())
    );
    panes.finish();
}

#[test]
fn sue_writers_binder_moves_an_item_into_a_folder_by_dragging_with_undo() {
    let mut panes = Panes::new("writer-binder-drag");
    let (chapter, scene) = panes.chapter_and_scene();
    let chapters = manuscript::children(panes.arena(), panes.draft());
    assert!(chapters.len() > 1, "the novel has two chapters");
    let other = chapters[1];
    let from = panes.binder_row(&panes.name(scene));
    let to = panes.binder_row(&panes.name(other));
    panes.drag(from, to);
    assert_eq!(manuscript::parent(panes.arena(), scene), Some(other));
    panes.undo();
    assert_eq!(manuscript::parent(panes.arena(), scene), Some(chapter));
    panes.finish();
}

#[test]
fn sue_writers_binder_buttons_say_their_name_and_keys() {
    let mut panes = Panes::new("writer-binder-tips");
    let heading = panes
        .rect_in("Binder", 0.0, BINDER)
        .expect("the binder's heading");
    // Hover along the heading's line from the right until the tip of
    // New Document shows (New Folder's comes first).
    let mut said = false;
    for x in (100..235).rev().step_by(4) {
        panes.hover(egui::pos2(x as f32, heading.center().y));
        if panes.rect_of("New Document").is_some() {
            said = panes.rect_of("Ctrl+N").is_some();
            break;
        }
    }
    assert!(
        said,
        "a tip names New Document and Ctrl+N: {:?}",
        panes.shown()
    );
    panes.finish();
}

#[test]
fn sue_writers_outliner_lists_the_folder_and_opens_a_row_on_double_click() {
    let mut panes = Panes::new("writer-outliner");
    let (chapter, scene) = panes.chapter_and_scene();
    let chapter_name = panes.name(chapter);
    let row = panes.binder_row(&chapter_name);
    panes.click(row);
    panes.command("view.outliner");
    // The columns' titles and the chapter's scene as a row.
    for title in ["Title", "Synopsis", "Label", "Status", "Words", "Export"] {
        assert!(
            panes.rect_of(title).is_some(),
            "{title}: {:?}",
            panes.shown()
        );
    }
    let scene_name = panes.name(scene);
    let at = panes
        .rect_in(&scene_name, BINDER, 1440.0)
        .unwrap_or_else(|| panic!("the scene is a row: {:?}", panes.shown()))
        .center();
    panes.double_click(at);
    // Opened: the outliner gave way to the text.
    assert!(panes.rect_of("Synopsis").is_none(), "{:?}", panes.shown());
    panes.finish();
}

#[test]
fn sue_writers_corkboard_moves_a_card_and_shows_an_empty_folder_with_a_way_out() {
    let mut panes = Panes::new("writer-corkboard");
    let draft = panes.draft();
    let draft_name = panes.name(draft);
    let row = panes.binder_row(&draft_name);
    panes.click(row);
    panes.command("view.corkboard");
    let chapters = manuscript::children(panes.arena(), draft);
    let (first, second) = (chapters[0], chapters[1]);
    let a = panes
        .rect_in(&panes.name(first), BINDER, 1440.0)
        .expect("the first card");
    let b = panes
        .rect_in(&panes.name(second), BINDER, 1440.0)
        .expect("the second card");
    // The second card dropped on the first goes before it.
    panes.drag(b.center(), a.center());
    assert_eq!(manuscript::children(panes.arena(), draft)[0], second);
    panes.undo();
    assert_eq!(manuscript::children(panes.arena(), draft)[0], first);

    // A folder with nothing in it says so, and its button makes the
    // first document there.
    // A new folder starts with its name chosen to be typed over; Enter
    // keeps it.
    panes.command("binder.new_folder");
    panes.key(egui::Key::Enter, egui::Modifiers::NONE);
    let row = panes.binder_row("New Folder");
    panes.click(row);
    panes.command("view.corkboard");
    assert!(
        panes.rect_of("No cards here yet").is_some(),
        "{:?}",
        panes.shown()
    );
    let made = |panes: &Panes| {
        panes
            .arena()
            .iter()
            .filter(|(_, entity)| manuscript::is_document(entity))
            .count()
    };
    let before = made(&panes);
    let button = panes.at("New Document");
    panes.click(button);
    assert_eq!(made(&panes), before + 1, "a document was made");
    panes.finish();
}

#[test]
fn sue_writers_inspector_edits_synopsis_label_and_export_with_undo() {
    let mut panes = Panes::new("writer-inspector");
    let (_, scene) = panes.chapter_and_scene();
    let name = panes.name(scene);
    let row = panes.binder_row(&name);
    panes.click(row);
    panes.command("view.inspector");
    // The synopsis is a field of several lines: Enter starts a line.
    let old = panes.text(scene, manuscript::SYNOPSIS);
    let field = panes.at(old
        .as_deref()
        .unwrap_or("What happens here, in a line or two"));
    panes.click(field);
    panes.key(egui::Key::A, egui::Modifiers::COMMAND);
    panes.typed("A storm comes.");
    panes.key(egui::Key::Enter, egui::Modifiers::NONE);
    panes.typed("The lamp fails.");
    // Clicking elsewhere ends the typing and keeps it.
    let heading = panes.at("Keywords");
    panes.click(heading);
    assert_eq!(
        panes.text(scene, manuscript::SYNOPSIS).as_deref(),
        Some(
            "A storm comes.
The lamp fails."
        )
    );
    panes.undo();
    assert_eq!(panes.text(scene, manuscript::SYNOPSIS), old);

    // The label list.
    let label = panes.at("No Label");
    panes.click(label);
    let idea = panes.at("Idea");
    panes.click(idea);
    assert_eq!(
        panes.text(scene, manuscript::LABEL).as_deref(),
        Some("Idea")
    );
    panes.undo();
    assert_eq!(panes.text(scene, manuscript::LABEL), None);

    // Include in export.
    let included = |panes: &Panes| manuscript::included(panes.arena().get_by_id(scene).unwrap());
    assert!(included(&panes));
    let export = panes.at("Include in export");
    panes.click(export);
    assert!(!included(&panes), "left out of the export");
    // The binder says so on the row.
    assert!(
        panes.rect_in("Left out", 0.0, BINDER).is_some(),
        "{:?}",
        panes.shown()
    );
    panes.undo();
    assert!(included(&panes));
    panes.finish();
}

#[test]
fn sue_writers_comments_pane_replies_resolves_and_deletes_with_undo() {
    let mut panes = Panes::new("writer-comments");
    // Words at the caret of the scene the novel opened on, a comment on
    // them.
    panes.typed("A dark and stormy night");
    panes
        .runtime
        .typing
        .flush(&mut panes.runtime.engine.history);
    panes.command("review.comment");
    panes.typed("Too much?");
    panes.key(egui::Key::Enter, egui::Modifiers::COMMAND);
    panes.command("view.inspector");
    panes.command("review.comments");
    let comments = |panes: &Panes| {
        panes
            .arena()
            .iter()
            .filter(|(_, entity)| entity.class() == sue_model::comments::COMMENT)
            .map(|(_, entity)| entity.id())
            .collect::<Vec<_>>()
    };
    let found = comments(&panes);
    assert_eq!(found.len(), 1, "a comment was made");
    assert!(panes.rect_of("Too much?").is_some(), "{:?}", panes.shown());
    // A reply, sent with Enter.
    let reply = panes
        .texts
        .iter()
        .filter(|(text, _)| text == "Reply")
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the reply field's hint");
    panes.click(reply.center());
    panes.typed("Yes, a little.");
    panes.key(egui::Key::Enter, egui::Modifiers::NONE);
    let thread = sue_model::comments::thread(panes.arena(), found[0]).unwrap();
    assert_eq!(thread.replies.len(), 1, "the reply was kept");
    assert_eq!(thread.replies[0].text, "Yes, a little.");
    // Resolve.
    let resolve = panes.at("Resolve");
    panes.click(resolve);
    assert!(
        sue_model::comments::thread(panes.arena(), found[0])
            .unwrap()
            .resolved
    );
    panes.undo();
    assert!(
        !sue_model::comments::thread(panes.arena(), found[0])
            .unwrap()
            .resolved
    );
    panes.finish();
}
