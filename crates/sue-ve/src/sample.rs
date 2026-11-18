//! Media Sue makes itself, for the self-check, the tests and the
//! snapshot tour: a movie of moving color bars with a tone, written with
//! Sue's own MP4 export, so nothing of anyone's is needed to try SVE.

use std::path::Path;

use sue_core::{PixelFormat, RasterTile2D};

/// The color of bar `index` of eight.
const BARS: [[u8; 3]; 8] = [
    [235, 235, 235],
    [235, 235, 16],
    [16, 235, 235],
    [16, 235, 16],
    [235, 16, 235],
    [235, 16, 16],
    [16, 16, 235],
    [16, 16, 16],
];

/// Frame `index` of the sample movie: eight bars sliding left by
/// `index` pixels a frame, and a square whose shade counts the frames.
#[must_use]
pub fn frame(width: u32, height: u32, index: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut pixels = vec![0_u8; w * h * 4];
    let bar = (w / 8).max(1);
    let shade = ((index * 7) % 220 + 20) as u8;
    for y in 0..h {
        for x in 0..w {
            let at = (y * w + x) * 4;
            let rgb = if y < h / 3 && x < w / 4 {
                [shade, shade, shade]
            } else {
                BARS[((x + index as usize * 4) / bar) % 8]
            };
            pixels[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    pixels
}

/// Writes the sample movie: `seconds` long at `fps`, `width` by
/// `height`, with a 440 Hz tone (`sound`) at half scale.
///
/// # Errors
/// When this system has no MP4 encoder, or the file cannot be written.
pub fn write_movie(
    path: &Path,
    width: u32,
    height: u32,
    fps: u32,
    seconds: f64,
    sound: bool,
) -> Result<(), String> {
    let mut settings = sue_io::MovieSettings::new(width, height, fps);
    settings.quality = sue_io::MovieQuality::High;
    if sound {
        settings.sound = Some(sue_io::MovieSound {
            rate: 48_000,
            channels: 2,
        });
    }
    let mut sink = sue_platform::movie::open(sue_io::MovieFormat::Mp4H264, settings)
        .map_err(|error| error.text().to_owned())?;
    let frames = (seconds * f64::from(fps)).round() as u32;
    if sound {
        let count = (seconds * 48_000.0) as usize;
        let tone: Vec<f32> = (0..count)
            .flat_map(|at| {
                let value = (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [value, value]
            })
            .collect();
        sink.push_sound(&tone)
            .map_err(|error| error.text().to_owned())?;
    }
    for index in 0..frames {
        let tile = RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            frame(width, height, index),
        )
        .map_err(|error| error.to_string())?;
        sink.push_frame(&tile)
            .map_err(|error| error.text().to_owned())?;
    }
    let bytes = sink.finish().map_err(|error| error.text().to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}
