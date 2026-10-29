//! More picture formats, through the `image` crate: BMP, GIF (its first
//! frame), ICO, PNM, QOI, TGA, TIFF and WebP read into `Rgba8` tiles, and
//! the same formats written from them (WebP losslessly). PNG, JPEG and
//! JPEG XL keep their own codecs in [`crate::raster`] and [`crate::jxl`].

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbaImage};
use sue_core::{PixelFormat, RasterTile2D};

/// A picture format of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PictureFormat {
    /// Windows bitmap.
    Bmp,
    /// GIF (one frame, at most 256 colors).
    Gif,
    /// Windows icon (at most 256 by 256).
    Ico,
    /// Netpbm pixmap (`.ppm`, no transparency).
    Pnm,
    /// The Quite OK Image format.
    Qoi,
    /// Truevision TGA.
    Tga,
    /// TIFF.
    Tiff,
    /// WebP (written losslessly).
    WebP,
}

impl PictureFormat {
    /// Every format, in menu order.
    pub const ALL: [Self; 8] = [
        Self::WebP,
        Self::Tiff,
        Self::Bmp,
        Self::Gif,
        Self::Tga,
        Self::Qoi,
        Self::Ico,
        Self::Pnm,
    ];

    /// The file extension, without the dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Ico => "ico",
            Self::Pnm => "ppm",
            Self::Qoi => "qoi",
            Self::Tga => "tga",
            Self::Tiff => "tiff",
            Self::WebP => "webp",
        }
    }

    /// The format's name, e.g. `WebP`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Bmp => "BMP",
            Self::Gif => "GIF",
            Self::Ico => "ICO",
            Self::Pnm => "PPM",
            Self::Qoi => "QOI",
            Self::Tga => "TGA",
            Self::Tiff => "TIFF",
            Self::WebP => "WebP",
        }
    }

    /// The format of a file extension.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        let extension = extension.to_ascii_lowercase();
        match extension.as_str() {
            "bmp" | "dib" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "ico" => Some(Self::Ico),
            "ppm" | "pgm" | "pbm" | "pnm" | "pam" => Some(Self::Pnm),
            "qoi" => Some(Self::Qoi),
            "tga" => Some(Self::Tga),
            "tif" | "tiff" => Some(Self::Tiff),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    /// `false` for formats that drop transparency.
    #[must_use]
    pub fn keeps_alpha(self) -> bool {
        !matches!(self, Self::Pnm)
    }

    /// The largest side the format allows, if it has a limit.
    #[must_use]
    pub fn max_side(self) -> Option<u32> {
        match self {
            Self::Ico => Some(256),
            Self::Gif => Some(u32::from(u16::MAX)),
            Self::WebP => Some(16_383),
            _ => None,
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Bmp => ImageFormat::Bmp,
            Self::Gif => ImageFormat::Gif,
            Self::Ico => ImageFormat::Ico,
            Self::Pnm => ImageFormat::Pnm,
            Self::Qoi => ImageFormat::Qoi,
            Self::Tga => ImageFormat::Tga,
            Self::Tiff => ImageFormat::Tiff,
            Self::WebP => ImageFormat::WebP,
        }
    }
}

/// Reads a picture in any format of this module into an `Rgba8` tile. The
/// bytes tell the format; `hint` (from the file extension) names it for TGA,
/// which has no signature.
///
/// # Errors
/// If the bytes are not such a picture, or it is damaged.
pub fn decode_picture(bytes: &[u8], hint: Option<PictureFormat>) -> Result<RasterTile2D, String> {
    let format = match image::guess_format(bytes) {
        Ok(format) => format,
        Err(error) => hint
            .map(PictureFormat::image_format)
            .ok_or(error.to_string())?,
    };
    let known = [
        ImageFormat::Bmp,
        ImageFormat::Gif,
        ImageFormat::Ico,
        ImageFormat::Pnm,
        ImageFormat::Qoi,
        ImageFormat::Tga,
        ImageFormat::Tiff,
        ImageFormat::WebP,
    ];
    if !known.contains(&format) {
        return Err(format!("{format:?} pictures are read elsewhere"));
    }
    let picture = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| error.to_string())?
        .into_rgba8();
    let (width, height) = picture.dimensions();
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, picture.into_raw())
        .map_err(|error| error.to_string())
}

