//! Picking under the pointer: the color, the layer and the drawing
//! figure's body part, and the current layer's pixels placed on the
//! picture for the fills that look at it.

use egui::{Pos2, Rect};
use sue_core::EntityId;
use sue_wm::AppletContext;

use super::overlays::segment_distance;
use crate::doc;
use crate::fill::{Exclude, Refer};
use crate::studio::SuePaint;
use crate::tools::Tool;

impl SuePaint {
    /// The topmost shown layer with paint under picture point `p`, less
    /// what `exclude` leaves out (Select layer's Exclude row).
    pub(crate) fn layer_painted_at(
        &self,
        arena: &sue_core::Arena,
        p: Pos2,
        exclude: Exclude,
    ) -> Option<EntityId> {
        let p = [p.x.floor() as i64, p.y.floor() as i64];
        self.layers_painted_in(arena, [p, p], exclude)
            .first()
            .copied()
    }

    /// Every shown layer with paint in the picture box between the two
    /// pixels of `area` (both in it), topmost first, less what `exclude`
    /// leaves out.
    pub(crate) fn layers_painted_in(
        &self,
        arena: &sue_core::Arena,
        area: [[i64; 2]; 2],
        exclude: Exclude,
    ) -> Vec<EntityId> {
        let Some(doc) = self.docs.get(self.active) else {
            return Vec::new();
        };
        let (left, right) = (area[0][0].min(area[1][0]), area[0][0].max(area[1][0]));
        let (top, bottom) = (area[0][1].min(area[1][1]), area[0][1].max(area[1][1]));
        doc::layers(arena, doc.picture)
            .into_iter()
            .rev()
            .filter(|info| {
                info.style.visible
                    && !(exclude.drafts && info.style.draft)
                    && !(exclude.locked && info.locked)
                    && !(exclude.paper && info.kind == sue_model::picture::LayerKind::Paper)
                    && !(exclude.text
                        && arena
                            .get_by_id(info.id)
                            .is_ok_and(|entity| entity.class() == doc::TEXT))
            })
            .filter(|info| {
                let Some(raster) = doc::raster(arena, info.id) else {
                    return false;
                };
                let Some(bytes) = raster.bytes() else {
                    return false;
                };
                let (w, h) = (i64::from(raster.width()), i64::from(raster.height()));
                let [ox, oy] = info.style.offset;
                let (x0, x1) = ((left - ox).max(0), (right - ox).min(w - 1));
                let (y0, y1) = ((top - oy).max(0), (bottom - oy).min(h - 1));
                (y0..=y1).any(|y| {
                    (x0..=x1).any(|x| {
                        bytes
                            .get(((y * w + x) * 4 + 3) as usize)
                            .is_some_and(|alpha| *alpha > 0)
                    })
                })
            })
            .map(|info| info.id)
            .collect()
    }

    /// Makes the topmost shown layer with paint under picture point `p`
    /// the current layer (Ctrl, Shift and click; Select layer), less what
    /// `exclude` leaves out.
    pub(super) fn pick_layer_at(&mut self, arena: &sue_core::Arena, p: Pos2, exclude: Exclude) {
        if let Some(hit) = self.layer_painted_at(arena, p, exclude)
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            doc.layer = Some(hit);
            doc.picked.clear();
        }
    }

    /// Picks the color under `pos`: as shown (`mode` 0), or from the
    /// layers (1) the Eyedropper's Reference names, the editing one unless
    /// it says more, less what its Exclude row leaves out. Picking from
    /// layers where none has paint picks the transparent color.
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
        let rgba = if mode == 0 {
            doc.compositor.pixel(x, y)
        } else {
            let sub = self.toolbox.of(Tool::Eyedropper);
            let arena = &cx.world.arena;
            match sub.referred() {
                Refer::Editing => self.layer_info(arena).map(|info| {
                    doc::raster(arena, info.id)
                        .and_then(|raster| {
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
                            Some([pixel[0], pixel[1], pixel[2], pixel[3]])
                        })
                        .unwrap_or([0; 4])
                }),
                refer => {
                    let layers = crate::fill::ReferLayer::of_picture(arena, doc.picture);
                    let keep =
                        crate::fill::referred(&layers, doc.layer, &doc.picked, refer, sub.exclude);
                    let view = crate::composite::FrameView {
                        hidden: layers
                            .iter()
                            .filter(|layer| !layer.folder && !keep.contains(&layer.id))
                            .map(|layer| layer.id)
                            .collect(),
                        ..crate::composite::FrameView::default()
                    };
                    let pixel = crate::composite::render_view(
                        arena,
                        doc.picture,
                        [x, y, x + 1, y + 1],
                        crate::composite::RenderOptions {
                            paper: !sub.exclude.paper,
                            drafts: !sub.exclude.drafts,
                        },
                        Some(&view),
                    );
                    pixel
                        .get(..4)
                        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                }
            }
        };
        match rgba {
            // Nothing painted there on the layers looked at: the
            // transparent color (Obtain layer color on an empty spot).
            Some([_, _, _, 0]) if mode != 0 => self.transparent = true,
            Some([r, g, b, _]) => {
                self.transparent = false;
                self.set_drawing_color([r, g, b]);
            }
            None => {}
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
