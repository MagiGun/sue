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

use egui::{Key, Modifiers, Pos2, Rect, Ui, pos2, vec2};
use sue_core::txn::{Batch, SetProperty, SpawnEntity};
use sue_core::{
    Arena, CHILD, ComponentKind, Entity, EntityId, PixelFormat, Properties, RasterTile2D, Relation,
    ReversibleTransaction, UndoStack, Value,
};

use sue_model::class;
use sue_model::picture::{self, LayerKind};
use sue_platform::write_atomic;
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaEdit, FilePick, FileType, ItemState,
    Menu, MenuBar, PaletteEntry, ShellRequest, SueApplet, World,
};

use crate::brush::{Stabilizer, Stroke};
use crate::color::{self, History};
use crate::composite::{self, Compositor, RenderOptions};
use crate::dialogs::{Dialog, FilterDialog, NewDialog, PressureDialog};
use crate::doc::{self, NewPicture};
use crate::filters::Filter;
use crate::select::{Op, Selection};
use crate::tools::Toolbox;
use crate::view::View;

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
    "clip", "png", "jpg", "jpeg", "jxl", "psd", "psb", "webp", "tif", "tiff", "bmp",
];

/// `true` for a picture file Sue Paint opens as a canvas.
pub(crate) fn opens(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| OPENS.contains(&extension.to_ascii_lowercase().as_str()))
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
    fn new(picture: EntityId) -> Self {
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
    /// The corners of a polyline selection being clicked out.
    pub(crate) polyline: Vec<[f32; 2]>,
    /// The Selection Launcher shows under the selection.
    pub(crate) launcher: bool,
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
    /// How wide a frame is on the timeline, once zoomed; `None` fits
    /// the timeline to the palette.
    pub(crate) timeline_step: Option<f32>,
    /// A 3D layer's view being changed in Tool Property, and its view and
    /// pixels from before.
    pub(crate) view3d_before: Option<(EntityId, crate::three::View3D, Vec<u8>)>,
    /// A 3D layer view Tool Property asked for this frame, and whether its
    /// slider was let go.
    pub(crate) pending_view3d: Option<(EntityId, crate::three::View3D)>,
    pub(crate) view3d_release: bool,
    /// A camera being set up and not keyed yet: its folder, the frame and
    /// where it stands.
    pub(crate) camera_pending: Option<(EntityId, u32, crate::camera::Camera)>,
    /// The Material palette: the kind it shows (0 all, then by the kinds
    /// registered) and the material picked, by name.
    pub(crate) material_kind: u8,
    pub(crate) material_pick: Option<String>,
    /// Material thumbnails, by a hash of the material.
    pub(crate) material_thumbs: HashMap<u64, Vec<u8>>,
    /// CLIP STUDIO ASSETS on the start page's Home, fetched and cached.
    pub(crate) assets: crate::assets::Assets,
    /// The kinds of material, registered at start.
    pub(crate) material_kinds: crate::materials::MaterialKinds,
    /// The start page, shown while no canvas is open.
    pub(crate) start: crate::start::StartPage,
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
    /// The cel kept as the one being drawn whatever the frame (Lock
    /// current animation cel as editing target).
    pub(crate) cel_lock: Option<sue_core::EntityId>,
    /// Playback starts over at the end.
    pub(crate) looping: bool,
    pub(crate) pressure: Option<f32>,
    /// The pen pressure curve every tool uses (Pen Pressure Settings).
    pub(crate) pen_curve: sue_model::Response,
    /// The hint about fitting pen pressure was given (once a run).
    pub(crate) pen_hinted: bool,
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
            auto: crate::extras::AutoActions::default(),
            item_bank: Vec::new(),
            workspaces: Vec::new(),
            tiled: false,
            print: crate::print::PrintSettings::default(),
            bar_hidden: HashSet::new(),
            bar_extra: Vec::new(),
            ruler_bar: false,
            scroll_bars: true,
            interface: "auto".to_owned(),
            snap_grid: false,
            crop_frame: true,
            bleed: true,
            page_image: false,
            tone_area: false,
            proof_on: false,
            proof_profile: 0,
            proof_cache: None,
            tone_cache: None,
            sheet: None,
            laid_for: None,
            touches: Vec::new(),
            touch_gesture: None,
            docs: Vec::new(),
            active: 0,
            closed: HashSet::new(),
            menus: crate::menus::menus(),
            language: sue_lang::Watch::default(),
            bound: sue_wm::shortcuts::Bound::default(),
            menubar: MenuBar::new(),
            dialog: None,
            gesture: None,
            warp: None,
            vanishing_line: None,
            picked_lines: None,
            curve_points: Vec::new(),
            polyline: Vec::new(),
            launcher: true,
            webtoon: (1280, 800),
            timeline_on: true,
            audio_mix: None,
            audio_lap: None,
            audio_drag: None,
            timeline_step: None,
            camera_pending: None,
            view3d_before: None,
            pending_view3d: None,
            view3d_release: false,
            material_kind: 0,
            material_pick: None,
            material_thumbs: HashMap::new(),
            assets: crate::assets::Assets::default(),
            material_kinds: crate::materials::MaterialKinds::builtin(),
            start: crate::start::StartPage::default(),
            canvas_templates: crate::templates::CanvasTemplates::builtin(),
            drawing_tool: crate::tools::Tool::Pen,
            bare: false,
            touch: crate::studio_mode::Touch::default(),
            timelapse: HashMap::new(),
            timelapse_export: crate::timelapse::Export::default(),
            transform: None,
            modifiers: egui::Modifiers::NONE,
            onion: false,
            light_table: true,
            light_opacity: 0.5,
            light_color: false,
            light_cel: true,
            cel_lock: None,
            looping: true,
            pressure: None,
            pen_curve: crate::dialogs::DEFAULT_PEN_PRESSURE
                .parse()
                .unwrap_or_default(),
            pen_hinted: false,
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

/// The commands this build can run.
fn implemented(command: &str) -> bool {
    matches!(
        command,
        "file.new"
            | "file.open"
            | "file.start_page"
            | "file.duplicate.clip"
            | "file.export.webp"
            | "edit.outline_selection"
            | "edit.transform.scale_only"
            | "edit.transform.rotate"
            | "edit.transform.skew"
            | "edit.clear_history"
            | "layer.ungroup"
            | "layer.delete_empty"
            | "layer.delete_hidden"
            | "layer.hide_drafts"
            | "layer.rename"
            | "layer.transfer_down"
            | "layer.merge_visible_here"
            | "view.reset_turn"
            | "view.rotate_90"
            | "view.rotate_180"
            | "view.rotate_270"
            | "view.reset_display"
            | "view.paper"
            | "file.close"
            | "file.save"
            | "file.save_as"
            | "file.export.png"
            | "file.export.jpg"
            | "file.export.jxl"
            | "file.import.image"
            | "file.preferences"
            | "file.leave"
            | "app.quit"
            | "edit.undo"
            | "edit.redo"
            | "edit.cut"
            | "edit.copy"
            | "edit.paste"
            | "edit.clear"
            | "edit.clear_outside"
            | "edit.fill"
            | "edit.recolor"
            | "edit.luma_to_alpha"
            | "edit.align.left"
            | "edit.align.hcenter"
            | "edit.align.right"
            | "edit.align.top"
            | "edit.align.vcenter"
            | "edit.align.bottom"
            | "view.scroll_bar"
            | "layer.new.raster_top"
            | "file.new_clipboard"
            | "edit.canvas_height"
            | "window.brush_grid"
            | "edit.flip_h"
            | "edit.flip_v"
            | "edit.rotate180"
            | "canvas.flip_h"
            | "canvas.flip_v"
            | "canvas.rotate180"
            | "filter.brightness"
            | "filter.hue"
            | "filter.invert"
            | "filter.posterize"
            | "filter.binarize"
            | "filter.blur"
            | "filter.blur_strong"
            | "filter.gaussian"
            | "filter.mosaic"
            | "filter.noise"
            | "filter.sharpen"
            | "filter.sharpen_strong"
            | "layer.new"
            | "layer.new.fill"
            | "layer.new.paper"
            | "layer.from_selection"
            | "layer.duplicate"
            | "layer.delete"
            | "layer.toggle.visible"
            | "layer.toggle.clip"
            | "layer.toggle.draft"
            | "layer.toggle.lock"
            | "layer.toggle.lock_alpha"
            | "layer.properties"
            | "layer.select_above"
            | "layer.select_below"
            | "layer.order.front"
            | "layer.order.up"
            | "layer.order.down"
            | "layer.order.back"
            | "layer.merge_down"
            | "layer.merge_visible"
            | "layer.flatten"
            | "select.all"
            | "select.none"
            | "select.again"
            | "select.invert"
            | "select.from_layer"
            | "view.rotate_left"
            | "view.rotate_right"
            | "view.rotate_reset"
            | "view.flip_h"
            | "view.flip_v"
            | "view.zoom_in"
            | "view.zoom_out"
            | "view.actual"
            | "view.double"
            | "view.fit"
            | "view.grid"
            | "view.pixel_grid"
            | "view.selection"
            | "window.next_tab"
            | "window.prev_tab"
            | "window.hide_all"
            | "window.command_bar"
            | "window.tool"
            | "window.sub_tool"
            | "window.tool_property"
            | "window.brush_size"
            | "window.color_wheel"
            | "window.color_slider"
            | "window.color_set"
            | "window.color_history"
            | "window.navigator"
            | "window.layer"
            | "window.layer_property"
            | "window.history"
            | "window.timeline"
            | "help.shortcuts"
            | "window.hide_menus"
            | "window.workspace.reset"
            | "help.about"
            | "file.shortcuts"
            | "file.pressure"
            | "edit.transform.puppet"
            | "edit.transform.scale"
            | "edit.transform.free"
            | "edit.transform.distort"
            | "edit.transform.perspective"
            | "story.manager"
            | "story.order"
            | "story.open_page"
            | "story.work_settings"
            | "story.first"
            | "story.previous"
            | "story.next"
            | "story.last"
            | "story.add"
            | "story.insert"
            | "story.duplicate_page"
            | "story.delete"
            | "story.export_page"
            | "story.export_pages"
            | "filter.curve"
            | "filter.levels"
            | "filter.balance"
            | "filter.gradient_map"
            | "filter.focus_lines"
            | "filter.speed_lines"
            | "filter.tone"
            | "layer.new.tone"
            | "layer.new.frame"
            | "layer.new.balloon"
            | "layer.folder_insert"
            | "edit.canvas_size"
            | "edit.resolution"
            | "edit.crop"
            | "view.snap_special"
            | "view.snap_ruler"
            | "layer.new.vector"
            | "file.timelapse.record"
            | "file.timelapse.export"
            | "file.export.movie"
            | "layer.ruler.perspective"
            | "layer.ruler.symmetry"
            | "layer.ruler.circle"
            | "layer.ruler.delete"
            | "layer.ruler.show"
            | "layer.new.text"
            | "layer.correction.brightness"
            | "layer.correction.levels"
            | "layer.correction.curve"
            | "layer.correction.hue"
            | "layer.correction.balance"
            | "layer.correction.invert"
            | "layer.correction.posterize"
            | "layer.correction.binarize"
            | "layer.correction.gradient_map"
            | "layer.mask.outside"
            | "layer.mask.inside"
            | "layer.mask.delete"
            | "layer.mask.apply"
            | "layer.mask.toggle"
            | "layer.mask.edit"
            | "layer.new.folder"
            | "anim.timeline.new"
            | "anim.layer.folder"
            | "anim.cel.new"
            | "anim.cel.assign"
            | "anim.key.add"
            | "anim.key.delete"
            | "anim.frame.insert"
            | "anim.frame.delete"
            | "anim.onion"
            | "window.intermediate"
            | "window.approximate"
            | "edit.material.image"
            | "select.expand"
            | "select.shrink"
            | "select.feather"
            | "select.gamut"
            | "select.add_layer"
            | "select.subtract_layer"
            | "select.to_layer"
            | "select.quick_mask"
            | "select.launcher"
            | "edit.material.paste"
            | "file.export.webtoon"
            | "file.export.cels"
            | "anim.first"
            | "anim.last"
            | "anim.timeline.edit"
            | "anim.timeline.enable"
            | "file.import.audio"
            | "layer.new.3d"
            | "file.import.3d"
            | "filter.line_extraction"
            | "filter.motion_blur"
            | "filter.radial_blur"
            | "filter.zoom_blur"
            | "filter.smooth"
            | "filter.dust"
            | "filter.line_width"
            | "filter.wave"
            | "filter.pinch"
            | "filter.twirl"
            | "filter.zigzag"
            | "filter.crystallize"
            | "filter.lens_blur"
            | "filter.jpeg_noise"
            | "filter.polar"
            | "filter.ripple"
            | "filter.fisheye"
            | "filter.aberration"
            | "filter.retro"
            | "filter.perlin"
            | "filter.clouds"
            | "filter.unsharp"
            | "filter.lines_tones"
            | "file.export.pdf"
            | "file.export.epub"
            | "layer.toggle.reference"
            | "layer.rasterize"
            | "window.material"
            | "anim.layer.camera"
            | "anim.camera.key"
            | "anim.camera.unkey"
            | "anim.onion_settings"
            | "anim.light_table"
            | "anim.light_table.register"
            | "anim.light_table.clear"
            | "anim.light_table.reverse"
            | "anim.light_table.color"
            | "anim.light_table.cel"
            | "anim.light_table.deregister"
            | "anim.cel.lock"
            | "anim.cel.prev"
            | "anim.cel.next"
            | "anim.loop"
            | "anim.play"
            | "anim.next"
            | "anim.prev"
            | "file.export.gif"
            | "file.export.sequence"
            | "file.export.bmp"
            | "file.export.tif"
            | "file.export.tga"
            | "file.export.psd"
            | "file.export.psb"
            | "file.import.layers"
            | "file.import.vector"
            | "file.import.movie"
            | "file.import.pattern"
            | "file.recent.clear"
            | "file.revert"
            | "view.fit_navigator"
            | "view.print_size"
            | "view.ruler_bar"
            | "view.snap_grid"
            | "view.crop_frame"
            | "view.bleed"
            | "view.page_image"
            | "view.tone_area"
            | "view.profile_preview"
            | "view.profile_settings"
            | "story.combine"
            | "story.separate"
            | "story.page_settings"
            | "story.info"
            | "story.editor"
            | "story.import_text"
            | "edit.transform.mesh"
            | "window.interface.auto"
            | "window.interface.desktop"
            | "window.interface.tablet"
            | "window.interface.phone"
            | "canvas.rotate_left"
            | "canvas.rotate_right"
            | "edit.paste_here"
            | "edit.canvas_properties"
            | "edit.brush_tip"
            | "edit.material.tip"
            | "layer.new.gradient"
            | "layer.merge_selected"
            | "layer.convert"
            | "select.from_selection_layer"
            | "file.print_settings"
            | "file.print"
            | "file.export.batch"
            | "file.modifiers"
            | "file.command_bar"
            | "help.guide"
            | "help.updates"
            | "window.new"
            | "window.tile"
            | "window.cascade"
            | "window.workspace.save"
            | "window.workspace.manage"
            | "window.sub_view"
            | "window.cels"
            | "window.auto_actions"
            | "window.item_bank"
            | "window.info"
    ) || command.starts_with("file.recent.open.")
        || command.starts_with("window.workspace.use.")
        || TOOL_KEYS.iter().any(|(known, _, _)| *known == command)
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
    /// Edit, Align/Distribute: moves the current layer so its drawn
    /// pixels line up with the canvas's `edge` (left, hcenter, right, top,
    /// vcenter or bottom), one undo step.
    pub(crate) fn align_layer(&mut self, edge: &str, cx: &mut AppletContext<'_>) {
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        let Some(size) = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(picture::size)
        else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let (width, height) = (raster.width() as usize, raster.height() as usize);
        let mut bounds: Option<[usize; 4]> = None;
        for y in 0..height {
            for x in 0..width {
                if bytes[(y * width + x) * 4 + 3] > 0 {
                    let b = bounds.get_or_insert([x, y, x, y]);
                    b[0] = b[0].min(x);
                    b[1] = b[1].min(y);
                    b[2] = b[2].max(x);
                    b[3] = b[3].max(y);
                }
            }
        }
        let Some([left, top, right, bottom]) = bounds else {
            cx.notify(tr!("nothing-drawn-to-align"));
            return;
        };
        let [ox, oy] = info.style.offset;
        let (left, top) = (left as i64 + ox, top as i64 + oy);
        let (right, bottom) = (right as i64 + 1 + ox, bottom as i64 + 1 + oy);
        let (w, h) = (i64::from(size[0]), i64::from(size[1]));
        let (key, by) = match edge {
            "left" => (picture::X, -left),
            "hcenter" => (picture::X, (w - (right - left)) / 2 - left),
            "right" => (picture::X, w - right),
            "top" => (picture::Y, -top),
            "vcenter" => (picture::Y, (h - (bottom - top)) / 2 - top),
            _ => (picture::Y, h - bottom),
        };
        if by != 0 {
            let now = if key == picture::X { ox } else { oy };
            Self::apply(cx, doc::set(info.id, key, now + by));
        }
    }

    // ----- Puppet warp ------------------------------------------------------

    /// Starts puppet warp on the current layer.
    pub(crate) fn start_warp(&mut self, cx: &mut AppletContext<'_>) {
        if self.warp.is_some() {
            return;
        }
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        self.warp = Some(WarpSession {
            layer: info.id,
            size,
            bounds: crate::warp::bounds(&before, size[0], size[1]),
            before,
            pins: Vec::new(),
            shown: Vec::new(),
            offset: info.style.offset,
        });
        cx.notify(tr!("puppet-warp-click-put-pins-drag"));
    }

    /// Shows the layer bent by the pins as they are now.
    pub(crate) fn warp_preview(&mut self, cx: &mut AppletContext<'_>) {
        let Some(warp) = self.warp.as_mut() else {
            return;
        };
        if warp.pins == warp.shown {
            return;
        }
        let pixels = crate::warp::warp(&warp.before, warp.size[0], warp.size[1], &warp.pins, 8);
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, warp.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == pixels.len())
        {
            bytes.copy_from_slice(&pixels);
        }
        warp.shown = warp.pins.clone();
    }

    /// Ends puppet warp: keeps the bend as one undo step, or puts the
    /// layer back.
    pub(crate) fn finish_warp(&mut self, cx: &mut AppletContext<'_>, keep: bool) {
        let Some(warp) = self.warp.take() else {
            return;
        };
        self.gesture = None;
        let after = if keep {
            crate::warp::warp(&warp.before, warp.size[0], warp.size[1], &warp.pins, 2)
        } else {
            warp.before.clone()
        };
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, warp.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == after.len())
        {
            bytes.copy_from_slice(&after);
        }
        if keep
            && let Some(patch) = doc::diff_patch(&warp.before, &after, warp.size[0], warp.size[1])
        {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                warp.layer,
                tr!("puppet-warp-2"),
                vec![patch],
            )));
        }
    }

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

    /// Deletes the vector lines the Object tool picked, as one step.
    pub(crate) fn delete_picked_lines(&mut self, cx: &mut AppletContext<'_>) {
        let Some((layer, picked)) = self.picked_lines.take() else {
            return;
        };
        let Some(before) = doc::raster(&cx.world.arena, layer)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return;
        };
        let geometry = crate::vector::geometry(&cx.world.arena, layer);
        let lines: Vec<crate::vector::Line> = crate::vector::lines(&geometry)
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !picked.contains(index))
            .map(|(_, line)| line)
            .collect();
        let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) else {
            return;
        };
        let (width, height) = (raster.width(), raster.height());
        if let Some(painted) = crate::vector::render(&lines, width, height)
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let patch = doc::diff_patch(&before, raster.bytes()?, width, height)?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                tr!("delete-lines"),
                vec![patch],
            ))
        });
        let edit = crate::vector::keep(
            &mut cx.world.arena,
            layer,
            geometry,
            &lines,
            pixels,
            &tr!("delete-lines"),
        );
        cx.record(ArenaEdit(edit));
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Adds `path` to the picture's curve, figure and pen rulers, as one
    /// step named `label`, and turns Snap to Ruler on.
    pub(crate) fn add_ruler_path(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: crate::rulers::Path,
        label: &str,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let mut rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
        rulers.paths.push(path);
        let text = rulers.text(crate::rulers::PATHS);
        if Self::apply(
            cx,
            crate::rulers::edit(picture, crate::rulers::PATHS, text, label),
        ) && let Some(doc) = self.doc_mut()
        {
            doc.snap_ruler = true;
            doc.show_rulers = true;
        }
    }

    /// Lays the curve ruler through the points placed so far.
    pub(crate) fn finish_curve(&mut self, cx: &mut AppletContext<'_>) {
        let points = std::mem::take(&mut self.curve_points);
        if points.len() >= 2 {
            self.add_ruler_path(
                cx,
                crate::rulers::Path::curve(&points),
                &tr!("create-curve-ruler"),
            );
        }
    }

    /// `true` if `picture` records its timelapse.
    pub(crate) fn recording(&self, arena: &Arena, picture: EntityId) -> bool {
        arena
            .get_by_id(picture)
            .ok()
            .and_then(|entity| entity.property(crate::timelapse::RECORD))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Keeps a timelapse frame of the current canvas when it records and has
    /// changed since its last one, between strokes.
    fn record_timelapse(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        if self.gesture.is_some() {
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        if !self.recording(&cx.world.arena, picture) {
            return;
        }
        let Some(base) = crate::colorsets::folder(cx) else {
            return;
        };
        let dir = crate::timelapse::folder(&base, picture);
        let (seen, waiting, next) = match self.timelapse.get(&picture) {
            Some(known) => *known,
            None => (u64::MAX, u64::MAX, crate::timelapse::frames(&dir).len()),
        };
        let Some(doc) = self.doc() else {
            return;
        };
        if doc.playing.is_some() {
            return;
        }
        let (pixels, size, version) = doc.compositor.display();
        if size[0] == 0 || pixels.is_empty() || seen == version {
            return;
        }
        if waiting != version {
            // A change often paints twice; keep it once it settles.
            self.timelapse.insert(picture, (seen, version, next));
            ctx.request_repaint();
            return;
        }
        let kept = crate::timelapse::capture(&dir, next, pixels, size);
        match kept {
            Ok(()) => {
                self.timelapse.insert(picture, (version, version, next + 1));
            }
            Err(error) => {
                // Stop trying this picture until it changes again.
                self.timelapse.insert(picture, (version, version, next));
                cx.fail(tr!("cannot-record-the-timelapse", error = error));
            }
        }
    }

    /// Asks where to export the timelapse laid out as `export`.
    pub(crate) fn ask_export_timelapse(
        &mut self,
        cx: &mut AppletContext<'_>,
        export: crate::timelapse::Export,
    ) {
        self.timelapse_export = export;
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("timelapse"), |entity| entity.name().to_owned());
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "timelapse",
                &tr!("export-timelapse"),
                &tr!("timelapse-avi", name = name),
            )
            .types(vec![
                sue_wm::FileType::new(&tr!("motion-jpeg-avi-video"), &["avi"]),
                sue_wm::FileType::new(&tr!("animated-gif"), &["gif"]),
            ])
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the current canvas's timelapse to `path`, as a video or a
    /// GIF by its extension.
    fn export_timelapse(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let (Some(picture), Some(base)) = (self.picture(), crate::colorsets::folder(cx)) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let dir = crate::timelapse::folder(&base, picture);
        let gif = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
        let result = if gif {
            crate::timelapse::video_frames(&dir, self.timelapse_export).and_then(|(frames, fps)| {
                let count = frames.len();
                sue_io::encode_gif_animation(&frames, 1000 / fps.max(1)).map(|bytes| (bytes, count))
            })
        } else {
            crate::timelapse::export_avi(&dir, self.timelapse_export)
        };
        match result.and_then(|(bytes, count)| {
            write_atomic(path, &bytes)
                .map_err(|error| error.to_string())
                .map(|()| (bytes.len(), count))
        }) {
            Ok((size, count)) => cx.saved(
                path,
                tr!("frames-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!("cannot-export-the-timelapse", error = error)),
        }
    }

    /// The current layer when it is a camera folder.
    pub(crate) fn camera_folder(&self, arena: &Arena) -> Option<EntityId> {
        self.layer()
            .filter(|layer| crate::camera::is_camera(arena, *layer))
    }

    /// The frame shown and the time it is at, by the timeline's rate.
    pub(crate) fn frame_time(&self, arena: &Arena) -> (u32, sue_core::Time) {
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let rate = self
            .picture()
            .and_then(|picture| crate::anim::timeline(arena, picture))
            .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
        (frame, crate::camera::time_of(frame, rate))
    }

    /// Where `folder`'s camera stands at the frame shown, set up or keyed.
    pub(crate) fn camera_now(&self, arena: &Arena, folder: EntityId) -> crate::camera::Camera {
        let (frame, time) = self.frame_time(arena);
        if let Some((pending, at, camera)) = self.camera_pending
            && pending == folder
            && at == frame
        {
            return camera;
        }
        let size = self
            .picture()
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(picture::size)
            .unwrap_or([0, 0]);
        crate::camera::Camera::of(arena, folder, Some(time), size)
    }

    /// Keeps the current layer as a picture material named `name`.
    pub(crate) fn register_material(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let (Some(folder), Some(layer)) = (crate::materials::location(cx), self.layer()) else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        match crate::materials::register(&folder, name, raster) {
            Ok(_) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.panels.color_set = true;
                self.set_tab = 2;
                self.material_kind = self
                    .material_kinds
                    .iter()
                    .position(|kind| kind.id == crate::materials::IMAGE)
                    .map_or(0, |index| index as u8 + 1);
                self.start.materials_changed();
                self.material_pick = Some(name.to_owned());
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Puts `material` on the canvas as a new layer above the current one,
    /// in the drawing color.
    pub(crate) fn paste_material(
        &mut self,
        cx: &mut AppletContext<'_>,
        material: &crate::materials::Material,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        let Some(raster) = crate::materials::layer(material, size, self.drawing_color()) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        match doc::add_layer_pixels(
            &cx.world.arena,
            picture,
            self.layer(),
            material.name.clone(),
            raster,
        ) {
            Ok((spawn, id)) => {
                if Self::apply(
                    cx,
                    doc::Edit::new(tr!("paste-2", name = material.name)).then(spawn),
                ) && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Asks where to write the webtoon, cut every `height` pixels and
    /// scaled to `width`.
    pub(crate) fn ask_export_webtoon(
        &mut self,
        cx: &mut AppletContext<'_>,
        height: u32,
        width: u32,
    ) {
        self.webtoon = (height.max(1), width.max(1));
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("webtoon"), |entity| entity.name().to_owned());
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "webtoon",
                &tr!("export-webtoon"),
                &format!("{name}.png"),
            )
            .types(
                sue_io::PICTURE_SAVE_TYPES
                    .iter()
                    .map(|(what, extensions)| sue_wm::FileType::new(what, extensions))
                    .collect(),
            )
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the picture to numbered files next to `path`, scaled to the
    /// webtoon width and cut every webtoon height, top to bottom.
    fn export_webtoon(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(flat) = composite::flatten(
            &cx.world.arena,
            picture,
            RenderOptions {
                paper: true,
                drafts: false,
            },
        ) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let (cut, width) = self.webtoon;
        let from = [flat.width(), flat.height()];
        let to = [
            width,
            ((f64::from(from[1]) * f64::from(width) / f64::from(from[0].max(1))).round() as u32)
                .max(1),
        ];
        let Some(bytes) = flat.bytes() else {
            return;
        };
        let scaled = if to == from {
            bytes.to_vec()
        } else {
            doc::resize_rgba(bytes, from, to)
        };
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path.file_stem().map_or_else(
            || tr!("webtoon"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut written = 0;
        let mut top = 0;
        while top < to[1] {
            let rows = cut.min(to[1] - top);
            let start = (top * to[0] * 4) as usize;
            let piece = scaled[start..start + (rows * to[0] * 4) as usize].to_vec();
            let Ok(tile) = sue_core::RasterTile2D::from_bytes(
                to[0],
                rows,
                sue_core::PixelFormat::Rgba8,
                piece,
            ) else {
                break;
            };
            written += 1;
            let target = dir.join(format!("{stem}-{written:03}.{extension}"));
            if let Err(error) = sue_io::encode_as(&tile, &extension, 92, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                cx.fail(tr!("cannot-export-piece", written = written, error = error));
                return;
            }
            top += rows;
        }
        cx.notify(tr!(
            "exported-webtoon-as-pictures-px-wide",
            value = to[0],
            written = written
        ));
    }

    /// Writes each animation cel as a picture of its own, named for it, next
    /// to `path`.
    fn export_cels(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(sequence) = crate::anim::timeline(&cx.world.arena, picture) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let mut cels: Vec<EntityId> = crate::anim::folders(sequence)
            .into_iter()
            .flat_map(|index| crate::anim::keys(&sequence.tracks[index], sequence.rate))
            .filter_map(|(_, cel)| cel)
            .collect();
        cels.sort();
        cels.dedup();
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("cel"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut written = 0;
        for cel in cels {
            let (Some(raster), Ok(entity)) = (
                doc::raster(&cx.world.arena, cel),
                cx.world.arena.get_by_id(cel),
            ) else {
                continue;
            };
            let name: String = entity
                .name()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let target = dir.join(format!("{stem}-{name}.{extension}"));
            match sue_io::encode_as(raster, &extension, 92, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                Ok(()) => written += 1,
                Err(error) => {
                    cx.fail(tr!("cannot-export-2", name = name, error = error));
                    return;
                }
            }
        }
        cx.notify(tr!("exported-cels", written = written));
    }

    /// Adds a 3D layer showing `model` (a new model entity) above the
    /// current layer, as one step.
    pub(crate) fn add_3d_layer(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        model: sue_core::Entity,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        match crate::three::new_layer(&cx.world.arena, picture, self.layer(), name, model) {
            Ok((edit, id)) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
                self.toolbox.pick(crate::tools::Tool::Operation);
                self.toolbox.pick_sub(crate::tools::Tool::Operation, 0);
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Puts the sound file at `path` on a new audio track of the timeline,
    /// from the frame shown.
    fn import_audio(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(sequence) = crate::anim::timeline(&cx.world.arena, picture).cloned() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path
            .file_stem()
            .map_or_else(|| tr!("audio"), |stem| stem.to_string_lossy().into_owned());
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
        let decoded = sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                sue_io::decode_audio(bytes, extension.as_deref()).map_err(|(error, _)| error)
            });
        match decoded {
            Ok((audio, _)) => {
                let frame = self.doc().map_or(0, |doc| doc.frame);
                let (edit, _) = crate::anim::add_audio(picture, &sequence, &name, audio.pcm, frame);
                Self::apply(cx, edit);
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// Imports the model at `path` (OBJ) as a 3D layer.
    fn import_3d(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        use sue_core::FormatHandler as _;
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path
            .file_stem()
            .map_or_else(|| tr!("model"), |stem| stem.to_string_lossy().into_owned());
        let bundle = sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                sue_io::ObjFormat
                    .import(&bytes, &name, &sue_core::Properties::new())
                    .map_err(|error| error.to_string())
            });
        match bundle.map(|bundle| bundle.entities.into_iter().next()) {
            Ok(Some(model)) => self.add_3d_layer(cx, &name, model),
            Ok(None) => cx.fail(tr!("file-has-no-model")),
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// Shows 3D layer `layer` as `view` sees it, in place (while it is being
    /// changed; [`Self::finish_view3d`] records it).
    pub(crate) fn preview_view3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        view: &crate::three::View3D,
    ) {
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(drawn) = crate::three::buffers_of(&cx.world.arena, layer, view, size)
            .and_then(|buffers| crate::three::draw(&buffers, view))
        else {
            return;
        };
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            let properties = entity.properties_mut();
            for (key, value) in [
                (crate::three::YAW, f64::from(view.yaw)),
                (crate::three::PITCH, f64::from(view.pitch)),
                (crate::three::DISTANCE, f64::from(view.distance)),
                (crate::three::FOV, f64::from(view.fov)),
                (crate::three::LIGHT, f64::from(view.light)),
                (crate::three::LINE_WIDTH, f64::from(view.line_width)),
                (crate::three::CREASE, f64::from(view.crease)),
                (crate::three::PAN_X, f64::from(view.pan[0])),
                (crate::three::PAN_Y, f64::from(view.pan[1])),
            ] {
                properties.set(key, value);
            }
            properties.set(crate::three::STYLE, i64::from(view.style));
        }
        if let (Some(into), Some(from)) = (
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut),
            drawn.bytes(),
        ) {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Records a 3D layer's view change as one step: back to `base` and
    /// `before` (its pixels), then the change.
    pub(crate) fn finish_view3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        base: crate::three::View3D,
        before: &[u8],
    ) {
        let Some(now) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
        else {
            return;
        };
        if now == base {
            return;
        }
        self.preview_view3d(cx, layer, &base);
        if let Some(into) =
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
            && into.len() == before.len()
        {
            into.copy_from_slice(before);
        }
        if let Some(edit) =
            crate::three::set_view(&cx.world.arena, layer, &now, &tr!("move-camera"))
        {
            Self::apply(cx, edit);
        }
    }

    /// Convert to Lines and Tones: the current layer's lines on a new layer
    /// (Line Extraction) and its shading on another as dots (a gray layer
    /// with the Tone effect on), the layer itself hidden, as one step.
    pub(crate) fn lines_and_tones(&mut self, cx: &mut AppletContext<'_>) {
        if let Some(layer) = self
            .layer()
            .filter(|layer| crate::three::is_3d(&cx.world.arena, *layer))
        {
            self.lines_and_tones_3d(cx, layer);
            return;
        }
        let (Some(picture), Some(info)) = (self.picture(), self.paintable(&cx.world.arena).ok())
        else {
            cx.fail(tr!("pick-raster-layer-convert-lines-tones"));
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        if [raster.width(), raster.height()] != size || info.style.offset != [0, 0] {
            cx.fail(tr!("move-layer-back-into-place-merge"));
            return;
        }
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let mut lines = bytes.to_vec();
        Filter::LineExtraction {
            sensitivity: 0.5,
            black: 0.12,
            smooth: 1.0,
        }
        .apply(&mut lines, size[0], size[1], None, [0, 0]);
        // The shading: darker is more ink.
        let tones: Vec<u8> = bytes
            .chunks(4)
            .flat_map(|p| {
                let a = f32::from(p[3]) / 255.0;
                let l =
                    (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
                        / 255.0;
                let gray = (l * 255.0).round() as u8;
                [
                    gray,
                    gray,
                    gray,
                    ((1.0 - l) * a * 255.0).round().min(255.0) as u8,
                ]
            })
            .collect();
        let tile = |pixels: Vec<u8>| {
            sue_core::RasterTile2D::from_bytes(
                size[0],
                size[1],
                sue_core::PixelFormat::Rgba8,
                pixels,
            )
        };
        let (Ok(lines), Ok(tones)) = (tile(lines), tile(tones)) else {
            return;
        };
        let Ok((spawns, ids)) = doc::add_layers_pixels(
            &cx.world.arena,
            picture,
            Some(info.id),
            vec![(tr!("tones"), tones), (tr!("lines"), lines)],
        ) else {
            return;
        };
        let mut edit = doc::Edit::new(tr!("convert-to-lines-and-tones"));
        for spawn in spawns {
            edit = edit.then(spawn);
        }
        let edit = edit
            .then(SetProperty::new(ids[0], crate::effects::TONE, true))
            .then(SetProperty::new(info.id, picture::VISIBLE, false));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(ids[1]);
        }
    }

    /// Convert to Lines and Tones for a 3D layer: its outline and folds as
    /// lines, its shading as tone, each on a layer of its own, the 3D layer
    /// hidden, as one step (Clip Studio Paint's LT conversion of 3D).
    fn lines_and_tones_3d(&mut self, cx: &mut AppletContext<'_>, layer: EntityId) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(view) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
        else {
            return;
        };
        let Some(buffers) = crate::three::buffers_of(&cx.world.arena, layer, &view, size) else {
            cx.fail(tr!("n-3d-layer-has-no-model"));
            return;
        };
        let line = crate::three::lines(&buffers, view.line_width.max(1.0), view.crease);
        let lines: Vec<u8> = line
            .iter()
            .flat_map(|ink| [0, 0, 0, (ink * 255.0).round() as u8])
            .collect();
        let tones: Vec<u8> = (0..line.len())
            .flat_map(|at| {
                let shade = if buffers.covered(at) {
                    1.0 - buffers.light[at]
                } else {
                    0.0
                };
                [0, 0, 0, (shade.clamp(0.0, 1.0) * 255.0).round() as u8]
            })
            .collect();
        let tile = |pixels: Vec<u8>| {
            sue_core::RasterTile2D::from_bytes(
                size[0],
                size[1],
                sue_core::PixelFormat::Rgba8,
                pixels,
            )
        };
        let (Ok(lines), Ok(tones)) = (tile(lines), tile(tones)) else {
            return;
        };
        let Ok((spawns, ids)) = doc::add_layers_pixels(
            &cx.world.arena,
            picture,
            Some(layer),
            vec![(tr!("tones"), tones), (tr!("lines"), lines)],
        ) else {
            return;
        };
        let mut edit = doc::Edit::new(tr!("convert-to-lines-and-tones"));
        for spawn in spawns {
            edit = edit.then(spawn);
        }
        let edit = edit
            .then(SetProperty::new(ids[0], crate::effects::TONE, true))
            .then(SetProperty::new(layer, picture::VISIBLE, false));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(ids[1]);
        }
    }

    /// Select Color Gamut: the pixels near the drawing color, of the current
    /// layer or (with `all`) of every layer as shown.
    pub(crate) fn select_gamut(&mut self, cx: &mut AppletContext<'_>, tolerance: f32, all: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let color = self.drawing_color();
        let found = if all {
            composite::flatten(&cx.world.arena, picture, RenderOptions::default()).and_then(
                |tile| {
                    Some(Selection::of_color(
                        tile.bytes()?,
                        tile.width(),
                        tile.height(),
                        color,
                        tolerance,
                    ))
                },
            )
        } else {
            self.layer()
                .and_then(|layer| doc::raster(&cx.world.arena, layer))
                .and_then(|raster| {
                    let offset = self
                        .layer_info(&cx.world.arena)
                        .map_or([0, 0], |info| info.style.offset);
                    let found = Selection::of_color(
                        raster.bytes()?,
                        raster.width(),
                        raster.height(),
                        color,
                        tolerance,
                    );
                    Some(found.shifted([-offset[0], -offset[1]]))
                })
        };
        match found.filter(|selection| !selection.is_empty()) {
            Some(selection) => {
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(Some(selection));
                }
            }
            None => cx.notify(tr!("no-pixels-color-there")),
        }
    }

    /// Quick Mask: the selection becomes a red layer to paint on, and back.
    pub(crate) fn toggle_quick_mask(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(layer) = self.doc().and_then(|doc| doc.quick_mask) {
            let selection = doc::raster(&cx.world.arena, layer).map(Selection::from_alpha);
            if Self::apply(
                cx,
                doc::Edit::new(tr!("quick-mask")).then(doc::delete(layer)),
            ) && let Some(doc) = self.doc_mut()
            {
                doc.quick_mask = None;
                doc.set_selection(selection);
                doc.layer = doc::layers(&cx.world.arena, picture)
                    .last()
                    .map(|info| info.id);
            }
            return;
        }
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        let alpha: Vec<u8> = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map_or_else(
                || vec![0; (size[0] * size[1]) as usize],
                |selection| selection.mask().to_vec(),
            );
        let pixels: Vec<u8> = alpha
            .iter()
            .flat_map(|alpha| [230, 40, 40, *alpha])
            .collect();
        let Ok(raster) = sue_core::RasterTile2D::from_bytes(
            size[0],
            size[1],
            sue_core::PixelFormat::Rgba8,
            pixels,
        ) else {
            return;
        };
        let top = doc::layers(&cx.world.arena, picture)
            .last()
            .map(|info| info.id);
        match doc::add_layer_pixels(&cx.world.arena, picture, top, tr!("quick-mask"), raster) {
            Ok((spawn, id)) => {
                let edit = doc::Edit::new(tr!("quick-mask"))
                    .then(spawn)
                    .then(SetProperty::new(id, picture::DRAFT, true))
                    .then(SetProperty::new(id, picture::OPACITY, 0.5));
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.quick_mask = Some(id);
                    doc.layer = Some(id);
                    doc.set_selection(None);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Picks the Ruler tool's sub tool of `mode`.
    pub(crate) fn pick_ruler(&mut self, mode: u8) {
        use crate::tools::Tool;
        self.toolbox.pick(Tool::Ruler);
        if let Some(index) = self
            .toolbox
            .subs(Tool::Ruler)
            .iter()
            .position(|sub| sub.mode == mode)
        {
            self.toolbox.pick_sub(Tool::Ruler, index);
        }
    }

    /// Lays a perspective ruler of `count` vanishing points on the
    /// picture, as Create Perspective Ruler does.
    pub(crate) fn make_perspective(&mut self, cx: &mut AppletContext<'_>, count: u32) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        let ruler = crate::rulers::Perspective::new(count, size);
        let edit = crate::rulers::edit(
            picture,
            crate::rulers::PERSPECTIVE,
            Some(ruler.to_text()),
            &tr!("create-perspective-ruler"),
        );
        if Self::apply(cx, edit) {
            self.pick_ruler(1);
            if let Some(doc) = self.doc_mut() {
                doc.ruler_on = true;
                doc.show_rulers = true;
            }
        }
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

    /// Opens a tab for every picture in the arena that has none (the newest
    /// becomes current), drops tabs whose picture is gone, and keeps every
    /// tab's current layer valid.
    pub(crate) fn sync_docs(&mut self, arena: &Arena) {
        let pictures: Vec<EntityId> = arena
            .iter()
            .filter(|(_, entity)| picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .collect();
        let current = self.picture();
        let shown = self.docs.get(self.active).map(|doc| doc.picture);
        let before = self.docs.len();
        self.docs.retain(|doc| pictures.contains(&doc.picture));
        let kept_index = (self.docs.len() == before && shown == current).then_some(self.active);
        let mut added = None;
        for id in pictures {
            if !self.closed.contains(&id) && !self.docs.iter().any(|doc| doc.picture == id) {
                self.docs.push(Doc::new(id));
                added = Some(self.docs.len() - 1);
            }
        }
        self.active = added
            .or(kept_index)
            .or_else(|| current.and_then(|id| self.docs.iter().position(|doc| doc.picture == id)))
            .unwrap_or(0)
            .min(self.docs.len().saturating_sub(1));
        for doc in &mut self.docs {
            let stack = doc::layers(arena, doc.picture);
            if doc
                .layer
                .is_none_or(|layer| !stack.iter().any(|info| info.id == layer))
            {
                doc.layer = stack
                    .iter()
                    .rev()
                    .find(|info| info.kind == LayerKind::Raster)
                    .or(stack.last())
                    .map(|info| info.id);
            }
            if let Some(size) = arena.get_by_id(doc.picture).ok().and_then(picture::size)
                && doc
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.size() != size)
            {
                doc.selection = None;
                doc.edges = None;
            }
        }
    }

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
    fn rewrite(
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

    /// Runs a filter on the current layer at once.
    fn filter_now(&mut self, cx: &mut AppletContext<'_>, filter: Filter) {
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, &filter.label(), |pixels, width, height| {
            filter.apply(pixels, width, height, selection.as_ref(), offset);
        });
    }

    /// Opens a filter's dialog over the current layer.
    fn filter_dialog(&mut self, cx: &mut AppletContext<'_>, filter: Filter) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        self.dialog = Some(Dialog::Filter(FilterDialog::new(
            filter,
            info.id,
            raster,
            self.doc().and_then(|doc| doc.selection.clone()),
            info.style.offset,
        )));
    }

    /// The selection, or everything when nothing is selected.
    fn selection_or_all(&self, arena: &Arena) -> Option<Selection> {
        let doc = self.doc()?;
        doc.selection.clone().or_else(|| {
            let [width, height] = picture::size(arena.get_by_id(doc.picture).ok()?)?;
            Some(Selection::all(width, height))
        })
    }

    /// Mirrors (and for 180 degrees, turns) every raster layer.
    fn transform_canvas(
        &mut self,
        cx: &mut AppletContext<'_>,
        flip_h: bool,
        flip_v: bool,
        label: &str,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        let mut edit = Batch::new(label);
        for info in doc::layers(arena, picture) {
            if info.kind != LayerKind::Raster {
                continue;
            }
            let Some(raster) = doc::raster(arena, info.id) else {
                continue;
            };
            let (w, h) = (raster.width(), raster.height());
            if let Ok(Some(pixels)) = doc::rewrite(arena, info.id, label, |pixels, w, h| {
                mirror(pixels, w, h, flip_h, flip_v);
            }) {
                edit = edit.then(pixels);
            }
            let [x, y] = info.style.offset;
            if flip_h && (x != 0 || w != width) {
                edit = edit.then(SetProperty::new(
                    info.id,
                    picture::X,
                    i64::from(width) - x - i64::from(w),
                ));
            }
            if flip_v && (y != 0 || h != height) {
                edit = edit.then(SetProperty::new(
                    info.id,
                    picture::Y,
                    i64::from(height) - y - i64::from(h),
                ));
            }
        }
        if !edit.is_empty() {
            Self::apply(cx, edit);
        }
    }

    /// Copies the selected pixels of the current layer.
    fn copy(&mut self, cx: &mut AppletContext<'_>) -> bool {
        let arena = &cx.world.arena;
        let Some(info) = self.layer_info(arena) else {
            return false;
        };
        let Some(raster) = doc::raster(arena, info.id) else {
            cx.fail(tr!("only-raster-layers-can-copied"));
            return false;
        };
        let Some(selection) = self.selection_or_all(arena) else {
            return false;
        };
        let Some([x0, y0, x1, y1]) = selection.bounds() else {
            return false;
        };
        let (width, height) = (x1 - x0, y1 - y0);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        let Some(bytes) = raster.bytes() else {
            return false;
        };
        let [ox, oy] = info.style.offset;
        for y in 0..height {
            for x in 0..width {
                let (lx, ly) = (i64::from(x0 + x) - ox, i64::from(y0 + y) - oy);
                if lx < 0
                    || ly < 0
                    || lx >= i64::from(raster.width())
                    || ly >= i64::from(raster.height())
                {
                    continue;
                }
                let from = ((ly as u32 * raster.width() + lx as u32) * 4) as usize;
                let to = ((y * width + x) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                let cover = selection.at(x0 + x, y0 + y);
                pixels[to + 3] = (f32::from(pixels[to + 3]) * cover).round() as u8;
            }
        }
        match RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) {
            Ok(raster) => {
                self.clipboard = Some(Clip {
                    raster,
                    origin: [x0, y0],
                });
                true
            }
            Err(_) => false,
        }
    }

    /// Pastes the copied pixels as a new layer above the current one.
    fn paste(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let (Some(picture), Some(clip)) = (self.picture(), self.clipboard.clone()) else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        let source = clip.raster.bytes().unwrap_or_default();
        for y in 0..clip.raster.height() {
            for x in 0..clip.raster.width() {
                let (px, py) = (clip.origin[0] + x, clip.origin[1] + y);
                if px >= width || py >= height {
                    continue;
                }
                let from = ((y * clip.raster.width() + x) * 4) as usize;
                let to = ((py * width + px) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&source[from..from + 4]);
            }
        }
        let Ok(raster) = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) else {
            return;
        };
        let layer = Entity::new(name, raster).with_class(class::LAYER);
        let id = layer.id();
        let order = {
            let below = self.layer();
            let stack = doc::layers(arena, picture);
            let index = below.and_then(|below| stack.iter().position(|info| info.id == below));
            match index {
                Some(index) => sue_core::OrderKey::between(
                    Some(&stack[index].order),
                    stack.get(index + 1).map(|info| &info.order),
                ),
                None => arena.relations().next_order(picture, CHILD),
            }
        };
        let spawn =
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order));
        if Self::apply(cx, spawn)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
        }
    }

    /// Writes the flattened picture as `ext` (png, jpg, jxl, bmp, tif or tga).
    fn export(&mut self, cx: &mut AppletContext<'_>, ext: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(flat) = composite::flatten(arena, picture, options) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        let bytes = sue_io::encode_as(&flat, ext, jpeg_quality(cx), jxl_effort(cx));
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("export-failed", error = error));
                return;
            }
        };
        self.export_bytes(cx, bytes, ext);
    }

    /// Layered Photoshop document export (`.psd`, or `.psb` when `big`).
    fn export_psd(&mut self, cx: &mut AppletContext<'_>, big: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(psd) = crate::psd_io::to_psd(&cx.world.arena, picture) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        match sue_io::psd::encode_psd(&psd, big) {
            Ok(bytes) => self.export_bytes(cx, bytes, if big { "psb" } else { "psd" }),
            Err(error) => cx.fail(tr!("export-failed", error = error)),
        }
    }

    /// Asks where to write the exported `bytes` (`ext` files), or writes
    /// them to the exports folder when nobody is there to ask.
    fn export_bytes(&mut self, cx: &mut AppletContext<'_>, bytes: Vec<u8>, ext: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
        let file = format!("{name}.{ext}");
        if cx.view.interactive {
            // The save dialog asks where; `picked` writes it.
            let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
            self.pending_export = Some(bytes);
            cx.request(ShellRequest::PickFile(
                FilePick::save(
                    Some(AppletId::new(ID).expect("a valid id")),
                    "export",
                    &tr!("export"),
                    &file,
                )
                .extensions(&[ext])
                .starting_in(Some(start)),
            ));
        } else {
            let path = free_path(&export_dir(cx).join(&file));
            self.write_export(cx, &path, &bytes);
        }
    }

    /// Save As: the save dialog asks where and as what, a Sue project or
    /// any picture format (the picture flattened, as Export does).
    fn save_as(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            cx.request(ShellRequest::Command("file.save_as".to_owned()));
            return;
        };
        if !cx.view.interactive {
            cx.request(ShellRequest::Command("file.save_as".to_owned()));
            return;
        }
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
        let clip = FileType::new(&tr!("clip-studio-paint-file"), &["clip"]);
        let sue = FileType::new(&tr!("sue-project"), &["sue"]);
        let saves_clip = crate::clip_io::saves_clip(cx);
        let mut types = if saves_clip {
            vec![clip, sue]
        } else {
            vec![sue, clip]
        };
        types.extend(
            sue_io::PICTURE_SAVE_TYPES
                .iter()
                .map(|(what, extensions)| FileType::new(what, extensions)),
        );
        let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "save-as",
                &tr!("save-as"),
                &format!("{name}.{}", if saves_clip { "clip" } else { "sue" }),
            )
            .types(types)
            .starting_in(Some(start)),
        ));
    }

    /// Writes what Save As picked: the project, or the flattened picture in
    /// the format of `path`'s extension.
    fn save_as_to(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if extension == "clip" {
            if let Some(picture) = self.picture() {
                self.write_clip(cx, picture, path);
            }
            return;
        }
        if extension == "sue" || extension.is_empty() {
            let path = if extension.is_empty() {
                path.with_extension("sue")
            } else {
                path.to_path_buf()
            };
            // The project is the shell's to write.
            cx.request(ShellRequest::Picked {
                pick: FilePick::save(None, "save-project", &tr!("save-project"), ""),
                path,
            });
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(flat) = composite::flatten(&cx.world.arena, picture, options) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        match sue_io::encode_as(&flat, &extension, jpeg_quality(cx), jxl_effort(cx)) {
            Ok(bytes) => {
                if self.write_export(cx, path, &bytes) {
                    cx.request(ShellRequest::Remember {
                        path: path.to_path_buf(),
                        saved: true,
                    });
                }
            }
            Err(error) => cx.fail(tr!("cannot-save-as", extension = extension, error = error)),
        }
    }

    /// Save when Sue Paint saves `.clip` files: the picture's own `.clip`
    /// again, or Save As for one never saved.
    fn save_clip(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        match self.clip_paths.get(&picture).cloned() {
            Some(path) => self.write_clip(cx, picture, &path),
            None => self.save_as(cx),
        }
    }

    /// Writes `picture` as a `.clip` at `path`, and remembers it there.
    fn write_clip(&mut self, cx: &mut AppletContext<'_>, picture: EntityId, path: &Path) {
        let bytes = match crate::clip_io::encode(&cx.world.arena, picture) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("cannot-save-as", extension = "clip", error = error));
                return;
            }
        };
        if self.write_export(cx, path, &bytes) {
            self.clip_paths.insert(picture, path.to_path_buf());
            self.saved_at.insert(picture, cx.history.cursor());
            if let Some(doc) = self.docs.iter().find(|doc| doc.picture == picture) {
                let (_, _, version) = doc.compositor.display();
                self.clean.insert(picture, version);
            }
            cx.request(ShellRequest::Remember {
                path: path.to_path_buf(),
                saved: true,
            });
        }
    }

    /// Writes an encoded export to `path` and says where it went. Returns
    /// `true` once written.
    fn write_export(&mut self, cx: &mut AppletContext<'_>, path: &Path, bytes: &[u8]) -> bool {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match write_atomic(path, bytes) {
            Ok(()) => {
                cx.saved(path, tr!("kib", value = bytes.len().div_ceil(1024)));
                true
            }
            Err(error) => {
                cx.fail(tr!("cannot-write", path = path.display(), error = error));
                false
            }
        }
    }

    /// Sue Paint's own colors, used when `sue_paint.theme` is Custom; they
    /// start as the Gray theme.
    fn custom_colors() -> Vec<RiceDeclaration> {
        let color = |key: &str, hex: &str, description: &str| {
            RiceDeclaration::new(
                format!("sue_paint_colors.{key}"),
                RiceParameter::ColorHex {
                    value: hex
                        .parse()
                        .unwrap_or_else(|error| panic!("built-in color {hex}: {error}")),
                },
                description,
            )
        };
        vec![
            color("void", "#2b2b2b", "Around the picture"),
            color("panel", "#3a3a3a", "Palettes and menus"),
            color("pill", "#444444", "Fields, chips and cards"),
            color("hover", "#505050", "Hovered controls"),
            color("active", "#5e5e5e", "Pressed and chosen controls"),
            color("border", "#2a2a2a", "Hairline borders"),
            color(
                "border_active",
                "#d8d8d8",
                "Borders of chosen and focused controls",
            ),
            color("text", "#e6e6e6", "Primary text and active icons"),
            color("muted", "#a4a4a4", "Secondary text"),
            color("selection", "#6a6a6a", "Text selection highlight"),
            color("selection_text", "#ffffff", "Text inside a selection"),
            color("grid", "#474747", "Grids and rulers"),
            color("shadow", "#141414", "Shadow under floating surfaces"),
        ]
    }

    /// Remembers every open picture as saved, for the tabs' modified dot.
    pub(crate) fn mark_clean(&mut self) {
        for doc in &self.docs {
            let (_, _, version) = doc.compositor.display();
            self.clean.insert(doc.picture, version);
        }
    }

    /// Writes the color sets to Sue Paint's data folder.
    pub(crate) fn save_color_sets(&self, cx: &mut AppletContext<'_>) {
        if let Err(error) = self
            .color_sets
            .save(crate::colorsets::folder(cx).as_deref())
        {
            cx.fail(tr!("cannot-save-the-color-sets", error = error));
        }
    }

    /// Asks for a picture file and opens it as a new canvas, or as a new
    /// layer of the current one.
    pub(crate) fn pick_image(&mut self, cx: &mut AppletContext<'_>, as_layer: bool) {
        let start = self
            .last_dir
            .clone()
            .or_else(|| cx.catalog.folders.get("home").cloned());
        let (purpose, title) = if as_layer {
            ("layer", tr!("import-picture-as-a-layer"))
        } else {
            ("open", tr!("open-picture"))
        };
        cx.request(ShellRequest::PickFile(
            FilePick::open(
                Some(AppletId::new(ID).expect("a valid id")),
                purpose,
                &title,
            )
            .extensions(OPENS)
            .starting_in(start),
        ));
    }

    /// Opens the picture file at `path` as a new canvas, or as a new layer.
    pub(crate) fn open_picked(&mut self, cx: &mut AppletContext<'_>, path: &Path, as_layer: bool) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match sue_platform::fs::read(path) {
            Ok(bytes) => match self.open_bytes(cx, path, &bytes, as_layer) {
                Ok(()) => {
                    if !as_layer {
                        Self::remember_recent(cx, path);
                    }
                }
                Err(error) => cx.fail(error),
            },
            Err(error) => cx.fail(tr!("cannot-read", path = path.display(), error = error)),
        }
    }

    /// Opens picture file `bytes` (read from `path`) as a new canvas, or as
    /// a new layer of the current canvas.
    ///
    /// # Errors
    /// If no format reads the file as pixels.
    pub fn open_bytes(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let name = path.file_stem().map_or_else(
            || tr!("picture"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let file = path
            .file_name()
            .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
        if bytes.starts_with(b"8BPS") {
            return self.open_psd(cx, &name, &file, bytes, as_layer);
        }
        if sue_io::clip::sniff(bytes) {
            return self.open_clip(cx, path, &name, &file, bytes, as_layer);
        }
        let formats = sue_io::formats(sue_model::registry());
        let bundle = formats
            .import(bytes, &file, &Properties::new())
            .map_err(|error| tr!("cannot-open", file = file, error = error))?;
        let raster = bundle
            .entities
            .iter()
            .find_map(|entity| entity.component(ComponentKind::RasterTile2D)?.as_raster())
            .ok_or_else(|| tr!("has-no-pixels-sue-paint-can", file = file))?;
        let rgba = to_rgba(raster);
        let target = self.picture().filter(|_| as_layer);
        match target {
            Some(picture) => {
                let layer = Entity::new(name, rgba).with_class(class::LAYER);
                let id = layer.id();
                let order = cx.world.arena.relations().next_order(picture, CHILD);
                let spawn = SpawnEntity::new(layer)
                    .with_relation(Relation::new(picture, CHILD, id).ordered(order));
                if Self::apply(cx, spawn)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            None => {
                let (width, height) = (rgba.width(), rgba.height());
                let image = Entity::empty(name.clone(), class::IMAGE)
                    .with_property(picture::WIDTH, i64::from(width))
                    .with_property(picture::HEIGHT, i64::from(height))
                    .with_property(picture::DPI, 72.0);
                let picture_id = image.id();
                let layer = Entity::new(name.clone(), rgba).with_class(class::LAYER);
                let layer_id = layer.id();
                let edit = Batch::new(tr!("open-3", file = file))
                    .then(SpawnEntity::new(image))
                    .then(
                        SpawnEntity::new(layer).with_relation(
                            Relation::new(picture_id, CHILD, layer_id)
                                .ordered(sue_core::OrderKey::default()),
                        ),
                    );
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture_id);
                    self.saved_at.insert(picture_id, cx.history.cursor());
                }
            }
        }
        Ok(())
    }

    /// Opens a Photoshop document with its layers and folders kept: as a
    /// new canvas, or into the current one above the current layer.
    fn open_psd(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        file: &str,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let psd = sue_io::psd::decode_psd(bytes)
            .map_err(|error| tr!("cannot-open", file = file, error = error))?;
        self.open_layers(cx, name, file, &psd, 72.0, as_layer)
            .map(|_| ())
    }

    /// Opens a Clip Studio file. One Sue saved comes back whole, from the
    /// Sue project it carries; any other opens with its layers and folders.
    /// Save writes the file again.
    fn open_clip(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        name: &str,
        file: &str,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let clip = sue_io::clip::decode_clip(bytes)
            .map_err(|error| tr!("cannot-open", file = file, error = error))?;
        if !as_layer
            && let Some(sue) = &clip.sue
            && let Some((edit, picture)) =
                crate::clip_io::embedded(&cx.world.arena, sue, &tr!("open-3", file = file))
        {
            if edit.is_none_or(|edit| Self::apply(cx, edit)) {
                self.activate(&cx.world.arena, picture);
                self.saved_at.insert(picture, cx.history.cursor());
                self.clip_paths.insert(picture, path.to_path_buf());
            }
            return Ok(());
        }
        if let Some(picture) = self.open_layers(cx, name, file, &clip.psd, clip.dpi, as_layer)? {
            self.clip_paths.insert(picture, path.to_path_buf());
        }
        Ok(())
    }

    /// Opens layered `psd` as a new canvas at `dpi`, or into the current
    /// one above the current layer. Answers the new canvas.
    fn open_layers(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        file: &str,
        psd: &sue_io::psd::Psd,
        dpi: f64,
        as_layer: bool,
    ) -> Result<Option<EntityId>, String> {
        if let Some(picture) = self.picture().filter(|_| as_layer) {
            let below = self.doc().and_then(|doc| doc.layer);
            let edit = crate::psd_io::import(&cx.world.arena, picture, below, psd)?;
            Self::apply(cx, edit);
            return Ok(None);
        }
        let image = Entity::empty(name.to_owned(), class::IMAGE)
            .with_property(picture::WIDTH, i64::from(psd.width))
            .with_property(picture::HEIGHT, i64::from(psd.height))
            .with_property(picture::DPI, dpi);
        let picture = image.id();
        let layers = crate::psd_io::import(&cx.world.arena, picture, None, psd)?;
        let edit = Batch::new(tr!("open-3", file = file))
            .then(SpawnEntity::new(image))
            .then(layers);
        if Self::apply(cx, edit) {
            self.activate(&cx.world.arena, picture);
            self.saved_at.insert(picture, cx.history.cursor());
            if let Some(top) = doc::layers(&cx.world.arena, picture)
                .iter()
                .rev()
                .find(|info| !doc::is_folder(&cx.world.arena, info.id))
                .map(|info| info.id)
                && let Some(doc) = self.doc_mut()
            {
                doc.layer = Some(top);
            }
            return Ok(Some(picture));
        }
        Ok(None)
    }

    /// File, Import, Vector, Movie or Pattern from Image: the file at
    /// `path` as a vector layer, as cels of a new animation folder, or
    /// tiled over a new layer.
    pub(crate) fn import_file(&mut self, cx: &mut AppletContext<'_>, purpose: &str, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path.file_stem().map_or_else(
            || tr!("imported"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let bytes = match sue_platform::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("cannot-read", path = path.display(), error = error));
                return;
            }
        };
        let arena = &cx.world.arena;
        let below = self.doc().and_then(|doc| doc.layer);
        let size = arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
            .unwrap_or([1, 1]);
        let made: Result<(doc::Edit, Option<EntityId>), String> = match purpose {
            "import-vector" => String::from_utf8(bytes)
                .map_err(|_| tr!("the-file-is-not-text"))
                .and_then(|text| sue_io::svg::parse_svg(&text))
                .and_then(|drawing| {
                    let lines = crate::vector::from_svg(&drawing, size);
                    if lines.is_empty() {
                        return Err(tr!("the-drawing-has-no-lines"));
                    }
                    let (spawn, id) =
                        doc::add_vector_lines(arena, picture, below, name.clone(), &lines)?;
                    Ok((
                        doc::Edit::new(tr!("import-vector", name = name)).then(spawn),
                        Some(id),
                    ))
                }),
            "import-movie" => sue_io::decode_frames(&bytes).and_then(|(frames, fps)| {
                let (edit, track) =
                    crate::anim::import_frames(arena, picture, below, &name, &frames, fps)?;
                if let Some(doc) = self.doc_mut() {
                    doc.track = track;
                }
                Ok((edit, None))
            }),
            _ => sue_io::decode_picture(&bytes, None)
                .map(|tile| to_rgba(&tile))
                .and_then(|tile| {
                    crate::materials::tiled(&tile, size)
                        .ok_or_else(|| tr!("the-picture-has-no-pixels"))
                })
                .and_then(|raster| {
                    let (spawn, id) =
                        doc::add_layer_pixels(arena, picture, below, name.clone(), raster)?;
                    Ok((
                        doc::Edit::new(tr!("pattern-from", name = name)).then(spawn),
                        Some(id),
                    ))
                }),
        };
        match made {
            Ok((edit, layer)) => {
                if Self::apply(cx, edit) {
                    if purpose == "import-movie" {
                        self.panels.timeline = true;
                    }
                    if let Some(layer) = layer
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.layer = Some(layer);
                    }
                }
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// Edit, Create Brush Tip from Image: the current layer (inside the
    /// selection) becomes a brush tip shape, kept in Sue Paint's data
    /// folder, and a new Brush sub tool paints with it.
    fn make_brush_tip(&mut self, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let (Some(layer), Some(selection)) = (self.layer(), self.selection_or_all(arena)) else {
            return;
        };
        let Some(raster) = doc::raster(arena, layer) else {
            return;
        };
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let (w, h) = (raster.width(), raster.height());
        let mask = selection.mask();
        let (mut min, mut max) = ([u32::MAX; 2], [0_u32; 2]);
        for y in 0..h {
            for x in 0..w {
                let index = (y * w + x) as usize;
                if bytes[index * 4 + 3] > 0 && mask.get(index).is_some_and(|cover| *cover > 0) {
                    min = [min[0].min(x), min[1].min(y)];
                    max = [max[0].max(x + 1), max[1].max(y + 1)];
                }
            }
        }
        if max[0] <= min[0] || max[1] <= min[1] {
            cx.fail(tr!("draw-tips-shape-layer-first"));
            return;
        }
        let (cw, ch) = (max[0] - min[0], max[1] - min[1]);
        let mut cut = Vec::with_capacity((cw * ch * 4) as usize);
        for y in min[1]..max[1] {
            for x in min[0]..max[0] {
                let index = (y * w + x) as usize;
                let mut pixel = [
                    bytes[index * 4],
                    bytes[index * 4 + 1],
                    bytes[index * 4 + 2],
                    bytes[index * 4 + 3],
                ];
                pixel[3] = ((u32::from(pixel[3]) * u32::from(mask[index])) / 255) as u8;
                cut.extend_from_slice(&pixel);
            }
        }
        // Coverage comes from the alpha: dark paint on clear.
        for pixel in cut.chunks_mut(4) {
            if pixel[3] == 255 {
                pixel[3] = 254;
            }
        }
        let name = tr!("tip", value = crate::brush::shapes().len() + 1);
        let Some(shape) = crate::brush::TipShape::from_rgba(&name, &cut, cw, ch) else {
            return;
        };
        if let Some(folder) = crate::materials::location(cx).map(|folder| folder.join("tips"))
            && let Ok(png) = sue_io::encode_png_rgba(cw, ch, &cut)
        {
            let _ = sue_platform::fs::create_dir_all(&folder)
                .and_then(|()| sue_platform::fs::write(folder.join(format!("{name}.png")), png));
        }
        let id = crate::brush::register_shape(shape);
        let tip = crate::brush::Tip {
            size: (cw.max(ch) as f32).clamp(8.0, 200.0),
            shape: id,
            spacing: 0.25,
            ..crate::brush::Tip::default()
        };
        self.toolbox.add_sub(crate::tools::SubTool::new(
            crate::tools::Tool::Brush,
            &name,
            0,
            tip,
        ));
        cx.notify(tr!("new-brush-sub-tool", name = name));
    }

    /// File, Print: the pages (every page of a story, or the canvas) laid
    /// on paper as Print Settings says, written as a PDF and opened in the
    /// system's viewer to print.
    pub(crate) fn print_now(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let ids = match self.story_here(arena) {
            Some((_, pages, _)) if self.print.all_pages => pages,
            _ => vec![picture],
        };
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let pages: Vec<RasterTile2D> = ids
            .iter()
            .filter_map(|page| composite::flatten(arena, *page, options))
            .collect();
        let entity = arena.get_by_id(picture).ok();
        let dpi = entity
            .and_then(|entity| entity.property(picture::DPI))
            .and_then(Value::as_f64)
            .unwrap_or(72.0) as f32;
        let name = entity.map_or_else(|| tr!("print"), |entity| entity.name().to_owned());
        let folder = crate::colorsets::folder(cx)
            .map_or_else(std::env::temp_dir, |folder| folder.join("print"));
        let path = folder.join(format!("{}.pdf", name.replace(['/', '\\', ':'], "_")));
        let printed = crate::print::pdf(&self.print, &pages, dpi, &name).and_then(|bytes| {
            sue_platform::fs::create_dir_all(&folder)
                .and_then(|()| sue_platform::fs::write(&path, bytes))
                .map_err(|error| error.to_string())?;
            sue_platform::open_document(&path).map_err(|error| error.to_string())
        });
        match printed {
            Ok(()) => cx.notify(tr!(
                "pages-ready-print-your-pdf-viewer",
                count = pages.len()
            )),
            Err(error) => cx.fail(tr!("cannot-print", error = error)),
        }
    }

    /// Batch Export of canvases: every open canvas flattened to `path`'s
    /// format, each named after `path` and its canvas.
    fn export_canvases(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("canvas"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.last_dir = Some(dir.clone());
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let pictures: Vec<EntityId> = {
            let mut seen = Vec::new();
            for doc in &self.docs {
                if !seen.contains(&doc.picture) {
                    seen.push(doc.picture);
                }
            }
            seen
        };
        let mut written = 0;
        for picture in pictures {
            let arena = &cx.world.arena;
            let Some(flat) = composite::flatten(arena, picture, options) else {
                continue;
            };
            let name = arena
                .get_by_id(picture)
                .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
            let target = free_path(&dir.join(format!("{stem}-{name}.{extension}")));
            match sue_io::encode_as(&flat, &extension, jpeg_quality(cx), jxl_effort(cx)).and_then(
                |bytes| sue_platform::fs::write(&target, bytes).map_err(|error| error.to_string()),
            ) {
                Ok(()) => written += 1,
                Err(error) => {
                    cx.fail(tr!("cannot-export-2", name = name, error = error));
                    return;
                }
            }
        }
        cx.notify(tr!(
            "exported-canvases-to",
            dir = dir.display(),
            written = written
        ));
    }

    /// Goes back to where the picture was last saved or opened (File,
    /// Revert), as undoing step by step would; Redo brings the changes
    /// back.
    fn revert(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(saved) = self.saved_at.get(&picture).copied() else {
            cx.fail(tr!("save-picture-first-revert-goes-back"));
            return;
        };
        let mut steps = 0;
        let mut at = cx.history.cursor();
        while at != saved {
            let Some(parent) = cx.history.parent(at) else {
                cx.fail(tr!("last-save-another-branch-history-pick"));
                return;
            };
            at = parent;
            steps += 1;
        }
        if steps == 0 {
            cx.notify(tr!("nothing-changed-since-last-save"));
            return;
        }
        for _ in 0..steps {
            if let Err(error) = cx.undo(false) {
                cx.fail(tr!("cannot-revert", error = error));
                return;
            }
        }
        self.sync_docs(&cx.world.arena);
        cx.notify(tr!("reverted-changes-redo-brings-them-back", steps = steps));
    }

    /// Reads the recent pictures from the work Sue remembers (shared with
    /// the Hub and every studio) whenever it changed, into Open Recent.
    fn load_recent(&mut self, cx: &AppletContext<'_>) {
        let revision = cx.work.revision();
        if self.recent_seen == Some(revision) {
            return;
        }
        self.recent_seen = Some(revision);
        self.recent_files = cx
            .work
            .recent()
            .filter(|file| opens(&file.path))
            .take(10)
            .map(|file| file.path.clone())
            .collect();
        crate::menus::set_recent(
            &mut self.menus,
            crate::menus::recent_items(&self.recent_files),
        );
    }

    /// Puts `path` first among the files Sue remembers.
    fn remember_recent(cx: &mut AppletContext<'_>, path: &Path) {
        cx.request(ShellRequest::Remember {
            path: path.to_path_buf(),
            saved: false,
        });
    }

    /// Creates a picture from the New dialog and makes it current.
    pub(crate) fn make_picture(&mut self, cx: &mut AppletContext<'_>, spec: &NewPicture) {
        match doc::create(spec) {
            Ok((edit, picture)) => {
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Makes `picture` the current tab, opening one if it has none.
    pub(crate) fn activate(&mut self, arena: &Arena, picture: EntityId) {
        self.closed.remove(&picture);
        self.sync_docs(arena);
        if let Some(index) = self.docs.iter().position(|doc| doc.picture == picture) {
            self.active = index;
        }
    }

    /// A New dialog filled from the rice.
    pub(crate) fn new_dialog(cx: &AppletContext<'_>) -> NewDialog {
        let rice = cx.rice();
        let mut spec = NewPicture::default();
        if let Some(width) = rice
            .get_i64("sue_paint.width")
            .and_then(|v| u32::try_from(v).ok())
        {
            spec.width = width;
        }
        if let Some(height) = rice
            .get_i64("sue_paint.height")
            .and_then(|v| u32::try_from(v).ok())
        {
            spec.height = height;
        }
        if let Some(dpi) = rice.get_i64("sue_paint.dpi") {
            spec.dpi = dpi as f64;
        }
        NewDialog::new(&spec)
    }

    /// Records what an unfinished gesture or slider already changed in
    /// place, so nothing escapes the history when the studio closes.
    fn settle(&mut self, cx: &mut AppletContext<'_>) {
        match self.gesture.take() {
            Some(Gesture::Stroke { stroke, label, .. }) => {
                if let Some(raster) = doc::raster(&cx.world.arena, stroke.layer()) {
                    let edit = stroke.finish(raster, &label);
                    if !edit.is_empty() {
                        cx.record(ArenaEdit(edit));
                    }
                }
            }
            Some(Gesture::Move { layer, old, .. }) => {
                let mut steps = Batch::new(tr!("move-layer"));
                for (key, before) in [(picture::X, old[0].clone()), (picture::Y, old[1].clone())] {
                    steps = steps.then(match before {
                        Some(value) => SetProperty::new(layer, key, value),
                        None => SetProperty::remove(layer, key),
                    });
                }
                cx.record(ArenaEdit(steps));
            }
            _ => {}
        }
        if let Some(live) = self.live.take() {
            let label = cx
                .world
                .arena
                .get_by_id(live.id)
                .ok()
                .and_then(|entity| entity.property(&live.key).cloned())
                .map_or_else(
                    || tr!("set-2", key = live.key),
                    |value| doc::label(&live.key, &value),
                );
            let step = match live.old {
                Some(old) => SetProperty::new(live.id, live.key, old),
                None => SetProperty::remove(live.id, live.key),
            };
            cx.record(ArenaEdit(Batch::new(label).then(step)));
        }
    }

    /// Moves the current layer to `index` (0 is the bottom).
    /// Moves the current layer up or down as Clip Studio Paint's layer
    /// palette does with folders: a folder moves with everything in it, a
    /// layer leaves its folder past the folder's top or bottom and joins a
    /// folder it moves into. Returns `false` when a plain move does.
    fn move_in_folders(&mut self, cx: &mut AppletContext<'_>, up: bool) -> bool {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return false;
        };
        let arena = &cx.world.arena;
        let stack: Vec<EntityId> = doc::layers(arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect();
        let Some(at) = stack.iter().position(|id| *id == layer) else {
            return false;
        };
        if doc::is_folder(arena, layer) {
            // The folder and its layers move as one block, past one layer.
            let members = doc::members(arena, picture, layer);
            let low = stack
                .iter()
                .position(|id| members.contains(id))
                .map_or(at, |first| first.min(at));
            let neighbor = if up {
                stack.get(at + 1)
            } else {
                low.checked_sub(1).and_then(|below| stack.get(below))
            };
            let Some(&neighbor) = neighbor else {
                return true;
            };
            let to = if up { low } else { at };
            if let Some(edit) = doc::reorder(arena, picture, neighbor, to) {
                Self::apply(cx, edit);
            }
            return true;
        }
        let folder = doc::folder_of(arena, layer);
        if up {
            let Some(&above) = stack.get(at + 1) else {
                return false;
            };
            if Some(above) == folder {
                // Out past the top of its folder.
                let edit = doc::reorder(arena, picture, layer, at + 1);
                let out = doc::set_folder(arena, layer, doc::folder_of(arena, above));
                for edit in [edit, out].into_iter().flatten() {
                    Self::apply(cx, edit);
                }
                return true;
            }
            let joins = if doc::is_folder(arena, above) && doc::folder_of(arena, above) == folder {
                // Right under an empty folder.
                doc::members(arena, picture, above)
                    .is_empty()
                    .then_some(above)
            } else {
                doc::folder_of(arena, above).filter(|outer| {
                    Some(*outer) != folder && !doc::folders_around(arena, *outer).contains(&layer)
                })
            };
            if let Some(into) = joins
                && let Some(edit) = doc::set_folder(arena, layer, Some(into))
            {
                // Into the folder from below, staying put.
                Self::apply(cx, edit);
                return true;
            }
            false
        } else {
            let below = at
                .checked_sub(1)
                .and_then(|index| stack.get(index))
                .copied();
            if let Some(folder) = folder
                && below.is_none_or(|below| !doc::folders_around(arena, below).contains(&folder))
            {
                // Out past the bottom of its folder, staying put.
                if let Some(edit) = doc::set_folder(arena, layer, doc::folder_of(arena, folder)) {
                    Self::apply(cx, edit);
                }
                return true;
            }
            if let Some(below) = below
                && doc::is_folder(arena, below)
                && doc::folder_of(arena, below) == folder
            {
                // Down into the folder under it, as its top layer.
                let edit = doc::reorder(arena, picture, layer, at - 1);
                let into = doc::set_folder(arena, layer, Some(below));
                for edit in [edit, into].into_iter().flatten() {
                    Self::apply(cx, edit);
                }
                return true;
            }
            false
        }
    }

    fn move_layer_to(&mut self, cx: &mut AppletContext<'_>, index: usize) {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return;
        };
        if let Some(relate) = doc::reorder(&cx.world.arena, picture, layer, index) {
            Self::apply(cx, relate);
        }
    }

    /// Flips a property of the current layer.
    fn toggle_layer(&mut self, cx: &mut AppletContext<'_>, key: &str, default: bool) {
        let Some(layer) = self.layer() else {
            return;
        };
        let now = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_bool)
            .unwrap_or(default);
        Self::apply(cx, doc::set(layer, key, !now));
    }

    /// Changes the view of the current tab.
    fn with_view(
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

    // ----- Commands ---------------------------------------------------------

    /// How a menu item for `command` looks right now.
    pub(crate) fn state(
        &self,
        command: &str,
        arena: &Arena,
        history: &UndoStack<World>,
    ) -> ItemState {
        if !implemented(command) {
            return ItemState::off();
        }
        let has_doc = self.doc().is_some();
        let info = self.layer_info(arena);
        let raster = info
            .as_ref()
            .is_some_and(|info| info.kind == LayerKind::Raster);
        let selection = self.doc().is_some_and(|doc| doc.selection.is_some());
        let flag = |on: bool| ItemState::check(on);
        let when = |on: bool| {
            if on {
                ItemState::on()
            } else {
                ItemState::off()
            }
        };
        match command {
            "file.new" | "file.open" | "file.save" | "file.save_as" | "file.preferences"
            | "file.shortcuts" | "file.pressure" | "file.leave" | "app.quit" | "help.shortcuts"
            | "help.about" => ItemState::on(),
            "edit.undo" => when(history.can_undo()),
            "edit.redo" => when(history.can_redo()),
            "edit.paste" | "edit.paste_here" => when(has_doc && self.clipboard.is_some()),
            "layer.merge_selected" => {
                when(self.doc().is_some_and(|doc| !doc.picked.is_empty()) && raster)
            }
            "layer.convert" | "edit.brush_tip" | "edit.material.tip" => when(raster),
            "select.from_selection_layer" => when(
                info.as_ref()
                    .and_then(|info| arena.get_by_id(info.id).ok())
                    .is_some_and(|entity| {
                        entity.property(SELECTION_LAYER).and_then(Value::as_bool) == Some(true)
                    }),
            ),
            "edit.clear_outside" | "select.none" | "select.invert" => when(selection),
            "select.again" => when(self.doc().is_some_and(|doc| doc.last_selection.is_some())),
            "select.expand" | "select.shrink" | "select.feather" | "select.to_layer" => {
                when(selection)
            }
            "select.add_layer" | "select.subtract_layer" => when(raster),
            "select.quick_mask" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.quick_mask.is_some())),
            "select.launcher" => flag(self.launcher),
            "anim.timeline.enable" => flag(self.timeline_on),
            "layer.from_selection" => when(selection && raster),
            "edit.cut"
            | "edit.copy"
            | "edit.clear"
            | "edit.fill"
            | "edit.recolor"
            | "edit.luma_to_alpha"
            | "edit.align.left"
            | "edit.align.hcenter"
            | "edit.align.right"
            | "edit.align.top"
            | "edit.align.vcenter"
            | "edit.align.bottom"
            | "edit.flip_h"
            | "edit.flip_v"
            | "edit.rotate180"
            | "select.from_layer"
            | "layer.toggle.lock_alpha" => when(raster),
            command if command.starts_with("filter.") => when(raster),
            "layer.merge_down" => {
                let below_raster = info.as_ref().is_some_and(|info| {
                    let stack = doc::layers(arena, self.picture().unwrap_or_default());
                    stack
                        .iter()
                        .position(|layer| layer.id == info.id)
                        .and_then(|at| at.checked_sub(1))
                        .is_some_and(|below| stack[below].kind == LayerKind::Raster)
                });
                when(raster && below_raster)
            }
            "layer.new.paper" => when(
                has_doc
                    && !doc::layers(arena, self.picture().unwrap_or_default())
                        .iter()
                        .any(|layer| layer.kind == LayerKind::Paper),
            ),
            "layer.toggle.visible" => {
                info.map_or(ItemState::off(), |info| flag(info.style.visible))
            }
            "layer.toggle.clip" => info.map_or(ItemState::off(), |info| flag(info.style.clip)),
            "layer.mask.outside" | "layer.mask.inside" => {
                let masked = info
                    .as_ref()
                    .is_some_and(|info| doc::mask_of(arena, info.id).is_some());
                let mut state =
                    when(raster && !masked && (command == "layer.mask.outside" || selection));
                state.checked = Some(masked);
                state
            }
            "layer.mask.delete" | "layer.mask.apply" => when(
                info.as_ref()
                    .is_some_and(|info| doc::mask_of(arena, info.id).is_some()),
            ),
            "layer.mask.toggle" | "layer.mask.edit" => match info
                .as_ref()
                .filter(|info| doc::mask_of(arena, info.id).is_some())
            {
                Some(info) => flag(if command == "layer.mask.toggle" {
                    doc::mask_on(arena, info.id)
                } else {
                    self.doc().is_some_and(|doc| doc.mask_edit)
                }),
                None => ItemState::off(),
            },
            "layer.toggle.draft" => info.map_or(ItemState::off(), |info| flag(info.style.draft)),
            "layer.rasterize" => when(
                info.as_ref()
                    .is_some_and(|info| crate::vector::is_vector(arena, info.id)),
            ),
            "layer.toggle.reference" => info.map_or(ItemState::off(), |info| {
                flag(doc::is_reference(arena, info.id))
            }),
            "layer.toggle.lock" => info.map_or(ItemState::off(), |info| flag(info.locked)),
            "view.grid" => flag(self.grid),
            "file.timelapse.record" => self.doc().map_or(ItemState::off(), |doc| {
                flag(
                    arena
                        .get_by_id(doc.picture)
                        .ok()
                        .and_then(|entity| entity.property(crate::timelapse::RECORD))
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )
            }),
            "anim.onion" => flag(self.onion),
            "anim.loop" => flag(self.looping),
            "anim.light_table" => flag(self.light_table),
            "anim.light_table.color" => flag(self.light_color),
            "anim.light_table.cel" => flag(self.light_cel),
            "anim.cel.lock" => flag(self.cel_lock.is_some()),
            "view.snap_special" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.ruler_on)),
            "view.snap_ruler" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.snap_ruler)),
            "layer.ruler.show" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.show_rulers)),
            "layer.ruler.delete" => when(
                self.doc()
                    .is_some_and(|doc| !crate::rulers::Rulers::of(arena, doc.picture).is_empty()),
            ),
            "view.pixel_grid" => flag(self.pixel_grid),
            "view.selection" => flag(self.show_selection),
            "view.flip_h" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.view.flip_h)),
            "view.flip_v" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.view.flip_v)),
            "window.command_bar" => flag(self.panels.command_bar),
            "window.tool" => flag(self.panels.tool),
            "window.sub_tool" => flag(self.panels.sub_tool),
            "window.tool_property" => flag(self.panels.tool_property),
            "window.brush_size" => flag(self.panels.brush_size),
            "window.color_wheel" => flag(self.panels.color && self.color_tab == 0),
            "window.color_slider" => flag(self.panels.color && self.color_tab == 1),
            "window.intermediate" => flag(self.panels.color && self.color_tab == 2),
            "window.approximate" => flag(self.panels.color && self.color_tab == 3),
            "window.color_set" => flag(self.panels.color_set && self.set_tab == 0),
            "window.material" => flag(self.panels.color_set && self.set_tab == 2),
            "window.color_history" => flag(self.panels.color_set && self.set_tab == 1),
            "window.navigator" => flag(self.panels.navigator),
            "window.sub_view" => flag(self.panels.navigator && self.nav_tab == 1),
            "window.cels" => flag(self.panels.layer && self.layer_tab == 3),
            "window.tile" => flag(self.tiled),
            "view.ruler_bar" => flag(self.ruler_bar),
            "view.scroll_bar" => flag(self.scroll_bars),
            "file.new_clipboard" => when(self.clipboard.is_some()),
            "window.brush_grid" => flag(self.panels.sub_tool && self.tool_tab == 2),
            "window.info" => flag(self.dock.has(crate::dock::Palette::Info)),
            "window.auto_actions" => flag(self.dock.has(crate::dock::Palette::AutoAction)),
            "window.item_bank" => flag(self.dock.has(crate::dock::Palette::ItemBank)),
            interface if interface.starts_with("window.interface.") => {
                flag(self.interface == interface["window.interface.".len()..])
            }
            "view.snap_grid" => flag(self.snap_grid),
            "view.paper" => match self.picture().and_then(|picture| {
                doc::layers(arena, picture)
                    .into_iter()
                    .find(|info| info.kind == LayerKind::Paper)
            }) {
                Some(paper) => flag(paper.style.visible),
                None => ItemState::off(),
            },
            "view.crop_frame" => flag(self.crop_frame),
            "view.bleed" => flag(self.bleed),
            "view.page_image" => flag(self.page_image),
            "view.tone_area" => flag(self.tone_area),
            "view.profile_preview" => flag(self.proof_on),
            "window.cascade" => when(self.tiled),
            "window.new" => when(has_doc),
            "window.layer" => flag(self.panels.layer && self.layer_tab == 0),
            "window.layer_property" => flag(self.panels.layer && self.layer_tab == 1),
            "window.history" => flag(self.panels.layer && self.layer_tab == 2),
            "window.timeline" => flag(self.panels.timeline),
            "window.hide_all" => flag(!self.any_panel()),
            "window.next_tab" | "window.prev_tab" => when(self.docs.len() > 1),
            _ => when(has_doc),
        }
    }

    fn any_panel(&self) -> bool {
        let p = self.panels;
        p.tool
            || p.sub_tool
            || p.tool_property
            || p.brush_size
            || p.color
            || p.color_set
            || p.navigator
            || p.layer
            || p.timeline
    }

    /// Runs a command of the menus, the command bar or a shortcut.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    pub(crate) fn run(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        // Studio Mode's keys T1 to T7 run the commands the rice names.
        if let Some(index) = command
            .strip_prefix("touch.custom.")
            .and_then(|index| index.parse::<usize>().ok())
        {
            let custom = self.custom_command(index, cx);
            if !custom.starts_with("touch.") && !custom.is_empty() {
                self.run(&custom, cx);
            }
            return;
        }
        self.record_auto(command);
        if command.starts_with("story.") && self.story_command(command, cx) {
            return;
        }
        if (command.starts_with("anim.")
            || command.starts_with("file.export.gif")
            || command == "file.export.sequence"
            || command == "file.export.movie"
            || command == "file.export.cels")
            && self.anim_command(command, cx)
        {
            return;
        }
        if !self.state(command, &cx.world.arena, cx.history).enabled {
            return;
        }
        let rgb = color::unit(self.drawing_color());
        match command {
            "file.new" => self.dialog = Some(Dialog::New(Self::new_dialog(cx))),
            "file.open" => self.pick_image(cx, false),
            "file.import.image" => self.pick_image(cx, true),
            "file.close" => {
                if let Some(picture) = self.picture() {
                    self.closed.insert(picture);
                    self.docs.retain(|doc| doc.picture != picture);
                    self.active = self.active.min(self.docs.len().saturating_sub(1));
                }
            }
            "file.save" if crate::clip_io::saves_clip(cx) && self.picture().is_some() => {
                self.save_clip(cx);
            }
            "file.save" => {
                cx.request(ShellRequest::Command("file.save".to_owned()));
                self.mark_clean();
                let at = cx.history.cursor();
                for doc in &self.docs {
                    self.saved_at.insert(doc.picture, at);
                }
            }
            "file.revert" => self.revert(cx),
            "file.start_page" => {
                self.start.forced = true;
                self.dialog = None;
            }
            "file.recent.clear" => {
                for path in std::mem::take(&mut self.recent_files) {
                    cx.request(ShellRequest::Forget(path));
                }
            }
            recent if recent.starts_with("file.recent.open.") => {
                let index = recent["file.recent.open.".len()..].parse::<usize>().ok();
                if let Some(path) = index
                    .and_then(|index| self.recent_files.get(index))
                    .cloned()
                {
                    if sue_platform::fs::exists(&path) {
                        self.open_picked(cx, &path, false);
                    } else {
                        cx.fail(tr!("is-gone", path = path.display()));
                        cx.request(ShellRequest::Forget(path));
                    }
                }
            }
            "file.import.vector" | "file.import.movie" | "file.import.pattern" => {
                if self.picture().is_none() {
                    cx.fail(tr!("open-make-canvas-first"));
                    return;
                }
                let (purpose, title, extensions): (&str, &str, &[&str]) = match command {
                    "file.import.vector" => ("import-vector", &tr!("import-vector-2"), &["svg"]),
                    "file.import.movie" => (
                        "import-movie",
                        &tr!("import-movie-2"),
                        &["gif", "png", "apng", "webp", "avi"],
                    ),
                    _ => (
                        "import-pattern",
                        &tr!("pattern-from-image"),
                        &["png", "jpg", "jpeg", "jxl", "webp", "bmp", "tif", "tiff"],
                    ),
                };
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), purpose, title)
                        .extensions(extensions)
                        .starting_in(start),
                ));
            }
            "file.print_settings" => self.dialog = Some(Dialog::Print(self.print, false)),
            "file.print" => self.dialog = Some(Dialog::Print(self.print, true)),
            "file.export.batch" => {
                if self.story_here(&cx.world.arena).is_some() {
                    self.story_command("story.export_pages", cx);
                } else {
                    let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
                    cx.request(ShellRequest::PickFile(
                        FilePick::save(
                            Some(me_id()),
                            "batch",
                            &tr!("batch-export-every-open-canvas"),
                            &tr!("canvas-png"),
                        )
                        .types(
                            sue_io::PICTURE_SAVE_TYPES
                                .iter()
                                .map(|(what, extensions)| FileType::new(what, extensions))
                                .collect(),
                        )
                        .starting_in(Some(start)),
                    ));
                }
            }
            "file.modifiers" => {
                let rice = cx.rice();
                let chosen = crate::dialogs::MODIFIERS.map(|(key, _, choices)| {
                    let now = rice.get_str(key).unwrap_or(choices[0]);
                    choices
                        .iter()
                        .position(|choice| *choice == now)
                        .unwrap_or(0)
                });
                self.dialog = Some(Dialog::Modifiers(chosen));
            }
            "file.command_bar" => {
                self.panels.command_bar = true;
                self.dialog = Some(Dialog::CommandBar(String::new()));
            }
            "help.guide" => {
                self.dialog = Some(Dialog::Message(
                    tr!("sue-paint-guide"),
                    crate::extras::guide(),
                ))
            }
            "help.updates" => {
                self.dialog = Some(Dialog::Message(
                    tr!("check-for-updates"),
                    crate::extras::updates(),
                ))
            }
            "file.export.bmp" => self.export(cx, "bmp"),
            "file.export.webp" => self.export(cx, "webp"),
            "file.duplicate.clip" => {
                if let Some(picture) = self.picture() {
                    match crate::clip_io::encode(&cx.world.arena, picture) {
                        Ok(bytes) => self.export_bytes(cx, bytes, "clip"),
                        Err(error) => cx.fail(tr!("export-failed", error = error)),
                    }
                }
            }
            "file.export.tif" => self.export(cx, "tif"),
            "file.export.tga" => self.export(cx, "tga"),
            "file.export.psd" => self.export_psd(cx, false),
            "file.export.psb" => self.export_psd(cx, true),
            "file.import.layers" => {
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(
                        Some(me_id()),
                        "import-layers",
                        &tr!("import-image-as-separate-layers"),
                    )
                    .extensions(&[
                        "clip", "psd", "psb", "png", "jpg", "jpeg", "jxl", "webp", "tif", "tiff",
                        "bmp",
                    ])
                    .starting_in(start),
                ));
            }
            "file.save_as" => self.save_as(cx),
            "file.export.png" => self.export(cx, "png"),
            "file.export.jpg" => self.export(cx, "jpg"),
            "file.export.jxl" => self.export(cx, "jxl"),
            "file.preferences" => {
                self.dialog = Some(Dialog::Preferences(crate::prefs::Preferences::read(
                    cx.rice(),
                )));
            }
            "file.shortcuts" => cx.request(ShellRequest::OpenSettings(Some("keymaps".to_owned()))),
            "edit.transform.puppet" => self.start_warp(cx),
            "edit.transform.mesh" => {
                self.start_warp(cx);
                // Mesh Transformation: a 4 by 4 grid of points over the
                // layer's pixels, each dragged to bend what is near it.
                if let Some(warp) = self.warp.as_mut()
                    && warp.pins.is_empty()
                    && let Some([x0, y0, x1, y1]) = warp.bounds
                {
                    for row in 0..4 {
                        for column in 0..4 {
                            let at = [
                                x0 as f32 + (x1 - x0) as f32 * column as f32 / 3.0,
                                y0 as f32 + (y1 - y0) as f32 * row as f32 / 3.0,
                            ];
                            warp.pins.push(crate::warp::Pin::at(at));
                        }
                    }
                    cx.notify(tr!("mesh-transformation-drag-points-bend-layer"));
                }
            }
            "edit.transform.scale" | "edit.transform.scale_only" | "edit.transform.rotate" => {
                self.start_transform(cx, false);
            }
            "edit.transform.free"
            | "edit.transform.distort"
            | "edit.transform.skew"
            | "edit.transform.perspective" => {
                self.start_transform(cx, true);
            }
            "file.pressure" => {
                self.dialog = Some(Dialog::Pressure(PressureDialog::new(self.pen_curve)));
            }
            "file.leave" => cx.request(ShellRequest::Hub),
            "app.quit" => cx.request(ShellRequest::Command("app.quit".to_owned())),
            "edit.undo" | "edit.redo" => {
                cx.typing.flush(cx.history);
                let result = if command == "edit.undo" {
                    cx.history.undo(cx.world)
                } else {
                    cx.history.redo(cx.world)
                };
                if let Err(error) = result {
                    cx.fail(tr!("undo-failed", error = error));
                }
            }
            "edit.copy" => {
                if self.copy(cx) {
                    cx.notify(tr!("copied"));
                }
            }
            "edit.cut" => {
                if self.copy(cx) {
                    self.run_clear(cx, false, &tr!("cut"));
                }
            }
            "edit.paste" => self.paste(cx, &tr!("pasted")),
            "layer.from_selection" => {
                let kept = self.clipboard.take();
                if self.copy(cx) {
                    self.paste(cx, &tr!("from-selection"));
                }
                self.clipboard = kept;
            }
            "edit.clear"
                if self.toolbox.tool() == crate::tools::Tool::Operation
                    && self
                        .picked_lines
                        .as_ref()
                        .is_some_and(|(_, picked)| !picked.is_empty()) =>
            {
                self.delete_picked_lines(cx);
            }
            "edit.clear" => self.run_clear(cx, false, &tr!("clear")),
            "edit.clear_outside" => self.run_clear(cx, true, &tr!("clear-outside-selection-2")),
            "edit.fill" => {
                let Some(selection) = self.selection_or_all(&cx.world.arena) else {
                    return;
                };
                let size = selection.size();
                let alpha_lock = self
                    .layer_info(&cx.world.arena)
                    .is_some_and(|info| info.alpha_locked);
                let offset = self
                    .layer_info(&cx.world.arena)
                    .map_or([0, 0], |info| info.style.offset);
                let mask: Vec<u8> = (0..size[1])
                    .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
                    .map(|(x, y)| (selection.at(x, y) * 255.0).round() as u8)
                    .collect();
                self.rewrite(cx, &tr!("fill"), |pixels, width, height| {
                    crate::fill::paint_mask(
                        pixels,
                        [width, height],
                        offset,
                        &mask,
                        size,
                        rgb,
                        1.0,
                        alpha_lock,
                    );
                });
            }
            "edit.clear_history" => {
                // Edit, Clear work history: this canvas's steps only, as
                // each canvas keeps its own.
                cx.typing.flush(cx.history);
                *cx.history = sue_core::UndoStack::new();
                cx.notify(tr!("work-history-cleared"));
            }
            "edit.outline_selection" => {
                if self.doc().is_some_and(|doc| doc.selection.is_some()) {
                    self.dialog = Some(Dialog::SelectBy(3, 4.0));
                } else {
                    cx.fail(tr!("select-area-keep-first"));
                }
            }
            "edit.recolor" => self.filter_now(
                cx,
                Filter::Recolor {
                    rgb: self.drawing_color(),
                },
            ),
            "layer.new.raster_top" => {
                self.run("layer.new", cx);
            }
            // A canvas the size of what was copied, with it pasted in.
            "file.new_clipboard" => {
                let Some(clip) = self.clipboard.clone() else {
                    return;
                };
                let spec = NewPicture {
                    name: tr!("illustration"),
                    width: clip.raster.width(),
                    height: clip.raster.height(),
                    dpi: 72.0,
                    paper: Some(sue_core::Color::srgb8(255, 255, 255)),
                    timelapse: false,
                };
                self.make_picture(cx, &spec);
                self.clipboard = Some(Clip {
                    raster: clip.raster.clone(),
                    origin: [0, 0],
                });
                self.paste(cx, &tr!("pasted"));
                self.clipboard = Some(clip);
            }
            // Clip Studio Paint's Change canvas height: the canvas's size,
            // for a webtoon's length.
            "edit.canvas_height" => self.run("edit.canvas_size", cx),
            "window.brush_grid" => {
                self.panels.sub_tool = true;
                self.tool_tab = 2;
            }
            align if align.starts_with("edit.align.") => {
                self.align_layer(&align["edit.align.".len()..], cx);
            }
            "edit.luma_to_alpha" => self.filter_now(
                cx,
                Filter::BrightnessToOpacity {
                    rgb: self.drawing_color(),
                },
            ),
            "edit.flip_h" => self.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
                mirror(p, w, h, true, false)
            }),
            "edit.flip_v" => self.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
                mirror(p, w, h, false, true)
            }),
            "edit.rotate180" => {
                self.rewrite(cx, &tr!("rotate-layer"), |p, w, h| {
                    mirror(p, w, h, true, true)
                });
            }
            "canvas.flip_h" => self.transform_canvas(cx, true, false, &tr!("flip-canvas")),
            "canvas.flip_v" => self.transform_canvas(cx, false, true, &tr!("flip-canvas")),
            "canvas.rotate180" => self.transform_canvas(cx, true, true, &tr!("rotate-canvas")),
            "filter.invert" => self.filter_now(cx, Filter::Invert),
            "filter.blur" => self.filter_now(cx, Filter::GaussianBlur { radius: 1.0 }),
            "filter.blur_strong" => self.filter_now(cx, Filter::GaussianBlur { radius: 3.0 }),
            "filter.sharpen" => self.filter_now(cx, Filter::Sharpen { amount: 0.8 }),
            "filter.sharpen_strong" => self.filter_now(cx, Filter::Sharpen { amount: 2.0 }),
            "filter.brightness" => self.filter_dialog(
                cx,
                Filter::BrightnessContrast {
                    brightness: 0.0,
                    contrast: 0.0,
                },
            ),
            "filter.hue" => self.filter_dialog(
                cx,
                Filter::HueSaturation {
                    hue: 0.0,
                    saturation: 0.0,
                    luminosity: 0.0,
                },
            ),
            "filter.posterize" => self.filter_dialog(cx, Filter::Posterize { levels: 4 }),
            "filter.binarize" => self.filter_dialog(cx, Filter::Binarize { threshold: 0.5 }),
            "filter.gaussian" => self.filter_dialog(cx, Filter::GaussianBlur { radius: 4.0 }),
            "filter.mosaic" => self.filter_dialog(cx, Filter::Mosaic { size: 12 }),
            "filter.line_extraction" => self.filter_dialog(
                cx,
                Filter::LineExtraction {
                    sensitivity: 0.5,
                    black: 0.12,
                    smooth: 1.0,
                },
            ),
            "filter.lines_tones" => self.lines_and_tones(cx),
            "filter.motion_blur" => self.filter_dialog(
                cx,
                Filter::MotionBlur {
                    angle: 0.0,
                    distance: 20.0,
                },
            ),
            "filter.radial_blur" => self.filter_dialog(cx, Filter::RadialBlur { angle: 10.0 }),
            "filter.zoom_blur" => self.filter_dialog(cx, Filter::ZoomBlur { amount: 0.3 }),
            "filter.smooth" => self.filter_now(cx, Filter::Smooth),
            "filter.dust" => self.filter_dialog(cx, Filter::Dust { size: 10 }),
            "filter.line_width" => self.filter_dialog(cx, Filter::LineWidth { by: 1.0 }),
            "filter.wave" => self.filter_dialog(
                cx,
                Filter::Wave {
                    amplitude: 8.0,
                    wavelength: 80.0,
                },
            ),
            "filter.pinch" => self.filter_dialog(cx, Filter::Pinch { amount: 0.5 }),
            "filter.polar" => self.filter_dialog(cx, Filter::Polar { inverse: false }),
            "filter.ripple" => self.filter_dialog(
                cx,
                Filter::Ripple {
                    amplitude: 6.0,
                    wavelength: 40.0,
                },
            ),
            "filter.fisheye" => self.filter_dialog(cx, Filter::Fisheye { amount: 0.6 }),
            "filter.twirl" => self.filter_dialog(cx, Filter::Twirl { angle: 180.0 }),
            "filter.zigzag" => self.filter_dialog(
                cx,
                Filter::ZigZag {
                    amplitude: 10.0,
                    folds: 5.0,
                },
            ),
            "filter.crystallize" => self.filter_dialog(cx, Filter::Crystallize { size: 20 }),
            "filter.lens_blur" => self.filter_dialog(cx, Filter::LensBlur { radius: 8 }),
            "filter.jpeg_noise" => self.filter_dialog(cx, Filter::JpegNoise { strength: 0.6 }),
            "filter.aberration" => self.filter_dialog(cx, Filter::Aberration { shift: 6.0 }),
            "filter.retro" => self.filter_dialog(cx, Filter::Retro { amount: 0.6 }),
            "filter.perlin" => self.filter_dialog(
                cx,
                Filter::PerlinNoise {
                    scale: 60.0,
                    contrast: 1.0,
                },
            ),
            "filter.clouds" => self.filter_now(
                cx,
                Filter::Clouds {
                    from: self.main,
                    to: self.sub,
                },
            ),
            "filter.unsharp" => self.filter_dialog(
                cx,
                Filter::Unsharp {
                    radius: 3.0,
                    amount: 1.0,
                    threshold: 0.0,
                },
            ),
            "filter.noise" => self.filter_dialog(cx, Filter::Noise { amount: 0.2 }),
            "filter.curve" => self.filter_dialog(
                cx,
                Filter::ToneCurve {
                    rgb: sue_model::Response::LINEAR,
                    red: sue_model::Response::LINEAR,
                    green: sue_model::Response::LINEAR,
                    blue: sue_model::Response::LINEAR,
                    channel: 0,
                },
            ),
            "filter.levels" => self.filter_dialog(
                cx,
                Filter::Levels {
                    black: 0.0,
                    white: 1.0,
                    gamma: 1.0,
                    out_black: 0.0,
                    out_white: 1.0,
                },
            ),
            "filter.balance" => self.filter_dialog(
                cx,
                Filter::ColorBalance {
                    shifts: [[0.0; 3]; 3],
                    range: 1,
                },
            ),
            "filter.gradient_map" => {
                let (from, to) = (self.main, self.sub);
                self.filter_dialog(cx, Filter::GradientMap { from, to });
            }
            "filter.focus_lines" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::FocusLines {
                        count: 160,
                        gap: 0.45,
                        width: 5.0,
                        jitter: 0.4,
                        seed: 1,
                        rgb,
                    },
                );
            }
            "filter.speed_lines" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::SpeedLines {
                        count: 90,
                        angle: 0.0,
                        width: 4.0,
                        jitter: 0.5,
                        seed: 1,
                        rgb,
                    },
                );
            }
            "filter.tone" => {
                let rgb = self.drawing_color();
                self.filter_dialog(
                    cx,
                    Filter::Tone {
                        density: 0.2,
                        cell: 8.0,
                        angle: 45.0,
                        rgb,
                    },
                );
            }
            "layer.new.tone" => {
                // A new layer of tone, in the selection if there is one.
                self.run("layer.new", cx);
                self.run("filter.tone", cx);
            }
            "layer.new.frame" => {
                self.dialog = Some(Dialog::Frame(crate::comic::FrameSpec::default()));
            }
            "layer.new.balloon" => self.add_balloon_layer(cx),
            command if command.starts_with("layer.correction.") => {
                let filter = match command.trim_start_matches("layer.correction.") {
                    "brightness" => Filter::BrightnessContrast {
                        brightness: 0.0,
                        contrast: 0.0,
                    },
                    "levels" => Filter::Levels {
                        black: 0.0,
                        white: 1.0,
                        gamma: 1.0,
                        out_black: 0.0,
                        out_white: 1.0,
                    },
                    "curve" => Filter::ToneCurve {
                        rgb: sue_model::Response::LINEAR,
                        red: sue_model::Response::LINEAR,
                        green: sue_model::Response::LINEAR,
                        blue: sue_model::Response::LINEAR,
                        channel: 0,
                    },
                    "hue" => Filter::HueSaturation {
                        hue: 0.0,
                        saturation: 0.0,
                        luminosity: 0.0,
                    },
                    "balance" => Filter::ColorBalance {
                        shifts: [[0.0; 3]; 3],
                        range: 1,
                    },
                    "invert" => Filter::Invert,
                    "posterize" => Filter::Posterize { levels: 4 },
                    "binarize" => Filter::Binarize { threshold: 0.5 },
                    "gradient_map" => Filter::GradientMap {
                        from: self.main,
                        to: self.sub,
                    },
                    _ => {
                        cx.notify(tr!("correction-not-sue-paint-yet"));
                        return;
                    }
                };
                let Some(picture) = self.picture() else {
                    return;
                };
                let text = filter.to_text().unwrap_or_default();
                let (edit, layer) = doc::new_correction(
                    &cx.world.arena,
                    picture,
                    self.layer(),
                    &filter.label(),
                    &text,
                );
                if Self::apply(cx, edit) {
                    if let Some(doc) = self.doc_mut() {
                        doc.layer = Some(layer);
                    }
                    if filter != Filter::Invert {
                        self.dialog = Some(Dialog::Correction(layer, filter, text, true));
                    }
                }
            }
            "layer.new.text" => {
                let middle = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(picture::size)
                    .map_or([0.0, 0.0], |[w, h]| [w as f32 / 2.0, h as f32 / 2.0]);
                let mut lettering = crate::lettering::Lettering::new(middle, self.drawing_color());
                lettering.center = true;
                self.dialog = Some(Dialog::Text(None, lettering));
            }
            "view.snap_special" => {
                if let Some(doc) = self.doc_mut() {
                    doc.ruler_on = !doc.ruler_on;
                }
            }
            "view.snap_ruler" => {
                if let Some(doc) = self.doc_mut() {
                    doc.snap_ruler = !doc.snap_ruler;
                }
            }
            "file.timelapse.record" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let on = self.recording(&cx.world.arena, picture);
                let edit = doc::Edit::new(if on {
                    tr!("stop-recording-timelapse")
                } else {
                    tr!("record-timelapse-2")
                })
                .then(SetProperty::new(picture, crate::timelapse::RECORD, !on));
                Self::apply(cx, edit);
            }
            "file.timelapse.export" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Timelapse(self.timelapse_export));
                }
            }
            "layer.ruler.show" => {
                if let Some(doc) = self.doc_mut() {
                    doc.show_rulers = !doc.show_rulers;
                }
            }
            "layer.ruler.perspective" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Perspective(2));
                }
            }
            "layer.ruler.symmetry" | "layer.ruler.circle" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let Some(size) = cx
                    .world
                    .arena
                    .get_by_id(picture)
                    .ok()
                    .and_then(picture::size)
                else {
                    return;
                };
                let middle = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
                let (key, text, label, mode) = if command == "layer.ruler.symmetry" {
                    let rulers = crate::rulers::Rulers {
                        symmetry: Some((middle, std::f32::consts::FRAC_PI_2)),
                        ..crate::rulers::Rulers::default()
                    };
                    (
                        crate::rulers::SYMMETRY,
                        rulers.text(crate::rulers::SYMMETRY),
                        tr!("create-symmetrical-ruler"),
                        0,
                    )
                } else {
                    let special = crate::rulers::Special::Concentric(middle);
                    (
                        crate::rulers::SPECIAL,
                        Some(special.to_text()),
                        tr!("create-concentric-circle-ruler"),
                        2,
                    )
                };
                if Self::apply(cx, crate::rulers::edit(picture, key, text, &label)) {
                    self.pick_ruler(mode);
                    if mode == 2 {
                        self.toolbox.current_mut().kind = 2;
                    }
                    if let Some(doc) = self.doc_mut() {
                        doc.ruler_on = true;
                        doc.show_rulers = true;
                    }
                }
            }
            "layer.ruler.delete" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
                let mut edit = doc::Edit::new(tr!("delete-ruler"));
                for key in crate::rulers::ALL {
                    if rulers.text(key).is_some() {
                        edit = edit.then(SetProperty::remove(picture, key));
                    }
                }
                self.vanishing_line = None;
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            "edit.material.paste" => {
                let library = self
                    .material_kinds
                    .scan(crate::materials::location(cx).as_deref());
                if let Some(material) = library
                    .into_iter()
                    .find(|material| self.material_pick.as_deref() == Some(material.name.as_str()))
                {
                    self.paste_material(cx, &material);
                } else {
                    cx.notify(tr!("pick-material-material-palette-first"));
                }
            }
            "edit.material.image" => {
                let name = self
                    .layer_info(&cx.world.arena)
                    .map_or_else(|| tr!("material"), |info| info.name);
                if self.paintable(&cx.world.arena).is_ok() {
                    self.dialog = Some(Dialog::Material(name));
                } else {
                    cx.fail(tr!("pick-raster-layer-register-as-material"));
                }
            }
            "edit.canvas_size" | "edit.resolution" => {
                let Some(old) = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(|entity| {
                        let dpi = entity
                            .property(picture::DPI)
                            .and_then(Value::as_f64)
                            .unwrap_or(72.0);
                        picture::size(entity).map(|size| (size, dpi))
                    })
                else {
                    return;
                };
                let (size, dpi) = old;
                self.dialog = Some(if command == "edit.canvas_size" {
                    Dialog::Canvas(crate::dialogs::CanvasDialog {
                        size,
                        old: size,
                        anchor: 4,
                    })
                } else {
                    Dialog::Resolution(crate::dialogs::ResolutionDialog {
                        size,
                        old: size,
                        dpi,
                        keep: true,
                    })
                });
            }
            "edit.crop" => {
                let (Some(picture), Some(bounds)) = (
                    self.picture(),
                    self.doc()
                        .and_then(|doc| doc.selection.as_ref())
                        .and_then(Selection::bounds),
                ) else {
                    cx.fail(tr!("select-area-keep-first"));
                    return;
                };
                let [x0, y0, x1, y1] = bounds;
                match doc::resize_canvas(
                    &cx.world.arena,
                    picture,
                    [x1 - x0, y1 - y0],
                    [-i64::from(x0), -i64::from(y0)],
                ) {
                    Ok(edit) => {
                        let edit = sue_core::txn::Batch::new(tr!("crop")).then(edit);
                        Self::apply(cx, edit);
                        if let Some(doc) = self.doc_mut() {
                            doc.set_selection(None);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.new" | "layer.new.fill" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let name = if command == "layer.new.fill" {
                    tr!("fill")
                } else {
                    doc::new_layer_name(arena, picture)
                };
                let fill = (command == "layer.new.fill").then(|| {
                    let rgb = self.drawing_color();
                    [rgb[0], rgb[1], rgb[2], 255]
                });
                match doc::add_raster_layer(arena, picture, self.layer(), name, fill) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, spawn)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.new.vector" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let name = doc::new_layer_name(&cx.world.arena, picture);
                match doc::add_vector_layer(&cx.world.arena, picture, self.layer(), name) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, spawn)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.new.paper" => {
                if let Some(picture) = self.picture() {
                    let (spawn, _) = doc::add_paper(
                        &cx.world.arena,
                        picture,
                        sue_core::Color::srgb8(255, 255, 255),
                    );
                    Self::apply(cx, spawn);
                }
            }
            "layer.duplicate" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::duplicate(&cx.world.arena, picture, layer) {
                        Ok((spawn, id)) => {
                            if Self::apply(cx, spawn)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(id);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.delete" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    if doc::is_folder(&cx.world.arena, layer) {
                        let edit = doc::delete_folder(&cx.world.arena, picture, layer);
                        Self::apply(cx, edit);
                        return;
                    }
                    if doc::layers(&cx.world.arena, picture).len() <= 1 {
                        cx.fail(tr!("picture-keeps-least-one-layer"));
                        return;
                    }
                    Self::apply(cx, doc::delete(layer));
                }
            }
            "layer.toggle.visible" => self.toggle_layer(cx, picture::VISIBLE, true),
            "file.export.pdf" | "file.export.epub" => {
                self.ask_export_book(cx, command == "file.export.epub")
            }

            "file.export.webtoon" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Webtoon(self.webtoon.0, self.webtoon.1));
                }
            }
            "layer.new.3d" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Shape3D(0));
                }
            }
            "file.import.audio" => {
                if self
                    .picture()
                    .and_then(|picture| crate::anim::timeline(&cx.world.arena, picture))
                    .is_none()
                {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                    return;
                }
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), "anim-audio", &tr!("import-audio-2"))
                        .extensions(&["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"])
                        .starting_in(start),
                ));
            }
            "file.import.3d" => {
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), "model3d", &tr!("import-3d-data"))
                        .extensions(&["obj"])
                        .starting_in(start),
                ));
            }
            "layer.rasterize" => {
                let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
                    return;
                };
                match doc::rasterize(&cx.world.arena, picture, layer) {
                    Ok((edit, id)) => {
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.toggle.reference" => {
                if let Some(layer) = self.layer() {
                    let on = doc::is_reference(&cx.world.arena, layer);
                    Self::apply(cx, doc::set(layer, doc::REFERENCE, !on));
                }
            }
            "layer.toggle.clip" => self.toggle_layer(cx, picture::CLIP, false),
            "layer.mask.outside" | "layer.mask.inside" => {
                let Some(layer) = self.layer() else {
                    return;
                };
                let selection = self.doc().and_then(|doc| doc.selection.clone());
                if command == "layer.mask.inside" && selection.is_none() {
                    cx.fail(tr!("select-what-to-hide-first"));
                    return;
                }
                let inside = command == "layer.mask.inside";
                let show = |x: i64, y: i64| {
                    let cover = selection.as_ref().map_or(1.0, |selection| {
                        if x < 0 || y < 0 {
                            0.0
                        } else {
                            selection.at(x as u32, y as u32)
                        }
                    });
                    if inside { 1.0 - cover } else { cover }
                };
                match doc::new_mask(&cx.world.arena, layer, show) {
                    Ok((edit, _)) => {
                        Self::apply(cx, edit);
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.mask.delete" => {
                if let Some(edit) = self
                    .layer()
                    .and_then(|layer| doc::delete_mask(&cx.world.arena, layer))
                {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.mask_edit = false;
                    }
                }
            }
            "layer.mask.apply" => {
                if let Some(layer) = self.layer() {
                    match doc::apply_mask(&cx.world.arena, layer) {
                        Ok(edit) => {
                            Self::apply(cx, edit);
                            if let Some(doc) = self.doc_mut() {
                                doc.mask_edit = false;
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.mask.toggle" => {
                if let Some(layer) = self.layer() {
                    let on = doc::mask_on(&cx.world.arena, layer);
                    Self::apply(cx, doc::set(layer, doc::MASK_ON, !on));
                }
            }
            "layer.mask.edit" => {
                let masked = self
                    .layer()
                    .is_some_and(|layer| doc::mask_of(&cx.world.arena, layer).is_some());
                if let Some(doc) = self.doc_mut() {
                    doc.mask_edit = masked && !doc.mask_edit;
                }
            }
            "layer.toggle.draft" => self.toggle_layer(cx, picture::DRAFT, false),
            "layer.toggle.lock" => self.toggle_layer(cx, picture::LOCKED, false),
            "layer.toggle.lock_alpha" => self.toggle_layer(cx, picture::ALPHA_LOCKED, false),
            "layer.properties" | "window.layer_property" => {
                self.panels.layer = true;
                self.layer_tab = 1;
            }
            "window.history" => {
                self.panels.layer = true;
                self.layer_tab = 2;
            }
            "layer.select_above" | "layer.select_below" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let stack = doc::layers(&cx.world.arena, picture);
                let at = self
                    .layer()
                    .and_then(|layer| stack.iter().position(|info| info.id == layer));
                let next = match (at, command == "layer.select_above") {
                    (Some(at), true) => stack.get(at + 1),
                    (Some(at), false) => at.checked_sub(1).and_then(|at| stack.get(at)),
                    (None, _) => stack.last(),
                };
                if let (Some(next), Some(doc)) = (next.map(|info| info.id), self.doc_mut()) {
                    doc.layer = Some(next);
                }
            }
            "layer.folder_insert" | "layer.new.folder" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let layer = self
                    .layer()
                    .filter(|layer| !doc::is_folder(&cx.world.arena, *layer));
                let (edit, folder) = doc::new_folder(
                    &cx.world.arena,
                    picture,
                    layer.or(self.layer()),
                    command == "layer.folder_insert",
                );
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(folder);
                }
            }
            "layer.order.up" | "layer.order.down"
                if self.move_in_folders(cx, command == "layer.order.up") => {}
            "layer.order.front" | "layer.order.up" | "layer.order.down" | "layer.order.back" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let stack = doc::layers(&cx.world.arena, picture);
                let Some(at) = self
                    .layer()
                    .and_then(|layer| stack.iter().position(|info| info.id == layer))
                else {
                    return;
                };
                let last = stack.len().saturating_sub(1);
                let to = match command {
                    "layer.order.front" => last,
                    "layer.order.up" => (at + 1).min(last),
                    "layer.order.down" => at.saturating_sub(1),
                    _ => 0,
                };
                self.move_layer_to(cx, to);
            }
            "layer.merge_down" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::merge_down(&cx.world.arena, picture, layer) {
                        Ok((edit, into)) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(into);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.ungroup" => {
                let (Some(picture), Some(folder)) = (self.picture(), self.layer()) else {
                    return;
                };
                let arena = &cx.world.arena;
                if !doc::is_folder(arena, folder) {
                    cx.fail(tr!("pick-a-layer-folder-first"));
                    return;
                }
                // Its layers go where it was; then it goes.
                let outer = doc::folder_of(arena, folder);
                let mut edit = doc::Edit::new(tr!("ungroup-layer-folder"));
                for layer in doc::layers(arena, picture) {
                    if doc::folder_of(arena, layer.id) == Some(folder)
                        && let Some(moved) = doc::set_folder(arena, layer.id, outer)
                    {
                        edit = edit.then(moved);
                    }
                }
                edit = edit.then(sue_core::txn::DespawnEntity::new(folder));
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = None;
                }
            }
            "layer.delete_empty" | "layer.delete_hidden" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let stack = doc::layers(arena, picture);
                let gone: Vec<EntityId> = stack
                    .iter()
                    .filter(|info| {
                        info.kind == LayerKind::Raster && !doc::is_folder(arena, info.id)
                    })
                    .filter(|info| {
                        if command == "layer.delete_hidden" {
                            !info.style.visible
                        } else {
                            doc::raster(arena, info.id).is_some_and(|raster| {
                                raster.bytes().is_some_and(|bytes| {
                                    bytes.iter().skip(3).step_by(4).all(|a| *a == 0)
                                })
                            })
                        }
                    })
                    .map(|info| info.id)
                    .collect();
                if gone.is_empty() {
                    cx.notify(tr!("no-such-layers"));
                    return;
                }
                if gone.len() >= stack.len() {
                    cx.fail(tr!("picture-keeps-least-one-layer"));
                    return;
                }
                let mut edit = doc::Edit::new(if command == "layer.delete_hidden" {
                    tr!("hidden-layers")
                } else {
                    tr!("empty-layers")
                });
                for id in gone {
                    edit = edit.then(sue_core::txn::DespawnEntity::new(id));
                }
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = None;
                }
            }
            "layer.hide_drafts" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let mut edit = doc::Edit::new(tr!("hide-all-visible-draft-layers"));
                for info in doc::layers(&cx.world.arena, picture) {
                    if info.style.draft && info.style.visible {
                        edit = edit.then(SetProperty::new(info.id, picture::VISIBLE, false));
                    }
                }
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            "layer.rename" => {
                if let Some(info) = self.layer_info(&cx.world.arena) {
                    self.panels.layer = true;
                    self.layer_tab = 0;
                    self.renaming = Some((info.id, info.name));
                }
            }
            "layer.merge_visible_here" => {
                // Merge Visible Layers: the visible layers become one, the
                // hidden ones stay.
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                match doc::merge_visible(arena, picture, false) {
                    Ok((merged, id)) => {
                        let mut edit = doc::Edit::new(tr!("merge-visible-layers")).then(merged);
                        for info in doc::layers(arena, picture) {
                            if info.style.visible && !doc::is_folder(arena, info.id) {
                                edit = edit.then(sue_core::txn::DespawnEntity::new(info.id));
                            }
                        }
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "view.rotate_90" | "view.rotate_180" | "view.rotate_270" => {
                let angle = match command {
                    "view.rotate_90" => 90.0,
                    "view.rotate_180" => 180.0,
                    _ => 270.0,
                };
                self.with_view(cx, |view, rect, size| view.set_rotation(rect, size, angle));
            }
            "view.reset_turn" | "view.reset_display" => {
                let fit = command == "view.reset_display";
                self.with_view(cx, |view, rect, size| {
                    view.set_rotation(rect, size, 0.0);
                    if view.flip_h {
                        view.flip(rect, size, true);
                    }
                    if view.flip_v {
                        view.flip(rect, size, false);
                    }
                    if fit {
                        view.fit(rect, size);
                    }
                });
            }
            "view.paper" => {
                let paper = self.picture().and_then(|picture| {
                    doc::layers(&cx.world.arena, picture)
                        .into_iter()
                        .find(|info| info.kind == LayerKind::Paper)
                });
                if let Some(paper) = paper {
                    Self::apply(
                        cx,
                        doc::set(paper.id, picture::VISIBLE, !paper.style.visible),
                    );
                }
            }
            "layer.transfer_down" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::transfer_down(&cx.world.arena, picture, layer) {
                        Ok((edit, _)) => {
                            Self::apply(cx, edit);
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.merge_visible" | "layer.flatten" => {
                if let Some(picture) = self.picture() {
                    match doc::merge_visible(&cx.world.arena, picture, command == "layer.flatten") {
                        Ok((edit, id)) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(id);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "select.all" => {
                let all = self.doc().and_then(|doc| {
                    let [w, h] = picture::size(cx.world.arena.get_by_id(doc.picture).ok()?)?;
                    Some(Selection::all(w, h))
                });
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(all);
                }
            }
            "select.none" => {
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(None);
                }
            }
            "select.again" => {
                if let Some(doc) = self.doc_mut() {
                    let again = doc.last_selection.clone();
                    doc.set_selection(again);
                }
            }
            "select.invert" => {
                if let Some(doc) = self.doc_mut()
                    && let Some(mut selection) = doc.selection.clone()
                {
                    selection.invert();
                    doc.set_selection(Some(selection));
                }
            }
            "select.expand" | "select.shrink" | "select.feather" => {
                let kind = match command {
                    "select.expand" => 0,
                    "select.shrink" => 1,
                    _ => 2,
                };
                if self.doc().is_some_and(|doc| doc.selection.is_some()) {
                    self.dialog = Some(Dialog::SelectBy(kind, 4.0));
                }
            }
            "select.gamut" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Gamut(0.1, false));
                }
            }
            "select.add_layer" | "select.subtract_layer" => {
                let Some(raster) = self
                    .layer()
                    .and_then(|layer| doc::raster(&cx.world.arena, layer))
                else {
                    return;
                };
                let from = Selection::from_alpha(raster);
                let op = if command == "select.add_layer" {
                    Op::Add
                } else {
                    Op::Subtract
                };
                if let Some(doc) = self.doc_mut() {
                    let [width, height] = from.size();
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(width, height));
                    selection.apply_mask(from.mask(), op);
                    doc.set_selection(Some(selection));
                }
            }
            "select.to_layer" => {
                let (Some(picture), Some(selection)) = (
                    self.picture(),
                    self.doc().and_then(|doc| doc.selection.clone()),
                ) else {
                    return;
                };
                // A selection layer: the selection in green, left out of
                // exports; Add Selection takes it back.
                let [width, height] = selection.size();
                let pixels: Vec<u8> = selection
                    .mask()
                    .iter()
                    .flat_map(|alpha| [60, 200, 90, *alpha])
                    .collect();
                let Ok(raster) = sue_core::RasterTile2D::from_bytes(
                    width,
                    height,
                    sue_core::PixelFormat::Rgba8,
                    pixels,
                ) else {
                    return;
                };
                match doc::add_layer_pixels(
                    &cx.world.arena,
                    picture,
                    self.layer(),
                    tr!("selection"),
                    raster,
                ) {
                    Ok((spawn, id)) => {
                        let edit = doc::Edit::new(tr!("convert-to-selection-layer"))
                            .then(spawn)
                            .then(SetProperty::new(id, SELECTION_LAYER, true))
                            .then(SetProperty::new(id, picture::DRAFT, true))
                            .then(SetProperty::new(
                                id,
                                picture::OPACITY,
                                cx.rice()
                                    .get_i64("sue_paint.selection_layer_opacity")
                                    .unwrap_or(50)
                                    .clamp(0, 100) as f64
                                    / 100.0,
                            ));
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "select.quick_mask" => self.toggle_quick_mask(cx),
            "select.from_selection_layer" => {
                if let Some(raster) = self
                    .layer()
                    .and_then(|layer| doc::raster(&cx.world.arena, layer))
                {
                    let selection = Selection::from_alpha(raster);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(Some(selection));
                    }
                }
            }
            "canvas.rotate_left" | "canvas.rotate_right" => {
                if let Some(picture) = self.picture() {
                    match doc::rotate_canvas(
                        &cx.world.arena,
                        picture,
                        command == "canvas.rotate_right",
                    ) {
                        Ok(edit) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.set_selection(None);
                                doc.view.fitted = false;
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "edit.paste_here" => {
                let (Some(doc), Some(clip)) = (self.doc(), self.clipboard.as_ref()) else {
                    return;
                };
                let size = cx
                    .world
                    .arena
                    .get_by_id(doc.picture)
                    .ok()
                    .and_then(picture::size)
                    .unwrap_or([1, 1]);
                let center = doc
                    .view
                    .to_picture(self.viewport, size, self.viewport.center());
                let origin = [
                    (center.x - clip.raster.width() as f32 / 2.0)
                        .round()
                        .max(0.0) as u32,
                    (center.y - clip.raster.height() as f32 / 2.0)
                        .round()
                        .max(0.0) as u32,
                ];
                let saved = clip.origin;
                if let Some(clip) = self.clipboard.as_mut() {
                    clip.origin = origin;
                }
                self.paste(cx, &tr!("pasted"));
                if let Some(clip) = self.clipboard.as_mut() {
                    clip.origin = saved;
                }
            }
            "edit.canvas_properties" => {
                if let Some(picture) = self.picture() {
                    let arena = &cx.world.arena;
                    let entity = arena.get_by_id(picture).ok();
                    let paper = doc::layers(arena, picture)
                        .into_iter()
                        .find(|info| info.kind == LayerKind::Paper);
                    let paper_color = paper
                        .as_ref()
                        .and_then(|info| arena.get_by_id(info.id).ok())
                        .map(|entity| {
                            let rgb = picture::paper_color(entity)
                                .to_space(sue_core::ColorSpace::Srgb)
                                .rgb;
                            rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
                        });
                    self.dialog =
                        Some(Dialog::CanvasProperties(crate::dialogs::CanvasProperties {
                            picture,
                            name: entity
                                .map(|entity| entity.name().to_owned())
                                .unwrap_or_default(),
                            dpi: entity
                                .and_then(|entity| entity.property(picture::DPI))
                                .and_then(Value::as_f64)
                                .unwrap_or(72.0),
                            paper: paper.as_ref().map(|info| info.id),
                            paper_color: paper_color.unwrap_or([255, 255, 255]),
                            paper_shown: paper.as_ref().is_some_and(|info| info.style.visible),
                        }));
                }
            }
            "edit.brush_tip" | "edit.material.tip" => self.make_brush_tip(cx),
            "layer.new.gradient" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size)
                else {
                    return;
                };
                let (from, to) = (self.main, self.sub);
                let mut pixels = Vec::with_capacity((width * height * 4) as usize);
                for y in 0..height {
                    let t = if height > 1 {
                        y as f32 / (height - 1) as f32
                    } else {
                        0.0
                    };
                    let mix = |a: u8, b: u8| {
                        (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
                    };
                    let row = [
                        mix(from[0], to[0]),
                        mix(from[1], to[1]),
                        mix(from[2], to[2]),
                        255,
                    ];
                    for _ in 0..width {
                        pixels.extend_from_slice(&row);
                    }
                }
                let Ok(raster) =
                    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
                else {
                    return;
                };
                match doc::add_layer_pixels(arena, picture, self.layer(), tr!("gradient"), raster) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, doc::Edit::new(tr!("new-gradient-layer")).then(spawn))
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.merge_selected" => {
                let (Some(picture), Some(doc)) = (self.picture(), self.doc()) else {
                    return;
                };
                let mut ids = doc.picked.clone();
                ids.extend(doc.layer);
                match doc::merge_layers(&cx.world.arena, picture, &ids) {
                    Ok((edit, into)) => {
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(into);
                            doc.picked.clear();
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.convert" => {
                if let Some(info) = self.layer_info(&cx.world.arena) {
                    let vector = crate::vector::is_vector(&cx.world.arena, info.id);
                    self.dialog = Some(Dialog::Convert(info.id, u8::from(!vector), info.name));
                }
            }
            "select.launcher" => self.launcher = !self.launcher,
            "select.from_layer" => {
                let arena = &cx.world.arena;
                let Some(info) = self.layer_info(arena) else {
                    return;
                };
                let Some(size) = self
                    .picture()
                    .and_then(|picture| arena.get_by_id(picture).ok())
                    .and_then(picture::size)
                else {
                    return;
                };
                let selection = doc::raster(arena, info.id).map(|raster| {
                    let alpha = Selection::from_alpha(raster);
                    let mut mask = vec![0_u8; size[0] as usize * size[1] as usize];
                    let [ox, oy] = info.style.offset;
                    for y in 0..raster.height() {
                        for x in 0..raster.width() {
                            let (px, py) = (i64::from(x) + ox, i64::from(y) + oy);
                            if px >= 0
                                && py >= 0
                                && px < i64::from(size[0])
                                && py < i64::from(size[1])
                            {
                                mask[(py as u32 * size[0] + px as u32) as usize] =
                                    (alpha.at(x, y) * 255.0).round() as u8;
                            }
                        }
                    }
                    Selection::from_mask(size[0], size[1], mask)
                });
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(selection.flatten());
                }
            }
            "view.rotate_left" | "view.rotate_right" => {
                // Preferences, Canvas: the angle each turn takes.
                let step = cx
                    .rice()
                    .get_i64("sue_paint.rotate_step")
                    .unwrap_or(15)
                    .clamp(1, 90) as f32;
                let step = if command == "view.rotate_left" {
                    -step
                } else {
                    step
                };
                self.with_view(cx, |view, rect, size| view.rotate_by(rect, size, step));
            }
            "view.rotate_reset" => {
                self.with_view(cx, |view, rect, size| view.set_rotation(rect, size, 0.0))
            }
            "view.flip_h" => self.with_view(cx, |view, rect, size| view.flip(rect, size, true)),
            "view.flip_v" => self.with_view(cx, |view, rect, size| view.flip(rect, size, false)),
            "view.zoom_in" => self.with_view(cx, |view, rect, size| {
                view.zoom_at(rect, size, rect.center(), 1.25);
            }),
            "view.zoom_out" => self.with_view(cx, |view, rect, size| {
                view.zoom_at(rect, size, rect.center(), 0.8);
            }),
            "view.actual" => self.with_view(cx, |view, rect, size| {
                view.set_zoom_at(rect, size, rect.center(), 1.0);
            }),
            "view.double" => self.with_view(cx, |view, rect, size| {
                view.set_zoom_at(rect, size, rect.center(), 2.0);
            }),
            "view.fit" => self.with_view(cx, |view, rect, size| view.fit(rect, size)),
            "view.grid" => self.grid = !self.grid,
            "view.ruler_bar" => self.ruler_bar = !self.ruler_bar,
            "view.scroll_bar" => self.scroll_bars = !self.scroll_bars,
            interface if interface.starts_with("window.interface.") => {
                let chosen = interface["window.interface.".len()..].to_owned();
                match cx.edit_rice(sue_rice::SetRiceValue::new(
                    "display.interface",
                    sue_rice::RiceValue::Text(chosen),
                )) {
                    Ok(_) => cx.notify(tr!("interface-changes-now-window-interface-changes")),
                    Err(error) => cx.fail(tr!("cannot-change-the-interface", error = error)),
                }
            }
            "view.snap_grid" => self.snap_grid = !self.snap_grid,
            "view.crop_frame" => self.crop_frame = !self.crop_frame,
            "view.bleed" => self.bleed = !self.bleed,
            "view.page_image" => self.page_image = !self.page_image,
            "view.tone_area" => self.tone_area = !self.tone_area,
            "view.profile_preview" => self.proof_on = !self.proof_on,
            "view.profile_settings" => self.dialog = Some(Dialog::Proof(self.proof_profile)),
            "view.fit_navigator" => self.with_view(cx, |view, rect, size| {
                // The canvas fills the window, as the navigator shows it.
                let zoom = (rect.width() / size[0].max(1) as f32)
                    .max(rect.height() / size[1].max(1) as f32);
                view.set_rotation(rect, size, 0.0);
                view.set_zoom_at(rect, size, rect.center(), zoom);
                let middle = egui::pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
                view.center_on(rect, size, middle);
            }),
            "view.print_size" => {
                let dpi = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(|entity| entity.property(picture::DPI))
                    .and_then(Value::as_f64)
                    .unwrap_or(72.0) as f32;
                // A screen shows about 96 points an inch.
                let zoom = 96.0 / dpi.max(1.0);
                self.with_view(cx, |view, rect, size| {
                    view.set_zoom_at(rect, size, rect.center(), zoom)
                });
            }
            "view.pixel_grid" => self.pixel_grid = !self.pixel_grid,
            "view.selection" => self.show_selection = !self.show_selection,
            "window.next_tab" | "window.prev_tab" => {
                let count = self.docs.len().max(1);
                self.active = if command == "window.next_tab" {
                    (self.active + 1) % count
                } else {
                    (self.active + count - 1) % count
                };
            }
            "window.hide_all" => {
                let show = !self.any_panel();
                let command_bar = self.panels.command_bar;
                self.panels = Panels {
                    command_bar,
                    tool: show,
                    sub_tool: show,
                    tool_property: show,
                    brush_size: show,
                    color: show,
                    color_set: show,
                    navigator: show,
                    layer: show,
                    timeline: show,
                };
            }
            "window.command_bar" => self.panels.command_bar = !self.panels.command_bar,
            "window.hide_menus" => self.menus_hidden = !self.menus_hidden,
            "window.workspace.save" => {
                self.dialog = Some(Dialog::Workspace(tr!(
                    "workspace-2",
                    value = self.workspaces.len() + 1
                )))
            }
            "window.workspace.manage" => self.dialog = Some(Dialog::Workspaces),
            used if used.starts_with("window.workspace.use.") => {
                if let Ok(index) = used["window.workspace.use.".len()..].parse::<usize>() {
                    self.use_workspace(index);
                }
            }
            "window.sub_view" => {
                self.panels.navigator = true;
                self.nav_tab = 1;
            }
            // Palettes of their own, opened from the strip.
            "window.info" | "window.auto_actions" | "window.item_bank" => {
                use crate::dock::Palette;
                let palette = match command {
                    "window.info" => Palette::Info,
                    "window.auto_actions" => Palette::AutoAction,
                    _ => Palette::ItemBank,
                };
                if !self.dock.has(palette) {
                    self.dock.iconize(palette);
                }
                if self.dock.iconized.contains(&palette) {
                    self.dock.popped = Some(palette);
                }
            }
            "window.cels" => {
                self.panels.layer = true;
                self.layer_tab = 3;
            }
            "window.new" => {
                if let Some(doc) = self.doc() {
                    let mut view = Doc::new(doc.picture);
                    view.view = doc.view;
                    view.layer = doc.layer;
                    view.frame = doc.frame;
                    view.track = doc.track;
                    self.docs.insert(self.active + 1, view);
                    self.active += 1;
                    self.tiled = true;
                }
            }
            "window.tile" => self.tiled = true,
            "window.cascade" => self.tiled = false,
            "window.workspace.reset" => {
                self.dock = crate::dock::Dock::default();
                self.panels = Panels::default();
                self.menus_hidden = false;
            }
            "window.tool" => self.panels.tool = !self.panels.tool,
            "window.sub_tool" => self.panels.sub_tool = !self.panels.sub_tool,
            "window.tool_property" => self.panels.tool_property = !self.panels.tool_property,
            "window.brush_size" => self.panels.brush_size = !self.panels.brush_size,
            "window.color_wheel"
            | "window.color_slider"
            | "window.intermediate"
            | "window.approximate" => {
                let tab = match command {
                    "window.color_slider" => 1,
                    "window.intermediate" => 2,
                    "window.approximate" => 3,
                    _ => 0,
                };
                if self.panels.color && self.color_tab == tab {
                    self.panels.color = false;
                } else {
                    self.panels.color = true;
                    self.color_tab = tab;
                }
            }
            "window.color_set" | "window.color_history" | "window.material" => {
                let tab = match command {
                    "window.color_history" => 1,
                    "window.material" => 2,
                    _ => 0,
                };
                if self.panels.color_set && self.set_tab == tab {
                    self.panels.color_set = false;
                } else {
                    self.panels.color_set = true;
                    self.set_tab = tab;
                }
            }
            "window.navigator" => self.panels.navigator = !self.panels.navigator,
            "window.layer" => {
                if self.panels.layer && self.layer_tab == 0 {
                    self.panels.layer = false;
                } else {
                    self.panels.layer = true;
                    self.layer_tab = 0;
                }
            }
            "window.timeline" => self.panels.timeline = !self.panels.timeline,
            "help.shortcuts" => self.dialog = Some(Dialog::Shortcuts),
            "help.about" => cx.notify(tr!("sue-paint-layered-painting-inside-sue")),
            _ => {}
        }
    }

    /// Edit, Outline Selection: a line `width` pixels wide along the
    /// selection's edge, half inside and half outside, in the drawing
    /// color on the current layer.
    pub(crate) fn outline_selection(&mut self, cx: &mut AppletContext<'_>, width: f32) {
        let Some(selection) = self.doc().and_then(|doc| doc.selection.clone()) else {
            return;
        };
        let half = (width / 2.0).max(0.5);
        let outer = selection.expanded(half);
        let inner = selection.shrunk(half);
        let size = selection.size();
        let mask: Vec<u8> = outer
            .mask()
            .iter()
            .zip(inner.mask())
            .map(|(out, inside)| out.saturating_sub(*inside))
            .collect();
        let rgb = color::unit(self.drawing_color());
        let alpha_lock = self
            .layer_info(&cx.world.arena)
            .is_some_and(|info| info.alpha_locked);
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, &tr!("outline-selection"), |pixels, w, h| {
            crate::fill::paint_mask(pixels, [w, h], offset, &mask, size, rgb, 1.0, alpha_lock);
        });
    }

    /// Clears the selected pixels (or, `outside`, the unselected ones).
    fn run_clear(&mut self, cx: &mut AppletContext<'_>, outside: bool, label: &str) {
        let Some(mut selection) = self.selection_or_all(&cx.world.arena) else {
            return;
        };
        if outside {
            selection.invert();
        }
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, label, |pixels, width, height| {
            for y in 0..height {
                for x in 0..width {
                    let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                    if px < 0 || py < 0 {
                        continue;
                    }
                    let cover = selection.at(px as u32, py as u32);
                    if cover > 0.0 {
                        let at = ((y * width + x) * 4 + 3) as usize;
                        pixels[at] = (f32::from(pixels[at]) * (1.0 - cover)).round() as u8;
                    }
                }
            }
        });
    }

    // ----- Keys ---------------------------------------------------------------

    /// The studio's shortcuts (the shell already took undo, redo, save,
    /// open, Settings and the command palette).
    pub(crate) fn keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        // Puppet warp keeps Enter and Esc for itself.
        if ctx.text_edit_focused()
            || self.dialog.is_some()
            || self.menubar.is_open()
            || self.warp.is_some()
            || self.transform.is_some()
        {
            return;
        }
        // The shortcuts as Settings has them (Clip Studio Paint's unless
        // changed), any number of chords each, tools and brush keys too.
        let taken = ctx.input_mut(|input| {
            sue_wm::shortcuts::take(input, cx.rice(), KEYS_SECTION, shortcut_table())
        });
        for name in taken {
            if self.tool_key(name) {
                continue;
            }
            if implemented(name) {
                self.run(name, cx);
            } else {
                let label = crate::menus::label_of(&self.menus, name).unwrap_or(name);
                cx.notify(tr!("not-sue-paint-yet", label = label));
            }
        }
    }

    /// Runs a tool or brush key's command; `false` if `command` is not one.
    fn tool_key(&mut self, command: &str) -> bool {
        let letter = match command {
            "tool.zoom" => '/',
            "tool.hand" => 'H',
            "tool.operation" => 'O',
            "tool.move_layer" => 'K',
            "tool.selection" => 'M',
            "tool.auto_select" => 'W',
            "tool.eyedropper" => 'I',
            "tool.pen" => 'P',
            "tool.brush" => 'B',
            "tool.eraser" => 'E',
            "tool.blend" => 'J',
            "tool.fill" => 'G',
            "tool.figure" => 'U',
            "tool.text" => 'T',
            "tool.correct_line" => 'Y',
            _ => '\0',
        };
        if letter != '\0' {
            self.toolbox.cycle(letter);
            return true;
        }
        match command {
            // The hand's Rotate sub tool.
            "tool.rotate" => {
                self.toolbox.pick(crate::tools::Tool::Hand);
                self.toolbox.pick_sub(crate::tools::Tool::Hand, 1);
            }
            "tool.previous_sub" => self.toolbox.step_sub(false),
            "tool.next_sub" => self.toolbox.step_sub(true),
            "color.swap" => {
                std::mem::swap(&mut self.main, &mut self.sub);
                self.transparent = false;
                self.hex = color::hex(self.drawing_color());
            }
            // Between the drawing color and the transparent color.
            "color.transparent" => self.transparent = !self.transparent,
            "brush.smaller" | "brush.bigger" => {
                let tip = &mut self.toolbox.current_mut().tip;
                tip.size = if command == "brush.bigger" {
                    (tip.size * 1.2).max(tip.size + 1.0)
                } else {
                    (tip.size / 1.2).min(tip.size - 1.0)
                }
                .clamp(0.5, 2000.0);
            }
            _ => return false,
        }
        true
    }

    // ----- The frame ----------------------------------------------------------

    /// Builds the menus again when the language changed (with the recent
    /// files and workspaces in them), and keeps their keys in step with the
    /// settings.
    fn follow_language(&mut self, cx: &AppletContext<'_>) {
        if self.language.changed() {
            self.menus = crate::menus::menus();
            crate::menus::set_recent(
                &mut self.menus,
                crate::menus::recent_items(&self.recent_files),
            );
            self.refresh_workspace_menu();
            self.bound.invalidate();
        }
        self.bound
            .refresh(&mut self.menus, cx.rice(), KEYS_SECTION, shortcut_table());
    }

    fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        self.pen_curve = cx
            .rice()
            .get_str(crate::dialogs::PEN_PRESSURE)
            .and_then(|text| text.parse().ok())
            .unwrap_or_default();
        let present = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        let reveal = cx.view.reveal.clamp(0.0, 1.0);
        self.hide = 1.0 - reveal;
        // The view's ground and picture turn opaque within the first third
        // of the way in.
        let cover = (reveal / 0.35).clamp(0.0, 1.0);
        self.cover = cover * cover * (3.0 - 2.0 * cover);
        self.ready = present && reveal >= 0.999;
        self.present = present;
        if present {
            self.shown = true;
        }
        self.sync_docs(&cx.world.arena);
        self.animate(cx, &ui.ctx().clone());
        self.record_timelapse(cx, &ui.ctx().clone());
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.sync(&cx.world.arena);
        }
        let ctx = ui.ctx().clone();
        if self.ready {
            self.keys(&ctx, cx);
        }
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        self.interface = cx
            .rice()
            .get_str("display.interface")
            .unwrap_or("auto")
            .to_owned();
        let form = cx.view.form_factor;
        if self.laid_for != Some(form) {
            self.lay_out_for(form);
        }
        // Phones and tablets have their own start page.
        if form != sue_platform::FormFactor::Desktop && self.start.shows(self.docs.is_empty()) {
            self.mobile_start_ui(ui, full, cx, &theme);
            if self.ready {
                self.dialog_ui(&ctx, cx, &theme);
            }
            return;
        }
        self.start.full = false;
        // Simple Mode, unless Studio Mode was chosen.
        self.bare = form != sue_platform::FormFactor::Desktop && !crate::simple::studio_mode(cx);
        if self.bare {
            self.simple_frame(ui, full, cx, &theme);
            if self.ready {
                self.dialog_ui(&ctx, cx, &theme);
            }
            return;
        }
        // Studio Mode on phones and tablets.
        if form != sue_platform::FormFactor::Desktop {
            self.studio_frame(ui, full, cx, &theme);
            if self.ready {
                self.dialog_ui(&ctx, cx, &theme);
            }
            return;
        }
        // On the desktop the start page shows while no canvas is open.
        if form == sue_platform::FormFactor::Desktop && self.start.shows(self.docs.is_empty()) {
            self.start_ui(ui, full, cx, &theme);
            if self.ready {
                self.dialog_ui(&ctx, cx, &theme);
            }
            return;
        }
        if form == sue_platform::FormFactor::Tablet {
            // Bigger targets for fingers and pens.
            ui.spacing_mut().interact_size.y = 32.0;
            ui.spacing_mut().button_padding = vec2(8.0, 6.0);
        }

        // The menus are in Sue's top bar (`bar_ui`); the command bar sits
        // at the page's top. While the studio comes in or goes out, every
        // palette slides in from or out to its edge of the window.
        let top_height = if self.menus_hidden || !self.panels.command_bar {
            0.0
        } else {
            38.0
        };
        let lift = self.hide * top_height;
        if top_height > 0.0 {
            let top = full.top() - lift;
            let strip = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), top + 38.0));
            self.command_bar(ui, strip, cx, &theme);
        }
        let body = Rect::from_min_max(pos2(full.left(), full.top() + top_height - lift), full.max);
        self.body(ui, body, cx, &theme);
        if self.ready {
            self.dialog_ui(&ctx, cx, &theme);
        }
    }

    /// Puts the palettes where `form`'s layout wants them, once, when the
    /// studio first shows in it or the interface changes: a tablet keeps the
    /// tool strip and the layers docked and the rest as icons beside the
    /// canvas, ready to pop out over it.
    fn lay_out_for(&mut self, form: sue_platform::FormFactor) {
        use crate::dock::Palette;
        self.laid_for = Some(form);
        self.sheet = None;
        match form {
            sue_platform::FormFactor::Tablet => {
                self.dock = crate::dock::Dock::default();
                for palette in [
                    Palette::SubTool,
                    Palette::BrushSize,
                    Palette::Color,
                    Palette::ColorSet,
                    Palette::Navigator,
                ] {
                    self.dock.iconize(palette);
                }
            }
            sue_platform::FormFactor::Desktop => {
                if self.dock.iconized.len() >= 4 {
                    self.dock = crate::dock::Dock::default();
                }
            }
            sue_platform::FormFactor::Mobile => {}
        }
    }

    /// Lays out the palettes around the canvas: docked, collapsed or
    /// floating (`dock`), the timeline under the canvas, and the canvas.
    fn body(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &sue_wm::Theme) {
        let mut center = self.dock_ui(ui, body, cx, theme);
        let room = center.height() > 360.0 && center.width() > 320.0;
        if self.panels.timeline && room {
            let height = self.dock.timeline.min(center.height() - 200.0).max(90.0);
            let top = center.bottom() - height * (1.0 - self.hide);
            let timeline =
                Rect::from_min_max(pos2(center.left(), top), pos2(center.right(), top + height));
            self.timeline_panel(ui, timeline, cx, theme);
            if self.ready {
                self.timeline_splitter(ui, top, center.x_range(), theme);
            }
            center.max.y = top;
        }
        self.canvas_area(ui, center, cx, theme);
    }
}

