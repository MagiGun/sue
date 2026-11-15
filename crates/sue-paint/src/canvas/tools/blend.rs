//! The Blend tool on the canvas: blur, finger tip and copy stamp dabbed
//! along the drag, the whole drag one undo step.

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
        let Some(before) = doc::raster(&cx.world.arena, info.id)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return;
        };
        let offset = info.style.offset;
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
            layer: info.id,
            before,
            last: [p.x - offset[0] as f32, p.y - offset[1] as f32],
            since: 0.0,
            offset,
            label: crate::localize(&sub.name),
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
        if let Some(after) = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let size = [raster.width(), raster.height()];
            raster.bytes().map(|bytes| (bytes.to_vec(), size))
        }) && let Some(patch) = doc::diff_patch(&before, &after.0, after.1[0], after.1[1])
        {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                layer,
                label,
                vec![patch],
            )));
        }
    }
}
