//! Paint: a raster layer on the canvas you draw on with the mouse or a pen.
//! The pixels live in the arena (`RasterTile2D`, RGBA); strokes write into
//! them in place, and before a 64-pixel tile changes for the first time in
//! a stroke its old pixels are kept, so each stroke becomes one undo step
//! that stores only the tiles it touched (LAW 6), never a snapshot.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_core::{
    Arena, AssetHandle, ComponentKind, Entity, PixelFormat, RasterTile2D, ReversibleTransaction,
    TxnError,
};
use sue_io::{JxlPipeline, NativeJxlPipeline};
use sue_platform::write_atomic;
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, ArenaEdit, MediaClass, ModeSpec, NewEntity, NodeId,
    SceneNode, SpawnSpec, SueApplet, World, project_to_timeline, widgets,
};

use crate::util::{file_name, free_path};

/// The applet id.
pub const ID: &str = "paint";

/// Metadata counting edits to a raster, so its texture follows undo too.
pub const REVISION: &str = "revision";

/// Side of an undo tile.
const TILE: u32 = 64;

/// The values the swatch steps through: white to black.
const VALUES: [u8; 5] = [255, 192, 128, 64, 0];

/// What a drag does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Free-hand strokes.
    Brush,
    /// A straight line.
    Line,
    /// A rectangle outline.
    Rect,
    /// Paints the background value back.
    Eraser,
}

impl Tool {
    const ALL: [Self; 4] = [Self::Brush, Self::Line, Self::Rect, Self::Eraser];

    fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Line => "Line",
            Self::Rect => "Rect",
            Self::Eraser => "Eraser",
        }
    }
}

/// The background value of new layers.
const PAPER: u8 = 0x14;

/// One tile's pixels before and after a stroke.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TileDelta {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    before: Vec<u8>,
    after: Vec<u8>,
}

/// The tiles a stroke changed: the stroke's inverse delta.
#[derive(Clone, Debug)]
pub struct PaintTiles {
    handle: AssetHandle,
    tiles: Vec<TileDelta>,
}

fn raster_mut(arena: &mut Arena, handle: AssetHandle) -> Result<&mut RasterTile2D, TxnError> {
    let entity = arena.get_mut(handle)?;
    match entity.component_mut(ComponentKind::RasterTile2D) {
        Some(sue_core::ComponentData::RasterTile2D(tile)) => Ok(tile),
        _ => Err(TxnError::new("the entity has no raster")),
    }
}

fn bump_revision(arena: &mut Arena, handle: AssetHandle) {
    if let Ok(entity) = arena.get_mut(handle) {
        let next = entity
            .meta(REVISION)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        entity.set_meta(REVISION, next.to_string());
    }
}

fn write_tiles(
    arena: &mut Arena,
    handle: AssetHandle,
    tiles: &[TileDelta],
    after: bool,
) -> Result<(), TxnError> {
    let raster = raster_mut(arena, handle)?;
    let stride = raster.width() as usize * 4;
    let bytes = raster
        .bytes_mut()
        .ok_or_else(|| TxnError::new("the raster is not 8-bit"))?;
    for tile in tiles {
        let source = if after { &tile.after } else { &tile.before };
        let row = tile.width as usize * 4;
        for line in 0..tile.height as usize {
            let at = (tile.y as usize + line) * stride + tile.x as usize * 4;
            bytes[at..at + row].copy_from_slice(&source[line * row..(line + 1) * row]);
        }
    }
    bump_revision(arena, handle);
    Ok(())
}

impl ReversibleTransaction<Arena> for PaintTiles {
    fn label(&self) -> String {
        "Paint stroke".to_owned()
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        write_tiles(arena, self.handle, &self.tiles, true)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        write_tiles(arena, self.handle, &self.tiles, false)
    }
}

/// A stroke in progress: the tiles it touched, as they were.
#[derive(Debug, Default)]
struct Stroke2 {
    before: BTreeMap<(u32, u32), Vec<u8>>,
    start: Pos2,
    last: Pos2,
}

/// Brush settings of one layer.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Brush {
    tool: Tool,
    size: f32,
    value: u8,
}

