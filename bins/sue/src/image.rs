//! Image: a decoded picture (`RasterTile2D` from JPEG XL, PNG or JPEG) on
//! the canvas. Its pixels go straight from the arena to the GPU. The pill
//! zooms to 1:1, overlays a pixel grid, re-encodes to lossless JPEG XL and
//! opens the file's bytes in Code / Hex.

use std::collections::HashSet;
use std::path::Path;

use egui::{Color32, Pos2, Rect, Stroke, Ui, pos2, vec2};
use sue_core::{ComponentKind, Entity, PixelFormat};
use sue_io::{JxlError, JxlPipeline, NativeJxlPipeline, byte_map_tile, meta};
use sue_platform::write_atomic;
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, MediaClass, ModeSpec, NewEntity, NodeId,
    SceneNode, ShellRequest, SpawnNode, SueApplet, World, widgets,
};

use crate::util::{file_name, free_path, human_bytes};

/// The applet id.
pub const ID: &str = "image";

/// `true` for a decoded picture (not a byte map, not a paint layer).
#[must_use]
pub fn is_picture(entity: &Entity) -> bool {
    entity.component(ComponentKind::RasterTile2D).is_some()
        && entity.meta(meta::WIDTH).is_some()
        && entity.meta(meta::ENTROPY).is_none()
}

/// Pictures on the canvas.
#[derive(Debug, Default)]
pub struct Image {
    grid: HashSet<NodeId>,
}

/// The image rect of a picture fitted into `area`.
fn fitted(area: Rect, width: u32, height: u32) -> Rect {
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let size = if area.width() / area.height().max(1.0) > aspect {
        vec2(area.height() * aspect, area.height())
    } else {
        vec2(area.width(), area.width() / aspect)
    };
    Rect::from_center_size(area.center(), size)
}

impl Image {
    fn reencode(cx: &mut AppletContext<'_>, node: NodeId) {
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let name = entity.name().to_owned();
        let source = entity.meta(meta::SOURCE).map(str::to_owned);
        let jpeg = entity.meta(meta::FORMAT) == Some("JPEG");
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
        let transcoded = jpeg
            .then(|| source.as_deref().and_then(|path| std::fs::read(path).ok()))
            .flatten()
            .map(|bytes| NativeJxlPipeline.transcode_jpeg_lossless(&bytes));
        let (bytes, how) = match transcoded {
            Some(Ok(bytes)) => (bytes, "bit-exact JPEG recompression"),
            Some(Err(JxlError::CodecUnavailable { .. })) | None => {
                match NativeJxlPipeline.encode_layers(&[tile], effort) {
                    Ok(bytes) => (bytes, "lossless"),
                    Err(error) => {
                        cx.fail(format!("JPEG XL encoding failed: {error}"));
                        return;
                    }
                }
            }
            Some(Err(error)) => {
                cx.fail(error.to_string());
                return;
            }
        };
        let dir = source
            .as_deref()
            .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
            .unwrap_or_else(|| crate::paint::export_dir(cx));
        let stem = Path::new(&name).file_stem().map_or_else(
            || "image".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let path = free_path(&dir.join(format!("{stem}.jxl")));
        match write_atomic(&path, &bytes) {
            Ok(()) => cx.notify(format!(
                "Saved {} ({}, {how})",
                file_name(&path),
                human_bytes(bytes.len() as u64)
            )),
            Err(error) => cx.fail(format!("Cannot write {}: {error}", path.display())),
        }
    }

    /// Opens the picture's file bytes in Code / Hex.
    fn inspect_hex(cx: &mut AppletContext<'_>, node: NodeId) {
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let name = entity.name().to_owned();
        let Some(path) = entity.meta(meta::SOURCE).map(str::to_owned) else {
            cx.fail("This picture has no file to inspect");
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(format!("Cannot read {path}: {error}"));
                return;
            }
        };
        let length = bytes.len();
        let width =
            u32::try_from(cx.rice().get_i64("io.byte_map_width").unwrap_or(64)).unwrap_or(64);
        let entropy = sue_io::shannon_entropy(&bytes);
        let tile = match byte_map_tile(bytes, width) {
            Ok(tile) => tile,
            Err(error) => {
                cx.fail(error.to_string());
                return;
            }
        };
        let near = cx.node(node).map_or(Rect::ZERO, |n| n.rect);
        let rect = Rect::from_min_size(near.right_top() + vec2(48.0, 0.0), vec2(620.0, 460.0));
        let hex = AppletId::new(crate::hex::ID).expect("the Code / Hex id is valid");
        let mut txn = SpawnNode::new(
            NewEntity {
                name: format!("{name} bytes"),
                base: tile.into(),
                meta: vec![
                    (meta::BYTES.to_owned(), length.to_string()),
                    (meta::ENTROPY.to_owned(), format!("{entropy:.3}")),
                    (meta::SOURCE.to_owned(), path),
                ],
                size: rect.size(),
                props: Vec::new(),
            },
            hex,
            rect,
        );
        cx.typing.flush(cx.history);
        match sue_core::ReversibleTransaction::apply(&mut txn, cx.world) {
            Ok(()) => {
                let placed = txn.placed();
                cx.history.record(txn);
                if let Some((node, _)) = placed {
                    cx.request(ShellRequest::Focus(node));
                }
            }
            Err(error) => cx.fail(error.to_string()),
        }
    }
}

