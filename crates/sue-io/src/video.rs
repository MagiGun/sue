//! Video files Sue writes on its own: Motion JPEG in an AVI container, which
//! the players people already have open (a timelapse, an animation). Each
//! frame is a whole JPEG picture, so frames need no encoder of their own
//! and every one is a key frame.

use sue_core::RasterTile2D;

/// A Motion JPEG AVI being written, frame by frame.
#[derive(Debug)]
pub struct MjpegAvi {
    width: u32,
    height: u32,
    fps: u32,
    /// The `movi` list's chunks so far.
    movi: Vec<u8>,
    /// Each frame's offset in `movi` (from the `movi` tag) and length.
    index: Vec<(u32, u32)>,
    largest: u32,
}

fn put(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    put(out, body.len() as u32);
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

fn list(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(b"LIST");
    put(out, body.len() as u32 + 4);
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
}

impl MjpegAvi {
    /// A video of `width` by `height` pictures, `fps` a second.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            width,
            height,
            fps: fps.max(1),
            movi: Vec::new(),
            index: Vec::new(),
            largest: 0,
        }
    }

    /// Adds a frame already encoded as a JPEG of the video's size.
    pub fn push_jpeg(&mut self, jpeg: &[u8]) {
        // Offsets count from the `movi` tag, which is 4 bytes before.
        let offset = self.movi.len() as u32 + 4;
        chunk(&mut self.movi, b"00dc", jpeg);
        self.index.push((offset, jpeg.len() as u32));
        self.largest = self.largest.max(jpeg.len() as u32);
    }

    /// Adds `tile` as a frame, a JPEG at `quality` over white.
    ///
    /// # Errors
    /// If the tile is not the video's size or will not encode.
    pub fn push_frame(&mut self, tile: &RasterTile2D, quality: u8) -> Result<(), String> {
        if tile.width() != self.width || tile.height() != self.height {
            return Err(format!(
                "every frame is {} by {} pixels",
                self.width, self.height
            ));
        }
        let jpeg = crate::encode_as(tile, "jpg", quality, 1)?;
        self.push_jpeg(&jpeg);
        Ok(())
    }

    /// How many frames it has.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.index.len()
    }

    /// The finished file.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let frames = self.index.len() as u32;
        let mut avih = Vec::with_capacity(56);
        put(&mut avih, 1_000_000 / self.fps);
        put(&mut avih, self.largest.saturating_mul(self.fps));
        put(&mut avih, 0);
        put(&mut avih, 0x10); // It has an index.
        put(&mut avih, frames);
        put(&mut avih, 0);
        put(&mut avih, 1);
        put(&mut avih, self.largest);
        put(&mut avih, self.width);
        put(&mut avih, self.height);
        for _ in 0..4 {
            put(&mut avih, 0);
        }
        let mut strh = Vec::with_capacity(56);
        strh.extend_from_slice(b"vids");
        strh.extend_from_slice(b"MJPG");
        put(&mut strh, 0);
        put(&mut strh, 0); // Priority and language.
        put(&mut strh, 0);
        put(&mut strh, 1); // Scale over rate is the frame time.
        put(&mut strh, self.fps);
        put(&mut strh, 0);
        put(&mut strh, frames);
        put(&mut strh, self.largest);
        put(&mut strh, u32::MAX);
        put(&mut strh, 0);
        let (w, h) = (
            self.width.min(0x7FFF) as u16,
            self.height.min(0x7FFF) as u16,
        );
        for value in [0_u16, 0, w, h] {
            strh.extend_from_slice(&value.to_le_bytes());
        }
        let mut strf = Vec::with_capacity(40);
        put(&mut strf, 40);
        put(&mut strf, self.width);
        put(&mut strf, self.height);
        strf.extend_from_slice(&1_u16.to_le_bytes());
        strf.extend_from_slice(&24_u16.to_le_bytes());
        strf.extend_from_slice(b"MJPG");
        put(
            &mut strf,
            self.width.saturating_mul(self.height).saturating_mul(3),
        );
        for _ in 0..4 {
            put(&mut strf, 0);
        }
        let mut strl = Vec::new();
        chunk(&mut strl, b"strh", &strh);
        chunk(&mut strl, b"strf", &strf);
        let mut hdrl = Vec::new();
        chunk(&mut hdrl, b"avih", &avih);
        list(&mut hdrl, b"strl", &strl);
        let mut idx1 = Vec::with_capacity(self.index.len() * 16);
        for (offset, length) in &self.index {
            idx1.extend_from_slice(b"00dc");
            put(&mut idx1, 0x10); // A key frame.
            put(&mut idx1, *offset);
            put(&mut idx1, *length);
        }
        let mut body = Vec::with_capacity(self.movi.len() + idx1.len() + 512);
        body.extend_from_slice(b"AVI ");
        list(&mut body, b"hdrl", &hdrl);
        list(&mut body, b"movi", &self.movi);
        chunk(&mut body, b"idx1", &idx1);
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend_from_slice(b"RIFF");
        put(&mut out, body.len() as u32);
        out.extend_from_slice(&body);
        out
    }
}

