//! Tutorial acceptance (P5-2): "Clip Studio Paint Crash Course For
//! BEGINNERS", "50 Quick Tips & Tricks You Need" and "Things I like about
//! Clip Studio Paint", followed step by step in the running Sue Paint.
//!
//! Each test is one row of the report `tutorial-crash-tips.md` and is
//! named after it: `cc_` and the time for the Crash Course, `t50_` and
//! the tip for the fifty tips, `tl_` and the time for the vector layers
//! video, and the report's own id (`cc1_`, `v4_`, `l2_`) for the rows
//! the picture added. Every step drives real frames: keys, clicks found
//! where a frame drew them and never by a fixed position, drags on the
//! canvas, and the menus' commands as the menus run them. The row table,
//! with the rows that are excluded or outside Sue, is
//! `plan/v0.0.8/acceptance/crash_tips.md` beside the plan.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton};
use sue_core::EntityId;

// ----- Driving ---------------------------------------------------------------

/// Sue Paint with a new canvas, its palettes as a person kept them in
/// 0.0.75 (the canvas the drawing helpers measure).
fn open(tag: &str) -> PaintDriver {
    PaintDriver::new(&format!("tut-ct-{tag}"))
}

/// Sue Paint with a new canvas and the palettes its layout style lays
/// out by itself (Tool Property among them), as a new person sees it.
fn open_fresh(tag: &str) -> PaintDriver {
    let mut driver = PaintDriver::fresh(&format!("tut-ct-{tag}"));
    driver.settle(4);
    driver
}

/// One frame with `events` while `held` are held; what it draws.
fn step_held(
    driver: &mut PaintDriver,
    events: Vec<Event>,
    held: Modifiers,
) -> Vec<(String, egui::Rect)> {
    let mut events = events;
    events.insert(0, Event::ModifiersChanged(held));
    driver.step(events)
}

/// A press at `from` with `held`, a drag through `points` and the
/// release at the last one; what the frames after draw.
fn path_held(
    driver: &mut PaintDriver,
    points: &[Pos2],
    button: PointerButton,
    held: Modifiers,
) -> Vec<(String, egui::Rect)> {
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: held,
    };
    let first = points[0];
    step_held(driver, vec![Event::PointerMoved(first)], held);
    step_held(driver, vec![press(first, true)], held);
    for pair in points.windows(2) {
        for index in 1..=6 {
            let at = pair[0] + (pair[1] - pair[0]) * (index as f32 / 6.0);
            step_held(driver, vec![Event::PointerMoved(at)], held);
        }
    }
    let last = *points.last().unwrap();
    step_held(driver, vec![press(last, false)], held);
    for _ in 0..4 {
        step_held(driver, Vec::new(), Modifiers::NONE);
    }
    driver.texts.clone()
}

/// A drag of the main button from `from` to `to`.
fn stroke(driver: &mut PaintDriver, from: Pos2, to: Pos2) {
    path_held(driver, &[from, to], PointerButton::Primary, Modifiers::NONE);
}

/// A click of `button` at `at` with `held`.
fn click_held(driver: &mut PaintDriver, at: Pos2, button: PointerButton, held: Modifiers) {
    path_held(driver, &[at], button, held);
}

/// A click of the main button at `at`.
fn click(driver: &mut PaintDriver, at: Pos2) {
    click_held(driver, at, PointerButton::Primary, Modifiers::NONE);
}

/// A key pressed and let go with `held`.
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

/// A key pressed and let go on its own.
fn tap(driver: &mut PaintDriver, key_name: Key) {
    press(driver, key_name, Modifiers::NONE);
}

/// A menu command, as its item runs it, and the frames after.
fn run(driver: &mut PaintDriver, command: &str) {
    driver.command(command);
    driver.settle(4);
}

/// One frame read through egui's accessibility tree: every widget with
/// a name and where it is.
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

/// Where the widget named `name` is: below the menu bar first, then the
/// topmost and leftmost.
fn widget(driver: &mut PaintDriver, name: &str) -> Option<egui::Rect> {
    let all: Vec<egui::Rect> = widgets(driver)
        .into_iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| rect)
        .collect();
    let best = |rects: &[egui::Rect]| {
        rects.iter().copied().min_by(|a, b| {
            a.top()
                .total_cmp(&b.top())
                .then(a.left().total_cmp(&b.left()))
        })
    };
    let below: Vec<egui::Rect> = all.iter().copied().filter(|r| r.top() > 30.0).collect();
    best(&below).or_else(|| best(&all))
}

/// A click on the widget named `name`.
fn click_widget(driver: &mut PaintDriver, name: &str) {
    let rect = widget(driver, name).unwrap_or_else(|| {
        let names: Vec<String> = widgets(driver).into_iter().map(|(l, _)| l).collect();
        panic!("{name:?} is on screen: {names:?}")
    });
    click(driver, rect.center());
}

/// Whether the last frame drew `text`.
fn shows(driver: &PaintDriver, text: &str) -> bool {
    driver.texts.iter().any(|(found, _)| found == text)
}

/// Selects what a text field holds and types `text` over it.
fn type_over(driver: &mut PaintDriver, text: &str) {
    driver.step(vec![key(Key::A, Modifiers::COMMAND)]);
    driver.step(vec![Event::Text(text.to_owned())]);
    driver.settle(2);
}

/// Gives command `command` the chord `chord`, as Settings, Keymaps does
/// once a person has pressed it there, taking it from any command that
/// held it before.
fn bind(driver: &mut PaintDriver, command: &str, chord: &str) {
    let section = "sue_paint_keys";
    let rice = &mut driver.runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(rice, section, command, &chord.parse().unwrap());
    rice.set(&setting, value).unwrap();
    driver.settle(2);
}

// ----- Reading the canvas ----------------------------------------------------

/// The open picture.
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

/// The picture's layers, bottom first.
fn layers(driver: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(arena(driver), picture(driver))
}

/// The layer named `name`.
fn named(driver: &PaintDriver, name: &str) -> EntityId {
    layers(driver)
        .into_iter()
        .find(|info| info.name == name)
        .unwrap_or_else(|| {
            let names: Vec<String> = layers(driver).into_iter().map(|i| i.name).collect();
            panic!("a layer named {name:?}: {names:?}")
        })
        .id
}

/// The top layer.
fn top(driver: &PaintDriver) -> EntityId {
    layers(driver).last().expect("a layer").id
}

/// The layer's style.
fn style(driver: &PaintDriver, layer: EntityId) -> sue_model::picture::LayerStyle {
    layers(driver)
        .into_iter()
        .find(|info| info.id == layer)
        .expect("the layer")
        .style
}

/// The picture's size.
fn size(driver: &PaintDriver) -> [u32; 2] {
    sue_model::picture::size(arena(driver).get_by_id(picture(driver)).unwrap()).unwrap()
}

/// Picture pixel `p` of `layer` (where the layer sits), straight RGBA.
fn pixel(driver: &PaintDriver, layer: EntityId, p: [i64; 2]) -> [u8; 4] {
    let [ox, oy] = style(driver, layer).offset;
    let Some(raster) = sue_paint::doc::raster(arena(driver), layer) else {
        return [0; 4];
    };
    let (x, y) = (p[0] - ox, p[1] - oy);
    if x < 0 || y < 0 || x >= i64::from(raster.width()) || y >= i64::from(raster.height()) {
        return [0; 4];
    }
    let at = ((y * i64::from(raster.width()) + x) * 4) as usize;
    let bytes = raster.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// How many pixels of `layer` have paint.
fn painted(driver: &PaintDriver, layer: EntityId) -> usize {
    sue_paint::doc::raster(arena(driver), layer).map_or(0, |raster| {
        raster
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    })
}

/// The picture as a person sees it, flattened.
fn seen(driver: &PaintDriver) -> sue_core::RasterTile2D {
    sue_paint::composite::flatten(
        arena(driver),
        picture(driver),
        sue_paint::composite::RenderOptions::default(),
    )
    .expect("the picture flattens")
}

/// Picture pixel `p` of the flattened picture.
fn seen_at(tile: &sue_core::RasterTile2D, p: [i64; 2]) -> [u8; 4] {
    let at = ((p[1] * i64::from(tile.width()) + p[0]) * 4) as usize;
    let bytes = tile.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// The canvas's middle on screen: the picture's middle while the view has
/// not moved.
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

/// The picture pixel under screen point `at`, while the view is neither
/// turned nor flipped nor scrolled.
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

/// The Pen tool's G-pen, picked in the Sub Tool palette (P alone goes
/// on to the pencil when the pen is already the tool).
fn pen(driver: &mut PaintDriver) {
    driver.settle(2);
    if !shows(driver, "G-pen") {
        run(driver, "tool.pen");
    }
    driver.click_text("G-pen");
}

/// The sub tool `sub` of the tool `key_name` picks, `marker` being a
/// sub tool only that tool lists: the key pressed until the Sub Tool
/// palette lists it (a key held by several tools goes round them, as in
/// Clip Studio Paint), then the sub tool clicked.
fn sub_tool(driver: &mut PaintDriver, key_name: Key, marker: &str, sub: &str) {
    driver.settle(2);
    for _ in 0..4 {
        if shows(driver, marker) {
            break;
        }
        tap(driver, key_name);
        driver.settle(2);
    }
    assert!(shows(driver, marker), "{marker}: {:?}", driver.texts);
    driver.click_text(sub);
}

/// A pen line across the canvas's middle on a new raster layer: the
/// layer.
fn ink_line(driver: &mut PaintDriver) -> EntityId {
    run(driver, "layer.new");
    pen(driver);
    let m = middle(driver);
    stroke(driver, m + vec2(-120.0, 0.0), m + vec2(120.0, 0.0));
    let layer = top(driver);
    assert!(painted(driver, layer) > 0, "the line is painted");
    layer
}

// ----- Crash Course: setting up -----------------------------------------------

/// CC 01:13: New, the 4K preset at 72 dpi as an illustration, OK.
#[test]
fn cc_0113_new_makes_a_4k_illustration_at_72_dpi() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("new-4k");
    press(&mut driver, Key::N, Modifiers::COMMAND);
    driver.settle(8);
    assert!(driver.rect_of("New").is_some(), "New opens");
    click_widget(&mut driver, "Illustration");
    // Sue keeps 4K among the canvas sizes beside Width (Clip Studio
    // Paint among its presets): the second "Custom" of the dialog.
    let sizes = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Custom")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the size list");
    click(&mut driver, sizes.center());
    driver.click_text("4K 3840 x 2160");
    driver.click_text("OK");
    driver.settle(10);
    assert_eq!(size(&driver), [3840, 2160]);
    let dpi = arena(&driver)
        .get_by_id(picture(&driver))
        .unwrap()
        .property(sue_model::picture::DPI)
        .and_then(sue_core::Value::as_f64);
    assert_eq!(dpi, Some(72.0));
}

/// CC 01:48: the Tool palette at the left holds Zoom; a click picks it.
#[test]
fn cc_0148_the_tool_palette_picks_zoom() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("tool-zoom");
    let zoom_button = widget(&mut driver, "Zoom (/)").expect("Zoom on the Tool palette");
    assert!(
        zoom_button.left() < 200.0,
        "at the left of the window: {zoom_button:?}"
    );
    click(&mut driver, zoom_button.center());
    driver.settle(4);
    assert!(shows(&driver, "Zoom In"), "{:?}", driver.texts);
}

/// CC 01:57: the Sub Tool palette lists the brushes of the Brush tool.
#[test]
fn cc_0157_the_sub_tool_palette_lists_the_brushes() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("sub-tool-brushes");
    run(&mut driver, "tool.pick.brush");
    let texts = driver.settle(4);
    for brush in ["Round watercolor brush", "Flat watercolor brush"] {
        assert!(
            texts.iter().any(|(text, _)| text == brush),
            "{brush}: {texts:?}"
        );
    }
}

/// CC 02:08: palettes come and go from the Window menu (Brush Size, the
/// color wheel, the Command Bar), and one can float.
#[test]
fn cc_0208_palettes_come_and_go_from_the_window_menu() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("palettes");
    assert!(driver.rect_of("Brush Size").is_some(), "{:?}", driver.texts);
    for (command, name) in [
        ("window.color_wheel", "Color Wheel"),
        ("window.navigator", "Navigator"),
    ] {
        let was = driver.rect_of(name).is_some();
        run(&mut driver, command);
        driver.settle(6);
        if was {
            assert!(driver.rect_of(name).is_none(), "{name} closed");
            run(&mut driver, command);
            driver.settle(6);
        }
        assert!(driver.rect_of(name).is_some(), "{name} is shown");
    }
}

/// CC 02:35: B picks the brushes; P the pen, and P again the pencil.
#[test]
fn cc_0235_b_picks_brushes_and_p_cycles_pen_and_pencil() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("keys-b-p");
    tap(&mut driver, Key::B);
    driver.settle(4);
    assert!(
        shows(&driver, "Round watercolor brush"),
        "{:?}",
        driver.texts
    );
    tap(&mut driver, Key::P);
    driver.settle(4);
    assert!(shows(&driver, "G-pen"), "{:?}", driver.texts);
    tap(&mut driver, Key::P);
    driver.settle(4);
    assert!(shows(&driver, "Mechanical pencil"), "{:?}", driver.texts);
    assert!(!shows(&driver, "G-pen"));
}

/// CC 02:43: Tool Property shows each tool's own settings.
#[test]
fn cc_0243_tool_property_follows_the_tool() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("tool-property");
    tap(&mut driver, Key::P);
    driver.settle(4);
    assert!(!shows(&driver, "Tolerance"), "{:?}", driver.texts);
    tap(&mut driver, Key::G);
    driver.settle(4);
    assert!(shows(&driver, "Tolerance"), "{:?}", driver.texts);
}

/// A `.sut` of one sub tool, as Clip Studio Paint and Sue write it.
fn a_sut(name: &str) -> Vec<u8> {
    let preset = sue_model::BrushPreset::new(name);
    sue_io::brushes::sut::export(&preset, &|_| None).unwrap()
}

/// CC 03:01: a `.sut` let go on Sue Paint lands at the end of the group
/// that is open, and is picked.
#[test]
fn cc_0301_a_sut_let_go_lands_in_the_open_group_and_is_picked() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("drop-sut");
    tap(&mut driver, Key::B);
    driver.settle(4);
    std::fs::create_dir_all(&driver.root).unwrap();
    let file = driver.root.join("Inking brush.sut");
    std::fs::write(&file, a_sut("Inking brush")).unwrap();
    driver
        .runtime
        .request(ShellRequest::OpenFile(file), &mut NoTextures, driver.now);
    driver.settle(8);
    let row = widget(&mut driver, "Inking brush").expect("the brush is a sub tool");
    let first = widget(&mut driver, "Round watercolor brush").expect("the open group");
    assert!(row.top() > first.top(), "after the group's brushes");
    let shown = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Inking brush")
        .count();
    assert!(shown >= 2, "listed and picked: {:?}", driver.texts);
}

/// CC 03:15: the trash button under the list deletes the picked sub
/// tool.
#[test]
fn cc_0315_the_trash_button_deletes_the_picked_sub_tool() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("sub-tool-delete");
    tap(&mut driver, Key::P);
    click_widget(&mut driver, "Duplicate Sub Tool");
    type_over(&mut driver, "Spare pen");
    driver.step(vec![key(Key::Enter, Modifiers::NONE)]);
    driver.settle(6);
    assert!(widget(&mut driver, "Spare pen").is_some(), "the copy");
    let trash = widget(&mut driver, "Delete Sub Tool").expect("the trash button");
    let list = widget(&mut driver, "Spare pen").expect("the list");
    assert!(trash.top() > list.top(), "under the list");
    click(&mut driver, trash.center());
    driver.settle(4);
    assert!(widget(&mut driver, "Spare pen").is_none(), "deleted");
}

