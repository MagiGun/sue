//! Group K: writing and text. The marks of type (bold, italic,
//! underline, struck through), how lines sit on a page (left, center,
//! right, justified, indented, listed), the parts of a manuscript (a
//! page, a binder, a comment, a table, a footnote) and the ways a
//! writer looks at it (a corkboard, an outline).
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else. A line of writing
//! is always a level stroke of the pen, and lines of writing are four
//! units apart from middle to middle where there is room. A capital
//! letter stands on the square keyline, from y 3.75 to 20.25.

use super::{Art, Keyline, Part};

/// An icon drawn as a line, with no solid twin.
const fn line(keyline: Keyline, parts: &'static [Part]) -> Art {
    Art {
        keyline,
        line: parts,
        solid: &[],
    }
}

/// The solid upright of the bold letter: a bar three units wide, its
/// corners rounded by the pen drawn round it.
const BOLD_STEM: &str = "M6.75 3.75 H9.75 V20.25 H6.75 Z";

pub(super) const BOLD: Art = line(
    Keyline::Square,
    &[
        Part::Fill(BOLD_STEM),
        Part::Stroke(BOLD_STEM),
        Part::Stroke(
            "M9.75 3.75 H13.5 A4.125 4.125 0 0 1 13.5 12 H9.75 M9.75 12 H14.25 A4.125 4.125 0 0 1 \
             14.25 20.25 H9.75",
        ),
    ],
);

pub(super) const ITALIC: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M10.75 3.75 H18.25 M5.75 20.25 H13.25 M14.5 3.75 L9.5 20.25",
    )],
);

pub(super) const UNDERLINE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 3.75 V11 A4.25 4.25 0 0 0 16.25 11 V3.75"),
        Part::Stroke("M5.75 20.25 H18.25"),
    ],
);

/// The letter S: a bowl open to the right over a bowl open to the left,
/// joined by a slanted spine.
const LETTER_S: &str = "M16.25 6.5 C15.5 4.75 14 3.75 12 3.75 C9.6 3.75 7.75 5.3 7.75 7.75 C7.75 \
                        12 16.25 11.5 16.25 16.25 C16.25 18.7 14.4 20.25 12 20.25 C10 20.25 8.5 \
                        19.25 7.75 17.5";

pub(super) const STRIKETHROUGH: Art = line(
    Keyline::Square,
    &[Part::Stroke(LETTER_S), Part::Stroke("M3.75 12 H20.25")],
);

pub(super) const ALIGN_LEFT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 5.75 H20.25 M3.75 9.75 H11.25 M3.75 14.25 H20.25 M3.75 18.25 H11.25",
    )],
);

pub(super) const ALIGN_CENTER: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 5.75 H20.25 M7.75 9.75 H16.25 M3.75 14.25 H20.25 M7.75 18.25 H16.25",
    )],
);

pub(super) const ALIGN_RIGHT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 5.75 H20.25 M12.75 9.75 H20.25 M3.75 14.25 H20.25 M12.75 18.25 H20.25",
    )],
);

pub(super) const ALIGN_JUSTIFY: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 5.75 H20.25 M3.75 9.75 H20.25 M3.75 14.25 H20.25 M3.75 18.25 H20.25",
    )],
);

/// The lines of an indented paragraph: a whole line above and below,
/// two short ones between them that start further in.
const INDENTED: &str = "M3.75 5.75 H20.25 M11.75 9.75 H20.25 M11.75 14.25 H20.25 M3.75 18.25 \
                        H20.25";

/// The head pointing right, into the indent.
const INTO: &str = "M3.75 9 L8.75 12 L3.75 15 Z";

/// The head pointing left, out of the indent.
const OUT_OF: &str = "M8.75 9 L3.75 12 L8.75 15 Z";

pub(super) const INDENT_MORE: Art = line(
    Keyline::Square,
    &[Part::Stroke(INDENTED), Part::Fill(INTO), Part::Stroke(INTO)],
);

pub(super) const INDENT_LESS: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(INDENTED),
        Part::Fill(OUT_OF),
        Part::Stroke(OUT_OF),
    ],
);

/// The two items of a list: a line of writing and a shorter one under
/// it, twice, after the room for a mark.
const ITEMS: &str = "M10.75 4.75 H20.25 M10.75 8.75 H16.75 M10.75 15.25 H20.25 M10.75 19.25 \
                     H16.75";

