//! The bugs people reported against 0.0.8: a new comic page closed the
//! app, the Layer palette lagged, some Sub Tool stroke previews were
//! striped, and New hid most of its options below a scroll. Each is
//! driven through real frames here, found on screen by its text or by
//! what it tells assistive technology.

use super::*;
use sue_core::EntityId;

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// The canvas in focus.
fn picture(driver: &PaintDriver) -> EntityId {
    driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// One frame with `events`, telling assistive technology what is on
/// screen: every named control and where it is.
fn told(driver: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    driver.ctx.enable_accesskit();
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
    let found = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some((
                node.label()?.to_owned(),
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    driver.ctx.disable_accesskit();
    found
}

/// A click in the middle of the control named `name` that is below the
/// menu bar, then a few frames.
fn click_named(driver: &mut PaintDriver, name: &str) {
    let controls = told(driver, Vec::new());
    let at = controls
        .iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| *rect)
        .find(|rect| rect.top() > 30.0)
        .map(|rect| rect.center())
        .unwrap_or_else(|| panic!("{name:?} is on screen"));
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(6);
}

/// Opens File, New (Ctrl+N) and lets it come in.
fn open_new(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    driver.settle(10);
    assert!(driver.rect_of("New").is_some(), "the New dialog is open");
}

/// The pages of the work in focus (the canvas alone when it is no
/// story).
fn pages(driver: &PaintDriver) -> Vec<EntityId> {
    let arena = &driver.runtime.engine.world.arena;
    let page = picture(driver);
    sue_paint::story::story_of(arena, page)
        .map_or_else(|| vec![page], |story| sue_paint::story::pages(arena, story))
}

/// A line drawn across the canvas's middle.
fn draw(driver: &mut PaintDriver) {
    let middle = paint_middle(&driver.runtime);
    driver.drag(
        middle - vec2(60.0, 20.0),
        middle + vec2(60.0, 20.0),
        egui::PointerButton::Primary,
    );
}

/// Saves the work in focus as a Sue project at `path`.
fn save_as(driver: &mut PaintDriver, path: &std::path::Path) {
    driver.step(vec![key(egui::Key::S, egui::Modifiers::COMMAND)]);
    driver.settle(6);
    let asked = driver
        .runtime
        .pending_pick()
        .expect("Save asks where")
        .clone();
    driver.runtime.request(
        ShellRequest::Picked {
            pick: asked,
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        driver.now,
    );
    let start = Instant::now();
    while !path.exists() || driver.runtime.transfers.poll() {
        assert!(start.elapsed() < Duration::from_secs(300), "the save ends");
        driver.step(Vec::new());
    }
    driver.settle(6);
}

/// A new work of `kind` made through New as it comes, drawn on, saved
/// and opened again.
fn new_work_round_trip(tag: &str, kind: &str, more: impl FnOnce(&mut PaintDriver)) {
    let mut driver = PaintDriver::new(tag);
    let before = picture(&driver);
    let clock = Instant::now();
    open_new(&mut driver);
    click_named(&mut driver, kind);
    more(&mut driver);
    driver.click_text("OK");
    driver.settle(10);
    let page = picture(&driver);
    assert_ne!(page, before, "{kind}: the new work is in front");
    let made = clock.elapsed();
    draw(&mut driver);
    let drawn = clock.elapsed();
    let count = pages(&driver).len();
    let layers = sue_model::picture::layers(&driver.runtime.engine.world.arena, page).len();
    // Kept as unsaved work in the background, as a minute after a change.
    driver.runtime.autosave();
    driver.runtime.wait_jobs();
    driver.settle(4);
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join(format!("{tag}.sue"));
    save_as(&mut driver, &path);
    let saved = clock.elapsed();
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        sue_core::SueFile::from_bytes(&bytes).is_ok(),
        "{kind} saved"
    );
    // Opened again in a Sue that never saw it.
    let (mut runtime, root) = runtime(&format!("{tag}-again"));
    runtime
        .open_project(&path, &bytes)
        .expect("the project opens");
    let mut again = PaintDriver::with(runtime, root);
    let arena = &again.runtime.engine.world.arena;
    assert!(arena.get_by_id(page).is_ok(), "{kind}: the page came back");
    let back = sue_paint::story::story_of(arena, page)
        .map_or(1, |story| sue_paint::story::pages(arena, story).len());
    assert_eq!(back, count, "{kind}: every page came back");
    assert_eq!(
        sue_model::picture::layers(arena, page).len(),
        layers,
        "{kind}: every layer came back"
    );
    // The page shows and takes another line.
    again.runtime.request(
        ShellRequest::OpenIn {
            entity: page,
            applet: paint(),
        },
        &mut NoTextures,
        again.now,
    );
    again.settle(10);
    assert_eq!(picture(&again), page, "{kind}: the page is in front");
    draw(&mut again);
    eprintln!(
        "{kind}: made {made:?}, drawn {drawn:?}, saved {saved:?} ({} bytes), reopened {:?}, {count} pages",
        bytes.len(),
        clock.elapsed()
    );
}

#[test]
fn a_new_comic_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-comic", "Comic", |_| {});
}

#[test]
fn a_new_comic_with_all_settings_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-comic-all", "Show all comic settings", |_| {});
}

