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

#[cfg(test)]
mod tests {
    use super::*;

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
