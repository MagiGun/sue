//! Group I: the icons of animation and the timeline. Playback, stepping
//! through frames, cels, onion skin, a new timeline and the animation
//! folder.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

pub(super) const PLAY: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(
        "M7.75 6.66 A1.25 1.25 0 0 1 9.62 5.58 L18.87 10.92 A1.25 1.25 0 0 1 18.87 13.08 L9.62 \
         18.42 A1.25 1.25 0 0 1 7.75 17.34 Z",
    )],
    solid: &[],
};

pub(super) const STOP: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(
        "M7.75 5.75 H16.25 A2 2 0 0 1 18.25 7.75 V16.25 A2 2 0 0 1 16.25 18.25 H7.75 A2 2 0 0 1 \
         5.75 16.25 V7.75 A2 2 0 0 1 7.75 5.75 Z",
    )],
    solid: &[],
};

pub(super) const FRAME_FIRST: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M5.75 7 V17"),
        Part::Stroke("M9.25 12 L14.25 7.5 V16.5 Z"),
        Part::Stroke("M14.25 12 L19.25 7.5 V16.5 Z"),
    ],
    solid: &[],
};

pub(super) const FRAME_LAST: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M18.25 7 V17"),
        Part::Stroke("M14.75 12 L9.75 7.5 V16.5 Z"),
        Part::Stroke("M9.75 12 L4.75 7.5 V16.5 Z"),
    ],
    solid: &[],
};

pub(super) const FRAME_PREV: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M18.25 7 V17"),
        Part::Stroke(
            "M12.73 7.17 A1 1 0 0 1 14.25 8.02 L14.25 15.98 A1 1 0 0 1 12.73 16.83 L6.16 12.86 \
             A1 1 0 0 1 6.16 11.14 Z",
        ),
    ],
    solid: &[],
};

pub(super) const FRAME_NEXT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M5.75 7 V17"),
        Part::Stroke(
            "M9.75 8.02 A1 1 0 0 1 11.27 7.17 L17.84 11.14 A1 1 0 0 1 17.84 12.86 L11.27 16.83 \
             A1 1 0 0 1 9.75 15.98 Z",
        ),
    ],
    solid: &[],
};

pub(super) const LOOP: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M3.75 12 V10.75 A3 3 0 0 1 6.75 7.75 H19.5"),
        Part::Stroke("M16.5 4.75 L19.5 7.75 L16.5 10.75"),
        Part::Stroke("M20.25 12 V13.25 A3 3 0 0 1 17.25 16.25 H4.5"),
        Part::Stroke("M7.5 13.25 L4.5 16.25 L7.5 19.25"),
    ],
    solid: &[],
};

/// The sheet of a cel, whole.
const SHEET: &str = "M4.75 4.75 H19.25 A2 2 0 0 1 21.25 6.75 V17.25 A2 2 0 0 1 19.25 19.25 H4.75 \
                     A2 2 0 0 1 2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 Z";

/// The sheet of a cel, open where its lower right corner would be, to
/// leave room for a mark.
const SHEET_OPEN: &str = "M21.25 10.25 V6.75 A2 2 0 0 0 19.25 4.75 H4.75 A2 2 0 0 0 2.75 6.75 \
                          V17.25 A2 2 0 0 0 4.75 19.25 H12.25";

/// The two slots of the peg holes along the top of a cel.
const SLOTS: &str = "M6.5 8.75 H8.5 M15.5 8.75 H17.5";

/// The round peg hole between the slots.
const PEG: &str = "M10.9 8.75 A1.1 1.1 0 0 1 13.1 8.75 A1.1 1.1 0 0 1 10.9 8.75 Z";

pub(super) const CEL: Art = Art {
    keyline: Keyline::Wide,
    line: &[Part::Stroke(SHEET), Part::Stroke(SLOTS), Part::Fill(PEG)],
    solid: &[],
};

/// A small plus in the lower right corner of a wide icon: something new.
const PLUS: &str = "M18.25 13.75 V18.75 M15.75 16.25 H20.75";

pub(super) const CEL_NEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SHEET_OPEN),
        Part::Stroke(SLOTS),
        Part::Fill(PEG),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

/// The key in the lower right corner of the cel a frame shows.
const KEY: &str = "M18.25 13.75 L20.75 16.25 L18.25 18.75 L15.75 16.25 Z";

pub(super) const CEL_SPECIFY: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(SHEET_OPEN),
        Part::Stroke(SLOTS),
        Part::Fill(PEG),
        Part::Fill(KEY),
        Part::Stroke(KEY),
    ],
    solid: &[],
};

/// The bulb of the onion.
const BULB: &str = "M12 5.75 C13 8.25 19.25 9.5 19.25 14.25 C19.25 18.25 16 21.25 12 21.25 C8 \
                    21.25 4.75 18.25 4.75 14.25 C4.75 9.5 11 8.25 12 5.75 Z";

pub(super) const ONION_SKIN: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(BULB),
        Part::Stroke("M12 5.75 V2.75"),
        Part::Stroke("M12 5.75 C7.5 11 7.5 17 12 21.25 M12 5.75 C16.5 11 16.5 17 12 21.25"),
    ],
    solid: &[],
};

/// The two keys on the track of a new timeline.
const KEYS: &str = "M7 6.25 L9.5 8.75 L7 11.25 L4.5 8.75 Z M15 6.25 L17.5 8.75 L15 11.25 L12.5 \
                    8.75 Z";

pub(super) const TIMELINE_NEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 8.75 H21.25"),
        Part::Fill(KEYS),
        Part::Stroke(KEYS),
        Part::Stroke("M2.75 16.25 H12.25"),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

/// The play triangle inside the animation folder.
const INNER_PLAY: &str = "M10.75 11 L14.75 13.5 L10.75 16 Z";

pub(super) const ANIMATION_FOLDER: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke(
            "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H19.25 A2 2 0 0 1 21.25 9.75 V17.25 A2 \
             2 0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z",
        ),
        Part::Fill(INNER_PLAY),
        Part::Stroke(INNER_PLAY),
    ],
    solid: &[],
};
