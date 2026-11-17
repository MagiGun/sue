//! The Sue Paint studio: its state between frames, the commands behind the
//! menus and shortcuts, and the applet the shell runs.
//!
//! The studio keeps no picture of its own (backbone rule 1). Every open
//! canvas tab is a picture entity in the shared arena; the studio holds its
//! view, its screen image ([`Compositor`]), the current layer and the
//! selection. Pictures appear as tabs as soon as they exist in the arena,
//! from a New dialog, an opened file, a loaded project or a redo.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, vec2};
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D, ReversibleTransaction, Value};

use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, AppletId, ArenaEdit, Menu, MenuBar};

use crate::color::{self, History};
use crate::composite::Compositor;
use crate::dialogs::Dialog;
use crate::doc::{self};
use crate::select::{Op, Selection};
use crate::tools::Toolbox;
use crate::view::View;

mod applet;
mod assets;
pub(crate) use assets::{FILE_OBJECT, THEME_DENSITY};
mod clipboard;
mod edit;
pub(crate) mod exports;
mod files;
mod frame;
mod layers;
pub(crate) use layers::ASK_DELETE_FOLDERS;
mod materials;
mod media;
mod rulers;
mod saving;
mod three;

/// Fingers on the canvas since the first came down: whether, when they
/// lift, it was a tap of two (undo) or three (redo) fingers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TouchTap {
    /// When the first finger came down.
    pub began: f64,
    /// The most fingers down at once.
    pub most: usize,
    /// How far the fingers moved, in points.
    pub travel: f32,
    /// The view's zoom and pan when they came down: a pinch or a drag
    /// changes them, and is no tap.
    pub view: (f32, egui::Vec2),
}

/// The applet id.
pub const ID: &str = "sue-paint";

/// The picture files Sue Paint opens as canvases.
const OPENS: &[&str] = &[
    "clip", "png", "jpg", "jpeg", "jxl", "psd", "psb", "webp", "tif", "tiff", "bmp", "gif", "tga",
    "qoi", "ico", "ppm", "pgm", "pbm", "pnm", "pam",
];

/// The brush files Sue Paint makes sub tools of when one is handed to
/// Sue. A picture can be a brush tip too, but handed to Sue it is a
/// picture; the Tool Group palette imports it as a brush.
const BRUSHES: &[&str] = &["sut", "abr", "gbr", "gih"];

/// The sound files that go on the timeline.
const SOUNDS: &[&str] = &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"];

/// `true` when the extension of `path` is one of `extensions`, in any
/// case.
fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase().as_str()))
}

/// `true` for a picture file Sue Paint opens as a canvas.
pub(crate) fn opens(path: &Path) -> bool {
    has_extension(path, OPENS)
}

/// One open canvas tab.
#[derive(Debug)]
pub(crate) struct Doc {
    pub picture: EntityId,
    pub view: View,
    pub compositor: Compositor,
    pub layer: Option<EntityId>,
    /// The animation frame shown (from 0).
    pub frame: u32,
    /// The animation folder (timeline track) being edited.
    pub track: usize,
    /// Playing since then, from that frame.
    pub playing: Option<(sue_platform::time::Instant, u32)>,
    /// Strokes follow the special, perspective and symmetrical rulers
    /// (Snap to Special Ruler).
    pub ruler_on: bool,
    /// Strokes that start near the linear ruler follow it (Snap to Ruler).
    pub snap_ruler: bool,
    /// The rulers show, and strokes follow them (Show Ruler).
    pub show_rulers: bool,
    /// Drawing edits the current layer's mask, not its pixels.
    pub mask_edit: bool,
    /// The Quick Mask layer while Quick Mask is on.
    pub quick_mask: Option<EntityId>,
    /// Layers picked with the current one (Ctrl+click), for Merge
    /// Selected Layers.
    pub picked: Vec<EntityId>,
    pub selection: Option<Selection>,
    pub last_selection: Option<Selection>,
    pub edges: Option<Vec<[[f32; 2]; 2]>>,
    /// The window of its own this canvas shows in, by the number its
    /// view is named with (`None`: a tab of the canvas column).
    pub window: Option<u64>,
}

impl Doc {
    pub(crate) fn new(picture: EntityId) -> Self {
        Self {
            picture,
            view: View::default(),
            compositor: Compositor::new(picture),
            layer: None,
            frame: 0,
            track: 0,
            playing: None,
            ruler_on: true,
            snap_ruler: true,
            show_rulers: true,
            mask_edit: false,
            quick_mask: None,
            picked: Vec::new(),
            selection: None,
            last_selection: None,
            edges: None,
            window: None,
        }
    }

    /// Replaces the selection (`None` deselects), keeping the old one for
    /// Reselect.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        let selection = selection.filter(|selection| !selection.is_empty());
        if let Some(old) = self.selection.take() {
            self.last_selection = Some(old);
        }
        self.selection = selection;
        self.edges = None;
    }
}

/// A stroke Smart Shape corrected while the pen is still down: the shape it
/// became, and what a drag does to it.
#[derive(Clone, Debug)]
pub(crate) struct Shaping {
    /// The shape as corrected, in layer pixels.
    pub(crate) path: Vec<[f32; 2]>,
    /// What a drag turns and scales it about (`smart::pivot`).
    pub(crate) pivot: [f32; 2],
    /// The point of the shape the pen was at when it became one, which a
    /// drag carries to wherever the pen goes.
    pub(crate) reference: [f32; 2],
    /// Where the pen was at the last redraw, so a still pen redraws nothing.
    pub(crate) at: [f32; 2],
}

