//! Sue Paint: the Filter group (Edit, Tonal Correction and the Filter
//! menu), every command run on a canvas, undone and redone.

use super::*;
use sue_core::{Entity, EntityId};

/// Every entity of the arena, by id: what undo must give back exactly.
fn entities(runtime: &Runtime) -> Vec<Entity> {
    let mut all: Vec<Entity> = runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.clone())
        .collect();
    all.sort_by_key(Entity::id);
    all
}

/// The canvas in focus.
fn picture(driver: &PaintDriver) -> EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The raster layers of the canvas, bottom first, paper left out.
fn rasters(driver: &PaintDriver) -> Vec<EntityId> {
    sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture(driver))
        .into_iter()
        .filter(|info| info.kind == sue_model::picture::LayerKind::Raster)
        .map(|info| info.id)
        .collect()
}

/// Paints `layer` with something every filter has work on: colors that
/// change from pixel to pixel, two clear bands, half clear pixels, dark
/// specks and a dark line. Set up before any step is recorded.
fn paint_pattern(driver: &mut PaintDriver, layer: EntityId) {
    let raster = sue_paint::doc::raster_mut(&mut driver.runtime.engine.world.arena, layer)
        .expect("a raster layer");
    let width = raster.width() as usize;
    let bytes = raster.bytes_mut().expect("8-bit pixels");
    for (index, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (index % width, index / width);
        let hash = (x as u32)
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add((y as u32).wrapping_mul(0x85EB_CA6B));
        let hash = hash ^ (hash >> 13);
        let mut rgba = [
            (x * 255 / width.max(1)) as u8,
            (y % 256) as u8,
            (hash & 0xFF) as u8,
            255,
        ];
        if x < width / 8 || x > width * 7 / 8 {
            rgba[3] = 0;
        } else if x < width / 4 {
            rgba[3] = 128;
        }
        if y % 97 == 50 || (x % 61 == 30 && y % 53 == 20) {
            rgba = [10, 10, 10, 255];
        }
        pixel.copy_from_slice(&rgba);
    }
}

/// A canvas with a raster layer over the paper, painted, and nothing on
/// its undo history since.
fn canvas(tag: &str) -> (PaintDriver, EntityId) {
    let mut driver = PaintDriver::new(tag);
    driver.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    driver.settle(4);
    let layer = *rasters(&driver).last().expect("the new raster layer");
    paint_pattern(&mut driver, layer);
    driver.settle(4);
    (driver, layer)
}

/// The commands of the group that run at once; the others open a
/// dialog.
const AT_ONCE: [&str; 8] = [
    "filter.invert",
    "filter.grayscale",
    "filter.blur",
    "filter.blur_strong",
    "filter.smooth",
    "filter.clouds",
    "filter.sharpen",
    "filter.sharpen_strong",
];

/// Runs `command` as the menus do, and OK (Enter) on the dialog it
/// opens. Answers whether the dialog showed its OK button.
fn run_and_ok(driver: &mut PaintDriver, command: &str) -> bool {
    driver.command(command);
    driver.settle(4);
    if AT_ONCE.contains(&command) {
        return true;
    }
    let shown = driver.rect_of("OK").is_some() && driver.rect_of("Cancel").is_some();
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert!(
        driver.rect_of("OK").is_none(),
        "{command}: its dialog is still open"
    );
    shown
}

/// Undoes (or redoes) until the history is `depth` deep.
fn walk_to(driver: &mut PaintDriver, depth: usize) {
    for _ in 0..16 {
        let now = driver.runtime.engine.history.depth();
        if now == depth {
            return;
        }
        driver.command(if now > depth {
            "edit.undo"
        } else {
            "edit.redo"
        });
        driver.settle(2);
    }
    panic!(
        "the history stays {} deep, not {depth}",
        driver.runtime.engine.history.depth()
    );
}

/// The tonal corrections whose dialog opens on values that change
/// nothing, as Clip Studio Paint's do.
const UNCHANGED_AT_FIRST: [&str; 5] = [
    "filter.brightness",
    "filter.levels",
    "filter.curve",
    "filter.hue",
    "filter.balance",
];

