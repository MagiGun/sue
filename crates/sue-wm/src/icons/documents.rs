//! Group J: the icons of Sue Paint's documents and materials. The kinds
//! of work the New dialog offers, the kinds of material, and the start
//! page's Draw, Projects and the list of pictures.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// An upright page, whole.
const PAGE: &str = "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 \
                    A2 2 0 0 1 4.75 19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

/// A sheet lying on its long side, whole.
const SHEET: &str = "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 \
                     A2 2 0 0 1 2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 Z";

/// A square tile, whole.
const TILE: &str = "M5.75 3.75 H18.25 A2 2 0 0 1 20.25 5.75 V18.25 A2 2 0 0 1 18.25 20.25 H5.75 \
                    A2 2 0 0 1 3.75 18.25 V5.75 A2 2 0 0 1 5.75 3.75 Z";

pub(super) const DOC_ILLUSTRATION: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M6.75 4.75 H17.25 A2 2 0 0 1 19.25 6.75 V13.25 A2 2 0 0 1 17.25 15.25 H6.75 A2 2 0 \
             0 1 4.75 13.25 V6.75 A2 2 0 0 1 6.75 4.75 Z",
        ),
        Part::Stroke("M12 2.75 V4.75"),
        Part::Stroke("M9 15.25 L6.75 21.25 M15 15.25 L17.25 21.25"),
        Part::Stroke("M8.25 11.75 C10 8 12.5 12.5 15.75 8.25"),
    ],
    solid: &[],
};

pub(super) const DOC_WEBTOON: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M5.75 3.75 H8.75 A2 2 0 0 1 10.75 5.75 V18.25 A2 2 0 0 1 8.75 20.25 H5.75 A2 2 0 0 \
             1 3.75 18.25 V5.75 A2 2 0 0 1 5.75 3.75 Z",
        ),
        Part::Stroke("M3.75 8.75 H10.75 M3.75 15.25 H10.75"),
        Part::Stroke("M17.25 4.5 V19.5"),
        Part::Stroke("M14.5 16.75 L17.25 19.5 L20 16.75"),
    ],
    solid: &[],
};

pub(super) const DOC_COMIC: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(PAGE),
        Part::Stroke("M4.75 9.75 H19.25"),
        Part::Stroke("M13.25 9.75 L10.75 21.25"),
    ],
    solid: &[],
};

pub(super) const DOC_PRINT_BOOK: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(PAGE),
        Part::Stroke("M8.75 2.75 V21.25"),
        Part::Stroke("M12.25 7.75 H16.25"),
    ],
    solid: &[],
};

pub(super) const DOC_COMIC_SETTINGS: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M19.25 12.25 V4.75 A2 2 0 0 0 17.25 2.75 H6.75 A2 2 0 0 0 4.75 4.75 V19.25 A2 2 0 0 \
             0 6.75 21.25 H10.25",
        ),
        Part::Stroke("M4.75 8.75 H19.25"),
        Part::Stroke("M12 2.75 V8.75"),
        Part::Fill(
            "M18.9 18.78 L19.8 19.45 L19.06 20.73 L18.04 20.28 L17.11 20.81 L16.99 21.93 L15.51 \
             21.93 L15.39 20.81 L14.46 20.28 L13.44 20.73 L12.7 19.45 L13.6 18.78 L13.6 17.72 \
             L12.7 17.05 L13.44 15.77 L14.46 16.22 L15.39 15.69 L15.51 14.57 L16.99 14.57 L17.11 \
             15.69 L18.04 16.22 L19.06 15.77 L19.8 17.05 L18.9 17.72 Z M15.05 18.25 A1.2 1.2 0 0 \
             0 17.45 18.25 A1.2 1.2 0 0 0 15.05 18.25 Z",
        ),
    ],
    solid: &[],
};

/// The play triangle on the picture of an animation.
const SMALL_PLAY: &str = "M10.75 7.75 L14.75 10 L10.75 12.25 Z";

pub(super) const DOC_ANIMATION: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SHEET),
        Part::Fill(SMALL_PLAY),
        Part::Stroke(SMALL_PLAY),
        Part::Stroke("M5.75 15.25 H18.25"),
        Part::Fill("M7.75 15.25 A1.25 1.25 0 0 1 10.25 15.25 A1.25 1.25 0 0 1 7.75 15.25 Z"),
    ],
    solid: &[],
};

