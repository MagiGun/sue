//! Sue Paint: cels, the Timeline, the light table, timelapse.

use super::*;

#[test]
fn sue_paint_animates_cels_on_a_timeline_and_exports_them() {
    let (mut runtime, root) = runtime("paint-anim");
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
    let mut stroke = |runtime: &mut Runtime, y: f32| {
        let (from, to) = (middle + vec2(-60.0, y), middle + vec2(60.0, y));
        step(
            runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        step(
            runtime,
            vec![
                egui::Event::PointerMoved(middle + vec2(0.0, y)),
                egui::Event::PointerMoved(to),
            ],
        );
        step(runtime, vec![press(to, false)]);
    };

    // A cel at frame 1 drawn high, another from frame 4 drawn low.
    command(&mut runtime, "anim.cel.new");
    stroke(&mut runtime, -40.0);
    for _ in 0..3 {
        command(&mut runtime, "anim.next");
    }
    command(&mut runtime, "anim.cel.new");
    stroke(&mut runtime, 40.0);
    let arena = &runtime.engine.world.arena;
    let sequence = sue_paint::anim::timeline(arena, picture).expect("a timeline");
    let keys = sue_paint::anim::keys(&sequence.tracks[0], sequence.rate);
    assert_eq!(
        keys.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(),
        vec![0, 3]
    );

    // Onion skin and undo.
    command(&mut runtime, "anim.onion");
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "anim.cel.new");
    assert_eq!(
        runtime.engine.history.depth(),
        depth + 1,
        "a cel is one step"
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(runtime.engine.history.depth(), depth);

    // File, Import, Audio: a sound on an audio track, from the frame
    // shown; playing the animation plays it, stopping stops it.
    std::fs::create_dir_all(&root).unwrap();
    let tone = sue_core::PCMFloat1D::new(8_000, 1, vec![0.2; 8_000]).unwrap();
    std::fs::write(root.join("Tone.wav"), sue_io::encode_wav(&tone)).unwrap();
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(paint.clone()), "anim-audio", "Import Audio"),
            path: root.join("Tone.wav"),
        },
        &mut NoTextures,
        start,
    );
    let sequence = sue_paint::anim::timeline(&runtime.engine.world.arena, picture).unwrap();
    assert_eq!(
        sue_paint::anim::audio_tracks(sequence).len(),
        1,
        "an audio track"
    );
    command(&mut runtime, "anim.first");
    command(&mut runtime, "anim.play");
    step(&mut runtime, Vec::new());
    assert!(
        runtime.transport.is_playing_for("sue-paint"),
        "the sound plays with it"
    );
    command(&mut runtime, "anim.play");
    step(&mut runtime, Vec::new());
    assert!(
        !runtime.transport.is_playing_for("sue-paint"),
        "and stops with it"
    );
    // Back to the second cel's frame for what follows.
    command(&mut runtime, "anim.first");
    for _ in 0..3 {
        command(&mut runtime, "anim.next");
    }

    // Onion Skin Settings keeps its settings on the picture.
    command(&mut runtime, "anim.onion_settings");
    step(&mut runtime, Vec::new());
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    let arena = &runtime.engine.world.arena;
    assert!(
        arena
            .get_by_id(picture)
            .unwrap()
            .property(sue_paint::anim::ONION)
            .is_some()
    );
    assert_eq!(
        sue_paint::anim::Onion::of(arena, picture),
        sue_paint::anim::Onion::default()
    );

    // The light table: the cel being drawn cannot trace itself; the
    // first cel, picked in the Layer palette, goes on its light table.
    let (first, second) = (keys[0].1.unwrap(), keys[1].1.unwrap());
    command(&mut runtime, "anim.light_table.register");
    assert!(sue_paint::anim::light_table(&runtime.engine.world.arena, second).is_empty());
    command(&mut runtime, "layer.select_below");
    command(&mut runtime, "anim.light_table.register");
    assert_eq!(
        sue_paint::anim::light_table(&runtime.engine.world.arena, second),
        vec![first]
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert!(sue_paint::anim::light_table(&runtime.engine.world.arena, second).is_empty());

    // The image sequence shows one cel per frame.
    std::fs::create_dir_all(&root).unwrap();
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(paint.clone()),
                "anim-sequence",
                "Export Animation",
                "Walk.png",
            ),
            path: root.join("Walk.png"),
        },
        &mut NoTextures,
        start,
    );
    let ink = |frame: u32, upper: bool| {
        let bytes = std::fs::read(root.join(format!("Walk-{frame:04}.png"))).unwrap();
        let tile = sue_io::decode_png(&bytes).unwrap().tile;
        let (width, height) = (tile.width(), tile.height());
        let pixels = tile.bytes().unwrap();
        let rows = if upper {
            0..height / 2
        } else {
            height / 2..height
        };
        rows.flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|(x, y)| pixels[((y * width + x) * 4) as usize] < 100)
            .count()
    };
    assert!(
        ink(2, true) > 50 && ink(2, false) == 0,
        "frame 2 shows the first cel"
    );
    assert!(
        ink(5, false) > 50 && ink(5, true) == 0,
        "frame 5 shows the second cel"
    );
    assert!(root.join("Walk-0024.png").exists(), "every frame");
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(paint.clone()),
                "anim-gif",
                "Export Animation",
                "Walk.gif",
            ),
            path: root.join("Walk.gif"),
        },
        &mut NoTextures,
        start,
    );
    assert!(
        std::fs::read(root.join("Walk.gif"))
            .unwrap()
            .starts_with(b"GIF8")
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_light_table_moves_registers_files_and_shows_on_every_cel() {
    let (mut runtime, root) = runtime("paint-light-table");
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
    // A reference layer, then a timeline with a cel to draw.
    command(&mut runtime, "layer.new");
    let reference = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    command(&mut runtime, "anim.layer.folder");
    command(&mut runtime, "anim.cel.new");
    step(&mut runtime, Vec::new());
    // The reference layer on the general light table. The cel sits in
    // its animation folder, on top of the stack.
    let arena = &runtime.engine.world.arena;
    let cel = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .rev()
        .find(|info| sue_paint::anim::cel_folder(arena, info.id).is_some())
        .unwrap()
        .id;
    assert_ne!(cel, reference);
    command(&mut runtime, "layer.select_below");
    command(&mut runtime, "anim.light_table.register_general");
    step(&mut runtime, Vec::new());
    assert_eq!(
        sue_paint::anim::general_light_table(&runtime.engine.world.arena, picture),
        vec![reference]
    );
    // The Light table tool moves it, as one step; Reset puts it back.
    command(&mut runtime, "anim.light_table.tool");
    let middle = paint_middle(&runtime);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    let depth = runtime.engine.history.depth();
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    for part in 1..=5 {
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(
                middle + vec2(part as f32 * 10.0, 0.0),
            )],
        );
    }
    step(&mut runtime, vec![press(middle + vec2(50.0, 0.0), false)]);
    let place = |runtime: &Runtime| {
        sue_paint::anim::place_of(runtime.engine.world.arena.get_by_id(reference).unwrap())
    };
    assert!(place(&runtime)[0] > 5.0, "moved: {:?}", place(&runtime));
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    command(&mut runtime, "anim.light_table.reset");
    assert_eq!(place(&runtime), [0.0; 3], "back where it was");
    command(&mut runtime, "anim.light_table.tool");
    // Select and register file: a picture on the cel's light table
    // only, hidden in the picture itself.
    let file = root.join("trace.png");
    std::fs::create_dir_all(&root).unwrap();
    let tile = sue_core::RasterTile2D::from_bytes(
        8,
        8,
        sue_core::PixelFormat::Rgba8,
        [10, 20, 30, 255].repeat(64),
    )
    .unwrap();
    std::fs::write(&file, sue_io::encode_png(&tile).unwrap()).unwrap();
    command(&mut runtime, "layer.select_above");
    command(&mut runtime, "anim.light_table.file");
    let _ = runtime.answer_pick(&file, &mut NoTextures, start);
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let on_cel = sue_paint::anim::light_table(arena, cel);
    assert_eq!(on_cel.len(), 1, "the file is on the cel's light table");
    let entity = arena.get_by_id(on_cel[0]).unwrap();
    assert_eq!(
        entity
            .property(sue_paint::anim::ONLY)
            .and_then(sue_core::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        entity
            .property(sue_model::picture::VISIBLE)
            .and_then(sue_core::Value::as_bool),
        Some(false),
        "never shown in the picture itself"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_exposure_sheet_goes_out_and_back_as_csv() {
    let (mut runtime, root) = runtime("paint-xsheet");
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
    // Cel 1 on frame 1, cel 2 on frame 3.
    for name in [
        "anim.layer.folder",
        "anim.cel.new",
        "anim.next",
        "anim.next",
        "anim.cel.new",
    ] {
        command(&mut runtime, name);
    }
    let sheet = root.join("sheet.csv");
    command(&mut runtime, "file.export.exposure_sheet");
    let _ = runtime.answer_pick(&sheet, &mut NoTextures, start);
    step(&mut runtime, Vec::new());
    let text = std::fs::read_to_string(&sheet).expect("written");
    let rows: Vec<&str> = text.lines().collect();
    assert!(rows[0].starts_with("Frame,"), "{text}");
    assert_eq!(rows.len(), 25, "a header and 24 frames");
    assert_eq!((rows[1], rows[2], rows[3]), ("1,1", "2,", "3,2"));
    // Swapped in the file and read back: cel 2 first, then cel 1.
    let swapped = text
        .replacen(
            "1,1
", "1,2
", 1,
        )
        .replacen(
            "3,2
", "3,1
", 1,
        );
    std::fs::write(&sheet, swapped).unwrap();
    command(&mut runtime, "file.import.exposure_sheet");
    let _ = runtime.answer_pick(&sheet, &mut NoTextures, start);
    step(&mut runtime, Vec::new());
    command(&mut runtime, "file.export.exposure_sheet");
    let again = root.join("again.csv");
    let _ = runtime.answer_pick(&again, &mut NoTextures, start);
    let text = std::fs::read_to_string(&again).expect("written again");
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!((rows[1], rows[3]), ("1,2", "3,1"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_records_a_timelapse_and_exports_it_as_a_video() {
    let (mut runtime, root) = runtime("paint-timelapse");
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
    // File, Timelapse, Record Timelapse: the picture keeps that, and a
    // frame goes in for the canvas as it is and after each stroke.
    command(&mut runtime, "file.timelapse.record");
    for _ in 0..3 {
        step(&mut runtime, Vec::new());
    }
    let dir = sue_paint::timelapse::folder(&runtime.engine.catalog.folders["paint"], picture);
    assert_eq!(sue_paint::timelapse::frames(&dir).len(), 1);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = runtime.shell.page_rect().center();
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    for y in [-40.0, 40.0] {
        let (a, b) = (middle + vec2(-80.0, y), middle + vec2(80.0, y));
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(a), press(a, true)],
        );
        step(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(middle + vec2(0.0, y)),
                egui::Event::PointerMoved(b),
            ],
        );
        step(&mut runtime, vec![press(b, false)]);
        for _ in 0..3 {
            step(&mut runtime, Vec::new());
        }
    }
    assert_eq!(sue_paint::timelapse::frames(&dir).len(), 3, "one a stroke");
    // Export Timelapse writes a video of them.
    std::fs::create_dir_all(&root).unwrap();
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(paint.clone()),
                "timelapse",
                "Export Timelapse",
                "Drawing.avi",
            ),
            path: root.join("Drawing.avi"),
        },
        &mut NoTextures,
        start,
    );
    let video = std::fs::read(root.join("Drawing.avi")).unwrap();
    assert_eq!(&video[..4], b"RIFF");
    assert_eq!(&video[8..12], b"AVI ");
    // Stopping keeps the frames but adds no more.
    command(&mut runtime, "file.timelapse.record");
    let a = middle + vec2(-80.0, 80.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(a), press(a, true)],
    );
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(a + vec2(60.0, 0.0))],
    );
    step(&mut runtime, vec![press(a + vec2(60.0, 0.0), false)]);
    for _ in 0..3 {
        step(&mut runtime, Vec::new());
    }
    assert_eq!(sue_paint::timelapse::frames(&dir).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);

    // Edit, Register Material, Image keeps the drawing in the Material
    // palette; pasting it makes a new layer with it in the middle.
    let materials = runtime.engine.catalog.folders["paint"].join("materials");
    let _ = std::fs::remove_dir_all(&materials);
    command(&mut runtime, "edit.material.image");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    assert_eq!(
        std::fs::read_dir(&materials).unwrap().count(),
        1,
        "one material file"
    );
    let before = sue_paint::doc::layers(&runtime.engine.world.arena, picture).len();
    command(&mut runtime, "edit.material.paste");
    let stack = sue_paint::doc::layers(&runtime.engine.world.arena, picture);
    assert_eq!(stack.len(), before + 1);
    let pasted =
        sue_paint::doc::raster(&runtime.engine.world.arena, stack.last().unwrap().id).unwrap();
    assert!(
        pasted.bytes().unwrap().chunks(4).any(|pixel| pixel[3] > 0),
        "with the drawing on it"
    );
    let _ = std::fs::remove_dir_all(&materials);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_timeline_zooms_and_pans_with_the_wheel_and_the_middle_button() {
    let mut paint = PaintDriver::new("paint-timeline-nav");
    // New Timeline asks for its settings; Enter keeps them.
    paint.command("anim.timeline.new");
    paint.settle(2);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(2);
    // The timeline makes no folder by itself (TR-15): one is made here.
    paint.command("anim.layer.folder");
    let texts = paint.settle(30);
    // The Timeline sits under the canvas: its frames' numbers along
    // its ruler, one at each division line (every 6 frames).
    let frames = egui::Rect::from_min_max(Pos2::new(380.0, 600.0), Pos2::new(1100.0, 900.0));
    let label = |texts: &[(String, egui::Rect)], name: &str| {
        text_in(texts, name, frames).unwrap_or_else(|| {
            panic!("frame {name} is numbered on the Timeline's ruler: {texts:?}")
        })
    };
    let (seven, thirteen) = (label(&texts, "7"), label(&texts, "13"));
    assert!((seven.top() - thirteen.top()).abs() < 0.5, "one ruler");
    let spacing = thirteen.left() - seven.left();
    assert!(spacing > 10.0, "{spacing}");
    let wheel = |delta: egui::Vec2, modifiers| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers,
    };

    // The wheel up over frame 7 zooms in around it.
    let over = seven.left_center() + vec2(1.0, 0.0);
    paint.step(vec![egui::Event::PointerMoved(over)]);
    paint.step(vec![wheel(vec2(0.0, 60.0), egui::Modifiers::NONE)]);
    let texts = paint.settle(30);
    let (seven_now, thirteen_now) = (label(&texts, "7"), label(&texts, "13"));
    let zoomed = thirteen_now.left() - seven_now.left();
    assert!(
        zoomed > spacing * 1.15,
        "frames are wider: {spacing} then {zoomed}"
    );
    assert!(
        (seven_now.left() - seven.left()).abs() < 3.0,
        "frame 7 stays under the pointer: {seven:?} then {seven_now:?}"
    );

    // Shift and the wheel pan sideways: later frames come in, the
    // zoom stays.
    let ten = label(&texts, "13");
    paint.step(vec![
        egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
        wheel(vec2(0.0, -60.0), egui::Modifiers::SHIFT),
    ]);
    let texts = paint.settle(30);
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    let texts_after = paint.settle(2);
    let ten_now = label(&texts_after, "13");
    let nineteen_now = label(&texts, "19");
    assert!(
        (ten_now.left() - (ten.left() - 60.0)).abs() < 3.0,
        "frame 13 moved 60 points left: {ten:?} then {ten_now:?}"
    );
    assert!(
        ((nineteen_now.left() - ten_now.left()) - zoomed).abs() < 1.0,
        "the zoom stays"
    );

    // A middle-button drag pans the frames with the pointer.
    let from = ten_now.center() + vec2(0.0, 40.0);
    let texts = paint.drag(from, from + vec2(45.0, 0.0), egui::PointerButton::Middle);
    let ten_dragged = label(&texts, "13");
    assert!(
        (ten_dragged.left() - (ten_now.left() + 45.0)).abs() < 3.0,
        "frame 13 followed the middle button: {ten_now:?} then {ten_dragged:?}"
    );

    // The eye beside the animation folder hides and shows the folder
    // itself, the one the Layer palette lists.
    let folder_shown = |paint: &PaintDriver| {
        paint
            .runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| {
                entity
                    .property("folder.animation")
                    .and_then(sue_core::Value::as_bool)
                    == Some(true)
            })
            .map(|(_, entity)| sue_model::picture::LayerStyle::of(entity).visible)
            .collect::<Vec<bool>>()
    };
    assert_eq!(folder_shown(&paint), [true]);
    let name = texts
        .iter()
        .find(|(text, rect)| text.starts_with("Folder") && rect.top() > 800.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the animation folder's name beside its track: {texts:?}"));
    let eye = Pos2::new(name.left() - 34.0, name.center().y);
    let click = |paint: &mut PaintDriver| {
        let press = |pressed| egui::Event::PointerButton {
            pos: eye,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint.step(vec![egui::Event::PointerMoved(eye)]);
        paint.step(vec![press(true)]);
        paint.step(vec![press(false)]);
        paint.settle(3);
    };
    click(&mut paint);
    assert_eq!(folder_shown(&paint), [false], "the eye hid the folder");
    click(&mut paint);
    assert_eq!(folder_shown(&paint), [true], "and showed it again");
    assert!(
        paint
            .runtime
            .shell
            .message()
            .is_none_or(|(_, error)| !error),
        "{:?}",
        paint.runtime.shell.message()
    );
}

// ----- The timeline's interface (P4-7) ---------------------------------------

/// The arena as bytes (entities, ids, relations), to compare before and
/// after an undo.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

/// The picture open in Sue Paint.
fn open_picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas")
}

