//! Group A: the icons of the window, of moving about and of the small
//! controls every part of Sue shares.
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

pub(super) const WINDOW_MINIMIZE: Art = line(
    Keyline::Square,
    &[Part::Stroke("M5.75 11.75 H18.25 V12.25 H5.75 Z")],
);

pub(super) const WINDOW_MAXIMIZE: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M6.75 4.75 H17.25 A2 2 0 0 1 19.25 6.75 V17.25 A2 2 0 0 1 17.25 19.25 H6.75 A2 2 0 0 1 \
         4.75 17.25 V6.75 A2 2 0 0 1 6.75 4.75 Z",
    )],
);

pub(super) const WINDOW_RESTORE: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M6.75 9.75 H12.25 A2 2 0 0 1 14.25 11.75 V17.25 A2 2 0 0 1 12.25 19.25 H6.75 A2 2 0 \
             0 1 4.75 17.25 V11.75 A2 2 0 0 1 6.75 9.75 Z",
        ),
        Part::Stroke(
            "M9.75 6.25 V5.75 A1 1 0 0 1 10.75 4.75 H18.25 A1 1 0 0 1 19.25 5.75 V13.25 A1 1 0 0 \
             1 18.25 14.25 H17.75",
        ),
    ],
);

pub(super) const WINDOW_CLOSE: Art =
    line(Keyline::Square, &[Part::Stroke("M5 5 L19 19 M19 5 L5 19")]);

pub(super) const CLOSE: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M7.25 7.25 L16.75 16.75 M16.75 7.25 L7.25 16.75",
    )],
);

pub(super) const CHECK: Art = line(
    Keyline::Square,
    &[Part::Stroke("M4.5 12.5 L9.5 17.5 L19.5 6.5")],
);

pub(super) const CHEVRON_LEFT: Art = line(
    Keyline::Square,
    &[Part::Stroke("M15 5.25 L8.25 12 L15 18.75")],
);

pub(super) const CHEVRON_RIGHT: Art = line(
    Keyline::Square,
    &[Part::Stroke("M9 5.25 L15.75 12 L9 18.75")],
);

pub(super) const CHEVRON_UP: Art = line(
    Keyline::Square,
    &[Part::Stroke("M5.25 15 L12 8.25 L18.75 15")],
);

pub(super) const CHEVRON_DOWN: Art = line(
    Keyline::Square,
    &[Part::Stroke("M5.25 9 L12 15.75 L18.75 9")],
);

pub(super) const CHEVRONS_LEFT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M11.5 6 L5.5 12 L11.5 18 M18.5 6 L12.5 12 L18.5 18",
    )],
);

pub(super) const CHEVRONS_RIGHT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M5.5 6 L11.5 12 L5.5 18 M12.5 6 L18.5 12 L12.5 18",
    )],
);

/// The small solid triangle that points down.
const CARET_DOWN_LINE: &str = "M7 9.75 H17 L12 15.25 Z";

pub(super) const CARET_DOWN: Art = line(
    Keyline::Square,
    &[Part::Fill(CARET_DOWN_LINE), Part::Stroke(CARET_DOWN_LINE)],
);

/// The small solid triangle that points right.
const CARET_RIGHT_LINE: &str = "M9.75 7 V17 L15.25 12 Z";

pub(super) const CARET_RIGHT: Art = line(
    Keyline::Square,
    &[Part::Fill(CARET_RIGHT_LINE), Part::Stroke(CARET_RIGHT_LINE)],
);

pub(super) const ARROW_LEFT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M19.5 12 H4.5 M10.75 5.75 L4.5 12 L10.75 18.25",
    )],
);

pub(super) const ARROW_RIGHT: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M4.5 12 H19.5 M13.25 5.75 L19.5 12 L13.25 18.25",
    )],
);

pub(super) const ARROW_UP: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M12 19.5 V4.5 M5.75 10.75 L12 4.5 L18.25 10.75",
    )],
);

pub(super) const ARROW_DOWN: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M12 4.5 V19.5 M5.75 13.25 L12 19.5 L18.25 13.25",
    )],
);

pub(super) const MENU: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M4.75 5.75 H19.25 M4.75 12 H19.25 M4.75 18.25 H19.25",
    )],
);