/// What a press on the canvas started.
#[derive(Debug)]
pub(crate) enum Gesture {
    /// A brush stroke, on the brush engine (`canvas/engine.rs`).
    Engine(Box<crate::canvas::engine::EngineStroke>),
    /// The eraser on a vector layer: its pixels and lines from before, the
    /// lines now, and where the eraser last was (layer pixels).
    VectorErase {
        layer: EntityId,
        before: Vec<u8>,
        geometry: sue_model::Geometry,
        lines: Vec<crate::vector::Line>,
        last: [f32; 2],
        radius: f32,
        mode: crate::vector::Erase,
        offset: [i64; 2],
        changed: bool,
        label: String,
    },
    /// A Correct Line drag on vector `layer`: its pixels and lines from
    /// before, the lines as the drag began and now, the control point or
    /// place it holds (line, sample, distance along), where it began and
    /// last was (layer pixels), the path traced with its pressure, and the
    /// lines Process whole line has done.
    Correct {
        layer: EntityId,
        before: Vec<u8>,
        geometry: sue_model::Geometry,
        base: Vec<crate::vector::Line>,
        lines: Vec<crate::vector::Line>,
        grab: Option<(usize, usize, f32)>,
        start: [f32; 2],
        last: [f32; 2],
        path: Vec<([f32; 2], f32)>,
        done: Vec<usize>,
        offset: [i64; 2],
        changed: bool,
        label: String,
    },
    /// The Object tool moving vector lines: the layer's pixels and lines
    /// from before, the lines as the drag began, where it began and how far
    /// it has moved them (layer pixels).
    MoveLines {
        layer: EntityId,
        before: Vec<u8>,
        geometry: sue_model::Geometry,
        base: Vec<crate::vector::Line>,
        start: [f32; 2],
        by: [f32; 2],
        offset: [i64; 2],
    },
    /// Drawing a ruler with the Ruler Pen (picture pixels).
    RulerPen { points: Vec<[f32; 2]> },
    /// The Object tool on a camera folder: where the drag began (picture
    /// pixels), the camera then, and the handle it grabbed (inside moves,
    /// a corner or an edge zooms, the stalk or outside turns).
    Camera {
        folder: EntityId,
        start: [f32; 2],
        base: crate::camera::Camera,
        grip: crate::transform_ui::FrameGrip,
    },
    /// Painting the selection with the Selection pen (or out of it with
    /// Erase selection): where the pen last was (picture pixels).
    SelectPaint { add: bool, last: [f32; 2] },
    /// The Object tool turning a 3D layer's camera: where the drag began
    /// (screen), the view then, its pixels then, and how it moves (0 turn,
    /// 1 pan, 2 zoom).
    Orbit {
        layer: EntityId,
        start: Pos2,
        base: crate::three::View3D,
        before: Vec<u8>,
        mode: u8,
    },
    /// Posing a drawing figure: bending (or, with `twist`, twisting) its
    /// `joint`; its pose and pixels from before.
    Pose {
        layer: EntityId,
        joint: usize,
        start: Pos2,
        base: crate::figure::Pose,
        before: Vec<u8>,
        /// How the drag poses: 0 bends and swings the joint, 1 twists
        /// it, 2 takes the part where the pointer goes (a hand or foot
        /// with its limb following), 3 to 5 turn the joint about its own
        /// x, y or z (a ring of the rotation manipulator).
        how: u8,
        /// Where on the layer the drag began.
        grip: [f32; 2],
        /// The point of the model the drag carries, and for a ring the
        /// joint on the layer and which way round the ring runs.
        like: [f32; 3],
    },
    /// Dragging a ruler's handle: the ruler's text from before.
    RulerHandle {
        handle: crate::rulers::Handle,
        before: Option<String>,
    },
    /// Dragging puppet warp's pins: moving or turning the picked ones,
    /// or a box that picks them.
    Pin {
        drag: crate::canvas::puppet::PinDrag,
    },
    /// A Blend tool drag on `layer`: its pixels from before, where the
    /// last dab was (layer pixels), and how far the pen went since.
    Blend {
        layer: EntityId,
        before: Vec<u8>,
        last: [f32; 2],
        since: f32,
        offset: [i64; 2],
        label: String,
        /// On a gray mask (`layer` is the mask): its values as RGBA that
        /// the blend dabs work on, `before` being its gray bytes.
        work: Option<Vec<u8>>,
    },
    /// Dragging a transform handle: what it grabbed, where (picture
    /// pixels), the box as it was, the rotation centre or lattice point
    /// it grabbed, and the lattice's points then.
    Transform {
        handle: crate::transform_ui::Handle,
        start: [f32; 2],
        quad: crate::transform::Quad,
        center: [f32; 2],
        scale: [f32; 2],
        angle: f32,
        grab: [f32; 2],
        points: Vec<[f32; 2]>,
    },
    /// Scrolling the view.
    Pan { last: Pos2 },
    /// Ctrl+Alt and drag: the brush size follows the pointer across.
    BrushSize { start: f32, size: f32 },
    /// Turning the view.
    Rotate { start: f32, rotation: f32 },
    /// A rectangle, ellipse, line or gradient from `from` to `to` (picture
    /// pixels).
    Shape {
        tool: crate::tools::Tool,
        sub: Box<crate::tools::SubTool>,
        op: Op,
        from: Pos2,
        to: Pos2,
    },
    /// A lasso selection.
    Lasso { op: Op, points: Vec<[f32; 2]> },
    /// Lasso fill (Figure) or Close and fill (Fill): drawn round, then
    /// filled with the drawing color.
    LassoFill {
        sub: Box<crate::tools::SubTool>,
        close: bool,
        points: Vec<[f32; 2]>,
    },
    /// The Object tool moving a panel or effect lines.
    Comic(Box<crate::comic::ComicEdit>),
    /// A balloon drawn by hand (the Balloon pen).
    Outline {
        sub: Box<crate::tools::SubTool>,
        points: Vec<[f32; 2]>,
    },
    /// The Light table tool moving, scaling or turning the light table's
    /// layers: each one's place as the drag began (and the value it was
    /// kept as, for an exact undo), where the drag began
    /// (picture pixels), the handle of the box it grabbed and the box.
    LightTable {
        layers: Vec<(EntityId, crate::anim::Place, Option<Value>)>,
        start: Pos2,
        grip: crate::transform_ui::FrameGrip,
        quad: crate::transform::Quad,
    },
    /// The Object tool on a layer whose keyframes are on: where the drag
    /// began (picture pixels), how the layer stood, the handle of its box
    /// it grabbed and the box, and the key shown while it moves (applied
    /// in place, put back before the real step).
    Keyed {
        layer: EntityId,
        start: [f32; 2],
        base: crate::anim::motion::Pose,
        grip: crate::transform_ui::FrameGrip,
        quad: crate::transform::Quad,
        preview: Option<Box<crate::doc::Edit>>,
    },
    /// Moving layers (one, or a folder's): each layer, where it was and
    /// its position properties before.
    Move {
        layers: Vec<(EntityId, [i64; 2], [Option<Value>; 2])>,
        start: Pos2,
    },
    /// Picking colors while the button is down.
    Pick { mode: u8 },
    /// The Fill tool pressed and dragged: every area the drag crosses
    /// fills on release, as one undo step (`seeds` in picture pixels).
    Bucket {
        sub: Box<crate::tools::SubTool>,
        seeds: Vec<[f32; 2]>,
    },
    /// Shrink selection: a lasso whose selection shrinks to what is drawn
    /// inside it.
    ShrinkLasso {
        sub: Box<crate::tools::SubTool>,
        op: Op,
        points: Vec<[f32; 2]>,
    },
    /// A drag that scrubs: the Zoom tool's zoom, or the Operation tool's
    /// Edit timeline through the frames; from `start` on screen, with the
    /// zoom or the frame it began at.
    Scrub {
        tool: crate::tools::Tool,
        sub: Box<crate::tools::SubTool>,
        start: Pos2,
        base: f32,
        moved: bool,
    },
    /// A loop picking vector lines, the picked lines' box dragged, or
    /// the Vector eraser over several layers (`canvas/lines.rs`).
    Lines(Box<crate::canvas::lines::LinesGesture>),
    /// The Gradient tool on a gradient layer: one end of its line moved
    /// (`end` 0 the start, 1 the end), the other kept (picture pixels).
    GradientLine {
        layer: EntityId,
        line: [[f32; 2]; 2],
        end: usize,
    },
    /// Move layer moving a pair of number properties of `entity` (a
    /// layer's tone dots, the picture's grid): `keys` as they were
    /// (`before`, read as `base`), from where the drag began (picture
    /// pixels). The undo step `label` is made when it is let go.
    Nudge {
        entity: EntityId,
        keys: [&'static str; 2],
        base: [f64; 2],
        before: [Option<Value>; 2],
        start: Pos2,
        label: String,
    },
    /// A handle of Change Canvas Size's box on the canvas dragged:
    /// `handle` 0 to 3 the corners from the top left clockwise, 4 to 7
    /// the edges from the top, 8 the inside; the box as it was when the
    /// drag began, from `start` (picture pixels).
    CanvasSize {
        handle: usize,
        start: Pos2,
        from: crate::dialogs::CanvasDialog,
    },
}

/// The Gradient tool's work between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct GradientWork {
    /// The stop the Tool Property bar edits.
    pub stop: usize,
    /// A gradient layer's new gradient from Tool Property, shown on the
    /// next frame.
    pub pending: Option<(EntityId, doc::Gradient)>,
    /// A gradient layer shown changed while a control is held, and its
    /// gradient from before: kept as one undo step when let go.
    pub preview: Option<(EntityId, doc::Gradient)>,
}

/// A rectangle or ellipse selection waiting for its angle (Adjust angle
/// after fixed): dragged from `from` to `to` (picture pixels) on
/// `picture`, combined as `op` when a click fixes it.
#[derive(Clone, Debug)]
pub(crate) struct SelectTurn {
    pub picture: EntityId,
    pub sub: Box<crate::tools::SubTool>,
    pub op: Op,
    pub from: Pos2,
    pub to: Pos2,
}

/// Which palettes show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "one switch per palette")]
pub(crate) struct Panels {
    pub command_bar: bool,
    pub tool: bool,
    pub sub_tool: bool,
    pub tool_property: bool,
    pub brush_size: bool,
    pub color: bool,
    pub color_set: bool,
    pub navigator: bool,
    pub layer: bool,
    pub timeline: bool,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            command_bar: true,
            tool: true,
            sub_tool: true,
            tool_property: true,
            brush_size: true,
            color: true,
            color_set: true,
            navigator: true,
            layer: true,
            timeline: true,
        }
    }
}

