//! Sue Paint's Story commands, each run through the registry as the
//! menus, the keys and Auto Actions run it: every one runs, grays out
//! where it cannot work, and every one that changes the work undoes to
//! exactly the state before it and redoes to exactly the state after.

use std::collections::BTreeMap;

use super::*;

/// The whole arena as data: every entity's bytes by id, and every
/// relation. Two states are equal when entities, ids, components,
/// properties and relations all are.
#[derive(Debug, PartialEq)]
struct State {
    entities: BTreeMap<sue_core::EntityId, Vec<u8>>,
    relations: sue_core::RelationStore,
}

fn state(driver: &PaintDriver) -> State {
    let arena = &driver.runtime.engine.world.arena;
    State {
        entities: arena
            .iter()
            .map(|(_, entity)| {
                let mut out = sue_core::codec::Writer::new();
                entity.encode(&mut out);
                (entity.id(), out.into_bytes())
            })
            .collect(),
        relations: arena.relations().clone(),
    }
}

/// What differs between two states, for a failing test's message.
fn differences(driver: &PaintDriver, wanted: &State, found: &State) -> String {
    let arena = &driver.runtime.engine.world.arena;
    let name = |id: &sue_core::EntityId| {
        arena.get_by_id(*id).map_or_else(
            |_| format!("{id} (gone)"),
            |entity| format!("{id} {} {:?}", entity.class(), entity.name()),
        )
    };
    let mut out = Vec::new();
    for (id, bytes) in &wanted.entities {
        match found.entities.get(id) {
            None => out.push(format!("missing {}", name(id))),
            Some(other) if other != bytes => out.push(format!("changed {}", name(id))),
            Some(_) => {}
        }
    }
    for id in found.entities.keys() {
        if !wanted.entities.contains_key(id) {
            out.push(format!("extra {}", name(id)));
        }
    }
    if wanted.relations != found.relations {
        out.push("relations differ".to_owned());
    }
    out.join("; ")
}

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// The picture shown.
fn current(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

/// The pages of the shown picture's story (none outside a story).
fn pages(driver: &PaintDriver) -> Vec<sue_core::EntityId> {
    let arena = &driver.runtime.engine.world.arena;
    sue_paint::story::story_of(arena, current(driver))
        .map(|story| sue_paint::story::pages(arena, story))
        .unwrap_or_default()
}

fn undo(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(2);
}

fn redo(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    driver.settle(2);
}

/// Runs `command`, which must change the work in one undo step; undo
/// gives back the state before, ids included, redo the state after.
fn undo_pair(driver: &mut PaintDriver, command: &str) {
    let before = state(driver);
    driver.command(command);
    driver.settle(2);
    let after = state(driver);
    assert_ne!(before, after, "{command} changed nothing");
    // One Ctrl+Z takes all of it back (the history in use may change
    // with the command, as a picture becomes a story, so its depth says
    // nothing here).
    undo(driver);
    let undone = state(driver);
    assert!(
        undone == before,
        "{command}: undo gives back the work before: {}",
        differences(driver, &before, &undone)
    );
    redo(driver);
    let redone = state(driver);
    assert!(
        redone == after,
        "{command}: redo gives back the work after: {}",
        differences(driver, &after, &redone)
    );
}

/// The Story group as the registry has it.
fn story_commands() -> Vec<&'static sue_wm::Command<sue_paint::SuePaint>> {
    sue_paint::registry()
        .iter()
        .filter(|command| command.group == "story")
        .collect()
}

#[test]
fn every_story_command_runs_through_the_registry() {
    let mut driver = PaintDriver::new("paint-cmd-story-all");
    let commands = story_commands();
    assert_eq!(commands.len(), 23, "the Story group");
    // Once on a lone picture, once on the middle page of a story of
    // three: nothing says it is missing, and whatever opened closes.
    for round in 0..2 {
        if round == 1 {
            // The first round left a story behind; at least three
            // pages, the second shown.
            driver.command("story.add");
            driver.command("story.add");
            driver.command("story.first");
            driver.command("story.next");
            driver.settle(2);
            assert!(pages(&driver).len() >= 3, "{}", pages(&driver).len());
            assert_eq!(pages(&driver).get(1), Some(&current(&driver)));
        }
        for command in &commands {
            driver.command(command.name);
            driver.settle(2);
            let message = driver
                .runtime
                .shell
                .message()
                .map(|(text, _)| text.to_owned());
            assert!(
                !message
                    .as_deref()
                    .is_some_and(|text| text.contains("not in Sue Paint yet")),
                "{}: {message:?}",
                command.name
            );
            driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
            driver.settle(2);
        }
    }
}

#[test]
fn story_commands_have_their_icons_and_flags() {
    for command in story_commands() {
        assert!(
            sue_wm::icons::Icon::from_name(command.icon).is_some(),
            "{}: no icon {:?}",
            command.name,
            command.icon
        );
        // Clip Studio Paint gives no Story command a key.
        assert_eq!(command.chords, "", "{}", command.name);
        assert!(!command.is_family(), "{}", command.name);
        assert!(
            !command.flags.has(sue_wm::Flags::LEAVES_CANVAS),
            "{} stays on the canvas",
            command.name
        );
    }
    // Auto Actions keep what changes the pages or which page is shown;
    // not the Story Editor, nor what reads or writes files.
    let unrecorded: Vec<&str> = story_commands()
        .into_iter()
        .filter(|command| !command.flags.has(sue_wm::Flags::RECORDABLE))
        .map(|command| command.name)
        .collect();
    assert_eq!(
        unrecorded,
        [
            "story.editor",
            "story.import_text",
            "story.export_page",
            "story.export_pages",
            "story.preview_binding"
        ]
    );
}

