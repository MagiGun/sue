//! Views in windows of their own, through the whole runtime: the host
//! opens what the runtime lists, draws each window with its own frames,
//! and closing one leaves the main window.

use super::*;
use sue_platform::{HeadlessWindows, MAIN_SURFACE, WindowSupport};

fn open_view(view: &str) -> ShellRequest {
    ShellRequest::OpenView {
        applet: AppletId::new(crate::gallery::ID).unwrap(),
        view: view.to_owned(),
        title: format!("Gallery {view}"),
    }
}

/// One frame of the main window and of each window the runtime lists, as
/// the desktop host draws them.
fn frame(runtime: &mut Runtime, host: &mut HeadlessWindows, now: Instant) {
    let specs = runtime.windows();
    host.sync(&specs, 1.0);
    let revision = runtime.revision();
    host.main_frame(revision, &specs, |ui| runtime.ui(ui, &mut NoTextures, now));
    for spec in &specs {
        if let Some(surface) = host.surface_of(&spec.key) {
            host.window_frame(surface, revision, |ui| {
                runtime.window_ui(&spec.key, ui, &mut NoTextures, now);
            })
            .unwrap();
        }
    }
    let specs = runtime.windows();
    host.sync(&specs, 1.0);
}

#[test]
fn a_view_gets_a_window_of_its_own_and_closes_alone() {
    let (mut runtime, _root) = runtime("view-windows");
    runtime.shell.set_window_support(WindowSupport::Many);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, [1440.0, 900.0], 1.0);
    let now = Instant::now();
    runtime.request(open_view("one"), &mut NoTextures, now);
    runtime.request(open_view("two"), &mut NoTextures, now);
    for _ in 0..3 {
        frame(&mut runtime, &mut host, now);
    }
    let keys: Vec<String> = runtime.windows().into_iter().map(|spec| spec.key).collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert_eq!(host.windows().len(), 2);
    let two = host.surface_of(&keys[1]).unwrap();
    assert_ne!(two, MAIN_SURFACE);
    // The system's close button on the second.
    runtime.window_closed(&keys[1], &mut NoTextures, now);
    frame(&mut runtime, &mut host, now);
    assert_eq!(runtime.windows().len(), 1);
    assert!(host.info(two).is_none(), "its window closed");
    assert!(host.surface_of(&keys[0]).is_some(), "the first stays");
    // Sue goes on: the main window still draws and Sue does not quit.
    frame(&mut runtime, &mut host, now);
    assert!(!runtime.wants_exit());
    assert!(runtime.shell.hub_open());
}

#[test]
fn without_windows_of_its_own_a_view_floats_in_the_main_window() {
    let (mut runtime, _root) = runtime("view-floats");
    // A runtime without a desktop host has one window.
    assert_eq!(runtime.shell.window_support(), WindowSupport::One);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, [1440.0, 900.0], 1.0);
    let now = Instant::now();
    runtime.request(open_view("one"), &mut NoTextures, now);
    for _ in 0..3 {
        frame(&mut runtime, &mut host, now);
    }
    assert!(runtime.windows().is_empty());
    assert!(host.windows().is_empty());
    let key = sue_wm::view_key(&AppletId::new(crate::gallery::ID).unwrap(), "one");
    let (rect, _) = runtime.shell.view_rects(&key).unwrap();
    assert!(runtime.shell.page_rect().contains_rect(rect));
}

#[test]
fn the_revision_follows_documents_and_settings() {
    let (mut runtime, _root) = runtime("view-revision");
    let before = runtime.revision();
    runtime
        .engine
        .world
        .rice
        .set("display.phone_frame", sue_rice::RiceValue::Bool(true))
        .unwrap();
    let after_rice = runtime.revision();
    assert_ne!(after_rice, before, "a setting changed what windows show");
    runtime
        .engine
        .world
        .arena
        .spawn("note.md", sue_core::StringBuffer::from("a note"));
    assert_ne!(runtime.revision(), after_rice, "so did a document");
}

