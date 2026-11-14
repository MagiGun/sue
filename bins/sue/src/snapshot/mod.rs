//! Offscreen snapshots: boots the real shell headlessly against a fresh,
//! temporary profile (so the defaults are what you see), drives it through
//! scripted scenes with real egui frames (springs glide to rest at a fixed
//! frame time), and paints each scene with the GPU into a PNG.
//!
//! The scenes are one tour: each starts where the one before it left Sue.
//! They live in `scenes/`, one file per area, and `scenes::run` holds
//! their order.
//!
//! A run can be scaled ([`Options::scale`], the display's pixels per
//! point) and themed ([`Options::preset`]); the pictures keep their
//! names, so `compare` can set any two runs side by side.

pub mod compare;
mod scenes;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use egui::{Pos2, Rect, vec2};
use sue_core::{EntityId, PCMFloat1D, PixelFormat, RasterTile2D, ReversibleTransaction};
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

/// The size in pixels of a window of `size` points at `scale` pixels per
/// point.
fn pixel_size(size: [u32; 2], scale: f32) -> [u32; 2] {
    size.map(|points| ((points as f32 * scale).round() as u32).max(1))
}

/// Drives the runtime with egui frames and paints the last one.
struct Camera2 {
    ctx: egui::Context,
    painter: EguiPainter,
    /// The size of the window, in points.
    size: [u32; 2],
    /// Pixels per point: the picture is `size` times this, in pixels.
    scale: f32,
    start: Instant,
    now: Instant,
    /// Input for the coming frames, one list per frame.
    input: Vec<Vec<egui::Event>>,
}

impl Camera2 {
    /// The size of the picture, in pixels.
    fn pixels(&self) -> [u32; 2] {
        pixel_size(self.size, self.scale)
    }

