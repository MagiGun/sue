//! Group H: the palettes of Sue Paint, color and the brush.
//!
//! The art of the icons of Sue Paint's palettes: the tabs they show in a
//! dock, color and the brush.
//!
//! Each was drawn on the grid of this module from the description of its
//! line in the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

pub(super) const PALETTE_COMMAND_BAR: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 9.75 A2 2 0 0 1 4.75 7.75 H19.25 A2 2 0 0 1 21.25 9.75 V14.25 A2 2 0 0 1 \
            19.25 16.25 H4.75 A2 2 0 0 1 2.75 14.25 V9.75 Z",
        ),
        Part::Fill(
            "M5.75 11.25 A0.75 0.75 0 0 1 6.5 10.5 H8 A0.75 0.75 0 0 1 8.75 11.25 V12.75 A0.75 \
            0.75 0 0 1 8 13.5 H6.5 A0.75 0.75 0 0 1 5.75 12.75 V11.25 Z M10.5 11.25 A0.75 0.75 \
            0 0 1 11.25 10.5 H12.75 A0.75 0.75 0 0 1 13.5 11.25 V12.75 A0.75 0.75 0 0 1 12.75 \
            13.5 H11.25 A0.75 0.75 0 0 1 10.5 12.75 V11.25 Z M15.25 11.25 A0.75 0.75 0 0 1 16 \
            10.5 H17.5 A0.75 0.75 0 0 1 18.25 11.25 V12.75 A0.75 0.75 0 0 1 17.5 13.5 H16 A0.75 \
            0.75 0 0 1 15.25 12.75 V11.25 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_TOOLS: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M6.75 11.75 H17.25 L16.458 18.484 A2 2 0 0 1 14.472 20.25 H9.528 A2 2 0 0 1 7.542 \
            18.484 L6.75 11.75 Z",
        ),
        Part::Stroke("M8.75 11.75 L5.25 4.25 M12 11.75 V3.75 M15.25 11.75 L18.75 4.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_SUB_TOOLS: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(
            "M3.75 5 A1.25 1.25 0 0 1 5 3.75 H8.5 A1.25 1.25 0 0 1 9.75 5 V8.5 A1.25 1.25 0 0 1 \
            8.5 9.75 H5 A1.25 1.25 0 0 1 3.75 8.5 V5 Z",
        ),
        Part::Stroke(
            "M3.75 5 A1.25 1.25 0 0 1 5 3.75 H8.5 A1.25 1.25 0 0 1 9.75 5 V8.5 A1.25 1.25 0 0 1 \
            8.5 9.75 H5 A1.25 1.25 0 0 1 3.75 8.5 V5 Z",
        ),
        Part::Stroke("M6.75 9.75 V16.25 A2 2 0 0 0 8.75 18.25 H12.25 M6.75 12 H12.25"),
        Part::Stroke(
            "M12.25 10.75 A1 1 0 0 1 13.25 9.75 H19.25 A1 1 0 0 1 20.25 10.75 V13.25 A1 1 0 0 1 \
            19.25 14.25 H13.25 A1 1 0 0 1 12.25 13.25 V10.75 Z M12.25 17.25 A1 1 0 0 1 13.25 \
            16.25 H19.25 A1 1 0 0 1 20.25 17.25 V19.25 A1 1 0 0 1 19.25 20.25 H13.25 A1 1 0 0 1 \
            12.25 19.25 V17.25 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_TOOL_PROPERTY: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M6.75 3.75 V20.25 M12 3.75 V20.25 M17.25 3.75 V20.25"),
        Part::Clear(
            "M4.75 14.5 A2 2 0 0 1 8.75 14.5 A2 2 0 0 1 4.75 14.5 Z M10 8 A2 2 0 0 1 14 8 A2 2 0 \
            0 1 10 8 Z M15.25 12.5 A2 2 0 0 1 19.25 12.5 A2 2 0 0 1 15.25 12.5 Z",
        ),
        Part::Stroke(
            "M4.75 14.5 A2 2 0 0 1 8.75 14.5 A2 2 0 0 1 4.75 14.5 Z M10 8 A2 2 0 0 1 14 8 A2 2 0 \
            0 1 10 8 Z M15.25 12.5 A2 2 0 0 1 19.25 12.5 A2 2 0 0 1 15.25 12.5 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_BRUSH_SIZE: Art = Art {
    keyline: Keyline::Wide,
    line: &[Part::Fill(
        "M2 12 A1.5 1.5 0 0 1 5 12 A1.5 1.5 0 0 1 2 12 Z M6.5 12 A2.75 2.75 0 0 1 12 12 A2.75             2.75 0 0 1 6.5 12 Z M13.5 12 A4.25 4.25 0 0 1 22 12 A4.25 4.25 0 0 1 13.5 12 Z",
    )],
    solid: &[],
};

