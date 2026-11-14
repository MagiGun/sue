//! Sue's icons: one set, drawn as path data and rasterized for the pixels
//! they land on.
//!
//! Every picture on a button anywhere in Sue is an [`Icon`], drawn by
//! [`paint`] (or [`paint_styled`] for the solid twin of an on and off
//! pair) in the one color the caller passes. Icons carry no color of
//! their own.
//!
//! # The legal rule
//!
//! Every icon is drawn from scratch, on the grid below, from a written
//! description of what it means. That description is the doc comment of
//! its line in the table at the end of this file. No icon is traced
//! from, copied from, drawn while looking at, or matched side by side
//! against the artwork of Clip Studio Paint or of any other program,
//! icon set or web site. Pictures everybody uses for an idea (a
//! magnifier for search, a bin for delete, a folder) are fine, drawn our
//! own way. Where an idea has no such picture, Sue invents one, and the
//! tooltip carries the name.
//!
//! # The grid
//!
//! - The canvas is 24 by 24 units, x to the right and y down. The live
//!   area is the middle 20 by 20: nothing, the width of the pen
//!   included, comes nearer than 2 units to the edge of the canvas.
//! - Every icon is fitted to one of four keyline shapes ([`Keyline`]),
//!   all centered on the canvas, so that icons of different outlines
//!   look the same size side by side: a circle 20 across, a square of
//!   18, a wide box of 20 by 16 and a tall box of 16 by 20. The outside
//!   of the pen touches the keyline; it does not cross it.
//! - A straight stem that runs along an axis lies on x.75 when it is
//!   left of or above the middle of the canvas and on x.25 when it is
//!   right of or below it, so that its outer side is on a whole unit. A
//!   stem on the middle itself stays there. The top of a curve (the
//!   side of a ring, the lobe of a heart) follows the same rule wherever
//!   the picture allows it. Drawn like that, an icon is on whole pixels
//!   at 24 and 48 pixels as it is, and what is written is what is seen;
//!   at every other size the pixel fitting ([`fit`]) moves stems and the
//!   tops of curves to where they are sharp. A stem written elsewhere is
//!   moved at 24 too, by up to half a unit, which is why a test refuses
//!   it.
//! - Other points sit on the quarter unit grid where the picture is
//!   free, and where the geometry decides (a tangent, the points of a
//!   star) they are written to two or three decimals.
//!
//! # The pen
//!
//! - One weight: 1.5 units, which is 1.5 points at the default size of
//!   24 points. Round caps, round joins. Nothing is drawn with a second
//!   weight; the pen belongs to the stroker, not to the drawing, so the
//!   weight of the whole set is one number ([`weight`]).
//! - Corners of an outer shape are rounded with a radius of 2 units
//!   along the middle of the pen, corners of a detail inside with 1. A
//!   corner that is meant to be a point (the tip of an arrow, the fold
//!   of a page) is left to the round join.
//! - Two lines never come nearer than one pen: 1.5 units of clear space
//!   between the sides of two strokes, so 3 units between their middles.
//! - Sizes, in points: 16 (menus, tree rows, tabs), 20 (dense palettes),
//!   24 (toolbars, the default), 32 (touch bars), 48 and 64 (Hub tiles,
//!   dialogs). [`weight`] gives the pen for each: 1.25 points at 16, so
//!   that small icons do not clog, 1.5 from 20 to 24, and in proportion
//!   above (2 at 32, 3 at 48).
//!
//! # The two styles
//!
//! - [`IconStyle::Line`] is the style of every icon: one line of one
//!   weight, with at most one small solid accent (a dot, the head of an
//!   arrow, the fold of a page). No second tone, no translucent fill.
//! - [`IconStyle::Solid`] exists only for icons that are the "on" half
//!   of an on and off pair (a pin, a heart, an eye, a lock, a star): the
//!   same outline, filled. An icon without a solid twin is drawn as its
//!   line for both styles.
//!
//! # Writing an icon
//!
//! An icon is an [`Art`]: its keyline and its [`Part`]s, each a string
//! of SVG path data restricted to `M L H V C Q A Z` ([`path`]). A part
//! is stroked with the pen, filled, or cleared (a gap cut into what was
//! drawn before it). In a filled part, a subpath that runs the other way
//! round than the one it lies in is a hole. The art lives in one file
//! per group, so that two people drawing never edit the same file, and
//! the icon gets one line in the table at the end of this file: its
//! description, its name in Rust, its name as a string and its art.
//!
//! Names say what the picture is or does (`Layer`, `LayerNew`,
//! `layer-new`), with `Left Right Up Down` for directions. An icon is
//! never named after where it is used, or after another program's word
//! for it.
//!
//! # How an icon reaches the screen
//!
//! The path data is cut into straight pieces a fiftieth of a pixel true
//! ([`path`]), fitted to the pixel grid ([`fit`]), stroked into polygons
//! with round caps and joins ([`stroke`]) and filled into a tile of
//! coverage at exactly the number of pixels it will cover ([`raster`]).
//! The tile is kept as a texture, one per icon, style and size (the
//! `cache` module), and drawn as one quad whose corners are on whole
//! pixels, multiplied by the tint. Nothing is tessellated per frame and
//! every edge is anti-aliased by area. [`svg`] writes the same art as an
//! SVG file and [`sheet`] draws the contact sheet the set is reviewed
//! on.

