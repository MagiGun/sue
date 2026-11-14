//! The twelve reference icons: the first drawn, one for each keyline
//! shape as a plain line, as a line with a solid accent and with a solid
//! twin. They set the look every other icon follows.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// The ring every circle icon is drawn in: 20 units across with the pen.
const RING: &str = "M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z";

pub(super) const SETTINGS: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke(
            "M10.42 5.44 L10.63 2.85 A9.25 9.25 0 0 1 13.37 2.85 L13.58 5.44 A6.75 6.75 0 0 1 \
             15.53 6.24 L17.5 4.56 A9.25 9.25 0 0 1 19.44 6.5 L17.76 8.47 A6.75 6.75 0 0 1 18.56 \
             10.42 L21.15 10.63 A9.25 9.25 0 0 1 21.15 13.37 L18.56 13.58 A6.75 6.75 0 0 1 17.76 \
             15.53 L19.44 17.5 A9.25 9.25 0 0 1 17.5 19.44 L15.53 17.76 A6.75 6.75 0 0 1 13.58 \
             18.56 L13.37 21.15 A9.25 9.25 0 0 1 10.63 21.15 L10.42 18.56 A6.75 6.75 0 0 1 8.47 \
             17.76 L6.5 19.44 A9.25 9.25 0 0 1 4.56 17.5 L6.24 15.53 A6.75 6.75 0 0 1 5.44 13.58 \
             L2.85 13.37 A9.25 9.25 0 0 1 2.85 10.63 L5.44 10.42 A6.75 6.75 0 0 1 6.24 8.47 L4.56 \
             6.5 A9.25 9.25 0 0 1 6.5 4.56 L8.47 6.24 A6.75 6.75 0 0 1 10.42 5.44 Z",
        ),
        Part::Stroke("M9 12 A3 3 0 0 1 15 12 A3 3 0 0 1 9 12 Z"),
    ],
    solid: &[],
};

pub(super) const SEARCH: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 10.75 A7 7 0 0 1 17.75 10.75 A7 7 0 0 1 3.75 10.75 Z"),
        Part::Stroke("M15.7 15.7 L20.25 20.25"),
    ],
    solid: &[],
};

pub(super) const FOLDER: Art = Art {
    keyline: Keyline::Wide,
    line: &[Part::Stroke(
        "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L11.25 7.25 H19.25 A2 2 0 0 1 21.25 9.25 V17.25 A2 2 \
         0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z",
    )],
    solid: &[],
};

pub(super) const DELETE: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke("M4.75 6.25 H19.25"),
        Part::Stroke("M9 6.25 V4.25 A1.5 1.5 0 0 1 10.5 2.75 H13.5 A1.5 1.5 0 0 1 15 4.25 V6.25"),
        Part::Stroke("M5.75 6.25 V19.25 A2 2 0 0 0 7.75 21.25 H16.25 A2 2 0 0 0 18.25 19.25 V6.25"),
        Part::Stroke("M9.75 10.5 V17 M14.25 10.5 V17"),
    ],
    solid: &[],
};

pub(super) const INFO: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke(RING),
        Part::Stroke("M12 11 V16.75"),
        Part::Fill("M10.9 7.5 A1.1 1.1 0 0 1 13.1 7.5 A1.1 1.1 0 0 1 10.9 7.5 Z"),
    ],
    solid: &[],
};

/// The tuft of the brush.
const TUFT: &str = "M3.85 20.15 C5.83 20.43 7.88 19.66 9.72 17.53 L6.47 14.28 C4.34 16.12 3.57 \
                    18.17 3.85 20.15 Z";

pub(super) const TOOL_BRUSH: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M8.77 12.54 L18.2 4.1 A1.2 1.2 0 0 1 19.9 5.8 L11.46 15.23 Z"),
        Part::Fill(TUFT),
        Part::Stroke(TUFT),
    ],
    solid: &[],
};

