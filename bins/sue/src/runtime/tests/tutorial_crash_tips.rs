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

/// A click on the widget named `name` with the secondary button.
fn right_click_widget(driver: &mut PaintDriver, name: &str) {
    let rect = widget(driver, name).unwrap_or_else(|| panic!("{name:?} is on screen"));
    click_held(
        driver,
        rect.center(),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
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

/// How deep the history is.
fn depth(driver: &PaintDriver) -> usize {
    driver.runtime.engine.history.depth()
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
    assert!(shows(&driver, "Round watercolor brush"), "{:?}", driver.texts);
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
    assert!(bigger > before && smaller < before, "{before} {bigger} {smaller}");
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

/// CC 04:37: File, Pen Pressure Settings: draw to adjust, with a graph.
#[test]
fn cc_0437_pen_pressure_settings_adjust_by_drawing() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("pressure-dialog");
    run(&mut driver, "file.pressure");
    driver.settle(6);
    assert!(shows(&driver, "Adjust pen pressure"), "{:?}", driver.texts);
    assert!(shows(&driver, "Check adjusted settings"));
    assert!(shows(&driver, "Show pen pressure graph"));
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
    assert!(i32::from(p[2]) > i32::from(p[0]) + 40, "the line shows blue: {p:?}");
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