/// Paints a hard-edged, antialiased disc at `center` (raster pixels),
/// keeping the old pixels of every tile it touches first.
fn stamp(tile: &mut RasterTile2D, stroke: &mut Stroke2, center: Pos2, radius: f32, value: u8) {
    let (width, height) = (tile.width(), tile.height());
    let stride = width as usize * 4;
    let reach = radius + 1.0;
    let x0 = (center.x - reach).floor().max(0.0) as u32;
    let y0 = (center.y - reach).floor().max(0.0) as u32;
    let x1 = ((center.x + reach).ceil().max(0.0) as u32).min(width);
    let y1 = ((center.y + reach).ceil().max(0.0) as u32).min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let Some(bytes) = tile.bytes_mut() else {
        return;
    };
    for ty in (y0 / TILE)..=((y1 - 1) / TILE) {
        for tx in (x0 / TILE)..=((x1 - 1) / TILE) {
            stroke.before.entry((tx, ty)).or_insert_with(|| {
                let (w, h) = (TILE.min(width - tx * TILE), TILE.min(height - ty * TILE));
                let mut copy = Vec::with_capacity(w as usize * h as usize * 4);
                for line in 0..h as usize {
                    let at = (ty as usize * TILE as usize + line) * stride
                        + tx as usize * TILE as usize * 4;
                    copy.extend_from_slice(&bytes[at..at + w as usize * 4]);
                }
                copy
            });
        }
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let distance = pos2(x as f32 + 0.5, y as f32 + 0.5).distance(center);
            let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let at = y as usize * stride + x as usize * 4;
            for channel in 0..3 {
                let old = f32::from(bytes[at + channel]);
                bytes[at + channel] = (old + (f32::from(value) - old) * coverage).round() as u8;
            }
            bytes[at + 3] = 255;
        }
    }
}

/// Stamps discs along a segment.
fn segment(
    tile: &mut RasterTile2D,
    stroke: &mut Stroke2,
    from: Pos2,
    to: Pos2,
    radius: f32,
    value: u8,
) {
    let steps = (from.distance(to) / (radius * 0.4).max(0.5))
        .ceil()
        .max(1.0) as usize;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        stamp(tile, stroke, from + (to - from) * t, radius, value);
    }
}

/// The finished stroke's undo step.
fn finish(tile: &RasterTile2D, handle: AssetHandle, stroke: Stroke2) -> PaintTiles {
    let width = tile.width();
    let stride = width as usize * 4;
    let bytes = tile.bytes().unwrap_or_default();
    let tiles = stroke
        .before
        .into_iter()
        .map(|((tx, ty), before)| {
            let (w, h) = (
                TILE.min(width - tx * TILE),
                TILE.min(tile.height() - ty * TILE),
            );
            let mut after = Vec::with_capacity(before.len());
            for line in 0..h as usize {
                let at =
                    (ty as usize * TILE as usize + line) * stride + tx as usize * TILE as usize * 4;
                after.extend_from_slice(&bytes[at..at + w as usize * 4]);
            }
            TileDelta {
                x: tx * TILE,
                y: ty * TILE,
                width: w,
                height: h,
                before,
                after,
            }
        })
        .collect();
    PaintTiles { handle, tiles }
}

/// The image rect of a raster fitted into `area`.
fn fitted(area: Rect, width: u32, height: u32) -> Rect {
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let size = if area.width() / area.height().max(1.0) > aspect {
        vec2(area.height() * aspect, area.height())
    } else {
        vec2(area.width(), area.width() / aspect)
    };
    Rect::from_center_size(area.center(), size)
}

/// Paint layers on the canvas.
#[derive(Debug, Default)]
pub struct Paint {
    brushes: HashMap<NodeId, Brush>,
    strokes: HashMap<NodeId, Stroke2>,
}

impl Paint {
    fn brush(&mut self, node: NodeId, cx: &AppletContext<'_>) -> Brush {
        let size = cx.rice().get_i64("paint.brush_size").unwrap_or(6) as f32;
        *self.brushes.entry(node).or_insert(Brush {
            tool: Tool::Brush,
            size,
            value: 255,
        })
    }

