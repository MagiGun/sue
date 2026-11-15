//! Sue Paint: stories, frames, balloons, text, batch import.

use super::*;

#[test]
fn sue_paint_stories_add_step_through_delete_and_export_pages() {
    let (mut runtime, root) = runtime("paint-story");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let current = |runtime: &Runtime| {
        runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
    };
    let first = current(&runtime).unwrap();
    let pages = |runtime: &Runtime| {
        let arena = &runtime.engine.world.arena;
        sue_paint::story::story_of(arena, first)
            .map(|story| sue_paint::story::pages(arena, story))
            .unwrap_or_default()
    };

    // Add Page makes the picture a story and opens the new second page.
    command(&mut runtime, "story.add");
    step(&mut runtime, Vec::new());
    assert_eq!(pages(&runtime).len(), 2);
    assert_eq!(current(&runtime), Some(pages(&runtime)[1]));
    // Insert Page goes before the current one.
    command(&mut runtime, "story.insert");
    step(&mut runtime, Vec::new());
    let now_pages = pages(&runtime);
    assert_eq!(now_pages.len(), 3);
    assert_eq!(current(&runtime), Some(now_pages[1]));
    // Stepping through.
    command(&mut runtime, "story.last");
    assert_eq!(current(&runtime), Some(now_pages[2]));
    command(&mut runtime, "story.first");
    assert_eq!(current(&runtime), Some(first));
    command(&mut runtime, "story.next");
    assert_eq!(current(&runtime), Some(now_pages[1]));
    // Delete Page opens the next one; undo brings the page back.
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "story.delete");
    step(&mut runtime, Vec::new());
    assert_eq!(pages(&runtime).len(), 2);
    assert_eq!(current(&runtime), Some(now_pages[2]));
    assert_eq!(runtime.engine.history.depth(), depth + 1);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(pages(&runtime), now_pages);

    // Export All Pages writes one numbered file per page.
    std::fs::create_dir_all(&root).unwrap();
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(paint.clone()),
                "story-pages",
                "Export All Pages",
                "Book.png",
            ),
            path: root.join("Book.png"),
        },
        &mut NoTextures,
        start,
    );
    for number in 1..=3 {
        let path = root.join(format!("Book-{number:03}.png"));
        assert!(path.exists(), "{} was written", path.display());
    }

    // The story lives in the project, its pages in order.
    let project = root.join("story.sue");
    runtime.save_project_to(&project).unwrap();
    let bytes = std::fs::read(&project).unwrap();
    runtime.open_project(&project, &bytes).unwrap();
    assert_eq!(pages(&runtime), now_pages);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_makes_manga_frames_balloons_and_focus_lines() {
    let (mut runtime, root) = runtime("paint-manga");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let layer_of = |runtime: &Runtime, class: &str| {
        let arena = &runtime.engine.world.arena;
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .find(|info| {
                arena
                    .get_by_id(info.id)
                    .is_ok_and(|entity| entity.class() == class)
            })
            .map(|info| info.id)
    };
    let text_of = |runtime: &Runtime, layer, key| {
        runtime
            .engine
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| {
                entity
                    .property(key)
                    .and_then(sue_core::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let drag =
        |step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>), runtime: &mut Runtime, from, to| {
            step(
                runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            step(runtime, vec![egui::Event::PointerMoved(to)]);
            step(runtime, vec![press(to, false)]);
        };
    let middle = paint_middle(&runtime);

    // A frame border folder of the manga layout: five panels.
    command(&mut runtime, "layer.new.frame");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let frame = layer_of(&runtime, sue_paint::manga::FRAME).expect("a frame layer");
    let panels = |runtime: &Runtime| {
        sue_paint::manga::decode_panels(&text_of(runtime, frame, sue_paint::manga::PANELS)).len()
    };
    assert_eq!(panels(&runtime), 5);
    let click =
        |step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>), runtime: &mut Runtime, at| {
            step(
                runtime,
                vec![egui::Event::PointerMoved(at), press(at, true)],
            );
            step(runtime, vec![press(at, false)]);
        };
    // The polyline frame: corners clicked out, Enter closes the panel.
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let corners = [
        middle + vec2(-120.0, 60.0),
        middle + vec2(-20.0, 50.0),
        middle + vec2(-30.0, 140.0),
        middle + vec2(-130.0, 130.0),
    ];
    for corner in corners {
        click(&mut step, &mut runtime, corner);
    }
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(panels(&runtime), 6, "a polyline panel");
    // The Object tool drags one of its corners, as one step.
    let corner_of = |runtime: &Runtime| {
        sue_paint::manga::decode_panels(&text_of(runtime, frame, sue_paint::manga::PANELS))
            .last()
            .map(|panel| panel[1])
            .unwrap()
    };
    let was = corner_of(&runtime);
    step(&mut runtime, vec![key(egui::Key::O, plain)]);
    let depth = runtime.engine.history.depth();
    drag(
        &mut step,
        &mut runtime,
        corners[1],
        corners[1] + vec2(30.0, 0.0),
    );
    let moved = corner_of(&runtime);
    assert!(
        moved[0] > was[0] && (moved[1] - was[1]).abs() < 0.5,
        "{was:?} {moved:?}"
    );
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    for _ in 0..2 {
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
    }
    assert_eq!(panels(&runtime), 5, "undone");
    // The Frame Border tool's division cuts the middle panel in two.
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let depth = runtime.engine.history.depth();
    drag(
        &mut step,
        &mut runtime,
        middle - vec2(0.0, 30.0),
        middle + vec2(0.0, 30.0),
    );
    assert_eq!(panels(&runtime), 6, "divided");
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(panels(&runtime), 5, "undone");

    // A balloon and its tail on a balloon layer.
    step(&mut runtime, vec![key(egui::Key::T, plain)]);
    step(&mut runtime, vec![key(egui::Key::T, plain)]);
    drag(
        &mut step,
        &mut runtime,
        middle - vec2(40.0, 30.0),
        middle + vec2(40.0, 30.0),
    );
    let balloon = layer_of(&runtime, sue_paint::manga::BALLOON).expect("a balloon layer");
    let shapes = |runtime: &Runtime| {
        sue_paint::manga::decode_shapes(&text_of(runtime, balloon, sue_paint::manga::SHAPES)).len()
    };
    assert_eq!(shapes(&runtime), 1);
    let kinds = |runtime: &Runtime| {
        sue_paint::manga::decode_shapes(&text_of(runtime, balloon, sue_paint::manga::SHAPES))
    };
    // Clip Studio Paint's Balloon sub tools in turn: rounded, cloud and
    // flash balloons, then a tail and a thought balloon's bubbles.
    let mut at = middle + vec2(-260.0, -200.0);
    for _ in 0..3 {
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
        drag(&mut step, &mut runtime, at, at + vec2(80.0, 50.0));
        at += vec2(110.0, 0.0);
    }
    let made = kinds(&runtime);
    assert!(matches!(
        made.as_slice(),
        [
            sue_paint::manga::Shape::Ellipse(_),
            sue_paint::manga::Shape::Rounded(_),
            sue_paint::manga::Shape::Cloud(_),
            sue_paint::manga::Shape::Flash(_),
        ]
    ));
    // The curve balloon: corners clicked out, a smooth outline through
    // them; the polyline balloon: straight between them, closed by
    // clicking the first corner again.
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let curve = middle + vec2(-150.0, 60.0);
    for corner in [
        vec2(0.0, 0.0),
        vec2(80.0, -10.0),
        vec2(90.0, 50.0),
        vec2(-10.0, 60.0),
    ] {
        click(&mut step, &mut runtime, curve + corner);
    }
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    assert!(matches!(
        kinds(&runtime).last(),
        Some(sue_paint::manga::Shape::Drawn(points)) if points.len() == 40
    ));
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let poly = middle + vec2(0.0, 100.0);
    for corner in [
        vec2(0.0, 0.0),
        vec2(90.0, 0.0),
        vec2(45.0, 70.0),
        vec2(0.0, 0.0),
    ] {
        click(&mut step, &mut runtime, poly + corner);
    }
    assert!(matches!(
        kinds(&runtime).last(),
        Some(sue_paint::manga::Shape::Drawn(points)) if points.len() == 3
    ));
    // The Balloon pen: a balloon in the outline drawn round.
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let round = middle + vec2(200.0, 150.0);
    step(
        &mut runtime,
        vec![
            egui::Event::PointerMoved(round + vec2(60.0, 0.0)),
            press(round + vec2(60.0, 0.0), true),
        ],
    );
    for part in 1..=24 {
        let angle = part as f32 / 24.0 * std::f32::consts::TAU;
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(
                round + vec2(angle.cos() * 60.0, angle.sin() * 40.0),
            )],
        );
    }
    step(&mut runtime, vec![press(round + vec2(60.0, 0.0), false)]);
    assert!(matches!(
        kinds(&runtime).last(),
        Some(sue_paint::manga::Shape::Drawn(points)) if points.len() > 10
    ));
    // The tail.
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    drag(&mut step, &mut runtime, middle, middle + vec2(70.0, 60.0));
    assert_eq!(shapes(&runtime), 8, "a tail");
    assert!(matches!(
        kinds(&runtime).last(),
        Some(sue_paint::manga::Shape::Tail(_))
    ));
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let depth = runtime.engine.history.depth();
    drag(&mut step, &mut runtime, middle, middle + vec2(-90.0, 90.0));
    assert_eq!(shapes(&runtime), 11, "three bubbles");
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");

    // The Figure tool's Saturated line, the first of its own group after
    // the seven of Direct draw: a layer of lines of its own.
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    for _ in 0..7 {
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    }
    let count = sue_paint::doc::layers(&runtime.engine.world.arena, picture).len();
    drag(
        &mut step,
        &mut runtime,
        middle - vec2(40.0, 30.0),
        middle + vec2(40.0, 30.0),
    );
    assert_eq!(
        sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
        count + 1,
        "saturated lines on a new layer"
    );
    // The Object tool moves them, as one undo step.
    let effect = layer_of(&runtime, sue_paint::manga::EFFECT).expect("an effect line layer");
    let spec = |runtime: &Runtime| {
        sue_paint::manga::EffectLines::decode(&text_of(
            runtime,
            effect,
            sue_paint::manga::EFFECT_SPEC,
        ))
        .expect("effect lines")
    };
    let before = spec(&runtime);
    step(&mut runtime, vec![key(egui::Key::O, plain)]);
    let depth = runtime.engine.history.depth();
    drag(&mut step, &mut runtime, middle, middle + vec2(50.0, 20.0));
    let after = spec(&runtime);
    assert!(
        after.a[0] > before.a[0] && after.b[0] > before.b[0],
        "moved"
    );
    assert_eq!(after.count, before.count);
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");

    // Focus lines on a new layer, applied as one step.
    command(&mut runtime, "layer.new");
    step(&mut runtime, Vec::new());
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "filter.focus_lines");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(runtime.engine.history.depth(), depth + 1);
    let arena = &runtime.engine.world.arena;
    let top = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    let raster = sue_paint::doc::raster(arena, top).unwrap();
    let inked = raster
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 128)
        .count();
    assert!(inked > 10_000, "focus lines: {inked} pixels");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_batch_imports_a_folder_one_picture_a_page() {
    let (mut runtime, root) = runtime("paint-batch-import");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    // Three pictures, and a text file that is no picture.
    let folder = root.join("scans");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, shade) in [("b.png", 20_u8), ("a.png", 10), ("c.png", 30)] {
        let tile = sue_core::RasterTile2D::from_bytes(
            8,
            8,
            sue_core::PixelFormat::Rgba8,
            [shade, shade, shade, 255].repeat(64),
        )
        .unwrap();
        std::fs::write(folder.join(name), sue_io::encode_png(&tile).unwrap()).unwrap();
    }
    std::fs::write(folder.join("notes.txt"), "not a picture").unwrap();
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint.clone(),
            command: "file.import.batch".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let _ = runtime.answer_pick(&folder, &mut NoTextures, start);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let arena = &runtime.engine.world.arena;
    let story = sue_paint::story::story_of(arena, picture).expect("a story");
    let pages = sue_paint::story::pages(arena, story);
    assert_eq!(pages.len(), 3, "one page a picture");
    // Each page has its picture as a layer, in name order.
    let names: Vec<Vec<String>> = pages
        .iter()
        .map(|page| {
            sue_paint::doc::layers(arena, *page)
                .into_iter()
                .filter_map(|info| arena.get_by_id(info.id).ok())
                .map(|entity| entity.name().to_owned())
                .collect()
        })
        .collect();
    for (page, name) in names.iter().zip(["a", "b", "c"]) {
        assert!(page.iter().any(|layer| layer.starts_with(name)), "{page:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_types_text_on_the_canvas_as_one_step() {
    let (mut runtime, root) = runtime("paint-typing");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let texts = |runtime: &Runtime| -> Vec<String> {
        let arena = &runtime.engine.world.arena;
        sue_paint::doc::layers(arena, picture)
            .iter()
            .filter_map(|info| arena.get_by_id(info.id).ok())
            .filter(|entity| entity.class() == "sue.paint.text")
            .map(|entity| {
                entity
                    .property("text.content")
                    .and_then(sue_core::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    };
    // The Text tool (T), clicked on the canvas: the words are typed
    // right there, letters that are also shortcuts included.
    step(&mut runtime, vec![key(egui::Key::T, plain)]);
    let middle = runtime.shell.page_rect().center();
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    step(&mut runtime, vec![egui::Event::PointerMoved(middle)]);
    step(&mut runtime, vec![button(middle, true)]);
    step(&mut runtime, vec![button(middle, false)]);
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![egui::Event::Text("Hi".to_owned())]);
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(
        &mut runtime,
        vec![key(egui::Key::B, plain), egui::Event::Text("Bo".to_owned())],
    );
    assert!(texts(&runtime).is_empty(), "kept only when the typing ends");
    step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(texts(&runtime), vec!["Hi\nBo".to_owned()]);
    // One undo takes the words away.
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    assert!(texts(&runtime).is_empty());
    // Text on Curve: a curve drawn, then typed along it.
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    let from = middle + vec2(-120.0, 80.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    for part in 1..=20 {
        let x = part as f32 * 12.0;
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(
                from + vec2(x, -(x / 240.0 * std::f32::consts::PI).sin() * 60.0),
            )],
        );
    }
    step(&mut runtime, vec![button(from + vec2(240.0, 0.0), false)]);
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![egui::Event::Text("Curve".to_owned())]);
    step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(texts(&runtime), vec!["Curve".to_owned()]);
    let arena = &runtime.engine.world.arena;
    let points = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .filter_map(|info| arena.get_by_id(info.id).ok())
        .find_map(|entity| {
            entity
                .property("text.path")
                .and_then(sue_core::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(|path| path.split(';').count())
        })
        .unwrap_or(0);
    assert!(points > 10, "the words keep their curve: {points} points");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_text_tool_writes_and_edits_a_text_layer() {
    let (mut runtime, root) = runtime("paint-text");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    let text_layers = |runtime: &Runtime| -> Vec<sue_core::EntityId> {
        let arena = &runtime.engine.world.arena;
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .filter(|info| {
                arena
                    .get_by_id(info.id)
                    .is_ok_and(|entity| entity.class() == "sue.paint.text")
            })
            .map(|info| info.id)
            .collect()
    };
    // T, a click, the words, Ctrl+Enter.
    step(&mut runtime, vec![key(egui::Key::T, plain)]);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    step(&mut runtime, vec![press(middle, false)]);
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![egui::Event::Text("Hello".to_owned())]);
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    let layers = text_layers(&runtime);
    assert_eq!(layers.len(), 1, "a text layer");
    let content = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .get_by_id(layers[0])
            .ok()
            .and_then(|entity| {
                entity
                    .property("text.content")
                    .and_then(sue_core::Value::as_str)
                    .map(str::to_owned)
            })
    };
    assert_eq!(content(&runtime).as_deref(), Some("Hello"));
    let inked = sue_paint::doc::raster(&runtime.engine.world.arena, layers[0])
        .unwrap()
        .bytes()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 128)
        .count();
    assert!(inked > 200, "the words are drawn: {inked}");
    // A click on the words edits them in the same layer.
    let depth = runtime.engine.history.depth();
    let on = middle + vec2(10.0, 8.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(on), press(on, true)],
    );
    step(&mut runtime, vec![press(on, false)]);
    step(&mut runtime, Vec::new());
    step(
        &mut runtime,
        vec![
            key(egui::Key::End, plain),
            egui::Event::Text(" Sue".to_owned()),
        ],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    assert_eq!(text_layers(&runtime), layers, "the same layer");
    assert_eq!(content(&runtime).as_deref(), Some("Hello Sue"));
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    let _ = std::fs::remove_dir_all(&root);
}

