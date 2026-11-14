//! What an export does to a picture on its way out: resize it, turn and
//! mirror it, change its colors, lay it over a background, fit it into
//! the largest size a format allows, and write it in the chosen format.
//! Pure functions of their inputs, so they run on a background thread.
//!
//! The texts stay English here; the interface reads them in the language
//! in use through `sue_lang::translate("io", ...)`.

use std::path::Path;

use sue_core::{PixelFormat, RasterTile2D};

use crate::{
    JxlError, JxlPipeline, NativeJxlPipeline, PictureFormat, ProgressiveTarget, decode_jpeg,
    decode_picture, decode_png, encode_as,
};

/// The picture has no pixel bytes to work on.
const NO_PIXELS: &str = "the picture has no pixels";
/// Float pictures are not exported through these operations.
const NO_FLOAT: &str = "float pictures are not converted yet";
/// This build has no JPEG XL encoder.
const NO_JXL: &str = "JPEG XL writing is not in this build";

/// How a picture is resized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resize {
    /// Not at all.
    Keep,
    /// By a percentage.
    Scale(f32),
    /// To a width, keeping the shape.
    Width(u32),
    /// To a height, keeping the shape.
    Height(u32),
    /// To fit inside a box, keeping the shape.
    Fit(u32, u32),
    /// To exactly this size.
    Exact(u32, u32),
}

/// How pixels are resampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// The nearest pixel: crisp pixel art.
    Nearest,
    /// A straight blend of neighbors.
    Bilinear,
    /// Catmull-Rom: sharper.
    Bicubic,
    /// Lanczos: sharpest, for photos.
    Lanczos,
}

impl Filter {
    /// Every filter.
    pub const ALL: [Self; 4] = [Self::Lanczos, Self::Bicubic, Self::Bilinear, Self::Nearest];