/// A Photoshop brush file (`.abr` version 6) with one round tip.
fn an_abr() -> Vec<u8> {
    let side = 9_u32;
    let mut sample = Vec::new();
    // The id (a length and its bytes), two unknown words, the list's
    // version and length, its bounds.
    sample.push(4);
    sample.extend_from_slice(b"tip1");
    sample.extend_from_slice(&[0; 4 + 8 + 16]);
    // One channel, written.
    sample.extend_from_slice(&1_u32.to_be_bytes());
    sample.extend_from_slice(&1_u32.to_be_bytes());
    let pixels = (side * side) as usize;
    sample.extend_from_slice(&((23 + pixels) as u32).to_be_bytes());
    sample.extend_from_slice(&0_u32.to_be_bytes());
    for value in [0, 0, side, side] {
        sample.extend_from_slice(&value.to_be_bytes());
    }
    sample.extend_from_slice(&8_u16.to_be_bytes());
    sample.push(0);
    for y in 0..side {
        for x in 0..side {
            let (dx, dy) = (x as f32 - 4.0, y as f32 - 4.0);
            sample.push(if dx * dx + dy * dy <= 16.0 { 255 } else { 0 });
        }
    }
    while sample.len() % 4 != 0 {
        sample.push(0);
    }
    let mut samp = (sample.len() as u32).to_be_bytes().to_vec();
    samp.extend_from_slice(&sample);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&6_u16.to_be_bytes());
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(b"8BIMsamp");
    bytes.extend_from_slice(&(samp.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&samp);
    bytes
}

/// CC 03:25 (and tip 13): a Photoshop `.abr` let go the same way.
#[test]
fn cc_0325_an_abr_let_go_lands_in_the_open_group() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("drop-abr");
    tap(&mut driver, Key::B);
    driver.settle(4);
    std::fs::create_dir_all(&driver.root).unwrap();
    let file = driver.root.join("Soft round.abr");
    std::fs::write(&file, an_abr()).unwrap();
    driver
        .runtime
        .request(ShellRequest::OpenFile(file), &mut NoTextures, driver.now);
    driver.settle(8);
    assert!(
        widget(&mut driver, "Soft round").is_some(),
        "{:?}",
        driver.texts
    );
}

/// CC 03:41: pen pressure changes the line's width.
#[test]
fn cc_0341_pen_pressure_changes_the_width() {
    use sue_platform::stylus::{self, PenKind, PenSample, PenSource};
    let mut driver = open("pressure-width");
    run(&mut driver, "layer.new");
    pen(&mut driver);
    for _ in 0..6 {
        tap(&mut driver, Key::CloseBracket);
    }
    let layer = top(&driver);
    let line = |driver: &mut PaintDriver, y: f32, pressure: f32| {
        let start = middle(driver) + vec2(-100.0, y);
        let mut time = driver.ctx.input(|input| input.time) + 1.0;
        let sample = |kind, pos: Pos2, time, pressure| {
            let mut sample = PenSample::at(PenSource::WinTab, [pos.x, pos.y], time);
            sample.kind = kind;
            sample.pressure = pressure;
            sample
        };
        stylus::push_for_test(sample(PenKind::Down, start, time, Some(pressure)));
        driver.step(vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        let mut at = start;
        for _ in 0..25 {
            at += vec2(8.0, 0.0);
            time += 0.016;
            stylus::push_for_test(sample(PenKind::Move, at, time, Some(pressure)));
            driver.step(vec![Event::PointerMoved(at)]);
        }
        stylus::push_for_test(sample(PenKind::Up, at, time + 0.004, None));
        driver.step(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        driver.settle(3);
        stylus::forget_for_test();
    };
    line(&mut driver, -60.0, 0.25);
    let light = painted(&driver, layer);
    line(&mut driver, 60.0, 1.0);
    let heavy = painted(&driver, layer) - light;
    assert!(
        heavy as f32 > light as f32 * 1.5,
        "a heavy line is wider: {light} against {heavy}"
    );
}

/// CC 03:56: Ctrl+Z undoes the stroke.
#[test]
fn cc_0356_ctrl_z_undoes() {
    let mut driver = open("undo");
    let layer = ink_line(&mut driver);
    press(&mut driver, Key::Z, Modifiers::COMMAND);
    assert_eq!(painted(&driver, layer), 0, "the line is gone");
}

/// The brush size Tool Property shows, in px.
fn brush_size(driver: &PaintDriver) -> f32 {
    driver
        .texts
        .iter()
        .find_map(|(text, _)| text.strip_suffix("\npx")?.parse::<f32>().ok())
        .unwrap_or_else(|| panic!("a brush size: {:?}", driver.texts))
}

/// CC 04:00: [ and ] make the brush smaller and bigger.
#[test]
fn cc_0400_brackets_resize_the_brush() {
    let mut driver = open("brackets");
    tap(&mut driver, Key::P);
    let before = brush_size(&driver);
    tap(&mut driver, Key::CloseBracket);
    let bigger = brush_size(&driver);
    tap(&mut driver, Key::OpenBracket);
    tap(&mut driver, Key::OpenBracket);
    let smaller = brush_size(&driver);
    assert!(
        bigger > before && smaller < before,
        "{before} {bigger} {smaller}"
    );
}

/// CC 04:06: the zoom is shown under the canvas; Ctrl and plus zooms in,
/// Ctrl and minus out.
#[test]
fn cc_0406_ctrl_plus_and_minus_zoom_and_the_strip_shows_it() {
    let mut driver = open("zoom-keys");
    let before = zoom(&driver);
    press(&mut driver, Key::Plus, Modifiers::COMMAND);
    let bigger = zoom(&driver);
    assert!(bigger > before, "{before} to {bigger}");
    press(&mut driver, Key::Minus, Modifiers::COMMAND);
    press(&mut driver, Key::Minus, Modifiers::COMMAND);
    let smaller = zoom(&driver);
    assert!(smaller < before, "{before} to {smaller}");
    // The strip under the canvas shows the same number.
    let texts = driver.settle(2);
    let shown = format!("{:.1}", smaller * 100.0);
    assert!(
        texts.iter().any(|(text, _)| text.contains(&shown)),
        "{shown} in {texts:?}"
    );
}

/// CC 04:37: File, Pen Pressure Settings: a few lines drawn on the pad
/// with a light pen, Check adjusted settings fits the curve to them, and
/// OK keeps it for every tool.
#[test]
fn cc_0437_pen_pressure_settings_adjust_by_drawing() {
    use sue_platform::stylus::{self, PenKind, PenSample, PenSource};
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("pressure-dialog");
    let kept = |driver: &PaintDriver| {
        driver
            .runtime
            .engine
            .world
            .rice
            .get_str("sue_paint.pen_pressure")
            .map(str::to_owned)
    };
    let before = kept(&driver);
    run(&mut driver, "file.pressure");
    driver.settle(6);
    assert!(shows(&driver, "Adjust pen pressure"), "{:?}", driver.texts);
    assert!(shows(&driver, "Show pen pressure graph"));
    // The pad lies between the Check button and the graph's check box.
    let check = widget(&mut driver, "Check adjusted settings").expect("the Check button");
    let graph = widget(&mut driver, "Show pen pressure graph").expect("the graph box");
    let top = check.bottom() + 12.0;
    let bottom = graph.top() - 12.0;
    assert!(bottom > top + 20.0, "a pad: {check:?} {graph:?}");
    let mut time = driver.ctx.input(|input| input.time) + 1.0;
    let sample = |kind, pos: Pos2, time, pressure| {
        let mut sample = PenSample::at(PenSource::WinTab, [pos.x, pos.y], time);
        sample.kind = kind;
        sample.pressure = pressure;
        sample
    };
    for row in 0..3 {
        let y = top + (bottom - top) * (row as f32 + 1.0) / 4.0;
        let mut at = egui::pos2(check.left() + 20.0, y);
        stylus::push_for_test(sample(PenKind::Down, at, time, Some(0.3)));
        driver.step(vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        for index in 0..14 {
            at += vec2(12.0, 0.0);
            time += 0.016;
            // A light hand: between a fifth and two fifths of the way.
            let pressure = 0.2 + 0.2 * (index as f32 / 13.0);
            stylus::push_for_test(sample(PenKind::Move, at, time, Some(pressure)));
            driver.step(vec![Event::PointerMoved(at)]);
        }
        stylus::push_for_test(sample(PenKind::Up, at, time + 0.004, None));
        driver.step(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        driver.settle(2);
        time += 0.2;
    }
    stylus::forget_for_test();
    click_widget(&mut driver, "Check adjusted settings");
    assert!(
        shows(&driver, "Stronger") && shows(&driver, "Lighter"),
        "the curve is fitted and can be tried: {:?}",
        driver.texts
    );
    click_widget(&mut driver, "OK");
    driver.settle(4);
    let after = kept(&driver);
    assert!(after.is_some(), "the fitted curve is kept");
    assert_ne!(after, before, "a light hand gets a new curve");
}

// ----- Crash Course: the sketch -------------------------------------------------

/// Opens Settings at the page `id`.
fn settings(driver: &mut PaintDriver, id: &str) {
    driver.runtime.request(
        ShellRequest::OpenSettings(Some(id.to_owned())),
        &mut NoTextures,
        driver.now,
    );
    driver.settle(8);
}

/// Types `text` into the empty Settings filter.
fn filter_settings(driver: &mut PaintDriver, text: &str) {
    let hint = sue_lang::text("settings", "filter-settings-ellipsis");
    driver.click_text(&hint);
    driver.step(vec![Event::Text(text.to_owned())]);
    driver.settle(6);
}

/// Settings, Keymaps, as a person gives `command` (found by its `name`,
/// holding `was`) the key `key_name`, taking it from the command that
/// held it.
fn rebind_in_settings(driver: &mut PaintDriver, name: &str, was: &str, key_name: Key) {
    settings(driver, "keymaps");
    filter_settings(driver, name);
    assert!(shows(driver, name), "{name}: {:?}", driver.texts);
    // The command's row, under the filter that holds the same words.
    let row = driver
        .texts
        .iter()
        .filter(|(text, _)| text == name)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    let chord = driver
        .texts
        .iter()
        .filter(|(text, rect)| text == was && (rect.center().y - row.center().y).abs() < 12.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("{was} on the row of {name}: {:?}", driver.texts));
    click(driver, chord.center());
    driver.step(vec![key(key_name, Modifiers::NONE)]);
    driver.settle(6);
    let take = driver
        .texts
        .iter()
        .find(|(text, _)| text.starts_with("Take ") && text.contains(" from "))
        .map(|(text, _)| text.clone());
    if let Some(take) = take {
        driver.click_text(&take);
        driver.settle(4);
    }
    driver.runtime.shell.close_window();
    driver.settle(6);
}

/// CC 06:00: the author gives X to the transparent color; a stroke with
/// it erases, and X again goes back to the color.
#[test]
fn cc_0600_x_given_to_the_transparent_color_erases_and_goes_back() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("transparent-x");
    let layer = ink_line(&mut driver);
    rebind_in_settings(&mut driver, "Drawing or Transparent Color", "C", Key::X);
    let line = painted(&driver, layer);
    tap(&mut driver, Key::X);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-40.0, 0.0), m + vec2(40.0, 0.0));
    let erased = painted(&driver, layer);
    assert!(
        erased < line,
        "the transparent color erased: {line} to {erased}"
    );
    tap(&mut driver, Key::X);
    stroke(&mut driver, m + vec2(-40.0, 30.0), m + vec2(40.0, 30.0));
    assert!(painted(&driver, layer) > erased, "the color paints again");
}

/// A loop round the left half of the line `ink_line` draws, with the
/// Lasso.
fn lasso_left(driver: &mut PaintDriver) {
    tap(driver, Key::M);
    click_widget(driver, "Lasso");
    let m = middle(driver);
    path_held(
        driver,
        &[
            m + vec2(-150.0, -40.0),
            m + vec2(-10.0, -40.0),
            m + vec2(-10.0, 40.0),
            m + vec2(-150.0, 40.0),
            m + vec2(-150.0, -40.0),
        ],
        PointerButton::Primary,
        Modifiers::NONE,
    );
}

/// How many pixels of `layer` have paint left and right of the picture's
/// middle.
fn painted_sides(driver: &PaintDriver, layer: EntityId) -> (usize, usize) {
    let [w, _] = size(driver);
    let half = i64::from(w) / 2;
    let [ox, _] = style(driver, layer).offset;
    let Some(raster) = sue_paint::doc::raster(arena(driver), layer) else {
        return (0, 0);
    };
    let width = raster.width() as usize;
    let mut sides = (0, 0);
    for (index, pixel) in raster
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        if pixel[3] == 0 {
            continue;
        }
        if (index % width) as i64 + ox < half {
            sides.0 += 1;
        } else {
            sides.1 += 1;
        }
    }
    sides
}

/// Whether `layer` has paint in the rows from `y0` to `y1` of the
/// picture's left half.
fn left_ink_between(driver: &PaintDriver, layer: EntityId, y0: i64, y1: i64) -> bool {
    let half = i64::from(size(driver)[0]) / 2;
    (y0..y1).any(|y| (0..half).any(|x| pixel(driver, layer, [x, y])[3] > 0))
}

/// CC 06:26 and 06:39: the Lasso circles the left half, Delete clears
/// inside it and nothing else.
#[test]
fn cc_0626_lasso_then_delete_clears_inside_the_selection() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lasso-delete");
    let layer = ink_line(&mut driver);
    let (left, right) = painted_sides(&driver, layer);
    assert!(left > 0 && right > 0);
    lasso_left(&mut driver);
    let before = bytes(&driver);
    tap(&mut driver, Key::Delete);
    let (_, right_after) = painted_sides(&driver, layer);
    let [w, h] = size(&driver);
    let inside = (0..i64::from(w) / 2 - 40)
        .filter(|x| (0..i64::from(h)).any(|y| pixel(&driver, layer, [*x, y])[3] > 0))
        .count();
    assert_eq!(inside, 0, "inside the lasso is cleared");
    assert_eq!(right_after, right, "the right half stays");
    undo_pair(&mut driver, &before);
}

/// CC 06:43: Ctrl+T on the lassoed part, a drag inside its box, Enter.
#[test]
fn cc_0643_ctrl_t_moves_the_selected_part_and_enter_commits() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("transform");
    let layer = ink_line(&mut driver);
    let (_, right) = painted_sides(&driver, layer);
    lasso_left(&mut driver);
    press(&mut driver, Key::T, Modifiers::COMMAND);
    driver.settle(4);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(-80.0, 60.0));
    tap(&mut driver, Key::Enter);
    driver.settle(4);
    let line_y = i64::from(size(&driver)[1]) / 2;
    assert!(
        left_ink_between(&driver, layer, line_y + 20, line_y + 200),
        "the left part went down"
    );
    assert_eq!(painted_sides(&driver, layer).1, right, "the rest stays");
}

/// CC 07:20 (and 19:33): the Liquify tool pushes the drawing along.
#[test]
fn cc_0720_liquify_pushes_the_drawing() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("liquify");
    let layer = ink_line(&mut driver);
    click_widget(&mut driver, "Liquify (J)");
    driver.settle(4);
    assert!(shows(&driver, "Liquify"), "{:?}", driver.texts);
    let before = sue_paint::doc::raster(arena(&driver), layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(0.0, -10.0), m + vec2(0.0, 60.0));
    let after = sue_paint::doc::raster(arena(&driver), layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    assert!(after != before, "the line was pushed");
    let [w, h] = size(&driver);
    let line_y = i64::from(h) / 2;
    let pushed =
        (line_y + 6..line_y + 120).any(|y| pixel(&driver, layer, [i64::from(w) / 2, y])[3] > 0);
    assert!(pushed, "ink below the line's middle");
}

/// CC 07:56 (and tip 22): the author gives Flip Horizontal of the view
/// F3; F3 then mirrors the view, so a stroke at the right lands at the
/// picture's left.
#[test]
fn cc_0756_f3_given_to_flip_horizontal_mirrors_the_view() {
    let mut driver = open("flip-f3");
    run(&mut driver, "layer.new");
    // F3 is Copy's second key: the author takes it for the flip.
    bind(&mut driver, "edit.copy", "Ctrl+C");
    bind(&mut driver, "view.flip_h", "F3");
    tap(&mut driver, Key::F3);
    pen(&mut driver);
    let layer = top(&driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(60.0, 0.0), m + vec2(120.0, 0.0));
    let (left, right) = painted_sides(&driver, layer);
    assert!(left > 0 && right == 0, "mirrored: {left} {right}");
    // F3 again turns it back.
    tap(&mut driver, Key::F3);
    stroke(&mut driver, m + vec2(60.0, 30.0), m + vec2(120.0, 30.0));
    assert!(painted_sides(&driver, layer).1 > 0, "back as it was");
}