pub(super) const PICTURE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 A2 2 0 0 \
             1 2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 Z",
        ),
        Part::Stroke("M2.75 15.5 L8 10.25 L13.75 16 L17 12.75 L21.25 17"),
        Part::Fill("M15.25 8.5 A1.5 1.5 0 0 1 18.25 8.5 A1.5 1.5 0 0 1 15.25 8.5 Z"),
    ],
    solid: &[],
};

/// The folded corner of the document.
const FOLD: &str = "M13.5 2.75 V7 A1.5 1.5 0 0 0 15 8.5 H19.25 Z";

pub(super) const DOCUMENT: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M6.75 2.75 H13.5 L19.25 8.5 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 0 0 1 4.75 \
             19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z",
        ),
        Part::Fill(FOLD),
        Part::Stroke(FOLD),
        Part::Stroke("M8.5 13 H15.5 M8.5 16.75 H13"),
    ],
    solid: &[],
};

/// The outline of the star.
const STAR_LINE: &str = "M12 3.22 L14.57 9.39 L21.23 9.92 L16.15 14.27 L17.7 20.77 L12 17.29 \
                         L6.3 20.77 L7.85 14.27 L2.77 9.92 L9.43 9.39 Z";

pub(super) const STAR: Art = Art {
    keyline: Keyline::Circle,
    line: &[Part::Stroke(STAR_LINE)],
    solid: &[Part::Fill(STAR_LINE), Part::Stroke(STAR_LINE)],
};

/// The outline of the heart.
const HEART_LINE: &str = "M12 6.76 A4.5 4.5 0 1 0 4.98 12.35 L12 19.75 L19.02 12.35 A4.5 4.5 0 1 \
                          0 12 6.76 Z";

pub(super) const HEART: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(HEART_LINE)],
    solid: &[Part::Fill(HEART_LINE), Part::Stroke(HEART_LINE)],
};

/// The two lids of the eye.
const LIDS: &str = "M2.75 12 C5.5 8 8.5 5.75 12 5.75 C15.5 5.75 18.5 8 21.25 12 C18.5 16 15.5 \
                    18.25 12 18.25 C8.5 18.25 5.5 16 2.75 12 Z";

pub(super) const EYE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(LIDS),
        Part::Stroke("M9 12 A3 3 0 0 1 15 12 A3 3 0 0 1 9 12 Z"),
    ],
    solid: &[
        Part::Fill(
            "M2.75 12 C5.5 8 8.5 5.75 12 5.75 C15.5 5.75 18.5 8 21.25 12 C18.5 16 15.5 18.25 12 \
             18.25 C8.5 18.25 5.5 16 2.75 12 Z M15.25 12 A3.25 3.25 0 0 0 8.75 12 A3.25 3.25 0 0 \
             0 15.25 12 Z",
        ),
        Part::Stroke(LIDS),
    ],
};

/// The shackle of the lock.
const SHACKLE: &str = "M8 10.75 V6.75 A4 4 0 0 1 16 6.75 V10.75";

/// The body of the lock.
const BODY: &str = "M6.75 10.75 H17.25 A2 2 0 0 1 19.25 12.75 V19.25 A2 2 0 0 1 17.25 21.25 \
                    H6.75 A2 2 0 0 1 4.75 19.25 V12.75 A2 2 0 0 1 6.75 10.75 Z";

pub(super) const LOCK: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(SHACKLE),
        Part::Stroke(BODY),
        Part::Stroke("M12 15 V17"),
    ],
    solid: &[
        Part::Stroke(SHACKLE),
        Part::Fill(
            "M6.75 10.75 H17.25 A2 2 0 0 1 19.25 12.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 \
             0 0 1 4.75 19.25 V12.75 A2 2 0 0 1 6.75 10.75 Z M10.9 14.75 V17.25 A1.1 1.1 0 0 0 \
             13.1 17.25 V14.75 A1.1 1.1 0 0 0 10.9 14.75 Z",
        ),
        Part::Stroke(BODY),
    ],
};
