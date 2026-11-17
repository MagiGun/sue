//! Phase 6, "Brush engine: mixing, blenders, paper, ends, smooth
//! curves": the tutorial steps and gaps it closes, each followed in the
//! running Sue Paint and named after its row (`cc_` and the time for
//! the Crash Course, `cc5_` and the like for its report's ids, `t50_`
//! for the fifty tips, `fg_` and the time or `fgv` and the id for the
//! full guide). Every step drives real frames: keys, clicks on what a
//! frame drew, drags on the canvas and the menus' commands; the pixels
//! the layers hold are read after.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton};
use sue_core::EntityId;

// ----- Driving ---------------------------------------------------------------

fn open(tag: &str) -> PaintDriver {
    PaintDriver::new(&format!("p6-brush-{tag}"))
}

fn open_fresh(tag: &str) -> PaintDriver {
    let mut driver = PaintDriver::fresh(&format!("p6-brush-{tag}"));
    driver.settle(4);
    driver
}

/// One frame with `events` while `held` are held.
fn step_held(driver: &mut PaintDriver, events: Vec<Event>, held: Modifiers) {
    let mut events = events;
    events.insert(0, Event::ModifiersChanged(held));
    driver.step(events);
}

/// A press at the first of `points`, one frame for each move through
/// the rest (`between` moves on the way to each), and the release at the
/// last.
fn path(driver: &mut PaintDriver, points: &[Pos2], between: usize) {
    let held = Modifiers::NONE;
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: held,
    };
    let first = points[0];
    step_held(driver, vec![Event::PointerMoved(first)], held);
    step_held(driver, vec![press(first, true)], held);
    for pair in points.windows(2) {
        for index in 1..=between.max(1) {
            let at = pair[0] + (pair[1] - pair[0]) * (index as f32 / between.max(1) as f32);
            step_held(driver, vec![Event::PointerMoved(at)], held);
        }
    }
    let last = *points.last().unwrap();
    step_held(driver, vec![press(last, false)], held);
    for _ in 0..4 {
        step_held(driver, Vec::new(), held);
    }
}

/// A drag of the main button from `from` to `to`.
fn stroke(driver: &mut PaintDriver, from: Pos2, to: Pos2) {
    path(driver, &[from, to], 6);
}

/// A click of the main button at `at`.
fn click(driver: &mut PaintDriver, at: Pos2) {
    path(driver, &[at], 1);
}

fn press(driver: &mut PaintDriver, key_name: Key, held: Modifiers) {
    driver.step(vec![key(key_name, held)]);
    driver.step(vec![Event::Key {
        key: key_name,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: held,
    }]);
    driver.settle(3);
}

fn tap(driver: &mut PaintDriver, key_name: Key) {
    press(driver, key_name, Modifiers::NONE);
}

fn run(driver: &mut PaintDriver, command: &str) {
    driver.command(command);
    driver.settle(4);
}

