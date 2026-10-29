//! The canvas: tabs of open pictures, the turned and zoomed view, every
//! tool's gesture under the pointer or pen, the overlays (selection,
//! shape previews, brush outline, grids) and the status strip below.
//!
//! Pointer input is read as egui's raw event stream, in order, so a fast
//! stroke keeps every sample between frames and a pen's pressure arrives
//! with it (from touch events that carry force).

use egui::epaint::Vertex;
use egui::{
    Align2, Color32, Event, Id, Key, Mesh, Modifiers, PointerButton, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, TouchPhase, Ui, pos2, vec2,
};
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use crate::brush::{Finish, Stabilizer, Stroke as BrushStroke};
use crate::color;
use crate::composite::{self, RenderOptions};
use crate::doc;
use crate::fill;
use crate::icons::Icon;
use crate::panels::{child, icon_button};
use crate::select::{Op, Selection};
use crate::studio::{Gesture, SuePaint, texture_key};
use crate::tools::{SubTool, Tool};

/// The angle of a screen vector, in degrees.
fn angle(v: egui::Vec2) -> f32 {
    v.y.atan2(v.x).to_degrees()
}

/// Points around an ellipse inside two corners.
fn ellipse_points(from: Pos2, to: Pos2) -> Vec<[f32; 2]> {
    let center = from + (to - from) / 2.0;
    let (rx, ry) = (((to.x - from.x) / 2.0).abs(), ((to.y - from.y) / 2.0).abs());
    (0..=96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            [center.x + rx * t.cos(), center.y + ry * t.sin()]
        })
        .collect()
}

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
        let slide = self.hide * 26.0;
        let tabs = Rect::from_min_size(rect.min - vec2(0.0, slide), vec2(rect.width(), 26.0));
        self.tabs(ui, tabs, cx, theme);
        let status = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - 26.0 + slide),
            pos2(rect.right(), rect.bottom() + slide),
        );
        let mut viewport = Rect::from_min_max(
            pos2(rect.left(), tabs.bottom()),
            pos2(rect.right(), status.top()),
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
            self.status_strip(ui, status, None, cx, theme);
            return;
        };
        let (hide, ready) = (self.hide, self.ready);
        let exit_view = self.exit_view.filter(|_| self.leaving());
        if let Some(doc) = self.doc_mut() {
            if !ready && hide > 0.0 {
                // Coming in, the picture shrinks from filling the window to
                // its place; going out, it grows from the view it had back
                // to filling the window, where its card waits.
                let mut fitted = doc.view;
                fitted.fit(viewport, size);
                doc.view = match exit_view {
                    Some(from) => from.blend(&fitted, hide),
                    None => fitted,
                };
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
        self.warp_ui(ui, viewport, size, cx, theme);
        self.selection_launcher(ui, viewport, size, cx, theme);
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
            if enter && self.polyline.len() >= 3 {
                self.finish_polyline(self.select_op);
            } else if escape || self.toolbox.tool() != Tool::Select {
                self.polyline.clear();
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
        self.status_strip(ui, status, Some(size), cx, theme);
    }

    /// Keeps track of the fingers on the canvas. A second finger takes back
    /// the stroke the first one began (the fingers move the view instead);
    /// a quick tap of two fingers undoes, of three redoes, as in Clip
    /// Studio Paint on tablets.
    #[allow(
        clippy::too_many_arguments,
        reason = "one touch event and where it lands"
    )]
    fn track_touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        pos: Pos2,
        time: f64,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        match phase {
            TouchPhase::Start => {
                if self.touches.is_empty() {
                    self.touch_gesture = Some((time, 1, 0.0));
                }
                self.touches.retain(|(known, _)| *known != id);
                self.touches.push((id, pos));
                if let Some((_, most, _)) = self.touch_gesture.as_mut() {
                    *most = (*most).max(self.touches.len());
                }
                if self.touches.len() == 2 && matches!(self.gesture, Some(Gesture::Stroke { .. })) {
                    // The first finger was not drawing after all.
                    self.release(viewport, size, cx);
                    let _ = cx.undo(false);
                }
                if self.touches.len() >= 2 {
                    self.gesture = None;
                }
            }
            TouchPhase::Move => {
                if let Some((_, at)) = self.touches.iter_mut().find(|(known, _)| *known == id) {
                    let moved = at.distance(pos);
                    *at = pos;
                    if let Some((_, _, travel)) = self.touch_gesture.as_mut() {
                        *travel += moved;
                    }
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                self.touches.retain(|(known, _)| *known != id);
                if self.touches.is_empty()
                    && let Some((began, most, travel)) = self.touch_gesture.take()
                    && phase == TouchPhase::End
                    && time - began < 0.35
                    && travel < 24.0
                {
                    match most {
                        2 => self.run("edit.undo", cx),
                        3 => self.run("edit.redo", cx),
                        _ => {}
                    }
                }
            }
        }
    }

    /// View, Tone Area: screen tone layers tinted blue over the picture.
    fn tone_overlay(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if !self.tone_area {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let arena = &cx.world.arena;
        let tones: Vec<doc::LayerInfo> = doc::layers(arena, doc.picture)
            .into_iter()
            .filter(|info| {
                info.style.visible
                    && arena
                        .get_by_id(info.id)
                        .ok()
                        .and_then(|entity| entity.property(crate::effects::TONE))
                        .and_then(sue_core::Value::as_bool)
                        == Some(true)
            })
            .collect();
        if tones.is_empty() {
            return;
        }
        let version = tones.iter().fold(0_u64, |sum, info| {
            sum.wrapping_mul(31).wrapping_add(info.revision)
        });
        let (_, _, shown) = doc.compositor.display();
        let version = version ^ shown;
        // A mask at most 1024 pixels a side.
        let scale = (size[0].max(size[1]) as f32 / 1024.0).max(1.0);
        let small = [
            ((size[0] as f32 / scale) as u32).max(1),
            ((size[1] as f32 / scale) as u32).max(1),
        ];
        if self
            .tone_cache
            .as_ref()
            .is_none_or(|(seen, _, _)| *seen != version)
        {
            let mut mask = vec![0_u8; (small[0] * small[1] * 4) as usize];
            for info in &tones {
                let Some(raster) = doc::raster(arena, info.id) else {
                    continue;
                };
                let Some(bytes) = raster.bytes() else {
                    continue;
                };
                for y in 0..small[1] {
                    for x in 0..small[0] {
                        let px = (x as f32 * scale) as i64 - info.style.offset[0];
                        let py = (y as f32 * scale) as i64 - info.style.offset[1];
                        if px < 0
                            || py < 0
                            || px >= i64::from(raster.width())
                            || py >= i64::from(raster.height())
                        {
                            continue;
                        }
                        let alpha =
                            bytes[((py as u32 * raster.width() + px as u32) * 4 + 3) as usize];
                        let at = ((y * small[0] + x) * 4) as usize;
                        if alpha > 0 {
                            mask[at..at + 4].copy_from_slice(&[40, 120, 255, 110]);
                        }
                    }
                }
            }
            self.tone_cache = Some((version, small, mask));
        }
        let Some((version, small, mask)) = &self.tone_cache else {
            return;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if let Some(texture) = cx.textures.texture(
            texture_key(doc.picture, 0x544F_4E45),
            *version,
            *small,
            4,
            mask,
            false,
        ) {
            let mut mesh = Mesh::with_texture(texture);
            for (corner, uv) in doc.view.corners(viewport, size).iter().zip([
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
            ui.painter().with_clip_rect(viewport).add(Shape::mesh(mesh));
        }
    }

    /// A comic page's frames: the finished size (the crop frame and its
    /// marks), the bleed beyond it and the inner border inside it, from
    /// the page's settings or Clip Studio Paint's defaults (3 mm of bleed,
    /// a 10 mm inner border).
    fn page_frames(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let arena = &cx.world.arena;
        if crate::story::story_of(arena, doc.picture).is_none()
            || !(self.crop_frame || self.bleed || self.page_image)
        {
            return;
        }
        let entity = arena.get_by_id(doc.picture).ok();
        let number = |key: &str| {
            entity
                .and_then(|entity| entity.property(key))
                .and_then(sue_core::Value::as_f64)
        };
        let dpi = number(picture::DPI).unwrap_or(72.0) as f32;
        let mm = dpi / 25.4;
        let bleed = number(crate::story::PAGE_BLEED).map_or(3.0 * mm, |value| value as f32 * mm);
        let inner = number(crate::story::PAGE_INNER).map_or(10.0 * mm, |value| value as f32 * mm);
        let (w, h) = (size[0] as f32, size[1] as f32);
        let view = doc.view;
        let screen = |x: f32, y: f32| view.to_screen(viewport, size, pos2(x, y));
        let painter = ui.painter().with_clip_rect(viewport);
        let quad = |x0: f32, y0: f32, x1: f32, y1: f32| {
            vec![
                screen(x0, y0),
                screen(x1, y0),
                screen(x1, y1),
                screen(x0, y1),
            ]
        };
        if self.page_image && bleed > 0.0 {
            // The bleed dimmed, as the page will be cut.
            let dim = Color32::from_black_alpha(110);
            for (x0, y0, x1, y1) in [
                (0.0, 0.0, w, bleed),
                (0.0, h - bleed, w, h),
                (0.0, bleed, bleed, h - bleed),
                (w - bleed, bleed, w, h - bleed),
            ] {
                painter.add(Shape::convex_polygon(
                    quad(x0, y0, x1, y1),
                    dim,
                    Stroke::NONE,
                ));
            }
        }
        let outline = |points: Vec<Pos2>, stroke: Stroke| {
            painter.add(Shape::closed_line(points, stroke));
        };
        if self.bleed && bleed > 0.0 {
            outline(
                quad(0.5, 0.5, w - 0.5, h - 0.5),
                Stroke::new(1.0, Color32::from_rgb(230, 60, 60)),
            );
        }
        if self.crop_frame {
            outline(
                quad(bleed, bleed, w - bleed, h - bleed),
                Stroke::new(1.0, Color32::from_rgb(40, 110, 230)),
            );
            if inner > 0.0 {
                outline(
                    quad(
                        bleed + inner,
                        bleed + inner,
                        w - bleed - inner,
                        h - bleed - inner,
                    ),
                    Stroke::new(1.0, Color32::from_rgb(40, 110, 230).gamma_multiply(0.6)),
                );
            }
            // Crop marks at each corner, outside the finished page.
            let mark = Stroke::new(1.0, theme.text.gamma_multiply(0.8));
            for (cx_, cy_, sx, sy) in [
                (bleed, bleed, -1.0, -1.0),
                (w - bleed, bleed, 1.0, -1.0),
                (bleed, h - bleed, -1.0, 1.0),
                (w - bleed, h - bleed, 1.0, 1.0),
            ] {
                let reach = bleed.max(4.0 * mm);
                painter.line_segment(
                    [screen(cx_ + sx * 1.0, cy_), screen(cx_ + sx * reach, cy_)],
                    mark,
                );
                painter.line_segment(
                    [screen(cx_, cy_ + sy * 1.0), screen(cx_, cy_ + sy * reach)],
                    mark,
                );
            }
        }
    }

    /// View, Ruler Bar: pixel rulers along the top and left of the view.
    fn ruler_bars(&self, ui: &mut Ui, viewport: Rect, size: [u32; 2], theme: &Theme) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let view = doc.view;
        let painter = ui.painter().with_clip_rect(viewport);
        let thick = 18.0;
        let top = Rect::from_min_size(viewport.min, vec2(viewport.width(), thick));
        let left = Rect::from_min_size(viewport.min, vec2(thick, viewport.height()));
        painter.rect_filled(top, 0.0, theme.panel);
        painter.rect_filled(left, 0.0, theme.panel);
        // A step of 1, 2 or 5 times a power of ten, about 60 points apart.
        let points_per_pixel = view.zoom.max(1e-3);
        let raw = 60.0 / points_per_pixel;
        let power = 10_f32.powf(raw.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|m| m * power)
            .find(|s| *s >= raw)
            .unwrap_or(raw);
        let font = theme.font(theme.small_size - 1.0);
        let ticks = |along: Rect, horizontal: bool| {
            let count = if horizontal {
                along.width()
            } else {
                along.height()
            } as i32;
            let mut last: Option<i64> = None;
            for offset in 0..count {
                let at = if horizontal {
                    pos2(along.left() + offset as f32, along.bottom())
                } else {
                    pos2(along.right(), along.top() + offset as f32)
                };
                let p = view.to_picture(viewport, size, at);
                let value = if horizontal { p.x } else { p.y };
                let index = (value / (step / 5.0)).floor() as i64;
                if last.is_some_and(|last| last != index) {
                    let major = index.rem_euclid(5) == 0;
                    let length = if major { thick * 0.6 } else { thick * 0.25 };
                    let (a, b) = if horizontal {
                        (at, at - vec2(0.0, length))
                    } else {
                        (at, at - vec2(length, 0.0))
                    };
                    painter.line_segment([a, b], Stroke::new(1.0, theme.muted));
                    if major {
                        let label = format!("{}", (index as f32 * step / 5.0).round() as i64);
                        let text_at = if horizontal {
                            at + vec2(2.0, -thick + 1.0)
                        } else {
                            at + vec2(-thick + 1.0, 2.0)
                        };
                        painter.text(text_at, Align2::LEFT_TOP, label, font.clone(), theme.muted);
                    }
                }
                last = Some(index);
            }
        };
        ticks(top, true);
        ticks(left, false);
        painter.rect_filled(
            Rect::from_min_size(viewport.min, vec2(thick, thick)),
            0.0,
            theme.panel,
        );
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
                if let Some(texture) = cx.textures.texture(
                    crate::studio::texture_key(doc.picture, 0),
                    version,
                    size,
                    4,
                    pixels,
                    false,
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
                .map_or_else(|_| "Picture".to_owned(), |entity| entity.name().to_owned());
            let (_, _, version) = doc.compositor.display();
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
                .on_hover_text("Close (the picture stays in your project)")
                .clicked()
            {
                close = Some(index);
            } else if response.clicked() || response.secondary_clicked() {
                pick = Some(index);
            }
            // Right-click on a tab: what can be done with that canvas.
            response.context_menu(|ui| {
                for (label, command) in [
                    ("Save", "file.save"),
                    ("Save As...", "file.save_as"),
                    ("Close", "file.close"),
                ] {
                    if ui.button(label).clicked() {
                        tab_menu = Some((index, command));
                        ui.close();
                    }
                }
            });
        }
        let plus = Rect::from_min_size(pos2(x + 2.0, strip.top() + 3.0), vec2(22.0, 22.0));
        if icon_button(ui, plus, Icon::NewLayer, true, false, "New (Ctrl+N)", theme) {
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
                RichText::new("No canvas is open")
                    .size(theme.heading_size)
                    .color(theme.text),
            );
            ui.label(
                RichText::new("Pictures you make here are saved in your Sue project")
                    .color(theme.muted),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(70.0);
                if ui.button("New...").clicked() {
                    self.run("file.new", cx);
                }
                if ui.button("Open...").clicked() {
                    self.run("file.open", cx);
                }
            });
        });
    }

    // ----- Input ------------------------------------------------------------------

    fn canvas_input(
        &mut self,
        ui: &Ui,
        viewport: Rect,
        response: &Response,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let ctx = ui.ctx().clone();
        let blocked = self.dialog.is_some() || self.menubar.is_open() || !self.ready;
        let (events, space, time, dt, held) = ctx.input(|input| {
            (
                input.events.clone(),
                input.key_down(Key::Space),
                input.time,
                input.unstable_dt,
                input.modifiers,
            )
        });
        self.modifiers = held;
        let space = space && !ctx.text_edit_focused();
        // Spread this frame's pointer samples evenly over the frame's time,
        // so the stabilizer and flicks see the pen's real speed.
        let moves = events
            .iter()
            .filter(|event| matches!(event, Event::PointerMoved(_)))
            .count()
            .max(1);
        let frame = dt.clamp(0.001, 0.1);
        let step = frame / moves as f32;
        self.clock = (time - f64::from(frame), step);
        self.pointer = response.hover_pos();
        for event in events {
            match event {
                Event::Touch {
                    id,
                    phase,
                    pos,
                    force,
                    ..
                } => {
                    self.track_touch(id.0, phase, pos, time, viewport, size, cx);
                    match phase {
                        TouchPhase::Start | TouchPhase::Move => {
                            if let Some(force) = force {
                                // Through the pen pressure curve every tool shares.
                                self.pressure = Some(self.pen_curve.eval(force.clamp(0.0, 1.0)));
                            }
                        }
                        TouchPhase::End | TouchPhase::Cancel => self.pressure = None,
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } => {
                    if blocked
                        || self.gesture.is_some()
                        || !viewport.contains(pos)
                        || !response.hovered()
                    {
                        continue;
                    }
                    match button {
                        PointerButton::Primary => {
                            self.press(pos, modifiers, space, viewport, size, cx)
                        }
                        PointerButton::Middle => self.gesture = Some(Gesture::Pan { last: pos }),
                        // A right-click picks a color, as in Clip Studio Paint.
                        PointerButton::Secondary => {
                            self.pick(pos, 0, viewport, size, cx);
                            self.gesture = Some(Gesture::Pick { mode: 0 });
                        }
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) => {
                    self.clock.0 += f64::from(self.clock.1);
                    self.drag(pos, viewport, size, cx);
                }
                Event::PointerButton {
                    pos,
                    pressed: false,
                    ..
                } if self.gesture.is_some() => {
                    self.drag(pos, viewport, size, cx);
                    self.release(viewport, size, cx);
                }
                _ => {}
            }
        }
        // Two fingers or more move, zoom and turn the view.
        let pinch = ctx.input(egui::InputState::multi_touch);
        if let Some(touch) = pinch.filter(|_| !blocked)
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            doc.view.pan += touch.translation_delta;
            if (touch.zoom_delta - 1.0).abs() > 1e-4 {
                doc.view
                    .zoom_at(viewport, size, touch.center_pos, touch.zoom_delta);
            }
            if touch.rotation_delta.abs() > 1e-4 {
                doc.view
                    .rotate_by(viewport, size, touch.rotation_delta.to_degrees());
            }
        }
        if response.hovered() && !blocked && pinch.is_none() {
            let (scroll, zoom, shift) = ctx.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.zoom_delta(),
                    input.modifiers.shift,
                )
            });
            if let (Some(pointer), Some(doc)) =
                (response.hover_pos(), self.docs.get_mut(self.active))
            {
                if shift && (scroll.x != 0.0 || scroll.y != 0.0) {
                    // Shift and the wheel turn the view.
                    doc.view
                        .rotate_by(viewport, size, (scroll.x + scroll.y) * 0.15);
                } else if (zoom - 1.0).abs() > 1e-4 {
                    doc.view.zoom_at(viewport, size, pointer, zoom);
                } else if scroll.y != 0.0 {
                    doc.view
                        .zoom_at(viewport, size, pointer, (scroll.y * 0.0025).exp());
                }
                if scroll.x != 0.0 {
                    doc.view.pan.x += scroll.x;
                }
            }
            let tool = if space {
                Tool::Hand
            } else {
                self.toolbox.tool()
            };
            let icon = match (tool, &self.gesture) {
                (_, Some(Gesture::Pan { .. })) => egui::CursorIcon::Grabbing,
                (Tool::Hand, _) => egui::CursorIcon::Grab,
                (Tool::Zoom, _) => egui::CursorIcon::ZoomIn,
                (Tool::MoveLayer, _) => egui::CursorIcon::Move,
                (tool, _) if tool.paints() => egui::CursorIcon::None,
                _ => egui::CursorIcon::Crosshair,
            };
            ctx.set_cursor_icon(icon);
        }
        // Show what the stroke painted this frame.
        if let Some(Gesture::Stroke { stroke, offset, .. }) = &mut self.gesture
            && let Some([x0, y0, x1, y1]) = stroke.take_dirty()
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
            let rect = [
                shift(x0, offset[0]),
                shift(y0, offset[1]),
                shift(x1, offset[0]),
                shift(y1, offset[1]),
            ];
            doc.compositor.refresh(&cx.world.arena, rect);
        }
        if self.gesture.is_some() {
            ctx.request_repaint();
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per tool")]
    fn press(
        &mut self,
        pos: Pos2,
        modifiers: Modifiers,
        space: bool,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let mut tool = self.toolbox.tool();
        // Clip Studio Paint's modifiers: Space scrolls, Shift and Space
        // turn the view, Ctrl and Space click to zoom in and Alt and Space
        // to zoom out; Ctrl and Shift pick the layer under the pointer;
        // Ctrl and Alt drag the brush size; Alt picks a color.
        if space && modifiers.shift {
            self.gesture = Some(Gesture::Rotate {
                start: angle(pos - viewport.center()),
                rotation: doc.view.rotation,
            });
            return;
        }
        if space && (modifiers.command || modifiers.alt) {
            let factor = if modifiers.alt { 1.0 / 1.5 } else { 1.5 };
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.view.zoom_at(viewport, size, pos, factor);
            }
            return;
        }
        let rice = |key: &str, default: &str| cx.rice().get_str(key).unwrap_or(default).to_owned();
        let (alt_does, space_does, ctrl_shift) = (
            rice("sue_paint.alt_tool", "eyedropper"),
            rice("sue_paint.space_tool", "hand"),
            rice("sue_paint.ctrl_shift", "pick_layer"),
        );
        let space = space && space_does != "none";
        if modifiers.command && modifiers.shift && !space && ctrl_shift != "none" {
            self.pick_layer_at(&cx.world.arena, p);
            return;
        }
        if modifiers.command && modifiers.alt && tool.paints() {
            self.gesture = Some(Gesture::BrushSize {
                start: pos.x,
                size: self.toolbox.current().tip.size,
            });
            return;
        }
        if !space && self.transform.is_some() {
            let view = doc.view;
            self.transform_press(pos, view, viewport, size);
            return;
        }
        if !space && self.warp.is_some() {
            // Puppet warp: a pin under the pointer is dragged (Alt takes
            // it away), anywhere else a new pin goes in.
            let view = doc.view;
            let Some(warp) = self.warp.as_mut() else {
                return;
            };
            let local = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
            let offset = warp.offset;
            let hit = warp.pins.iter().position(|pin| {
                let at = pos2(pin.to[0] + offset[0] as f32, pin.to[1] + offset[1] as f32);
                view.to_screen(viewport, size, at).distance(pos) <= 9.0
            });
            match hit {
                Some(index) if modifiers.alt => {
                    warp.pins.remove(index);
                    self.warp_preview(cx);
                }
                Some(index) => self.gesture = Some(Gesture::Pin { index }),
                None => {
                    warp.pins.push(crate::warp::Pin::at(local));
                    self.gesture = Some(Gesture::Pin {
                        index: warp.pins.len() - 1,
                    });
                }
            }
            return;
        }
        if space {
            tool = Tool::Hand;
        } else if modifiers.alt && tool.paints() && tool != Tool::Eraser && tool != Tool::Figure {
            match alt_does.as_str() {
                "eyedropper" => tool = Tool::Eyedropper,
                "eraser" => tool = Tool::Eraser,
                _ => {}
            }
        }
        let sub: SubTool = if tool == self.toolbox.tool() {
            self.toolbox.current().clone()
        } else {
            self.toolbox.of(tool).clone()
        };
        let op = match (modifiers.shift, modifiers.alt) {
            (true, true) => Op::Intersect,
            (true, false) => Op::Add,
            (false, true) => Op::Subtract,
            (false, false) => self.select_op,
        };
        match tool {
            Tool::Hand => {
                self.gesture = Some(if sub.mode == 1 && !space {
                    Gesture::Rotate {
                        start: angle(pos - viewport.center()),
                        rotation: doc.view.rotation,
                    }
                } else {
                    Gesture::Pan { last: pos }
                });
            }
            Tool::Zoom => {
                let factor = if (sub.mode == 1) != modifiers.alt {
                    1.0 / 1.5
                } else {
                    1.5
                };
                if let Some(doc) = self.docs.get_mut(self.active) {
                    doc.view.zoom_at(viewport, size, pos, factor);
                }
            }
            Tool::Eyedropper => {
                self.pick(pos, sub.mode, viewport, size, cx);
                self.gesture = Some(Gesture::Pick { mode: sub.mode });
            }
            Tool::Pen | Tool::Pencil | Tool::Brush | Tool::Airbrush | Tool::Eraser => {
                let info = match self.drawable(&cx.world.arena) {
                    Ok(info) => info,
                    Err(error) => {
                        cx.fail(error);
                        return;
                    }
                };
                let offset = info.style.offset;
                let rgb = color::unit(self.drawing_color());
                let mut tip = sub.tip;
                if self.transparent {
                    tip.eraser = true;
                }
                if tip.screen_size {
                    // The size is the screen's: as many layer pixels as it
                    // covers at this zoom.
                    tip.size /= doc.view.zoom.max(0.01);
                }
                // Editing the mask: the pen shows the layer, the eraser
                // hides it, in the mask's own pixels.
                let target = doc
                    .mask_edit
                    .then(|| doc::mask_of(&cx.world.arena, info.id))
                    .flatten();
                let vector = target.is_none() && crate::vector::is_vector(&cx.world.arena, info.id);
                if vector && tip.eraser {
                    // The eraser takes vector lines away, as its Vector
                    // setting says.
                    let Some(before) = doc::raster(&cx.world.arena, info.id)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec)
                    else {
                        return;
                    };
                    let geometry = crate::vector::geometry(&cx.world.arena, info.id);
                    self.gesture = Some(Gesture::VectorErase {
                        layer: info.id,
                        before,
                        lines: crate::vector::lines(&geometry),
                        geometry,
                        last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
                        radius: (tip.size / 2.0).max(0.5),
                        mode: crate::vector::Erase::of(sub.kind),
                        offset,
                        changed: false,
                        label: sub.name.clone(),
                    });
                    self.vector_erase_drag(pos, viewport, size, cx);
                    return;
                }
                let (target, rgb, locked) = match target {
                    Some(mask) => (mask, [1.0; 3], false),
                    None => (info.id, rgb, info.alpha_locked && !tip.eraser && !vector),
                };
                let mut stroke = BrushStroke::new(target, tip, rgb, locked);
                let other = if self.sub_active { self.main } else { self.sub };
                stroke.set_sub_color(color::unit(other));
                // The picture's rulers, in the layer's pixels.
                let rulers = if doc.show_rulers {
                    crate::rulers::Rulers::of(&cx.world.arena, doc.picture)
                        .shifted([-(offset[0] as f32), -(offset[1] as f32)])
                } else {
                    crate::rulers::Rulers::default()
                };
                if let Some((center, angle)) = rulers.symmetry.filter(|_| doc.ruler_on)
                    && let Some(ruler) = self
                        .toolbox
                        .subs(Tool::Ruler)
                        .iter()
                        .find(|sub| sub.mode == 0)
                {
                    // The symmetrical ruler: a copy through every line.
                    stroke.set_copies(crate::brush::symmetry(
                        center,
                        angle,
                        crate::rulers::turns(ruler.lines, ruler.mirror),
                        ruler.mirror,
                    ));
                }
                let shifted = doc
                    .selection
                    .as_ref()
                    .filter(|_| offset != [0, 0])
                    .map(|selection| selection.shifted(offset));
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                // A stroke that starts near the linear ruler goes onto it.
                let snap = crate::rulers::Snap::begin(
                    &rulers,
                    local,
                    doc.snap_ruler,
                    doc.ruler_on,
                    24.0 / doc.view.zoom.max(0.01),
                );
                let snap = snap.or_else(|| {
                    self.snap_grid
                        .then(|| {
                            crate::rulers::Snap::grid(
                                local,
                                [offset[0] as f32, offset[1] as f32],
                                64.0,
                                12.0 / doc.view.zoom.max(0.01),
                            )
                        })
                        .flatten()
                });
                let local = snap.as_ref().map_or(local, |snap| snap.project(local));
                let mut stabilizer = Stabilizer::new(sub.stabilization, sub.adjust_by_speed);
                let start = stabilizer.sample(local, doc.view.zoom, self.clock.1);
                let pressure = self.pressure.unwrap_or(1.0);
                // Vector lines are kept whole, not cut by the selection.
                let selection = (!vector)
                    .then(|| shifted.as_ref().or(doc.selection.as_ref()))
                    .flatten();
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, target) {
                    stroke.to(raster, start, pressure, selection);
                }
                self.gesture = Some(Gesture::Stroke {
                    stroke: Box::new(stroke),
                    stabilizer,
                    offset,
                    raw: local,
                    label: sub.name.clone(),
                    shifted,
                    trail: vec![(self.clock.0, local)],
                    correction: sub.correction,
                    flick: sub.flick,
                    line: modifiers.shift,
                    snap,
                    vector,
                });
            }
            Tool::Text => self.text_at(p, cx),
            Tool::Blend => {
                let info = match self.paintable(&cx.world.arena) {
                    Ok(info) => info,
                    Err(error) => {
                        cx.fail(error);
                        return;
                    }
                };
                let Some(before) = doc::raster(&cx.world.arena, info.id)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                else {
                    return;
                };
                let offset = info.style.offset;
                self.gesture = Some(Gesture::Blend {
                    layer: info.id,
                    before,
                    last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
                    since: 0.0,
                    offset,
                    label: sub.name.clone(),
                });
            }
            Tool::Ruler if sub.mode == 6 => {
                // Curve Ruler: each click places a point; clicking the
                // last one again (or Enter) lays the ruler.
                let near_last = self.curve_points.last().is_some_and(|last| {
                    doc.view
                        .to_screen(viewport, size, pos2(last[0], last[1]))
                        .distance(pos)
                        <= 8.0
                });
                if near_last {
                    self.finish_curve(cx);
                } else {
                    self.curve_points.push([p.x, p.y]);
                }
            }
            Tool::Ruler if sub.mode == 8 => {
                self.gesture = Some(Gesture::RulerPen {
                    points: vec![[p.x, p.y]],
                });
            }
            Tool::Ruler => {
                let picture = doc.picture;
                let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
                let reach = 9.0 / doc.view.zoom.max(0.01);
                let hit = doc
                    .show_rulers
                    .then(|| rulers.handle_at([p.x, p.y], reach, size))
                    .flatten();
                match hit {
                    Some(crate::rulers::Handle::Vanishing(index))
                        if sub.mode == 1 && sub.kind == 1 =>
                    {
                        // Delete vanishing point.
                        let mut points = rulers
                            .perspective
                            .map(|ruler| ruler.points)
                            .unwrap_or_default();
                        if index < points.len() {
                            points.remove(index);
                        }
                        let text = (!points.is_empty())
                            .then(|| crate::rulers::Perspective { points }.to_text());
                        Self::apply(
                            cx,
                            crate::rulers::edit(
                                picture,
                                crate::rulers::PERSPECTIVE,
                                text,
                                "Delete Vanishing Point",
                            ),
                        );
                    }
                    Some(handle) => {
                        self.gesture = Some(Gesture::RulerHandle {
                            handle,
                            before: rulers.text(handle.key()),
                        });
                    }
                    None => {
                        self.gesture = Some(Gesture::Shape {
                            tool,
                            sub: Box::new(sub),
                            op,
                            from: p,
                            to: p,
                        });
                    }
                }
            }
            Tool::Frame | Tool::Balloon => {
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                });
            }
            Tool::Figure | Tool::Gradient => {
                if let Err(error) = self.paintable(&cx.world.arena) {
                    cx.fail(error);
                    return;
                }
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                });
            }
            Tool::Select if sub.mode == 3 => {
                // Polyline: each click a corner; the first one again (or
                // Enter) closes it.
                let near_first = self.polyline.first().is_some_and(|first| {
                    doc.view
                        .to_screen(viewport, size, pos2(first[0], first[1]))
                        .distance(pos)
                        <= 8.0
                });
                if near_first && self.polyline.len() >= 3 {
                    self.finish_polyline(op);
                } else {
                    self.polyline.push([p.x, p.y]);
                }
            }
            Tool::Select if matches!(sub.mode, 4 | 5) => {
                let add = sub.mode == 4;
                let radius = (sub.tip.size / 2.0).max(0.5);
                if let Some(doc) = self.docs.get_mut(self.active) {
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    selection.disc([p.x, p.y], radius, add);
                    doc.selection = Some(selection);
                    doc.edges = None;
                }
                self.gesture = Some(Gesture::SelectPaint {
                    add,
                    last: [p.x, p.y],
                });
            }
            Tool::Select => {
                self.gesture = Some(if sub.mode == 2 {
                    Gesture::Lasso {
                        op,
                        points: vec![[p.x, p.y]],
                    }
                } else {
                    Gesture::Shape {
                        tool,
                        sub: Box::new(sub),
                        op,
                        from: p,
                        to: p,
                    }
                });
            }
            Tool::AutoSelect => self.auto_select(p, &sub, op, size, cx),
            Tool::Fill => self.bucket(p, &sub, size, cx),
            Tool::MoveLayer => {
                let Some(info) = self.layer_info(&cx.world.arena) else {
                    return;
                };
                if info.kind != LayerKind::Raster || info.locked {
                    cx.fail(format!("{} cannot be moved", info.name));
                    return;
                }
                let old = cx
                    .world
                    .arena
                    .get_by_id(info.id)
                    .map(|entity| {
                        [
                            entity.property(picture::X).cloned(),
                            entity.property(picture::Y).cloned(),
                        ]
                    })
                    .unwrap_or_default();
                self.gesture = Some(Gesture::Move {
                    layer: info.id,
                    start: p,
                    origin: info.style.offset,
                    old,
                });
            }
            Tool::Operation if sub.mode == 1 => self.pick_layer_at(&cx.world.arena, p),
            Tool::Operation
                if self
                    .layer()
                    .is_some_and(|layer| crate::three::is_3d(&cx.world.arena, layer)) =>
            {
                // A 3D layer: drag to turn the camera, Shift to move it,
                // Ctrl to come nearer or go further.
                let Some(layer) = self.layer() else {
                    return;
                };
                let (Some(base), Some(before)) = (
                    cx.world
                        .arena
                        .get_by_id(layer)
                        .ok()
                        .map(crate::three::View3D::of),
                    doc::raster(&cx.world.arena, layer)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec),
                ) else {
                    return;
                };
                let mode = if modifiers.shift {
                    1
                } else if modifiers.command {
                    2
                } else {
                    0
                };
                self.gesture = Some(Gesture::Orbit {
                    layer,
                    start: pos,
                    base,
                    before,
                    mode,
                });
            }
            Tool::Operation if self.camera_folder(&cx.world.arena).is_some() => {
                // The camera frame: inside moves it, a corner zooms, outside
                // turns it.
                let Some(folder) = self.camera_folder(&cx.world.arena) else {
                    return;
                };
                let base = self.camera_now(&cx.world.arena, folder);
                let corners = base.frame(size);
                let near_corner = corners.iter().any(|corner| {
                    doc.view
                        .to_screen(viewport, size, pos2(corner[0], corner[1]))
                        .distance(pos)
                        <= 10.0
                });
                let inside = crate::camera::inside(&corners, [p.x, p.y]);
                let mode = if near_corner {
                    1
                } else if inside {
                    0
                } else {
                    2
                };
                self.gesture = Some(Gesture::Camera {
                    folder,
                    start: [p.x, p.y],
                    base,
                    mode,
                });
            }
            Tool::Operation => {
                // Object: click a vector line to pick it (Shift adds or
                // takes away), drag to move what is picked.
                let Some(info) = self.layer_info(&cx.world.arena) else {
                    return;
                };
                if !crate::vector::is_vector(&cx.world.arena, info.id) {
                    self.picked_lines = None;
                    cx.notify("Object picks vector lines: pick a vector layer");
                    return;
                }
                let offset = info.style.offset;
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                let geometry = crate::vector::geometry(&cx.world.arena, info.id);
                let lines = crate::vector::lines(&geometry);
                let reach = 8.0 / doc.view.zoom.max(0.01);
                let hit = crate::correct::line_at(&lines, local, reach).map(|(line, _)| line);
                let mut picked = match self.picked_lines.take() {
                    Some((layer, picked)) if layer == info.id => picked,
                    _ => Vec::new(),
                };
                match hit {
                    None => {
                        if !modifiers.shift {
                            picked.clear();
                        }
                    }
                    Some(line) if modifiers.shift => {
                        if let Some(at) = picked.iter().position(|index| *index == line) {
                            picked.remove(at);
                        } else {
                            picked.push(line);
                        }
                    }
                    Some(line) => {
                        if !picked.contains(&line) {
                            picked = vec![line];
                        }
                    }
                }
                let moving = hit.is_some() && !picked.is_empty();
                self.picked_lines = Some((info.id, picked));
                if moving
                    && let Some(before) = doc::raster(&cx.world.arena, info.id)
                        .and_then(|raster| raster.bytes())
                        .map(<[u8]>::to_vec)
                {
                    self.gesture = Some(Gesture::MoveLines {
                        layer: info.id,
                        before,
                        geometry,
                        base: lines,
                        start: local,
                        by: [0.0; 2],
                        offset,
                    });
                }
            }
            Tool::CorrectLine if sub.mode == 7 => {
                // Remove dust: drag a box, or click for the whole layer.
                if let Err(error) = self.paintable(&cx.world.arena) {
                    cx.fail(error);
                    return;
                }
                self.gesture = Some(Gesture::Shape {
                    tool,
                    sub: Box::new(sub),
                    op,
                    from: p,
                    to: p,
                });
            }
            Tool::CorrectLine => {
                let info = match self.drawable(&cx.world.arena) {
                    Ok(info) => info,
                    Err(error) => {
                        cx.fail(error);
                        return;
                    }
                };
                if !crate::vector::is_vector(&cx.world.arena, info.id) {
                    cx.fail(format!(
                        "{} is not a vector layer: Correct Line works on vector lines",
                        info.name
                    ));
                    return;
                }
                let Some(before) = doc::raster(&cx.world.arena, info.id)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                else {
                    return;
                };
                let offset = info.style.offset;
                let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                // Control points are small: a generous reach, as in CSP.
                let reach = 12.0 / doc.view.zoom.max(0.01);
                let geometry = crate::vector::geometry(&cx.world.arena, info.id);
                let mut lines = crate::vector::lines(&geometry);
                let mut grab = None;
                let mut changed = false;
                match (sub.mode, sub.kind) {
                    // Add control point, Split line: where the line is.
                    (0, 1 | 6) => {
                        if let Some((line, along)) = crate::correct::line_at(&lines, local, reach) {
                            if sub.kind == 1 {
                                crate::correct::add_control(&mut lines[line], along);
                                changed = true;
                            } else {
                                changed = crate::correct::split(&mut lines, line, along);
                            }
                        }
                    }
                    (0, kind) => {
                        if let Some((line, index)) =
                            crate::correct::control_at(&lines, local, reach)
                        {
                            match kind {
                                2 => {
                                    changed =
                                        crate::correct::delete_control(&mut lines[line], index)
                                }
                                3 => {
                                    crate::correct::switch_corner(&mut lines[line], index);
                                    changed = true;
                                }
                                _ => grab = Some((line, index, 0.0)),
                            }
                        }
                    }
                    (1, _) => {
                        grab = crate::correct::line_at(&lines, local, reach)
                            .map(|(line, along)| (line, 0, along));
                    }
                    _ => {}
                }
                self.gesture = Some(Gesture::Correct {
                    layer: info.id,
                    before,
                    geometry,
                    base: lines.clone(),
                    lines,
                    grab,
                    start: local,
                    last: local,
                    path: Vec::new(),
                    done: Vec::new(),
                    offset,
                    changed: false,
                    label: sub.name.clone(),
                });
                if changed {
                    self.correct_changed(cx);
                }
                if matches!(sub.mode, 2..=6) {
                    self.correct_to(local, cx);
                }
            }
            other => cx.notify(format!("{} arrives in a later Sue Paint", other.label())),
        }
    }

    /// Remove dust over the box from `from` to `to` (picture pixels), or the
    /// whole layer for a click.
    fn remove_dust(&mut self, from: Pos2, to: Pos2, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let offset = info.style.offset;
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let (width, height) = (raster.width(), raster.height());
        let rect = if (to - from).length() < 3.0 {
            [0, 0, width, height]
        } else {
            let local = |value: f32, by: i64, top: u32| {
                ((value - by as f32).round().max(0.0) as u32).min(top)
            };
            let (min, max) = (from.min(to), from.max(to));
            [
                local(min.x, offset[0], width),
                local(min.y, offset[1], height),
                local(max.x, offset[0], width),
                local(max.y, offset[1], height),
            ]
        };
        let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        let mut after = before.clone();
        let size = sub.amount.round().max(1.0) as u32;
        if crate::correct::remove_dust(&mut after, width, height, rect, size) == 0 {
            cx.notify("No dust that small there");
            return;
        }
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id)
            && let Some(bytes) = raster.bytes_mut()
        {
            bytes.copy_from_slice(&after);
        }
        if let Some(patch) = doc::diff_patch(&before, &after, width, height) {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                info.id,
                "Remove Dust",
                vec![patch],
            )));
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Closes the polyline selection clicked out so far, combined as `op`.
    fn finish_polyline(&mut self, op: Op) {
        let points = std::mem::take(&mut self.polyline);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let size = doc.compositor.display().1;
        let mut selection = doc
            .selection
            .clone()
            .filter(|_| op != Op::Replace)
            .unwrap_or_else(|| Selection::new(size[0], size[1]));
        selection.polygon(&points, op);
        doc.set_selection(Some(selection));
    }

    /// Clip Studio Paint's Selection Launcher: the commands for a selection,
    /// in a bar under it.
    fn selection_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.launcher || self.gesture.is_some() {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some([x0, y0, x1, y1]) = doc.selection.as_ref().and_then(Selection::bounds) else {
            return;
        };
        let view = doc.view;
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x as f32, y as f32)));
        let bottom = corners
            .iter()
            .map(|corner| corner.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let middle = corners.iter().map(|corner| corner.x).sum::<f32>() / 4.0;
        let buttons: [(Icon, &str, &str); 9] = [
            (Icon::Deselect, "Deselect", "select.none"),
            (Icon::Reset, "Invert selected area", "select.invert"),
            (Icon::ZoomIn, "Expand selected area", "select.expand"),
            (Icon::ZoomOut, "Shrink selected area", "select.shrink"),
            (Icon::Clear, "Clear", "edit.clear"),
            (
                Icon::ClearOutside,
                "Clear outside selection",
                "edit.clear_outside",
            ),
            (Icon::FillSelection, "Fill", "edit.fill"),
            (Icon::Fit, "Scale/Rotate", "edit.transform.scale"),
            (Icon::Frame, "Crop", "edit.crop"),
        ];
        let width = buttons.len() as f32 * 26.0 + 8.0;
        let bar = Rect::from_min_size(pos2(middle - width / 2.0, bottom + 10.0), vec2(width, 30.0));
        let bar = bar.translate(vec2(
            (viewport.left() + 4.0 - bar.left()).max(0.0)
                + (viewport.right() - 4.0 - bar.right()).min(0.0),
            (viewport.bottom() - 4.0 - bar.bottom()).min(0.0),
        ));
        ui.painter()
            .rect_filled(bar, theme.control_radius, theme.panel);
        ui.painter().rect_stroke(
            bar,
            theme.control_radius,
            theme.hairline(),
            egui::StrokeKind::Inside,
        );
        let mut run = None;
        for (index, (icon, tip, command)) in buttons.iter().enumerate() {
            let at = Rect::from_min_size(
                pos2(bar.left() + 4.0 + index as f32 * 26.0, bar.top() + 4.0),
                vec2(22.0, 22.0),
            );
            if icon_button(ui, at, *icon, true, false, tip, theme) {
                run = Some(*command);
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    /// Paints the Correct Line gesture's layer again from its lines.
    fn correct_changed(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Correct {
            layer,
            lines,
            changed,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        *changed = true;
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            // The whole picture: the compositor keeps it to the canvas.
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// A Correct Line press or drag at `local` (layer pixels): reworks the
    /// lines as the sub tool says.
    fn correct_to(&mut self, local: [f32; 2], cx: &mut AppletContext<'_>) {
        use crate::correct::{self, Widen};
        let sub = self.toolbox.current().clone();
        let pressure = self.pressure.unwrap_or(1.0);
        let Some(Gesture::Correct {
            lines,
            base,
            grab,
            start,
            last,
            path,
            done,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let radius = (sub.tip.size / 2.0).max(1.0);
        let by = [local[0] - start[0], local[1] - start[1]];
        let mut edited = false;
        match sub.mode {
            0 => {
                if let Some((line, index, _)) = *grab {
                    let mut fresh = base[line].clone();
                    match sub.kind {
                        // Up widens (or makes more opaque), down narrows.
                        4 | 5 => correct::shape_control(
                            &mut fresh,
                            index,
                            (-by[1] / 60.0).exp(),
                            sub.kind == 5,
                        ),
                        _ => correct::move_control(&mut fresh, index, by),
                    }
                    lines[line] = fresh;
                    edited = true;
                }
            }
            1 => {
                if let Some((line, _, along)) = *grab {
                    lines[line] = correct::pinch(
                        &base[line],
                        along,
                        by,
                        sub.amount.max(1.0),
                        sub.level,
                        sub.kind == 0,
                    );
                    edited = true;
                }
            }
            2..=4 => {
                // Every half brush along the way.
                let step = (radius * 0.5).max(0.5);
                let length = (local[0] - last[0]).hypot(local[1] - last[1]);
                let count = (length / step).ceil().max(1.0) as usize;
                for n in 1..=count {
                    let t = n as f32 / count as f32;
                    let at = [
                        last[0] + (local[0] - last[0]) * t,
                        last[1] + (local[1] - last[1]) * t,
                    ];
                    edited |= match sub.mode {
                        2 => correct::simplify(lines, at, radius, sub.level, sub.whole),
                        3 => correct::connect(lines, at, radius),
                        _ => correct::adjust_width(
                            lines,
                            at,
                            radius,
                            Widen::of(sub.kind),
                            sub.amount,
                            sub.whole.then_some(&mut *done),
                        ),
                    };
                }
            }
            _ => path.push((local, pressure)),
        }
        *last = local;
        if edited {
            self.correct_changed(cx);
        }
    }

    /// The eraser on a vector layer dragged to `pos`: every line it passes
    /// over goes as its Vector setting says, and the layer is painted again
    /// from the lines left.
    fn vector_erase_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let Some(Gesture::VectorErase {
            layer,
            lines,
            last,
            radius,
            mode,
            offset,
            changed,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let to = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        let step = (*radius * 0.5).max(0.5);
        let length = (to[0] - last[0]).hypot(to[1] - last[1]);
        let count = (length / step).ceil().max(1.0) as usize;
        let mut erased = false;
        for n in 1..=count {
            let t = n as f32 / count as f32;
            let at = [
                last[0] + (to[0] - last[0]) * t,
                last[1] + (to[1] - last[1]) * t,
            ];
            if let Some(left) = crate::vector::erase(lines, at, *radius, *mode) {
                *lines = left;
                erased = true;
            }
        }
        *last = to;
        if !erased {
            return;
        }
        *changed = true;
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, size[0], size[1]]);
        }
    }

    /// A Ruler tool drag from `from` to `to` (picture pixels): lays the
    /// sub tool's ruler, as Clip Studio Paint's Create ruler tools do.
    fn ruler_drag(&mut self, from: Pos2, to: Pos2, sub: &SubTool, cx: &mut AppletContext<'_>) {
        use crate::rulers::{self, Rulers, Special};
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        let Some(picture) = self.picture() else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = dx.hypot(dy);
        let dragged = length >= 4.0;
        // Shift keeps the line to every 45 degrees (Snap angle).
        let angle = if self.modifiers.shift {
            (dy.atan2(dx) / FRAC_PI_4).round() * FRAC_PI_4
        } else {
            dy.atan2(dx)
        };
        let end = [from.x + angle.cos() * length, from.y + angle.sin() * length];
        let (key, text, label) = match sub.mode {
            7 => {
                if !dragged {
                    return;
                }
                let path = rulers::Path::figure([from.x, from.y], [to.x, to.y], sub.kind == 1);
                let mut all = Rulers::of(&cx.world.arena, picture);
                all.paths.push(path);
                (
                    rulers::PATHS,
                    all.text(rulers::PATHS),
                    "Create Figure Ruler",
                )
            }
            5 => {
                if !dragged {
                    return;
                }
                let ruler = Rulers {
                    linear: Some([[from.x, from.y], end]),
                    ..Rulers::default()
                };
                (
                    rulers::LINEAR,
                    ruler.text(rulers::LINEAR),
                    "Create Linear Ruler",
                )
            }
            2 => {
                let special = match sub.kind {
                    0 => Special::Parallel(if dragged { angle } else { 0.0 }),
                    1 => Special::Radial([from.x, from.y]),
                    _ => Special::Concentric([from.x, from.y]),
                };
                (
                    rulers::SPECIAL,
                    Some(special.to_text()),
                    "Create Special Ruler",
                )
            }
            1 => {
                // Add vanishing point: two lines along edges that meet in
                // the distance; where they cross is the point.
                if !dragged {
                    return;
                }
                let line = [[from.x, from.y], end];
                let Some(first) = self.vanishing_line.take() else {
                    self.vanishing_line = Some(line);
                    return;
                };
                let Some(point) = rulers::crossing(first, line) else {
                    cx.notify("Those lines run side by side, so they never meet: draw two that do");
                    return;
                };
                let current = Rulers::of(&cx.world.arena, picture).perspective;
                let Some(ruler) = rulers::Perspective::with_point(current.as_ref(), point) else {
                    cx.fail("A perspective ruler has at most three vanishing points");
                    return;
                };
                (
                    rulers::PERSPECTIVE,
                    Some(ruler.to_text()),
                    "Add Vanishing Point",
                )
            }
            _ => {
                // Through where the drag began, along it (upright for a
                // click).
                let ruler = Rulers {
                    symmetry: Some(([from.x, from.y], if dragged { angle } else { FRAC_PI_2 })),
                    ..Rulers::default()
                };
                (
                    rulers::SYMMETRY,
                    ruler.text(rulers::SYMMETRY),
                    "Create Symmetrical Ruler",
                )
            }
        };
        if Self::apply(cx, rulers::edit(picture, key, text, label))
            && let Some(doc) = self.doc_mut()
        {
            doc.show_rulers = true;
            if matches!(sub.mode, 5..=8) {
                doc.snap_ruler = true;
            } else {
                doc.ruler_on = true;
            }
        }
    }

    /// A Blend tool drag to `pos`: dabs every fifth of the brush along the
    /// way, each one reading the pixels as they were before it.
    fn blend_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let sub = self.toolbox.current().clone();
        let kind = crate::blend::BlendKind::of_mode(sub.mode);
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let Some(Gesture::Blend {
            layer,
            last,
            since,
            offset,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let (layer, origin) = (*layer, *offset);
        let to = [p.x - origin[0] as f32, p.y - origin[1] as f32];
        let radius = (sub.tip.size / 2.0).max(1.0);
        let step = (radius * 0.2).max(1.0);
        let (dx, dy) = (to[0] - last[0], to[1] - last[1]);
        let length = dx.hypot(dy);
        if length < 0.5 {
            return;
        }
        let mut dirty: Option<[u32; 4]> = None;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
            let (width, height) = (raster.width(), raster.height());
            if let Some(pixels) = raster.bytes_mut() {
                let mut along = step - *since;
                let unit = [dx / length, dy / length];
                while along <= length {
                    let at = [last[0] + unit[0] * along, last[1] + unit[1] * along];
                    let motion = [unit[0] * step, unit[1] * step];
                    if let Some([x0, y0, x1, y1]) = crate::blend::dab(
                        pixels,
                        width,
                        height,
                        kind,
                        at,
                        radius,
                        sub.tip.opacity,
                        motion,
                    ) {
                        dirty = Some(dirty.map_or([x0, y0, x1, y1], |[a, b, c, d]| {
                            [a.min(x0), b.min(y0), c.max(x1), d.max(y1)]
                        }));
                    }
                    along += step;
                }
                *since = length - (along - step);
            }
        }
        *last = to;
        if let (Some([x0, y0, x1, y1]), Some(doc)) = (dirty, self.docs.get_mut(self.active)) {
            let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
            let rect = [
                shift(x0, origin[0]),
                shift(y0, origin[1]),
                shift(x1, origin[0]),
                shift(y1, origin[1]),
            ];
            doc.compositor.refresh(&cx.world.arena, rect);
        }
    }

    fn drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2], cx: &mut AppletContext<'_>) {
        if let Some(Gesture::Pick { mode }) = self.gesture {
            self.pick(pos, mode, viewport, size, cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::Blend { .. })) {
            self.blend_drag(pos, viewport, size, cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::VectorErase { .. })) {
            self.vector_erase_drag(pos, viewport, size, cx);
            return;
        }
        if let Some(Gesture::MoveLines {
            layer,
            base,
            start,
            by,
            offset,
            ..
        }) = self.gesture.as_mut()
        {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            *by = [
                p.x - offset[0] as f32 - start[0],
                p.y - offset[1] as f32 - start[1],
            ];
            let picked = self
                .picked_lines
                .as_ref()
                .map(|(_, picked)| picked.clone())
                .unwrap_or_default();
            let lines = crate::vector::moved(base, &picked, *by);
            let layer = *layer;
            if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
                && let Some(painted) =
                    crate::vector::render(&lines, raster.width(), raster.height())
                && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
            {
                into.copy_from_slice(from);
            }
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.compositor
                    .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
            }
            return;
        }
        if let Some(Gesture::Orbit {
            layer,
            start,
            base,
            mode,
            ..
        }) = &self.gesture
        {
            let (layer, start, base, mode) = (*layer, *start, *base, *mode);
            let (dx, dy) = (pos.x - start.x, pos.y - start.y);
            let mut view = base;
            match mode {
                1 => {
                    let scale = base.distance / 600.0;
                    view.pan = [base.pan[0] - dx * scale, base.pan[1] + dy * scale];
                }
                2 => view.distance = (base.distance * (dy / 200.0).exp()).clamp(0.6, 100.0),
                _ => {
                    view.yaw = base.yaw - dx * 0.4;
                    view.pitch = (base.pitch + dy * 0.4).clamp(-89.0, 89.0);
                }
            }
            self.preview_view3d(cx, layer, &view);
            return;
        }
        if let Some(Gesture::Camera {
            folder,
            start,
            base,
            mode,
        }) = self.gesture
        {
            let Some(doc) = self.docs.get(self.active) else {
                return;
            };
            let p = doc.view.to_picture(viewport, size, pos);
            let frame = doc.frame;
            let mut camera = base;
            let center = base.center;
            match mode {
                0 => {
                    camera.center = [center[0] + p.x - start[0], center[1] + p.y - start[1]];
                }
                1 => {
                    // Corners further out see more: the camera zooms out.
                    let from = (start[0] - center[0]).hypot(start[1] - center[1]).max(1.0);
                    let to = (p.x - center[0]).hypot(p.y - center[1]).max(1.0);
                    camera.scale = (base.scale * from / to).clamp(0.01, 20.0);
                }
                _ => {
                    let from = (start[1] - center[1]).atan2(start[0] - center[0]);
                    let to = (p.y - center[1]).atan2(p.x - center[0]);
                    camera.angle = base.angle + (to - from);
                }
            }
            self.camera_pending = Some((folder, frame, camera));
            return;
        }
        if let Some(Gesture::Correct { offset, .. }) = &self.gesture {
            let offset = *offset;
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            self.correct_to([p.x - offset[0] as f32, p.y - offset[1] as f32], cx);
            return;
        }
        if matches!(self.gesture, Some(Gesture::Transform { .. })) {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            let offset = self
                .transform
                .as_ref()
                .map_or([0.0; 2], |session| session.offset.map(|value| value as f32));
            self.transform_drag([p.x - offset[0], p.y - offset[1]], self.modifiers);
            self.transform_preview(cx);
            return;
        }
        if let Some(Gesture::RulerHandle { handle, .. }) = self.gesture {
            let Some(doc) = self.docs.get(self.active) else {
                return;
            };
            let p = doc.view.to_picture(viewport, size, pos);
            let picture = doc.picture;
            let fix = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 1)
                .is_none_or(|sub| sub.fix_eye_level);
            let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture).moved(
                handle,
                [p.x, p.y],
                fix,
                size,
            );
            if let Some(text) = rulers.text(handle.key())
                && let Ok(entity) = cx.world.arena.get_by_id_mut(picture)
            {
                // Shown as it moves; one undo step when it is let go.
                entity.properties_mut().set(handle.key(), text);
            }
            return;
        }
        if let Some(Gesture::Pin { index }) = self.gesture {
            let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
                return;
            };
            let p = view.to_picture(viewport, size, pos);
            if let Some(warp) = self.warp.as_mut()
                && let Some(pin) = warp.pins.get_mut(index)
            {
                pin.to = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
            }
            self.warp_preview(cx);
            return;
        }
        let pressure = self.pressure.unwrap_or(1.0);
        let (time, step) = self.clock;
        let (Some(gesture), Some(doc)) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        match gesture {
            Gesture::Stroke {
                stroke,
                stabilizer,
                offset,
                raw,
                shifted,
                trail,
                line,
                snap,
                vector,
                ..
            } => {
                let mut local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
                if let Some(snap) = snap.as_mut() {
                    // Following a ruler: nothing is drawn until the pen
                    // shows which way it goes.
                    match snap.follow(local, 4.0 / doc.view.zoom.max(0.01)) {
                        Some(on) => local = on,
                        None => return,
                    }
                }
                *raw = local;
                if *line {
                    // A straight line waits for the release.
                    return;
                }
                trail.push((time, local));
                if trail.len() > 64 {
                    trail.drain(..trail.len() - 64);
                }
                let smooth = stabilizer.sample(local, doc.view.zoom, step);
                let smooth = snap.as_ref().map_or(smooth, |snap| snap.project(smooth));
                // Fast is 2000 screen points a second (velocity dynamics).
                stroke.set_speed(stabilizer.speed() / 2000.0);
                let selection = (!*vector)
                    .then(|| shifted.as_ref().or(doc.selection.as_ref()))
                    .flatten();
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, stroke.layer()) {
                    stroke.to(raster, smooth, pressure, selection);
                }
            }
            Gesture::Pan { last } => {
                doc.view.pan += pos - *last;
                *last = pos;
            }
            Gesture::BrushSize { start, size } => {
                let factor = ((pos.x - *start) / 120.0).exp();
                let next = (*size * factor).clamp(0.5, 2000.0);
                self.toolbox.current_mut().tip.size = next;
            }
            Gesture::Rotate { start, rotation } => {
                let now = angle(pos - viewport.center());
                doc.view
                    .set_rotation(viewport, size, *rotation + (now - *start));
            }
            Gesture::Shape { to, .. } => *to = p,
            Gesture::SelectPaint { add, last } => {
                let radius = (self.toolbox.current().tip.size / 2.0).max(0.5);
                let to = [p.x, p.y];
                let length = (to[0] - last[0]).hypot(to[1] - last[1]);
                let count = (length / (radius * 0.5).max(0.5)).ceil().max(1.0) as usize;
                let mut selection = doc
                    .selection
                    .clone()
                    .unwrap_or_else(|| Selection::new(size[0], size[1]));
                for n in 1..=count {
                    let t = n as f32 / count as f32;
                    selection.disc(
                        [
                            last[0] + (to[0] - last[0]) * t,
                            last[1] + (to[1] - last[1]) * t,
                        ],
                        radius,
                        *add,
                    );
                }
                doc.selection = Some(selection);
                doc.edges = None;
                *last = to;
            }
            Gesture::RulerPen { points } => {
                if points.last().is_none_or(|last| {
                    (last[0] - p.x).hypot(last[1] - p.y) > 2.0 / doc.view.zoom.max(0.01)
                }) {
                    points.push([p.x, p.y]);
                }
            }
            Gesture::Lasso { points, .. } => {
                if points.last().is_none_or(|last| {
                    (last[0] - p.x).hypot(last[1] - p.y) > 1.5 / doc.view.zoom.max(0.01)
                }) {
                    points.push([p.x, p.y]);
                }
            }
            Gesture::Move {
                layer,
                start,
                origin,
                ..
            } => {
                let (dx, dy) = (
                    (p.x - start.x).round() as i64,
                    (p.y - start.y).round() as i64,
                );
                if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                    entity.properties_mut().set(picture::X, origin[0] + dx);
                    entity.properties_mut().set(picture::Y, origin[1] + dy);
                }
            }
            Gesture::Pick { .. }
            | Gesture::Pin { .. }
            | Gesture::Transform { .. }
            | Gesture::Blend { .. }
            | Gesture::RulerHandle { .. }
            | Gesture::VectorErase { .. }
            | Gesture::Correct { .. }
            | Gesture::MoveLines { .. }
            | Gesture::Camera { .. }
            | Gesture::Orbit { .. } => {}
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per gesture")]
    fn release(&mut self, viewport: Rect, size: [u32; 2], cx: &mut AppletContext<'_>) {
        let _ = viewport;
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let pressure = self.pressure.unwrap_or(1.0);
        match gesture {
            Gesture::Stroke {
                mut stroke,
                mut stabilizer,
                offset,
                raw,
                label,
                shifted,
                trail,
                correction,
                flick,
                line,
                snap,
                vector,
            } => {
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                // A line along a ruler stays exactly on it.
                let snapped = snap.as_ref().is_some_and(crate::rulers::Snap::chosen);
                let (correction, flick) = if line || snapped {
                    (0, false)
                } else {
                    (correction, flick)
                };
                let layer = stroke.layer();
                let selection = (!vector)
                    .then(|| shifted.as_ref().or(doc.selection.as_ref()))
                    .flatten();
                // The pen's velocity over its last few hundredths of a
                // second, for a flick.
                let velocity = trail.last().and_then(|&(end, at)| {
                    let &(start, from) = trail.iter().find(|(time, _)| end - time <= 0.05)?;
                    let seconds = (end - start) as f32;
                    (seconds > 0.004)
                        .then(|| [(at[0] - from[0]) / seconds, (at[1] - from[1]) / seconds])
                });
                let fast = velocity.filter(|v| v[0].hypot(v[1]) * doc.view.zoom > 700.0);
                let finish = Finish {
                    correction,
                    flick: fast.filter(|_| flick),
                };
                // A pen's touch ends before its button does, so the last
                // points keep the stroke's own last pressure, not a full one.
                let last = stroke
                    .samples()
                    .last()
                    .map_or(pressure, |sample| sample.pressure);
                let mut drawn = None;
                if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
                    if line {
                        stroke.to(raster, raw, last, selection);
                    } else {
                        for point in stabilizer.catch_up(raw) {
                            let point = snap.as_ref().map_or(point, |snap| snap.project(point));
                            stroke.to(raster, point, last, selection);
                        }
                    }
                    // A vector line keeps the samples as drawn.
                    drawn = vector.then(|| stroke.samples().to_vec());
                    stroke.end(raster, finish, selection);
                }
                if let Some([x0, y0, x1, y1]) = stroke.take_dirty() {
                    let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
                    doc.compositor.refresh(
                        &cx.world.arena,
                        [
                            shift(x0, offset[0]),
                            shift(y0, offset[1]),
                            shift(x1, offset[0]),
                            shift(y1, offset[1]),
                        ],
                    );
                }
                let eraser = stroke.tip().eraser;
                let kept = drawn.map(|samples| crate::vector::Line::of(&stroke, samples, finish));
                if let Some(raster) = doc::raster(&cx.world.arena, layer) {
                    let edit = stroke.finish(raster, &label);
                    if !edit.is_empty() {
                        match kept {
                            Some(kept) => {
                                // The line stays on the vector layer, in
                                // the same undo step as its pixels.
                                let old = crate::vector::geometry(&cx.world.arena, layer);
                                let mut lines = crate::vector::lines(&old);
                                lines.push(kept);
                                let edit = crate::vector::keep(
                                    &mut cx.world.arena,
                                    layer,
                                    old,
                                    &lines,
                                    Some(edit),
                                    &label,
                                );
                                cx.record(ArenaEdit(edit));
                            }
                            None => {
                                cx.record(ArenaEdit(edit));
                            }
                        }
                        if !eraser {
                            self.recent.push(self.drawing_color());
                        }
                    }
                }
            }
            Gesture::Shape {
                tool,
                sub,
                op,
                from,
                to,
            } => match tool {
                Tool::Select => {
                    let tiny = (to - from).length() < 2.0;
                    let Some(doc) = self.docs.get_mut(self.active) else {
                        return;
                    };
                    if tiny && op == Op::Replace {
                        doc.set_selection(None);
                        return;
                    }
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    let (min, max) = (from.min(to), from.max(to));
                    if sub.mode == 1 {
                        selection.ellipse([min.x, min.y, max.x, max.y], op);
                    } else {
                        let clamp =
                            |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
                        selection.rect(
                            [
                                clamp(min.x, size[0]),
                                clamp(min.y, size[1]),
                                clamp(max.x, size[0]),
                                clamp(max.y, size[1]),
                            ],
                            op,
                        );
                    }
                    doc.set_selection(Some(selection));
                }
                Tool::Figure => self.figure(from, to, &sub, size, cx),
                Tool::Gradient => self.gradient(from, to, &sub, cx),
                Tool::Frame => self.frame_drag(from, to, &sub, cx),
                Tool::Ruler => self.ruler_drag(from, to, &sub, cx),
                Tool::Balloon => self.balloon_drag(from, to, &sub, cx),
                Tool::CorrectLine => self.remove_dust(from, to, &sub, cx),
                _ => {}
            },
            Gesture::VectorErase {
                layer,
                before,
                geometry,
                lines,
                changed,
                label,
                ..
            } => {
                if !changed {
                    return;
                }
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        label.clone(),
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    &label,
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::Correct {
                layer,
                before,
                geometry,
                mut lines,
                path,
                mut changed,
                label,
                ..
            } => {
                // Redraw vector line and its width act on the whole trace.
                let sub = self.toolbox.current().clone();
                let reach = (8.0
                    / self
                        .docs
                        .get(self.active)
                        .map_or(1.0, |doc| doc.view.zoom)
                        .max(0.01))
                .max(sub.tip.size / 2.0);
                let traced = match sub.mode {
                    5 => {
                        let points: Vec<[f32; 2]> = path.iter().map(|(point, _)| *point).collect();
                        crate::correct::redraw(&mut lines, &points, reach)
                    }
                    6 => crate::correct::redraw_width(&mut lines, &path, reach),
                    _ => false,
                };
                if traced {
                    if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
                        && let Some(painted) =
                            crate::vector::render(&lines, raster.width(), raster.height())
                        && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
                    {
                        into.copy_from_slice(from);
                    }
                    if let Some(doc) = self.docs.get_mut(self.active) {
                        doc.compositor
                            .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
                    }
                    changed = true;
                }
                if !changed {
                    return;
                }
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        label.clone(),
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    &label,
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::MoveLines {
                layer,
                before,
                geometry,
                base,
                by,
                ..
            } => {
                if by == [0.0; 2] {
                    return;
                }
                let picked = self
                    .picked_lines
                    .as_ref()
                    .map(|(_, picked)| picked.clone())
                    .unwrap_or_default();
                let lines = crate::vector::moved(&base, &picked, by);
                let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let patch =
                        doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
                    Some(sue_core::txn::RasterEdit::new(
                        layer,
                        "Move Lines",
                        vec![patch],
                    ))
                });
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    layer,
                    geometry,
                    &lines,
                    pixels,
                    "Move Lines",
                );
                cx.record(ArenaEdit(edit));
            }
            Gesture::RulerPen { points } => {
                if points.len() >= 2 {
                    // Smoothed a little, as a pen line would be.
                    let path = crate::rulers::Path::curve(&crate::correct::simplified_points(
                        &points, 1.0,
                    ));
                    self.add_ruler_path(cx, path, "Create Ruler");
                }
            }
            Gesture::Camera { .. } => {
                // Keyed at this frame, as one step.
                if self.camera_pending.is_some() {
                    self.run("anim.camera.key", cx);
                }
            }
            Gesture::SelectPaint { .. } => {
                // Keep what is selected (nothing selected is no selection).
                if let Some(doc) = self.docs.get_mut(self.active)
                    && let Some(selection) = doc.selection.take()
                {
                    doc.set_selection(Some(selection));
                }
            }
            Gesture::Orbit {
                layer,
                base,
                before,
                ..
            } => self.finish_view3d(cx, layer, base, &before),
            Gesture::RulerHandle { handle, before } => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let key = handle.key();
                let after = crate::rulers::Rulers::of(&cx.world.arena, picture).text(key);
                // Back as it was, then the move as one undo step.
                if let Ok(entity) = cx.world.arena.get_by_id_mut(picture) {
                    match &before {
                        Some(text) => {
                            entity.properties_mut().set(key, text.clone());
                        }
                        None => {
                            entity.properties_mut().remove(key);
                        }
                    }
                }
                if after != before {
                    Self::apply(cx, crate::rulers::edit(picture, key, after, "Move Ruler"));
                }
            }
            Gesture::Lasso { op, points } => {
                if let Some(doc) = self.docs.get_mut(self.active) {
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(size[0], size[1]));
                    selection.polygon(&points, op);
                    doc.set_selection(Some(selection));
                }
            }
            Gesture::Move { layer, old, .. } => {
                let now = cx
                    .world
                    .arena
                    .get_by_id(layer)
                    .map(|entity| {
                        [
                            entity.property(picture::X).cloned(),
                            entity.property(picture::Y).cloned(),
                        ]
                    })
                    .unwrap_or_default();
                if now != old {
                    let mut steps = Batch::new("Move Layer");
                    for (key, before) in [(picture::X, &old[0]), (picture::Y, &old[1])] {
                        steps = steps.then(match before {
                            Some(value) => SetProperty::new(layer, key, value.clone()),
                            None => SetProperty::remove(layer, key),
                        });
                    }
                    cx.record(ArenaEdit(steps));
                }
            }
            Gesture::Pan { .. }
            | Gesture::Rotate { .. }
            | Gesture::Pick { .. }
            | Gesture::Pin { .. }
            | Gesture::Transform { .. }
            | Gesture::BrushSize { .. } => {}
            Gesture::Blend {
                layer,
                before,
                label,
                ..
            } => {
                // The whole drag is one undo step.
                if let Some(after) = doc::raster(&cx.world.arena, layer).and_then(|raster| {
                    let size = [raster.width(), raster.height()];
                    raster.bytes().map(|bytes| (bytes.to_vec(), size))
                }) && let Some(patch) =
                    doc::diff_patch(&before, &after.0, after.1[0], after.1[1])
                {
                    cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                        layer,
                        label,
                        vec![patch],
                    )));
                }
            }
        }
    }

    /// Picks the color under `pos`: as shown, or from the current layer.
    /// Makes the topmost visible layer with paint under picture point `p`
    /// the current layer (Ctrl, Shift and click).
    fn pick_layer_at(&mut self, arena: &sue_core::Arena, p: Pos2) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let hit = doc::layers(arena, doc.picture)
            .into_iter()
            .rev()
            .find(|info| {
                if !info.style.visible {
                    return false;
                }
                let Some(raster) = doc::raster(arena, info.id) else {
                    return false;
                };
                let (x, y) = (
                    p.x as i64 - info.style.offset[0],
                    p.y as i64 - info.style.offset[1],
                );
                if x < 0
                    || y < 0
                    || x >= i64::from(raster.width())
                    || y >= i64::from(raster.height())
                {
                    return false;
                }
                raster.bytes().is_some_and(|bytes| {
                    let at = ((y as usize * raster.width() as usize) + x as usize) * 4 + 3;
                    bytes.get(at).is_some_and(|alpha| *alpha > 0)
                })
            });
        if let Some(info) = hit {
            doc.layer = Some(info.id);
        }
    }

    fn pick(
        &mut self,
        pos: Pos2,
        mode: u8,
        viewport: Rect,
        size: [u32; 2],
        cx: &AppletContext<'_>,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        if p.x < 0.0 || p.y < 0.0 || p.x >= size[0] as f32 || p.y >= size[1] as f32 {
            return;
        }
        let (x, y) = (p.x as u32, p.y as u32);
        let rgb = if mode == 0 {
            doc.compositor.pixel(x, y).map(|[r, g, b, _]| [r, g, b])
        } else {
            self.layer_info(&cx.world.arena).and_then(|info| {
                let raster = doc::raster(&cx.world.arena, info.id)?;
                let (lx, ly) = (
                    i64::from(x) - info.style.offset[0],
                    i64::from(y) - info.style.offset[1],
                );
                if lx < 0
                    || ly < 0
                    || lx >= i64::from(raster.width())
                    || ly >= i64::from(raster.height())
                {
                    return None;
                }
                let at = ((ly as u32 * raster.width() + lx as u32) * 4) as usize;
                let pixel = raster.bytes()?.get(at..at + 4)?;
                (pixel[3] > 0).then(|| [pixel[0], pixel[1], pixel[2]])
            })
        };
        if let Some(rgb) = rgb {
            self.set_drawing_color(rgb);
        }
    }

    /// The area a flood fill from `p` reaches, as a picture-sized mask.
    fn flood_mask(
        &self,
        p: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        arena: &sue_core::Arena,
    ) -> Option<Vec<u8>> {
        if p.x < 0.0 || p.y < 0.0 || p.x >= size[0] as f32 || p.y >= size[1] as f32 {
            return None;
        }
        let picture_id = self.picture()?;
        let reference = if sub.mode == 2 {
            // Only the reference layers, as they show.
            let view = composite::FrameView {
                hidden: doc::layers(arena, picture_id)
                    .into_iter()
                    .map(|info| info.id)
                    .filter(|id| !doc::is_reference(arena, *id))
                    .collect(),
                ..composite::FrameView::default()
            };
            composite::render_view(
                arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
                Some(&view),
            )
        } else if sub.mode == 1 {
            composite::render(
                arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
            )
        } else {
            let info = self.layer_info(arena)?;
            let raster = doc::raster(arena, info.id)?;
            let bytes = raster.bytes()?;
            if info.style.offset == [0, 0] && [raster.width(), raster.height()] == size {
                bytes.to_vec()
            } else {
                let mut placed = vec![0_u8; (size[0] * size[1] * 4) as usize];
                for y in 0..raster.height() {
                    for x in 0..raster.width() {
                        let (px, py) = (
                            i64::from(x) + info.style.offset[0],
                            i64::from(y) + info.style.offset[1],
                        );
                        if px < 0 || py < 0 || px >= i64::from(size[0]) || py >= i64::from(size[1])
                        {
                            continue;
                        }
                        let from = ((y * raster.width() + x) * 4) as usize;
                        let to = ((py as u32 * size[0] + px as u32) * 4) as usize;
                        placed[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                    }
                }
                placed
            }
        };
        let mut mask = fill::flood_closing(
            &reference,
            size,
            [p.x as u32, p.y as u32],
            sub.tolerance,
            fill::gap_radius(sub.gap),
            sub.spread,
        );
        if let Some(selection) = self.doc().and_then(|doc| doc.selection.as_ref()) {
            for (index, value) in mask.iter_mut().enumerate() {
                let (x, y) = (index as u32 % size[0], index as u32 / size[0]);
                *value = (f32::from(*value) * selection.at(x, y)).round() as u8;
            }
        }
        Some(mask)
    }

    /// The paint bucket.
    fn bucket(&mut self, p: Pos2, sub: &SubTool, size: [u32; 2], cx: &mut AppletContext<'_>) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(mask) = self.flood_mask(p, sub, size, &cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
        let (offset, alpha_lock, opacity) = (info.style.offset, info.alpha_locked, sub.tip.opacity);
        match doc::rewrite(&cx.world.arena, info.id, "Fill", |pixels, width, height| {
            fill::paint_mask(
                pixels,
                [width, height],
                offset,
                &mask,
                size,
                rgb,
                opacity,
                alpha_lock,
            );
        }) {
            Ok(Some(edit)) => {
                if Self::apply(cx, edit) {
                    self.recent.push(self.drawing_color());
                }
            }
            Ok(None) => {}
            Err(error) => cx.fail(error),
        }
    }

    /// The auto select tool: a flood fill that makes a selection.
    fn auto_select(
        &mut self,
        p: Pos2,
        sub: &SubTool,
        op: Op,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(mask) = self.flood_mask(p, sub, size, &cx.world.arena) else {
            return;
        };
        if let Some(doc) = self.doc_mut() {
            let mut selection = doc
                .selection
                .clone()
                .unwrap_or_else(|| Selection::new(size[0], size[1]));
            selection.apply_mask(&mask, op);
            doc.set_selection(Some(selection));
        }
    }

    /// The figure tool: lines and outlines with the brush, filled shapes as
    /// fills.
    fn figure(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
        let (min, max) = (from.min(to), from.max(to));
        if sub.mode >= 3 {
            let mut shape = Selection::new(size[0], size[1]);
            if sub.mode == 3 {
                let clamp = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
                shape.rect(
                    [
                        clamp(min.x, size[0]),
                        clamp(min.y, size[1]),
                        clamp(max.x, size[0]),
                        clamp(max.y, size[1]),
                    ],
                    Op::Replace,
                );
            } else {
                shape.ellipse([min.x, min.y, max.x, max.y], Op::Replace);
            }
            let selection = self.doc().and_then(|doc| doc.selection.clone());
            let mask: Vec<u8> = (0..size[0] * size[1])
                .map(|index| {
                    let (x, y) = (index % size[0], index / size[0]);
                    let inside = shape.at(x, y)
                        * selection
                            .as_ref()
                            .map_or(1.0, |selection| selection.at(x, y));
                    (inside * 255.0).round() as u8
                })
                .collect();
            let (offset, alpha_lock, opacity) =
                (info.style.offset, info.alpha_locked, sub.tip.opacity);
            if let Ok(Some(edit)) = doc::rewrite(
                &cx.world.arena,
                info.id,
                &sub.name,
                |pixels, width, height| {
                    fill::paint_mask(
                        pixels,
                        [width, height],
                        offset,
                        &mask,
                        size,
                        rgb,
                        opacity,
                        alpha_lock,
                    );
                },
            ) {
                Self::apply(cx, edit);
            }
            return;
        }
        let points: Vec<[f32; 2]> = match sub.mode {
            0 => vec![[from.x, from.y], [to.x, to.y]],
            1 => vec![
                [min.x, min.y],
                [max.x, min.y],
                [max.x, max.y],
                [min.x, max.y],
                [min.x, min.y],
            ],
            _ => ellipse_points(from, to),
        };
        let offset = info.style.offset;
        let shifted = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map(|selection| {
                if offset == [0, 0] {
                    selection.clone()
                } else {
                    selection.shifted(offset)
                }
            });
        let plain = crate::brush::Tip {
            taper_start: 0.0,
            taper_end: 0.0,
            mix: 0.0,
            ..sub.tip
        };
        let mut stroke = BrushStroke::new(info.id, plain, rgb, info.alpha_locked);
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id) {
            for point in points {
                stroke.to(
                    raster,
                    [point[0] - offset[0] as f32, point[1] - offset[1] as f32],
                    1.0,
                    shifted.as_ref(),
                );
            }
        }
        if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
            let edit = stroke.finish(raster, &sub.name);
            if !edit.is_empty() {
                cx.record(ArenaEdit(edit));
            }
        }
    }

    /// The gradient tool.
    fn gradient(&mut self, from: Pos2, to: Pos2, sub: &SubTool, cx: &mut AppletContext<'_>) {
        if (to - from).length() < 1.0 {
            return;
        }
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let [r, g, b] = color::unit(self.main);
        let opacity = sub.tip.opacity;
        let start = [r, g, b, opacity];
        let end = if sub.mode == 1 {
            let [r, g, b] = color::unit(self.sub);
            [r, g, b, opacity]
        } else {
            [r, g, b, 0.0]
        };
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let (offset, alpha_lock) = (info.style.offset, info.alpha_locked);
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            info.id,
            "Gradient",
            |pixels, width, height| {
                fill::gradient(
                    pixels,
                    [width, height],
                    offset,
                    [[from.x, from.y], [to.x, to.y]],
                    [start, end],
                    sub.mode == 2,
                    selection.as_ref(),
                    alpha_lock,
                );
            },
        ) {
            Self::apply(cx, edit);
        }
    }

    // ----- Drawing ----------------------------------------------------------------------

    fn draw_picture(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(viewport);
        let corners = doc.view.corners(viewport, size);
        let (mut pixels, display_size, mut version) = doc.compositor.display();
        let mut salt = 0;
        if self.proof_on && display_size == size {
            // View, Color Profile, Preview: the picture as printed.
            let profile = self.proof_profile;
            if self
                .proof_cache
                .as_ref()
                .is_none_or(|(seen, kind, _)| *seen != version || *kind != profile)
            {
                self.proof_cache = Some((version, profile, crate::proof::proof(pixels, profile)));
            }
            if let Some((_, _, proofed)) = &self.proof_cache {
                pixels = proofed;
                salt = 0x5052_4F4F;
                version = version.wrapping_mul(8).wrapping_add(profile as u64);
            }
        }
        let texture = (display_size == size)
            .then(|| {
                cx.textures.texture(
                    texture_key(doc.picture, salt),
                    version,
                    size,
                    4,
                    pixels,
                    doc.view.zoom >= 2.0,
                )
            })
            .flatten();
        match texture {
            Some(texture) => {
                let mut mesh = Mesh::with_texture(texture);
                let uvs = [
                    pos2(0.0, 0.0),
                    pos2(1.0, 0.0),
                    pos2(1.0, 1.0),
                    pos2(0.0, 1.0),
                ];
                let tint = Color32::WHITE.gamma_multiply(self.cover);
                for (corner, uv) in corners.iter().zip(uvs) {
                    mesh.vertices.push(Vertex {
                        pos: *corner,
                        uv,
                        color: tint,
                    });
                }
                mesh.indices = vec![0, 1, 2, 0, 2, 3];
                painter.add(Shape::mesh(mesh));
            }
            None => {
                painter.add(Shape::convex_polygon(
                    corners.to_vec(),
                    Color32::from_gray(235),
                    Stroke::NONE,
                ));
            }
        }
        painter.add(Shape::closed_line(
            corners.to_vec(),
            Stroke::new(1.0, theme.border.gamma_multiply(self.cover)),
        ));
    }

    #[allow(clippy::too_many_lines, reason = "every overlay of the canvas")]
    /// Puppet warp's pins, the mesh they bend, and its bar (Reset, Cancel,
    /// OK; Enter keeps the bend and Esc puts the layer back).
    fn warp_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(warp) = self.warp.as_ref() else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let offset = [warp.offset[0] as f32, warp.offset[1] as f32];
        let screen =
            |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0] + offset[0], p[1] + offset[1]));
        let painter = ui.painter().with_clip_rect(viewport);
        // The mesh over the layer's pixels, bent as the pins bend them.
        if let Some([x0, y0, x1, y1]) = warp.bounds {
            let cells = 10;
            let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
            let at = |i: u32, j: u32| {
                let point = [
                    x0 as f32 + w * i as f32 / cells as f32,
                    y0 as f32 + h * j as f32 / cells as f32,
                ];
                screen(crate::warp::forward(point, &warp.pins))
            };
            let stroke = Stroke::new(1.0, theme.muted.gamma_multiply(0.7));
            for j in 0..=cells {
                painter.add(Shape::line((0..=cells).map(|i| at(i, j)).collect(), stroke));
            }
            for i in 0..=cells {
                painter.add(Shape::line((0..=cells).map(|j| at(i, j)).collect(), stroke));
            }
        }
        let held = match self.gesture {
            Some(Gesture::Pin { index }) => Some(index),
            _ => None,
        };
        for (index, pin) in warp.pins.iter().enumerate() {
            let at = screen(pin.to);
            let fill = if held == Some(index) {
                theme.border_active
            } else {
                theme.text
            };
            painter.circle_filled(at, 5.5, fill);
            painter.circle_stroke(at, 5.5, Stroke::new(1.5, Color32::BLACK));
        }
        // The bar.
        let pins = warp.pins.len();
        let mut action = None;
        egui::Area::new(Id::unique("sue-paint-warp-bar"))
            .fade_in(false)
            .order(egui::Order::Foreground)
            .fixed_pos(viewport.center_top() + vec2(-200.0, 10.0))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::pill_frame(theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Puppet warp").color(theme.text));
                        ui.label(
                            RichText::new(match pins {
                                0 => "Click to put pins".to_owned(),
                                1 => "1 pin".to_owned(),
                                count => format!("{count} pins"),
                            })
                            .color(theme.muted),
                        );
                        if ui
                            .button("Reset")
                            .on_hover_text("Take every pin away")
                            .clicked()
                        {
                            action = Some(0);
                        }
                        if ui.button("Cancel").clicked() {
                            action = Some(1);
                        }
                        if ui.button("OK").clicked() {
                            action = Some(2);
                        }
                    });
                });
            });
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
            )
        });
        if enter {
            action = Some(2);
        } else if escape {
            action = Some(1);
        }
        match action {
            Some(0) => {
                if let Some(warp) = self.warp.as_mut() {
                    warp.pins.clear();
                }
                self.warp_preview(cx);
            }
            Some(1) => self.finish_warp(cx, false),
            Some(2) => self.finish_warp(cx, true),
            _ => {}
        }
    }

    fn overlays(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        response: &Response,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.ready {
            return;
        }
        let painter = ui.painter().with_clip_rect(viewport);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let view = doc.view;
        let screen = |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0], p[1]));
        // Grids.
        let visible = {
            let points = [
                viewport.left_top(),
                viewport.right_top(),
                viewport.right_bottom(),
                viewport.left_bottom(),
            ]
            .map(|corner| view.to_picture(viewport, size, corner));
            let min_x = points
                .iter()
                .map(|p| p.x)
                .fold(f32::INFINITY, f32::min)
                .max(0.0);
            let max_x = points
                .iter()
                .map(|p| p.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(size[0] as f32);
            let min_y = points
                .iter()
                .map(|p| p.y)
                .fold(f32::INFINITY, f32::min)
                .max(0.0);
            let max_y = points
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(size[1] as f32);
            [min_x, min_y, max_x, max_y]
        };
        let lines = |step: f32, color: Color32| {
            let count = ((visible[2] - visible[0]) / step) + ((visible[3] - visible[1]) / step);
            if count > 600.0 || step <= 0.0 {
                return;
            }
            let mut x = (visible[0] / step).floor() * step;
            while x <= visible[2] {
                painter.line_segment(
                    [screen([x, visible[1]]), screen([x, visible[3]])],
                    Stroke::new(1.0, color),
                );
                x += step;
            }
            let mut y = (visible[1] / step).floor() * step;
            while y <= visible[3] {
                painter.line_segment(
                    [screen([visible[0], y]), screen([visible[2], y])],
                    Stroke::new(1.0, color),
                );
                y += step;
            }
        };
        if self.pixel_grid && view.zoom >= 12.0 {
            lines(1.0, Color32::from_black_alpha(40));
        }
        if self.grid {
            lines(64.0, Color32::from_rgba_unmultiplied(0, 120, 255, 70));
        }
        // The selection.
        if self.show_selection
            && let Some(selection) = &doc.selection
        {
            let edges = doc.edges.get_or_insert_with(|| selection.edges());
            if edges.len() < 60_000 {
                for [a, b] in edges.iter() {
                    let (a, b) = (screen(*a), screen(*b));
                    painter.line_segment([a, b], Stroke::new(2.0, Color32::BLACK));
                    painter.line_segment([a, b], Stroke::new(1.0, Color32::WHITE));
                }
            }
        }
        // What a drag is drawing.
        match &self.gesture {
            Some(Gesture::Shape {
                tool,
                sub,
                from,
                to,
                ..
            }) => {
                let outline = Stroke::new(1.0, theme.border_active);
                let (min, max) = (from.min(*to), from.max(*to));
                match tool {
                    Tool::Gradient => {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                        painter.circle_stroke(screen([from.x, from.y]), 4.0, outline);
                        painter.circle_stroke(screen([to.x, to.y]), 4.0, outline);
                    }
                    Tool::Figure | Tool::Frame | Tool::Balloon | Tool::Ruler
                        if (tool == &Tool::Figure && sub.mode == 0)
                            || sub.mode == 1
                            || tool == &Tool::Ruler =>
                    {
                        painter.line_segment(
                            [screen([from.x, from.y]), screen([to.x, to.y])],
                            outline,
                        );
                    }
                    _ if (tool == &Tool::Select && sub.mode == 1)
                        || (tool == &Tool::Figure && matches!(sub.mode, 2 | 4))
                        || (tool == &Tool::Balloon && sub.mode == 0) =>
                    {
                        let points = ellipse_points(*from, *to).into_iter().map(screen).collect();
                        painter.add(Shape::closed_line(points, outline));
                    }
                    _ => {
                        let points = [
                            [min.x, min.y],
                            [max.x, min.y],
                            [max.x, max.y],
                            [min.x, max.y],
                        ]
                        .map(screen);
                        painter.add(Shape::closed_line(points.to_vec(), outline));
                    }
                }
            }
            Some(Gesture::Lasso { points, .. }) => {
                let path = points.iter().copied().map(screen).collect::<Vec<_>>();
                painter.add(Shape::line(path, Stroke::new(1.0, theme.border_active)));
            }
            _ => {}
        }
        if doc.show_rulers {
            let turns = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 0)
                .map_or(1, |sub| crate::rulers::turns(sub.lines, sub.mirror));
            let rulers = crate::rulers::Rulers::of(&cx.world.arena, doc.picture);
            ruler_overlay(
                &painter,
                &rulers,
                self.vanishing_line,
                [doc.ruler_on, doc.snap_ruler],
                turns,
                size,
                view.zoom,
                &screen,
            );
        }
        // A curve ruler being placed, or a ruler being drawn with the pen.
        let violet = Color32::from_rgb(170, 120, 255);
        if !self.curve_points.is_empty() {
            let mut points = self.curve_points.clone();
            if let Some(pointer) = self.pointer {
                let at = view.to_picture(viewport, size, pointer);
                points.push([at.x, at.y]);
            }
            let path = crate::rulers::Path::curve(&points);
            let line: Vec<Pos2> = path.points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(1.5, violet)));
            for point in &self.curve_points {
                painter.circle_stroke(screen(*point), 4.0, Stroke::new(1.5, violet));
            }
        }
        if let Some(Gesture::RulerPen { points }) = &self.gesture {
            let line: Vec<Pos2> = points.iter().map(|point| screen(*point)).collect();
            painter.add(Shape::line(line, Stroke::new(1.5, violet)));
        }
        // The brush outline under the pointer.
        let tool = self.toolbox.tool();
        let mode = self.toolbox.current().mode;
        if tool == Tool::CorrectLine
            && let Some(layer) = doc.layer
            && crate::vector::is_vector(&cx.world.arena, layer)
        {
            // The lines' control points, or the trace being drawn.
            let offset = doc::layers(&cx.world.arena, doc.picture)
                .into_iter()
                .find(|info| info.id == layer)
                .map_or([0, 0], |info| info.style.offset);
            let shift = |at: [f32; 2]| screen([at[0] + offset[0] as f32, at[1] + offset[1] as f32]);
            let lines = match &self.gesture {
                Some(Gesture::Correct { lines, .. }) => lines.clone(),
                _ => crate::vector::lines(&crate::vector::geometry(&cx.world.arena, layer)),
            };
            let violet = Color32::from_rgb(120, 160, 255);
            if mode == 0 {
                for line in &lines {
                    for index in line.controls() {
                        let at = shift(line.samples[index].at);
                        let rect = Rect::from_center_size(at, vec2(7.0, 7.0));
                        if line.is_corner(index) {
                            painter.rect_filled(rect, 0.0, violet);
                        } else {
                            painter.rect_filled(rect, 0.0, Color32::WHITE);
                            painter.rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(1.0, violet),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                }
            }
            if let Some(Gesture::Correct { path, .. }) = &self.gesture
                && path.len() > 1
            {
                let points: Vec<Pos2> = path.iter().map(|(at, _)| shift(*at)).collect();
                painter.add(Shape::line(points, Stroke::new(1.5, violet)));
            }
        }
        if tool == Tool::Operation
            && let Some(folder) = doc
                .layer
                .filter(|layer| crate::camera::is_camera(&cx.world.arena, *layer))
        {
            // The camera's frame on the layers it sees.
            let (frame, time) = {
                let rate = crate::anim::timeline(&cx.world.arena, doc.picture)
                    .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
                (doc.frame, crate::camera::time_of(doc.frame, rate))
            };
            let camera = match self.camera_pending {
                Some((pending, at, camera)) if pending == folder && at == frame => camera,
                _ => crate::camera::Camera::of(&cx.world.arena, folder, Some(time), size),
            };
            let corners = camera.frame(size).map(&screen);
            let red = Color32::from_rgb(235, 90, 90);
            painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(2.0, red)));
            for corner in corners {
                painter.rect_filled(Rect::from_center_size(corner, vec2(8.0, 8.0)), 0.0, red);
            }
            let middle = screen(camera.center);
            painter.line_segment(
                [middle - vec2(8.0, 0.0), middle + vec2(8.0, 0.0)],
                Stroke::new(1.5, red),
            );
            painter.line_segment(
                [middle - vec2(0.0, 8.0), middle + vec2(0.0, 8.0)],
                Stroke::new(1.5, red),
            );
        }
        if tool == Tool::Operation
            && let Some((layer, picked)) = &self.picked_lines
            && doc.layer == Some(*layer)
            && !picked.is_empty()
        {
            // The picked lines, outlined, in their box.
            let offset = doc::layers(&cx.world.arena, doc.picture)
                .into_iter()
                .find(|info| info.id == *layer)
                .map_or([0, 0], |info| info.style.offset);
            let base = crate::vector::lines(&crate::vector::geometry(&cx.world.arena, *layer));
            let lines = match &self.gesture {
                Some(Gesture::MoveLines { base, by, .. }) => {
                    crate::vector::moved(base, picked, *by)
                }
                _ => base,
            };
            let blue = Color32::from_rgb(70, 130, 255);
            let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
            for line in picked.iter().filter_map(|index| lines.get(*index)) {
                let points: Vec<Pos2> = line
                    .samples
                    .iter()
                    .map(|sample| {
                        for axis in 0..2 {
                            min[axis] = min[axis].min(sample.at[axis]);
                            max[axis] = max[axis].max(sample.at[axis]);
                        }
                        screen([
                            sample.at[0] + offset[0] as f32,
                            sample.at[1] + offset[1] as f32,
                        ])
                    })
                    .collect();
                painter.add(Shape::line(points, Stroke::new(1.5, blue)));
            }
            if min[0].is_finite() {
                let corners = [
                    [min[0], min[1]],
                    [max[0], min[1]],
                    [max[0], max[1]],
                    [min[0], max[1]],
                ]
                .map(|[x, y]| screen([x + offset[0] as f32, y + offset[1] as f32]));
                painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(1.0, blue)));
            }
        }
        if response.hovered()
            && (tool.paints()
                || tool == Tool::Blend
                || (tool == Tool::CorrectLine && matches!(mode, 2 | 3 | 4 | 6)))
            && let Some(pointer) = self.pointer
        {
            let radius = (self.toolbox.current().tip.size * 0.5 * view.zoom).max(1.5);
            painter.circle_stroke(
                pointer,
                radius,
                Stroke::new(2.0, Color32::from_black_alpha(140)),
            );
            painter.circle_stroke(
                pointer,
                radius,
                Stroke::new(1.0, Color32::from_white_alpha(220)),
            );
            painter.circle_filled(pointer, 1.0, Color32::WHITE);
        }
        let _ = cx;
    }

    fn status_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        size: Option<[u32; 2]>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.top(), theme.hairline());
        let Some(size) = size else {
            return;
        };
        let viewport = self.viewport;
        let mut inner = child(ui, strip.shrink2(vec2(8.0, 2.0)), "status-strip");
        let pointer = self.pointer;
        let layer_name = self
            .layer_info(&cx.world.arena)
            .map(|info| info.name)
            .unwrap_or_default();
        let dpi = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(|entity| {
                entity
                    .property(picture::DPI)
                    .and_then(sue_core::Value::as_f64)
            })
            .unwrap_or(72.0);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        inner.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut percent = doc.view.zoom * 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut percent)
                        .range(1.0..=6400.0)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .changed()
            {
                doc.view
                    .set_zoom_at(viewport, size, viewport.center(), percent / 100.0);
            }
            if ui.small_button("-").on_hover_text("Zoom out").clicked() {
                doc.view.zoom_at(viewport, size, viewport.center(), 0.8);
            }
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                doc.view.zoom_at(viewport, size, viewport.center(), 1.25);
            }
            ui.separator();
            let mut rotation = doc.view.rotation;
            if ui
                .add(
                    egui::DragValue::new(&mut rotation)
                        .range(-180.0..=180.0)
                        .max_decimals(1)
                        .suffix(" \u{b0}"),
                )
                .changed()
            {
                doc.view.set_rotation(viewport, size, rotation);
            }
            if ui
                .small_button("\u{21ba}")
                .on_hover_text("Rotate left")
                .clicked()
            {
                doc.view.rotate_by(viewport, size, -15.0);
            }
            if ui
                .small_button("\u{21bb}")
                .on_hover_text("Rotate right")
                .clicked()
            {
                doc.view.rotate_by(viewport, size, 15.0);
            }
            if ui
                .small_button("0\u{b0}")
                .on_hover_text("Reset rotation")
                .clicked()
            {
                doc.view.set_rotation(viewport, size, 0.0);
            }
            if ui
                .selectable_label(doc.view.flip_h, "Flip")
                .on_hover_text("Flip the view left to right")
                .clicked()
            {
                doc.view.flip(viewport, size, true);
            }
            if ui
                .small_button("Fit")
                .on_hover_text("Fit to screen (Ctrl+0)")
                .clicked()
            {
                doc.view.fit(viewport, size);
            }
            if ui
                .small_button("100%")
                .on_hover_text("Actual pixels (Ctrl+Alt+0)")
                .clicked()
            {
                doc.view.set_zoom_at(viewport, size, viewport.center(), 1.0);
            }
            ui.separator();
            let small = |text: String| {
                RichText::new(text)
                    .size(theme.small_size + 0.5)
                    .color(theme.muted)
            };
            ui.label(small(format!("{} x {} px, {dpi:.0} dpi", size[0], size[1])));
            if let Some(pointer) = pointer {
                let p = doc.view.to_picture(viewport, size, pointer);
                if p.x >= 0.0 && p.y >= 0.0 && p.x < size[0] as f32 && p.y < size[1] as f32 {
                    ui.separator();
                    ui.label(small(format!("{:.0}, {:.0}", p.x.floor(), p.y.floor())));
                }
            }
            if !layer_name.is_empty() {
                ui.separator();
                ui.label(small(layer_name.clone()));
            }
        });
        let _ = Align2::LEFT_CENTER;
    }
}