#[test]
fn a_new_fanzine_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-fanzine", "Printing of fanzine", |_| {});
}

#[test]
fn a_new_illustration_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-illustration", "Illustration", |_| {});
}

#[test]
fn a_new_moving_illustration_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-moving", "Illustration", |driver| {
        click_named(driver, "Create animated illustration");
    });
}

/// The raster layers of `page` that nothing is drawn on.
fn blank_layers(driver: &PaintDriver, page: EntityId) -> Vec<sue_core::RasterTile2D> {
    let arena = &driver.runtime.engine.world.arena;
    sue_model::picture::layers(arena, page)
        .into_iter()
        .filter_map(|layer| {
            arena
                .get_by_id(layer)
                .ok()?
                .component(sue_core::ComponentKind::RasterTile2D)?
                .as_raster()
                .cloned()
        })
        .collect()
}

/// The cause of "the app just closes" on a new comic: every blank page
/// took its whole pixel buffer at once (104 MB a page at 600 dots an
/// inch, 835 MB for the eight pages New starts with, before a line was
/// drawn), and a program that is refused memory ends without a word.
/// Blank pages now share one buffer of zeros, so a comic of any number
/// of pages opens; a page gets pixels of its own when it is drawn on.
#[test]
fn a_comics_blank_pages_share_one_buffer_until_drawn_on() {
    let mut driver = PaintDriver::new("p81-bugs-comic-memory");
    open_new(&mut driver);
    click_named(&mut driver, "Comic");
    driver.click_text("OK");
    driver.settle(10);
    // Story, Add Page, many times over: the Story menu's way to more.
    for _ in 0..40 {
        driver.command("story.add");
        driver.step(Vec::new());
    }
    driver.command("story.first");
    driver.settle(6);
    let all = pages(&driver);
    assert_eq!(all.len(), 48);
    let blanks: Vec<sue_core::RasterTile2D> = all
        .iter()
        .flat_map(|page| blank_layers(&driver, *page))
        .collect();
    assert_eq!(blanks.len(), 48, "one raster layer a page");
    assert_eq!([blanks[0].width(), blanks[0].height()], [4299, 6070]);
    assert!(
        blanks
            .iter()
            .all(|raster| raster.shares_pixels_with(&blanks[0])),
        "48 blank pages of 104 MB each hold one buffer between them"
    );
    drop(blanks);
    // Drawing gives the page drawn on its own pixels; the rest stay.
    let first = picture(&driver);
    assert_eq!(first, all[0]);
    draw(&mut driver);
    let drawn = blank_layers(&driver, first);
    let other = blank_layers(&driver, all[1]);
    assert!(drawn[0].bytes().unwrap().iter().any(|byte| *byte != 0));
    assert!(!drawn[0].shares_pixels_with(&other[0]));
    assert!(other[0].bytes().unwrap().iter().all(|byte| *byte == 0));
    // A new layer on the page is blank too, and shares with the pages.
    driver.command("layer.new");
    driver.settle(4);
    let layers = blank_layers(&driver, first);
    assert_eq!(layers.len(), 2);
    assert!(
        layers
            .iter()
            .any(|raster| raster.shares_pixels_with(&other[0]))
    );
    // Undo takes the line back and the page is as it was.
    driver.command("edit.undo");
    driver.command("edit.undo");
    driver.settle(4);
    let back = blank_layers(&driver, first);
    assert_eq!(back.len(), 1);
    assert!(back[0].bytes().unwrap().iter().all(|byte| *byte == 0));
}

