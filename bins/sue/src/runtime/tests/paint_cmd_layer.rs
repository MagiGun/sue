//! Sue Paint: every command of the Layer group, run through the registry
//! on a canvas set up so that it has something to do. A command that
//! changes the picture is undone and redone, and the arena is compared
//! with how it was before and after, ids included (rule 7).

use std::collections::BTreeMap;

use super::*;

/// Everything in the arena, comparable: each entity by its id (its
/// revision counter left out) and every relation.
type Content = (BTreeMap<sue_core::EntityId, sue_core::Entity>, Vec<String>);

fn content(arena: &sue_core::Arena) -> Content {
    let entities = arena
        .iter()
        .map(|(_, entity)| (entity.id(), entity.clone()))
        .collect();
    let mut relations: Vec<String> = arena
        .relations()
        .iter()
        .map(|relation| format!("{relation:?}"))
        .collect();
    relations.sort();
    (entities, relations)
}

/// What a canvas needs before a command can do its work. Every case
/// starts on a new canvas whose layer lies under a layer filled with the
/// drawing color, which is current, with all of the canvas selected.
const BASE: &[&str] = &["layer.new.fill", "select.all"];

/// One command, what is run before it on top of [`BASE`], and how many
/// steps it puts on the undo stack (0 for one that opens a dialog, a
/// palette or changes only what is shown).
struct Case {
    command: &'static str,
    before: &'static [&'static str],
    steps: usize,
    /// The key pressed after it: Esc closes what it opened, Enter
    /// confirms a dialog that keeps what the command made.
    then: egui::Key,
}

