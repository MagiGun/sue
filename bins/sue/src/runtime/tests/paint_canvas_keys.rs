//! Sue Paint's canvas keys and its view, move and operation tools,
//! driven through real frames: the Modifier Key Settings and what they
//! say the keys, the wheel and the buttons do; a tool key held down;
//! Rotate's snap; Zoom's drag; Move layer with a selection and on a
//! folder; the Operation tool's sub tools; the Eyedropper on an empty
//! spot.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton};
use sue_paint::tools::Tool;

/// One frame with `events` while `held` are held (the keyboard's state,
/// as a window reports it beside its events); what it draws.
fn step_held(
    driver: &mut PaintDriver,
    events: Vec<Event>,
    held: Modifiers,
) -> Vec<(String, egui::Rect)> {
    let mut events = events;
    events.insert(0, Event::ModifiersChanged(held));
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let found = text_rects(&output);
    output.drop_without_applying_deltas();
    driver.texts.clone_from(&found);
    found
}

/// A press of `button` at `from` with `held`, a drag to `to` in ten
/// steps and the release; what the frames after draw.
fn drag_held(
    driver: &mut PaintDriver,
    from: Pos2,
    to: Pos2,
    button: PointerButton,
    held: Modifiers,
) -> Vec<(String, egui::Rect)> {
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: held,
    };
    step_held(driver, vec![Event::PointerMoved(from)], held);
    step_held(driver, vec![press(from, true)], held);
    for index in 1..=10 {
        let at = from + (to - from) * (index as f32 / 10.0);
        step_held(driver, vec![Event::PointerMoved(at)], held);
    }
    step_held(driver, vec![press(to, false)], held);
    for _ in 0..4 {
        step_held(driver, Vec::new(), Modifiers::NONE);
    }
    driver.texts.clone()
}

/// A turn of the mouse wheel by `delta` with `held`.
fn wheel(delta: egui::Vec2, held: Modifiers) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: held,
    }
}

/// A key coming down or up.
fn key_event(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// A key pressed and let go, as a person taps it.
fn tap(driver: &mut PaintDriver, key: Key) {
    driver.step(vec![key_event(key, true)]);
    driver.step(vec![key_event(key, false)]);
}

/// The open picture.
fn picture(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

/// The picture's layers, bottom first.
fn layers(driver: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture(driver))
}

/// The top layer.
fn top(driver: &PaintDriver) -> sue_core::EntityId {
    layers(driver).last().expect("a layer").id
}

/// Where `layer` sits.
fn offset(driver: &PaintDriver, layer: sue_core::EntityId) -> [i64; 2] {
    let entity = driver.runtime.engine.world.arena.get_by_id(layer).unwrap();
    [sue_model::picture::X, sue_model::picture::Y].map(|key| {
        entity
            .property(key)
            .and_then(sue_core::Value::as_i64)
            .unwrap_or(0)
    })
}

/// Picture pixel `p` of `layer` (where the layer sits), straight RGBA.
fn pixel(driver: &PaintDriver, layer: sue_core::EntityId, p: [i64; 2]) -> [u8; 4] {
    let [ox, oy] = offset(driver, layer);
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    let (x, y) = (p[0] - ox, p[1] - oy);
    if x < 0 || y < 0 || x >= i64::from(raster.width()) || y >= i64::from(raster.height()) {
        return [0; 4];
    }
    let at = ((y * i64::from(raster.width()) + x) * 4) as usize;
    let bytes = raster.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// How many pixels of `layer` have paint.
fn painted(driver: &PaintDriver, layer: sue_core::EntityId) -> usize {
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    raster
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count()
}

/// How much paint `layer` holds: its alpha added up.
fn ink(driver: &PaintDriver, layer: sue_core::EntityId) -> u64 {
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    raster
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| u64::from(pixel[3]))
        .sum()
}

/// The canvas point (on screen) of picture pixel `p`, found by asking
/// where screen points land: the middle of the canvas is the picture's
/// middle while the view has not moved.
fn canvas_middle(driver: &PaintDriver) -> Pos2 {
    paint_middle(&driver.runtime)
}

/// The window's title: it carries the zoom ("... 65.3%) - Sue Paint").
fn zoom(driver: &PaintDriver) -> f32 {
    let title = driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&driver.runtime.engine.world.arena))
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
}

