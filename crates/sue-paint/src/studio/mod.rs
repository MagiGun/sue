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

use crate::brush::{Stabilizer, Stroke};
use crate::color::{self, History};
use crate::composite::Compositor;
use crate::dialogs::Dialog;
use crate::doc::{self};
use crate::select::{Op, Selection};
use crate::tools::Toolbox;
use crate::view::View;

mod applet;
mod clipboard;
mod edit;
mod files;
mod frame;
mod layers;
mod materials;
mod media;
mod rulers;
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
    /// A brush stroke (`shifted` is the selection moved into the layer's
    /// pixels when the layer is offset; `trail` the recent raw samples with
    /// their times, for the pen's speed as it lifts).
    Stroke {
        /// Boxed: a tip with its dynamics curves is large.
        stroke: Box<Stroke>,
        stabilizer: Stabilizer,
        offset: [i64; 2],
        raw: [f32; 2],
        label: String,
        shifted: Option<Selection>,
        trail: Vec<(f64, [f32; 2])>,
        correction: u32,
        flick: bool,
        /// Shift and drag: one straight line, drawn on release.
        line: bool,
        /// The ruler the stroke follows.
        snap: Option<crate::rulers::Snap>,
        /// On a vector layer: the stroke is kept as a line too.
        vector: bool,
        /// Smart Shape: where and since when the pen has rested, and
        /// whether the stroke already became its shape.
        rest: Option<(f64, Pos2)>,
        shaped: bool,
        /// The shape the stroke became, while the pen is still down: a drag
        /// then moves a line's far end, or turns and sizes a shape around
        /// its middle, and lifting the pen keeps what the drag left.
        shaping: Option<Shaping>,
    },
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
    /// pixels), the camera then, and whether it moves, zooms or turns.
    Camera {
        folder: EntityId,
        start: [f32; 2],
        base: crate::camera::Camera,
        mode: u8,
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
        twist: bool,
    },
    /// Dragging a ruler's handle: the ruler's text from before.
    RulerHandle {
        handle: crate::rulers::Handle,
        before: Option<String>,
    },
    /// Dragging puppet warp pin `index`.
    Pin { index: usize },
    /// A Blend tool drag on `layer`: its pixels from before, where the
    /// last dab was (layer pixels), and how far the pen went since.
    Blend {
        layer: EntityId,
        before: Vec<u8>,
        last: [f32; 2],
        since: f32,
        offset: [i64; 2],
        label: String,
    },
    /// Dragging a transform handle: what it grabbed, where (layer pixels),
    /// and the box as it was.
    Transform {
        handle: crate::transform_ui::Handle,
        start: [f32; 2],
        quad: crate::transform::Quad,
        center: [f32; 2],
        scale: [f32; 2],
        angle: f32,
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
    /// The Light table tool moving (or, with Shift, turning) the light
    /// table's layers: each one's place as the drag began.
    LightTable {
        layers: Vec<(EntityId, [f32; 3])>,
        start: Pos2,
        turn: bool,
    },
    /// Moving a layer.
    Move {
        layer: EntityId,
        start: Pos2,
        origin: [i64; 2],
        old: [Option<Value>; 2],
    },
    /// Picking colors while the button is down.
    Pick { mode: u8 },
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
    /// Command bar buttons people hid (Command Bar Settings).
    pub(crate) bar_hidden: HashSet<String>,
    /// Commands people added to the command bar.
    pub(crate) bar_extra: Vec<String>,
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
    pub(crate) timeline_view: crate::anim_ui::TimelineView,
    /// A 3D layer's view being changed in Tool Property, and its view and
    /// pixels from before.
    pub(crate) view3d_before: Option<(EntityId, crate::three::View3D, Vec<u8>)>,
    /// A 3D layer view Tool Property asked for this frame, and whether its
    /// slider was let go.
    pub(crate) pending_view3d: Option<(EntityId, crate::three::View3D)>,
    pub(crate) view3d_release: bool,
    /// The drawing figure's joint picked to pose.
    pub(crate) figure_joint: Option<usize>,
    /// A figure's pose being changed in Tool Property, its pose and pixels
    /// from before; the pose asked for this frame; and whether it is let go.
    pub(crate) pose_before: Option<(EntityId, crate::figure::Pose, Vec<u8>)>,
    pub(crate) pending_pose: Option<(EntityId, crate::figure::Pose)>,
    pub(crate) pose_release: bool,
    /// A figure's body shape being dragged in Tool Property, and the one let
    /// go of this frame, to keep.
    pub(crate) body_drag: Option<(EntityId, crate::figure::Body)>,
    pub(crate) pending_body: Option<(EntityId, crate::figure::Body)>,
    /// A camera being set up and not keyed yet: its folder, the frame and
    /// where it stands.
    pub(crate) camera_pending: Option<(EntityId, u32, crate::camera::Camera)>,
    /// The Material palette: the kind it shows (0 all, then by the kinds
    /// registered) and the material picked, by name.
    pub(crate) material_kind: u8,
    pub(crate) material_pick: Option<String>,
    /// Material thumbnails, by a hash of the material.
    pub(crate) material_thumbs: HashMap<u64, Vec<u8>>,
    /// Photos being read by the Pose and Hand Scanners.
    pub(crate) scans: crate::scan::Scans,
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
    pub(crate) pressure: Option<f32>,
    /// The pen's lean (as `sue_platform::stylus::tilt` has it) read with
    /// this frame's pointer event, until the tip lifts.
    pub(crate) sample_lean: Option<[f32; 2]>,
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
            bar_hidden: HashSet::new(),
            bar_extra: Vec::new(),
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
            timeline_view: crate::anim_ui::TimelineView::default(),
            camera_pending: None,
            view3d_before: None,
            pending_view3d: None,
            figure_joint: None,
            pose_before: None,
            pending_pose: None,
            pose_release: false,
            body_drag: None,
            pending_body: None,
            view3d_release: false,
            material_kind: 0,
            material_pick: None,
            material_thumbs: HashMap::new(),
            scans: crate::scan::Scans::default(),
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
            stamp: None,
            cel_lock: None,
            gathered: std::collections::HashSet::new(),
            layer_drag: None,
            looping: true,
            pressure: None,
            sample_lean: None,
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
        }
    }
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
    /// Where the layer's pixels are, for the mesh drawn over them.
    pub bounds: Option<[u32; 4]>,
    /// The layer's offset in the picture.
    pub offset: [i64; 2],
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
        let info = self.drawable(arena)?;
        if crate::vector::is_vector(arena, info.id) {
            return Err(tr!("vector-layer-draw-pens-brushes-pick", name = info.name));
        }
        Ok(info)
    }

    /// The current layer if pens and brushes can draw on it.
    pub(crate) fn drawable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        if let Some(info) = self.layer_info(arena)
            && crate::three::is_3d(arena, info.id)
        {
            return Err(tr!("n-3d-layer-draw-layer-above-use", name = info.name));
        }
        let info = self.layer_info(arena).ok_or(tr!("pick-layer-draw-first"))?;
        match info.kind {
            LayerKind::Raster => {}
            LayerKind::Paper => return Err(tr!("paper-layer-cannot-drawn")),
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
        if doc::raster(arena, info.id).is_none_or(|raster| raster.format() != PixelFormat::Rgba8) {
            return Err(tr!("is-not-an-rgba-layer", name = info.name));
        }
        Ok(info)
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