pub(super) const MORE: Art = line(
    Keyline::Wide,
    &[Part::Fill(
        "M2 12 A2 2 0 0 1 6 12 A2 2 0 0 1 2 12 Z M10 12 A2 2 0 0 1 14 12 A2 2 0 0 1 10 12 Z M18 \
         12 A2 2 0 0 1 22 12 A2 2 0 0 1 18 12 Z",
    )],
);

pub(super) const MORE_VERTICAL: Art = line(
    Keyline::Tall,
    &[Part::Fill(
        "M10 4 A2 2 0 0 1 14 4 A2 2 0 0 1 10 4 Z M10 12 A2 2 0 0 1 14 12 A2 2 0 0 1 10 12 Z M10 \
         20 A2 2 0 0 1 14 20 A2 2 0 0 1 10 20 Z",
    )],
);

pub(super) const GRIP: Art = line(
    Keyline::Tall,
    &[Part::Fill(
        "M7 5 A1.5 1.5 0 0 1 10 5 A1.5 1.5 0 0 1 7 5 Z M14 5 A1.5 1.5 0 0 1 17 5 A1.5 1.5 0 0 1 \
         14 5 Z M7 12 A1.5 1.5 0 0 1 10 12 A1.5 1.5 0 0 1 7 12 Z M14 12 A1.5 1.5 0 0 1 17 12 A1.5 \
         1.5 0 0 1 14 12 Z M7 19 A1.5 1.5 0 0 1 10 19 A1.5 1.5 0 0 1 7 19 Z M14 19 A1.5 1.5 0 0 \
         1 17 19 A1.5 1.5 0 0 1 14 19 Z",
    )],
);

pub(super) const PLUS: Art = line(
    Keyline::Square,
    &[Part::Stroke("M12 4.75 V19.25 M4.75 12 H19.25")],
);

pub(super) const MINUS: Art = line(
    Keyline::Square,
    &[Part::Stroke("M4.75 11.75 H19.25 V12.25 H4.75 Z")],
);

pub(super) const FILTER: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M3.75 4.75 H20.25 L14.25 12 V17.5 L9.75 19.75 V12 Z",
    )],
);

pub(super) const SORT_UP: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 19.25 V4.75 M3.75 8.75 L7.75 4.75 L11.75 8.75"),
        Part::Stroke("M14.25 6.75 H16.25 M14.25 12 H18.25 M14.25 17.25 H20.25"),
    ],
);

pub(super) const SORT_DOWN: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M7.75 4.75 V19.25 M3.75 15.25 L7.75 19.25 L11.75 15.25"),
        Part::Stroke("M14.25 6.75 H20.25 M14.25 12 H18.25 M14.25 17.25 H16.25"),
    ],
);

pub(super) const VIEW_GRID: Art = line(
    Keyline::Square,
    &[Part::Stroke(
        "M4.75 3.75 H8.75 A1 1 0 0 1 9.75 4.75 V8.75 A1 1 0 0 1 8.75 9.75 H4.75 A1 1 0 0 1 3.75 \
         8.75 V4.75 A1 1 0 0 1 4.75 3.75 Z M15.25 3.75 H19.25 A1 1 0 0 1 20.25 4.75 V8.75 A1 1 0 \
         0 1 19.25 9.75 H15.25 A1 1 0 0 1 14.25 8.75 V4.75 A1 1 0 0 1 15.25 3.75 Z M4.75 14.25 \
         H8.75 A1 1 0 0 1 9.75 15.25 V19.25 A1 1 0 0 1 8.75 20.25 H4.75 A1 1 0 0 1 3.75 19.25 \
         V15.25 A1 1 0 0 1 4.75 14.25 Z M15.25 14.25 H19.25 A1 1 0 0 1 20.25 15.25 V19.25 A1 1 0 \
         0 1 19.25 20.25 H15.25 A1 1 0 0 1 14.25 19.25 V15.25 A1 1 0 0 1 15.25 14.25 Z",
    )],
);

pub(super) const VIEW_GRID_SMALL: Art = line(
    Keyline::Square,
    &[Part::Fill(
        "M3 3 H7.5 V7.5 H3 Z M9.75 3 H14.25 V7.5 H9.75 Z M16.5 3 H21 V7.5 H16.5 Z M3 9.75 H7.5 \
         V14.25 H3 Z M9.75 9.75 H14.25 V14.25 H9.75 Z M16.5 9.75 H21 V14.25 H16.5 Z M3 16.5 H7.5 \
         V21 H3 Z M9.75 16.5 H14.25 V21 H9.75 Z M16.5 16.5 H21 V21 H16.5 Z",
    )],
);