/// The picture's timeline, cloned.
fn timeline_of(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(&paint.runtime.engine.world.arena, open_picture(paint))
        .expect("a timeline")
        .clone()
}

/// One press of `key` with no modifier.
fn press(paint: &mut PaintDriver, key_name: egui::Key) {
    paint.step(vec![key(key_name, egui::Modifiers::NONE)]);
    paint.settle(2);
}

/// The control on the row of the label `label`, right of it: what a
/// dialog's form draws there, found by where the label was drawn.
fn right_of(paint: &PaintDriver, label: &str) -> egui::Rect {
    let at = paint
        .rect_of(label)
        .unwrap_or_else(|| panic!("{label:?} is on screen: {:?}", paint.texts));
    paint
        .texts
        .iter()
        .filter(|(_, rect)| {
            (rect.center().y - at.center().y).abs() < 8.0 && rect.left() > at.right()
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("a field beside {label:?}: {:?}", paint.texts))
}

/// Clicks the field at `rect`, types `text` over what it holds and ends
/// typing with Tab.
fn type_into(paint: &mut PaintDriver, rect: egui::Rect, text: &str) {
    let at = rect.center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.settle(2);
    paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    paint.step(vec![egui::Event::Text(text.to_owned())]);
    paint.step(vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
    paint.settle(2);
}

/// Types `text` into the field beside the label `label`.
fn fill(paint: &mut PaintDriver, label: &str, text: &str) {
    let rect = right_of(paint, label);
    type_into(paint, rect, text);
}

/// Hands Sue Paint the file a save dialog for `purpose` would pick.
fn picked(paint: &mut PaintDriver, purpose: &str, path: &std::path::Path) {
    let applet = AppletId::new(sue_paint::ID).unwrap();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(Some(applet), purpose, "Export", "x"),
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(2);
}

/// Draws a short line across the canvas `dy` points from its middle.
fn line_on_canvas(paint: &mut PaintDriver, dy: f32) {
    let middle = paint_middle(&paint.runtime);
    paint.drag(
        middle + vec2(-80.0, dy),
        middle + vec2(80.0, dy),
        egui::PointerButton::Primary,
    );
}

/// An animation folder with a drawn cel on frame 1 and another on frame
/// 4, and a 2 second tone on the timeline.
fn walk(paint: &mut PaintDriver) {
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    paint.settle(2);
    line_on_canvas(paint, -40.0);
    for _ in 0..3 {
        paint.command("anim.next");
    }
    paint.command("anim.cel.new");
    paint.settle(2);
    line_on_canvas(paint, 40.0);
    std::fs::create_dir_all(&paint.root).unwrap();
    let tone: Vec<f32> = (0..96_000)
        .map(|index| (index as f32 * 0.05).sin() * 0.5)
        .collect();
    let tone = sue_core::PCMFloat1D::new(48_000, 1, tone).unwrap();
    let path = paint.root.join("Tone.wav");
    std::fs::write(&path, sue_io::encode_wav(&tone)).unwrap();
    paint.command("anim.first");
    let applet = AppletId::new(sue_paint::ID).unwrap();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(applet), "anim-audio", "Import Audio"),
            path,
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(4);
}

#[test]
fn sue_paint_new_timeline_asks_first_and_gives_every_layer_a_track() {
    // TR-15, AB-7, ABV-2: New Timeline is a dialog; it makes no folder,
    // and every layer, the paper included, gets a track.
    let mut paint = PaintDriver::new("paint-timeline-new");
    let picture = open_picture(&paint);
    let before = arena_bytes(&paint);
    paint.command("anim.timeline.new");
    paint.settle(3);
    assert!(
        paint.rect_of("Timeline name").is_some(),
        "{:?}",
        paint.texts
    );
    fill(&mut paint, "Timeline name", "Walk cycle");
    fill(&mut paint, "Playback time", "48");
    press(&mut paint, egui::Key::Enter);
    let arena = &paint.runtime.engine.world.arena;
    let sequence = sue_paint::anim::timeline(arena, picture).expect("a timeline");
    assert!(
        sue_paint::anim::folders(sequence).is_empty(),
        "no folder made"
    );
    let layers = sue_paint::doc::layers(arena, picture).len();
    assert_eq!(sequence.tracks.len(), layers, "a track a layer");
    assert_eq!(sue_paint::anim::length(arena, picture), 48);
    assert_eq!(
        sue_paint::anim::setup::Settings::of(arena, picture).name,
        "Walk cycle"
    );
    let after = arena_bytes(&paint);
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert!(arena_bytes(&paint) == before, "one undo takes it all back");
    paint
        .runtime
        .engine
        .history
        .redo(&mut paint.runtime.engine.world)
        .unwrap();
    assert!(arena_bytes(&paint) == after, "redo");
    // A second timeline keeps the first one pick away (ABV-4).
    paint.command("anim.timeline.new");
    paint.settle(3);
    press(&mut paint, egui::Key::Enter);
    let arena = &paint.runtime.engine.world.arena;
    assert_eq!(sue_paint::anim::setup::timelines(arena, picture).len(), 2);
    let texts = paint.settle(4);
    assert!(
        texts.iter().any(|(text, _)| text == "Timeline 2"),
        "the picker names the timeline shown"
    );
}

#[test]
fn sue_paint_exports_an_mp4_with_its_sound_and_falls_back_to_avi() {
    // A25-12, D22: the movie dialog offers MP4 (H.264) first where Windows
    // has an encoder; the movie carries the timeline's sound.
    let mut paint = PaintDriver::new("paint-movie");
    walk(&mut paint);
    paint.command("file.export.movie");
    let texts = paint.settle(3);
    let mp4 = sue_platform::movie::mp4_available();
    if mp4 {
        assert!(
            texts.iter().any(|(text, _)| text == "MP4 (H.264)"),
            "MP4 first: {texts:?}"
        );
    }
    assert!(paint.rect_of("Apply 2D camera effects").is_some());
    fill(&mut paint, "Width", "320");
    press(&mut paint, egui::Key::Enter);
    let file = paint.root.join(if mp4 { "Walk.mp4" } else { "Walk.avi" });
    picked(&mut paint, "anim-movie", &file);
    let bytes = std::fs::read(&file).expect("the movie is written");
    if mp4 {
        assert_eq!(&bytes[4..8], b"ftyp", "an MP4");
        assert!(
            bytes.windows(4).any(|window| window == b"mp4a"),
            "with its sound"
        );
        // Kept for a person to play and look at (the package report).
        if let Ok(keep) = std::env::var("SUE_KEEP_MOVIE") {
            std::fs::write(keep, &bytes).unwrap();
        }
    } else {
        assert_eq!(&bytes[..4], b"RIFF");
    }
    // Where Windows has no H.264 encoder, the AVI is offered, and one line
    // says what brings MP4.
    sue_platform::movie::pretend_no_mp4_encoder(true);
    paint.command("file.export.movie");
    let texts = paint.settle(3);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.contains("Media Feature Pack")),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|(text, _)| text == "MP4 (H.264)"));
    press(&mut paint, egui::Key::Enter);
    let avi = paint.root.join("Walk.avi");
    picked(&mut paint, "anim-movie", &avi);
    sue_platform::movie::pretend_no_mp4_encoder(false);
    assert_eq!(&std::fs::read(&avi).unwrap()[..4], b"RIFF");
}