pub(super) const PALETTE_BRUSH_GRID: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(
        "M6 7.5 A1.5 1.5 0 0 1 9 7.5 A1.5 1.5 0 0 1 6 7.5 Z M14 7.5 A2.25 2.25 0 0 1 18.5 \
            7.5 A2.25 2.25 0 0 1 14 7.5 Z M4.5 16.5 A3 3 0 0 1 10.5 16.5 A3 3 0 0 1 4.5 16.5 Z \
            M12.75 16.5 A3.75 3.75 0 0 1 20.25 16.5 A3.75 3.75 0 0 1 12.75 16.5 Z",
    )],
    solid: &[],
};

pub(super) const PALETTE_COLOR_WHEEL: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke("M3.5 12 A8.5 8.5 0 0 1 20.5 12 A8.5 8.5 0 0 1 3.5 12 Z"),
        Part::Stroke(
            "M8.75 9.75 A1 1 0 0 1 9.75 8.75 H14.25 A1 1 0 0 1 15.25 9.75 V14.25 A1 1 0 0 1 \
            14.25 15.25 H9.75 A1 1 0 0 1 8.75 14.25 V9.75 Z",
        ),
        Part::Clear("M17.26 5.99 A0.75 0.75 0 0 1 18.76 5.99 A0.75 0.75 0 0 1 17.26 5.99 Z"),
        Part::Fill("M17.26 5.99 A0.75 0.75 0 0 1 18.76 5.99 A0.75 0.75 0 0 1 17.26 5.99 Z"),
        Part::Stroke("M17.26 5.99 A0.75 0.75 0 0 1 18.76 5.99 A0.75 0.75 0 0 1 17.26 5.99 Z"),
    ],
    solid: &[],
};

pub(super) const PALETTE_COLOR_SLIDERS: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 6.75 H20.25 M3.75 12 H20.25 M3.75 17.25 H20.25"),
        Part::Clear("M7 6.75 A1.25 1.25 0 0 1 9.5 6.75 A1.25 1.25 0 0 1 7 6.75 Z"),
        Part::Fill("M7 6.75 A1.25 1.25 0 0 1 9.5 6.75 A1.25 1.25 0 0 1 7 6.75 Z"),
        Part::Stroke("M7 6.75 A1.25 1.25 0 0 1 9.5 6.75 A1.25 1.25 0 0 1 7 6.75 Z"),
        Part::Clear("M14.5 12 A1.25 1.25 0 0 1 17 12 A1.25 1.25 0 0 1 14.5 12 Z"),
        Part::Fill("M14.5 12 A1.25 1.25 0 0 1 17 12 A1.25 1.25 0 0 1 14.5 12 Z"),
        Part::Stroke("M14.5 12 A1.25 1.25 0 0 1 17 12 A1.25 1.25 0 0 1 14.5 12 Z"),
        Part::Clear("M9.75 17.25 A1.25 1.25 0 0 1 12.25 17.25 A1.25 1.25 0 0 1 9.75 17.25 Z"),
        Part::Fill("M9.75 17.25 A1.25 1.25 0 0 1 12.25 17.25 A1.25 1.25 0 0 1 9.75 17.25 Z"),
        Part::Stroke("M9.75 17.25 A1.25 1.25 0 0 1 12.25 17.25 A1.25 1.25 0 0 1 9.75 17.25 Z"),
    ],
    solid: &[],
};

