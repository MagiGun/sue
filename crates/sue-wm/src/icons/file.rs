//! Group B: the icons of files and of what is done with data: opening,
//! saving, bringing in and sending out, the clipboard, labels and tools.
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

/// A sheet of paper with its top right corner cut for the fold.
const SHEET: &str = "M6.75 2.75 H13.25 L19.25 8.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 0 0 \
                     1 4.75 19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z";

/// The folded corner of that sheet.
const FOLD: &str = "M13.25 2.75 V7.25 A1.5 1.5 0 0 0 14.75 8.75 H19.25 Z";

pub(super) const FILE_NEW: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(SHEET),
        Part::Fill(FOLD),
        Part::Stroke(FOLD),
        Part::Stroke("M12 12.25 V18.25 M9 15.25 H15"),
    ],
);

/// A closed folder seen from the front, as the reference icon draws it.
const FOLDER: &str = "M2.75 6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H19.25 A2 2 0 0 1 21.25 9.75 \
                      V17.25 A2 2 0 0 1 19.25 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z";

/// The back of an open folder: its left side, its tab and its top.
const FOLDER_BACK: &str = "M2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75 H9 L12 7.75 H17.25 A2 2 0 0 1 \
                           19.25 9.75 V10.75";

/// The front of an open folder, leaning to the right.
const FOLDER_FRONT: &str = "M2.75 17.25 L5.5 11.75 A2 2 0 0 1 7.25 10.75 H21.25 L18.5 18.25 A2 2 \
                            0 0 1 16.75 19.25 H4.75 A2 2 0 0 1 2.75 17.25 Z";

/// The folded corner of the sheet that stands in [`FILE_OPEN`].
const OPEN_FOLD: &str = "M13.25 4.75 V6.75 A1 1 0 0 0 14.25 7.75 H16.25 Z";

pub(super) const FILE_OPEN: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke("M2.75 17.25 V6.75 A2 2 0 0 1 4.75 4.75"),
        Part::Stroke("M7.75 10.75 V5.75 A1 1 0 0 1 8.75 4.75 H13.25 L16.25 7.75 V10.75"),
        Part::Fill(OPEN_FOLD),
        Part::Stroke(OPEN_FOLD),
        Part::Stroke(FOLDER_FRONT),
    ],
);

pub(super) const FOLDER_OPEN: Art = line(
    Keyline::Wide,
    &[Part::Stroke(FOLDER_BACK), Part::Stroke(FOLDER_FRONT)],
);

pub(super) const FOLDER_NEW: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(FOLDER),
        Part::Stroke("M12 10.75 V15.75 M9.5 13.25 H14.5"),
    ],
);

/// The case of a storage disk: a square with its top right corner cut.
const DISK: &str = "M5.75 3.75 H15.5 L20.25 8.5 V18.25 A2 2 0 0 1 18.25 20.25 H5.75 A2 2 0 0 1 \
                    3.75 18.25 V5.75 A2 2 0 0 1 5.75 3.75 Z";

/// The shutter at the top of the disk.
const SHUTTER: &str = "M7.75 3.75 V8.75 H14.25 V3.75";

pub(super) const SAVE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(DISK),
        Part::Stroke(SHUTTER),
        Part::Stroke("M7.75 20.25 V14.25 H16.25 V20.25"),
    ],
);

pub(super) const SAVE_AS: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(DISK),
        Part::Stroke(SHUTTER),
        Part::Fill(
            "M6.65 15.5 A1.1 1.1 0 0 1 8.85 15.5 A1.1 1.1 0 0 1 6.65 15.5 Z M10.9 15.5 A1.1 1.1 0 \
             0 1 13.1 15.5 A1.1 1.1 0 0 1 10.9 15.5 Z M15.15 15.5 A1.1 1.1 0 0 1 17.35 15.5 A1.1 \
             1.1 0 0 1 15.15 15.5 Z",
        ),
    ],
);