#[test]
fn sue_paint_audio_clip_settings_fade_out_ends_in_silence_and_undo_restores_it() {
    // The plan's test for Audio Clip Settings: a one second fade out ends
    // the clip in silence, as one undo step.
    let mut paint = PaintDriver::new("paint-audio-clip");
    walk(&mut paint);
    let picture = open_picture(&paint);
    let sound = |paint: &PaintDriver| {
        let sequence = timeline_of(paint);
        let track = sue_paint::anim::audio_tracks(&sequence)[0];
        let source = sequence.tracks[track].clips()[0].source;
        paint
            .runtime
            .engine
            .world
            .arena
            .get_by_id(source)
            .unwrap()
            .component(sue_core::ComponentKind::PCMFloat1D)
            .and_then(|component| component.as_pcm())
            .unwrap()
            .samples()
            .to_vec()
    };
    let loud = sound(&paint);
    let before = arena_bytes(&paint);
    paint.command("anim.audio.settings");
    paint.settle(3);
    fill(&mut paint, "Fade out", "1");
    press(&mut paint, egui::Key::Enter);
    let faded = sound(&paint);
    assert_eq!(faded.len(), loud.len(), "a fade keeps the length");
    let tail = &faded[faded.len() - 100..];
    assert!(
        tail.iter().all(|sample| sample.abs() < 0.01),
        "silence at the end"
    );
    let early = &faded[1000..2000];
    assert!(
        early.iter().any(|sample| sample.abs() > 0.3),
        "loud before the fade"
    );
    // The mix the timeline plays and exports ends in silence too.
    let mixed =
        sue_paint::anim::mix(&paint.runtime.engine.world.arena, &timeline_of(&paint)).unwrap();
    let samples = mixed.samples();
    assert!(
        samples[samples.len() - 200..]
            .iter()
            .all(|sample| sample.abs() < 0.01)
    );
    let after = arena_bytes(&paint);
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert_eq!(sound(&paint), loud, "undo restores the sound");
    assert!(arena_bytes(&paint) == before, "and nothing else changed");
    paint
        .runtime
        .engine
        .history
        .redo(&mut paint.runtime.engine.world)
        .unwrap();
    assert!(arena_bytes(&paint) == after, "redo");
    // A cut shortens the clip with its sound; the volume stays on the
    // clip in decibels.
    paint.command("anim.audio.settings");
    paint.settle(3);
    fill(&mut paint, "Cut from the start", "0.5");
    fill(&mut paint, "Volume", "-6");
    press(&mut paint, egui::Key::Enter);
    let sequence = timeline_of(&paint);
    let track = sue_paint::anim::audio_tracks(&sequence)[0];
    let clip = &sequence.tracks[track].clips()[0];
    assert!(
        (clip.duration.seconds() - 1.5).abs() < 0.01,
        "{}",
        clip.duration.seconds()
    );
    assert!((sue_paint::anim::volume_of(clip) - 0.501).abs() < 0.01);
    let _ = picture;
}