// ----- The Page Manager and the 3D preview (P4-15) ---------------------------

/// A story of `count` pages on a new canvas, the first page shown; the
/// driver and the pages in order.
fn story_of_pages(tag: &str, count: usize) -> (PaintDriver, Vec<sue_core::EntityId>) {
    let mut driver = PaintDriver::new(tag);
    for _ in 1..count {
        driver.command("story.add");
        driver.settle(2);
    }
    driver.command("story.first");
    driver.settle(4);
    let pages = story_pages(&driver);
    assert_eq!(pages.len(), count);
    (driver, pages)
}

/// The pages of the story the picture shown is in.
fn story_pages(driver: &PaintDriver) -> Vec<sue_core::EntityId> {
    let arena = &driver.runtime.engine.world.arena;
    let shown = shown_page(driver).expect("a picture is shown");
    sue_paint::story::story_of(arena, shown)
        .map(|story| sue_paint::story::pages(arena, story))
        .unwrap_or_default()
}

/// The picture Sue Paint shows.
fn shown_page(driver: &PaintDriver) -> Option<sue_core::EntityId> {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
}

/// The middle of the Page Manager's cell of page `number` (counting from
/// 1): the paper above the number drawn under it.
fn page_cell(driver: &PaintDriver, number: usize) -> Pos2 {
    let texts = &driver.texts;
    let label = texts
        .iter()
        .filter(|(text, _)| *text == number.to_string() || *text == format!("{number} (open)"))
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("page {number} is in the Page Manager: {texts:?}"));
    label.center() - vec2(0.0, 40.0)
}

