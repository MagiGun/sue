//! The Sub View palette: the reference picture showing, the list of
//! every picture, and the zoom, turn and picture buttons below them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Color32, Id, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::SuePaint;
use crate::subview::{Shown, decode, place, unplace};

impl SuePaint {
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
