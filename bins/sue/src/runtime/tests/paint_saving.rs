//! What File, Save writes in Sue Paint: the Sue project by default; the
//! `.clip` a canvas came from only once the person said so; a flat
//! picture back in its format, with a word first when layers would be
//! merged. Save never writes a file the person did not choose.

use super::*;

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// Opens `path` with Sue Paint's own Open.
fn open(driver: &mut PaintDriver, path: &std::path::Path) {
    driver.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(paint()), "open", "Open"),
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        driver.now,
    );
    driver.settle(20);
}

/// The canvas Sue Paint has in front.
fn canvas(driver: &PaintDriver) -> sue_core::EntityId {
    driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The window title Sue Paint asks for.
fn title(driver: &PaintDriver) -> String {
    driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.window_title(&driver.runtime.engine.world.arena))
        .unwrap_or_default()
}

/// Ctrl+S, and the frames after.
fn save(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::S, egui::Modifiers::COMMAND)]);
    driver.settle(6);
}

/// A `.clip` of `width` by `height` with one layer, made by Sue's writer
/// (as any file made elsewhere is read).
fn a_clip(width: u32, height: u32) -> Vec<u8> {
    let mut doc = sue_io::clip::ClipDocument::new(width, height, 300.0);
    doc.root.children.push(sue_io::clip::ClipLayer::raster(
        "Ink",
        4,
        4,
        sue_io::clip::ClipBitmap::color(8, 8, &[9_u8, 9, 9, 255].repeat(64)),
    ));
    sue_io::clip::write(&doc).unwrap()
}

/// The layers of the canvas in front.
fn layer_count(driver: &PaintDriver) -> usize {
    sue_model::picture::layers(&driver.runtime.engine.world.arena, canvas(driver)).len()
}

#[test]
fn a_new_canvas_saves_as_a_sue_project() {
    let mut driver = PaintDriver::new("save-new");
    assert_eq!(driver.runtime.pending_pick(), None);
    save(&mut driver);
    let asked = driver.runtime.pending_pick().expect("Save asks where");
    assert_eq!(asked.purpose, "save-project");
    assert!(
        asked.types.iter().all(|kind| kind.extensions == ["sue"]),
        "a Sue project, not a .clip: {:?}",
        asked.types
    );
    assert_eq!(
        driver
            .runtime
            .engine
            .world
            .rice
            .get_str("sue_paint.save_format"),
        Some("sue")
    );
}

#[test]
fn save_on_a_canvas_from_a_clip_asks_once_and_then_exports_back() {
    let mut driver = PaintDriver::new("save-clip");
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("Elsewhere.clip");
    let original = a_clip(64, 48);
    std::fs::write(&path, &original).unwrap();
    open(&mut driver, &path);
    assert!(
        title(&driver).starts_with("Elsewhere.clip"),
        "the title names the file: {}",
        title(&driver)
    );
    // The first Save writes nothing and asks.
    save(&mut driver);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "nothing written yet"
    );
    assert!(driver.rect_of("Export to the .clip").is_some(), "Save asks");
    assert!(driver.rect_of("Keep in a Sue Project").is_some());
    // Cancel: still nothing written, and the next Save asks again.
    driver.click_text("Cancel");
    assert!(driver.rect_of("Export to the .clip").is_none());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    save(&mut driver);
    assert!(
        driver.rect_of("Export to the .clip").is_some(),
        "asked again"
    );
    driver.click_text("Export to the .clip");
    let written = sue_io::clip::read(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written.sue, None, "no hidden Sue project");
    assert_eq!(written.layers()[0].name, "Ink");
    // Answered: the next Save exports without a word.
    std::fs::remove_file(&path).unwrap();
    save(&mut driver);
    assert!(
        driver.rect_of("Export to the .clip").is_none(),
        "asks only once"
    );
    assert!(path.exists(), "Save exported back");
}

#[test]
fn keeping_a_clip_canvas_in_a_project_asks_where_beside_it() {
    let mut driver = PaintDriver::new("save-clip-keep");
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("Comic page.clip");
    let original = a_clip(32, 32);
    std::fs::write(&path, &original).unwrap();
    open(&mut driver, &path);
    save(&mut driver);
    driver.click_text("Keep in a Sue Project");
    let asked = driver.runtime.pending_pick().expect("asks where").clone();
    assert_eq!(asked.purpose, "save-project");
    assert_eq!(asked.save_as.as_deref(), Some("Comic page.sue"));
    assert_eq!(asked.start.as_deref(), Some(driver.root.as_path()));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "the .clip is left alone"
    );
    // Saved where the person said: the project, from then on.
    let project = driver.root.join("Comic page.sue");
    driver.runtime.request(
        ShellRequest::Picked {
            pick: asked,
            path: project.clone(),
        },
        &mut NoTextures,
        driver.now,
    );
    driver.settle(4);
    assert!(sue_core::SueFile::from_bytes(&std::fs::read(&project).unwrap()).is_ok());
    assert!(!title(&driver).starts_with("Comic page.clip"));
    save(&mut driver);
    assert!(driver.rect_of("Export to the .clip").is_none());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // Export Back to the Same .clip still writes it, in one step; the
    // canvas was not changed, so the file comes back byte for byte.
    std::fs::remove_file(&path).unwrap();
    driver.command("file.export.clip_back");
    driver.settle(4);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "exported back as it was"
    );
}

#[test]
fn a_canvas_from_a_png_saves_back_and_warns_before_merging_layers() {
    let mut driver = PaintDriver::new("save-png");
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("Sketch.png");
    let pixels = [200_u8, 10, 10, 255].repeat(16 * 12);
    let tile =
        sue_core::RasterTile2D::from_bytes(16, 12, sue_core::PixelFormat::Rgba8, pixels).unwrap();
    std::fs::write(&path, sue_io::encode_png(&tile).unwrap()).unwrap();
    open(&mut driver, &path);
    assert!(
        title(&driver).starts_with("Sketch.png"),
        "{}",
        title(&driver)
    );
    // One layer: Save writes the PNG back, no question.
    std::fs::remove_file(&path).unwrap();
    save(&mut driver);
    assert!(driver.rect_of("Save as PNG").is_none(), "no question");
    let saved = sue_io::decode_png(&std::fs::read(&path).unwrap())
        .unwrap()
        .tile;
    assert_eq!([saved.width(), saved.height()], [16, 12]);
    assert_eq!(&saved.bytes().unwrap()[..4], &[200, 10, 10, 255]);
    // A second layer: Save says it merges them, and asks.
    driver.command("layer.new");
    driver.settle(4);
    assert_eq!(layer_count(&driver), 2);
    let before = std::fs::read(&path).unwrap();
    save(&mut driver);
    assert!(
        driver.rect_of("Save as PNG").is_some(),
        "the layers warning"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before, "nothing written yet");
    driver.click_text("Save as PNG");
    assert!(sue_io::decode_png(&std::fs::read(&path).unwrap()).is_ok());
    assert_eq!(layer_count(&driver), 2, "the canvas keeps its layers");
    // Once said, Save writes it as it is.
    std::fs::remove_file(&path).unwrap();
    save(&mut driver);
    assert!(driver.rect_of("Save as PNG").is_none());
    assert!(path.exists());
}