/// The view's turn, as the Navigator shows it: the number under its
/// zoom.
fn rotation(texts: &[(String, egui::Rect)]) -> f32 {
    let navigator = texts
        .iter()
        .find(|(text, _)| text == "Navigator")
        .map(|(_, rect)| *rect)
        .expect("the Navigator shows");
    let mut numbers: Vec<(f32, egui::Rect)> = texts
        .iter()
        .filter(|(_, rect)| {
            rect.top() > navigator.bottom() && (rect.left() - navigator.left()).abs() < 80.0
        })
        .filter_map(|(text, rect)| Some((text.trim().parse::<f32>().ok()?, *rect)))
        .collect();
    numbers.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
    numbers
        .get(1)
        .map(|(value, _)| *value)
        .unwrap_or_else(|| panic!("the Navigator's numbers: {numbers:?}"))
}

/// Whether the Sub Tool palette lists `tool`'s first sub tool.
fn shows_tool(texts: &[(String, egui::Rect)], tool: Tool) -> bool {
    let name = tool.sub_tools()[0].name.clone();
    texts.iter().any(|(text, _)| *text == name)
}

/// A stroke of the current tool from `from` to `to` (on screen).
fn stroke(driver: &mut PaintDriver, from: Pos2, to: Pos2) {
    drag_held(driver, from, to, PointerButton::Primary, Modifiers::NONE);
}

/// Sue Paint's data folder in a test's home.
fn data(driver: &PaintDriver) -> PathBuf {
    driver.root.join("data").join("paint")
}

#[test]
fn modifier_key_settings_give_ctrl_alone_the_view_s_turn() {
    // The 2025 animation tutorial's step: File, Modifier Key Settings,
    // Ctrl changes to rotate, OK; then Ctrl and a drag turns the canvas.
    let mut driver = PaintDriver::new("paint-keys-dialog");
    let texts = step_held(
        &mut driver,
        vec![Event::Key {
            key: Key::Y,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT,
        }],
        Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT,
    );
    let _ = texts;
    let texts = driver.settle(4);
    for wanted in [
        "Modifier Key Settings",
        "General",
        "For One Tool",
        "Keys on the Canvas",
        "Mouse Wheel",
        "Mouse Buttons",
        "Ctrl",
        "Ctrl+Shift",
        "Shift+Alt+Space",
        "Change tool temporarily",
        "Change brush size",
        "Operation: Object",
        "Operation: Select layer",
        "Hand: Rotate",
        "Zoom: Zoom Out",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == wanted),
            "{wanted:?} in {texts:?}"
        );
    }
    // Ctrl's tool: from the Object tool to the Hand's Rotate.
    let ctrl_row = driver.rect_of("Operation: Object").unwrap();
    driver.click_text("Operation: Object");
    let rotate = driver
        .texts
        .iter()
        .filter(|(text, rect)| text == "Hand: Rotate" && rect.top() > ctrl_row.bottom())
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the list shows Hand: Rotate");
    driver.step(vec![Event::PointerMoved(rotate.center())]);
    for pressed in [true, false] {
        driver.step(vec![Event::PointerButton {
            pos: rotate.center(),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]);
    }
    driver.settle(4);
    // One tool's rows say what General does.
    driver.click_text("For One Tool");
    let texts = driver.settle(4);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text == "As in General: Hand: Rotate"),
        "{texts:?}"
    );
    driver.click_text("OK");
    driver.settle(4);
    let kept = std::fs::read_to_string(data(&driver).join("modifier_keys.txt"))
        .expect("the changed rows are kept in a file");
    assert!(
        kept.contains("general press ctrl = borrow hand 1"),
        "{kept}"
    );
    // Ctrl and a drag with the Pen turns the view and paints nothing.
    driver.command("layer.new");
    driver.settle(2);
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    let before = rotation(&driver.settle(2));
    let texts = drag_held(
        &mut driver,
        middle + vec2(150.0, 0.0),
        middle + vec2(0.0, 150.0),
        PointerButton::Primary,
        Modifiers::COMMAND,
    );
    let after = rotation(&texts);
    assert!((after - before).abs() > 30.0, "{before} to {after}");
    assert_eq!(painted(&driver, layer), 0, "Ctrl turned, it did not paint");
}

