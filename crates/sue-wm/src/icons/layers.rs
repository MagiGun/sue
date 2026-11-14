//! Group G: the layers of a picture, and the light table of an animation.
//!
//! The art of the icons of layers: the Layer palette and its commands, what
//! a layer can be set to, the effects of the Layer Property palette, and
//! the light table of the Animation Cels palette.
//!
//! Each was drawn on the grid of this module from the description of its
//! line in the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// A plus at the lower right: something is made.
const PLUS: &str = "M18.25 13.75 V18.75 M15.75 16.25 H20.75";

/// A check at the lower right: something is done for good.
const CHECK: &str = "M15.25 16.25 L17.25 18.25 L21.25 14.25";

/// A plus at the upper right.
const PLUS_HIGH: &str = "M18.25 5.25 V10.25 M15.75 7.75 H20.75";

/// A minus at the upper right.
const MINUS_HIGH: &str = "M15.75 7.75 H20.75";

/// The body of the small padlock at the lower right.
const PAD_BODY: &str = "M14.25 15.25 A1 1 0 0 1 15.25 14.25 H20.25 A1 1 0 0 1 21.25 15.25 V18.25 \
    A1 1 0 0 1 20.25 19.25 H15.25 A1 1 0 0 1 14.25 18.25 V15.25 Z";

/// The shackle of the small padlock at the lower right.
const PAD_SHACKLE: &str = "M16.25 14.25 V12.75 A1.5 1.5 0 0 1 19.25 12.75 V14.25";

/// The lit panel of the light table.
const SLAB: &str = "M2.75 15.25 A2 2 0 0 1 4.75 13.25 H19.25 A2 2 0 0 1 21.25 15.25 V17.25 A2 2 \
    0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 V15.25 Z";

pub(super) const LAYERS: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M4.165 8.921 A0.75 0.75 0 0 1 4.165 7.579 L11.665 3.829 A0.75 0.75 0 0 1 12.335 \
            3.829 L19.835 7.579 A0.75 0.75 0 0 1 19.835 8.921 L12.335 12.671 A0.75 0.75 0 0 1 \
            11.665 12.671 L4.165 8.921 Z",
        ),
        Part::Stroke("M3.75 12.464 L11.665 16.421 A0.75 0.75 0 0 0 12.335 16.421 L20.25 12.464"),
        Part::Stroke("M3.75 16.214 L11.665 20.171 A0.75 0.75 0 0 0 12.335 20.171 L20.25 16.214"),
    ],
    solid: &[],
};

pub(super) const LAYER_NEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.165 9.546 A0.75 0.75 0 0 1 3.165 8.204 L9.915 4.829 A0.75 0.75 0 0 1 10.585 \
            4.829 L17.335 8.204 A0.75 0.75 0 0 1 17.335 9.546 L10.585 12.921 A0.75 0.75 0 0 1 \
            9.915 12.921 L3.165 9.546 Z",
        ),
        Part::Stroke("M2.75 13.089 L9.915 16.671 A0.75 0.75 0 0 0 10.585 16.671 L17.75 13.089"),
        Part::Clear(PLUS),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

pub(super) const LAYER_NEW_VECTOR: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M5.25 17.25 C5.25 10.75 9.25 6.75 15.25 6.75"),
        Part::Fill(
            "M3.25 16.5 A1 1 0 0 1 4.25 15.5 H6.25 A1 1 0 0 1 7.25 16.5 V18.5 A1 1 0 0 1 6.25 \
            19.5 H4.25 A1 1 0 0 1 3.25 18.5 V16.5 Z M13.75 5.75 A1 1 0 0 1 14.75 4.75 H16.75 A1 \
            1 0 0 1 17.75 5.75 V7.75 A1 1 0 0 1 16.75 8.75 H14.75 A1 1 0 0 1 13.75 7.75 V5.75 Z",
        ),
        Part::Clear(PLUS),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

