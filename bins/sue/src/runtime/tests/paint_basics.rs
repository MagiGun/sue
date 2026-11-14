//! Sue Paint: pictures, undo, saving, the start page, every command.

use super::*;

#[test]
fn sue_paint_makes_a_picture_draws_with_a_pen_and_undoes() {
    let (mut runtime, root) = runtime("paint-studio");
    let ctx = egui::Context::default();
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    // The studio's panels slide in first.
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert!(runtime.shell.reveal() > 0.999);
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID)
    );
    step(&mut runtime, Vec::new());
    // Ctrl+N opens New, Enter makes the picture with the riced size.
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .expect("New made a picture");
    assert_eq!(
        sue_model::picture::size(arena.get_by_id(picture).unwrap()),
        Some([1600, 1200])
    );
    let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    let alpha = |runtime: &Runtime, x: u32, y: u32| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize]
    };
    let depth = runtime.engine.history.depth();

    // A pen stroke from the middle of the canvas to the right, with pen
    // pressure. The picture is fitted, so the middle is pixel (800, 600).
    let viewport_center = paint_middle(&runtime);
    let to = viewport_center + vec2(100.0, 0.0);
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: Some(0.6),
    };
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Start, viewport_center),
            egui::Event::PointerMoved(viewport_center),
            egui::Event::PointerButton {
                pos: viewport_center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Move, to),
            egui::Event::PointerMoved(viewport_center + vec2(50.0, 0.0)),
            egui::Event::PointerMoved(to),
        ],
    );
    // A pen's touch ends before its button does.
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::End, to),
            egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(
        runtime.engine.history.depth(),
        depth + 1,
        "one stroke, one undo step"
    );
    let across = |runtime: &Runtime| (590..610).map(|y| alpha(runtime, 900, y)).max();
    assert!(across(&runtime) > Some(200), "the stroke crossed x = 900");
    assert_eq!(alpha(&runtime, 900, 700), 0, "and stayed thin");
    // Pressure 0.6 of a 10 px G-pen (Clip Studio's size): about 6 px
    // wide, not 10.
    let wide = (580..620)
        .filter(|y| alpha(&runtime, 900, *y) > 128)
        .count();
    assert!((4..=8).contains(&wide), "{wide} px wide");
    // The G-pen tapers into its end instead of leaving a blob.
    let ink = |x: u32| {
        (580..620)
            .map(|y| u32::from(alpha(&runtime, x, y)))
            .sum::<u32>()
    };
    let end = (900..1300).rev().find(|x| ink(*x) > 0).unwrap();
    assert!(end > 990, "the stroke reached x = {end}");
    assert!(ink(end - 4) < ink(900), "it thins toward its end");

    // Ctrl+Shift+N adds a layer above; Ctrl+Z takes it away again, and
    // a second Ctrl+Z the stroke.
    step(
        &mut runtime,
        vec![key(
            egui::Key::N,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )],
    );
    assert_eq!(
        sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
        3
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(
        sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
        2
    );
    assert_eq!(across(&runtime), Some(0), "undo took the stroke away");

    // The picture lives in the project: it saves and comes back.
    let path = root.join("paint.sue");
    runtime.save_project_to(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    runtime.open_project(&path, &bytes).unwrap();
    assert!(runtime.engine.world.arena.get_by_id(picture).is_ok());
    assert_eq!(
        sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
        2
    );
    step(&mut runtime, Vec::new());
    let _ = std::fs::remove_dir_all(&root);
}

/// Every Sue Paint menu command runs on a canvas, and every tool draws
/// on it, without a panic and without saying it is not there yet.
#[test]
fn sue_paint_every_menu_command_and_tool_works() {
    let (mut runtime, root) = runtime("paint-everything");
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
    let not_yet = |runtime: &Runtime| {
        runtime.shell.message().is_some_and(|(text, _)| {
            text.contains("not in Sue Paint yet") || text.contains("arrives in a later")
        })
    };
    // Every tool, each sub tool its key cycles to, drawn across the
    // canvas.
    let middle = paint_middle(&runtime);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    for letter in [
        egui::Key::P,
        egui::Key::B,
        egui::Key::E,
        egui::Key::J,
        egui::Key::G,
        egui::Key::U,
        egui::Key::T,
        egui::Key::Y,
        egui::Key::M,
        egui::Key::W,
        egui::Key::I,
        egui::Key::O,
        egui::Key::K,
        egui::Key::H,
    ] {
        for _ in 0..4 {
            step(&mut runtime, vec![key(letter, plain)]);
            let from = middle - vec2(60.0, 20.0);
            step(
                &mut runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            for part in 1..=6 {
                step(
                    &mut runtime,
                    vec![egui::Event::PointerMoved(
                        from + vec2(part as f32 * 20.0, part as f32 * 8.0),
                    )],
                );
            }
            step(&mut runtime, vec![press(from + vec2(120.0, 48.0), false)]);
            step(&mut runtime, Vec::new());
            assert!(
                !not_yet(&runtime),
                "{letter:?}: {:?}",
                runtime.shell.message()
            );
            step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
            step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
            step(&mut runtime, Vec::new());
        }
    }
    // Every command in the menus, as Clip Studio Paint's are laid out.
    // First the entries the studio adds while it runs (Open Recent's
    // last item and Window, Workspace), then the menu bar itself.
    let mut commands: Vec<String> = [
        "file.recent.clear",
        "window.workspace.save",
        "window.workspace.manage",
        "window.workspace.default",
        "window.workspace.reset",
    ]
    .map(str::to_owned)
    .to_vec();
    let menus = sue_paint::menus();
    commands.extend(
        menus
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(sue_wm::MenuItem::commands)
            .map(str::to_owned),
    );
    assert!(commands.len() > 300, "{}", commands.len());
    // Leaving, quitting, printing and help go outside the canvas.
    let outside = [
        "file.leave",
        "file.quit",
        "file.close",
        "file.print",
        "help.",
        "window.new",
        "window.interface.",
    ];
    for command in commands
        .iter()
        .filter(|command| !outside.iter().any(|skip| command.starts_with(skip)))
    {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.clone(),
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        assert!(
            !not_yet(&runtime),
            "{command}: {:?}",
            runtime.shell.message()
        );
        // Close whatever opened.
        step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
        step(&mut runtime, Vec::new());
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn each_canvas_keeps_its_own_undo_history() {
    let (mut runtime, root) = runtime("paint-histories");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
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
    let focus = |runtime: &Runtime| runtime.applets.get(&paint).and_then(|a| a.focus()).unwrap();
    let count = |runtime: &Runtime, picture| {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture).len()
    };
    // Two canvases, a new layer in each.
    let mut pictures = Vec::new();
    for _ in 0..2 {
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let picture = focus(&runtime);
        let before = count(&runtime, picture);
        command(&mut runtime, "layer.new");
        step(&mut runtime, Vec::new());
        assert_eq!(count(&runtime, picture), before + 1);
        pictures.push((picture, before));
    }
    // Undo in the second takes back its layer only.
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(count(&runtime, pictures[1].0), pictures[1].1);
    assert_eq!(
        count(&runtime, pictures[0].0),
        pictures[0].1 + 1,
        "the first keeps its"
    );
    // Back in the first, its own undo takes back its own layer.
    command(&mut runtime, "window.prev_tab");
    step(&mut runtime, Vec::new());
    assert_eq!(focus(&runtime), pictures[0].0);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(count(&runtime, pictures[0].0), pictures[0].1);
    assert_eq!(count(&runtime, pictures[1].0), pictures[1].1);
    // A setting changed meanwhile is Sue's, not a canvas's.
    assert!(
        runtime
            .engine
            .history
            .history()
            .iter()
            .all(|label| !label.starts_with("Set ")),
        "no settings in a canvas's history"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_clip_file_opens_in_sue_paint_from_anywhere() {
    let (mut runtime, root) = runtime("clip-open");
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    std::fs::create_dir_all(&root).unwrap();
    let psd = sue_io::psd::Psd {
        width: 64,
        height: 48,
        layers: vec![sue_io::psd::PsdLayer::pixels(
            "Ink",
            4,
            4,
            8,
            8,
            [9_u8, 9, 9, 255].repeat(64),
        )],
        merged: Vec::new(),
    };
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };
    // The Sue menu's Open File (and the start page's Open), Sue
    // Paint's own Open, and a file dropped on the window.
    for (index, how) in ["open-file", "paint-open", "dropped"].iter().enumerate() {
        let path = root.join(format!("Made in Clip Studio {index}.clip"));
        std::fs::write(&path, sue_io::clip::encode_clip(&psd, 300.0, None).unwrap()).unwrap();
        let before = pictures(&runtime);
        match *how {
            "open-file" => runtime.open_path(&path, &mut NoTextures, now),
            "paint-open" => runtime.request(
                ShellRequest::Picked {
                    pick: FilePick::open(
                        Some(AppletId::new(sue_paint::ID).unwrap()),
                        "open",
                        "Open",
                    ),
                    path: path.clone(),
                },
                &mut NoTextures,
                now,
            ),
            // A dropped file reaches the runtime as this request.
            _ => runtime.request(ShellRequest::OpenFile(path.clone()), &mut NoTextures, now),
        }
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        assert_eq!(
            pictures(&runtime),
            before + 1,
            "{how}: the .clip opened as a canvas"
        );
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
            Some(sue_paint::ID),
            "{how}: in Sue Paint"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_paints_nothing_under_a_menu_or_sheet() {
    let (mut runtime, root) = runtime("paint-covered");
    let ctx = egui::Context::default();
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
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
    let bytes = |runtime: &Runtime| {
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        sue_paint::doc::raster(arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec()
    };
    let middle = paint_middle(&runtime);
    // A sheet painted over the left of the canvas, as a phone's are: on
    // the page's layer, over the stroke's path.
    let sheet = egui::Rect::from_min_max(middle - vec2(300.0, 200.0), middle + vec2(0.0, 200.0));
    let stroke = |runtime: &mut Runtime,
                  step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                  from: egui::Pos2| {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        step(
            runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        for i in 1..=6 {
            let at = from + vec2(20.0 * i as f32, 0.0);
            step(runtime, vec![egui::Event::PointerMoved(at)]);
        }
        step(runtime, vec![press(from + vec2(120.0, 0.0), false)]);
    };
    let before = bytes(&runtime);
    let depth = runtime.engine.history.depth();
    let covered_frames = |ctx: &egui::Context| {
        sue_wm::cover::cover(ctx, egui::LayerId::background(), sheet);
    };
    covered_frames(&ctx);
    let mut covered_step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        covered_frames(&ctx);
        step(runtime, events);
    };
    stroke(&mut runtime, &mut covered_step, middle - vec2(250.0, 0.0));
    assert_eq!(bytes(&runtime), before, "nothing painted under the sheet");
    assert_eq!(runtime.engine.history.depth(), depth);
    // Beside the sheet the canvas paints as ever.
    stroke(&mut runtime, &mut covered_step, middle + vec2(40.0, 0.0));
    assert_ne!(bytes(&runtime), before, "painted beside it");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_saves_as_a_project_or_any_picture_type() {
    let (mut runtime, root) = runtime("paint-save-as");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
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
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let pick = FilePick::save(Some(paint.clone()), "save-as", "Save As", "Picture.sue");
    std::fs::create_dir_all(&root).unwrap();

    // A picture type writes the flattened picture in that format.
    for (name, format) in [
        ("Picture.qoi", sue_io::PictureFormat::Qoi),
        ("Picture.webp", sue_io::PictureFormat::WebP),
    ] {
        let path = root.join(name);
        runtime.request(
            ShellRequest::Picked {
                pick: pick.clone(),
                path: path.clone(),
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("{name} was written"));
        let decoded = sue_io::decode_picture(&bytes, Some(format)).unwrap();
        assert_eq!([decoded.width(), decoded.height()], [1600, 1200]);
    }
    assert_eq!(runtime.project(), None, "a picture type is not the project");

    // The Sue project type saves the whole project there.
    let project = root.join("Picture.sue");
    runtime.request(
        ShellRequest::Picked {
            pick,
            path: project.clone(),
        },
        &mut NoTextures,
        start,
    );
    step(&mut runtime, Vec::new());
    step(&mut runtime, Vec::new());
    assert_eq!(runtime.project(), Some(project.as_path()));
    assert!(sue_core::SueFile::from_bytes(&std::fs::read(&project).unwrap()).is_ok());

    // Clip Studio's type writes a .clip with the picture's layers and
    // Sue's own copy of it, and Save writes that file again from then on.
    let clip = root.join("Picture.clip");
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(Some(paint.clone()), "save-as", "Save As", "Picture.clip"),
            path: clip.clone(),
        },
        &mut NoTextures,
        start,
    );
    step(&mut runtime, Vec::new());
    let read = sue_io::clip::decode_clip(&std::fs::read(&clip).unwrap()).unwrap();
    assert_eq!((read.psd.width, read.psd.height), (1600, 1200));
    assert!(read.sue.is_some(), "Sue's copy rides along");
    std::fs::remove_file(&clip).unwrap();
    step(
        &mut runtime,
        vec![key(egui::Key::S, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    assert!(clip.exists(), "Save wrote the .clip again");

    // A .clip made elsewhere opens as a new canvas with its layers.
    let mut layer =
        sue_io::psd::PsdLayer::pixels("Ink", 10, 20, 30, 40, [12_u8, 34, 56, 255].repeat(30 * 40));
    layer.opacity = 128;
    let foreign = root.join("Elsewhere.clip");
    let psd = sue_io::psd::Psd {
        width: 200,
        height: 100,
        layers: vec![layer],
        merged: Vec::new(),
    };
    std::fs::write(
        &foreign,
        sue_io::clip::encode_clip(&psd, 350.0, None).unwrap(),
    )
    .unwrap();
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };
    let before = pictures(&runtime);
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(paint), "open", "Open"),
            path: foreign,
        },
        &mut NoTextures,
        start,
    );
    step(&mut runtime, Vec::new());
    assert_eq!(
        pictures(&runtime),
        before + 1,
        "the .clip opened as a canvas"
    );
    let arena = &runtime.engine.world.arena;
    let opened = arena
        .iter()
        .find(|(_, entity)| entity.name() == "Elsewhere")
        .map(|(_, entity)| entity.id())
        .expect("named after its file");
    let layers = sue_model::picture::layers(arena, opened);
    assert!(
        layers
            .iter()
            .filter_map(|id| arena.get_by_id(*id).ok())
            .any(|entity| entity.name() == "Ink"),
        "its layer came along"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_opens_on_its_start_page_where_draw_opens_the_last_picture_or_a_new_one() {
    let (mut runtime, root) = runtime("paint-start");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };
    let click = |at: Pos2| {
        move |frame: usize| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            match frame {
                0 => vec![egui::Event::PointerMoved(at)],
                1 => vec![button(true)],
                2 => vec![button(false)],
                _ => Vec::new(),
            }
        }
    };
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, start);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // The round Draw button at the top of the start page's rail.
    let draw = runtime.shell.page_rect().min + vec2(42.0, 40.0);

    // No canvas and no picture worked on lately: Draw asks for a new one.
    frames(&mut runtime, &ctx, size, start, click(draw));
    assert!(runtime.applets.captures_keys(), "Draw shows the New dialog");
    assert_eq!(pictures(&runtime), 0);
    if let Some(applet) = runtime.applets.get_mut(&paint) {
        applet.navigate("dialog/close");
    }
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());

    // With a picture opened lately, Draw opens it as a canvas.
    std::fs::create_dir_all(&root).unwrap();
    let photo = root.join("photo.png");
    let pixels = vec![120_u8; 8 * 6 * 4];
    std::fs::write(&photo, sue_io::encode_png_rgba(8, 6, &pixels).unwrap()).unwrap();
    runtime.request(
        ShellRequest::Remember {
            path: photo.clone(),
            saved: false,
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, click(draw));
    assert_eq!(pictures(&runtime), 1, "Draw opened the last picture");
    assert!(!runtime.applets.captures_keys());
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID)
    );
    // Opening it is a transfer the start page lists.
    assert!(
        runtime
            .transfers
            .list()
            .iter()
            .any(|transfer| transfer.label == "photo.png")
    );
    // The pages and popups open by name, File, Start Page included.
    let applet = runtime.applets.get_mut(&paint).unwrap();
    for target in ["start/materials", "start/help", "start/close"] {
        assert!(applet.navigate(target), "{target}");
    }
    // File, Start Page is a command Sue Paint has.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint,
            command: "file.start_page".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert!(runtime.shell.message().is_none_or(|(_, error)| !error));
    let _ = std::fs::remove_dir_all(&root);
}
