//! The Image Viewer: a picture on the canvas, filling the window. It zooms
//! (fit, 100%, the wheel), pans, and walks through the canvas's other
//! pictures with the arrow keys; it does the simple edits (turn, mirror,
//! crop, resize, brightness and contrast, saturation and hue, grayscale,
//! invert), each one undo step on the very same entity; and it hands the
//! picture on: Save As in any picture format (the File Manager's type
//! list), a lossless JPEG XL re-encode, the Converter, Sue Paint (the
//! same entity becomes a layer) and Sue Writer (into the text).

use std::path::Path;

use egui::{
    Align2, Color32, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2,
    pos2, vec2,
};
use sue_core::txn::{Batch, ReplaceComponent, SetMeta};
use sue_core::{ComponentKind, Entity, EntityId, PixelFormat, RasterTile2D};
use sue_io::meta;
use sue_paint::filters::Filter;
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaEdit, FilePick, FileType, ModeSpec,
    PaletteEntry, ShellRequest, SueAction, SueApplet, Theme, wheel, widgets,
};

use crate::util::human_bytes;

/// The applet id.
pub const ID: &str = "image-viewer";

fn me() -> AppletId {
    AppletId::new(ID).expect("a valid id")
}

/// The picture types the viewer saves as, its own format first.
fn save_types(first: Option<&str>) -> Vec<FileType> {
    let mut types: Vec<FileType> = sue_io::PICTURE_SAVE_TYPES
        .iter()
        .map(|(name, extensions)| FileType::new(name, extensions))
        .collect();
    if let Some(first) = first
        && let Some(at) = types
            .iter()
            .position(|kind| kind.extensions.iter().any(|ext| ext == first))
    {
        let kind = types.remove(at);
        types.insert(0, kind);
    }
    types
}

/// What dragging on the picture does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    /// Pans.
    Hand,
    /// Draws the rectangle to crop to.
    Crop,
}

/// Adjustments being previewed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Adjust {
    brightness: f32,
    contrast: f32,
    saturation: f32,
    hue: f32,
}

impl Adjust {
    const NONE: Self = Self {
        brightness: 0.0,
        contrast: 0.0,
        saturation: 0.0,
        hue: 0.0,
    };

    fn apply(&self, pixels: &mut [u8], width: u32, height: u32) {
        if self.brightness != 0.0 || self.contrast != 0.0 {
            Filter::BrightnessContrast {
                brightness: self.brightness,
                contrast: self.contrast,
            }
            .apply(pixels, width, height, None, [0, 0]);
        }
        if self.saturation != 0.0 || self.hue != 0.0 {
            Filter::HueSaturation {
                hue: self.hue,
                saturation: self.saturation,
                luminosity: 0.0,
            }
            .apply(pixels, width, height, None, [0, 0]);
        }
    }
}

/// The Image Viewer.
#[derive(Debug)]
pub struct Viewer {
    current: Option<EntityId>,
    fit: bool,
    zoom: f32,
    pan: Vec2,
    tool: Tool,
    crop: Option<[f32; 4]>,
    drag_from: Option<Pos2>,
    adjust: Option<Adjust>,
    /// The adjust preview: its pixels, size and version.
    preview: Option<(Vec<u8>, [u32; 2], u64)>,
    resize: Option<(u32, u32, bool)>,
    replace: bool,
    shown: bool,
    /// The last revision shown, to keep the texture fresh.
    seen: u64,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            current: None,
            fit: true,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tool: Tool::Hand,
            crop: None,
            drag_from: None,
            adjust: None,
            preview: None,
            resize: None,
            replace: false,
            shown: false,
            seen: 0,
        }
    }
}

/// An 8-bit picture as straight RGBA, whatever it was.
fn rgba(raster: &RasterTile2D) -> Option<Vec<u8>> {
    let bytes = raster.bytes()?;
    match raster.format() {
        PixelFormat::Rgba8 => Some(bytes.to_vec()),
        PixelFormat::Gray8 => Some(bytes.iter().flat_map(|v| [*v, *v, *v, 255]).collect()),
        PixelFormat::RgbaF32 => None,
    }
}

/// Turns RGBA `pixels` a quarter turn (`clockwise` or not).
fn rotate(pixels: &[u8], width: u32, height: u32, clockwise: bool) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = if clockwise {
                (h - 1 - y, x)
            } else {
                (y, w - 1 - x)
            };
            let from = (y * w + x) * 4;
            let to = (ny * h + nx) * 4;
            out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }
    out
}

