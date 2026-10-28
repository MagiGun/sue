//! Offscreen snapshots: boots the real shell headlessly, drops sample files
//! onto it through the same ingest path as the window, and renders each scene
//! with the GPU renderer into a PNG.

use std::path::{Path, PathBuf};
use std::time::Instant;

use sue_core::{PixelFormat, RasterTile2D};
use sue_io::{JxlPipeline, NativeJxlPipeline};
use sue_platform::{FileDrop, GpuContext, HeadlessHost, InputEvent, SafeAreaInsets};
use sue_rice::DEFAULT_RICE;
use sue_wm::{FrameTarget, Rect, render_offscreen};

use crate::engine::Engine;
use crate::png;
use crate::runtime::Runtime;

/// A 256×160 test card: hue ramps, a gray wedge and an alpha border.
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

/// Renders the Launchpad, Image Viewer, Text Studio and Hex Inspector scenes
/// into `dir` as PNG files and returns their paths.
///
/// # Errors
/// GPU, boot or file-system failures.
pub fn write_snapshots(dir: &Path, width: u32, height: u32) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let gpu = GpuContext::headless().map_err(|error| error.to_string())?;
    let host = HeadlessHost::detect().map_err(|error| error.to_string())?;
    let mut runtime = Runtime::new(Engine::boot(&host)?)?;

    let samples = dir.join("samples");
    std::fs::create_dir_all(&samples).map_err(|error| error.to_string())?;
    let jxl = NativeJxlPipeline
        .encode_layers(&[&test_card()], 3)
        .map_err(|error| error.to_string())?;
    let files = [
        ("test-card.jxl", jxl),
        ("default.suerice", DEFAULT_RICE.as_bytes().to_vec()),
        ("archive.zip", sample_binary()),
    ];

    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let frame = FrameTarget {
        width,
        height,
        scale: 1.0,
    };
    let mut written = Vec::new();
    let mut shoot = |runtime: &mut Runtime, name: &str| -> Result<(), String> {
        let output = runtime.frame(viewport, SafeAreaInsets::ZERO, Instant::now());
        let pixels = render_offscreen(
            &gpu.device,
            &gpu.queue,
            frame,
            &output.canvas,
            &runtime.fonts,
            &runtime.engine.world.arena,
            runtime.theme().background,
        )?;
        let path = dir.join(format!("{name}.png"));
        std::fs::write(&path, png::encode_rgba(width, height, &pixels))
            .map_err(|error| error.to_string())?;
        written.push(path);
        Ok(())
    };

    shoot(&mut runtime, "1-launchpad")?;
    for (index, (file, bytes)) in files.into_iter().enumerate() {
        let path = samples.join(file);
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        runtime.handle(&InputEvent::FileDrop(FileDrop::Dropped(path)));
        let scene = ["2-image-viewer", "3-text-studio", "4-hex-inspector"][index];
        shoot(&mut runtime, scene)?;
    }
    Ok(written)
}
