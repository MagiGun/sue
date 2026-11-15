//! Sue Paint's canvas chrome (P4-1f), through the whole runtime: the tab
//! strip, a canvas in a window of its own (gap CT-16, on P3-20's views),
//! New Window as a tab of its own (CC-6), pictures dropped on the canvas
//! (X-7, H-7), the status strip and the scroll bars.

use std::sync::Arc;

use super::*;
use sue_platform::WindowSupport;

fn paint_id() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// A file dropped on Sue's window, as the system hands it over.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

/// One frame of Sue's main window with `events` and the files `dropped`;
/// the whole of what it drew. The driver's texts follow it.
fn main_frame(
    driver: &mut PaintDriver,
    events: Vec<egui::Event>,
    dropped: &[PathBuf],
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        dropped_files: dropped
            .iter()
            .map(|path| Arc::new(Dropped(path.clone())) as egui::DroppedFileHandle)
            .collect(),
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let (runtime, now) = (&mut driver.runtime, driver.now);
    let mut output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    output.textures_delta.clear();
    driver.texts = text_rects(&output);
    output
}

/// One frame of the window of the view named `key`, 960 by 640 points,
/// with `events`, as the desktop host draws it; what it drew.
fn view_frame(driver: &mut PaintDriver, key: &str, events: Vec<egui::Event>) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))),
        events,
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let (runtime, now) = (&mut driver.runtime, driver.now);
    let mut output = sue_platform::windows::window_pass(&driver.ctx, key, raw, |ui| {
        runtime.window_ui(key, ui, &mut NoTextures, now);
    });
    output.textures_delta.clear();
    output
}

/// A click at `at` in the window of view `key`.
fn click_in_view(driver: &mut PaintDriver, key: &str, at: Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    view_frame(driver, key, vec![egui::Event::PointerMoved(at)]);
    view_frame(driver, key, vec![press(true)]);
    view_frame(driver, key, vec![press(false)]);
    view_frame(driver, key, Vec::new());
}

/// The pictures drawn as pictures this frame: without textures (as in
/// these tests) the view fills the picture's quad with one light gray
/// (faded with what it is drawn in), so each such quad is a canvas on
/// screen. Their rects.
fn pictures_drawn(output: &egui::FullOutput) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, found: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, found);
                }
            }
            egui::Shape::Path(path)
                if path.closed && path.points.len() == 4 && is_ground(path.fill) =>
            {
                found.push(egui::Rect::from_points(&path.points));
            }
            _ => {}
        }
    }
    /// The light gray of a picture without its texture, at any opacity.
    fn is_ground(fill: egui::Color32) -> bool {
        let [r, g, b, a] = fill.to_array();
        let gray = f32::from(r) * 255.0 / f32::from(a.max(1));
        a > 0 && r == g && g == b && (gray - 235.0).abs() < 2.5
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// The canvas Sue Paint works on now.
fn current(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&paint_id())
        .and_then(|applet| applet.focus())
        .expect("a canvas is current")
}

/// The name of picture `id`, as its tab reads it.
fn name_of(driver: &PaintDriver, id: sue_core::EntityId) -> String {
    driver
        .runtime
        .engine
        .world
        .arena
        .get_by_id(id)
        .unwrap()
        .name()
        .to_owned()
}

/// The tabs that read `name` (with a star while it has changes not
/// saved), left to right: the texts named so in the strip over the
/// canvas (the topmost row of them).
fn tabs_named(driver: &PaintDriver, name: &str) -> Vec<egui::Rect> {
    let starred = format!("{name}*");
    let mut found: Vec<egui::Rect> = driver
        .texts
        .iter()
        .filter(|(text, _)| text == name || *text == starred)
        .map(|(_, rect)| *rect)
        .collect();
    let Some(top) = found.iter().map(|rect| rect.top()).reduce(f32::min) else {
        return found;
    };
    found.retain(|rect| (rect.top() - top).abs() < 2.0);
    found.sort_by(|a, b| a.left().total_cmp(&b.left()));
    found
}

/// The window title Sue Paint asks for now (with the zoom in it).
fn title(driver: &PaintDriver) -> String {
    driver
        .runtime
        .applets
        .get(&paint_id())
        .and_then(|applet| applet.window_title(&driver.runtime.engine.world.arena))
        .unwrap_or_default()
}

/// A second canvas, made with Ctrl+N; it is current. Its id.
fn second_canvas(driver: &mut PaintDriver) -> sue_core::EntityId {
    let first = current(driver);
    driver.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(30);
    let second = current(driver);
    assert_ne!(first, second, "Ctrl+N made a second canvas");
    second
}