/// Every picture format Sue saves, for a "save as type" list: what people
/// read and the extensions, the usual one first.
pub const PICTURE_SAVE_TYPES: &[(&str, &[&str])] = &[
    ("PNG picture", &["png"]),
    ("JPEG XL picture", &["jxl"]),
    ("JPEG picture", &["jpg", "jpeg"]),
    ("WebP picture", &["webp"]),
    ("TIFF picture", &["tiff", "tif"]),
    ("BMP picture", &["bmp"]),
    ("TGA picture", &["tga"]),
    ("GIF picture", &["gif"]),
    ("QOI picture", &["qoi"]),
];

/// Encodes `tile` as the picture format of `extension` (any of
/// [`PICTURE_SAVE_TYPES`]): JPEG at `jpeg_quality` over white, JPEG XL
/// losslessly at `jxl_effort`.
///
/// # Errors
/// If the extension is not a picture format Sue writes, or encoding fails.
pub fn encode_as(
    tile: &RasterTile2D,
    extension: &str,
    jpeg_quality: u8,
    jxl_effort: u8,
) -> Result<Vec<u8>, String> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    match extension.as_str() {
        "png" => crate::encode_png(tile),
        "jpg" | "jpeg" => {
            // JPEG has no transparency: lay the picture over white.
            let mut flat = tile.clone();
            if flat.format() == PixelFormat::Rgba8
                && let Some(pixels) = flat.bytes_mut()
            {
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    let alpha = u32::from(pixel[3]);
                    for channel in &mut pixel[..3] {
                        *channel =
                            ((u32::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
                    }
                    pixel[3] = 255;
                }
            }
            crate::encode_jpeg(&flat, jpeg_quality)
        }
        "jxl" => {
            use crate::JxlPipeline;
            crate::NativeJxlPipeline
                .encode_layers(&[tile], jxl_effort)
                .map_err(|error| error.to_string())
        }
        other => match PictureFormat::from_extension(other) {
            Some(format) => encode_picture(tile, format),
            None => Err(format!("Sue cannot write .{other} pictures")),
        },
    }
}

/// Writes an `Rgba8` (or `Gray8`) tile as `format`.
///
/// # Errors
/// If the tile is too large for the format, or the encoder fails.
pub fn encode_picture(tile: &RasterTile2D, format: PictureFormat) -> Result<Vec<u8>, String> {
    let (width, height) = (tile.width(), tile.height());
    if let Some(max) = format.max_side()
        && width.max(height) > max
    {
        return Err(format!(
            "{} pictures are at most {max} by {max} pixels; make it smaller first",
            format.name()
        ));
    }
    let bytes = tile.bytes().ok_or("the picture has no pixels")?;
    let rgba: Vec<u8> = match tile.format() {
        PixelFormat::Rgba8 => bytes.to_vec(),
        PixelFormat::Gray8 => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        PixelFormat::RgbaF32 => return Err("float pictures are written as PNG or JPEG XL".into()),
    };
    let picture =
        RgbaImage::from_raw(width, height, rgba).ok_or("the pixels do not fit the size")?;
    let picture = if format.keeps_alpha() {
        DynamicImage::ImageRgba8(picture)
    } else {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(picture).into_rgb8())
    };
    let mut out = Cursor::new(Vec::new());
    picture
        .write_to(&mut out, format.image_format())
        .map_err(|error| error.to_string())?;
    Ok(out.into_inner())
}

