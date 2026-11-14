//! Group F: what is done to a selection and to the canvas. A selection
//! is always the same dashed box, and what happens to it is drawn
//! inside or across it; snapping is always the same magnet, held over
//! what is snapped to.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// The dashed box of a selection: four round corners and a dash in
/// the middle of each side.
pub(super) const FRAME: &str = "M3.75 6.75 V5.75 A2 2 0 0 1 5.75 3.75 H6.75 M9.75 3.75 H14.25 \
                                M17.25 3.75 H18.25 A2 2 0 0 1 20.25 5.75 V6.75 M20.25 9.75 V14.25 \
                                M20.25 17.25 V18.25 A2 2 0 0 1 18.25 20.25 H17.25 M9.75 20.25 \
                                H14.25 M6.75 20.25 H5.75 A2 2 0 0 1 3.75 18.25 V17.25 M3.75 9.75 \
                                V14.25";

/// The line that strikes the selection out, from corner to corner where
/// the dashed box leaves out its two corners.
const SLASH: &str = "M3.75 20.25 L20.25 3.75";

/// The dashed box of a selection without its top right and bottom left
/// corners, which the slash takes the place of.
const STRUCK: &str = "M3.75 6.75 V5.75 A2 2 0 0 1 5.75 3.75 H6.75 M9.75 3.75 H14.25 M20.25 \
                      9.75 V14.25 M20.25 17.25 V18.25 A2 2 0 0 1 18.25 20.25 H17.25 M9.75 \
                      20.25 H14.25 M3.75 9.75 V14.25";

/// The solid half of the inverted selection.
const HALF: &str = "M20.25 9.75 V18.25 A2 2 0 0 1 18.25 20.25 H9.75 Z";

/// The horseshoe of the magnet, upright: a broad band bent round at
/// the top, its two legs pointing down.
const MAGNET: &str = "M5.75 14.25 V10 A6.25 6.25 0 0 1 18.25 10 V14.25 H14.25 V10 A2.25 2.25 0 \
                      0 0 9.75 10 V14.25 Z";

/// The two poles of the magnet: the ends of its legs, solid.
const POLES: &str = "M5.75 12.25 H9.75 V14.25 H5.75 Z M14.25 12.25 H18.25 V14.25 H14.25 Z";

/// The chip of the drawing color.
const CHIP: &str = "M5.25 3.75 H9.25 A1.5 1.5 0 0 1 10.75 5.25 V9.25 A1.5 1.5 0 0 1 9.25 10.75 \
                    H5.25 A1.5 1.5 0 0 1 3.75 9.25 V5.25 A1.5 1.5 0 0 1 5.25 3.75 Z";

pub(super) const SELECT_ALL: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(FRAME),
        Part::Stroke(
            "M8.75 7.75 H15.25 A1 1 0 0 1 16.25 8.75 V15.25 A1 1 0 0 1 15.25 16.25 H8.75 A1 1 0 0 \
             1 7.75 15.25 V8.75 A1 1 0 0 1 8.75 7.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const DESELECT: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(STRUCK), Part::Stroke(SLASH)],
    solid: &[],
};

pub(super) const SELECT_INVERT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M3.75 6.75 V5.75 A2 2 0 0 1 5.75 3.75 H6.75 M9.75 3.75 H14.25 M17.25 3.75 H18.25 A2 2 \
             0 0 1 20.25 5.75 V6.75 M6.75 20.25 H5.75 A2 2 0 0 1 3.75 18.25 V17.25 M3.75 9.75 \
             V14.25",
        ),
        Part::Fill(HALF),
        Part::Stroke(HALF),
    ],
    solid: &[],
};

pub(super) const SELECT_EXPAND: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(FRAME),
        Part::Stroke("M7.75 7.75 L16.25 16.25"),
        Part::Stroke("M7.75 10.75 V7.75 H10.75 M16.25 13.25 V16.25 H13.25"),
    ],
    solid: &[],
};

pub(super) const SELECT_SHRINK: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(FRAME),
        Part::Stroke("M7.25 7.25 L10.75 10.75 M16.75 16.75 L13.25 13.25"),
        Part::Stroke("M10.75 7.75 V10.75 H7.75 M13.25 16.25 V13.25 H16.25"),
    ],
    solid: &[],
};