pub(super) const LAYER_NEW_FOLDER: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H19.25 A2 2 0 0 1 21.25 9.75 V17.25 A2 \
            2 0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z",
        ),
        Part::Clear(PLUS),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

pub(super) const LAYER_DUPLICATE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M7.165 9.421 A0.75 0.75 0 0 1 7.165 8.079 L13.665 4.829 A0.75 0.75 0 0 1 14.335 \
            4.829 L20.835 8.079 A0.75 0.75 0 0 1 20.835 9.421 L14.335 12.671 A0.75 0.75 0 0 1 \
            13.665 12.671 L7.165 9.421 Z",
        ),
        Part::Clear(
            "M3.165 15.921 A0.75 0.75 0 0 1 3.165 14.579 L9.665 11.329 A0.75 0.75 0 0 1 10.335 \
            11.329 L16.835 14.579 A0.75 0.75 0 0 1 16.835 15.921 L10.335 19.171 A0.75 0.75 0 0 \
            1 9.665 19.171 L3.165 15.921 Z",
        ),
        Part::Stroke(
            "M3.165 15.921 A0.75 0.75 0 0 1 3.165 14.579 L9.665 11.329 A0.75 0.75 0 0 1 10.335 \
            11.329 L16.835 14.579 A0.75 0.75 0 0 1 16.835 15.921 L10.335 19.171 A0.75 0.75 0 0 \
            1 9.665 19.171 L3.165 15.921 Z",
        ),
    ],
    solid: &[],
};

pub(super) const LAYER_MERGE_DOWN: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M4.915 15.796 A0.75 0.75 0 0 1 4.915 14.454 L11.665 11.079 A0.75 0.75 0 0 1 12.335 \
            11.079 L19.085 14.454 A0.75 0.75 0 0 1 19.085 15.796 L12.335 19.171 A0.75 0.75 0 0 \
            1 11.665 19.171 L4.915 15.796 Z",
        ),
        Part::Clear("M12 4.75 V13.25 M8.75 10 L12 13.25 L15.25 10"),
        Part::Stroke("M12 4.75 V13.25 M8.75 10 L12 13.25 L15.25 10"),
    ],
    solid: &[],
};

pub(super) const LAYER_TRANSFER_DOWN: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.165 10.046 A0.75 0.75 0 0 1 3.165 8.704 L7.915 6.329 A0.75 0.75 0 0 1 8.585 \
            6.329 L13.335 8.704 A0.75 0.75 0 0 1 13.335 10.046 L8.585 12.421 A0.75 0.75 0 0 1 \
            7.915 12.421 L3.165 10.046 Z",
        ),
        Part::Stroke("M2.75 13.589 L7.915 16.171 A0.75 0.75 0 0 0 8.585 16.171 L13.75 13.589"),
        Part::Stroke("M18.25 4.75 V18.75 M15.25 15.75 L18.25 18.75 L21.25 15.75"),
    ],
    solid: &[],
};

pub(super) const LAYER_CLIP: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M7.665 16.046 A0.75 0.75 0 0 1 7.665 14.704 L13.915 11.579 A0.75 0.75 0 0 1 14.585 \
            11.579 L20.835 14.704 A0.75 0.75 0 0 1 20.835 16.046 L14.585 19.171 A0.75 0.75 0 0 \
            1 13.915 19.171 L7.665 16.046 Z",
        ),
        Part::Clear("M2.75 5.75 H5.75 A3 3 0 0 1 8.75 8.75 V13.25 M6 10.5 L8.75 13.25 L11.5 10.5"),
        Part::Stroke("M2.75 5.75 H5.75 A3 3 0 0 1 8.75 8.75 V13.25 M6 10.5 L8.75 13.25 L11.5 10.5"),
    ],
    solid: &[],
};

