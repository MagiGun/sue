//! Shapes drawn on a layer: figures and straight lines, and the
//! points around an ellipse.

use egui::Pos2;
use sue_wm::{AppletContext, ArenaEdit};

use crate::brush::Stroke as BrushStroke;
use crate::color;
use crate::doc;
use crate::fill;
use crate::select::{Op, Selection};
use crate::studio::SuePaint;
use crate::tools::SubTool;

/// Points around an ellipse inside two corners.
pub(super) fn ellipse_points(from: Pos2, to: Pos2) -> Vec<[f32; 2]> {
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
    /// The figure tool: lines and outlines with the brush, filled shapes as
    /// fills.
    pub(super) fn figure(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if sub.mode < 3 {
            // A line figure on a vector layer is laid as a vector line.
            let (min, max) = (from.min(to), from.max(to));
            let points = Self::figure_points(sub.mode, from, to, min, max);
            if self.figure_on_vector(&points, sub.mode != 2, sub, cx) {
                return;
            }
        }
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
        let points = Self::figure_points(sub.mode, from, to, min, max);
        self.stroke_line(points, sub, cx);
    }

    /// The points of the line figure `mode` (0 a straight line, 1 a
    /// rectangle, else an ellipse) dragged from `from` to `to`, whose
    /// corners are `min` and `max`.
    fn figure_points(mode: u8, from: Pos2, to: Pos2, min: Pos2, max: Pos2) -> Vec<[f32; 2]> {
        match mode {
            0 => vec![[from.x, from.y], [to.x, to.y]],
            1 => vec![
                [min.x, min.y],
                [max.x, min.y],
                [max.x, max.y],
                [min.x, max.y],
                [min.x, min.y],
            ],
            _ => ellipse_points(from, to),
        }
    }

    /// The Figure tool's lines: `points` (picture pixels) drawn with the
    /// sub tool's brush, untapered, within the selection, as one step.
    pub(crate) fn stroke_line(
        &mut self,
        points: Vec<[f32; 2]>,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
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
            stroke.flush(raster, shifted.as_ref());
        }
        if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
            let edit = stroke.finish(raster, &sub.name);
            if !edit.is_empty() {
                cx.record(ArenaEdit(edit));
            }
        }
    }
}