/// Every widget with a name a frame draws, and where.
fn widgets(driver: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    driver.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    driver.texts = text_rects(&output);
    let mut found = Vec::new();
    if let Some(update) = &output.platform_output.accesskit_update {
        for (_, node) in &update.nodes {
            if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                found.push((
                    label.to_owned(),
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ));
            }
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// A click on the widget named `name` inside `area`.
fn click_widget_in(driver: &mut PaintDriver, name: &str, area: egui::Rect) {
    let all = widgets(driver);
    let rect = all
        .iter()
        .filter(|(label, rect)| label == name && area.contains(rect.center()))
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| {
            let names: Vec<&String> = all.iter().map(|(l, _)| l).collect();
            panic!("{name:?} in {area:?}: {names:?}")
        });
    click(driver, rect.center());
    driver.settle(4);
}

fn shows(driver: &PaintDriver, text: &str) -> bool {
    driver.texts.iter().any(|(found, _)| found == text)
}

/// The texts the last frame drew inside `area`.
fn texts_in(driver: &PaintDriver, area: egui::Rect) -> Vec<String> {
    driver
        .texts
        .iter()
        .filter(|(_, rect)| area.contains(rect.center()))
        .map(|(text, _)| text.clone())
        .collect()
}

// ----- Reading the canvas ----------------------------------------------------

fn picture(driver: &PaintDriver) -> EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

fn arena(driver: &PaintDriver) -> &sue_core::Arena {
    &driver.runtime.engine.world.arena
}

fn layers(driver: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(arena(driver), picture(driver))
}

fn top(driver: &PaintDriver) -> EntityId {
    layers(driver).last().expect("a layer").id
}

fn size(driver: &PaintDriver) -> [u32; 2] {
    sue_model::picture::size(arena(driver).get_by_id(picture(driver)).unwrap()).unwrap()
}

/// The layer's pixels.
fn raster_bytes(driver: &PaintDriver, layer: EntityId) -> Vec<u8> {
    sue_paint::doc::raster(arena(driver), layer)
        .and_then(sue_core::RasterTile2D::bytes)
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

/// Picture pixel `p` of `layer` (a layer that sits at the picture's
/// corner), straight RGBA.
fn pixel(driver: &PaintDriver, layer: EntityId, p: [i64; 2]) -> [u8; 4] {
    let Some(raster) = sue_paint::doc::raster(arena(driver), layer) else {
        return [0; 4];
    };
    let (x, y) = (p[0], p[1]);
    if x < 0 || y < 0 || x >= i64::from(raster.width()) || y >= i64::from(raster.height()) {
        return [0; 4];
    }
    let at = ((y * i64::from(raster.width()) + x) * 4) as usize;
    let bytes = raster.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// Paints every pixel of `layer` with `rgba(x, y)`, as a picture made
/// before the step would hold them.
fn fill_with(driver: &mut PaintDriver, layer: EntityId, rgba: impl Fn(u32, u32) -> [u8; 4]) {
    let arena = &mut driver.runtime.engine.world.arena;
    let raster = sue_paint::doc::raster_mut(arena, layer).expect("pixels");
    let width = raster.width();
    for (i, p) in raster.bytes_mut().expect("bytes").chunks_mut(4).enumerate() {
        p.copy_from_slice(&rgba(i as u32 % width, i as u32 / width));
    }
    driver.settle(2);
}

fn middle(driver: &PaintDriver) -> Pos2 {
    paint_middle(&driver.runtime)
}

/// The zoom, as the window's title carries it.
fn zoom(driver: &PaintDriver) -> f32 {
    let title = driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(arena(driver)))
        .expect("Sue Paint names its window");
    title
        .split_whitespace()
        .find_map(|word| {
            word.trim_end_matches(')')
                .strip_suffix('%')?
                .parse::<f32>()
                .ok()
        })
        .unwrap_or_else(|| panic!("a zoom in {title:?}"))
        / 100.0
}

/// The picture pixel under screen point `at`.
fn under(driver: &PaintDriver, at: Pos2) -> [i64; 2] {
    let [w, h] = size(driver);
    let scale = zoom(driver);
    let d = (at - middle(driver)) / scale;
    [
        (w as f32 / 2.0 + d.x).floor() as i64,
        (h as f32 / 2.0 + d.y).floor() as i64,
    ]
}

/// The arena as bytes: entities, ids, components, properties, relations.
fn bytes(driver: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    arena(driver).encode(&mut out);
    out.into_bytes()
}

/// Rule 7: the last step undone gives `before` back exactly, ids and
/// all, and redone gives what it made.
fn undo_pair(driver: &mut PaintDriver, before: &[u8]) {
    let after = bytes(driver);
    assert_ne!(after, before, "the step changed the picture");
    press(driver, Key::Z, Modifiers::COMMAND);
    assert!(bytes(driver) == before, "undo gives back what was");
    press(driver, Key::Y, Modifiers::COMMAND);
    assert!(bytes(driver) == after, "redo gives back what was made");
}

/// The sub tool `name` of the tool whose id is `tool`, picked by its
/// command, as the Sub Tool palette picks it.
fn pick(driver: &mut PaintDriver, tool: &str, name: &str) {
    run(
        driver,
        &format!("subtool.{tool}.{}", name.replace(' ', "_")),
    );
}

/// A new raster layer: its id.
fn new_layer(driver: &mut PaintDriver) -> EntityId {
    run(driver, "layer.new");
    top(driver)
}

// ----- CC 28:14: mixing ------------------------------------------------------

/// CC 28:14: a color mixing brush (Oil paint) blends the colors it
/// passes over: white dragged out of black takes the black along. The
/// tutorial's own test of this row is ignored in its file
/// (`tutorial_crash_tips::cc_2814_oil_paint_mixes_the_colors_it_crosses`);
/// this is the same step, and it passes.
#[test]
fn cc_2814_oil_paint_mixes_the_colors_it_crosses() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("oil");
    let layer = new_layer(&mut driver);
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-200.0, -100.0), m + vec2(0.0, 100.0));
    run(&mut driver, "edit.fill");
    run(&mut driver, "select.none");
    // White paint dragged from the black into the empty side.
    tap(&mut driver, Key::X);
    run(&mut driver, "tool.pick.brush");
    driver.click_text("Thick paint");
    driver.click_text("Oil paint");
    for _ in 0..8 {
        tap(&mut driver, Key::CloseBracket);
    }
    stroke(&mut driver, m + vec2(-60.0, 0.0), m + vec2(60.0, 0.0));
    let [x0, y] = under(&driver, m + vec2(-60.0, 0.0));
    let [x1, _] = under(&driver, m + vec2(60.0, 0.0));
    let row: Vec<[u8; 4]> = (x0..x1).map(|x| pixel(&driver, layer, [x, y])).collect();
    let mixed = row
        .iter()
        .filter(|p| p[3] > 0 && p[0] > 30 && p[0] < 225)
        .count();
    assert!(
        mixed > 10,
        "a mix of black and white along the stroke: {mixed}"
    );
}

