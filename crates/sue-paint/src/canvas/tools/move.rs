//! The Move layer tool on the canvas: a drag moves the current raster
//! layer, every layer of the current folder, or (with a selection) only
//! the pixels selected, one undo step when it is let go. With "Move the
//! object at the clicked position" it moves the layer painted where the
//! click lands instead.

use egui::{Pos2, Rect};
use sue_core::EntityId;
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::fill::Exclude;
use crate::studio::{Gesture, SuePaint};
use crate::tools::SubTool;

impl SuePaint {
    /// The Move layer tool pressed.
    pub(super) fn move_press(&mut self, press: &Press, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            viewport,
            size,
            ..
        } = *press;
        let Some(picture) = self.picture() else {
            return;
        };
        let current = self.layer();
        // Move the object at the clicked position: the layer painted there
        // (Moved layers become selected: it becomes the current one).
        let clicked = sub
            .move_clicked
            .then(|| self.layer_painted_at(&cx.world.arena, p, Exclude::default()))
            .flatten();
        if let Some(layer) = clicked
            && sub.move_select
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(layer);
        }
        let Some(target) = clicked.or(current) else {
            return;
        };
        // A selection on the current layer: only what is selected moves,
        // and the selection with it.
        let selected = self.doc().is_some_and(|doc| doc.selection.is_some());
        if selected && Some(target) == self.layer() && !doc::is_folder(&cx.world.arena, target) {
            self.start_transform(cx, false);
            if self.transform.is_some() {
                let Some(view) = self.doc().map(|doc| doc.view) else {
                    return;
                };
                self.transform_press(pos, view, viewport, size);
                if let Some(Gesture::Transform { handle, .. }) = self.gesture.as_mut() {
                    *handle = crate::transform_ui::Handle::Move;
                }
                self.move_lift = true;
            }
            return;
        }
        let arena = &cx.world.arena;
        let wanted = if doc::is_folder(arena, target) {
            doc::members(arena, picture, target)
        } else {
            vec![target]
        };
        let infos = doc::layers(arena, picture);
        let movable: Vec<doc::LayerInfo> = infos
            .into_iter()
            .filter(|info| wanted.contains(&info.id))
            .filter(|info| info.kind == LayerKind::Raster && !info.locked)
            .collect();
        if movable.is_empty() {
            let name = doc::layers(arena, picture)
                .into_iter()
                .find(|info| info.id == target)
                .map(|info| info.name)
                .unwrap_or_default();
            cx.fail(tr!("cannot-be-moved", name = name));
            return;
        }
        let layers = movable
            .iter()
            .map(|info| (info.id, info.style.offset, position(arena, info.id)))
            .collect();
        self.gesture = Some(Gesture::Move { layers, start: p });
    }

    /// The layers dragged to `pos`: both where they were and where they
    /// go are drawn again.
    pub(super) fn move_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let (Some(Gesture::Move { layers, start }), Some(doc)) =
            (self.gesture.as_ref(), self.docs.get(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let (dx, dy) = (
            (p.x - start.x).round() as i64,
            (p.y - start.y).round() as i64,
        );
        let mut rect: Option<[u32; 4]> = None;
        for (layer, origin, _) in layers {
            let next = [origin[0] + dx, origin[1] + dy];
            let Some(area) = moved(&mut cx.world.arena, *layer, next) else {
                continue;
            };
            rect = Some(rect.map_or(area, |rect| {
                [
                    rect[0].min(area[0]),
                    rect[1].min(area[1]),
                    rect[2].max(area[2]),
                    rect[3].max(area[3]),
                ]
            }));
        }
        if let Some(rect) = rect
            && let Some(doc) = self.doc_mut()
        {
            doc.compositor.refresh(&cx.world.arena, rect);
        }
    }

    /// The layers let go: the move as one undo step.
    pub(super) fn move_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Move { layers, .. } = gesture else {
            return;
        };
        let mut steps = Batch::new(tr!("move-layer"));
        let mut changed = false;
        for (layer, _, old) in layers {
            if position(&cx.world.arena, layer) == old {
                continue;
            }
            changed = true;
            for (key, before) in [(picture::X, &old[0]), (picture::Y, &old[1])] {
                steps = steps.then(match before {
                    Some(value) => SetProperty::new(layer, key, value.clone()),
                    None => SetProperty::remove(layer, key),
                });
            }
        }
        if changed {
            cx.record(ArenaEdit(steps));
        }
    }

    /// The selected pixels Move layer dragged, let go: kept where they
    /// are as one undo step.
    pub(super) fn move_lift_release(&mut self, cx: &mut AppletContext<'_>) {
        self.move_lift = false;
        self.finish_transform(cx, true);
    }
}

/// `layer`'s position properties as they are now.
fn position(arena: &sue_core::Arena, layer: EntityId) -> [Option<sue_core::Value>; 2] {
    arena
        .get_by_id(layer)
        .map(|entity| {
            [
                entity.property(picture::X).cloned(),
                entity.property(picture::Y).cloned(),
            ]
        })
        .unwrap_or_default()
}

/// Puts `layer` at `next`; the picture area it covered and covers now,
/// or `None` when it did not move.
fn moved(arena: &mut sue_core::Arena, layer: EntityId, next: [i64; 2]) -> Option<[u32; 4]> {
    let (was, size) = arena.get_by_id(layer).ok().and_then(|entity| {
        let raster = doc::raster(arena, layer)?;
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
    })?;
    if was == next {
        return None;
    }
    if let Ok(entity) = arena.get_by_id_mut(layer) {
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
    Some([
        a[0].min(b[0]) as u32,
        a[1].min(b[1]) as u32,
        a[2].max(b[2]).min(i64::from(u32::MAX)) as u32,
        a[3].max(b[3]).min(i64::from(u32::MAX)) as u32,
    ])
}
