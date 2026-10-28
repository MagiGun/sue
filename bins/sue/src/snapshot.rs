//! Offscreen snapshots: boots the real shell headlessly against a fresh,
//! temporary profile (so the defaults are what you see), drives it through
//! scripted scenes with real egui frames (springs glide to rest at a fixed
//! frame time), and paints each scene with the GPU into a PNG.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, vec2};
use sue_core::{PCMFloat1D, PixelFormat, RasterTile2D};
use sue_io::{JxlPipeline, NativeJxlPipeline, encode_png, encode_png_rgba, encode_wav};
use sue_platform::{
    EguiPainter, FormFactor, FramePacing, GpuContext, HardwareCapabilities, HeadlessHost, OsFamily,
    PlatformPaths, wgpu,
};
use sue_rice::DEFAULT_RICE;
use sue_wm::{AppletId, NodeId, ShellRequest};

use crate::engine::{BootOptions, Engine};
use crate::runtime::Runtime;

/// A 256x160 test card: gray ramps, a checker and a soft border.
pub fn test_card() -> RasterTile2D {
    let (width, height) = (256_u32, 160_u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = if y < 80 {
                x as u8
            } else if y < 120 {
                if (x / 16 + y / 16) % 2 == 0 { 230 } else { 40 }
            } else {
                ((x / 32) * 36) as u8
            };
            let edge = x < 4 || y < 4 || x >= width - 4 || y >= height - 4;
            pixels.extend([value, value, value, if edge { 96 } else { 255 }]);
        }
    }
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).expect("test card size")
}

/// Deterministic high-entropy bytes behind a ZIP signature.
fn sample_binary() -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut bytes = b"PK\x03\x04\x14\0\0\0\x08\0".to_vec();
    bytes.extend((0..6000).map(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state.to_le_bytes()[7]
    }));
    bytes
}

/// Two seconds of a soft A major chord.
fn sample_tone() -> PCMFloat1D {
    let rate = 44_100_u32;
    let samples = (0..rate * 2)
        .flat_map(|frame| {
            let t = frame as f32 / rate as f32;
            let chord = [440.0_f32, 554.37, 659.25]
                .iter()
                .map(|hz| (t * hz * std::f32::consts::TAU).sin())
                .sum::<f32>()
                * 0.15;
            [chord, chord * 0.9]
        })
        .collect();
    PCMFloat1D::new(rate, 2, samples).expect("tone size")
}

/// Writes the sample files the scenes open into `home`.
fn write_samples(home: &Path) -> Result<(), String> {
    std::fs::create_dir_all(home.join("Documents")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(home.join("Pictures")).map_err(|error| error.to_string())?;
    let card = test_card();
    let jxl = NativeJxlPipeline
        .encode_layers(&[&card], 3)
        .map_err(|error| error.to_string())?;
    let files: [(&str, Vec<u8>); 6] = [
        ("test-card.jxl", jxl),
        ("photo.png", encode_png(&card)?),
        ("chord.wav", encode_wav(&sample_tone())),
        ("archive.zip", sample_binary()),
        ("default.suerice", DEFAULT_RICE.as_bytes().to_vec()),
        (
            "README.md",
            b"# Sue\n\nA jack of all trades is a master of none,\nbut oftentimes better than a master of one.\n"
                .to_vec(),
        ),
    ];
    for (name, bytes) in files {
        std::fs::write(home.join(name), bytes).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Drives the runtime with egui frames and paints the last one.
struct Camera2 {
    ctx: egui::Context,
    painter: EguiPainter,
    size: [u32; 2],
    start: Instant,
    now: Instant,
    /// Input for the coming frames, one list per frame.
    input: Vec<Vec<egui::Event>>,
}

impl Camera2 {
    fn frame(&mut self, runtime: &mut Runtime) -> egui::FullOutput {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                vec2(self.size[0] as f32, self.size[1] as f32),
            )),
            time: Some(self.now.duration_since(self.start).as_secs_f64()),
            max_texture_side: Some(self.painter.max_texture_side()),
            events: if self.input.is_empty() {
                Vec::new()
            } else {
                self.input.remove(0)
            },
            ..egui::RawInput::default()
        };
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut self.painter, now));
        self.now += Duration::from_millis(8);
        output
    }

    /// Clicks at `at` over the next frames.
    fn click(&mut self, at: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.input.push(vec![egui::Event::PointerMoved(at)]);
        self.input.push(vec![button(true)]);
        self.input.push(vec![button(false)]);
    }

    /// Runs frames until the shell sleeps.
    fn settle(&mut self, runtime: &mut Runtime) {
        for count in 0.. {
            let output = self.frame(runtime);
            self.painter.apply_textures(output.textures_delta);
            let asleep = !matches!(runtime.pacing(self.now), FramePacing::Animate { .. });
            if (asleep && count > 3 && self.input.is_empty()) || count > 600 {
                break;
            }
        }
    }

    /// Runs frames until the shell sleeps, then paints one more.
    fn shoot(&mut self, runtime: &mut Runtime, path: &Path) -> Result<(), String> {
        self.settle(runtime);
        self.paint(runtime, path)
    }

    /// Runs `count` frames, then paints the next one: a moment in the
    /// middle of an animation.
    fn shoot_after(
        &mut self,
        runtime: &mut Runtime,
        path: &Path,
        count: usize,
    ) -> Result<(), String> {
        for _ in 0..count {
            let output = self.frame(runtime);
            self.painter.apply_textures(output.textures_delta);
        }
        self.paint(runtime, path)
    }

    /// Paints one frame into a PNG.
    fn paint(&mut self, runtime: &mut Runtime, path: &Path) -> Result<(), String> {
        let output = self.frame(runtime);
        let pixels = self.painter.paint_offscreen(
            &self.ctx,
            self.size,
            runtime.theme().void,
            output.shapes,
            output.textures_delta,
            output.pixels_per_point,
        )?;
        std::fs::write(path, encode_png_rgba(self.size[0], self.size[1], &pixels)?)
            .map_err(|error| error.to_string())
    }
}