pub(super) const VIEW_LIST: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M9.25 6.75 H20.25 M9.25 12 H20.25 M9.25 17.25 H20.25"),
        Part::Fill(
            "M3.25 6.75 A1.5 1.5 0 0 1 6.25 6.75 A1.5 1.5 0 0 1 3.25 6.75 Z M3.25 12 A1.5 1.5 0 0 \
             1 6.25 12 A1.5 1.5 0 0 1 3.25 12 Z M3.25 17.25 A1.5 1.5 0 0 1 6.25 17.25 A1.5 1.5 0 \
             0 1 3.25 17.25 Z",
        ),
    ],
);

pub(super) const EXTERNAL: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M10 5.75 H5.75 A2 2 0 0 0 3.75 7.75 V18.25 A2 2 0 0 0 5.75 20.25 H16.25 A2 2 0 0 0 \
             18.25 18.25 V14",
        ),
        Part::Stroke("M13.75 3.75 H20.25 V10.25 M20.25 3.75 L11.25 12.75"),
    ],
);

pub(super) const DETAILS: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M10.25 6.75 L15.5 12 L10.25 17.25"),
    ],
);

/// The cap of the pin.
const PIN_CAP: &str = "M7.75 2.75 H16.25";

/// The body of the pin: a neck that widens into a plate.
const PIN_BODY: &str = "M9.75 2.75 V8.75 L5.75 13.25 H18.25 L14.25 8.75 V2.75";

/// The needle of the pin.
const PIN_NEEDLE: &str = "M12 13.25 V21.25";

pub(super) const PIN: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(PIN_CAP),
        Part::Stroke(PIN_BODY),
        Part::Stroke(PIN_NEEDLE),
    ],
    solid: &[
        Part::Fill("M9.75 2.75 V8.75 L5.75 13.25 H18.25 L14.25 8.75 V2.75 Z"),
        Part::Stroke(PIN_CAP),
        Part::Stroke(PIN_BODY),
        Part::Stroke(PIN_NEEDLE),
    ],
};

pub(super) const HELP: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Stroke("M9.25 9.25 A2.75 2.75 0 1 1 13.5 11.55 C12.6 12.1 12 12.6 12 13.75"),
        Part::Fill("M10.9 17 A1.1 1.1 0 0 1 13.1 17 A1.1 1.1 0 0 1 10.9 17 Z"),
    ],
);

pub(super) const WARNING: Art = line(
    Keyline::Square,
    &[
        Part::Stroke(
            "M13.77 5.33 L20.02 17.33 A2 2 0 0 1 18.25 20.25 H5.75 A2 2 0 0 1 3.98 17.33 L10.23 \
             5.33 A2 2 0 0 1 13.77 5.33 Z",
        ),
        Part::Stroke("M12 9.75 V13.75"),
        Part::Fill("M10.9 17 A1.1 1.1 0 0 1 13.1 17 A1.1 1.1 0 0 1 10.9 17 Z"),
    ],
);

pub(super) const ERROR: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke("M8.58 3.75 H15.42 L20.25 8.58 V15.42 L15.42 20.25 H8.58 L3.75 15.42 V8.58 Z"),
        Part::Stroke("M12 7.75 V12.75"),
        Part::Fill("M10.9 16.25 A1.1 1.1 0 0 1 13.1 16.25 A1.1 1.1 0 0 1 10.9 16.25 Z"),
    ],
);

pub(super) const QUESTION: Art = line(
    Keyline::Tall,
    &[
        Part::Stroke("M7.5 7.5 A4.5 4.5 0 1 1 14.6 11.18 C13 12.35 12 13.25 12 15.5"),
        Part::Fill("M10.65 20 A1.35 1.35 0 0 1 13.35 20 A1.35 1.35 0 0 1 10.65 20 Z"),
    ],
);

pub(super) const RADIO_ON: Art = line(
    Keyline::Circle,
    &[
        Part::Stroke(RING),
        Part::Fill("M7.75 12 A4.25 4.25 0 0 1 16.25 12 A4.25 4.25 0 0 1 7.75 12 Z"),
    ],
);

pub(super) const RADIO_OFF: Art = line(Keyline::Circle, &[Part::Stroke(RING)]);

