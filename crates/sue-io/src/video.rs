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

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::PixelFormat;

    fn read(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
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