/// CC 08:19: Ctrl+S asks where, and writes the file under the name typed.
#[test]
fn cc_0819_ctrl_s_saves_under_the_name_typed() {
    let mut driver = open("save");
    ink_line(&mut driver);
    press(&mut driver, Key::S, Modifiers::COMMAND);
    assert!(driver.runtime.pending_pick().is_some(), "Save asks where");
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("tutorial dragon.sue");
    let now = driver.now;
    assert!(driver.runtime.answer_pick(&path, &mut NoTextures, now));
    driver.settle(10);
    driver.finish_exports();
    assert!(path.exists(), "saved");
}

/// CC 08:40 (CC-10): Save As offers Photoshop Document; the `.psd` it
/// writes opens again with its layers.
#[test]
fn cc_0840_save_as_writes_a_psd_that_opens_again() {
    let mut driver = open("save-psd");
    ink_line(&mut driver);
    // A test run opens no system dialog: the Save As dialog's answer, a
    // name ending in .psd (its "Photoshop Document" type), comes as the
    // dialog gives it.
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("dragon.psd");
    let paint = AppletId::new(sue_paint::ID).unwrap();
    driver.runtime.request(
        ShellRequest::Picked {
            pick: sue_wm::FilePick::save(Some(paint), "save-as", "Save As", "dragon.psd"),
            path: path.clone(),
        },
        &mut NoTextures,
        driver.now,
    );
    driver.finish_exports();
    driver.settle(10);
    let psd = sue_io::psd::decode_psd(&std::fs::read(&path).expect("the psd is written")).unwrap();
    assert!(!psd.layers.is_empty(), "its layers are kept");
}

/// CC 09:07 and 09:09: V (the author's key) picks Move layer, which
/// moves only the lassoed part.
#[test]
fn cc_0907_v_given_to_move_layer_moves_the_lassoed_part() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("move-v");
    let layer = ink_line(&mut driver);
    let (_, right) = painted_sides(&driver, layer);
    bind(&mut driver, "tool.move_layer", "V");
    lasso_left(&mut driver);
    tap(&mut driver, Key::V);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(-80.0, 60.0));
    assert_eq!(
        painted_sides(&driver, layer).1,
        right,
        "the right half stays"
    );
    let line_y = i64::from(size(&driver)[1]) / 2;
    assert!(
        left_ink_between(&driver, layer, line_y + 20, line_y + 200),
        "the lassoed part went down"
    );
}

/// CC 09:25: the eye hides the layer and shows it again.
#[test]
fn cc_0925_the_eye_hides_and_shows_a_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("eye");
    let layer = ink_line(&mut driver);
    click_widget(&mut driver, "Hide Layer");
    assert!(!style(&driver, layer).visible, "hidden");
    click_widget(&mut driver, "Show Layer");
    assert!(style(&driver, layer).visible, "shown");
}

/// CC 09:53: New Raster Layer, and the ink on it leaves the sketch alone.
#[test]
fn cc_0953_new_raster_layer_keeps_the_sketch_apart() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("new-raster");
    let sketch = ink_line(&mut driver);
    let kept = sue_paint::doc::raster(arena(&driver), sketch)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    click_widget(&mut driver, "Raster Layer");
    let ink = top(&driver);
    assert_ne!(ink, sketch, "a new layer on top");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, 40.0), m + vec2(60.0, 40.0));
    assert!(painted(&driver, ink) > 0);
    assert!(
        sue_paint::doc::raster(arena(&driver), sketch)
            .unwrap()
            .bytes()
            .unwrap()
            == kept.as_slice(),
        "the sketch is untouched"
    );
}

/// The Layer palette's opacity wedge: the "Opacity" control beside the
/// blending mode.
fn opacity_wedge(driver: &mut PaintDriver) -> egui::Rect {
    let all = widgets(driver);
    let blend = all
        .iter()
        .find(|(name, _)| name == "Blending mode")
        .map(|(_, rect)| *rect)
        .expect("the blending mode");
    all.iter()
        .filter(|(name, rect)| {
            name == "Opacity" && (rect.center().y - blend.center().y).abs() < 10.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the opacity beside it")
}

/// CC 10:33 (and 21:28): the sketch layer's opacity is lowered.
#[test]
fn cc_1033_the_sketch_opacity_is_lowered() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("opacity");
    let layer = ink_line(&mut driver);
    let wedge = opacity_wedge(&mut driver);
    let at = egui::pos2(wedge.left() + wedge.width() * 0.4, wedge.center().y);
    click(&mut driver, at);
    let opacity = style(&driver, layer).opacity;
    assert!(opacity < 0.7 && opacity > 0.1, "{opacity}");
}

/// The middle pixel of the picture as a person sees it.
fn seen_middle(driver: &PaintDriver) -> [u8; 4] {
    let [w, h] = size(driver);
    seen_at(&seen(driver), [i64::from(w) / 2, i64::from(h) / 2])
}

/// CC 10:46: Ctrl+B (the author's key) switches the layer color on, the
/// row shows it, and it switches off again.
#[test]
fn cc_1046_ctrl_b_switches_the_layer_color() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("layer-color");
    ink_line(&mut driver);
    bind(&mut driver, "layer.toggle.layer_color", "Ctrl+B");
    let plain = seen_middle(&driver);
    let before = bytes(&driver);
    press(&mut driver, Key::B, Modifiers::COMMAND);
    let p = seen_middle(&driver);
    assert!(
        i32::from(p[2]) > i32::from(p[0]) + 40,
        "the line shows blue: {p:?}"
    );
    assert!(
        widgets(&mut driver)
            .iter()
            .any(|(name, _)| name == "Layer color is on"),
        "the row carries the mark"
    );
    undo_pair(&mut driver, &before);
    press(&mut driver, Key::B, Modifiers::COMMAND);
    assert_eq!(seen_middle(&driver), plain, "as it was again");
}

/// CC 11:16: a G-pen inks the line art.
#[test]
fn cc_1116_the_g_pen_inks() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("g-pen");
    run(&mut driver, "layer.new");
    pen(&mut driver);
    let layer = top(&driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-50.0, 0.0), m + vec2(50.0, 20.0));
    assert!(painted(&driver, layer) > 50);
}

/// The view's turn, as the strip under the canvas shows it: the number
/// before its degree sign.
fn turn(driver: &PaintDriver) -> f32 {
    let degree = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "°")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the degree sign");
    driver
        .texts
        .iter()
        .filter(|(_, rect)| (rect.center().y - degree.center().y).abs() < 2.0)
        .filter(|(_, rect)| rect.right() <= degree.left() + 1.0)
        .max_by(|a, b| a.1.right().total_cmp(&b.1.right()))
        .and_then(|(text, _)| text.trim().parse::<f32>().ok())
        .expect("the turn")
}

/// CC 12:02: R picks Rotate; a drag turns the view.
#[test]
fn cc_1202_r_rotates_the_view() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("rotate");
    tap(&mut driver, Key::R);
    assert!(shows(&driver, "Rotate"), "{:?}", driver.texts);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(150.0, 0.0), m + vec2(140.0, 50.0));
    driver.settle(2);
    assert!(turn(&driver).abs() > 1.0, "turned: {}", turn(&driver));
}

/// CC 12:11: Shift held while rotating snaps in 45 degree steps.
#[test]
fn cc_1211_shift_snaps_the_rotation_to_45_degrees() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("rotate-snap");
    tap(&mut driver, Key::R);
    driver.settle(4);
    let m = middle(&driver);
    path_held(
        &mut driver,
        &[m + vec2(150.0, 0.0), m + vec2(110.0, 110.0)],
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    driver.settle(2);
    let snapped = turn(&driver);
    assert!(snapped.abs() > 1.0, "turned: {snapped}");
    assert!(
        (snapped / 45.0 - (snapped / 45.0).round()).abs() < 0.01,
        "a step of 45: {snapped}"
    );
}

/// CC-1 (12:02 to 12:21): Rotate's Tool Property has a Snap angle check
/// box; ticked, a plain drag turns in its steps.
#[test]
fn cc1_rotate_snap_angle_turns_in_steps() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("rotate-snap-angle");
    tap(&mut driver, Key::R);
    driver.settle(4);
    click_widget(&mut driver, "Snap angle");
    assert!(shows(&driver, "Step of angle"), "{:?}", driver.texts);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(150.0, 0.0), m + vec2(140.0, 50.0));
    driver.settle(2);
    let snapped = turn(&driver);
    assert!(
        (snapped / 45.0 - (snapped / 45.0).round()).abs() < 0.01,
        "a step of 45 without Shift: {snapped}"
    );
    // Unticked, the drag turns freely again.
    click_widget(&mut driver, "Snap angle");
    stroke(&mut driver, m + vec2(150.0, 0.0), m + vec2(140.0, 50.0));
    driver.settle(2);
    let free = turn(&driver);
    assert!(
        (free / 45.0 - (free / 45.0).round()).abs() > 0.01,
        "free: {free}"
    );
}

// ----- Crash Course: flats, masks and folders ---------------------------------

/// A closed box of pen lines on a new raster layer (the dragon's line
/// art), its corners `half` screen points from the canvas's middle:
/// the layer.
fn line_box(driver: &mut PaintDriver, half: egui::Vec2) -> EntityId {
    run(driver, "layer.new");
    pen(driver);
    let m = middle(driver);
    let corners = [
        m + vec2(-half.x, -half.y),
        m + vec2(half.x, -half.y),
        m + vec2(half.x, half.y),
        m + vec2(-half.x, half.y),
        m + vec2(-half.x, -half.y - 4.0),
    ];
    path_held(driver, &corners, PointerButton::Primary, Modifiers::NONE);
    top(driver)
}

/// Inside, outside and on the box `line_box` draws with `half` 100 by
/// 80: picture points.
fn box_points(driver: &PaintDriver) -> ([i64; 2], [i64; 2]) {
    let m = middle(driver);
    (under(driver, m), under(driver, m + vec2(200.0, 150.0)))
}

/// The tutorial's state at 14:39: line art in a box, a folder below it
/// masked to the inside of the box, a flat layer inside the folder
/// filled with the Fill tool. Returns (line art, folder, flat layer).
fn dragon(driver: &mut PaintDriver) -> (EntityId, EntityId, EntityId) {
    let lines = line_box(driver, vec2(100.0, 80.0));
    let m = middle(driver);
    // 13:58: Auto Select, Refer only editing layer, outside the lines.
    sub_tool(
        driver,
        Key::W,
        "Refer Reference Layers",
        "Refer Only Editing Layer",
    );
    click(driver, m + vec2(200.0, 150.0));
    // 14:16: Invert.
    run(driver, "select.invert");
    // 14:29: below the line art, a folder.
    driver.click_text("Layer 1");
    click_widget(driver, "New Layer Folder");
    let folder = layers(driver)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(arena(driver), info.id))
        .expect("a folder")
        .id;
    // 14:34: a mask on the folder from the selection.
    run(driver, "layer.mask.outside");
    assert!(
        sue_paint::doc::mask_of(arena(driver), folder).is_some(),
        "the folder has a mask"
    );
    run(driver, "select.none");
    // 14:37: a new raster layer lands inside the folder.
    let before: Vec<EntityId> = layers(driver).into_iter().map(|info| info.id).collect();
    click_widget(driver, "Raster Layer");
    let flat = layers(driver)
        .into_iter()
        .map(|info| info.id)
        .find(|id| !before.contains(id))
        .expect("a new layer");
    // 14:39: Fill, Refer only editing layer, one click fills the layer.
    sub_tool(driver, Key::G, "Close and fill", "Refer Only Editing Layer");
    click(driver, m + vec2(200.0, 150.0));
    (lines, folder, flat)
}

/// CC 13:44 (and tip 47): Lasso fill fills the loop drawn round.
#[test]
fn cc_1344_lasso_fill_fills_the_loop() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lasso-fill");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    sub_tool(&mut driver, Key::G, "Close and fill", "Lasso fill");
    let m = middle(&driver);
    path_held(
        &mut driver,
        &[
            m + vec2(-60.0, -40.0),
            m + vec2(60.0, -40.0),
            m + vec2(60.0, 40.0),
            m + vec2(-60.0, 40.0),
        ],
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(pixel(&driver, layer, under(&driver, m))[3] > 0, "filled");
    assert_eq!(
        pixel(&driver, layer, under(&driver, m + vec2(120.0, 0.0)))[3],
        0,
        "only the loop"
    );
}

/// CC 13:58, 14:16, 14:29, 14:34, 14:37, 14:39 (CT-1, CT-4, CCS-8): W
/// selects the outside of the line art, Invert, a folder below the
/// lines, a mask on it from the selection, a new layer lands inside it,
/// and one click of Fill fills that layer; the picture shows the color
/// inside the lines only.
#[test]
fn cc_1358_auto_select_invert_folder_mask_and_fill() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("dragon");
    let (_, folder, flat) = dragon(&mut driver);
    assert_eq!(
        sue_paint::doc::folder_of(arena(&driver), flat),
        Some(folder),
        "the new layer is inside the folder"
    );
    let [w, h] = size(&driver);
    assert_eq!(
        painted(&driver, flat),
        (w * h) as usize,
        "the whole layer is filled"
    );
    let (inside, outside) = box_points(&driver);
    let tile = seen(&driver);
    let ink = seen_at(&tile, inside);
    let paper = seen_at(&tile, outside);
    assert!(ink[0] < 60, "the flat shows inside: {ink:?}");
    assert!(paper[0] > 200, "the mask hides it outside: {paper:?}");
}

/// The grays of `layer`'s mask, one byte a pixel, and its width.
fn mask_grays(driver: &PaintDriver, layer: EntityId) -> (Vec<u8>, i64) {
    let mask = sue_paint::doc::mask_of(arena(driver), layer).expect("a mask");
    let raster = sue_paint::doc::raster(arena(driver), mask).expect("the mask's pixels");
    let bytes = raster.bytes().unwrap();
    let count = (raster.width() * raster.height()) as usize;
    let step = bytes.len() / count.max(1);
    let grays = (0..count).map(|index| bytes[index * step]).collect();
    (grays, i64::from(raster.width()))
}

/// The gray of `layer`'s mask at picture point `p`.
fn mask_at(driver: &PaintDriver, layer: EntityId, p: [i64; 2]) -> u8 {
    let [ox, oy] = sue_paint::doc::mask_offset(arena(driver), layer).unwrap_or([0, 0]);
    let (grays, width) = mask_grays(driver, layer);
    let height = grays.len() as i64 / width;
    let (x, y) = (p[0] - ox, p[1] - oy);
    if x < 0 || y < 0 || x >= width || y >= height {
        return 0;
    }
    grays[(y * width + x) as usize]
}

/// Clicks the mask thumbnail on the row of the layer `name`: what is
/// drawn now goes on the mask.
fn edit_mask(driver: &mut PaintDriver, name: &str) {
    driver.click_text(name);
    let row = driver.rect_of(name).unwrap();
    let thumb = widgets(driver)
        .into_iter()
        .filter(|(label, rect)| {
            label == "Draw on the mask: the pen shows, the eraser hides"
                && (rect.center().y - row.center().y).abs() < 20.0
        })
        .map(|(_, rect)| rect)
        .next()
        .expect("the mask thumbnail on the row");
    click(driver, thumb.center());
}