/// An animated GIF of `frames` (RGBA8, all the same size), each shown
/// `delay_ms` milliseconds, looping forever.
///
/// # Errors
/// If there are no frames, they differ in size or format, or encoding
/// fails.
pub fn encode_gif_animation(frames: &[RasterTile2D], delay_ms: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};
    let first = frames.first().ok_or("an animation needs frames")?;
    let (width, height) = (first.width(), first.height());
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(|error| error.to_string())?;
        for tile in frames {
            if tile.width() != width
                || tile.height() != height
                || tile.format() != PixelFormat::Rgba8
            {
                return Err("every frame must be the same size of RGBA pixels".to_owned());
            }
            let pixels = tile.bytes().ok_or("a frame has no pixels")?.to_vec();
            let image =
                RgbaImage::from_raw(width, height, pixels).ok_or("a frame's pixels do not fit")?;
            let frame =
                Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(delay_ms.max(1), 1));
            encoder
                .encode_frame(frame)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_animated_gif_keeps_every_frame() {
        use image::AnimationDecoder;
        let frame = |shade: u8| {
            RasterTile2D::from_bytes(4, 3, PixelFormat::Rgba8, [shade, 0, 0, 255].repeat(12))
                .unwrap()
        };
        let bytes = encode_gif_animation(&[frame(0), frame(120), frame(250)], 80).unwrap();
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[1].delay().numer_denom_ms(), (80, 1));
        assert!(frames[2].buffer().get_pixel(0, 0)[0] > 200);
        assert!(encode_gif_animation(&[], 80).is_err());
        let other = RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, vec![0; 16]).unwrap();
        assert!(encode_gif_animation(&[frame(0), other], 80).is_err());
    }

    #[test]
    fn every_save_type_writes_a_picture_that_reads_back() {
        let mut tile = RasterTile2D::new(8, 6, PixelFormat::Rgba8).unwrap();
        for (index, byte) in tile.bytes_mut().unwrap().iter_mut().enumerate() {
            *byte = if index % 4 == 3 {
                255
            } else {
                (index * 7) as u8
            };
        }
        for (_, extensions) in PICTURE_SAVE_TYPES {
            let bytes = encode_as(&tile, extensions[0], 90, 3)
                .unwrap_or_else(|error| panic!("{}: {error}", extensions[0]));
            assert!(bytes.len() > 16, "{}", extensions[0]);
        }
        let png = encode_as(&tile, "PNG", 90, 3).unwrap();
        assert_eq!(crate::decode_png(&png).unwrap().tile.width(), 8);
        assert!(encode_as(&tile, "doc", 90, 3).is_err());
    }

    /// Eight by six pixels of four opaque colors and a see-through corner.
    fn tile(alpha: bool) -> RasterTile2D {
        let mut pixels = Vec::new();
        for y in 0..6_u32 {
            for x in 0..8_u32 {
                let color: [u8; 4] = match (x < 4, y < 3) {
                    (true, true) => [255, 0, 0, 255],
                    (false, true) => [0, 200, 0, 255],
                    (true, false) => [0, 0, 255, 255],
                    (false, false) => [250, 250, 250, 255],
                };
                let see_through = alpha && x == 0 && y == 0;
                pixels.extend(if see_through { [255, 0, 0, 0] } else { color });
            }
        }
        RasterTile2D::from_bytes(8, 6, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn every_format_round_trips_its_pixels() {
        for format in PictureFormat::ALL {
            let alpha = format.keeps_alpha() && format != PictureFormat::Gif;
            let source = tile(alpha);
            let bytes = encode_picture(&source, format).unwrap_or_else(|error| {
                panic!("{} did not encode: {error}", format.name());
            });
            let back = decode_picture(&bytes, Some(format)).unwrap_or_else(|error| {
                panic!("{} did not decode: {error}", format.name());
            });
            assert_eq!((back.width(), back.height()), (8, 6), "{}", format.name());
            let (a, b) = (source.bytes().unwrap(), back.bytes().unwrap());
            let worst = a
                .iter()
                .zip(b)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            // GIF keeps a palette, so its colors may shift a little.
            let allowed = if format == PictureFormat::Gif { 8 } else { 0 };
            assert!(
                worst <= allowed,
                "{} changed a pixel by {worst}",
                format.name()
            );
            assert_eq!(
                PictureFormat::from_extension(format.extension()),
                Some(format)
            );
        }
    }

    #[test]
    fn icons_refuse_pictures_larger_than_they_allow() {
        let big = RasterTile2D::new(300, 10, PixelFormat::Rgba8).unwrap();
        let error = encode_picture(&big, PictureFormat::Ico).unwrap_err();
        assert!(error.contains("256"));
        assert!(decode_picture(b"not a picture", None).is_err());
    }
}
