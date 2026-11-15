//! Sue Paint's palettes keep what they hold in sight: Quick Access's
//! set fits the palette that pops out of the strip (or scrolls, with its
//! bar showing, where the window is too short), and the Sub Tool list
//! scrolls to the sub tool chosen.

use super::*;

/// One frame of a window `size` points big with `events`: every piece of
/// text drawn, where, and the rect it is clipped to.
fn frame(
    driver: &mut PaintDriver,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect, egui::Rect)> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            vec2(size[0], size[1]),
        )),
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
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                found.push((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                    clipped.clip_rect,
                ));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// `count` frames of a window `size` big; what the last one draws.
fn settle(
    driver: &mut PaintDriver,
    size: [f32; 2],
    count: usize,
) -> Vec<(String, egui::Rect, egui::Rect)> {
    let mut texts = Vec::new();
    for _ in 0..count {
        texts = frame(driver, size, Vec::new());
    }
    texts
}

/// Where the text starting with `start` is drawn, and the rect it is
/// clipped to: the topmost when it is drawn more than once.
fn find(texts: &[(String, egui::Rect, egui::Rect)], start: &str) -> (egui::Rect, egui::Rect) {
    texts
        .iter()
        .filter(|(text, ..)| text.starts_with(start))
        .map(|(_, rect, clip)| (*rect, *clip))
        .min_by(|a, b| a.0.top().total_cmp(&b.0.top()))
        .unwrap_or_else(|| {
            let all: Vec<&String> = texts.iter().map(|(text, ..)| text).collect();
            panic!("{start:?} is drawn: {all:?}")
        })
}

/// Whether all of `rect` shows inside `clip` (to a fraction of a point).
fn whole(rect: egui::Rect, clip: egui::Rect) -> bool {
    clip.expand(0.5).contains_rect(rect)
}

#[test]
fn quick_access_set_one_fits_the_palette_that_pops_out_of_the_strip() {
    let mut driver = PaintDriver::fresh("paint-fit-quick-access");
    let size = [1440.0, 900.0];
    settle(&mut driver, size, 4);
    // In the default layout Quick Access is an icon of the strip; Window,
    // Quick Access pops it out.
    driver.command("window.quick_access");
    let texts = settle(&mut driver, size, 8);
    let (undo, _) = find(&texts, "Undo");
    let (settings, clip) = find(&texts, "Quick Access Settings");
    assert!(
        whole(settings, clip),
        "Set 1's last button, its settings, shows whole: {settings:?} in {clip:?}"
    );
    assert!(
        settings.top() > undo.bottom(),
        "it is the set's last button"
    );
    assert!(settings.bottom() <= size[1], "inside the window");
}

#[test]
fn quick_access_scrolls_where_the_window_is_too_short_for_the_set() {
    let mut driver = PaintDriver::fresh("paint-fit-quick-access-short");
    let size = [1440.0, 560.0];
    settle(&mut driver, size, 4);
    driver.command("window.quick_access");
    let texts = settle(&mut driver, size, 8);
    let (settings, clip) = find(&texts, "Quick Access Settings");
    assert!(
        !whole(settings, clip),
        "a window this short cannot show the whole set: {settings:?} in {clip:?}"
    );
    assert!(
        clip.bottom() <= size[1] + 0.5,
        "the palette ends in the window"
    );
    // The wheel over the set brings its last button into sight.
    let (undo, _) = find(&texts, "Undo");
    let over = undo.center();
    frame(&mut driver, size, vec![egui::Event::PointerMoved(over)]);
    for _ in 0..6 {
        frame(
            &mut driver,
            size,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -200.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            }],
        );
    }
    let texts = settle(&mut driver, size, 30);
    let (settings, clip) = find(&texts, "Quick Access Settings");
    assert!(
        whole(settings, clip),
        "scrolled to the end, the last button shows: {settings:?} in {clip:?}"
    );
}

#[test]
fn the_sub_tool_list_scrolls_to_the_sub_tool_chosen() {
    let mut driver = PaintDriver::fresh("paint-fit-sub-tool");
    let size = [1440.0, 900.0];
    settle(&mut driver, size, 4);
    // The Operation tool's last sub tool lies below the list's edge in
    // the default layout until the list follows the choice.
    driver.command("subtool.operation.Light_table");
    let texts = settle(&mut driver, size, 30);
    let (chosen, clip) = find(&texts, "Light table");
    assert!(
        whole(chosen, clip),
        "the chosen sub tool shows whole: {chosen:?} in {clip:?}"
    );
    // Back to the first: the list follows it up again.
    driver.command("subtool.operation.Object");
    let texts = settle(&mut driver, size, 30);
    let (first, clip) = find(&texts, "Object");
    assert!(whole(first, clip), "{first:?} in {clip:?}");
}