pub(super) const LAYER_REFERENCE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.915 15.296 A0.75 0.75 0 0 1 3.915 13.954 L11.665 10.079 A0.75 0.75 0 0 1 12.335 \
            10.079 L20.085 13.954 A0.75 0.75 0 0 1 20.085 15.296 L12.335 19.171 A0.75 0.75 0 0 \
            1 11.665 19.171 L3.915 15.296 Z",
        ),
        Part::Clear(
            "M12 17.25 C9 14 7.75 12 7.75 9.25 A4.25 4.25 0 0 1 16.25 9.25 C16.25 12 15 14 12 \
            17.25 Z",
        ),
        Part::Stroke(
            "M12 17.25 C9 14 7.75 12 7.75 9.25 A4.25 4.25 0 0 1 16.25 9.25 C16.25 12 15 14 12 \
            17.25 Z",
        ),
        Part::Fill("M10.9 9.25 A1.1 1.1 0 0 1 13.1 9.25 A1.1 1.1 0 0 1 10.9 9.25 Z"),
    ],
    solid: &[],
};

pub(super) const LAYER_DRAFT: Art = Art {
    keyline: Keyline::Wide,
    line: &[Part::Stroke(
        "M2.905 12.456 A0.75 0.75 0 0 1 3.165 11.329 L4.077 10.873 M6.76 9.531 L8.854 8.485 \
            M11.537 7.143 L11.665 7.079 A0.75 0.75 0 0 1 12.335 7.079 L13.679 7.751 M16.363 \
            9.093 L18.456 10.14 M21.095 11.544 A0.75 0.75 0 0 1 20.835 12.671 L19.923 13.127 \
            M17.24 14.469 L15.146 15.515 M12.463 16.857 L12.335 16.921 A0.75 0.75 0 0 1 11.665 \
            16.921 L10.321 16.249 M7.637 14.907 L5.544 13.86",
    )],
    solid: &[],
};

pub(super) const LAYER_LOCK_ALPHA: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H13.25 A2 2 0 0 1 15.25 6.75 V15.25 A2 2 0 0 1 \
            13.25 17.25 H4.75 A2 2 0 0 1 2.75 15.25 V6.75 Z",
        ),
        Part::Fill(
            "M9 4.75 H13.25 A2 2 0 0 1 15.25 6.75 V11 H9 V4.75 Z M2.75 11 H9 V17.25 H4.75 A2 2 0 \
            0 1 2.75 15.25 V11 Z",
        ),
        Part::Clear(PAD_BODY),
        Part::Clear(PAD_SHACKLE),
        Part::Stroke(PAD_BODY),
        Part::Stroke(PAD_SHACKLE),
    ],
    solid: &[],
};

pub(super) const LAYER_MASK: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 \
            19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 V6.75 Z",
        ),
        Part::Fill("M7.75 12 A4.25 4.25 0 0 1 16.25 12 A4.25 4.25 0 0 1 7.75 12 Z"),
    ],
    solid: &[],
};

pub(super) const LAYER_MASK_NEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H15.25 A2 2 0 0 1 17.25 6.75 V14.25 A2 2 0 0 1 \
            15.25 16.25 H4.75 A2 2 0 0 1 2.75 14.25 V6.75 Z",
        ),
        Part::Fill("M7.25 10.5 A2.75 2.75 0 0 1 12.75 10.5 A2.75 2.75 0 0 1 7.25 10.5 Z"),
        Part::Clear(PLUS),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

pub(super) const LAYER_MASK_APPLY: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H15.25 A2 2 0 0 1 17.25 6.75 V14.25 A2 2 0 0 1 \
            15.25 16.25 H4.75 A2 2 0 0 1 2.75 14.25 V6.75 Z",
        ),
        Part::Fill("M7.25 10.5 A2.75 2.75 0 0 1 12.75 10.5 A2.75 2.75 0 0 1 7.25 10.5 Z"),
        Part::Clear(CHECK),
        Part::Stroke(CHECK),
    ],
    solid: &[],
};