/// A slider dragging a property: it changes in place and becomes one undo
/// step when the drag ends.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LiveProperty {
    pub id: EntityId,
    pub key: String,
    pub old: Option<Value>,
}

/// Pixels copied from a layer and where they came from.
#[derive(Clone, Debug)]
pub(crate) struct Clip {
    pub raster: RasterTile2D,
    pub origin: [u32; 2],
}

/// Sue Paint.
pub struct SuePaint {
    pub(crate) toolbox: Toolbox,
    pub(crate) main: [u8; 3],
    pub(crate) sub: [u8; 3],
    pub(crate) sub_active: bool,
    /// Painting with the transparent color: brushes erase (CSP's C).
    pub(crate) transparent: bool,
    /// The menu bar and command bar are hidden (CSP's Shift+Tab).
    pub(crate) menus_hidden: bool,
    /// Where the palettes are docked, floating or collapsed.
    pub(crate) dock: crate::dock::Dock,
    /// The Sub View's reference pictures.
    pub(crate) sub_view: crate::subview::SubView,
    /// The color sets.
    pub(crate) color_sets: crate::colorsets::ColorSets,
    /// The color set's settings menu is open.
    pub(crate) set_menu: bool,
    /// What the color palettes keep between frames: the Color Slider's
    /// mode, the Color Set's menu and the swatch being dragged.
    pub(crate) color_ui: crate::colorsets::ColorUi,
    /// Navigator or Sub View.
    pub(crate) nav_tab: u8,
    /// Each picture's version when last saved or opened, for the tabs'
    /// modified dot.
    pub(crate) clean: HashMap<EntityId, u64>,
    /// Where in the undo history each picture was last saved or opened
    /// (File, Revert goes back there).
    pub(crate) saved_at: HashMap<EntityId, sue_core::NodeId>,
    /// The `.clip` file each picture was opened from or saved to (Save
    /// writes it again).
    pub(crate) clip_paths: HashMap<EntityId, PathBuf>,
    /// Where Save writes each canvas that does not go in the project: a
    /// `.clip` or a flat picture it was opened from.
    pub(crate) save_targets: HashMap<EntityId, saving::SaveTarget>,
    /// The question Save is asking, if any.
    pub(crate) save_ask: Option<saving::SaveAsk>,
    /// The part of the active picture on screen, `[x0, y0, x1, y1]`, and
    /// the picture pixels a side one screen pixel spans there (see
    /// `composite::view_step`), as of the last frame.
    pub(crate) on_screen: ([u32; 4], u32),
    /// The pictures opened or saved lately, newest first (File, Open
    /// Recent), from the work Sue remembers.
    pub(crate) recent_files: Vec<PathBuf>,
    /// The revision of the work Sue remembers they were read at.
    pub(crate) recent_seen: Option<u64>,
    /// The Auto Actions palette's actions.
    pub(crate) auto: crate::extras::AutoActions,
    /// The Item Bank's sub tools.
    pub(crate) item_bank: Vec<crate::extras::Item>,
    /// The registered workspaces.
    pub(crate) workspaces: Vec<crate::extras::Workspace>,
    /// The workspace Reset to Default goes back to, once people chose one
    /// (Use Current Workspace as Default).
    pub(crate) default_workspace: Option<crate::extras::Workspace>,
    /// Canvas presets registered in the New dialog.
    pub(crate) canvas_presets: Vec<crate::dialogs::CanvasPreset>,
    /// Every open canvas shows at once, side by side (Window, Canvas,
    /// Tile).
    pub(crate) tiled: bool,
    /// How pictures go on paper (File, Print Settings).
    pub(crate) print: crate::print::PrintSettings,
    /// The command bar's buttons (Command Bar Settings).
    pub(crate) bar: crate::bar_items::Bar,
    /// Quick Access's sets of buttons.
    pub(crate) quick_access: crate::bar_items::QuickAccess,
    /// The bars' popovers and the button being dragged.
    pub(crate) bar_ui: crate::bar_items::BarUi,
    /// Rulers along the canvas's top and left (View, Ruler Bar).
    pub(crate) ruler_bar: bool,
    /// The canvas's scrollbars show (View, Scroll Bar).
    pub(crate) scroll_bars: bool,
    /// Edit, Advanced Fill's settings, kept between uses.
    pub(crate) advanced_fill: crate::dialogs::AdvancedFill,
    /// `display.interface` as the rice has it (for the Interface menu).
    pub(crate) interface: String,
    /// Strokes near a grid line follow it (View, Snap to Grid).
    pub(crate) snap_grid: bool,
    /// A page's crop marks and inner border show (View, Crop Frame).
    pub(crate) crop_frame: bool,
    /// The bleed beyond the finished page shows (View, Bleed).
    pub(crate) bleed: bool,
    /// The page shows as it will be cut, the bleed dimmed (View, Show
    /// Page Image).
    pub(crate) page_image: bool,
    /// Screen tone layers are tinted (View, Tone Area).
    pub(crate) tone_area: bool,
    /// The View menu's safety margin, vector paths and playback rate.
    pub(crate) view_marks: crate::commands::ViewMarks,
    /// The picture shows as printed (View, Color Profile, Preview).
    pub(crate) proof_on: bool,
    /// Which of [`crate::proof::PROFILES`].
    pub(crate) proof_profile: usize,
    /// The proofed picture and the version and profile it was made for.
    pub(crate) proof_cache: Option<(u64, usize, Vec<u8>)>,
    /// The tone area mask and the version it was made for.
    pub(crate) tone_cache: Option<(u64, [u32; 2], Vec<u8>)>,
    /// The sheet the phone layout shows, if any.
    pub(crate) sheet: Option<crate::phone::Sheet>,
    /// The layout the studio was last drawn in, to lay the palettes out
    /// again when it changes.
    pub(crate) laid_for: Option<sue_platform::FormFactor>,
    /// The desktop's palette layout, kept while a tablet layout shows.
    pub(crate) desk_dock: Option<crate::dock::Dock>,
    /// The Window menu's palettes as the dock last drew them, so a palette
    /// shown again comes to the front of its tabs.
    pub(crate) panels_seen: Panels,
    /// The palette layout as last written to the data folder, and since
    /// when (in the input's seconds) it differs, to write it again.
    pub(crate) layout_kept: Vec<String>,
    pub(crate) layout_changed: Option<f64>,
    /// The fingers on the canvas: each touch's id and where it is.
    pub(crate) touches: Vec<(u64, Pos2)>,
    /// A touch gesture in progress: when it began, the most fingers it had
    /// and how far they moved, for two- and three-finger taps.
    pub(crate) touch_gesture: Option<TouchTap>,
    pub(crate) hue: f32,
    pub(crate) recent: History,
    pub(crate) docs: Vec<Doc>,
    pub(crate) active: usize,
    pub(crate) closed: HashSet<EntityId>,
    pub(crate) menus: Vec<Menu>,
    /// When to build the menus again, in another language.
    pub(crate) language: sue_lang::Watch,
    /// The shortcuts as Settings has them, for the menus and the shell.
    pub(crate) bound: sue_wm::shortcuts::Bound,
    /// The key each command palette entry shows, as Settings has it, and
    /// the settings' revision it was read at.
    pub(crate) palette_keys: (Option<u64>, Vec<(String, String)>),
    pub(crate) menubar: MenuBar,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) gesture: Option<Gesture>,
    /// Puppet warp, while it is on.
    pub(crate) warp: Option<WarpSession>,
    /// The first of the two lines that make a vanishing point (Perspective
    /// Ruler, Add vanishing point), in picture pixels.
    pub(crate) vanishing_line: Option<[[f32; 2]; 2]>,
    /// The vector lines the Object tool has picked: their layer and
    /// indices.
    pub(crate) picked_lines: Option<(EntityId, Vec<usize>)>,
    /// The points of a curve ruler being placed (picture pixels).
    pub(crate) curve_points: Vec<[f32; 2]>,
    /// The corners of a polyline selection, balloon or frame being
    /// clicked out.
    pub(crate) polyline: Vec<[f32; 2]>,
    /// The tool and sub tool mode clicking out `polyline`.
    pub(crate) polyline_for: Option<(crate::tools::Tool, u8)>,
    /// A command a palette asked for, run when the palette has a context.
    pub(crate) pending_command: Option<&'static str>,
    /// Effect line settings changed in Tool Property, to draw again.
    pub(crate) pending_effect: Option<(EntityId, crate::manga::EffectLines)>,
    /// The Selection Launcher shows under the selection.
    pub(crate) launcher: bool,
    /// The Transform launcher shows under a transform's box.
    pub(crate) transform_launcher: bool,
    /// The Text launcher shows under the current text layer.
    pub(crate) text_launcher: bool,
    /// The Object launcher shows under the current layer's drawing with
    /// the Operation tool.
    pub(crate) object_launcher: bool,
    /// Where the Object launcher's layer has its drawing: the layer, its
    /// revision, and the picture pixels it covers (none when empty).
    pub(crate) object_bounds: Option<(EntityId, u64, Option<[f32; 4]>)>,
    /// How Export Webtoon cuts and scales the picture.
    pub(crate) webtoon: (u32, u32),
    /// The timeline drives what shows (Enable Timeline).
    pub(crate) timeline_on: bool,
    /// The timeline's sound mixed, and what it was mixed from.
    pub(crate) audio_mix: Option<(u64, sue_core::Shared<sue_core::PCMFloat1D>)>,
    /// The lap of playback the sound was started for (`None` while
    /// stopped).
    pub(crate) audio_lap: Option<i64>,
    /// An audio clip being dragged in the timeline: its track, where the
    /// drag began (frame) and the clip's start then.
    pub(crate) audio_drag: Option<(usize, i64, i64)>,
    /// Where the Timeline palette looks: its zoom, the first frame at its
    /// left and how far its tracks are scrolled.
    pub(crate) timeline_view: crate::panels::timeline::TimelineView,
    /// What the Timeline palette keeps besides its view: the frames
    /// picked, its clipboard, export settings waiting for their file,
    /// the thumbnail size and time unit, and the drags under way.
    pub(crate) anim_ui: crate::panels::timeline::AnimUi,
    /// A 3D layer's view being changed in Tool Property, and its view and
    /// pixels from before.
    pub(crate) view3d_before: Option<(EntityId, crate::three::View3D, Vec<u8>)>,
    /// A 3D layer view Tool Property asked for this frame, and whether its
    /// slider was let go.
    pub(crate) pending_view3d: Option<(EntityId, crate::three::View3D)>,
    pub(crate) view3d_release: bool,
    /// The drawing figure's joint picked to pose.
    pub(crate) figure_joint: Option<usize>,
    /// Posing keeps every joint within what a body can do.
    pub(crate) figure_limits: bool,
    /// The hand setup as Tool Property last left it, and the hand of a
    /// body it sets (0 left, 1 right, 2 both).
    pub(crate) hand_setup: crate::figure::HandSetup,
    pub(crate) hand_side: usize,
    /// A figure's pose being changed in Tool Property, its pose and pixels
    /// from before; the pose asked for this frame; and whether it is let go.
    pub(crate) pose_before: Option<(EntityId, crate::figure::Pose, Vec<u8>)>,
    pub(crate) pending_pose: Option<(EntityId, crate::figure::Pose)>,
    pub(crate) pose_release: bool,
    /// A figure's body shape let go of this frame, to keep (Tool Property
    /// now shapes figures live through `pending_shape`).
    pub(crate) pending_body: Option<(EntityId, crate::figure::Body)>,
    /// A 3D layer and its model as they were before a drag of Tool
    /// Property or the canvas changed them in place, to record the drag
    /// as one step when it is let go.
    pub(crate) three_draft: Option<crate::studio::three::Draft3d>,
    /// The object placement and the body shape Tool Property asked for
    /// this frame, shown at once and kept when let go.
    pub(crate) pending_place: Option<(EntityId, crate::three::Placement)>,
    pub(crate) pending_shape: Option<(EntityId, crate::figure::Body)>,
    /// The body part the body shape chart shapes (0 the full body).
    pub(crate) figure_part: usize,
    /// A camera being set up and not keyed yet: its folder, the frame and
    /// where it stands.
    pub(crate) camera_pending: Option<(EntityId, u32, crate::camera::Camera)>,
    /// One camera value being set in Tool Property and not keyed yet:
    /// its folder, the frame, the value's channel and what it is set to.
    pub(crate) camera_value: Option<(EntityId, u32, &'static str, f64)>,
    /// Edit Layers with Active Keyframes: a layer whose keyframes are on
    /// takes strokes, drawn where they show.
    pub(crate) edit_keyed: bool,
    /// What each canvas point sees on the layer a stroke is drawn on,
    /// when keyframes or a keyed folder move it at the frame shown: the
    /// stroke lands where the layer shows.
    pub(crate) stroke_warp: Option<[f32; 6]>,
    /// The Material palette: the kind it shows (0 all, then by the kinds
    /// registered) and the material picked, by name.
    pub(crate) material_kind: u8,
    pub(crate) material_pick: Option<String>,
    /// Material thumbnails, by a hash of the material.
    pub(crate) material_thumbs: HashMap<u64, Vec<u8>>,
    /// What the Information, Item Bank, Auto Action, Sub View and
    /// Material palettes keep besides the fields above (their options,
    /// kept pictures, sets and filters).
    pub(crate) extras: crate::extras::Extras,
    /// Photos being read by the Pose and Hand Scanners.
    pub(crate) scans: crate::scan::Scans,
    /// The Hand Scanner while its dialog is open: its camera, tracker
    /// and the pose to go back to.
    pub(crate) hand_scan: Option<Box<crate::dialogs::hand_scanner::HandScan>>,
    /// The kinds of material, registered at start.
    pub(crate) material_kinds: crate::materials::MaterialKinds,
    /// The Hub's Materials place (drawn by `hub_ui`), and the old cache
    /// it clears once.
    pub(crate) hub: crate::hub::MaterialsPlace,
    /// The New canvas page of phones and tablets.
    pub(crate) new_canvas: crate::mobile::NewCanvas,
    /// The drawing tool Simple Mode's swap button goes back to from the
    /// eraser.
    pub(crate) drawing_tool: crate::tools::Tool,
    /// The canvas shows without its tabs and status strip (Simple Mode).
    pub(crate) bare: bool,
    /// Studio Mode's fullscreen and modifier keys.
    pub(crate) touch: crate::studio_mode::Touch,
    /// The New canvas page's templates, registered at start.
    pub(crate) canvas_templates: crate::templates::CanvasTemplates,
    /// Each recording picture's compositor version at its last timelapse
    /// frame, the version it is waiting to settle on, and its next frame
    /// number.
    pub(crate) timelapse: HashMap<EntityId, (u64, u64, usize)>,
    /// How the timelapse being exported is laid out.
    pub(crate) timelapse_export: crate::timelapse::Export,
    /// Scale/Rotate or Free Transform, while it is on.
    pub(crate) transform: Option<crate::transform_ui::TransformSession>,
    /// Words being typed on the canvas with the Text tool.
    pub(crate) typing: Option<crate::lettering::Typing>,
    /// The keys held this frame (Shift frees a transform's aspect).
    pub(crate) modifiers: egui::Modifiers,
    /// Onion skin shows the cels around the current frame.
    pub(crate) onion: bool,
    /// The light table shows (Enable Light Table).
    pub(crate) light_table: bool,
    /// How much the light table's layers show, 0 to 1.
    pub(crate) light_opacity: f32,
    /// The light table's layers show in one color (Color mode).
    pub(crate) light_color: bool,
    /// The cel's own light table shows (Show cel-specific light table).
    pub(crate) light_cel: bool,
    /// The general light table shows on every cel (Show general light
    /// table).
    pub(crate) light_general: bool,
    /// How much the general light table's layers show, 0 to 1.
    pub(crate) general_opacity: f32,
    /// The opacity slider sets the general light table's (Switch opacity
    /// target); else the cel's.
    pub(crate) opacity_general: bool,
    /// Dragging on the canvas moves the light table's layers (Light table
    /// tool).
    pub(crate) light_tool: bool,
    /// The one color the light table's layers show in while Color mode is
    /// on (Change layer color).
    pub(crate) light_tint: [u8; 3],
    /// Copy stamp: where Alt and click set it to copy from, and how far
    /// from the brush that is once a stroke starts (picture pixels).
    pub(crate) stamp: Option<([f32; 2], Option<[f32; 2]>)>,
    /// The cel kept as the one being drawn whatever the frame (Lock
    /// current animation cel as editing target).
    pub(crate) cel_lock: Option<sue_core::EntityId>,
    /// Pictures whose older animation folders were gathered this session.
    pub(crate) gathered: std::collections::HashSet<EntityId>,
    /// The layer being dragged in the Layer palette.
    pub(crate) layer_drag: Option<EntityId>,
    /// Playback starts over at the end.
    pub(crate) looping: bool,
    /// How hard the window's touch presses (its force, through the pen
    /// pressure curve) while it is down: a stroke drawn from the
    /// pointer's events alone takes it.
    pub(crate) pressure: Option<f32>,
    /// The pen pressure curve every tool uses (Pen Pressure Settings).
    pub(crate) pen_curve: sue_model::Response,
    /// The hint about fitting pen pressure was given (once a run).
    pub(crate) pen_hinted: bool,
    /// The pen's eraser end picks the Eraser, and turning the pen back the
    /// tool before (Clip Studio Paint).
    pub(crate) eraser_end: crate::tools::EraserEnd,
    pub(crate) panels: Panels,
    pub(crate) color_tab: u8,
    /// The Intermediate Color palette's corner colors (top left, top right,
    /// bottom left, bottom right) and how many steps it blends them in.
    pub(crate) intermediate: ([[u8; 3]; 4], u32),
    /// The Approximate Color palette's axes (across, down: 0 hue, 1
    /// saturation, 2 brightness) and how far it reaches, 0 to 1.
    pub(crate) approximate: (u8, u8, f32),
    pub(crate) set_tab: u8,
    pub(crate) tool_tab: u8,
    pub(crate) layer_tab: u8,
    pub(crate) select_op: Op,
    pub(crate) renaming: Option<(EntityId, String)>,
    pub(crate) live: Option<LiveProperty>,
    pub(crate) grid: bool,
    pub(crate) pixel_grid: bool,
    pub(crate) show_selection: bool,
    pub(crate) clipboard: Option<Clip>,
    pub(crate) hex: String,
    pub(crate) viewport: Rect,
    pub(crate) pointer: Option<Pos2>,
    pub(crate) thumbs: HashMap<EntityId, (u64, Vec<u8>, [u32; 2])>,
    /// Page thumbnails for the Page Manager, by page.
    pub(crate) page_thumbs: HashMap<EntityId, (u64, Vec<u8>, [u32; 2])>,
    pub(crate) preview: Option<(u64, Vec<u8>)>,
    /// Each sub tool's sample stroke for the Sub Tool list, by its tip.
    pub(crate) sub_previews: std::collections::HashMap<u64, Vec<u8>>,
    pub(crate) last_dir: Option<PathBuf>,
    /// An encoded export waiting for the save dialog to say where it goes.
    pub(crate) pending_export: Option<Vec<u8>>,
    pub(crate) shown: bool,
    /// The time of the pointer sample being handled and the time between
    /// samples this frame, in seconds.
    pub(crate) clock: (f64, f32),
    /// How far the panels are out of view while the studio comes in or
    /// goes out (0 when all are in place).
    pub(crate) hide: f32,
    /// `true` once the studio is all in and takes input.
    pub(crate) ready: bool,
    /// How opaque the view's ground and picture are, 0 to 1.
    pub(crate) cover: f32,
    /// `true` while the studio is the one the shell shows (not leaving).
    pub(crate) present: bool,
    /// The id of the layout style in use (`sue_paint.layout_style`, as
    /// `layout_style::current` reads it), to follow a change made in
    /// Settings, by the Window menu or by undo.
    pub(crate) layout_style: &'static str,
    /// While Hide All Palettes hides them: the palettes and the dock as
    /// they were, which Show All Palettes puts back exactly.
    pub(crate) hidden_all: Option<(Panels, crate::dock::Dock)>,
    /// A collapsed palette opened from the strip stays open as a tab until
    /// its icon is clicked again (Display palette as tab), instead of
    /// closing on a click elsewhere (Display palette as pop-up).
    pub(crate) strip_as_tab: bool,
    /// The registered workspace last used, checked in Window, Workspace
    /// and named by Reload.
    pub(crate) current_workspace: Option<String>,
    /// Each layout style left this run and its palettes as they were, so
    /// switching styles and back loses nothing even without a data folder.
    pub(crate) kept_styles: Vec<(&'static str, Vec<String>)>,
    /// The pen's samples since its latest touch down, for a stroke of the
    /// new brush engine (see `canvas/engine.rs`).
    pub(crate) pen_stream: Vec<sue_platform::stylus::PenSample>,
    /// A Figure line under way between clicks: a Curve to bend, a
    /// Bezier Curve's handles (see `canvas/shape.rs`).
    pub(crate) figure_draft: crate::canvas::FigureDraft,
    /// What the modifier keys, the wheel and the right and middle
    /// buttons do on the canvas, read from its file on first use.
    pub(crate) modifier_keys: Option<crate::modifier_keys::ModifierKeys>,
    /// A tool picked by holding its key, to go back from when the key
    /// comes up.
    pub(crate) held_tool: Option<crate::modifier_keys::HeldTool>,
    /// The transform under way was started by Move layer to move the
    /// selected pixels, and ends when the drag does.
    pub(crate) move_lift: bool,
    /// How a turn of the view snaps: its step in degrees (0 turns
    /// freely), and whether Shift snaps it (by that step, else 45).
    pub(crate) turning: (f32, bool),
    /// The Selection launcher's buttons and its settings popover.
    pub(crate) launcher_set: crate::launchers::LauncherSet,
    /// How transforms, Mesh Transformation and Puppet warp behave: their
    /// Tool Property, kept between one use and the next.
    pub(crate) transform_prefs: crate::transform_ui::TransformPrefs,
    /// Where the Light table tool's centre of rotation is, from the middle
    /// of its box (picture pixels): it goes along when the box moves.
    pub(crate) light_centre: [f32; 2],
    /// The canvas chrome between frames: canvases in windows of their
    /// own, pictures dropped on the canvas, the tab menu.
    pub(crate) chrome: crate::canvas::Chrome,
    /// The Object tool's Mode on picked vector lines: Free transform
    /// moves the box's corners one by one; off, the handles scale it.
    pub(crate) line_free: bool,
    /// Adjust line thickness when scaling picked vector lines.
    pub(crate) line_thicken: bool,
    /// Lines drawn or laid on a vector layer join their ends to the ends
    /// of lines near them (Vector magnet).
    pub(crate) vector_magnet: bool,
    /// What Tool Property asked of the picked lines this frame.
    pub(crate) pending_lines: Option<crate::canvas::lines::LineChange>,
    /// The picked lines as they were while Tool Property shows a change.
    pub(crate) line_preview: Option<crate::canvas::lines::LinePreview>,
    /// The lines Tool Property's change shows now.
    pub(crate) line_lines_shown: Option<Vec<crate::vector::Line>>,
    /// The hex codes typed for the picked lines' main and sub color.
    pub(crate) line_hex: [String; 2],
    /// Fill, when it looks at vector layers, stops at their lines'
    /// middles instead of their edges (Fill up to vector path).
    pub(crate) fill_vector_path: bool,
    /// The Page Manager while it shows in the canvas area, filling it or
    /// docked beside the page (see `pages.rs`).
    pub(crate) page_view: Option<crate::pages::PageView>,
    /// The frame border panels the Object tool picked last, by layer
    /// and index, for Divide Frame Border Equally and Combine Frames.
    pub(crate) comic_picked: Option<(EntityId, Vec<usize>)>,
    /// What the tool palettes keep between frames: Sub Tool Detail's
    /// category, the naming dialog, the arrangement last kept.
    pub(crate) tool_palettes: crate::panels::ToolPalettes,
    /// What the Layer palette keeps between frames: two panes, the
    /// search, the palette color menu, where its buttons are.
    pub(crate) layer_palette: crate::panels::LayerPalette,
    /// File's export settings dialog, Batch Convert Pictures, and the
    /// file each canvas was last written to (see `studio/files.rs`).
    pub(crate) files: files::Files,
    /// Edit, Pick Screen Color while it waits for the click.
    pub(crate) screen_pick: Option<sue_platform::screen::ScreenPick>,
    /// A screen pick asked for by its command, started on the next frame
    /// (`true` hides Sue's windows while it waits).
    pub(crate) screen_pick_asked: Option<bool>,
    /// The gradient sets, read the first time a gradient map opens.
    pub(crate) gradients: crate::dialogs::GradientLibrary,
    /// The Gradient tool's bar, and a gradient layer changed from Tool
    /// Property (`canvas/tools/gradient.rs`).
    pub(crate) gradient_work: GradientWork,
    /// A rectangle or ellipse selection dragged out with Adjust angle
    /// after fixed, turning with the pointer until a click fixes it.
    pub(crate) select_turn: Option<SelectTurn>,
    /// The exports running in the background.
    pub(crate) exports: exports::Exports,
    /// Show Edited Cel Only: the cel being drawn shows in its animation
    /// folder in place of the one the frame shows.
    pub(crate) edited_cel_only: bool,
    /// Check Cel Motion or Check Surrounding Cels by Key Input under way.
    pub(crate) cel_peek: Option<crate::panels::timeline::CelPeek>,
    /// The sub tools saved as defaults, the brush shapes registered and
    /// the brush tip picker (`brush_rows`).
    pub(crate) brush_work: crate::brush_rows::BrushWork,
    /// File, New's template, put on the new canvas on the next frame, so
    /// it is a step of that canvas's own history.
    pub(crate) pending_template: Option<String>,
}

/// How the brush engine took a stroke (for the runtime tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrokeSeen {
    /// The sub tool's name.
    pub sub: String,
    /// Laid through its paint behaviours (dabs, mixing, pictures,
    /// paper), not as a round tip's coverage.
    pub behaviours: bool,
    /// Kept as a line of a vector layer.
    pub vector: bool,
}