// ----- CC-5, FGV-23: the Blend group -----------------------------------------

const BLENDERS: [&str; 5] = [
    "Blend",
    "Painterly blender",
    "Wet bleed blender",
    "Textured blender",
    "Soothing watercolor",
];

/// CC-5 (16:00, 22:38) and FGV-23 (00:28:00): the Blend tool lists
/// Blend, Blur, Finger Tip, Painterly blender, Wet bleed blender,
/// Textured blender, Soothing watercolor and Copy stamp, in that order.
#[test]
fn cc5_fgv23_the_blend_tool_lists_its_blenders() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("blend-list");
    run(&mut driver, "tool.blend");
    driver.settle(4);
    let order = [
        "Blend",
        "Blur",
        "Finger Tip",
        "Painterly blender",
        "Wet bleed blender",
        "Textured blender",
        "Soothing watercolor",
        "Copy stamp",
    ];
    let tops: Vec<f32> = order
        .iter()
        .map(|name| {
            driver
                .texts
                .iter()
                .filter(|(text, rect)| text == name && rect.left() > 40.0)
                .map(|(_, rect)| rect.top())
                .fold(f32::INFINITY, f32::min)
        })
        .collect();
    assert!(
        tops.iter().all(|top| top.is_finite()),
        "every sub tool is listed: {:?} in {:?}",
        tops,
        driver.texts
    );
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "in Clip Studio Paint's order: {tops:?}"
    );
}

