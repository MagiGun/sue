//! Sue Paint: what is dragged on the canvas without being a stroke, run
//! in the running app: the Selection launcher and its settings, the
//! transforms (their modes, the rotation centre, Ctrl on a corner, Mesh
//! Transformation's grid, several layers at once), puppet warp's pins, the
//! camera frame and the light table's box.

use super::*;
use egui::pos2;

/// What a frame drew of Sue Paint's launchers: each button's id and rect.
fn buttons(driver: &PaintDriver) -> Vec<(String, egui::Rect)> {
    sue_paint::shown_buttons(&driver.ctx)
}

/// A click at `at`, then frames until things settle.
fn click(driver: &mut PaintDriver, at: Pos2, modifiers: egui::Modifiers) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(4);
}

/// A drag from `from` to `to` holding `modifiers`.
fn drag_with(driver: &mut PaintDriver, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let held = |events: Vec<egui::Event>| {
        let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
        all.extend(events);
        all
    };
    driver.step(held(vec![egui::Event::PointerMoved(from)]));
    driver.step(held(vec![press(from, true)]));
    for index in 1..=16 {
        let at = from + (to - from) * (index as f32 / 16.0);
        driver.step(held(vec![egui::Event::PointerMoved(at)]));
    }
    driver.step(held(vec![press(to, false)]));
    driver.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    driver.settle(6);
}

/// Clicks the launcher button that runs `id`.
fn click_button(driver: &mut PaintDriver, id: &str) {
    let at = buttons(driver)
        .into_iter()
        .find(|(found, _)| found == id)
        .unwrap_or_else(|| panic!("{id} is on a launcher: {:?}", buttons(driver)))
        .1
        .center();
    click(driver, at, egui::Modifiers::NONE);
}

fn picture(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

fn arena(driver: &PaintDriver) -> &sue_core::Arena {
    &driver.runtime.engine.world.arena
}

/// The whole arena as bytes: entities, ids, components, properties and
/// relations.
fn bytes(driver: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    arena(driver).encode(&mut out);
    out.into_bytes()
}

fn depth(driver: &PaintDriver) -> usize {
    driver.runtime.engine.history.depth()
}

fn layers(driver: &PaintDriver) -> Vec<sue_core::EntityId> {
    sue_paint::doc::layers(arena(driver), picture(driver))
        .into_iter()
        .map(|info| info.id)
        .collect()
}

/// The current layer: the topmost one made last.
fn top(driver: &PaintDriver) -> sue_core::EntityId {
    *layers(driver).last().expect("a layer")
}

/// Where `layer` has drawing, in its own pixels.
fn drawn(driver: &PaintDriver, layer: sue_core::EntityId) -> Option<[u32; 4]> {
    let raster = sue_paint::doc::raster(arena(driver), layer)?;
    sue_paint::warp::bounds(raster.bytes()?, raster.width(), raster.height())
}

/// The alpha of `layer` at `(x, y)`.
fn alpha(driver: &PaintDriver, layer: sue_core::EntityId, x: u32, y: u32) -> u8 {
    let raster = sue_paint::doc::raster(arena(driver), layer).unwrap();
    raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize]
}

/// A filled box on the current layer: a rectangle selection from `from`
/// to `to` (screen) filled with the drawing color. The selection stays.
fn filled_box(driver: &mut PaintDriver, from: Pos2, to: Pos2) {
    driver.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    driver.drag(from, to, egui::PointerButton::Primary);
    driver.command("edit.fill");
    driver.settle(4);
}

/// Runs `work`, which changes the document, as one undo step: undo brings
/// back the arena as it was, ids included, and redo the arena as `work`
/// left it.
fn undo_pair(driver: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (bytes(driver), depth(driver));
    work(driver);
    driver.settle(4);
    let after = bytes(driver);
    assert_ne!(after, before, "{what} changes the document");
    assert_eq!(depth(driver), steps + 1, "{what} is one undo step");
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(bytes(driver), before, "{what} undone");
    driver.command("edit.redo");
    driver.settle(2);
    assert_eq!(bytes(driver), after, "{what} redone");
}