pub(super) const LIST_BULLETS: Art = line(
    Keyline::Square,
    &[
        Part::Fill(
            "M3.5 4.75 A1.75 1.75 0 0 1 7 4.75 A1.75 1.75 0 0 1 3.5 4.75 Z M3.5 15.25 A1.75 1.75 \
             0 0 1 7 15.25 A1.75 1.75 0 0 1 3.5 15.25 Z",
        ),
        Part::Stroke(ITEMS),
    ],
);

pub(super) const LIST_NUMBERS: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M3.75 5.25 L5.75 3.75 V9.75"),
        Part::Stroke("M3.75 15.75 A2 2 0 0 1 7.75 15.75 C7.75 17.5 3.75 18.25 3.75 20.25 H7.75"),
        Part::Stroke(ITEMS),
    ],
);

pub(super) const COMMENT: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V14.25 A2 2 0 0 1 19.25 16.25 H10.75 \
             L6.75 19.25 V16.25 H4.75 A2 2 0 0 1 2.75 14.25 V6.75 A2 2 0 0 1 4.75 4.75 Z",
        ),
        Part::Stroke("M7 8.75 H17 M7 12.25 H13.5"),
    ],
);

/// The heading row of the table, solid.
const HEADING_ROW: &str = "M2.75 9.75 V6.75 A2 2 0 0 1 4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 \
                           V9.75 Z";

pub(super) const TABLE: Art = line(
    Keyline::Wide,
    &[
        Part::Fill(HEADING_ROW),
        Part::Stroke(
            "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 A2 2 0 \
             0 1 2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 Z",
        ),
        Part::Stroke("M2.75 9.75 H21.25 M2.75 14.25 H21.25 M8.75 9.75 V19.25 M15.25 9.75 V19.25"),
    ],
);

/// A sheet of paper standing upright, its corners rounded, nothing
/// folded.
const SHEET: &str = "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V19.25 A2 2 0 0 1 17.25 21.25 \
                     H6.75 A2 2 0 0 1 4.75 19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

pub(super) const PAGE: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(SHEET),
        Part::Stroke("M8.5 7.75 H15.5 M8.5 11.75 H15.5 M8.5 16.25 H13"),
    ],
);

pub(super) const PAGE_EMPTY: Art = line(
    Keyline::Tall,
    &[Part::Stroke(SHEET), Part::Stroke("M8.75 6.75 V10.75")],
);

/// The label on the front of the binder.
const LABEL: &str = "M12.25 6.75 H16.25 V9.75 H12.25 Z";

pub(super) const BINDER: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(
            "M8.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V19.25 A2 2 0 0 1 17.25 21.25 H8.75 A2 2 0 \
             0 1 6.75 19.25 V4.75 A2 2 0 0 1 8.75 2.75 Z",
        ),
        Part::Stroke("M4.75 6.75 H9.25 M4.75 12 H9.25 M4.75 17.25 H9.25"),
        Part::Fill(LABEL),
        Part::Stroke(LABEL),
    ],
);

/// A capital A that leaves room under it, for a mark that says what is
/// done to the letters.
const LETTER_A: &str = "M6.25 14.75 L12 3.75 L17.75 14.75 M8.34 10.75 H15.66";

/// A capital A as tall as a letter stands, at the left, with room for a
/// sign beside it.
const LETTER_A_TALL: &str = "M3.75 20.25 L9.5 3.75 L15.25 20.25 M5.84 14.25 H13.16";

/// A small capital A, standing on the same line as the tall one.
const LETTER_A_SHORT: &str = "M4.75 20.25 L9 9.75 L13.25 20.25 M6.37 16.25 H11.63";

/// The head pointing up: larger.
const UP: &str = "M15.75 10.75 L18 7.25 L20.25 10.75 Z";

/// The head pointing down: smaller.
const DOWN: &str = "M15.75 10.75 L18 14.25 L20.25 10.75 Z";

pub(super) const TEXT_LARGER: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(LETTER_A_TALL),
        Part::Fill(UP),
        Part::Stroke(UP),
    ],
);

pub(super) const TEXT_SMALLER: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(LETTER_A_SHORT),
        Part::Fill(DOWN),
        Part::Stroke(DOWN),
    ],
);

/// The bar of color under the letter.
const COLOR_BAR: &str = "M4 17.5 H20 A1 1 0 0 1 21 18.5 V20 A1 1 0 0 1 20 21 H4 A1 1 0 0 1 3 20 \
                         V18.5 A1 1 0 0 1 4 17.5 Z";