/// CC 14:55: the mask thumbnail clicked, a hard brush with the
/// transparent color hides, with a color shows.
#[test]
fn cc_1455_painting_on_the_mask_hides_and_shows() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mask-paint");
    let (_, folder, _) = dragon(&mut driver);
    let folder_name = arena(&driver).get_by_id(folder).unwrap().name().to_owned();
    edit_mask(&mut driver, &folder_name);
    pen(&mut driver);
    for _ in 0..8 {
        tap(&mut driver, Key::CloseBracket);
    }
    let m = middle(&driver);
    let (inside, _) = box_points(&driver);
    let shown = mask_at(&driver, folder, inside);
    tap(&mut driver, Key::C);
    stroke(&mut driver, m + vec2(-30.0, 0.0), m + vec2(30.0, 0.0));
    let hidden = mask_at(&driver, folder, inside);
    assert!(
        hidden < shown,
        "the transparent color hides: {shown} to {hidden}"
    );
    assert!(
        seen_at(&seen(&driver), inside)[0] > 200,
        "the paper shows there"
    );
    tap(&mut driver, Key::C);
    let out = under(&driver, m + vec2(160.0, 0.0));
    stroke(&mut driver, m + vec2(150.0, 0.0), m + vec2(170.0, 0.0));
    assert!(mask_at(&driver, folder, out) > 0, "a color shows outside");
    assert!(seen_at(&seen(&driver), out)[0] < 60, "the flat shows there");
}

/// CC 14:55, the other side (CT-1): while a folder's mask is edited,
/// the tools that lay pixels in the mask take it, and the ones that work
/// on a layer's own pixels (a figure, a filter) still say why they
/// cannot, instead of doing nothing.
#[test]
fn cc_1455_tools_that_skip_masks_still_refuse_a_folder() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mask-refuse");
    let (_, folder, _) = dragon(&mut driver);
    let name = arena(&driver).get_by_id(folder).unwrap().name().to_owned();
    edit_mask(&mut driver, &name);
    let refusal = format!("{name} is a folder: pick a layer in it to draw on");
    let message = |driver: &PaintDriver| {
        driver
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned())
            .unwrap_or_default()
    };
    let before = bytes(&driver);
    sub_tool(&mut driver, Key::U, "Straight Line", "Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(150.0, 100.0), m + vec2(250.0, 180.0));
    assert_eq!(message(&driver), refusal, "the figure says why");
    assert!(bytes(&driver) == before, "and changes nothing");
    driver.runtime.shell.dismiss_overlays();
    run(&mut driver, "filter.grayscale");
    assert_eq!(message(&driver), refusal, "the filter says why");
    assert!(bytes(&driver) == before, "and changes nothing");
    // The pen still draws in the mask.
    pen(&mut driver);
    tap(&mut driver, Key::C);
    let (inside, _) = box_points(&driver);
    let shown = mask_at(&driver, folder, inside);
    stroke(&mut driver, m + vec2(-30.0, 0.0), m + vec2(30.0, 0.0));
    assert!(mask_at(&driver, folder, inside) < shown, "the pen hides");
}

/// CC 15:16 (CT-2): the Fill tool on the mask fills an area of it, so
/// the flat shows there.
#[test]
fn cc_1516_fill_on_the_mask_shows_an_area() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mask-fill");
    let (_, folder, _) = dragon(&mut driver);
    let name = arena(&driver).get_by_id(folder).unwrap().name().to_owned();
    edit_mask(&mut driver, &name);
    let (_, outside) = box_points(&driver);
    assert_eq!(mask_at(&driver, folder, outside), 0);
    sub_tool(
        &mut driver,
        Key::G,
        "Close and fill",
        "Refer Only Editing Layer",
    );
    let before = bytes(&driver);
    let at = middle(&driver) + vec2(200.0, 150.0);
    click(&mut driver, at);
    assert!(
        mask_at(&driver, folder, outside) > 200,
        "the mask is filled"
    );
    assert!(seen_at(&seen(&driver), outside)[0] < 60, "the flat shows");
    undo_pair(&mut driver, &before);
}

/// CC 16:00 (CT-3, CC-5): the Blend tool on the mask softens its edge.
#[test]
fn cc_1600_the_blend_tool_softens_the_mask_edge() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mask-blend");
    let (_, folder, _) = dragon(&mut driver);
    let name = arena(&driver).get_by_id(folder).unwrap().name().to_owned();
    edit_mask(&mut driver, &name);
    let grays = |driver: &PaintDriver| {
        mask_grays(driver, folder)
            .0
            .iter()
            .filter(|gray| **gray > 10 && **gray < 245)
            .count()
    };
    let hard = grays(&driver);
    tap(&mut driver, Key::J);
    driver.click_text("Blur");
    for _ in 0..10 {
        tap(&mut driver, Key::CloseBracket);
    }
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-100.0, -60.0), m + vec2(-100.0, 60.0));
    let soft = grays(&driver);
    assert!(soft > hard + 50, "the edge has grays: {hard} to {soft}");
}

/// CC 16:16: a textured brush on the mask, for a painterly edge.
#[test]
fn cc_1616_a_textured_brush_paints_the_mask() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mask-texture");
    let (_, folder, _) = dragon(&mut driver);
    let name = arena(&driver).get_by_id(folder).unwrap().name().to_owned();
    edit_mask(&mut driver, &name);
    run(&mut driver, "tool.pick.brush");
    driver.click_text("India ink");
    driver.click_text("Dry ink");
    let mask = sue_paint::doc::mask_of(arena(&driver), folder).unwrap();
    let before = sue_paint::doc::raster(arena(&driver), mask)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(140.0, -40.0), m + vec2(160.0, 40.0));
    let after = sue_paint::doc::raster(arena(&driver), mask)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    assert!(after != before, "the mask took the brush");
}

/// CC 16:34: a blank mask on the line art (nothing selected), and the
/// eraser on it hides part of the lines without erasing them.
#[test]
fn cc_1634_a_blank_mask_on_the_lines_hides_without_erasing() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lines-mask");
    let lines = line_box(&mut driver, vec2(100.0, 80.0));
    let kept = sue_paint::doc::raster(arena(&driver), lines)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    run(&mut driver, "layer.mask.outside");
    let m = middle(&driver);
    let corner = under(&driver, m + vec2(-100.0, -80.0));
    assert!(mask_at(&driver, lines, corner) > 200, "the mask shows all");
    // A new mask is what the tools draw on next (full guide, 00:08:51).
    tap(&mut driver, Key::E);
    for _ in 0..8 {
        tap(&mut driver, Key::CloseBracket);
    }
    stroke(&mut driver, m + vec2(-110.0, -80.0), m + vec2(-60.0, -80.0));
    assert!(mask_at(&driver, lines, corner) < 50, "hidden on the mask");
    assert!(
        sue_paint::doc::raster(arena(&driver), lines)
            .unwrap()
            .bytes()
            .unwrap()
            == kept.as_slice(),
        "the lines themselves are kept"
    );
}

/// CC 17:20: back on the flat layer, another color and a click with
/// Fill changes the flat.
#[test]
fn cc_1720_fill_again_changes_the_flat() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("refill");
    let (_, _, flat) = dragon(&mut driver);
    let (inside, _) = box_points(&driver);
    let first = pixel(&driver, flat, inside);
    // The sub color (white) becomes the drawing color.
    tap(&mut driver, Key::X);
    let at = middle(&driver);
    click(&mut driver, at);
    let second = pixel(&driver, flat, inside);
    assert_ne!(first, second, "the flat took the other color");
    assert!(second[0] > 200, "{second:?}");
}

/// CC 18:01: more layers inside the folder, painted inside a selection.
#[test]
fn cc_1801_more_layers_in_the_folder_paint_inside_a_selection() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("horns");
    let (_, folder, _) = dragon(&mut driver);
    click_widget(&mut driver, "Raster Layer");
    let horns = top_new(&driver, folder);
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-40.0, -40.0), m + vec2(0.0, 0.0));
    pen(&mut driver);
    for _ in 0..8 {
        tap(&mut driver, Key::CloseBracket);
    }
    stroke(&mut driver, m + vec2(-80.0, -20.0), m + vec2(40.0, -20.0));
    assert!(painted(&driver, horns) > 0, "painted");
    assert_eq!(
        pixel(&driver, horns, under(&driver, m + vec2(30.0, -20.0)))[3],
        0,
        "nothing outside the selection"
    );
}

/// The layer of `folder` made last: the member that is not the first
/// flat layer.
fn top_new(driver: &PaintDriver, folder: EntityId) -> EntityId {
    let members = sue_paint::doc::members(arena(driver), picture(driver), folder);
    assert!(members.len() >= 2, "the new layer is in the folder");
    *members
        .iter()
        .max_by_key(|id| {
            layers(driver)
                .iter()
                .position(|info| info.id == **id)
                .unwrap_or(0)
        })
        .unwrap()
}

/// CC 20:12: a double click on a layer's name renames it.
#[test]
fn cc_2012_double_click_renames_a_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("rename");
    let layer = top(&driver);
    let at = driver.rect_of("Layer 1").unwrap().center();
    let press = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    driver.step(vec![Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(4);
    // The name is selected: what is typed takes its place.
    driver.step(vec![Event::Text("Horns".to_owned())]);
    driver.settle(2);
    let before = bytes(&driver);
    tap(&mut driver, Key::Enter);
    assert_eq!(arena(&driver).get_by_id(layer).unwrap().name(), "Horns");
    undo_pair(&mut driver, &before);
    // Renamed again: the field takes the keys again.
    let at = driver.rect_of("Horns").unwrap().center();
    for pressed in [true, false, true, false] {
        driver.step(vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
    driver.settle(4);
    driver.step(vec![Event::Text("Tail".to_owned())]);
    tap(&mut driver, Key::Enter);
    assert_eq!(arena(&driver).get_by_id(layer).unwrap().name(), "Tail");
}

/// CC 20:12, the other way in: a rename moved to another layer by
/// Layer, Change Layer Name leaves the first layer's field behind, and
/// a double click on that layer later still takes the keys.
#[test]
fn cc_2012_a_rename_moved_away_does_not_stick() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("rename-moved");
    run(&mut driver, "layer.new");
    let first = named(&driver, "Layer 1");
    let second = named(&driver, "Layer 2");
    let double_click = |driver: &mut PaintDriver, name: &str| {
        let at = driver.rect_of(name).unwrap().center();
        for pressed in [true, false, true, false] {
            driver.step(vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
        driver.settle(4);
    };
    double_click(&mut driver, "Layer 1");
    // The rename moves to the layer above through the menu command.
    run(&mut driver, "layer.select_above");
    run(&mut driver, "layer.rename");
    driver.step(vec![Event::Text("Wings".to_owned())]);
    tap(&mut driver, Key::Enter);
    assert_eq!(arena(&driver).get_by_id(second).unwrap().name(), "Wings");
    double_click(&mut driver, "Layer 1");
    driver.step(vec![Event::Text("Body".to_owned())]);
    tap(&mut driver, Key::Enter);
    assert_eq!(arena(&driver).get_by_id(first).unwrap().name(), "Body");
}

/// CC 20:42 (and 28:01; CT-5): several layers picked with their boxes,
/// Ctrl+G puts them all in one new folder.
#[test]
fn cc_2042_ctrl_g_puts_every_picked_layer_in_a_folder() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("group");
    run(&mut driver, "layer.new");
    run(&mut driver, "layer.new");
    let ids: Vec<EntityId> = layers(&driver)
        .into_iter()
        .filter(|info| info.name.starts_with("Layer"))
        .map(|info| info.id)
        .collect();
    assert_eq!(ids.len(), 3);
    for name in ["Layer 1", "Layer 2"] {
        let row = driver.rect_of(name).unwrap();
        click_held(
            &mut driver,
            row.center(),
            PointerButton::Primary,
            Modifiers::COMMAND,
        );
    }
    let before = bytes(&driver);
    press(&mut driver, Key::G, Modifiers::COMMAND);
    let folder = sue_paint::doc::folder_of(arena(&driver), ids[0]).expect("in a folder");
    for id in &ids {
        assert_eq!(
            sue_paint::doc::folder_of(arena(&driver), *id),
            Some(folder),
            "every picked layer went in"
        );
    }
    undo_pair(&mut driver, &before);
}

/// Picks `mode` from the Layer palette's blending mode list.
fn blend_mode(driver: &mut PaintDriver, mode: &str) {
    let layer = top(driver);
    let before = style(driver, layer).blend;
    click_widget(driver, "Blending mode");
    // A long list scrolls: the wheel over it until the mode can be had.
    for _ in 0..12 {
        let at = driver.rect_of(mode).expect("the mode is listed").center();
        if at.y < 600.0 {
            break;
        }
        let over = driver.rect_of("Darken").map_or(at, |rect| rect.center());
        driver.step(vec![
            Event::PointerMoved(over),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -60.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            },
        ]);
        driver.settle(3);
    }
    driver.click_text(mode);
    let _ = before;
}

/// CC 21:02, 24:11, 24:28, 25:24: new layers set to Multiply, Overlay
/// and Screen from the blending mode list.
#[test]
fn cc_2102_blend_modes_from_the_layer_palette() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blend-modes");
    for (mode, want) in [
        ("Multiply", sue_model::BlendMode::Multiply),
        ("Overlay", sue_model::BlendMode::Overlay),
        ("Screen", sue_model::BlendMode::Screen),
    ] {
        run(&mut driver, "layer.new");
        let layer = top(&driver);
        blend_mode(&mut driver, mode);
        assert_eq!(style(&driver, layer).blend, want, "{mode}");
    }
}

/// CC 25:41 (CT-14, CCS-6): Add (Glow) and Glow Dodge are in the list,
/// and a bright Add (Glow) layer lightens what is below.
#[test]
fn cc_2541_add_glow_lights_the_rim() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("add-glow");
    run(&mut driver, "edit.fill");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    click_widget(&mut driver, "Blending mode");
    assert!(shows(&driver, "Glow Dodge"), "{:?}", driver.texts);
    driver.click_text("Add (Glow)");
    assert_eq!(style(&driver, layer).blend, sue_model::BlendMode::AddGlow);
    tap(&mut driver, Key::X);
    run(&mut driver, "edit.fill");
    let p = seen_middle(&driver);
    assert!(p[0] > 200, "white over black glows: {p:?}");
}

/// CC 22:15 (CC-6): Window, Canvas, New Window adds a second tab of the
/// same canvas and leaves tiling alone.
#[test]
fn cc_2215_new_window_adds_a_tab_of_the_same_canvas() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("new-window");
    let canvas = picture(&driver);
    let width = driver.runtime.shell.page_rect().width();
    let before = zoom(&driver);
    run(&mut driver, "window.new");
    driver.settle(6);
    let tabs = driver
        .texts
        .iter()
        .filter(|(text, rect)| text.starts_with("Illustration") && rect.top() < 100.0)
        .count();
    assert_eq!(tabs, 2, "two tabs: {:?}", driver.texts);
    assert_eq!(picture(&driver), canvas, "of the same canvas");
    assert_eq!(driver.runtime.shell.page_rect().width(), width);
    assert!(
        (zoom(&driver) - before).abs() < 0.01,
        "a whole view, not a tile"
    );
}

/// CC 22:38: the Blend tool softens a hard edge.
#[test]
fn cc_2238_the_blend_tool_softens_a_shadow_edge() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blend-shadow");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, -60.0), m + vec2(0.0, 60.0));
    run(&mut driver, "edit.fill");
    run(&mut driver, "select.none");
    let partial = |driver: &PaintDriver| {
        sue_paint::doc::raster(arena(driver), layer)
            .unwrap()
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 10 && pixel[3] < 245)
            .count()
    };
    let hard = partial(&driver);
    tap(&mut driver, Key::J);
    driver.click_text("Blur");
    for _ in 0..10 {
        tap(&mut driver, Key::CloseBracket);
    }
    stroke(&mut driver, m + vec2(0.0, -50.0), m + vec2(0.0, 50.0));
    assert!(
        partial(&driver) > hard + 50,
        "{hard} to {}",
        partial(&driver)
    );
}