/// CC-5 and FGV-23: each blender works the paint under it in its own
/// way, with no paint of its own: across black and white stripes in red,
/// none lays red, each changes the stripes, and no two leave them alike.
/// One stroke is one undo step.
#[test]
fn cc5_fgv23_each_blender_works_the_paint_its_own_way() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blenders");
    run(&mut driver, "view.actual");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |x, _| {
        let v = if (x / 12).is_multiple_of(2) { 0 } else { 255 };
        [v, v, v, 255]
    });
    let stripes = raster_bytes(&driver, layer);
    let m = middle(&driver);
    let [_, y] = under(&driver, m);
    let [x0, _] = under(&driver, m + vec2(-60.0, 0.0));
    let [x1, _] = under(&driver, m + vec2(60.0, 0.0));
    let mut looks = Vec::new();
    for name in BLENDERS {
        pick(&mut driver, "blend", name);
        let before = bytes(&driver);
        stroke(&mut driver, m + vec2(-120.0, 0.0), m + vec2(120.0, 0.0));
        let row: Vec<[u8; 4]> = (x0..x1).map(|x| pixel(&driver, layer, [x, y])).collect();
        let was: Vec<[u8; 4]> = (x0..x1)
            .map(|x| {
                let at = ((y * i64::from(size(&driver)[0]) + x) * 4) as usize;
                [
                    stripes[at],
                    stripes[at + 1],
                    stripes[at + 2],
                    stripes[at + 3],
                ]
            })
            .collect();
        let moved: f32 = row
            .iter()
            .zip(&was)
            .map(|(a, b)| f32::from(a[0].abs_diff(b[0])))
            .sum::<f32>()
            / row.len() as f32;
        let red = row.iter().map(|p| p[0].saturating_sub(p[1])).max().unwrap();
        assert!(moved > 8.0, "{name} changes the stripes: {moved}");
        assert!(red <= 2, "{name} lays no paint of its own: {red}");
        // The band the stroke worked, for telling the blenders apart.
        let band: Vec<u8> = (y - 8..=y + 8)
            .flat_map(|yy| (x0..x1).map(move |x| (x, yy)))
            .map(|(x, yy)| pixel(&driver, layer, [x, yy])[0])
            .collect();
        looks.push((name, band));
        if name == "Blend" {
            undo_pair(&mut driver, &before);
        }
        press(&mut driver, Key::Z, Modifiers::COMMAND);
        assert_eq!(raster_bytes(&driver, layer), stripes, "{name}: one undo");
    }
    for (i, (a, ra)) in looks.iter().enumerate() {
        for (b, rb) in &looks[i + 1..] {
            let differ = ra
                .iter()
                .zip(rb)
                .map(|(p, q)| f32::from(p.abs_diff(*q)))
                .sum::<f32>()
                / ra.len() as f32;
            assert!(
                differ > 1.5,
                "{a} and {b} leave the stripes alike: {differ}"
            );
        }
    }
}

/// CC-4 (07:20): Liquify, a group of its own beside Blend, has a
/// Hardness in Tool Property (50 % to begin with), and a harder dab
/// moves more of the picture whole.
#[test]
fn cc4_liquify_has_a_hardness() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("liquify-hardness");
    pick(&mut driver, "blend", "Liquify");
    driver.settle(4);
    assert!(shows(&driver, "Hardness"), "{:?}", driver.texts);
    // The value on the Hardness row itself, not any 50 on the screen.
    let row = driver
        .texts
        .iter()
        .find(|(text, _)| text == "Hardness")
        .map(|(_, rect)| *rect)
        .expect("the Hardness row");
    assert!(
        driver
            .texts
            .iter()
            .any(|(text, rect)| text.starts_with("50")
                && rect.left() > row.left()
                && (rect.center().y - row.center().y).abs() < row.height()),
        "Hardness 50 %: {:?}",
        driver.texts
    );
}

// ----- 00:27:43: Airbrush, Tone scraping -------------------------------------

/// Full guide 00:27:43: the Airbrush's Tone scraping takes a tone away
/// in dots, not in an even fade. One undo step.
#[test]
fn fg_002743_tone_scraping_scrapes_a_tone_in_dots() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("tone-scraping");
    run(&mut driver, "view.actual");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |_, _| [0, 0, 0, 255]);
    run(&mut driver, "tool.pick.airbrush");
    assert!(shows(&driver, "Tone scraping"), "{:?}", driver.texts);
    pick(&mut driver, "airbrush", "Tone scraping");
    let before = bytes(&driver);
    let m = middle(&driver);
    for dy in [-12.0, -4.0, 4.0, 12.0] {
        stroke(&mut driver, m + vec2(-100.0, dy), m + vec2(100.0, dy));
    }
    let [cx, cy] = under(&driver, m);
    let (mut gone, mut kept, mut between) = (0, 0, 0);
    for y in cy - 8..=cy + 8 {
        for x in cx - 40..=cx + 40 {
            match pixel(&driver, layer, [x, y])[3] {
                0..=40 => gone += 1,
                215..=255 => kept += 1,
                _ => between += 1,
            }
        }
    }
    assert!(gone > 100, "dots scraped away: {gone}");
    assert!(kept > 100, "tone kept between them: {kept}");
    assert!(between < gone + kept, "dots, not a fade: {between}");
    // The four strokes are four undo steps; the last one's pair.
    let _ = before;
    let now = bytes(&driver);
    press(&mut driver, Key::Z, Modifiers::COMMAND);
    press(&mut driver, Key::Y, Modifiers::COMMAND);
    assert!(bytes(&driver) == now, "undone and redone");
}

// ----- Tip 23, CT-22, CTV-9: the new erasers ---------------------------------