/// The settings section Sue Paint's shortcuts live in.
pub(crate) const KEYS_SECTION: &str = "sue_paint_keys";

/// Sue Paint's shortcuts with their default chords (Clip Studio Paint's),
/// each command once with every chord it has, labeled as its menu item.
pub(crate) fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: std::sync::OnceLock<Vec<sue_wm::shortcuts::Shortcut>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        // The labels are the settings' English descriptions; Settings and
        // the Keyboard Shortcuts list read them in other languages from
        // the language database.
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = crate::menus::menus();
        let mut table = sue_wm::shortcuts::table(shortcuts(), |command| {
            crate::menus::label_of(&menus, command)
                .unwrap_or(command)
                .trim_end_matches("...")
                .to_owned()
        });
        table.extend(TOOL_KEYS.iter().map(|(command, label, chords)| {
            sue_wm::shortcuts::Shortcut::new(command, *label, chords)
        }));
        table
    })
}

/// Tool and brush keys, Clip Studio Paint's letters: command, what
/// Settings calls it, default chords.
const TOOL_KEYS: [(&str, &str, &str); 22] = [
    ("tool.zoom", "Zoom Tool", "/"),
    ("tool.hand", "Hand Tool", "H"),
    ("tool.rotate", "Rotate Tool", "R"),
    ("tool.operation", "Operation Tool", "O"),
    ("tool.move_layer", "Move Layer Tool", "K"),
    ("tool.selection", "Selection Tool", "M"),
    ("tool.auto_select", "Auto Select Tool", "W"),
    ("tool.eyedropper", "Eyedropper Tool", "I"),
    ("tool.pen", "Pen and Pencil Tools", "P"),
    ("tool.brush", "Brush, Airbrush and Decoration Tools", "B"),
    ("tool.eraser", "Eraser Tool", "E"),
    ("tool.blend", "Blend Tool", "J"),
    ("tool.fill", "Fill and Gradient Tools", "G"),
    ("tool.figure", "Figure, Frame Border and Ruler Tools", "U"),
    ("tool.text", "Text and Balloon Tools", "T"),
    ("tool.correct_line", "Correct Line Tool", "Y"),
    ("tool.previous_sub", "Previous Sub Tool", ","),
    ("tool.next_sub", "Next Sub Tool", "."),
    ("color.swap", "Swap Main and Sub Color", "X"),
    ("color.transparent", "Drawing or Transparent Color", "C"),
    ("brush.smaller", "Smaller Brush", "["),
    ("brush.bigger", "Bigger Brush", "]"),
];

