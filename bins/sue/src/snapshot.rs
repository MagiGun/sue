//! Offscreen snapshots: boots the real shell headlessly against a fresh,
//! temporary profile (so the defaults are what you see), opens sample files
//! through the same path as the window, and renders each scene with the GPU
//! renderer into a PNG.

use std::path::{Path, PathBuf};
use std::time::Instant;

use sue_core::{PCMFloat1D, PixelFormat, RasterTile2D};
use sue_io::{JxlPipeline, NativeJxlPipeline, encode_png, encode_png_rgba, encode_wav};
use sue_platform::{
    FormFactor, GpuContext, HardwareCapabilities, HeadlessHost, OsFamily, PlatformPaths,
    SafeAreaInsets,
};
use sue_rice::DEFAULT_RICE;
use sue_wm::{FrameTarget, Rect, render_offscreen};

use crate::engine::{BootOptions, Engine};
use crate::runtime::Runtime;

/// A 256x160 test card: hue ramps, a gray wedge and an alpha border.
pub fn test_card() -> RasterTile2D {
    let (width, height) = (256_u32, 160_u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = if y < 120 {
                (x as u8, (y * 2) as u8, (255 - x) as u8)
            } else {
                let gray = ((x / 32) * 36) as u8;
                (gray, gray, gray)
            };
            let edge = x < 4 || y < 4 || x >= width - 4 || y >= height - 4;
            pixels.extend([r, g, b, if edge { 96 } else { 255 }]);
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
    std::fs::create_dir_all(home.join("Projects")).map_err(|error| error.to_string())?;
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
    };
    let mut runtime = Runtime::new(Engine::boot_with(&host, options)?)?;

    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let frame = FrameTarget {
        width,
        height,
        scale: 1.0,
    };
    let mut written = Vec::new();
    let mut shoot = |runtime: &mut Runtime, name: &str| -> Result<(), String> {
        // Two frames: the second lays out against the first one's widgets.
        runtime.frame(viewport, SafeAreaInsets::ZERO, Instant::now());
        let output = runtime.frame(viewport, SafeAreaInsets::ZERO, Instant::now());
        let pixels = render_offscreen(
            &gpu.device,
            &gpu.queue,
            frame,
            &output.canvas,
            &runtime.fonts,
            &runtime.engine.world.arena,
            runtime.theme().base,
        )?;
        let path = dir.join(format!("{name}.png"));
        std::fs::write(&path, encode_png_rgba(width, height, &pixels)?)
            .map_err(|error| error.to_string())?;
        written.push(path);
        Ok(())
    };

    let home = paths.home_dir;
    shoot(&mut runtime, "01-home")?;
    for (page, name) in [
        ("appearance", "02-settings-appearance"),
        ("display", "03-settings-display"),
        ("audio", "04-settings-audio"),
        ("keymaps", "05-settings-keymaps"),
    ] {
        runtime.open_settings(Some(page));
        shoot(&mut runtime, name)?;
    }
    runtime.open_path(&home.join("test-card.jxl"));
    shoot(&mut runtime, "06-image-viewer")?;
    runtime.open_path(&home.join("README.md"));
    runtime.show_dock(crate::history::ID);
    shoot(&mut runtime, "07-text-studio")?;
    runtime.open_path(&home.join("archive.zip"));
    runtime.show_dock(crate::inspector::ID);
    shoot(&mut runtime, "08-hex-inspector")?;
    runtime.open_path(&home.join("chord.wav"));
    shoot(&mut runtime, "09-player")?;
    runtime.open_path(&home.join("photo.png"));
    runtime.show_dock(crate::convert::ID);
    shoot(&mut runtime, "10-convert")?;
    let _ = std::fs::remove_dir_all(&root);
    Ok(written)
}