pub(super) const IMPORT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M13.25 3.75 H18.25 A2 2 0 0 1 20.25 5.75 V18.25 A2 2 0 0 1 18.25 20.25 H13.25",
        ),
        Part::Stroke("M3.75 12 H14.75 M10.75 8 L14.75 12 L10.75 16"),
    ],
);

pub(super) const EXPORT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M10.75 3.75 H5.75 A2 2 0 0 0 3.75 5.75 V18.25 A2 2 0 0 0 5.75 20.25 H10.75"),
        Part::Stroke("M9.25 12 H20.25 M16.25 8 L20.25 12 L16.25 16"),
    ],
);

/// The arrow that leaves the sheet of [`EXPORT_PDF`].
const OUT: &str = "M8.75 14.25 H20.25 M17.5 11.5 L20.25 14.25 L17.5 17";

/// The folded corner of the narrow sheet of [`EXPORT_PDF`].
const NARROW_FOLD: &str = "M9.75 3.75 V6.25 A1.5 1.5 0 0 0 11.25 7.75 H13.25 Z";

pub(super) const EXPORT_PDF: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M5.75 3.75 H9.75 L13.25 7.75 V18.25 A2 2 0 0 1 11.25 20.25 H5.75 A2 2 0 0 1 3.75 \
             18.25 V5.75 A2 2 0 0 1 5.75 3.75 Z",
        ),
        Part::Fill(NARROW_FOLD),
        Part::Stroke(NARROW_FOLD),
        Part::Clear(OUT),
        Part::Stroke(OUT),
    ],
);

pub(super) const PRINT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 8.75 V3.75 H16.25 V8.75"),
        Part::Stroke(
            "M7.75 17.25 H5.75 A2 2 0 0 1 3.75 15.25 V10.75 A2 2 0 0 1 5.75 8.75 H18.25 A2 2 0 0 \
             1 20.25 10.75 V15.25 A2 2 0 0 1 18.25 17.25 H16.25",
        ),
        Part::Stroke("M7.75 14.25 H16.25 V20.25 H7.75 Z"),
    ],
);

pub(super) const DOWNLOAD: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 V15.5 M7.5 11 L12 15.5 L16.5 11"),
        Part::Stroke(
            "M3.75 15.25 V18.25 A2 2 0 0 0 5.75 20.25 H18.25 A2 2 0 0 0 20.25 18.25 V15.25",
        ),
    ],
);

pub(super) const TRANSFERS: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 19.5 V4.5 M3.75 8.5 L7.75 4.5 L11.75 8.5"),
        Part::Stroke("M16.25 4.5 V19.5 M12.25 15.5 L16.25 19.5 L20.25 15.5"),
    ],
);

/// The sheet in front of [`COPY`] and [`DUPLICATE`].
const FRONT: &str = "M10.75 7.75 H17.25 A2 2 0 0 1 19.25 9.75 V19.25 A2 2 0 0 1 17.25 21.25 \
                     H10.75 A2 2 0 0 1 8.75 19.25 V9.75 A2 2 0 0 1 10.75 7.75 Z";

/// What shows of the sheet behind it.
const BEHIND: &str = "M4.75 15.25 V4.75 A2 2 0 0 1 6.75 2.75 H14.25";

pub(super) const COPY: Art = line(Keyline::Tall, &[Part::Stroke(FRONT), Part::Stroke(BEHIND)]);

pub(super) const DUPLICATE: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(FRONT),
        Part::Stroke(BEHIND),
        Part::Stroke("M14.25 11.25 V17.25 M11.25 14.25 H17.25"),
    ],
);

pub(super) const CUT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M3.75 17.5 A2.75 2.75 0 0 1 9.25 17.5 A2.75 2.75 0 0 1 3.75 17.5 Z M14.75 17.5 A2.75 \
             2.75 0 0 1 20.25 17.5 A2.75 2.75 0 0 1 14.75 17.5 Z",
        ),
        Part::Stroke("M8.44 15.56 L19.5 4.5 M15.56 15.56 L4.5 4.5"),
    ],
);