pub(super) const TEXT_COLOR: Art = line(
    Keyline::Square,
    &[Part::Stroke(LETTER_A), Part::Fill(COLOR_BAR)],
);

pub(super) const SPELLING: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(LETTER_A),
        Part::Stroke(
            "M3.75 18.75 Q5.125 16.75 6.5 18.75 Q7.875 20.75 9.25 18.75 Q10.625 16.75 12 18.75 \
             Q13.375 20.75 14.75 18.75 Q16.125 16.75 17.5 18.75 Q18.875 20.75 20.25 18.75",
        ),
    ],
);

/// The marker, lying from the top right down to the bottom left: a
/// body with a round end, and a slanted tip.
const MARKER: &str = "M15.93 4.53 A2.5 2.5 0 0 1 19.47 8.07 L12.77 14.77 L9.6 16.4 L7.6 14.4 \
                      L9.23 11.23 Z";

/// The stripe the marker has drawn.
const STRIPE: &str = "M4 18.75 H15 A1 1 0 0 1 16 19.75 V20 A1 1 0 0 1 15 21 H4 A1 1 0 0 1 3 20 \
                      V19.75 A1 1 0 0 1 4 18.75 Z";

pub(super) const HIGHLIGHT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(MARKER),
        Part::Stroke("M9.23 11.23 L12.77 14.77"),
        Part::Fill(STRIPE),
    ],
);

/// The bowl of the paragraph mark.
const BOWL: &str = "M13.25 3.75 H10 A4.25 4.25 0 0 0 10 12.25 H13.25 Z";

pub(super) const STYLE: Art = line(
    Keyline::Tall,
    &[
        Part::Fill(BOWL),
        Part::Stroke(BOWL),
        Part::Stroke("M13.25 3.75 H17.25 M13.25 3.75 V20.25 M17.25 3.75 V20.25"),
    ],
);

pub(super) const FOOTNOTE: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(SHEET),
        Part::Stroke("M8.5 6.75 H15.5 M8.5 10.75 H15.5 M8.5 14.25 H11.25"),
        Part::Fill("M7.75 18.25 A1.25 1.25 0 0 1 10.25 18.25 A1.25 1.25 0 0 1 7.75 18.25 Z"),
        Part::Stroke("M12.5 18.25 H15.5"),
    ],
);

pub(super) const TEXT_VERTICAL: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M17.25 3.75 V20.25 M12 3.75 V14.25 M6.75 3.75 V15"),
        Part::Fill("M3.75 15.5 L6.75 20.25 L9.75 15.5 Z"),
    ],
);

/// The head pointing right, from the left edge to the words.
const FROM_LEFT: &str = "M3.75 9.5 L6.75 12 L3.75 14.5 Z";

/// The head pointing left, from the right edge to the words.
const FROM_RIGHT: &str = "M20.25 9.5 L17.25 12 L20.25 14.5 Z";

pub(super) const TEXT_CENTER: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M9.75 8.75 H14.25 M9.75 12 H14.25 M9.75 15.25 H14.25"),
        Part::Fill(FROM_LEFT),
        Part::Stroke(FROM_LEFT),
        Part::Fill(FROM_RIGHT),
        Part::Stroke(FROM_RIGHT),
    ],
);

/// The four cards of the corkboard, two rows of two.
const CARDS: &str = "M2.75 4.75 H9.75 V9.75 H2.75 Z M14.25 4.75 H21.25 V9.75 H14.25 Z M2.75 \
                     14.25 H9.75 V19.25 H2.75 Z M14.25 14.25 H21.25 V19.25 H14.25 Z";

/// The four pins, one in the top middle of each card.
const PINS: &str = "M5.25 7.25 A1 1 0 0 1 7.25 7.25 A1 1 0 0 1 5.25 7.25 Z M16.75 7.25 A1 1 0 0 1 \
                    18.75 7.25 A1 1 0 0 1 16.75 7.25 Z M5.25 16.75 A1 1 0 0 1 7.25 16.75 A1 1 0 \
                    0 1 5.25 16.75 Z M16.75 16.75 A1 1 0 0 1 18.75 16.75 A1 1 0 0 1 16.75 16.75 Z";

pub(super) const CORKBOARD: Art = line(Keyline::Wide, &[Part::Stroke(CARDS), Part::Fill(PINS)]);