/// Tip 23 (05:41, CT-22): the Multiple layers eraser erases on every
/// layer it can at once (a hidden or locked one is left alone), as one
/// undo step that gives every layer back.
#[test]
fn t50_23_ct22_the_multiple_layers_eraser_erases_every_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("multiple-layers");
    run(&mut driver, "view.actual");
    let lower = new_layer(&mut driver);
    fill_with(&mut driver, lower, |_, _| [200, 0, 0, 255]);
    let hidden = new_layer(&mut driver);
    fill_with(&mut driver, hidden, |_, _| [0, 200, 0, 255]);
    run(&mut driver, "layer.toggle.visible");
    let upper = new_layer(&mut driver);
    fill_with(&mut driver, upper, |_, _| [0, 0, 200, 255]);
    run(&mut driver, "tool.eraser");
    assert!(shows(&driver, "Multiple layers"), "{:?}", driver.texts);
    assert!(shows(&driver, "Snap eraser"), "{:?}", driver.texts);
    pick(&mut driver, "eraser", "Multiple layers");
    let before = bytes(&driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    let at = under(&driver, m);
    assert_eq!(pixel(&driver, upper, at)[3], 0, "the layer drawn on");
    assert_eq!(pixel(&driver, lower, at)[3], 0, "the layer below too");
    assert_eq!(pixel(&driver, hidden, at)[3], 255, "not the hidden one");
    let off = under(&driver, m + vec2(0.0, 120.0));
    assert_eq!(pixel(&driver, lower, off)[3], 255, "only along the stroke");
    undo_pair(&mut driver, &before);
}

/// Tip 23 (CT-22): a layer in a hidden folder is not shown, so the
/// Multiple layers eraser leaves it alone, as it does a hidden layer.
#[test]
fn t50_23_ct22_the_multiple_layers_eraser_skips_a_hidden_folder() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("multiple-layers-folder");
    run(&mut driver, "view.actual");
    let drawn = new_layer(&mut driver);
    fill_with(&mut driver, drawn, |_, _| [0, 0, 200, 255]);
    let tucked = new_layer(&mut driver);
    fill_with(&mut driver, tucked, |_, _| [0, 200, 0, 255]);
    run(&mut driver, "layer.folder_insert");
    let folder = layers(&driver)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(arena(&driver), info.id))
        .expect("a folder")
        .id;
    assert_eq!(
        sue_paint::doc::folder_of(arena(&driver), tucked),
        Some(folder)
    );
    assert_eq!(sue_paint::doc::folder_of(arena(&driver), drawn), None);
    run(&mut driver, "layer.toggle.visible");
    let shown = |driver: &PaintDriver, id: EntityId| {
        layers(driver)
            .into_iter()
            .find(|info| info.id == id)
            .is_some_and(|info| info.style.visible)
    };
    assert!(!shown(&driver, folder), "the folder is hidden");
    assert!(shown(&driver, tucked), "its layer keeps its own eye open");
    // From the folder down past its layer to the one outside it.
    run(&mut driver, "layer.select_below");
    run(&mut driver, "layer.select_below");
    pick(&mut driver, "eraser", "Multiple layers");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    let at = under(&driver, m);
    assert_eq!(pixel(&driver, drawn, at)[3], 0, "the layer drawn on");
    assert_eq!(
        pixel(&driver, tucked, at)[3],
        255,
        "not a layer in a hidden folder"
    );
}

/// V-9 (CTV-9): the Snap eraser on a vector layer takes a line away up
/// to where it crosses another, in one touch.
#[test]
fn ctv9_the_snap_eraser_erases_a_vector_line_to_its_crossing() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("snap-eraser");
    run(&mut driver, "layer.new.vector");
    let layer = top(&driver);
    pick(&mut driver, "pen", "G-pen");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-100.0, 0.0), m + vec2(100.0, 0.0));
    stroke(&mut driver, m + vec2(0.0, -80.0), m + vec2(0.0, 40.0));
    let lines = |driver: &PaintDriver| {
        sue_paint::vector::lines(&sue_paint::vector::geometry(arena(driver), layer))
    };
    assert_eq!(lines(&driver).len(), 2);
    pick(&mut driver, "eraser", "Snap eraser");
    let before = bytes(&driver);
    stroke(&mut driver, m + vec2(-20.0, 30.0), m + vec2(20.0, 30.0));
    let [_, y] = under(&driver, m);
    let lowest = lines(&driver)
        .iter()
        .flat_map(|line| line.samples.iter().map(|sample| sample.at[1]))
        .fold(f32::MIN, f32::max);
    assert!(
        (lowest - y as f32).abs() < 6.0,
        "the overshoot stops at the crossing: {lowest} against {y}"
    );
    undo_pair(&mut driver, &before);
}