pub(super) const PASTE: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke(
            "M8.75 4.75 H6.75 A2 2 0 0 0 4.75 6.75 V19.25 A2 2 0 0 0 6.75 21.25 H17.25 A2 2 0 0 0 \
             19.25 19.25 V6.75 A2 2 0 0 0 17.25 4.75 H15.25",
        ),
        Part::Stroke(
            "M9.75 2.75 H14.25 A1 1 0 0 1 15.25 3.75 V5.75 A1 1 0 0 1 14.25 6.75 H9.75 A1 1 0 0 1 \
             8.75 5.75 V3.75 A1 1 0 0 1 9.75 2.75 Z",
        ),
        Part::Stroke("M8.75 12 H15.25 M8.75 16.25 H13"),
    ],
);

pub(super) const RENAME: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(
            "M4.75 5.75 H19.25 A2 2 0 0 1 21.25 7.75 V16.25 A2 2 0 0 1 19.25 18.25 H4.75 A2 2 0 0 \
             1 2.75 16.25 V7.75 A2 2 0 0 1 4.75 5.75 Z",
        ),
        Part::Stroke("M6.25 12 H10.75"),
        Part::Stroke("M13.75 8.75 H16.75 M15.25 8.75 V15.25 M13.75 15.25 H16.75"),
    ],
);

pub(super) const EDIT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M4.5 19.5 L5.75 14.75 L15.9 4.6 A1.5 1.5 0 0 1 18 4.6 L19.4 6 A1.5 1.5 0 0 1 19.4 \
             8.1 L9.25 18.25 Z",
        ),
        Part::Stroke("M13.5 7 L17 10.5"),
        Part::Stroke("M13.5 20.25 H20.25"),
    ],
);

pub(super) const TAG: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M3.75 5.75 A2 2 0 0 1 5.75 3.75 H12.17 A2 2 0 0 1 13.59 4.34 L19.66 10.41 A2 2 0 0 1 \
             19.66 13.24 L13.24 19.66 A2 2 0 0 1 10.41 19.66 L4.34 13.59 A2 2 0 0 1 3.75 12.17 Z",
        ),
        Part::Fill("M6.9 8.25 A1.35 1.35 0 0 1 9.6 8.25 A1.35 1.35 0 0 1 6.9 8.25 Z"),
    ],
);

pub(super) const MOVE_TO: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke(FOLDER),
        Part::Stroke("M5.75 13.25 H17.25 M14.5 10.5 L17.25 13.25 L14.5 16"),
    ],
);

pub(super) const RECENT: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M12 6.75 V12 L15.75 14.25"),
    ],
);

pub(super) const HISTORY: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M5.68 17.3 A8.25 8.25 0 1 0 4.42 8.75"),
        Part::Stroke("M3.75 3.75 V8.75 H9.25"),
        Part::Stroke("M12 7.5 V12 L15.25 14"),
    ],
);

pub(super) const LINK: Art = line(
    Keyline::Wide,
    &[
        Part::Stroke("M10.25 7.75 H7 A4.25 4.25 0 0 0 7 16.25 H10.25"),
        Part::Stroke("M13.75 7.75 H17 A4.25 4.25 0 0 1 17 16.25 H13.75"),
        Part::Stroke("M8.25 12 H15.75"),
    ],
);

pub(super) const WRENCH: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M17.26 4.26 L13.94 7.59 A1.75 1.75 0 0 0 16.41 10.06 L19.74 6.74 A5.25 5.25 0 0 1 12.88 \
         13.8 L6.99 19.69 A1.9 1.9 0 0 1 4.31 17.01 L10.2 11.12 A5.25 5.25 0 0 1 17.26 4.26 Z",
    )],
);

pub(super) const ALL: Art = line(
    Keyline::Circle,
    &[Part::Stroke(
        "M12 3.75 V20.25 M4.86 7.88 L19.14 16.12 M19.14 7.88 L4.86 16.12",
    )],
);
