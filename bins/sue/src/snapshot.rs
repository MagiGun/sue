//! Offscreen snapshots: boots the real shell headlessly against a fresh,
//! temporary profile (so the defaults are what you see), drives it through
//! scripted scenes with real egui frames (springs glide to rest at a fixed
//! frame time), and paints each scene with the GPU into a PNG.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Pos2, Rect, vec2};
use sue_core::{PCMFloat1D, PixelFormat, RasterTile2D};
use sue_io::{JxlPipeline, NativeJxlPipeline, encode_png, encode_png_rgba, encode_wav};
use sue_platform::time::Instant;
use sue_platform::{
    EguiPainter, FormFactor, FramePacing, GpuContext, HardwareCapabilities, HeadlessHost, OsFamily,
    PlatformPaths, wgpu,
};
use sue_rice::DEFAULT_RICE;
use sue_wm::{AppletId, ShellRequest};

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

/// Where the Hub drew `applet`'s box and its pin in the last frame.
fn hub_box(runtime: &Runtime, applet: &str) -> Option<(Rect, Rect)> {
    runtime
        .shell
        .hub_boxes()
        .iter()
        .find(|(id, _, _)| id.as_str() == applet)
        .map(|(_, rect, pin)| (*rect, *pin))
}

/// Renders the shell's main scenes into `dir` as PNG files and returns
/// their paths.
///
/// # Errors
/// GPU, boot or file-system failures.
#[allow(clippy::too_many_lines, reason = "one scripted tour of Sue")]
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
    // It remembers work in the throwaway profile, for the Hub's lists.
    let mut runtime = Runtime::new(Engine::boot_with(&host, options)?)?.remembering(true);
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
    let id = |text: &str| AppletId::new(text).map_err(|error| error.to_string());
    let paint = id(sue_paint::ID)?;
    let writer = id(sue_writer::ID)?;
    let convert = id(crate::convert::ID)?;

    // 00. The Hub Sue opens on; 01. Sue Paint pinned with a click on its
    // pin; 02. the Sue menu under the Sue button.
    shoot(&mut runtime, &mut camera, "00-hub")?;
    if let Some((_, pin)) = hub_box(&runtime, sue_paint::ID) {
        camera.click(pin.center());
    }
    shoot(&mut runtime, &mut camera, "01-hub-pinned")?;
    runtime.request(ShellRequest::Pin(None), &mut camera.painter, camera.now);
    camera.click(runtime.shell.sue_button().center());
    shoot(&mut runtime, &mut camera, "02-sue-menu")?;
    runtime.shell.dismiss_overlays();

    // 03. The command palette.
    runtime.shell.open_palette();
    runtime.shell.palette_query("open");
    shoot(&mut runtime, &mut camera, "03-command-palette")?;
    runtime.shell.dismiss_overlays();

    // 04 and 05. Settings, on two categories.
    for (page, name) in [
        ("appearance", "04-settings-appearance"),
        ("general", "05-settings-general"),
    ] {
        runtime.request(
            ShellRequest::OpenSettings(Some(page.to_owned())),
            &mut camera.painter,
            camera.now,
        );
        shoot(&mut runtime, &mut camera, name)?;
        if page == "appearance" {
            // 04b. The color wheel behind the Void swatch.
            camera.click(Pos2::new(1176.0, 318.0));
            shoot(&mut runtime, &mut camera, "04b-color-wheel")?;
            camera.click(Pos2::new(700.0, 860.0));
            camera.settle(&mut runtime);
        }
    }
    runtime.shell.close_window();
    runtime.shell.dismiss_overlays();

    // 06. The Image Viewer, a tool: Open Picture asks the system's dialog,
    // answered here with a JPEG XL picture.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: id(crate::viewer::ID)?,
            command: "viewer.open".to_owned(),
        },
        &mut camera.painter,
        camera.now,
    );
    if !runtime.answer_pick(&home.join("test-card.jxl"), &mut camera.painter, camera.now) {
        return Err("Open Picture did not ask for a file".to_owned());
    }
    shoot(&mut runtime, &mut camera, "06-image-viewer")?;

    // 07. The Converter with a picture, a song and a text file.
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
    runtime.request(ShellRequest::Open(convert), &mut camera.painter, camera.now);
    shoot(&mut runtime, &mut camera, "07-converter")?;

    // 08a. Sue Paint opens on its start page while no canvas is open: the
    // projects, the materials and the three popups.
    runtime.request(
        ShellRequest::Open(paint.clone()),
        &mut camera.painter,
        camera.now,
    );
    for (target, name) in [
        ("start/projects", "08a-paint-start-projects"),
        ("start/materials", "08b-paint-start-materials"),
        ("start/transfers", "08c-paint-start-transfers"),
        ("start/help", "08d-paint-start-help"),
        ("start/settings", "08e-paint-start-settings"),
        ("start/maintenance", "08f-paint-start-maintenance"),
    ] {
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("start/close");
            applet.navigate(target);
        }
        shoot(&mut runtime, &mut camera, name)?;
    }
    if let Some(applet) = runtime.applets.get_mut(&paint) {
        applet.navigate("start/close");
        applet.navigate("start/projects");
    }

    // 08. Sue Paint with a sample illustration, 09. the Sue menu over it,
    // 10. its Layer menu and 11. its dialogs.
    runtime.with_context(&mut camera.painter, camera.now, |cx| {
        sue_paint::sample_picture(cx, 1600, 1200)
    });
    shoot(&mut runtime, &mut camera, "08-sue-paint")?;
    // 08g. The Sub Tool list and the Brush Size grid, as tabs of the tool
    // palette; the layer's History.
    for (target, name) in [
        ("tab/tool/0", "08g-sue-paint-sub-tools"),
        ("tab/tool/2", "08h-sue-paint-brush-size"),
        ("tab/layer/2", "08i-sue-paint-history"),
    ] {
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate(target);
        }
        shoot(&mut runtime, &mut camera, name)?;
    }
    if let Some(applet) = runtime.applets.get_mut(&paint) {
        applet.navigate("tab/tool/1");
        applet.navigate("tab/layer/0");
    }
    runtime.request(ShellRequest::SueMenu, &mut camera.painter, camera.now);
    shoot(&mut runtime, &mut camera, "09-sue-menu-in-paint")?;
    runtime.shell.dismiss_overlays();
    camera.settle(&mut runtime);
    for (target, name) in [
        ("menu/Layer", "10-sue-paint-menu"),
        ("dialog/new", "11-sue-paint-new"),
        ("dialog/pressure", "11b-sue-paint-pressure"),
    ] {
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("dialog/close");
            applet.navigate(target);
        }
        shoot(&mut runtime, &mut camera, name)?;
    }
    // 11c. The Page Manager of a manga of five pages.
    for command in [
        "story.add",
        "story.add",
        "story.add",
        "story.add",
        "story.first",
    ] {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut camera.painter,
            camera.now,
        );
    }
    if let Some(applet) = runtime.applets.get_mut(&paint) {
        applet.navigate("dialog/close");
        applet.navigate("dialog/pages");
    }
    shoot(&mut runtime, &mut camera, "11c-sue-paint-pages")?;
    if let Some(applet) = runtime.applets.get_mut(&paint) {
        applet.navigate("dialog/close");
    }
    // 11d. The Timeline of a page: two cels, onion skin on.
    for command in [
        "anim.cel.new",
        "anim.next",
        "anim.next",
        "anim.next",
        "anim.cel.new",
        "anim.layer.folder",
        "anim.onion",
    ] {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut camera.painter,
            camera.now,
        );
    }
    shoot(&mut runtime, &mut camera, "11d-sue-paint-timeline")?;

    // 12 and 13. Back to the Hub, then into Sue Paint again: the page
    // comes in over the Hub and its palettes slide in.
    runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
    camera.settle(&mut runtime);
    runtime.request(
        ShellRequest::Open(paint.clone()),
        &mut camera.painter,
        camera.now,
    );
    // Kept apart until the last shot, which borrows the list.
    let mut entering = Vec::new();
    for (frames, name) in [(6, "12-entering-a"), (14, "13-entering-b")] {
        let path = dir.join(format!("{name}.png"));
        camera.shoot_after(&mut runtime, &path, frames)?;
        entering.push(path);
    }
    camera.settle(&mut runtime);

    // 14 to 19. Sue Writer on a sample novel: pages, the corkboard, the
    // outliner, composition mode, the compile dialog and the Format menu.
    let novel = runtime
        .with_context(&mut camera.painter, camera.now, sue_writer::sample_project)
        .ok_or("the sample novel was not made")?;
    runtime.request(
        ShellRequest::OpenIn {
            entity: novel,
            applet: writer.clone(),
        },
        &mut camera.painter,
        camera.now,
    );
    // Let the fonts the novel uses load, then settle.
    for _ in 0..3 {
        camera.settle(&mut runtime);
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    shoot(&mut runtime, &mut camera, "14-sue-writer")?;
    for (target, name) in [
        ("view/corkboard", "15-sue-writer-corkboard"),
        ("view/outliner", "16-sue-writer-outliner"),
        ("view/composition", "17-sue-writer-composition"),
        ("dialog/compile", "18-sue-writer-compile"),
        ("menu/Format", "19-sue-writer-format-menu"),
    ] {
        if let Some(applet) = runtime.applets.get_mut(&writer) {
            applet.navigate("dialog/close");
            applet.navigate("view/document");
            applet.navigate(target);
        }
        shoot(&mut runtime, &mut camera, name)?;
    }
    // 19-start. The start page: templates as pages, and the Presets list.
    if let Some(applet) = runtime.applets.get_mut(&writer) {
        applet.navigate("dialog/close");
        applet.navigate("view/document");
        applet.navigate("start");
    }
    shoot(&mut runtime, &mut camera, "19-sue-writer-start")?;
    runtime.request(
        ShellRequest::OpenIn {
            entity: novel,
            applet: writer.clone(),
        },
        &mut camera.painter,
        camera.now,
    );
    // 19a. The comments beside the text (Review, Comments), shown and put
    // away again.
    for shown in [true, false] {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: writer.clone(),
                command: "review.comments".to_owned(),
            },
            &mut camera.painter,
            camera.now,
        );
        if shown {
            shoot(&mut runtime, &mut camera, "19a-sue-writer-comments")?;
        }
    }

    // 19b and 19c. The Hub's lists: the novel saved as a project, the
    // picture opened earlier, and a picture another run kept unsaved; then
    // the same Hub in Paper.
    let project = home.join("Novel.sue");
    runtime.save_project_to(&project)?;
    let bytes = std::fs::read(&project).map_err(|error| error.to_string())?;
    let mut other = sue_wm::WorkList::open(runtime.engine.paths.data_dir.join("work"));
    other
        .keep(
            &bytes,
            None,
            "Illustration",
            Some(paint.clone()),
            sue_wm::unix_now().saturating_sub(300),
        )
        .map_err(|error| error.to_string())?;
    runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
    shoot(&mut runtime, &mut camera, "19b-hub-lists")?;
    let paper = sue_rice::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == "paper")
        .ok_or("no Paper preset")?;
    let txn = paper.transaction(&runtime.engine.world.rice);
    runtime
        .engine
        .history
        .apply(&mut runtime.engine.world, sue_wm::RiceEdit(txn))
        .map_err(|error| error.to_string())?;
    shoot(&mut runtime, &mut camera, "19c-hub-lists-paper")?;
    runtime
        .engine
        .history
        .undo(&mut runtime.engine.world)
        .map_err(|error| error.to_string())?;

    // 19d. Sue in Japanese, chosen from the Sue menu's Language submenu;
    // 19e. Sue Paint set apart in Spanish while Sue stays Japanese.
    runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
    camera.click(runtime.shell.sue_button().center());
    let now = camera.now;
    let japanese = sue_lang::name("ja");
    runtime.with_shell(&mut camera.painter, now, |shell, io, now| {
        shell.sue_menu_pick(&["Language", &japanese], io, now)
    });
    camera.settle(&mut runtime);
    camera.click(runtime.shell.sue_button().center());
    shoot(&mut runtime, &mut camera, "19d-sue-menu-japanese")?;
    runtime.shell.dismiss_overlays();
    let _ = runtime.engine.world.rice.set(
        "sue_paint.language",
        sue_rice::RiceValue::Text("es".to_owned()),
    );
    runtime.request(
        ShellRequest::Open(paint.clone()),
        &mut camera.painter,
        camera.now,
    );
    camera.click(runtime.shell.sue_button().center());
    shoot(
        &mut runtime,
        &mut camera,
        "19e-sue-paint-spanish-in-japanese-sue",
    )?;
    runtime.shell.dismiss_overlays();
    for (key, value) in [
        (sue_wm::LANGUAGE_KEY, sue_lang::SYSTEM),
        ("sue_paint.language", sue_lang::FOLLOW),
    ] {
        let _ = runtime
            .engine
            .world
            .rice
            .set(key, sue_rice::RiceValue::Text(value.to_owned()));
    }

    // 19f to 19k. The Sue menu's Layout in a computer's window: the phone
    // layout filling it and phone shaped, the tablet layout, and back.
    let layout = |runtime: &mut Runtime, camera: &mut Camera2, path: &[&str]| {
        runtime.request(ShellRequest::SueMenu, &mut camera.painter, camera.now);
        runtime.with_shell(&mut camera.painter, camera.now, |shell, io, now| {
            shell.sue_menu_pick(path, io, now)
        })
    };
    runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
    camera.click(runtime.shell.sue_button().center());
    shoot(&mut runtime, &mut camera, "19f-sue-menu-layout")?;
    runtime.shell.dismiss_overlays();
    layout(&mut runtime, &mut camera, &["Layout", "Phone"]);
    shoot(&mut runtime, &mut camera, "19g-desktop-as-phone-hub")?;
    runtime.request(
        ShellRequest::Open(paint.clone()),
        &mut camera.painter,
        camera.now,
    );
    shoot(&mut runtime, &mut camera, "19h-desktop-as-phone-sue-paint")?;
    layout(&mut runtime, &mut camera, &["Layout", "Phone Shaped"]);
    shoot(
        &mut runtime,
        &mut camera,
        "19i-desktop-phone-shaped-sue-paint",
    )?;
    layout(&mut runtime, &mut camera, &["Layout", "Tablet"]);
    shoot(&mut runtime, &mut camera, "19j-desktop-as-tablet-sue-paint")?;
    layout(
        &mut runtime,
        &mut camera,
        &["Layout", "Automatic (Desktop)"],
    );
    runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
    shoot(&mut runtime, &mut camera, "19k-desktop-again-hub")?;
    let _ = runtime
        .engine
        .world
        .rice
        .set(sue_wm::PHONE_FRAME_KEY, sue_rice::RiceValue::Bool(false));

    // 20 to 23. The touch layouts: the Hub, Sue Paint and Sue Writer on a
    // phone and on a tablet.
    for (interface, size, prefix) in [
        ("phone", [430_u32, 932_u32], "20-phone"),
        ("tablet", [1180, 820], "22-tablet"),
    ] {
        let _ = runtime.engine.world.rice.set(
            "display.interface",
            sue_rice::RiceValue::Text(interface.to_owned()),
        );
        camera.size = size;
        runtime.request(ShellRequest::Hub, &mut camera.painter, camera.now);
        shoot(&mut runtime, &mut camera, &format!("{prefix}-hub"))?;
        runtime.request(
            ShellRequest::Open(paint.clone()),
            &mut camera.painter,
            camera.now,
        );
        shoot(&mut runtime, &mut camera, &format!("{prefix}-sue-paint"))?;
        // Sue Paint's start page on a touch screen: Projects, the New
        // canvas page (Illustration and Comic), the materials, and
        // Settings and Help as sheets.
        for (targets, name) in [
            (&["start/show"][..], "paint-start"),
            (&["start/new"], "paint-new-canvas"),
            (&["start/new/2"], "paint-new-comic"),
            (
                &["start/hide", "start/show", "start/materials"],
                "paint-materials",
            ),
            (&["start/projects", "start/settings"], "paint-settings"),
            (&["start/close", "start/help"], "paint-help"),
        ] {
            if let Some(applet) = runtime.applets.get_mut(&paint) {
                for target in targets {
                    applet.navigate(target);
                }
            }
            shoot(&mut runtime, &mut camera, &format!("{prefix}-{name}"))?;
        }
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("start/hide");
        }
        // Simple Mode: the canvas, then its "..." menu.
        shoot(&mut runtime, &mut camera, &format!("{prefix}-paint-simple"))?;
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("sheet/more");
        }
        shoot(
            &mut runtime,
            &mut camera,
            &format!("{prefix}-paint-simple-more"),
        )?;
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("sheet/close");
        }
        // Studio Mode: the canvas, its modifier key bar and its Menu.
        let _ = runtime.engine.world.rice.set(
            sue_paint::simple::MODE_KEY,
            sue_rice::RiceValue::Text("studio".to_owned()),
        );
        shoot(&mut runtime, &mut camera, &format!("{prefix}-paint-studio"))?;
        for (target, name) in [
            ("keys/open", "paint-studio-keys"),
            ("sheet/menu", "paint-studio-menu"),
        ] {
            if let Some(applet) = runtime.applets.get_mut(&paint) {
                applet.navigate("sheet/close");
                applet.navigate(target);
            }
            shoot(&mut runtime, &mut camera, &format!("{prefix}-{name}"))?;
        }
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("sheet/close");
        }
        let _ = runtime.engine.world.rice.set(
            sue_paint::simple::MODE_KEY,
            sue_rice::RiceValue::Text("simple".to_owned()),
        );
        runtime.request(
            ShellRequest::OpenIn {
                entity: novel,
                applet: writer.clone(),
            },
            &mut camera.painter,
            camera.now,
        );
        shoot(&mut runtime, &mut camera, &format!("{prefix}-sue-writer"))?;
        // The start page, and the page upright and on its side.
        if let Some(applet) = runtime.applets.get_mut(&writer) {
            applet.navigate("start");
        }
        shoot(
            &mut runtime,
            &mut camera,
            &format!("{prefix}-sue-writer-start"),
        )?;
        runtime.request(
            ShellRequest::OpenIn {
                entity: novel,
                applet: writer.clone(),
            },
            &mut camera.painter,
            camera.now,
        );
        let upright = camera.size;
        camera.size = [upright[1], upright[0]];
        shoot(
            &mut runtime,
            &mut camera,
            &format!("{prefix}-sue-writer-turned"),
        )?;
        camera.size = upright;
        if interface == "phone" {
            // Writing with the on-screen keyboard up: the window above it
            // keeps the caret's line and the formatting in sight.
            camera.size = [430, 520];
            if let Some(applet) = runtime.applets.get_mut(&writer) {
                applet.navigate("text/end");
            }
            shoot(
                &mut runtime,
                &mut camera,
                &format!("{prefix}-sue-writer-typing"),
            )?;
        }
    }
    // 24 to 26. Settings shaped to each screen: a phone's list and page,
    // upright and on its side; a tablet's two panes, upright and on its
    // side; a small desktop window.
    for (interface, size, page, name) in [
        ("phone", [393_u32, 851_u32], "", "24-phone-settings"),
        ("phone", [393, 851], "display", "24a-phone-settings-display"),
        (
            "phone",
            [851, 393],
            "appearance",
            "24b-phone-on-its-side-settings",
        ),
        (
            "tablet",
            [800, 1280],
            "appearance",
            "25-tablet-upright-settings",
        ),
        (
            "tablet",
            [1280, 800],
            "display",
            "25a-tablet-on-its-side-settings",
        ),
        ("desktop", [720, 520], "display", "26-small-window-settings"),
    ] {
        let _ = runtime.engine.world.rice.set(
            sue_wm::INTERFACE_KEY,
            sue_rice::RiceValue::Text(interface.to_owned()),
        );
        camera.size = size;
        runtime.request(
            ShellRequest::OpenSettings(Some(page.to_owned())),
            &mut camera.painter,
            camera.now,
        );
        shoot(&mut runtime, &mut camera, name)?;
    }
    runtime.shell.close_window();
    let _ = runtime.engine.world.rice.set(
        sue_wm::INTERFACE_KEY,
        sue_rice::RiceValue::Text("auto".to_owned()),
    );
    written.extend(entering);
    let _ = std::fs::remove_dir_all(&root);
    Ok(written)
}