fn node_of(runtime: &Runtime, applet: &str) -> Option<NodeId> {
    runtime
        .engine
        .world
        .scene
        .iter()
        .find(|(_, node)| node.applet.as_str() == applet)
        .map(|(id, _)| id)
}

/// Renders the shell's main scenes into `dir` as PNG files and returns
/// their paths.
///
/// # Errors
/// GPU, boot or file-system failures.
pub fn write_snapshots(dir: &Path, width: u32, height: u32) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let gpu = GpuContext::headless().map_err(|error| error.to_string())?;
    let root = dir.join("profile");
    let _ = std::fs::remove_dir_all(&root);
    let paths = PlatformPaths {
        config_dir: root.join("config"),
        data_dir: root.join("data"),
        cache_dir: root.join("cache"),
        home_dir: root.join("home"),
    };
    write_samples(&paths.home_dir)?;
    let host = HeadlessHost::new(
        OsFamily::current(),
        FormFactor::Desktop,
        HardwareCapabilities::probe_portable(),
        paths.clone(),
    );
    let options = BootOptions {
        audio_devices: Vec::new(),
        gpu_label: Some(gpu.adapter_label()),
        gpu_kind: Some(gpu.adapter_kind().to_owned()),
        gpus: gpu.available().to_vec(),
    };
    let mut runtime = Runtime::new(Engine::boot_with(&host, options)?)?;
    let start = Instant::now();
    let mut camera = Camera2 {
        ctx: egui::Context::default(),
        painter: EguiPainter::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb),
        size: [width, height],
        start,
        now: start,
        input: Vec::new(),
    };
    let mut written = Vec::new();
    let mut shoot =
        |runtime: &mut Runtime, camera: &mut Camera2, name: &str| -> Result<(), String> {
            let path = dir.join(format!("{name}.png"));
            camera.shoot(runtime, &path)?;
            written.push(path);
            Ok(())
        };
    let home = paths.home_dir;

    // 0. The Hub Sue opens with, then 1. the canvas behind it, with the
    // Welcome document, a tone and the top pill.
    shoot(&mut runtime, &mut camera, "00-hub")?;
    runtime.request(ShellRequest::Hub(false), &mut camera.painter, camera.now);
    runtime.request(
        ShellRequest::Create {
            applet: sue_wm::AppletId::new(crate::audio_node::ID)
                .map_err(|error| error.to_string())?,
            key: "tone".to_owned(),
        },
        &mut camera.painter,
        camera.now,
    );
    runtime.request(ShellRequest::ResetCamera, &mut camera.painter, camera.now);
    runtime.request(ShellRequest::Select(None), &mut camera.painter, camera.now);
    shoot(&mut runtime, &mut camera, "01-canvas")?;

    // 2. The selection pill above the document.
    let document = node_of(&runtime, crate::document::ID).ok_or("no document")?;
    runtime.request(
        ShellRequest::Select(Some(document)),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "02-selection-pill")?;

    // 3. Document mode: the camera frames the document, text selected.
    runtime.request(
        ShellRequest::Mode(crate::document::ID.to_owned()),
        &mut camera.painter,
        camera.now,
    );
    if let Some(document) = node_of(&runtime, crate::document::ID) {
        crate::document::select_words(&camera.ctx, document, 30, 38);
    }
    shoot(&mut runtime, &mut camera, "03-document-mode")?;

    // 4. Everything projected onto the open timeline, audio selected.
    for applet in [crate::document::ID, crate::audio_node::ID] {
        if let Some(node) = node_of(&runtime, applet) {
            let seconds = if applet == crate::audio_node::ID {
                3.0
            } else {
                4.0
            };
            runtime.with_context(&mut camera.painter, camera.now, |cx| {
                sue_wm::project_to_timeline(cx, node, seconds);
            });
        }
    }
    camera.settle(&mut runtime);
    runtime.request(ShellRequest::ResetCamera, &mut camera.painter, camera.now);
    runtime.shell.seek(1.25);
    if let Some(audio) = node_of(&runtime, crate::audio_node::ID) {
        runtime.request(
            ShellRequest::Select(Some(audio)),
            &mut camera.painter,
            camera.now,
        );
    }
    shoot(&mut runtime, &mut camera, "04-timeline")?;

    // 5. The marking menu and 6. the command palette.
    runtime.request(
        ShellRequest::Timeline(false),
        &mut camera.painter,
        camera.now,
    );
    runtime.request(ShellRequest::Select(None), &mut camera.painter, camera.now);
    let center = runtime.shell.canvas_rect().center();
    runtime.shell.open_radial(center);
    shoot(&mut runtime, &mut camera, "05-radial-menu")?;
    runtime
        .shell
        .radial_new_file(Some("Picture"), &runtime.applets);
    shoot(&mut runtime, &mut camera, "05b-new-file-wheels")?;
    runtime.shell.dismiss_overlays();
    runtime.shell.open_palette();
    runtime.shell.palette_query("new");
    shoot(&mut runtime, &mut camera, "06-command-palette")?;

    // 7 and 8. Settings, on two categories.
    for (page, name) in [
        ("appearance", "07-settings-appearance"),
        ("display", "08-settings-display"),
    ] {
        runtime.request(
            ShellRequest::OpenSettings(Some(page.to_owned())),
            &mut camera.painter,
            camera.now,
        );
        shoot(&mut runtime, &mut camera, name)?;
        if page == "appearance" {
            // 7b. The color wheel behind the Void swatch.
            camera.click(Pos2::new(1176.0, 318.0));
            shoot(&mut runtime, &mut camera, "07b-color-wheel")?;
            camera.click(Pos2::new(700.0, 860.0));
            camera.settle(&mut runtime);
        }
    }
    runtime.shell.close_window();
    runtime.shell.dismiss_overlays();

    // 9. Files and the Inspector open, with a JPEG XL picture on the canvas.
    runtime.open_path(&home.join("test-card.jxl"), &mut camera.painter, camera.now);
    for drawer in [crate::files::ID, crate::inspector::ID] {
        runtime.request(
            ShellRequest::ShowDrawer(AppletId::new(drawer).map_err(|error| error.to_string())?),
            &mut camera.painter,
            camera.now,
        );
    }
    shoot(&mut runtime, &mut camera, "09-drawers-image")?;

    // 9a. Notes, with its toolbar, next to the editable Inspector.
    runtime.request(
        ShellRequest::ShowDrawer(
            AppletId::new(crate::notes::ID).map_err(|error| error.to_string())?,
        ),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "09a-notes-inspector")?;
    runtime.request(
        ShellRequest::ShowDrawer(
            AppletId::new(crate::files::ID).map_err(|error| error.to_string())?,
        ),
        &mut camera.painter,
        camera.now,
    );

    // 9b. The large File Manager, as cards with picture thumbnails.
    let _ = runtime
        .engine
        .world
        .rice
        .set("files.view", sue_rice::RiceValue::Text("cards".to_owned()));
    runtime.request(
        ShellRequest::OpenWindow(
            AppletId::new(crate::files::ID).map_err(|error| error.to_string())?,
        ),
        &mut camera.painter,
        camera.now,
    );
    camera.settle(&mut runtime);
    for _ in 0..40 {
        // The thumbnails come from a background thread.
        runtime.timer(&mut camera.painter, camera.now);
        let output = camera.frame(&mut runtime);
        camera.painter.apply_textures(output.textures_delta);
        std::thread::sleep(Duration::from_millis(25));
    }
    shoot(&mut runtime, &mut camera, "09b-file-manager")?;
    runtime.shell.close_window();

    // 9c. The large Player, with the sample song added and playing.
    let player = AppletId::new(crate::player::ID).map_err(|error| error.to_string())?;
    runtime.request(
        ShellRequest::SendFile {
            to: player.clone(),
            purpose: "import".to_owned(),
            path: home.clone(),
        },
        &mut camera.painter,
        camera.now,
    );
    for _ in 0..80 {
        runtime.timer(&mut camera.painter, camera.now);
        camera.now += Duration::from_millis(25);
        std::thread::sleep(Duration::from_millis(10));
    }
    runtime.request(
        ShellRequest::AppletCommand {
            applet: player.clone(),
            command: "player.toggle".to_owned(),
        },
        &mut camera.painter,
        camera.now,
    );
    for _ in 0..40 {
        runtime.timer(&mut camera.painter, camera.now);
        camera.now += Duration::from_millis(25);
        std::thread::sleep(Duration::from_millis(10));
    }
    runtime.request(
        ShellRequest::OpenWindow(player.clone()),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "09c-player")?;
    runtime.shell.close_window();

    // 9d. The large Converter with a picture, a song and a text file.
    let convert = AppletId::new(crate::convert::ID).map_err(|error| error.to_string())?;
    for name in ["photo.png", "chord.wav", "README.md"] {
        runtime.request(
            ShellRequest::SendFile {
                to: convert.clone(),
                purpose: "add".to_owned(),
                path: home.join(name),
            },
            &mut camera.painter,
            camera.now,
        );
    }
    runtime.request(
        ShellRequest::OpenWindow(convert),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "09d-converter")?;
    runtime.request(
        ShellRequest::AppletCommand {
            applet: player,
            command: "player.toggle".to_owned(),
        },
        &mut camera.painter,
        camera.now,
    );
    runtime.shell.close_window();
    let _ = runtime
        .engine
        .world
        .rice
        .set("files.view", sue_rice::RiceValue::Text("list".to_owned()));

    // 10. Code / Hex on an unknown file, with the History drawer.
    runtime.open_path(&home.join("archive.zip"), &mut camera.painter, camera.now);
    runtime.request(
        ShellRequest::ShowDrawer(
            AppletId::new(crate::history::ID).map_err(|error| error.to_string())?,
        ),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "10-code-hex")?;

    // 11. Sue Paint with a sample illustration, 12. its Layer menu and
    // 13. its New dialog.
    let studio = AppletId::new(sue_paint::ID).map_err(|error| error.to_string())?;
    runtime.request(
        ShellRequest::OpenStudio(studio.clone()),
        &mut camera.painter,
        camera.now,
    );
    let sample = runtime.with_context(&mut camera.painter, camera.now, |cx| {
        sue_paint::sample_picture(cx, 1600, 1200)
    });
    shoot(&mut runtime, &mut camera, "11-sue-paint")?;
    // 11b. The same, wide: the side drawers closed, every palette docked.
    let open: Vec<AppletId> = [runtime.shell.left_drawer(), runtime.shell.right_drawer()]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    for id in &open {
        runtime.shell.toggle_drawer_applet(id, &runtime.applets);
    }
    shoot(&mut runtime, &mut camera, "11b-sue-paint-wide")?;
    for id in &open {
        runtime.shell.toggle_drawer_applet(id, &runtime.applets);
    }
    for (target, name) in [
        ("menu/Layer", "12-sue-paint-menu"),
        ("dialog/new", "13-sue-paint-new"),
    ] {
        if let Some(applet) = runtime.applets.get_mut(&studio) {
            applet.navigate("dialog/close");
            applet.navigate(target);
        }
        shoot(&mut runtime, &mut camera, name)?;
    }
    if let Some(applet) = runtime.applets.get_mut(&studio) {
        applet.navigate("dialog/close");
    }

    // 14. Back on the canvas, the same picture as a card.
    runtime.request(ShellRequest::CloseStudio, &mut camera.painter, camera.now);
    if let Some(entity) = sample.and_then(|id| runtime.engine.world.arena.handle_of(id)) {
        runtime.request(
            ShellRequest::Place {
                entity,
                focus: true,
            },
            &mut camera.painter,
            camera.now,
        );
    }
    shoot(&mut runtime, &mut camera, "14-illustration-card")?;

    // 15 and 16. Open in Sue Paint from the card: the camera zooms into it,
    // then the panels slide in.
    runtime.request(
        ShellRequest::OpenStudio(studio.clone()),
        &mut camera.painter,
        camera.now,
    );
    for (frames, name) in [(14, "15-entering-zoom"), (26, "16-entering-panels")] {
        let path = dir.join(format!("{name}.png"));
        camera.shoot_after(&mut runtime, &path, frames)?;
        written.push(path);
    }
    let _ = std::fs::remove_dir_all(&root);
    Ok(written)
}
