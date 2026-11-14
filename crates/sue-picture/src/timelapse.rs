//! Timelapse, as in Clip Studio Paint: while a canvas records, every change
//! to it is kept as a frame, and Export Timelapse makes a video of how the
//! picture was drawn. The frames are small JPEG pictures in Sue Paint's own
//! folder, a folder a picture, so recording costs the undo history and the
//! project nothing; the picture keeps only whether it records
//! ([`RECORD`]).

use std::path::{Path, PathBuf};

use sue_core::{EntityId, PixelFormat, RasterTile2D};

/// Whether a picture records its timelapse (`Bool`).
pub const RECORD: &str = "timelapse.record";

/// The longest side a frame is kept at.
pub const FRAME_SIDE: u32 = 1280;

/// The folder `picture`'s frames are kept in, under Sue Paint's folder
/// `base`.
#[must_use]
pub fn folder(base: &Path, picture: EntityId) -> PathBuf {
    base.join("timelapse").join(picture.to_string())
}

/// The frames kept in `dir`, in order.
#[must_use]
pub fn frames(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = sue_platform::list_dir(dir, true) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| path.extension().is_some_and(|ext| ext == "jpg"))
        .collect();
    out.sort();
    out
}

/// `size` shrunk to fit a `side` by `side` box, keeping its shape.
#[must_use]
pub fn fit(size: [u32; 2], side: u32) -> [u32; 2] {
    let longest = size[0].max(size[1]).max(1);
    if longest <= side {
        return size;
    }
    let scale = f64::from(side) / f64::from(longest);
    [
        ((f64::from(size[0]) * scale).round() as u32).max(1),
        ((f64::from(size[1]) * scale).round() as u32).max(1),
    ]
}

/// Keeps `pixels` (RGBA8, `size`) as frame `index` in `dir`.
///
/// # Errors
/// If the frame will not encode or write.
pub fn capture(dir: &Path, index: usize, pixels: &[u8], size: [u32; 2]) -> Result<(), String> {
    let to = fit(size, FRAME_SIDE);
    let small = if to == size {
        pixels.to_vec()
    } else {
        crate::doc::resize_rgba(pixels, size, to)
    };
    let tile = RasterTile2D::from_bytes(to[0], to[1], PixelFormat::Rgba8, small)
        .map_err(|error| error.to_string())?;
    let jpeg = sue_io::encode_as(&tile, "jpg", 85, 1)?;
    sue_platform::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    sue_platform::write_atomic(&dir.join(format!("{index:06}.jpg")), &jpeg)
        .map_err(|error| error.to_string())
}

/// How Export Timelapse lays the video out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Export {
    /// The longest side of the video, in pixels.
    pub side: u32,
    /// Fit to this many seconds; `None` exports every frame at 30 a second.
    pub seconds: Option<f32>,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            side: 1080,
            seconds: Some(30.0),
        }
    }
}

/// The frames of `dir` read back, each laid over white in a `size` box
/// (the last frame's shape at `export.side`), and the frames a second:
/// every frame at 30 a second, or as many as fit the length, skipping some
/// when there are more than 60 a second. The last frame holds a second.
///
/// # Errors
/// If there are no frames, or one will not read.
pub fn video_frames(dir: &Path, export: Export) -> Result<(Vec<RasterTile2D>, u32), String> {
    let paths = frames(dir);
    let last = paths
        .last()
        .ok_or(tr!("canvas-has-no-timelapse-yet-record"))?;
    let read = |path: &Path| -> Result<RasterTile2D, String> {
        let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
        Ok(sue_io::decode_jpeg(&bytes)?.tile)
    };
    let shape = read(last)?;
    // Even sides, as video players like them.
    let even = |value: u32| (value.max(2) / 2) * 2;
    let size = fit([shape.width(), shape.height()], export.side.max(16)).map(even);
    let count = paths.len();
    let (fps, step) = match export.seconds {
        Some(seconds) if seconds > 0.0 => {
            let wanted = (count as f32 / seconds).ceil().max(1.0);
            if wanted > 60.0 {
                (60, (wanted / 60.0).ceil() as usize)
            } else {
                (wanted as u32, 1)
            }
        }
        _ => (30, 1),
    };
    let mut out = Vec::new();
    let mut chosen: Vec<&PathBuf> = paths.iter().step_by(step).collect();
    if chosen.last() != Some(&last) {
        chosen.push(last);
    }
    for path in chosen {
        out.push(letterbox(&read(path)?, size));
    }
    if let Some(end) = out.last().cloned() {
        out.extend(std::iter::repeat_n(end, fps as usize));
    }
    Ok((out, fps))
}