    /// Its name in English; `sue_lang::translate("io", ...)` gives it in
    /// the language in use.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lanczos => "Lanczos (sharpest)",
            Self::Bicubic => "Bicubic",
            Self::Bilinear => "Bilinear (soft)",
            Self::Nearest => "Nearest (pixel art)",
        }
    }

    fn support(self) -> f32 {
        match self {
            Self::Nearest => 0.5,
            Self::Bilinear => 1.0,
            Self::Bicubic => 2.0,
            Self::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f32) -> f32 {
        let x = x.abs();
        match self {
            Self::Nearest => f32::from(x < 0.5),
            Self::Bilinear => (1.0 - x).max(0.0),
            Self::Bicubic => {
                if x < 1.0 {
                    1.5 * x * x * x - 2.5 * x * x + 1.0
                } else if x < 2.0 {
                    -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
                } else {
                    0.0
                }
            }
            Self::Lanczos => {
                if x < 1e-6 {
                    1.0
                } else if x < 3.0 {
                    let pi = std::f32::consts::PI * x;
                    3.0 * pi.sin() * (pi / 3.0).sin() / (pi * pi)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Everything an export can change about a picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureOptions {
    /// Resizing.
    pub resize: Resize,
    /// Resampling.
    pub filter: Filter,
    /// Clockwise quarter turns.
    pub quarter_turns: u8,
    /// Mirror left to right.
    pub flip_h: bool,
    /// Mirror top to bottom.
    pub flip_v: bool,
    /// Grays only.
    pub grayscale: bool,
    /// -1 to 1.
    pub brightness: f32,
    /// -1 to 1.
    pub contrast: f32,
    /// -1 to 1.
    pub saturation: f32,
    /// Fill transparency with the background.
    pub flatten: bool,
    /// The background for flattening.
    pub background: [u8; 3],
    /// JPEG quality, 1 to 100.
    pub jpeg_quality: u8,
    /// JPEG XL effort, 1 to 9.
    pub jxl_effort: u8,
}

impl Default for PictureOptions {
    fn default() -> Self {
        Self {
            resize: Resize::Keep,
            filter: Filter::Lanczos,
            quarter_turns: 0,
            flip_h: false,
            flip_v: false,
            grayscale: false,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            flatten: false,
            background: [255, 255, 255],
            jpeg_quality: 90,
            jxl_effort: 7,
        }
    }
}

/// `extension` without its dot, in lower case.
fn plain(extension: &str) -> String {
    extension.trim_start_matches('.').to_ascii_lowercase()
}

/// The format of this module's list that `extension` names, if any.
fn listed(extension: &str) -> Option<PictureFormat> {
    PictureFormat::from_extension(&plain(extension))
}

/// `true` when files of `extension` cannot hold transparency.
fn flattens(extension: &str) -> bool {
    matches!(plain(extension).as_str(), "jpg" | "jpeg")
        || listed(extension).is_some_and(|format| !format.keeps_alpha())
}

/// Reads a picture file's bytes into RGBA8 pixels. The extension of
/// `path` picks the codec.
///
/// # Errors
/// If no codec reads it.
pub fn read_picture(path: &Path, bytes: &[u8]) -> Result<RasterTile2D, String> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let tile = match extension.as_str() {
        "png" => decode_png(bytes)?.tile,
        "jpg" | "jpeg" => decode_jpeg(bytes)?.tile,
        "jxl" => {
            NativeJxlPipeline
                .decode(bytes, ProgressiveTarget::Full)
                .map_err(|error| error.to_string())?
                .tile
        }
        _ => decode_picture(bytes, PictureFormat::from_extension(&extension))?,
    };
    rgba(&tile)
}

/// `tile` as RGBA8.
///
/// # Errors
/// If it holds float pixels.
fn rgba(tile: &RasterTile2D) -> Result<RasterTile2D, String> {
    let bytes = tile.bytes().ok_or(NO_PIXELS)?;
    let pixels = match tile.format() {
        PixelFormat::Rgba8 => return Ok(tile.clone()),
        PixelFormat::Gray8 => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        PixelFormat::RgbaF32 => return Err(NO_FLOAT.to_owned()),
    };
    RasterTile2D::from_bytes(tile.width(), tile.height(), PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())
}

/// The size `resize` makes of `width` by `height`.
#[must_use]
pub fn resized(width: u32, height: u32, resize: Resize) -> (u32, u32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let (nw, nh) = match resize {
        Resize::Keep => (w, h),
        Resize::Scale(percent) => (w * percent / 100.0, h * percent / 100.0),
        Resize::Width(target) => (target as f32, h * target as f32 / w),
        Resize::Height(target) => (w * target as f32 / h, target as f32),
        Resize::Fit(bw, bh) => {
            let scale = (bw as f32 / w).min(bh as f32 / h);
            (w * scale, h * scale)
        }
        Resize::Exact(bw, bh) => (bw as f32, bh as f32),
    };
    (
        (nw.round() as u32).clamp(1, 32_768),
        (nh.round() as u32).clamp(1, 32_768),
    )
}

/// Resamples premultiplied RGBA rows along one axis.
fn resample_axis(
    source: &[f32],
    (width, height): (usize, usize),
    out_len: usize,
    horizontal: bool,
    filter: Filter,
) -> Vec<f32> {
    let in_len = if horizontal { width } else { height };
    let lines = if horizontal { height } else { width };
    let scale = out_len as f32 / in_len as f32;
    let stretch = if scale < 1.0 { 1.0 / scale } else { 1.0 };
    let support = filter.support() * stretch;
    let (out_w, out_h) = if horizontal {
        (out_len, height)
    } else {
        (width, out_len)
    };
    let mut out = vec![0.0_f32; out_w * out_h * 4];
    // The taps of each output position, shared by every line.
    let taps: Vec<(usize, Vec<f32>)> = (0..out_len)
        .map(|at| {
            let center = (at as f32 + 0.5) / scale;
            let first = (center - support).floor().max(0.0) as usize;
            let last = ((center + support).ceil() as usize).min(in_len);
            let mut weights: Vec<f32> = (first..last)
                .map(|index| filter.weight((index as f32 + 0.5 - center) / stretch))
                .collect();
            let total: f32 = weights.iter().sum();
            if total.abs() > 1e-6 {
                weights.iter_mut().for_each(|weight| *weight /= total);
            } else if let Some(nearest) = weights.first_mut() {
                *nearest = 1.0;
            }
            (first, weights)
        })
        .collect();
    for line in 0..lines {
        for (at, (first, weights)) in taps.iter().enumerate() {
            let mut sum = [0.0_f32; 4];
            for (offset, weight) in weights.iter().enumerate() {
                let index = first + offset;
                let pixel = if horizontal {
                    line * width + index
                } else {
                    index * width + line
                } * 4;
                for channel in 0..4 {
                    sum[channel] += source[pixel + channel] * weight;
                }
            }
            let pixel = if horizontal {
                line * out_w + at
            } else {
                at * out_w + line
            } * 4;
            out[pixel..pixel + 4].copy_from_slice(&sum);
        }
    }
    out
}

/// Resizes RGBA8 pixels with `filter`, blending in premultiplied alpha so
/// transparent edges never turn dark.
#[must_use]
pub fn resize(
    pixels: &[u8],
    (width, height): (u32, u32),
    (nw, nh): (u32, u32),
    filter: Filter,
) -> Vec<u8> {
    if (width, height) == (nw, nh) {
        return pixels.to_vec();
    }
    let premultiplied: Vec<f32> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = f32::from(pixel[3]) / 255.0;
            [
                f32::from(pixel[0]) * alpha,
                f32::from(pixel[1]) * alpha,
                f32::from(pixel[2]) * alpha,
                alpha * 255.0,
            ]
        })
        .collect();
    let (w, h) = (width as usize, height as usize);
    let across = resample_axis(&premultiplied, (w, h), nw as usize, true, filter);
    let done = resample_axis(&across, (nw as usize, h), nh as usize, false, filter);
    done.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = pixel[3].clamp(0.0, 255.0);
            let unpremultiply = |value: f32| {
                if alpha <= 0.0 {
                    0
                } else {
                    (value * 255.0 / alpha).round().clamp(0.0, 255.0) as u8
                }
            };
            [
                unpremultiply(pixel[0]),
                unpremultiply(pixel[1]),
                unpremultiply(pixel[2]),
                alpha.round() as u8,
            ]
        })
        .collect()
}