/// CC 23:14 (CT-15, CC-7): a layer dragged onto New Raster Layer is
/// copied right above it as "<name> Copy", and the copy is picked.
#[test]
fn cc_2314_a_layer_dragged_onto_new_layer_is_copied() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("drag-copy");
    let layer = ink_line(&mut driver);
    let count = layers(&driver).len();
    let row = driver.rect_of("Layer 2").unwrap().center();
    let new = widget(&mut driver, "Raster Layer").unwrap().center();
    driver.drag(row, new, PointerButton::Primary);
    assert_eq!(layers(&driver).len(), count + 1, "copied");
    let all = layers(&driver);
    let at = all.iter().position(|info| info.id == layer).unwrap();
    assert_eq!(all[at + 1].name, "Layer 2 Copy", "right above, named so");
    // Picked: the next stroke goes on the copy.
    let copy = all[at + 1].id;
    let ink = painted(&driver, copy);
    let m = middle(&driver);
    pen(&mut driver);
    stroke(&mut driver, m + vec2(-50.0, 60.0), m + vec2(50.0, 60.0));
    assert!(painted(&driver, copy) > ink, "the copy is picked");
}

/// CC 23:30: Lock Transparent Pixels, then paint over recolors only what
/// is painted.
#[test]
fn cc_2330_lock_transparent_pixels_recolors_only_the_paint() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lock-alpha");
    let layer = ink_line(&mut driver);
    let line = painted(&driver, layer);
    click_widget(&mut driver, "Lock Transparent Pixels");
    tap(&mut driver, Key::X);
    for _ in 0..10 {
        tap(&mut driver, Key::CloseBracket);
    }
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -30.0), m + vec2(60.0, 30.0));
    assert_eq!(painted(&driver, layer), line, "no new pixels");
    let p = pixel(&driver, layer, under(&driver, m));
    assert!(p[0] > 200, "the line took the other color: {p:?}");
}

/// CC 26:22: Clip to Layer Below, and the paint shows on the lines only.
#[test]
fn cc_2622_clip_to_layer_below_paints_on_the_lines_only() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("clip");
    ink_line(&mut driver);
    run(&mut driver, "layer.new");
    let clipped = top(&driver);
    click_widget(&mut driver, "Clip to Layer Below");
    assert!(style(&driver, clipped).clip);
    tap(&mut driver, Key::X);
    run(&mut driver, "edit.fill");
    let m = middle(&driver);
    let tile = seen(&driver);
    let on = seen_at(&tile, under(&driver, m));
    let off = seen_at(&tile, under(&driver, m + vec2(0.0, 60.0)));
    assert!(on[0] > 200, "white on the line: {on:?}");
    assert!(
        off[0] > 200 && off[3] == 255,
        "the paper elsewhere: {off:?}"
    );
    // The paint did not cover the paper: hidden, the clip shows a black
    // line again.
    click_widget(&mut driver, "Clip to Layer Below");
    assert!(!style(&driver, clipped).clip);
}

/// CC 27:49: the transparent color pulls paint back.
#[test]
fn cc_2749_the_transparent_color_pulls_paint_back() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("transparent-c");
    let layer = ink_line(&mut driver);
    let line = painted(&driver, layer);
    tap(&mut driver, Key::C);
    for _ in 0..6 {
        tap(&mut driver, Key::CloseBracket);
    }
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, 0.0), m + vec2(0.0, 0.0));
    assert!(painted(&driver, layer) < line);
}

/// CC 28:01, 28:07, 28:11 (X-9): Ctrl+G groups, the author's Ctrl+J
/// duplicates the folder with what it holds, the original is hidden and
/// Merge Selected Layers bakes the copy into one raster layer that keeps
/// its name; the original stays as it was.
#[test]
fn cc_2801_group_duplicate_and_merge_the_copy() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("merge-copy");
    let (lines, _, _) = dragon(&mut driver);
    let look = seen(&driver);
    // Ctrl+G on the line art and the masked folder.
    let folder_name = layers(&driver)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(arena(&driver), info.id))
        .unwrap()
        .name;
    driver.click_text("Layer 2");
    let row = driver.rect_of(&folder_name).unwrap();
    click_held(
        &mut driver,
        row.center(),
        PointerButton::Primary,
        Modifiers::COMMAND,
    );
    press(&mut driver, Key::G, Modifiers::COMMAND);
    let group = sue_paint::doc::folder_of(arena(&driver), lines).expect("grouped");
    let group_name = arena(&driver).get_by_id(group).unwrap().name().to_owned();
    // Ctrl+J, the author's key for Duplicate Layer.
    bind(&mut driver, "layer.duplicate", "Ctrl+J");
    press(&mut driver, Key::J, Modifiers::COMMAND);
    let copy_name = format!("{group_name} Copy");
    let copy = named(&driver, &copy_name);
    assert!(sue_paint::doc::is_folder(arena(&driver), copy));
    assert_eq!(
        sue_paint::doc::members(arena(&driver), picture(&driver), copy).len(),
        sue_paint::doc::members(arena(&driver), picture(&driver), group).len(),
        "the copy holds a copy of every member"
    );
    // The original hidden with its eye (by the layer command here).
    driver.click_text(&group_name);
    run(&mut driver, "layer.toggle.visible");
    driver.click_text(&copy_name);
    let original: Vec<EntityId> = sue_paint::doc::members(arena(&driver), picture(&driver), group);
    run(&mut driver, "layer.merge_selected");
    let merged = named(&driver, &copy_name);
    assert!(
        !sue_paint::doc::is_folder(arena(&driver), merged),
        "one raster layer"
    );
    assert_eq!(
        sue_paint::doc::members(arena(&driver), picture(&driver), group),
        original,
        "the original is untouched"
    );
    // The merged layer looks as the group did: masks applied.
    let (inside, outside) = box_points(&driver);
    let now = seen(&driver);
    assert_eq!(seen_at(&now, inside), seen_at(&look, inside));
    assert_eq!(seen_at(&now, outside), seen_at(&look, outside));
}

/// CC 28:14: a color mixing brush (Oil paint) blends the colors it
/// passes over. A defect found here: the built-in mixing brushes lay
/// their own color only (`Mixing::new` loads the brush fully, so what it
/// picks up never shows); sent to the owner of the brush presets.
#[test]
#[ignore = "CC 28:14 defect: Oil paint does not mix (brush presets, p5-brush)"]
fn cc_2814_oil_paint_mixes_the_colors_it_crosses() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("oil");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
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

/// CC 28:57: Window, Material opens the Material palette.
#[test]
fn cc_2857_window_material_opens_the_material_palette() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("material");
    let was = driver.rect_of("Material").is_some();
    run(&mut driver, "window.material");
    driver.settle(6);
    if was {
        run(&mut driver, "window.material");
        driver.settle(6);
    }
    assert!(driver.rect_of("Material").is_some(), "{:?}", driver.texts);
}

/// CC 29:55 (CT-18): File, Export (Single Layer), PNG: the export
/// settings first, then the picture.
#[test]
fn cc_2955_export_single_layer_as_png() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("export-png");
    ink_line(&mut driver);
    run(&mut driver, "file.export.png");
    driver.settle(6);
    assert!(shows(&driver, "Export Settings"), "{:?}", driver.texts);
    driver.click_text("Export");
    driver.settle(6);
    driver.finish_exports();
    let mut found = Vec::new();
    let mut folders = vec![driver.root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext == "png") {
                found.push(path);
            }
        }
    }
    assert_eq!(found.len(), 1, "one PNG: {found:?}");
    let back = sue_io::decode_png(&std::fs::read(&found[0]).unwrap()).unwrap();
    assert_eq!(
        [back.tile.width(), back.tile.height()],
        size(&driver),
        "the whole picture"
    );
}

// ----- Crash Course: the rows the picture added (CC-1 to CC-12) --------------

/// CC 00:48 and 01:13 (X-1): Sue Paint opens to its canvas area, never
/// to a launcher; with no canvas open New sits in the middle and opens
/// the New dialog.
#[test]
fn cc_0048_no_launcher_and_new_in_the_middle_with_no_canvas() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("no-canvas");
    run(&mut driver, "file.close");
    driver.settle(20);
    assert!(shows(&driver, "No canvas is open"), "{:?}", driver.texts);
    let page = driver.runtime.shell.page_rect();
    let new = driver.rect_of("New...").expect("New is offered");
    assert!((new.center().x - page.center().x).abs() < 200.0);
    driver.click_text("New...");
    assert!(driver.rect_of("Preset").is_some(), "the New dialog");
}

/// CC 05:26 (CT-11): sub tools of several tools gathered where one key
/// reaches them: a pen dragged out onto the Tool palette becomes a tool,
/// that tool dragged onto the Brush tool's list joins it, and B shows
/// it among the brushes.
#[test]
fn cc_0526_a_pen_moved_into_the_brush_tool_is_reached_by_b() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("groups");
    pen(&mut driver);
    let mapping = widget(&mut driver, "Mapping pen").expect("Mapping pen");
    let eraser = widget(&mut driver, "Eraser (E)").expect("the Eraser tool");
    let before_eraser = egui::pos2(eraser.center().x, eraser.top() + eraser.height() / 8.0);
    driver.drag(mapping.center(), before_eraser, PointerButton::Primary);
    let made = widgets(&mut driver)
        .into_iter()
        .filter(|(name, rect)| name == "Mapping pen" && rect.left() < 46.0)
        .map(|(_, rect)| rect)
        .next()
        .expect("a Mapping pen tool on the Tool palette");
    click_widget(&mut driver, "Brush (B)");
    let list = widget(&mut driver, "Round watercolor brush").expect("the Brush list");
    driver.drag(made.center(), list.center(), PointerButton::Primary);
    driver.settle(4);
    pen(&mut driver);
    assert!(
        widget(&mut driver, "Mapping pen").is_none(),
        "it left the Pen tool"
    );
    sub_tool(
        &mut driver,
        Key::B,
        "Round watercolor brush",
        "Round watercolor brush",
    );
    assert!(
        widget(&mut driver, "Mapping pen").is_some(),
        "B reaches it among the brushes: {:?}",
        driver.texts
    );
}

/// CC-2 (13:58): Auto Select has three sub tools: the editing layer,
/// the other layers, and the reference layers.
#[test]
fn cc2_auto_select_has_three_sub_tools() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("auto-select-subs");
    tap(&mut driver, Key::W);
    driver.settle(4);
    for name in [
        "Refer Only Editing Layer",
        "Refer Other Layers",
        "Refer Reference Layers",
    ] {
        assert!(shows(&driver, name), "{name}: {:?}", driver.texts);
    }
}

/// How many pixels the selection holds, read by turning it into a
/// selection layer and undoing that.
fn selected(driver: &mut PaintDriver) -> usize {
    let count = layers(driver).len();
    run(driver, "select.to_layer");
    if layers(driver).len() == count {
        return 0;
    }
    let made = top(driver);
    let held = painted(driver, made);
    press(driver, Key::Z, Modifiers::COMMAND);
    held
}

/// CC-3 (06:26, 18:05): the Shrink selection sub tool: a loop drawn
/// round a part of the drawing selects that part only.
#[test]
fn cc3_shrink_selection_selects_what_is_drawn_inside() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("shrink-selection");
    ink_line(&mut driver);
    tap(&mut driver, Key::M);
    driver.click_text("Shrink selection");
    let m = middle(&driver);
    let ring = [
        m + vec2(-150.0, -60.0),
        m + vec2(-10.0, -60.0),
        m + vec2(-10.0, 60.0),
        m + vec2(-150.0, 60.0),
        m + vec2(-150.0, -60.0),
    ];
    path_held(&mut driver, &ring, PointerButton::Primary, Modifiers::NONE);
    let held = selected(&mut driver);
    let zoomed = zoom(&driver);
    let ring_area = (140.0 * 120.0 / (zoomed * zoomed)) as usize;
    assert!(held > 0, "something is selected");
    assert!(
        held < ring_area / 3,
        "only the drawing: {held} of {ring_area}"
    );
}

/// CC-4 (07:20): Liquify is a tool of its own with one sub tool, its
/// modes in Tool Property.
#[test]
fn cc4_liquify_is_one_tool_with_its_modes() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("liquify-modes");
    click_widget(&mut driver, "Liquify (J)");
    driver.settle(4);
    assert!(!shows(&driver, "Blur"), "no Blend list: {:?}", driver.texts);
    click_widget(&mut driver, "Mode");
    for name in [
        "Push",
        "Expand",
        "Pinch",
        "Twirl clockwise",
        "Twirl counterclockwise",
        "Push left",
        "Push right",
    ] {
        assert!(shows(&driver, name), "the mode {name}: {:?}", driver.texts);
    }
    driver.click_text("Pinch");
    assert!(shows(&driver, "Pinch"), "picked");
}

/// CC-6 (22:15): Window, Canvas lists New Window, Cascade, Tiled, then
/// Next and Previous canvas and the open canvases; Ctrl+Shift+Tab goes
/// back to the previous canvas.
#[test]
fn cc6_window_canvas_lists_the_canvases_and_ctrl_shift_tab_goes_back() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("canvas-menu");
    let first = picture(&driver);
    press(&mut driver, Key::N, Modifiers::COMMAND);
    tap(&mut driver, Key::Enter);
    driver.settle(10);
    let second = picture(&driver);
    assert_ne!(first, second);
    press(&mut driver, Key::Tab, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(picture(&driver), first, "back to the first canvas");
    driver.click_text("Window");
    driver.click_text("Canvas");
    driver.settle(4);
    let at = |name: &str| driver.rect_of(name).map(|rect| rect.top());
    let order = [
        "New Window",
        "Cascade",
        "Tile",
        "Next Canvas",
        "Previous Canvas",
    ];
    let tops: Vec<Option<f32>> = order.iter().map(|name| at(name)).collect();
    assert!(
        tops.iter().all(Option::is_some),
        "{order:?} in {:?}",
        driver.texts
    );
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
    let numbered = driver
        .texts
        .iter()
        .filter(|(text, _)| text.starts_with("1 ") || text.starts_with("2 "))
        .count();
    assert!(numbered >= 2, "the open canvases: {:?}", driver.texts);
}

/// CC-9 (28:18): Alt and a click with the Fill tool picks the color
/// under the pointer.
#[test]
fn cc9_alt_picks_a_color_with_the_fill_tool() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("alt-fill");
    let layer = ink_line(&mut driver);
    // White to draw with.
    tap(&mut driver, Key::X);
    sub_tool(
        &mut driver,
        Key::G,
        "Close and fill",
        "Refer Only Editing Layer",
    );
    let m = middle(&driver);
    click_held(&mut driver, m, PointerButton::Primary, Modifiers::ALT);
    // The black of the line is the drawing color now: a fill of the
    // empty area lays black.
    run(&mut driver, "layer.new");
    let flat = top(&driver);
    click(&mut driver, m + vec2(0.0, 80.0));
    let p = pixel(&driver, flat, under(&driver, m + vec2(0.0, 80.0)));
    assert!(p[3] > 0 && p[0] < 60, "black was picked: {p:?}");
    let _ = layer;
}

/// CC-12 (04:37, 30:05): the File menu in Clip Studio Paint's order:
/// Export (Single Layer) before Import, the settings together, Manage
/// Fonts, Pen Pressure Settings, Quit.
#[test]
fn cc12_the_file_menu_in_order() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("file-menu");
    driver.click_text("File");
    driver.settle(4);
    let order = [
        "New...",
        "Save",
        "Save As...",
        "Export (Single Layer)",
        "Import",
        "Preferences...",
        "Shortcut Settings...",
        "Modifier Key Settings...",
        "Manage Fonts...",
        "Pen Pressure Settings...",
    ];
    let tops: Vec<Option<f32>> = order
        .iter()
        .map(|name| driver.rect_of(name).map(|rect| rect.top()))
        .collect();
    assert!(
        tops.iter().all(Option::is_some),
        "{order:?}: {tops:?} in {:?}",
        driver.texts
    );
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}

// ----- Things I like about Clip Studio Paint (vector layers) -----------------

/// The vector lines of `layer`.
fn lines_on(driver: &PaintDriver, layer: EntityId) -> Vec<sue_paint::vector::Line> {
    sue_paint::vector::lines(&sue_paint::vector::geometry(arena(driver), layer))
}