/// The keys of the canvas views open now.
fn canvas_views(driver: &PaintDriver) -> Vec<String> {
    driver
        .runtime
        .shell
        .views()
        .into_iter()
        .filter(|key| key.contains("canvas:"))
        .collect()
}

#[test]
fn a_tab_dragged_out_of_the_strip_opens_its_canvas_in_a_window_of_its_own() {
    let mut driver = PaintDriver::new("chrome-tear");
    driver.runtime.shell.set_window_support(WindowSupport::Many);
    let first = current(&driver);
    let second = second_canvas(&mut driver);
    let name = name_of(&driver, second);
    driver.settle(2);
    let tabs = tabs_named(&driver, &name);
    assert!(!tabs.is_empty(), "the tab of {name:?}: {:?}", driver.texts);
    let tab = *tabs.last().unwrap();
    // Down out of the strip and onto the canvas: torn off.
    driver.drag(
        tab.center(),
        tab.center() + vec2(0.0, 240.0),
        egui::PointerButton::Primary,
    );
    let specs = driver.runtime.windows();
    assert_eq!(specs.len(), 1, "one window of its own: {specs:?}");
    let view = specs[0].key.clone();
    assert!(view.ends_with("canvas:0"), "{view}");
    assert_eq!(specs[0].title, name, "the window is named as its canvas");
    // The window draws its canvas; the column draws the other one.
    let mut drawn = Vec::new();
    for _ in 0..3 {
        drawn = pictures_drawn(&view_frame(&mut driver, &view, Vec::new()));
    }
    assert_eq!(drawn.len(), 1, "the window shows its canvas");
    let in_window = drawn[0];
    let main = main_frame(&mut driver, Vec::new(), &[]);
    assert_eq!(
        pictures_drawn(&main).len(),
        1,
        "the column shows one canvas"
    );
    // The torn canvas has no tab any more: one tab is left.
    let left = name_of(&driver, first);
    let all: usize = if left == name {
        tabs_named(&driver, &name).len()
    } else {
        tabs_named(&driver, &left).len() + tabs_named(&driver, &name).len()
    };
    assert_eq!(all, 1, "{:?}", driver.texts);
    // A press in the window makes its canvas current; a press in the
    // column, the column's.
    assert_eq!(current(&driver), second, "the torn canvas stays current");
    let column = pictures_drawn(&main)[0];
    driver.step(vec![egui::Event::PointerMoved(column.center())]);
    driver.step(vec![egui::Event::PointerButton {
        pos: column.center(),
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }]);
    driver.step(vec![egui::Event::PointerButton {
        pos: column.center(),
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    driver.settle(2);
    assert_eq!(current(&driver), first, "a press in the column");
    click_in_view(&mut driver, &view, in_window.center());
    assert_eq!(current(&driver), second, "a press in the window");
    // Closing the window (the system's close button) puts the canvas
    // back in the tabs, current.
    driver
        .runtime
        .window_closed(&view, &mut NoTextures, driver.now);
    driver.settle(3);
    assert!(driver.runtime.windows().is_empty());
    assert!(canvas_views(&driver).is_empty());
    assert_eq!(current(&driver), second);
    let tabs = if left == name {
        tabs_named(&driver, &name).len()
    } else {
        tabs_named(&driver, &left).len() + tabs_named(&driver, &name).len()
    };
    assert_eq!(tabs, 2, "both canvases are tabs again");
    let main = main_frame(&mut driver, Vec::new(), &[]);
    assert_eq!(pictures_drawn(&main).len(), 1);
}

#[test]
fn where_sue_has_one_window_a_torn_canvas_floats_over_the_page() {
    let mut driver = PaintDriver::new("chrome-float");
    assert_eq!(driver.runtime.shell.window_support(), WindowSupport::One);
    let second = second_canvas(&mut driver);
    // Window, Canvas, Canvas in Its Own Window: the same as tearing.
    driver.command("window.float");
    driver.settle(4);
    let views = canvas_views(&driver);
    assert_eq!(views.len(), 1, "{views:?}");
    assert!(driver.runtime.windows().is_empty(), "no system window");
    let (frame, _) = driver.runtime.shell.view_rects(&views[0]).unwrap();
    assert!(driver.runtime.shell.page_rect().contains_rect(frame));
    // The page shows both: the column's canvas and the floating one.
    let drawn = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]));
    assert_eq!(drawn.len(), 2, "{drawn:?}");
    assert!(drawn.iter().any(|rect| frame.intersects(*rect)));
    // Run again, the command puts it back.
    assert_eq!(current(&driver), second);
    driver.command("window.float");
    driver.settle(4);
    assert!(canvas_views(&driver).is_empty());
    assert_eq!(current(&driver), second);
    assert_eq!(
        pictures_drawn(&main_frame(&mut driver, Vec::new(), &[])).len(),
        1
    );
}