/// Draws `rulers` (picture pixels, `screen` maps them) in Clip Studio
/// Paint's violet, faint while their snapping (`[special, linear]`) is off,
/// with the first line of a vanishing point still to be drawn dashed.
#[allow(clippy::too_many_arguments, reason = "the rulers and how they show")]
#[allow(clippy::too_many_lines, reason = "one block per ruler")]
fn ruler_overlay(
    painter: &egui::Painter,
    rulers: &crate::rulers::Rulers,
    vanishing_line: Option<[[f32; 2]; 2]>,
    snapping: [bool; 2],
    turns: u32,
    size: [u32; 2],
    zoom: f32,
    screen: &dyn Fn([f32; 2]) -> Pos2,
) {
    use crate::rulers::Special;
    let violet = Color32::from_rgb(170, 120, 255);
    let tone = |on: bool| {
        if on {
            violet
        } else {
            violet.gamma_multiply(0.35)
        }
    };
    let (special, linear) = (tone(snapping[0]), tone(snapping[1]));
    let (w, h) = (size[0] as f32, size[1] as f32);
    let diagonal = w.hypot(h);
    let reach = diagonal * 4.0;
    let middle = [w / 2.0, h / 2.0];
    // A whole line through `at` along `direction`.
    let across = |at: [f32; 2], direction: [f32; 2], stroke: Stroke| {
        painter.line_segment(
            [
                screen([at[0] - direction[0] * reach, at[1] - direction[1] * reach]),
                screen([at[0] + direction[0] * reach, at[1] + direction[1] * reach]),
            ],
            stroke,
        );
    };
    if let Some((center, angle)) = rulers.symmetry {
        let turns = turns.max(1);
        for line in 0..turns {
            let theta = angle + line as f32 * std::f32::consts::PI / turns as f32;
            let (dx, dy) = (theta.cos() * reach, theta.sin() * reach);
            let ends = [
                screen([center[0] - dx, center[1] - dy]),
                screen([center[0] + dx, center[1] + dy]),
            ];
            painter.add(Shape::dashed_line(
                &ends,
                Stroke::new(1.5, special),
                8.0,
                5.0,
            ));
        }
        painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
    }
    if let Some(perspective) = &rulers.perspective {
        // Lines from every vanishing point to the canvas edge, all round.
        let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        let rim: Vec<[f32; 2]> = (0..4)
            .flat_map(|side| {
                let (a, b) = (corners[side], corners[(side + 1) % 4]);
                (0..4).map(move |step| {
                    let t = step as f32 / 4.0;
                    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                })
            })
            .collect();
        let faint = Stroke::new(1.0, special.gamma_multiply(0.4));
        for point in &perspective.points {
            for target in &rim {
                painter.line_segment([screen(*point), screen(*target)], faint);
            }
        }
        let (origin, direction) = perspective.eye_level();
        across(origin, direction, Stroke::new(1.5, special));
        let upright = [-direction[1], direction[0]];
        match perspective.points.len() {
            1 => across(origin, upright, faint),
            2 => {
                if let Some(at) = rulers.eye_handle(size) {
                    across(at, upright, faint);
                }
            }
            _ => {}
        }
        for point in &perspective.points {
            painter.circle_stroke(screen(*point), 5.0, Stroke::new(1.5, special));
            painter.circle_filled(screen(*point), 1.5, special);
        }
        if let Some(at) = rulers.eye_handle(size) {
            painter.rect_stroke(
                Rect::from_center_size(screen(at), vec2(8.0, 8.0)),
                0.0,
                Stroke::new(1.5, special),
                egui::StrokeKind::Middle,
            );
        }
    }
    match rulers.special {
        Some(Special::Parallel(angle)) => {
            let direction = [angle.cos(), angle.sin()];
            let normal = [-direction[1], direction[0]];
            for step in -6..=6 {
                let away = step as f32 * diagonal / 12.0;
                across(
                    [middle[0] + normal[0] * away, middle[1] + normal[1] * away],
                    direction,
                    Stroke::new(1.0, special),
                );
            }
        }
        Some(Special::Radial(center)) => {
            for step in 0..24 {
                let theta = step as f32 * std::f32::consts::TAU / 24.0;
                painter.line_segment(
                    [
                        screen(center),
                        screen([
                            center[0] + theta.cos() * reach,
                            center[1] + theta.sin() * reach,
                        ]),
                    ],
                    Stroke::new(1.0, special),
                );
            }
            painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
        }
        Some(Special::Concentric(center)) => {
            for step in 1..=12 {
                let radius = step as f32 * diagonal / 12.0 * zoom;
                painter.circle_stroke(screen(center), radius, Stroke::new(1.0, special));
            }
            painter.circle_stroke(screen(center), 5.0, Stroke::new(1.5, special));
        }
        None => {}
    }
    for path in &rulers.paths {
        let mut points: Vec<Pos2> = path.points.iter().map(|point| screen(*point)).collect();
        if path.closed
            && let Some(first) = points.first().copied()
        {
            points.push(first);
        }
        painter.add(Shape::line(points, Stroke::new(1.5, linear)));
    }
    if let Some([a, b]) = rulers.linear {
        painter.line_segment([screen(a), screen(b)], Stroke::new(2.0, linear));
        for end in [a, b] {
            painter.circle_filled(screen(end), 3.5, linear);
        }
    }
    if let Some([a, b]) = vanishing_line {
        let direction = [b[0] - a[0], b[1] - a[1]];
        let length = direction[0].hypot(direction[1]).max(1e-3);
        let direction = [direction[0] / length, direction[1] / length];
        let ends = [
            screen([a[0] - direction[0] * reach, a[1] - direction[1] * reach]),
            screen([a[0] + direction[0] * reach, a[1] + direction[1] * reach]),
        ];
        painter.add(Shape::dashed_line(
            &ends,
            Stroke::new(1.5, violet),
            8.0,
            5.0,
        ));
    }
}
