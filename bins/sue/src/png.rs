//! A minimal PNG encoder (stored deflate blocks) for offscreen snapshots.

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut crc = !0_u32;
    for chunk in chunks {
        for &byte in *chunk {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).expect("PNG chunks are under 4 GiB");
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// Encodes tightly packed 8-bit RGBA pixels as a PNG file.
///
/// # Panics
/// If `pixels` is not `width × height × 4` bytes.
#[must_use]
pub fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    assert_eq!(pixels.len(), row * height as usize, "pixel buffer size");
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in pixels.chunks(row.max(1)) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        zlib.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        let last = u8::from(blocks.peek().is_none());
        let length = u16::try_from(block.len()).expect("blocks are at most 65535 bytes");
        zlib.push(last);
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(&[b"123456789"]), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn encodes_a_valid_structure() {
        let png = encode_rgba(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]), "IEND CRC");
    }
}