mod cache;
pub mod fit;
pub mod path;
pub mod raster;
mod reference;
pub mod sheet;
pub mod stroke;
mod svg;

use egui::{Color32, Painter, Rect, pos2, vec2};

pub use cache::cached;
pub use svg::svg;

/// The width of the pen in units of the 24 unit canvas.
pub const WEIGHT: f32 = 1.5;

/// The margin of the canvas nothing is drawn in, in units.
pub const MARGIN: f32 = 2.0;

/// How many pens wide the gap is that [`Part::Clear`] cuts: the pen
/// itself and a clear pen on each side of it.
pub const CLEAR: f32 = 3.0;

/// The largest tile that is made, in pixels. A larger icon is drawn from
/// a tile of this size, scaled up.
const LARGEST: f32 = 512.0;

/// One layer of an icon: path data, and what is done with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Drawn along with the pen.
    Stroke(&'static str),
    /// Filled. Every subpath is closed, and one that runs the other way
    /// round than the subpath it lies in is a hole.
    Fill(&'static str),
    /// A gap cut into the parts before it: what a pen three times as
    /// wide would cover is taken away. Followed by a `Stroke` of the
    /// same data, it lets a line cross a shape with clear space on both
    /// sides.
    Clear(&'static str),
}

impl Part {
    /// The path data.
    #[must_use]
    pub const fn data(&self) -> &'static str {
        match self {
            Self::Stroke(data) | Self::Fill(data) | Self::Clear(data) => data,
        }
    }
}

/// The shape an icon is fitted to, centered on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keyline {
    /// A circle 20 units across: round pictures.
    Circle,
    /// A square of 18 units: square pictures and those that lie
    /// diagonally.
    Square,
    /// A box 20 wide and 16 high.
    Wide,
    /// A box 16 wide and 20 high.
    Tall,
}

impl Keyline {
    /// All four.
    pub const ALL: [Self; 4] = [Self::Circle, Self::Square, Self::Wide, Self::Tall];

    /// Its name: `circle`, `square`, `wide`, `tall`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Wide => "wide",
            Self::Tall => "tall",
        }
    }

    /// Its width and height, in units.
    #[must_use]
    pub const fn size(self) -> [f32; 2] {
        match self {
            Self::Circle => [20.0, 20.0],
            Self::Square => [18.0, 18.0],
            Self::Wide => [20.0, 16.0],
            Self::Tall => [16.0, 20.0],
        }
    }

    /// The box it fills on the canvas: `[left, top, right, bottom]`.
    #[must_use]
    pub fn bounds(self) -> [f32; 4] {
        let [width, height] = self.size();
        let middle = fit::CANVAS / 2.0;
        [
            middle - width / 2.0,
            middle - height / 2.0,
            middle + width / 2.0,
            middle + height / 2.0,
        ]
    }
}

