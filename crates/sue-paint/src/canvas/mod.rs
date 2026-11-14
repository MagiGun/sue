//! The canvas: tabs of open pictures, the turned and zoomed view, every
//! tool's gesture under the pointer or pen, the overlays (selection,
//! shape previews, brush outline, grids) and the status strip below.
//!
//! Pointer input is read as egui's raw event stream, in order, so a fast
//! stroke keeps every sample between frames and a pen's pressure arrives
//! with it (from touch events that carry force).

use egui::epaint::Vertex;
use egui::{Color32, Id, Key, Mesh, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use sue_model::picture;
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::{child, icon_button};
use crate::studio::SuePaint;
use crate::tools::Tool;
use view::{SCROLLBAR, scrollbar};

mod drag;
pub(crate) mod engine;
mod fill;
mod input;
mod overlays;
mod pick;
mod press;
mod release;
mod shape;
mod status;
mod view;

impl SuePaint {
    /// The canvas column: tabs, view and status strip.
    pub(crate) fn canvas_area(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        // While the studio comes in or goes out, the tabs and the status
        // strip slide off with the panels and the view takes their room.
        // Simple Mode shows the canvas bare: no tabs, no status strip.
        let chrome = if self.bare { 0.0 } else { 26.0 };
        let slide = self.hide * chrome;
        let tabs = Rect::from_min_size(rect.min - vec2(0.0, slide), vec2(rect.width(), chrome));
        if !self.bare {
            self.tabs(ui, tabs, cx, theme);
        }
        let status = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - chrome + slide),
            pos2(rect.right(), rect.bottom() + slide),
        );
        let bars = self.scroll_bars && !self.bare;
        let scroll = if bars { SCROLLBAR } else { 0.0 };
        let mut viewport = Rect::from_min_max(
            pos2(rect.left(), tabs.bottom()),
            pos2(rect.right() - scroll, status.top()),
        );
        if self.tiled && self.docs.len() > 1 {
            viewport = self.tile_others(ui, viewport, cx, theme);
        }
        self.viewport = viewport;
        ui.painter()
            .rect_filled(viewport, 0.0, theme.void.gamma_multiply(self.cover));
        let size = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(picture::size);
        let Some(size) = size else {
            self.empty_canvas(ui, viewport, cx, theme);
            if !self.bare {
                self.status_strip(ui, status, None, theme);
            }
            return;
        };
        let (hide, ready, leaving) = (self.hide, self.ready, self.leaving());
        if let Some(doc) = self.doc_mut() {
            if !ready && hide > 0.0 && !leaving {
                // Coming in, the picture fits the room the palettes leave
                // as they slide in; going out, it stays as it was.
                doc.view.fit(viewport, size);
            } else if !doc.view.fitted {
                doc.view.fit(viewport, size);
            }
        }
        let response = ui.interact(
            viewport,
            Id::unique("sue-paint-canvas"),
            Sense::click_and_drag(),
        );
        self.canvas_input(ui, viewport, &response, size, cx);
        self.draw_picture(ui, viewport, size, cx, theme);
        self.tone_overlay(ui, viewport, size, cx);
        self.page_frames(ui, viewport, size, cx, theme);
        self.overlays(ui, viewport, size, &response, cx, theme);
        self.typing_ui(ui, viewport, size, cx);
        self.warp_ui(ui, viewport, size, cx, theme);
        self.selection_launcher(ui, viewport, size, cx, theme);
        self.text_launcher(ui, viewport, size, cx, theme);
        self.object_launcher(ui, viewport, size, cx, theme);
        if self.ruler_bar {
            self.ruler_bars(ui, viewport, size, theme);
        }
        if !self.polyline.is_empty() {
            let (enter, escape) = ui.input(|input| {
                (
                    input.key_pressed(Key::Enter),
                    input.key_pressed(Key::Escape),
                )
            });
            // The corners belong to the sub tool that started them.
            let owner = self.polyline_for.unwrap_or((Tool::Select, 3));
            let now = (self.toolbox.tool(), self.toolbox.current().mode);
            let least = if owner.0 == Tool::Figure { 2 } else { 3 };
            if enter && self.polyline.len() >= least {
                self.finish_corners(self.select_op, cx);
            } else if escape || now != owner {
                self.polyline.clear();
                self.polyline_for = None;
            }
        }
        if !self.curve_points.is_empty() {
            // Enter lays the curve ruler, Esc drops it.
            let (enter, escape) = ui.input(|input| {
                (
                    input.key_pressed(Key::Enter),
                    input.key_pressed(Key::Escape),
                )
            });
            if enter {
                self.finish_curve(cx);
            } else if escape || self.toolbox.tool() != Tool::Ruler {
                self.curve_points.clear();
            }
        }
        if self.transform.is_some() {
            self.transform_ui(ui, viewport, size, cx, theme);
        }
        if !self.bare {
            self.status_strip(ui, status, Some(size), theme);
        }
        if bars {
            let bar = Rect::from_min_max(
                pos2(viewport.right(), viewport.top()),
                pos2(viewport.right() + SCROLLBAR, viewport.bottom()),
            );
            ui.painter().rect_filled(bar, 0.0, theme.panel);
            if let Some(doc) = self.docs.get_mut(self.active) {
                let extent = size[1] as f32 * doc.view.zoom;
                if let Some(pan) = scrollbar(
                    ui,
                    bar,
                    false,
                    doc.view.pan.y,
                    extent,
                    viewport.height(),
                    "sue-paint-scroll-v",
                    theme,
                ) {
                    doc.view.pan.y = pan;
                }
            }
        }
    }

    /// Window, Canvas, Tile: every open canvas in a grid cell of `area`,
    /// the current one live in its cell (returned), the others as they
    /// last showed; a click on one makes it current.
    fn tile_others(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Rect {
        let count = self.docs.len();
        let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
        let rows = count.div_ceil(columns);
        let cell_size = vec2(area.width() / columns as f32, area.height() / rows as f32);
        let cell = |index: usize| {
            let (column, row) = (index % columns, index / columns);
            Rect::from_min_size(
                area.min + vec2(column as f32 * cell_size.x, row as f32 * cell_size.y),
                cell_size,
            )
            .shrink(1.0)
        };
        let mut pick = None;
        for index in (0..count).filter(|index| *index != self.active) {
            let rect = cell(index);
            let doc = &self.docs[index];
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, 0.0, theme.void);
            let size = cx
                .world
                .arena
                .get_by_id(doc.picture)
                .ok()
                .and_then(picture::size);
            let (pixels, shown, version) = doc.compositor.display();
            if let Some(size) = size.filter(|size| *size == shown) {
                let mut view = doc.view;
                view.fit(rect, size);
                let corners = view.corners(rect, size);
                let compositor = &doc.compositor;
                if let Some(texture) = cx.textures.texture_changed(
                    crate::studio::texture_key(doc.picture, 0),
                    version,
                    size,
                    4,
                    pixels,
                    false,
                    &|since| compositor.changed_since(since),
                ) {
                    let mut mesh = Mesh::with_texture(texture);
                    for (corner, uv) in corners.iter().zip([
                        pos2(0.0, 0.0),
                        pos2(1.0, 0.0),
                        pos2(1.0, 1.0),
                        pos2(0.0, 1.0),
                    ]) {
                        mesh.vertices.push(Vertex {
                            pos: *corner,
                            uv,
                            color: Color32::WHITE,
                        });
                    }
                    mesh.indices = vec![0, 1, 2, 0, 2, 3];
                    painter.add(Shape::mesh(mesh));
                }
            }
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, theme.border),
                egui::StrokeKind::Inside,
            );
            if ui
                .interact(rect, Id::unique(("sue-paint-tile", index)), Sense::click())
                .clicked()
            {
                pick = Some(index);
            }
        }
        if let Some(index) = pick {
            self.active = index;
        }
        let own = cell(self.active);
        ui.painter().rect_stroke(
            own,
            0.0,
            Stroke::new(1.0, theme.border_active),
            egui::StrokeKind::Outside,
        );
        own
    }

    fn tabs(&mut self, ui: &mut Ui, strip: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.bottom(), theme.hairline());
        let font = theme.font(theme.small_size + 0.5);
        let mut x = strip.left() + 2.0;
        let mut pick = None;
        let mut tab_menu = None;
        let mut close = None;
        for (index, doc) in self.docs.iter().enumerate() {
            let name = cx
                .world
                .arena
                .get_by_id(doc.picture)
                .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
            let (_, _, version) = doc.compositor.display();
            // A picture never saved is clean as it first showed.
            if version > 0 {
                self.clean.entry(doc.picture).or_insert(version);
            }
            let modified = self
                .clean
                .get(&doc.picture)
                .is_some_and(|clean| *clean != version);
            let galley = ui.painter().layout_no_wrap(name, font.clone(), theme.text);
            let tab = Rect::from_min_size(
                pos2(x, strip.top() + 2.0),
                vec2(
                    galley.size().x + 38.0 + if modified { 12.0 } else { 0.0 },
                    24.0,
                ),
            );
            x = tab.right() + 2.0;
            let chosen = index == self.active;
            let response = ui.interact(
                tab,
                Id::unique(("sue-paint-doc-tab", doc.picture.to_u128())),
                Sense::click(),
            );
            ui.painter().rect_filled(
                tab,
                0.0,
                if chosen {
                    theme.panel
                } else if response.hovered() {
                    theme.hover
                } else {
                    theme.pill
                },
            );
            let text_width = galley.size().x;
            ui.painter().galley(
                pos2(tab.left() + 10.0, tab.center().y - galley.size().y / 2.0),
                galley,
                if chosen { theme.text } else { theme.muted },
            );
            if modified {
                // Changed since it was opened or saved.
                ui.painter().circle_filled(
                    pos2(tab.left() + 16.0 + text_width, tab.center().y),
                    3.0,
                    if chosen { theme.text } else { theme.muted },
                );
            }
            let cross =
                Rect::from_center_size(pos2(tab.right() - 13.0, tab.center().y), vec2(16.0, 16.0));
            let cross_response = ui.interact(
                cross,
                Id::unique(("sue-paint-doc-close", doc.picture.to_u128())),
                Sense::click(),
            );
            let cross_color = if cross_response.hovered() {
                theme.text
            } else {
                theme.muted
            };
            ui.painter().line_segment(
                [
                    cross.left_top() + vec2(4.0, 4.0),
                    cross.right_bottom() - vec2(4.0, 4.0),
                ],
                Stroke::new(1.2, cross_color),
            );
            ui.painter().line_segment(
                [
                    cross.right_top() + vec2(-4.0, 4.0),
                    cross.left_bottom() + vec2(4.0, -4.0),
                ],
                Stroke::new(1.2, cross_color),
            );
            if cross_response
                .on_hover_text(tr!("close-picture-stays-your-project"))
                .clicked()
            {
                close = Some(index);
            } else if response.clicked() || response.secondary_clicked() {
                pick = Some(index);
            }
            // Right-click on a tab: what can be done with that canvas.
            response.context_menu(|ui| {
                for (label, command) in [
                    (tr!("save"), "file.save"),
                    (tr!("save-as-ellipsis"), "file.save_as"),
                    (tr!("close"), "file.close"),
                ] {
                    if ui.button(label).clicked() {
                        tab_menu = Some((index, command));
                        ui.close();
                    }
                }
            });
        }
        let plus = Rect::from_min_size(pos2(x + 2.0, strip.top() + 3.0), vec2(22.0, 22.0));
        if icon_button(
            ui,
            plus,
            Icon::NewLayer,
            true,
            false,
            &tr!("new-ctrl-n"),
            theme,
        ) {
            self.run("file.new", cx);
        }
        if let Some(index) = pick {
            self.active = index;
        }
        if let Some(index) = close {
            self.active = index;
            self.run("file.close", cx);
        }
        if let Some((index, command)) = tab_menu {
            self.active = index;
            self.run(command, cx);
        }
    }

    fn empty_canvas(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let card = Rect::from_center_size(viewport.center(), vec2(320.0, 150.0));
        let mut inner = child(ui, card, "empty-canvas");
        inner.multiply_opacity(self.cover);
        inner.vertical_centered(|ui| {
            ui.label(
                RichText::new(tr!("no-canvas-is-open"))
                    .size(theme.heading_size)
                    .color(theme.text),
            );
            ui.label(RichText::new(tr!("pictures-you-make-here-are-saved")).color(theme.muted));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(70.0);
                if ui.button(tr!("new-ellipsis")).clicked() {
                    self.run("file.new", cx);
                }
                if ui.button(tr!("open-ellipsis")).clicked() {
                    self.run("file.open", cx);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::input::{lean_in_picture, tilt_of};
    use egui::Pos2;

    #[test]
    fn the_pens_lean_turns_and_mirrors_with_the_view() {
        let near =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        let at = egui::pos2(40.0, 30.0);
        // Zoomed only: the same way, the same length.
        let zoomed = |p: Pos2| egui::pos2(p.x * 3.0, p.y * 3.0);
        assert!(near(
            lean_in_picture(Some([0.3, -0.4]), at, zoomed),
            [0.3, -0.4]
        ));
        // A view flipped left to right mirrors it.
        let flipped = |p: Pos2| egui::pos2(500.0 - p.x, p.y);
        assert!(near(
            lean_in_picture(Some([0.5, 0.0]), at, flipped),
            [-0.5, 0.0]
        ));
        // A view turned a quarter: leaning down on screen leans the
        // picture's way that the screen's down lands on.
        let turned = |p: Pos2| egui::pos2(-p.y, p.x);
        assert!(near(
            lean_in_picture(Some([0.0, 0.5]), at, turned),
            [-0.5, 0.0]
        ));
        // Upright or unknown: no way at all; how far is tilt's.
        assert_eq!(lean_in_picture(None, at, zoomed), [0.0, 0.0]);
        assert_eq!(lean_in_picture(Some([0.0, 0.0]), at, zoomed), [0.0, 0.0]);
        assert_eq!(tilt_of(None), 0.0);
        assert!((tilt_of(Some([0.6, 0.8])) - 1.0).abs() < 1e-6);
    }
}
