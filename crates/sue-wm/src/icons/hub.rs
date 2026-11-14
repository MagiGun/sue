//! Group D: the icons of the Hub and of the apps: the studios, the
//! places and buttons of the Hub, and the kinds of file it shows.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// An icon that is one line and nothing else.
const fn line(keyline: Keyline, line: &'static [Part]) -> Art {
    Art {
        keyline,
        line,
        solid: &[],
    }
}

/// The outline of the palette: a round board with a bite taken out of
/// its lower right, where the hand holds it.
const PALETTE: &str = "M15.16 20.69 A9.25 9.25 0 1 1 20.69 15.16 A4.4 4.4 0 0 0 15.16 20.69 Z";

pub(super) const APP_PAINT: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(PALETTE),
        Part::Fill(
            "M5.68 10.64 A1.25 1.25 0 0 1 8.18 10.64 A1.25 1.25 0 0 1 5.68 10.64 Z M8.125 7.45 \
             A1.25 1.25 0 0 1 10.625 7.45 A1.25 1.25 0 0 1 8.125 7.45 Z M12.11 6.93 A1.25 1.25 0 \
             0 1 14.61 6.93 A1.25 1.25 0 0 1 12.11 6.93 Z M15.3 9.375 A1.25 1.25 0 0 1 17.8 9.375 \
             A1.25 1.25 0 0 1 15.3 9.375 Z",
        ),
    ],
);

/// The vane of the quill: a long feather leaning from its foot at the
/// lower left up to its tip at the top right.
const VANE: &str = "M8.75 15.25 C7.75 8.5 13 3.75 20.25 3.75 C20.25 11 15.5 16.25 8.75 15.25 Z";

pub(super) const APP_WRITER: Art = line(
    Keyline::Square,
    &[Part::Stroke(VANE), Part::Stroke("M3.75 20.25 L14.25 9.75")],
);

/// The blank canvas on the easel of [`APP_GENERIC`].
const CANVAS: &str = "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V12.25 A2 2 0 0 1 17.25 14.25 H6.75 \
                      A2 2 0 0 1 4.75 12.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

pub(super) const APP_GENERIC: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(CANVAS),
        Part::Stroke("M8.75 14.25 L6.25 21.25 M15.25 14.25 L17.75 21.25 M12 14.25 V18.25"),
    ],
);

/// A tall sheet with rounded corners and no fold.
const SHEET: &str = "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 \
                     A2 2 0 0 1 4.75 19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

pub(super) const TEMPLATE: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(SHEET),
        Part::Stroke(
            "M9.75 5.75 H14.25 A1 1 0 0 1 15.25 6.75 V8.75 A1 1 0 0 1 14.25 9.75 H9.75 A1 1 0 0 1 \
             8.75 8.75 V6.75 A1 1 0 0 1 9.75 5.75 Z",
        ),
        Part::Stroke("M8.75 14.25 H15.25 M8.75 17.25 H12.75"),
    ],
);

/// The ring of the lifebuoy that [`RECOVER`] is.
const BUOY: &str = "M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z M8 12 A4 4 0 0 1 \
                    16 12 A4 4 0 0 1 8 12 Z";

pub(super) const RECOVER: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(BUOY),
        Part::Stroke(
            "M5.46 5.46 L9.17 9.17 M18.54 5.46 L14.83 9.17 M5.46 18.54 L9.17 14.83 M18.54 18.54 \
             L14.83 14.83",
        ),
    ],
);

/// A closed folder, as the reference icon draws it.
const FOLDER: &str = "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H19.25 A2 2 0 0 1 21.25 9.75 \
                      V17.25 A2 2 0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z";

/// The arrow that leaves the folder of [`FOLDER_REVEAL`].
const OUT: &str = "M13.25 11.75 L20.25 4.75 M15.25 4.75 H20.25 V9.75";