/// Turns RGBA8 pixels a quarter turn clockwise, `turns` times.
#[must_use]
fn turn(pixels: &[u8], (width, height): (u32, u32), turns: u8) -> (Vec<u8>, (u32, u32)) {
    let (w, h) = (width as usize, height as usize);
    match turns % 4 {
        0 => (pixels.to_vec(), (width, height)),
        2 => {
            let out: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .rev()
                .flatten()
                .copied()
                .collect();
            (out, (width, height))
        }
        quarter => {
            let mut out = vec![0_u8; pixels.len()];
            for y in 0..h {
                for x in 0..w {
                    let (nx, ny) = if quarter == 1 {
                        (h - 1 - y, x)
                    } else {
                        (y, w - 1 - x)
                    };
                    let from = (y * w + x) * 4;
                    let to = (ny * h + nx) * 4;
                    out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
                }
            }
            (out, (height, width))
        }
    }
}

/// Mirrors RGBA8 pixels.
fn flip(pixels: &mut [u8], (width, height): (u32, u32), horizontal: bool) {
    let (w, h) = (width as usize, height as usize);
    if horizontal {
        for row in pixels.chunks_exact_mut(w * 4) {
            for x in 0..w / 2 {
                for channel in 0..4 {
                    row.swap(x * 4 + channel, (w - 1 - x) * 4 + channel);
                }
            }
        }
    } else {
        for y in 0..h / 2 {
            let (top, bottom) = pixels.split_at_mut((h - 1 - y) * w * 4);
            top[y * w * 4..(y + 1) * w * 4].swap_with_slice(&mut bottom[..w * 4]);
        }
    }
}

