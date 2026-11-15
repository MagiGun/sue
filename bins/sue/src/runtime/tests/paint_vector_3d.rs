//! Sue Paint: vector layers, figures, 3D layers, the scanners.

use super::*;

/// The Pose and Hand Scanners in the running app, on real photos with
/// the real models (downloaded once): `SUE_SCAN_PHOTOS` names a folder
/// with `body.jpg` and `hand.jpg`.
/// `cargo test -p sue -- --ignored sue_paint_scanners`
#[test]
#[ignore = "needs the network and photos"]
fn sue_paint_scanners_pose_a_figure_and_a_hand_from_photos() {
    let photos = PathBuf::from(std::env::var("SUE_SCAN_PHOTOS").unwrap());
    let (mut runtime, root) = runtime("paint-scanners");
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
    let pose_of = |runtime: &Runtime| {
        let arena = &runtime.engine.world.arena;
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let model = sue_paint::figure::of_layer(arena, layer).unwrap();
        sue_paint::figure::Pose::of(arena.get_by_id(model).unwrap())
    };
    for (new, scanner, photo) in [
        ("layer.new.figure", "figure.pose_scanner", "body.jpg"),
        ("layer.new.hand", "figure.hand_scanner", "hand.jpg"),
    ] {
        command(&mut runtime, new);
        step(&mut runtime, Vec::new());
        let before = pose_of(&runtime);
        let depth = runtime.engine.history.depth();
        command(&mut runtime, scanner);
        let _ = runtime.answer_pick(&photos.join(photo), &mut NoTextures, start);
        // The scan runs in the background; wait for it.
        let waited = Instant::now();
        while pose_of(&runtime) == before {
            assert!(
                waited.elapsed() < Duration::from_secs(120),
                "{scanner}: {:?} {:?}",
                runtime.shell.message(),
                runtime
                    .transfers
                    .list()
                    .iter()
                    .map(|transfer| (transfer.label.clone(), transfer.state.clone()))
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(20));
            runtime.timer(&mut NoTextures, Instant::now());
            step(&mut runtime, Vec::new());
        }
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        println!("{scanner}: {:?}", pose_of(&runtime));
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sue_paint_vector_layers_keep_lines_and_erase_up_to_intersections() {
    let (mut runtime, root) = runtime("paint-vector");
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
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    // Layer, New Layer, Vector Layer.
    command(&mut runtime, "layer.new.vector");
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    assert!(sue_paint::vector::is_vector(
        &runtime.engine.world.arena,
        layer
    ));
    let lines = |runtime: &Runtime| {
        sue_paint::vector::lines(&sue_paint::vector::geometry(
            &runtime.engine.world.arena,
            layer,
        ))
    };
    // How far right the ink on the layer reaches.
    let right = |runtime: &Runtime| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        let (w, bytes) = (raster.width(), raster.bytes().unwrap());
        (0..raster.height())
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|(x, y)| bytes[((y * w + x) * 4 + 3) as usize] > 0)
            .map(|(x, _)| x)
            .max()
            .unwrap_or(0)
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let drag = |runtime: &mut Runtime,
                step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                from: egui::Pos2,
                to: egui::Pos2| {
        step(
            runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        for part in 1..=8 {
            let at = from + (to - from) * (part as f32 / 8.0);
            step(runtime, vec![egui::Event::PointerMoved(at)]);
        }
        step(runtime, vec![press(to, false)]);
    };
    // A level line crossed by an upright one: two lines, each one step.
    let middle = runtime.shell.page_rect().center();
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(-120.0, 0.0),
        middle + vec2(120.0, 0.0),
    );
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(0.0, -120.0),
        middle + vec2(0.0, 120.0),
    );
    assert_eq!(lines(&runtime).len(), 2);
    let full = right(&runtime);
    command(&mut runtime, "edit.undo");
    assert_eq!(lines(&runtime).len(), 1, "undo takes the line away");
    command(&mut runtime, "edit.redo");
    assert_eq!(lines(&runtime).len(), 2);
    // The Vector eraser (up to intersection) on the level line's right
    // end takes it away up to the upright.
    step(&mut runtime, vec![key(egui::Key::E, plain)]);
    for _ in 0..3 {
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    }
    let at = middle + vec2(80.0, 0.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(at), press(at, true)],
    );
    step(&mut runtime, vec![press(at, false)]);
    assert_eq!(lines(&runtime).len(), 2, "the left part and the upright");
    let cut = right(&runtime);
    let upright = lines(&runtime)[1].samples[0].at[0] as u32;
    assert!(
        cut < full && cut <= upright + 12,
        "{cut} against {full}, upright at {upright}"
    );
    command(&mut runtime, "edit.undo");
    assert_eq!(right(&runtime), full, "undo brings the line back");
    assert_eq!(lines(&runtime).len(), 2);

    // Correct Line (Y), Pinch vector line: dragging the level line's
    // middle up pulls it into a curve; undo puts it back.
    let drawn = lines(&runtime);
    step(&mut runtime, vec![key(egui::Key::Y, plain)]);
    step(&mut runtime, vec![key(egui::Key::Period, plain)]);
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(-60.0, 0.0),
        middle + vec2(-60.0, -40.0),
    );
    let pulled = lines(&runtime);
    let lift = |line: &sue_paint::vector::Line| {
        line.samples
            .iter()
            .map(|sample| sample.at[1])
            .fold(f32::INFINITY, f32::min)
    };
    assert!(
        lift(&pulled[0]) < lift(&drawn[0]) - 5.0,
        "the line is pulled up"
    );
    assert_eq!(pulled[1], drawn[1], "the other line stays");
    command(&mut runtime, "edit.undo");
    assert_eq!(lines(&runtime), drawn);

    // The Operation tool's Object (O): drag the upright to move it,
    // Delete takes it away, undo brings it back.
    step(&mut runtime, vec![key(egui::Key::O, plain)]);
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(0.0, 80.0),
        middle + vec2(40.0, 80.0),
    );
    let moved = lines(&runtime);
    assert!(
        moved[1].samples[0].at[0] > drawn[1].samples[0].at[0] + 10.0,
        "moved right"
    );
    assert_eq!(moved[0], drawn[0], "the other line stays");
    command(&mut runtime, "edit.clear");
    assert_eq!(lines(&runtime).len(), 1);
    command(&mut runtime, "edit.undo");
    assert_eq!(lines(&runtime), moved);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_figures_pose_and_become_vector_lines() {
    let (mut runtime, root) = runtime("paint-figure");
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
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let layers = |runtime: &Runtime| sue_paint::doc::layers(&runtime.engine.world.arena, picture);
    // Layer, New Layer, 3D Drawing Figure: a standing figure, drawn.
    command(&mut runtime, "layer.new.figure");
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let layer = layers(&runtime)
        .into_iter()
        .find(|info| sue_paint::figure::of_layer(arena, info.id).is_some())
        .map(|info| info.id)
        .expect("a figure layer");
    let pixels = |runtime: &Runtime| {
        sue_paint::doc::raster(&runtime.engine.world.arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec()
    };
    let standing = pixels(&runtime);
    assert!(
        standing.chunks(4).any(|pixel| pixel[3] > 0),
        "the figure shows"
    );
    // A preset pose draws it again, as one undo step.
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "figure.preset.1");
    step(&mut runtime, Vec::new());
    assert_ne!(pixels(&runtime), standing, "arms out");
    assert_eq!(runtime.engine.history.depth(), depth + 1);
    // Convert to Vector Lines and Tones: the figure's lines as strokes.
    let before = layers(&runtime).len();
    command(&mut runtime, "layer.lines_tones_vector");
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let vector = layers(&runtime)
        .into_iter()
        .find(|info| sue_paint::vector::is_vector(arena, info.id))
        .expect("a vector layer of lines");
    let strokes = sue_paint::vector::lines(&sue_paint::vector::geometry(arena, vector.id));
    assert!(
        strokes.len() >= 5,
        "the outline and folds as strokes: {}",
        strokes.len()
    );
    assert_eq!(layers(&runtime).len(), before + 2, "tones and lines");
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(layers(&runtime).len(), before, "undone as one step");
    // The 3D hand and head: new layers of their own kinds, posed by
    // their own presets, each one step.
    for (new, kind, pose) in [
        (
            "layer.new.hand",
            sue_paint::figure::Kind::Hand,
            "figure.preset.2",
        ),
        (
            "layer.new.head",
            sue_paint::figure::Kind::Head,
            "figure.preset.4",
        ),
    ] {
        command(&mut runtime, new);
        step(&mut runtime, Vec::new());
        let arena = &runtime.engine.world.arena;
        let layer = layers(&runtime)
            .into_iter()
            .find(|info| {
                sue_paint::figure::of_layer(arena, info.id)
                    .and_then(|model| arena.get_by_id(model).ok())
                    .is_some_and(|model| sue_paint::figure::Body::of(model).kind == kind)
            })
            .map(|info| info.id)
            .expect("a layer of the kind");
        let drawn = |runtime: &Runtime| {
            sue_paint::doc::raster(&runtime.engine.world.arena, layer)
                .unwrap()
                .bytes()
                .unwrap()
                .to_vec()
        };
        let first = drawn(&runtime);
        let covered = first.chunks(4).filter(|pixel| pixel[3] > 0).count();
        assert!(covered > 5_000, "{kind:?} shows, framed large: {covered}");
        let depth = runtime.engine.history.depth();
        command(&mut runtime, pose);
        step(&mut runtime, Vec::new());
        assert_ne!(drawn(&runtime), first, "{kind:?} posed");
        assert_eq!(runtime.engine.history.depth(), depth + 1);
    }
    // Register pose as material: the head's pose kept in the materials
    // folder, to use again.
    let materials = root.join("materials-here");
    let _ = runtime.engine.world.rice.set(
        "sue_paint.materials_dir",
        sue_rice::RiceValue::Text(materials.display().to_string()),
    );
    command(&mut runtime, "figure.register_pose");
    step(&mut runtime, Vec::new());
    let kept = materials.join("poses").join("Head pose 1.pose");
    let text = std::fs::read_to_string(&kept).expect("the pose is kept");
    assert!(
        text.starts_with(
            "head
v1;"
        ),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_3d_layers_turn_and_become_lines_and_tones() {
    let (mut runtime, root) = runtime("paint-3d");
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
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let three_d = |runtime: &Runtime| {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .find(|info| sue_paint::three::is_3d(&runtime.engine.world.arena, info.id))
            .map(|info| info.id)
    };
    // Layer, New Layer, 3D Layer: a cube, drawn.
    command(&mut runtime, "layer.new.3d");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    let layer = three_d(&runtime).expect("a 3D layer");
    let arena = &runtime.engine.world.arena;
    assert!(sue_paint::three::model_of(arena, layer).is_some());
    let drawn = sue_paint::doc::raster(arena, layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec();
    assert!(drawn.chunks(4).any(|pixel| pixel[3] > 0));
    let yaw = |runtime: &Runtime| {
        sue_paint::three::View3D::of(runtime.engine.world.arena.get_by_id(layer).unwrap()).yaw
    };
    let before = yaw(&runtime);
    // The Object tool (picked for a new 3D layer) turns the camera.
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = runtime.shell.page_rect().center();
    step(&mut runtime, vec![egui::Event::PointerMoved(middle)]);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    for part in 1..=4 {
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(
                middle + vec2(25.0 * part as f32, 0.0),
            )],
        );
    }
    step(&mut runtime, vec![press(middle + vec2(100.0, 0.0), false)]);
    let after = yaw(&runtime);
    assert!(
        (after - (before - 40.0)).abs() < 1.0,
        "turned by the drag: {before} to {after}"
    );
    assert_ne!(
        sue_paint::doc::raster(&runtime.engine.world.arena, layer)
            .unwrap()
            .bytes()
            .unwrap(),
        &drawn[..]
    );
    command(&mut runtime, "edit.undo");
    assert!((yaw(&runtime) - before).abs() < 1e-3, "one step to undo");
    assert_eq!(
        sue_paint::doc::raster(&runtime.engine.world.arena, layer)
            .unwrap()
            .bytes()
            .unwrap(),
        &drawn[..]
    );
    // Convert to Lines and Tones from the model.
    command(&mut runtime, "filter.lines_tones");
    let names: Vec<String> = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .into_iter()
        .map(|info| info.name)
        .collect();
    assert!(
        names.contains(&"Lines".to_owned()) && names.contains(&"Tones".to_owned()),
        "{names:?}"
    );
    let lines = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .into_iter()
        .find(|info| info.name == "Lines")
        .unwrap();
    let raster = sue_paint::doc::raster(&runtime.engine.world.arena, lines.id).unwrap();
    assert!(
        raster
            .bytes()
            .unwrap()
            .chunks(4)
            .any(|pixel| pixel[3] > 200),
        "outlines drawn"
    );
    // File, Import, 3D Data: an OBJ model becomes a 3D layer.
    std::fs::create_dir_all(&root).unwrap();
    let obj = root.join("Wedge.obj");
    std::fs::write(
        &obj,
        "v 0 0 0
v 1 0 0
v 0 1 0
v 0 0 1
f 1 2 3
f 1 2 4
f 1 3 4
f 2 3 4
",
    )
    .unwrap();
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(paint.clone()), "model3d", "Import 3D Data"),
            path: obj,
        },
        &mut NoTextures,
        start,
    );
    assert!(
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .iter()
            .any(|info| info.name == "Wedge"
                && sue_paint::three::is_3d(&runtime.engine.world.arena, info.id))
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ----- Vector lines (P4-14) -------------------------------------------------------

/// The picture Sue Paint has open in `driver`.
fn picture_of(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

/// The current top layer of the open picture: the one a new layer
/// command just made.
fn top_layer(driver: &PaintDriver) -> sue_core::EntityId {
    sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture_of(driver))
        .last()
        .expect("a layer")
        .id
}

/// The vector lines of `layer`.
fn lines_on(driver: &PaintDriver, layer: sue_core::EntityId) -> Vec<sue_paint::vector::Line> {
    let arena = &driver.runtime.engine.world.arena;
    sue_paint::vector::lines(&sue_paint::vector::geometry(arena, layer))
}

/// How many pixels of `layer` have ink.
fn inked(driver: &PaintDriver, layer: sue_core::EntityId) -> usize {
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    raster
        .bytes()
        .unwrap()
        .chunks(4)
        .filter(|pixel| pixel[3] > 0)
        .count()
}

/// Presses at the first of `points`, moves through the rest and lets go
/// at the last, with `modifiers` held.
fn trace(driver: &mut PaintDriver, points: &[Pos2], modifiers: egui::Modifiers) {
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    driver.step(vec![
        egui::Event::PointerMoved(points[0]),
        press(points[0], true),
    ]);
    for pair in points.windows(2) {
        for part in 1..=6 {
            let at = pair[0] + (pair[1] - pair[0]) * (part as f32 / 6.0);
            driver.step(vec![egui::Event::PointerMoved(at)]);
        }
    }
    let last = *points.last().unwrap();
    driver.step(vec![press(last, false)]);
    driver.settle(6);
}

/// Where picture pixel `at` is on screen, from a line drawn from screen
/// point `from` whose first sample is `first`, at `scale` screen points
/// a picture pixel.
fn on_screen(at: [f32; 2], from: Pos2, first: [f32; 2], scale: f32) -> Pos2 {
    from + vec2(at[0] - first[0], at[1] - first[1]) * scale
}

/// The undo pair of rule 7 for the last step: the lines and pixels of
/// `layer` as `before` after undo, as `after` after redo.
fn undo_pair(
    driver: &mut PaintDriver,
    layer: sue_core::EntityId,
    before: &(Vec<sue_paint::vector::Line>, usize),
    after: &(Vec<sue_paint::vector::Line>, usize),
) {
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(
        lines_on(driver, layer),
        before.0,
        "undo puts the lines back"
    );
    assert_eq!(inked(driver, layer), before.1, "and their pixels");
    driver.command("edit.redo");
    driver.settle(2);
    assert_eq!(lines_on(driver, layer), after.0, "redo makes them again");
    assert_eq!(inked(driver, layer), after.1);
}

/// CT-6, CTL-1 and CTL-2 (TL 01:11 to 01:34, T50 tip 21): the Object
/// tool on a vector layer. A loop drawn round two of three lines picks
/// them; Delete takes just those; the box's bottom handle stretches them
/// (Scale) and its corner bends them (Free transform); in Tool Property
/// a typed hex code recolors every picked line, and the brush size
/// thickens them. Every change is one step with its undo pair.
#[test]
fn sue_paint_object_tool_loops_boxes_and_restyles_vector_lines() {
    let mut d = PaintDriver::new("paint-vector-object");
    d.command("layer.new.vector");
    d.settle(2);
    let layer = top_layer(&d);
    let middle = d.runtime.shell.page_rect().center();
    d.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    // Three level lines, the lowest apart from the other two.
    let starts = [
        middle + vec2(-100.0, -60.0),
        middle + vec2(-100.0, -20.0),
        middle + vec2(-100.0, 100.0),
    ];
    for from in starts {
        d.drag(from, from + vec2(200.0, 0.0), egui::PointerButton::Primary);
    }
    let drawn = lines_on(&d, layer);
    assert_eq!(drawn.len(), 3);
    let first = drawn[0].samples[0].at;
    let scale = (starts[1].y - starts[0].y) / (drawn[1].samples[0].at[1] - first[1]);
    let screen = |at: [f32; 2]| on_screen(at, starts[0], first, scale);

    // Object (O): a loop round the top two lines picks them.
    d.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    let ring = [
        middle + vec2(-130.0, -90.0),
        middle + vec2(130.0, -90.0),
        middle + vec2(130.0, 10.0),
        middle + vec2(-130.0, 10.0),
        middle + vec2(-130.0, -88.0),
    ];
    trace(&mut d, &ring, egui::Modifiers::NONE);
    d.click_text("Tool Property");
    let texts = d.settle(4);
    assert!(
        texts.iter().any(|(text, _)| text == "2 lines picked"),
        "Tool Property names what the loop picked"
    );
    assert!(texts.iter().any(|(text, _)| text == "Editing vector"));
    // Delete takes the two picked lines and leaves the third.
    let before = (lines_on(&d, layer), inked(&d, layer));
    d.command("edit.clear");
    d.settle(2);
    let left = lines_on(&d, layer);
    assert_eq!(
        left,
        [drawn[2].clone()],
        "only the line outside the loop stays"
    );
    let after = (left, inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);
    d.command("edit.undo");
    d.settle(2);
    assert_eq!(lines_on(&d, layer), drawn);

    // Pick them again; the bottom middle handle pulled down 40 points
    // stretches them taller, the third line stays.
    trace(&mut d, &ring, egui::Modifiers::NONE);
    let [_, top, _, bottom] = sue_paint::vector::bounds(&drawn, &[0, 1]).unwrap();
    let left_x = drawn[0].samples[0].at[0];
    let right_x = drawn[0].samples.last().unwrap().at[0];
    let handle = screen([(left_x + right_x) / 2.0, bottom]);
    let before = (lines_on(&d, layer), inked(&d, layer));
    d.drag(
        handle,
        handle + vec2(0.0, 40.0),
        egui::PointerButton::Primary,
    );
    let stretched = lines_on(&d, layer);
    let [_, new_top, _, new_bottom] = sue_paint::vector::bounds(&stretched, &[0, 1]).unwrap();
    assert!(
        (new_top - top).abs() < 0.5,
        "the top edge stays: {new_top} {top}"
    );
    assert!(
        new_bottom - bottom > 40.0 / scale * 0.8,
        "the box is taller: {bottom} to {new_bottom}"
    );
    assert_eq!(stretched[2], drawn[2], "a line not picked stays");
    let after = (stretched.clone(), inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);

    // Mode: Free transform; the bottom right corner dragged right moves
    // only that corner, so the top line keeps its length.
    d.click_text("Free Transform");
    let [_, _, right, bottom] = sue_paint::vector::bounds(&stretched, &[0, 1]).unwrap();
    let corner = screen([right, bottom]);
    d.drag(
        corner,
        corner + vec2(50.0, 0.0),
        egui::PointerButton::Primary,
    );
    let bent = lines_on(&d, layer);
    let length =
        |line: &sue_paint::vector::Line| line.samples.last().unwrap().at[0] - line.samples[0].at[0];
    assert!(
        (length(&bent[0]) - length(&stretched[0])).abs() < 1.0,
        "the top line keeps its length"
    );
    assert!(
        length(&bent[1]) > length(&stretched[1]) + 10.0,
        "the lower one reaches right"
    );

    // Tool Property: the main color's hex code typed recolors both
    // picked lines as soon as it is whole, as one step.
    let before = (lines_on(&d, layer), inked(&d, layer));
    let was = sue_paint::color::hex(sue_paint::color::bytes(bent[0].colors[0]));
    d.click_text(&was);
    d.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    d.step(vec![egui::Event::Text("#1414".to_owned())]);
    assert_eq!(
        lines_on(&d, layer)[0].colors,
        bent[0].colors,
        "not yet whole"
    );
    d.step(vec![egui::Event::Text("14".to_owned())]);
    d.settle(3);
    let recolored = lines_on(&d, layer);
    let gray = [20.0 / 255.0; 3];
    for index in [0, 1] {
        for (have, want) in recolored[index].colors[0].iter().zip(gray) {
            assert!((have - want).abs() < 1e-3, "line {index} recolored");
        }
    }
    assert_eq!(
        recolored[2].colors, drawn[2].colors,
        "the third keeps its color"
    );
    let after = (recolored.clone(), inked(&d, layer));
    d.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    undo_pair(&mut d, layer, &before, &after);

    // The brush size typed into its field thickens both lines.
    let size = sue_wm::ui::input::plain_number(f64::from(recolored[0].tip.size), 1);
    let wider = recolored[0].tip.size * 3.0;
    let before = (lines_on(&d, layer), inked(&d, layer));
    d.click_text(&size);
    d.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    d.step(vec![egui::Event::Text(format!("{wider}"))]);
    d.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    d.settle(4);
    let thick = lines_on(&d, layer);
    for line in &thick[..2] {
        assert!((line.tip.size - wider).abs() < 0.05, "{}", line.tip.size);
    }
    assert_eq!(thick[2].tip, drawn[2].tip);
    assert!(
        inked(&d, layer) > before.1,
        "thicker lines cover more: {} against {}",
        inked(&d, layer),
        before.1
    );
    let after = (thick, inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);
}

/// CT-7 and CTL-3 (TL 01:54 to 01:58), FGV-29's Vector magnet: the
/// Figure tool on a vector layer lays its shapes as vector lines with
/// the sub tool's brush (the ellipse takes the brush shape), and with
/// Vector magnet on a straight line ending near another line's end
/// joins it. Each figure is one step with its undo pair.
#[test]
fn sue_paint_figures_on_a_vector_layer_become_lines_with_the_brush() {
    let mut d = PaintDriver::new("paint-vector-figure");
    d.command("layer.new.vector");
    d.settle(2);
    let layer = top_layer(&d);
    let middle = d.runtime.shell.page_rect().center();
    d.command("tool.pick.figure");
    // Straight Line, Rectangle, Ellipse.
    d.command("tool.next_sub");
    d.command("tool.next_sub");
    let texts = d.settle(3);
    assert!(
        texts.iter().any(|(text, _)| text == "Ellipse"),
        "the Ellipse sub tool"
    );
    let before = (lines_on(&d, layer), inked(&d, layer));
    d.drag(
        middle + vec2(-80.0, -50.0),
        middle + vec2(80.0, 50.0),
        egui::PointerButton::Primary,
    );
    let ellipse = lines_on(&d, layer);
    assert_eq!(ellipse.len(), 1, "the ellipse is a vector line");
    let line = &ellipse[0];
    assert!(line.samples.len() > 90, "round: {}", line.samples.len());
    assert!(
        line.marks.is_empty(),
        "its control points come from its path"
    );
    let first = line.samples[0].at;
    let last = line.samples.last().unwrap().at;
    assert!(
        (first[0] - last[0]).hypot(first[1] - last[1]) < 0.5,
        "closed"
    );
    assert!(line.tip.size > 0.0 && line.tip.taper_start == 0.0);
    assert!(inked(&d, layer) > 200, "painted");
    let after = (ellipse.clone(), inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);

    // The Straight Line sub tool with Vector magnet on: a line ending a
    // few points from the ellipse's start goes onto it.
    d.command("tool.previous_sub");
    d.command("tool.previous_sub");
    d.settle(3);
    d.click_text("Tool Property");
    d.click_text("Vector magnet");
    let start = ellipse[0].samples[0].at;
    let scale = 160.0
        / (ellipse[0]
            .samples
            .iter()
            .map(|s| s.at[0])
            .fold(f32::MIN, f32::max)
            - ellipse[0]
                .samples
                .iter()
                .map(|s| s.at[0])
                .fold(f32::MAX, f32::min));
    let near = middle + vec2(80.0, 0.0) + vec2(0.0, 2.0);
    d.drag(near + vec2(0.0, 120.0), near, egui::PointerButton::Primary);
    let joined = lines_on(&d, layer);
    assert_eq!(joined.len(), 2);
    let end = joined[1].samples.last().unwrap().at;
    assert!(
        (end[0] - start[0]).hypot(end[1] - start[1]) < 0.01,
        "the magnet joined the ends: {end:?} {start:?} at {scale}"
    );
    // A filled shape is not a line: on a vector layer it is refused
    // with a notice, and nothing changes.
    d.command("tool.next_sub");
    d.command("tool.next_sub");
    d.command("tool.next_sub");
    d.settle(2);
    let kept = lines_on(&d, layer);
    d.drag(
        middle,
        middle + vec2(30.0, 30.0),
        egui::PointerButton::Primary,
    );
    assert_eq!(lines_on(&d, layer), kept);
}

/// Owned row "the vector eraser on several layers": with two vector
/// layers picked, the Vector eraser takes the lines it
/// touches on both, as one step that undo takes back whole.
#[test]
fn sue_paint_vector_eraser_goes_over_every_picked_vector_layer() {
    let mut d = PaintDriver::new("paint-vector-layers");
    let middle = d.runtime.shell.page_rect().center();
    let mut layers = Vec::new();
    for offset in [-30.0, 30.0] {
        d.command("layer.new.vector");
        d.settle(2);
        layers.push(top_layer(&d));
        d.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
        let from = middle + vec2(-100.0, offset);
        d.drag(from, from + vec2(200.0, 0.0), egui::PointerButton::Primary);
    }
    for layer in &layers {
        assert_eq!(lines_on(&d, *layer).len(), 1);
    }
    let before: Vec<_> = layers
        .iter()
        .map(|layer| (lines_on(&d, *layer), inked(&d, *layer)))
        .collect();
    d.command("layer.select_all");
    d.settle(2);
    // The Vector eraser (up to intersection: these lines cross nothing,
    // so each goes whole).
    d.step(vec![key(egui::Key::E, egui::Modifiers::NONE)]);
    for _ in 0..3 {
        d.step(vec![key(egui::Key::Period, egui::Modifiers::NONE)]);
    }
    d.settle(2);
    d.drag(
        middle + vec2(0.0, -60.0),
        middle + vec2(0.0, 60.0),
        egui::PointerButton::Primary,
    );
    for layer in &layers {
        assert!(
            lines_on(&d, *layer).is_empty(),
            "the line on every picked layer goes"
        );
        assert_eq!(inked(&d, *layer), 0);
    }
    d.command("edit.undo");
    d.settle(2);
    for (layer, (lines, ink)) in layers.iter().zip(&before) {
        assert_eq!(&lines_on(&d, *layer), lines, "one undo brings both back");
        assert_eq!(inked(&d, *layer), *ink);
    }
    d.command("edit.redo");
    d.settle(2);
    for layer in &layers {
        assert!(lines_on(&d, *layer).is_empty());
    }
    // A tap, not a drag, erases where it lands too: on the upper line
    // only, the lower one stays.
    d.command("edit.undo");
    d.settle(2);
    let tap = middle + vec2(0.0, -30.0);
    d.drag(tap, tap, egui::PointerButton::Primary);
    assert!(lines_on(&d, layers[0]).is_empty(), "the tapped line goes");
    assert_eq!(lines_on(&d, layers[1]), before[1].0, "the other stays");
    d.command("edit.undo");
    d.settle(2);
    assert_eq!(lines_on(&d, layers[0]), before[0].0, "undo brings it back");
}

/// FGV-29, Vector magnet on pen strokes: with the pen on a vector
/// layer, a line drawn to end a couple of points from another line's
/// end stops near it with the magnet off, and goes exactly onto it with
/// the magnet on, its pixels painted again in the same step; one undo
/// takes the whole stroke back.
#[test]
fn sue_paint_vector_magnet_joins_a_pen_line_to_a_near_end() {
    let mut d = PaintDriver::new("paint-vector-magnet-pen");
    d.command("layer.new.vector");
    d.settle(2);
    let layer = top_layer(&d);
    let middle = d.runtime.shell.page_rect().center();
    d.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    let start = middle + vec2(-100.0, -40.0);
    d.drag(
        start,
        start + vec2(200.0, 0.0),
        egui::PointerButton::Primary,
    );
    let first = lines_on(&d, layer);
    assert_eq!(first.len(), 1);
    let target = first[0].samples[0].at;
    let gap = |line: &sue_paint::vector::Line| {
        let end = line.samples.last().unwrap().at;
        (end[0] - target[0]).hypot(end[1] - target[1])
    };
    // Magnet off: the end stays where the pen let go, a little apart.
    let near = start + vec2(2.0, 2.0);
    d.drag(near + vec2(0.0, 120.0), near, egui::PointerButton::Primary);
    let apart = lines_on(&d, layer);
    assert_eq!(apart.len(), 2);
    assert!(gap(&apart[1]) > 0.5, "apart: {}", gap(&apart[1]));
    d.command("edit.undo");
    d.settle(2);
    assert_eq!(lines_on(&d, layer), first);
    // Magnet on, from Tool Property: the same stroke joins the end.
    d.click_text("Tool Property");
    d.click_text("Vector magnet");
    let before = (lines_on(&d, layer), inked(&d, layer));
    d.drag(near + vec2(0.0, 120.0), near, egui::PointerButton::Primary);
    let joined = lines_on(&d, layer);
    assert_eq!(joined.len(), 2);
    assert!(gap(&joined[1]) < 0.01, "joined: {}", gap(&joined[1]));
    assert_eq!(joined[0], first[0], "the other line stays");
    // The pixels are the lines': painting the layer again from them
    // gives the same picture.
    let raster = sue_paint::doc::raster(&d.runtime.engine.world.arena, layer).unwrap();
    let again = sue_paint::vector::render(&joined, raster.width(), raster.height()).unwrap();
    assert_eq!(
        raster.bytes(),
        again.bytes(),
        "pixels follow the joined line"
    );
    let after = (joined, inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);
}

/// Layer, Update Lines to New Brush Engine (the action P3-8 left as a
/// model function): lines drawn with the legacy engine become lines of
/// the brush engine as one step, the notice says so, and a second run
/// says there is nothing left. A line with a flick or post correction
/// stays legacy (`vector::update_lines_kept`, tested in sue-picture).
#[test]
fn sue_paint_update_lines_to_the_new_brush_engine_is_one_step() {
    let mut d = PaintDriver::new("paint-vector-update");
    d.command("layer.new.vector");
    d.settle(2);
    let layer = top_layer(&d);
    // Two lines as Sue 0.0.75 kept them (new strokes are lines of the
    // brush engine already), painted onto the layer as it saved them.
    let pen = sue_paint::brush::Tip {
        size: 6.0,
        ..sue_paint::brush::Tip::default()
    };
    let legacy: Vec<sue_paint::vector::Line> = [100.0, 160.0]
        .into_iter()
        .map(|y| {
            let points: Vec<[f32; 2]> = (0..=16).map(|i| [80.0 + 10.0 * i as f32, y]).collect();
            sue_paint::vector::figure_line(&points, pen, [[0.0; 3], [1.0; 3]], false)
        })
        .collect();
    {
        let arena = &mut d.runtime.engine.world.arena;
        let old = sue_paint::vector::geometry(arena, layer);
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let painted = sue_paint::vector::render(&legacy, raster.width(), raster.height()).unwrap();
        let _ = sue_paint::vector::keep(arena, layer, old, &legacy, None, "old lines");
        if let (Some(into), Some(from)) = (
            sue_paint::doc::raster_mut(arena, layer).and_then(sue_core::RasterTile2D::bytes_mut),
            painted.bytes(),
        ) {
            into.copy_from_slice(from);
        }
    }
    d.settle(2);
    let drawn = lines_on(&d, layer);
    assert_eq!(drawn.len(), 2);
    assert!(
        drawn.iter().all(|line| line.engine.is_none()),
        "lines of the legacy engine"
    );
    let waiting = sue_paint::vector::legacy_lines(&d.runtime.engine.world.arena, layer);
    let before = (drawn.clone(), inked(&d, layer));
    d.command("layer.update_lines");
    let texts = d.settle(3);
    let updated = lines_on(&d, layer);
    for (was, now) in drawn.iter().zip(&updated) {
        assert_eq!(
            now.engine.is_some(),
            sue_paint::vector::updates_unchanged(was),
            "updated unless it would lose its flick or post correction"
        );
    }
    let notice = if waiting.kept > 0 {
        format!(
            "Updated {} lines to the new brush engine. {} lines with a flick or post correction stay as they are, since the new engine cannot draw those yet.",
            waiting.updated, waiting.kept
        )
    } else {
        format!(
            "Updated {} lines to the new brush engine. Undo takes it back.",
            waiting.updated
        )
    };
    assert!(
        texts.iter().any(|(text, _)| text.contains(&notice[..30])),
        "the notice says what changed: {texts:?}"
    );
    let after = (updated, inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);
    d.command("layer.update_lines");
    let texts = d.settle(3);
    assert!(
        texts.iter().any(
            |(text, _)| text.contains("already uses the new brush engine")
                || text.contains("Nothing was updated")
        ),
        "a second run says there is nothing left"
    );
    assert_eq!(lines_on(&d, layer), after.0, "and changes nothing");
}

/// TL 01:46 (crash tips, vector lines): with Correct Line's control
/// points, the open end of a line dragged onto its other end closes the
/// shape: the ends meet exactly, not only near each other. One step with
/// its undo pair.
#[test]
fn sue_paint_an_open_end_dragged_onto_the_other_closes_the_line() {
    let mut d = PaintDriver::new("paint-vector-close");
    d.command("layer.new.vector");
    d.settle(2);
    let layer = top_layer(&d);
    let middle = d.runtime.shell.page_rect().center();
    d.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    // A short line to measure the view by, then an open square.
    let mark = middle + vec2(-200.0, -150.0);
    d.drag(mark, mark + vec2(30.0, 0.0), egui::PointerButton::Primary);
    let corner = middle + vec2(-60.0, -60.0);
    let square = [
        corner,
        corner + vec2(120.0, 0.0),
        corner + vec2(120.0, 120.0),
        corner + vec2(0.0, 120.0),
        corner + vec2(0.0, 30.0),
    ];
    trace(&mut d, &square, egui::Modifiers::NONE);
    let drawn = lines_on(&d, layer);
    assert_eq!(drawn.len(), 2);
    let (a, b) = (drawn[0].samples[0].at, drawn[1].samples[0].at);
    let scale = (corner.y - mark.y) / (b[1] - a[1]);
    let screen = |at: [f32; 2]| on_screen(at, corner, b, scale);
    let start = drawn[1].samples[0].at;
    let end = drawn[1].samples.last().unwrap().at;
    assert!((end[0] - start[0]).hypot(end[1] - start[1]) > 10.0, "open");
    // Correct Line (Y), its first sub tool: move control points.
    d.step(vec![key(egui::Key::Y, egui::Modifiers::NONE)]);
    d.settle(2);
    let before = (lines_on(&d, layer), inked(&d, layer));
    let to = screen(start) + vec2(3.0, 4.0);
    d.drag(screen(end), to, egui::PointerButton::Primary);
    let closed = lines_on(&d, layer);
    let last = closed[1].samples.last().unwrap().at;
    assert_eq!(last, closed[1].samples[0].at, "the ends meet exactly");
    assert_eq!(closed[0], drawn[0], "the other line stays");
    let after = (closed, inked(&d, layer));
    undo_pair(&mut d, layer, &before, &after);
}

/// FG-25 and FGV-33 (full guide 00:34:56 and 00:35:22): Fill, Refer
/// Other Layers, with Fill up to vector path. Inside a vector ellipse,
/// a fill on the raster layer below stops at the line's soft edge; with
/// the option on it reaches in under the line to its middle, so it
/// covers more, and stays inside.
#[test]
fn sue_paint_fill_up_to_vector_path_reaches_under_the_line() {
    let mut d = PaintDriver::new("paint-vector-fill");
    let picture = picture_of(&d);
    let paint_layer = top_layer(&d);
    d.command("layer.new.vector");
    d.settle(2);
    let lines_layer = top_layer(&d);
    let middle = d.runtime.shell.page_rect().center();
    // A thick ellipse on the vector layer.
    d.command("tool.pick.figure");
    d.command("tool.next_sub");
    d.command("tool.next_sub");
    d.settle(2);
    d.drag(
        middle + vec2(-90.0, -60.0),
        middle + vec2(90.0, 60.0),
        egui::PointerButton::Primary,
    );
    assert_eq!(lines_on(&d, lines_layer).len(), 1);
    // Back to the raster layer below, and Fill: Refer Other Layers.
    d.command("layer.select_below");
    d.settle(2);
    let arena = &d.runtime.engine.world.arena;
    assert_eq!(
        sue_paint::doc::layers(arena, picture)
            .iter()
            .position(|info| info.id == paint_layer),
        Some(sue_paint::doc::layers(arena, picture).len() - 2)
    );
    d.command("tool.pick.fill");
    d.settle(2);
    let fill = |d: &mut PaintDriver| {
        d.drag(middle, middle, egui::PointerButton::Primary);
        inked(d, paint_layer)
    };
    let plain = fill(&mut d);
    assert!(plain > 1000, "the inside fills: {plain}");
    d.command("edit.undo");
    d.settle(2);
    assert_eq!(inked(&d, paint_layer), 0);
    d.click_text("Tool Property");
    // Under P4-2's rows it sits low in Tool Property: the wheel brings it
    // into view.
    let tab = d.rect_of("Tool Property").expect("the Tool Property tab");
    for _ in 0..40 {
        if d.rect_of("Fill up to vector path").is_some() {
            break;
        }
        let over = tab.center() + vec2(0.0, 120.0);
        d.step(vec![
            egui::Event::PointerMoved(over),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -40.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
        d.settle(2);
    }
    d.click_text("Fill up to vector path");
    let under = fill(&mut d);
    assert!(
        under > plain,
        "it reaches in under the line: {under} against {plain}"
    );
    // Only to the line's middle: far less than the whole canvas.
    let raster = sue_paint::doc::raster(&d.runtime.engine.world.arena, paint_layer).unwrap();
    let all = (raster.width() * raster.height()) as usize;
    assert!(under < all / 2, "it stays inside the ellipse");
    d.command("edit.undo");
    d.settle(2);
    assert_eq!(inked(&d, paint_layer), 0, "one step");
}
