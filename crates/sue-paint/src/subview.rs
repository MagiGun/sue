//! Sub View: reference pictures beside the canvas, as in Clip Studio
//! Paint. Import pictures, step through them or pick one from the list,
//! zoom and turn the view of one, and pick colors from it with the
//! eyedropper. The pictures stay files on disk; only the one showing is
//! decoded.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use egui::{Color32, Id, Mesh, Pos2, Rect, Sense, Shape, Ui, pos2, vec2};
use sue_core::PixelFormat;
use sue_io::{JxlPipeline, NativeJxlPipeline, PictureFormat, ProgressiveTarget};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::SuePaint;

/// A decoded reference picture.
#[derive(Clone, Debug)]
pub(crate) struct Shown {
    path: PathBuf,
    size: [u32; 2],
    pixels: Vec<u8>,
}

/// The reference pictures and how the one showing is viewed.
#[derive(Clone, Debug)]
pub(crate) struct SubView {
    /// The pictures, in the order they were imported.
    pub images: Vec<PathBuf>,
    /// The one showing.
    pub current: usize,
    /// Zoom as a multiple of fitting the view.
    pub zoom: f32,
    /// Turn in degrees.
    pub rotation: f32,
    /// Clicks pick colors instead of nothing.
    pub eyedropper: bool,
    /// The list of every picture shows instead of the picture.
    pub list: bool,
    shown: Option<Shown>,
}

impl Default for SubView {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            current: 0,
            zoom: 1.0,
            rotation: 0.0,
            eyedropper: true,
            list: false,
            shown: None,
        }
    }
}

/// Reads a picture file into RGBA8 pixels.
///
/// # Errors
/// If no codec reads it.
pub(crate) fn decode(path: &Path) -> Result<([u32; 2], Vec<u8>), String> {
    let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let tile = match extension.as_str() {
        "png" => sue_io::decode_png(&bytes)?.tile,
        "jpg" | "jpeg" => sue_io::decode_jpeg(&bytes)?.tile,
        "jxl" => {
            NativeJxlPipeline
                .decode(&bytes, ProgressiveTarget::Full)
                .map_err(|error| error.to_string())?
                .tile
        }
        _ => sue_io::decode_picture(&bytes, PictureFormat::from_extension(&extension))?,
    };
    let pixels = match (tile.format(), tile.bytes()) {
        (PixelFormat::Rgba8, Some(bytes)) => bytes.to_vec(),
        (PixelFormat::Gray8, Some(bytes)) => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        _ => return Err(tr!("pictures-pixels-cannot-shown")),
    };
    Ok(([tile.width(), tile.height()], pixels))
}

/// Where picture pixel `p` of a `size` picture lands in `rect`, fitted,
/// zoomed by `zoom` and turned by `degrees` around the middle.
fn place(rect: Rect, size: [u32; 2], zoom: f32, degrees: f32, p: Pos2) -> Pos2 {
    let fit =
        (rect.width() / size[0].max(1) as f32).min(rect.height() / size[1].max(1) as f32) * zoom;
    let v = (p - pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0)) * fit;
    let (sin, cos) = degrees.to_radians().sin_cos();
    rect.center() + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// The picture pixel under screen point `s` (the inverse of [`place`]).
fn unplace(rect: Rect, size: [u32; 2], zoom: f32, degrees: f32, s: Pos2) -> Pos2 {
    let fit =
        (rect.width() / size[0].max(1) as f32).min(rect.height() / size[1].max(1) as f32) * zoom;
    let v = s - rect.center();
    let (sin, cos) = (-degrees).to_radians().sin_cos();
    let turned = vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos) / fit.max(1e-6);
    pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0) + turned
}

impl SuePaint {
    /// Adds a reference picture and shows it.
    pub(crate) fn add_reference(&mut self, path: &Path, cx: &mut AppletContext<'_>) {
        let view = &mut self.sub_view;
        if !view.images.iter().any(|known| known == path) {
            view.images.push(path.to_path_buf());
        }
        view.current = view
            .images
            .iter()
            .position(|known| known == path)
            .unwrap_or(0);
        view.shown = None;
        view.list = false;
        if let Err(error) = decode(path) {
            cx.fail(tr!("cannot-show", path = path.display(), error = error));
        }
    }

