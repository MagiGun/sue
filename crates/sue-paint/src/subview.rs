//! Sub View: reference pictures beside the canvas, as in Clip Studio
//! Paint. Import pictures, step through them or pick one from the list,
//! zoom and turn the view of one, and pick colors from it with the
//! eyedropper. The pictures stay files on disk; only the one showing is
//! decoded.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, pos2, vec2};
use sue_core::PixelFormat;
use sue_io::{JxlPipeline, NativeJxlPipeline, PictureFormat, ProgressiveTarget};
use sue_wm::AppletContext;

use crate::studio::SuePaint;

/// A decoded reference picture.
#[derive(Clone, Debug)]
pub(crate) struct Shown {
    pub(crate) path: PathBuf,
    pub(crate) size: [u32; 2],
    pub(crate) pixels: Vec<u8>,
}

/// The reference pictures and how the one showing is viewed.
#[derive(Clone, Debug)]
pub(crate) struct SubView {
    /// The pictures, in the order they were imported.
    pub images: Vec<PathBuf>,
    /// The one showing.
    pub current: usize,
    /// Zoom as a multiple of fitting the view.
    pub zoom: f32,
    /// Turn in degrees.
    pub rotation: f32,
    /// Clicks pick colors instead of nothing.
    pub eyedropper: bool,
    /// The list of every picture shows instead of the picture.
    pub list: bool,
    pub(crate) shown: Option<Shown>,
}

impl Default for SubView {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            current: 0,
            zoom: 1.0,
            rotation: 0.0,
            eyedropper: true,
            list: false,
            shown: None,
        }
    }
}

/// Reads a picture file into RGBA8 pixels.
///
/// # Errors
/// If no codec reads it.
pub(crate) fn decode(path: &Path) -> Result<([u32; 2], Vec<u8>), String> {
    let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let tile = match extension.as_str() {
        "png" => sue_io::decode_png(&bytes)?.tile,
        "jpg" | "jpeg" => sue_io::decode_jpeg(&bytes)?.tile,
        "jxl" => {
            NativeJxlPipeline
                .decode(&bytes, ProgressiveTarget::Full)
                .map_err(|error| error.to_string())?
                .tile
        }
        _ => sue_io::decode_picture(&bytes, PictureFormat::from_extension(&extension))?,
    };
    let pixels = match (tile.format(), tile.bytes()) {
        (PixelFormat::Rgba8, Some(bytes)) => bytes.to_vec(),
        (PixelFormat::Gray8, Some(bytes)) => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        _ => return Err(tr!("pictures-pixels-cannot-shown")),
    };
    Ok(([tile.width(), tile.height()], pixels))
}

/// Where picture pixel `p` of a `size` picture lands in `rect`, fitted,
/// zoomed by `zoom` and turned by `degrees` around the middle.
pub(crate) fn place(rect: Rect, size: [u32; 2], zoom: f32, degrees: f32, p: Pos2) -> Pos2 {
    let fit =
        (rect.width() / size[0].max(1) as f32).min(rect.height() / size[1].max(1) as f32) * zoom;
    let v = (p - pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0)) * fit;
    let (sin, cos) = degrees.to_radians().sin_cos();
    rect.center() + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// The picture pixel under screen point `s` (the inverse of [`place`]).
pub(crate) fn unplace(rect: Rect, size: [u32; 2], zoom: f32, degrees: f32, s: Pos2) -> Pos2 {
    let fit =
        (rect.width() / size[0].max(1) as f32).min(rect.height() / size[1].max(1) as f32) * zoom;
    let v = s - rect.center();
    let (sin, cos) = (-degrees).to_radians().sin_cos();
    let turned = vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos) / fit.max(1e-6);
    pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0) + turned
}

impl SuePaint {
    /// Adds a reference picture and shows it.
    pub(crate) fn add_reference(&mut self, path: &Path, cx: &mut AppletContext<'_>) {
        let view = &mut self.sub_view;
        if !view.images.iter().any(|known| known == path) {
            view.images.push(path.to_path_buf());
        }
        view.current = view
            .images
            .iter()
            .position(|known| known == path)
            .unwrap_or(0);
        view.shown = None;
        view.list = false;
        if let Err(error) = decode(path) {
            cx.fail(tr!("cannot-show", path = path.display(), error = error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_points_come_back_however_turned_and_zoomed() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0));
        for (zoom, degrees) in [(1.0, 0.0), (2.5, 37.0), (0.4, -90.0)] {
            for p in [pos2(0.0, 0.0), pos2(30.0, 12.0), pos2(64.0, 32.0)] {
                let back = unplace(
                    rect,
                    [64, 32],
                    zoom,
                    degrees,
                    place(rect, [64, 32], zoom, degrees, p),
                );
                assert!((back - p).length() < 1e-3, "{p:?} came back as {back:?}");
            }
        }
        // Fitted, the middle of the picture is the middle of the view.
        assert!(
            (place(rect, [64, 32], 1.0, 0.0, pos2(32.0, 16.0)) - rect.center()).length() < 1e-4
        );
    }
}