pub(super) const OUTLINER: Art = line(
    Keyline::Square,
    &[
        Part::Fill("M4 5.75 A1.75 1.75 0 0 1 7.5 5.75 A1.75 1.75 0 0 1 4 5.75 Z"),
        Part::Stroke("M10.25 5.75 H20.25 M14.25 12 H20.25 M14.25 18.25 H20.25"),
        Part::Stroke("M5.75 9.75 V16.25 A2 2 0 0 0 7.75 18.25 H10.75 M5.75 12 H10.75"),
    ],
);

#[cfg(test)]
mod tests {
    use egui::Color32;
    use sue_core::ReversibleTransaction;
    use sue_rice::{RiceSchema, builtin_presets};

    use super::super::raster::Tile;
    use super::super::{Icon, IconStyle, REFERENCE, weight};
    use crate::Theme;

    /// Group K, in the order of the table.
    const TEXT: [Icon; 28] = [
        Icon::Bold,
        Icon::Italic,
        Icon::Underline,
        Icon::Strikethrough,
        Icon::AlignLeft,
        Icon::AlignCenter,
        Icon::AlignRight,
        Icon::AlignJustify,
        Icon::IndentMore,
        Icon::IndentLess,
        Icon::ListBullets,
        Icon::ListNumbers,
        Icon::Comment,
        Icon::Table,
        Icon::Page,
        Icon::PageEmpty,
        Icon::Binder,
        Icon::TextLarger,
        Icon::TextSmaller,
        Icon::TextColor,
        Icon::Highlight,
        Icon::Spelling,
        Icon::Style,
        Icon::Footnote,
        Icon::TextVertical,
        Icon::TextCenter,
        Icon::Corkboard,
        Icon::Outliner,
    ];

    /// How much ink two tiles do not have in common, in whole pixels:
    /// 0 for the same picture.
    fn difference(one: &Tile, other: &Tile) -> f32 {
        let apart: u32 = one
            .alpha
            .iter()
            .zip(&other.alpha)
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum();
        apart as f32 / 255.0
    }