impl Default for SuePaint {
    fn default() -> Self {
        Self {
            toolbox: Toolbox::default(),
            main: [0, 0, 0],
            sub: [255, 255, 255],
            sub_active: false,
            hue: 0.0,
            recent: History::default(),
            saved_at: HashMap::new(),
            clip_paths: HashMap::new(),
            save_targets: HashMap::new(),
            save_ask: None,
            on_screen: ([0, 0, u32::MAX, u32::MAX], 1),
            recent_files: Vec::new(),
            recent_seen: None,
            canvas_presets: Vec::new(),
            auto: crate::extras::AutoActions::default(),
            item_bank: Vec::new(),
            workspaces: Vec::new(),
            default_workspace: None,
            tiled: false,
            print: crate::print::PrintSettings::default(),
            bar: crate::bar_items::Bar::default(),
            quick_access: crate::bar_items::QuickAccess::default(),
            bar_ui: crate::bar_items::BarUi::default(),
            ruler_bar: false,
            scroll_bars: true,
            advanced_fill: crate::dialogs::AdvancedFill::default(),
            interface: "auto".to_owned(),
            snap_grid: false,
            crop_frame: true,
            bleed: true,
            page_image: false,
            tone_area: false,
            view_marks: crate::commands::ViewMarks::default(),
            proof_on: false,
            proof_profile: 0,
            proof_cache: None,
            tone_cache: None,
            sheet: None,
            laid_for: None,
            desk_dock: None,
            panels_seen: Panels::default(),
            layout_kept: Vec::new(),
            layout_changed: None,
            touches: Vec::new(),
            touch_gesture: None,
            docs: Vec::new(),
            active: 0,
            closed: HashSet::new(),
            menus: crate::menus::menus(),
            language: sue_lang::Watch::default(),
            bound: sue_wm::shortcuts::Bound::default(),
            palette_keys: (None, Vec::new()),
            menubar: MenuBar::new(),
            dialog: None,
            gesture: None,
            warp: None,
            vanishing_line: None,
            picked_lines: None,
            curve_points: Vec::new(),
            polyline: Vec::new(),
            polyline_for: None,
            pending_effect: None,
            pending_command: None,
            launcher: true,
            transform_launcher: true,
            text_launcher: true,
            object_launcher: true,
            object_bounds: None,
            webtoon: (1280, 800),
            timeline_on: true,
            audio_mix: None,
            audio_lap: None,
            audio_drag: None,
            timeline_view: crate::panels::timeline::TimelineView::default(),
            anim_ui: crate::panels::timeline::AnimUi::default(),
            camera_pending: None,
            camera_value: None,
            edit_keyed: false,
            stroke_warp: None,
            view3d_before: None,
            pending_view3d: None,
            figure_joint: None,
            figure_limits: true,
            hand_setup: crate::figure::HandSetup::default(),
            hand_side: 2,
            pose_before: None,
            pending_pose: None,
            pose_release: false,
            pending_body: None,
            three_draft: None,
            pending_place: None,
            pending_shape: None,
            figure_part: 0,
            view3d_release: false,
            material_kind: 0,
            material_pick: None,
            material_thumbs: HashMap::new(),
            extras: crate::extras::Extras::default(),
            scans: crate::scan::Scans::default(),
            hand_scan: None,
            material_kinds: crate::materials::MaterialKinds::builtin(),
            hub: crate::hub::MaterialsPlace::default(),
            new_canvas: crate::mobile::NewCanvas::default(),
            canvas_templates: crate::templates::CanvasTemplates::builtin(),
            drawing_tool: crate::tools::Tool::Pen,
            bare: false,
            touch: crate::studio_mode::Touch::default(),
            timelapse: HashMap::new(),
            timelapse_export: crate::timelapse::Export::default(),
            transform: None,
            typing: None,
            modifiers: egui::Modifiers::NONE,
            onion: false,
            light_table: true,
            light_opacity: 0.5,
            light_color: false,
            light_cel: true,
            light_general: false,
            general_opacity: 0.5,
            opacity_general: false,
            light_tool: false,
            light_tint: crate::anim::LIGHT_TABLE_TINT
                .map(|channel| (channel * 255.0).round() as u8),
            stamp: None,
            cel_lock: None,
            gathered: std::collections::HashSet::new(),
            layer_drag: None,
            looping: true,
            pressure: None,
            pen_curve: crate::dialogs::DEFAULT_PEN_PRESSURE
                .parse()
                .unwrap_or_default(),
            pen_hinted: false,
            eraser_end: crate::tools::EraserEnd::default(),
            panels: Panels::default(),
            color_tab: 0,
            intermediate: (
                [
                    [220, 60, 60],
                    [250, 220, 70],
                    [60, 110, 220],
                    [255, 255, 255],
                ],
                6,
            ),
            approximate: (0, 2, 0.5),
            set_tab: 0,
            tool_tab: 1,
            layer_tab: 0,
            select_op: Op::Replace,
            renaming: None,
            live: None,
            grid: false,
            pixel_grid: true,
            show_selection: true,
            clipboard: None,
            hex: color::hex([0, 0, 0]),
            viewport: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            pointer: None,
            thumbs: HashMap::new(),
            page_thumbs: HashMap::new(),
            preview: None,
            sub_previews: std::collections::HashMap::new(),
            last_dir: None,
            transparent: false,
            menus_hidden: false,
            dock: crate::dock::Dock::default(),
            sub_view: crate::subview::SubView::default(),
            color_sets: crate::colorsets::ColorSets::default(),
            set_menu: false,
            color_ui: crate::colorsets::ColorUi::default(),
            nav_tab: 0,
            clean: HashMap::new(),
            pending_export: None,
            shown: false,
            clock: (0.0, 1.0 / 60.0),
            hide: 0.0,
            ready: true,
            cover: 1.0,
            present: false,
            layout_style: crate::layout_style::CLASSIC,
            hidden_all: None,
            strip_as_tab: false,
            current_workspace: None,
            kept_styles: Vec::new(),
            pen_stream: Vec::new(),
            figure_draft: crate::canvas::FigureDraft::default(),
            modifier_keys: None,
            held_tool: None,
            move_lift: false,
            turning: (0.0, true),
            launcher_set: crate::launchers::LauncherSet::default(),
            transform_prefs: crate::transform_ui::TransformPrefs::default(),
            light_centre: [0.0; 2],
            chrome: crate::canvas::Chrome::default(),
            line_free: false,
            line_thicken: true,
            vector_magnet: false,
            pending_lines: None,
            line_preview: None,
            line_lines_shown: None,
            line_hex: [String::new(), String::new()],
            fill_vector_path: false,
            page_view: None,
            comic_picked: None,
            tool_palettes: crate::panels::ToolPalettes::default(),
            layer_palette: crate::panels::LayerPalette::default(),
            files: files::Files::default(),
            screen_pick: None,
            screen_pick_asked: None,
            gradients: crate::dialogs::GradientLibrary::default(),
            gradient_work: GradientWork::default(),
            select_turn: None,
            exports: exports::Exports::default(),
            edited_cel_only: false,
            cel_peek: None,
            brush_work: crate::brush_rows::BrushWork::default(),
            pending_template: None,
        }
    }
}