pub(super) const MATERIAL_IMAGE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(TILE),
        Part::Stroke("M3.75 15.25 H20.25"),
        Part::Stroke("M6.5 15.25 L10.75 10 L15 15.25"),
        Part::Fill("M14.25 8.25 A1.25 1.25 0 0 1 16.75 8.25 A1.25 1.25 0 0 1 14.25 8.25 Z"),
    ],
    solid: &[],
};

pub(super) const MATERIAL_PATTERN: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(TILE),
        Part::Stroke("M3.75 8.75 H20.25 M3.75 15.25 H20.25"),
        Part::Stroke("M12 3.75 V8.75 M7.75 8.75 V15.25 M16.25 8.75 V15.25 M12 15.25 V20.25"),
    ],
    solid: &[],
};

pub(super) const MATERIAL_TONE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(TILE),
        Part::Fill(
            "M6.25 8 A1.75 1.75 0 0 1 9.75 8 A1.75 1.75 0 0 1 6.25 8 Z M10.6 8 A1.4 1.4 0 0 1 13.4              8 A1.4 1.4 0 0 1 10.6 8 Z M15 8 A1 1 0 0 1 17 8 A1 1 0 0 1 15 8 Z M6.6 12 A1.4 1.4 0              0 1 9.4 12 A1.4 1.4 0 0 1 6.6 12 Z M11 12 A1 1 0 0 1 13 12 A1 1 0 0 1 11 12 Z M7 16 A1              1 0 0 1 9 16 A1 1 0 0 1 7 16 Z",
        ),
    ],
    solid: &[],
};

pub(super) const MATERIAL_BRUSH_TIP: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke("M7.75 8.75 V3.75 A1 1 0 0 1 8.75 2.75 H15.25 A1 1 0 0 1 16.25 3.75 V8.75"),
        Part::Stroke(
            "M7.75 8.75 H16.25 C18.75 12.5 16.5 16.5 12 21.25 C7.5 16.5 5.25 12.5 7.75 8.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const MATERIAL_POSE: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke("M9.25 5.5 A2.75 2.75 0 0 1 14.75 5.5 A2.75 2.75 0 0 1 9.25 5.5 Z"),
        Part::Stroke("M5.75 8.25 L12.5 11.25 L18.25 13.5"),
        Part::Stroke("M12.5 11.25 L11.5 15.5"),
        Part::Stroke("M7.5 21.25 L11.5 15.5 L15.25 17.5 L14.5 21.25"),
    ],
    solid: &[],
};

pub(super) const DRAW: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M8.25 15.75 L12.14 14.55 L20.09 6.59 L17.41 3.91 L9.45 11.86 Z"),
        Part::Stroke("M12.14 14.55 L9.45 11.86"),
        Part::Stroke(
            "M8.25 15.75 C5.5 16 3.75 17.25 4 18.75 C4.5 21 9 20 12 18.5 C15 17 18.5 17.25 20.25 \
             19.75",
        ),
    ],
    solid: &[],
};

pub(super) const PROJECTS: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M4.75 10.75 H19.25 A2 2 0 0 1 21.25 12.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 A2 2              0 0 1 2.75 17.25 V12.75 A2 2 0 0 1 4.75 10.75 Z",
        ),
        Part::Stroke("M4.75 7.75 H19.25"),
        Part::Stroke("M6.75 4.75 H17.25"),
    ],
    solid: &[],
};

pub(super) const PICTURE_LIST: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.75 4.75 H7.75 A1 1 0 0 1 8.75 5.75 V9.75 A1 1 0 0 1 7.75 10.75 H3.75 A1 1 0 0 1 \
             2.75 9.75 V5.75 A1 1 0 0 1 3.75 4.75 Z",
        ),
        Part::Stroke(
            "M3.75 13.25 H7.75 A1 1 0 0 1 8.75 14.25 V18.25 A1 1 0 0 1 7.75 19.25 H3.75 A1 1 0 0 \
             1 2.75 18.25 V14.25 A1 1 0 0 1 3.75 13.25 Z",
        ),
        Part::Stroke("M12 5.75 H21.25 M12 9.75 H17.5 M12 14.25 H21.25 M12 18.25 H17.5"),
    ],
    solid: &[],
};