/// A press and release of `button` at `at`, then a few frames.
fn press_at(driver: &mut PaintDriver, at: Pos2, button: egui::PointerButton) {
    let event = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![event(true)]);
    driver.step(vec![event(false)]);
    driver.settle(6);
}

/// The story's property `key`, as text.
fn story_text(driver: &PaintDriver, page: sue_core::EntityId, key: &str) -> Option<String> {
    let arena = &driver.runtime.engine.world.arena;
    let story = sue_paint::story::story_of(arena, page)?;
    arena
        .get_by_id(story)
        .ok()?
        .property(key)
        .and_then(sue_core::Value::as_str)
        .map(str::to_owned)
}

/// CTV-5 and FG-32: the Page Manager is a view that fills the canvas
/// area; a page dragged onto another's place moves there as one undo
/// step; a page opened takes the canvas, the pages docked in a column
/// beside it.
#[test]
fn page_manager_fills_the_canvas_area_reorders_by_drag_and_docks() {
    let (mut driver, pages) = story_of_pages("page-manager-view", 4);
    driver.command("story.manager");
    let texts = driver.settle(6);
    assert!(
        texts.iter().any(|(text, _)| text == "Page Manager"),
        "the Page Manager shows: {texts:?}"
    );
    assert!(texts.iter().any(|(text, _)| text == "4 pages"));
    let title = driver.rect_of("Page Manager").unwrap();
    let second = page_cell(&driver, 2);
    assert!(second.y > title.bottom());
    // Page 2 dragged onto page 4's place.
    let depth = driver.runtime.engine.history.depth();
    let to = page_cell(&driver, 4);
    driver.drag(second, to, egui::PointerButton::Primary);
    let moved = story_pages(&driver);
    assert_eq!(moved, vec![pages[0], pages[2], pages[3], pages[1]]);
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1);
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(story_pages(&driver), pages, "undone");
    driver.step(vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(story_pages(&driver), moved, "redone");
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    // A double-click opens page 3; the pages stay, docked beside it,
    // one under another.
    let third = page_cell(&driver, 3);
    let event = |pressed| egui::Event::PointerButton {
        pos: third,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(third)]);
    driver.step(vec![event(true)]);
    driver.step(vec![event(false)]);
    driver.step(vec![event(true)]);
    driver.step(vec![event(false)]);
    driver.settle(8);
    assert_eq!(shown_page(&driver), Some(pages[2]));
    let (one, two) = (page_cell(&driver, 1), page_cell(&driver, 2));
    assert!(
        (one.x - two.x).abs() < 1.0 && two.y > one.y,
        "docked, the pages stand in one column: {one:?} {two:?}"
    );
    let column = driver.rect_of("Page Manager").unwrap();
    assert!(
        column.left() < driver.runtime.shell.page_rect().center().x,
        "docked at the left of the canvas area"
    );
    assert!(driver.texts.iter().any(|(text, _)| text == "3 (open)"));
}

