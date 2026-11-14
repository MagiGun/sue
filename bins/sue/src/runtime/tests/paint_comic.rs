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

    // The Figure tool's Saturated line: a layer of lines of its own.
    step(&mut runtime, vec![key(egui::Key::U, plain)]);
    for _ in 0..5 {
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