/// Brightness, contrast, saturation, grays and flattening, pixel by pixel.
fn adjust(pixels: &mut [u8], options: &PictureOptions, flatten: bool) {
    let contrast = (1.0 + options.contrast).max(0.0).powf(2.0);
    for pixel in pixels.as_chunks_mut::<4>().0.iter_mut() {
        let mut rgb = [pixel[0], pixel[1], pixel[2]].map(|value| f32::from(value) / 255.0);
        for value in &mut rgb {
            *value = ((*value - 0.5) * contrast + 0.5 + options.brightness * 0.5).clamp(0.0, 1.0);
        }
        let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        let saturation = if options.grayscale {
            0.0
        } else {
            1.0 + options.saturation
        };
        for value in &mut rgb {
            *value = (luma + (*value - luma) * saturation).clamp(0.0, 1.0);
        }
        if flatten {
            let alpha = f32::from(pixel[3]) / 255.0;
            for (value, background) in rgb.iter_mut().zip(options.background) {
                *value = *value * alpha + f32::from(background) / 255.0 * (1.0 - alpha);
            }
            pixel[3] = 255;
        }
        for (channel, value) in pixel.iter_mut().zip(rgb) {
            *channel = (value * 255.0).round() as u8;
        }
    }
}

/// Changes a picture as the options say, on its way to a file of
/// `extension`: a format with a largest side (an icon, say) gets the
/// picture fitted into it, and a format without transparency gets it laid
/// over the background.
///
/// # Errors
/// If it holds float pixels.
pub fn transform_picture(
    tile: &RasterTile2D,
    options: &PictureOptions,
    extension: &str,
) -> Result<RasterTile2D, String> {
    let tile = rgba(tile)?;
    let mut size = (tile.width(), tile.height());
    let mut pixels = tile.bytes().ok_or(NO_PIXELS)?.to_vec();
    let mut new_size = resized(size.0, size.1, options.resize);
    // Icons and the like have a largest size: fit into it.
    if let Some(max) = listed(extension).and_then(PictureFormat::max_side)
        && new_size.0.max(new_size.1) > max
    {
        new_size = resized(new_size.0, new_size.1, Resize::Fit(max, max));
    }
    if new_size != size {
        pixels = resize(&pixels, size, new_size, options.filter);
        size = new_size;
    }
    if !options.quarter_turns.is_multiple_of(4) {
        (pixels, size) = turn(&pixels, size, options.quarter_turns);
    }
    if options.flip_h {
        flip(&mut pixels, size, true);
    }
    if options.flip_v {
        flip(&mut pixels, size, false);
    }
    let flatten = options.flatten || flattens(extension);
    let touched = options.grayscale
        || options.brightness != 0.0
        || options.contrast != 0.0
        || options.saturation != 0.0
        || flatten;
    if touched {
        adjust(&mut pixels, options, flatten);
    }
    RasterTile2D::from_bytes(size.0, size.1, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())
}

