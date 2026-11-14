//! Sue Paint's Edit commands, run one by one on a canvas through the
//! registry as the menus run them: each works, and each that changes the
//! picture undoes to the very arena it found and redoes to the one it
//! made, entity ids included (rule 7).

use super::*;

/// The data as bytes: every entity (all but its revision, which undo
/// moves on) in the order of their ids, then every relation.
fn arena_bytes(arena: &sue_core::Arena) -> Vec<u8> {
    let mut entities: Vec<([u8; 16], Vec<u8>)> = arena
        .iter()
        .map(|(_, entity)| {
            let mut out = sue_core::codec::Writer::new();
            entity.encode(&mut out);
            (entity.id().to_bytes(), out.into_bytes())
        })
        .collect();
    entities.sort_by(|a, b| a.0.cmp(&b.0));
    let mut bytes: Vec<u8> = entities.into_iter().flat_map(|(_, bytes)| bytes).collect();
    let mut out = sue_core::codec::Writer::new();
    arena.relations().encode(&mut out);
    bytes.extend(out.into_bytes());
    bytes
}

fn bytes_now(driver: &PaintDriver) -> Vec<u8> {
    arena_bytes(&driver.runtime.engine.world.arena)
}

fn depth(driver: &PaintDriver) -> usize {
    driver.runtime.engine.history.depth()
}

/// Runs `command` and lets the frames after it draw.
fn run(driver: &mut PaintDriver, command: &str) {
    driver.command(command);
    driver.settle(3);
}

/// A canvas with a stroke off its middle, so that every alignment and
/// every flip moves it, and the whole canvas selected.
fn canvas_with_a_stroke() -> PaintDriver {
    let mut driver = PaintDriver::new("paint-cmd-edit");
    driver.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    let middle = paint_middle(&driver.runtime);
    driver.drag(
        middle + vec2(20.0, 10.0),
        middle + vec2(160.0, 70.0),
        egui::PointerButton::Primary,
    );
    assert!(depth(&driver) > 0, "the stroke is a step");
    driver
}

/// The commands that change the picture on that canvas, with something
/// copied: each must leave one undo step.
const CHANGES_THE_PICTURE: &[&str] = &[
    "edit.cut",
    "edit.paste",
    "edit.paste_here",
    "edit.clear",
    "edit.fill",
    "edit.recolor",
    "edit.luma_to_alpha",
    "edit.flip_h",
    "edit.flip_v",
    "edit.rotate180",
    "edit.align.left",
    "edit.align.hcenter",
    "edit.align.right",
    "edit.align.top",
    "edit.align.vcenter",
    "edit.align.bottom",
];

#[test]
fn every_edit_command_runs_and_undoes_to_the_arena_it_found() {
    let mut driver = canvas_with_a_stroke();
    let registry = sue_paint::registry();
    let edit: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "edit")
        .map(|command| command.name)
        .collect();
    assert!(edit.len() >= 43, "{edit:?}");
    for name in CHANGES_THE_PICTURE {
        assert!(edit.contains(name), "{name} is an Edit command");
    }

    // Undo and Redo first: the stroke goes and comes back.
    let drawn = bytes_now(&driver);
    run(&mut driver, "edit.undo");
    assert_ne!(bytes_now(&driver), drawn, "undo takes the stroke away");
    run(&mut driver, "edit.redo");
    assert_eq!(bytes_now(&driver), drawn, "redo puts it back exactly");

    // Something to paste.
    run(&mut driver, "select.all");
    run(&mut driver, "edit.copy");
    // The stroke is in the drawing color; the other one makes Convert
    // to Drawing Color and Convert Brightness to Opacity change it.
    run(&mut driver, "color.swap");

    let mut changed = Vec::new();
    for &name in &edit {
        if matches!(name, "edit.undo" | "edit.redo" | "edit.clear_history") {
            continue;
        }
        // Each starts from the same picture with the whole canvas
        // selected; Escape closes what the one before opened.
        driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        driver.settle(2);
        run(&mut driver, "select.all");
        let before = bytes_now(&driver);
        let steps = depth(&driver);
        run(&mut driver, name);
        let message = driver
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned());
        assert!(
            !message
                .as_deref()
                .is_some_and(|text| text.contains("not in Sue Paint")),
            "{name}: {message:?}"
        );
        // A transform or a warp waits for the pen; Escape leaves it as
        // it was.
        driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        driver.settle(3);
        let after = bytes_now(&driver);
        if after == before {
            continue;
        }
        changed.push(name);
        assert_eq!(depth(&driver), steps + 1, "{name} is one undo step");
        run(&mut driver, "edit.undo");
        assert!(
            bytes_now(&driver) == before,
            "{name}: undo gives back the arena it found"
        );
        run(&mut driver, "edit.redo");
        assert!(
            bytes_now(&driver) == after,
            "{name}: redo gives back the arena it made"
        );
        run(&mut driver, "edit.undo");
        assert!(bytes_now(&driver) == before, "{name}: and undo again");
    }
    for name in CHANGES_THE_PICTURE {
        assert!(
            changed.contains(name),
            "{name} changed nothing: {changed:?}"
        );
    }

    // Clear work history: no step is left and the picture stays.
    let before = bytes_now(&driver);
    run(&mut driver, "edit.clear_history");
    assert_eq!(depth(&driver), 0, "the history is empty");
    assert!(bytes_now(&driver) == before, "the picture stays");
}