#[test]
fn ctrl_borrows_the_object_tool_and_moves_a_raster_layer() {
    // Ctrl held with a drawing tool: Object (Move Layer's work on a
    // raster layer), one undo step; then the Pen again.
    let mut driver = PaintDriver::new("paint-keys-ctrl");
    driver.command("layer.new");
    tap(&mut driver, Key::P);
    driver.settle(2);
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    stroke(
        &mut driver,
        middle + vec2(-40.0, 0.0),
        middle + vec2(40.0, 0.0),
    );
    let ink = painted(&driver, layer);
    assert!(ink > 0);
    assert_eq!(offset(&driver, layer), [0, 0]);
    drag_held(
        &mut driver,
        middle,
        middle + vec2(30.0, 30.0),
        PointerButton::Primary,
        Modifiers::COMMAND,
    );
    let moved = offset(&driver, layer);
    assert!(moved[0] > 0 && moved[1] > 0, "{moved:?}");
    assert_eq!(painted(&driver, layer), ink, "nothing more painted");
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(offset(&driver, layer), [0, 0], "undo puts it back");
    driver.command("edit.redo");
    driver.settle(2);
    assert_eq!(offset(&driver, layer), moved);
    // Without Ctrl the Pen paints again.
    stroke(
        &mut driver,
        middle + vec2(-40.0, 60.0),
        middle + vec2(40.0, 60.0),
    );
    assert!(painted(&driver, layer) > ink);
}