thread_local! {
    /// The last brush stroke begun on this thread (the one that runs the
    /// studio's frames).
    static LAST_STROKE: std::cell::RefCell<Option<StrokeSeen>> =
        const { std::cell::RefCell::new(None) };
}

/// Notes `seen` as the last brush stroke begun on this thread.
pub(crate) fn saw_stroke(seen: StrokeSeen) {
    LAST_STROKE.with(|last| *last.borrow_mut() = Some(seen));
}

/// The last brush stroke begun on this thread, as the brush engine took
/// it (for the runtime tests: every brush sub tool paints through the
/// engine's worker).
#[must_use]
pub fn last_stroke() -> Option<StrokeSeen> {
    LAST_STROKE.with(|last| last.borrow().clone())
}

/// What the canvas showed of an animation in its last frame: the frame
/// it showed (the playhead's, or another while Check Cel Motion or Check
/// Surrounding Cels shows one) and the cels it hid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewSeen {
    /// The frame shown.
    pub frame: u32,
    /// The cels not shown.
    pub hidden: std::collections::HashSet<EntityId>,
}

thread_local! {
    /// What the canvas last showed of an animation on this thread.
    static LAST_VIEW: std::cell::RefCell<Option<ViewSeen>> =
        const { std::cell::RefCell::new(None) };
}

