//! The Navigator palette.

use egui::{Color32, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::{Wedge, child, icon_button};
use crate::studio::{SuePaint, texture_key};

impl SuePaint {
    pub(crate) fn navigator(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let (body, tab) = self.torn_header(
            ui,
            rect,
            &[
                (tr!("navigator"), Icon::Navigator, true, None),
                (
                    tr!("sub-view"),
                    Icon::SubView,
                    true,
                    Some(crate::dock::Palette::SubView),
                ),
            ],
            self.nav_tab.min(1),
            theme,
        );
        self.nav_tab = tab;
        let body = body.shrink(6.0);
        if self.nav_tab == 1 {
            self.sub_view(ui, body, cx, theme);
            return;
        }
        let viewport = self.viewport;
        let controls = 58.0;
        let thumb_area = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - controls));
        ui.painter().rect_filled(thumb_area, 0.0, theme.void);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let (pixels, size, version) = doc.compositor.display();
        if size[0] == 0 {
            return;
        }
        let scale = (thumb_area.width() / size[0] as f32).min(thumb_area.height() / size[1] as f32);
        let thumb = Rect::from_center_size(
            thumb_area.center(),
            vec2(size[0] as f32, size[1] as f32) * scale,
        );
        // The canvas's own texture: only what changed goes up.
        let compositor = &doc.compositor;
        if let Some(texture) = cx.textures.texture_changed(
            texture_key(doc.picture, 0),
            version,
            size,
            4,
            pixels,
            doc.view.zoom >= 2.0,
            &|since| compositor.changed_since(since),
        ) {
            ui.painter().image(
                texture,
                thumb,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // The part the canvas shows.
        let corners = [
            viewport.left_top(),
            viewport.right_top(),
            viewport.right_bottom(),
            viewport.left_bottom(),
        ]
        .map(|corner| {
            let p = doc.view.to_picture(viewport, size, corner);
            thumb.min + p.to_vec2() * scale
        });
        // Clipped: zoomed out, the view is larger than the picture. The
        // theme's own frame (rule 10 of the 0.0.8 plan: never Clip Studio
        // Paint's red): the focus color over a wider line of the shadow
        // color, so it reads on light and dark pictures alike.
        let line = theme.selected_stroke();
        let clipped = ui.painter().with_clip_rect(thumb_area);
        clipped.add(Shape::closed_line(
            corners.to_vec(),
            Stroke::new(line.width + 2.0, theme.shadow),
        ));
        clipped.add(Shape::closed_line(
            corners.to_vec(),
            Stroke::new(line.width, theme.focus()),
        ));
        let response = ui.interact(
            thumb_area,
            Id::unique("sue-paint-navigator"),
            Sense::click_and_drag(),
        );
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            let p = ((pointer - thumb.min) / scale).to_pos2();
            doc.view.center_on(viewport, size, p);
        }
        // Zoom and rotation, as Clip Studio Paint's Navigator: each a
        // value, a wedge and its buttons on one row; fit and the flips
        // below.
        let row = 22.0;
        let buttons_width = 5.0 * (row + 4.0);
        let wedge_width = (body.width() - 62.0 - 8.0 - buttons_width - 4.0).max(24.0);
        let mut zoom_button = None;
        let mut turn_button = None;
        let mut percent = doc.view.zoom * 100.0;
        let mut rotation = doc.view.rotation;
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), thumb_area.bottom() + 4.0), body.max),
            "navigator-controls",
        );
        inner.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let mut zoomed = false;
        let mut turned = false;
        inner.horizontal(|ui| {
            zoomed |= ui
                .add_sized(
                    vec2(62.0, row),
                    egui::DragValue::new(&mut percent)
                        .range(1.0..=6400.0)
                        .max_decimals(1),
                )
                .on_hover_text(tr!("zoom"))
                .changed();
            zoomed |= ui
                .add(Wedge::new(&mut percent, 1.0..=6400.0, wedge_width, theme).logarithmic())
                .on_hover_text(tr!("zoom"))
                .changed();
            for (index, (icon, tip)) in [
                (Icon::ZoomOut, tr!("zoom-out")),
                (Icon::ZoomIn, tr!("zoom-in")),
                (Icon::Actual, "100%".to_owned()),
                (Icon::Fit, tr!("fit-to-screen-2")),
            ]
            .into_iter()
            .enumerate()
            {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(row), Sense::hover());
                if icon_button(ui, rect, icon, true, false, &tip, theme) {
                    zoom_button = Some(index);
                }
            }
        });
        inner.horizontal(|ui| {
            turned |= ui
                .add_sized(
                    vec2(62.0, row),
                    egui::DragValue::new(&mut rotation)
                        .range(-180.0..=180.0)
                        .max_decimals(1),
                )
                .on_hover_text(tr!("rotation"))
                .changed();
            turned |= ui
                .add(Wedge::new(
                    &mut rotation,
                    -180.0..=180.0,
                    wedge_width,
                    theme,
                ))
                .on_hover_text(tr!("rotation"))
                .changed();
            for (index, (icon, tip)) in [
                (Icon::RotateLeft, tr!("rotate-left")),
                (Icon::RotateRight, tr!("rotate-right")),
                (Icon::Reset, tr!("reset-rotation")),
                (Icon::FlipH, tr!("flip-horizontally")),
                (Icon::FlipV, tr!("flip-vertically")),
            ]
            .into_iter()
            .enumerate()
            {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(row), Sense::hover());
                let active = match index {
                    3 => doc.view.flip_h,
                    4 => doc.view.flip_v,
                    _ => false,
                };
                if icon_button(ui, rect, icon, true, active, &tip, theme) {
                    turn_button = Some(index);
                }
            }
        });
        let center = viewport.center();
        if zoomed {
            doc.view
                .set_zoom_at(viewport, size, center, percent / 100.0);
        }
        if turned {
            doc.view.set_rotation(viewport, size, rotation);
        }
        match zoom_button {
            Some(0) => doc.view.zoom_at(viewport, size, center, 0.8),
            Some(1) => doc.view.zoom_at(viewport, size, center, 1.25),
            Some(2) => doc.view.set_zoom_at(viewport, size, center, 1.0),
            Some(_) => doc.view.fit(viewport, size),
            None => {}
        }
        match turn_button {
            Some(0) => doc.view.rotate_by(viewport, size, -15.0),
            Some(1) => doc.view.rotate_by(viewport, size, 15.0),
            Some(2) => doc.view.set_rotation(viewport, size, 0.0),
            Some(3) => doc.view.flip(viewport, size, true),
            Some(_) => doc.view.flip(viewport, size, false),
            None => {}
        }
    }
}