/// Resamples RGBA `pixels` to `to` with a box filter (shrinking) or
/// bilinear sampling (growing).
fn resample(pixels: &[u8], [width, height]: [u32; 2], to: [u32; 2]) -> Vec<u8> {
    let (tw, th) = (to[0].max(1), to[1].max(1));
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    let sx = width as f32 / tw as f32;
    let sy = height as f32 / th as f32;
    for y in 0..th {
        for x in 0..tw {
            if sx > 1.0 || sy > 1.0 {
                let x0 = (x as f32 * sx) as u32;
                let y0 = (y as f32 * sy) as u32;
                let x1 = (((x + 1) as f32 * sx) as u32).clamp(x0 + 1, width);
                let y1 = (((y + 1) as f32 * sy) as u32).clamp(y0 + 1, height);
                let mut sum = [0_u32; 4];
                let mut count = 0;
                for row in y0..y1 {
                    for column in x0..x1 {
                        let at = ((row * width + column) * 4) as usize;
                        for (total, value) in sum.iter_mut().zip(&pixels[at..at + 4]) {
                            *total += u32::from(*value);
                        }
                        count += 1;
                    }
                }
                out.extend(sum.map(|total| (total / count.max(1)) as u8));
            } else {
                let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
                let (x0, y0) = (fx as u32, fy as u32);
                let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let at = |cx: u32, cy: u32| ((cy * width + cx) * 4) as usize;
                for channel in 0..4 {
                    let a = f32::from(pixels[at(x0, y0) + channel]);
                    let b = f32::from(pixels[at(x1, y0) + channel]);
                    let c = f32::from(pixels[at(x0, y1) + channel]);
                    let d = f32::from(pixels[at(x1, y1) + channel]);
                    let top = a + (b - a) * tx;
                    let bottom = c + (d - c) * tx;
                    out.push((top + (bottom - top) * ty).round() as u8);
                }
            }
        }
    }
    out
}

impl Viewer {
    fn raster<'a>(&self, cx: &'a AppletContext<'_>) -> Option<&'a RasterTile2D> {
        cx.arena()
            .get_by_id(self.current?)
            .ok()?
            .component(ComponentKind::RasterTile2D)?
            .as_raster()
    }

    /// The pictures on the canvas, in the order they were placed.
    fn pictures(cx: &AppletContext<'_>) -> Vec<EntityId> {
        let scene = &cx.world.scene;
        scene
            .stacked()
            .into_iter()
            .filter_map(|node| scene.get(node))
            .filter_map(|node| cx.world.arena.get(node.entity).ok())
            .filter(|entity| crate::image::is_picture(entity))
            .map(Entity::id)
            .collect()
    }