/// The drawing of one icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Art {
    /// The shape it is fitted to.
    pub keyline: Keyline,
    /// The icon as a line, part over part.
    pub line: &'static [Part],
    /// Its solid twin, for the "on" half of an on and off pair. Empty
    /// for an icon that has none.
    pub solid: &'static [Part],
}

impl Art {
    /// The parts drawn for `style`: the solid twin when it is asked for
    /// and there is one, the line otherwise.
    #[must_use]
    pub const fn parts(&self, style: IconStyle) -> &'static [Part] {
        match style {
            IconStyle::Solid if !self.solid.is_empty() => self.solid,
            _ => self.line,
        }
    }
}

/// How an icon is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IconStyle {
    /// As a line of one weight: the style of every icon.
    #[default]
    Line,
    /// Filled: the "on" half of an on and off pair. An icon without a
    /// solid twin is drawn as its line.
    Solid,
}

impl IconStyle {
    /// Both.
    pub const ALL: [Self; 2] = [Self::Line, Self::Solid];

    /// Its name: `line`, `solid`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Solid => "solid",
        }
    }
}

/// Makes [`Icon`], the list of all of them, their names and their art
/// from one table, so that a new icon is one line.
macro_rules! icons {
    ($( $(#[$about:meta])* $icon:ident = $name:literal => $art:expr, )*) => {
        /// Every icon Sue draws. The doc comment of each is the written
        /// description it was drawn from.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Icon {
            $( $(#[$about])* $icon, )*
        }

        impl Icon {
            /// Every icon, in the order of the table.
            pub const ALL: &'static [Self] = &[ $( Self::$icon, )* ];

            /// Its name as a string, such as `layer-new`.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$icon => $name, )*
                }
            }

            /// Its drawing.
            #[must_use]
            pub const fn art(self) -> &'static Art {
                match self {
                    $( Self::$icon => &$art, )*
                }
            }
        }
    };
}

impl Icon {
    /// The icon called `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|icon| icon.name() == name)
    }

    /// Whether it has a solid twin.
    #[must_use]
    pub const fn has_solid(self) -> bool {
        !self.art().solid.is_empty()
    }

    /// The icon as a tile `pixels` wide, drawn with a pen `stroke`
    /// pixels wide.
    ///
    /// # Panics
    /// When the icon's path data cannot be read, which the tests of this
    /// module rule out for every icon in the table.
    #[must_use]
    pub fn tile(self, style: IconStyle, pixels: usize, stroke: f32) -> raster::Tile {
        match raster::render(self.art().parts(style), pixels, stroke) {
            Ok(tile) => tile,
            Err(error) => panic!(
                "the icon {} has path data that cannot be read: {error:?}",
                self.name()
            ),
        }
    }
}

/// The width of the pen, in points, for an icon `points` wide: 1.5 from
/// 20 to 24 points, 1.25 at 16 so that small icons do not clog, in
/// proportion above 24 (2 at 32, 3 at 48), and never under 1.
#[must_use]
pub fn weight(points: f32) -> f32 {
    let small = 1.25;
    if points <= 16.0 {
        (small * points / 16.0).max(1.0)
    } else if points < 20.0 {
        small + (WEIGHT - small) * (points - 16.0) / 4.0
    } else if points <= 24.0 {
        WEIGHT
    } else {
        WEIGHT * points / 24.0
    }
}

/// Draws `icon` in `tint`, as large as the shorter side of `rect` and in
/// its middle.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, tint: Color32) {
    paint_styled(painter, rect, icon, IconStyle::Line, tint);
}