pub(super) const PALETTE_COLOR_SET: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M12 18.873 A1.25 1.25 0 0 1 10.569 18.342 L10.511 18.25 M8.168 14.5 L4.74 9.014 \
            A1.25 1.25 0 0 1 5.137 7.291 L8.529 5.172 A1.25 1.25 0 0 1 9.272 4.984 M15.832 14.5 \
            L16.081 14.898 A1.25 1.25 0 0 1 15.683 16.62 L15.25 16.891",
        ),
        Part::Clear(
            "M10 18.25 A1.25 1.25 0 0 1 8.75 17 V6 A1.25 1.25 0 0 1 10 4.75 H14 A1.25 1.25 0 0 1 \
            15.25 6 V17 A1.25 1.25 0 0 1 14 18.25 H10 Z",
        ),
        Part::Stroke(
            "M10 18.25 A1.25 1.25 0 0 1 8.75 17 V16.891 M8.75 13.568 V6 A1.25 1.25 0 0 1 10 4.75 \
            H14 A1.25 1.25 0 0 1 14.729 4.984 M15.25 15.432 V17 A1.25 1.25 0 0 1 14 18.25 \
            H13.489 M10.925 18.25 H10",
        ),
        Part::Clear(
            "M8.317 16.62 A1.25 1.25 0 0 1 7.919 14.898 L13.748 5.569 A1.25 1.25 0 0 1 15.471 \
            5.172 L18.863 7.291 A1.25 1.25 0 0 1 19.26 9.014 L13.431 18.342 A1.25 1.25 0 0 1 \
            11.709 18.74 L8.317 16.62 Z",
        ),
        Part::Stroke(
            "M8.317 16.62 A1.25 1.25 0 0 1 7.919 14.898 L13.748 5.569 A1.25 1.25 0 0 1 15.471 \
            5.172 L18.863 7.291 A1.25 1.25 0 0 1 19.26 9.014 L13.431 18.342 A1.25 1.25 0 0 1 \
            11.709 18.74 L8.317 16.62 Z",
        ),
        Part::Fill("M11 14.5 A1 1 0 0 1 13 14.5 A1 1 0 0 1 11 14.5 Z"),
    ],
    solid: &[],
};

pub(super) const PALETTE_INTERMEDIATE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Fill("M8.75 6.75 H15.25 V17.25 H8.75 V6.75 Z"),
        Part::Stroke(
            "M2.75 8.75 A2 2 0 0 1 4.75 6.75 H19.25 A2 2 0 0 1 21.25 8.75 V15.25 A2 2 0 0 1             19.25 17.25 H4.75 A2 2 0 0 1 2.75 15.25 V8.75 Z",
        ),
        Part::Stroke("M8.75 6.75 V17.25 M15.25 6.75 V17.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_APPROXIMATE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill("M8.75 8.75 H15.25 V15.25 H8.75 V8.75 Z"),
        Part::Stroke(
            "M3.75 5.75 A2 2 0 0 1 5.75 3.75 H18.25 A2 2 0 0 1 20.25 5.75 V18.25 A2 2 0 0 1 \
            18.25 20.25 H5.75 A2 2 0 0 1 3.75 18.25 V5.75 Z",
        ),
        Part::Stroke("M8.75 3.75 V20.25 M15.25 3.75 V20.25 M3.75 8.75 H20.25 M3.75 15.25 H20.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_COLOR_HISTORY: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M20.25 7.75 H3.75 M7.25 4.25 L3.75 7.75 L7.25 11.25"),
        Part::Fill(
            "M3 14.75 A1.25 1.25 0 0 1 4.25 13.5 H6.25 A1.25 1.25 0 0 1 7.5 14.75 V19.75 A1.25             1.25 0 0 1 6.25 21 H4.25 A1.25 1.25 0 0 1 3 19.75 V14.75 Z",
        ),
        Part::Stroke(
            "M10.75 15.25 A1 1 0 0 1 11.75 14.25 H12.25 A1 1 0 0 1 13.25 15.25 V19.25 A1 1 0 0 1             12.25 20.25 H11.75 A1 1 0 0 1 10.75 19.25 V15.25 Z M17.25 15.25 A1 1 0 0 1 18.25             14.25 H19.25 A1 1 0 0 1 20.25 15.25 V19.25 A1 1 0 0 1 19.25 20.25 H18.25 A1 1 0 0 1             17.25 19.25 V15.25 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_NAVIGATOR: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 \
            19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 V6.75 Z",
        ),
        Part::Stroke(
            "M10.75 8.75 A1 1 0 0 1 11.75 7.75 H17.25 A1 1 0 0 1 18.25 8.75 V12.25 A1 1 0 0 1 \
            17.25 13.25 H11.75 A1 1 0 0 1 10.75 12.25 V8.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_SUB_VIEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 9.75 A2 2 0 0 1 4.75 7.75 H19.25 A2 2 0 0 1 21.25 9.75 V17.25 A2 2 0 0 1 \
            19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 V9.75 Z",
        ),
        Part::Stroke("M2.75 17.25 L7.75 12.25 L12.75 17.25 L15.25 14.75 L19.75 19.25"),
        Part::Clear("M9.75 6.5 A2.25 2.25 0 0 1 14.25 6.5 A2.25 2.25 0 0 1 9.75 6.5 Z"),
        Part::Fill("M9.75 6.5 A2.25 2.25 0 0 1 14.25 6.5 A2.25 2.25 0 0 1 9.75 6.5 Z"),
        Part::Stroke("M12 8.5 V11"),
    ],
    solid: &[],
};