    /// Changes the picture's pixels to `pixels` of `size`, as one undo step.
    fn replace(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        pixels: Vec<u8>,
        size: [u32; 2],
    ) {
        let Some(id) = self.current else {
            return;
        };
        let raster = match RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels) {
            Ok(raster) => raster,
            Err(error) => {
                cx.fail(error.to_string());
                return;
            }
        };
        let batch = Batch::new(label)
            .then(ReplaceComponent::new(id, raster, label))
            .then(SetMeta::new(id, meta::WIDTH, size[0].to_string()))
            .then(SetMeta::new(id, meta::HEIGHT, size[1].to_string()));
        if let Err(error) = cx.apply(ArenaEdit(batch)) {
            cx.fail(error.to_string());
        }
        self.crop = None;
    }

    /// Runs an edit over the whole picture.
    fn edit(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        change: impl FnOnce(Vec<u8>, [u32; 2]) -> (Vec<u8>, [u32; 2]),
    ) {
        let Some(raster) = self.raster(cx) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(pixels) = rgba(raster) else {
            cx.fail("This picture's pixels cannot be edited here");
            return;
        };
        let (pixels, size) = change(pixels, size);
        self.replace(cx, label, pixels, size);
    }

    /// Runs `command`. Returns `false` if there is no such command.
    #[allow(
        clippy::too_many_lines,
        reason = "one match over the viewer's commands"
    )]
    fn run(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        match command {
            "viewer.fit" => self.fit = true,
            "viewer.actual" => {
                self.fit = false;
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            "viewer.zoom_in" | "viewer.zoom_out" => {
                let factor = if command.ends_with("in") { 1.25 } else { 0.8 };
                self.zoom = (self.zoom * factor).clamp(0.02, 64.0);
                self.fit = false;
            }
            "viewer.rotate_right" | "viewer.rotate_left" => {
                let clockwise = command.ends_with("right");
                self.edit(
                    cx,
                    if clockwise {
                        "Rotate Right"
                    } else {
                        "Rotate Left"
                    },
                    |pixels, [w, h]| (rotate(&pixels, w, h, clockwise), [h, w]),
                );
            }
            "viewer.flip_h" | "viewer.flip_v" => {
                let horizontal = command.ends_with('h');
                self.edit(
                    cx,
                    if horizontal {
                        "Flip Horizontal"
                    } else {
                        "Flip Vertical"
                    },
                    |mut pixels, size| {
                        sue_paint::mirror(&mut pixels, size[0], size[1], horizontal, !horizontal);
                        (pixels, size)
                    },
                );
            }
            "viewer.grayscale" => {
                self.edit(cx, "Grayscale", |mut pixels, size| {
                    for pixel in pixels.as_chunks_mut::<4>().0 {
                        let y = (0.2126 * f32::from(pixel[0])
                            + 0.7152 * f32::from(pixel[1])
                            + 0.0722 * f32::from(pixel[2]))
                        .round() as u8;
                        pixel[..3].fill(y);
                    }
                    (pixels, size)
                });
            }
            "viewer.invert" => {
                self.edit(cx, "Invert", |mut pixels, size| {
                    Filter::Invert.apply(&mut pixels, size[0], size[1], None, [0, 0]);
                    (pixels, size)
                });
            }
            "viewer.crop" => {
                self.tool = if self.tool == Tool::Crop {
                    Tool::Hand
                } else {
                    Tool::Crop
                };
                self.crop = None;
            }
            "viewer.crop_apply" => {
                if let Some([x, y, w, h]) = self.crop {
                    self.edit(cx, "Crop", |pixels, [width, height]| {
                        let (x0, y0) = (x.max(0.0) as u32, y.max(0.0) as u32);
                        let x1 = ((x + w) as u32).min(width).max(x0 + 1);
                        let y1 = ((y + h) as u32).min(height).max(y0 + 1);
                        let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
                        for row in y0..y1 {
                            let at = ((row * width + x0) * 4) as usize;
                            out.extend_from_slice(&pixels[at..at + ((x1 - x0) * 4) as usize]);
                        }
                        (out, [x1 - x0, y1 - y0])
                    });
                    self.tool = Tool::Hand;
                }
            }
            "viewer.adjust" => {
                self.adjust = if self.adjust.is_some() {
                    None
                } else {
                    Some(Adjust::NONE)
                };
                self.preview = None;
            }
            "viewer.resize" => {
                if let Some(raster) = self.raster(cx) {
                    self.resize = Some((raster.width(), raster.height(), true));
                }
            }
            "viewer.save_as" | "viewer.reencode" => {
                let Some(entity) = self.current.and_then(|id| cx.arena().get_by_id(id).ok()) else {
                    return true;
                };
                let stem = Path::new(entity.name()).file_stem().map_or_else(
                    || "Picture".to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                );
                let source = entity.meta(meta::SOURCE).map(std::path::PathBuf::from);
                let first = if command == "viewer.reencode" {
                    Some("jxl")
                } else {
                    source
                        .as_ref()
                        .and_then(|path| path.extension())
                        .and_then(|ext| ext.to_str())
                        .map(|ext| {
                            if ext.eq_ignore_ascii_case("jpeg") {
                                "jpg"
                            } else {
                                ext
                            }
                        })
                };
                let types = save_types(first.map(str::to_ascii_lowercase).as_deref());
                let ext = types
                    .first()
                    .and_then(|kind| kind.extensions.first())
                    .cloned()
                    .unwrap_or_else(|| "png".to_owned());
                let start = source
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(Path::to_path_buf)
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                let title = if command == "viewer.reencode" {
                    "Re-encode as JPEG XL"
                } else {
                    "Save Picture As"
                };
                let pick = FilePick::save(Some(me()), "save_as", title, &format!("{stem}.{ext}"))
                    .types(types)
                    .starting_in(start);
                cx.request(ShellRequest::PickFile(pick));
            }
            "viewer.save" => self.replace = true,
            "viewer.convert" => {
                if let Some(id) = self.current
                    && let Ok(convert) = AppletId::new(crate::convert::ID)
                {
                    cx.request(ShellRequest::SendEntity {
                        to: convert,
                        purpose: "add".to_owned(),
                        entity: id,
                    });
                }
            }
            "viewer.paint" => {
                if let Some(id) = self.current
                    && let Ok(paint) = AppletId::new(sue_paint::ID)
                {
                    cx.request(ShellRequest::SendEntity {
                        to: paint,
                        purpose: "adopt".to_owned(),
                        entity: id,
                    });
                }
            }
            "viewer.writer" => {
                if let Some(id) = self.current
                    && let Ok(writer) = AppletId::new(sue_writer::ID)
                {
                    cx.request(ShellRequest::SendEntity {
                        to: writer,
                        purpose: "figure".to_owned(),
                        entity: id,
                    });
                }
            }
            "viewer.show" => {
                if let Some(path) = self
                    .current
                    .and_then(|id| cx.arena().get_by_id(id).ok())
                    .and_then(|entity| entity.meta(meta::SOURCE))
                {
                    cx.request(ShellRequest::Command(format!("reveal-file:{path}")));
                }
            }
            "viewer.next" | "viewer.previous" => {
                let pictures = Self::pictures(cx);
                if let Some(at) = self
                    .current
                    .and_then(|id| pictures.iter().position(|known| *known == id))
                {
                    let step = if command.ends_with("next") {
                        1
                    } else {
                        pictures.len() - 1
                    };
                    self.show(pictures[(at + step) % pictures.len()]);
                }
            }
            "viewer.leave" => cx.request(ShellRequest::CloseStudio),
            _ => return false,
        }
        true
    }

    fn show(&mut self, id: EntityId) {
        self.current = Some(id);
        self.fit = true;
        self.pan = Vec2::ZERO;
        self.crop = None;
        self.adjust = None;
        self.preview = None;
        self.resize = None;
        self.tool = Tool::Hand;
    }

    fn keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        if ctx.text_edit_focused() {
            return;
        }
        let none = Modifiers::NONE;
        let taken = ctx.input_mut(|input| {
            sue_wm::shortcuts::take(input, cx.rice(), KEYS_SECTION, shortcut_table())
        });
        for command in taken {
            self.run(command, cx);
        }
        if ctx.input_mut(|input| input.consume_key(none, Key::Escape)) {
            if self.tool == Tool::Crop || self.adjust.is_some() || self.resize.is_some() {
                self.tool = Tool::Hand;
                self.crop = None;
                self.adjust = None;
                self.resize = None;
            } else {
                cx.request(ShellRequest::CloseStudio);
            }
        }
        if self.tool == Tool::Crop
            && self.crop.is_some()
            && ctx.input_mut(|input| input.consume_key(none, Key::Enter))
        {
            self.run("viewer.crop_apply", cx);
        }
    }

    #[allow(clippy::too_many_lines, reason = "the viewer's whole window")]
    fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let present = cx.view.studio.as_ref().map(AppletId::as_str) == Some(ID);
        let reveal = cx.view.studio_reveal.clamp(0.0, 1.0);
        if present && !self.shown {
            self.shown = true;
            let selected = cx
                .view
                .selected
                .and_then(|node| cx.entity(node))
                .filter(|entity| crate::image::is_picture(entity))
                .map(Entity::id);
            if let Some(id) = selected {
                self.show(id);
            }
        }
        if self
            .current
            .is_some_and(|id| cx.arena().get_by_id(id).is_err())
        {
            self.current = None;
        }
        let ctx = ui.ctx().clone();
        if present && reveal >= 0.999 {
            self.keys(&ctx, cx);
        }
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        // The picture lines up with its card as the studio comes in; the
        // bars slide in from the edges.
        let hide = 1.0 - reveal * reveal * (3.0 - 2.0 * reveal);
        let ground = Color32::from_gray(0x16);
        let cover = (reveal / 0.35).clamp(0.0, 1.0);
        ui.painter()
            .rect_filled(full, 0.0, ground.gamma_multiply(cover));
        let bar_height = 40.0;
        let bar = Rect::from_min_size(
            full.min - vec2(0.0, hide * bar_height),
            vec2(full.width(), bar_height),
        );
        let info_height = 26.0;
        let info = Rect::from_min_max(
            pos2(
                full.left(),
                full.bottom() - info_height + hide * info_height,
            ),
            pos2(full.right(), full.bottom() + hide * info_height),
        );
        let side = if self.adjust.is_some() || self.resize.is_some() {
            260.0
        } else {
            0.0
        };
        let panel = Rect::from_min_max(
            pos2(full.right() - side + hide * side, bar.bottom()),
            pos2(full.right() + hide * side, info.top()),
        );
        let view = Rect::from_min_max(
            pos2(full.left(), full.top() + bar_height),
            pos2(full.right() - side, full.bottom() - info_height),
        );
        self.picture_ui(ui, view, cx, &theme, cover);
        self.bar_ui(ui, bar, cx, &theme);
        self.info_ui(ui, info, cx, &theme);
        if side > 0.0 {
            self.panel_ui(ui, panel, cx, &theme);
        }
        if self.replace {
            self.replace_ui(&ctx, cx, &theme);
        }
    }

    fn picture_ui(
        &mut self,
        ui: &mut Ui,
        view: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        cover: f32,
    ) {
        let Some(id) = self.current else {
            ui.painter().text(
                view.center(),
                Align2::CENTER_CENTER,
                "Double-click a picture on the canvas to see it here.",
                theme.font(theme.ui_size + 2.0),
                theme.muted,
            );
            return;
        };
        let Some(entity) = cx.world.arena.get_by_id(id).ok() else {
            return;
        };
        let revision = entity.revision();
        let Some(raster) = entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
        else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let channels = match raster.format() {
            PixelFormat::Gray8 => 1,
            PixelFormat::Rgba8 => 4,
            PixelFormat::RgbaF32 => 0,
        };
        // The adjust preview is made from a smaller copy, so sliders move
        // smoothly on large photos.
        if let Some(adjust) = self.adjust {
            let version = revision
                ^ (adjust.brightness.to_bits() as u64).rotate_left(7)
                ^ (adjust.contrast.to_bits() as u64).rotate_left(19)
                ^ (adjust.saturation.to_bits() as u64).rotate_left(31)
                ^ (adjust.hue.to_bits() as u64).rotate_left(43);
            if self
                .preview
                .as_ref()
                .is_none_or(|(_, _, known)| *known != version)
                && let Some(pixels) = rgba(raster)
            {
                let scale = (1600.0 / size[0].max(size[1]) as f32).min(1.0);
                let small = [
                    ((size[0] as f32 * scale) as u32).max(1),
                    ((size[1] as f32 * scale) as u32).max(1),
                ];
                let mut pixels = if scale < 1.0 {
                    resample(&pixels, size, small)
                } else {
                    pixels
                };
                adjust.apply(&mut pixels, small[0], small[1]);
                self.preview = Some((pixels, small, version));
            }
        }
        let response = ui.interact(
            view,
            egui::Id::unique("sue-viewer-picture"),
            Sense::click_and_drag(),
        );
        let fit = (view.width() / size[0].max(1) as f32)
            .min(view.height() / size[1].max(1) as f32)
            .min(1.0)
            * 0.96;
        // The wheel zooms about the pointer.
        if response.hovered() {
            let scroll =
                ui.input(|input| input.smooth_scroll_delta.y + input.zoom_delta().ln() * 240.0);
            if scroll.abs() > 0.1 {
                let before = if self.fit { fit } else { self.zoom };
                let after = (before * (scroll / 240.0).exp()).clamp(0.02, 64.0);
                if let Some(pointer) = response.hover_pos() {
                    let center = view.center() + self.pan;
                    self.pan += (pointer - center) * (1.0 - after / before);
                }
                self.zoom = after;
                self.fit = false;
            }
        }
        let zoom = if self.fit { fit } else { self.zoom };
        if self.fit {
            self.pan = Vec2::ZERO;
        }
        let shown = Rect::from_center_size(
            view.center() + self.pan,
            vec2(size[0] as f32, size[1] as f32) * zoom,
        );
        let painter = ui.painter().with_clip_rect(view);
        // A checkerboard under clear pixels.
        let cell = 12.0;
        let board = shown.intersect(view);
        if board.is_positive() {
            painter.rect_filled(board, 0.0, Color32::from_gray(0xd8).gamma_multiply(cover));
            let first = ((board.left() - shown.left()) / cell).floor() as i64;
            let top = ((board.top() - shown.top()) / cell).floor() as i64;
            let columns = (board.width() / cell).ceil() as i64 + 2;
            let rows = (board.height() / cell).ceil() as i64 + 2;
            if columns * rows < 40_000 {
                for row in top..top + rows {
                    for column in first..first + columns {
                        if (row + column) % 2 == 0 {
                            let square = Rect::from_min_size(
                                shown.min + vec2(column as f32 * cell, row as f32 * cell),
                                Vec2::splat(cell),
                            )
                            .intersect(board);
                            if square.is_positive() {
                                painter.rect_filled(
                                    square,
                                    0.0,
                                    Color32::from_gray(0xf4).gamma_multiply(cover),
                                );
                            }
                        }
                    }
                }
            }
        }
        let nearest = zoom > 2.0;
        let texture = match &self.preview {
            Some((pixels, small, version)) if self.adjust.is_some() => {
                cx.textures
                    .texture(texture_key(id) ^ 1, *version, *small, 4, pixels, nearest)
            }
            _ => raster.bytes().filter(|_| channels > 0).and_then(|bytes| {
                cx.textures
                    .texture(texture_key(id), revision, size, channels, bytes, nearest)
            }),
        };
        self.seen = revision;
        match texture {
            Some(texture) => {
                painter.image(
                    texture,
                    shown,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(cover),
                );
            }
            None => {
                painter.rect_filled(shown, 0.0, theme.pill);
                painter.text(
                    shown.center(),
                    Align2::CENTER_CENTER,
                    format!("{} x {}", size[0], size[1]),
                    theme.font(theme.ui_size),
                    theme.muted,
                );
            }
        }
        // Dragging pans, or draws the crop.
        let to_image = |pos: Pos2| {
            let local = (pos - shown.min) / zoom;
            pos2(
                local.x.clamp(0.0, size[0] as f32),
                local.y.clamp(0.0, size[1] as f32),
            )
        };
        match self.tool {
            Tool::Hand => {
                if response.dragged() {
                    self.pan += response.drag_delta();
                    self.fit = false;
                    self.zoom = zoom;
                }
                if response.double_clicked() {
                    if self.fit {
                        self.run("viewer.actual", cx);
                    } else {
                        self.fit = true;
                    }
                }
            }
            Tool::Crop => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                if response.drag_started() {
                    self.drag_from = response.interact_pointer_pos().map(to_image);
                }
                if response.dragged()
                    && let (Some(from), Some(pos)) =
                        (self.drag_from, response.interact_pointer_pos())
                {
                    let to = to_image(pos);
                    let (x0, x1) = (from.x.min(to.x).round(), from.x.max(to.x).round());
                    let (y0, y1) = (from.y.min(to.y).round(), from.y.max(to.y).round());
                    self.crop = Some([x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)]);
                }
            }
        }
        if let Some([x, y, w, h]) = self.crop {
            let rect = Rect::from_min_size(shown.min + vec2(x, y) * zoom, vec2(w, h) * zoom);
            let dim = Color32::from_black_alpha(140);
            for outside in [
                Rect::from_min_max(shown.min, pos2(shown.right(), rect.top())),
                Rect::from_min_max(pos2(shown.left(), rect.bottom()), shown.max),
                Rect::from_min_max(
                    pos2(shown.left(), rect.top()),
                    pos2(rect.left(), rect.bottom()),
                ),
                Rect::from_min_max(
                    pos2(rect.right(), rect.top()),
                    pos2(shown.right(), rect.bottom()),
                ),
            ] {
                painter.rect_filled(outside, 0.0, dim);
            }
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.5, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.left_top() - vec2(0.0, 4.0),
                Align2::LEFT_BOTTOM,
                format!("{w:.0} x {h:.0}  (Enter crops)"),
                theme.font(theme.small_size),
                Color32::WHITE,
            );
        }
    }

    fn bar_ui(&mut self, ui: &mut Ui, bar: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(bar, 0.0, theme.panel);
        ui.painter()
            .hline(bar.x_range(), bar.bottom(), theme.hairline());
        let mut run: Option<&'static str> = None;
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(bar.shrink2(vec2(8.0, 5.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.set_clip_rect(bar.intersect(ui.clip_rect()));
        let ui = &mut row;
        ui.spacing_mut().item_spacing.x = 4.0;
        let has = self.current.is_some();
        let mut button = |ui: &mut Ui, label: &str, command: &'static str, hint: &str, on: bool| {
            if ui
                .add_enabled_ui(has, |ui| {
                    widgets::pill_button(ui, label, on, theme).on_hover_text(hint)
                })
                .inner
                .clicked()
            {
                run = Some(command);
            }
        };
        button(
            ui,
            "Fit",
            "viewer.fit",
            "Fit the picture to the window (0)",
            self.fit,
        );
        button(
            ui,
            "100%",
            "viewer.actual",
            "One picture pixel per screen pixel (1)",
            !self.fit && (self.zoom - 1.0).abs() < 0.01,
        );
        button(ui, "\u{2212}", "viewer.zoom_out", "Zoom out (-)", false);
        button(ui, "+", "viewer.zoom_in", "Zoom in (+)", false);
        widgets::pill_divider(ui, theme);
        button(
            ui,
            "\u{27F2}",
            "viewer.rotate_left",
            "Turn left (Shift+R)",
            false,
        );
        button(
            ui,
            "\u{27F3}",
            "viewer.rotate_right",
            "Turn right (R)",
            false,
        );
        button(
            ui,
            "Flip \u{2194}",
            "viewer.flip_h",
            "Mirror left to right",
            false,
        );
        button(
            ui,
            "Flip \u{2195}",
            "viewer.flip_v",
            "Mirror top to bottom",
            false,
        );
        button(
            ui,
            "Crop",
            "viewer.crop",
            "Drag a rectangle, then Enter (C)",
            self.tool == Tool::Crop,
        );
        button(
            ui,
            "Adjust",
            "viewer.adjust",
            "Brightness, contrast, saturation and hue",
            self.adjust.is_some(),
        );
        button(
            ui,
            "Resize",
            "viewer.resize",
            "Change the picture's size",
            self.resize.is_some(),
        );
        button(ui, "Grayscale", "viewer.grayscale", "Turn it gray", false);
        button(
            ui,
            "Invert",
            "viewer.invert",
            "Turn every color into its opposite",
            false,
        );
        widgets::pill_divider(ui, theme);
        button(
            ui,
            "Save As...",
            "viewer.save_as",
            "Save as any picture format (Ctrl+Shift+S)",
            false,
        );
        ui.add_enabled_ui(has, |ui| {
            ui.menu_button(RichText::new("More").size(theme.ui_size), |ui| {
                for (label, command) in [
                    ("Save Over the File...", "viewer.save"),
                    ("Re-encode as JPEG XL...", "viewer.reencode"),
                    ("Convert...", "viewer.convert"),
                    ("Open in Sue Paint", "viewer.paint"),
                    ("Put in Sue Writer", "viewer.writer"),
                    ("Show in Folder", "viewer.show"),
                ] {
                    if ui.button(label).clicked() {
                        run = Some(command);
                        ui.close();
                    }
                }
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pill_button(ui, "Back to Canvas", false, theme)
                .on_hover_text("Esc")
                .clicked()
            {
                run = Some("viewer.leave");
            }
            if widgets::pill_button(ui, "\u{203A}", false, theme)
                .on_hover_text("Next picture on the canvas (Right)")
                .clicked()
            {
                run = Some("viewer.next");
            }
            if widgets::pill_button(ui, "\u{2039}", false, theme)
                .on_hover_text("Previous picture on the canvas (Left)")
                .clicked()
            {
                run = Some("viewer.previous");
            }
        });
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    fn info_ui(&self, ui: &mut Ui, info: Rect, cx: &AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(info, 0.0, theme.panel);
        ui.painter()
            .hline(info.x_range(), info.top(), theme.hairline());
        let Some(entity) = self.current.and_then(|id| cx.arena().get_by_id(id).ok()) else {
            return;
        };
        let raster = entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster());
        let mut parts = vec![entity.name().to_owned()];
        if let Some(raster) = raster {
            parts.push(format!("{} x {} px", raster.width(), raster.height()));
        }
        if let Some(format) = entity.meta(meta::FORMAT) {
            parts.push(format.to_owned());
        }
        if let Some(bytes) = entity
            .meta(meta::BYTES)
            .and_then(|bytes| bytes.parse::<u64>().ok())
        {
            parts.push(human_bytes(bytes));
        }
        if let Some(path) = entity.meta(meta::SOURCE) {
            parts.push(path.to_owned());
        }
        let pictures = Self::pictures(cx);
        if let Some(at) = pictures.iter().position(|id| Some(*id) == self.current) {
            parts.push(format!("{} of {}", at + 1, pictures.len()));
        }
        let text = parts.join("    |    ");
        ui.painter().with_clip_rect(info).text(
            info.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
    }

    fn panel_ui(&mut self, ui: &mut Ui, panel: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(panel, 0.0, theme.panel);
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(panel.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut child;
        let mut apply_adjust = false;
        let mut apply_resize = None;
        if let Some(adjust) = &mut self.adjust {
            widgets::heading(ui, "Adjust", theme);
            for (label, value, range) in [
                ("Brightness", &mut adjust.brightness, -1.0..=1.0),
                ("Contrast", &mut adjust.contrast, -1.0..=1.0),
                ("Saturation", &mut adjust.saturation, -1.0..=1.0),
                ("Hue", &mut adjust.hue, -180.0..=180.0),
            ] {
                ui.label(RichText::new(label).color(theme.muted));
                ui.add(egui::Slider::new(value, range).show_value(true));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    apply_adjust = true;
                }
                if ui.button("Reset").clicked() {
                    *adjust = Adjust::NONE;
                }
            });
        }
        if let Some((width, height, keep)) = &mut self.resize {
            let original = self
                .current
                .and_then(|id| cx.arena().get_by_id(id).ok())
                .and_then(|entity| entity.component(ComponentKind::RasterTile2D))
                .and_then(|data| data.as_raster())
                .map(|raster| [raster.width(), raster.height()]);
            widgets::heading(ui, "Resize", theme);
            let aspect = original.map_or(1.0, |[w, h]| w as f32 / h.max(1) as f32);
            ui.horizontal(|ui| {
                ui.label("Width");
                if ui
                    .add(egui::DragValue::new(width).range(1..=32_768).suffix(" px"))
                    .changed()
                    && *keep
                {
                    *height = ((*width as f32 / aspect).round() as u32).max(1);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Height");
                if ui
                    .add(egui::DragValue::new(height).range(1..=32_768).suffix(" px"))
                    .changed()
                    && *keep
                {
                    *width = ((*height as f32 * aspect).round() as u32).max(1);
                }
            });
            ui.checkbox(keep, "Keep the proportions");
            ui.horizontal(|ui| {
                for percent in [25_u32, 50, 200] {
                    if ui.small_button(format!("{percent}%")).clicked()
                        && let Some([w, h]) = original
                    {
                        *width = (w * percent / 100).max(1);
                        *height = (h * percent / 100).max(1);
                    }
                }
            });
            if ui.button("Resize").clicked() {
                apply_resize = Some([*width, *height]);
            }
        }
        if apply_adjust && let Some(adjust) = self.adjust.take() {
            self.preview = None;
            self.edit(cx, "Adjust", |mut pixels, size| {
                adjust.apply(&mut pixels, size[0], size[1]);
                (pixels, size)
            });
        }
        if let Some(to) = apply_resize {
            self.resize = None;
            self.edit(cx, "Resize", |pixels, size| {
                (resample(&pixels, size, to), to)
            });
        }
    }

    fn replace_ui(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>, theme: &Theme) {
        let source = self
            .current
            .and_then(|id| cx.arena().get_by_id(id).ok())
            .and_then(|entity| entity.meta(meta::SOURCE))
            .map(std::path::PathBuf::from);
        let mut close = false;
        egui::Modal::new(egui::Id::unique("sue-viewer-replace"))
            .area(egui::Modal::default_area(egui::Id::unique("sue-viewer-replace")).fade_in(false))
            .frame(widgets::window_frame(theme).inner_margin(16))
            .show(ctx, |ui| match &source {
                Some(path) => {
                    ui.label(
                        RichText::new("Save over the original file?").size(theme.heading_size),
                    );
                    ui.label(RichText::new(path.display().to_string()).color(theme.muted));
                    ui.label("The file is replaced with the picture as it is now.");
                    ui.horizontal(|ui| {
                        if ui.button("Save Over It").clicked() {
                            self.write_to(cx, path);
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                None => {
                    ui.label("This picture has no file of its own yet. Use Save As.");
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                }
            });
        if close || ctx.input(|input| input.key_pressed(Key::Escape)) {
            self.replace = false;
        }
    }

    /// Writes the picture to `path` in the format its extension names.
    fn write_to(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(raster) = self.raster(cx) else {
            return;
        };
        let ext = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "png".to_owned());
        let quality = cx
            .rice()
            .get_i64("io.jpeg_quality")
            .and_then(|quality| u8::try_from(quality).ok())
            .unwrap_or(90);
        let effort = cx
            .rice()
            .get_i64("io.jxl_effort")
            .and_then(|effort| u8::try_from(effort).ok())
            .unwrap_or(7);
        match sue_io::encode_as(raster, &ext, quality, effort) {
            Ok(bytes) => match sue_platform::write_atomic(path, &bytes) {
                Ok(()) => cx.saved(
                    path,
                    format!(
                        "{}, {}",
                        ext.to_uppercase(),
                        human_bytes(bytes.len() as u64)
                    ),
                ),
                Err(error) => cx.fail(format!("Cannot write {}: {error}", path.display())),
            },
            Err(error) => cx.fail(error),
        }
    }
}

/// The settings section the viewer's shortcuts live in.
const KEYS_SECTION: &str = "image_viewer_keys";

/// The viewer's shortcuts with their default chords.
fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    use sue_wm::shortcuts::Shortcut;
    static TABLE: std::sync::OnceLock<Vec<Shortcut>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            Shortcut::new("viewer.next", "Next Picture", "Right or Space"),
            Shortcut::new("viewer.previous", "Previous Picture", "Left"),
            Shortcut::new("viewer.fit", "Fit to the Window", "0"),
            Shortcut::new("viewer.actual", "Actual Size", "1"),
            Shortcut::new("viewer.zoom_in", "Zoom In", "+ or ="),
            Shortcut::new("viewer.zoom_out", "Zoom Out", "-"),
            Shortcut::new("viewer.rotate_right", "Turn Right", "R"),
            Shortcut::new("viewer.rotate_left", "Turn Left", "Shift+R"),
            Shortcut::new("viewer.crop", "Crop", "C"),
            Shortcut::new("viewer.save_as", "Save As", "Ctrl+Shift+S"),
        ]
    })
}

fn texture_key(id: EntityId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-viewer", id).hash(&mut hasher);
    hasher.finish()
}

impl SueApplet for Viewer {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Image Viewer", AppletRole::Studio, "\u{1F5BC}");
        manifest.summary =
            "Look at pictures large, turn, crop and adjust them, and save or send them anywhere."
                .to_owned();
        manifest.order = 27;
        manifest.mode = Some(ModeSpec {
            label: "Image Viewer".to_owned(),
            in_pill: false,
        });
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        crate::image::is_picture(entity)
    }

    fn studio_focus(&self) -> Option<EntityId> {
        self.current
    }

    fn card_frame(&self, area: Rect, entity: &Entity) -> Option<Rect> {
        // Where the picture will sit, fitted between the bars.
        let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
        let size = vec2(raster.width() as f32, raster.height() as f32);
        let view = Rect::from_min_max(
            pos2(area.left(), area.top() + 40.0),
            pos2(area.right(), area.bottom() - 26.0),
        );
        let fit = (view.width() / size.x.max(1.0))
            .min(view.height() / size.y.max(1.0))
            .min(1.0)
            * 0.96;
        Some(Rect::from_center_size(view.center(), size * fit))
    }

    fn studio_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.frame(ui, cx);
    }

    fn rice_declarations(&self) -> Vec<sue_rice::RiceDeclaration> {
        sue_wm::shortcuts::declarations(KEYS_SECTION, shortcut_table())
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        if cx.view.studio.as_ref().map(AppletId::as_str) != Some(ID) {
            self.shown = false;
        }
        sue_wm::Tick::default()
    }

    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        // Arrows and space walk through pictures, as viewers do.
        ["Space", "Left", "Right", "Ctrl+0"]
            .iter()
            .filter_map(|chord| chord.parse().ok())
            .collect()
    }

    fn captures_keys(&self) -> bool {
        self.replace
    }

    fn commands(&self) -> Vec<PaletteEntry> {
        [
            ("Image Viewer: Save As...", "viewer.save_as"),
            ("Image Viewer: Re-encode as JPEG XL...", "viewer.reencode"),
            ("Image Viewer: Rotate Right", "viewer.rotate_right"),
            ("Image Viewer: Crop", "viewer.crop"),
        ]
        .into_iter()
        .map(|(label, command)| PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: String::new(),
        })
        .collect()
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        self.run(name, cx)
    }

    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<SueAction> {
        let command = |name: &str| ShellRequest::AppletCommand {
            applet: me(),
            command: name.to_owned(),
        };
        if current {
            return [
                ("Save As", "viewer.save_as"),
                ("Re-encode as JPEG XL", "viewer.reencode"),
                ("Rotate Right", "viewer.rotate_right"),
                ("Crop", "viewer.crop"),
                ("Adjust", "viewer.adjust"),
                ("Convert", "viewer.convert"),
                ("Open in Sue Paint", "viewer.paint"),
                ("Put in Sue Writer", "viewer.writer"),
            ]
            .into_iter()
            .map(|(label, name)| SueAction::new("Image Viewer", label, command(name)))
            .collect();
        }
        match focus {
            Some(entity) if crate::image::is_picture(entity) => vec![SueAction::new(
                wheel::SEND_TO,
                "Image Viewer",
                ShellRequest::OpenIn {
                    entity: entity.id(),
                    applet: me(),
                },
            )],
            _ => Vec::new(),
        }
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if pick.purpose == "save_as" {
            self.write_to(cx, path);
        }
    }

    fn navigate(&mut self, target: &str) -> bool {
        match target {
            "tool/crop" => {
                self.tool = Tool::Crop;
                self.crop = Some([40.0, 30.0, 200.0, 140.0]);
                true
            }
            "panel/adjust" => {
                self.adjust = Some(Adjust {
                    brightness: 0.15,
                    contrast: 0.1,
                    saturation: 0.3,
                    hue: 0.0,
                });
                true
            }
            "panel/close" => {
                self.adjust = None;
                self.resize = None;
                self.tool = Tool::Hand;
                self.crop = None;
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
    fn quarter_turns_and_resampling_keep_the_picture() {
        // A 3 by 2 picture: red, green, blue on top, white below.
        let mut pixels = Vec::new();
        for color in [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255; 4],
            [255; 4],
            [255; 4],
        ] {
            pixels.extend(color);
        }
        let turned = rotate(&pixels, 3, 2, true);
        // Turned right, the top row becomes the right column, top down.
        assert_eq!(&turned[4..8], &[255, 0, 0, 255]);
        assert_eq!(
            rotate(&rotate(&turned, 2, 3, false), 3, 2, false),
            rotate(&pixels, 3, 2, false)
        );
        assert_eq!(rotate(&turned, 2, 3, false), pixels, "left undoes right");
        let small = resample(&pixels, [3, 2], [1, 1]);
        assert_eq!(small.len(), 4);
        let big = resample(&pixels, [3, 2], [6, 4]);
        assert_eq!(big.len(), 6 * 4 * 4);
        assert_eq!(&big[..4], &[255, 0, 0, 255]);
        assert_eq!(save_types(Some("jxl"))[0].extensions[0], "jxl");
    }
}
