//! Sue Paint's dialogs by family (P4-8): the one dialog host, sized by
//! what it holds; New with each kind of work's own settings (an
//! animation's presets, output frame and timeline, a comic's borders,
//! monochrome, a moving illustration) and the presets that keep them;
//! Convert Layer, Create Perspective Ruler and Expand Selected Area.
//! Every step is driven through real frames and found on screen by its
//! text or by what it tells assistive technology, never by a fixed
//! position.

use super::*;
use sue_core::{Entity, EntityId, Value};

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

/// A property of `id`.
fn property(driver: &PaintDriver, id: EntityId, key: &str) -> Option<Value> {
    driver
        .runtime
        .engine
        .world
        .arena
        .get_by_id(id)
        .ok()
        .and_then(|entity| entity.property(key).cloned())
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
    driver.now += std::time::Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    driver.texts = text_rects(&output);
    let found = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let label = node.label()?.to_owned();
            let bounds = node.bounds()?;
            Some((
                label,
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    found
}

/// A click in the middle of the control named `name` (found by what it
/// tells assistive technology), then a few frames.
fn click_named(driver: &mut PaintDriver, name: &str) {
    let controls = told(driver, Vec::new());
    let at = controls
        .iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| rect.center())
        .next()
        .unwrap_or_else(|| {
            let names: Vec<&String> = controls.iter().map(|(label, _)| label).collect();
            panic!("{name:?} is on screen: {names:?}")
        });
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        told(driver, events);
    }
    driver.settle(6);
}

