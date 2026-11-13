//! Offscreen snapshots: boots the real shell headlessly against a fresh,
//! temporary profile (so the defaults are what you see), drives it through
//! scripted scenes with real egui frames (springs glide to rest at a fixed
//! frame time), and paints each scene with the GPU into a PNG.
//!
//! The scenes are one tour: each starts where the one before it left Sue.
//! They live in `scenes/`, one file per area, and `scenes::run` holds
//! their order.

mod scenes;

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Pos2, Rect, vec2};
use sue_core::{EntityId, PCMFloat1D, PixelFormat, RasterTile2D};
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

/// The tour through Sue that the scenes share: the running shell, the
/// camera, and what earlier scenes made for later ones.
struct Tour<'a> {
    runtime: Runtime,
    camera: Camera2,
    /// Where the pictures go.
    dir: &'a Path,
    /// The pictures written so far.
    written: Vec<PathBuf>,
    /// The two pictures taken in the middle of entering a studio. They
    /// are listed after all the others.
    entering: Vec<PathBuf>,
    /// The profile's home folder, where the sample files are.
    home: PathBuf,
    paint: AppletId,
    writer: AppletId,
    convert: AppletId,
    /// The sample novel, once Sue Writer's first scene has made it.
    novel: Option<EntityId>,
}

impl Tour<'_> {
    /// Lets the shell come to rest, then writes the picture `name`.
    fn shoot(&mut self, name: &str) -> Result<(), String> {
        let path = self.dir.join(format!("{name}.png"));
        self.camera.shoot(&mut self.runtime, &path)?;
        self.written.push(path);
        Ok(())
    }

    /// Runs `frames` frames and writes the picture `name` from the next
    /// one: a moment in the middle of an animation.
    fn shoot_entering(&mut self, name: &str, frames: usize) -> Result<(), String> {
        let path = self.dir.join(format!("{name}.png"));
        self.camera.shoot_after(&mut self.runtime, &path, frames)?;
        self.entering.push(path);
        Ok(())
    }

    /// Runs frames until the shell sleeps.
    fn settle(&mut self) {
        self.camera.settle(&mut self.runtime);
    }

    /// Asks the shell for `request` and serves it at once.
    fn request(&mut self, request: ShellRequest) {
        self.runtime
            .request(request, &mut self.camera.painter, self.camera.now);
    }

    /// Runs `command` of `applet`.
    fn command(&mut self, applet: &AppletId, command: &str) {
        self.request(ShellRequest::AppletCommand {
            applet: applet.clone(),
            command: command.to_owned(),
        });
    }

    /// Runs a command of Sue Paint.
    fn paint_command(&mut self, command: &str) {
        let paint = self.paint.clone();
        self.command(&paint, command);
    }

    /// Sends Sue Paint to each of `targets` in turn (a tab, a dialog, a
    /// page of its start page).
    fn paint_go(&mut self, targets: &[&str]) {
        if let Some(applet) = self.runtime.applets.get_mut(&self.paint) {
            for target in targets {
                applet.navigate(target);
            }
        }
    }

    /// Sends Sue Writer to each of `targets` in turn.
    fn writer_go(&mut self, targets: &[&str]) {
        if let Some(applet) = self.runtime.applets.get_mut(&self.writer) {
            for target in targets {
                applet.navigate(target);
            }
        }
    }

    /// Sets a setting directly, outside the history.
    fn set(&mut self, key: &str, value: sue_rice::RiceValue) {
        let _ = self.runtime.engine.world.rice.set(key, value);
    }

    /// Opens the Sue menu and picks the entry at `path` in it.
    fn sue_menu_pick(&mut self, path: &[&str]) {
        self.runtime.with_shell(
            &mut self.camera.painter,
            self.camera.now,
            |shell, io, now| shell.sue_menu_pick(path, io, now),
        );
    }

    /// Gives a page that reads from the network a few seconds to load.
    fn wait_for_the_network(&mut self) {
        for _ in 0..60 {
            let output = self.camera.frame(&mut self.runtime);
            self.camera.painter.apply_textures(output.textures_delta);
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Opens the sample novel in Sue Writer.
    fn open_novel(&mut self) -> Result<(), String> {
        let entity = self.novel.ok_or("the sample novel was not made")?;
        self.request(ShellRequest::OpenIn {
            entity,
            applet: self.writer.clone(),
        });
        Ok(())
    }
}

/// Whether the scenes that read from the network are wanted.
fn online() -> bool {
    std::env::var_os("SUE_SNAPSHOT_ONLINE").is_some()
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
    // It remembers work in the throwaway profile, for the Hub's lists.
    let runtime = Runtime::new(Engine::boot_with(&host, options)?)?.remembering(true);
    let start = Instant::now();
    let camera = Camera2 {
        ctx: egui::Context::default(),
        painter: EguiPainter::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb),
        size: [width, height],
        start,
        now: start,
        input: Vec::new(),
    };
    let id = |text: &str| AppletId::new(text).map_err(|error| error.to_string());
    let mut tour = Tour {
        runtime,
        camera,
        dir,
        written: Vec::new(),
        entering: Vec::new(),
        home: paths.home_dir,
        paint: id(sue_paint::ID)?,
        writer: id(sue_writer::ID)?,
        convert: id(crate::convert::ID)?,
        novel: None,
    };
    scenes::run(&mut tour)?;
    let Tour {
        mut written,
        entering,
        ..
    } = tour;
    written.extend(entering);
    let _ = std::fs::remove_dir_all(&root);
    Ok(written)
}
