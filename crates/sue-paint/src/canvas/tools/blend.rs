//! The Blend tool on the canvas: blur, finger tip and copy stamp dabbed
//! along the drag, the whole drag one undo step. Editing a layer's mask,
//! they blend the mask (softening its edge, say), in its own pixels where
//! it sits.

use egui::{Pos2, Rect};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// Copy stamp: Alt and click at `p` (picture pixels) sets where it
    /// copies from.
    pub(super) fn copy_stamp_source(&mut self, p: Pos2, cx: &mut AppletContext<'_>) {
        self.stamp = Some(([p.x, p.y], None));
        cx.notify(tr!("copy-stamp-source-set"));
    }

    /// The Blend tool pressed: a blend drag begins (Copy stamp needs its
    /// source set first).
    pub(super) fn blend_press(&mut self, press: &Press, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let p = press.p;
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let arena = &cx.world.arena;
        // Editing the mask: the mask is what blends.
        let mask = self
            .docs
            .get(self.active)
            .filter(|doc| doc.mask_edit)
            .and_then(|_| doc::mask_of(arena, info.id))
            .zip(doc::mask_offset(arena, info.id));
        let (target, offset) = mask.unwrap_or((info.id, info.style.offset));
        let Some(raster) = doc::raster(arena, target) else {
            return;
        };
        let Some(mut before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        // A gray mask blends as gray RGBA, its value in every channel.
        let work: Option<Vec<u8>> = (raster.format() == sue_core::PixelFormat::Gray8)
            .then(|| before.iter().flat_map(|&v| [v, v, v, 255]).collect());
        if let Some(work) = &work {
            before.clone_from(work);
        }
        if sub.mode == crate::blend::COPY_STAMP {
            match &mut self.stamp {
                None => {
                    cx.fail(tr!("copy-stamp-needs-source"));
                    return;
                }
                // The first stroke fixes the distance; later ones
                // keep it, as Clip Studio's aligned stamp does.
                Some((source, gap @ None)) => {
                    *gap = Some([source[0] - p.x, source[1] - p.y]);
                }
                Some(_) => {}
            }
        }
        self.gesture = Some(Gesture::Blend {
            layer: target,
            before,
            last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
            since: 0.0,
            offset,
            label: crate::localize(&sub.name),
            work,
        });
    }

    /// A Blend tool drag to `pos`: dabs every fifth of the brush along the
    /// way, each one reading the pixels as they were before it.
    pub(super) fn blend_drag(
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
        let stamp = self.stamp.and_then(|(_, gap)| gap);
        let Some(Gesture::Blend {
            layer,
            before,
            last,
            since,
            offset,
            work,
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
            let gray = work.is_some();
            let pixels = match work.as_mut() {
                Some(work) => Some(work.as_mut_slice()),
                None => raster.bytes_mut(),
            };
            if let Some(pixels) = pixels {
                let mut along = step - *since;
                let unit = [dx / length, dy / length];
                while along <= length {
                    let at = [last[0] + unit[0] * along, last[1] + unit[1] * along];
                    let motion = [unit[0] * step, unit[1] * step];
                    let changed = if sub.mode == crate::blend::COPY_STAMP {
                        stamp.and_then(|gap| {
                            crate::blend::stamp(
                                pixels,
                                width,
                                height,
                                before,
                                at,
                                radius,
                                sub.tip.opacity,
                                gap,
                            )
                        })
                    } else {
                        crate::blend::dab(
                            pixels,
                            width,
                            height,
                            kind,
                            at,
                            radius,
                            sub.tip.opacity,
                            motion,
                        )
                    };
                    if let Some([x0, y0, x1, y1]) = changed {
                        dirty = Some(dirty.map_or([x0, y0, x1, y1], |[a, b, c, d]| {
                            [a.min(x0), b.min(y0), c.max(x1), d.max(y1)]
                        }));
                    }
                    along += step;
                }
                *since = length - (along - step);
            }
            // A gray mask takes back the values the dabs left.
            if gray
                && let (Some(work), Some([x0, y0, x1, y1]), Some(values)) =
                    (work.as_ref(), dirty, raster.bytes_mut())
            {
                for y in y0..y1.min(height) {
                    for x in x0..x1.min(width) {
                        let at = (y * width + x) as usize;
                        values[at] = work[at * 4];
                    }
                }
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

    /// The Blend tool lifted: the whole drag is one undo step.
    pub(super) fn blend_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Blend {
            layer,
            before,
            label,
            ..
        } = gesture
        else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let patch = if raster.format() == sue_core::PixelFormat::Gray8 {
            gray_patch(&before, raster)
        } else {
            raster
                .bytes()
                .and_then(|after| doc::diff_patch(&before, after, raster.width(), raster.height()))
        };
        if let Some(patch) = patch {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                layer,
                label,
                vec![patch],
            )));
        }
    }
}

/// The one patch that takes a gray raster from `before` (its values as
/// gray RGBA) to what it holds now: the rectangle around every value that
/// changed.
fn gray_patch(
    before: &[u8],
    raster: &sue_core::RasterTile2D,
) -> Option<sue_core::txn::RasterPatch> {
    let after = raster.bytes()?;
    let width = raster.width() as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (at, (a, b)) in before.as_chunks::<4>().0.iter().zip(after).enumerate() {
        if a[0] != *b {
            let (x, y) = (at % width, at / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 >= x1 {
        return None;
    }
    let rows = |bytes: &[u8], per: usize| -> Vec<u8> {
        (y0..y1)
            .flat_map(|y| {
                bytes[(y * width + x0) * per..(y * width + x1) * per]
                    .iter()
                    .step_by(per)
                    .copied()
            })
            .collect()
    };
    Some(sue_core::txn::RasterPatch {
        x: x0 as u32,
        y: y0 as u32,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        before: rows(before, 4),
        after: rows(after, 1),
    })
}
