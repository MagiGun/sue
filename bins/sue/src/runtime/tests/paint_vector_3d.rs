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