impl SueApplet for Image {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Image", AppletRole::Node, "\u{1F5BC}");
        manifest.summary = "View JPEG XL, PNG and JPEG pictures pixel for pixel.".to_owned();
        manifest.order = 40;
        manifest.opens = vec![MediaClass::Image];
        manifest.mode = Some(ModeSpec {
            label: "Image".to_owned(),
            in_pill: false,
        });
        manifest.lane = Some("Paint/Visual Track".to_owned());
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        is_picture(entity)
    }

    fn node_size(&self, entity: &Entity) -> egui::Vec2 {
        entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
            .map_or(vec2(480.0, 360.0), |tile| {
                let scale = (640.0 / tile.width().max(1) as f32)
                    .min(480.0 / tile.height().max(1) as f32)
                    .min(1.0);
                vec2(
                    (tile.width() as f32 * scale).max(160.0),
                    tile.height() as f32 * scale + sue_wm::NODE_TITLE,
                )
            })
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let Some(handle) = cx.node(node).map(|n| n.entity) else {
            return;
        };
        let area = ui.max_rect();
        let theme = cx.theme.clone();
        ui.painter().rect_filled(area, 0.0, theme.void);
        let Ok(entity) = cx.world.arena.get(handle) else {
            return;
        };
        let Some(tile) = entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster())
        else {
            return;
        };
        let (width, height) = (tile.width(), tile.height());
        let image = fitted(area, width, height);
        let pixel = image.width() / width.max(1) as f32;
        let screen_pixel = pixel * cx.view.zoom;
        let channels = match tile.format() {
            PixelFormat::Gray8 => 1,
            PixelFormat::Rgba8 => 4,
            PixelFormat::RgbaF32 => 0,
        };
        let texture = tile.bytes().filter(|_| channels > 0).and_then(|bytes| {
            cx.textures.texture(
                handle.to_bits(),
                0,
                [width, height],
                channels,
                bytes,
                screen_pixel > 2.0,
            )
        });
        match texture {
            Some(texture) => {
                ui.painter().image(
                    texture,
                    image,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ui.painter().rect_filled(image, 0.0, theme.pill);
                ui.painter().text(
                    image.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{width} x {height}"),
                    theme.font(theme.small_size),
                    theme.muted,
                );
            }
        }
        if self.grid.contains(&node) && screen_pixel >= 6.0 {
            let visible = ui.clip_rect().intersect(image);
            let stroke = Stroke::new(
                1.0 / cx.view.zoom.max(0.1),
                theme.shadow.gamma_multiply(0.5),
            );
            let first_x = ((visible.left() - image.left()) / pixel).floor() as i64;
            let last_x = ((visible.right() - image.left()) / pixel).ceil() as i64;
            for column in first_x.max(0)..=last_x.min(i64::from(width)) {
                let x = image.left() + column as f32 * pixel;
                ui.painter().vline(x, visible.y_range(), stroke);
            }
            let first_y = ((visible.top() - image.top()) / pixel).floor() as i64;
            let last_y = ((visible.bottom() - image.top()) / pixel).ceil() as i64;
            for row in first_y.max(0)..=last_y.min(i64::from(height)) {
                let y = image.top() + row as f32 * pixel;
                ui.painter().hline(visible.x_range(), y, stroke);
            }
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        if widgets::pill_button(ui, "1:1 Zoom", false, &theme)
            .on_hover_text("One picture pixel per screen pixel")
            .clicked()
            && let Some(scene_node) = cx.node(node)
            && let Some(width) = cx
                .entity(node)
                .and_then(|entity| entity.component(ComponentKind::RasterTile2D))
                .and_then(|data| data.as_raster())
                .map(sue_core::RasterTile2D::width)
        {
            let content = Rect::from_min_max(
                scene_node.rect.min + vec2(0.0, sue_wm::NODE_TITLE),
                scene_node.rect.max,
            );
            let height = cx
                .entity(node)
                .and_then(|entity| entity.component(ComponentKind::RasterTile2D))
                .and_then(|data| data.as_raster())
                .map_or(1, sue_core::RasterTile2D::height);
            let shown = fitted(content, width, height);
            let percent = (width as f32 / shown.width().max(1.0) * 100.0).round() as u32;
            cx.request(ShellRequest::Frame {
                node,
                zoom_percent: Some(percent.max(1)),
            });
        }
        let on = self.grid.contains(&node);
        if widgets::pill_button(ui, "Pixel Grid", on, &theme)
            .on_hover_text("Outline pixels when zoomed in")
            .clicked()
            && !self.grid.insert(node)
        {
            self.grid.remove(&node);
        }
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Re-encode .jxl", false, &theme).clicked() {
            Self::reencode(cx, node);
        }
        if widgets::pill_button(ui, "Inspect Hex", false, &theme).clicked() {
            Self::inspect_hex(cx, node);
        }
    }

    fn duration(&self, node: &SceneNode, _world: &World) -> f64 {
        node.prop("dur", 4.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_keep_their_aspect() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0));
        assert_eq!(fitted(area, 100, 200).size(), vec2(200.0, 400.0));
    }
}