/// `tile` scaled to fit `size`, centered on white.
fn letterbox(tile: &RasterTile2D, size: [u32; 2]) -> RasterTile2D {
    let from = [tile.width(), tile.height()];
    let scale =
        (f64::from(size[0]) / f64::from(from[0])).min(f64::from(size[1]) / f64::from(from[1]));
    let to = [
        ((f64::from(from[0]) * scale).round() as u32).clamp(1, size[0]),
        ((f64::from(from[1]) * scale).round() as u32).clamp(1, size[1]),
    ];
    let pixels = tile
        .bytes()
        .map_or_else(Vec::new, |bytes| crate::doc::resize_rgba(bytes, from, to));
    let mut out = vec![255_u8; (size[0] * size[1] * 4) as usize];
    let (left, top) = ((size[0] - to[0]) / 2, (size[1] - to[1]) / 2);
    for y in 0..to[1] {
        let from_row = (y * to[0] * 4) as usize;
        let into_row = (((top + y) * size[0] + left) * 4) as usize;
        if let Some(row) = pixels.get(from_row..from_row + (to[0] * 4) as usize) {
            out[into_row..into_row + row.len()].copy_from_slice(row);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out)
        .unwrap_or_else(|_| tile.clone())
}

/// The timelapse in `dir` as a Motion JPEG AVI video, and how many frames
/// it has.
///
/// # Errors
/// As [`video_frames`].
pub fn export_avi(dir: &Path, export: Export) -> Result<(Vec<u8>, usize), String> {
    let (frames, fps) = video_frames(dir, export)?;
    let first = frames.first().ok_or(tr!("no-frames"))?;
    let mut video = sue_io::MjpegAvi::new(first.width(), first.height(), fps);
    for frame in &frames {
        video.push_frame(frame, 85)?;
    }
    let count = video.frames();
    Ok((video.finish(), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_kept_and_export_as_a_video_of_the_drawing() {
        let dir = std::env::temp_dir().join(format!("sue-timelapse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            video_frames(&dir, Export::default()).is_err(),
            "nothing recorded"
        );
        // Three steps of a 64 by 32 picture darkening.
        for (index, shade) in [255_u8, 160, 60].into_iter().enumerate() {
            let pixels = [shade, shade, shade, 255].repeat(64 * 32);
            capture(&dir, index, &pixels, [64, 32]).unwrap();
        }
        assert_eq!(frames(&dir).len(), 3);
        assert_eq!(fit([4000, 2000], 1280), [1280, 640]);
        // Every frame at 30 a second, the last held a second.
        let all = Export {
            side: 32,
            seconds: None,
        };
        let (video, fps) = video_frames(&dir, all).unwrap();
        assert_eq!(fps, 30);
        assert_eq!(video.len(), 3 + 30);
        assert_eq!((video[0].width(), video[0].height()), (32, 16));
        assert!(
            video[0].bytes().unwrap()[0] > video[2].bytes().unwrap()[0],
            "it darkens"
        );
        let (avi, count) = export_avi(&dir, all).unwrap();
        assert_eq!(&avi[..4], b"RIFF");
        assert_eq!(count, 33);
        // Fit to a second: three frames a second.
        let (_, fps) = video_frames(
            &dir,
            Export {
                side: 32,
                seconds: Some(1.0),
            },
        )
        .unwrap();
        assert_eq!(fps, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
