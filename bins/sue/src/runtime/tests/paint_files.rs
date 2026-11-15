//! Sue Paint takes the files that are its own: every picture as a new
//! canvas, brush files as sub tools, sound files on the timeline.

use super::*;

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// How many canvases (layered pictures) the project holds.
fn canvases(runtime: &Runtime) -> usize {
    runtime
        .engine
        .world
        .arena
        .iter()
        .filter(|(_, entity)| sue_model::picture::is_picture(entity))
        .count()
}

/// The canvas Sue Paint has in front.
fn canvas(runtime: &Runtime) -> sue_core::EntityId {
    runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The pixels of the one layer of `canvas`.
fn layer_of(runtime: &Runtime, canvas: sue_core::EntityId) -> &sue_core::RasterTile2D {
    let arena = &runtime.engine.world.arena;
    arena
        .iter()
        .filter(|(_, entity)| {
            arena.relations().parent(entity.id(), sue_core::CHILD) == Some(canvas)
        })
        .find_map(|(_, entity)| {
            entity
                .component(sue_core::ComponentKind::RasterTile2D)?
                .as_raster()
        })
        .expect("the canvas has a layer with pixels")
}

/// What Sue Paint would ask about at quit.
fn unsaved(runtime: &Runtime) -> Vec<(String, String)> {
    runtime
        .applets
        .get(&paint())
        .map(|applet| applet.unsaved(&runtime.engine.world.arena))
        .unwrap_or_default()
}

fn a_picture() -> sue_core::RasterTile2D {
    let pixels = [10_u8, 200, 30, 255].repeat(6 * 4);
    sue_core::RasterTile2D::from_bytes(6, 4, sue_core::PixelFormat::Rgba8, pixels).unwrap()
}

#[test]
fn every_picture_file_opens_as_a_canvas_in_sue_paint() {
    use sue_io::PictureFormat;
    let (mut runtime, root) = runtime("paint-files");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let tile = a_picture();
    // The file name's extension, in any case, and the format written.
    let files = [
        ("a.gif", PictureFormat::Gif),
        ("a.TGA", PictureFormat::Tga),
        ("a.qoi", PictureFormat::Qoi),
        ("a.ico", PictureFormat::Ico),
        ("a.ppm", PictureFormat::Pnm),
        ("a.pnm", PictureFormat::Pnm),
        ("a.bmp", PictureFormat::Bmp),
        ("a.webp", PictureFormat::WebP),
        ("a.tiff", PictureFormat::Tiff),
    ];
    for (name, format) in files {
        let path = root.join(name);
        let bytes = sue_io::encode_picture(&tile, format).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let before = canvases(&runtime);
        let entities = runtime.engine.world.arena.len();
        // A dropped file reaches the runtime as this request.
        runtime.request(ShellRequest::OpenFile(path.clone()), &mut NoTextures, now);
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        assert_eq!(canvases(&runtime), before + 1, "{name} opened as a canvas");
        assert_eq!(
            runtime.engine.world.arena.len(),
            entities + 2,
            "{name}: a canvas and its layer, and no copy of the file beside them"
        );
        assert_eq!(runtime.shell.page(), Some(&paint()), "{name}: in Sue Paint");
        let canvas = canvas(&runtime);
        let layer = layer_of(&runtime, canvas);
        assert_eq!((layer.width(), layer.height()), (6, 4), "{name}");
        assert_eq!(
            &layer.bytes().unwrap()[..4],
            &[10, 200, 30, 255],
            "{name}: its pixels"
        );
        // The canvas says where it came from and how large the file was.
        let entity = runtime.engine.world.arena.get_by_id(canvas).unwrap();
        assert_eq!(
            entity.meta(sue_io::meta::SOURCE),
            Some(path.display().to_string().as_str()),
            "{name}"
        );
        assert_eq!(
            entity.meta(sue_io::meta::BYTES),
            Some(bytes.len().to_string().as_str()),
            "{name}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_tga_opens_by_its_extension_alone() {
    let (mut runtime, root) = runtime("paint-tga");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let bytes = sue_io::encode_picture(&a_picture(), sue_io::PictureFormat::Tga).unwrap();
    // TGA has no signature: without the hint nothing reads these bytes.
    assert!(sue_io::decode_picture(&bytes, None).is_err());
    // So the same bytes under a name that says nothing do not open.
    let nameless = root.join("a.dat");
    std::fs::write(&nameless, &bytes).unwrap();
    let before = canvases(&runtime);
    runtime.open_path(&nameless, &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(canvases(&runtime), before);
    // Named .tga they are a canvas.
    let named = root.join("a.tga");
    std::fs::write(&named, &bytes).unwrap();
    runtime.open_path(&named, &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(canvases(&runtime), before + 1);
    assert_eq!(runtime.shell.page(), Some(&paint()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_canvas_opened_and_not_edited_is_clean_at_quit() {
    let (mut runtime, root) = runtime("paint-clean");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let path = root.join("photo.png");
    std::fs::write(&path, sue_io::encode_png(&a_picture()).unwrap()).unwrap();
    runtime.open_path(&path, &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(runtime.shell.page(), Some(&paint()));
    assert_eq!(unsaved(&runtime), Vec::new(), "nothing to ask about");
    // An edit is asked about: the check above is not empty by accident.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint(),
            command: "edit.clear".to_owned(),
        },
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let asked = unsaved(&runtime);
    assert_eq!(asked.len(), 1, "the cleared canvas is not saved");
    assert_eq!(asked[0].1, "photo");
    // Undone, it is as it was opened.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint(),
            command: "edit.undo".to_owned(),
        },
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        sue_io::encode_png(&a_picture()).unwrap(),
        "the file was never written"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_dropped_brush_file_becomes_a_sub_tool() {
    let (mut runtime, root) = runtime("paint-brush-drop");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    // A GIMP brush, version 2: a 2 by 2 gray tip called Dot.
    let mut file = Vec::new();
    let name = b"Dot\0";
    file.extend((28 + name.len() as u32).to_be_bytes());
    file.extend(2_u32.to_be_bytes());
    file.extend([2_u32, 2, 1].iter().flat_map(|value| value.to_be_bytes()));
    file.extend(b"GIMP");
    file.extend(40_u32.to_be_bytes());
    file.extend(name);
    file.extend([0, 255, 128, 0]);
    let path = root.join("Dot.GBR");
    std::fs::write(&path, &file).unwrap();
    let entities = runtime.engine.world.arena.len();
    runtime.request(ShellRequest::OpenFile(path.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let (message, error) = runtime.shell.message().expect("Sue Paint said what it did");
    assert!(!error, "{message}");
    assert_eq!(message, "One brush added from Dot.GBR");
    assert_eq!(runtime.shell.page(), Some(&paint()), "in Sue Paint");
    assert_eq!(
        runtime.engine.world.arena.len(),
        entities,
        "a brush is a tool, not a thing in the project"
    );
    // A file that only has a brush's name is refused, and says why.
    let broken = root.join("broken.abr");
    std::fs::write(&broken, b"not a brush").unwrap();
    runtime.request(ShellRequest::OpenFile(broken), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let (message, error) = runtime.shell.message().unwrap();
    assert!(error && message.contains("Photoshop brush"), "{message}");
    assert_eq!(runtime.engine.world.arena.len(), entities);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_dropped_sound_file_goes_on_the_timeline_of_the_canvas_in_front() {
    let (mut runtime, root) = runtime("paint-sound-drop");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let samples: Vec<f32> = (0..800).map(|at| (at as f32 * 0.1).sin() * 0.5).collect();
    let pcm = sue_core::PCMFloat1D::new(8_000, 1, samples).unwrap();
    let sound = root.join("beep.wav");
    std::fs::write(&sound, sue_io::encode_wav(&pcm)).unwrap();
    let sounds = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| {
                entity
                    .component(sue_core::ComponentKind::PCMFloat1D)
                    .is_some()
            })
            .count()
    };
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            now,
        );
    };

    // A canvas with a timeline in front: the sound is Sue Paint's.
    let picture = root.join("photo.png");
    std::fs::write(&picture, sue_io::encode_png(&a_picture()).unwrap()).unwrap();
    runtime.open_path(&picture, &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let canvas = canvas(&runtime);
    // Without a timeline the drop says what to do first.
    runtime.request(ShellRequest::OpenFile(sound.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let (message, error) = runtime.shell.message().unwrap();
    assert!(
        error && message.contains("Make a timeline first"),
        "{message}"
    );
    assert_eq!(sounds(&runtime), 0);
    command(&mut runtime, "anim.timeline.new");
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    runtime.request(ShellRequest::OpenFile(sound.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(sounds(&runtime), 1, "the sound came in");
    assert_eq!(runtime.shell.page(), Some(&paint()));
    assert_eq!(
        self::canvas(&runtime),
        canvas,
        "the same canvas is in front"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_float_picture_opens_as_an_8_bit_copy_and_save_asks_where() {
    let (mut runtime, root) = fresh_runtime("paint-float");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    // The file the float picture was read from. No reader of Sue's makes
    // float pixels of a file today, so the picture is put in the project
    // as a reader would leave it.
    let source = root.join("render.exr");
    std::fs::write(&source, b"float pixels, as they were").unwrap();
    let samples = [0.25_f32, 0.5, 2.0, 1.0].repeat(3 * 2);
    let tile = sue_core::RasterTile2D::from_f32(3, 2, samples.clone()).unwrap();
    let float = sue_core::Entity::new("render.exr", tile)
        .with_meta(sue_io::meta::WIDTH, "3")
        .with_meta(sue_io::meta::HEIGHT, "2")
        .with_meta(sue_io::meta::SOURCE, source.display().to_string());
    assert!(sue_io::is_flat_picture(&float));
    let handle = runtime.engine.world.arena.insert(float);
    let id = runtime.engine.world.arena.get(handle).unwrap().id();
    assert_eq!(
        runtime
            .applets
            .page_for(runtime.engine.world.arena.get(handle).unwrap()),
        Some(paint()),
        "Sue Paint takes a flat picture"
    );
    runtime.request(
        ShellRequest::OpenIn {
            entity: id,
            applet: paint(),
        },
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    assert_eq!(runtime.shell.page(), Some(&paint()));
    assert!(
        runtime.shell.message().is_none_or(|(_, error)| !error),
        "it opened without a complaint"
    );
    let canvas = canvas(&runtime);
    assert_ne!(canvas, id);
    // The canvas has an 8-bit copy, values above 1 cut off at white.
    let layer = layer_of(&runtime, canvas);
    assert_eq!(layer.format(), sue_core::PixelFormat::Rgba8);
    assert_eq!(&layer.bytes().unwrap()[..4], &[64, 128, 255, 255]);
    assert_eq!(
        runtime
            .engine
            .world
            .arena
            .get_by_id(canvas)
            .unwrap()
            .meta(sue_io::meta::SOURCE),
        Some(source.display().to_string().as_str()),
        "the canvas says where its pixels came from"
    );
    // The float picture itself is as it was.
    let kept = runtime.engine.world.arena.get_by_id(id).unwrap();
    assert_eq!(
        kept.component(sue_core::ComponentKind::RasterTile2D)
            .and_then(|component| component.as_raster())
            .and_then(sue_core::RasterTile2D::f32_samples),
        Some(samples.as_slice())
    );

    // Save asks where to write, and writes nothing until it is told.
    assert_eq!(runtime.pending_pick(), None);
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint(),
            command: "file.save".to_owned(),
        },
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let asked = runtime.pending_pick().expect("Save asked where");
    assert!(asked.save_as.is_some(), "with a save dialog");
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"float pixels, as they were",
        "the float file is not written over"
    );
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        1,
        "and nothing was written beside it"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_project_whose_canvas_adopted_a_flat_picture_opens_on_that_canvas() {
    let (mut runtime, root) = runtime("paint-adopted");
    let ctx = egui::Context::default();
    let now = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    // A flat 8-bit picture in the project, as ingest leaves one, taken by
    // Sue Paint: it becomes the layer of a new canvas, the same entity.
    let flat = sue_core::Entity::new("photo.png", a_picture())
        .with_meta(sue_io::meta::WIDTH, "6")
        .with_meta(sue_io::meta::HEIGHT, "4");
    let handle = runtime.engine.world.arena.insert(flat);
    let layer = runtime.engine.world.arena.get(handle).unwrap().id();
    runtime.request(
        ShellRequest::OpenIn {
            entity: layer,
            applet: paint(),
        },
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let picture = canvas(&runtime);
    assert_ne!(picture, layer);
    assert_eq!(
        runtime
            .engine
            .world
            .arena
            .relations()
            .parent(layer, sue_core::CHILD),
        Some(picture),
        "the flat picture is the canvas's layer"
    );
    // The layer still reads as a flat picture, so it is what a project
    // may hand Sue Paint first when it opens again (0.0.75 projects hold
    // such canvases). That opens the canvas it belongs to, with no
    // complaint that it is a layer already.
    let (first, studio) = runtime.first_page(Some(&paint())).expect("something opens");
    assert_eq!(studio, paint());
    runtime.request(ShellRequest::Hub, &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    for entity in [first, layer] {
        runtime.request(
            ShellRequest::OpenIn {
                entity,
                applet: paint(),
            },
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let complaint = runtime.shell.message().filter(|(_, error)| *error);
        assert_eq!(complaint, None, "it opened without a complaint");
        assert_eq!(runtime.shell.page(), Some(&paint()));
        assert_eq!(canvas(&runtime), picture, "the canvas, not a new one");
    }
    assert_eq!(canvases(&runtime), 1);
    let _ = std::fs::remove_dir_all(&root);
}