/// The studio's default chords, Clip Studio Paint's.
fn shortcuts() -> Vec<(Modifiers, Key, &'static str)> {
    let command = Modifiers::COMMAND;
    let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
    let alt = Modifiers::ALT;
    let shift = Modifiers::SHIFT;
    let none = Modifiers::NONE;
    vec![
        // File.
        (shifted | alt, Key::K, "help.shortcuts"),
        (command, Key::K, "file.preferences"),
        (shifted, Key::S, "file.save_as"),
        (command | alt, Key::S, "file.save_as"),
        (shift | alt, Key::S, "file.save_as"),
        (command, Key::P, "file.print"),
        (shifted, Key::N, "layer.new"),
        (command, Key::N, "file.new"),
        (command, Key::W, "file.close"),
        // Edit.
        (shifted, Key::T, "edit.transform.free"),
        (command, Key::T, "edit.transform.scale"),
        (command, Key::U, "filter.hue"),
        (shifted, Key::I, "select.invert"),
        (command, Key::I, "filter.invert"),
        (command, Key::X, "edit.cut"),
        (command, Key::C, "edit.copy"),
        (command, Key::V, "edit.paste"),
        (none, Key::F2, "edit.cut"),
        (none, Key::F3, "edit.copy"),
        (none, Key::F4, "edit.paste"),
        (command, Key::Delete, "edit.clear"),
        (command, Key::Backspace, "edit.clear"),
        (shift, Key::Delete, "edit.fill"),
        (shift, Key::Backspace, "edit.fill"),
        (alt, Key::Delete, "edit.fill"),
        (alt, Key::Backspace, "edit.fill"),
        (none, Key::Delete, "edit.clear"),
        (none, Key::Backspace, "edit.clear"),
        // Layer.
        (command | alt, Key::G, "layer.toggle.clip"),
        (command, Key::G, "layer.folder_insert"),
        (shifted, Key::E, "layer.merge_visible_here"),
        (shifted, Key::G, "layer.ungroup"),
        (shift | alt, Key::E, "layer.merge_selected"),
        (command, Key::E, "layer.merge_down"),
        (alt, Key::CloseBracket, "layer.select_above"),
        (alt, Key::OpenBracket, "layer.select_below"),
        (command, Key::CloseBracket, "layer.order.up"),
        (command, Key::OpenBracket, "layer.order.down"),
        // Selection.
        (shifted, Key::D, "select.again"),
        (shift, Key::F7, "select.invert"),
        (command, Key::A, "select.all"),
        (command, Key::D, "select.none"),
        // View.
        (command | alt, Key::Num0, "view.actual"),
        (command, Key::Num0, "view.fit"),
        (shifted, Key::Num2, "view.rotate_reset"),
        (command, Key::Plus, "view.zoom_in"),
        (command, Key::Equals, "view.zoom_in"),
        (command, Key::Semicolon, "view.zoom_in"),
        (command, Key::Minus, "view.zoom_out"),
        (command, Key::R, "view.ruler_bar"),
        (command, Key::Num1, "view.snap_ruler"),
        (command, Key::Num2, "view.snap_special"),
        (command, Key::Num3, "view.snap_grid"),
        (none, Key::Minus, "view.rotate_left"),
        (none, Key::Equals, "view.rotate_right"),
        // Window.
        (shifted, Key::Tab, "window.prev_tab"),
        (command, Key::Tab, "window.next_tab"),
        (shift, Key::Tab, "window.hide_menus"),
        (none, Key::Tab, "window.hide_all"),
        // Help.
        (none, Key::F1, "help.shortcuts"),
        (alt, Key::I, "help.shortcuts"),
    ]
}

