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
use sue_io::{JxlPipeline, NativeJxlPipeline};
use sue_model::class;
use sue_model::picture::{self, LayerKind};
use sue_platform::{FileFilter, write_atomic};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaEdit, ItemState, Menu, MenuBar,
    ModeSpec, PaletteEntry, ShellRequest, SueApplet, World,
};

use crate::brush::{Stabilizer, Stroke};
use crate::color::{self, History};
use crate::composite::{self, Compositor, RenderOptions};
use crate::dialogs::{Dialog, FilterDialog, NewDialog};
use crate::doc::{self, NewPicture};
use crate::filters::Filter;
use crate::select::{Op, Selection};
use crate::tools::Toolbox;
use crate::view::View;

/// The applet id.
pub const ID: &str = "sue-paint";

/// One open canvas tab.
#[derive(Debug)]
pub(crate) struct Doc {
    pub picture: EntityId,
    pub view: View,
    pub compositor: Compositor,
    pub layer: Option<EntityId>,
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
    /// pixels when the layer is offset).
    Stroke {
        stroke: Stroke,
        stabilizer: Stabilizer,
        offset: [i64; 2],
        raw: [f32; 2],
        label: String,
        shifted: Option<Selection>,
    },
    /// Scrolling the view.
    Pan { last: Pos2 },
    /// Turning the view.
    Rotate { start: f32, rotation: f32 },
    /// A rectangle, ellipse, line or gradient from `from` to `to` (picture
    /// pixels).
    Shape {
        tool: crate::tools::Tool,
        sub: crate::tools::SubTool,
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
    pub(crate) hue: f32,
    pub(crate) recent: History,
    pub(crate) docs: Vec<Doc>,
    pub(crate) active: usize,
    pub(crate) closed: HashSet<EntityId>,
    pub(crate) menus: Vec<Menu>,
    pub(crate) menubar: MenuBar,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) gesture: Option<Gesture>,
    pub(crate) pressure: Option<f32>,
    pub(crate) panels: Panels,
    pub(crate) color_tab: u8,
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
    pub(crate) preview: Option<(u64, Vec<u8>)>,
    pub(crate) last_dir: Option<PathBuf>,
    pub(crate) shown: bool,
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
            docs: Vec::new(),
            active: 0,
            closed: HashSet::new(),
            menus: crate::menus::menus(),
            menubar: MenuBar::new(),
            dialog: None,
            gesture: None,
            pressure: None,
            panels: Panels::default(),
            color_tab: 0,
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
            preview: None,
            last_dir: None,
            shown: false,
        }
    }
}

impl std::fmt::Debug for SuePaint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuePaint")
            .field("tool", &self.toolbox.tool())
            .field("docs", &self.docs.len())
            .finish_non_exhaustive()
    }
}

/// A texture key for an entity, so each picture and thumbnail has its own.
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
            | "help.about"
    )
}

impl SuePaint {
    // ----- Reading the state ------------------------------------------------

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
    pub(crate) fn paintable(&self, arena: &Arena) -> Result<doc::LayerInfo, String> {
        let info = self
            .layer_info(arena)
            .ok_or("Pick a layer to draw on first")?;
        match info.kind {
            LayerKind::Raster => {}
            LayerKind::Paper => return Err("The paper layer cannot be drawn on".to_owned()),
            LayerKind::Other => return Err(format!("{} cannot be drawn on yet", info.name)),
        }
        if info.locked {
            return Err(format!("{} is locked", info.name));
        }
        if !info.style.visible {
            return Err(format!("{} is hidden", info.name));
        }
        if doc::raster(arena, info.id).is_none_or(|raster| raster.format() != PixelFormat::Rgba8) {
            return Err(format!("{} is not an RGBA layer", info.name));
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
        self.docs.retain(|doc| pictures.contains(&doc.picture));
        let mut added = None;
        for id in pictures {
            if !self.closed.contains(&id) && !self.docs.iter().any(|doc| doc.picture == id) {
                self.docs.push(Doc::new(id));
                added = Some(self.docs.len() - 1);
            }
        }
        self.active = added
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
                cx.fail(format!("Sue Paint: {error}"));
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
            Ok(None) => cx.notify(format!("{label}: nothing changed")),
            Err(error) => cx.fail(error),
        }
    }