    /// The Sub View palette's body.
    pub(crate) fn sub_view(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let controls = 58.0;
        let image_area = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - controls));
        ui.painter().rect_filled(image_area, 0.0, theme.void);
        let view = &mut self.sub_view;
        let current = view.images.get(view.current).cloned();
        if view.shown.as_ref().map(|shown| &shown.path) != current.as_ref() {
            view.shown = current.as_ref().and_then(|path| {
                decode(path).ok().map(|(size, pixels)| Shown {
                    path: path.clone(),
                    size,
                    pixels,
                })
            });
        }
        let mut picked: Option<[u8; 3]> = None;
        let mut choose = None;
        if view.list {
            // Every picture, by name, to pick one.
            let mut inner = crate::panels::child(ui, image_area.shrink(4.0), "sub-view-list");
            egui::ScrollArea::vertical()
                .id_salt("sue-paint-sub-view-list")
                .show(&mut inner, |ui| {
                    for (index, path) in view.images.iter().enumerate() {
                        let name = path
                            .file_name()
                            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                        if ui
                            .add(egui::Button::selectable(index == view.current, name))
                            .clicked()
                        {
                            choose = Some(index);
                        }
                    }
                });
        } else if let Some(shown) = &view.shown {
            let mut hasher = DefaultHasher::new();
            shown.path.hash(&mut hasher);
            if let Some(texture) = cx.textures.texture(
                hasher.finish() ^ 0x5B_5B,
                1,
                shown.size,
                4,
                &shown.pixels,
                false,
            ) {
                let corners = [
                    pos2(0.0, 0.0),
                    pos2(shown.size[0] as f32, 0.0),
                    pos2(shown.size[0] as f32, shown.size[1] as f32),
                    pos2(0.0, shown.size[1] as f32),
                ]
                .map(|corner| place(image_area, shown.size, view.zoom, view.rotation, corner));
                let mut mesh = Mesh::with_texture(texture);
                for (corner, uv) in corners.iter().zip([
                    pos2(0.0, 0.0),
                    pos2(1.0, 0.0),
                    pos2(1.0, 1.0),
                    pos2(0.0, 1.0),
                ]) {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: *corner,
                        uv,
                        color: Color32::WHITE,
                    });
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter()
                    .with_clip_rect(image_area)
                    .add(Shape::mesh(mesh));
            }
            let response = ui.interact(
                image_area,
                Id::unique("sue-paint-sub-view"),
                Sense::click_and_drag(),
            );
            if view.eyedropper
                && (response.clicked() || response.dragged())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let p = unplace(image_area, shown.size, view.zoom, view.rotation, pointer);
                if p.x >= 0.0
                    && p.y >= 0.0
                    && (p.x as u32) < shown.size[0]
                    && (p.y as u32) < shown.size[1]
                {
                    let at = ((p.y as usize) * shown.size[0] as usize + p.x as usize) * 4;
                    picked = Some([shown.pixels[at], shown.pixels[at + 1], shown.pixels[at + 2]]);
                }
            }
            if response.hovered() && view.eyedropper {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        } else {
            ui.painter().text(
                image_area.center(),
                egui::Align2::CENTER_CENTER,
                "Import a picture to see it here",
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
        }
        if let Some(index) = choose {
            view.current = index;
            view.list = false;
        }
        // Zoom and turn.
        let mut inner = crate::panels::child(
            ui,
            Rect::from_min_max(pos2(body.left(), image_area.bottom() + 4.0), body.max),
            "sub-view-controls",
        );
        inner.spacing_mut().slider_width = ((body.width() - 30.0) / 2.0 - 40.0).max(30.0);
        inner.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut view.zoom, 0.1..=8.0)
                    .logarithmic(true)
                    .max_decimals(1)
                    .suffix("x"),
            )
            .on_hover_text(tr!("zoom-the-reference"));
            ui.add(
                egui::Slider::new(&mut view.rotation, -180.0..=180.0)
                    .max_decimals(0)
                    .suffix(" \u{b0}"),
            )
            .on_hover_text(tr!("turn-the-reference"));
        });
        let row = inner.available_rect_before_wrap();
        let count = view.images.len();
        let buttons: [(Icon, &str, bool, bool); 7] = [
            (Icon::Open, &tr!("import-a-picture"), true, false),
            (Icon::Prev, &tr!("previous-picture"), count > 1, false),
            (Icon::Next, &tr!("next-picture"), count > 1, false),
            (Icon::List, &tr!("every-picture"), count > 0, view.list),
            (
                Icon::Eyedropper,
                &tr!("pick-colors-from-the-picture"),
                true,
                view.eyedropper,
            ),
            (Icon::Fit, &tr!("fit-untilted"), count > 0, false),
            (
                Icon::Delete,
                &tr!("take-this-picture-away"),
                count > 0,
                false,
            ),
        ];
        let mut clicked = None;
        for (index, (icon, tip, enabled, active)) in buttons.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(row.left() + index as f32 * 26.0, row.top()),
                vec2(22.0, 22.0),
            );
            if icon_button(&mut inner, rect, icon, enabled, active, tip, theme) {
                clicked = Some(index);
            }
        }
        match clicked {
            Some(0) => cx.request(ShellRequest::PickFile(
                FilePick::open(
                    Some(AppletId::new(crate::studio::ID).expect("a valid id")),
                    "subview",
                    &tr!("import-a-reference-picture"),
                )
                .extensions(&[
                    "png", "jpg", "jpeg", "jxl", "webp", "bmp", "gif", "tif", "tiff", "tga", "qoi",
                ]),
            )),
            Some(1) => view.current = (view.current + count - 1) % count.max(1),
            Some(2) => view.current = (view.current + 1) % count.max(1),
            Some(3) => view.list = !view.list,
            Some(4) => view.eyedropper = !view.eyedropper,
            Some(5) => {
                view.zoom = 1.0;
                view.rotation = 0.0;
            }
            Some(6) => {
                view.images.remove(view.current.min(count - 1));
                view.current = view.current.min(view.images.len().saturating_sub(1));
                view.shown = None;
            }
            _ => {}
        }
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_points_come_back_however_turned_and_zoomed() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0));
        for (zoom, degrees) in [(1.0, 0.0), (2.5, 37.0), (0.4, -90.0)] {
            for p in [pos2(0.0, 0.0), pos2(30.0, 12.0), pos2(64.0, 32.0)] {
                let back = unplace(
                    rect,
                    [64, 32],
                    zoom,
                    degrees,
                    place(rect, [64, 32], zoom, degrees, p),
                );
                assert!((back - p).length() < 1e-3, "{p:?} came back as {back:?}");
            }
        }
        // Fitted, the middle of the picture is the middle of the view.
        assert!(
            (place(rect, [64, 32], 1.0, 0.0, pos2(32.0, 16.0)) - rect.center()).length() < 1e-4
        );
    }
}