#[test]
fn a_new_webtoon_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-webtoon", "Webtoon", |_| {});
}

#[test]
fn a_new_animation_opens_draws_saves_and_reopens() {
    new_work_round_trip("p81-bugs-animation", "Animation", |_| {});
}

/// What one frame of a window of `size` shows: every piece of text and
/// every control named to assistive technology, with their rects.
struct Seen {
    texts: Vec<(String, egui::Rect)>,
    named: Vec<(String, egui::Rect)>,
}

impl Seen {
    fn text(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
    }

    fn named(&self, name: &str) -> Option<egui::Rect> {
        self.named
            .iter()
            .filter(|(found, _)| found == name)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
    }
}

/// One frame of a window of `size` with `events`.
fn look(driver: &mut PaintDriver, size: egui::Vec2, events: Vec<egui::Event>) -> Seen {
    driver.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
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
    let named = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some((
                node.label()?.to_owned(),
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    let texts = text_rects(&output);
    output.drop_without_applying_deltas();
    driver.ctx.disable_accesskit();
    Seen { texts, named }
}

/// A click at `at` in a window of `size`, and the frames after.
fn click_in(driver: &mut PaintDriver, size: egui::Vec2, at: Pos2) -> Seen {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    look(driver, size, vec![egui::Event::PointerMoved(at)]);
    look(driver, size, vec![press(true)]);
    look(driver, size, vec![press(false)]);
    let mut seen = look(driver, size, Vec::new());
    for _ in 0..12 {
        seen = look(driver, size, Vec::new());
    }
    seen
}

/// New shows everything it holds for every kind of work in a window of
/// 1366 by 768: its last rows stand above its buttons, and the dialog
/// and its buttons are inside the window. Nothing has to be scrolled to.
/// (People did not notice that New went on below.)
#[test]
fn new_shows_all_it_holds_in_a_small_window() {
    let size = vec2(1366.0, 768.0);
    let mut driver = PaintDriver::new("p81-bugs-new-size");
    for _ in 0..10 {
        look(&mut driver, size, Vec::new());
    }
    look(
        &mut driver,
        size,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    for _ in 0..20 {
        look(&mut driver, size, Vec::new());
    }
    for (kind, last_rows) in [
        ("Illustration", vec!["Template", "Record timelapse"]),
        ("Webtoon", vec!["Template", "Record timelapse"]),
        (
            "Comic",
            vec!["Inner border", "Template", "Record timelapse"],
        ),
        (
            "Printing of fanzine",
            vec!["Inner border", "Template", "Record timelapse"],
        ),
        (
            "Show all comic settings",
            vec!["First page number", "Template", "Record timelapse"],
        ),
        (
            "Animation",
            vec!["Division line", "Left", "Right", "Record timelapse"],
        ),
    ] {
        let seen = look(&mut driver, size, Vec::new());
        let at = seen
            .named(kind)
            .unwrap_or_else(|| panic!("{kind} is offered"))
            .center();
        let seen = click_in(&mut driver, size, at);
        let ok = seen
            .text("OK")
            .unwrap_or_else(|| panic!("{kind}: OK shows"));
        let cancel = seen.text("Cancel").expect("Cancel shows");
        assert!(
            ok.bottom() <= size.y && cancel.bottom() <= size.y && ok.top() > 0.0,
            "{kind}: the buttons are in the window: {ok:?}"
        );
        for row in last_rows {
            let rect = seen
                .text(row)
                .or_else(|| seen.named(row))
                .unwrap_or_else(|| panic!("{kind}: {row} is drawn"));
            assert!(
                rect.bottom() <= ok.top() && rect.top() >= 0.0,
                "{kind}: {row} shows above the buttons, not scrolled away: {rect:?} over {ok:?}"
            );
        }
    }
}
