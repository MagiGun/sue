//! Fills and selections by area: the flood mask, the bucket, lasso
//! fill, auto select and gradients.
//!
//! What a fill looks at is what the canvas shows at the frame shown (an
//! animation's other cels stay out of it); where it lays its color is
//! [`AreaTarget`]: the editing layer, or its mask while the mask is being
//! edited, and the transparent color clears instead of painting.

use egui::Pos2;
use sue_core::{Arena, EntityId};
use sue_wm::AppletContext;

use crate::color;
use crate::composite::{self, FrameView, RenderOptions};
use crate::doc;
use crate::fill;
use crate::select::{Op, Selection};
use crate::studio::SuePaint;
use crate::tools::SubTool;

/// Where a fill, a lasso fill or a gradient lays its color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AreaTarget {
    /// The raster written: the editing layer, or its mask.
    pub(crate) id: EntityId,
    /// Where the raster's top left sits in the picture.
    pub(crate) offset: [i64; 2],
    /// The color laid (sRGB 0 to 1); `None` clears (the transparent
    /// color, or hiding on a mask).
    pub(crate) color: Option<[f32; 3]>,
    /// The layer's alpha is kept (Lock Transparent Pixels).
    pub(crate) alpha_lock: bool,
    /// The raster is the layer's mask.
    pub(crate) mask: bool,
}

/// The pixels of raster `id` laid at `offset` on a picture of `size`
/// (straight RGBA8), clear around them.
fn placed(arena: &Arena, id: EntityId, offset: [i64; 2], size: [u32; 2]) -> Option<Vec<u8>> {
    let raster = doc::raster(arena, id)?;
    let bytes = raster.bytes()?;
    let (width, height) = (raster.width(), raster.height());
    if offset == [0, 0] && [width, height] == size {
        return Some(bytes.to_vec());
    }
    let mut out = vec![0_u8; size[0] as usize * size[1] as usize * 4];
    for y in 0..height {
        let py = i64::from(y) + offset[1];
        if py < 0 || py >= i64::from(size[1]) {
            continue;
        }
        for x in 0..width {
            let px = i64::from(x) + offset[0];
            if px < 0 || px >= i64::from(size[0]) {
                continue;
            }
            let from = ((y * width + x) * 4) as usize;
            let to = ((py as u32 * size[0] + px as u32) * 4) as usize;
            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
        }
    }
    Some(out)
}

impl SuePaint {
    /// Where a fill lays its color now, or why it cannot: the editing
    /// layer, or its mask while Edit Mask is on. On a mask any color
    /// shows the layer and the transparent color hides it, as brushes do
    /// there.
    pub(crate) fn area_target(&self, arena: &Arena) -> Result<AreaTarget, String> {
        let info = self.paintable(arena)?;
        let clear = self.transparent;
        let editing_mask = self.doc().is_some_and(|doc| doc.mask_edit);
        if editing_mask && let Some(mask) = doc::mask_of(arena, info.id) {
            return Ok(AreaTarget {
                id: mask,
                offset: doc::mask_offset(arena, info.id).unwrap_or(info.style.offset),
                color: (!clear).then_some([1.0; 3]),
                alpha_lock: false,
                mask: true,
            });
        }
        Ok(AreaTarget {
            id: info.id,
            offset: info.style.offset,
            color: (!clear).then(|| color::unit(self.drawing_color())),
            alpha_lock: info.alpha_locked,
            mask: false,
        })
    }

    /// What the frame shown shows of an animation (its cels, and the
    /// layers its keyframes move or fade), while the timeline is on;
    /// `None` for a still picture.
    pub(crate) fn frame_view(&self, arena: &Arena) -> Option<FrameView> {
        let (picture, sequence, _) = self.timeline_here(arena).filter(|_| self.timeline_on)?;
        let frame = self.doc()?.frame;
        let mut view = crate::anim::view_in(arena, picture, &sequence, frame, None);
        // The camera frames what is shown; a fill works in the picture.
        view.cameras.clear();
        Some(view)
    }

