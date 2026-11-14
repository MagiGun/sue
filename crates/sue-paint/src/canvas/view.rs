//! Drawing the picture into the view, with the tone overlay, page
//! frames, rulers along the edges and the scrollbars.

use egui::epaint::Vertex;
use egui::{Align2, Color32, Id, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use sue_model::picture;
use sue_wm::{AppletContext, Theme};

use crate::doc;
use crate::studio::{SuePaint, texture_key};

impl SuePaint {
    /// View, Tone Area: screen tone layers tinted blue over the picture.
    pub(super) fn tone_overlay(
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
    pub(super) fn page_frames(
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
        // The folio, as it will print.
        if let Some((text, at, points)) = crate::story::folio_for(arena, doc.picture) {
            let view = doc.view;
            let at = view.to_screen(viewport, size, pos2(at[0], at[1]));
            ui.painter().with_clip_rect(viewport).text(
                at,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional((points * view.zoom).max(4.0)),
                Color32::BLACK,
            );
        }
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
    pub(super) fn ruler_bars(&self, ui: &mut Ui, viewport: Rect, size: [u32; 2], theme: &Theme) {
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

    pub(super) fn draw_picture(
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
        // While painting only what the stroke changed goes to the GPU; a
        // proofed picture changes everywhere, so it goes whole.
        let compositor = &doc.compositor;
        let changed = |since: u64| {
            if salt == 0 {
                compositor.changed_since(since)
            } else {
                None
            }
        };
        let texture = (display_size == size)
            .then(|| {
                cx.textures.texture_changed(
                    texture_key(doc.picture, salt),
                    version,
                    size,
                    4,
                    pixels,
                    doc.view.zoom >= 2.0,
                    &changed,
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
}

/// How thick the canvas's scrollbars are.
pub(super) const SCROLLBAR: f32 = 12.0;

/// A scrollbar along `bar` for a picture `extent` points long seen through
/// a view `seen` long, at `pan` (the picture's center from the view's).
/// The picture can scroll until its edge reaches the view's middle. The new
/// pan while the thumb is dragged.
#[allow(clippy::too_many_arguments, reason = "a scrollbar's whole state")]
pub(super) fn scrollbar(
    ui: &mut Ui,
    bar: Rect,
    horizontal: bool,
    pan: f32,
    extent: f32,
    seen: f32,
    salt: &str,
    theme: &Theme,
) -> Option<f32> {
    let along = |rect: Rect| {
        if horizontal {
            rect.width()
        } else {
            rect.height()
        }
    };
    let start = |rect: Rect| if horizontal { rect.left() } else { rect.top() };
    let reach = (extent + seen) / 2.0;
    let length = along(bar);
    let thumb = (length * seen / (extent + seen).max(1.0)).clamp(16.0, length);
    // Panning right moves the picture right, so the thumb goes left.
    let t = ((reach - pan) / (2.0 * reach).max(1.0)).clamp(0.0, 1.0);
    let at = start(bar) + t * (length - thumb);
    let thumb_rect = if horizontal {
        Rect::from_min_size(pos2(at, bar.top() + 2.0), vec2(thumb, bar.height() - 4.0))
    } else {
        Rect::from_min_size(pos2(bar.left() + 2.0, at), vec2(bar.width() - 4.0, thumb))
    };
    let response = ui.interact(bar, Id::unique(salt), Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(bar, 0.0, theme.void);
    painter.rect_filled(
        thumb_rect,
        3.0,
        if response.dragged() || response.hovered() {
            theme.text.gamma_multiply(0.8)
        } else {
            theme.muted.gamma_multiply(0.75)
        },
    );
    let pointer = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())?;
    let p = if horizontal { pointer.x } else { pointer.y };
    let t = ((p - start(bar) - thumb / 2.0) / (length - thumb).max(1.0)).clamp(0.0, 1.0);
    Some(reach - t * 2.0 * reach)
}

/// Layer rectangles moved by the layer's `offset` onto the picture.
pub(super) fn shift_rects(rects: &[[u32; 4]], offset: [i64; 2]) -> Vec<[u32; 4]> {
    let shift = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
    rects
        .iter()
        .map(|[x0, y0, x1, y1]| {
            [
                shift(*x0, offset[0]),
                shift(*y0, offset[1]),
                shift(*x1, offset[0]),
                shift(*y1, offset[1]),
            ]
        })
        .collect()
}

/// The part of a picture of `size` that `view` shows in `viewport`, as
/// `[x0, y0, x1, y1]` (with a pixel to spare), and how many picture pixels
/// a side a screen pixel spans there.
pub(super) fn on_screen(
    view: &crate::view::View,
    viewport: Rect,
    size: [u32; 2],
    pixels_per_point: f32,
) -> ([u32; 4], u32) {
    let corners = [
        viewport.left_top(),
        viewport.right_top(),
        viewport.right_bottom(),
        viewport.left_bottom(),
    ]
    .map(|corner| view.to_picture(viewport, size, corner));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for corner in corners {
        x0 = x0.min(corner.x);
        y0 = y0.min(corner.y);
        x1 = x1.max(corner.x);
        y1 = y1.max(corner.y);
    }
    let clamp = |value: f32, max: u32| (value.max(0.0) as u32).min(max);
    let visible = [
        clamp(x0 - 1.0, size[0]),
        clamp(y0 - 1.0, size[1]),
        clamp(x1 + 2.0, size[0]),
        clamp(y1 + 2.0, size[1]),
    ];
    (
        visible,
        crate::composite::view_step(view.zoom * pixels_per_point),
    )
}
