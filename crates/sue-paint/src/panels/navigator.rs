//! The Navigator palette: the whole picture small, the part the canvas
//! shows framed on it (drag or click to move the view), and under it two
//! rows, zoom and rotation, each a number, a wedge to drag and the View
//! commands' buttons.

use egui::{Color32, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, Tip};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon as OldIcon;
use crate::panels::child;
use crate::studio::{SuePaint, texture_key};

/// The zoom row's buttons: View commands, as the View menu has them.
pub(crate) const ZOOM_COMMANDS: [(&str, Icon); 4] = [
    ("view.zoom_out", Icon::ZoomOut),
    ("view.zoom_in", Icon::ZoomIn),
    ("view.actual", Icon::ZoomActual),
    ("view.fit", Icon::ZoomFit),
];

/// The rotation row's buttons.
pub(crate) const TURN_COMMANDS: [(&str, Icon); 5] = [
    ("view.rotate_left", Icon::RotateLeft),
    ("view.rotate_right", Icon::RotateRight),
    ("view.rotate_reset", Icon::Reset),
    ("view.flip_h", Icon::FlipHorizontal),
    ("view.flip_v", Icon::FlipVertical),
];

/// The commands of the Navigator's palette menu.
pub(crate) const MENU_COMMANDS: [&str; 4] = [
    "view.fit_navigator",
    "view.reset_display",
    "view.actual",
    "view.reset_turn",
];

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
                (tr!("navigator"), OldIcon::Navigator, true, None),
                (
                    tr!("sub-view"),
                    OldIcon::SubView,
                    true,
                    Some(crate::dock::Palette::SubView),
                ),
            ],
            self.nav_tab.min(1),
            theme,
        );
        self.nav_tab = tab;
        let body = body.shrink(theme.space(Space::S3));
        if self.nav_tab == 1 {
            self.sub_view(ui, body, cx, theme);
            return;
        }
        let density = Density::of(ui);
        let row = density.icon_button();
        let gap = theme.space(Space::S2);
        // Two rows of a value and its wedge, then the buttons in as many
        // lines as the width takes.
        let count = ZOOM_COMMANDS.len() + TURN_COMMANDS.len();
        let per_line = ((body.width() + gap) / (row + gap)).floor().max(1.0) as usize;
        let lines = count.div_ceil(per_line) as f32;
        let controls = (2.0 + lines) * (row + gap) + gap;
        let thumb_area = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - controls));
        ui.painter()
            .rect_filled(thumb_area, theme.corner(sue_wm::ui::Radius::Sm), theme.void);
        let viewport = self.viewport;
        let mut zoomed = None;
        let mut turned = None;
        if let Some(doc) = self.docs.get_mut(self.active) {
            let (pixels, size, version) = doc.compositor.display();
            if size[0] > 0 && thumb_area.width() > 0.0 && thumb_area.height() > 0.0 {
                let scale =
                    (thumb_area.width() / size[0] as f32).min(thumb_area.height() / size[1] as f32);
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
                // The part the canvas shows: the theme's own frame, a
                // line in the focus color over a wider one in the shadow
                // color, so it reads on light and dark pictures alike.
                let corners: Vec<Pos2> = [
                    viewport.left_top(),
                    viewport.right_top(),
                    viewport.right_bottom(),
                    viewport.left_bottom(),
                ]
                .map(|corner| {
                    let p = doc.view.to_picture(viewport, size, corner);
                    thumb.min + p.to_vec2() * scale
                })
                .to_vec();
                let line = theme.selected_stroke();
                let clipped = ui.painter().with_clip_rect(thumb_area);
                clipped.add(Shape::closed_line(
                    corners.clone(),
                    Stroke::new(line.width + theme.space(Space::S1), theme.shadow),
                ));
                clipped.add(Shape::closed_line(
                    corners,
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
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }
        // Zoom and rotation: each a value and a wedge on a row of its own;
        // the View commands' buttons under them.
        let zoom = self.doc().map_or(1.0, |doc| doc.view.zoom);
        let rotation = self.doc().map_or(0.0, |doc| doc.view.rotation);
        let has_doc = self.doc().is_some();
        let field = (row * 2.6).round();
        let top = thumb_area.bottom() + gap;
        for index in 0..2_usize {
            let y = top + index as f32 * (row + gap);
            let line = Rect::from_min_max(pos2(body.left(), y), pos2(body.right(), y + row));
            let mut inner = child(ui, line, if index == 0 { "nav-zoom" } else { "nav-turn" });
            let wedge_width = (line.width() - field - gap).max(0.0);
            let field_rect = Rect::from_min_size(line.min, vec2(field, row));
            let wedge_rect = Rect::from_min_size(
                pos2(field_rect.right() + gap, line.top()),
                vec2(wedge_width, row),
            );
            if index == 0 {
                let mut percent = f64::from(zoom * 100.0);
                let name = tr!("zoom");
                let a = sue_wm::ui::number_field("sue-paint-nav-zoom", &mut percent)
                    .range(1.0..=6400.0)
                    .decimals(1)
                    .suffix("%")
                    .label(&name)
                    .enabled(has_doc)
                    .show_at(&mut inner, field_rect, theme);
                let a = sue_wm::ui::tooltip(a, &Tip::new(&name), theme);
                let mut wedged = percent;
                let b = if wedge_width >= row {
                    let b =
                        sue_wm::ui::wedge("sue-paint-nav-zoom-wedge", &mut wedged, 1.0..=6400.0)
                            .logarithmic()
                            .decimals(1)
                            .label(&name)
                            .enabled(has_doc)
                            .show_at(&mut inner, wedge_rect, theme);
                    sue_wm::ui::tooltip(b, &Tip::new(&name), theme).changed()
                } else {
                    false
                };
                if a.changed() {
                    zoomed = Some(percent as f32 / 100.0);
                } else if b {
                    zoomed = Some(wedged as f32 / 100.0);
                }
            } else {
                let mut degrees = f64::from(rotation);
                let name = tr!("rotation");
                let a = sue_wm::ui::number_field("sue-paint-nav-turn", &mut degrees)
                    .range(-180.0..=180.0)
                    .decimals(1)
                    .suffix("\u{b0}")
                    .label(&name)
                    .enabled(has_doc)
                    .show_at(&mut inner, field_rect, theme);
                let a = sue_wm::ui::tooltip(a, &Tip::new(&name), theme);
                let mut wedged = degrees;
                let b = if wedge_width >= row {
                    let b =
                        sue_wm::ui::wedge("sue-paint-nav-turn-wedge", &mut wedged, -180.0..=180.0)
                            .decimals(1)
                            .label(&name)
                            .enabled(has_doc)
                            .show_at(&mut inner, wedge_rect, theme);
                    sue_wm::ui::tooltip(b, &Tip::new(&name), theme).changed()
                } else {
                    false
                };
                if a.changed() {
                    turned = Some(degrees as f32);
                } else if b {
                    turned = Some(wedged as f32);
                }
            }
        }
        let area = Rect::from_min_max(pos2(body.left(), top + 2.0 * (row + gap)), body.max);
        let buttons = child(ui, area, "nav-buttons");
        let mut at = area.min;
        for (command, icon) in ZOOM_COMMANDS.iter().chain(TURN_COMMANDS.iter()) {
            if at.x + row > area.right() + gap && at.x > area.left() {
                at = pos2(area.left(), at.y + row + gap);
            }
            let rect = Rect::from_min_size(at, vec2(row, row));
            self.command_button(&buttons, rect, command, *icon, cx, theme);
            at.x += row + gap;
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            let size = doc.compositor.display().1;
            let center = viewport.center();
            if let Some(zoom) = zoomed {
                doc.view.set_zoom_at(viewport, size, center, zoom);
            }
            if let Some(degrees) = turned {
                doc.view.set_rotation(viewport, size, degrees);
            }
        }
    }
}
