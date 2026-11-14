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
    command(&mut runtime, "anim.timeline.new");
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
        "anim.timeline.new",
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
    paint.command("anim.timeline.new");
    let texts = paint.settle(30);
    // The Timeline sits under the canvas: its frames' numbers along
    // its ruler, between the columns.
    let frames = egui::Rect::from_min_max(Pos2::new(380.0, 600.0), Pos2::new(1100.0, 900.0));
    let label = |texts: &[(String, egui::Rect)], name: &str| {
        text_in(texts, name, frames).unwrap_or_else(|| {
            panic!("frame {name} is numbered on the Timeline's ruler: {texts:?}")
        })
    };
    let (four, seven) = (label(&texts, "4"), label(&texts, "7"));
    assert!((four.top() - seven.top()).abs() < 0.5, "one ruler");
    let spacing = seven.left() - four.left();
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
    let (four_now, seven_now) = (label(&texts, "4"), label(&texts, "7"));
    let zoomed = seven_now.left() - four_now.left();
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
    let ten = label(&texts, "10");
    paint.step(vec![
        egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
        wheel(vec2(0.0, -60.0), egui::Modifiers::SHIFT),
    ]);
    let texts = paint.settle(30);
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    let texts_after = paint.settle(2);
    let ten_now = label(&texts_after, "10");
    let thirteen_now = label(&texts, "13");
    assert!(
        (ten_now.left() - (ten.left() - 60.0)).abs() < 3.0,
        "frame 10 moved 60 points left: {ten:?} then {ten_now:?}"
    );
    assert!(
        ((thirteen_now.left() - ten_now.left()) - zoomed).abs() < 1.0,
        "the zoom stays"
    );

    // A middle-button drag pans the frames with the pointer.
    let from = ten_now.center() + vec2(0.0, 40.0);
    let texts = paint.drag(from, from + vec2(45.0, 0.0), egui::PointerButton::Middle);
    let ten_dragged = label(&texts, "10");
    assert!(
        (ten_dragged.left() - (ten_now.left() + 45.0)).abs() < 3.0,
        "frame 10 followed the middle button: {ten_now:?} then {ten_dragged:?}"
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
        .find(|(text, rect)| text.ends_with(" :") && rect.top() > 600.0)
        .map(|(_, rect)| *rect)
        .expect("the animation folder's name beside its track");
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