pub(super) const LAYER_COLOR: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.165 9.546 A0.75 0.75 0 0 1 3.165 8.204 L9.915 4.829 A0.75 0.75 0 0 1 10.585 \
            4.829 L17.335 8.204 A0.75 0.75 0 0 1 17.335 9.546 L10.585 12.921 A0.75 0.75 0 0 1 \
            9.915 12.921 L3.165 9.546 Z",
        ),
        Part::Stroke("M2.75 13.089 L9.915 16.671 A0.75 0.75 0 0 0 10.585 16.671 L17.75 13.089"),
        Part::Clear(
            "M18 11.75 C19.75 13.75 20.75 15.25 20.75 16.5 A2.75 2.75 0 0 1 15.25 16.5 C15.25 \
            15.25 16.25 13.75 18 11.75 Z",
        ),
        Part::Fill(
            "M18 11.75 C19.75 13.75 20.75 15.25 20.75 16.5 A2.75 2.75 0 0 1 15.25 16.5 C15.25 \
            15.25 16.25 13.75 18 11.75 Z",
        ),
        Part::Stroke(
            "M18 11.75 C19.75 13.75 20.75 15.25 20.75 16.5 A2.75 2.75 0 0 1 15.25 16.5 C15.25 \
            15.25 16.25 13.75 18 11.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const LAYER_COLOR_MODE: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke("M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z"),
        Part::Fill("M12 4.5 A7.5 7.5 0 0 0 12 19.5 Z"),
    ],
    solid: &[],
};

pub(super) const LAYER_PROPERTY: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M3.165 9.546 A0.75 0.75 0 0 1 3.165 8.204 L9.915 4.829 A0.75 0.75 0 0 1 10.585 \
            4.829 L17.335 8.204 A0.75 0.75 0 0 1 17.335 9.546 L10.585 12.921 A0.75 0.75 0 0 1 \
            9.915 12.921 L3.165 9.546 Z",
        ),
        Part::Stroke("M2.75 13.089 L9.915 16.671 A0.75 0.75 0 0 0 10.585 16.671 L17.75 13.089"),
        Part::Clear(
            "M18.25 12.25 L19.05 14.45 L21.25 15.25 L19.05 16.05 L18.25 18.25 L17.45 16.05 \
            L15.25 15.25 L17.45 14.45 Z",
        ),
        Part::Fill(
            "M18.25 12.25 L19.05 14.45 L21.25 15.25 L19.05 16.05 L18.25 18.25 L17.45 16.05 \
            L15.25 15.25 L17.45 14.45 Z",
        ),
        Part::Stroke(
            "M18.25 12.25 L19.05 14.45 L21.25 15.25 L19.05 16.05 L18.25 18.25 L17.45 16.05 \
            L15.25 15.25 L17.45 14.45 Z",
        ),
    ],
    solid: &[],
};

pub(super) const LAYER_OPACITY_TARGET: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M4.165 9.921 A0.75 0.75 0 0 1 4.165 8.579 L11.665 4.829 A0.75 0.75 0 0 1 12.335 \
            4.829 L19.835 8.579 A0.75 0.75 0 0 1 19.835 9.921 L12.335 13.671 A0.75 0.75 0 0 1 \
            11.665 13.671 L4.165 9.921 Z",
        ),
        Part::Stroke(
            "M3.75 15.214 L7.147 16.912 M10.277 18.477 L11.665 19.171 A0.75 0.75 0 0 0 12.335 \
            19.171 L13.723 18.477 M16.853 16.912 L20.25 15.214",
        ),
    ],
    solid: &[],
};