pub(super) const FOLDER_REVEAL: Art = line(
    Keyline::Wide,
    &[Part::Stroke(FOLDER), Part::Clear(OUT), Part::Stroke(OUT)],
);

/// A wide window with rounded corners.
const WINDOW: &str = "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 \
                      A2 2 0 0 1 2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 Z";

pub(super) const LAYOUT_STYLE: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(WINDOW),
        Part::Stroke("M2.75 8.75 H21.25 M7.75 8.75 V19.25 M16.25 8.75 V19.25"),
    ],
);

/// The play mark in the frame of [`FILM`].
const FRAME_PLAY: &str = "M9.75 9.25 L14.25 12 L9.75 14.75 Z";

pub(super) const FILM: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(WINDOW),
        Part::Stroke("M6.75 4.75 V19.25 M17.25 4.75 V19.25"),
        Part::Stroke("M2.75 9.75 H6.75 M2.75 14.25 H6.75 M17.25 9.75 H21.25 M17.25 14.25 H21.25"),
        Part::Fill(FRAME_PLAY),
        Part::Stroke(FRAME_PLAY),
    ],
);

pub(super) const AUDIO: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 10.25 V13.75 M7.75 7 V17 M12 3.75 V20.25 M16.25 7.75 V16.25 M20.25 10.75 V13.25",
    )],
);

pub(super) const CUBE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 L20.25 7.875 V16.125 L12 20.25 L3.75 16.125 V7.875 Z"),
        Part::Stroke("M3.75 7.875 L12 12 L20.25 7.875 M12 12 V20.25"),
    ],
);

pub(super) const BOOK: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke("M12 7.25 C10.25 5.75 7 5 2.75 5.75 V17.75 C7 17 10.25 17.75 12 19.25"),
        Part::Stroke("M12 7.25 C13.75 5.75 17 5 21.25 5.75 V17.75 C17 17 13.75 17.75 12 19.25"),
        Part::Stroke("M12 7.25 V19.25"),
    ],
);

pub(super) const MAIL: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(WINDOW),
        Part::Stroke("M3.5 6.5 L12 12.75 L20.5 6.5"),
    ],
);

pub(super) const FEEDBACK: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M9.58 18.05 A7.5 7.5 0 1 0 5.7 13.82 L3.75 20.25 Z"),
        Part::Fill(
            "M7.5 11.25 A1 1 0 0 1 9.5 11.25 A1 1 0 0 1 7.5 11.25 Z M11.75 11.25 A1 1 0 0 1 13.75 \
             11.25 A1 1 0 0 1 11.75 11.25 Z M16 11.25 A1 1 0 0 1 18 11.25 A1 1 0 0 1 16 11.25 Z",
        ),
    ],
);

/// The body of the filing cabinet that [`FILES`] is, standing on two
/// short feet.
const CABINET: &str = "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V17.25 A2 2 0 0 1 17.25 19.25 \
                       H6.75 A2 2 0 0 1 4.75 17.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

pub(super) const FILES: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(CABINET),
        Part::Stroke("M4.75 10.75 H19.25"),
        Part::Stroke("M9.75 5.75 V6.75 H14.25 V5.75 M9.75 14.25 V15.25 H14.25 V14.25"),
        Part::Stroke("M7.75 19.25 V21.25 M16.25 19.25 V21.25"),
    ],
);