/// Notes `seen` as what the canvas shows of an animation now.
pub(crate) fn saw_view(seen: ViewSeen) {
    LAST_VIEW.with(|last| *last.borrow_mut() = Some(seen));
}

/// What the canvas showed of an animation in the last frame on this
/// thread (for the runtime tests).
#[must_use]
pub fn last_view() -> Option<ViewSeen> {
    LAST_VIEW.with(|last| last.borrow().clone())
}

/// Picture pixels the runtime tests asked the canvas to read as it shows
/// them, and the screen colors it read there in its last frame.
type Probe = (Vec<[u32; 2]>, Vec<Option<[u8; 4]>>);

thread_local! {
    /// What the canvas is asked to read, and read (see [`probe_canvas`]).
    static CANVAS_PROBE: std::cell::RefCell<Probe> =
        const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}

/// Asks the canvas to read the screen colors of picture pixels `points`
/// in every frame from now on (for the runtime tests; none stops it).
pub fn probe_canvas(points: Vec<[u32; 2]>) {
    CANVAS_PROBE.with(|probe| *probe.borrow_mut() = (points, Vec::new()));
}

/// The screen colors the canvas showed at the points [`probe_canvas`]
/// asked for, in its last frame on this thread.
#[must_use]
pub fn canvas_probed() -> Vec<Option<[u8; 4]>> {
    CANVAS_PROBE.with(|probe| probe.borrow().1.clone())
}