/// Sue Paint's applet id.
fn me_id() -> AppletId {
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

/// Where exports go without a dialog: `io.convert_dir`, or the home folder.
/// The JPEG quality Settings has for conversions.
fn jpeg_quality(cx: &AppletContext<'_>) -> u8 {
    cx.rice()
        .get_i64("io.jpeg_quality")
        .and_then(|quality| u8::try_from(quality).ok())
        .unwrap_or(90)
}

/// The JPEG XL effort Settings has for conversions.
fn jxl_effort(cx: &AppletContext<'_>) -> u8 {
    cx.rice()
        .get_i64("io.jxl_effort")
        .and_then(|effort| u8::try_from(effort).ok())
        .unwrap_or(7)
}

fn export_dir(cx: &AppletContext<'_>) -> PathBuf {
    let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
    if riced.is_empty() {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    } else {
        PathBuf::from(riced)
    }
}

/// `path`, or `name (2).ext` and so on if it exists.
fn free_path(path: &Path) -> PathBuf {
    if !sue_platform::fs::exists(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| !sue_platform::fs::exists(candidate))
        .unwrap_or_else(|| path.to_path_buf())
}

impl SueApplet for SuePaint {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Sue Paint", AppletRole::Studio, "\u{1F3A8}")
            .localized("paint");
        manifest.summary = tr!("applet-summary").to_owned();
        manifest.order = 25;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let int = |value, min, max| RiceParameter::IntRange {
            value,
            min: Some(min),
            max: Some(max),
        };
        let mut declarations = vec![
            RiceDeclaration::new(
                "sue_paint.width",
                int(1600, 1, 10_000),
                "Width of new illustrations (px)",
            ),
            RiceDeclaration::new(
                "sue_paint.height",
                int(1200, 1, 10_000),
                "Height of new illustrations (px)",
            ),
            RiceDeclaration::new(
                "sue_paint.dpi",
                int(72, 1, 2400),
                "Resolution of new illustrations (dpi)",
            ),
            RiceDeclaration::new(
                crate::materials::LOCATION_KEY,
                RiceParameter::Text {
                    value: String::new(),
                },
                "Where Sue Paint keeps its materials and brush tips (empty: its own folder)",
            ),
            RiceDeclaration::new(
                crate::clip_io::SAVE_FORMAT_KEY,
                RiceParameter::EnumChoice {
                    value: "clip".to_owned(),
                    options: vec!["clip".to_owned(), "sue".to_owned()],
                },
                "How Save keeps a picture: a Clip Studio Paint .clip file, or a Sue project",
            ),
            RiceDeclaration::new(
                crate::simple::MODE_KEY,
                RiceParameter::EnumChoice {
                    value: "simple".to_owned(),
                    options: vec!["simple".to_owned(), "studio".to_owned()],
                },
                "Sue Paint on phones and tablets: Simple Mode or Studio Mode",
            ),
            RiceDeclaration::new(
                "sue_paint.theme",
                RiceParameter::EnumChoice {
                    value: "gray".to_owned(),
                    options: [
                        "gray",
                        sue_wm::STUDIO_THEME_SUE,
                        "void",
                        "graphite",
                        "paper",
                        sue_wm::STUDIO_THEME_CUSTOM,
                    ]
                    .map(String::from)
                    .to_vec(),
                },
                "Sue Paint's colors: Gray, Sue's own theme (follows it), a preset, or Custom (below)",
            ),
            RiceDeclaration::new(
                crate::dialogs::PEN_PRESSURE,
                RiceParameter::Text {
                    value: crate::dialogs::DEFAULT_PEN_PRESSURE.to_owned(),
                },
                "Pen pressure curve for every tool, as pressure,output points (File, Pen Pressure Settings draws it)",
            ),
        ];
        // File, Preferences: what Sue Paint honors of Clip Studio's.
        let choice = |value: &str, options: &[&str]| RiceParameter::EnumChoice {
            value: value.to_owned(),
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        };
        declarations.extend([
            RiceDeclaration::new(
                "sue_paint.shift_line",
                RiceParameter::Bool { value: true },
                "Shift and drag draws a straight line, shown as it is drawn (Preferences, Tools)",
            )
            .labeled("Shift for straight lines"),
            RiceDeclaration::new(
                "sue_paint.two_finger_tap",
                choice("undo", &["undo", "none"]),
                "What a quick tap of two fingers on the canvas does (Preferences, Touch gestures)",
            )
            .labeled("Two-finger tap"),
            RiceDeclaration::new(
                "sue_paint.three_finger_tap",
                choice("redo", &["redo", "none"]),
                "What a quick tap of three fingers on the canvas does (Preferences, Touch gestures)",
            )
            .labeled("Three-finger tap"),
            RiceDeclaration::new(
                "sue_paint.brush_cursor",
                choice("brush", &["brush", "crosshair", "dot"]),
                "The pointer over the canvas while painting: the brush's outline, a crosshair or a dot (Preferences, Cursor)",
            )
            .labeled("Brush cursor"),
            RiceDeclaration::new(
                "sue_paint.selection_layer_opacity",
                int(50, 0, 100),
                "Opacity of a new selection layer (%; Preferences, Layer/Frame)",
            )
            .labeled("Opacity of selection layers"),
            RiceDeclaration::new(
                "sue_paint.rotate_step",
                int(15, 1, 90),
                "How far Rotate Left and Rotate Right turn the view (degrees; Preferences, Canvas)",
            )
            .labeled("Rotation step"),
        ]);
        for (key, name, choices) in crate::dialogs::MODIFIERS {
            declarations.push(RiceDeclaration::new(
                key,
                RiceParameter::EnumChoice {
                    value: choices[0].to_owned(),
                    options: choices.iter().map(|choice| (*choice).to_owned()).collect(),
                },
                tr!("what-does-canvas-file-modifier-key", name = name),
            ));
        }
        declarations.extend(crate::studio_mode::CUSTOM_DEFAULTS.iter().enumerate().map(
            |(index, command)| {
                RiceDeclaration::new(
                    crate::studio_mode::custom_key(index),
                    RiceParameter::Text {
                        value: (*command).to_owned(),
                    },
                    format!("The command Studio Mode's key T{} runs", index + 1),
                )
            },
        ));
        declarations.extend(Self::custom_colors());
        declarations.extend(sue_wm::shortcuts::declarations(
            KEYS_SECTION,
            shortcut_table(),
        ));
        declarations
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.load_recent(cx);
        self.load_extras(cx);
        self.frame(ui, cx);
    }

    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        let no_canvas = self.docs.is_empty();
        if cx.view.form_factor != sue_platform::FormFactor::Desktop && self.start.shows(no_canvas) {
            self.mobile_header(ui, rect, &theme);
            return;
        }
        if cx.view.form_factor != sue_platform::FormFactor::Desktop
            && !crate::simple::studio_mode(cx)
        {
            if let Some(command) = self.simple_bar(ui, rect, cx, &theme) {
                self.run(&command, cx);
            }
            return;
        }
        if cx.view.form_factor != sue_platform::FormFactor::Desktop {
            self.studio_bar(ui, rect, cx, &theme);
            return;
        }
        // The start page has no menus; everything is on its buttons.
        if self.menus_hidden
            || (cx.view.form_factor == sue_platform::FormFactor::Desktop
                && self.start.shows(self.docs.is_empty()))
        {
            return;
        }
        let mut menubar = std::mem::take(&mut self.menubar);
        let picked = {
            let arena = &cx.world.arena;
            let history = &*cx.history;
            let state = |command: &str| self.state(command, arena, history);
            menubar.ui(ui, rect, &self.menus, &state, &theme)
        };
        self.menubar = menubar;
        if let Some(command) = picked {
            self.run(&command, cx);
        }
    }

    fn accepts(&self, entity: &Entity) -> bool {
        picture::is_picture(entity)
    }

    fn focus(&self) -> Option<EntityId> {
        self.picture()
    }

    /// Each canvas has its own history; the pages of a story share the
    /// story's, as a comic's pages share their Page Manager.
    fn history_scope(&self, arena: &sue_core::Arena) -> Option<EntityId> {
        let picture = self.picture()?;
        Some(crate::story::story_of(arena, picture).unwrap_or(picture))
    }

    /// Clip Studio Paint files come to Sue Paint as they are, wherever
    /// they are opened (the Sue menu, the start page, a drop, the Hub),
    /// since only Sue Paint reads their layers.
    fn opens_file(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("clip"))
    }

    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        // Every chord of the studio's shortcuts wins while it is open (Tab
        // hides the palettes, / is the Zoom tool, Ctrl+0 fits the picture).
        self.bound.chords(shortcut_table())
    }

    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<sue_wm::SueAction> {
        let me = || AppletId::new(ID).expect("a valid id");
        if current {
            return Vec::new();
        }
        // A picture file open elsewhere (a PNG in the Image Viewer, say)
        // opens as a new picture.
        let file = focus
            .filter(|entity| !picture::is_picture(entity))
            .filter(|entity| {
                entity
                    .component(sue_core::ComponentKind::RasterTile2D)
                    .is_some()
            })
            .and_then(|entity| entity.meta(sue_io::meta::SOURCE))
            .map(PathBuf::from)
            .filter(|path| sue_platform::fs::is_file(path));
        match file {
            Some(path) => vec![sue_wm::SueAction::new(
                sue_wm::sue_menu::SEND_TO,
                "Sue Paint",
                ShellRequest::SendFile {
                    to: me(),
                    purpose: "open".to_owned(),
                    path,
                },
            )],
            None => Vec::new(),
        }
    }

    fn take_entity(&mut self, purpose: &str, entity: EntityId, cx: &mut AppletContext<'_>) {
        if !matches!(purpose, "open" | "adopt") {
            return;
        }
        if cx.arena().get_by_id(entity).is_ok_and(picture::is_picture) {
            // A layered picture opens as it is.
            self.activate(&cx.world.arena, entity);
            cx.request(ShellRequest::Open(me_id()));
            return;
        }
        // Any other picture becomes a layered picture whose layer is that
        // very entity, and Sue Paint opens on it.
        let name = cx.arena().get_by_id(entity).map_or_else(
            |_| tr!("picture"),
            |found| {
                Path::new(found.name()).file_stem().map_or_else(
                    || found.name().to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                )
            },
        );
        match doc::adopt(cx.arena(), entity, &name) {
            Ok((edit, picture)) => {
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture);
                    cx.request(ShellRequest::Open(me_id()));
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if matches!(pick.purpose.as_str(), "open" | "import" | "layer")
            && cx.view.page.as_ref().map(AppletId::as_str) != Some(ID)
        {
            cx.request(ShellRequest::Open(me_id()));
        }
        if self.start_picked(&pick.purpose, path, cx) {
            return;
        }
        match pick.purpose.as_str() {
            // "import" is a file opened anywhere in Sue that Sue Paint
            // takes itself (see `opens_file`).
            "open" | "import" => self.open_picked(cx, path, false),
            "layer" => self.open_picked(cx, path, true),
            "import-layers" => self.open_picked(cx, path, true),
            "story-text" => self.import_text(cx, path),
            "batch" => self.export_canvases(cx, path),
            "import-vector" | "import-movie" | "import-pattern" => {
                self.import_file(cx, pick.purpose.as_str(), path)
            }
            "export" => {
                if let Some(bytes) = self.pending_export.take() {
                    self.write_export(cx, path, &bytes);
                }
            }
            "save-as" => self.save_as_to(cx, path),
            "story-pages" => self.export_pages(cx, path),
            "book-pdf" => self.export_book(cx, path, false),
            "book-epub" => self.export_book(cx, path, true),
            "anim-gif" => self.export_animation(cx, path, true),
            "anim-movie" => self.export_movie(cx, path),
            "webtoon" => self.export_webtoon(cx, path),
            "anim-cels" => self.export_cels(cx, path),
            "timelapse" => self.export_timelapse(cx, path),
            "anim-sequence" => self.export_animation(cx, path, false),
            "subview" => self.add_reference(path, cx),
            "model3d" => self.import_3d(cx, path),
            "anim-audio" => self.import_audio(cx, path),
            "colorset-import" => match sue_platform::fs::read_to_string(path) {
                Ok(text) => {
                    let colors = crate::colorsets::import(&text);
                    if colors.is_empty() {
                        cx.fail(tr!("no-colors-in-that-file"));
                    } else {
                        let name = path.file_stem().map_or_else(
                            || tr!("imported"),
                            |stem| stem.to_string_lossy().into_owned(),
                        );
                        self.color_sets.add_set(&name, colors);
                        self.save_color_sets(cx);
                    }
                }
                Err(error) => cx.fail(tr!("cannot-read-it", error = error)),
            },
            "colorset-export" => {
                let (name, colors) = self
                    .color_sets
                    .sets
                    .get(self.color_sets.current)
                    .cloned()
                    .unwrap_or_default();
                match write_atomic(
                    path,
                    crate::colorsets::export_gpl(&name, &colors).as_bytes(),
                ) {
                    Ok(()) => cx.saved(path, tr!("gimp-palette")),
                    Err(error) => cx.fail(tr!("cannot-write-it", error = error)),
                }
            }
            _ => {}
        }
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        let mut templates: Vec<sue_wm::FileTemplate> = crate::dialogs::new_presets()
            .for_target(crate::dialogs::NEW_PRESET_TARGET)
            .map(|preset| {
                // Short labels fit the wheel's wedges.
                let label = match preset.name.as_str() {
                    "Print (A4, 350 dpi)" => tr!("print-a4"),
                    "Screen (Full HD)" => tr!("full-hd"),
                    "Transparent Sprite" => tr!("sprite"),
                    name => name.to_owned(),
                };
                sue_wm::FileTemplate::new(tr!("picture"), label, &preset.name)
            })
            .collect();
        templates.push(sue_wm::FileTemplate::new(
            tr!("picture"),
            tr!("custom-ellipsis"),
            "custom",
        ));
        templates
    }

    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        if key == "custom" {
            self.dialog = Some(Dialog::New(Self::new_dialog(cx)));
        } else {
            let library = crate::dialogs::new_presets();
            let Some(preset) = library.get(crate::dialogs::NEW_PRESET_TARGET, key) else {
                return;
            };
            let mut dialog = NewDialog::new(&NewPicture::default());
            dialog.apply_preset(preset);
            let mut spec = dialog.spec();
            spec.name = preset.name.clone();
            self.make_picture(cx, &spec);
        }
        // Then into Sue Paint.
        cx.request(ShellRequest::Open(me_id()));
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            self.shown = false;
            self.settle(cx);
        }
        sue_wm::Tick::default()
    }

    fn captures_keys(&self) -> bool {
        // A dialog or a stroke in progress owns the keys, so the global
        // undo cannot pull pixels from under a preview or a pen.
        self.dialog.is_some() || self.gesture.is_some()
    }

    fn commands(&self) -> Vec<PaletteEntry> {
        [
            (tr!("new-illustration-ellipsis"), "file.new", "Ctrl+N"),
            (tr!("open-picture-ellipsis"), "file.open", ""),
            (tr!("start-page"), "file.start_page", ""),
            (tr!("export-as-png-ellipsis"), "file.export.png", ""),
            (tr!("keyboard-shortcuts"), "help.shortcuts", ""),
        ]
        .into_iter()
        .map(|(label, command, hint)| PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: hint.to_owned(),
        })
        .collect()
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        if !implemented(name) {
            return false;
        }
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me_id()));
        }
        self.sync_docs(&cx.world.arena);
        self.run(name, cx);
        true
    }

    fn navigate(&mut self, target: &str) -> bool {
        if let Some(title) = target.strip_prefix("menu/") {
            return self.menubar.open(&self.menus, title);
        }
        match target {
            "dialog/new" => {
                self.dialog = Some(Dialog::New(NewDialog::new(&NewPicture::default())));
                true
            }
            "dialog/preferences" => {
                let Ok(rice) = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE) else {
                    return false;
                };
                self.dialog = Some(Dialog::Preferences(crate::prefs::Preferences::read(&rice)));
                true
            }
            "dialog/pages" => {
                self.dialog = Some(Dialog::Pages(crate::dialogs::PagesDialog { chosen: 0 }));
                true
            }
            "dialog/pressure" => {
                // A sample of someone's drawing, so the pad shows lines.
                let mut pressure = PressureDialog::new(self.pen_curve);
                let line: Vec<(egui::Vec2, f32)> = (0..120)
                    .map(|step| {
                        let t = step as f32 / 119.0;
                        let force = (t * std::f32::consts::PI).sin() * 0.55 + 0.05;
                        (vec2(20.0 + t * 480.0, 75.0 + (t * 9.0).sin() * 40.0), force)
                    })
                    .collect();
                pressure.raw = line.iter().map(|(_, force)| *force).collect();
                pressure.lines.push(line);
                self.dialog = Some(Dialog::Pressure(pressure));
                true
            }
            start if start.starts_with("start/") => self.start.navigate(start),
            // A palette's tab: `tab/<palette>/<index>`.
            tab if tab.starts_with("tab/") => {
                let mut parts = tab["tab/".len()..].split('/');
                let (Some(palette), Some(Ok(index))) =
                    (parts.next(), parts.next().map(str::parse::<u8>))
                else {
                    return false;
                };
                match palette {
                    "tool" => self.tool_tab = index,
                    "layer" => self.layer_tab = index,
                    "navigator" => self.nav_tab = index,
                    "color" => self.color_tab = index,
                    _ => return false,
                }
                true
            }
            "sheet/more" => {
                self.sheet = Some(crate::phone::Sheet::More);
                true
            }
            "sheet/pens" => {
                self.sheet = Some(crate::phone::Sheet::Pens);
                true
            }
            "sheet/close" => {
                self.sheet = None;
                self.touch.keys = false;
                true
            }
            "sheet/menu" => {
                self.sheet = Some(crate::phone::Sheet::Menu);
                true
            }
            "keys/open" => {
                self.touch.keys = true;
                true
            }
            "dialog/close" => {
                self.dialog = None;
                self.menubar.close();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_chord_becomes_a_setting() {
        let table = shortcut_table();
        // Every chord of the old table made it in, grouped by command.
        let chords: usize = table
            .iter()
            .map(|shortcut| shortcut.chords.chords().len())
            .sum();
        assert_eq!(chords, shortcuts().len() + TOOL_KEYS.len());
        let save_as = table
            .iter()
            .find(|shortcut| shortcut.command == "file.save_as")
            .unwrap();
        assert_eq!(save_as.chords.chords().len(), 3);
        let menus = crate::menus::menus();
        for shortcut in table {
            assert!(
                implemented(shortcut.command)
                    || crate::menus::label_of(&menus, shortcut.command).is_some(),
                "{} is bound but neither built nor in a menu",
                shortcut.command
            );
        }
        let declarations = sue_wm::shortcuts::declarations(KEYS_SECTION, table);
        let mut keys: Vec<&str> = declarations
            .iter()
            .map(|declaration| declaration.path.as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), table.len(), "one setting per command");
    }

    #[test]
    fn every_implemented_command_is_in_a_menu_or_a_shortcut() {
        let mut named: Vec<String> = crate::menus::menus()
            .iter()
            .flat_map(|menu| menu.items.iter().flat_map(|item| item.commands()))
            .map(str::to_owned)
            .collect();
        named.extend(
            shortcut_table()
                .iter()
                .map(|shortcut| shortcut.command.to_owned()),
        );
        for command in named.iter().filter(|command| implemented(command)) {
            assert!(command.contains('.'));
        }
        assert!(named.iter().filter(|command| implemented(command)).count() > 60);
        assert!(implemented("story.add"), "pages are built");
        // Every menu item does something.
        let missing: Vec<String> = crate::menus::commands(&crate::menus::menus())
            .into_iter()
            .map(|(_, command)| command)
            .filter(|command| {
                !implemented(command) && !command.starts_with("window.workspace.preset")
            })
            .collect();
        assert!(missing.is_empty(), "grayed menu items: {missing:?}");
    }

    #[test]
    fn mirroring_flips_rows_and_columns() {
        let mut pixels: Vec<u8> = (0..2 * 2 * 4).map(|value| value as u8).collect();
        mirror(&mut pixels, 2, 2, true, false);
        assert_eq!(&pixels[..4], &[4, 5, 6, 7]);
        mirror(&mut pixels, 2, 2, true, true);
        assert_eq!(&pixels[..4], &[8, 9, 10, 11]);
    }
}
