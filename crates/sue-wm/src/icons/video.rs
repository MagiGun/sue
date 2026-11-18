//! Group L: the icons of video editing (Sue VE). The studio, the editing
//! tools, snapping, markers, the in and out marks, inserting and
//! overwriting, and effects.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else.

use super::{Art, Keyline, Part};

/// An icon that is one line and nothing else.
const fn line(keyline: Keyline, line: &'static [Part]) -> Art {
    Art {
        keyline,
        line,
        solid: &[],
    }
}

pub(super) const APP_VIDEO: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M4.75 10.75 H19.25 V17.75 A1.5 1.5 0 0 1 17.75 19.25 H6.25 A1.5 1.5 0 0 1 4.75 \
             17.75 Z",
        ),
        Part::Stroke("M4.6 8.4 L18.2 4.75 L19 7.5 L5.4 11.15 Z"),
        Part::Stroke("M9.1 7.2 L10.9 9.6 M13.6 6 L15.4 8.4"),
    ],
);

pub(super) const TOOL_RAZOR: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 4.75 H16.25 V11.75 H7.75 Z"),
        Part::Stroke("M12 11.75 V20.25"),
        Part::Stroke("M10.25 8.75 H13.75"),
    ],
);

pub(super) const TOOL_RIPPLE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M9.75 5.75 H6.75 V18.25 H9.75"),
        Part::Stroke("M11.75 12 H19.25"),
        Part::Stroke("M16.5 9.25 L19.25 12 L16.5 14.75"),
    ],
);

pub(super) const TOOL_ROLL: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M8.75 5.75 H10.75 V18.25 H8.75"),
        Part::Stroke("M15.25 5.75 H13.25 V18.25 H15.25"),
        Part::Stroke("M6.25 9.75 L3.75 12 L6.25 14.25 M17.75 9.75 L20.25 12 L17.75 14.25"),
    ],
);

pub(super) const TOOL_SLIP: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M3.75 7.75 H20.25 V16.25 H3.75 Z"),
        Part::Stroke("M7.75 12 H16.25"),
        Part::Stroke("M10 9.75 L7.75 12 L10 14.25 M14 9.75 L16.25 12 L14 14.25"),
    ],
);

pub(super) const TOOL_SLIDE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M9.75 10.75 H14.25 V17.25 H9.75 Z"),
        Part::Stroke("M3.75 10.75 H6.75 V17.25 H3.75 M20.25 10.75 H17.25 V17.25 H20.25"),
        Part::Stroke("M8.75 5.75 H15.25"),
        Part::Stroke("M10.75 3.75 L8.75 5.75 L10.75 7.75 M13.25 3.75 L15.25 5.75 L13.25 7.75"),
    ],
);

pub(super) const MAGNET: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M5.75 4.75 H9.75 V12 A2.25 2.25 0 0 0 14.25 12 V4.75 H18.25 V12 A6.25 6.25 0 0 1 \
             5.75 12 Z",
        ),
        Part::Stroke("M5.75 8.75 H9.75 M14.25 8.75 H18.25"),
    ],
);

pub(super) const MARKER: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M7.75 4.75 H16.25 V14.25 L12 19.25 L7.75 14.25 Z",
    )],
);

pub(super) const MARK_IN: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M16.25 5.75 H10.75 V18.25 H16.25"),
        Part::Stroke("M6.75 12 H8.25"),
    ],
);

pub(super) const MARK_OUT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 5.75 H13.25 V18.25 H7.75"),
        Part::Stroke("M15.75 12 H17.25"),
    ],
);

pub(super) const INSERT: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 V11.25"),
        Part::Stroke("M9.25 8.5 L12 11.25 L14.75 8.5"),
        Part::Stroke("M3.75 14.25 H9.75 V19.25 H3.75 M20.25 14.25 H14.25 V19.25 H20.25"),
    ],
);

pub(super) const OVERWRITE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M12 3.75 V10.25"),
        Part::Stroke("M9.25 7.5 L12 10.25 L14.75 7.5"),
        Part::Stroke("M3.75 14.25 H20.25 V19.25 H3.75 Z"),
    ],
);

pub(super) const EFFECTS: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M12 3.75 L13.9 10.1 L20.25 12 L13.9 13.9 L12 20.25 L10.1 13.9 L3.75 12 L10.1 10.1 Z",
    )],
);

#[cfg(test)]
mod tests {
    use super::super::Icon;

    /// The table holds every icon of this group, and each one draws with
    /// the art of this file.
    #[test]
    fn the_group_is_whole() {
        let group = [
            (Icon::AppVideo, &super::APP_VIDEO),
            (Icon::ToolRazor, &super::TOOL_RAZOR),
            (Icon::ToolRipple, &super::TOOL_RIPPLE),
            (Icon::ToolRoll, &super::TOOL_ROLL),
            (Icon::ToolSlip, &super::TOOL_SLIP),
            (Icon::ToolSlide, &super::TOOL_SLIDE),
            (Icon::Magnet, &super::MAGNET),
            (Icon::Marker, &super::MARKER),
            (Icon::MarkIn, &super::MARK_IN),
            (Icon::MarkOut, &super::MARK_OUT),
            (Icon::Insert, &super::INSERT),
            (Icon::Overwrite, &super::OVERWRITE),
            (Icon::Effects, &super::EFFECTS),
        ];
        for (icon, art) in group {
            assert_eq!(icon.art(), art, "{}", icon.name());
            assert!(!icon.has_solid(), "{} is no on and off pair", icon.name());
        }
    }
}