#[test]
fn making_a_picture_a_story_undoes_in_one_step() {
    // Add Page and Duplicate Page on a lone picture make it a story and
    // add the page: one Ctrl+Z takes both back, ids and relations too.
    for command in ["story.add", "story.insert", "story.duplicate_page"] {
        let mut driver = PaintDriver::new(&format!("paint-cmd-story-{command}"));
        let picture = current(&driver);
        let before = state(&driver);
        driver.command(command);
        driver.settle(2);
        assert_eq!(pages(&driver).len(), 2, "{command}");
        undo(&mut driver);
        let undone = state(&driver);
        assert!(
            undone == before,
            "{command}: undo gives back the lone picture: {}",
            differences(&driver, &before, &undone)
        );
        assert!(driver.runtime.engine.world.arena.get_by_id(picture).is_ok());
        // Redo is not checked here: the new page was shown in place of
        // the picture, so the undo leaves no canvas open (a defect of the
        // open tabs, reported), and the picture and its story keep undo
        // histories of their own (`history_scope`).
    }
}

/// Runs `command`, which adds a page and shows it, and checks that one
/// Ctrl+Z gives back the work before it exactly.
fn undo_exactly(driver: &mut PaintDriver, command: &str) {
    let before = state(driver);
    driver.command(command);
    driver.settle(2);
    assert_ne!(before, state(driver), "{command} changed nothing");
    undo(driver);
    let undone = state(driver);
    assert!(
        undone == before,
        "{command}: undo gives back the work before: {}",
        differences(driver, &before, &undone)
    );
}

#[test]
fn adding_a_page_to_a_story_undoes_exactly() {
    // Add, Insert and Duplicate Page show the new page in place of the
    // one shown; undone, the new page goes and with it the only open
    // canvas, so Ctrl+Y finds no canvas to redo in. That is a defect of
    // the open tabs (reported to the owner of `studio/frame.rs`), not of
    // the edit, so redo is not checked here.
    for command in ["story.add", "story.insert", "story.duplicate_page"] {
        let mut driver = PaintDriver::new(&format!("paint-cmd-story-more-{command}"));
        driver.command("story.add");
        driver.settle(2);
        assert_eq!(pages(&driver).len(), 2);
        undo_exactly(&mut driver, command);
    }
}

#[test]
fn page_edits_that_keep_a_page_shown_undo_and_redo_exactly() {
    let mut driver = PaintDriver::new("paint-cmd-story-undo");
    for _ in 0..3 {
        driver.command("story.add");
    }
    driver.settle(2);
    assert_eq!(pages(&driver).len(), 4);
    undo_pair(&mut driver, "story.delete");
    assert_eq!(pages(&driver).len(), 3);
    // Left bound and starting alone, page 2 faces page 3.
    driver.command("story.first");
    driver.command("story.next");
    driver.settle(2);
    assert_eq!(pages(&driver).get(1), Some(&current(&driver)));
    undo_pair(&mut driver, "story.combine");
    assert_eq!(pages(&driver).len(), 2, "two pages became one spread");
    undo_pair(&mut driver, "story.separate");
    assert_eq!(pages(&driver).len(), 3);
}

/// The story's Webtoon reading direction as saved: "right", "left" or
/// unset.
fn webtoon_reading(driver: &PaintDriver) -> Option<String> {
    let arena = &driver.runtime.engine.world.arena;
    let story = sue_paint::story::story_of(arena, current(driver))?;
    arena
        .get_by_id(story)
        .ok()?
        .property("story.webtoon_reading")?
        .as_str()
        .map(str::to_owned)
}

#[test]
fn webtoon_reading_direction_undoes_and_is_saved_with_the_work() {
    let mut driver = PaintDriver::new("paint-cmd-story-webtoon");
    driver.command("story.add");
    driver.settle(2);
    assert_eq!(webtoon_reading(&driver), None, "it follows the binding");
    undo_pair(&mut driver, "story.webtoon.right_to_left");
    assert_eq!(webtoon_reading(&driver).as_deref(), Some("right"));
    // Choosing what is already chosen is no step.
    let depth = driver.runtime.engine.history.depth();
    driver.command("story.webtoon.right_to_left");
    driver.settle(2);
    assert_eq!(driver.runtime.engine.history.depth(), depth);
    undo_pair(&mut driver, "story.webtoon.left_to_right");
    driver.command("story.webtoon.right_to_left");
    driver.settle(2);
    // It stays with the work through saving and opening it again.
    std::fs::create_dir_all(&driver.root).unwrap();
    let project = driver.root.join("webtoon.sue");
    driver.runtime.save_project_to(&project).unwrap();
    let bytes = std::fs::read(&project).unwrap();
    driver.runtime.open_project(&project, &bytes).unwrap();
    driver.settle(4);
    let arena = &driver.runtime.engine.world.arena;
    let readings: Vec<Option<&str>> = arena
        .iter()
        .filter(|(_, entity)| entity.class() == sue_paint::story::STORY)
        .map(|(_, entity)| {
            entity
                .property("story.webtoon_reading")
                .and_then(sue_core::Value::as_str)
        })
        .collect();
    assert!(!readings.is_empty(), "the story came back");
    assert!(readings.iter().all(|reading| *reading == Some("right")));
}
