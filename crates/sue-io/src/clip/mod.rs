//! Clip Studio Paint files (`.clip`), read and written with their layers,
//! folders, blend modes, opacity, visibility and clipping, as the same
//! [`Psd`] records Photoshop documents use.
//!
//! A `.clip` is a chunk file: a `CSFCHUNK` header, a `Head` chunk, one
//! `Exta` chunk per stored bitmap, an SQLite database (`SQLi`) that
//! describes the canvas and its layers, and an empty `Foot`. A layer's
//! pixels are reached through the database (layer, mipmap, mipmap level,
//! offscreen) and sit in an `Exta` chunk as 256 by 256 tiles, each zlib
//! compressed as an alpha plane followed by BGRx.
//!
//! Correction layers and gradient or fill layers, which Clip Studio draws
//! from their settings, come in as empty layers in their places; text and
//! vector layers come in as the pixels Clip Studio last drew for them.
//!
//! A file Sue writes also carries the Sue project of the picture in a
//! `SueProject` table, so Sue opens its own `.clip` files with nothing lost
//! (vector lines, tones, effects), while Clip Studio sees the layers.

mod container;
mod offscreen;
mod read;
mod schema;
mod write;

pub use container::{Chunk, Layout, layout, sniff};
pub use offscreen::{
    AttributePart, BitmapSurvey, Block, BlockList, attribute_parts, survey_bitmap,
};
pub use read::decode_clip;
pub use write::encode_clip;

use crate::psd::Psd;

/// A Clip Studio file read: its layers as Photoshop records, its
/// resolution, and the Sue project inside when Sue wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// The canvas and its layers (bottom first, folders as in [`Psd`]);
    /// `merged` is left empty.
    pub psd: Psd,
    /// Pixels per inch.
    pub dpi: f64,
    /// The `.sue` bytes of the picture, in files Sue wrote.
    pub sue: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::schema::{blend_of, composite_of};
    use super::*;
    use crate::psd::{PsdKind, PsdLayer};

    fn checker(name: &str, left: i32, top: i32, width: u32, height: u32) -> PsdLayer {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let on = (x / 7 + y / 5) % 2 == 0;
                pixels.extend_from_slice(&[
                    (x * 255 / width) as u8,
                    (y * 255 / height) as u8,
                    90,
                    if on { 255 } else { 0 },
                ]);
            }
        }
        PsdLayer::pixels(name, left, top, width, height, pixels)
    }

    /// `layer`'s pixel at (x, y) of the canvas, or clear.
    fn at(layer: &PsdLayer, x: i32, y: i32) -> [u8; 4] {
        let (lx, ly) = (x - layer.left, y - layer.top);
        if lx < 0 || ly < 0 || lx >= layer.width as i32 || ly >= layer.height as i32 {
            return [0; 4];
        }
        let o = (ly as usize * layer.width as usize + lx as usize) * 4;
        layer.pixels[o..o + 4].try_into().unwrap()
    }

    #[test]
    fn layers_and_folders_round_trip_through_a_clip_file() {
        let mut square = checker("Square", 200, 100, 300, 280);
        square.opacity = 200;
        square.blend = *b"mul ";
        square.clip = true;
        let mut hidden = checker("Hidden \u{65e5}\u{672c}", -40, -30, 90, 60);
        hidden.visible = false;
        let mut folder = PsdLayer::folder("Inks", PsdKind::FolderOpen { open: false });
        folder.opacity = 128;
        let psd = Psd {
            width: 600,
            height: 400,
            layers: vec![
                checker("Back", 0, 0, 600, 400),
                PsdLayer::folder("</Layer group>", PsdKind::FolderEnd),
                hidden.clone(),
                square.clone(),
                folder,
                PsdLayer::pixels("Empty", 0, 0, 0, 0, Vec::new()),
            ],
            merged: Vec::new(),
        };
        let bytes = encode_clip(&psd, 350.0, Some(b"SUE project")).unwrap();
        assert!(sniff(&bytes));
        let clip = decode_clip(&bytes).unwrap();
        assert_eq!(
            (clip.psd.width, clip.psd.height, clip.dpi),
            (600, 400, 350.0)
        );
        assert_eq!(clip.sue.as_deref(), Some(b"SUE project".as_slice()));
        let names: Vec<&str> = clip.psd.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Back",
                "</Layer group>",
                "Hidden \u{65e5}\u{672c}",
                "Square",
                "Inks",
                "Empty"
            ]
        );
        let back = &clip.psd.layers;
        assert_eq!(back[4].kind, PsdKind::FolderOpen { open: false });
        assert_eq!(back[4].opacity, 128);
        assert_eq!(
            (back[3].opacity, back[3].blend, back[3].clip),
            (200, *b"mul ", true)
        );
        assert!(!back[2].visible);
        // Every pixel lands where it was, cut to the canvas.
        for (original, read) in [
            (&psd.layers[0], &back[0]),
            (&square, &back[3]),
            (&hidden, &back[2]),
        ] {
            for (x, y) in [
                (0, 0),
                (1, 0),
                (7, 5),
                (250, 150),
                (499, 379),
                (599, 399),
                (10, 10),
                (45, 25),
            ] {
                let want = at(original, x, y);
                let got = at(read, x, y);
                if want[3] == 0 {
                    assert_eq!(got[3], 0, "{} at {x},{y}", original.name);
                } else {
                    assert_eq!(got, want, "{} at {x},{y}", original.name);
                }
            }
        }
        assert!(back[2].left >= 0 && back[2].top >= 0);
    }

    #[test]
    fn broken_files_are_refused_with_a_reason() {
        assert!(decode_clip(b"CSFCHUNK").is_err());
        assert!(decode_clip(b"8BPS").is_err());
        let psd = Psd {
            width: 10,
            height: 10,
            layers: vec![checker("A", 0, 0, 10, 10)],
            merged: Vec::new(),
        };
        let mut bytes = encode_clip(&psd, 72.0, None).unwrap();
        let cut = bytes.len() - 40;
        bytes.truncate(cut);
        assert!(decode_clip(&bytes).is_err());
        assert!(encode_clip(&Psd { width: 0, ..psd }, 72.0, None).is_err());
    }

    #[test]
    fn blend_modes_map_both_ways() {
        for key in [
            *b"norm", *b"mul ", *b"scrn", *b"over", *b"diff", *b"lum ", *b"pass",
        ] {
            assert_eq!(blend_of(composite_of(key)), key);
        }
        assert_eq!(blend_of(999), *b"norm");
    }
}
