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
const AT_ONCE: [&str; 9] = [
    "filter.invert",
    "filter.grayscale",
    "filter.lines_tones",
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
    assert_eq!(group.len(), 52, "{group:?}");
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
    // Every dialog is as tall as its settings, so OK and Cancel show in
    // each (P3-1 found Drop Shadow, Focus Lines and Speed Lines cut off).
    assert!(cut_off.is_empty(), "OK or Cancel cut off: {cut_off:?}");
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
    assert_eq!(group.len(), 52);
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

/// The layer's pixels now.
fn pixels_of(driver: &PaintDriver, layer: EntityId) -> Vec<u8> {
    sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
        .expect("8-bit pixels")
}

/// Every text `names` is on screen.
fn shows_all(driver: &PaintDriver, names: &[&str]) {
    for name in names {
        assert!(
            driver.rect_of(name).is_some(),
            "{name:?} is in the dialog: {:?}",
            driver
                .texts
                .iter()
                .map(|(text, _)| text)
                .collect::<Vec<_>>()
        );
    }
}

/// Where a test stands before it opens a dialog: the history's depth
/// and every entity.
fn standing(driver: &PaintDriver) -> (usize, Vec<Entity>) {
    (
        driver.runtime.engine.history.depth(),
        entities(&driver.runtime),
    )
}

/// OK (Enter) on the open dialog, then undo and redo walked and checked
/// exact against where the test stood before it opened the dialog
/// (`from`); answers the layer's pixels after OK.
fn ok_and_undo_pair(
    driver: &mut PaintDriver,
    layer: EntityId,
    from: &(usize, Vec<Entity>),
) -> Vec<u8> {
    let (base, before) = (from.0, &from.1);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let done = driver.runtime.engine.history.depth();
    assert_eq!(done, base + 1, "one undo step");
    let after = entities(&driver.runtime);
    let result = pixels_of(driver, layer);
    walk_to(driver, base);
    assert!(entities(&driver.runtime) == *before, "undo is exact");
    walk_to(driver, done);
    assert!(entities(&driver.runtime) == after, "redo is exact");
    result
}

#[test]
fn level_correction_changes_one_channel_alone() {
    // H-1: Level Correction has RGB, Red, Green and Blue.
    let (mut driver, layer) = canvas("paint-filter-levels");
    let before = pixels_of(&driver, layer);
    let from = standing(&driver);
    driver.command("filter.levels");
    driver.settle(4);
    shows_all(&driver, &["RGB", "Red", "Green", "Blue", "Gamma"]);
    driver.click_text("Red");
    // The output's Highlight is the lower of the two: a click near the
    // start of its track pulls red's white far down.
    let lowest = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Highlight")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the output highlight");
    // Its track lies between its name and its number field ("1.00",
    // on the same row): a click a third of the way along it.
    let field = driver
        .texts
        .iter()
        .filter(|(text, rect)| text == "1.00" && (rect.center().y - lowest.center().y).abs() < 4.0)
        .map(|(_, rect)| *rect)
        .next()
        .expect("the output highlight's field");
    let span = field.left() - lowest.left();
    let at = lowest.left_center() + vec2(span * 0.7, 0.0);
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(4);
    let after = ok_and_undo_pair(&mut driver, layer, &from);
    let (mut darker, mut same) = (0, true);
    for (old, new) in before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
    {
        if old[3] == 0 {
            continue;
        }
        if new[0] < old[0] {
            darker += 1;
        }
        same &= new[1] == old[1] && new[2] == old[2];
    }
    assert!(darker > 1000, "red pulled down: {darker}");
    assert!(same, "green and blue untouched");
}