pub(super) const RELOAD: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M18.32 17.3 A8.25 8.25 0 1 1 19.58 8.75"),
        Part::Stroke("M20.25 3.75 V8.75 H14.75"),
    ],
);

pub(super) const HOME: Art = line(
    Keyline::Square,
    &[
        Part::Stroke("M3.75 11.25 L12 4 L20.25 11.25"),
        Part::Stroke("M5.75 9.5 V18.25 A2 2 0 0 0 7.75 20.25 H16.25 A2 2 0 0 0 18.25 18.25 V9.5"),
        Part::Stroke(
            "M9.75 20.25 V15.25 A1 1 0 0 1 10.75 14.25 H13.25 A1 1 0 0 1 14.25 15.25 V20.25",
        ),
    ],
);

/// The tests of groups A, B and C together: the three files are drawn
/// as one set and judged side by side.
#[cfg(test)]
mod tests {
    use egui::Color32;
    use sue_core::ReversibleTransaction;
    use sue_rice::{RiceSchema, builtin_presets};

    use super::super::raster::Tile;
    use super::super::{Icon, IconStyle, REFERENCE, weight};
    use crate::Theme;

    /// Group A, the window, moving about and the shared controls, the
    /// reference icons of the group included.
    const CHROME: [Icon; 46] = [
        Icon::WindowMinimize,
        Icon::WindowMaximize,
        Icon::WindowRestore,
        Icon::WindowClose,
        Icon::Close,
        Icon::Check,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::ChevronsLeft,
        Icon::ChevronsRight,
        Icon::CaretDown,
        Icon::CaretRight,
        Icon::ArrowLeft,
        Icon::ArrowRight,
        Icon::ArrowUp,
        Icon::ArrowDown,
        Icon::Menu,
        Icon::More,
        Icon::MoreVertical,
        Icon::Grip,
        Icon::Plus,
        Icon::Minus,
        Icon::Search,
        Icon::Filter,
        Icon::SortUp,
        Icon::SortDown,
        Icon::ViewGrid,
        Icon::ViewGridSmall,
        Icon::ViewList,
        Icon::External,
        Icon::Details,
        Icon::Pin,
        Icon::Heart,
        Icon::Star,
        Icon::Info,
        Icon::Help,
        Icon::Warning,
        Icon::Error,
        Icon::Question,
        Icon::RadioOn,
        Icon::RadioOff,
        Icon::Reload,
        Icon::Home,
        Icon::Settings,
    ];

    /// Group B, files and data, the reference icons of the group
    /// included.
    const FILE: [Icon; 27] = [
        Icon::FileNew,
        Icon::FileOpen,
        Icon::Folder,
        Icon::FolderOpen,
        Icon::FolderNew,
        Icon::Save,
        Icon::SaveAs,
        Icon::Import,
        Icon::Export,
        Icon::ExportPdf,
        Icon::Print,
        Icon::Download,
        Icon::Transfers,
        Icon::Copy,
        Icon::Cut,
        Icon::Paste,
        Icon::Duplicate,
        Icon::Delete,
        Icon::Rename,
        Icon::Edit,
        Icon::Tag,
        Icon::MoveTo,
        Icon::Recent,
        Icon::History,
        Icon::Link,
        Icon::Wrench,
        Icon::All,
    ];

    /// Group C, editing and the view, the reference icons of the group
    /// included.
    const EDIT: [Icon; 26] = [
        Icon::Undo,
        Icon::Redo,
        Icon::ZoomIn,
        Icon::ZoomOut,
        Icon::ZoomFit,
        Icon::ZoomActual,
        Icon::FlipHorizontal,
        Icon::FlipVertical,
        Icon::RotateLeft,
        Icon::RotateRight,
        Icon::Reset,
        Icon::Swap,
        Icon::Replace,
        Icon::Reverse,
        Icon::Confirm,
        Icon::Cancel,
        Icon::Eye,
        Icon::EyeOff,
        Icon::Lock,
        Icon::Unlock,
        Icon::Fullscreen,
        Icon::Keyboard,
        Icon::Moon,
        Icon::Sun,
        Icon::Language,
        Icon::Update,
    ];

    fn all() -> impl Iterator<Item = Icon> {
        CHROME.into_iter().chain(FILE).chain(EDIT)
    }