    /// Every pair of an icon of group K and another icon of the set,
    /// with how many pixels of ink tell them apart at 16 pixels, the
    /// closest first.
    fn look_alikes() -> Vec<(f32, Icon, Icon)> {
        let tiles: Vec<(Icon, Tile)> = Icon::ALL
            .iter()
            .map(|icon| (*icon, icon.tile(IconStyle::Line, 16, weight(16.0))))
            .collect();
        let mut pairs = Vec::new();
        for icon in TEXT {
            let mine = icon.tile(IconStyle::Line, 16, weight(16.0));
            for (other, tile) in &tiles {
                if *other != icon && !(TEXT.contains(other) && *other < icon) {
                    pairs.push((difference(&mine, tile), icon, *other));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        pairs
    }

    #[test]
    fn the_text_icons_are_each_their_own_picture_at_16() {
        // Twelve pixels of a 256 pixel tile: more than a dot or a short
        // line that a glance would miss. The members of a family that
        // sit side by side in one bar and differ by meaning in one
        // place (where the short lines end, which way the head of the
        // indent points) need ten.
        let families: [&[Icon]; 2] = [
            &[
                Icon::AlignLeft,
                Icon::AlignCenter,
                Icon::AlignRight,
                Icon::AlignJustify,
            ],
            &[Icon::IndentMore, Icon::IndentLess],
        ];
        for (apart, one, other) in look_alikes() {
            let kin = families
                .iter()
                .any(|family| family.contains(&one) && family.contains(&other));
            let least = if kin { 10.0 } else { 12.0 };
            assert!(
                apart >= least,
                "{} and {} differ by only {apart:.1} pixels of ink at 16",
                one.name(),
                other.name()
            );
        }
        assert_eq!(
            Icon::ALL.iter().filter(|icon| TEXT.contains(icon)).count(),
            TEXT.len()
        );
    }

    /// The colors of the built-in theme preset `id`.
    fn theme(id: &str) -> Theme {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        let preset = builtin_presets()
            .into_iter()
            .find(|preset| preset.id == id)
            .unwrap();
        preset.transaction(&rice).apply(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    /// A picture in the making: RGB bytes.
    struct Picture {
        width: usize,
        height: usize,
        rgb: Vec<u8>,
    }

    impl Picture {
        fn new(width: usize, height: usize, ground: Color32) -> Self {
            let rgb = (0..width * height)
                .flat_map(|_| [ground.r(), ground.g(), ground.b()])
                .collect();
            Self { width, height, rgb }
        }

        fn rect(&mut self, [left, top]: [usize; 2], [w, h]: [usize; 2], color: Color32) {
            for y in top..(top + h).min(self.height) {
                for x in left..(left + w).min(self.width) {
                    let at = (y * self.width + x) * 3;
                    self.rgb[at..at + 3].copy_from_slice(&[color.r(), color.g(), color.b()]);
                }
            }
        }

        /// Lays `tile` down in `ink`, each of its pixels `zoom` wide.
        fn tile(&mut self, at: [usize; 2], tile: &Tile, zoom: usize, ink: Color32) {
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
                        for (channel, color) in [ink.r(), ink.g(), ink.b()].into_iter().enumerate()
                        {
                            let under = f32::from(self.rgb[pixel + channel]);
                            self.rgb[pixel + channel] =
                                (under + (f32::from(color) - under) * cover).round() as u8;
                        }
                    }
                }
            }
        }

        fn save(&self, file: &std::path::Path) {
            use zune_core::bit_depth::BitDepth;
            use zune_core::colorspace::ColorSpace;
            use zune_core::options::EncoderOptions;
            let options =
                EncoderOptions::new(self.width, self.height, ColorSpace::RGB, BitDepth::Eight);
            let mut out = Vec::new();
            zune_png::PngEncoder::new(&self.rgb, options)
                .encode(&mut out)
                .unwrap();
            std::fs::write(file, out).unwrap();
        }
    }

    /// One sheet of `icons` on the panel color of `theme`: each icon at
    /// `sizes` pixels, every pixel `zoom` wide, `columns` to a row.
    fn sheet(
        theme: &Theme,
        icons: &[Icon],
        sizes: &[usize],
        zoom: usize,
        columns: usize,
    ) -> Picture {
        let gap = 8 * zoom;
        let tallest = sizes.iter().copied().max().unwrap_or(16);
        let cell = [
            sizes.iter().sum::<usize>() * zoom + (sizes.len() + 1) * gap,
            tallest * zoom + 2 * gap,
        ];
        let rows = icons.len().div_ceil(columns);
        let mut picture = Picture::new(columns * cell[0], rows * cell[1], theme.void);
        for (index, icon) in icons.iter().enumerate() {
            let corner = [(index % columns) * cell[0], (index / columns) * cell[1]];
            picture.rect(
                [corner[0] + 2, corner[1] + 2],
                [cell[0] - 4, cell[1] - 4],
                theme.panel,
            );
            let mut x = corner[0] + gap;
            for &pixels in sizes {
                let tile = icon.tile(IconStyle::Line, pixels, weight(pixels as f32));
                let y = corner[1] + gap + (tallest - pixels) * zoom / 2;
                picture.tile([x, y], &tile, zoom, theme.text);
                x += pixels * zoom + gap;
            }
        }
        picture
    }

    /// Writes the contact sheets of group K into the folder the
    /// environment variable `SUE_ICON_PROOFS` names, in the dark preset
    /// Sue ships in and in paper: every icon at 16, 24 and 48 pixels as
    /// they are, the twelve reference icons first to judge the weight by
    /// (`*-text.png`), and the same with every pixel three wide
    /// (`*-text-zoom.png`), the 16 and 24 pixel tiles alone eight wide
    /// (`*-text-16.png`). Also prints the pairs of icons whose ink at 16
    /// is most alike. Run it while drawing:
    /// `cargo test -p sue-wm text_contact_sheets -- --ignored --nocapture`.
    #[test]
    #[ignore = "writes pictures for the eye; set SUE_ICON_PROOFS to a folder"]
    fn text_contact_sheets() {
        let Some(folder) = std::env::var_os("SUE_ICON_PROOFS").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&folder).unwrap();
        for (id, name) in [("void", "dark"), ("paper", "paper")] {
            let theme = theme(id);
            let all: Vec<Icon> = REFERENCE.into_iter().chain(TEXT).collect();
            sheet(&theme, &all, &[16, 24, 48], 1, 8)
                .save(&folder.join(format!("p2-7f-{name}-text.png")));
            sheet(&theme, &TEXT, &[16, 24, 48], 3, 4)
                .save(&folder.join(format!("p2-7f-{name}-text-zoom.png")));
            sheet(&theme, &TEXT, &[16, 24], 6, 5)
                .save(&folder.join(format!("p2-7f-{name}-text-16.png")));
        }
        for (shared, one, other) in look_alikes().iter().take(20) {
            println!("{shared:.1} {} {}", one.name(), other.name());
        }
    }
}