#[test]
fn sue_paint_selection_launcher_buttons_are_arranged_and_kept() {
    // WS-12, CT-21, FG-12: the launcher's set in Clip Studio Paint's
    // order, its settings, buttons added, the default back, all kept.
    let mut driver = PaintDriver::new("paint-launcher-set");
    let middle = paint_middle(&driver.runtime);
    filled_box(
        &mut driver,
        middle - vec2(60.0, 40.0),
        middle + vec2(60.0, 40.0),
    );
    let ids: Vec<String> = buttons(&driver).into_iter().map(|(id, _)| id).collect();
    assert_eq!(
        ids,
        [
            "select.none",
            "edit.crop",
            "select.invert",
            "select.expand",
            "select.shrink",
            "edit.clear",
            "edit.clear_outside",
            "edit.cut_paste",
            "edit.copy_paste",
            "edit.transform.scale",
            "edit.fill",
            "layer.new.tone",
            "launcher.settings",
        ]
    );
    // The buttons sit in a row under the selection.
    let shown = buttons(&driver);
    assert!(
        shown
            .windows(2)
            .all(|pair| pair[0].1.left() < pair[1].1.left())
    );
    assert!(shown[0].1.top() > middle.y + 40.0, "under the selection");

    // Settings: a command found by its name goes on the launcher, last
    // before Settings, and the file keeps it.
    click_button(&mut driver, "launcher.settings");
    assert!(
        driver.rect_of("Selection Launcher Settings").is_some(),
        "the settings show: {:?}",
        driver.texts
    );
    let search = driver
        .rect_of("Add a command")
        .expect("the search")
        .center();
    click(&mut driver, search, egui::Modifiers::NONE);
    driver.step(vec![egui::Event::Text("Flip Hor".to_owned())]);
    driver.settle(4);
    driver.click_text("Flip Horizontal");
    let ids: Vec<String> = buttons(&driver).into_iter().map(|(id, _)| id).collect();
    assert_eq!(
        ids[ids.len() - 2..],
        ["edit.flip_h".to_owned(), "launcher.settings".to_owned()],
        "{ids:?}"
    );
    let file = driver
        .root
        .join("data")
        .join("paint")
        .join("selection_launcher.txt");
    let kept = std::fs::read_to_string(&file).expect("the launcher's file");
    assert!(kept.lines().any(|line| line == "edit.flip_h"), "{kept}");
    // A separator goes in as a line of its own.
    driver.click_text("Add Separator");
    let kept = std::fs::read_to_string(&file).unwrap();
    assert_eq!(kept.lines().last(), Some("-"));
    // Restore Default puts the shipped set back.
    driver.click_text("Restore Default");
    let kept = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        kept.lines().filter(|line| !line.starts_with('#')).count(),
        12
    );
    assert!(!kept.contains("edit.flip_h"));
    let _ = arena(&driver);
}

#[test]
fn sue_paint_cut_and_copy_and_paste_make_a_new_layer_in_one_step() {
    // FG-12, CT-21: the launcher's Cut and Paste and Copy and Paste.
    let mut driver = PaintDriver::new("paint-cut-paste");
    let middle = paint_middle(&driver.runtime);
    filled_box(
        &mut driver,
        middle - vec2(50.0, 30.0),
        middle + vec2(50.0, 30.0),
    );
    let first = top(&driver);
    let [x0, y0, x1, y1] = drawn(&driver, first).expect("the box is drawn");
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    let count = layers(&driver).len();
    undo_pair(&mut driver, "Copy and Paste", |driver| {
        click_button(driver, "edit.copy_paste");
    });
    assert_eq!(layers(&driver).len(), count + 1, "a new layer");
    let copy = top(&driver);
    assert_ne!(copy, first);
    assert_eq!(
        drawn(&driver, copy),
        Some([x0, y0, x1, y1]),
        "in the same place"
    );
    assert!(alpha(&driver, first, cx, cy) > 0, "the original stays");

    // Cut and Paste: the original is cleared under the selection.
    let mut driver = PaintDriver::new("paint-cut-paste-2");
    filled_box(
        &mut driver,
        middle - vec2(50.0, 30.0),
        middle + vec2(50.0, 30.0),
    );
    let first = top(&driver);
    let count = layers(&driver).len();
    undo_pair(&mut driver, "Cut and Paste", |driver| {
        click_button(driver, "edit.cut_paste");
    });
    assert_eq!(layers(&driver).len(), count + 1);
    assert_eq!(drawn(&driver, first), None, "the original is cut");
    assert!(alpha(&driver, top(&driver), cx, cy) > 0, "the copy has it");
}