    fn frame(&mut self, runtime: &mut Runtime) -> egui::FullOutput {
        let mut raw = egui::RawInput {
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
        raw.viewports
            .entry(raw.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(self.scale);
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
        let size = self.pixels();
        let pixels = self.painter.paint_offscreen(
            &self.ctx,
            size,
            runtime.theme().void,
            output.shapes,
            output.textures_delta,
            output.pixels_per_point,
        )?;
        std::fs::write(path, encode_png_rgba(size[0], size[1], &pixels)?)
            .map_err(|error| error.to_string())
    }

    /// Paints one frame of `draw` alone, without the shell, into a PNG:
    /// with the context, the painter, the size and the scale the scenes
    /// are drawn with, on a ground of `clear`.
    fn paint_drawing(
        &mut self,
        path: &Path,
        clear: egui::Color32,
        mut draw: impl FnMut(&egui::Painter, Rect),
    ) -> Result<(), String> {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(self.size[0] as f32, self.size[1] as f32));
        let mut raw = egui::RawInput {
            screen_rect: Some(rect),
            time: Some(self.now.duration_since(self.start).as_secs_f64()),
            max_texture_side: Some(self.painter.max_texture_side()),
            ..egui::RawInput::default()
        };
        raw.viewports
            .entry(raw.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(self.scale);
        let output = self.ctx.run_ui(raw, |ui| draw(ui.painter(), rect));
        self.now += Duration::from_millis(8);
        let size = self.pixels();
        let pixels = self.painter.paint_offscreen(
            &self.ctx,
            size,
            clear,
            output.shapes,
            output.textures_delta,
            output.pixels_per_point,
        )?;
        std::fs::write(path, encode_png_rgba(size[0], size[1], &pixels)?)
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

    /// Lets the shell come to rest, then writes the picture `name` of
    /// what `draw` draws alone, without the shell, in the theme of the
    /// run: a sheet for review that is no part of Sue's interface.
    fn shoot_drawing(
        &mut self,
        name: &str,
        mut draw: impl FnMut(&egui::Painter, Rect, &sue_wm::Theme),
    ) -> Result<(), String> {
        self.settle();
        let theme = self.runtime.theme().clone();
        let path = self.dir.join(format!("{name}.png"));
        self.camera
            .paint_drawing(&path, theme.void, |painter, rect| {
                draw(painter, rect, &theme);
            })?;
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

/// The smallest and largest scale a run can ask for.
const SCALES: std::ops::RangeInclusive<f32> = 0.5..=4.0;

/// A file the harness puts in its profile folder, so that it never
/// removes a folder it did not make.
const PROFILE_MARK: &str = "sue-snapshot-profile";

/// How a snapshot run differs from the plain one.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Pixels per point, as a display's scale setting gives it. Scenes
    /// keep their size in points, so a picture is this many times larger
    /// in pixels. 1.0 is the plain run.
    pub scale: f32,
    /// The id of a built-in theme preset (`void`, `graphite`, `paper`
    /// and so on) applied before the first scene. `None` leaves the
    /// defaults.
    pub preset: Option<String>,
    /// Where the throwaway profile goes. `None` puts it in the output
    /// folder, as `profile`. A few scenes draw paths inside the profile,
    /// so two runs that are to be compared need the same profile folder.
    pub profile: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: 1.0,
            preset: None,
            profile: None,
        }
    }
}

impl Options {
    /// Reads `--scale <n>`, `--preset <id>` and `--profile <dir>`.
    ///
    /// # Errors
    /// An option it does not know, a missing value, a scale outside 0.5
    /// to 4, or a preset Sue does not have.
    pub fn parse(args: &[&str]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter();
        while let Some(option) = args.next() {
            if !matches!(*option, "--scale" | "--preset" | "--profile") {
                return Err(format!("--snapshot has no option {option}"));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{option} needs a value"))?;
            match *option {
                "--scale" => {
                    options.scale = value
                        .parse()
                        .ok()
                        .filter(|scale| SCALES.contains(scale))
                        .ok_or_else(|| {
                            format!("--scale takes a number from 0.5 to 4, not {value}")
                        })?;
                }
                "--preset" => {
                    preset(value)?;
                    options.preset = Some((*value).to_owned());
                }
                _ => options.profile = Some(PathBuf::from(value)),
            }
        }
        Ok(options)
    }
}

/// The built-in theme preset with this id.
fn preset(id: &str) -> Result<sue_rice::ThemePreset, String> {
    let presets = sue_rice::builtin_presets();
    let known = presets
        .iter()
        .map(|preset| preset.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    presets
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| format!("there is no theme preset {id}; Sue has {known}"))
}

/// Runs `sue --snapshot <dir>` with the options after the folder.
#[must_use]
pub fn cli(dir: &str, options: &[&str]) -> ExitCode {
    let options = match Options::parse(options) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("sue: {error}");
            return ExitCode::from(2);
        }
    };
    match write_snapshots_with(Path::new(dir), 1440, 900, &options) {
        Ok(paths) => {
            for path in paths {
                println!("{}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("snapshot failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Reads what follows the two folders of `--snapshot-compare`: any
/// number of `--changed <scene>`.
fn changed_scenes(args: &[&str]) -> Result<Vec<String>, String> {
    args.chunks(2)
        .map(|pair| match pair {
            ["--changed", scene] => Ok((*scene).to_owned()),
            _ => Err(
                "after the two folders --snapshot-compare takes only --changed <scene>".to_owned(),
            ),
        })
        .collect()
}

/// Runs `sue --snapshot-compare <before> <after> [--changed <scene>]...`:
/// prints how each scene of the second folder stands against the first.
/// Fails (exit code 1) when a scene differs that no `--changed` names;
/// exit code 2 means the comparison could not be made.
#[must_use]
pub fn compare_cli(args: &[&str]) -> ExitCode {
    let [before, after, rest @ ..] = args else {
        eprintln!("sue: --snapshot-compare needs two folders");
        return ExitCode::from(2);
    };
    let scenes = changed_scenes(rest).and_then(|changed| {
        compare::compare_folders(Path::new(before), Path::new(after), &changed)
    });
    match scenes {
        Ok(scenes) => {
            print!("{}", compare::report(&scenes));
            if scenes.iter().any(compare::Scene::unexpected) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("sue: {error}");
            ExitCode::from(2)
        }
    }
}

/// Makes the empty folder the throwaway profile lives in. A folder the
/// caller named is removed only when an earlier run left it behind.
fn fresh_profile(dir: &Path, options: &Options) -> Result<PathBuf, String> {
    let root = match &options.profile {
        Some(root) => {
            if root.exists() && !root.join(PROFILE_MARK).is_file() {
                return Err(format!(
                    "{} exists and is not a snapshot profile; name a folder that does not exist",
                    root.display()
                ));
            }
            root.clone()
        }
        None => dir.join("profile"),
    };
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    std::fs::write(root.join(PROFILE_MARK), b"").map_err(|error| error.to_string())?;
    Ok(root)
}

/// Renders the shell's main scenes into `dir` as PNG files and returns
/// their paths: the plain run, at scale 1.0 with the default theme.
///
/// # Errors
/// GPU, boot or file-system failures.
pub fn write_snapshots(dir: &Path, width: u32, height: u32) -> Result<Vec<PathBuf>, String> {
    write_snapshots_with(dir, width, height, &Options::default())
}

/// Renders the shell's main scenes into `dir` as PNG files and returns
/// their paths. `width` and `height` are the desktop window's size in
/// points.
///
/// # Errors
/// GPU, boot or file-system failures, or a preset Sue does not have.
pub fn write_snapshots_with(
    dir: &Path,
    width: u32,
    height: u32,
    options: &Options,
) -> Result<Vec<PathBuf>, String> {
    let theme = options.preset.as_deref().map(preset).transpose()?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let gpu = GpuContext::headless().map_err(|error| error.to_string())?;
    let root = fresh_profile(dir, options)?;
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
    let boot = BootOptions {
        audio_devices: Vec::new(),
        gpu_label: Some(gpu.adapter_label()),
        gpu_kind: Some(gpu.adapter_kind().to_owned()),
        gpus: gpu.available().to_vec(),
    };
    // It remembers work in the throwaway profile, for the Hub's lists.
    let mut runtime = Runtime::new(Engine::boot_with(&host, boot)?)?.remembering(true);
    if let Some(theme) = theme {
        // Outside the history, so that no scene's Undo takes it back.
        let rice = &mut runtime.engine.world.rice;
        theme
            .transaction(rice)
            .apply(rice)
            .map_err(|error| error.to_string())?;
    }
    let start = Instant::now();
    let camera = Camera2 {
        ctx: egui::Context::default(),
        painter: EguiPainter::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb),
        size: [width, height],
        scale: options.scale,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_run_has_scale_one_and_no_preset() {
        assert_eq!(Options::parse(&[]), Ok(Options::default()));
        assert_eq!(Options::default().scale, 1.0);
    }

    #[test]
    fn options_are_read_in_any_order() {
        let options =
            Options::parse(&["--preset", "paper", "--scale", "1.5", "--profile", "p"]).unwrap();
        assert_eq!(
            options,
            Options {
                scale: 1.5,
                preset: Some("paper".to_owned()),
                profile: Some(PathBuf::from("p")),
            }
        );
    }

    #[test]
    fn wrong_options_are_refused_with_the_reason() {
        for (args, reason) in [
            (&["--scale"][..], "--scale needs a value"),
            (&["--scale", "0"], "from 0.5 to 4"),
            (&["--scale", "nan"], "from 0.5 to 4"),
            (&["--scale", "big"], "from 0.5 to 4"),
            (&["--preset", "no-such"], "there is no theme preset no-such"),
            (&["--zoom", "2"], "no option --zoom"),
        ] {
            let error = Options::parse(args).unwrap_err();
            assert!(error.contains(reason), "{args:?}: {error}");
        }
    }

    #[test]
    fn every_built_in_preset_can_be_asked_for() {
        let presets = sue_rice::builtin_presets();
        assert!(presets.len() > 1);
        for preset in presets {
            assert!(Options::parse(&["--preset", &preset.id]).is_ok());
        }
    }

    #[test]
    fn a_scaled_picture_keeps_its_points() {
        assert_eq!(pixel_size([1440, 900], 1.0), [1440, 900]);
        assert_eq!(pixel_size([1440, 900], 1.5), [2160, 1350]);
        assert_eq!(pixel_size([430, 932], 2.0), [860, 1864]);
        assert_eq!(pixel_size([393, 851], 1.5), [590, 1277]);
    }

    #[test]
    fn only_changed_follows_the_two_folders() {
        assert_eq!(changed_scenes(&[]), Ok(Vec::new()));
        assert_eq!(
            changed_scenes(&["--changed", "00-hub", "--changed", "20-*"]),
            Ok(vec!["00-hub".to_owned(), "20-*".to_owned()])
        );
        assert!(changed_scenes(&["--changed"]).is_err());
        assert!(changed_scenes(&["--same", "00-hub"]).is_err());
    }

    #[test]
    fn a_named_profile_folder_that_is_not_ours_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("sue-snapshot-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let theirs = dir.join("theirs");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("keep.txt"), "keep").unwrap();
        let named = |root: &Path| Options {
            profile: Some(root.to_path_buf()),
            ..Options::default()
        };
        assert!(fresh_profile(&dir, &named(&theirs)).is_err());
        assert!(theirs.join("keep.txt").is_file());

        // One the harness made is emptied and used again.
        let ours = dir.join("ours");
        assert_eq!(fresh_profile(&dir, &named(&ours)), Ok(ours.clone()));
        std::fs::write(ours.join("left-over.txt"), "x").unwrap();
        assert_eq!(fresh_profile(&dir, &named(&ours)), Ok(ours.clone()));
        assert!(!ours.join("left-over.txt").exists());
        assert!(ours.join(PROFILE_MARK).is_file());

        // Without a name it is `profile` in the output folder.
        assert_eq!(
            fresh_profile(&dir, &Options::default()),
            Ok(dir.join("profile"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