#[test]
fn gradient_map_offers_its_set_and_maps_with_stops() {
    // H-2, X-2, CTV-12: a gradient set list and gradients of several
    // stops; picking one maps the layer through it.
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let (mut driver, layer) = canvas("paint-filter-gradient");
    let before = pixels_of(&driver, layer);
    let from = standing(&driver);
    driver.command("filter.gradient_map");
    driver.settle(4);
    shows_all(
        &driver,
        &[
            "Gradient Set",
            "Dusk",
            "Forest",
            "Mixing Rate",
            "Position (%)",
        ],
    );
    driver.click_text("Dusk");
    let after = ok_and_undo_pair(&mut driver, layer, &from);
    let dusk = sue_paint::gradient_map::GradientSet::builtin()
        .gradients
        .into_iter()
        .find(|gradient| gradient.name == "Dusk")
        .expect("Sue ships Dusk")
        .map;
    assert!(dusk.stops().len() > 2, "more than two stops");
    let mut want = before.clone();
    let [width, height] = {
        let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
        [raster.width(), raster.height()]
    };
    sue_paint::filters::Filter::Gradient { map: dusk }.apply(
        &mut want,
        width,
        height,
        None,
        [0, 0],
    );
    assert!(after == want, "the layer mapped through Dusk's stops");
}

#[test]
fn radial_blur_turns_round_the_centre_put_on_its_pad() {
    // FGV-52: the centre is put by a click on the picture.
    let (mut driver, layer) = canvas("paint-filter-radial");
    let from = standing(&driver);
    driver.command("filter.radial_blur");
    driver.settle(4);
    let middle = ok_and_undo_pair(&mut driver, layer, &from);
    walk_to(&mut driver, from.0);
    driver.command("filter.radial_blur");
    driver.settle(4);
    let heading = driver.rect_of("Center").expect("the center pad's heading");
    // The pad lies under its heading; its top left corner.
    let at = heading.left_bottom() + vec2(heading.height(), heading.height() * 1.5);
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(4);
    let corner = ok_and_undo_pair(&mut driver, layer, &from);
    assert!(
        corner != middle,
        "a centre at the corner turns it otherwise"
    );
}

#[test]
fn perlin_noise_and_curved_surface_have_their_settings() {
    // FGV-53, CTV-11: Perlin Noise's six settings; Distort starts with
    // Curved Surface.
    let (mut driver, layer) = canvas("paint-filter-perlin");
    let from = standing(&driver);
    driver.command("filter.perlin");
    driver.settle(4);
    shows_all(
        &driver,
        &[
            "Scale",
            "Amplitude",
            "Attenuation",
            "Repetition",
            "Offset X",
            "Offset Y",
        ],
    );
    let noise = ok_and_undo_pair(&mut driver, layer, &from);
    assert!(
        noise
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2] && pixel[3] == 255),
        "gray noise over all of it"
    );
    walk_to(&mut driver, from.0);
    let menus = sue_paint::menus();
    let filter = menus
        .iter()
        .find(|menu| menu.title == "Filter")
        .expect("the Filter menu");
    let distort = filter
        .items
        .iter()
        .find_map(|item| match item {
            sue_wm::MenuItem::Sub { label, items } if label == "Distort" => Some(items),
            _ => None,
        })
        .expect("Filter, Distort");
    assert!(
        matches!(distort.first(), Some(sue_wm::MenuItem::Action { command, .. }) if command == "filter.curved_surface"),
        "Curved Surface first"
    );
    driver.command("filter.curved_surface");
    driver.settle(4);
    shows_all(&driver, &["Amount", "Both Ways", "Across", "Up and Down"]);
    driver.click_text("Across");
    ok_and_undo_pair(&mut driver, layer, &from);
}

#[test]
fn artistic_shows_its_line_and_color_settings() {
    // FG-29, FGV-54.
    let (mut driver, layer) = canvas("paint-filter-artistic");
    let before = pixels_of(&driver, layer);
    let from = standing(&driver);
    driver.command("filter.artistic");
    driver.settle(4);
    shows_all(
        &driver,
        &[
            "Colors and Lines",
            "Lines",
            "Colors",
            "Line Thickness",
            "Line Simplicity",
            "Line Density",
            "Line Opacity",
            "Line Anti-aliasing",
            "Color Blending",
            "Color Blur",
            "Number of Colors",
        ],
    );
    let after = ok_and_undo_pair(&mut driver, layer, &from);
    assert!(after != before);
}

