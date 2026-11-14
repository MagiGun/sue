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
mod canvas;
mod chrome;
mod edit;
mod file;
pub mod fit;
mod hub;
mod layers;
mod palettes;
pub mod path;
pub mod raster;
mod reference;
pub mod sheet;
pub mod stroke;
mod svg;
mod tools;

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

    // Group A: the window, moving about and the small shared controls
    // (`chrome.rs`).
    /// A level line across the middle, a little shorter than the minus.
    /// Puts a window away, down to the bar.
    WindowMinimize = "window-minimize" => chrome::WINDOW_MINIMIZE,
    /// One box that fills the picture. Makes a window fill the screen.
    WindowMaximize = "window-maximize" => chrome::WINDOW_MAXIMIZE,
    /// A box in front of the corner of a second box. Brings a window
    /// that fills the screen back to its own size.
    WindowRestore = "window-restore" => chrome::WINDOW_RESTORE,
    /// A large cross from corner to corner. Closes a window.
    WindowClose = "window-close" => chrome::WINDOW_CLOSE,
    /// A cross, clearly smaller than the one of the window. Closes or
    /// removes the thing it sits on: a tab, a tag, a search.
    Close = "close" => chrome::CLOSE,
    /// A tick: a short stroke down and a long one up to the right.
    /// Chosen, done, switched on.
    Check = "check" => chrome::CHECK,
    /// An angle that points left. Back, the one before, fold in.
    ChevronLeft = "chevron-left" => chrome::CHEVRON_LEFT,
    /// An angle that points right. On, the next one, a list that opens
    /// to the side.
    ChevronRight = "chevron-right" => chrome::CHEVRON_RIGHT,
    /// An angle that points up. Fold away, one up.
    ChevronUp = "chevron-up" => chrome::CHEVRON_UP,
    /// An angle that points down. Unfold, one down.
    ChevronDown = "chevron-down" => chrome::CHEVRON_DOWN,
    /// Two angles, one behind the other, that point left. Fold a whole
    /// side of the window away to the left, or go to the very first.
    ChevronsLeft = "chevrons-left" => chrome::CHEVRONS_LEFT,
    /// Two angles, one behind the other, that point right. Fold a whole
    /// side of the window away to the right, or go to the very last.
    ChevronsRight = "chevrons-right" => chrome::CHEVRONS_RIGHT,
    /// A small solid triangle that points down. A button that opens a
    /// list under it; an open branch of a tree.
    CaretDown = "caret-down" => chrome::CARET_DOWN,
    /// A small solid triangle that points right. A closed branch of a
    /// tree.
    CaretRight = "caret-right" => chrome::CARET_RIGHT,
    /// An arrow to the left: a shaft and an open head. Go back.
    ArrowLeft = "arrow-left" => chrome::ARROW_LEFT,
    /// An arrow to the right: a shaft and an open head. Go on.
    ArrowRight = "arrow-right" => chrome::ARROW_RIGHT,
    /// An arrow that points up: a shaft and an open head. Move up.
    ArrowUp = "arrow-up" => chrome::ARROW_UP,
    /// An arrow that points down: a shaft and an open head. Move down.
    ArrowDown = "arrow-down" => chrome::ARROW_DOWN,
    /// Three level lines of one length, one under the other and
    /// spread over the height of the picture. Opens a menu.
    Menu = "menu" => chrome::MENU,
    /// Three solid dots in a row, as wide as the picture. More that did
    /// not fit.
    More = "more" => chrome::MORE,
    /// Three solid dots, one under the other, as tall as the picture.
    /// More, on a narrow or upright bar.
    MoreVertical = "more-vertical" => chrome::MORE_VERTICAL,
    /// Six solid dots, two beside each other three times. A place to
    /// take hold of and drag.
    Grip = "grip" => chrome::GRIP,
    /// A plus: an upright and a level line that cross. Add.
    Plus = "plus" => chrome::PLUS,
    /// A minus: one level line across the middle, a little heavier than
    /// a stroke so that it holds its own at 16 points. Take away.
    Minus = "minus" => chrome::MINUS,
    /// A funnel: wide at the top, with a narrow spout. Show only what
    /// passes.
    Filter = "filter" => chrome::FILTER,
    /// An arrow that points up beside three lines that grow longer
    /// downward. Sorted from first to last.
    SortUp = "sort-up" => chrome::SORT_UP,
    /// An arrow that points down beside three lines that grow shorter
    /// downward. Sorted from last to first.
    SortDown = "sort-down" => chrome::SORT_DOWN,
    /// Four boxes, two by two. Shown as large cards.
    ViewGrid = "view-grid" => chrome::VIEW_GRID,
    /// Nine small solid squares, three by three. Shown as small cards.
    ViewGridSmall = "view-grid-small" => chrome::VIEW_GRID_SMALL,
    /// Three rows, each a solid dot and a line. Shown as a list.
    ViewList = "view-list" => chrome::VIEW_LIST,
    /// A box with its top right corner open and an arrow leaving
    /// through it. Opens outside Sue.
    External = "external" => chrome::EXTERNAL,
    /// A ring with a large angle in it that points right. Everything
    /// about one thing, on a page of its own.
    Details = "details" => chrome::DETAILS,
    /// A pushpin standing upright: a cap, a neck that widens into a
    /// plate, and a needle. Kept in place; solid when it is.
    Pin = "pin" => chrome::PIN,
    /// A ring with a question mark in it. Help.
    Help = "help" => chrome::HELP,
    /// A triangle with round corners that stands on its base, and in it
    /// an exclamation mark. Take care: something may go wrong.
    Warning = "warning" => chrome::WARNING,
    /// A sign with eight sides and in it an exclamation mark. Something
    /// went wrong and stopped.
    Error = "error" => chrome::ERROR,
    /// A large question mark by itself. Sue asks and waits for an
    /// answer.
    Question = "question" => chrome::QUESTION,
    /// A ring with a large solid dot in it. The one that is chosen, of
    /// several.
    RadioOn = "radio-on" => chrome::RADIO_ON,
    /// An empty ring. One that is not chosen, of several.
    RadioOff = "radio-off" => chrome::RADIO_OFF,
    /// A ring, open at the right, that runs clockwise and ends in the
    /// corner of an arrow head. Load again.
    Reload = "reload" => chrome::RELOAD,
    /// A house: a roof, walls and a door. The start page.
    Home = "home" => chrome::HOME,

    // Group B: files and what is done with data (`file.rs`).
    /// A sheet of paper with a folded corner and a plus on it. A new
    /// file.
    FileNew = "file-new" => file::FILE_NEW,
    /// An open folder with a sheet of paper standing up out of it
    /// behind its front. Open a file.
    FileOpen = "file-open" => file::FILE_OPEN,
    /// A folder whose front leans open. A folder that shows what is in
    /// it.
    FolderOpen = "folder-open" => file::FOLDER_OPEN,
    /// A closed folder with a plus on it. A new folder.
    FolderNew = "folder-new" => file::FOLDER_NEW,
    /// A storage disk: a square case with one corner cut, a shutter at
    /// the top and a label below. Save.
    Save = "save" => file::SAVE,
    /// The storage disk with three dots in place of its label. Save
    /// under another name; Sue asks which.
    SaveAs = "save-as" => file::SAVE_AS,
    /// An arrow that goes from the left into a box open on that side.
    /// Bring something in from a file.
    Import = "import" => file::IMPORT,
    /// An arrow that leaves to the right from a box open on that side.
    /// Write something out to a file.
    Export = "export" => file::EXPORT,
    /// A sheet of paper with a folded corner and an arrow that leaves
    /// it to the right. Make the finished pages: a PDF.
    ExportPdf = "export-pdf" => file::EXPORT_PDF,
    /// A printer: a box, a sheet going in at the top and a sheet coming
    /// out at the front. Print.
    Print = "print" => file::PRINT,
    /// An arrow that points down into a tray. Fetch something and keep
    /// it.
    Download = "download" => file::DOWNLOAD,
    /// Two arrows side by side, one up and one down. What is being sent
    /// and fetched.
    Transfers = "transfers" => file::TRANSFERS,
    /// A sheet in front of the corner of a second sheet. Copy.
    Copy = "copy" => file::COPY,
    /// A pair of scissors, open, points up. Cut.
    Cut = "cut" => file::CUT,
    /// A clipboard: a board with a clip at its top and two lines of
    /// writing. Paste.
    Paste = "paste" => file::PASTE,
    /// A sheet in front of the corner of a second sheet, with a plus on
    /// it. Make another one of this.
    Duplicate = "duplicate" => file::DUPLICATE,
    /// A text field with a little writing in it and a text cursor
    /// after the writing. Give another name.
    Rename = "rename" => file::RENAME,
    /// A pencil, point down to the left, over the line it is writing.
    /// Edit.
    Edit = "edit" => file::EDIT,
    /// A label with a hole for its string: square at one end, pointed
    /// at the other. A tag.
    Tag = "tag" => file::TAG,
    /// A closed folder with an arrow in it that points right. Move
    /// into a folder.
    MoveTo = "move-to" => file::MOVE_TO,
    /// A clock: a ring and two hands. Used a short while ago.
    Recent = "recent" => file::RECENT,
    /// A clock whose ring runs backward and ends in the corner of an
    /// arrow head. What was done, step by step.
    History = "history" => file::HISTORY,
    /// Two links of a chain, joined. A link to something elsewhere.
    Link = "link" => file::LINK,
    /// A wrench lying from the bottom left up to its open jaw at the
    /// top right. Tools, repair, setting up.
    Wrench = "wrench" => file::WRENCH,
    /// An asterisk of six arms: the sign that stands for anything.
    /// Everything, with nothing left out.
    All = "all" => file::ALL,

    // Group C: editing and the view (`edit.rs`).
    /// An arrow that comes back to the left out of a turn. Undo.
    Undo = "undo" => edit::UNDO,
    /// An arrow that goes on to the right out of a turn. Redo.
    Redo = "redo" => edit::REDO,
    /// A magnifier with a plus in its lens. Zoom in.
    ZoomIn = "zoom-in" => edit::ZOOM_IN,
    /// A magnifier with a minus in its lens. Zoom out.
    ZoomOut = "zoom-out" => edit::ZOOM_OUT,
    /// The four corners of a frame around a small box. Show the whole
    /// picture in the window.
    ZoomFit = "zoom-fit" => edit::ZOOM_FIT,
    /// The numbers one to one, with a colon between them. One pixel of
    /// the picture on one of the screen.
    ZoomActual = "zoom-actual" => edit::ZOOM_ACTUAL,
    /// An upright line with a triangle on its left that points away from
    /// it, and the same triangle mirrored, solid, on its right. Flip left
    /// to right.
    FlipHorizontal = "flip-horizontal" => edit::FLIP_HORIZONTAL,
    /// A level line with a triangle above it that points away from it,
    /// and the same triangle mirrored, solid, below. Flip top to bottom.
    FlipVertical = "flip-vertical" => edit::FLIP_VERTICAL,
    /// A box, and a quarter of a circle with an arrow head that turns
    /// over it to the left. Turn a quarter counterclockwise.
    RotateLeft = "rotate-left" => edit::ROTATE_LEFT,
    /// A box, and a quarter of a circle with an arrow head that turns
    /// over it to the right. Turn a quarter clockwise.
    RotateRight = "rotate-right" => edit::ROTATE_RIGHT,
    /// An arrow that runs backward round a solid dot. Back to how it
    /// was at the start.
    Reset = "reset" => edit::RESET,
    /// Two level arrows, the upper to the right and the lower to the
    /// left. The two change places.
    Swap = "swap" => edit::SWAP,
    /// An empty square and a solid one side by side at the bottom, and
    /// an arrow that arcs over from the first into the place of the
    /// second. Put another in its place.
    Replace = "replace" => edit::REPLACE,
    /// An upright arrow with a head at both ends beside three lines of
    /// one length. Turn the order round.
    Reverse = "reverse" => edit::REVERSE,
    /// A ring with a tick in it. Yes: apply it.
    Confirm = "confirm" => edit::CONFIRM,
    /// A ring with a cross in it. No: leave it.
    Cancel = "cancel" => edit::CANCEL,
    /// The open eye, struck through from the top left to the bottom
    /// right. Hidden.
    EyeOff = "eye-off" => edit::EYE_OFF,
    /// The padlock with its shackle open: one end free, lifted off the
    /// body. Not locked.
    Unlock = "unlock" => edit::UNLOCK,
    /// Two arrows that point apart, to the top right and to the bottom
    /// left corner. Fill the whole screen.
    Fullscreen = "fullscreen" => edit::FULLSCREEN,
    /// A keyboard: a wide box with a row of keys and a long key under
    /// them. Keyboard shortcuts.
    Keyboard = "keyboard" => edit::KEYBOARD,
    /// A crescent moon. The dark look.
    Moon = "moon" => edit::MOON,
    /// A sun: a ring with eight short rays. The light look.
    Sun = "sun" => edit::SUN,
    /// A globe: a ring, the line round its middle and a narrow oval
    /// from pole to pole. The language.
    Language = "language" => edit::LANGUAGE,
    /// A ring with an arrow in it that points up. A newer version.
    Update = "update" => edit::UPDATE,
    /// An artist's palette: a round board with a bite out of its lower
    /// right where the hand holds it, and four solid dabs of paint in an
    /// arc along its upper edge. Sue Paint, the studio.
    AppPaint = "app-paint" => hub::APP_PAINT,
    /// A quill: a long feather leaning from the lower left up to the top
    /// right, its shaft running on down past it to the nib. Sue Writer, the
    /// studio.
    AppWriter = "app-writer" => hub::APP_WRITER,
    /// An easel with nothing on it: a blank canvas standing on three
    /// legs, ready for any kind of work. A studio or tool that has no
    /// picture of its own.
    AppGeneric = "app-generic" => hub::APP_GENERIC,
    /// A sheet laid out before anything is on it: a small box for a
    /// picture at the top and two lines for writing under it. A template,
    /// a ready-made start for something new.
    Template = "template" => hub::TEMPLATE,
    /// A lifebuoy: a ring with a hole, bound across in four places. Work
    /// Sue kept safe when it was not saved, and bringing it back.
    Recover = "recover" => hub::RECOVER,
    /// A closed folder with an arrow leaving it at its top right corner.
    /// Showing a folder in the system's own file manager.
    FolderReveal = "folder-reveal" => hub::FOLDER_REVEAL,
    /// A wide window cut into a bar along its top, a column on each side
    /// and a large space in the middle. How a studio's palettes are laid
    /// out around the canvas: its layout style.
    LayoutStyle = "layout-style" => hub::LAYOUT_STYLE,
    /// A strip of film: a frame between two rails cut into cells, with a
    /// solid play mark in the frame. A film or a video.
    Film = "film" => hub::FILM,
    /// A sound wave: five upright strokes side by side, tallest in the
    /// middle. Sound, a kind of file.
    Audio = "audio" => hub::AUDIO,
    /// A cube seen from above a corner: three faces meeting in the
    /// middle. A 3D model or scene.
    Cube = "cube" => hub::CUBE,
    /// A book lying open: two pages curving down to the spine. A guide to
    /// read.
    Book = "book" => hub::BOOK,
    /// A closed envelope: a box with its flap folded down to a point in
    /// the middle. Mail, writing to someone.
    Mail = "mail" => hub::MAIL,
    /// A round speech bubble with its tail at the lower left and three
    /// solid dots in a row in it. Telling the makers of Sue what you
    /// think.
    Feedback = "feedback" => hub::FEEDBACK,
    /// Three shapes together: a triangle on top, a circle and a square
    /// under it. Materials, the parts kept ready to put into a work.
    Materials = "materials" => hub::MATERIALS,
    /// A filing cabinet: a tall box of two drawers, each with a pull, on
    /// two short feet. Files: everything made in Sue, kept to look
    /// through.
    Files = "files" => hub::FILES,
    /// A magnifying glass held in the hand: a round lens with a glint of
    /// light along its upper left, and a solid stout handle down to the
    /// right. The zoom tool.
    ToolZoom = "tool-zoom" => tools::TOOL_ZOOM,
    /// An open hand seen from its back: four fingers up, the thumb out to
    /// the left, a round palm. The hand tool, which moves the view.
    ToolHand = "tool-hand" => tools::TOOL_HAND,
    /// A sheet tilted to one side, inside one curved arrow that turns
    /// nearly all the way round it, its solid head at the lower left.
    /// The tool that rotates the view of the canvas.
    ToolRotateView = "tool-rotate-view" => tools::TOOL_ROTATE_VIEW,
    /// A box with an arrow pointer, solid, resting on its lower right
    /// corner. The tool that picks an object and works on it.
    ToolOperation = "tool-operation" => tools::TOOL_OPERATION,
    /// A cross of four arrows, pointing up, down, left and right. The
    /// tool that moves a layer.
    ToolMoveLayer = "tool-move-layer" => tools::TOOL_MOVE_LAYER,
    /// A box drawn with a dashed line: the marching line round a
    /// selected area. The selection tool.
    ToolSelect = "tool-select" => tools::TOOL_SELECT,
    /// A lasso: a flat loop of rope, and the loose end of the rope
    /// hanging from its lower left and curling away to the right. The
    /// tool that selects an area drawn freehand.
    ToolSelectLasso = "tool-select-lasso" => tools::TOOL_SELECT_LASSO,
    /// A magic wand: a slim rod rising to the right, a band round it
    /// near its tip, and a solid sparkle of four points just past the
    /// tip. The tool that selects an area of one color with a click.
    ToolAutoSelect = "tool-auto-select" => tools::TOOL_AUTO_SELECT,
    /// A dropper lying from the top right down to the bottom left: a
    /// solid rubber bulb, a collar, and a glass tube that narrows to a
    /// point. The tool that picks a color from the canvas.
    ToolEyedropper = "tool-eyedropper" => tools::TOOL_EYEDROPPER,
    /// The steel nib of a dip pen in its holder, pointing down to the
    /// bottom left: a broad pointed nib with a slit and a solid round
    /// hole. The pen tool.
    ToolPen = "tool-pen" => tools::TOOL_PEN,
    /// A pencil pointing down to the bottom left: a straight shaft, a
    /// sharpened cone, and a band near its end where the rubber sits.
    /// The pencil tool.
    ToolPencil = "tool-pencil" => tools::TOOL_PENCIL,
    /// A spray can: a tall body, a narrow square cap on it and a nozzle
    /// sticking out of the cap to the left, with a fan of solid drops
    /// flying from the nozzle and spreading as they go. The airbrush
    /// tool.
    ToolAirbrush = "tool-airbrush" => tools::TOOL_AIRBRUSH,
    /// Three sparkles of four points: a large one drawn as a line, and
    /// two small solid ones beside it. The tool that stamps a trail of
    /// ornaments.
    ToolDecoration = "tool-decoration" => tools::TOOL_DECORATION,
    /// A block of rubber lying on its lower left corner, with a line
    /// across it where its sleeve ends, and the ground it rubs on. The
    /// eraser tool.
    ToolEraser = "tool-eraser" => tools::TOOL_ERASER,
    /// Two rings side by side that overlap, solid where they overlap:
    /// two colors running into each other. The blend tool.
    ToolBlend = "tool-blend" => tools::TOOL_BLEND,
    /// A spiral that winds inward: the picture stirred round like
    /// liquid. The liquify tool.
    ToolLiquify = "tool-liquify" => tools::TOOL_LIQUIFY,
    /// A bucket of paint tipped on its corner, with its handle, the
    /// paint in it solid up to a level line, and a solid drop falling
    /// from its lip.
    /// The fill tool.
    ToolFill = "tool-fill" => tools::TOOL_FILL,
    /// A solid upright band, and after it three columns of solid dots,
    /// each column of smaller dots than the last, the middle one set
    /// between the rows of the other two: a tone that fades from full to empty, as
    /// print shows it.
    /// The gradient tool.
    ToolGradient = "tool-gradient" => tools::TOOL_GRADIENT,
    /// A circle in front of a square, the square showing behind it at
    /// the top left. The tool that draws lines and shapes.
    ToolFigure = "tool-figure" => tools::TOOL_FIGURE,
    /// A page of a comic cut into three panels: a slanted gutter across
    /// it and an upright one below. The tool that draws panel frames.
    ToolFrame = "tool-frame" => tools::TOOL_FRAME,
    /// A speech balloon: an oval with a pointed tail down to the lower
    /// left. The tool that draws the balloons of a comic.
    ToolBalloon = "tool-balloon" => tools::TOOL_BALLOON,
    /// A flat ruler lying from the top left down to the bottom right,
    /// with three marks along one edge. The ruler tool.
    ToolRuler = "tool-ruler" => tools::TOOL_RULER,
    /// A capital letter T with serifs: a bar whose ends turn down, a
    /// stem and a foot. The text tool.
    ToolText = "tool-text" => tools::TOOL_TEXT,
    /// A curved line between two small square points, with a solid
    /// round point on its middle: a drawn line and the points it is
    /// reshaped by. The tool that corrects lines.
    ToolCorrectLine = "tool-correct-line" => tools::TOOL_CORRECT_LINE,
    /// A dashed box hugging a box drawn whole that nearly fills it: the
    /// selection round everything there is. Select all.
    SelectAll = "select-all" => canvas::SELECT_ALL,
    /// A dashed box struck through by a line from the bottom left to
    /// the top right. Deselect: the selection is let go.
    Deselect = "deselect" => canvas::DESELECT,
    /// A box cut from corner to corner: the upper left half a dashed
    /// line, the lower right half solid. Invert the selection: what was
    /// outside is now inside.
    SelectInvert = "select-invert" => canvas::SELECT_INVERT,
    /// A dashed box, and inside it one slanted line with an arrow head
    /// at each end, pushing two corners apart. Expand the selection.
    SelectExpand = "select-expand" => canvas::SELECT_EXPAND,
    /// A dashed box, and inside it two arrows that come from two
    /// corners and point at each other. Shrink the selection.
    SelectShrink = "select-shrink" => canvas::SELECT_SHRINK,
    /// A dashed box with a cross inside it. Clear: what is inside the
    /// selection is taken away.
    ClearInside = "clear-inside" => canvas::CLEAR_INSIDE,
    /// A small dashed box at the bottom left and a cross outside it, at
    /// the top right. Clear outside: what is not in the selection is
    /// taken away.
    ClearOutside = "clear-outside" => canvas::CLEAR_OUTSIDE,
    /// A dashed box with a solid drop of paint inside it. Fill: the
    /// selection is filled with the drawing color.
    FillSelection = "fill-selection" => canvas::FILL_SELECTION,
    /// Two right angles that cross at two corners and leave a square
    /// between them: the marks a picture is cut along. Crop.
    Crop = "crop" => canvas::CROP,
    /// A box with a solid square handle on each of its four corners:
    /// the frame a picture is scaled and turned by. Transform.
    Transform = "transform" => canvas::TRANSFORM,
    /// An upright horseshoe magnet with solid poles, over the edge of a
    /// ruler with three marks, the middle one longer. Snap to the ruler.
    SnapRuler = "snap-ruler" => canvas::SNAP_RULER,
    /// An upright horseshoe magnet with solid poles, over a row of the
    /// square cells of a grid. Snap to the grid.
    SnapGrid = "snap-grid" => canvas::SNAP_GRID,
    /// Two color chips, a solid one at the top left and an empty one at
    /// the bottom right, and two bent arrows that lead from each to the
    /// other. Swap the drawing color and the second color.
    ColorSwap = "color-swap" => canvas::COLOR_SWAP,
    // Group G: layers and the light table (`layers.rs`).
    /// Three sheets lying one over the other: the layers of a picture, and the palette that lists
    /// them.
    Layers = "layers" => layers::LAYERS,
    /// Two sheets lying one over the other, with a plus at the lower right: a new raster layer.
    LayerNew = "layer-new" => layers::LAYER_NEW,
    /// A curve between two solid square anchor points, with a plus at the lower right: a new vector
    /// layer.
    LayerNewVector = "layer-new-vector" => layers::LAYER_NEW_VECTOR,
    /// A folder with a plus at its lower right corner: a new folder that layers are put into.
    LayerNewFolder = "layer-new-folder" => layers::LAYER_NEW_FOLDER,
    /// Two equal sheets, one lying behind and above the other: a copy of the layer.
    LayerDuplicate = "layer-duplicate" => layers::LAYER_DUPLICATE,
    /// An arrow pressing down into a sheet: the layer is merged into the one below it.
    LayerMergeDown = "layer-merge-down" => layers::LAYER_MERGE_DOWN,
    /// Two sheets one over the other, and beside them an arrow pointing down: what is drawn moves
    /// to the layer below.
    LayerTransferDown = "layer-transfer-down" => layers::LAYER_TRANSFER_DOWN,
    /// An arrow that bends down onto a sheet: the layer is clipped to the layer below it.
    LayerClip = "layer-clip" => layers::LAYER_CLIP,
    /// A marker pin standing on a sheet: the layer that other tools look at.
    LayerReference = "layer-reference" => layers::LAYER_REFERENCE,
    /// A sheet drawn with a broken line: a draft layer, which is neither printed nor exported.
    LayerDraft = "layer-draft" => layers::LAYER_DRAFT,
    /// A checkered square, the sign of see-through pixels, with a padlock at its lower right: the
    /// transparent pixels are locked.
    LayerLockAlpha = "layer-lock-alpha" => layers::LAYER_LOCK_ALPHA,
    /// A frame with a solid round shape in it: a layer mask, which hides what lies outside the
    /// shape.
    LayerMask = "layer-mask" => layers::LAYER_MASK,
    /// A layer mask with a plus at its lower right corner: a new layer mask.
    LayerMaskNew = "layer-mask-new" => layers::LAYER_MASK_NEW,
    /// A layer mask with a check at its lower right corner: the mask is applied to the layer for
    /// good.
    LayerMaskApply = "layer-mask-apply" => layers::LAYER_MASK_APPLY,
    /// Two sheets one over the other, with a solid drop of paint at the lower right: the layer is
    /// shown in its layer color.
    LayerColor = "layer-color" => layers::LAYER_COLOR,
    /// A ring whose left half is solid: the layers on the light table are each shown in one color.
    LayerColorMode = "layer-color-mode" => layers::LAYER_COLOR_MODE,
    /// Two sheets one over the other, with a solid four pointed spark at the lower right: the
    /// effects a layer can have.
    LayerProperty = "layer-property" => layers::LAYER_PROPERTY,
    /// A sheet over the edge of a second one drawn with a broken line: whether the opacity changes
    /// all the layers or each on its own.
    LayerOpacityTarget = "layer-opacity-target" => layers::LAYER_OPACITY_TARGET,
    /// A sheet lying over a solid one: the paper, a layer of one color under the whole picture.
    LayerPaper = "layer-paper" => layers::LAYER_PAPER,
    /// A solid ball and a solid block, with one line running round both at a distance: a border
    /// drawn round what is on the layer.
    BorderEffect = "border-effect" => layers::BORDER_EFFECT,
    /// A drop with a second line inside its lower left rim, where the paint gathers: the watercolor
    /// edge.
    WatercolorEdge = "watercolor-edge" => layers::WATERCOLOR_EDGE,
    /// A solid hill with its own outline lifted off above it: the lines are taken out of a picture.
    ExtractLine = "extract-line" => layers::EXTRACT_LINE,
    /// Rows of dots that grow from small at the upper left to large at the lower right: the layer
    /// is shown as a halftone.
    Tone = "tone" => layers::TONE,
    /// A lit panel with rays of light rising from it: the light table, which shows other drawings
    /// through the layer.
    LightTable = "light-table" => layers::LIGHT_TABLE,
    /// A light table with a plus at its upper right: the chosen layer is put on the light table.
    LightTableAdd = "light-table-add" => layers::LIGHT_TABLE_ADD,
    /// A light table with a minus at its upper right: the chosen layer is taken off the light
    /// table.
    LightTableRemove = "light-table-remove" => layers::LIGHT_TABLE_REMOVE,
    /// A light table with a page at its upper right: a picture file is put on the light table.
    LightTableFile = "light-table-file" => layers::LIGHT_TABLE_FILE,
    /// A light table with a solid pointer at its upper right: the tool that moves what lies on the
    /// light table.
    LightTableTool = "light-table-tool" => layers::LIGHT_TABLE_TOOL,
    /// A cel with its peg holes, and a padlock at its lower right: the cel drawn on stays the same,
    /// whichever cel is shown.
    CelLock = "cel-lock" => layers::CEL_LOCK,

    // Group H: palettes, color and the brush (`palettes.rs`).
    /// A long bar with three solid keys in it: the command bar.
    PaletteCommandBar = "palette-command-bar" => palettes::PALETTE_COMMAND_BAR,
    /// A cup with three pens standing in it: the palette of tools.
    PaletteTools = "palette-tools" => palettes::PALETTE_TOOLS,
    /// A solid square with a branch running down from it to two boxes: the sub tools of the tool.
    PaletteSubTools = "palette-sub-tools" => palettes::PALETTE_SUB_TOOLS,
    /// Three upright sliders with round knobs at different heights: the settings of the tool.
    PaletteToolProperty = "palette-tool-property" => palettes::PALETTE_TOOL_PROPERTY,
    /// Three solid dots in a row, each much larger than the one before, filling the width: the
    /// brush sizes.
    PaletteBrushSize = "palette-brush-size" => palettes::PALETTE_BRUSH_SIZE,
    /// Four rings of four sizes laid out in a square: the brush sizes as a grid.
    PaletteBrushGrid = "palette-brush-grid" => palettes::PALETTE_BRUSH_GRID,
    /// A ring with a square in it and a solid knob on the ring: the wheel of hues, and the square
    /// that sets how light and how strong the color is.
    PaletteColorWheel = "palette-color-wheel" => palettes::PALETTE_COLOR_WHEEL,
    /// Three level bars, each with a solid knob at its own place: the color sliders.
    PaletteColorSliders = "palette-color-sliders" => palettes::PALETTE_COLOR_SLIDERS,
    /// Three color cards fanned out from one rivet: a set of colors to pick from.
    PaletteColorSet = "palette-color-set" => palettes::PALETTE_COLOR_SET,
    /// A strip cut into three cells, the middle one solid: a color that lies between the colors
    /// on either side of it.
    PaletteIntermediate = "palette-intermediate" => palettes::PALETTE_INTERMEDIATE,
    /// A square cut into three by three cells, the middle one solid: colors close to the current
    /// one.
    PaletteApproximate = "palette-approximate" => palettes::PALETTE_APPROXIMATE,
    /// An arrow pointing back to the left over a row of three color chips, the first one solid:
    /// the colors used before.
    PaletteColorHistory = "palette-color-history" => palettes::PALETTE_COLOR_HISTORY,
    /// A frame with a smaller frame inside it, up and to the right: the part of the picture the
    /// view shows.
    PaletteNavigator = "palette-navigator" => palettes::PALETTE_NAVIGATOR,
    /// A picture with a push pin through its top: a picture kept beside the canvas to look at.
    PaletteSubView = "palette-sub-view" => palettes::PALETTE_SUB_VIEW,
    /// A ball, a block and a cone standing on a shelf: the materials kept for use in pictures.
    PaletteMaterials = "palette-materials" => palettes::PALETTE_MATERIALS,
    /// Three steps in a column, each a ring with its line of text, the rings joined by a line and
    /// the last one solid: what was done, in order.
    PaletteHistory = "palette-history" => palettes::PALETTE_HISTORY,
    /// A solid play mark beside three lines: a list of steps that run by themselves.
    PaletteAutoAction = "palette-auto-action" => palettes::PALETTE_AUTO_ACTION,
    /// A box with its lid on and a handle slot on its front: items put away to use again.
    PaletteItemBank = "palette-item-bank" => palettes::PALETTE_ITEM_BANK,
    /// A cross hair beside two lines of numbers: where the pointer is and what is under it.
    PaletteInfo = "palette-info" => palettes::PALETTE_INFO,
    /// Two tracks of different lengths crossed by a playhead with a solid head: the timeline.
    PaletteTimeline = "palette-timeline" => palettes::PALETTE_TIMELINE,
    /// Two cels lying one behind the other, peg holes along the top: the cels of an animation.
    PaletteCels = "palette-cels" => palettes::PALETTE_CELS,
    /// A curve rising in a corner between two axes, with a solid point at its end: how the pressure
    /// of the pen sets the width of the stroke.
    PressureCurve = "pressure-curve" => palettes::PRESSURE_CURVE,
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
