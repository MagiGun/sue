//! Group C: the icons of editing and of the view: undo and redo, zoom,
//! flipping and turning, yes and no, shown and locked, and the window's
//! own switches.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// The ring every circle icon is drawn in: 20 units across with the pen.
const RING: &str = "M2.75 12 A9.25 9.25 0 0 1 21.25 12 A9.25 9.25 0 0 1 2.75 12 Z";

/// An icon that is one line and nothing else.
const fn line(keyline: Keyline, line: &'static [Part]) -> Art {
    Art {
        keyline,
        line,
        solid: &[],
    }
}

pub(super) const UNDO: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M4.75 8.75 H14.25 A5.25 5.25 0 0 1 14.25 19.25 H8.75"),
        Part::Stroke("M8.75 4.75 L4.75 8.75 L8.75 12.75"),
    ],
);

pub(super) const REDO: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M19.25 8.75 H9.75 A5.25 5.25 0 0 0 9.75 19.25 H15.25"),
        Part::Stroke("M15.25 4.75 L19.25 8.75 L15.25 12.75"),
    ],
);

/// The lens of the two zoom icons.
const LENS: &str = "M3.75 10.75 A7 7 0 0 1 17.75 10.75 A7 7 0 0 1 3.75 10.75 Z";

/// The handle of that lens.
const HANDLE: &str = "M15.7 15.7 L20.25 20.25";

pub(super) const ZOOM_IN: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(LENS),
        Part::Stroke(HANDLE),
        Part::Stroke("M10.75 7.75 V13.75 M7.75 10.75 H13.75"),
    ],
);

pub(super) const ZOOM_OUT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(LENS),
        Part::Stroke(HANDLE),
        Part::Stroke("M7.75 10.75 H13.75"),
    ],
);

pub(super) const ZOOM_FIT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M3.75 8.25 V5.75 A2 2 0 0 1 5.75 3.75 H8.25 M15.75 3.75 H18.25 A2 2 0 0 1 20.25 5.75 \
             V8.25 M20.25 15.75 V18.25 A2 2 0 0 1 18.25 20.25 H15.75 M8.25 20.25 H5.75 A2 2 0 0 1 \
             3.75 18.25 V15.75",
        ),
        Part::Stroke(
            "M9.75 8.75 H14.25 A1 1 0 0 1 15.25 9.75 V14.25 A1 1 0 0 1 14.25 15.25 H9.75 A1 1 0 0 \
             1 8.75 14.25 V9.75 A1 1 0 0 1 9.75 8.75 Z",
        ),
    ],
);

pub(super) const ZOOM_ACTUAL: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke("M5.5 8.75 L7.75 6.75 V17.25 M16 8.75 L18.25 6.75 V17.25"),
        Part::Fill(
            "M11.4 9.5 A1.1 1.1 0 0 1 13.6 9.5 A1.1 1.1 0 0 1 11.4 9.5 Z M11.4 14.5 A1.1 1.1 0 0 \
             1 13.6 14.5 A1.1 1.1 0 0 1 11.4 14.5 Z",
        ),
    ],
);

/// The solid half of [`FLIP_HORIZONTAL`].
const FLIPPED_RIGHT: &str = "M15.25 5.75 V18.25 L20.25 12 Z";

pub(super) const FLIP_HORIZONTAL: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 V20.25"),
        Part::Stroke("M8.75 5.75 V18.25 L3.75 12 Z"),
        Part::Fill(FLIPPED_RIGHT),
        Part::Stroke(FLIPPED_RIGHT),
    ],
);

/// The solid half of [`FLIP_VERTICAL`].
const FLIPPED_DOWN: &str = "M5.75 15.25 H18.25 L12 20.25 Z";

pub(super) const FLIP_VERTICAL: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M3.75 12 H20.25"),
        Part::Stroke("M5.75 8.75 H18.25 L12 3.75 Z"),
        Part::Fill(FLIPPED_DOWN),
        Part::Stroke(FLIPPED_DOWN),
    ],
);

pub(super) const ROTATE_LEFT: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M14.25 10.75 H19.25 A2 2 0 0 1 21.25 12.75 V17.25 A2 2 0 0 1 19.25 19.25 H14.25 A2 2 \
             0 0 1 12.25 17.25 V12.75 A2 2 0 0 1 14.25 10.75 Z",
        ),
        Part::Stroke("M16.75 5.25 A9.75 9.75 0 0 0 7 15"),
        Part::Stroke("M4 12 L7 15 L10 12"),
    ],
);

pub(super) const ROTATE_RIGHT: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M4.75 10.75 H9.75 A2 2 0 0 1 11.75 12.75 V17.25 A2 2 0 0 1 9.75 19.25 H4.75 A2 2 0 0 \
             1 2.75 17.25 V12.75 A2 2 0 0 1 4.75 10.75 Z",
        ),
        Part::Stroke("M7.25 5.25 A9.75 9.75 0 0 1 17 15"),
        Part::Stroke("M14 12 L17 15 L20 12"),
    ],
);

pub(super) const RESET: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke("M7.23 16.77 A6.75 6.75 0 1 0 7.23 7.23"),
        Part::Stroke("M10.22 6.97 L7.23 7.23 L7.49 4.24"),
        Part::Fill("M10.25 12 A1.75 1.75 0 0 1 13.75 12 A1.75 1.75 0 0 1 10.25 12 Z"),
    ],
);