/// CT-34 and CTV-6: the Page Manager's right-click menu in Clip Studio
/// Paint's order, 3D Preview for Binding last; its page commands act on
/// the page clicked; Webtoon Reading Direction stands the pages in
/// columns read from the right.
#[test]
fn page_manager_right_click_menu_acts_on_the_page_and_turns_to_webtoon() {
    let (mut driver, pages) = story_of_pages("page-manager-menu", 4);
    driver.command("story.manager");
    driver.settle(6);
    let third = page_cell(&driver, 3);
    press_at(&mut driver, third, egui::PointerButton::Secondary);
    let in_order = |driver: &PaintDriver, order: &[&str]| {
        let tops: Vec<f32> = order
            .iter()
            .map(|label| {
                driver
                    .rect_of(label)
                    .unwrap_or_else(|| panic!("{label:?} is in the menu: {:?}", driver.texts))
                    .top()
            })
            .collect();
        assert!(
            tops.windows(2).all(|pair| pair[0] < pair[1]),
            "the menu's order: {order:?} {tops:?}"
        );
    };
    in_order(
        &driver,
        &[
            "Open Page",
            "First Page",
            "Previous Page",
            "Next Page",
            "Last Page",
            "Add Page...",
            "Duplicate Page",
            "Delete Page",
            "Combine Pages",
            "Split Pages",
        ],
    );
    // Taller than the room under the page, the menu scrolls: End shows
    // its last item, 3D Preview for Binding.
    driver.step(vec![key(egui::Key::End, egui::Modifiers::NONE)]);
    driver.settle(30);
    in_order(
        &driver,
        &[
            "Page Reordering",
            "Webtoon Reading Direction",
            "Batch Process",
            "Edit Text",
            "3D Preview for Binding...",
        ],
    );
    driver.step(vec![key(egui::Key::Home, egui::Modifiers::NONE)]);
    driver.settle(30);
    // Delete Page deletes the page right-clicked, not the one shown.
    driver.click_text("Delete Page");
    driver.settle(4);
    assert_eq!(story_pages(&driver), vec![pages[0], pages[1], pages[3]]);
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(6);
    assert_eq!(story_pages(&driver), pages, "undone");
    // Webtoon Reading Direction, Right to Left.
    driver.command("story.manager");
    driver.settle(6);
    let first = page_cell(&driver, 1);
    press_at(&mut driver, first, egui::PointerButton::Secondary);
    // By the keyboard: typing its first letter finds the submenu, Right
    // opens it, Enter picks its first item.
    driver.step(vec![egui::Event::Text("W".to_owned())]);
    driver.settle(4);
    driver.step(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
    driver.settle(30);
    assert!(
        driver.rect_of("Right to Left").is_some(),
        "{:?}",
        driver.texts
    );
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(6);
    assert_eq!(
        story_text(&driver, pages[0], "story.webtoon_reading").as_deref(),
        Some("right")
    );
    // Down a column, then on to the next column, to the left.
    let cells: Vec<Pos2> = (1..=4).map(|number| page_cell(&driver, number)).collect();
    assert!(
        (cells[0].x - cells[1].x).abs() < 1.0 && cells[1].y > cells[0].y,
        "page 2 under page 1: {cells:?}"
    );
    assert!(
        cells[3].x < cells[0].x,
        "the last page in a column left of the first: {cells:?}"
    );
}

/// CT-37 and FG-32: 3D Preview for Binding shows the book open at the
/// spread of the page shown, turns with the arrow keys and closes.
#[test]
fn binding_preview_opens_at_the_page_turns_and_closes() {
    let (mut driver, _) = story_of_pages("binding-preview", 4);
    driver.command("story.preview_binding");
    let texts = driver.settle(6);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text == "3D Preview for Binding"),
        "{texts:?}"
    );
    // A book bound on the left starts with page 1 alone.
    assert!(driver.rect_of("1 of 4").is_some(), "{:?}", driver.texts);
    driver.step(vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert!(driver.rect_of("2 - 3 of 4").is_some(), "{:?}", driver.texts);
    driver.step(vec![key(egui::Key::ArrowLeft, egui::Modifiers::NONE)]);
    driver.settle(4);
    assert!(driver.rect_of("1 of 4").is_some());
    driver.click_text("Close");
    assert!(driver.rect_of("3D Preview for Binding").is_none());
}

