//! Picking under the pointer: the color, the layer and the drawing
//! figure's body part, and the current layer's pixels placed on the
//! picture for the fills that look at it.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::overlays::segment_distance;
use crate::doc;
use crate::studio::SuePaint;

impl SuePaint {
    /// Makes the topmost visible layer with paint under picture point `p`
    /// the current layer (Ctrl, Shift and click).
    pub(super) fn pick_layer_at(&mut self, arena: &sue_core::Arena, p: Pos2) {
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

    /// Picks the color under `pos`: as shown, or from the current layer.
    pub(super) fn pick(
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

    /// The current layer's pixels where they sit on a picture of `size`
    /// (straight RGBA8), for fills that look at it.
    pub(crate) fn layer_on_picture(
        &self,
        arena: &sue_core::Arena,
        size: [u32; 2],
    ) -> Option<Vec<u8>> {
        let info = self.layer_info(arena)?;
        let raster = doc::raster(arena, info.id)?;
        let bytes = raster.bytes()?;
        if info.style.offset == [0, 0] && [raster.width(), raster.height()] == size {
            return Some(bytes.to_vec());
        }
        let mut placed = vec![0_u8; (size[0] * size[1] * 4) as usize];
        for y in 0..raster.height() {
            for x in 0..raster.width() {
                let (px, py) = (
                    i64::from(x) + info.style.offset[0],
                    i64::from(y) + info.style.offset[1],
                );
                if px < 0 || py < 0 || px >= i64::from(size[0]) || py >= i64::from(size[1]) {
                    continue;
                }
                let from = ((y * raster.width() + x) * 4) as usize;
                let to = ((py as u32 * size[0] + px as u32) * 4) as usize;
                placed[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
            }
        }
        Some(placed)
    }

    /// The drawing figure's body part (its joint) under picture point `p`
    /// on 3D layer `layer`, if there is one near enough.
    pub(crate) fn figure_part_at(
        &self,
        arena: &sue_core::Arena,
        layer: sue_core::EntityId,
        p: Pos2,
        size: [u32; 2],
    ) -> Option<usize> {
        let model = arena
            .get_by_id(crate::figure::of_layer(arena, layer)?)
            .ok()?;
        let (pose, body) = (
            crate::figure::Pose::of(model),
            crate::figure::Body::of(model),
        );
        let view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
        let raster = doc::raster(arena, layer)?;
        let lens =
            crate::three::Camera::new(&view, [raster.width(), raster.height()], body.frame());
        let offset = self
            .layer_info(arena)
            .map_or([0, 0], |info| info.style.offset);
        let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        let zoom = self.doc().map_or(1.0, |doc| doc.view.zoom).max(0.01);
        let reach = (16.0 / zoom).max(size[0].min(size[1]) as f32 * 0.004);
        crate::figure::bones(&pose, &body)
            .iter()
            .enumerate()
            .filter_map(|(joint, (start, end))| {
                let (a, b) = (lens.project(*start)?, lens.project(*end)?);
                let distance = segment_distance(local, a, b);
                (distance <= reach).then_some((joint, distance))
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(joint, _)| joint)
    }
}