// ----- FGV-27, FGV-28, 00:12:49: Sub Tool Detail's texture, watercolor edge
// and dual brush --------------------------------------------------------------

/// Sub Tool Detail opened for the G-pen, and its window.
fn detail(driver: &mut PaintDriver) -> egui::Rect {
    pick(driver, "pen", "G-pen");
    run(driver, "window.sub_tool_detail");
    driver.settle(6);
    widgets(driver)
        .into_iter()
        .filter(|(label, _)| label == "Sub Tool Detail")
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("Sub Tool Detail's window")
}

/// A G-pen stroke on a new layer across the middle, pressing evenly: the
/// layer's pixels; then undone.
fn pen_stroke(driver: &mut PaintDriver) -> Vec<u8> {
    let layer = new_layer(driver);
    let m = middle(driver);
    for _ in 0..6 {
        tap(driver, Key::CloseBracket);
    }
    stroke(driver, m + vec2(-150.0, 0.0), m + vec2(150.0, 0.0));
    let pixels = raster_bytes(driver, layer);
    press(driver, Key::Z, Modifiers::COMMAND);
    run(driver, "layer.delete");
    pixels
}

/// How many pixels hold some paint but are not fully covered.
fn partly(pixels: &[u8]) -> usize {
    pixels.chunks(4).filter(|p| p[3] > 10 && p[3] < 245).count()
}

/// How many pixels hold paint.
fn inked(pixels: &[u8]) -> usize {
    pixels.chunks(4).filter(|p| p[3] > 10).count()
}

/// FGV-27 (00:31:45, 00:31:58): Sub Tool Detail's Texture chooses a
/// paper and shows its scale ratio, rotation angle, brightness,
/// contrast, invert, emphasize density, texture mode and apply by each
/// plot; the chosen paper then breaks the stroke up.
#[test]
fn fgv27_texture_choice_with_its_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("texture-choice");
    run(&mut driver, "view.actual");
    let plain = pen_stroke(&mut driver);
    let window = detail(&mut driver);
    click_widget_in(&mut driver, "Texture", window);
    assert!(
        texts_in(&driver, window).iter().any(|t| t == "None"),
        "no paper yet: {:?}",
        texts_in(&driver, window)
    );
    // The paper's list: Sue's own papers.
    driver.click_text("None");
    driver.settle(4);
    let at = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Canvas")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("Canvas in the list: {:?}", driver.texts));
    click(&mut driver, at.center());
    driver.settle(4);
    let rows = texts_in(&driver, window);
    for row in [
        "Scale ratio",
        "Rotation angle",
        "Brightness",
        "Contrast",
        "Invert texture",
        "Emphasize density",
        "Texture mode",
        "Apply by each plot",
    ] {
        assert!(rows.iter().any(|t| t == row), "{row}: {rows:?}");
    }
    run(&mut driver, "window.sub_tool_detail");
    let textured = pen_stroke(&mut driver);
    assert!(
        partly(&textured) > partly(&plain) * 2 + 200,
        "the paper breaks the stroke up: {} against {}",
        partly(&textured),
        partly(&plain)
    );
}

/// FGV-28 (00:32:23) and 00:12:49: the watercolor edge on the brush,
/// with Range, Opacity, Darkness, Blurring width and Process after brush
/// stroke; on, the stroke's inside is see-through and its rim darker.
#[test]
fn fgv28_watercolor_edge_on_the_brush() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("watercolor-edge");
    run(&mut driver, "view.actual");
    let plain = pen_stroke(&mut driver);
    let window = detail(&mut driver);
    click_widget_in(&mut driver, "Watercolor edge", window);
    click_widget_in(&mut driver, "Watercolor edge", window);
    let rows = texts_in(&driver, window);
    for row in [
        "Range",
        "Opacity",
        "Darkness",
        "Blurring width",
        "Process after brush stroke",
    ] {
        assert!(rows.iter().any(|t| t == row), "{row}: {rows:?}");
    }
    run(&mut driver, "window.sub_tool_detail");
    let edged = pen_stroke(&mut driver);
    assert!(
        partly(&edged) > partly(&plain) * 2 + 200,
        "the inside is see-through: {} against {}",
        partly(&edged),
        partly(&plain)
    );
}