/// Where picture point `p` is on screen while the view has not moved.
fn screen_of(driver: &PaintDriver, p: [f32; 2]) -> Pos2 {
    let [w, h] = size(driver);
    let scale = zoom(driver);
    middle(driver) + vec2(p[0] - w as f32 / 2.0, p[1] - h as f32 / 2.0) * scale
}

/// A new vector layer and the pen: the layer.
fn vector_layer(driver: &mut PaintDriver) -> EntityId {
    run(driver, "layer.new.vector");
    pen(driver);
    top(driver)
}

/// TL 00:21: the Command Bar has Save among its icons.
#[test]
fn tl_0021_the_command_bar_has_save() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("command-bar-save");
    // Save is the button right of Open, as in Clip Studio Paint's bar
    // (it tells assistive technology no name of its own).
    let open_button = widget(&mut driver, "Open Picture").expect("Open on the bar");
    let save = open_button.translate(vec2(open_button.width() + 2.0, 0.0));
    ink_line(&mut driver);
    click(&mut driver, save.center());
    assert!(driver.runtime.pending_pick().is_some(), "Save asks where");
}

/// TL 00:39: a pen on a vector layer keeps the stroke as a line.
#[test]
fn tl_0039_a_vector_layer_keeps_strokes_as_lines() {
    let mut driver = open("vector-draw");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    assert_eq!(lines_on(&driver, layer).len(), 1);
}

/// TL 00:50 (CTS-5): an overshoot erased up to the crossing with the
/// Vector eraser.
#[test]
fn tl_0050_the_vector_eraser_erases_up_to_the_intersection() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-erase");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-100.0, 0.0), m + vec2(100.0, 0.0));
    stroke(&mut driver, m + vec2(0.0, -80.0), m + vec2(0.0, 40.0));
    assert_eq!(lines_on(&driver, layer).len(), 2);
    tap(&mut driver, Key::E);
    driver.click_text("Vector");
    stroke(&mut driver, m + vec2(-20.0, 30.0), m + vec2(20.0, 30.0));
    let lines = lines_on(&driver, layer);
    let [_, y] = under(&driver, m);
    let lowest = lines
        .iter()
        .flat_map(|line| line.samples.iter().map(|sample| sample.at[1]))
        .fold(f32::MIN, f32::max);
    assert!(
        (lowest - y as f32).abs() < 6.0,
        "the overshoot stops at the crossing: {lowest} against {y}"
    );
}

/// TL 01:03: Correct Line's control point drags a short end to meet
/// the other line.
#[test]
fn tl_0103_a_control_point_drags_the_end_to_meet() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-point");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-100.0, 0.0), m + vec2(100.0, 0.0));
    stroke(&mut driver, m + vec2(0.0, -80.0), m + vec2(0.0, -30.0));
    sub_tool(&mut driver, Key::Y, "Pinch vector line", "Control point");
    let end = lines_on(&driver, layer)[1].samples.last().unwrap().at;
    driver.drag(screen_of(&driver, end), m, PointerButton::Primary);
    let moved = lines_on(&driver, layer)[1].samples.last().unwrap().at;
    let [_, y] = under(&driver, m);
    assert!((moved[1] - y as f32).abs() < 4.0, "it reaches: {moved:?}");
}

/// TL 01:11 (CTL-1): Object, a loop round the lines picks them, and the
/// box's bottom handle stretches them.
#[test]
fn tl_0111_object_loops_lines_and_stretches_them() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-object");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, -40.0), m + vec2(80.0, -40.0));
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    let drawn = lines_on(&driver, layer);
    tap(&mut driver, Key::O);
    path_held(
        &mut driver,
        &[
            m + vec2(-120.0, -70.0),
            m + vec2(120.0, -70.0),
            m + vec2(120.0, 30.0),
            m + vec2(-120.0, 30.0),
            m + vec2(-120.0, -68.0),
        ],
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let [_, top_y, _, bottom] = sue_paint::vector::bounds(&drawn, &[0, 1]).unwrap();
    let handle = screen_of(&driver, [size(&driver)[0] as f32 / 2.0, bottom]);
    driver.drag(handle, handle + vec2(0.0, 40.0), PointerButton::Primary);
    let now = lines_on(&driver, layer);
    let [_, new_top, _, new_bottom] = sue_paint::vector::bounds(&now, &[0, 1]).unwrap();
    assert!((new_top - top_y).abs() < 1.0, "the top stays");
    assert!(
        new_bottom > bottom + 20.0,
        "taller: {bottom} to {new_bottom}"
    );
}

/// TL 01:20 and 01:32 (CTL-2): the picked lines take a new main color
/// typed as hex, and a bigger brush size.
#[test]
fn tl_0120_picked_lines_take_a_color_and_a_size() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-recolor");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    let drawn = lines_on(&driver, layer);
    tap(&mut driver, Key::O);
    path_held(
        &mut driver,
        &[
            m + vec2(-120.0, -40.0),
            m + vec2(120.0, -40.0),
            m + vec2(120.0, 40.0),
            m + vec2(-120.0, 40.0),
            m + vec2(-120.0, -38.0),
        ],
        PointerButton::Primary,
        Modifiers::NONE,
    );
    driver.click_text("Tool Property");
    driver.settle(4);
    let was = sue_paint::color::hex(sue_paint::color::bytes(drawn[0].colors[0]));
    driver.click_text(&was);
    driver.step(vec![key(Key::A, Modifiers::COMMAND)]);
    driver.step(vec![Event::Text("#141414".to_owned())]);
    driver.settle(3);
    tap(&mut driver, Key::Escape);
    let recolored = lines_on(&driver, layer);
    assert!(
        (recolored[0].colors[0][0] - 20.0 / 255.0).abs() < 1e-3,
        "{:?}",
        recolored[0].colors
    );
    let size_text = sue_wm::ui::input::plain_number(f64::from(drawn[0].tip.size), 1);
    driver.click_text(&size_text);
    driver.step(vec![key(Key::A, Modifiers::COMMAND)]);
    driver.step(vec![Event::Text(format!("{}", drawn[0].tip.size * 2.0))]);
    tap(&mut driver, Key::Enter);
    let thick = lines_on(&driver, layer);
    assert!((thick[0].tip.size - drawn[0].tip.size * 2.0).abs() < 0.05);
}

/// TL 01:35 (CTS-5): Simplify vector line brushed along a wobbly circle
/// smooths it toward a clean curve.
#[test]
fn tl_0135_simplify_vector_line_smooths_the_wobble() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-simplify");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    let circle: Vec<Pos2> = (0..=48)
        .map(|step| {
            let a = step as f32 / 48.0 * std::f32::consts::TAU;
            let wobble = if step % 2 == 0 { 6.0 } else { -6.0 };
            m + vec2(a.cos(), a.sin()) * (70.0 + wobble)
        })
        .collect();
    path_held(
        &mut driver,
        &circle,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    // How far the line strays from its mean radius, on average.
    let wobble = |driver: &PaintDriver| {
        let line = &lines_on(driver, layer)[0];
        let [w, h] = size(driver);
        let centre = [w as f32 / 2.0, h as f32 / 2.0];
        let radii: Vec<f32> = line
            .samples
            .iter()
            .map(|sample| (sample.at[0] - centre[0]).hypot(sample.at[1] - centre[1]))
            .collect();
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        radii.iter().map(|r| (r - mean).abs()).sum::<f32>() / radii.len() as f32
    };
    let before = wobble(&driver);
    sub_tool(
        &mut driver,
        Key::Y,
        "Pinch vector line",
        "Simplify vector line",
    );
    for _ in 0..10 {
        tap(&mut driver, Key::CloseBracket);
    }
    path_held(
        &mut driver,
        &circle,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let after = wobble(&driver);
    assert!(after < before * 0.7, "smoother: {before} to {after}");
}

/// TL 01:46: the open end dragged onto the other end closes the circle.
#[test]
fn tl_0146_an_open_end_dragged_onto_the_start_closes_it() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-close");
    let layer = vector_layer(&mut driver);
    let m = middle(&driver);
    let arc: Vec<Pos2> = (0..=40)
        .map(|step| {
            let a = step as f32 / 48.0 * std::f32::consts::TAU;
            m + vec2(a.cos(), a.sin()) * 70.0
        })
        .collect();
    path_held(&mut driver, &arc, PointerButton::Primary, Modifiers::NONE);
    let line = lines_on(&driver, layer)[0].clone();
    let (start, end) = (line.samples[0].at, line.samples.last().unwrap().at);
    sub_tool(&mut driver, Key::Y, "Pinch vector line", "Control point");
    driver.drag(
        screen_of(&driver, end),
        screen_of(&driver, start) + vec2(3.0, 2.0),
        PointerButton::Primary,
    );
    let closed = lines_on(&driver, layer)[0].clone();
    assert_eq!(
        closed.samples.last().unwrap().at,
        closed.samples[0].at,
        "the ends meet exactly"
    );
}

/// TL 01:54 (CT-7, CTL-3): the Ellipse figure on the vector layer is a
/// vector line drawn with the brush.
#[test]
fn tl_0154_the_ellipse_figure_on_a_vector_layer_is_a_line() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("vector-ellipse");
    let layer = vector_layer(&mut driver);
    sub_tool(&mut driver, Key::U, "Straight Line", "Ellipse");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -40.0), m + vec2(60.0, 40.0));
    let lines = lines_on(&driver, layer);
    assert_eq!(lines.len(), 1, "one line");
    assert!(lines[0].samples.len() > 8, "round");
}

// ----- 50 Quick Tips & Tricks ---------------------------------------------------

/// Clicks the button of command `id` on the launcher shown (the
/// Selection Launcher under a selection).
fn launcher(driver: &mut PaintDriver, id: &str) {
    let shown = sue_paint::shown_buttons(&driver.ctx);
    let at = shown
        .iter()
        .find(|(found, _)| found == id)
        .unwrap_or_else(|| panic!("{id} on the launcher: {shown:?}"))
        .1
        .center();
    click(driver, at);
}

/// Whether the control named `name` is ticked, as a fresh frame tells
/// assistive technology.
fn ticked(driver: &mut PaintDriver, name: &str) -> Option<bool> {
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
    let found = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .find(|(_, node)| node.label() == Some(name))
        .and_then(|(_, node)| node.toggled())
        .map(|toggled| toggled == egui::accesskit::Toggled::True);
    output.drop_without_applying_deltas();
    found
}

/// Whether the last frame drew any of `texts`.
fn shows_any(driver: &PaintDriver, texts: &[&str]) -> bool {
    texts.iter().any(|text| shows(driver, text))
}

/// Tip 1 (00:26): the Layer palette's button splits the list into two
/// panes that list the same layers.
#[test]
fn t50_01_the_layer_list_splits_into_two_panes() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("split-list");
    let once = driver.texts.iter().filter(|(t, _)| t == "Layer 1").count();
    click_widget(&mut driver, "Split Layer List");
    driver.settle(4);
    let twice = driver.texts.iter().filter(|(t, _)| t == "Layer 1").count();
    assert_eq!(twice, once * 2, "{once} to {twice}");
}

/// Tip 2 (00:42, CT-31): a sub tool on the Command Bar, as a button
/// that picks it (put there through Command Bar Settings).
#[test]
fn t50_02_a_sub_tool_on_the_command_bar() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("bar-sub-tool");
    run(&mut driver, "file.command_bar");
    driver.settle(8);
    click_widget(&mut driver, "Category");
    driver.click_text("Sub Tools");
    driver.click_text("Search");
    driver.step(vec![Event::Text("Turnip pen".to_owned())]);
    driver.settle(3);
    let entry = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Turnip pen")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the entry");
    click(&mut driver, entry.center());
    driver.click_text("Add");
    tap(&mut driver, Key::Escape);
    driver.runtime.shell.dismiss_overlays();
    driver.settle(6);
    let on_bar = widgets(&mut driver)
        .into_iter()
        .find(|(name, rect)| name.starts_with("Turnip pen") && rect.top() < 70.0)
        .map(|(_, rect)| rect)
        .expect("a Turnip pen button on the bar");
    pen(&mut driver);
    click(&mut driver, on_bar.center());
    let shown = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Turnip pen")
        .count();
    assert!(shown >= 2, "listed and picked: {:?}", driver.texts);
}

/// Tip 3 (00:53): a 3D drawing figure on the canvas, posed and turned
/// by drags on the canvas as the video does, each in one undo step: a
/// drag on the body bends it at a joint, a drag beside it turns the
/// camera round it.
#[test]
fn t50_03_a_3d_figure_on_the_canvas() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("figure-3d");
    run(&mut driver, "layer.new.figure");
    let layer = top(&driver);
    assert!(sue_paint::three::is_3d(arena(&driver), layer), "a 3D layer");
    let yaw = |driver: &PaintDriver| {
        sue_paint::three::View3D::of(arena(driver).get_by_id(layer).unwrap()).yaw
    };
    let drawn = |driver: &PaintDriver| {
        sue_paint::doc::raster(arena(driver), layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec()
    };
    assert!(painted(&driver, layer) > 0, "the figure is drawn");
    let (start, before) = (yaw(&driver), drawn(&driver));
    // Posed: the Object tool, picked for the new figure, bends the part
    // under a drag on the body; the camera stays.
    let m = middle(&driver);
    assert!(
        pixel(&driver, layer, under(&driver, m))[3] > 0,
        "on the body"
    );
    stroke(&mut driver, m, m + vec2(60.0, 0.0));
    driver.settle(4);
    assert!(drawn(&driver) != before, "the posed figure is drawn again");
    assert!((yaw(&driver) - start).abs() < 0.01, "the camera stays");
    run(&mut driver, "edit.undo");
    assert!(drawn(&driver) == before, "one undo step puts the pose back");
    // Turned: a drag beside the figure turns the camera round it.
    // Well clear of every part (a part takes a press 16 points away).
    let [width, _] = size(&driver);
    let beside = (1..60)
        .map(|step| m - vec2(10.0 * step as f32, 0.0))
        .find(|at| pixel(&driver, layer, under(&driver, *at))[3] == 0)
        .map(|clear| clear - vec2(60.0, 0.0))
        .filter(|at| (0..i64::from(width)).contains(&under(&driver, *at)[0]))
        .expect("canvas beside the figure");
    stroke(&mut driver, beside, beside + vec2(100.0, 0.0));
    driver.settle(4);
    assert!((yaw(&driver) - start).abs() > 5.0, "the figure turned");
    assert!(drawn(&driver) != before, "the turned figure is drawn again");
    run(&mut driver, "edit.undo");
    assert!((yaw(&driver) - start).abs() < 0.01, "one undo step");
    assert!(drawn(&driver) == before, "undo draws it as it was");
}

/// Tip 4 (01:00, CTV-1): Convert to Lines and Tones opens its dialog
/// with the line and tone groups and a preview.
#[test]
fn t50_04_convert_to_lines_and_tones_opens_its_dialog() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lines-tones");
    ink_line(&mut driver);
    run(&mut driver, "filter.lines_tones");
    driver.settle(6);
    assert!(shows(&driver, "Extract line"), "{:?}", driver.texts);
    assert!(shows(&driver, "Posterization"));
    assert!(shows(&driver, "Frequency"));
}

/// Tip 5 (01:13, CT-35): File, Import, Pose Scanner asks for a photo.
#[test]
fn t50_05_file_import_pose_scanner_asks_for_a_photo() {
    let mut driver = open("pose-scanner");
    run(&mut driver, "file.import.pose_scanner");
    driver.settle(4);
    assert!(
        driver.runtime.pending_pick().is_some(),
        "a photo is asked for"
    );
}

/// Tip 6 (01:31, CTV-2): Layer Property's Border Effect outlines what is
/// on the layer, with its thickness, edge color and anti-aliasing.
#[test]
fn t50_06_border_effect_outlines_the_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("border");
    ink_line(&mut driver);
    // What covers the picture without its paper: the line, then the line
    // and its outline.
    let covered = |driver: &PaintDriver| {
        sue_paint::composite::flatten(
            arena(driver),
            picture(driver),
            sue_paint::composite::RenderOptions {
                paper: false,
                drafts: true,
            },
        )
        .unwrap()
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count()
    };
    let before = covered(&driver);
    run(&mut driver, "layer.properties");
    click_widget(&mut driver, "Border effect");
    driver.settle(4);
    for row in ["Edge color", "Anti-aliasing on border effect"] {
        assert!(shows(&driver, row), "{row}: {:?}", driver.texts);
    }
    assert!(covered(&driver) > before, "an outline round the line");
}