    /// Writes a layer as lossless JPEG XL next to Sue's other exports.
    fn export(cx: &mut AppletContext<'_>, node: NodeId) {
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let name = entity.name().to_owned();
        let Some(tile) = entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
        else {
            return;
        };
        let effort = cx
            .rice()
            .get_i64("io.jxl_effort")
            .and_then(|effort| u8::try_from(effort).ok())
            .unwrap_or(7);
        let bytes = match NativeJxlPipeline.encode_layers(&[tile], effort) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(format!("JPEG XL export failed: {error}"));
                return;
            }
        };
        let dir = export_dir(cx);
        let path = free_path(&dir.join(format!("{name}.jxl")));
        match write_atomic(&path, &bytes) {
            Ok(()) => cx.notify(format!(
                "Exported {} ({} bytes, lossless)",
                file_name(&path),
                bytes.len()
            )),
            Err(error) => cx.fail(format!("Cannot write {}: {error}", path.display())),
        }
    }
}

/// Paints a wave and two bars into `node`s layer as one undo step, the way
/// a mouse would (scripted runs, the self-check and snapshots). Returns the
/// number of undo tiles the strokes touched.
pub fn scribble(cx: &mut AppletContext<'_>, node: NodeId) -> usize {
    let Some(handle) = cx.node(node).map(|node| node.entity) else {
        return 0;
    };
    let Ok(tile) = raster_mut(&mut cx.world.arena, handle) else {
        return 0;
    };
    let (width, height) = (tile.width() as f32, tile.height() as f32);
    let mut stroke = Stroke2::default();
    let wave: Vec<Pos2> = (0..=48)
        .map(|step| {
            let t = step as f32 / 48.0;
            pos2(
                width * (0.08 + 0.84 * t),
                height * (0.55 + 0.25 * (t * std::f32::consts::TAU * 1.5).sin()),
            )
        })
        .collect();
    for pair in wave.windows(2) {
        segment(tile, &mut stroke, pair[0], pair[1], 6.0, 255);
    }
    for (row, value) in [(0.18, 192), (0.26, 128)] {
        segment(
            tile,
            &mut stroke,
            pos2(width * 0.08, height * row),
            pos2(width * 0.42, height * row),
            4.0,
            value,
        );
    }
    let delta = finish(tile, handle, stroke);
    bump_revision(&mut cx.world.arena, handle);
    let touched = delta.tiles.len();
    cx.record(ArenaEdit(delta));
    touched
}

/// Where exports go: `io.convert_dir`, or the home folder.
pub fn export_dir(cx: &AppletContext<'_>) -> PathBuf {
    let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
    if riced.is_empty() {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    } else {
        PathBuf::from(riced)
    }
}

/// A new blank layer of `width` by `height`.
///
/// # Errors
/// If the size is zero.
pub fn blank_layer(width: u32, height: u32) -> Result<RasterTile2D, sue_core::CoreError> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..width as usize * height as usize {
        pixels.extend_from_slice(&[PAPER, PAPER, PAPER, 255]);
    }
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
}