/// FGV-28 (00:32:06, 00:32:12): the dual brush, with its mode, its own
/// size (or linked to the main size), spacing, scatter and hardness;
/// on, a second tip breaks the stroke up.
#[test]
fn fgv28_dual_brush_on_the_brush() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("dual-brush");
    run(&mut driver, "view.actual");
    let plain = pen_stroke(&mut driver);
    let window = detail(&mut driver);
    click_widget_in(&mut driver, "Dual brush", window);
    click_widget_in(&mut driver, "Dual brush", window);
    let rows = texts_in(&driver, window);
    for row in [
        "Mode",
        "Link to main brush size",
        "2-Brush size",
        "2-Spacing",
        "2-Scatter",
        "2-Hardness",
    ] {
        assert!(rows.iter().any(|t| t == row), "{row}: {rows:?}");
    }
    run(&mut driver, "window.sub_tool_detail");
    let dual = pen_stroke(&mut driver);
    assert!(
        inked(&dual) > 0 && dual != plain,
        "the second tip changes the stroke"
    );
    assert!(
        partly(&dual) > partly(&plain) + 200,
        "broken up: {} against {}",
        partly(&dual),
        partly(&plain)
    );
}

// ----- Smooth arcs from sparse mouse input -----------------------------------

/// A mouse moved fast reports places far apart: a circle drawn with one
/// pointer event every 60 screen points comes out as an arc through
/// them, never as straight pieces with a corner at each event (a chord
/// between two of them lies 2.8 points inside the circle at its middle).
#[test]
fn sparse_mouse_strokes_draw_smooth_arcs() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("sparse-arcs");
    run(&mut driver, "view.actual");
    let layer = new_layer(&mut driver);
    pick(&mut driver, "pen", "G-pen");
    let m = middle(&driver);
    let radius = 160.0_f32;
    let step = 60.0 / radius;
    let count = (std::f32::consts::TAU * 0.75 / step) as usize;
    let points: Vec<Pos2> = (0..=count)
        .map(|i| {
            let angle = i as f32 * step;
            m + vec2(radius * angle.cos(), radius * angle.sin())
        })
        .collect();
    path(&mut driver, &points, 1);
    let [cx, cy] = under(&driver, m);
    let pixels = raster_bytes(&driver, layer);
    let width = i64::from(size(&driver)[0]);
    // The line's middle along each ray from the circle's middle, away
    // from the stroke's ends.
    let rays = 400;
    let mut rs = Vec::new();
    for k in 40..(rays * 3 / 4 - 40) {
        let angle = (k as f32 / rays as f32) * std::f32::consts::TAU;
        let (c, s) = (angle.cos(), angle.sin());
        let (mut sum, mut weight) = (0.0, 0.0);
        for r in (radius as i64 - 20)..(radius as i64 + 20) {
            let x = cx + (r as f32 * c).round() as i64;
            let y = cy + (r as f32 * s).round() as i64;
            let a = f32::from(pixels[((y * width + x) * 4 + 3) as usize]);
            sum += a * r as f32;
            weight += a;
        }
        assert!(weight > 0.0, "the line passes ray {k}");
        rs.push(sum / weight);
    }
    // How far the line wavers about its own mean over the span of one
    // event to the next: a polyline sags between events and comes back
    // at each (well over a point), an arc keeps its radius.
    let span = ((step / std::f32::consts::TAU) * rays as f32 / 2.0) as usize;
    let mut worst = 0.0_f32;
    for k in span..rs.len() - span {
        let mean = rs[k - span..=k + span].iter().sum::<f32>() / (2 * span + 1) as f32;
        worst = worst.max((rs[k] - mean).abs());
    }
    assert!(worst < 0.8, "the line keeps to an arc: wavers {worst}");
    let mean = rs.iter().sum::<f32>() / rs.len() as f32;
    assert!((mean - radius).abs() < 6.0, "on the circle: {mean}");
}