/// The most frames [`decode_frames`] reads from one file.
pub const MAX_FRAMES: usize = 3_000;

/// The frames of a movie or an animated picture, each a straight RGBA8
/// tile, and its frames per second: animated GIF, animated PNG, animated
/// WebP and Motion JPEG AVI (as Sue writes it). At most [`MAX_FRAMES`].
///
/// # Errors
/// If the file is none of these or cannot be read.
pub fn decode_frames(bytes: &[u8]) -> Result<(Vec<RasterTile2D>, f64), String> {
    use image::AnimationDecoder;
    use std::io::Cursor;
    let collect = |frames: image::Frames<'_>| -> Result<(Vec<RasterTile2D>, f64), String> {
        let mut tiles = Vec::new();
        let mut total_ms = 0.0;
        for frame in frames.take(MAX_FRAMES) {
            let frame = frame.map_err(|error| error.to_string())?;
            let (numer, denom) = frame.delay().numer_denom_ms();
            total_ms += f64::from(numer) / f64::from(denom.max(1));
            let buffer = frame.into_buffer();
            let (width, height) = buffer.dimensions();
            tiles.push(
                RasterTile2D::from_bytes(
                    width,
                    height,
                    sue_core::PixelFormat::Rgba8,
                    buffer.into_raw(),
                )
                .map_err(|error| error.to_string())?,
            );
        }
        if tiles.is_empty() {
            return Err("the file has no frames".to_owned());
        }
        let each = total_ms / tiles.len() as f64;
        let fps = if each > 0.0 {
            (1000.0 / each).clamp(1.0, 120.0)
        } else {
            12.0
        };
        Ok((tiles, fps))
    };
    if bytes.starts_with(b"GIF8") {
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        return collect(decoder.into_frames());
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        if decoder.is_apng().map_err(|error| error.to_string())? {
            let frames = decoder
                .apng()
                .map_err(|error| error.to_string())?
                .into_frames();
            return collect(frames);
        }
        let tile = crate::decode_png(bytes)?.tile;
        return Ok((vec![tile], 12.0));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        return collect(decoder.into_frames());
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ") {
        return avi_frames(bytes);
    }
    Err("Sue reads movies as animated GIF, PNG or WebP, or Motion JPEG AVI".to_owned())
}