// ----- Frames and balloons (P4-15) --------------------------------------------

/// The canvas' frame border layer (the topmost) and its panels.
fn frame_panels(
    driver: &PaintDriver,
) -> Option<(sue_core::EntityId, Vec<sue_paint::manga::Panel>)> {
    let arena = &driver.runtime.engine.world.arena;
    let picture = shown_page(driver)?;
    sue_paint::doc::layers(arena, picture)
        .into_iter()
        .rev()
        .find_map(|info| {
            let entity = arena.get_by_id(info.id).ok()?;
            (entity.class() == sue_paint::manga::FRAME).then(|| {
                let panels = entity
                    .property(sue_paint::manga::PANELS)
                    .and_then(sue_core::Value::as_str)
                    .map(sue_paint::manga::decode_panels)
                    .unwrap_or_default();
                (info.id, panels)
            })
        })
}

/// The box round a panel: `[left, top, right, bottom]`.
fn panel_box(panel: &[[f32; 2]]) -> [f32; 4] {
    panel.iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[x0, y0, x1, y1], p| [x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])],
    )
}

/// Picks the Frame Border tool's Rectangle frame and drags one from `a`
/// to `b` (screen points); the canvas' picture pixels per screen point
/// and where screen point zero lands, read from the panel it made.
fn frame_from(driver: &mut PaintDriver, a: Pos2, b: Pos2) -> (egui::Vec2, Pos2) {
    driver.step(vec![key(egui::Key::U, egui::Modifiers::NONE)]);
    driver.step(vec![key(egui::Key::U, egui::Modifiers::NONE)]);
    driver.settle(2);
    let before = frame_panels(driver).map_or(0, |(_, panels)| panels.len());
    driver.drag(a, b, egui::PointerButton::Primary);
    let (_, panels) = frame_panels(driver).expect("a frame border layer");
    assert_eq!(panels.len(), before + 1, "a rectangle frame");
    let [x0, y0, x1, y1] = panel_box(panels.last().unwrap());
    let scale = vec2((x1 - x0) / (b.x - a.x), (y1 - y0) / (b.y - a.y));
    (scale, Pos2::new(x0 - a.x * scale.x, y0 - a.y * scale.y))
}