/// Draws `icon` in `style` and `tint`, as large as the shorter side of
/// `rect` and in its middle. The picture is a tile made for exactly the
/// pixels it covers, with its corners on whole pixels.
pub fn paint_styled(painter: &Painter, rect: Rect, icon: Icon, style: IconStyle, tint: Color32) {
    let side = rect.width().min(rect.height());
    if !(side > 0.0 && side.is_finite()) || tint == Color32::TRANSPARENT {
        return;
    }
    let scale = painter.pixels_per_point();
    let pixels = (side * scale).round().max(1.0);
    let made = pixels.min(LARGEST);
    let stroke = weight(side) * scale * made / pixels;
    let texture = cache::texture(painter.ctx(), icon, style, made as usize, stroke);
    let middle = rect.center();
    let corner = |at: f32| ((at * scale) - pixels / 2.0).round() / scale;
    let place = Rect::from_min_size(
        pos2(corner(middle.x), corner(middle.y)),
        vec2(pixels / scale, pixels / scale),
    );
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(texture, place, whole, tint);
}

/// The twelve icons the set was first judged on: one for each keyline
/// shape as a plain line, as a line with a solid accent, and with a
/// solid twin. They are icons like any other; the contact sheet shows
/// them large ([`sheet::Sheet::Cards`]).
pub const REFERENCE: [Icon; 12] = [
    Icon::Settings,
    Icon::Search,
    Icon::Folder,
    Icon::Delete,
    Icon::Info,
    Icon::ToolBrush,
    Icon::Picture,
    Icon::Document,
    Icon::Star,
    Icon::Heart,
    Icon::Eye,
    Icon::Lock,
];

icons! {
    /// A gear: a toothed wheel with a round hole in its middle. Settings.
    Settings = "settings" => reference::SETTINGS,
    /// A magnifier: a round lens with a short handle down to the right.
    /// Search.
    Search = "search" => reference::SEARCH,
    /// A closed folder seen from the front: a box with a tab on its top
    /// left. A folder of files.
    Folder = "folder" => reference::FOLDER,
    /// A bin: a can with a lid, a handle on the lid and two ribs. Delete.
    Delete = "delete" => reference::DELETE,
    /// A ring with a small letter i in it: a stem under a solid dot.
    /// Information about something.
    Info = "info" => reference::INFO,
    /// A round paint brush lying from the top right down to the bottom
    /// left: a handle, and a solid pointed tuft of bristles. The brush
    /// tool.
    ToolBrush = "tool-brush" => reference::TOOL_BRUSH,
    /// A framed picture of two hills under a solid round sun. An image.
    Picture = "picture" => reference::PICTURE,
    /// A sheet of paper with its top right corner folded down, solid,
    /// and two lines of writing. A document.
    Document = "document" => reference::DOCUMENT,
    /// A star with five points. A favorite; solid when it is one.
    Star = "star" => reference::STAR,
    /// A heart. Something liked; solid when it is.
    Heart = "heart" => reference::HEART,
    /// An open eye: two lids and a round iris. Shown; solid for the
    /// state that is on.
    Eye = "eye" => reference::EYE,
    /// A padlock: a body with a keyhole and a shackle over it. Locked;
    /// solid for the state that is on.
    Lock = "lock" => reference::LOCK,
}

/// Frames drawn without a window or a GPU, for the tests of this module.
#[cfg(test)]
mod headless {
    use egui::epaint::{ClippedShape, ImageDelta};
    use egui::{Context, Painter, Rect, TextureId, pos2, vec2};

    /// What one frame gave.
    pub struct Seen {
        /// What was drawn.
        pub shapes: Vec<ClippedShape>,
        /// The scale it was drawn at.
        pub pixels_per_point: f32,
        /// Every texture handed to the painter: its id and its pixels.
        pub sets: Vec<(TextureId, ImageDelta)>,
        /// How many textures the painter was told to free.
        pub freed: usize,
    }