#[test]
fn every_filter_command_runs_undoes_and_redoes() {
    let (mut driver, _) = canvas("paint-filter-all");
    let registry = sue_paint::registry();
    let group: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "filter")
        .map(|command| command.name)
        .collect();
    assert_eq!(group.len(), 50, "{group:?}");
    let base = driver.runtime.engine.history.depth();
    let before = entities(&driver.runtime);
    let mut cut_off = Vec::new();
    for name in group {
        if !run_and_ok(&mut driver, name) {
            cut_off.push(name);
        }
        let message = driver
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned());
        let done = driver.runtime.engine.history.depth();
        if UNCHANGED_AT_FIRST.contains(&name) {
            // OK on the starting values keeps the picture as it was.
            assert_eq!(done, base, "{name}: {message:?}");
            assert!(entities(&driver.runtime) == before, "{name} changed it");
            continue;
        }
        assert!(done > base, "{name} changed nothing: {message:?}");
        let after = entities(&driver.runtime);
        walk_to(&mut driver, base);
        assert!(
            entities(&driver.runtime) == before,
            "{name}: undo does not give the canvas back as it was"
        );
        walk_to(&mut driver, done);
        assert!(
            entities(&driver.runtime) == after,
            "{name}: redo does not give its result back"
        );
        walk_to(&mut driver, base);
    }
    // Every dialog shows its OK and Cancel: the dialog host is as tall
    // as what a dialog holds (P4-8).
    assert!(cut_off.is_empty(), "OK and Cancel cut off: {cut_off:?}");
}

#[test]
fn filters_are_grayed_without_a_raster_layer() {
    let mut driver = PaintDriver::new("paint-filter-paper");
    // From the first raster layer down to the paper: no filter works
    // there. A grayed command does nothing at all: no step, no dialog
    // and no message (a filter that ran would say the paper cannot be
    // drawn on).
    driver.command("layer.select_below");
    driver.settle(4);
    let message = |driver: &PaintDriver| {
        driver
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned())
    };
    let depth = driver.runtime.engine.history.depth();
    let said = message(&driver);
    let registry = sue_paint::registry();
    let group: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "filter")
        .map(|command| command.name)
        .collect();
    assert_eq!(group.len(), 50);
    for name in group {
        driver.command(name);
        driver.settle(4);
        assert_eq!(driver.runtime.engine.history.depth(), depth, "{name}");
        assert!(driver.rect_of("OK").is_none(), "{name} opened a dialog");
        assert_eq!(message(&driver), said, "{name} ran on the paper");
    }
}

#[test]
fn grayscale_turns_every_picked_layer_gray_in_one_step() {
    let (mut driver, top) = canvas("paint-filter-gray");
    // A second painted layer under it, then both picked.
    let [.., below, _] = rasters(&driver)[..] else {
        panic!("two raster layers");
    };
    paint_pattern(&mut driver, below);
    driver.command("layer.select_all");
    driver.settle(2);
    let alpha = |driver: &PaintDriver, layer| -> Vec<u8> {
        sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer)
            .and_then(|raster| raster.bytes())
            .expect("pixels")
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect()
    };
    let alphas = [alpha(&driver, below), alpha(&driver, top)];
    let depth = driver.runtime.engine.history.depth();
    driver.command("filter.grayscale");
    driver.settle(4);
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1, "one step");
    for (layer, alphas) in [below, top].into_iter().zip(&alphas) {
        let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        assert!(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]),
            "every pixel is gray"
        );
        let kept: Vec<u8> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        assert_eq!(&kept, alphas, "alpha is kept");
    }
    // Pure blue goes dark, not white: the gray has the color's
    // brightness.
    let blue = [0u8, 0, 255, 255];
    let raster = sue_paint::doc::raster_mut(&mut driver.runtime.engine.world.arena, top).unwrap();
    raster.bytes_mut().unwrap()[..4].copy_from_slice(&blue);
    driver.command("layer.select_none");
    driver.command("filter.grayscale");
    driver.settle(4);
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, top).unwrap();
    let gray = raster.bytes().unwrap()[0];
    assert!(gray < 40, "blue turned {gray}");
}

#[test]
fn grayscale_keeps_to_the_selection_and_names_its_step() {
    let (mut driver, layer) = canvas("paint-filter-gray-selection");
    // Select All, then Shrink Selected Area by its first value (4): a
    // band at the canvas edge is left out.
    driver.command("select.all");
    driver.settle(2);
    driver.command("select.shrink");
    driver.settle(4);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let depth = driver.runtime.engine.history.depth();
    driver.command("filter.grayscale");
    driver.settle(4);
    let history = &driver.runtime.engine.history;
    assert_eq!(history.depth(), depth + 1, "one step");
    assert_eq!(history.label(history.cursor()), Some("Grayscale"));
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    let (width, height) = (raster.width() as usize, raster.height() as usize);
    let pixels = raster.bytes().unwrap().as_chunks::<4>().0;
    let gray = |pixel: &[u8; 4]| pixel[0] == pixel[1] && pixel[1] == pixel[2];
    for y in 8..height - 8 {
        for x in 8..width - 8 {
            assert!(gray(&pixels[y * width + x]), "({x}, {y}) is selected");
        }
    }
    // The top row was left out of the selection: its colors stay.
    let top = &pixels[width / 4..width * 3 / 4];
    assert!(
        top.iter().any(|pixel| pixel[3] > 0 && !gray(pixel)),
        "the unselected edge turned gray too"
    );
}