pub(super) const LAYER_PAPER: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(
            "M4.165 14.329 A0.75 0.75 0 0 0 4.165 15.671 L11.665 19.421 A0.75 0.75 0 0 0 12.335 \
            19.421 L19.835 15.671 A0.75 0.75 0 0 0 19.835 14.329 L15.177 12 L12 13.589 L8.823 \
            12 L4.165 14.329 Z",
        ),
        Part::Stroke(
            "M4.165 14.329 A0.75 0.75 0 0 0 4.165 15.671 L11.665 19.421 A0.75 0.75 0 0 0 12.335 \
            19.421 L19.835 15.671 A0.75 0.75 0 0 0 19.835 14.329 L15.177 12 L12 13.589 L8.823 \
            12 L4.165 14.329 Z",
        ),
        Part::Clear(
            "M4.165 9.671 A0.75 0.75 0 0 1 4.165 8.329 L11.665 4.579 A0.75 0.75 0 0 1 12.335 \
            4.579 L19.835 8.329 A0.75 0.75 0 0 1 19.835 9.671 L12.335 13.421 A0.75 0.75 0 0 1 \
            11.665 13.421 L4.165 9.671 Z",
        ),
        Part::Stroke(
            "M4.165 9.671 A0.75 0.75 0 0 1 4.165 8.329 L11.665 4.579 A0.75 0.75 0 0 1 12.335 \
            4.579 L19.835 8.329 A0.75 0.75 0 0 1 19.835 9.671 L12.335 13.421 A0.75 0.75 0 0 1 \
            11.665 13.421 L4.165 9.671 Z",
        ),
    ],
    solid: &[],
};

pub(super) const BORDER_EFFECT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill("M6.75 9.25 A2.5 2.5 0 0 1 11.75 9.25 A2.5 2.5 0 0 1 6.75 9.25 Z"),
        Part::Stroke("M6.75 9.25 A2.5 2.5 0 0 1 11.75 9.25 A2.5 2.5 0 0 1 6.75 9.25 Z"),
        Part::Fill(
            "M13.25 14.25 A1 1 0 0 1 14.25 13.25 H16.25 A1 1 0 0 1 17.25 14.25 V16.25 A1 1 0 0 1 \
            16.25 17.25 H14.25 A1 1 0 0 1 13.25 16.25 V14.25 Z",
        ),
        Part::Stroke(
            "M13.25 14.25 A1 1 0 0 1 14.25 13.25 H16.25 A1 1 0 0 1 17.25 14.25 V16.25 A1 1 0 0 1 \
            16.25 17.25 H14.25 A1 1 0 0 1 13.25 16.25 V14.25 Z",
        ),
        Part::Stroke(
            "M3.75 9.25 A5.5 5.5 0 0 1 9.5 3.756 A5.5 5.5 0 0 1 14.727 9.75 M9.75 14.727 A5.5 \
            5.5 0 0 1 3.75 9.25 M14.727 9.75 H16.25 A4 4 0 0 1 20.25 13.75 V16.25 A4 4 0 0 1 \
            16.25 20.25 H13.75 A4 4 0 0 1 9.75 16.25 V14.727",
        ),
    ],
    solid: &[],
};

pub(super) const WATERCOLOR_EDGE: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M12 2.75 C15.25 6.75 18.75 10.75 18.75 14.5 A6.75 6.75 0 0 1 5.25 14.5 C5.25 10.75 \
            8.75 6.75 12 2.75 Z",
        ),
        Part::Stroke("M8.476 13.217 A3.75 3.75 0 0 0 11.673 18.236"),
    ],
    solid: &[],
};

pub(super) const EXTRACT_LINE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 11.25 L8.75 4.75 L12.25 8.25 L15.25 5.25 L21.25 11.25"),
        Part::Fill("M2.75 19.25 L8.75 12.75 L12.25 16.25 L15.25 13.25 L21.25 19.25 Z"),
    ],
    solid: &[],
};

