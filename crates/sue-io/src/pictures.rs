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
    ("ICO picture", &["ico"]),
    ("PPM picture", &["ppm"]),
];

/// `tile` laid over white, for the formats that hold no transparency.
fn over_white(tile: &RasterTile2D) -> RasterTile2D {
    let mut flat = tile.clone();
    if flat.format() == PixelFormat::Rgba8
        && let Some(pixels) = flat.bytes_mut()
    {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
    flat
}

/// Encodes `tile` as the picture format of `extension` (any of
/// [`PICTURE_SAVE_TYPES`]): JPEG at `jpeg_quality` over white, JPEG XL
/// losslessly at `jxl_effort`. A PPM holds no transparency either and is
/// laid over white like a JPEG.
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
        "jpg" | "jpeg" => crate::encode_jpeg(&over_white(tile), jpeg_quality),
        "jxl" => {
            use crate::JxlPipeline;
            crate::NativeJxlPipeline
                .encode_layers(&[tile], jxl_effort)
                .map_err(|error| error.to_string())
        }
        other => match PictureFormat::from_extension(other) {
            Some(format) if !format.keeps_alpha() => encode_picture(&over_white(tile), format),
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

/// An animated PNG (APNG, Clip Studio Paint's animated sticker) of
/// `frames` (same-size RGBA8 tiles), `delay_ms` apart, looping forever.
/// Each frame is a whole PNG image; viewers that know no APNG show the
/// first.
///
/// # Errors
/// If there are no frames, they differ in size, or a frame does not encode.
pub fn encode_apng(frames: &[RasterTile2D], delay_ms: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    let first = frames.first().ok_or("an animation needs frames")?;
    let (width, height) = (first.width(), first.height());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut sequence = 0_u32;
    let delay = u16::try_from(delay_ms.max(1)).unwrap_or(u16::MAX);
    for (index, tile) in frames.iter().enumerate() {
        if tile.width() != width || tile.height() != height || tile.format() != PixelFormat::Rgba8 {
            return Err("every frame must be the same size of RGBA pixels".to_owned());
        }
        let pixels = tile.bytes().ok_or("a frame has no pixels")?;
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|error| error.to_string())?;
        let chunks = png_chunks(&png)?;
        if index == 0 {
            let header = chunks
                .iter()
                .find(|(kind, _)| kind == b"IHDR")
                .ok_or("a PNG without a header")?;
            put_chunk(&mut out, b"IHDR", &header.1);
            let mut control = Vec::new();
            control.extend((frames.len() as u32).to_be_bytes());
            control.extend(0_u32.to_be_bytes());
            put_chunk(&mut out, b"acTL", &control);
        }
        let mut control = Vec::new();
        control.extend(sequence.to_be_bytes());
        control.extend(width.to_be_bytes());
        control.extend(height.to_be_bytes());
        control.extend(0_u32.to_be_bytes());
        control.extend(0_u32.to_be_bytes());
        control.extend(delay.to_be_bytes());
        control.extend(1000_u16.to_be_bytes());
        // Each frame replaces the last whole.
        control.extend([0, 0]);
        put_chunk(&mut out, b"fcTL", &control);
        sequence += 1;
        for (kind, data) in chunks.iter().filter(|(kind, _)| kind == b"IDAT") {
            if index == 0 {
                put_chunk(&mut out, kind, data);
            } else {
                let mut frame = sequence.to_be_bytes().to_vec();
                frame.extend(data);
                put_chunk(&mut out, b"fdAT", &frame);
                sequence += 1;
            }
        }
    }
    put_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// A PNG chunk: its kind and data.
type Chunk = ([u8; 4], Vec<u8>);

/// An animated WebP of `frames` (same-size RGBA8 tiles), `delay_ms`
/// apart, looping forever, each frame lossless.
///
/// # Errors
/// If there are no frames, they differ in size, or a frame does not encode.
pub fn encode_webp_animation(frames: &[RasterTile2D], delay_ms: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::webp::WebPEncoder;
    let first = frames.first().ok_or("an animation needs frames")?;
    let (width, height) = (first.width(), first.height());
    let three = |value: u32| {
        let bytes = value.to_le_bytes();
        [bytes[0], bytes[1], bytes[2]]
    };
    let mut body = b"WEBP".to_vec();
    // The canvas: animated, with alpha.
    let mut header = vec![0b0001_0010, 0, 0, 0];
    header.extend(three(width - 1));
    header.extend(three(height - 1));
    riff_chunk(&mut body, b"VP8X", &header);
    // A clear background, looping forever.
    riff_chunk(&mut body, b"ANIM", &[0, 0, 0, 0, 0, 0]);
    for tile in frames {
        if tile.width() != width || tile.height() != height || tile.format() != PixelFormat::Rgba8 {
            return Err("every frame must be the same size of RGBA pixels".to_owned());
        }
        let pixels = tile.bytes().ok_or("a frame has no pixels")?;
        let mut still = Vec::new();
        WebPEncoder::new_lossless(&mut still)
            .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|error| error.to_string())?;
        // The still picture's VP8L chunk, as it is, inside the frame.
        let bitstream = riff_chunks(still.get(12..).ok_or("a WebP too short")?)
            .into_iter()
            .find(|(kind, _)| kind == b"VP8L")
            .ok_or("a WebP frame without its picture")?;
        let mut frame = Vec::new();
        frame.extend(three(0));
        frame.extend(three(0));
        frame.extend(three(width - 1));
        frame.extend(three(height - 1));
        frame.extend(three(delay_ms.clamp(1, 0xFF_FFFF)));
        // No blending, so each frame replaces the last whole.
        frame.push(0b0000_0010);
        riff_chunk(&mut frame, b"VP8L", &bitstream.1);
        riff_chunk(&mut body, b"ANMF", &frame);
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

/// Appends a RIFF chunk, padded to an even length.
fn riff_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend(kind);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

/// The RIFF chunks in `data`: kind and contents.
fn riff_chunks(data: &[u8]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut at = 0;
    while at + 8 <= data.len() {
        let kind = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        let length =
            u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let Some(contents) = data.get(at + 8..at + 8 + length) else {
            break;
        };
        chunks.push((kind, contents.to_vec()));
        at += 8 + length + length % 2;
    }
    chunks
}

/// A PNG's chunks.
fn png_chunks(png: &[u8]) -> Result<Vec<Chunk>, String> {
    let mut chunks = Vec::new();
    let mut at = 8;
    while at + 12 <= png.len() {
        let length = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let kind = [png[at + 4], png[at + 5], png[at + 6], png[at + 7]];
        let data = png
            .get(at + 8..at + 8 + length)
            .ok_or("a PNG chunk runs past the end")?;
        chunks.push((kind, data.to_vec()));
        at += 12 + length;
    }
    Ok(chunks)
}

/// Appends a PNG chunk with its length and CRC.
fn put_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    out.extend(crc32(kind, data).to_be_bytes());
}

/// The CRC-32 PNG chunks carry, over the kind and the data.
fn crc32(kind: &[u8], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// An animated GIF of `frames` (RGBA8, all the same size), each shown
/// `delay_ms` milliseconds, looping forever.
///
/// # Errors
/// If there are no frames, they differ in size or format, or encoding
/// fails.
pub fn encode_gif_animation(frames: &[RasterTile2D], delay_ms: u32) -> Result<Vec<u8>, String> {
    encode_gif_animation_looping(frames, delay_ms, None)
}

/// An animated GIF of `frames`, each shown `delay_ms`, played `loops`
/// times (`None` for ever).
///
/// # Errors
/// No frames, frames of different sizes or not RGBA8, or the encoder
/// failing.
pub fn encode_gif_animation_looping(
    frames: &[RasterTile2D],
    delay_ms: u32,
    loops: Option<u16>,
) -> Result<Vec<u8>, String> {
    encode_gif_animation_with(frames, delay_ms, loops, false)
}

/// [`encode_gif_animation_looping`], each frame dithered first when
/// `dither` is set: its colors spread by Floyd and Steinberg's error
/// diffusion over a fixed palette of 252 colors (6 reds, 7 greens, 6
/// blues), so smooth shading shows as fine dots instead of bands. Clear
/// pixels stay clear.
///
/// # Errors
/// No frames, frames of different sizes or not RGBA8, or the encoder
/// failing.
pub fn encode_gif_animation_with(
    frames: &[RasterTile2D],
    delay_ms: u32,
    loops: Option<u16>,
    dither: bool,
) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};
    let first = frames.first().ok_or("an animation needs frames")?;
    let (width, height) = (first.width(), first.height());
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
        encoder
            .set_repeat(loops.map_or(Repeat::Infinite, Repeat::Finite))
            .map_err(|error| error.to_string())?;
        for tile in frames {
            if tile.width() != width
                || tile.height() != height
                || tile.format() != PixelFormat::Rgba8
            {
                return Err("every frame must be the same size of RGBA pixels".to_owned());
            }
            let mut pixels = tile.bytes().ok_or("a frame has no pixels")?.to_vec();
            if dither {
                dithered(&mut pixels, width as usize, height as usize);
            }
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

/// How many steps each of red, green and blue has in the dithering
/// palette, less one (6, 7 and 6 levels: 252 colors).
const DITHER_STEPS: [f32; 3] = [5.0, 6.0, 5.0];

/// `pixels` (RGBA8, `width` by `height`) dithered in place onto the fixed
/// palette of [`DITHER_STEPS`] with Floyd and Steinberg's error
/// diffusion; clear pixels are passed over and keep their alpha.
fn dithered(pixels: &mut [u8], width: usize, height: usize) {
    // The error carried to this row and the next, one column of margin
    // on each side.
    let mut this = vec![[0.0_f32; 3]; width + 2];
    let mut next = vec![[0.0_f32; 3]; width + 2];
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) * 4;
            if pixels[at + 3] == 0 {
                continue;
            }
            let mut error = [0.0_f32; 3];
            for channel in 0..3 {
                let want =
                    (f32::from(pixels[at + channel]) + this[x + 1][channel]).clamp(0.0, 255.0);
                let step = 255.0 / DITHER_STEPS[channel];
                let got = (want / step).round() * step;
                pixels[at + channel] = got.round().clamp(0.0, 255.0) as u8;
                error[channel] = want - got;
            }
            for channel in 0..3 {
                this[x + 2][channel] += error[channel] * 7.0 / 16.0;
                next[x][channel] += error[channel] * 3.0 / 16.0;
                next[x + 1][channel] += error[channel] * 5.0 / 16.0;
                next[x + 2][channel] += error[channel] / 16.0;
            }
        }
        std::mem::swap(&mut this, &mut next);
        next.fill([0.0; 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dithering_keeps_the_palette_and_the_shade_and_leaves_clear_pixels_alone() {
        // A gray ramp across 64 columns, 16 rows, with a clear corner.
        let (width, height) = (64_usize, 16_usize);
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let shade = (x * 255 / (width - 1)) as u8;
                let alpha = if x < 2 && y < 2 { 0 } else { 255 };
                pixels.extend_from_slice(&[shade, shade, shade, alpha]);
            }
        }
        let before = pixels.clone();
        dithered(&mut pixels, width, height);
        for (pixel, was) in pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(before.as_chunks::<4>().0)
        {
            assert_eq!(pixel[3], was[3], "alpha is kept");
            if was[3] == 0 {
                assert_eq!(pixel, was, "a clear pixel is left alone");
                continue;
            }
            for channel in 0..3 {
                let step = 255.0 / DITHER_STEPS[channel];
                let on = f32::from(pixel[channel]) / step;
                assert!((on - on.round()).abs() < 0.02, "on the palette: {pixel:?}");
            }
        }
        // Each column's mean is the ramp's shade: the dots average out.
        for x in 4..width - 4 {
            let mean: f32 = (2..height)
                .map(|y| f32::from(pixels[(y * width + x) * 4]))
                .sum::<f32>()
                / (height - 2) as f32;
            let want = f32::from(before[x * 4]);
            assert!((mean - want).abs() < 12.0, "column {x}: {mean} for {want}");
        }
        // More than one level in a column between two palette levels:
        // dots, not a band.
        let column: std::collections::BTreeSet<u8> =
            (2..height).map(|y| pixels[(y * width + 20) * 4]).collect();
        assert!(column.len() > 1, "{column:?}");
    }

    #[test]
    fn a_dithered_gif_reads_back_with_every_frame() {
        use image::AnimationDecoder;
        let ramp: Vec<u8> = (0..32_u32)
            .flat_map(|x| [(x * 8) as u8, 0, 255 - (x * 8) as u8, 255])
            .collect();
        let frame = RasterTile2D::from_bytes(32, 1, PixelFormat::Rgba8, ramp).unwrap();
        let bytes = encode_gif_animation_with(&[frame.clone(), frame], 50, Some(2), true).unwrap();
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(decoder.into_frames().collect_frames().unwrap().len(), 2);
    }

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
    fn an_animated_gif_says_how_often_it_loops() {
        let frame = RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, vec![9; 16]).unwrap();
        // The loop count follows the NETSCAPE2.0 name, little end first;
        // 0 loops for ever.
        let count = |bytes: &[u8]| {
            let at = bytes
                .windows(11)
                .position(|window| window == b"NETSCAPE2.0")
                .expect("a loop extension");
            u16::from_le_bytes([bytes[at + 13], bytes[at + 14]])
        };
        let frames = [frame.clone(), frame];
        assert_eq!(count(&encode_gif_animation(&frames, 50).unwrap()), 0);
        assert_eq!(
            count(&encode_gif_animation_looping(&frames, 50, Some(3)).unwrap()),
            3
        );
        assert_eq!(
            count(&encode_gif_animation_looping(&frames, 50, None).unwrap()),
            0
        );
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

    #[test]
    fn icons_and_pixmaps_are_save_types_and_a_pixmap_lies_over_white() {
        for extension in ["ico", "ppm"] {
            assert!(
                PICTURE_SAVE_TYPES
                    .iter()
                    .any(|(_, extensions)| extensions.contains(&extension)),
                "{extension}"
            );
        }
        // Half see-through red and a clear pixel.
        let tile =
            RasterTile2D::from_bytes(2, 1, PixelFormat::Rgba8, vec![255, 0, 0, 128, 0, 0, 0, 0])
                .unwrap();
        let ppm = encode_as(&tile, "ppm", 90, 3).unwrap();
        let back = decode_picture(&ppm, Some(PictureFormat::Pnm)).unwrap();
        assert_eq!(
            back.bytes().unwrap(),
            [255, 127, 127, 255, 255, 255, 255, 255]
        );
        // An icon keeps its transparency.
        let ico = encode_as(&tile, "ico", 90, 3).unwrap();
        let back = decode_picture(&ico, Some(PictureFormat::Ico)).unwrap();
        assert_eq!(back.bytes().unwrap(), tile.bytes().unwrap());
        // One too large says so in words.
        let large = RasterTile2D::new(300, 2, PixelFormat::Rgba8).unwrap();
        assert!(encode_as(&large, "ico", 90, 3).unwrap_err().contains("256"));
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

    #[test]
    fn animated_webp_holds_every_frame_and_its_timing() {
        use image::AnimationDecoder;
        let frame = |shade: u8| {
            RasterTile2D::from_bytes(5, 3, PixelFormat::Rgba8, vec![shade; 60]).unwrap()
        };
        let frames = [frame(10), frame(120), frame(250)];
        let bytes = encode_webp_animation(&frames, 80).unwrap();
        let decoder =
            image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
        let back: Vec<image::Frame> = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(back.len(), 3);
        for (decoded, shade) in back.iter().zip([10_u8, 120, 250]) {
            assert_eq!(decoded.buffer().get_pixel(0, 0).0, [shade; 4]);
            assert_eq!(decoded.delay().numer_denom_ms(), (80, 1));
        }
        assert!(encode_webp_animation(&[], 80).is_err());
    }

    #[test]
    fn apng_holds_every_frame_and_reads_back_as_its_first() {
        let frame = |shade: u8| {
            RasterTile2D::from_bytes(4, 3, PixelFormat::Rgba8, vec![shade; 48]).unwrap()
        };
        let frames = [frame(10), frame(120), frame(250)];
        let bytes = encode_apng(&frames, 125).unwrap();
        let chunks = png_chunks(&bytes).unwrap();
        let count = |kind: &[u8; 4]| chunks.iter().filter(|(k, _)| k == kind).count();
        assert_eq!(count(b"acTL"), 1);
        assert_eq!(count(b"fcTL"), 3);
        assert!(count(b"fdAT") >= 2);
        let actl = &chunks.iter().find(|(k, _)| k == b"acTL").unwrap().1;
        assert_eq!(&actl[..4], &3_u32.to_be_bytes());
        // Every chunk's CRC checks, so other programs read it.
        let mut at = 8;
        while at < bytes.len() {
            let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let stored =
                u32::from_be_bytes(bytes[at + 8 + length..at + 12 + length].try_into().unwrap());
            assert_eq!(
                stored,
                crc32(&bytes[at + 4..at + 8], &bytes[at + 8..at + 8 + length])
            );
            at += 12 + length;
        }
        // A plain PNG reader sees the first frame.
        let first = crate::decode_png(&bytes).unwrap().tile;
        assert_eq!(first.bytes().unwrap()[0], 10);
        assert!(encode_apng(&[], 100).is_err());
    }
}