#[test]
fn a_tabs_menu_moves_its_canvas_into_a_window_and_closing_the_canvas_closes_it() {
    let mut driver = PaintDriver::new("chrome-tab-menu");
    driver.runtime.shell.set_window_support(WindowSupport::Many);
    let second = second_canvas(&mut driver);
    let name = name_of(&driver, second);
    driver.settle(2);
    let tab = *tabs_named(&driver, &name).last().unwrap();
    let press = |pressed| egui::Event::PointerButton {
        pos: tab.center(),
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(tab.center())]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(3);
    for item in ["Save", "Close", "Canvas in Its Own Window"] {
        assert!(driver.rect_of(item).is_some(), "{item}: {:?}", driver.texts);
    }
    driver.click_text("Canvas in Its Own Window");
    assert_eq!(driver.runtime.windows().len(), 1);
    // Its window has a button that puts it back in the tabs.
    let view = driver.runtime.windows()[0].key.clone();
    let texts = text_rects(&view_frame(&mut driver, &view, Vec::new()));
    assert!(
        texts.iter().any(|(text, _)| text == "%"),
        "the zoom: {texts:?}"
    );
    // File, Close on the windowed canvas closes its window too.
    assert_eq!(current(&driver), second);
    driver.command("file.close");
    driver.settle(6);
    if let Some(dont) = driver.rect_of("Don't Save") {
        let _ = dont;
        driver.click_text("Don't Save");
    }
    driver.settle(4);
    assert!(
        driver.runtime.windows().is_empty(),
        "its window closed with it"
    );
}

#[test]
fn new_window_adds_a_tab_and_leaves_tiling_alone() {
    let mut driver = PaintDriver::new("chrome-new-window");
    let picture = current(&driver);
    let name = name_of(&driver, picture);
    let alone = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    driver.command("window.new");
    driver.settle(4);
    let drawn = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]));
    assert_eq!(
        tabs_named(&driver, &name).len(),
        2,
        "two tabs of one picture"
    );
    // Neither of them changed anything: no star on either.
    assert!(
        driver
            .texts
            .iter()
            .all(|(text, _)| *text != format!("{name}*")),
        "{:?}",
        driver.texts
    );
    // One canvas fills the column, where it was: not tiled.
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert!(
        (drawn[0].center() - alone.center()).length() < 1.0,
        "{drawn:?}"
    );
    // Tile still tiles them: the current one moves into its cell.
    driver.command("window.tile");
    driver.settle(4);
    let tiled = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]));
    assert!(
        tiled
            .iter()
            .all(|rect| (rect.center().x - alone.center().x).abs() > 40.0),
        "tiled: {tiled:?}"
    );
}

/// A 4 by 4 picture file in `dir`, named `name`.
fn picture_file(dir: &std::path::Path, name: &str) -> PathBuf {
    let pixels: Vec<u8> = (0..16).flat_map(|_| [200, 40, 40, 255]).collect();
    let png = sue_io::encode_png_rgba(4, 4, &pixels).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, png).unwrap();
    path
}

#[test]
fn a_picture_dropped_on_the_canvas_becomes_a_layer_and_on_the_tabs_a_canvas() {
    let mut driver = PaintDriver::new("chrome-drop");
    let picture = current(&driver);
    let layers = |driver: &PaintDriver| {
        sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture).len()
    };
    let before = layers(&driver);
    let file = picture_file(&driver.root, "dropped.png");
    let canvas = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    main_frame(
        &mut driver,
        vec![egui::Event::PointerMoved(canvas.center())],
        &[],
    );
    main_frame(&mut driver, Vec::new(), std::slice::from_ref(&file));
    driver.settle(6);
    assert_eq!(current(&driver), picture, "no canvas of its own");
    assert_eq!(layers(&driver), before + 1, "a new layer of the canvas");
    // Dropped on the tab strip, it opens as a canvas of its own.
    let name = name_of(&driver, picture);
    let tab = tabs_named(&driver, &name)[0];
    main_frame(
        &mut driver,
        vec![egui::Event::PointerMoved(tab.center())],
        &[],
    );
    main_frame(&mut driver, Vec::new(), std::slice::from_ref(&file));
    driver.settle(10);
    assert_ne!(current(&driver), picture, "a new canvas");
    assert_eq!(
        layers(&driver),
        before + 1,
        "the first canvas kept its layers"
    );
    // Opened from the Sue menu or the Hub, a picture still opens as a
    // canvas of its own: only a drop on the canvas makes a layer.
    let opened = current(&driver);
    driver.runtime.open_path(&file, &mut NoTextures, driver.now);
    driver.settle(10);
    assert_ne!(current(&driver), opened, "Open makes a canvas");
}