/// Draws the main window with a floating view over Sue Paint, and a
/// view's window of its own with Sue's frame, into PNG files in
/// `SUE_SHOT_DIR` (or the temporary folder), to look at.
#[test]
#[ignore = "writes pictures to look at; needs a GPU"]
fn pictures_of_a_view_floating_and_in_its_own_window() {
    use sue_platform::windows::{keep, window_pass};
    use sue_platform::{EguiPainter, GpuContext};
    let dir = std::env::var_os("SUE_SHOT_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    let gpu = GpuContext::headless().unwrap();
    let mut painter = EguiPainter::new(&gpu, sue_platform::wgpu::TextureFormat::Rgba8UnormSrgb);
    let (mut runtime, _root) = runtime("view-pictures");
    runtime.shell.set_own_frame(true);
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    let start = Instant::now();
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, start);
    runtime.request(
        ShellRequest::OpenView {
            applet: paint,
            view: "canvas".to_owned(),
            title: "Illustration, second view".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let input = |size: [f32; 2], step: u64| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            vec2(size[0], size[1]),
        )),
        time: Some(step as f64 * 0.008),
        max_texture_side: Some(painter_side()),
        ..egui::RawInput::default()
    };
    let mut step = 0;
    let main = |runtime: &mut Runtime, painter: &mut EguiPainter, step: &mut u64| {
        *step += 1;
        let now = start + Duration::from_millis(8 * *step);
        let specs = runtime.windows();
        let mut output = ctx.run_ui(input([1440.0, 900.0], *step), |ui| {
            runtime.ui(ui, painter, now);
            keep(ui.ctx(), &specs);
        });
        let textures = std::mem::take(&mut output.textures_delta);
        (output, textures)
    };
    let write = |name: &str, size: [u32; 2], pixels: Vec<u8>| {
        let png = sue_io::encode_png_rgba(size[0], size[1], &pixels).unwrap();
        std::fs::write(dir.join(name), png).unwrap();
    };
    // Floating, where the system gives one window.
    for _ in 0..150 {
        let (_, textures) = main(&mut runtime, &mut painter, &mut step);
        painter.apply_textures(textures);
    }
    let (output, textures) = main(&mut runtime, &mut painter, &mut step);
    let clear = runtime.theme().void;
    let pixels = painter
        .paint_offscreen(&ctx, [1440, 900], clear, output.shapes, textures, 1.0)
        .unwrap();
    write("p3-20-floating.png", [1440, 900], pixels);
    // The same on a tablet's layout: a finger-sized strip and close.
    runtime
        .engine
        .world
        .rice
        .set(
            sue_wm::INTERFACE_KEY,
            sue_rice::RiceValue::Text("tablet".to_owned()),
        )
        .unwrap();
    for _ in 0..60 {
        let (_, textures) = main(&mut runtime, &mut painter, &mut step);
        painter.apply_textures(textures);
    }
    let (output, textures) = main(&mut runtime, &mut painter, &mut step);
    let pixels = painter
        .paint_offscreen(&ctx, [1440, 900], clear, output.shapes, textures, 1.0)
        .unwrap();
    write("p3-20-floating-tablet.png", [1440, 900], pixels);
    runtime
        .engine
        .world
        .rice
        .set(
            sue_wm::INTERFACE_KEY,
            sue_rice::RiceValue::Text("desktop".to_owned()),
        )
        .unwrap();
    // In a window of its own, with Sue's frame.
    runtime.shell.set_window_support(WindowSupport::Many);
    let key = runtime.windows()[0].key.clone();
    let size = runtime.windows()[0].size;
    let mut last = None;
    for _ in 0..40 {
        let (_, textures) = main(&mut runtime, &mut painter, &mut step);
        painter.apply_textures(textures);
        let now = start + Duration::from_millis(8 * step);
        let mut output = window_pass(&ctx, &key, input(size, step), |ui| {
            runtime.window_ui(&key, ui, &mut painter, now);
        });
        let textures = std::mem::take(&mut output.textures_delta);
        if let Some((_, old)) = last.replace((output, textures)) {
            painter.apply_textures(old);
        }
    }
    let (output, textures) = last.unwrap();
    let pixels = [size[0] as u32, size[1] as u32];
    let drawn = painter
        .paint_offscreen(&ctx, pixels, clear, output.shapes, textures, 1.0)
        .unwrap();
    write("p3-20-own-window.png", pixels, drawn);
}

fn painter_side() -> usize {
    8192
}

#[test]
fn an_open_views_window_sleeps_while_nothing_changes() {
    let (mut runtime, _root) = runtime("view-sleeps");
    runtime.shell.set_window_support(WindowSupport::Many);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, [1440.0, 900.0], 1.0);
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let start = Instant::now();
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, start);
    runtime.request(
        ShellRequest::OpenView {
            applet: paint,
            view: "canvas".to_owned(),
            title: "Canvas".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    // Let Sue settle: its first frames load, lay out and animate.
    let mut now = start;
    for _ in 0..120 {
        now += Duration::from_millis(16);
        frame(&mut runtime, &mut host, now);
    }
    // Nothing happens: the main window's frames change nothing a view
    // shows, so the view's window is never woken by them.
    let revision = runtime.revision();
    for _ in 0..10 {
        now += Duration::from_millis(16);
        let specs = runtime.windows();
        host.main_frame(revision, &specs, |ui| {
            runtime.ui(ui, &mut NoTextures, now);
        });
        assert_eq!(
            runtime.revision(),
            revision,
            "an idle frame changed nothing"
        );
    }
    let key = runtime.windows()[0].key.clone();
    let surface = host.surface_of(&key).unwrap();
    assert!(
        !host.due(runtime.revision()).contains(&surface),
        "the view's window sleeps"
    );
}