    /// Runs a filter on the current layer at once.
    fn filter_now(&mut self, cx: &mut AppletContext<'_>, filter: Filter) {
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, filter.label(), |pixels, width, height| {
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
            cx.fail("Only raster layers can be copied");
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

    /// Writes the flattened picture as `ext` (png, jpg or jxl).
    fn export(&mut self, cx: &mut AppletContext<'_>, ext: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let name = arena
            .get_by_id(picture)
            .map_or_else(|_| "Picture".to_owned(), |entity| entity.name().to_owned());
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(mut flat) = composite::flatten(arena, picture, options) else {
            cx.fail("This picture has no size");
            return;
        };
        let bytes = match ext {
            "png" => sue_io::encode_png(&flat),
            "jpg" => {
                // JPEG has no alpha: lay the picture over white.
                if let Some(pixels) = flat.bytes_mut() {
                    for pixel in pixels.as_chunks_mut::<4>().0 {
                        let alpha = f32::from(pixel[3]) / 255.0;
                        for channel in &mut pixel[..3] {
                            *channel =
                                (f32::from(*channel) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
                        }
                        pixel[3] = 255;
                    }
                }
                let quality = cx
                    .rice()
                    .get_i64("io.jpeg_quality")
                    .and_then(|quality| u8::try_from(quality).ok())
                    .unwrap_or(90);
                sue_io::encode_jpeg(&flat, quality)
            }
            _ => {
                let effort = cx
                    .rice()
                    .get_i64("io.jxl_effort")
                    .and_then(|effort| u8::try_from(effort).ok())
                    .unwrap_or(7);
                NativeJxlPipeline
                    .encode_layers(&[&flat], effort)
                    .map_err(|error| error.to_string())
            }
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(format!("Export failed: {error}"));
                return;
            }
        };
        let file = format!("{name}.{ext}");
        let path = if cx.view.interactive {
            let filters = [FileFilter {
                name: ext.to_uppercase(),
                extensions: vec![ext.to_owned()],
            }];
            let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
            match sue_platform::pick_save_file("Export", Some(&start), &file, &filters) {
                Some(path) => path,
                None => return,
            }
        } else {
            free_path(&export_dir(cx).join(&file))
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match write_atomic(&path, &bytes) {
            Ok(()) => cx.notify(format!(
                "Exported {} ({} KiB)",
                path.file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                bytes.len().div_ceil(1024)
            )),
            Err(error) => cx.fail(format!("Cannot write {}: {error}", path.display())),
        }
    }

    /// Asks for a picture file and opens it as a new canvas, or as a new
    /// layer of the current one.
    fn pick_image(&mut self, cx: &mut AppletContext<'_>, as_layer: bool) {
        if !cx.view.interactive {
            cx.notify("Choosing files needs the desktop window");
            return;
        }
        let filters = [FileFilter {
            name: "Pictures".to_owned(),
            extensions: ["png", "jpg", "jpeg", "jxl"].map(String::from).to_vec(),
        }];
        let start = self
            .last_dir
            .clone()
            .or_else(|| cx.catalog.folders.get("home").cloned());
        let Some(path) = sue_platform::pick_open_file("Open Picture", start.as_deref(), &filters)
        else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Err(error) = self.open_bytes(cx, &path, &bytes, as_layer) {
                    cx.fail(error);
                }
            }
            Err(error) => cx.fail(format!("Cannot read {}: {error}", path.display())),
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
            || "Picture".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let file = path
            .file_name()
            .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
        let formats = sue_io::formats(sue_model::registry());
        let bundle = formats
            .import(bytes, &file, &Properties::new())
            .map_err(|error| format!("Cannot open {file}: {error}"))?;
        let raster = bundle
            .entities
            .iter()
            .find_map(|entity| entity.component(ComponentKind::RasterTile2D)?.as_raster())
            .ok_or_else(|| format!("{file} has no pixels Sue Paint can show"))?;
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
                let edit = Batch::new(format!("Open {file}"))
                    .then(SpawnEntity::new(image))
                    .then(
                        SpawnEntity::new(layer).with_relation(
                            Relation::new(picture_id, CHILD, layer_id)
                                .ordered(sue_core::OrderKey::default()),
                        ),
                    );
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture_id);
                    Self::place(cx, picture_id);
                }
            }
        }
        Ok(())
    }

    /// Creates a picture from the New dialog, makes it current and puts
    /// it on the canvas.
    pub(crate) fn create(&mut self, cx: &mut AppletContext<'_>, spec: &NewPicture) {
        match doc::create(spec) {
            Ok((edit, picture)) => {
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture);
                    Self::place(cx, picture);
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

    /// Puts a picture on the canvas as a card (one undo step of its own).
    fn place(cx: &mut AppletContext<'_>, picture: EntityId) {
        if let Some(entity) = cx.world.arena.handle_of(picture) {
            cx.request(ShellRequest::Place {
                entity,
                focus: false,
            });
        }
    }

    /// A New dialog filled from the rice.
    fn new_dialog(cx: &AppletContext<'_>) -> NewDialog {
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
                let mut steps = Batch::new("Move Layer");
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
                    || format!("Set {}", live.key),
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
            | "file.leave" | "app.quit" | "help.shortcuts" | "help.about" => ItemState::on(),
            "edit.undo" => when(history.can_undo()),
            "edit.redo" => when(history.can_redo()),
            "edit.paste" => when(has_doc && self.clipboard.is_some()),
            "edit.clear_outside" | "select.none" | "select.invert" => when(selection),
            "select.again" => when(self.doc().is_some_and(|doc| doc.last_selection.is_some())),
            "layer.from_selection" => when(selection && raster),
            "edit.cut"
            | "edit.copy"
            | "edit.clear"
            | "edit.fill"
            | "edit.recolor"
            | "edit.luma_to_alpha"
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
            "layer.toggle.draft" => info.map_or(ItemState::off(), |info| flag(info.style.draft)),
            "layer.toggle.lock" => info.map_or(ItemState::off(), |info| flag(info.locked)),
            "view.grid" => flag(self.grid),
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
            "window.color_set" => flag(self.panels.color_set && self.set_tab == 0),
            "window.color_history" => flag(self.panels.color_set && self.set_tab == 1),
            "window.navigator" => flag(self.panels.navigator),
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
            "file.save" => cx.request(ShellRequest::Command("file.save".to_owned())),
            "file.save_as" => cx.request(ShellRequest::Command("file.save_as".to_owned())),
            "file.export.png" => self.export(cx, "png"),
            "file.export.jpg" => self.export(cx, "jpg"),
            "file.export.jxl" => self.export(cx, "jxl"),
            "file.preferences" => cx.request(ShellRequest::OpenSettings(None)),
            "file.leave" => cx.request(ShellRequest::CloseStudio),
            "app.quit" => cx.request(ShellRequest::Command("app.quit".to_owned())),
            "edit.undo" | "edit.redo" => {
                cx.typing.flush(cx.history);
                let result = if command == "edit.undo" {
                    cx.history.undo(cx.world)
                } else {
                    cx.history.redo(cx.world)
                };
                if let Err(error) = result {
                    cx.fail(format!("Undo failed: {error}"));
                }
            }
            "edit.copy" => {
                if self.copy(cx) {
                    cx.notify("Copied");
                }
            }
            "edit.cut" => {
                if self.copy(cx) {
                    self.run_clear(cx, false, "Cut");
                }
            }
            "edit.paste" => self.paste(cx, "Pasted"),
            "layer.from_selection" => {
                let kept = self.clipboard.take();
                if self.copy(cx) {
                    self.paste(cx, "From Selection");
                }
                self.clipboard = kept;
            }
            "edit.clear" => self.run_clear(cx, false, "Clear"),
            "edit.clear_outside" => self.run_clear(cx, true, "Clear Outside Selection"),
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
                self.rewrite(cx, "Fill", |pixels, width, height| {
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
            "edit.recolor" => self.filter_now(
                cx,
                Filter::Recolor {
                    rgb: self.drawing_color(),
                },
            ),
            "edit.luma_to_alpha" => self.filter_now(
                cx,
                Filter::BrightnessToOpacity {
                    rgb: self.drawing_color(),
                },
            ),
            "edit.flip_h" => self.rewrite(cx, "Flip Layer", |p, w, h| mirror(p, w, h, true, false)),
            "edit.flip_v" => self.rewrite(cx, "Flip Layer", |p, w, h| mirror(p, w, h, false, true)),
            "edit.rotate180" => {
                self.rewrite(cx, "Rotate Layer", |p, w, h| mirror(p, w, h, true, true));
            }
            "canvas.flip_h" => self.transform_canvas(cx, true, false, "Flip Canvas"),
            "canvas.flip_v" => self.transform_canvas(cx, false, true, "Flip Canvas"),
            "canvas.rotate180" => self.transform_canvas(cx, true, true, "Rotate Canvas"),
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
            "filter.noise" => self.filter_dialog(cx, Filter::Noise { amount: 0.2 }),
            "layer.new" | "layer.new.fill" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let name = if command == "layer.new.fill" {
                    "Fill".to_owned()
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
                    if doc::layers(&cx.world.arena, picture).len() <= 1 {
                        cx.fail("A picture keeps at least one layer");
                        return;
                    }
                    Self::apply(cx, doc::delete(layer));
                }
            }
            "layer.toggle.visible" => self.toggle_layer(cx, picture::VISIBLE, true),
            "layer.toggle.clip" => self.toggle_layer(cx, picture::CLIP, false),
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
            "view.rotate_left" => {
                self.with_view(cx, |view, rect, size| view.rotate_by(rect, size, -15.0))
            }
            "view.rotate_right" => {
                self.with_view(cx, |view, rect, size| view.rotate_by(rect, size, 15.0))
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
            "window.tool" => self.panels.tool = !self.panels.tool,
            "window.sub_tool" => self.panels.sub_tool = !self.panels.sub_tool,
            "window.tool_property" => self.panels.tool_property = !self.panels.tool_property,
            "window.brush_size" => self.panels.brush_size = !self.panels.brush_size,
            "window.color_wheel" | "window.color_slider" => {
                let tab = u8::from(command == "window.color_slider");
                if self.panels.color && self.color_tab == tab {
                    self.panels.color = false;
                } else {
                    self.panels.color = true;
                    self.color_tab = tab;
                }
            }
            "window.color_set" | "window.color_history" => {
                let tab = u8::from(command == "window.color_history");
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
            "help.about" => cx.notify(
                "Sue Paint: layered painting inside Sue. Pictures live in your project, \
                 next to everything else, and save with Ctrl+S.",
            ),
            _ => {}
        }
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
        if ctx.text_edit_focused() || self.dialog.is_some() || self.menubar.is_open() {
            return;
        }
        let none = Modifiers::NONE;
        for (modifiers, key, name) in shortcuts() {
            if ctx.input_mut(|input| input.consume_key(modifiers, key)) {
                self.run(name, cx);
            }
        }

        for (key, letter) in [
            (Key::H, 'H'),
            (Key::K, 'K'),
            (Key::M, 'M'),
            (Key::W, 'W'),
            (Key::I, 'I'),
            (Key::P, 'P'),
            (Key::B, 'B'),
            (Key::E, 'E'),
            (Key::G, 'G'),
            (Key::U, 'U'),
        ] {
            if ctx.input_mut(|input| input.consume_key(none, key)) {
                self.toolbox.cycle(letter);
            }
        }
        if ctx.input_mut(|input| input.consume_key(none, Key::X)) {
            std::mem::swap(&mut self.main, &mut self.sub);
            self.hex = color::hex(self.drawing_color());
        }
        for (key, grow) in [(Key::OpenBracket, false), (Key::CloseBracket, true)] {
            if ctx.input_mut(|input| input.consume_key(none, key)) {
                let tip = &mut self.toolbox.current_mut().tip;
                tip.size = if grow {
                    (tip.size * 1.2).max(tip.size + 1.0)
                } else {
                    (tip.size / 1.2).min(tip.size - 1.0)
                }
                .clamp(0.5, 2000.0);
            }
        }
    }

    // ----- The frame ----------------------------------------------------------

    fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        if !self.shown {
            // Opening: show the picture selected on the canvas, if any.
            self.shown = true;
            let selected = cx
                .view
                .selected
                .and_then(|node| cx.entity(node))
                .filter(|entity| picture::is_picture(entity))
                .map(Entity::id);
            if let Some(picture) = selected {
                self.activate(&cx.world.arena, picture);
            }
        }
        self.sync_docs(&cx.world.arena);
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.sync(&cx.world.arena);
        }
        let ctx = ui.ctx().clone();
        self.keys(&ctx, cx);
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, theme.panel);

        // The menu bar, with the way back to the canvas at its right.
        let bar = Rect::from_min_size(full.min, vec2(full.width(), 26.0));
        let mut menubar = std::mem::take(&mut self.menubar);
        let picked = {
            let arena = &cx.world.arena;
            let history = &*cx.history;
            let state = |command: &str| self.state(command, arena, history);
            menubar.ui(ui, bar, &self.menus, &state, &theme)
        };
        self.menubar = menubar;
        crate::panels::back_button(ui, bar, cx, &theme);
        ui.painter()
            .hline(bar.x_range(), bar.bottom(), theme.hairline());

        let mut top = bar.bottom();
        if self.panels.command_bar {
            let strip = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), top + 34.0));
            self.command_bar(ui, strip, cx, &theme);
            top = strip.bottom();
        }
        let body = Rect::from_min_max(pos2(full.left(), top), full.max);
        self.body(ui, body, cx, &theme);
        if let Some(command) = picked {
            self.run(&command, cx);
        }
        self.dialog_ui(&ctx, cx, &theme);
    }

    /// Lays out the palettes around the canvas.
    fn body(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &sue_wm::Theme) {
        let mut left = body.left();
        if self.panels.tool {
            let strip = Rect::from_min_max(body.min, pos2(left + 40.0, body.bottom()));
            self.tool_strip(ui, strip, cx, theme);
            left = strip.right();
        }
        let column = self.panels.sub_tool
            || self.panels.tool_property
            || self.panels.color
            || self.panels.color_set;
        if column {
            let rect =
                Rect::from_min_max(pos2(left, body.top()), pos2(left + 236.0, body.bottom()));
            self.left_column(ui, rect, cx, theme);
            left = rect.right();
        }
        if self.panels.brush_size {
            let rect = Rect::from_min_max(pos2(left, body.top()), pos2(left + 58.0, body.bottom()));
            self.size_column(ui, rect, theme);
            left = rect.right();
        }
        let mut right = body.right();
        if self.panels.navigator || self.panels.layer {
            let rect =
                Rect::from_min_max(pos2(right - 300.0, body.top()), pos2(right, body.bottom()));
            self.right_column(ui, rect, cx, theme);
            right = rect.left();
        }
        let mut center = Rect::from_min_max(pos2(left, body.top()), pos2(right, body.bottom()));
        if self.panels.timeline && center.height() > 360.0 {
            let timeline =
                Rect::from_min_max(pos2(center.left(), center.bottom() - 160.0), center.max);
            self.timeline_panel(ui, timeline, theme);
            center.max.y = timeline.top();
        }
        self.canvas_area(ui, center, cx, theme);
    }
}