#[test]
fn sue_paint_exports_sequences_cels_animated_pictures_and_sound_with_their_settings() {
    let mut paint = PaintDriver::new("paint-export-settings");
    walk(&mut paint);
    let root = paint.root.clone();
    // A25-11: Image Sequence: a prefix, a first number and a range.
    paint.command("file.export.sequence");
    paint.settle(3);
    let prefix = right_of(&paint, "File name prefix");
    type_into(&mut paint, prefix, "take_");
    fill(&mut paint, "First number", "10");
    fill(&mut paint, "Digits", "2");
    let range = right_of(&paint, "Frames");
    paint.step(vec![egui::Event::PointerMoved(range.center())]);
    paint.click_text("Every frame");
    paint.click_text("These frames");
    fill(&mut paint, "From frame", "3");
    fill(&mut paint, "To frame", "5");
    press(&mut paint, egui::Key::Enter);
    picked(&mut paint, "anim-sequence", &root.join("Walk.png"));
    for number in 10..13 {
        assert!(root.join(format!("take_{number}.png")).exists(), "{number}");
    }
    assert!(!root.join("take_13.png").exists(), "frames 3 to 5 only");
    // A25-10: the cels, a folder for each animation folder, so cel "1" of
    // two folders never overwrites the other.
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    paint.settle(2);
    paint.command("file.export.cels");
    paint.settle(3);
    press(&mut paint, egui::Key::Enter);
    picked(&mut paint, "anim-cels", &root.join("Cels.png"));
    let cel_files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| entry.path().join("1.png").exists())
        .collect();
    assert_eq!(cel_files.len(), 2, "two folders, each with its cel 1");
    // AB-12: an animated GIF that loops three times.
    paint.command("file.export.gif");
    paint.settle(3);
    fill(&mut paint, "Loop count", "3");
    press(&mut paint, egui::Key::Enter);
    picked(&mut paint, "anim-gif", &root.join("Walk.gif"));
    let gif = std::fs::read(root.join("Walk.gif")).unwrap();
    let at = gif
        .windows(11)
        .position(|window| window == b"NETSCAPE2.0")
        .unwrap();
    assert_eq!(u16::from_le_bytes([gif[at + 13], gif[at + 14]]), 3);
    // File, Export Animation, Audio: the timeline's sound through the
    // export dialog's sound page.
    paint.command("anim.export.audio");
    paint.settle(3);
    assert!(paint.rect_of("Export").is_some(), "{:?}", paint.texts);
    paint.click_text("Export");
    picked(&mut paint, "anim-sound", &root.join("Walk.wav"));
    let wav = std::fs::read(root.join("Walk.wav")).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    let read = sue_io::decode_audio(wav, Some("wav")).unwrap().0;
    assert!(read.pcm.frames() > 40_000, "the whole sound");
}

