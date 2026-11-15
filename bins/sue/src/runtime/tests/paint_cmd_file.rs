//! Sue Paint's File commands, run one by one through the registry on a
//! real canvas: every command of the group runs, the ones that change
//! the document undo to the arena they started from (ids included) and
//! redo to the arena they made, and the commands added for the tutorials
//! (Manage Fonts, File, Import, Pose Scanner) do what they say.

use super::*;

/// Every entity of the project, in id order, to compare whole arenas.
fn arena(paint: &PaintDriver) -> Vec<sue_core::Entity> {
    let mut entities: Vec<sue_core::Entity> = paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.clone())
        .collect();
    entities.sort_by_key(|entity| format!("{:?}", entity.id()));
    entities
}

/// Runs `command` and lets the frames after it settle.
fn run(paint: &mut PaintDriver, command: &str) {
    paint.command(command);
    paint.settle(4);
}

/// The canvas Sue Paint has in front.
fn canvas(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

fn layers(paint: &PaintDriver) -> usize {
    sue_paint::doc::layers(&paint.runtime.engine.world.arena, canvas(paint)).len()
}

/// `command` changes the document; undo gives back the arena before it,
/// redo the arena after it.
fn undo_pair(paint: &mut PaintDriver, command: &str) {
    let before = arena(paint);
    run(paint, command);
    let after = arena(paint);
    assert_ne!(before, after, "{command} changed nothing");
    run(paint, "edit.undo");
    assert!(
        arena(paint) == before,
        "{command}: undo is not the arena before"
    );
    run(paint, "edit.redo");
    assert!(
        arena(paint) == after,
        "{command}: redo is not the arena after"
    );
}

#[test]
fn every_file_command_runs_on_a_canvas() {
    let mut paint = PaintDriver::new("paint-cmd-file");
    let registry = sue_paint::registry();
    let commands: Vec<String> = registry
        .iter()
        .filter(|command| command.group == "file")
        .filter(|command| !command.flags.has(sue_wm::Flags::LEAVES_CANVAS))
        .map(|command| {
            if command.is_family() {
                format!("{}0", command.name)
            } else {
                command.name.to_owned()
            }
        })
        .collect();
    assert!(commands.len() >= 50, "{}", commands.len());
    // Open Recent and Clear Recent Files have no file to work on in a
    // fresh test home; every other command must show that it ran.
    let quiet_here = ["file.recent.clear", "file.recent.open.0"];
    let mut silent = Vec::new();
    for command in &commands {
        // Something copied, so New from Clipboard works.
        if command == "file.new_clipboard" {
            run(&mut paint, "select.all");
            run(&mut paint, "edit.copy");
        }
        // A mark in the status line: a command that says something
        // replaces it.
        paint.runtime.shell.notify(SILENT, false);
        let drawn_before = drawn(&paint.settle(2));
        let arena_before = arena(&paint);
        run(&mut paint, command);
        let said = paint
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_string());
        assert!(
            !said
                .as_deref()
                .is_some_and(|text| text.contains("not in Sue Paint yet")),
            "{command}: {said:?}"
        );
        // It ran: it opened something (a dialog, a page, a file
        // dialog's notice), said something, or changed the document.
        let ran = said.as_deref() != Some(SILENT)
            || drawn(&paint.settle(2)) != drawn_before
            || arena(&paint) != arena_before;
        if !ran && !quiet_here.contains(&command.as_str()) {
            silent.push(command.clone());
        }
        // Close whatever opened, and keep a canvas in front for the next.
        paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        paint.settle(2);
        if paint
            .runtime
            .applets
            .get(&AppletId::new(sue_paint::ID).unwrap())
            .and_then(|applet| applet.focus())
            .is_none()
        {
            run(&mut paint, "file.new");
            paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
            paint.settle(10);
        }
    }
    assert!(silent.is_empty(), "these did nothing: {silent:?}");
}

/// The status line's text while no command has said anything.
const SILENT: &str = "(test: nothing said yet)";

/// The texts a frame drew, without where: what a command that opens
/// something adds to the screen.
fn drawn(texts: &[(String, egui::Rect)]) -> std::collections::BTreeSet<String> {
    texts
        .iter()
        .map(|(text, _)| text.clone())
        .filter(|text| text != SILENT)
        .collect()
}

#[test]
fn record_timelapse_undoes_and_redoes() {
    let mut paint = PaintDriver::new("paint-cmd-file-timelapse");
    undo_pair(&mut paint, "file.timelapse.record");
    // Turned off again, it is a step of its own too.
    undo_pair(&mut paint, "file.timelapse.record");
}

#[test]
fn pose_scanner_under_import_makes_a_figure_when_none_is_picked() {
    let mut paint = PaintDriver::new("paint-cmd-file-pose");
    let before = layers(&paint);
    let figures = |paint: &PaintDriver| {
        paint
            .runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| entity.name() == "Drawing figure")
            .count()
    };
    assert_eq!(figures(&paint), 0);
    undo_pair(&mut paint, "file.import.pose_scanner");
    assert_eq!(layers(&paint), before + 1, "a 3D layer with the figure");
    assert!(figures(&paint) >= 1, "the new layer shows a drawing figure");
    // Made afresh, the new figure is the layer picked; run again, it
    // asks for the photo and makes no other figure.
    run(&mut paint, "edit.undo");
    run(&mut paint, "file.import.pose_scanner");
    assert_eq!(layers(&paint), before + 1);
    let made = arena(&paint);
    run(&mut paint, "file.import.pose_scanner");
    assert!(
        arena(&paint) == made,
        "a picked figure is posed, not added to"
    );
}

#[test]
fn manage_fonts_opens_the_shared_font_list_and_closes() {
    let mut paint = PaintDriver::new("paint-cmd-file-fonts");
    let before = arena(&paint);
    run(&mut paint, "file.fonts");
    let texts = paint.settle(4);
    let shown =
        |texts: &[(String, egui::Rect)], text: &str| texts.iter().any(|(found, _)| found == text);
    assert!(shown(&texts, "Manage Fonts"), "the title: {texts:?}");
    // The bundled families are always in the list, each in its own font.
    assert!(shown(&texts, "Hack"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.contains("font famil") || text.starts_with("Finding the fonts")),
        "how many families: {texts:?}"
    );
    // It opens on the family Sue writes in, its sample line shown.
    let sample = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .any(|(text, _)| text.starts_with("The quick brown fox"))
    };
    let times = |texts: &[(String, egui::Rect)], text: &str| {
        texts.iter().filter(|(found, _)| found == text).count()
    };
    assert!(sample(&texts), "{texts:?}");
    assert_eq!(times(&texts, "Hack"), 1, "Hack only in the list");
    // Picking another names it over the sample too.
    paint.click_text("Hack");
    let texts = paint.settle(4);
    assert!(sample(&texts), "{texts:?}");
    assert_eq!(times(&texts, "Hack"), 2, "Hack in the list and picked");
    // Close closes it, and nothing in the document changed.
    paint.click_text("Close");
    let texts = paint.settle(4);
    assert!(!shown(&texts, "Manage Fonts"), "{texts:?}");
    assert!(arena(&paint) == before);
}
