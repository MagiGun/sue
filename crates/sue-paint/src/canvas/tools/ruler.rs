//! The Ruler tool on the canvas: rulers laid by a drag (linear, special,
//! symmetrical, figure, perspective), the curve ruler clicked point by
//! point, the ruler pen drawn freehand, and ruler handles dragged.

use egui::{Pos2, Rect, pos2};
use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The Ruler tool pressed.
    pub(super) fn ruler_press(&mut self, press: &Press, sub: SubTool, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            op,
            viewport,
            size,
            ..
        } = *press;
        let tool = Tool::Ruler;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if sub.mode == 6 {
            // Curve Ruler: each click places a point; clicking the
            // last one again (or Enter) lays the ruler.
            let near_last = self.curve_points.last().is_some_and(|last| {
                doc.view
                    .to_screen(viewport, size, pos2(last[0], last[1]))
                    .distance(pos)
                    <= 8.0
            });
            if near_last {
                self.finish_curve(cx);
            } else {
                self.curve_points.push([p.x, p.y]);
            }
        } else if sub.mode == 8 {
            self.gesture = Some(Gesture::RulerPen {
                points: vec![[p.x, p.y]],
            });
        } else {
            let picture = doc.picture;
            let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
            let reach = 9.0 / doc.view.zoom.max(0.01);
            let hit = doc
                .show_rulers
                .then(|| rulers.handle_at([p.x, p.y], reach, size))
                .flatten();
            match hit {
                Some(crate::rulers::Handle::Vanishing(index)) if sub.mode == 1 && sub.kind == 1 => {
                    // Delete vanishing point.
                    let mut points = rulers
                        .perspective
                        .map(|ruler| ruler.points)
                        .unwrap_or_default();
                    if index < points.len() {
                        points.remove(index);
                    }
                    let text = (!points.is_empty())
                        .then(|| crate::rulers::Perspective { points }.to_text());
                    Self::apply(
                        cx,
                        crate::rulers::edit(
                            picture,
                            crate::rulers::PERSPECTIVE,
                            text,
                            &tr!("delete-vanishing-point"),
                        ),
                    );
                }
                Some(handle) => {
                    self.gesture = Some(Gesture::RulerHandle {
                        handle,
                        before: rulers.text(handle.key()),
                    });
                }
                None => {
                    self.gesture = Some(Gesture::Shape {
                        tool,
                        sub: Box::new(sub),
                        op,
                        from: p,
                        to: p,
                    });
                }
            }
        }
    }

    /// A Ruler tool drag from `from` to `to` (picture pixels): lays the
    /// sub tool's ruler, as Clip Studio Paint's Create ruler tools do.
    pub(super) fn ruler_drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        use crate::rulers::{self, Rulers, Special};
        use std::f32::consts::FRAC_PI_2;
        let Some(picture) = self.picture() else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = dx.hypot(dy);
        let dragged = length >= 4.0;
        // The sub tool's Step of angle, or Shift's 45 degrees, keeps
        // the line to its steps.
        let angle =
            crate::canvas::shape::snapped_angle(dy.atan2(dx), sub.angle_step, self.modifiers.shift)
                .unwrap_or_else(|| dy.atan2(dx));
        let end = [from.x + angle.cos() * length, from.y + angle.sin() * length];
        let (key, text, label) = match sub.mode {
            7 => {
                if !dragged {
                    return;
                }
                let path = rulers::Path::figure([from.x, from.y], [to.x, to.y], sub.kind == 1);
                let mut all = Rulers::of(&cx.world.arena, picture);
                all.paths.push(path);
                (
                    rulers::PATHS,
                    all.text(rulers::PATHS),
                    tr!("create-figure-ruler"),
                )
            }
            5 => {
                if !dragged {
                    return;
                }
                let ruler = Rulers {
                    linear: Some([[from.x, from.y], end]),
                    ..Rulers::default()
                };
                (
                    rulers::LINEAR,
                    ruler.text(rulers::LINEAR),
                    tr!("create-linear-ruler"),
                )
            }
            2 => {
                let special = match sub.kind {
                    0 => Special::Parallel(if dragged { angle } else { 0.0 }),
                    1 => Special::Radial([from.x, from.y]),
                    _ => Special::Concentric([from.x, from.y]),
                };
                (
                    rulers::SPECIAL,
                    Some(special.to_text()),
                    tr!("create-special-ruler"),
                )
            }
            1 => {
                // Add vanishing point: two lines along edges that meet in
                // the distance; where they cross is the point.
                if !dragged {
                    return;
                }
                let line = [[from.x, from.y], end];
                let Some(first) = self.vanishing_line.take() else {
                    self.vanishing_line = Some(line);
                    return;
                };
                let Some(point) = rulers::crossing(first, line) else {
                    cx.notify(tr!("those-lines-run-side-side-so"));
                    return;
                };
                let current = Rulers::of(&cx.world.arena, picture).perspective;
                let Some(ruler) = rulers::Perspective::with_point(current.as_ref(), point) else {
                    cx.fail(tr!("perspective-ruler-has-most-three-vanishing"));
                    return;
                };
                (
                    rulers::PERSPECTIVE,
                    Some(ruler.to_text()),
                    tr!("add-vanishing-point"),
                )
            }
            _ => {
                // Through where the drag began, along it (upright for a
                // click).
                let ruler = Rulers {
                    symmetry: Some(([from.x, from.y], if dragged { angle } else { FRAC_PI_2 })),
                    ..Rulers::default()
                };
                (
                    rulers::SYMMETRY,
                    ruler.text(rulers::SYMMETRY),
                    tr!("create-symmetrical-ruler"),
                )
            }
        };
        if Self::apply(cx, rulers::edit(picture, key, text, &label))
            && let Some(doc) = self.doc_mut()
        {
            doc.show_rulers = true;
            if matches!(sub.mode, 5..=8) {
                doc.snap_ruler = true;
            } else {
                doc.ruler_on = true;
            }
        }
    }

    /// A ruler handle dragged to `pos`.
    pub(super) fn ruler_handle_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::RulerHandle { handle, .. }) = self.gesture else {
            return;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let picture = doc.picture;
        let fix = self
            .toolbox
            .subs(Tool::Ruler)
            .iter()
            .find(|sub| sub.mode == 1)
            .is_none_or(|sub| sub.fix_eye_level);
        let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture).moved(
            handle,
            [p.x, p.y],
            fix,
            size,
        );
        if let Some(text) = rulers.text(handle.key())
            && let Ok(entity) = cx.world.arena.get_by_id_mut(picture)
        {
            // Shown as it moves; one undo step when it is let go.
            entity.properties_mut().set(handle.key(), text);
        }
    }

    /// A ruler handle let go.
    pub(super) fn ruler_handle_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::RulerHandle { handle, before } = gesture else {
            return;
        };
        let Some(picture) = self.picture() else {
            return;
        };
        let key = handle.key();
        let after = crate::rulers::Rulers::of(&cx.world.arena, picture).text(key);
        // Back as it was, then the move as one undo step.
        if let Ok(entity) = cx.world.arena.get_by_id_mut(picture) {
            match &before {
                Some(text) => {
                    entity.properties_mut().set(key, text.clone());
                }
                None => {
                    entity.properties_mut().remove(key);
                }
            }
        }
        if after != before {
            Self::apply(
                cx,
                crate::rulers::edit(picture, key, after, &tr!("move-ruler")),
            );
        }
    }

    /// The ruler pen carried on to `pos`.
    pub(super) fn ruler_pen_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let (Some(Gesture::RulerPen { points }), Some(doc)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        if points
            .last()
            .is_none_or(|last| (last[0] - p.x).hypot(last[1] - p.y) > 2.0 / doc.view.zoom.max(0.01))
        {
            points.push([p.x, p.y]);
        }
    }

    /// The ruler pen lifted: a ruler along what it drew.
    pub(super) fn ruler_pen_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::RulerPen { points } = gesture else {
            return;
        };
        if points.len() >= 2 {
            // Smoothed a little, as a pen line would be.
            let path = crate::rulers::Path::curve(&crate::correct::simplified_points(&points, 1.0));
            self.add_ruler_path(cx, path, &tr!("create-ruler"));
        }
    }
}