#[test]
fn smart_smoothing_has_weak_medium_strong_and_remove_noise() {
    // Full guide 00:42:11: Intensity, Remove noise and Preview.
    let (mut driver, layer) = canvas("paint-filter-smart");
    let before = pixels_of(&driver, layer);
    let from = standing(&driver);
    driver.command("edit.smart_smoothing");
    driver.settle(4);
    shows_all(
        &driver,
        &[
            "Intensity",
            "Weak",
            "Medium",
            "Strong",
            "Remove Noise",
            "Preview",
        ],
    );
    driver.click_text("Strong");
    driver.click_text("Remove Noise");
    let after = ok_and_undo_pair(&mut driver, layer, &from);
    assert!(after != before);
}

#[test]
fn smoothing_is_named_as_clip_studio_paint_names_it() {
    // A25-27: Filter, Blur, Smoothing, and its undo step says so.
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let (mut driver, _) = canvas("paint-filter-smoothing");
    assert_eq!(
        sue_paint::registry().label("filter.smooth").as_deref(),
        Some("Smoothing")
    );
    driver.command("filter.smooth");
    driver.settle(4);
    let history = &driver.runtime.engine.history;
    assert_eq!(history.label(history.cursor()), Some("Smoothing"));
}

#[test]
fn colorize_fills_the_line_art_from_the_hint_layer() {
    // FG-1: Edit, Colorize with a hint layer, on the device.
    let mut driver = PaintDriver::new("paint-colorize");
    let first = *rasters(&driver).last().expect("a raster layer");
    // The line art above: a black line down the middle.
    driver.command("layer.new");
    driver.settle(4);
    let lines = *rasters(&driver).last().expect("the line art layer");
    assert_ne!(lines, first);
    {
        let raster =
            sue_paint::doc::raster_mut(&mut driver.runtime.engine.world.arena, lines).unwrap();
        let width = raster.width();
        let height = raster.height();
        let bytes = raster.bytes_mut().unwrap();
        for y in 0..height {
            for x in width / 2 - 2..width / 2 + 2 {
                let at = ((y * width + x) * 4) as usize;
                bytes[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    // The hint layer under it: a red stroke left, a blue one right.
    {
        let raster =
            sue_paint::doc::raster_mut(&mut driver.runtime.engine.world.arena, first).unwrap();
        let width = raster.width();
        let bytes = raster.bytes_mut().unwrap();
        for y in 500..520 {
            for (x, rgb) in [(300, [220, 30, 30]), (1300, [30, 40, 220])] {
                let at = ((y * width + x) * 4) as usize;
                bytes[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    }
    driver.command("layer.select_below");
    driver.settle(4);
    driver.command("edit.colorize");
    driver.settle(4);
    shows_all(&driver, &["Line Art", "Blur Strength", "OK", "Cancel"]);
    let base = driver.runtime.engine.history.depth();
    let before = entities(&driver.runtime);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert_eq!(driver.runtime.engine.history.depth(), base + 1, "one step");
    let after = entities(&driver.runtime);
    // A new layer, right under the line art, left red and right blue.
    let infos = sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture(&driver));
    let at = infos.iter().position(|info| info.id == lines).unwrap();
    let colors = infos[at - 1].id;
    assert!(colors != first && colors != lines, "a layer of its own");
    let pixels = pixels_of(&driver, colors);
    let width = sue_paint::doc::raster(&driver.runtime.engine.world.arena, colors)
        .unwrap()
        .width();
    let pixel = |x: u32, y: u32| {
        let at = ((y * width + x) * 4) as usize;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    };
    assert_eq!(pixel(40, 40), [220, 30, 30, 255]);
    assert_eq!(pixel(1560, 1100), [30, 40, 220, 255]);
    walk_to(&mut driver, base);
    assert!(entities(&driver.runtime) == before, "undo is exact");
    walk_to(&mut driver, base + 1);
    assert!(entities(&driver.runtime) == after, "redo is exact");
}