pub(super) const PALETTE_MATERIALS: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M3.25 19.25 H20.75"),
        Part::Stroke("M2.75 13 A3.25 3.25 0 0 1 9.25 13 A3.25 3.25 0 0 1 2.75 13 Z"),
        Part::Stroke(
            "M10.75 9.5 A0.75 0.75 0 0 1 11.5 8.75 H14.5 A0.75 0.75 0 0 1 15.25 9.5 V15.5 A0.75 \
            0.75 0 0 1 14.5 16.25 H11.5 A0.75 0.75 0 0 1 10.75 15.5 V9.5 Z",
        ),
        Part::Stroke(
            "M16.93 15.614 L18.394 10.448 A0.5 0.5 0 0 1 19.356 10.448 L20.82 15.614 A0.5 0.5 0 \
            0 1 20.339 16.25 L17.411 16.25 A0.5 0.5 0 0 1 16.93 15.614 Z",
        ),
    ],
    solid: &[],
};

pub(super) const PALETTE_HISTORY: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M3.75 5.75 A2 2 0 0 1 7.75 5.75 A2 2 0 0 1 3.75 5.75 Z M3.75 12 A2 2 0 0 1 7.75 12 A2             2 0 0 1 3.75 12 Z",
        ),
        Part::Stroke("M5.75 7.75 V10 M5.75 14 V15.5"),
        Part::Fill("M3 18.25 A2.75 2.75 0 0 1 8.5 18.25 A2.75 2.75 0 0 1 3 18.25 Z"),
        Part::Stroke("M11.25 5.75 H20.25 M11.25 12 H17.25 M11.25 18.25 H20.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_AUTO_ACTION: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill("M3.75 7.5 L9.25 12 L3.75 16.5 Z"),
        Part::Stroke("M3.75 7.5 L9.25 12 L3.75 16.5 Z"),
        Part::Stroke("M12.75 6.75 H20.25 M12.75 12 H20.25 M12.75 17.25 H20.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_ITEM_BANK: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.75 11.25 A1.5 1.5 0 0 1 5.25 9.75 H18.75 A1.5 1.5 0 0 1 20.25 11.25 V17.75 A1.5 \
            1.5 0 0 1 18.75 19.25 H5.25 A1.5 1.5 0 0 1 3.75 17.75 V11.25 Z",
        ),
        Part::Stroke(
            "M2.75 6.25 A1.5 1.5 0 0 1 4.25 4.75 H19.75 A1.5 1.5 0 0 1 21.25 6.25 V8.25 A1.5 1.5 \
            0 0 1 19.75 9.75 H4.25 A1.5 1.5 0 0 1 2.75 8.25 V6.25 Z",
        ),
        Part::Stroke("M9.75 14.25 H14.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_INFO: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M4.25 12 A3.5 3.5 0 0 1 11.25 12 A3.5 3.5 0 0 1 4.25 12 Z"),
        Part::Stroke("M7.75 4.75 V8.5 M7.75 15.5 V19.25 M2.75 12 H4.25"),
        Part::Stroke("M14.25 9.75 H21.25 M14.25 14.25 H19.25"),
    ],
    solid: &[],
};

pub(super) const PALETTE_TIMELINE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 10.75 H21.25 M2.75 15.25 H10.25"),
        Part::Clear("M14.25 8.75 V19.25"),
        Part::Stroke("M14.25 8.75 V19.25"),
        Part::Fill("M12.25 4.75 H16.25 V6.75 L14.25 8.75 L12.25 6.75 Z"),
        Part::Stroke("M12.25 4.75 H16.25 V6.75 L14.25 8.75 L12.25 6.75 Z"),
    ],
    solid: &[],
};

pub(super) const PALETTE_CELS: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M6.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V15.25"),
        Part::Stroke(
            "M2.75 9.75 A2 2 0 0 1 4.75 7.75 H15.25 A2 2 0 0 1 17.25 9.75 V17.25 A2 2 0 0 1 \
            15.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 V9.75 Z",
        ),
        Part::Stroke("M6.25 11.75 H7.5 M12.5 11.75 H13.75"),
        Part::Fill("M9 11.75 A1 1 0 0 1 11 11.75 A1 1 0 0 1 9 11.75 Z"),
    ],
    solid: &[],
};

pub(super) const PRESSURE_CURVE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 3.75 V20.25 H20.25"),
        Part::Stroke("M7.25 16.75 C13.25 16.75 15.25 12.5 16.75 7.25"),
        Part::Fill("M15 7.25 A1.75 1.75 0 0 1 18.5 7.25 A1.75 1.75 0 0 1 15 7.25 Z"),
    ],
    solid: &[],
};