pub(super) const SWAP: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M3.75 7.75 H20.25 M16.5 4 L20.25 7.75 L16.5 11.5"),
        Part::Stroke("M20.25 16.25 H3.75 M7.5 12.5 L3.75 16.25 L7.5 20"),
    ],
);

/// The solid square that takes the other's place in [`REPLACE`].
const SQUARE_NEW: &str = "M15.25 14.25 H19.25 A1 1 0 0 1 20.25 15.25 V19.25 A1 1 0 0 1 19.25 20.25 \
                          H15.25 A1 1 0 0 1 14.25 19.25 V15.25 A1 1 0 0 1 15.25 14.25 Z";

pub(super) const REPLACE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M4.75 3.75 H8.75 A1 1 0 0 1 9.75 4.75 V8.75 A1 1 0 0 1 8.75 9.75 H4.75 A1 1 0 0 1 \
             3.75 8.75 V4.75 A1 1 0 0 1 4.75 3.75 Z",
        ),
        Part::Fill(SQUARE_NEW),
        Part::Stroke(SQUARE_NEW),
        Part::Stroke("M12.75 6.75 H13.75 A3.5 3.5 0 0 1 17.25 10.25 V11.25"),
        Part::Stroke("M15 9 L17.25 11.25 L19.5 9"),
    ],
);

pub(super) const REVERSE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M7.75 4.75 V19.25 M4.25 8.25 L7.75 4.75 L11.25 8.25 M4.25 15.75 L7.75 19.25 L11.25 \
             15.75",
        ),
        Part::Stroke("M14.25 6.75 H20.25 M14.25 12 H20.25 M14.25 17.25 H20.25"),
    ],
);

pub(super) const CONFIRM: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M7.5 12.25 L10.75 15.5 L16.5 8.75"),
    ],
);

pub(super) const CANCEL: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M8.5 8.5 L15.5 15.5 M15.5 8.5 L8.5 15.5"),
    ],
);

/// The stroke that crosses the eye out.
const SLASH: &str = "M4.75 4.75 L19.25 19.25";

pub(super) const EYE_OFF: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M2.75 12 C5.5 8 8.5 5.75 12 5.75 C15.5 5.75 18.5 8 21.25 12 C18.5 16 15.5 18.25 12 \
             18.25 C8.5 18.25 5.5 16 2.75 12 Z",
        ),
        Part::Stroke("M8.75 12 A3.25 3.25 0 0 1 15.25 12 A3.25 3.25 0 0 1 8.75 12 Z"),
        Part::Clear(SLASH),
        Part::Stroke(SLASH),
    ],
);

pub(super) const UNLOCK: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke("M7.75 10.75 V7 A4.25 4.25 0 0 1 15.99 5.55"),
        Part::Stroke(
            "M6.75 10.75 H17.25 A2 2 0 0 1 19.25 12.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 0 \
             0 1 4.75 19.25 V12.75 A2 2 0 0 1 6.75 10.75 Z",
        ),
        Part::Stroke("M12 15 V17"),
    ],
);

pub(super) const FULLSCREEN: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M13.75 3.75 H20.25 V10.25 M20.25 3.75 L13.5 10.5"),
        Part::Stroke("M10.25 20.25 H3.75 V13.75 M3.75 20.25 L10.5 13.5"),
    ],
);

pub(super) const KEYBOARD: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M4.75 5.75 H19.25 A2 2 0 0 1 21.25 7.75 V16.25 A2 2 0 0 1 19.25 18.25 H4.75 A2 2 0 0 \
             1 2.75 16.25 V7.75 A2 2 0 0 1 4.75 5.75 Z",
        ),
        Part::Fill(
            "M5.75 9.75 A1 1 0 0 1 7.75 9.75 A1 1 0 0 1 5.75 9.75 Z M9.25 9.75 A1 1 0 0 1 11.25 \
             9.75 A1 1 0 0 1 9.25 9.75 Z M12.75 9.75 A1 1 0 0 1 14.75 9.75 A1 1 0 0 1 12.75 9.75 \
             Z M16.25 9.75 A1 1 0 0 1 18.25 9.75 A1 1 0 0 1 16.25 9.75 Z",
        ),
        Part::Stroke("M8.25 14.25 H15.75"),
    ],
);

/// The crescent of the moon.
const CRESCENT: &str = "M19.6 13.77 A8 8 0 1 1 10.23 4.4 A7 7 0 0 0 19.6 13.77 Z";

pub(super) const MOON: Art = line(Keyline::Square, &[Part::Stroke(CRESCENT)]);

pub(super) const SUN: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke("M8 12 A4 4 0 0 1 16 12 A4 4 0 0 1 8 12 Z"),
        Part::Stroke(
            "M12 2.75 V4.75 M12 19.25 V21.25 M2.75 12 H4.75 M19.25 12 H21.25 M5.46 5.46 L6.87 \
             6.87 M17.13 6.87 L18.54 5.46 M5.46 18.54 L6.87 17.13 M17.13 17.13 L18.54 18.54",
        ),
    ],
);

pub(super) const LANGUAGE: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M12 2.75 A4 9.25 0 0 1 12 21.25 A4 9.25 0 0 1 12 2.75 Z"),
        Part::Stroke("M2.75 12 H21.25"),
    ],
);

pub(super) const UPDATE: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M12 16.75 V7.5 M8 11.5 L12 7.5 L16 11.5"),
    ],
);