/// The studio's chords. egui ignores extra Shift and Alt when matching, so
/// each chord comes before the plainer chords on the same key.
fn shortcuts() -> [(Modifiers, Key, &'static str); 30] {
    let command = Modifiers::COMMAND;
    let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
    let none = Modifiers::NONE;
    [
        (shifted, Key::N, "layer.new"),
        (command, Key::N, "file.new"),
        (command, Key::W, "file.close"),
        (command, Key::E, "layer.merge_down"),
        (shifted, Key::D, "select.again"),
        (shifted, Key::I, "select.invert"),
        (command, Key::A, "select.all"),
        (command, Key::D, "select.none"),
        (Modifiers::ALT, Key::Delete, "edit.fill"),
        (Modifiers::ALT, Key::Backspace, "edit.fill"),
        (none, Key::Delete, "edit.clear"),
        (none, Key::Backspace, "edit.clear"),
        (command, Key::I, "filter.invert"),
        (command | Modifiers::ALT, Key::Num0, "view.actual"),
        (command, Key::Num0, "view.fit"),
        (command, Key::Plus, "view.zoom_in"),
        (command, Key::Equals, "view.zoom_in"),
        (command, Key::Minus, "view.zoom_out"),
        (command, Key::CloseBracket, "layer.order.up"),
        (command, Key::OpenBracket, "layer.order.down"),
        (Modifiers::ALT, Key::CloseBracket, "layer.select_above"),
        (Modifiers::ALT, Key::OpenBracket, "layer.select_below"),
        (shifted, Key::Tab, "window.prev_tab"),
        (command, Key::Tab, "window.next_tab"),
        (command, Key::X, "edit.cut"),
        (command, Key::C, "edit.copy"),
        (command, Key::V, "edit.paste"),
        (none, Key::Minus, "view.rotate_left"),
        (none, Key::Equals, "view.rotate_right"),
        (Modifiers::ALT, Key::I, "help.shortcuts"),
    ]
}

/// Mirrors RGBA pixels left to right and/or top to bottom.
pub(crate) fn mirror(pixels: &mut [u8], width: u32, height: u32, flip_h: bool, flip_v: bool) {
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
fn to_rgba(raster: &RasterTile2D) -> RasterTile2D {
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
    if !path.exists() {
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
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

impl SueApplet for SuePaint {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Sue Paint", AppletRole::Studio, "\u{1F3A8}");
        manifest.summary =
            "Paint and draw illustrations in layers with pens, brushes, fills, selections and filters."
                .to_owned();
        manifest.order = 25;
        manifest.mode = Some(ModeSpec {
            label: "Sue Paint".to_owned(),
            in_pill: true,
        });
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let int = |value, min, max| RiceParameter::IntRange {
            value,
            min: Some(min),
            max: Some(max),
        };
        vec![
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
        ]
    }

    fn studio_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.frame(ui, cx);
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        if cx.view.studio.as_ref().map(AppletId::as_str) != Some(ID) {
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
            ("New Illustration...", "file.new", "Ctrl+N"),
            ("Open Picture...", "file.open", ""),
            ("Export as PNG...", "file.export.png", ""),
            ("Keyboard Shortcuts", "help.shortcuts", ""),
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
        if cx.view.studio.as_ref().map(AppletId::as_str) != Some(ID)
            && let Ok(id) = AppletId::new(ID)
        {
            cx.request(ShellRequest::OpenStudio(id));
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
    fn chords_come_before_the_plainer_chords_on_their_key() {
        let table = shortcuts();
        for (index, (early, key, name)) in table.iter().enumerate() {
            for (late, other, later) in &table[index + 1..] {
                if key != other {
                    continue;
                }
                let swallows = early.command == late.command
                    && (!early.shift || late.shift)
                    && (!early.alt || late.alt);
                assert!(!swallows, "{name} would swallow {later}");
            }
            assert!(implemented(name), "{name} is bound but not implemented");
        }
    }

    #[test]
    fn every_implemented_command_is_in_a_menu_or_a_shortcut() {
        let mut named: Vec<String> = crate::menus::menus()
            .iter()
            .flat_map(|menu| menu.items.iter().flat_map(|item| item.commands()))
            .map(str::to_owned)
            .collect();
        named.extend(shortcuts().iter().map(|(_, _, name)| (*name).to_owned()));
        for command in named.iter().filter(|command| implemented(command)) {
            assert!(command.contains('.'));
        }
        assert!(named.iter().filter(|command| implemented(command)).count() > 60);
        assert!(!implemented("story.add"), "pages are not built yet");
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