pub(super) const TONE: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Fill(
        "M5 6 A1 1 0 0 1 7 6 A1 1 0 0 1 5 6 Z M4.675 12 A1.325 1.325 0 0 1 7.325 12 A1.325 \
            1.325 0 0 1 4.675 12 Z M4.35 18 A1.65 1.65 0 0 1 7.65 18 A1.65 1.65 0 0 1 4.35 18 Z \
            M10.675 6 A1.325 1.325 0 0 1 13.325 6 A1.325 1.325 0 0 1 10.675 6 Z M10.35 12 A1.65 \
            1.65 0 0 1 13.65 12 A1.65 1.65 0 0 1 10.35 12 Z M10.025 18 A1.975 1.975 0 0 1 \
            13.975 18 A1.975 1.975 0 0 1 10.025 18 Z M16.35 6 A1.65 1.65 0 0 1 19.65 6 A1.65 \
            1.65 0 0 1 16.35 6 Z M16.025 12 A1.975 1.975 0 0 1 19.975 12 A1.975 1.975 0 0 1 \
            16.025 12 Z M15.7 18 A2.3 2.3 0 0 1 20.3 18 A2.3 2.3 0 0 1 15.7 18 Z",
    )],
    solid: &[],
};

pub(super) const LIGHT_TABLE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SLAB),
        Part::Stroke("M12 9.5 V5.75 M7.411 10.947 L5.26 7.875 M16.589 10.947 L18.74 7.875"),
    ],
    solid: &[],
};

pub(super) const LIGHT_TABLE_ADD: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SLAB),
        Part::Stroke("M10.75 9.5 V5.75 M6.161 10.947 L4.01 7.875"),
        Part::Clear(PLUS_HIGH),
        Part::Stroke(PLUS_HIGH),
    ],
    solid: &[],
};

pub(super) const LIGHT_TABLE_REMOVE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SLAB),
        Part::Stroke("M10.75 9.5 V5.75 M6.161 10.947 L4.01 7.875"),
        Part::Clear(MINUS_HIGH),
        Part::Stroke(MINUS_HIGH),
    ],
    solid: &[],
};

pub(super) const LIGHT_TABLE_FILE: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SLAB),
        Part::Stroke("M10.75 9.5 V5.75 M6.161 10.947 L4.01 7.875"),
        Part::Clear(
            "M16.25 4.75 H18.75 L21.25 7.25 V9.75 A1 1 0 0 1 20.25 10.75 H17.25 A1 1 0 0 1 16.25 \
            9.75 Z",
        ),
        Part::Stroke(
            "M16.25 4.75 H18.75 L21.25 7.25 V9.75 A1 1 0 0 1 20.25 10.75 H17.25 A1 1 0 0 1 16.25 \
            9.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const LIGHT_TABLE_TOOL: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SLAB),
        Part::Stroke("M10.75 9.5 V5.75 M6.161 10.947 L4.01 7.875"),
        Part::Clear(
            "M16.25 4.75 L21.25 9 L18.85 9.4 L20.1 12 L18.95 12.55 L17.7 9.95 L16.25 11.75 Z",
        ),
        Part::Fill(
            "M16.25 4.75 L21.25 9 L18.85 9.4 L20.1 12 L18.95 12.55 L17.7 9.95 L16.25 11.75 Z",
        ),
        Part::Stroke(
            "M16.25 4.75 L21.25 9 L18.85 9.4 L20.1 12 L18.95 12.55 L17.7 9.95 L16.25 11.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const CEL_LOCK: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H15.25 A2 2 0 0 1 17.25 6.75 V14.25 A2 2 0 0 1 \
            15.25 16.25 H4.75 A2 2 0 0 1 2.75 14.25 V6.75 Z",
        ),
        Part::Stroke("M6.25 8.75 H7.5 M12.5 8.75 H13.75"),
        Part::Fill("M9 8.75 A1 1 0 0 1 11 8.75 A1 1 0 0 1 9 8.75 Z"),
        Part::Clear(PAD_BODY),
        Part::Clear(PAD_SHACKLE),
        Part::Stroke(PAD_BODY),
        Part::Stroke(PAD_SHACKLE),
    ],
    solid: &[],
};