const fn case(command: &'static str, before: &'static [&'static str], steps: usize) -> Case {
    Case {
        command,
        before,
        steps,
        then: egui::Key::Escape,
    }
}

/// A command whose dialog is confirmed with Enter.
const fn confirmed(command: &'static str, steps: usize) -> Case {
    Case {
        command,
        before: &[],
        steps,
        then: egui::Key::Enter,
    }
}

/// Runs `command` as the menus do and lets the frames after settle; Esc
/// then closes what it opened.
fn run(driver: &mut PaintDriver, command: &str) {
    driver.command(command);
    driver.settle(3);
}

fn depth(driver: &PaintDriver) -> usize {
    driver.runtime.engine.history.depth()
}

fn arena(driver: &PaintDriver) -> Content {
    content(&driver.runtime.engine.world.arena)
}

const CASES: &[Case] = &[
    case("layer.new.raster_top", &[], 1),
    case("layer.new", &[], 1),
    case("layer.new.vector", &[], 1),
    case("layer.new.text", &[], 0),
    case("layer.new.balloon", &[], 1),
    case("layer.new.gradient", &[], 1),
    case("layer.new.fill", &[], 1),
    // A new layer, then the Tone dialog over it, which Esc closes.
    case("layer.new.tone", &[], 1),
    // The paper goes first: a canvas holds one.
    case(
        "layer.new.paper",
        &["layer.select_below", "layer.select_below", "layer.delete"],
        1,
    ),
    case("layer.new.3d", &[], 0),
    case("layer.new.figure", &[], 1),
    case("layer.new.hand", &[], 1),
    case("layer.new.head", &[], 1),
    case("layer.new.frame", &[], 0),
    // A correction layer's settings open over it: Enter keeps the
    // layer (Esc takes it away again, see below).
    confirmed("layer.correction.brightness", 1),
    confirmed("layer.correction.levels", 1),
    confirmed("layer.correction.curve", 1),
    confirmed("layer.correction.hue", 1),
    confirmed("layer.correction.balance", 1),
    case("layer.correction.invert", &[], 1),
    confirmed("layer.correction.posterize", 1),
    confirmed("layer.correction.binarize", 1),
    confirmed("layer.correction.gradient_map", 1),
    case("layer.folder_insert", &[], 1),
    case("layer.new.folder", &[], 1),
    case("layer.ungroup", &["layer.folder_insert"], 1),
    case("layer.from_selection", &[], 1),
    case("layer.duplicate", &[], 1),
    case("layer.delete", &[], 1),
    // The layer under the filled one is empty.
    case("layer.delete_empty", &[], 1),
    case("layer.delete_hidden", &["layer.toggle.visible"], 1),
    case("layer.hide_drafts", &["layer.toggle.draft"], 1),
    case("layer.mask.outside", &[], 1),
    case("layer.mask.inside", &[], 1),
    case("layer.mask.delete", &["layer.mask.outside"], 1),
    case("layer.mask.apply", &["layer.mask.outside"], 1),
    case("layer.mask.toggle", &["layer.mask.outside"], 1),
    case("layer.mask.edit", &["layer.mask.outside"], 0),
    case("layer.ruler.delete", &["layer.ruler.symmetry"], 1),
    case("layer.ruler.show", &[], 0),
    case("layer.ruler.perspective", &[], 0),
    case("layer.ruler.symmetry", &[], 1),
    case("layer.ruler.circle", &[], 1),
    case("layer.toggle.visible", &[], 1),
    case("layer.rename", &[], 0),
    case("layer.toggle.clip", &[], 1),
    case("layer.toggle.reference", &[], 1),
    case("layer.toggle.draft", &[], 1),
    case("layer.toggle.lock", &[], 1),
    case("layer.toggle.lock_alpha", &[], 1),
    case("layer.toggle.layer_color", &[], 1),
    case("layer.properties", &[], 0),
    case("layer.comps", &[], 0),
    case("layer.select_above", &[], 0),
    case("layer.select_below", &[], 0),
    case("layer.select_all", &[], 0),
    case("layer.select_none", &["layer.select_all"], 0),
    // The selection, not the picture, changes.
    case("layer.selection.overlap", &[], 0),
    // The filled layer is on top: the one under it goes up.
    case("layer.order.front", &["layer.select_below"], 1),
    case("layer.order.up", &["layer.select_below"], 1),
    case("layer.order.down", &[], 1),
    case("layer.order.back", &[], 1),
    case("layer.transfer_down", &[], 1),
    case("layer.merge_down", &[], 1),
    // Every other layer picked beside the filled one.
    case("layer.merge_selected", &["layer.select_all"], 1),
    case("layer.merge_visible_here", &[], 1),
    case("layer.merge_visible", &[], 1),
    case("layer.flatten", &[], 1),
    case("layer.rasterize", &["layer.new.vector"], 1),
    case("layer.convert", &[], 0),
    case("layer.lines_tones_vector", &["layer.new.figure"], 1),
    // A new vector layer has no line to update: a notice, no step. The
    // update itself is driven in paint_vector_3d.rs.
    case("layer.update_lines", &["layer.new.vector"], 0),
];

#[test]
fn every_layer_command_runs_and_its_changes_undo_and_redo() {
    // The cases name every command of the group, each once.
    let group: Vec<&str> = sue_paint::registry()
        .iter()
        .filter(|command| command.group == "layer")
        .map(|command| command.name)
        .collect();
    let named: Vec<&str> = CASES.iter().map(|case| case.command).collect();
    let mut sorted_group = group.clone();
    sorted_group.sort_unstable();
    let mut sorted_named = named.clone();
    sorted_named.sort_unstable();
    assert_eq!(sorted_named, sorted_group, "one case per Layer command");
    // Every case runs; what went wrong is listed at the end, all of it.
    let mut wrong = Vec::new();
    for case in CASES {
        if let Err(problem) = check(case) {
            wrong.push(problem);
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Runs one case on a canvas of its own.
fn check(case: &Case) -> Result<(), String> {
    let mut driver = PaintDriver::new("paint-cmd-layer");
    for command in BASE.iter().chain(case.before) {
        run(&mut driver, command);
    }
    let (before, start) = (arena(&driver), depth(&driver));
    run(&mut driver, case.command);
    driver.step(vec![key(case.then, egui::Modifiers::NONE)]);
    driver.settle(3);
    let (after, end) = (arena(&driver), depth(&driver));
    let name = case.command;
    if end.checked_sub(start) != Some(case.steps) {
        return Err(format!(
            "{name}: {} undo steps, not {} ({:?})",
            end as i64 - start as i64,
            case.steps,
            driver.runtime.shell.message()
        ));
    }
    if case.steps == 0 {
        return if before == after {
            Ok(())
        } else {
            Err(format!("{name} changed the picture with no undo step"))
        };
    }
    if before == after {
        return Err(format!("{name} changed nothing"));
    }
    for _ in 0..case.steps {
        run(&mut driver, "edit.undo");
    }
    if arena(&driver) != before {
        return Err(format!("{name}: undo is not exact"));
    }
    for _ in 0..case.steps {
        run(&mut driver, "edit.redo");
    }
    if arena(&driver) != after {
        return Err(format!("{name}: redo is not exact"));
    }
    Ok(())
}

/// Whether the top layer of the canvas in focus is drawn in its layer
/// color.
fn top_in_layer_color(driver: &PaintDriver) -> bool {
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let picture = driver
        .runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .expect("a canvas");
    let arena = &driver.runtime.engine.world.arena;
    let top = sue_paint::doc::layers(arena, picture)
        .last()
        .expect("a layer")
        .id;
    arena
        .get_by_id(top)
        .ok()
        .and_then(|entity| entity.property(sue_paint::effects::COLOR))
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
}

#[test]
fn ctrl_b_switches_the_layer_to_its_layer_color_and_back() {
    // Tutorial step CT-9: Ctrl+B tints a sketch layer blue, a second
    // Ctrl+B takes the tint off, and each is one undo step.
    let mut driver = PaintDriver::new("paint-cmd-layer-color");
    run(&mut driver, "layer.new.fill");
    assert!(!top_in_layer_color(&driver));
    let ctrl_b = || vec![key(egui::Key::B, egui::Modifiers::COMMAND)];
    let start = depth(&driver);
    driver.step(ctrl_b());
    driver.settle(2);
    assert!(top_in_layer_color(&driver), "Ctrl+B turns it on");
    driver.step(ctrl_b());
    driver.settle(2);
    assert!(!top_in_layer_color(&driver), "Ctrl+B again turns it off");
    assert_eq!(depth(&driver), start + 2);
    run(&mut driver, "edit.undo");
    assert!(top_in_layer_color(&driver), "undo brings the tint back");
}

/// The alpha of every pixel of the top layer of the canvas in focus.
fn top_alpha(driver: &PaintDriver) -> Vec<u8> {
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let picture = driver
        .runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .expect("a canvas");
    let arena = &driver.runtime.engine.world.arena;
    let top = sue_paint::doc::layers(arena, picture)
        .last()
        .expect("a layer")
        .id;
    sue_paint::doc::raster(arena, top)
        .and_then(sue_core::RasterTile2D::bytes)
        .expect("pixels")
        .iter()
        .skip(3)
        .step_by(4)
        .copied()
        .collect()
}

#[test]
fn select_overlapping_area_keeps_the_part_the_layer_covers() {
    // Tutorial step FGV-5. A layer drawn only inside a shrunk selection;
    // then all of the canvas is selected, and Select Overlapping Area
    // must leave the selection where the layer is drawn and nowhere
    // else. Convert to Selection Layer shows the selection as pixels.
    let mut driver = PaintDriver::new("paint-cmd-layer-overlap");
    run(&mut driver, "layer.new");
    run(&mut driver, "select.all");
    run(&mut driver, "select.shrink");
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(3);
    run(&mut driver, "edit.fill");
    let drawn = top_alpha(&driver);
    assert!(drawn.contains(&0), "the layer is not drawn at its border");
    assert!(drawn.contains(&255), "the layer is drawn inside");
    run(&mut driver, "select.all");
    run(&mut driver, "layer.selection.overlap");
    run(&mut driver, "select.to_layer");
    assert_eq!(top_alpha(&driver), drawn, "the selection is the overlap");
}

#[test]
fn select_overlapping_area_on_an_empty_layer_leaves_nothing_selected() {
    let mut driver = PaintDriver::new("paint-cmd-layer-overlap-empty");
    run(&mut driver, "layer.new");
    run(&mut driver, "select.all");
    run(&mut driver, "layer.selection.overlap");
    // With nothing selected, Convert to Selection Layer has nothing to
    // make.
    let start = depth(&driver);
    run(&mut driver, "select.to_layer");
    assert_eq!(depth(&driver), start, "the selection is gone");
}

#[test]
fn esc_on_a_new_correction_layer_takes_the_layer_away() {
    let mut driver = PaintDriver::new("paint-cmd-layer-correction");
    for command in BASE {
        run(&mut driver, command);
    }
    let (before, start) = (arena(&driver), depth(&driver));
    run(&mut driver, "layer.correction.levels");
    assert_eq!(depth(&driver), start + 1, "the layer is made at once");
    driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    driver.settle(3);
    assert_eq!(depth(&driver), start);
    assert!(arena(&driver) == before, "nothing of it is left");
}