/// Reads the points asked for off `compositor`, when any are.
pub(crate) fn probe_with(compositor: &crate::composite::Compositor) {
    CANVAS_PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        if probe.0.is_empty() {
            return;
        }
        let seen = probe
            .0
            .iter()
            .map(|[x, y]| compositor.pixel(*x, *y))
            .collect();
        probe.1 = seen;
    });
}

impl std::fmt::Debug for SuePaint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&tr!("suepaint"))
            .field("tool", &self.toolbox.tool())
            .field("docs", &self.docs.len())
            .finish_non_exhaustive()
    }
}

/// A texture key for an entity, so each picture and thumbnail has its own.
/// A layer made from a selection (Convert to Selection Layer), `Bool`.
pub(crate) const SELECTION_LAYER: &str = "layer.selection";

pub(crate) fn texture_key(id: EntityId, salt: u64) -> u64 {
    let bits = id.to_u128();
    (bits as u64) ^ ((bits >> 64) as u64) ^ salt
}

/// Puppet warp on one layer: its pixels from before, the pins, and what
/// the preview last showed.
#[derive(Clone, Debug)]
pub(crate) struct WarpSession {
    pub layer: EntityId,
    pub size: [u32; 2],
    pub before: Vec<u8>,
    pub pins: Vec<crate::warp::Pin>,
    pub shown: Vec<crate::warp::Pin>,
    /// The layer's offset in the picture.
    pub offset: [i64; 2],
    /// The settings the preview last showed.
    pub shown_settings: Option<crate::warp::PuppetSettings>,
    /// The mesh's triangles as the pins bend them, for Show mesh.
    pub mesh: Vec<crate::warp::Triangle>,
    /// The undo step's name: the menu item that started it (Puppet Warp
    /// or Mesh Transformation).
    pub label: String,
}

impl SuePaint {
    // ----- Puppet warp ------------------------------------------------------

    // ----- Reading the state ------------------------------------------------

    /// `true` while the studio slides away.
    pub(crate) fn leaving(&self) -> bool {
        !self.present
    }

    pub(crate) fn doc(&self) -> Option<&Doc> {
        self.docs.get(self.active)
    }

    pub(crate) fn doc_mut(&mut self) -> Option<&mut Doc> {
        self.docs.get_mut(self.active)
    }

    pub(crate) fn picture(&self) -> Option<EntityId> {
        self.doc().map(|doc| doc.picture)
    }

    pub(crate) fn layer(&self) -> Option<EntityId> {
        self.doc().and_then(|doc| doc.layer)
    }

    /// The color brushes paint with.
    pub(crate) fn drawing_color(&self) -> [u8; 3] {
        if self.sub_active { self.sub } else { self.main }
    }

    /// Sets the color brushes paint with.
    pub(crate) fn set_drawing_color(&mut self, rgb: [u8; 3]) {
        if self.sub_active {
            self.sub = rgb;
        } else {
            self.main = rgb;
        }
        let hsv = color::Hsv::from_rgb(color::unit(rgb));
        if hsv.s > 0.0 && hsv.v > 0.0 {
            self.hue = hsv.h;
        }
        self.hex = color::hex(rgb);
    }

    /// The current layer's panel facts.
    pub(crate) fn layer_info(&self, arena: &Arena) -> Option<doc::LayerInfo> {
        let (picture, layer) = (self.picture()?, self.layer()?);
        doc::layers(arena, picture)
            .into_iter()
            .find(|info| info.id == layer)
    }