    /// What Fill and Auto Select look at, as straight RGBA8 picture
    /// pixels: the editing layer (or the mask being edited) alone, or the
    /// layers `sub` refers to as the frame shown shows them.
    pub(in crate::canvas) fn fill_reference(
        &self,
        sub: &SubTool,
        size: [u32; 2],
        arena: &Arena,
    ) -> Option<Vec<u8>> {
        let picture_id = self.picture()?;
        match sub.referred() {
            fill::Refer::Editing => match self.area_target(arena) {
                Ok(target) if target.mask => placed(arena, target.id, target.offset, size),
                _ => self.layer_on_picture(arena, size),
            },
            refer => {
                let picked = self.doc().map(|doc| doc.picked.clone()).unwrap_or_default();
                fill::reference_in(
                    arena,
                    picture_id,
                    size,
                    self.layer(),
                    &picked,
                    refer,
                    sub.exclude,
                    self.frame_view(arena).as_ref(),
                )
            }
        }
    }

    /// The area a flood fill from each of `seeds` (picture pixels)
    /// reaches together, as a picture-sized mask; `clip` keeps it inside
    /// the selection (a fill does, Auto Select makes the next selection).
    /// A seed inside the area already taken adds nothing.
    fn flood_mask(
        &self,
        seeds: &[[f32; 2]],
        sub: &SubTool,
        size: [u32; 2],
        arena: &Arena,
        clip: bool,
    ) -> Option<Vec<u8>> {
        let inside = |p: &&[f32; 2]| {
            p[0] >= 0.0 && p[1] >= 0.0 && p[0] < size[0] as f32 && p[1] < size[1] as f32
        };
        let seeds: Vec<[u32; 2]> = seeds
            .iter()
            .filter(inside)
            .map(|p| [p[0] as u32, p[1] as u32])
            .collect();
        if seeds.is_empty() {
            return None;
        }
        let reference = self.fill_reference(sub, size, arena)?;
        let settings = sub.area();
        let mut mask: Option<Vec<u8>> = None;
        for seed in seeds {
            let index = (seed[1] * size[0] + seed[0]) as usize;
            if mask.as_ref().is_some_and(|mask| mask[index] > 0) {
                continue;
            }
            let area = fill::area(&reference, size, seed, &settings);
            match mask.as_mut() {
                None => mask = Some(area),
                Some(mask) => {
                    for (into, value) in mask.iter_mut().zip(area) {
                        *into = (*into).max(value);
                    }
                }
            }
        }
        let mut mask = mask?;
        if let Some(selection) = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .filter(|_| clip)
        {
            for (index, value) in mask.iter_mut().enumerate() {
                let (x, y) = (index as u32 % size[0], index as u32 / size[0]);
                *value = (f32::from(*value) * selection.at(x, y)).round() as u8;
            }
        }
        Some(mask)
    }

    /// Lays `target`'s color (or clears) at `opacity` over the picture
    /// pixels `mask` covers, as one undo step named `label`.
    fn lay_area(
        &mut self,
        target: AreaTarget,
        mask: &[u8],
        size: [u32; 2],
        opacity: f32,
        label: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let AreaTarget {
            id,
            offset,
            color,
            alpha_lock,
            mask: on_mask,
        } = target;
        match doc::rewrite(
            &cx.world.arena,
            id,
            label,
            |pixels, width, height| match color {
                Some(rgb) => fill::paint_mask(
                    pixels,
                    [width, height],
                    offset,
                    mask,
                    size,
                    rgb,
                    opacity,
                    alpha_lock,
                ),
                None => fill::paint_blended(
                    pixels,
                    [width, height],
                    offset,
                    mask,
                    size,
                    None,
                    opacity,
                    sue_model::BlendMode::Normal,
                    alpha_lock,
                ),
            },
        ) {
            Ok(Some(edit)) => {
                if Self::apply(cx, edit) && color.is_some() && !on_mask {
                    self.recent.push(self.drawing_color());
                }
            }
            Ok(None) => {}
            Err(error) => cx.fail(error),
        }
    }