pub(super) const CLEAR_INSIDE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(FRAME),
        Part::Stroke("M8.75 8.75 L15.25 15.25 M15.25 8.75 L8.75 15.25"),
    ],
    solid: &[],
};

pub(super) const CLEAR_OUTSIDE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M3.75 13.75 V12.75 A2 2 0 0 1 5.75 10.75 H6.75 M10.25 10.75 H11.25 A2 2 0 0 1 13.25 \
             12.75 V13.75 M13.25 17.25 V18.25 A2 2 0 0 1 11.25 20.25 H10.25 M6.75 20.25 H5.75 A2 2 \
             0 0 1 3.75 18.25 V17.25",
        ),
        Part::Stroke("M15.25 3.75 L20.25 8.75 M20.25 3.75 L15.25 8.75"),
    ],
    solid: &[],
};

pub(super) const FILL_SELECTION: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(FRAME),
        Part::Fill(
            "M12 7 C13.25 9.25 15.25 11 15.25 13.5 A3.25 3.25 0 0 1 8.75 13.5 C8.75 11 10.75 9.25 \
             12 7 Z",
        ),
    ],
    solid: &[],
};

pub(super) const CROP: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M7.75 3.75 V16.25 H20.25"),
        Part::Stroke("M3.75 7.75 H16.25 V20.25"),
    ],
    solid: &[],
};

pub(super) const TRANSFORM: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M4.75 4.75 H19.25 V19.25 H4.75 Z"),
        Part::Fill(
            "M3.5 3 H6 A0.5 0.5 0 0 1 6.5 3.5 V6 A0.5 0.5 0 0 1 6 6.5 H3.5 A0.5 0.5 0 0 1 3 6 V3.5 \
             A0.5 0.5 0 0 1 3.5 3 Z M3.5 17.5 H6 A0.5 0.5 0 0 1 6.5 18 V20.5 A0.5 0.5 0 0 1 6 21 \
             H3.5 A0.5 0.5 0 0 1 3 20.5 V18 A0.5 0.5 0 0 1 3.5 17.5 Z M18 3 H20.5 A0.5 0.5 0 0 1 \
             21 3.5 V6 A0.5 0.5 0 0 1 20.5 6.5 H18 A0.5 0.5 0 0 1 17.5 6 V3.5 A0.5 0.5 0 0 1 18 3 \
             Z M18 17.5 H20.5 A0.5 0.5 0 0 1 21 18 V20.5 A0.5 0.5 0 0 1 20.5 21 H18 A0.5 0.5 0 0 1 \
             17.5 20.5 V18 A0.5 0.5 0 0 1 18 17.5 Z",
        ),
    ],
    solid: &[],
};

pub(super) const SNAP_RULER: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(POLES),
        Part::Stroke(MAGNET),
        Part::Stroke("M3.75 20.25 H20.25 M7.75 20.25 V18.25 M12 20.25 V17.25 M16.25 20.25 V18.25"),
    ],
    solid: &[],
};

pub(super) const SNAP_GRID: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(POLES),
        Part::Stroke(MAGNET),
        Part::Stroke(
            "M3.75 17.25 H20.25 M3.75 20.25 H20.25 M3.75 17.25 V20.25 M7.75 17.25 V20.25 M12 17.25 \
             V20.25 M16.25 17.25 V20.25 M20.25 17.25 V20.25",
        ),
    ],
    solid: &[],
};

pub(super) const COLOR_SWAP: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(CHIP),
        Part::Stroke(CHIP),
        Part::Stroke(
            "M14.75 13.25 H18.75 A1.5 1.5 0 0 1 20.25 14.75 V18.75 A1.5 1.5 0 0 1 18.75 20.25 \
             H14.75 A1.5 1.5 0 0 1 13.25 18.75 V14.75 A1.5 1.5 0 0 1 14.75 13.25 Z",
        ),
        Part::Stroke(
            "M13.75 4.75 H16.25 A2 2 0 0 1 18.25 6.75 V10.25 M10.25 19.25 H7.75 A2 2 0 0 1 5.75 \
             17.25 V13.75",
        ),
        Part::Stroke("M16.25 8.25 L18.25 10.25 L20.25 8.25 M7.75 15.75 L5.75 13.75 L3.75 15.75"),
    ],
    solid: &[],
};