impl SueApplet for Paint {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Paint", AppletRole::Node, "\u{1F58C}");
        manifest.summary = "Paint with a brush, lines, rectangles and an eraser.".to_owned();
        manifest.order = 20;
        manifest.mode = Some(ModeSpec {
            label: "Paint".to_owned(),
            in_pill: true,
        });
        manifest.spawn = Some(SpawnSpec {
            button: "+ Paint".to_owned(),
            label: "New Paint Layer".to_owned(),
        });
        manifest.lane = Some("Paint/Visual Track".to_owned());
        manifest.opens = vec![MediaClass::Image];
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
                "paint.width",
                int(960, 64, 8192),
                "Width of new paint layers (px)",
            ),
            RiceDeclaration::new(
                "paint.height",
                int(600, 64, 8192),
                "Height of new paint layers (px)",
            ),
            RiceDeclaration::new(
                "paint.brush_size",
                int(6, 1, 128),
                "Starting brush size (px)",
            ),
        ]
    }

    fn accepts(&self, entity: &Entity) -> bool {
        entity.meta(REVISION).is_some()
    }

    fn node_size(&self, entity: &Entity) -> egui::Vec2 {
        entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
            .map_or(vec2(640.0, 426.0), |tile| {
                let scale = (640.0 / tile.width().max(1) as f32).min(1.0);
                vec2(
                    tile.width() as f32 * scale,
                    tile.height() as f32 * scale + sue_wm::NODE_TITLE,
                )
            })
    }

    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let width = u32::try_from(cx.rice().get_i64("paint.width").unwrap_or(960)).unwrap_or(960);
        let height = u32::try_from(cx.rice().get_i64("paint.height").unwrap_or(600)).unwrap_or(600);
        let tile = blank_layer(width, height).ok()?;
        let scale = (640.0 / width.max(1) as f32).min(1.0);
        Some(NewEntity {
            name: "Paint Layer".to_owned(),
            base: tile.into(),
            meta: vec![(REVISION.to_owned(), "0".to_owned())],
            size: vec2(
                width as f32 * scale,
                height as f32 * scale + sue_wm::NODE_TITLE,
            ),
            props: Vec::new(),
        })
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let Some(handle) = cx.node(node).map(|n| n.entity) else {
            return;
        };
        let brush = self.brush(node, cx);
        let area = ui.max_rect();
        let Some(entity) = cx.world.arena.get(handle).ok() else {
            return;
        };
        let revision = entity
            .meta(REVISION)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let Some(tile) = entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
        else {
            return;
        };
        let (width, height) = (tile.width(), tile.height());
        let image = fitted(area, width, height);
        let pixel = image.width() / width.max(1) as f32;
        ui.painter().rect_filled(area, 0.0, cx.theme.void);
        let nearest = pixel * cx.view.zoom > 2.0;
        match tile.bytes().and_then(|bytes| {
            cx.textures.texture(
                handle.to_bits(),
                revision,
                [width, height],
                4,
                bytes,
                nearest,
            )
        }) {
            Some(texture) => {
                ui.painter().image(
                    texture,
                    image,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ui.painter()
                    .rect_filled(image, 0.0, Color32::from_gray(PAPER));
            }
        }
        let response = ui.interact(
            image,
            egui::Id::unique(("sue-paint", node.0)),
            Sense::drag(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let to_raster =
            |at: Pos2| pos2((at.x - image.left()) / pixel, (at.y - image.top()) / pixel);
        let radius = (brush.size * 0.5).max(0.5);
        let value = if brush.tool == Tool::Eraser {
            PAPER
        } else {
            brush.value
        };
        let pointer = response.interact_pointer_pos().map(to_raster);
        if response.drag_started()
            && let Some(at) = pointer
        {
            let mut stroke = Stroke2 {
                start: at,
                last: at,
                ..Stroke2::default()
            };
            if matches!(brush.tool, Tool::Brush | Tool::Eraser)
                && let Ok(tile) = raster_mut(&mut cx.world.arena, handle)
            {
                stamp(tile, &mut stroke, at, radius, value);
                bump_revision(&mut cx.world.arena, handle);
            }
            self.strokes.insert(node, stroke);
        }
        if response.dragged()
            && let (Some(at), Some(stroke)) = (pointer, self.strokes.get_mut(&node))
        {
            match brush.tool {
                Tool::Brush | Tool::Eraser => {
                    if let Ok(tile) = raster_mut(&mut cx.world.arena, handle) {
                        segment(tile, stroke, stroke.last, at, radius, value);
                        bump_revision(&mut cx.world.arena, handle);
                    }
                }
                Tool::Line | Tool::Rect => {}
            }
            stroke.last = at;
        }
        // Previews of line and rectangle drags.
        if let Some(stroke) = self.strokes.get(&node) {
            let screen = |at: Pos2| image.min + at.to_vec2() * pixel;
            let preview = Stroke::new(brush.size * pixel, Color32::from_gray(value));
            match brush.tool {
                Tool::Line => {
                    ui.painter()
                        .line_segment([screen(stroke.start), screen(stroke.last)], preview);
                }
                Tool::Rect => {
                    ui.painter().rect_stroke(
                        Rect::from_two_pos(screen(stroke.start), screen(stroke.last)),
                        0.0,
                        preview,
                        egui::StrokeKind::Middle,
                    );
                }
                Tool::Brush | Tool::Eraser => {}
            }
        }
        if response.drag_stopped()
            && let Some(mut stroke) = self.strokes.remove(&node)
            && let Ok(tile) = raster_mut(&mut cx.world.arena, handle)
        {
            let (a, b) = (stroke.start, stroke.last);
            match brush.tool {
                Tool::Line => segment(tile, &mut stroke, a, b, radius, value),
                Tool::Rect => {
                    let corners = [a, pos2(b.x, a.y), b, pos2(a.x, b.y), a];
                    for pair in corners.windows(2) {
                        segment(tile, &mut stroke, pair[0], pair[1], radius, value);
                    }
                }
                Tool::Brush | Tool::Eraser => {}
            }
            let delta = finish(tile, handle, stroke);
            bump_revision(&mut cx.world.arena, handle);
            if !delta.tiles.is_empty() {
                cx.record(ArenaEdit(delta));
            }
        }
        if self.strokes.contains_key(&node) {
            ui.ctx().request_repaint();
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let mut brush = self.brush(node, cx);
        for tool in Tool::ALL {
            if widgets::pill_button(ui, tool.label(), brush.tool == tool, &theme).clicked() {
                brush.tool = tool;
            }
        }
        widgets::pill_divider(ui, &theme);
        ui.label(
            egui::RichText::new("Size")
                .size(theme.small_size)
                .color(theme.muted),
        );
        ui.add(
            egui::Slider::new(&mut brush.size, 1.0..=128.0)
                .logarithmic(true)
                .max_decimals(0)
                .show_value(true),
        );
        let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_gray(brush.value));
        ui.painter()
            .rect_stroke(rect, 4.0, theme.hairline(), egui::StrokeKind::Inside);
        if response
            .on_hover_text("Value: click for the next gray")
            .clicked()
        {
            let next = VALUES
                .iter()
                .position(|value| *value == brush.value)
                .map_or(0, |index| (index + 1) % VALUES.len());
            brush.value = VALUES[next];
        }
        self.brushes.insert(node, brush);
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Export .jxl", false, &theme).clicked() {
            Self::export(cx, node);
        }
        if widgets::pill_button(ui, "Project to Timeline", false, &theme).clicked() {
            project_to_timeline(cx, node, 4.0);
        }
    }

    fn duration(&self, node: &SceneNode, _world: &World) -> f64 {
        node.prop("dur", 4.0)
    }

    fn commands(&self) -> Vec<sue_wm::PaletteEntry> {
        vec![sue_wm::PaletteEntry {
            label: "Export Selected Layer as .jxl".to_owned(),
            command: "export".to_owned(),
            hint: String::new(),
        }]
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        match (name, cx.view.selected) {
            ("export", Some(node)) if cx.node(node).is_some_and(|n| n.applet.as_str() == ID) => {
                Self::export(cx, node);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::UndoStack;

    #[test]
    fn strokes_undo_and_redo_through_their_tiles_only() {
        let mut world = World::default();
        let handle = world.arena.spawn("layer", blank_layer(200, 100).unwrap());
        let mut stroke = Stroke2::default();
        {
            let tile = raster_mut(&mut world.arena, handle).unwrap();
            segment(
                tile,
                &mut stroke,
                pos2(10.0, 10.0),
                pos2(150.0, 20.0),
                4.0,
                255,
            );
        }
        // The stroke crossed three tiles of the first row only.
        assert_eq!(stroke.before.len(), 3);
        let delta = {
            let tile = raster_mut(&mut world.arena, handle).unwrap();
            finish(tile, handle, stroke)
        };
        let pixel = |world: &World| {
            world
                .arena
                .get(handle)
                .unwrap()
                .component(ComponentKind::RasterTile2D)
                .unwrap()
                .as_raster()
                .unwrap()
                .bytes()
                .unwrap()[(15 * 200 + 80) * 4]
        };
        assert_eq!(pixel(&world), 255);
        let mut history: UndoStack<World> = UndoStack::new();
        history.record(ArenaEdit(delta));
        history.undo(&mut world).unwrap();
        assert_eq!(pixel(&world), PAPER);
        history.redo(&mut world).unwrap();
        assert_eq!(pixel(&world), 255);
    }

    #[test]
    fn rasters_fit_their_node() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0));
        let image = fitted(area, 800, 400);
        assert_eq!(image.size(), vec2(400.0, 200.0));
    }
}
