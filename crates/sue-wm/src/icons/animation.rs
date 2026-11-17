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
        "M9.25 7.75 H14.75 A1.5 1.5 0 0 1 16.25 9.25 V14.75 A1.5 1.5 0 0 1 14.75 16.25 H9.25 A1.5          1.5 0 0 1 7.75 14.75 V9.25 A1.5 1.5 0 0 1 9.25 7.75 Z",
    )],
    solid: &[],
};

/// Two upright bars with round ends, side by side: hold the playing
/// where it is.
pub(super) const PAUSE: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(
        "M6.75 8.25 A1.5 1.5 0 0 1 9.75 8.25 V15.75 A1.5 1.5 0 0 1 6.75 15.75 Z M14.25 8.25 A1.5 \
         1.5 0 0 1 17.25 8.25 V15.75 A1.5 1.5 0 0 1 14.25 15.75 Z",
    )],
    solid: &[],
};

/// A diamond outline, the mark of a key on the timeline, with a plus at
/// its lower right: add a keyframe.
pub(super) const KEYFRAME_ADD: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M10 5 L16 11 L10 17 L4 11 Z"),
        Part::Stroke("M18.25 15.25 V20.25 M15.25 18.25 H20.25"),
    ],
    solid: &[],
};

/// The diamond of a key, alone: one value keyed at this frame, solid
/// when it is.
const KEY_DIAMOND: &str = "M12 5.25 L18.75 12 L12 18.75 L5.25 12 Z";

pub(super) const KEYFRAME: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(KEY_DIAMOND)],
    solid: &[Part::Fill(KEY_DIAMOND), Part::Stroke(KEY_DIAMOND)],
};

/// A solid diamond on a track that runs through it: keyframes on.
const KEY_ON_TRACK: &str = "M12 8 L16 12 L12 16 L8 12 Z";

pub(super) const KEYFRAMES: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 12 H5 M19 12 H21.25"),
        Part::Fill(KEY_ON_TRACK),
        Part::Stroke(KEY_ON_TRACK),
    ],
    solid: &[],
};

/// The two solid triangles of first frame, pointing to the left.
const TWO_LEFT: &str = "M9.25 12 L14.25 7.5 V16.5 Z M14.25 12 L19.25 7.5 V16.5 Z";

pub(super) const FRAME_FIRST: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M5.75 7 V17"),
        Part::Fill(TWO_LEFT),
        Part::Stroke(TWO_LEFT),
    ],
    solid: &[],
};

/// The two solid triangles of last frame, pointing to the right.
const TWO_RIGHT: &str = "M14.75 12 L9.75 7.5 V16.5 Z M9.75 12 L4.75 7.5 V16.5 Z";

pub(super) const FRAME_LAST: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M18.25 7 V17"),
        Part::Fill(TWO_RIGHT),
        Part::Stroke(TWO_RIGHT),
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

/// The ball as it is now, solid in front of where it was.
const BALL: &str = "M11 12 A5 5 0 0 1 21 12 A5 5 0 0 1 11 12 Z";

pub(super) const ONION_SKIN: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M5.13 7.91 A5 5 0 0 0 5.13 16.09"),
        Part::Stroke("M9.13 7.91 A5 5 0 0 0 9.13 16.09"),
        Part::Fill(BALL),
        Part::Stroke(BALL),
    ],
    solid: &[],
};

/// The head of the playhead on the track of a new timeline.
const PLAYHEAD: &str = "M12.25 4.75 H16.25 V6.75 L14.25 8.75 L12.25 6.75 Z";

pub(super) const TIMELINE_NEW: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 10.75 H21.25 M2.75 15.25 H10.25"),
        Part::Clear("M14.25 8.75 V13.25"),
        Part::Stroke("M14.25 8.75 V13.25"),
        Part::Fill(PLAYHEAD),
        Part::Stroke(PLAYHEAD),
        Part::Stroke(PLUS),
    ],
    solid: &[],
};

/// The play triangle inside the animation folder.
const INNER_PLAY: &str = "M9.75 9.75 L17 13.5 L9.75 17.25 Z";

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