#[test]
fn alt_picks_a_color_with_fill_and_a_right_click_picks_too() {
    // Alt and a click with the Fill tool picks a color (the crash course's
    // caption); a right-click picks the color shown.
    let mut driver = PaintDriver::new("paint-keys-alt");
    driver.command("layer.new");
    tap(&mut driver, Key::P);
    driver.settle(2);
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    // A black line, from the Pen's black.
    stroke(
        &mut driver,
        middle + vec2(-60.0, 0.0),
        middle + vec2(60.0, 0.0),
    );
    let size = |driver: &PaintDriver| {
        sue_model::picture::size(
            driver
                .runtime
                .engine
                .world
                .arena
                .get_by_id(picture(driver))
                .unwrap(),
        )
        .unwrap()
    };
    let [width, height] = size(&driver);
    let center = [i64::from(width / 2), i64::from(height / 2)];
    // The Pencil catches the paper's grain (0.0.8), so the middle pixel
    // itself may be a valley the lead skipped: the line is there when
    // pixels about it hold graphite.
    let near = (-4..=4).flat_map(|dy| (-12..=12).map(move |dx| [dx, dy]));
    assert!(
        near.filter(|[dx, dy]| pixel(&driver, layer, [center[0] + dx, center[1] + dy])[3] > 0)
            .count()
            > 40,
        "the line is there"
    );
    // The Fill tool, Alt and a click on the white paper: white.
    driver.command("tool.fill");
    driver.settle(2);
    let paper = middle + vec2(0.0, -90.0);
    drag_held(
        &mut driver,
        paper,
        paper,
        PointerButton::Primary,
        Modifiers::ALT,
    );
    tap(&mut driver, Key::P);
    driver.settle(2);
    let low = middle + vec2(0.0, 80.0);
    stroke(&mut driver, low + vec2(-30.0, 0.0), low + vec2(30.0, 0.0));
    let white = (0..height)
        .rev()
        .flat_map(|y| (0..width).map(move |x| [i64::from(x), i64::from(y)]))
        .map(|p| pixel(&driver, layer, p))
        .find(|pixel| pixel[3] == 255 && pixel[0] > 240)
        .is_some();
    assert!(white, "the Fill tool's Alt picked the paper's white");
    // A right-click on the black line picks black again.
    drag_held(
        &mut driver,
        middle,
        middle,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    let lower = middle + vec2(0.0, 120.0);
    stroke(
        &mut driver,
        lower + vec2(-30.0, 0.0),
        lower + vec2(30.0, 0.0),
    );
    let dark = (0..height)
        .rev()
        .flat_map(|y| (0..width).map(move |x| [i64::from(x), i64::from(y)]))
        .map(|p| pixel(&driver, layer, p))
        .find(|pixel| pixel[3] == 255)
        .expect("the last line is there");
    assert!(
        dark[0] < 128,
        "the right-click picked the dark line: {dark:?}"
    );
}

#[test]
fn a_file_of_modifier_keys_is_read_and_its_rows_win() {
    // The wheel scrolls instead of zooming and a right-click does
    // nothing, as a shared file says.
    let mut driver = PaintDriver::new("paint-keys-file");
    std::fs::write(
        data(&driver).join("modifier_keys.txt"),
        "# shared\ngeneral wheel none = scroll\ngeneral right none = nothing\nbad line\n",
    )
    .unwrap();
    let middle = canvas_middle(&driver);
    let before = zoom(&driver);
    step_held(
        &mut driver,
        vec![Event::PointerMoved(middle)],
        Modifiers::NONE,
    );
    for _ in 0..3 {
        step_held(
            &mut driver,
            vec![wheel(vec2(0.0, 60.0), Modifiers::NONE)],
            Modifiers::NONE,
        );
    }
    driver.settle(30);
    assert!(
        (zoom(&driver) - before).abs() < 0.05,
        "the wheel scrolled: {before} to {}",
        zoom(&driver)
    );
    // The line that could not be read is said.
    assert!(
        driver
            .runtime
            .shell
            .message()
            .is_some_and(|(text, _)| text.contains("modifier_keys.txt")),
        "{:?}",
        driver.runtime.shell.message()
    );
    // Shift and the wheel still turn (General's own row).
    let turn = rotation(&driver.settle(2));
    step_held(
        &mut driver,
        vec![wheel(vec2(0.0, 120.0), Modifiers::SHIFT)],
        Modifiers::SHIFT,
    );
    let turned = rotation(&driver.settle(30));
    assert!((turned - turn).abs() > 1.0, "{turn} to {turned}");
}

#[test]
fn the_wheel_zooms_by_sue_s_own_row() {
    let mut driver = PaintDriver::new("paint-keys-wheel");
    let middle = canvas_middle(&driver);
    let before = zoom(&driver);
    step_held(
        &mut driver,
        vec![Event::PointerMoved(middle)],
        Modifiers::NONE,
    );
    for _ in 0..3 {
        step_held(
            &mut driver,
            vec![wheel(vec2(0.0, 60.0), Modifiers::NONE)],
            Modifiers::NONE,
        );
    }
    driver.settle(30);
    assert!(
        zoom(&driver) > before + 1.0,
        "{before} to {}",
        zoom(&driver)
    );
}

#[test]
fn a_tool_key_held_down_borrows_its_tool_and_a_tap_keeps_it() {
    let mut driver = PaintDriver::new("paint-keys-held");
    driver.command("layer.new");
    let texts = driver.settle(4);
    assert!(
        shows_tool(&texts, Tool::Pen),
        "a picture starts with the Pen"
    );
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    stroke(
        &mut driver,
        middle + vec2(-60.0, 0.0),
        middle + vec2(60.0, 0.0),
    );
    let line = ink(&driver, layer);
    // E held: the Eraser while it is down, used on the line.
    driver.step(vec![key_event(Key::E, true)]);
    let texts = driver.settle(2);
    assert!(shows_tool(&texts, Tool::Eraser), "{texts:?}");
    stroke(
        &mut driver,
        middle + vec2(-20.0, 0.0),
        middle + vec2(20.0, 0.0),
    );
    assert!(ink(&driver, layer) < line, "the Eraser erased");
    let texts = driver.settle(2);
    assert!(
        shows_tool(&texts, Tool::Eraser),
        "still the Eraser while E is down"
    );
    driver.step(vec![key_event(Key::E, false)]);
    let texts = driver.settle(4);
    assert!(
        shows_tool(&texts, Tool::Pen),
        "E up: the Pen is back: {texts:?}"
    );
    // A quick tap of E picks the Eraser for good.
    driver.step(vec![key_event(Key::E, true)]);
    driver.step(vec![key_event(Key::E, false)]);
    let texts = driver.settle(40);
    assert!(shows_tool(&texts, Tool::Eraser), "{texts:?}");
}

#[test]
fn shift_snaps_the_rotate_tool_to_45_degrees() {
    let mut driver = PaintDriver::new("paint-keys-rotate");
    // R: the Hand's Rotate sub tool.
    tap(&mut driver, Key::R);
    driver.settle(2);
    let middle = canvas_middle(&driver);
    let texts = drag_held(
        &mut driver,
        middle + vec2(150.0, 0.0),
        middle + vec2(110.0, 110.0),
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    let snapped = rotation(&texts);
    assert!(snapped.abs() > 1.0, "it turned: {snapped}");
    assert!(
        (snapped / 45.0 - (snapped / 45.0).round()).abs() < 0.01,
        "a step of 45: {snapped}"
    );
    // Without Shift it turns freely.
    let texts = drag_held(
        &mut driver,
        middle + vec2(150.0, 0.0),
        middle + vec2(140.0, 30.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let free = rotation(&texts);
    assert!(
        (free / 45.0 - (free / 45.0).round()).abs() > 0.01,
        "free: {free}"
    );
}

#[test]
fn the_zoom_tool_zooms_as_it_drags_and_by_half_again_on_a_click() {
    let mut driver = PaintDriver::new("paint-keys-zoom");
    driver.command("tool.zoom");
    driver.settle(2);
    let middle = canvas_middle(&driver);
    let before = zoom(&driver);
    drag_held(
        &mut driver,
        middle,
        middle + vec2(140.0, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let dragged = zoom(&driver);
    assert!(
        (dragged / before - 2.0).abs() < 0.1,
        "dragged across: {before} to {dragged}"
    );
    drag_held(
        &mut driver,
        middle,
        middle,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let clicked = zoom(&driver);
    assert!(
        (clicked / dragged - 1.5).abs() < 0.05,
        "a click: {dragged} to {clicked}"
    );
    // Alt and a click zooms out.
    drag_held(
        &mut driver,
        middle,
        middle,
        PointerButton::Primary,
        Modifiers::ALT,
    );
    assert!((zoom(&driver) - dragged).abs() < 0.2, "{}", zoom(&driver));
}

#[test]
fn move_layer_moves_what_is_selected_and_a_folder_whole() {
    let mut driver = PaintDriver::new("paint-keys-move");
    driver.command("layer.new");
    tap(&mut driver, Key::P);
    driver.settle(2);
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    stroke(
        &mut driver,
        middle + vec2(-80.0, 0.0),
        middle + vec2(80.0, 0.0),
    );
    let line = painted(&driver, layer);
    // A rectangle around the left half of the line.
    tap(&mut driver, Key::M);
    driver.settle(2);
    drag_held(
        &mut driver,
        middle + vec2(-120.0, -30.0),
        middle + vec2(-5.0, 30.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let left_before = painted_where(&driver, layer, |x, _| x < 0);
    let right_before = painted_where(&driver, layer, |x, _| x > 0);
    // Move layer (K) drags it down: only the left half goes.
    tap(&mut driver, Key::K);
    driver.settle(2);
    drag_held(
        &mut driver,
        middle + vec2(-60.0, 0.0),
        middle + vec2(-60.0, 60.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(
        painted_where(&driver, layer, |x, _| x > 0),
        right_before,
        "the right half stays"
    );
    assert!(
        painted_where(&driver, layer, |x, y| x < 0 && y > 10) > left_before / 2,
        "the left half went down"
    );
    assert_eq!(offset(&driver, layer), [0, 0], "the layer itself stays");
    let history = &driver.runtime.engine.history;
    assert_eq!(
        history.label(history.cursor()),
        Some("Move Layer"),
        "the undo step is named for the move, not a transform"
    );
    // The tool is still Move layer, not a transform left open.
    let texts = driver.settle(2);
    assert!(
        !texts.iter().any(|(text, _)| text == "Scale/Rotate"),
        "no transform bar is left: {texts:?}"
    );
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(painted(&driver, layer), line, "undo puts it back");
    driver.command("select.none");
    driver.settle(2);
    // The layer in a folder, the folder current: the drag moves it.
    driver.command("layer.folder_insert");
    driver.settle(2);
    let folder = layers(&driver)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(&driver.runtime.engine.world.arena, info.id))
        .expect("a folder")
        .id;
    assert_eq!(
        sue_paint::doc::members(&driver.runtime.engine.world.arena, picture(&driver), folder),
        vec![layer]
    );
    drag_held(
        &mut driver,
        middle,
        middle + vec2(40.0, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(
        offset(&driver, layer)[0] > 0,
        "{:?}",
        offset(&driver, layer)
    );
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(offset(&driver, layer), [0, 0]);
}

/// How many pixels of `layer` have paint where `keep` says yes, given
/// each pixel's place from the picture's middle.
fn painted_where(
    driver: &PaintDriver,
    layer: sue_core::EntityId,
    keep: impl Fn(i64, i64) -> bool,
) -> usize {
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    let (w, h) = (i64::from(raster.width()), i64::from(raster.height()));
    let bytes = raster.bytes().unwrap();
    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            if bytes[((y * w + x) * 4 + 3) as usize] > 0 && keep(x - w / 2, y - h / 2) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn select_layer_picks_every_layer_painted_in_its_box() {
    // Two layers painted side by side; Select layer's box over both picks
    // them, so Merge Selected Layers makes one of them.
    let mut driver = PaintDriver::new("paint-keys-select-layer");
    let middle = canvas_middle(&driver);
    tap(&mut driver, Key::P);
    for x in [-60.0, 60.0] {
        driver.command("layer.new");
        driver.settle(2);
        stroke(
            &mut driver,
            middle + vec2(x - 20.0, 0.0),
            middle + vec2(x + 20.0, 0.0),
        );
    }
    let count = layers(&driver).len();
    tap(&mut driver, Key::O);
    driver.settle(2);
    driver.click_text("Select layer");
    drag_held(
        &mut driver,
        middle + vec2(-100.0, -30.0),
        middle + vec2(100.0, 30.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    driver.command("layer.merge_selected");
    driver.settle(4);
    assert_eq!(layers(&driver).len(), count - 1, "the two became one");
}

#[test]
fn edit_timeline_steps_the_frames_and_light_table_drags_its_layers() {
    let mut driver = PaintDriver::new("paint-keys-timeline");
    let middle = canvas_middle(&driver);
    // No timeline yet: Edit timeline says what it needs.
    tap(&mut driver, Key::O);
    driver.settle(2);
    driver.click_text("Edit timeline");
    drag_held(
        &mut driver,
        middle,
        middle,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(
        driver
            .runtime
            .shell
            .message()
            .is_some_and(|(text, _)| text.contains("timeline")),
        "{:?}",
        driver.runtime.shell.message()
    );
    // A reference layer on the general light table, and a timeline with
    // a cel at frame 1 and another from frame 4.
    driver.command("layer.new");
    driver.settle(2);
    let reference = top(&driver);
    driver.new_timeline();
    let ids = |driver: &PaintDriver| -> Vec<sue_core::EntityId> {
        layers(driver).into_iter().map(|info| info.id).collect()
    };
    let known = ids(&driver);
    driver.command("anim.cel.new");
    driver.settle(2);
    let first = *ids(&driver)
        .iter()
        .find(|id| !known.contains(id))
        .expect("a new cel");
    for _ in 0..3 {
        driver.command("anim.next");
    }
    let known = ids(&driver);
    driver.command("anim.cel.new");
    driver.settle(2);
    let fourth = *ids(&driver)
        .iter()
        .find(|id| !known.contains(id))
        .expect("a new cel");
    assert_ne!(first, fourth);
    driver.command("anim.first");
    driver.settle(2);
    // Edit timeline: three clicks step on to frame 4, whose cel the Pen
    // then draws on.
    tap(&mut driver, Key::O);
    driver.settle(2);
    for _ in 0..3 {
        drag_held(
            &mut driver,
            middle,
            middle,
            PointerButton::Primary,
            Modifiers::NONE,
        );
    }
    tap(&mut driver, Key::P);
    driver.settle(2);
    stroke(
        &mut driver,
        middle + vec2(-40.0, 0.0),
        middle + vec2(40.0, 0.0),
    );
    assert!(ink(&driver, fourth) > 0, "frame 4's cel is drawn on");
    assert_eq!(ink(&driver, first), 0, "frame 1's cel is not");
    // A drag to the left scrubs back to frame 1.
    tap(&mut driver, Key::O);
    driver.settle(2);
    drag_held(
        &mut driver,
        middle,
        middle + vec2(-200.0, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    tap(&mut driver, Key::P);
    driver.settle(2);
    stroke(
        &mut driver,
        middle + vec2(-40.0, 40.0),
        middle + vec2(40.0, 40.0),
    );
    assert!(ink(&driver, first) > 0, "back on frame 1's cel");
    // Light table: the reference layer on the general light table moves
    // with a drag, one undo step.
    driver.command("anim.first");
    driver.settle(2);
    driver.command("layer.select_below");
    driver.command("anim.light_table.register_general");
    driver.settle(2);
    let general =
        sue_paint::anim::general_light_table(&driver.runtime.engine.world.arena, picture(&driver));
    assert_eq!(general, vec![reference]);
    tap(&mut driver, Key::O);
    driver.settle(2);
    driver.click_text("Light table");
    let depth = driver.runtime.engine.history.depth();
    drag_held(
        &mut driver,
        middle,
        middle + vec2(50.0, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let place = sue_paint::anim::place_of(
        driver
            .runtime
            .engine
            .world
            .arena
            .get_by_id(reference)
            .unwrap(),
    );
    assert!(place[0] > 5.0, "moved: {place:?}");
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1);
}

#[test]
fn obtain_layer_color_on_an_empty_spot_picks_the_transparent_color() {
    // The Eyedropper's Obtain Layer Color clicked where the layer has no
    // paint: the Pen then draws with the transparent color, so it erases.
    let mut driver = PaintDriver::new("paint-keys-eyedropper");
    driver.command("layer.new");
    tap(&mut driver, Key::P);
    driver.settle(2);
    let layer = top(&driver);
    let middle = canvas_middle(&driver);
    stroke(
        &mut driver,
        middle + vec2(-60.0, 0.0),
        middle + vec2(60.0, 0.0),
    );
    let before = ink(&driver, layer);
    tap(&mut driver, Key::I);
    driver.settle(2);
    driver.click_text("Obtain Layer Color");
    let empty = middle + vec2(0.0, -100.0);
    drag_held(
        &mut driver,
        empty,
        empty,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    tap(&mut driver, Key::P);
    driver.settle(2);
    stroke(
        &mut driver,
        middle + vec2(-20.0, 0.0),
        middle + vec2(20.0, 0.0),
    );
    assert!(ink(&driver, layer) < before, "the transparent color erased");
}