/// Writes a picture as a file of `extension` (any of
/// [`crate::PICTURE_SAVE_TYPES`]), with the quality and effort of the
/// options.
///
/// # Errors
/// The encoder's message, or that Sue writes no such pictures.
pub fn write_picture(
    tile: &RasterTile2D,
    extension: &str,
    options: &PictureOptions,
) -> Result<Vec<u8>, String> {
    let effort = options.jxl_effort.clamp(1, 9);
    if plain(extension) == "jxl" {
        return NativeJxlPipeline
            .encode_layers(&[tile], effort)
            .map_err(|error| match error {
                JxlError::CodecUnavailable { .. } => NO_JXL.to_owned(),
                other => other.to_string(),
            });
    }
    encode_as(tile, extension, options.jpeg_quality.clamp(1, 100), effort)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32) -> RasterTile2D {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend([(x * 40) as u8, (y * 40) as u8, 128, 255]);
            }
        }
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn sizes_follow_the_resize_choice() {
        assert_eq!(resized(400, 200, Resize::Scale(50.0)), (200, 100));
        assert_eq!(resized(400, 200, Resize::Width(100)), (100, 50));
        assert_eq!(resized(400, 200, Resize::Height(100)), (200, 100));
        assert_eq!(resized(400, 200, Resize::Fit(100, 100)), (100, 50));
        assert_eq!(resized(400, 200, Resize::Exact(30, 70)), (30, 70));
        assert_eq!(resized(400, 200, Resize::Keep), (400, 200));
    }

    #[test]
    fn resizing_keeps_flat_colors_and_turning_moves_pixels() {
        let flat = [10_u8, 200, 30, 255].repeat(16 * 16);
        for filter in Filter::ALL {
            let small = resize(&flat, (16, 16), (5, 7), filter);
            assert_eq!(small.len(), 5 * 7 * 4);
            assert!(
                small
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == [10, 200, 30, 255]),
                "{filter:?}"
            );
        }
        // A see-through edge stays its own color, not darker.
        let mut edge = [255_u8, 0, 0, 255].repeat(4);
        edge.extend([0, 0, 0, 0].repeat(4));
        let blended = resize(&edge, (8, 1), (4, 1), Filter::Bilinear);
        assert!(
            blended
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel[0] == 255)
        );
        let tile = picture(3, 2);
        let pixels = tile.bytes().unwrap();
        let (turned, size) = turn(pixels, (3, 2), 1);
        assert_eq!(size, (2, 3));
        // The top left pixel moves to the top right.
        assert_eq!(&turned[4..8], &pixels[..4]);
        let (back, _) = turn(&turned, size, 3);
        assert_eq!(back, pixels);
        let mut mirrored = pixels.to_vec();
        flip(&mut mirrored, (3, 2), true);
        assert_eq!(&mirrored[8..12], &pixels[..4]);
        flip(&mut mirrored, (3, 2), true);
        flip(&mut mirrored, (3, 2), false);
        assert_eq!(&mirrored[..12], &pixels[12..]);
    }

    #[test]
    fn pictures_convert_to_every_picture_format() {
        let png = crate::encode_png(&picture(20, 10)).unwrap();
        let source = read_picture(Path::new("card.png"), &png).unwrap();
        let options = PictureOptions {
            resize: Resize::Width(10),
            grayscale: true,
            ..PictureOptions::default()
        };
        for (name, extensions) in crate::PICTURE_SAVE_TYPES {
            let extension = extensions[0];
            let changed = transform_picture(&source, &options, extension)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let bytes = write_picture(&changed, extension, &options)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(!bytes.is_empty(), "{name}");
            let out = Path::new("card").with_extension(extension);
            let back = read_picture(&out, &bytes).unwrap();
            assert_eq!((back.width(), back.height()), (10, 5), "{name}");
            let pixel = &back.bytes().unwrap()[..3];
            let spread = pixel.iter().max().unwrap() - pixel.iter().min().unwrap();
            assert!(spread <= 12, "{name} kept color {pixel:?}");
        }
        assert!(write_picture(&source, "wav", &options).is_err());
    }

    #[test]
    fn a_format_gets_its_largest_side_and_its_background() {
        // An icon is at most 256 pixels wide and high.
        let icon =
            transform_picture(&picture(600, 300), &PictureOptions::default(), ".ICO").unwrap();
        assert_eq!((icon.width(), icon.height()), (256, 128));
        let bytes = write_picture(&icon, "ico", &PictureOptions::default()).unwrap();
        let back = read_picture(Path::new("app.ico"), &bytes).unwrap();
        assert_eq!((back.width(), back.height()), (256, 128));
        // Other formats keep the size.
        let same =
            transform_picture(&picture(600, 300), &PictureOptions::default(), "png").unwrap();
        assert_eq!((same.width(), same.height()), (600, 300));
        // A format without transparency gets the background color.
        let clear = RasterTile2D::from_bytes(1, 1, PixelFormat::Rgba8, vec![0, 0, 0, 0]).unwrap();
        let options = PictureOptions {
            background: [10, 20, 30],
            ..PictureOptions::default()
        };
        for extension in ["ppm", "jpg"] {
            let flat = transform_picture(&clear, &options, extension).unwrap();
            assert_eq!(flat.bytes().unwrap(), [10, 20, 30, 255], "{extension}");
        }
        let kept = transform_picture(&clear, &options, "png").unwrap();
        assert_eq!(kept.bytes().unwrap(), [0, 0, 0, 0]);
        // Float pixels are refused.
        let float = RasterTile2D::new(1, 1, PixelFormat::RgbaF32).unwrap();
        assert!(transform_picture(&float, &options, "png").is_err());
    }

    #[test]
    fn every_text_here_is_in_the_language_database() {
        for filter in Filter::ALL {
            assert!(sue_lang::has_english("io", filter.name()), "{filter:?}");
        }
        for text in [NO_PIXELS, NO_FLOAT, NO_JXL] {
            assert!(sue_lang::has_english("io", text), "{text}");
        }
    }
}