#[test]
fn sue_paint_timeline_picks_moves_copies_and_pastes_frames_by_drag() {
    // AB-3, TR-4, TR-5, A25-2, ABV-12, A25V-13.
    let mut paint = PaintDriver::new("paint-timeline-drag");
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    let texts = paint.settle(10);
    // Where frames sit: the ruler's numbers; where the track sits: its
    // name.
    let number = |name: &str| {
        texts
            .iter()
            // The Timeline under the canvas, not the Layer palette's cels.
            .filter(|(text, rect)| text == name && rect.top() > 700.0 && rect.left() < 1150.0)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("frame {name} on the ruler: {texts:?}"))
    };
    let row = texts
        .iter()
        .find(|(text, _)| text.starts_with("Folder") && text.contains(':'))
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the track's name: {texts:?}"));
    // The ruler numbers every division line, six frames apart.
    let step = (number("7").left() - number("1").left()) / 6.0;
    let cell = |frame: u32| {
        Pos2::new(
            number("1").left() + step * (frame as f32 - 1.0) + step / 2.0,
            row.center().y + 4.0,
        )
    };
    // A drag across frames 1 to 3 picks them.
    paint.drag(cell(1), cell(3), egui::PointerButton::Primary);
    paint.command("anim.edit.copy");
    paint.settle(2);
    for _ in 0..9 {
        paint.command("anim.next");
    }
    paint.settle(2);
    // Paste lands at the frame shown: a click on the ruler goes there.
    paint.click_text("10");
    let before = arena_bytes(&paint);
    paint.command("anim.edit.paste");
    paint.settle(2);
    let sequence = timeline_of(&paint);
    let keys = sue_paint::anim::keys(&sequence.tracks[0], sequence.rate);
    let cel = keys[0].1.expect("cel 1");
    assert!(
        sue_paint::anim::cel_at(&sequence.tracks[0], sequence.rate, 9) == Some(cel),
        "pasted at frame 10: {keys:?}"
    );
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert!(arena_bytes(&paint) == before, "paste undoes");
    // Pick frame 1 alone, then drag it to frame 6: it moves there.
    paint.command("anim.first");
    paint.settle(2);
    // A click on its cel picks it (nothing else is picked).
    let press = |pressed| egui::Event::PointerButton {
        pos: cell(1),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(cell(1))]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(2);
    paint.drag(cell(1), cell(6), egui::PointerButton::Primary);
    let sequence = timeline_of(&paint);
    assert!(
        sue_paint::anim::cel_at(&sequence.tracks[0], sequence.rate, 5) == Some(cel),
        "moved to frame 6: {:?}",
        sue_paint::anim::keys(&sequence.tracks[0], sequence.rate)
    );
    assert!(
        sue_paint::anim::cel_at(&sequence.tracks[0], sequence.rate, 0) != Some(cel),
        "frame 1 is left blank: {:?}",
        sue_paint::anim::keys(&sequence.tracks[0], sequence.rate)
    );
    // A right click names the folder's cels and offers the commands of
    // that frame (A25-23, TR-16, ABV-13).
    let at = cell(2);
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }]);
    paint.step(vec![egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let texts = paint.settle(3);
    for name in [
        "1",
        "New Animation Cel",
        "Split Clip",
        "Paste Frames",
        "Track Label",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
}

#[test]
fn sue_paint_timeline_plays_its_range_reads_in_seconds_and_keeps_labels_and_keys() {
    // AB-13, A25-20, A25V-11, A25-21, TR-6, A25-16, TR-3, AB-1, AB-2.
    let mut paint = PaintDriver::new("paint-timeline-range");
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    paint.settle(4);
    let picture = open_picture(&paint);
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.play_range.start");
    for _ in 0..3 {
        paint.command("anim.next");
    }
    paint.command("anim.play_range.end");
    let settings = sue_paint::anim::setup::Settings::of(&paint.runtime.engine.world.arena, picture);
    assert_eq!((settings.start, settings.end), (2, Some(5)));
    // Playback stays in the range: the bar reads the frame shown first.
    paint.command("anim.play");
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..60 {
        let texts = paint.step(Vec::new());
        if let Some((readout, _)) = texts.iter().find(|(text, _)| text.contains(" to ")) {
            let frame: u32 = readout
                .split_whitespace()
                .last()
                .map(|_| readout.split_whitespace().next().unwrap_or("0"))
                .and_then(|first| first.parse().ok())
                .unwrap_or(0);
            if frame > 0 {
                seen.insert(frame);
            }
        }
    }
    paint.command("anim.play");
    paint.settle(2);
    assert!(
        !seen.is_empty() && seen.iter().all(|frame| (3..=6).contains(frame)),
        "played frames 3 to 6 only: {seen:?}"
    );
    assert!(seen.len() >= 3, "and played them: {seen:?}");
    // Seconds and frames on the bar: frame 3 reads "0+03".
    paint.command("anim.time_unit.seconds");
    paint.command("anim.first");
    paint.command("anim.next");
    paint.command("anim.next");
    let texts_seconds = paint.settle(3);
    assert!(
        texts_seconds
            .iter()
            .any(|(text, _)| text.starts_with("0+03")),
        "{texts_seconds:?}"
    );
    // A label, named in its dialog, over its frame.
    paint.command("anim.label");
    paint.settle(3);
    fill(&mut paint, "Label at frame 3", "Contact");
    press(&mut paint, egui::Key::Enter);
    let texts = paint.settle(3);
    assert!(texts.iter().any(|(text, _)| text == "Contact"), "{texts:?}");
    // A track label for an in-between.
    paint.command("anim.track_label.inbetween");
    let sequence = timeline_of(&paint);
    assert_eq!(
        sue_paint::anim::editing::track_labels(&sequence.tracks[0]),
        vec![(2, sue_paint::anim::editing::TrackLabel::InBetween)]
    );
    // No thumbnails: the cels' rows shrink to one line.
    paint.command("anim.thumbnails.none");
    paint.settle(2);
    // A layer keyed at frame 3, Hold; undone with one step.
    paint.command("layer.new");
    // The new layer, wherever it went (inside the folder picked or not).
    let layer = paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .find(|(_, entity)| entity.name() == "Layer 2")
        .map(|(_, entity)| entity.id())
        .expect("Layer 2 was made");
    let before = arena_bytes(&paint);
    paint.command("anim.key.layer");
    paint.command("anim.key.ease.hold");
    let arena = &paint.runtime.engine.world.arena;
    let time = sue_core::Time::from_frames(2, timeline_of(&paint).rate);
    assert_eq!(
        sue_paint::anim::motion::ease_at(arena, layer, time),
        Some(sue_paint::anim::motion::Ease::Hold)
    );
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert!(arena_bytes(&paint) == before, "keying undoes");
}
