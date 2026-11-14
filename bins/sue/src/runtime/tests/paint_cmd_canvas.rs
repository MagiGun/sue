//! Sue Paint's Canvas commands (Edit, Rotate/Flip Canvas): each one run
//! as its menu item runs it, with its undo pair.

use super::*;

/// The whole arena as the file writes it: entities with their ids,
/// components and properties, and every relation.
fn arena_bytes(runtime: &Runtime) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

/// The middle of the ink on `layer`, in canvas pixels, and how many
/// pixels hold it.
fn ink_middle(
    runtime: &Runtime,
    picture: sue_core::EntityId,
    layer: sue_core::EntityId,
) -> ([f64; 2], u64) {
    let arena = &runtime.engine.world.arena;
    let raster = sue_paint::doc::raster(arena, layer).unwrap();
    let offset = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .find(|info| info.id == layer)
        .unwrap()
        .style
        .offset;
    let bytes = raster.bytes().unwrap();
    let (mut sum_x, mut sum_y, mut count) = (0.0, 0.0, 0_u64);
    for y in 0..raster.height() {
        for x in 0..raster.width() {
            if bytes[((y * raster.width() + x) * 4 + 3) as usize] > 128 {
                sum_x += f64::from(x);
                sum_y += f64::from(y);
                count += 1;
            }
        }
    }
    assert!(count > 0, "the layer has ink");
    let count_f = count as f64;
    (
        [
            sum_x / count_f + offset[0] as f64,
            sum_y / count_f + offset[1] as f64,
        ],
        count,
    )
}

/// Where a pixel at `at` of a `size` canvas goes when `command` runs.
fn moved(command: &str, at: [f64; 2], size: [u32; 2]) -> [f64; 2] {
    let [x, y] = at;
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    match command {
        "canvas.flip_h" => [w - 1.0 - x, y],
        "canvas.flip_v" => [x, h - 1.0 - y],
        "canvas.rotate180" => [w - 1.0 - x, h - 1.0 - y],
        "canvas.rotate_right" => [h - 1.0 - y, x],
        "canvas.rotate_left" => [y, w - 1.0 - x],
        other => panic!("{other} is not a Canvas command"),
    }
}

#[test]
fn sue_paint_every_canvas_command_turns_the_picture_and_undoes() {
    let mut driver = PaintDriver::new("paint-cmd-canvas");
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let picture = driver
        .runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .expect("New made a picture");
    let layer = sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    // A stroke in the top left part of the canvas, so every turn and
    // mirror moves it somewhere else.
    let middle = paint_middle(&driver.runtime);
    driver.drag(
        middle - vec2(220.0, 150.0),
        middle - vec2(120.0, 130.0),
        egui::PointerButton::Primary,
    );
    let size = |runtime: &Runtime| {
        sue_model::picture::size(runtime.engine.world.arena.get_by_id(picture).unwrap()).unwrap()
    };
    assert_eq!(size(&driver.runtime), [1600, 1200]);
    let (start, start_count) = ink_middle(&driver.runtime, picture, layer);
    assert!(start[0] < 700.0 && start[1] < 500.0, "top left: {start:?}");

    // Every command the group registers runs, as the menu runs it.
    let commands: Vec<&str> = sue_paint::registry()
        .iter()
        .filter(|command| command.group == "canvas")
        .map(|command| command.name)
        .collect();
    assert_eq!(commands.len(), 5, "{commands:?}");
    let mut at = start;
    for command in commands {
        let before = arena_bytes(&driver.runtime);
        let before_size = size(&driver.runtime);
        let depth = driver.runtime.engine.history.depth();
        driver.command(command);
        driver.settle(4);
        assert_eq!(
            driver.runtime.engine.history.depth(),
            depth + 1,
            "{command}: one undo step"
        );
        // History names the step as the menu names the item.
        let history = &driver.runtime.engine.history;
        assert_eq!(
            history.label(history.cursor()).map(str::to_owned),
            sue_paint::registry().label(command),
            "{command}: its undo step"
        );
        let after = arena_bytes(&driver.runtime);
        assert_ne!(before, after, "{command} changed the picture");
        let want = moved(command, at, before_size);
        let (got, count) = ink_middle(&driver.runtime, picture, layer);
        assert_eq!(count, start_count, "{command}: no pixel lost or added");
        assert!(
            (got[0] - want[0]).abs() < 0.01 && (got[1] - want[1]).abs() < 0.01,
            "{command}: the ink is at {got:?}, not {want:?}"
        );
        let turned = command.starts_with("canvas.rotate_");
        let [w, h] = before_size;
        assert_eq!(
            size(&driver.runtime),
            if turned { [h, w] } else { [w, h] },
            "{command}"
        );

        // Undo gives back the arena as it was, ids and all; redo the
        // arena as the command left it.
        driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        driver.settle(2);
        assert!(
            arena_bytes(&driver.runtime) == before,
            "{command}: undo restores the arena"
        );
        driver.step(vec![key(
            egui::Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )]);
        driver.settle(2);
        assert!(
            arena_bytes(&driver.runtime) == after,
            "{command}: redo restores what it did"
        );
        at = got;
    }
}