pub(super) const MATERIALS: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 L16.5 10.75 H7.5 Z"),
        Part::Stroke("M3.75 17 A3.25 3.25 0 0 1 10.25 17 A3.25 3.25 0 0 1 3.75 17 Z"),
        Part::Stroke(
            "M15.25 14.25 H19.25 A1 1 0 0 1 20.25 15.25 V19.25 A1 1 0 0 1 19.25 20.25 H15.25 A1 1 \
             0 0 1 14.25 19.25 V15.25 A1 1 0 0 1 15.25 14.25 Z",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use egui::Color32;

    use super::super::{Icon, Part, REFERENCE, raster, weight};
    use crate::Theme;

    /// The icons of this group, in the order of the table.
    const HUB: [Icon; 15] = [
        Icon::AppPaint,
        Icon::AppWriter,
        Icon::AppGeneric,
        Icon::Template,
        Icon::Recover,
        Icon::FolderReveal,
        Icon::LayoutStyle,
        Icon::Film,
        Icon::Audio,
        Icon::Cube,
        Icon::Book,
        Icon::Mail,
        Icon::Feedback,
        Icon::Materials,
        Icon::Files,
    ];

    /// Drawings being tried, shown after the group on the sheet.
    const TRIALS: &[&[Part]] = &[];

    /// A picture in the making: RGB bytes.
    struct Canvas {
        width: usize,
        height: usize,
        rgb: Vec<u8>,
    }

    impl Canvas {
        fn new(width: usize, height: usize, ground: Color32) -> Self {
            let rgb = [ground.r(), ground.g(), ground.b()].repeat(width * height);
            Self { width, height, rgb }
        }

        /// Lays a tile down at `at` in `ink`, each of its pixels `zoom`
        /// wide.
        fn tile(&mut self, at: [usize; 2], tile: &raster::Tile, zoom: usize, ink: Color32) {
            for (index, alpha) in tile.alpha.iter().enumerate() {
                let cover = f32::from(*alpha) / 255.0;
                let (tx, ty) = (index % tile.size, index / tile.size);
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let (x, y) = (at[0] + tx * zoom + dx, at[1] + ty * zoom + dy);
                        if x >= self.width || y >= self.height {
                            continue;
                        }
                        let pixel = (y * self.width + x) * 3;
                        for (channel, value) in [ink.r(), ink.g(), ink.b()].into_iter().enumerate()
                        {
                            let under = f32::from(self.rgb[pixel + channel]);
                            self.rgb[pixel + channel] =
                                (under + (f32::from(value) - under) * cover).round() as u8;
                        }
                    }
                }
            }
        }

        fn png(&self) -> Vec<u8> {
            use zune_core::bit_depth::BitDepth;
            use zune_core::colorspace::ColorSpace;
            use zune_core::options::EncoderOptions;
            let options =
                EncoderOptions::new(self.width, self.height, ColorSpace::RGB, BitDepth::Eight);
            let mut out = Vec::new();
            zune_png::PngEncoder::new(&self.rgb, options)
                .encode(&mut out)
                .unwrap();
            out
        }
    }

    fn tile(parts: &[Part], pixels: usize) -> raster::Tile {
        raster::render(parts, pixels, weight(pixels as f32)).unwrap()
    }

    /// Writes the contact sheet of this group into the folder the
    /// environment variable `SUE_ICON_SHEET` names, in the colors of the
    /// presets Void (dark) and Paper (light): first every icon of the
    /// group in a row at 16, 24 and 48 pixels, with six reference icons
    /// after a gap to judge the weight against, then each icon at 16 and
    /// 24 with every pixel six wide. Run it while drawing:
    /// `cargo test -p sue-wm hub::tests::sheet -- --ignored`.
    #[test]
    #[ignore = "writes pictures for the eye; set SUE_ICON_SHEET to a folder"]
    fn sheet() {
        let Some(folder) = std::env::var_os("SUE_ICON_SHEET").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&folder).unwrap();
        let base = Theme::from_rice(&sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap());
        let neighbours = [
            REFERENCE[0],
            REFERENCE[2],
            REFERENCE[6],
            REFERENCE[7],
            REFERENCE[1],
            REFERENCE[9],
        ];
        let mut items: Vec<&[Part]> = HUB.iter().map(|icon| icon.art().line).collect();
        items.extend_from_slice(TRIALS);
        let others: Vec<&[Part]> = neighbours.iter().map(|icon| icon.art().line).collect();
        for preset in sue_rice::builtin_presets() {
            if preset.id != "void" && preset.id != "paper" {
                continue;
            }
            let theme = base.with_preset(&preset);
            let (ground, ink) = (theme.panel, theme.text);
            // At their own size: 16, 24 and 48 side by side, seven to a
            // row, the reference icons on a row of their own.
            let (pad, cell, columns) = (16, [128, 64], 7);
            let lines = items.len().div_ceil(columns) + 1;
            let mut canvas = Canvas::new(
                2 * pad + columns * cell[0],
                2 * pad + lines * cell[1],
                ground,
            );
            let place = |canvas: &mut Canvas, index: usize, parts: &[Part]| {
                let x = pad + (index % columns) * cell[0];
                let y = pad + (index / columns) * cell[1];
                let mut left = x;
                for pixels in [16_usize, 24, 48] {
                    canvas.tile([left, y + (48 - pixels) / 2], &tile(parts, pixels), 1, ink);
                    left += pixels + 8;
                }
            };
            for (index, parts) in items.iter().enumerate() {
                place(&mut canvas, index, parts);
            }
            for (index, parts) in others.iter().enumerate() {
                place(&mut canvas, (lines - 1) * columns + index, parts);
            }
            let file = folder.join(format!("p2-7b-{}.png", preset.id));
            std::fs::write(file, canvas.png()).unwrap();

            // Every pixel large: 16 at six times and 24 at four times.
            let (cell, columns) = ([2 * 96 + 3 * pad, 96 + pad], 4);
            let mut zoom = Canvas::new(
                pad + columns * cell[0],
                pad + items.len().div_ceil(columns) * cell[1],
                ground,
            );
            for (index, parts) in items.iter().enumerate() {
                let x = pad + (index % columns) * cell[0];
                let y = pad + (index / columns) * cell[1];
                zoom.tile([x, y], &tile(parts, 16), 6, ink);
                zoom.tile([x + 96 + pad, y], &tile(parts, 24), 4, ink);
            }
            let file = folder.join(format!("p2-7b-{}-zoom.png", preset.id));
            std::fs::write(file, zoom.png()).unwrap();
        }
    }

    /// How many pixels of two tiles differ by more than half.
    fn apart(one: &raster::Tile, other: &raster::Tile) -> usize {
        one.alpha
            .iter()
            .zip(&other.alpha)
            .filter(|(a, b)| a.abs_diff(**b) > 127)
            .count()
    }

    /// Every icon of this group can be told from every other icon of
    /// the set at the smallest size: they differ in at least this many
    /// of the 256 pixels of a tile of 16 (a folder and the same folder
    /// with an arrow leaving it differ in 15).
    #[test]
    fn every_hub_icon_is_told_apart_from_every_other_at_16() {
        let style = super::super::IconStyle::Line;
        for icon in HUB {
            let mine = icon.tile(style, 16, weight(16.0));
            for other in Icon::ALL.iter().filter(|other| **other != icon) {
                let theirs = other.tile(style, 16, weight(16.0));
                assert!(
                    apart(&mine, &theirs) >= 12,
                    "{} and {} are nearly the same picture at 16",
                    icon.name(),
                    other.name()
                );
            }
        }
    }

    /// The table holds every icon of this group, and each one draws
    /// with the art of this file.
    #[test]
    fn the_group_is_whole() {
        let arts = [
            &super::APP_PAINT,
            &super::APP_WRITER,
            &super::APP_GENERIC,
            &super::TEMPLATE,
            &super::RECOVER,
            &super::FOLDER_REVEAL,
            &super::LAYOUT_STYLE,
            &super::FILM,
            &super::AUDIO,
            &super::CUBE,
            &super::BOOK,
            &super::MAIL,
            &super::FEEDBACK,
            &super::MATERIALS,
            &super::FILES,
        ];
        for (icon, art) in HUB.iter().zip(arts) {
            assert_eq!(icon.art(), art, "{}", icon.name());
            assert!(!icon.has_solid(), "{} is no on and off pair", icon.name());
        }
    }
}