/// Tip 7 (01:50, CT-33, CTV-3): Auto Action records steps and plays
/// them back.
#[test]
fn t50_07_auto_action_records_and_plays() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("auto-action");
    // A new action records at once; Record (now Stop) ends it.
    run(&mut driver, "auto.new");
    run(&mut driver, "layer.new");
    run(&mut driver, "auto.record");
    let count = layers(&driver).len();
    run(&mut driver, "auto.play");
    driver.settle(6);
    assert_eq!(layers(&driver).len(), count + 1, "played: a layer more");
}

/// Tip 8 (02:10, CT-24): a palette color on a layer.
#[test]
fn t50_08_a_palette_color_marks_the_layer() {
    let mut driver = open("palette-color");
    let layer = top(&driver);
    let before = bytes(&driver);
    run(&mut driver, "layer.palette_color.red");
    assert!(sue_paint::doc::palette_color(arena(&driver), layer).is_some());
    undo_pair(&mut driver, &before);
}

/// Tip 9 (02:23): Ctrl+Shift and a click picks the layer under the
/// pointer.
#[test]
fn t50_09_ctrl_shift_click_picks_the_layer_under_the_pointer() {
    let mut driver = open("pick-layer");
    let first = ink_line(&mut driver);
    run(&mut driver, "layer.new");
    pen(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-50.0, 80.0), m + vec2(50.0, 80.0));
    click_held(
        &mut driver,
        m,
        PointerButton::Primary,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    // The next stroke goes on the line's layer.
    let ink = painted(&driver, first);
    stroke(&mut driver, m + vec2(-50.0, -60.0), m + vec2(50.0, -60.0));
    assert!(painted(&driver, first) > ink, "the line's layer is picked");
}

/// Tip 10 (02:35): the Selection Launcher crops to the selection.
#[test]
fn t50_10_the_selection_launcher_crops() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("launcher-crop");
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -40.0), m + vec2(60.0, 40.0));
    let was = size(&driver);
    click_widget(&mut driver, "Crop");
    let now = size(&driver);
    assert!(now[0] < was[0] && now[1] < was[1], "{was:?} to {now:?}");
}

/// Tip 11 (02:45): Alt and a click while drawing picks a color.
#[test]
fn t50_11_alt_picks_a_color_while_drawing() {
    let mut driver = open("alt-pen");
    ink_line(&mut driver);
    tap(&mut driver, Key::X);
    let m = middle(&driver);
    click_held(&mut driver, m, PointerButton::Primary, Modifiers::ALT);
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    stroke(&mut driver, m + vec2(-40.0, 60.0), m + vec2(40.0, 60.0));
    let p = pixel(&driver, layer, under(&driver, m + vec2(0.0, 60.0)));
    assert!(p[3] > 0 && p[0] < 60, "black picked: {p:?}");
}

/// Tip 12 (02:53, CTV-4): the Eyedropper's layer color sub tool picks
/// from the current layer only.
#[test]
fn t50_12_the_eyedropper_picks_from_the_layer_only() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("eyedropper-layer");
    ink_line(&mut driver);
    run(&mut driver, "layer.new");
    tap(&mut driver, Key::I);
    driver.click_text("Obtain Layer Color");
    let m = middle(&driver);
    // On the empty new layer the line below is not picked: the
    // transparent color is.
    tap(&mut driver, Key::X);
    click(&mut driver, m);
    pen(&mut driver);
    let layer = top(&driver);
    stroke(&mut driver, m + vec2(-40.0, 60.0), m + vec2(40.0, 60.0));
    assert_eq!(painted(&driver, layer), 0, "nothing to paint with");
}

/// Tip 14 (03:29, CT-34): the Page Manager's right click turns the
/// webtoon reading direction.
#[test]
fn t50_14_the_page_manager_menu_and_webtoon_direction() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("page-manager");
    run(&mut driver, "story.add");
    run(&mut driver, "story.add");
    run(&mut driver, "story.manager");
    driver.settle(6);
    let page = driver
        .texts
        .iter()
        .find(|(text, _)| text == "1")
        .map(|(_, rect)| *rect)
        .expect("page 1");
    click_held(
        &mut driver,
        page.center() + vec2(0.0, -30.0),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    assert!(
        shows(&driver, "Webtoon Reading Direction"),
        "{:?}",
        driver.texts
    );
}

/// Tip 15 (03:46): record a timelapse, then export it.
#[test]
fn t50_15_a_timelapse_is_recorded_and_exported() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("timelapse");
    run(&mut driver, "file.timelapse.record");
    ink_line(&mut driver);
    run(&mut driver, "file.timelapse.export");
    driver.settle(8);
    assert!(shows(&driver, "Export Timelapse"), "{:?}", driver.texts);
}

/// Tip 16 (04:08, CT-21, CTV-8): New Tone on the Selection Launcher
/// opens Simple Tone Settings and fills the selection with tone.
#[test]
fn t50_16_new_tone_from_the_launcher() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("launcher-tone");
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -40.0), m + vec2(60.0, 40.0));
    let count = layers(&driver).len();
    launcher(&mut driver, "layer.new.tone");
    driver.settle(4);
    assert!(shows(&driver, "Simple Tone Settings"), "{:?}", driver.texts);
    tap(&mut driver, Key::Enter);
    assert_eq!(layers(&driver).len(), count + 1, "a layer of tone");
}

/// Tip 17 (04:19, CT-27): the button under the wheel switches between the
/// HSV square and the HLS triangle.
#[test]
fn t50_17_the_wheel_switches_square_and_triangle() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("wheel-shape");
    click_widget(&mut driver, "Show the HLS triangle");
    assert!(
        widget(&mut driver, "Show the HSV square").is_some(),
        "the button now offers the square"
    );
}

/// Tip 18 (04:31, CT-28): a double click on the main color chip opens
/// Color Settings with the hex code.
#[test]
fn t50_18_double_click_on_the_color_opens_color_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("color-settings");
    let chip = widget(&mut driver, "Main color")
        .expect("the main color")
        .center();
    for pressed in [true, false, true, false] {
        driver.step(vec![
            Event::PointerMoved(chip),
            Event::PointerButton {
                pos: chip,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
    driver.settle(6);
    assert!(shows(&driver, "Color Settings"), "{:?}", driver.texts);
    assert!(shows(&driver, "#000000") || shows(&driver, "000000"));
}

/// Tip 19 (04:42, CT-26): the wheel over the blending mode steps it.
#[test]
fn t50_19_the_wheel_over_the_blend_list_steps_it() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blend-wheel");
    let layer = top(&driver);
    let at = widget(&mut driver, "Blending mode").unwrap().center();
    driver.step(vec![Event::PointerMoved(at)]);
    driver.settle(40);
    driver.step(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, -1.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    }]);
    driver.settle(3);
    assert_eq!(style(&driver, layer).blend, sue_model::BlendMode::Darken);
}

/// Tip 20 (04:56): a reference layer; Fill with Refer Reference Layers
/// stops at its lines while filling another layer.
#[test]
fn t50_20_fill_refers_to_the_reference_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("reference");
    let lines = line_box(&mut driver, vec2(100.0, 80.0));
    click_widget(&mut driver, "Set as Reference Layer");
    assert!(sue_paint::doc::is_reference(arena(&driver), lines));
    run(&mut driver, "layer.new");
    let flat = top(&driver);
    sub_tool(&mut driver, Key::G, "Close and fill", "Refer Other Layers");
    let m = middle(&driver);
    click(&mut driver, m);
    assert!(
        pixel(&driver, flat, under(&driver, m))[3] > 0,
        "filled inside"
    );
    assert_eq!(
        pixel(&driver, flat, under(&driver, m + vec2(200.0, 150.0)))[3],
        0,
        "stopped at the reference lines"
    );
}

/// Tip 22 (05:26): the Navigator's button flips the view.
#[test]
fn t50_22_the_navigator_flips_the_view() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("navigator-flip");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    let flip = widgets(&mut driver)
        .into_iter()
        .filter(|(name, rect)| name == "Flip Horizontal" && rect.left() > 1000.0)
        .map(|(_, rect)| rect)
        .next()
        .expect("the Navigator's flip");
    click(&mut driver, flip.center());
    pen(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(60.0, 0.0), m + vec2(120.0, 0.0));
    let (left, right) = painted_sides(&driver, layer);
    assert!(left > 0 && right == 0, "mirrored: {left} {right}");
}

/// Tip 24 (05:56, CT-21): Selection Launcher Settings opens, with the
/// buttons that can be added.
#[test]
fn t50_24_selection_launcher_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("launcher-settings");
    tap(&mut driver, Key::M);
    driver.click_text("Rectangle");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -40.0), m + vec2(60.0, 40.0));
    let ids: Vec<String> = sue_paint::shown_buttons(&driver.ctx)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for id in ["edit.cut_paste", "edit.copy_paste", "layer.new.tone"] {
        assert!(ids.iter().any(|found| found == id), "{id} in {ids:?}");
    }
    launcher(&mut driver, "launcher.settings");
    driver.settle(4);
    assert!(
        shows(&driver, "Selection Launcher Settings"),
        "{:?}",
        driver.texts
    );
}

/// Tip 25 (06:07): Window, Sub View shows reference pictures.
#[test]
fn t50_25_window_sub_view() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("sub-view");
    let was = widget(&mut driver, "Sub View").is_some();
    run(&mut driver, "window.sub_view");
    if was {
        run(&mut driver, "window.sub_view");
    }
    driver.settle(4);
    assert!(shows(&driver, "Sub View"), "{:?}", driver.texts);
}

/// Tip 26 (06:18, CTV-10): Create Perspective Ruler with its three types
/// and "Create new layer".
#[test]
fn t50_26_create_perspective_ruler() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("perspective");
    run(&mut driver, "layer.ruler.perspective");
    driver.settle(4);
    assert!(shows(&driver, "Create new layer"), "{:?}", driver.texts);
    let count = layers(&driver).len();
    driver.click_text("OK");
    driver.settle(4);
    assert_eq!(layers(&driver).len(), count + 1, "on a new layer");
}

/// Tip 28 and 44 (06:41, 11:07; CT-30): Sub Tool Detail with its
/// categories.
#[test]
fn t50_28_sub_tool_detail_lists_the_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("sub-tool-detail");
    pen(&mut driver);
    run(&mut driver, "window.sub_tool_detail");
    driver.settle(6);
    assert!(shows(&driver, "Sub Tool Detail"), "{:?}", driver.texts);
    assert!(shows_any(&driver, &["Brush Size", "Ink"]));
}

/// Tip 30 (07:05): File, Create New from Clipboard.
#[test]
fn t50_30_create_new_from_clipboard() {
    let mut driver = open("from-clipboard");
    ink_line(&mut driver);
    run(&mut driver, "select.all");
    press(&mut driver, Key::C, Modifiers::COMMAND);
    let first = picture(&driver);
    run(&mut driver, "file.new_clipboard");
    driver.settle(6);
    assert_ne!(picture(&driver), first, "a new canvas");
}

/// Tip 32 (07:43): a Tone Curve correction layer.
#[test]
fn t50_32_a_tone_curve_correction_layer() {
    let mut driver = open("tone-curve");
    let count = layers(&driver).len();
    run(&mut driver, "layer.correction.curve");
    tap(&mut driver, Key::Enter);
    assert_eq!(layers(&driver).len(), count + 1);
}

/// Tip 33 (07:55, CTV-11): Perlin Noise with its amplitude, attenuation
/// and repetition.
#[test]
fn t50_33_perlin_noise() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("perlin");
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    run(&mut driver, "filter.perlin");
    driver.settle(4);
    for row in ["Amplitude", "Attenuation", "Repetition"] {
        assert!(shows(&driver, row), "{row}: {:?}", driver.texts);
    }
    driver.click_text("OK");
    assert!(painted(&driver, layer) > 0, "noise");
}

/// Tip 34 (08:14, CTS-2): Onion Skin Settings.
#[test]
fn t50_34_onion_skin_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("onion");
    driver.new_timeline();
    run(&mut driver, "anim.onion_settings");
    driver.settle(4);
    assert!(shows(&driver, "Cels before"), "{:?}", driver.texts);
    assert!(shows(&driver, "Cels after"));
}

/// Tip 35 (08:27, CTV-12): Gradient Map with a gradient set.
#[test]
fn t50_35_gradient_map_with_sets() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("gradient-map");
    ink_line(&mut driver);
    let at = under(&driver, middle(&driver));
    let gray = seen_at(&seen(&driver), at);
    assert!(gray[0] == gray[1] && gray[1] == gray[2], "{gray:?} is gray");
    let count = layers(&driver).len();
    run(&mut driver, "layer.correction.gradient_map");
    driver.settle(4);
    assert!(shows(&driver, "Gradient Set"), "{:?}", driver.texts);
    driver.click_text("Dusk");
    driver.settle(2);
    tap(&mut driver, Key::Enter);
    driver.settle(4);
    assert_eq!(layers(&driver).len(), count + 1, "a correction layer");
    let mapped = seen_at(&seen(&driver), at);
    assert!(
        mapped[0] != mapped[1] || mapped[1] != mapped[2],
        "the gray line takes Dusk's colors: {mapped:?}"
    );
    run(&mut driver, "edit.undo");
    assert_eq!(layers(&driver).len(), count, "one undo step");
    assert_eq!(
        seen_at(&seen(&driver), at),
        gray,
        "undo shows it gray again"
    );
}

/// Tip 36 (08:46, CT-37): 3D Preview for Binding.
#[test]
fn t50_36_preview_for_binding() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("binding");
    run(&mut driver, "story.add");
    run(&mut driver, "story.add");
    assert!(!shows(&driver, "3D Preview for Binding"));
    run(&mut driver, "story.preview_binding");
    driver.settle(6);
    assert!(
        shows(&driver, "3D Preview for Binding"),
        "the preview opens: {:?}",
        driver.texts
    );
}

/// Tip 38 (09:19): a tool key held down borrows its tool (E, the
/// eraser) and lets go of it.
#[test]
fn t50_38_a_held_key_borrows_the_eraser() {
    let mut driver = open("held-e");
    let layer = ink_line(&mut driver);
    let line = painted(&driver, layer);
    let down = |pressed| Event::Key {
        key: Key::E,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    driver.step(vec![down(true)]);
    for _ in 0..40 {
        driver.step(vec![Event::Key {
            key: Key::E,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        }]);
    }
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-40.0, 0.0), m + vec2(40.0, 0.0));
    driver.step(vec![down(false)]);
    driver.settle(4);
    assert!(painted(&driver, layer) < line, "erased while held");
    stroke(&mut driver, m + vec2(-40.0, 40.0), m + vec2(40.0, 40.0));
    assert!(painted(&driver, layer) > 0, "the pen again");
}