    /// The paint bucket: the areas a click (or a drag, every area it
    /// crosses) at `seeds` reaches fill, as one undo step.
    pub(super) fn bucket(
        &mut self,
        seeds: &[[f32; 2]],
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let target = match self.area_target(&cx.world.arena) {
            Ok(target) => target,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(mask) = self.flood_mask(seeds, sub, size, &cx.world.arena, true) else {
            return;
        };
        self.lay_area(target, &mask, size, sub.tip.opacity, &tr!("fill"), cx);
    }

    /// Lasso fill and Close and fill: the area drawn round (or, closing,
    /// the areas inside it that lines close off) filled with the drawing
    /// color, within the selection.
    pub(super) fn lasso_fill(
        &mut self,
        sub: &SubTool,
        close: bool,
        points: &[[f32; 2]],
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if points.len() < 3 {
            return;
        }
        let Ok(target) = self.area_target(&cx.world.arena) else {
            return;
        };
        let Some(picture_id) = self.picture() else {
            return;
        };
        let mut shape = Selection::new(size[0], size[1]);
        shape.polygon(points, Op::Replace);
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let lasso: Vec<u8> = (0..size[0] * size[1])
            .map(|index| {
                let (x, y) = (index % size[0], index / size[0]);
                let inside = shape.at(x, y)
                    * selection
                        .as_ref()
                        .map_or(1.0, |selection| selection.at(x, y));
                (inside * 255.0).round() as u8
            })
            .collect();
        let mask = if close {
            let reference = composite::render_view(
                &cx.world.arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
                self.frame_view(&cx.world.arena).as_ref(),
            );
            if sub.mode == crate::tools::PAINT_UNPAINTED {
                let editing = if target.mask {
                    placed(&cx.world.arena, target.id, target.offset, size)
                } else {
                    self.layer_on_picture(&cx.world.arena, size)
                };
                let Some(layer) = editing else {
                    return;
                };
                fill::unpainted(&layer, &reference, size, &lasso)
            } else {
                fill::enclosed(&reference, size, &lasso)
            }
        } else {
            lasso
        };
        if mask.iter().all(|value| *value == 0) {
            return;
        }
        let label = crate::localize(&sub.name);
        self.lay_area(target, &mask, size, sub.tip.opacity, &label, cx);
    }

    /// The auto select tool: a flood fill that makes a selection.
    pub(super) fn auto_select(
        &mut self,
        p: Pos2,
        sub: &SubTool,
        op: Op,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(mask) = self.flood_mask(&[[p.x, p.y]], sub, size, &cx.world.arena, false) else {
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

    /// The gradient tool: from `from` to `to` on the editing layer (or
    /// the mask being edited, where it shows the layer); with the
    /// transparent color it clears as much as it would have laid.
    pub(super) fn gradient(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        if (to - from).length() < 1.0 {
            return;
        }
        let Ok(target) = self.area_target(&cx.world.arena) else {
            return;
        };
        let opacity = sub.tip.opacity;
        // Main to Sub Color runs from the main color to the sub color; the
        // others from the color brushes draw with (which X switches) to
        // nothing. On a mask every color shows the layer.
        let unit = |rgb: [u8; 3]| {
            if target.mask {
                [1.0; 3]
            } else {
                color::unit(rgb)
            }
        };
        let main_to_sub = sub.mode == 1;
        let [r, g, b] = if main_to_sub {
            unit(self.main)
        } else {
            target.color.unwrap_or([1.0; 3])
        };
        let start = [r, g, b, opacity];
        let end = if main_to_sub {
            let [r, g, b] = unit(self.sub);
            [r, g, b, opacity]
        } else {
            [r, g, b, 0.0]
        };
        let clear = target.color.is_none() && !main_to_sub;
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let (offset, alpha_lock) = (target.offset, target.alpha_lock);
        let line = [[from.x, from.y], [to.x, to.y]];
        let radial = sub.mode == 2;
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            target.id,
            &tr!("gradient"),
            |pixels, width, height| {
                if !clear {
                    fill::gradient(
                        pixels,
                        [width, height],
                        offset,
                        line,
                        [start, end],
                        radial,
                        selection.as_ref(),
                        alpha_lock,
                    );
                    return;
                }
                // Clearing: the gradient's cover, laid on nothing, says
                // how much of each pixel goes.
                let mut cover = vec![0_u8; pixels.len()];
                fill::gradient(
                    &mut cover,
                    [width, height],
                    offset,
                    line,
                    [start, end],
                    radial,
                    selection.as_ref(),
                    false,
                );
                for (pixel, shade) in pixels
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(cover.as_chunks::<4>().0)
                {
                    let keep = 1.0 - f32::from(shade[3]) / 255.0;
                    pixel[3] = (f32::from(pixel[3]) * keep).round() as u8;
                }
            },
        ) {
            Self::apply(cx, edit);
        }
    }
}