/// The JPEG frames of a Motion JPEG AVI and its frame rate.
fn avi_frames(bytes: &[u8]) -> Result<(Vec<RasterTile2D>, f64), String> {
    let read = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let mut fps = 12.0;
    if let Some(avih) = bytes.windows(4).position(|tag| tag == b"avih")
        && let Some(micros) = read(avih + 8).filter(|micros| *micros > 0)
    {
        fps = (1_000_000.0 / f64::from(micros)).clamp(1.0, 120.0);
    }
    let movi = bytes
        .windows(4)
        .position(|tag| tag == b"movi")
        .ok_or("the AVI has no frames")?;
    let mut at = movi + 4;
    let mut tiles = Vec::new();
    while at + 8 <= bytes.len() && tiles.len() < MAX_FRAMES {
        let tag = &bytes[at..at + 4];
        if tag == b"idx1" {
            break;
        }
        let size = read(at + 4).ok_or("the AVI is cut short")? as usize;
        let body = bytes
            .get(at + 8..at + 8 + size)
            .ok_or("the AVI is cut short")?;
        if tag == b"LIST" {
            // A list of chunks: step inside it.
            at += 12;
            continue;
        }
        if tag[2..] == *b"dc" || tag[2..] == *b"db" {
            if body.starts_with(&[0xFF, 0xD8]) {
                tiles.push(crate::decode_jpeg(body)?.tile);
            } else {
                return Err("only Motion JPEG AVI movies are read".to_owned());
            }
        }
        at += 8 + size + size % 2;
    }
    if tiles.is_empty() {
        return Err("the AVI has no frames".to_owned());
    }
    Ok((tiles, fps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::PixelFormat;

    fn read(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn movies_and_animations_read_back_frame_by_frame() {
        let frame = |shade: u8| {
            RasterTile2D::from_bytes(
                8,
                6,
                PixelFormat::Rgba8,
                [shade, shade, shade, 255].repeat(48),
            )
            .unwrap()
        };
        let mut video = MjpegAvi::new(8, 6, 10);
        for shade in [0_u8, 255] {
            video.push_frame(&frame(shade), 95).unwrap();
        }
        let (frames, fps) = decode_frames(&video.finish()).unwrap();
        assert_eq!(frames.len(), 2);
        assert!((fps - 10.0).abs() < 0.01);
        assert!(frames[1].bytes().unwrap()[0] > 200, "the white frame");
        let gif = crate::encode_gif_animation(&[frame(0), frame(255), frame(0)], 50).unwrap();
        let (frames, fps) = decode_frames(&gif).unwrap();
        assert_eq!(frames.len(), 3);
        assert!((fps - 20.0).abs() < 0.5);
        assert!(decode_frames(b"not a movie").is_err());
    }

    #[test]
    fn frames_become_an_indexed_motion_jpeg_avi() {
        let mut video = MjpegAvi::new(32, 16, 12);
        for shade in [0_u8, 128, 255] {
            let tile = RasterTile2D::from_bytes(
                32,
                16,
                PixelFormat::Rgba8,
                [shade, shade, shade, 255].repeat(32 * 16),
            )
            .unwrap();
            video.push_frame(&tile, 80).unwrap();
        }
        let wrong = RasterTile2D::new(8, 8, PixelFormat::Rgba8).unwrap();
        assert!(
            video.push_frame(&wrong, 80).is_err(),
            "every frame the same size"
        );
        assert_eq!(video.frames(), 3);
        let bytes = video.finish();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(read(&bytes, 4) as usize, bytes.len() - 8);
        assert_eq!(&bytes[8..12], b"AVI ");
        // The main header: frame time, frame count, size.
        let avih = bytes.windows(4).position(|tag| tag == b"avih").unwrap() + 8;
        assert_eq!(read(&bytes, avih), 1_000_000 / 12);
        assert_eq!(read(&bytes, avih + 16), 3);
        assert_eq!((read(&bytes, avih + 32), read(&bytes, avih + 36)), (32, 16));
        // The index points at each frame, a JPEG.
        let movi = bytes.windows(4).position(|tag| tag == b"movi").unwrap();
        let idx1 = bytes.windows(4).rposition(|tag| tag == b"idx1").unwrap();
        assert_eq!(read(&bytes, idx1 + 4), 3 * 16);
        for entry in 0..3 {
            let at = idx1 + 8 + entry * 16;
            assert_eq!(&bytes[at..at + 4], b"00dc");
            let frame = movi + read(&bytes, at + 8) as usize;
            assert_eq!(&bytes[frame..frame + 4], b"00dc");
            assert_eq!(
                &bytes[frame + 8..frame + 10],
                [0xFF, 0xD8],
                "a JPEG starts there"
            );
            let decoded =
                crate::decode_jpeg(&bytes[frame + 8..frame + 8 + read(&bytes, at + 12) as usize])
                    .unwrap();
            assert_eq!((decoded.tile.width(), decoded.tile.height()), (32, 16));
        }
    }
}