/// Opens File, New (Ctrl+N) and lets it come in.
fn open_new(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    driver.settle(10);
    assert!(driver.rect_of("New").is_some(), "the New dialog is open");
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

/// Undoes the last step and compares the arena with `before`, then redoes
/// it and compares with how it was after (rule 7: ids included).
fn undo_pair(driver: &mut PaintDriver, before: &[Entity], depth_before: usize) {
    let after = entities(&driver.runtime);
    let depth_after = driver.runtime.engine.history.depth();
    walk_to(driver, depth_before);
    assert!(
        entities(&driver.runtime) == before,
        "undo gives the picture back as it was"
    );
    walk_to(driver, depth_after);
    assert!(
        entities(&driver.runtime) == after,
        "redo gives the result back"
    );
}

#[test]
fn every_dialog_is_as_tall_as_what_it_holds_and_its_buttons_show() {
    // The filters whose dialogs were taller than the old size table gave
    // them: their OK and Cancel are on screen now, inside the window.
    let mut driver = PaintDriver::new("paint-dialogs-sized");
    for command in [
        "filter.drop_shadow",
        "filter.focus_lines",
        "filter.speed_lines",
    ] {
        driver.command(command);
        driver.settle(12);
        let ok = driver
            .rect_of("OK")
            .unwrap_or_else(|| panic!("{command}: OK is drawn"));
        let cancel = driver.rect_of("Cancel").expect("Cancel is drawn");
        assert!(ok.bottom() < 900.0 && cancel.bottom() < 900.0, "{command}");
        driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        driver.settle(6);
        assert!(
            driver.rect_of("OK").is_none(),
            "{command}: Escape closes it"
        );
    }
    // A short dialog is short, a long one long: Change Canvas Size ends
    // well above New's end.
    driver.command("edit.canvas_size");
    driver.settle(12);
    let short = driver.rect_of("OK").expect("Change Canvas Size has OK");
    driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    driver.settle(6);
    open_new(&mut driver);
    click_named(&mut driver, "Animation");
    let long = driver.rect_of("OK").expect("New has OK");
    assert!(
        long.bottom() - short.bottom() > 100.0,
        "the animation New is taller: {long:?} against {short:?}"
    );
    // The footer stands in Sue's one order: OK before Cancel, and the
    // extra action (Register preset) at the leading edge.
    let cancel = driver.rect_of("Cancel").unwrap();
    let register = driver.rect_of("Register preset").unwrap();
    assert!(register.left() < long.left() && long.left() < cancel.left());
}

#[test]
fn new_animation_from_the_1920_preset_has_its_blank_space_and_timeline() {
    // A25V-1, A25V-2, A25V-3, ABV-1, AB-24, A25-9, A25-20, A25-30.
    let mut driver = PaintDriver::new("paint-dialogs-new-animation");
    open_new(&mut driver);
    click_named(&mut driver, "Animation");
    // The 1920 x 1080 preset with blank space, as an animation starts.
    let preset = "1920 x 1080 (192 dpi) with blank space";
    assert!(driver.rect_of(preset).is_some(), "{:?}", driver.texts);
    for text in [
        "Size of output frame",
        "Title-safe area",
        "Overflow frame",
        "Blank space",
        "Timeline name",
        "Scene number",
        "Shot number",
        "Division line",
        "5 s 0 f (120 frames)",
    ] {
        assert!(driver.rect_of(text).is_some(), "{text} is shown");
    }
    // The five animation presets are in the list.
    click_named(&mut driver, "Preset");
    for name in [
        "720 x 540 (144 dpi) with blank space",
        "1280 x 720 (144 dpi) with blank space",
        "1080 x 1920 (72 dpi)",
        "1080 x 1080 (72 dpi)",
    ] {
        assert!(driver.rect_of(name).is_some(), "{name} is offered");
    }
    // Picking the square one follows its frame rate (30).
    driver.click_text("1080 x 1080 (72 dpi)");
    assert!(
        driver.rect_of("4 s 0 f (120 frames)").is_some(),
        "{:?}",
        driver.texts
    );
    click_named(&mut driver, "Preset");
    driver.click_text(preset);
    driver.click_text("OK");
    driver.settle(10);
    assert!(
        driver.rect_of("Size of output frame").is_none(),
        "New closed"
    );
    let made = picture(&driver);
    let size = sue_model::picture::size(driver.runtime.engine.world.arena.get_by_id(made).unwrap());
    assert_eq!(size, Some([2304, 1296]), "the frame with its blank space");
    let frame = sue_paint::anim::setup::OutputFrame::of(&driver.runtime.engine.world.arena, made)
        .expect("the output frame is kept");
    assert_eq!(frame.size, [1920, 1080]);
    assert_eq!(frame.blank, [108, 108, 192, 192]);
    assert_eq!(frame.rect(), [192, 108, 1920, 1080]);
    let settings = sue_paint::anim::setup::Settings::of(&driver.runtime.engine.world.arena, made);
    assert_eq!(settings.name, "Timeline 1");
    assert_eq!(
        (settings.scene, settings.shot, settings.division),
        (1, 1, 6)
    );
    assert_eq!(
        property(&driver, made, sue_model::picture::DPI).and_then(|v| v.as_f64()),
        Some(192.0)
    );
    // What New set beyond the canvas is one step of the new canvas's
    // history: undone, the output frame and the timeline's settings go;
    // redone, they are back as they were, ids and all.
    let after = entities(&driver.runtime);
    let depth_after = driver.runtime.engine.history.depth();
    walk_to(&mut driver, depth_after - 1);
    let arena = &driver.runtime.engine.world.arena;
    assert!(sue_paint::anim::setup::OutputFrame::of(arena, made).is_none());
    assert_eq!(
        sue_paint::anim::setup::Settings::of(arena, made).division,
        6,
        "the default again"
    );
    walk_to(&mut driver, depth_after);
    assert!(entities(&driver.runtime) == after, "redo brings it back");
}

#[test]
fn new_playback_time_reads_in_seconds_and_frames() {
    // A25-20: the playback time in the unit chosen.
    let mut driver = PaintDriver::new("paint-dialogs-playback-time");
    open_new(&mut driver);
    click_named(&mut driver, "Animation");
    click_named(&mut driver, "Playback time unit");
    driver.click_text("Seconds and frames");
    for text in [" s", " f", "5 s 0 f (120 frames)"] {
        assert!(
            driver.texts.iter().any(|(found, _)| found.contains(text)),
            "{text}: {:?}",
            driver.texts
        );
    }
}

#[test]
fn new_comic_with_every_setting_in_monochrome() {
    // FG-18, FGV-46: gray and monochrome, its inks and tone frequency,
    // the comic's bleed and border, and what Show all comic settings
    // shows more.
    let mut driver = PaintDriver::new("paint-dialogs-new-comic");
    open_new(&mut driver);
    click_named(&mut driver, "Comic");
    for text in ["Bleed width", "Inner border"] {
        assert!(driver.rect_of(text).is_some(), "{text} is shown");
    }
    assert!(driver.rect_of("Author").is_none(), "a plain comic hides it");
    click_named(&mut driver, "Show all comic settings");
    for text in ["Story information", "Author", "First page number"] {
        assert!(driver.rect_of(text).is_some(), "{text} is shown");
    }
    driver.click_text("Monochrome");
    for text in ["Drawing colors", "Black", "White", "Default tone frequency"] {
        assert!(driver.rect_of(text).is_some(), "{text} is shown");
    }
    driver.click_text("OK");
    driver.settle(10);
    let page = picture(&driver);
    assert_eq!(
        property(&driver, page, sue_paint::story::PAGE_BLEED).and_then(|v| v.as_f64()),
        Some(3.0)
    );
    assert_eq!(
        property(&driver, page, sue_paint::story::PAGE_INNER).and_then(|v| v.as_f64()),
        Some(10.0)
    );
    assert_eq!(
        property(&driver, page, "paint.expression").and_then(|v| v.as_i64()),
        Some(2)
    );
    assert_eq!(
        property(&driver, page, "paint.tone.frequency").and_then(|v| v.as_f64()),
        Some(60.0)
    );
    let layer = sue_paint::doc::layers(&driver.runtime.engine.world.arena, page)
        .into_iter()
        .find(|info| info.kind == sue_model::picture::LayerKind::Raster)
        .expect("the page's first layer");
    assert_eq!(
        property(&driver, layer.id, sue_paint::effects::EXPRESSION).and_then(|v| v.as_i64()),
        Some(2),
        "the first layer shows in monochrome"
    );
}

#[test]
fn new_moving_illustration_gets_a_timeline() {
    // TRV-10: Create moving illustration.
    let mut driver = PaintDriver::new("paint-dialogs-new-moving");
    open_new(&mut driver);
    click_named(&mut driver, "Create animated illustration");
    assert!(driver.rect_of("Frame rate").is_some(), "{:?}", driver.texts);
    driver.click_text("OK");
    driver.settle(10);
    let made = picture(&driver);
    assert!(
        sue_paint::anim::timeline(&driver.runtime.engine.world.arena, made).is_some(),
        "the illustration has a timeline"
    );
    let size = sue_model::picture::size(driver.runtime.engine.world.arena.get_by_id(made).unwrap());
    assert_eq!(size, Some([1600, 1200]), "an illustration's own size");
}

#[test]
fn a_registered_preset_brings_back_the_kind_of_work() {
    // CT-32: a canvas preset keeps more than size, dpi and paper.
    let mut driver = PaintDriver::new("paint-dialogs-preset");
    open_new(&mut driver);
    click_named(&mut driver, "Animation");
    driver.click_text("Register preset");
    let name = "1920 x 1080 (1)";
    assert!(driver.rect_of(name).is_some(), "{:?}", driver.texts);
    driver.click_text("Cancel");
    open_new(&mut driver);
    click_named(&mut driver, "Illustration");
    assert!(driver.rect_of("Size of output frame").is_none());
    click_named(&mut driver, "Preset");
    driver.click_text(name);
    assert!(
        driver.rect_of("Size of output frame").is_some(),
        "the preset made it an animation again"
    );
    assert!(driver.rect_of("Blank space").is_some());
    driver.click_text("Cancel");
}

#[test]
fn convert_layer_sets_expression_and_blend_and_keeps_the_original() {
    // FGV-7, FG-34: Convert Layer's expression color, blending mode and
    // Keep original layer, as one undo step.
    let mut driver = PaintDriver::new("paint-dialogs-convert");
    let page = picture(&driver);
    let layers_before = sue_paint::doc::layers(&driver.runtime.engine.world.arena, page).len();
    let before = entities(&driver.runtime);
    let depth = driver.runtime.engine.history.depth();
    driver.command("layer.convert");
    driver.settle(10);
    assert!(driver.rect_of("Keep original layer").is_some());
    click_named(&mut driver, "Expression Color");
    driver.click_text("Gray");
    click_named(&mut driver, "Blending mode");
    driver.click_text("Multiply");
    driver.click_text("Keep original layer");
    driver.click_text("Raster layer");
    driver.click_text("OK");
    driver.settle(6);
    let arena = &driver.runtime.engine.world.arena;
    let layers = sue_paint::doc::layers(arena, page);
    assert_eq!(layers.len(), layers_before + 1, "the original stays");
    let converted = layers
        .iter()
        .find(|info| {
            arena
                .get_by_id(info.id)
                .ok()
                .and_then(|entity| entity.property(sue_paint::effects::EXPRESSION))
                .and_then(Value::as_i64)
                == Some(1)
        })
        .expect("a gray layer");
    assert_eq!(
        property(&driver, converted.id, sue_model::picture::BLEND)
            .and_then(|v| v.as_str().map(str::to_owned)),
        Some(sue_model::BlendMode::Multiply.name().to_owned())
    );
    assert_eq!(
        driver.runtime.engine.history.depth(),
        depth + 1,
        "one undo step"
    );
    undo_pair(&mut driver, &before, depth);
}

#[test]
fn a_perspective_ruler_lands_on_a_new_layer() {
    // FGV-2, CTV-10: Create new layer, on at first.
    let mut driver = PaintDriver::new("paint-dialogs-perspective");
    let page = picture(&driver);
    let before = entities(&driver.runtime);
    let depth = driver.runtime.engine.history.depth();
    driver.command("layer.ruler.perspective");
    driver.settle(10);
    assert!(driver.rect_of("Create new layer").is_some());
    driver.click_text("OK");
    driver.settle(6);
    let arena = &driver.runtime.engine.world.arena;
    assert!(
        sue_paint::doc::layers(arena, page)
            .iter()
            .any(|info| info.name == "Perspective ruler"),
        "the ruler's own layer"
    );
    assert!(
        property(&driver, page, sue_paint::rulers::PERSPECTIVE).is_some(),
        "the ruler is laid"
    );
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1);
    undo_pair(&mut driver, &before, depth);
}

#[test]
fn expand_selected_area_offers_round_or_square_corners() {
    // FGV-50: the expansion type.
    let mut driver = PaintDriver::new("paint-dialogs-expand");
    driver.command("select.all");
    driver.settle(4);
    driver.command("select.expand");
    driver.settle(10);
    for text in ["Expansion type", "Round corners", "Square corners"] {
        assert!(driver.rect_of(text).is_some(), "{text}: {:?}", driver.texts);
    }
    driver.click_text("Square corners");
    driver.click_text("OK");
    driver.settle(6);
    assert!(
        driver.rect_of("Expansion type").is_none(),
        "the dialog closed"
    );
}
