//! Settings and the update window on Sue's shared controls (P4-18),
//! driven through real frames: every control walked to with Tab shows a
//! focus ring and names itself in a tooltip; the icon page tells an
//! optional filled twin from a file that could not be read; the update
//! card offers a release, reads its notes whole and says when a check
//! failed.

use super::*;

/// A frame of `paint` with `events`, keeping what it drew: the texts as
/// [`PaintDriver::step`] keeps them, and the rectangles outlined with no
/// with the width of their line (filled or not: a field shows focus by
/// its edge), and the circles drawn with a line.
fn frame(paint: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(egui::Rect, f32)> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    paint.texts = text_rects(&output);
    let mut outlines = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| match shape {
            egui::Shape::Rect(rect) if rect.stroke.color.a() > 0 => {
                outlines.push((rect.rect, rect.stroke.width));
            }
            egui::Shape::Circle(circle) if circle.stroke.color.a() > 0 => {
                let side = egui::Vec2::splat(circle.radius * 2.0);
                outlines.push((
                    egui::Rect::from_center_size(circle.center, side),
                    circle.stroke.width,
                ));
            }
            _ => {}
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    outlines
}

/// Frames until the focus ring has faded in; the outlines of the last.
fn settle_ring(paint: &mut PaintDriver) -> Vec<(egui::Rect, f32)> {
    let mut outlines = Vec::new();
    for _ in 0..20 {
        outlines = frame(paint, Vec::new());
    }
    outlines
}

/// What holds keyboard focus, and where it was drawn.
fn focused(paint: &PaintDriver) -> Option<(egui::Id, egui::Rect)> {
    let id = paint.ctx.memory(|memory| memory.focused())?;
    let response = paint.ctx.read_response(id)?;
    Some((id, response.rect))
}

/// Opens Settings at the page `id`.
fn open(paint: &mut PaintDriver, id: &str) {
    paint.runtime.request(
        ShellRequest::OpenSettings(Some(id.to_owned())),
        &mut NoTextures,
        paint.now,
    );
    paint.settle(8);
}

/// Walks the controls of the open Settings page with Tab, up to `most`
/// of them, and gives what is wrong: a control with no focus ring, or
/// one that names nothing when the pointer rests on it.
fn walk(paint: &mut PaintDriver, page: &str, most: usize) -> (usize, Vec<String>) {
    let window = paint.runtime.shell.window_rect().expect("Settings is open");
    let mut faults = Vec::new();
    let mut seen = Vec::new();
    let mut before = settle_ring(paint);
    for _ in 0..most {
        frame(paint, vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
        let outlines = settle_ring(paint);
        let Some((id, rect)) = focused(paint) else {
            break;
        };
        if seen.contains(&id) {
            break;
        }
        seen.push(id);
        // Focus left Settings (Tab went round to Sue's own bar): done.
        if !window.contains(rect.center()) {
            break;
        }
        // A ring: a strong outline on the control (around it, its edge,
        // or around a part of it such as a slider's knob) that was not
        // there before focus came.
        let ring = outlines.iter().any(|(drawn, width)| {
            *width >= 1.5
                && rect.expand(6.0).contains_rect(*drawn)
                && drawn.width() >= 6.0
                && !before.iter().any(|(old, was)| {
                    (old.min - drawn.min).length() < 0.5
                        && (old.max - drawn.max).length() < 0.5
                        && (was - width).abs() < 0.1
                })
        });
        let inside: Vec<&str> = paint
            .texts
            .iter()
            .filter(|(_, at)| rect.expand(2.0).contains(at.center()))
            .map(|(text, _)| text.as_str())
            .collect();
        let place = format!("{page}: the control at {rect:?} {inside:?}");
        if !ring {
            faults.push(format!("{place} shows no focus ring"));
        }
        // The pointer rests on it: a tooltip names it, drawn outside it.
        // One cut by the page's edge is scrolled into the middle first.
        let mut rect = rect;
        if !window.shrink(60.0).contains_rect(rect) {
            let at = window.center();
            frame(paint, vec![egui::Event::PointerMoved(at)]);
            frame(
                paint,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, at.y - rect.center().y),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            paint.settle(30);
            if let Some((_, moved)) = focused(paint) {
                rect = moved;
            }
        }
        let shown: Vec<(String, egui::Rect)> = paint.texts.clone();
        frame(paint, vec![egui::Event::PointerMoved(rect.center())]);
        for _ in 0..70 {
            frame(paint, Vec::new());
        }
        // Words drawn outside the control where none were before.
        let tip = paint.texts.iter().any(|(text, at)| {
            !text.is_empty()
                && !rect.contains(at.center())
                && !shown
                    .iter()
                    .any(|(old, place)| old == text && (place.min - at.min).length() < 1.0)
        });
        if !tip {
            faults.push(format!("{place} shows no tooltip"));
        }
        // Away again, so the next tip starts from nothing.
        frame(
            paint,
            vec![egui::Event::PointerMoved(Pos2::new(4.0, 890.0))],
        );
        before = settle_ring(paint);
    }
    (seen.len(), faults)
}

/// Every control of Settings that Tab reaches, on the pages most people
/// open (the look with its icon packs, the sounds, the studios' and the
/// keys), shows a focus ring and names itself in a tooltip.
#[test]
fn every_control_of_settings_shows_a_focus_ring_and_a_tooltip() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-focus");
    let mut faults = Vec::new();
    for (page, most) in [
        ("general", 30),
        ("appearance", 40),
        (sue_paint::ID, 30),
        ("keymaps", 20),
    ] {
        open(&mut paint, page);
        let (walked, found) = walk(&mut paint, page, most);
        assert!(walked >= 5, "{page}: Tab reached only {walked} controls");
        faults.extend(found);
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
    paint.runtime.shell.close_window();
}

/// Writes an icon pack of filled squares into `folder`: `names`, each as
/// `<name>.svg`, and the files `extra` as they are.
fn write_pack(folder: &std::path::Path, names: &[&str], extra: &[(&str, &str)]) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(
        folder.join("icon-pack.txt"),
        "format = 1\nname = Square Test\nauthor = Sue's tests\nlicence = CC0-1.0\nversion = 2\n",
    )
    .unwrap();
    let square = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" fill="black"/></svg>"#;
    for name in names {
        std::fs::write(folder.join(format!("{name}.svg")), square).unwrap();
    }
    for (file, text) in extra {
        std::fs::write(folder.join(file), text).unwrap();
    }
}

/// Installs the pack in `folder` through Install from Folder.
fn install(paint: &mut PaintDriver, folder: &std::path::Path) {
    open(paint, "appearance");
    let button = "Install from Folder...";
    for _ in 0..30 {
        if paint
            .rect_of(button)
            .is_some_and(|rect| rect.bottom() < 860.0)
        {
            break;
        }
        scroll(paint, 200.0);
    }
    paint.click_text(button);
    let now = paint.now;
    assert!(
        paint.runtime.answer_pick(folder, &mut NoTextures, now),
        "Install from Folder asked for a folder"
    );
    paint.settle(8);
}

/// Turns the mouse wheel over the middle of Settings' page.
fn scroll(paint: &mut PaintDriver, by: f32) {
    let at = paint
        .runtime
        .shell
        .window_rect()
        .expect("Settings is open")
        .center()
        + vec2(150.0, 0.0);
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -by),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    }]);
    paint.settle(20);
}

