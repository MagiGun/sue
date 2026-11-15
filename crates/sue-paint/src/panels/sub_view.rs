//! The Sub View palette: the reference picture showing (mirrored as its
//! flips say), or the list of every picture; under it the zoom and turn
//! sliders (Show Rotate and Flip) and the buttons (Show Command Bar).
//! Its palette menu holds the choices of gap WS-21 of the 0.0.8 plan:
//! Switch to Eyedropper Automatically, Show Command Bar, Show Rotate and
//! Flip, the two flips, and Sub View itself to hide it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Id, Mesh, Rect, Sense, Shape, Ui, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, ListRow, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::studio::SuePaint;
use crate::subview::{Shown, decode, place, unplace};

/// The commands of the Sub View's palette menu.
pub(crate) const MENU_COMMANDS: [&str; 6] = [
    "subview.eyedropper",
    "subview.show_bar",
    "subview.show_turn",
    "subview.flip_h",
    "subview.flip_v",
    "window.sub_view",
];

/// The Sub View's buttons, left to right.
pub(crate) const BAR_COMMANDS: [(&str, Icon); 9] = [
    ("subview.import", Icon::Import),
    ("subview.previous", Icon::ChevronLeft),
    ("subview.next", Icon::ChevronRight),
    ("subview.list", Icon::PictureList),
    ("subview.eyedropper", Icon::ToolEyedropper),
    ("subview.flip_h", Icon::FlipHorizontal),
    ("subview.flip_v", Icon::FlipVertical),
    ("subview.fit", Icon::ZoomFit),
    ("subview.remove", Icon::Delete),
];

impl SuePaint {
    /// The Sub View palette's body.
    pub(crate) fn sub_view(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let density = Density::of(ui);
        let gap = theme.space(Space::S2);
        let button = density.icon_button();
        let slider = density.control_height();
        let bar = if self.sub_view.show_bar {
            // As many lines of buttons as the width takes.
            let per_line = ((body.width() + gap) / (button + gap)).floor().max(1.0) as usize;
            let lines = BAR_COMMANDS.len().div_ceil(per_line);
            lines as f32 * (button + gap)
        } else {
            0.0
        };
        let turn = if self.sub_view.show_turn {
            slider + gap
        } else {
            0.0
        };
        let image_area =
            Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - bar - turn));
        ui.painter()
            .rect_filled(image_area, theme.corner(sue_wm::ui::Radius::Sm), theme.void);
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
            let mut inner = crate::panels::child(ui, image_area.shrink(gap), "sub-view-list");
            sue_wm::ui::ScrollView::vertical("sue-paint-sub-view-list")
                .on(theme.void)
                .show(&mut inner, theme, |ui| {
                    for (index, path) in view.images.iter().enumerate() {
                        let name = path
                            .file_name()
                            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                        if ListRow::new(("sub-view-picture", path), &name)
                            .icon(Icon::Picture)
                            .selected(index == view.current)
                            .show(ui, theme)
                            .response
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
                // A flip mirrors which part of the picture each corner shows.
                let (u0, u1) = if view.flip_h { (1.0, 0.0) } else { (0.0, 1.0) };
                let (v0, v1) = if view.flip_v { (1.0, 0.0) } else { (0.0, 1.0) };
                let mut mesh = Mesh::with_texture(texture);
                for (corner, uv) in
                    corners
                        .iter()
                        .zip([pos2(u0, v0), pos2(u1, v0), pos2(u1, v1), pos2(u0, v1)])
                {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: *corner,
                        uv,
                        color: crate::panels::run_button::UNTINTED,
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
                let at = unplace(image_area, shown.size, view.zoom, view.rotation, pointer);
                let p = view.mirrored(shown.size, at);
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
                tr!("sub-view-empty"),
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
        }
        if let Some(index) = choose {
            view.current = index;
            view.list = false;
        }
        // Zoom and turn.
        if view.show_turn {
            let row = Rect::from_min_size(
                pos2(body.left(), image_area.bottom() + gap),
                vec2(body.width(), slider),
            );
            let mut inner = crate::panels::child(ui, row, "sub-view-turn");
            let half = (row.width() - gap) / 2.0;
            let mut zoom = f64::from(view.zoom);
            let mut degrees = f64::from(view.rotation);
            let zoom_name = tr!("zoom-the-reference");
            let turn_name = tr!("turn-the-reference");
            let a = sue_wm::ui::slider("sue-paint-sub-view-zoom", &mut zoom, 0.1..=8.0)
                .logarithmic()
                .decimals(1)
                .suffix("x")
                .name(&zoom_name)
                .show_at(
                    &mut inner,
                    Rect::from_min_size(row.min, vec2(half, slider)),
                    theme,
                );
            sue_wm::ui::tooltip(a, &Tip::new(&zoom_name), theme);
            let b = sue_wm::ui::slider("sue-paint-sub-view-turn", &mut degrees, -180.0..=180.0)
                .decimals(0)
                .suffix("\u{b0}")
                .name(&turn_name)
                .show_at(
                    &mut inner,
                    Rect::from_min_size(
                        pos2(row.left() + half + gap, row.top()),
                        vec2(half, slider),
                    ),
                    theme,
                );
            sue_wm::ui::tooltip(b, &Tip::new(&turn_name), theme);
            view.zoom = zoom as f32;
            view.rotation = degrees as f32;
        }
        if self.sub_view.show_bar {
            let area = Rect::from_min_max(pos2(body.left(), body.bottom() - bar), body.max);
            let buttons = crate::panels::child(ui, area, "sub-view-buttons");
            let mut at = area.min;
            for (command, icon) in BAR_COMMANDS {
                if at.x + button > area.right() + 0.5 && at.x > area.left() {
                    at = pos2(area.left(), at.y + button + gap);
                }
                let rect = Rect::from_min_size(at, vec2(button, button));
                self.command_button(&buttons, rect, command, icon, cx, theme);
                at.x += button + gap;
            }
        }
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
    }
}