/// Starts a transform with `command` on a filled box around the canvas's
/// middle, the selection gone: the box's rect on the layer and on screen.
fn box_to_transform(driver: &mut PaintDriver, command: &str) -> ([u32; 4], Pos2, Pos2) {
    let middle = paint_middle(&driver.runtime);
    let (from, to) = (middle - vec2(60.0, 30.0), middle + vec2(60.0, 30.0));
    filled_box(driver, from, to);
    driver.command("select.none");
    driver.settle(2);
    let rect = drawn(driver, top(driver)).expect("the box is drawn");
    driver.command(command);
    driver.settle(4);
    (rect, from, to)
}

#[test]
fn sue_paint_scale_rotate_turns_around_a_moved_centre() {
    // TR-21: the rotation centre is dragged to the box's left end, and a
    // turn outside the box swings the box around it.
    let mut driver = PaintDriver::new("paint-transform-pivot");
    let (rect, from, to) = box_to_transform(&mut driver, "edit.transform.scale");
    let middle = pos2((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    let left = pos2(from.x, middle.y);
    // The centre's cross from the middle to the left edge's middle (which
    // is also the edge's handle: the centre is taken first).
    drag_with(
        &mut driver,
        middle,
        left + vec2(2.0, 0.0),
        egui::Modifiers::NONE,
    );
    // Outside the right end, turned a quarter down around it.
    let right = pos2(to.x + 40.0, middle.y);
    let down = pos2(left.x, middle.y + (right.x - left.x));
    drag_with(&mut driver, right, down, egui::Modifiers::SHIFT);
    let steps = depth(&driver);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert_eq!(depth(&driver), steps + 1, "one undo step");
    let [x0, y0, x1, y1] = drawn(&driver, top(&driver)).expect("still drawn");
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    assert!(y1 - y0 > x1 - x0, "upright now: {:?}", [x0, y0, x1, y1]);
    assert!((x1 - x0).abs_diff(h) <= 3 && (y1 - y0).abs_diff(w) <= 3);
    assert!(x0.abs_diff(rect[0]) <= h, "it stayed at the left end");
    assert!(y0 >= rect[1], "and swung down, around the moved centre");
}

#[test]
fn sue_paint_transform_modes_and_ctrl_distort() {
    // FG-13, FGV-49: the modes are named in the bar, Skew slides an edge,
    // Ctrl on a corner of Scale/Rotate moves that corner alone.
    let mut driver = PaintDriver::new("paint-transform-modes");
    let (rect, from, _) = box_to_transform(&mut driver, "edit.transform.skew");
    assert!(driver.rect_of("Skew").is_some(), "the bar names the mode");
    for name in ["Keep original image", "OK", "Cancel", "Reset"] {
        assert!(driver.rect_of(name).is_some(), "{name} is on the bar");
    }
    // The top left corner dragged right: the top edge slides, the bottom
    // stays.
    drag_with(
        &mut driver,
        from,
        from + vec2(40.0, 0.0),
        egui::Modifiers::NONE,
    );
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let layer = top(&driver);
    let skewed = drawn(&driver, layer).unwrap();
    assert!(
        skewed[2] > rect[2] + 5,
        "the top went right: {skewed:?} from {rect:?}"
    );
    assert_eq!(
        alpha(&driver, layer, rect[0] + 1, rect[3] - 2),
        255,
        "the bottom left stays"
    );
    assert_eq!(
        alpha(&driver, layer, rect[0] + 1, rect[1] + 1),
        0,
        "the top left left"
    );

    // Scale/Rotate with Ctrl on the bottom right corner: a distortion.
    let mut driver = PaintDriver::new("paint-transform-ctrl");
    let (rect, _, to) = box_to_transform(&mut driver, "edit.transform.scale");
    assert!(driver.rect_of("Scale/Rotate").is_some());
    assert!(driver.rect_of("Keep aspect ratio").is_some());
    undo_pair(&mut driver, "Ctrl distort", |driver| {
        drag_with(driver, to, to + vec2(50.0, 40.0), egui::Modifiers::COMMAND);
        driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let layer = top(&driver);
    let bent = drawn(&driver, layer).unwrap();
    assert!(bent[2] > rect[2] + 10 && bent[3] > rect[3] + 10, "{bent:?}");
    assert_eq!(
        alpha(&driver, layer, rect[0] + 1, rect[1] + 1),
        255,
        "the top left stays"
    );
    assert_eq!(
        alpha(&driver, layer, rect[2] + 2, rect[1] + 1),
        0,
        "the top right corner stays where it was"
    );
}

#[test]
fn sue_paint_mesh_transformation_bends_by_a_lattice_point() {
    // CTV-13, FG-13: Mesh Transformation lays a lattice (its points across
    // and down in the bar), a point bends what is near it, OK and Cancel.
    let mut driver = PaintDriver::new("paint-mesh");
    let (rect, from, to) = box_to_transform(&mut driver, "edit.transform.mesh");
    for name in ["Points across", "Points down", "OK", "Cancel"] {
        assert!(
            driver.texts.iter().any(|(text, _)| text == name),
            "{name} is on the bar: {:?}",
            driver.texts
        );
    }
    // The bottom right point of the lattice pulled further out.
    let _ = from;
    undo_pair(&mut driver, "Mesh Transformation", |driver| {
        drag_with(driver, to, to + vec2(40.0, 40.0), egui::Modifiers::NONE);
        driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let bent = drawn(&driver, top(&driver)).unwrap();
    assert!(
        bent[2] > rect[2] + 10 && bent[3] > rect[3] + 10,
        "{bent:?} from {rect:?}"
    );
    assert_eq!(bent[0], rect[0], "the far side stays");
    // Cancel puts everything back.
    let before = bytes(&driver);
    driver.command("edit.transform.mesh");
    driver.settle(4);
    drag_with(
        &mut driver,
        to + vec2(40.0, 40.0),
        to + vec2(90.0, 90.0),
        egui::Modifiers::NONE,
    );
    driver.click_text("Cancel");
    assert_eq!(bytes(&driver), before, "cancelled, as it was");
}

#[test]
fn sue_paint_transforms_a_folder_and_picked_layers_at_once() {
    // A25-7: a folder is transformed as a unit, and so are the layers
    // picked together; each moves, as one undo step.
    let mut driver = PaintDriver::new("paint-transform-folder");
    let middle = paint_middle(&driver.runtime);
    filled_box(
        &mut driver,
        middle - vec2(80.0, 20.0),
        middle - vec2(40.0, -20.0),
    );
    driver.command("select.none");
    let first = top(&driver);
    driver.command("layer.new");
    driver.settle(2);
    filled_box(
        &mut driver,
        middle + vec2(40.0, -20.0),
        middle + vec2(80.0, 20.0),
    );
    driver.command("select.none");
    let second = top(&driver);
    assert_ne!(first, second);
    let (a, b) = (
        drawn(&driver, first).unwrap(),
        drawn(&driver, second).unwrap(),
    );

    // Every layer picked: Ctrl+T, a drag down from inside, Enter.
    driver.command("layer.select_all");
    driver.settle(2);
    driver.step(vec![key(egui::Key::T, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    let steps = depth(&driver);
    drag_with(
        &mut driver,
        middle - vec2(60.0, 0.0),
        middle + vec2(-60.0, 50.0),
        egui::Modifiers::NONE,
    );
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert_eq!(depth(&driver), steps + 1, "one undo step for both layers");
    let (a2, b2) = (
        drawn(&driver, first).unwrap(),
        drawn(&driver, second).unwrap(),
    );
    assert!(a2[1] > a[1] + 10, "the first went down: {a:?} {a2:?}");
    assert!(
        b2[1] > b[1] + 10,
        "the second went down with it: {b:?} {b2:?}"
    );
    assert_eq!(a2[1] - a[1], b2[1] - b[1], "by as much");
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(drawn(&driver, first), Some(a));
    assert_eq!(drawn(&driver, second), Some(b));

    // The second layer in a folder of its own: the folder is current,
    // and transforming it moves what is in it and nothing else.
    driver.command("layer.select_none");
    driver.command("layer.folder_insert");
    driver.settle(2);
    let folder = sue_paint::doc::folder_of(arena(&driver), second).expect("in a folder");
    assert_eq!(
        sue_paint::doc::members(arena(&driver), picture(&driver), folder),
        [second]
    );
    driver.step(vec![key(egui::Key::T, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    drag_with(
        &mut driver,
        middle + vec2(70.0, 8.0),
        middle + vec2(70.0, -42.0),
        egui::Modifiers::NONE,
    );
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let b3 = drawn(&driver, second).unwrap();
    assert!(
        b3[1] + 10 < b[1],
        "the folder's layer went up: {b:?} {b3:?}"
    );
    assert_eq!(drawn(&driver, first), Some(a), "the layer outside stays");
}

/// Where the last frame drew the mark `id` (`mark.<id>` on the list).
fn mark(driver: &PaintDriver, id: &str) -> Pos2 {
    let wanted = format!("mark.{id}");
    buttons(driver)
        .into_iter()
        .find(|(found, _)| *found == wanted)
        .unwrap_or_else(|| panic!("{wanted} is drawn: {:?}", buttons(driver)))
        .1
        .center()
}

#[test]
fn sue_paint_puppet_warp_pins_pick_together_turn_and_go() {
    // TR-9, TR-10, TR-20: Puppet warp's settings on its bar, Shift and
    // click picks several pins that move together, a pin's ring turns
    // what is under it, Alt and click takes a pin away.
    let mut driver = PaintDriver::new("paint-puppet");
    let middle = paint_middle(&driver.runtime);
    filled_box(
        &mut driver,
        middle - vec2(80.0, 15.0),
        middle + vec2(80.0, 15.0),
    );
    driver.command("select.none");
    driver.settle(2);
    let layer = top(&driver);
    let rect = drawn(&driver, layer).unwrap();
    driver.command("edit.transform.puppet");
    driver.settle(4);
    for name in [
        "Puppet Warp",
        "Keep original image",
        "Show mesh",
        "Click to add pins",
        "Pick several pins",
        "Correct pin rotation",
        "Expansion",
        "Interpolation",
        "Smooth",
        "Reset",
        "Cancel",
        "OK",
    ] {
        assert!(
            driver.rect_of(name).is_some(),
            "{name} is on the bar: {:?}",
            driver.texts
        );
    }
    // Two pins, both picked with Shift, dragged up together: all of it
    // goes up, unbent, as one undo step.
    let (left, right) = (middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    undo_pair(&mut driver, "Puppet Warp", |driver| {
        click(driver, left, egui::Modifiers::NONE);
        click(driver, right, egui::Modifiers::NONE);
        assert!(driver.rect_of("2 pins").is_some(), "{:?}", driver.texts);
        let first = mark(driver, "pin.0");
        click(driver, first, egui::Modifiers::SHIFT);
        drag_with(
            driver,
            right,
            right - vec2(0.0, 40.0),
            egui::Modifiers::NONE,
        );
        driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let moved = drawn(&driver, layer).unwrap();
    assert!(
        moved[1] + 40 < rect[1],
        "it went up: {moved:?} from {rect:?}"
    );
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    assert!(
        (moved[2] - moved[0]).abs_diff(w) <= 4 && (moved[3] - moved[1]).abs_diff(h) <= 4,
        "the same shape: {moved:?} from {rect:?}"
    );

    // One pin in the middle, its ring dragged a quarter round: the bar
    // turns upright around the pin.
    let mut driver = PaintDriver::new("paint-puppet-turn");
    filled_box(
        &mut driver,
        middle - vec2(80.0, 15.0),
        middle + vec2(80.0, 15.0),
    );
    driver.command("select.none");
    let layer = top(&driver);
    driver.command("edit.transform.puppet");
    driver.settle(4);
    click(&mut driver, middle, egui::Modifiers::NONE);
    let pin = mark(&driver, "pin.0");
    // The ring Sue Paint draws round a picked pin, 22 points out.
    let ring = 22.0;
    drag_with(
        &mut driver,
        pin + vec2(ring, 0.0),
        pin + vec2(0.0, ring),
        egui::Modifiers::SHIFT,
    );
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let turned = drawn(&driver, layer).unwrap();
    assert!(
        turned[3] - turned[1] > turned[2] - turned[0],
        "upright now: {turned:?}"
    );

    // Alt and click takes a pin away; Esc puts the layer back.
    let mut driver = PaintDriver::new("paint-puppet-alt");
    filled_box(
        &mut driver,
        middle - vec2(80.0, 15.0),
        middle + vec2(80.0, 15.0),
    );
    driver.command("select.none");
    driver.command("edit.transform.puppet");
    driver.settle(4);
    click(&mut driver, middle, egui::Modifiers::NONE);
    assert!(driver.rect_of("1 pin").is_some());
    let only = mark(&driver, "pin.0");
    click(&mut driver, only, egui::Modifiers::ALT);
    assert!(
        driver.rect_of("Click to put pins").is_some(),
        "{:?}",
        driver.texts
    );
    let before = bytes(&driver);
    driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert!(driver.rect_of("Puppet Warp").is_none(), "the bar went");
    assert_eq!(bytes(&driver), before, "nothing changed");
}

/// A canvas with a timeline and a 2D camera folder, current, and the
/// Operation tool out.
fn camera_canvas(tag: &str) -> (PaintDriver, sue_core::EntityId) {
    let mut driver = PaintDriver::new(tag);
    driver.command("anim.timeline.new");
    driver.settle(2);
    driver.command("anim.layer.camera");
    driver.settle(2);
    let folder = *layers(&driver)
        .iter()
        .find(|layer| sue_paint::camera::is_camera(arena(&driver), **layer))
        .expect("a camera folder");
    driver.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    driver.settle(4);
    (driver, folder)
}

fn camera_at(
    driver: &PaintDriver,
    folder: sue_core::EntityId,
    frame: u32,
) -> sue_paint::camera::Camera {
    let size = sue_model::picture::size(arena(driver).get_by_id(picture(driver)).unwrap()).unwrap();
    let rate = sue_paint::anim::timeline(arena(driver), picture(driver))
        .unwrap()
        .rate;
    sue_paint::camera::Camera::of(
        arena(driver),
        folder,
        Some(sue_paint::camera::time_of(frame, rate)),
        size,
    )
}

#[test]
fn sue_paint_camera_frame_zooms_from_its_middle_with_alt_and_keeps_with_shift() {
    // AB-16, ABV-21, A25V-4: the camera frame's handles (corners, edges,
    // the stalk), its name under its corner, Alt zooming about the
    // middle, Shift keeping a move to one axis and a turn to steps.
    let (mut driver, folder) = camera_canvas("paint-camera-handles");
    let name = sue_paint::doc::layers(arena(&driver), picture(&driver))
        .into_iter()
        .find(|info| info.id == folder)
        .unwrap()
        .name;
    let corner = mark(&driver, "camera.corner.3");
    let label = driver
        .texts
        .iter()
        .filter(|(text, _)| *text == name)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| {
            a.center()
                .distance(corner)
                .total_cmp(&b.center().distance(corner))
        })
        .expect("the frame's name is drawn");
    assert!(
        (label.top() - corner.y).abs() < 40.0,
        "under its corner: {label:?} {corner:?}"
    );
    assert!(label.left() >= corner.x - 2.0, "beside its left edge");
    let base = camera_at(&driver, folder, 0);
    let corner = mark(&driver, "camera.corner.2");

    // Without Alt the far corner stays: the bottom right corner dragged
    // in zooms in, and the middle moves toward the top left.
    undo_pair(&mut driver, "Camera zoom", |driver| {
        drag_with(
            driver,
            corner,
            corner - vec2(80.0, 60.0),
            egui::Modifiers::NONE,
        );
    });
    let anchored = camera_at(&driver, folder, 0);
    assert!(
        anchored.scale > base.scale + 0.05,
        "it zoomed in: {anchored:?}"
    );
    let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1.0 && (a[1] - b[1]).abs() < 1.0;
    assert!(
        anchored.center[0] < base.center[0] - 5.0 && anchored.center[1] < base.center[1] - 5.0,
        "the middle moved: {anchored:?}"
    );

    // Outside the frame turns it (the stalk does too); with Shift by
    // steps of 15 degrees.
    let (right, bottom) = (
        mark(&driver, "camera.corner.1"),
        mark(&driver, "camera.corner.2"),
    );
    let middle = mark(&driver, "camera.middle");
    let outside = pos2(
        (right.x + bottom.x) / 2.0 + 30.0,
        (right.y + bottom.y) / 2.0,
    );
    let arm = outside - middle;
    let turned_to = middle + arm.rot90() * 0.35 + arm * 0.95;
    drag_with(&mut driver, outside, turned_to, egui::Modifiers::SHIFT);
    let turned = camera_at(&driver, folder, 0);
    let degrees = (turned.angle - anchored.angle).to_degrees();
    assert!(degrees.abs() > 5.0, "it turned: {degrees}");
    assert!(
        (degrees / 15.0 - (degrees / 15.0).round()).abs() < 1e-3,
        "{degrees}"
    );
    // Turned back, for what follows.
    driver.command("edit.undo");
    driver.settle(2);

    // Alt on the same corner, outward: it zooms out about its middle.
    let corner = mark(&driver, "camera.corner.2");
    undo_pair(&mut driver, "Alt zoom", |driver| {
        drag_with(
            driver,
            corner,
            corner + vec2(40.0, 30.0),
            egui::Modifiers::ALT,
        );
    });
    let zoomed = camera_at(&driver, folder, 0);
    assert!(
        zoomed.scale < anchored.scale - 0.05,
        "it zoomed out: {zoomed:?}"
    );
    assert!(
        close(zoomed.center, anchored.center),
        "about its middle: {zoomed:?}"
    );
    let anchored = zoomed;

    // Shift keeps a move along one axis.
    let middle = mark(&driver, "camera.middle") + vec2(30.0, 30.0);
    drag_with(
        &mut driver,
        middle,
        middle + vec2(50.0, 12.0),
        egui::Modifiers::SHIFT,
    );
    let moved = camera_at(&driver, folder, 0);
    assert!(
        moved.center[0] > anchored.center[0] + 20.0,
        "it moved across"
    );
    assert!(
        (moved.center[1] - anchored.center[1]).abs() < 0.5,
        "and not down"
    );
}

#[test]
fn sue_paint_light_table_box_scales_turns_and_flips_its_layers() {
    // AB-8, ABV-17: the Light table tool's box with corner handles and a
    // turning handle on a stalk, scale, turn and flip, each one step.
    let mut driver = PaintDriver::new("paint-light-box");
    let middle = paint_middle(&driver.runtime);
    driver.command("layer.new");
    driver.settle(2);
    filled_box(
        &mut driver,
        middle - vec2(40.0, 20.0),
        middle + vec2(40.0, 20.0),
    );
    driver.command("select.none");
    let reference = top(&driver);
    driver.command("anim.timeline.new");
    driver.command("anim.cel.new");
    driver.settle(2);
    driver.command("layer.select_below");
    driver.command("anim.light_table.register_general");
    driver.command("anim.light_table.tool");
    driver.settle(4);
    let place = |driver: &PaintDriver| {
        sue_paint::anim::Place::of(arena(driver).get_by_id(reference).unwrap())
    };
    // The box is around the drawing: its bottom right corner outward.
    let corner = mark(&driver, "light.corner.2");
    assert!(
        corner.x > middle.x + 20.0 && corner.y > middle.y + 10.0,
        "{corner:?}"
    );
    undo_pair(&mut driver, "Light table scale", |driver| {
        drag_with(
            driver,
            corner,
            corner + vec2(40.0, 20.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(place(&driver).scale > 1.2, "{:?}", place(&driver));
    let top_left = mark(&driver, "light.corner.0");
    assert!(
        top_left.distance(middle - vec2(40.0, 20.0)) < 6.0,
        "the far corner stayed: {top_left:?}"
    );
    // The stalk turns it.
    let stalk = mark(&driver, "light.stalk");
    undo_pair(&mut driver, "Light table turn", |driver| {
        drag_with(
            driver,
            stalk,
            stalk + vec2(80.0, 40.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(place(&driver).degrees.abs() > 10.0, "{:?}", place(&driver));
    // Flip, from the box's launcher.
    undo_pair(&mut driver, "Light table flip", |driver| {
        click_button(driver, "launcher.light.flip");
    });
    assert!(place(&driver).flip);
    // Reset, from the same launcher: back where it was.
    click_button(&mut driver, "anim.light_table.reset");
    assert!(place(&driver).is_still(), "{:?}", place(&driver));
}

#[test]
fn sue_paint_object_handles_key_a_layer_with_keyframes() {
    // AB-1: with keyframes on a layer, the Object tool's box round its
    // drawing moves, scales and turns it, each a key at the frame shown
    // in one undo step. (Turning keyframes on has no menu item yet, P4-7
    // adds it; the model's edit is applied here as that item will.)
    let mut driver = PaintDriver::new("paint-keyed-object");
    let middle = paint_middle(&driver.runtime);
    filled_box(
        &mut driver,
        middle - vec2(40.0, 20.0),
        middle + vec2(40.0, 20.0),
    );
    driver.command("select.none");
    driver.settle(2);
    let layer = layers(&driver)
        .into_iter()
        .find(|layer| drawn(&driver, *layer).is_some())
        .expect("the drawn layer");
    let mut on = sue_paint::anim::motion::enable_keyframes(layer, true);
    sue_core::ReversibleTransaction::apply(&mut on, &mut driver.runtime.engine.world.arena)
        .unwrap();
    driver.command("tool.operation");
    driver.settle(4);
    let size =
        sue_model::picture::size(arena(&driver).get_by_id(picture(&driver)).unwrap()).unwrap();
    let pose = |driver: &PaintDriver| {
        // No timeline: frames go at Sue Paint's 24 a second.
        let rate = sue_core::Rate::new(24, 1);
        sue_paint::anim::motion::pose(
            arena(driver),
            layer,
            sue_paint::camera::time_of(0, rate),
            size,
        )
        .unwrap()
    };
    let inside = mark(&driver, "keyed.middle") + vec2(10.0, 5.0);
    undo_pair(&mut driver, "Object move key", |driver| {
        drag_with(
            driver,
            inside,
            inside + vec2(60.0, 0.0),
            egui::Modifiers::SHIFT,
        );
    });
    let moved = pose(&driver);
    assert!(moved.position[0] > 50.0, "moved across: {moved:?}");
    assert!(moved.position[1].abs() < 0.5, "Shift kept it level");
    let corner = mark(&driver, "keyed.corner.2");
    undo_pair(&mut driver, "Object scale key", |driver| {
        drag_with(
            driver,
            corner,
            corner + vec2(40.0, 20.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(pose(&driver).scale > 1.2, "{:?}", pose(&driver));
    let stalk = mark(&driver, "keyed.stalk");
    undo_pair(&mut driver, "Object turn key", |driver| {
        drag_with(
            driver,
            stalk,
            stalk + vec2(80.0, 30.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(pose(&driver).rotation.abs() > 10.0, "{:?}", pose(&driver));
    assert!(
        !sue_paint::anim::motion::key_times(arena(&driver), layer).is_empty(),
        "keyed"
    );
}
