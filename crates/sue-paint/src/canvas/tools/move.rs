//! The Move layer tool on the canvas: a drag moves the current raster
//! layer, one undo step when it is let go.

use egui::{Pos2, Rect};
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};

impl SuePaint {
    /// The Move layer tool pressed.
    pub(super) fn move_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let p = press.p;
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        if info.kind != LayerKind::Raster || info.locked {
            cx.fail(tr!("cannot-be-moved", name = info.name));
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

    /// The layer dragged to `pos`: both where it was and where it goes
    /// are drawn again.
    pub(super) fn move_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let (
            Some(Gesture::Move {
                layer,
                start,
                origin,
                ..
            }),
            Some(doc),
        ) = (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let (dx, dy) = (
            (p.x - start.x).round() as i64,
            (p.y - start.y).round() as i64,
        );
        let (layer, next) = (*layer, [origin[0] + dx, origin[1] + dy]);
        // Where the layer was and where it goes, both drawn again:
        // the compositor only sees pixels change by itself.
        let Some((was, size)) = cx.world.arena.get_by_id(layer).ok().and_then(|entity| {
            let raster = doc::raster(&cx.world.arena, layer)?;
            let at = |key: &str| {
                entity
                    .property(key)
                    .and_then(sue_core::Value::as_i64)
                    .unwrap_or(0)
            };
            Some((
                [at(picture::X), at(picture::Y)],
                [raster.width(), raster.height()],
            ))
        }) else {
            return;
        };
        if was == next {
            return;
        }
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            entity.properties_mut().set(picture::X, next[0]);
            entity.properties_mut().set(picture::Y, next[1]);
        }
        let area = |at: [i64; 2]| {
            [
                at[0].max(0),
                at[1].max(0),
                (at[0] + i64::from(size[0])).max(0),
                (at[1] + i64::from(size[1])).max(0),
            ]
        };
        let (a, b) = (area(was), area(next));
        let rect = [
            a[0].min(b[0]) as u32,
            a[1].min(b[1]) as u32,
            a[2].max(b[2]).min(i64::from(u32::MAX)) as u32,
            a[3].max(b[3]).min(i64::from(u32::MAX)) as u32,
        ];
        if let Some(doc) = self.doc_mut() {
            doc.compositor.refresh(&cx.world.arena, rect);
        }
    }

    /// The layer let go: the move as one undo step.
    pub(super) fn move_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Move { layer, old, .. } = gesture else {
            return;
        };
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
            let mut steps = Batch::new(tr!("move-layer"));
            for (key, before) in [(picture::X, &old[0]), (picture::Y, &old[1])] {
                steps = steps.then(match before {
                    Some(value) => SetProperty::new(layer, key, value.clone()),
                    None => SetProperty::remove(layer, key),
                });
            }
            cx.record(ArenaEdit(steps));
        }
    }
}
