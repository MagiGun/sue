//! Fills and selections by area: the flood mask, the bucket, lasso
//! fill, auto select and gradients.

use egui::Pos2;
use sue_wm::AppletContext;

use crate::color;
use crate::composite::{self, RenderOptions};
use crate::doc;
use crate::fill;
use crate::select::{Op, Selection};
use crate::studio::SuePaint;
use crate::tools::SubTool;

impl SuePaint {
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
            self.layer_on_picture(arena, size)?
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
    pub(super) fn bucket(
        &mut self,
        p: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
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
        match doc::rewrite(
            &cx.world.arena,
            info.id,
            &tr!("fill"),
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
            Ok(Some(edit)) => {
                if Self::apply(cx, edit) {
                    self.recent.push(self.drawing_color());
                }
            }
            Ok(None) => {}
            Err(error) => cx.fail(error),
        }
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
        let Ok(info) = self.paintable(&cx.world.arena) else {
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
            let reference = composite::render(
                &cx.world.arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
            );
            if sub.mode == crate::tools::PAINT_UNPAINTED {
                let Some(layer) = self.layer_on_picture(&cx.world.arena, size) else {
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
        let rgb = color::unit(self.drawing_color());
        let (offset, alpha_lock, opacity) = (info.style.offset, info.alpha_locked, sub.tip.opacity);
        match doc::rewrite(
            &cx.world.arena,
            info.id,
            &crate::localize(&sub.name),
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
    pub(super) fn auto_select(
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

    /// The gradient tool.
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
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        // Main to Sub Color runs from the main color; the others from the
        // color brushes draw with, which X switches.
        let [r, g, b] = color::unit(if sub.mode == 1 {
            self.main
        } else {
            self.drawing_color()
        });
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
            &tr!("gradient"),
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
}