/// FGV-1: Divide Frame Border Equally cuts the panel picked into equal
/// panels and Combine Frames makes panels picked (Shift adds) one; each
/// is one undo step that undoes and redoes exactly.
#[test]
fn frames_divide_equally_and_combine_as_one_step_each() {
    let mut driver = PaintDriver::new("frame-commands");
    let middle = paint_middle(&driver.runtime);
    let (a, b) = (middle - vec2(150.0, 90.0), middle + vec2(150.0, 90.0));
    let (scale, zero) = frame_from(&mut driver, a, b);
    let to_screen = |p: [f32; 2]| Pos2::new((p[0] - zero.x) / scale.x, (p[1] - zero.y) / scale.y);
    let (layer, panels) = frame_panels(&driver).unwrap();
    let whole = panel_box(&panels[0]);
    let text_of = |driver: &PaintDriver| {
        driver
            .runtime
            .engine
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| {
                entity
                    .property(sue_paint::manga::PANELS)
                    .and_then(sue_core::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    };
    // The Object tool picks the panel; Divide asks how, then cuts it in
    // two across.
    driver.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    press_at(&mut driver, middle, egui::PointerButton::Primary);
    let one = text_of(&driver);
    let depth = driver.runtime.engine.history.depth();
    driver.command("layer.frame.divide_equally");
    driver.settle(6);
    assert!(
        driver.rect_of("Divide Frame Border Equally").is_some(),
        "{:?}",
        driver.texts
    );
    driver.click_text("OK");
    driver.settle(4);
    let (_, halves) = frame_panels(&driver).unwrap();
    assert_eq!(halves.len(), 2, "divided in two");
    let (left, right) = (panel_box(&halves[0]), panel_box(&halves[1]));
    assert!((left[0] - whole[0]).abs() < 0.5 && (right[2] - whole[2]).abs() < 0.5);
    assert!(
        ((left[2] - left[0]) - (right[2] - right[0])).abs() < 0.5,
        "equal: {left:?} {right:?}"
    );
    assert!(right[0] > left[2], "a gutter between: {left:?} {right:?}");
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1);
    let two = text_of(&driver);
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(text_of(&driver), one, "undone");
    driver.step(vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(text_of(&driver), two, "redone");
    // Click one half, Shift-click the other: Combine makes them one
    // panel as big as the first.
    let centre = |b: [f32; 4]| to_screen([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    press_at(&mut driver, centre(left), egui::PointerButton::Primary);
    let shift = |pressed| egui::Event::PointerButton {
        pos: centre(right),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::SHIFT,
    };
    driver.step(vec![egui::Event::PointerMoved(centre(right))]);
    driver.step(vec![shift(true)]);
    driver.step(vec![shift(false)]);
    driver.settle(4);
    let depth = driver.runtime.engine.history.depth();
    driver.command("layer.frame.combine");
    driver.settle(4);
    let (_, combined) = frame_panels(&driver).unwrap();
    assert_eq!(combined.len(), 1, "combined");
    let back = panel_box(&combined[0]);
    for (got, want) in back.iter().zip(whole) {
        assert!((got - want).abs() < 0.5, "{back:?} {whole:?}");
    }
    assert_eq!(driver.runtime.engine.history.depth(), depth + 1);
    driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(text_of(&driver), two, "undone");
}

/// Full guide 00:41:46: on a story's page a Rectangle frame dragged near
/// the default border snaps onto it.
#[test]
fn rectangle_frames_snap_to_the_page_default_border() {
    let mut driver = PaintDriver::new("frame-snap");
    driver.command("story.add");
    driver.settle(6);
    let page = shown_page(&driver).unwrap();
    let middle = paint_middle(&driver.runtime);
    let (scale, zero) = frame_from(
        &mut driver,
        middle - vec2(40.0, 30.0),
        middle + vec2(40.0, 30.0),
    );
    let to_screen = |p: [f32; 2]| Pos2::new((p[0] - zero.x) / scale.x, (p[1] - zero.y) / scale.y);
    let borders = sue_paint::story::borders(&driver.runtime.engine.world.arena, page)
        .expect("a story page has borders");
    let inner = borders.inner;
    // A few points inside the border's top left corner, to the middle.
    let start = to_screen([inner[0], inner[1]]) + vec2(5.0, 4.0);
    driver.drag(
        start,
        middle - vec2(60.0, 50.0),
        egui::PointerButton::Primary,
    );
    let (_, panels) = frame_panels(&driver).unwrap();
    let made = panel_box(panels.last().unwrap());
    assert!(
        // Panels are kept to the whole pixel.
        (made[0] - inner[0]).abs() <= 0.5 && (made[1] - inner[1]).abs() <= 0.5,
        "snapped onto the default border {inner:?}: {made:?}"
    );
    // Far from it, a corner stays where it was let go.
    let free = panel_box(&panels[panels.len() - 2]);
    assert!(free[0] > inner[0] + 1.0);
}

/// FGV-40 and FG-35: a balloon layer draws its balloons with the line in
/// the drawing color and the inside in the other color, kept on the
/// layer.
#[test]
fn balloons_take_the_drawing_and_the_other_color() {
    let mut driver = PaintDriver::new("balloon-colors");
    // Drawing with the sub color (X): the balloon is lined in white and
    // black inside.
    driver.command("color.swap");
    driver.settle(2);
    driver.step(vec![key(egui::Key::T, egui::Modifiers::NONE)]);
    driver.step(vec![key(egui::Key::T, egui::Modifiers::NONE)]);
    driver.settle(2);
    let middle = paint_middle(&driver.runtime);
    driver.drag(
        middle - vec2(60.0, 40.0),
        middle + vec2(60.0, 40.0),
        egui::PointerButton::Primary,
    );
    let arena = &driver.runtime.engine.world.arena;
    let picture = shown_page(&driver).unwrap();
    let balloon = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .find_map(|info| {
            let entity = arena.get_by_id(info.id).ok()?;
            (entity.class() == sue_paint::manga::BALLOON).then_some(entity)
        })
        .expect("a balloon layer");
    let text = |key| {
        balloon
            .property(key)
            .and_then(sue_core::Value::as_str)
            .map(str::to_owned)
    };
    assert_eq!(text("balloon.line_color").as_deref(), Some("#ffffff"));
    assert_eq!(text("balloon.fill_color").as_deref(), Some("#000000"));
    let raster = balloon
        .component(sue_core::ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
        .unwrap();
    let pixels = raster.bytes().unwrap();
    let painted: Vec<&[u8; 4]> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] == 255)
        .collect();
    assert!(!painted.is_empty());
    assert!(painted.iter().any(|p| p[..3] == [0, 0, 0]), "black inside");
    assert!(
        painted.iter().any(|p| p[..3] == [255, 255, 255]),
        "white line"
    );
}
