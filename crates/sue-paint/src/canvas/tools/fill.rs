//! The Fill tool on the canvas: a click fills the area under it, a drag
//! every area it crosses (on release, as one undo step); Close and fill
//! and Paint unpainted area are drawn round and filled on release.

use egui::{Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Fill tool pressed.
    pub(super) fn fill_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press { p, .. } = *press;
        if let Err(error) = self.area_target(&cx.world.arena) {
            cx.fail(error);
            return;
        }
        self.gesture = Some(
            if matches!(
                sub.mode,
                crate::tools::CLOSE_AND_FILL | crate::tools::PAINT_UNPAINTED
            ) {
                // Close and fill: drawn round, filled on release.
                Gesture::LassoFill {
                    close: true,
                    sub: Box::new(sub),
                    points: vec![[p.x, p.y]],
                }
            } else {
                Gesture::Bucket {
                    sub: Box::new(sub),
                    seeds: vec![[p.x, p.y]],
                }
            },
        );
    }

    /// The Fill tool dragged to `pos`: the drag's path, a seed every
    /// picture pixel or so, so no area it crosses is missed.
    pub(super) fn bucket_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::Bucket { seeds, .. }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let to = doc.view.to_picture(viewport, size, pos);
        let Some(&[x, y]) = seeds.last() else {
            seeds.push([to.x, to.y]);
            return;
        };
        let length = (to.x - x).hypot(to.y - y);
        let count = length.ceil() as usize;
        for step in 1..=count {
            let t = step as f32 / count as f32;
            seeds.push([x + (to.x - x) * t, y + (to.y - y) * t]);
        }
    }

    /// The Fill tool lifted: the areas the click or drag reached fill.
    pub(super) fn bucket_release(
        &mut self,
        gesture: Gesture,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Gesture::Bucket { sub, seeds } = gesture else {
            return;
        };
        if sub.tool == crate::tools::Tool::Gradient {
            self.contour_fill(&seeds, &sub, size, cx);
        } else {
            self.bucket(&seeds, &sub, size, cx);
        }
    }
}
