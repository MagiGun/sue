//! The Move layer tool on the canvas: a drag moves the current raster
//! layer, every layer of the current folder, or (with a selection) only
//! the pixels selected, one undo step when it is let go. With "Move the
//! object at the clicked position" it moves the layer painted where the
//! click lands instead; with "Move layers in the selected area" every
//! layer painted inside the selection, whole. Its Object to move can
//! also be the current layer's tone dots or the picture's grid.

use egui::{Pos2, Rect};
use sue_core::EntityId;
use sue_core::txn::{Batch, SetProperty};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::fill::Exclude;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{MOVES_GRID, MOVES_TONE, SubTool};

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
        match sub.move_target {
            MOVES_TONE => return self.tone_press(press, sub, cx),
            MOVES_GRID => return self.grid_press(press, picture, cx),
            _ => {}
        }
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
        // Move layers in the selected area: every layer painted inside
        // the selection moves whole, the topmost becoming current.
        if sub.move_in_selection && clicked.is_none() {
            let bounds = self
                .doc()
                .and_then(|doc| doc.selection.as_ref())
                .and_then(crate::select::Selection::bounds);
            if let Some([x0, y0, x1, y1]) = bounds {
                let area = [
                    [i64::from(x0), i64::from(y0)],
                    [
                        i64::from(x1.saturating_sub(1)),
                        i64::from(y1.saturating_sub(1)),
                    ],
                ];
                let inside = self.layers_painted_in(&cx.world.arena, area, Exclude::default());
                if inside.is_empty() {
                    cx.fail(tr!("tp-nothing-in-selection"));
                    return;
                }
                if let Some(first) = inside.first()
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(*first);
                    doc.picked = inside[1..].to_vec();
                }
                self.start_layer_move(picture, &inside, p, cx);
                return;
            }
        }
        // A selection on the current layer: only what is selected moves,
        // and the selection with it.
        let selected = self.doc().is_some_and(|doc| doc.selection.is_some());
        if selected && Some(target) == self.layer() && !doc::is_folder(&cx.world.arena, target) {
            self.start_transform_in(cx, crate::transform::Mode::ScaleRotate, false);
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
        let wanted = if doc::is_folder(&cx.world.arena, target) {
            doc::members(&cx.world.arena, picture, target)
        } else {
            vec![target]
        };
        self.start_layer_move(picture, &wanted, p, cx);
    }

    /// A drag of the raster layers among `wanted` that can move begins
    /// at `p`; when none can, says why.
    fn start_layer_move(
        &mut self,
        picture: EntityId,
        wanted: &[EntityId],
        p: Pos2,
        cx: &mut AppletContext<'_>,
    ) {
        let arena = &cx.world.arena;
        let infos = doc::layers(arena, picture);
        let movable: Vec<doc::LayerInfo> = infos
            .into_iter()
            .filter(|info| wanted.contains(&info.id))
            .filter(|info| info.kind == LayerKind::Raster && !info.locked)
            .collect();
        if movable.is_empty() {
            let name = doc::layers(arena, picture)
                .into_iter()
                .find(|info| wanted.first() == Some(&info.id))
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

    /// Move tone pattern pressed: the tone dots of the current layer (or
    /// of the layer painted where the click lands) move with the drag,
    /// the layer's pixels staying where they are.
    fn tone_press(&mut self, press: &Press, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let toned = |layer: &EntityId| {
            arena.get_by_id(*layer).is_ok_and(|entity| {
                entity
                    .property(crate::effects::TONE)
                    .and_then(sue_core::Value::as_bool)
                    .unwrap_or(false)
            })
        };
        let clicked = sub
            .move_clicked
            .then(|| self.layer_painted_at(arena, press.p, Exclude::default()))
            .flatten()
            .filter(|layer| toned(layer));
        let target = clicked.or_else(|| self.layer().filter(|layer| toned(layer)));
        let Some(layer) = target else {
            cx.fail(tr!("tp-no-tone-to-move"));
            return;
        };
        if clicked.is_some()
            && sub.move_select
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(layer);
        }
        self.nudge_press(
            layer,
            [crate::effects::TONE_DOT_X, crate::effects::TONE_DOT_Y],
            press.p,
            tr!("tp-move-tone-step"),
            cx,
        );
    }

    /// Move grid pressed: the picture's grid moves with the drag (and
    /// shows, if it was hidden).
    fn grid_press(&mut self, press: &Press, picture: EntityId, cx: &AppletContext<'_>) {
        self.grid = true;
        self.nudge_press(
            picture,
            [crate::grid::SHIFT_X, crate::grid::SHIFT_Y],
            press.p,
            tr!("tp-move-grid-step"),
            cx,
        );
    }

    /// A drag of the number properties `keys` of `entity` begins at `p`.
    fn nudge_press(
        &mut self,
        entity: EntityId,
        keys: [&'static str; 2],
        p: Pos2,
        label: String,
        cx: &AppletContext<'_>,
    ) {
        let Ok(found) = cx.world.arena.get_by_id(entity) else {
            return;
        };
        let before = keys.map(|key| found.property(key).cloned());
        let base = keys.map(|key| {
            found
                .property(key)
                .and_then(sue_core::Value::as_f64)
                .unwrap_or(0.0)
        });
        self.gesture = Some(Gesture::Nudge {
            entity,
            keys,
            base,
            before,
            start: p,
            label,
        });
    }

    /// The tone dots or the grid dragged to `pos`: shown where they go.
    pub(super) fn nudge_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let (
            Some(Gesture::Nudge {
                entity,
                keys,
                base,
                start,
                ..
            }),
            Some(doc),
        ) = (self.gesture.as_ref(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let by = [f64::from(p.x - start.x), f64::from(p.y - start.y)];
        if let Ok(found) = cx.world.arena.get_by_id_mut(*entity) {
            for axis in 0..2 {
                found
                    .properties_mut()
                    .set(keys[axis], (base[axis] + by[axis]).round());
            }
        }
        doc.compositor
            .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
    }

    /// The tone dots or the grid taken back as if never moved (a second
    /// finger came down): nothing to undo.
    pub(crate) fn nudge_abandon(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Nudge {
            entity,
            keys,
            before,
            ..
        }) = self.gesture.take()
        else {
            return;
        };
        if let Ok(found) = cx.world.arena.get_by_id_mut(entity) {
            put_back(found, keys, &before);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The tone dots or the grid let go (or the studio closing while they
    /// are held): put back as they were, then moved as one undo step.
    pub(crate) fn nudge_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Nudge {
            entity,
            keys,
            before,
            label,
            ..
        } = gesture
        else {
            return;
        };
        let Ok(found) = cx.world.arena.get_by_id_mut(entity) else {
            return;
        };
        let now = keys.map(|key| found.property(key).cloned());
        if now == before {
            return;
        }
        put_back(found, keys, &before);
        let mut edit = crate::doc::Edit::new(label);
        for (key, value) in keys.into_iter().zip(now) {
            if let Some(value) = value {
                edit = edit.then(SetProperty::new(entity, key, value));
            }
        }
        Self::apply(cx, edit);
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The selected pixels Move layer dragged, let go: kept where they
    /// are as one undo step.
    pub(super) fn move_lift_release(&mut self, cx: &mut AppletContext<'_>) {
        // The undo step is named for the move while the flag is up.
        self.finish_transform(cx, true);
        self.move_lift = false;
    }

    /// The name of the undo step a transform ends in: Move Layer for
    /// Move layer's drag of the selected pixels, else the transform's.
    pub(crate) fn transform_label(&self, free: bool) -> String {
        if self.move_lift {
            tr!("move-layer")
        } else if free {
            tr!("free-transform")
        } else {
            tr!("scale-rotate")
        }
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

/// `keys` of `entity` set back to `before` (removed where it had none).
fn put_back(
    entity: &mut sue_core::Entity,
    keys: [&'static str; 2],
    before: &[Option<sue_core::Value>; 2],
) {
    for (key, value) in keys.into_iter().zip(before) {
        match value {
            Some(value) => {
                entity.properties_mut().set(key, value.clone());
            }
            None => {
                entity.properties_mut().remove(key);
            }
        }
    }
}
