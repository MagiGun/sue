//! The twelve reference icons: the first drawn, one for each keyline
//! shape as a plain line, as a line with a solid accent and with a solid
//! twin. They set the look every other icon follows.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// The ring every circle icon is drawn in: 20 units across with the pen.
const RING: &str = "M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z";

/// A small ring in the middle of the canvas: the hub of the gear and the
/// iris of the eye.
const HUB: &str = "M8.75 12 A3.25 3.25 0 0 1 15.25 12 A3.25 3.25 0 0 1 8.75 12 Z";

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
        Part::Stroke(HUB),
    ],
    solid: &[],
};

pub(super) const SEARCH: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 10.5 A6.75 6.75 0 0 1 17.25 10.5 A6.75 6.75 0 0 1 3.75 10.5 Z"),
        Part::Stroke("M15.27 15.27 L20.25 20.25"),
    ],
    solid: &[],
};

pub(super) const FOLDER: Art = Art {
    keyline: Keyline::Wide,
    line: &[Part::Stroke(
        "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H19.25 A2 2 0 0 1 21.25 9.75 V17.25 A2 2 0 \
         0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z",
    )],
    solid: &[],
};

pub(super) const DELETE: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke("M4.75 6.75 H19.25"),
        Part::Stroke(
            "M8.75 6.75 V4.25 A1.5 1.5 0 0 1 10.25 2.75 H13.75 A1.5 1.5 0 0 1 15.25 4.25 V6.75",
        ),
        Part::Stroke("M5.75 6.75 V19.25 A2 2 0 0 0 7.75 21.25 H16.25 A2 2 0 0 0 18.25 19.25 V6.75"),
        Part::Stroke("M9.75 11 V17 M14.25 11 V17"),
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
const TUFT: &str = "M3.81 20.19 C5.79 20.47 7.84 19.7 9.68 17.57 L6.43 14.32 C4.3 16.16 3.53 \
                    18.21 3.81 20.19 Z";

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
const FOLD: &str = "M13.25 2.75 V7.25 A1.5 1.5 0 0 0 14.75 8.75 H19.25 Z";

pub(super) const DOCUMENT: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M6.75 2.75 H13.25 L19.25 8.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 0 0 1 4.75 \
             19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z",
        ),
        Part::Fill(FOLD),
        Part::Stroke(FOLD),
        Part::Stroke("M8.5 13.25 H15.5 M8.5 17.25 H13"),
    ],
    solid: &[],
};

/// The outline of the star: five points 8.9 units from (12, 12.4), which
/// is as large as the keyline circle lets it be, and five inner corners
/// at 0.45 of that.
const STAR_LINE: &str = "M12 3.5 L14.35 9.16 L20.46 9.65 L15.81 13.64 L17.23 19.6 L12 16.41 \
                         L6.77 19.6 L8.19 13.64 L3.54 9.65 L9.65 9.16 Z";

pub(super) const STAR: Art = Art {
    keyline: Keyline::Circle,
    line: &[Part::Stroke(STAR_LINE)],
    solid: &[Part::Fill(STAR_LINE), Part::Stroke(STAR_LINE)],
};

/// The outline of the heart: two lobes 4.5 in radius round (8.25, 9.25)
/// and (15.75, 9.25), and from each a straight side down to the point.
const HEART_LINE: &str = "M12 6.763 A4.5 4.5 0 1 0 4.984 12.345 L12 19.75 L19.016 12.345 A4.5 \
                          4.5 0 1 0 12 6.763 Z";

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
    line: &[Part::Stroke(LIDS), Part::Stroke(HUB)],
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
const SHACKLE: &str = "M7.75 10.75 V7 A4.25 4.25 0 0 1 16.25 7 V10.75";

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