/// The owner's report: a pack that leaves out the optional filled twins
/// of star, heart, eye and lock read "4 things could not be read". A
/// twin left out is said plainly and counted as nothing that failed; a
/// file that really cannot be read is counted, and listed on a click.
#[test]
fn an_icon_pack_without_its_filled_twins_reads_no_problem() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-icon-twins");
    let folder = paint.root.join("square-pack");
    write_pack(&folder, &["folder", "star", "heart", "eye", "lock"], &[]);
    install(&mut paint, &folder);
    assert!(shows(&paint, "Installed Square Test."), "{:?}", paint.texts);
    let said =
        |paint: &PaintDriver, part: &str| paint.texts.iter().any(|(text, _)| text.contains(part));
    assert!(!said(&paint, "could not be read"), "{:?}", paint.texts);
    // Its card: who made it, how many icons, and the twins in plain words.
    scroll(&mut paint, 400.0);
    assert!(
        said(&paint, "By Sue's tests, CC0-1.0, version 2. Draws 5 icons."),
        "{:?}",
        paint.texts
    );
    assert!(
        said(&paint, "Draws no filled twin for star, heart, eye, lock")
            || said(&paint, "Draws no filled twin for eye, heart, lock, star"),
        "{:?}",
        paint.texts
    );
    assert!(said(&paint, "Twins are optional."), "{:?}", paint.texts);
    // Chosen, Sue draws with it: the note under Icons stays quiet too.
    paint.click_text("Square Test");
    paint.settle(6);
    let home = sue_wm::icons::pack::IconHome::of(&paint.runtime.engine.paths);
    let chosen = home.chosen().expect("a pack is chosen");
    let pack = home.pack(&chosen).expect("the chosen pack is installed");
    assert_eq!(pack.manifest.name, "Square Test");
    assert!(!said(&paint, "could not be read"), "{:?}", paint.texts);

    // A pack with a file named after no icon: that one is a problem.
    let broken = paint.root.join("broken-pack");
    write_pack(
        &broken,
        &["folder"],
        &[(
            "no-such-icon.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )],
    );
    std::fs::write(
        broken.join("icon-pack.txt"),
        "format = 1\nname = Broken Test\nauthor = Sue's tests\nlicence = CC0-1.0\nversion = 1\n",
    )
    .unwrap();
    install(&mut paint, &broken);
    // Its card counts the one problem; the line at the top of Icons says
    // it too.
    assert!(
        shows(
            &paint,
            "1 file could not be read; Sue's own icon stands in for it."
        ),
        "{:?}",
        paint.texts
    );
    let said_on_install =
        "Installed Broken Test. 1 file could not be read; Sue's own icon stands in for it.";
    for _ in 0..10 {
        if shows(&paint, said_on_install) {
            break;
        }
        scroll(&mut paint, -200.0);
    }
    assert!(shows(&paint, said_on_install), "{:?}", paint.texts);
    for _ in 0..10 {
        if paint
            .rect_of("Show What Could Not Be Read")
            .is_some_and(|rect| rect.bottom() < 860.0)
        {
            break;
        }
        scroll(&mut paint, 200.0);
    }
    paint.click_text("Show What Could Not Be Read");
    assert!(said(&paint, "no-such-icon.svg"), "{:?}", paint.texts);
    paint.runtime.shell.close_window();
}

/// `true` when the last frame drew `text`.
fn shows(paint: &PaintDriver, text: &str) -> bool {
    paint.texts.iter().any(|(found, _)| found == text)
}

/// A release made here, with notes.
fn release() -> sue_platform::update::Release {
    sue_platform::update::Release {
        tag: "v9.9.9".to_owned(),
        name: "Sue 9.9.9".to_owned(),
        notes:
            "## What is new\n\n- A **better** card.\n- Notes read [plainly](https://example.com).\n"
                .to_owned(),
        page: "https://example.com/releases/v9.9.9".to_owned(),
        assets: Vec::new(),
    }
}

/// The update card names the release and shows its notes; Release Notes
/// opens them whole in Sue's dialog, which Close or Escape closes; Later puts the
/// card away. A failed check says so and offers Try Again.
#[test]
fn the_update_card_offers_a_release_and_reads_its_notes() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-update-card");
    paint.runtime.updater_mut().offer(release());
    paint.settle(8);
    assert!(shows(&paint, "Sue 9.9.9 is out"), "{:?}", paint.texts);
    assert!(shows(&paint, "A better card."), "{:?}", paint.texts);
    assert!(shows(&paint, "Notes read plainly."), "{:?}", paint.texts);
    let card = paint.rect_of("Sue 9.9.9 is out").expect("the card's title");
    paint.click_text("Release Notes");
    paint.settle(20);
    assert!(
        shows(&paint, "What is new in Sue 9.9.9"),
        "{:?}",
        paint.texts
    );
    assert!(shows(&paint, "Release Page"), "{:?}", paint.texts);
    paint.click_text("Close");
    paint.settle(30);
    assert!(
        !shows(&paint, "What is new in Sue 9.9.9"),
        "Close closes the notes: {:?}",
        paint.texts
    );
    // Escape closes them too.
    paint.click_text("Release Notes");
    paint.settle(20);
    assert!(
        shows(&paint, "What is new in Sue 9.9.9"),
        "{:?}",
        paint.texts
    );
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(30);
    assert!(
        !shows(&paint, "What is new in Sue 9.9.9"),
        "Escape closes the notes: {:?}",
        paint.texts
    );
    assert!(
        shows(&paint, "Sue 9.9.9 is out"),
        "the card stays: {:?}",
        paint.texts
    );
    paint.click_text("Later");
    paint.settle(30);
    assert!(
        !shows(&paint, "Sue 9.9.9 is out"),
        "Later puts it away: {:?}",
        paint.texts
    );
    assert!(
        card.right() > 1000.0,
        "the card sits at the right: {card:?}"
    );
    // A check that failed.
    paint
        .runtime
        .updater_mut()
        .fail("No network here.".to_owned());
    paint.settle(8);
    assert!(shows(&paint, "No network here."), "{:?}", paint.texts);
    assert!(shows(&paint, "Try Again"), "{:?}", paint.texts);
    let outlines = {
        let at = paint.rect_of("Try Again").unwrap();
        paint.step(vec![egui::Event::PointerMoved(at.center())]);
        for _ in 0..70 {
            paint.step(Vec::new());
        }
        paint
            .texts
            .iter()
            .filter(|(text, _)| text == "Try Again")
            .count()
    };
    assert!(
        outlines >= 2,
        "Try Again names itself in a tooltip: {:?}",
        paint.texts
    );
}