    /// One frame `size` points large at `scale` pixels per point that
    /// draws what `draw` asks for.
    pub fn frame(ctx: &Context, size: [f32; 2], scale: f32, draw: impl Fn(&Painter, Rect)) -> Seen {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(size[0], size[1]));
        let mut input = egui::RawInput {
            screen_rect: Some(rect),
            ..egui::RawInput::default()
        };
        input
            .viewports
            .entry(input.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(scale);
        let mut output = ctx.run_ui(input, |ui| draw(ui.painter(), rect));
        let sets = output
            .textures_delta
            .set
            .drain()
            .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
            .collect();
        let freed = output.textures_delta.free.len();
        output.textures_delta.clear();
        Seen {
            shapes: output.shapes,
            pixels_per_point: output.pixels_per_point,
            sets,
            freed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// The name an icon must have, from its name in Rust: `LayerNew`
    /// gives `layer-new`.
    fn kebab(icon: Icon) -> String {
        let mut name = String::new();
        for letter in format!("{icon:?}").chars() {
            if letter.is_ascii_uppercase() && !name.is_empty() {
                name.push('-');
            }
            name.push(letter.to_ascii_lowercase());
        }
        name
    }

    fn styles(icon: Icon) -> Vec<IconStyle> {
        if icon.has_solid() {
            IconStyle::ALL.to_vec()
        } else {
            vec![IconStyle::Line]
        }
    }

    #[test]
    fn every_icon_has_its_own_name_and_is_found_by_it() {
        let mut seen = HashMap::new();
        for icon in Icon::ALL {
            assert_eq!(icon.name(), kebab(*icon));
            assert_eq!(Icon::from_name(icon.name()), Some(*icon));
            assert!(seen.insert(icon.name(), icon).is_none(), "{}", icon.name());
        }
        assert_eq!(Icon::from_name("no-such-icon"), None);
        assert_eq!(Icon::from_name(""), None);
    }

    #[test]
    fn every_icon_parses_and_draws_something() {
        for icon in Icon::ALL {
            let art = icon.art();
            assert!(!art.line.is_empty(), "{} has no line", icon.name());
            for part in art.line.iter().chain(art.solid) {
                let paths = path::parse(part.data())
                    .unwrap_or_else(|error| panic!("{}: {error:?}", icon.name()));
                assert!(!paths.is_empty(), "{} has an empty part", icon.name());
            }
            // A line begins with something drawn, never with a gap.
            assert!(!matches!(art.line[0], Part::Clear(_)), "{}", icon.name());
        }
    }

    /// The box of everything `parts` draw, the pen included.
    fn ink(parts: &[Part]) -> [f32; 4] {
        let mut all = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for part in parts {
            let pen = match part {
                Part::Stroke(_) => WEIGHT,
                Part::Fill(_) => 0.0,
                Part::Clear(_) => continue,
            };
            let found = path::bounds(part.data(), pen).unwrap().unwrap();
            all = [
                all[0].min(found[0]),
                all[1].min(found[1]),
                all[2].max(found[2]),
                all[3].max(found[3]),
            ];
        }
        all
    }

    /// How far from the middle of the canvas `parts` draw, the pen
    /// included.
    fn reach(parts: &[Part]) -> f32 {
        let middle = fit::CANVAS / 2.0;
        let mut far = 0.0_f32;
        for part in parts {
            let pen = match part {
                Part::Stroke(_) => WEIGHT,
                Part::Fill(_) => 0.0,
                Part::Clear(_) => continue,
            };
            for path in path::parse(part.data()).unwrap() {
                for point in path.flatten(0.005).points {
                    let away = (point[0] - middle).hypot(point[1] - middle);
                    far = far.max(away + pen / 2.0);
                }
            }
        }
        far
    }

    /// Where the straight pieces of `data` that run along an axis lie:
    /// `(axis, place)`, axis 0 for an upright stem (its x) and 1 for a
    /// level one (its y). Pieces shorter than the pen is wide are no
    /// stems.
    fn stems(data: &str) -> Vec<(usize, f32)> {
        let mut found = Vec::new();
        for path in path::parse(data).unwrap() {
            let mut pen = path.start;
            let mut ends: Vec<path::Point> = path.segments.iter().map(path::Segment::end).collect();
            let lines: Vec<bool> = path
                .segments
                .iter()
                .map(|segment| matches!(segment, path::Segment::Line(_)))
                .collect();
            if path.closed {
                ends.push(path.start);
            }
            for (index, to) in ends.into_iter().enumerate() {
                let straight = lines.get(index).copied().unwrap_or(true);
                for axis in 0..2 {
                    let along = (to[1 - axis] - pen[1 - axis]).abs();
                    if straight && (to[axis] - pen[axis]).abs() < 1e-4 && along >= WEIGHT {
                        found.push((axis, pen[axis]));
                    }
                }
                pen = to;
            }
        }
        found
    }

    #[test]
    fn every_stem_has_its_outer_side_on_a_whole_unit() {
        let middle = fit::CANVAS / 2.0;
        for icon in Icon::ALL {
            let art = icon.art();
            for part in art.line.iter().chain(art.solid) {
                let Part::Stroke(data) = part else { continue };
                for (axis, place) in stems(data) {
                    if (place - middle).abs() < 1e-3 {
                        continue;
                    }
                    let wanted = if place < middle { 0.75 } else { 0.25 };
                    assert!(
                        (place - place.floor() - wanted).abs() < 1e-3,
                        "{}: the stem at {} {place} belongs on x{wanted}",
                        icon.name(),
                        if axis == 0 { "x" } else { "y" },
                    );
                }
            }
        }
        // The helper sees stems, and only stems.
        assert_eq!(stems("M4 6.25 H20"), vec![(1, 6.25)]);
        assert_eq!(stems("M4 4 V8 H5 L9 9 Z"), vec![(0, 4.0)]);
        assert_eq!(stems("M4 4 H9 V9 H4 Z").len(), 4);
        assert!(stems("M4 4 L20 20 A3 3 0 0 1 20 4").is_empty());
    }

    #[test]
    fn every_icon_stays_inside_the_canvas_the_live_area_and_its_keyline() {
        let slack = 0.01;
        for icon in Icon::ALL {
            let art = icon.art();
            for style in styles(*icon) {
                let ink = ink(art.parts(style));
                let name = format!("{} ({})", icon.name(), style.name());
                assert!(
                    ink[0] >= 0.0
                        && ink[1] >= 0.0
                        && ink[2] <= fit::CANVAS
                        && ink[3] <= fit::CANVAS,
                    "{name} leaves the canvas: {ink:?}"
                );
                let far = fit::CANVAS - MARGIN;
                assert!(
                    ink[0] >= MARGIN - slack
                        && ink[1] >= MARGIN - slack
                        && ink[2] <= far + slack
                        && ink[3] <= far + slack,
                    "{name} leaves the live area: {ink:?}"
                );
                let key = art.keyline.bounds();
                assert!(
                    ink[0] >= key[0] - slack
                        && ink[1] >= key[1] - slack
                        && ink[2] <= key[2] + slack
                        && ink[3] <= key[3] + slack,
                    "{name} leaves its keyline {key:?}: {ink:?}"
                );
                // A circle is round: its corners are no room to draw in.
                if art.keyline == Keyline::Circle {
                    let far = reach(art.parts(style));
                    assert!(
                        far <= Keyline::Circle.size()[0] / 2.0 + slack,
                        "{name} leaves its keyline circle: it reaches {far} from the middle"
                    );
                }
            }
        }
    }

    #[test]
    fn every_icon_gives_a_tile_at_16_24_and_48() {
        for icon in Icon::ALL {
            for style in styles(*icon) {
                for points in [16.0_f32, 24.0, 48.0] {
                    let tile = icon.tile(style, points as usize, weight(points));
                    assert_eq!(tile.size, points as usize);
                    assert_eq!(tile.alpha.len(), tile.size * tile.size);
                    let covered = tile.alpha.iter().filter(|alpha| **alpha > 127).count();
                    assert!(
                        covered >= tile.size,
                        "{} ({}) at {points} is all but empty",
                        icon.name(),
                        style.name()
                    );
                    // Nothing touches the edge of the tile: the margin
                    // is kept in pixels too.
                    let size = tile.size;
                    let edge = (0..size).any(|i| {
                        tile.alpha[i] > 0
                            || tile.alpha[(size - 1) * size + i] > 0
                            || tile.alpha[i * size] > 0
                            || tile.alpha[i * size + size - 1] > 0
                    });
                    assert!(
                        !edge,
                        "{} ({}) at {points} touches the edge",
                        icon.name(),
                        style.name()
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_icons_give_the_same_tile_at_24() {
        let mut seen: HashMap<Vec<u8>, String> = HashMap::new();
        for icon in Icon::ALL {
            for style in styles(*icon) {
                let name = format!("{} ({})", icon.name(), style.name());
                let tile = icon.tile(style, 24, weight(24.0));
                if let Some(other) = seen.insert(tile.alpha, name.clone()) {
                    panic!("{name} and {other} are the same picture at 24");
                }
            }
        }
    }

    #[test]
    fn a_solid_twin_covers_its_line_and_more() {
        for icon in Icon::ALL.iter().filter(|icon| icon.has_solid()) {
            let line = icon.tile(IconStyle::Line, 48, weight(48.0));
            let solid = icon.tile(IconStyle::Solid, 48, weight(48.0));
            let sum = |tile: &raster::Tile| tile.alpha.iter().map(|a| u32::from(*a)).sum::<u32>();
            assert!(
                sum(&solid) > sum(&line) * 5 / 4,
                "{} is hardly more solid than its line",
                icon.name()
            );
        }
        // An icon without a twin is its line in both styles.
        let plain = Icon::ALL.iter().find(|icon| !icon.has_solid()).unwrap();
        assert_eq!(
            plain.tile(IconStyle::Solid, 24, 1.5),
            plain.tile(IconStyle::Line, 24, 1.5)
        );
    }

    #[test]
    fn the_reference_icons_are_one_per_keyline_and_kind() {
        let mut seen = HashMap::new();
        for icon in REFERENCE {
            let art = icon.art();
            let kind = if icon.has_solid() {
                "solid twin"
            } else if art.line.iter().any(|part| matches!(part, Part::Fill(_))) {
                "accent"
            } else {
                "line"
            };
            assert!(
                seen.insert((art.keyline, kind), icon).is_none(),
                "{} doubles a {} {kind}",
                icon.name(),
                art.keyline.name()
            );
        }
        assert_eq!(seen.len(), 12);
    }

    #[test]
    fn the_pen_follows_the_size() {
        assert_eq!(weight(16.0), 1.25);
        assert_eq!(weight(18.0), 1.375);
        assert_eq!(weight(20.0), 1.5);
        assert_eq!(weight(24.0), 1.5);
        assert_eq!(weight(32.0), 2.0);
        assert_eq!(weight(48.0), 3.0);
        assert_eq!(weight(8.0), 1.0);
        let mut last = 0.0;
        for tenth in 40..=960 {
            let now = weight(tenth as f32 / 10.0);
            assert!(now >= last);
            last = now;
        }
    }

    #[test]
    fn keylines_are_centered_and_inside_the_live_area() {
        for keyline in Keyline::ALL {
            let [left, top, right, bottom] = keyline.bounds();
            assert_eq!(left + right, fit::CANVAS);
            assert_eq!(top + bottom, fit::CANVAS);
            assert!(left >= MARGIN && top >= MARGIN);
            assert!(!keyline.name().is_empty());
        }
    }
}
