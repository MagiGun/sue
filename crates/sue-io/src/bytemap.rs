//! The Tier 3 fallback: bytes as pixels, and their entropy.

use sue_core::{CoreError, PixelFormat, RasterTile2D};

/// Turns raw bytes into a `Gray8` byte-map tile `width` columns wide, one pixel
/// per byte. The vector is reused as the pixel buffer (padded with zeros to a
/// whole row), so the bytes are not copied.
///
/// # Errors
/// [`CoreError::InvalidBuffer`] if the map would be taller than `u32::MAX` rows.
pub fn byte_map_tile(mut bytes: Vec<u8>, width: u32) -> Result<RasterTile2D, CoreError> {
    let width = width.max(1);
    let columns = width as usize;
    let rows = bytes.len().div_ceil(columns).max(1);
    let height =
        u32::try_from(rows).map_err(|_| CoreError::InvalidBuffer("byte map is too tall"))?;
    bytes.resize(rows * columns, 0);
    RasterTile2D::from_bytes(width, height, PixelFormat::Gray8, bytes)
}

/// Shannon entropy in bits per byte (`0.0` to `8.0`). Values above about 7.5
/// suggest compressed or encrypted data.
#[must_use]
pub fn shannon_entropy(bytes: &[u8]) -> f32 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0_usize; 256];
    for &byte in bytes {
        counts[usize::from(byte)] += 1;
    }
    let total = bytes.len() as f64;
    let bits: f64 = counts
        .iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f64 / total;
            -p * p.log2()
        })
        .sum();
    bits as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_maps_pad_to_whole_rows_without_copying() {
        let bytes: Vec<u8> = (0..=99).collect();
        let tile = byte_map_tile(bytes, 64).unwrap();
        assert_eq!((tile.width(), tile.height()), (64, 2));
        let pixels = tile.bytes().unwrap();
        assert_eq!(pixels.len(), 128);
        assert_eq!(pixels[99], 99);
        assert_eq!(pixels[100], 0);

        // A buffer that is already a whole number of rows is used in place.
        let exact = vec![7_u8; 128];
        let address = exact.as_ptr();
        let exact_tile = byte_map_tile(exact, 64).unwrap();
        assert_eq!(exact_tile.bytes().unwrap().as_ptr(), address);
    }

    #[test]
    fn empty_input_still_makes_a_tile() {
        let tile = byte_map_tile(Vec::new(), 0).unwrap();
        assert_eq!((tile.width(), tile.height()), (1, 1));
    }

    #[test]
    fn entropy_bounds() {
        assert!(shannon_entropy(&[]).abs() < f32::EPSILON);
        assert!(shannon_entropy(&[42; 1000]).abs() < f32::EPSILON);
        let uniform: Vec<u8> = (0..=255).collect();
        assert!((shannon_entropy(&uniform) - 8.0).abs() < 1e-5);
        assert!((shannon_entropy(b"abab") - 1.0).abs() < 1e-6);
    }
}