/// Tip 39 (09:43, CTV-13): Mesh Transformation bends the layer.
#[test]
fn t50_39_mesh_transformation_bends_the_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("mesh");
    let layer = ink_line(&mut driver);
    let before = sue_paint::doc::raster(arena(&driver), layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    run(&mut driver, "edit.transform.mesh");
    driver.settle(4);
    assert!(shows(&driver, "Keep original image"), "{:?}", driver.texts);
    let m = middle(&driver);
    stroke(&mut driver, m, m + vec2(0.0, 40.0));
    tap(&mut driver, Key::Enter);
    let after = sue_paint::doc::raster(arena(&driver), layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    assert!(after != before, "bent");
}

/// Tip 40 (09:57): a frame border folder keeps the drawing inside.
#[test]
fn t50_40_a_frame_border_keeps_the_drawing_inside() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("frame");
    run(&mut driver, "layer.new.frame");
    tap(&mut driver, Key::Enter);
    driver.settle(4);
    assert!(
        layers(&driver).iter().any(|info| {
            arena(&driver)
                .get_by_id(info.id)
                .is_ok_and(|entity| entity.class() == sue_paint::manga::FRAME)
        }),
        "a frame border layer"
    );
    // A stroke from the canvas's edge to its middle, on a layer under
    // the frame border, shows only inside the frame.
    run(&mut driver, "layer.new");
    run(&mut driver, "layer.order.down");
    let frame = layers(&driver)
        .iter()
        .position(|info| {
            arena(&driver)
                .get_by_id(info.id)
                .is_ok_and(|entity| entity.class() == sue_paint::manga::FRAME)
        })
        .expect("the frame border layer");
    let drawn = layers(&driver)
        .iter()
        .position(|info| info.name == "Layer 2")
        .expect("the new layer");
    assert!(drawn < frame, "the new layer is under the frame border");
    pen(&mut driver);
    let m = middle(&driver);
    let [_, h] = size(&driver);
    let edge = screen_of(&driver, [2.0, h as f32 / 2.0]);
    stroke(&mut driver, edge, m);
    let tile = seen(&driver);
    let p = seen_at(&tile, [2, i64::from(h) / 2]);
    assert!(p[0] > 200, "outside the frame stays clear: {p:?}");
    let inside = seen_at(&tile, under(&driver, m - vec2(4.0, 0.0)));
    assert!(
        inside[0] < 100,
        "inside the frame the stroke shows: {inside:?}"
    );
}

/// Tip 41 (10:12, CTV-14): a balloon with text inside.
#[test]
fn t50_41_a_balloon_holds_text() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("balloon");
    let count = layers(&driver).len();
    run(&mut driver, "tool.pick.balloon");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-60.0, -40.0), m + vec2(60.0, 40.0));
    assert!(layers(&driver).len() > count, "a balloon layer");
}

/// Tip 42 (10:30, CT-36, CTV-15): Focus Lines and Speed Lines from the
/// Figure tool, on a new layer named after them.
#[test]
fn t50_42_focus_and_speed_lines() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("effect-lines");
    sub_tool(&mut driver, Key::U, "Straight Line", "Saturated Line");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, -60.0), m + vec2(80.0, 60.0));
    // The lines land on a new layer named after the sub tool (CTV-15).
    assert!(
        layers(&driver)
            .iter()
            .any(|info| info.name == "Saturated Line"),
        "{:?}",
        layers(&driver)
            .iter()
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
    );
}

/// Tip 45 (11:23, CT-32): a registered canvas preset keeps the
/// timelapse switch.
#[test]
fn t50_45_canvas_presets_keep_every_field() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("presets");
    press(&mut driver, Key::N, Modifiers::COMMAND);
    driver.settle(6);
    assert_eq!(ticked(&mut driver, "Record timelapse"), Some(false));
    driver.click_text("Record timelapse");
    driver.click_text("Register preset");
    let name = driver
        .texts
        .iter()
        .find(|(text, _)| text.ends_with("(1)"))
        .map(|(text, _)| text.clone())
        .expect("the new preset's name");
    driver.click_text("Cancel");
    press(&mut driver, Key::N, Modifiers::COMMAND);
    driver.settle(6);
    assert_eq!(ticked(&mut driver, "Record timelapse"), Some(false));
    click_widget(&mut driver, "Preset");
    driver.click_text(&name);
    assert_eq!(
        ticked(&mut driver, "Record timelapse"),
        Some(true),
        "the preset brought it back"
    );
}

/// Tip 46 (11:45): the New dialog's units include pt and paper sizes
/// such as A4.
#[test]
fn t50_46_units_and_paper_sizes_in_new() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("units");
    press(&mut driver, Key::N, Modifiers::COMMAND);
    driver.settle(6);
    let unit = driver
        .texts
        .iter()
        .find(|(text, _)| text == "px")
        .map(|(_, rect)| *rect)
        .expect("the unit list");
    click(&mut driver, unit.center());
    for unit in ["cm", "mm", "in", "pt"] {
        assert!(shows(&driver, unit), "{unit}: {:?}", driver.texts);
    }
    let pt = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "pt")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("pt in the list");
    click(&mut driver, pt.center());
    driver.settle(4);
    // At 72 dpi a point is a pixel; at 144 dpi the 1,600 pixel width is
    // 800 points.
    assert!(shows(&driver, " pt"), "the fields read in pt");
    assert!(shows(&driver, "1,600.00"), "{:?}", driver.texts);
    let dpi = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "72")
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the resolution field");
    click(&mut driver, dpi.center());
    type_over(&mut driver, "144");
    tap(&mut driver, Key::Tab);
    driver.settle(4);
    assert!(shows(&driver, "800.00"), "800 pt: {:?}", driver.texts);
    // A4 is among the paper sizes.
    let sizes = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Custom")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the size list");
    click(&mut driver, sizes.center());
    assert!(shows(&driver, "A4"), "A4: {:?}", driver.texts);
}

/// Tip 48 (12:23): Edit, Convert Brightness to Opacity.
#[test]
fn t50_48_convert_brightness_to_opacity() {
    let mut driver = open("luma-alpha");
    let layer = top(&driver);
    run(&mut driver, "edit.fill");
    tap(&mut driver, Key::X);
    run(&mut driver, "edit.fill");
    let before = bytes(&driver);
    run(&mut driver, "edit.luma_to_alpha");
    let m = middle(&driver);
    assert!(
        pixel(&driver, layer, under(&driver, m))[3] < 20,
        "white went clear"
    );
    undo_pair(&mut driver, &before);
}

/// Tip 49 (12:41): a column folds into icons with its arrow and opens
/// again; the Window menu hides a palette and brings it back.
#[test]
fn t50_49_palettes_fold_and_come_back() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("fold");
    assert!(shows(&driver, "Navigator"));
    let arrow = widgets(&mut driver)
        .into_iter()
        .filter(|(name, _)| name == "Fold this column into icons")
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the fold arrow");
    click(&mut driver, arrow.center());
    driver.settle(6);
    assert!(!shows(&driver, "Navigator"), "folded into icons");
    let names: Vec<(String, egui::Rect)> = widgets(&mut driver)
        .into_iter()
        .filter(|(_, rect)| (rect.center() - arrow.center()).length() < 40.0)
        .collect();
    let open_arrow = names
        .iter()
        .find(|(name, _)| name == "Open the palettes")
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the arrow that opens it: {names:?}"));
    click(&mut driver, open_arrow.center());
    driver.settle(6);
    assert!(
        shows(&driver, "Navigator"),
        "open again: {:?}",
        driver.texts
    );
    run(&mut driver, "window.navigator");
    driver.settle(4);
    assert!(!shows(&driver, "Navigator"), "hidden");
    run(&mut driver, "window.navigator");
    driver.settle(4);
    assert!(shows(&driver, "Navigator"), "back from the Window menu");
}

/// Tip 50 (13:04): Register Workspace, and switching back to it.
#[test]
fn t50_50_register_a_workspace_and_switch() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("workspace");
    run(&mut driver, "window.workspace.save");
    driver.settle(4);
    driver.click_text("Workspace 1");
    type_over(&mut driver, "Inking");
    driver.click_text("OK");
    driver.settle(4);
    driver.click_text("Window");
    driver.click_text("Workspace");
    driver.settle(4);
    assert!(shows(&driver, "Inking"), "{:?}", driver.texts);
}

/// The box round what `layer` holds: left, top, right, bottom.
fn ink_box(driver: &PaintDriver, layer: EntityId) -> Option<[i64; 4]> {
    let raster = sue_paint::doc::raster(arena(driver), layer)?;
    let width = raster.width() as i64;
    let mut found: Option<[i64; 4]> = None;
    for (index, pixel) in raster.bytes()?.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] < 128 {
            continue;
        }
        let (x, y) = (index as i64 % width, index as i64 / width);
        found = Some(match found {
            None => [x, y, x, y],
            Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
        });
    }
    found
}

/// Tip 27 (06:34, CT-23): a rectangle drawn with the Figure tool lies in
/// the perspective ruler's plane: its two sides are not as tall.
#[test]
fn t50_27_figures_follow_the_perspective_ruler() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("perspective-figure");
    run(&mut driver, "layer.ruler.perspective");
    tap(&mut driver, Key::Enter);
    driver.settle(4);
    run(&mut driver, "layer.new");
    let layer = top(&driver);
    sub_tool(&mut driver, Key::U, "Straight Line", "Rectangle");
    let m = middle(&driver);
    stroke(
        &mut driver,
        m + vec2(-260.0, -160.0),
        m + vec2(-100.0, -40.0),
    );
    let [left, _, right, _] = ink_box(&driver, layer).expect("a rectangle");
    let column = |x: i64| {
        let rows: Vec<i64> = (0..i64::from(size(&driver)[1]))
            .filter(|y| (x - 2..=x + 2).any(|cx| pixel(&driver, layer, [cx, *y])[3] > 128))
            .collect();
        rows.last().unwrap_or(&0) - rows.first().unwrap_or(&0)
    };
    let (l, r) = (column(left + 1), column(right - 1));
    assert!(
        l.abs_diff(r) > 6,
        "in perspective the sides differ: {l} {r}"
    );
}

/// Tip 31 (07:20, CT-29): the two screen color commands are the last of
/// the Edit menu, under Clip Studio Paint's names. Picking itself reads
/// the real screen (see the acceptance table).
#[test]
fn t50_31_the_edit_menu_has_both_screen_color_commands() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("screen-color");
    driver.click_text("Edit");
    driver.settle(4);
    let pick = driver
        .rect_of("Pick Screen Color...")
        .expect("Pick Screen Color");
    let hidden = driver
        .rect_of("Hide Windows and Pick Screen Color...")
        .expect("the hidden variant");
    assert!(hidden.top() > pick.top(), "in that order");
}

/// Tip 44 (11:07, CT-30): in Sub Tool Detail, the eye before a setting
/// hides it from Tool Property.
#[test]
fn t50_44_the_eye_in_sub_tool_detail_hides_a_row() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("detail-eye");
    pen(&mut driver);
    let property = |driver: &PaintDriver| {
        driver
            .texts
            .iter()
            .filter(|(t, r)| t == "Opacity" && r.left() < 360.0)
            .count()
    };
    let shown = property(&driver);
    assert!(shown > 0, "Tool Property shows Opacity");
    click_widget(
        &mut driver,
        "Sub Tool Detail: every setting, and which ones Tool Property shows",
    );
    driver.settle(4);
    let detail = widgets(&mut driver)
        .into_iter()
        .filter(|(name, _)| name == "Sub Tool Detail")
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("Sub Tool Detail");
    let ink = widgets(&mut driver)
        .into_iter()
        .find(|(name, rect)| name == "Ink" && detail.contains(rect.center()))
        .map(|(_, rect)| rect)
        .expect("Ink");
    click(&mut driver, ink.center());
    let opacity = driver
        .texts
        .iter()
        .filter(|(text, rect)| text == "Opacity" && detail.contains(rect.center()))
        .map(|(_, rect)| *rect)
        .next()
        .expect("Opacity in Sub Tool Detail");
    let eye = widgets(&mut driver)
        .into_iter()
        .filter(|(name, rect)| {
            name == "Hide in Tool Property" && rect.right() <= opacity.left() + 1.0
        })
        .map(|(_, rect)| rect)
        .min_by(|a, b| {
            (a.center().y - opacity.center().y)
                .abs()
                .total_cmp(&(b.center().y - opacity.center().y).abs())
        })
        .expect("the eye before Opacity");
    click(&mut driver, eye.center());
    driver.runtime.shell.dismiss_overlays();
    driver.settle(6);
    assert!(property(&driver) < shown, "Opacity left Tool Property");
}

/// Tip 47 (12:06): Lasso fill's Tool Property has opacity, anti-aliasing
/// and stabilization. (Clip Studio Paint's Blending mode row is missing:
/// a defect in the acceptance table.)
#[test]
fn t50_47_lasso_fill_tool_property() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("lasso-fill-rows");
    sub_tool(&mut driver, Key::G, "Close and fill", "Lasso fill");
    driver.click_text("Tool Property");
    driver.settle(4);
    for row in ["Opacity", "Anti-aliasing", "Stabilization"] {
        assert!(shows(&driver, row), "{row}: {:?}", driver.texts);
    }
}

// ----- 50 tips: the rows the picture added (V-n and L-n) -----------------

/// V-4 (03:05): the Eyedropper's Tool Property has rows of layers to
/// leave out. A defect: Sue's Eyedropper has the Refer row only.
#[test]
#[ignore = "V-4 defect: the Eyedropper has no Reference or Exclude rows (FGV-22)"]
fn v4_the_eyedropper_has_exclude_rows() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("eyedropper-rows");
    tap(&mut driver, Key::I);
    driver.click_text("Obtain Display Color");
    driver.click_text("Tool Property");
    driver.settle(4);
    assert!(
        driver
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("Leave out")),
        "{:?}",
        driver.texts
    );
}

/// V-7 (03:50): Export Timelapse offers the size, the length and the
/// aspect.
#[test]
fn v7_export_timelapse_offers_the_aspect() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("timelapse-aspect");
    run(&mut driver, "file.timelapse.record");
    ink_line(&mut driver);
    run(&mut driver, "file.timelapse.export");
    driver.settle(8);
    for row in ["Long side:", "Aspect ratio:", "Length:"] {
        assert!(shows(&driver, row), "{row}: {:?}", driver.texts);
    }
}

/// V-13 (09:43): Mesh Transformation's points across and down are set in
/// Tool Property, beside Keep original image.
#[test]
fn v13_mesh_transformation_has_its_point_counts() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open_fresh("mesh-points");
    ink_line(&mut driver);
    run(&mut driver, "edit.transform.mesh");
    driver.settle(4);
    for row in ["Across", "Down", "Keep original image"] {
        assert!(shows(&driver, row), "{row}: {:?}", driver.texts);
    }
}

/// V-14 (10:10): a balloon's Tool Property has its line color. A
/// defect: Sue's balloon has its line width and a hint only.
#[test]
#[ignore = "V-14 defect: the balloon has no Line color, Fill color or Toning rows (FGV-40)"]
fn v14_the_balloon_has_its_line_color() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("balloon-rows");
    run(&mut driver, "tool.pick.balloon");
    driver.click_text("Tool Property");
    driver.settle(4);
    assert!(shows(&driver, "Line color"), "{:?}", driver.texts);
}

/// V-15 (10:30): the Figure tool's effect lines sit in groups of their
/// own beside Direct draw.
#[test]
fn v15_the_figure_tool_has_its_line_groups() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("figure-groups");
    sub_tool(&mut driver, Key::U, "Straight Line", "Straight Line");
    for group in ["Direct draw", "Saturated Line", "Speed Line"] {
        assert!(shows(&driver, group), "{group}: {:?}", driver.texts);
    }
}

/// V-16 (08:06, CCS-6): the blending modes in Clip Studio Paint's order,
/// ending in Brightness.
#[test]
fn v16_the_blend_list_is_in_clip_studio_paints_order() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blend-order");
    click_widget(&mut driver, "Blending mode");
    let order = [
        "Normal",
        "Darken",
        "Multiply",
        "Color Burn",
        "Linear Burn",
        "Subtract",
        "Lighten",
        "Screen",
        "Color Dodge",
        "Glow Dodge",
        "Add",
        "Add (Glow)",
        "Overlay",
        "Soft Light",
        "Hard Light",
        "Difference",
        "Vivid Light",
        "Linear Light",
        "Pin Light",
        "Hard Mix",
        "Exclusion",
        "Darker Color",
        "Lighter Color",
        "Divide",
        "Hue",
        "Saturation",
        "Color",
        "Brightness",
    ];
    let tops: Vec<f32> = order
        .iter()
        .map(|name| {
            driver
                .texts
                .iter()
                .filter(|(text, rect)| text == name && rect.left() > 1100.0 && rect.top() > 480.0)
                .map(|(_, rect)| rect.top())
                .next()
                .unwrap_or_else(|| panic!("{name} in the list"))
        })
        .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}