/// Where the last frame of the main window laid the scroll bar `salt`.
fn bar_rect(driver: &PaintDriver, salt: &str) -> egui::Rect {
    driver
        .ctx
        .read_response(egui::Id::unique(salt))
        .unwrap_or_else(|| panic!("{salt} was drawn"))
        .rect
}

#[test]
fn the_status_strip_follows_the_zoom_and_the_scroll_bars_scroll() {
    let mut driver = PaintDriver::new("chrome-status");
    driver.settle(2);
    let zoom_before = title(&driver);
    let canvas = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    // The zoom field reads its number with a % after it.
    let field = |driver: &PaintDriver| {
        let unit = driver
            .texts
            .iter()
            .find(|(text, rect)| text == "%" && rect.top() > canvas.bottom())
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("the zoom field: {:?}", driver.texts));
        driver
            .texts
            .iter()
            .filter(|(_, rect)| (rect.center().y - unit.center().y).abs() < 2.0)
            .filter(|(_, rect)| rect.right() <= unit.left() + 1.0)
            .max_by(|a, b| a.1.right().total_cmp(&b.1.right()))
            .map(|(text, _)| text.clone())
            .unwrap()
    };
    let shown = field(&driver);
    assert!(zoom_before.contains(&shown), "{zoom_before} shows {shown}");
    // Zoom In (its keys): the field and the title follow.
    driver.command("view.zoom_in");
    driver.settle(3);
    main_frame(&mut driver, Vec::new(), &[]);
    let zoomed = field(&driver);
    assert_ne!(zoomed, shown, "the field follows the zoom");
    assert!(
        title(&driver).contains(&zoomed),
        "{} shows {zoomed}",
        title(&driver)
    );
    // The horizontal scroll bar in the status strip moves the picture
    // sideways; the vertical one at the right of the view, up and down.
    let before = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    let across = bar_rect(&driver, "sue-paint-scroll-h");
    assert!(across.top() > before.bottom() - 1.0 || across.top() > canvas.bottom());
    driver.drag(
        across.center(),
        across.center() - vec2(across.width() / 4.0, 0.0),
        egui::PointerButton::Primary,
    );
    let after = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    assert!(
        (after.center().x - before.center().x).abs() > 4.0,
        "sideways: {before:?} then {after:?}"
    );
    assert!((after.center().y - before.center().y).abs() < 1.0);
    let down = bar_rect(&driver, "sue-paint-scroll-v");
    assert!(down.height() > down.width(), "{down:?}");
    driver.drag(
        down.center(),
        down.center() + vec2(0.0, down.height() / 4.0),
        egui::PointerButton::Primary,
    );
    let moved = pictures_drawn(&main_frame(&mut driver, Vec::new(), &[]))[0];
    assert!(
        (moved.center().y - after.center().y).abs() > 4.0,
        "up and down: {after:?} then {moved:?}"
    );
    assert!((moved.center().x - after.center().x).abs() < 1.0);
}

#[test]
fn a_tab_dragged_along_the_strip_changes_the_order_of_the_canvases() {
    let mut driver = PaintDriver::new("chrome-reorder");
    let first = current(&driver);
    let second = second_canvas(&mut driver);
    driver.settle(2);
    let name = name_of(&driver, first);
    let mut tabs = tabs_named(&driver, &name);
    if name != name_of(&driver, second) {
        tabs.extend(tabs_named(&driver, &name_of(&driver, second)));
        tabs.sort_by(|a, b| a.left().total_cmp(&b.left()));
    }
    assert_eq!(tabs.len(), 2, "{:?}", driver.texts);
    // The first tab, along the strip past the middle of the second.
    let to = egui::pos2(tabs[1].right() + 12.0, tabs[0].center().y);
    driver.drag(tabs[0].center(), to, egui::PointerButton::Primary);
    assert!(driver.runtime.windows().is_empty());
    assert!(canvas_views(&driver).is_empty(), "moved, not torn off");
    // Window, Canvas, 1 is now the canvas that was second.
    driver.command("window.canvas.0");
    driver.settle(2);
    assert_eq!(current(&driver), second);
    driver.command("window.canvas.1");
    driver.settle(2);
    assert_eq!(current(&driver), first);
}