    /// The current layer if brushes may paint on it, or why not.
    /// The current layer if raster tools can work on it: not a vector
    /// layer, which only pens, brushes and erasers draw on.
    pub(crate) fn paintable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        self.paintable_on(arena, false)
    }

    /// As [`Self::paintable`], for a tool that lays its pixels in the mask
    /// while Edit Mask is on (Fill, Gradient, Edit Fill and Delete, the
    /// Blend tool): a layer of any kind whose mask is edited is taken, a
    /// folder's too (CT-1, CT-2).
    pub(crate) fn mask_paintable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        self.paintable_on(arena, true)
    }

    fn paintable_on(&self, arena: &Arena, masks: bool) -> Result<doc::LayerInfo, String> {
        let info = self.drawable_on(arena, masks)?;
        let on_mask = masks && self.editing_mask_of(arena, info.id);
        if !on_mask && crate::vector::is_vector(arena, info.id) {
            return Err(tr!("vector-layer-draw-pens-brushes-pick", name = info.name));
        }
        Ok(info)
    }

    /// The current layer while its mask is edited (Edit Mask on, and the
    /// layer has one), whatever kind of layer it is: a gradient, fill or
    /// tone layer's mask is gray pixels and takes brushes, fills and the
    /// Blend tool as a raster layer's does. `None` when no mask is
    /// edited; else the layer, or why not (locked or hidden, as for any
    /// layer).
    pub(crate) fn mask_layer(&self, arena: &Arena) -> Option<Result<doc::LayerInfo, String>> {
        self.doc().filter(|doc| doc.mask_edit)?;
        let info = self.layer_info(arena)?;
        doc::mask_of(arena, info.id)?;
        Some(if info.locked {
            Err(tr!("is-locked", name = info.name))
        } else if !info.style.visible {
            Err(tr!("is-hidden", name = info.name))
        } else {
            Ok(info)
        })
    }

    /// The current layer if pens and brushes can draw on it.
    pub(crate) fn drawable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        self.drawable_on(arena, false)
    }

    /// As [`Self::drawable`], for pens, brushes and erasers, which draw in
    /// the mask while Edit Mask is on: a layer of any kind whose mask is
    /// edited is taken, a folder's too.
    pub(crate) fn mask_drawable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        self.drawable_on(arena, true)
    }

    fn drawable_on(&self, arena: &Arena, masks: bool) -> Result<doc::LayerInfo, String> {
        if let Some(info) = self.layer_info(arena)
            && crate::three::is_3d(arena, info.id)
        {
            return Err(tr!("n-3d-layer-draw-layer-above-use", name = info.name));
        }
        let info = self.layer_info(arena).ok_or(tr!("pick-layer-draw-first"))?;
        // A File Object layer shows its file: it is rasterized first.
        if !(masks && self.editing_mask_of(arena, info.id)) && Self::is_file_object(arena, info.id)
        {
            return Err(tr!("file-object-rasterize-to-draw", name = info.name));
        }
        // While its mask is edited, a folder or any other kind of layer
        // with a mask takes the strokes of a tool that knows masks there,
        // in the mask's gray pixels. Other tools (figures, filters, warps)
        // work on the layer's own pixels, so they still refuse a folder.
        let on_mask = masks && self.editing_mask_of(arena, info.id);
        match info.kind {
            LayerKind::Raster => {}
            LayerKind::Paper => return Err(tr!("paper-layer-cannot-drawn")),
            LayerKind::Other if on_mask => {}
            LayerKind::Other if doc::is_folder(arena, info.id) => {
                return Err(tr!("folder-pick-layer-draw", name = info.name));
            }
            LayerKind::Other => return Err(tr!("cannot-be-drawn-on-yet", name = info.name)),
        }
        if info.locked {
            return Err(tr!("is-locked", name = info.name));
        }
        if !info.style.visible {
            return Err(tr!("is-hidden", name = info.name));
        }
        if !on_mask
            && doc::raster(arena, info.id)
                .is_none_or(|raster| raster.format() != PixelFormat::Rgba8)
        {
            return Err(tr!("is-not-an-rgba-layer", name = info.name));
        }
        Ok(info)
    }

    /// Whether Edit Mask is on and `layer` has a mask to edit.
    fn editing_mask_of(&self, arena: &Arena, layer: EntityId) -> bool {
        self.doc().is_some_and(|doc| doc.mask_edit) && doc::mask_of(arena, layer).is_some()
    }

    // ----- Keeping up with the arena ---------------------------------------

    // ----- Applying edits ---------------------------------------------------

    /// Applies an arena edit through the global history.
    pub(crate) fn apply(
        cx: &mut AppletContext<'_>,
        edit: impl ReversibleTransaction<Arena> + 'static,
    ) -> bool {
        match cx.apply(ArenaEdit(edit)) {
            Ok(_) => true,
            Err(error) => {
                cx.fail(tr!("sue-paint", error = error));
                false
            }
        }
    }

    /// Rewrites the current layer's pixels as one undo step.
    pub(crate) fn rewrite(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        change: impl FnOnce(&mut [u8], u32, u32),
    ) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        match doc::rewrite(&cx.world.arena, info.id, label, change) {
            Ok(Some(edit)) => {
                Self::apply(cx, edit);
            }
            Ok(None) => cx.notify(tr!("nothing-changed", label = label)),
            Err(error) => cx.fail(error),
        }
    }

    /// Changes the view of the current tab.
    pub(crate) fn with_view(
        &mut self,
        cx: &AppletContext<'_>,
        change: impl FnOnce(&mut View, Rect, [u32; 2]),
    ) {
        let viewport = self.viewport;
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let Some(size) = cx
            .world
            .arena
            .get_by_id(doc.picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        change(&mut doc.view, viewport, size);
    }

    // ----- The frame ----------------------------------------------------------
}

/// The settings section Sue Paint's shortcuts live in.
pub(crate) const KEYS_SECTION: &str = "sue_paint_keys";

pub(crate) use crate::commands::shortcut_table;
pub use exports::{export_at_once, hold_export_starts, hold_exports_to};
pub(crate) use files::export_dir;

/// Sue Paint's applet id.
pub(crate) fn me_id() -> AppletId {
    AppletId::new(ID).expect("a valid id")
}

/// Mirrors RGBA pixels left to right and/or top to bottom.
pub fn mirror(pixels: &mut [u8], width: u32, height: u32, flip_h: bool, flip_v: bool) {
    let source = pixels.to_vec();
    for y in 0..height {
        for x in 0..width {
            let sx = if flip_h { width - 1 - x } else { x };
            let sy = if flip_v { height - 1 - y } else { y };
            let from = ((sy * width + sx) * 4) as usize;
            let to = ((y * width + x) * 4) as usize;
            pixels[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
}

/// An 8-bit RGBA copy of any raster (gray and float pictures convert).
pub(crate) fn to_rgba(raster: &RasterTile2D) -> RasterTile2D {
    let (width, height) = (raster.width(), raster.height());
    let pixels = match (raster.format(), raster.bytes(), raster.f32_samples()) {
        (PixelFormat::Rgba8, Some(bytes), _) => bytes.to_vec(),
        (PixelFormat::Gray8, Some(bytes), _) => bytes
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        (_, _, Some(samples)) => samples
            .iter()
            .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
        _ => vec![0; width as usize * height as usize * 4],
    };
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .unwrap_or_else(|_| RasterTile2D::new(1, 1, PixelFormat::Rgba8).expect("a 1 by 1 raster"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirroring_flips_rows_and_columns() {
        let mut pixels: Vec<u8> = (0..2 * 2 * 4).map(|value| value as u8).collect();
        mirror(&mut pixels, 2, 2, true, false);
        assert_eq!(&pixels[..4], &[4, 5, 6, 7]);
        mirror(&mut pixels, 2, 2, true, true);
        assert_eq!(&pixels[..4], &[8, 9, 10, 11]);
    }
}