    #[test]
    fn the_three_groups_hold_each_icon_once_and_are_all_in_the_set() {
        let mut seen = std::collections::HashSet::new();
        for icon in all() {
            assert!(seen.insert(icon), "{} is listed twice", icon.name());
            assert!(Icon::ALL.contains(&icon));
        }
        assert_eq!(seen.len(), 46 + 27 + 26);
    }

    /// How much ink two tiles do not have in common, in whole pixels:
    /// 0 for the same picture.
    fn difference(one: &Tile, other: &Tile) -> f32 {
        let apart: u32 = one
            .alpha
            .iter()
            .zip(&other.alpha)
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum();
        apart as f32 / 255.0
    }

    /// Every pair of an icon of groups A, B and C and another icon of
    /// the set, with how many pixels of ink tell them apart at 16
    /// pixels, the closest first.
    fn look_alikes() -> Vec<(f32, Icon, Icon)> {
        let tiles: Vec<(Icon, Tile)> = Icon::ALL
            .iter()
            .map(|icon| (*icon, icon.tile(IconStyle::Line, 16, weight(16.0))))
            .collect();
        let mut pairs = Vec::new();
        for (index, (icon, mine)) in tiles.iter().enumerate() {
            for (other, tile) in &tiles[index + 1..] {
                if all().any(|one| one == *icon || one == *other) {
                    pairs.push((difference(mine, tile), *icon, *other));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        pairs
    }

    /// Pairs that are one picture by meaning and differ on purpose by
    /// one small mark: a plus, a smaller size, an open shackle. They
    /// never stand for each other where both are offered.
    const FAMILIES: [(Icon, Icon); 8] = [
        // The same dash: one in the title bar, one on a control.
        (Icon::WindowMinimize, Icon::Minus),
        // The same cross: the window's, and the smaller one of a tab.
        (Icon::WindowClose, Icon::Close),
        // A lock and the same lock opened.
        (Icon::Lock, Icon::Unlock),
        // A magnifier, and one with a plus or a minus in it.
        (Icon::ZoomIn, Icon::ZoomOut),
        (Icon::Search, Icon::ZoomOut),
        // A thing, and a new one of it.
        (Icon::Folder, Icon::FolderNew),
        (Icon::Copy, Icon::Duplicate),
        // A plus and a minus.
        (Icon::Plus, Icon::Minus),
    ];

    #[test]
    fn each_icon_of_the_three_groups_is_its_own_picture_at_16() {
        // Twelve pixels of a 256 pixel tile: more than a dot or a short
        // line that a glance would miss. The pairs of one family may
        // come closer, but never under two and a half pixels.
        for (apart, one, other) in look_alikes() {
            let family = FAMILIES
                .iter()
                .any(|pair| *pair == (one, other) || *pair == (other, one));
            let least = if family { 2.5 } else { 12.0 };
            assert!(
                apart >= least,
                "{} and {} differ by only {apart:.1} pixels of ink at 16",
                one.name(),
                other.name()
            );
        }
    }

    /// The colors of the built-in theme preset `id`.
    fn theme(id: &str) -> Theme {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        let preset = builtin_presets()
            .into_iter()
            .find(|preset| preset.id == id)
            .unwrap();
        preset.transaction(&rice).apply(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    /// A picture in the making: RGB bytes.
    struct Picture {
        width: usize,
        height: usize,
        rgb: Vec<u8>,
    }

    impl Picture {
        fn new(width: usize, height: usize, ground: Color32) -> Self {
            let rgb = (0..width * height)
                .flat_map(|_| [ground.r(), ground.g(), ground.b()])
                .collect();
            Self { width, height, rgb }
        }

        fn rect(&mut self, [left, top]: [usize; 2], [w, h]: [usize; 2], color: Color32) {
            for y in top..(top + h).min(self.height) {
                for x in left..(left + w).min(self.width) {
                    let at = (y * self.width + x) * 3;
                    self.rgb[at..at + 3].copy_from_slice(&[color.r(), color.g(), color.b()]);
                }
            }
        }

        /// Lays `tile` down in `ink`, each of its pixels `zoom` wide.
        fn tile(&mut self, at: [usize; 2], tile: &Tile, zoom: usize, ink: Color32) {
            for (index, alpha) in tile.alpha.iter().enumerate() {
                let cover = f32::from(*alpha) / 255.0;
                let (tx, ty) = (index % tile.size, index / tile.size);
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let (x, y) = (at[0] + tx * zoom + dx, at[1] + ty * zoom + dy);
                        if x >= self.width || y >= self.height {
                            continue;
                        }
                        let pixel = (y * self.width + x) * 3;
                        for (channel, color) in [ink.r(), ink.g(), ink.b()].into_iter().enumerate()
                        {
                            let under = f32::from(self.rgb[pixel + channel]);
                            self.rgb[pixel + channel] =
                                (under + (f32::from(color) - under) * cover).round() as u8;
                        }
                    }
                }
            }
        }

        fn save(&self, file: &std::path::Path) {
            use zune_core::bit_depth::BitDepth;
            use zune_core::colorspace::ColorSpace;
            use zune_core::options::EncoderOptions;
            let options =
                EncoderOptions::new(self.width, self.height, ColorSpace::RGB, BitDepth::Eight);
            let mut out = Vec::new();
            zune_png::PngEncoder::new(&self.rgb, options)
                .encode(&mut out)
                .unwrap();
            std::fs::write(file, out).unwrap();
        }
    }

    /// One sheet of `icons` on the panel color of `theme`: each icon at
    /// 16, 24 and 48 pixels, every pixel `zoom` wide, `columns` to a
    /// row. A row with a solid twin in it shows the twins on a second
    /// line.
    fn sheet(theme: &Theme, icons: &[Icon], zoom: usize, columns: usize) -> Picture {
        let gap = 8 * zoom;
        let cell = [(16 + 24 + 48) * zoom + 4 * gap, 48 * zoom + 2 * gap];
        let lines = |row: &[Icon]| 1 + usize::from(row.iter().any(|icon| icon.has_solid()));
        let rows: usize = icons.chunks(columns).map(lines).sum();
        let mut picture = Picture::new(columns * cell[0], rows * cell[1], theme.void);
        let mut top = 0;
        for row in icons.chunks(columns) {
            let tall = lines(row);
            for (column, icon) in row.iter().enumerate() {
                let left = column * cell[0];
                picture.rect(
                    [left + 2, top + 2],
                    [cell[0] - 4, tall * cell[1] - 4],
                    theme.panel,
                );
                for (line, style) in IconStyle::ALL.into_iter().enumerate() {
                    if style == IconStyle::Solid && !icon.has_solid() {
                        continue;
                    }
                    let mut x = left + gap;
                    for pixels in [16_usize, 24, 48] {
                        let tile = icon.tile(style, pixels, weight(pixels as f32));
                        let y = top + line * cell[1] + gap + (48 - pixels) * zoom / 2;
                        picture.tile([x, y], &tile, zoom, theme.text);
                        x += pixels * zoom + gap;
                    }
                }
            }
            top += tall * cell[1];
        }
        picture
    }

    /// Writes the contact sheets of groups A, B and C into the folder
    /// the environment variable `SUE_ICON_PROOFS` names, in the dark
    /// preset Sue ships in and in paper: the twelve reference icons and
    /// every icon of the three groups at 16, 24 and 48 pixels as they
    /// are (`*-actual.png`), and each group with every pixel two wide
    /// (`*-chrome.png`, `*-file.png`, `*-edit.png`). Also prints the
    /// pairs of icons whose ink at 16 is most alike. Run it while
    /// drawing:
    /// `cargo test -p sue-wm chrome::tests::contact_sheets -- --ignored --nocapture`.
    #[test]
    #[ignore = "writes pictures for the eye; set SUE_ICON_PROOFS to a folder"]
    fn contact_sheets() {
        let Some(folder) = std::env::var_os("SUE_ICON_PROOFS").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&folder).unwrap();
        for id in ["void", "paper"] {
            let theme = theme(id);
            let every: Vec<Icon> = REFERENCE.into_iter().chain(all()).collect();
            sheet(&theme, &every, 1, 12).save(&folder.join(format!("{id}-actual.png")));
            sheet(&theme, &CHROME, 2, 6).save(&folder.join(format!("{id}-chrome.png")));
            sheet(&theme, &FILE, 2, 6).save(&folder.join(format!("{id}-file.png")));
            sheet(&theme, &EDIT, 2, 6).save(&folder.join(format!("{id}-edit.png")));
        }
        for (apart, one, other) in look_alikes().iter().take(20) {
            println!("{apart:.1} {} {}", one.name(), other.name());
        }
    }
}
