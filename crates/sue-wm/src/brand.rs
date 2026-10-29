//! Sue's own icon (`assets/icons`): the app icon for the window and the
//! taskbar, and the silhouette that marks Sue in the interface, tinted to
//! the theme's text color so it stays monochrome in every theme.

use std::sync::OnceLock;

use egui::{Color32, Painter, Pos2, Rect};
use sue_platform::TextureUpload;
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

/// The app icon: a white silhouette on a black rounded square, at every
/// size its `.ico` holds.
pub const APP_ICON: &[u8] = include_bytes!("../../../assets/icons/sue_icon.ico");
/// The silhouette alone, white on clear, to tint.
pub const SILHOUETTE: &[u8] = include_bytes!("../../../assets/icons/sue_silhouette_white.ico");

/// The sizes the icons come in.
const SIDES: [u32; 6] = [16, 32, 48, 64, 128, 256];

/// The PNG of the smallest size in `ico` at least `side` pixels (or its
/// largest), and that size.
#[must_use]
pub fn ico_entry(ico: &[u8], side: u32) -> Option<(&[u8], u32)> {
    let count = usize::from(u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]));
    let mut entries: Vec<(u32, &[u8])> = (0..count)
        .filter_map(|index| {
            let entry = ico.get(6 + index * 16..22 + index * 16)?;
            let width = if entry[0] == 0 {
                256
            } else {
                u32::from(entry[0])
            };
            let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
            let data = ico.get(offset..offset.checked_add(size)?)?;
            data.starts_with(b"\x89PNG").then_some((width, data))
        })
        .collect();
    entries.sort_by_key(|(width, _)| *width);
    let (width, data) = entries
        .iter()
        .find(|(width, _)| *width >= side)
        .or_else(|| entries.last())?;
    Some((data, *width))
}

/// An icon's RGBA8 pixels at `side` pixels or the next size it has, and
/// that side.
#[must_use]
pub fn pixels(ico: &[u8], side: u32) -> Option<(Vec<u8>, u32)> {
    let (png, width) = ico_entry(ico, side)?;
    let options = DecoderOptions::default().png_set_strip_to_8bit(true);
    let mut decoder = zune_png::PngDecoder::new_with_options(ZCursor::new(png), options);
    decoder.decode_headers().ok()?;
    let channels = decoder
        .colorspace()
        .unwrap_or(ColorSpace::RGBA)
        .num_components();
    let samples = decoder.decode().ok()?.u8()?;
    let rgba: Vec<u8> = match channels {
        4 => samples,
        3 => samples
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        2 => samples
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        _ => samples
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
    };
    (rgba.len() == (width * width * 4) as usize).then_some((rgba, width))
}

/// Square RGBA8 pixels and their side.
type Square = (Vec<u8>, u32);

/// The silhouette decoded once per size.
fn silhouette(side: u32) -> Option<&'static Square> {
    static CACHE: [OnceLock<Option<Square>>; 6] = [const { OnceLock::new() }; 6];
    let index = SIDES
        .iter()
        .position(|known| *known >= side)
        .unwrap_or(SIDES.len() - 1);
    CACHE[index]
        .get_or_init(|| pixels(SILHOUETTE, SIDES[index]))
        .as_ref()
}

/// Draws Sue's silhouette fitted in `rect`, in `color`. Returns `false`
/// without a GPU (nothing drawn), so the caller can write "Sue" instead.
pub fn mark(
    painter: &Painter,
    textures: &mut dyn TextureUpload,
    rect: Rect,
    color: Color32,
) -> bool {
    let side = rect.width().min(rect.height());
    let pixels_per_point = painter.ctx().pixels_per_point();
    let Some((pixels, width)) = silhouette((side * pixels_per_point).ceil() as u32) else {
        return false;
    };
    let key = 0x5355_455f_4d41_524b ^ u64::from(*width);
    let Some(texture) = textures.texture(key, 1, [*width, *width], 4, pixels, false) else {
        return false;
    };
    let square = Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
    painter.image(
        texture,
        square,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        color,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icons_decode_at_every_size() {
        for ico in [APP_ICON, SILHOUETTE] {
            for side in SIDES {
                let (rgba, width) = pixels(ico, side).unwrap();
                assert_eq!(width, side);
                assert_eq!(rgba.len(), (side * side * 4) as usize);
            }
            // Between sizes, the next one up; past the largest, the largest.
            assert_eq!(ico_entry(ico, 20).unwrap().1, 32);
            assert_eq!(ico_entry(ico, 999).unwrap().1, 256);
        }
        // The silhouette is white on clear, so tinting gives the theme's color.
        let (rgba, _) = pixels(SILHOUETTE, 64).unwrap();
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] == 255)
                .all(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200)
        );
        assert!(ico_entry(b"nope", 16).is_none());
    }
}
