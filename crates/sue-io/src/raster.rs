//! PNG and JPEG: decoding with `zune-png` / `zune-jpeg`, and PNG encoding.
//! Decoded images are `Rgba8` arena rasters, like JPEG XL ones.

use sue_core::{PixelFormat, PixelStorage, RasterTile2D};
use zune_core::bit_depth::BitDepth;
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::{DecoderOptions, EncoderOptions};

/// Facts about a decoded PNG or JPEG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterInfo {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Color model of the file, e.g. `RGB + alpha`.
    pub color_model: &'static str,
    /// Bits per sample in the file.
    pub bits_per_sample: u32,
}

/// A decoded image.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedRaster {
    /// `Rgba8` pixels.
    pub tile: RasterTile2D,
    /// What the file said.
    pub info: RasterInfo,
}

fn color_model(colorspace: ColorSpace) -> &'static str {
    match colorspace {
        ColorSpace::Luma => "Gray",
        ColorSpace::LumaA => "Gray + alpha",
        ColorSpace::RGBA | ColorSpace::BGRA | ColorSpace::ARGB => "RGB + alpha",
        ColorSpace::CMYK => "CMYK",
        ColorSpace::YCCK => "YCCK",
        _ => "RGB",
    }
}

/// Widens 1- to 4-channel 8-bit samples to RGBA.
fn to_rgba(samples: &[u8], channels: usize) -> Result<Vec<u8>, String> {
    if !(1..=4).contains(&channels) {
        return Err(format!("unsupported channel count {channels}"));
    }
    Ok(samples
        .chunks_exact(channels)
        .flat_map(|chunk| match *chunk {
            [gray] => [gray, gray, gray, u8::MAX],
            [gray, alpha] => [gray, gray, gray, alpha],
            [r, g, b] => [r, g, b, u8::MAX],
            [r, g, b, a] => [r, g, b, a],
            _ => [0; 4],
        })
        .collect())
}

fn finish(
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    info: RasterInfo,
) -> Result<DecodedRaster, String> {
    let (width, height) = (
        u32::try_from(width).map_err(|_| "the image is too wide".to_owned())?,
        u32::try_from(height).map_err(|_| "the image is too tall".to_owned())?,
    );
    let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    Ok(DecodedRaster { tile, info })
}

/// Decodes a PNG into `Rgba8`. 16-bit images are reduced to 8 bits.
///
/// # Errors
/// A message from the decoder.
pub fn decode_png(bytes: &[u8]) -> Result<DecodedRaster, String> {
    let options = DecoderOptions::default().png_set_strip_to_8bit(true);
    let mut decoder = zune_png::PngDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|error| error.to_string())?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| "the PNG has no header".to_owned())?;
    let colorspace = decoder.colorspace().unwrap_or(ColorSpace::RGB);
    let bits = decoder.depth().map_or(8, |depth| match depth {
        BitDepth::Sixteen => 16,
        _ => 8,
    });
    let pixels = decoder
        .decode()
        .map_err(|error| error.to_string())?
        .u8()
        .ok_or_else(|| "the decoder did not produce 8-bit samples".to_owned())?;
    let rgba = to_rgba(&pixels, colorspace.num_components())?;
    finish(
        width,
        height,
        rgba,
        RasterInfo {
            width: u32::try_from(width).unwrap_or(u32::MAX),
            height: u32::try_from(height).unwrap_or(u32::MAX),
            color_model: color_model(colorspace),
            bits_per_sample: bits,
        },
    )
}

/// Decodes a baseline or progressive JPEG into `Rgba8`.
///
/// # Errors
/// A message from the decoder.
pub fn decode_jpeg(bytes: &[u8]) -> Result<DecodedRaster, String> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|error| error.to_string())?;
    let source = decoder.input_colorspace().unwrap_or(ColorSpace::RGB);
    let pixels = decoder.decode().map_err(|error| error.to_string())?;
    let info = decoder
        .info()
        .ok_or_else(|| "the JPEG has no header".to_owned())?;
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    let channels = pixels.len() / (width * height).max(1);
    let rgba = if channels == 4 {
        pixels
    } else {
        to_rgba(&pixels, channels)?
    };
    finish(
        width,
        height,
        rgba,
        RasterInfo {
            width: u32::from(info.width),
            height: u32::from(info.height),
            color_model: color_model(source),
            bits_per_sample: 8,
        },
    )
}

/// Encodes a raster as a compressed PNG (`Gray8` stays gray; everything
/// else becomes 8-bit RGBA).
///
/// # Errors
/// A message from the encoder.
pub fn encode_png(tile: &RasterTile2D) -> Result<Vec<u8>, String> {
    let (colorspace, pixels): (ColorSpace, std::borrow::Cow<'_, [u8]>) = match tile.pixels() {
        PixelStorage::U8(bytes) if tile.format() == PixelFormat::Gray8 => {
            (ColorSpace::Luma, bytes.into())
        }
        PixelStorage::U8(bytes) => (ColorSpace::RGBA, bytes.into()),
        PixelStorage::F32(samples) => (
            ColorSpace::RGBA,
            samples
                .iter()
                .map(|&sample| (sample.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect::<Vec<u8>>()
                .into(),
        ),
    };
    encode_png_pixels(
        tile.width() as usize,
        tile.height() as usize,
        colorspace,
        &pixels,
    )
}

/// Encodes tightly packed 8-bit RGBA pixels as a compressed PNG.
///
/// # Errors
/// If the buffer is not `width * height * 4` bytes, or the encoder fails.
pub fn encode_png_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, String> {
    encode_png_pixels(width as usize, height as usize, ColorSpace::RGBA, pixels)
}

fn encode_png_pixels(
    width: usize,
    height: usize,
    colorspace: ColorSpace,
    pixels: &[u8],
) -> Result<Vec<u8>, String> {
    let options = EncoderOptions::new(width, height, colorspace, BitDepth::Eight);
    let mut out = Vec::new();
    zune_png::PngEncoder::new(pixels, options)
        .encode(&mut out)
        .map_err(|error| format!("PNG encoder: {error:?}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> RasterTile2D {
        let pixels: Vec<u8> = (0..16_u32 * 9)
            .flat_map(|index| {
                let (x, y) = (index % 16, index / 16);
                [
                    (x * 16) as u8,
                    (y * 28) as u8,
                    200,
                    if x == 0 { 128 } else { 255 },
                ]
            })
            .collect();
        RasterTile2D::from_bytes(16, 9, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn png_round_trips_losslessly() {
        let tile = card();
        let png = encode_png(&tile).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let decoded = decode_png(&png).unwrap();
        assert_eq!(decoded.tile, tile);
        assert_eq!(decoded.info.width, 16);
        assert_eq!(decoded.info.color_model, "RGB + alpha");
        let gray = RasterTile2D::from_bytes(4, 2, PixelFormat::Gray8, vec![7; 8]).unwrap();
        let decoded = decode_png(&encode_png(&gray).unwrap()).unwrap();
        assert_eq!(decoded.info.color_model, "Gray");
        assert_eq!(decoded.tile.bytes().unwrap()[..4], [7, 7, 7, 255]);
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(decode_png(b"\x89PNG\r\n\x1a\nnope").is_err());
        assert!(decode_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 2]).is_err());
        assert!(encode_png_rgba(2, 2, &[0; 3]).is_err());
    }
}
